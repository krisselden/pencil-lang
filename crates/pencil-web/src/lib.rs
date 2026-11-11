//! The browser host: compiles `.pen` text in WASM and draws it with wgpu, on
//! WebGPU where it's available and WebGL2 otherwise.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::HashMap;

use pencil_core::frame::{self, Inputs};
use pencil_core::style::Font;
use pencil_core::terms::{Estimate, TextWidths};
use pencil_core::{Composition, compose, eval};
use pencil_render::{FigurePipeline, FrameInputs};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

#[wasm_bindgen]
pub struct Player {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    backend: String,
    /// What's loaded, with one pipeline per panel.
    figure: Option<(Composition, Vec<FigurePipeline>)>,
    /// Params a reader has taken over from their time bindings.
    params: Vec<(String, f64)>,
    /// The handle being dragged: which panel, and which of its handles.
    grabbed: Option<(usize, usize)>,
    /// How the page measures text: `(text, size in device pixels, is math)`. Expressions need
    /// real widths, and only the page knows its fonts.
    measure: Option<js_sys::Function>,
    measured: RefCell<HashMap<(String, u64, bool), f64>>,
    /// The page's text size where the figure is, in CSS pixels: an `em`.
    em: Option<f64>,
}

/// A page's own text widths, asking it once for each.
struct PageWidths<'a> {
    measure: &'a Option<js_sys::Function>,
    measured: &'a RefCell<HashMap<(String, u64, bool), f64>>,
}

impl TextWidths for PageWidths<'_> {
    fn width(&self, text: &str, size: f64, font: Font) -> f64 {
        let Some(measure) = self.measure else {
            return Estimate.width(text, size, font);
        };
        let math = font == Font::Math;
        let key = (text.to_string(), size.to_bits(), math);
        if let Some(width) = self.measured.borrow().get(&key) {
            return *width;
        }
        let width = measure
            .call3(
                &JsValue::NULL,
                &JsValue::from_str(text),
                &JsValue::from_f64(size),
                &JsValue::from_bool(math),
            )
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or_else(|| Estimate.width(text, size, font));
        self.measured.borrow_mut().insert(key, width);
        width
    }
}

fn js_error(message: impl std::fmt::Display) -> JsError {
    JsError::new(&message.to_string())
}

/// A compile error as a JS `Error` that also says where it is: `line` and `col`, 1-based or 0
/// for nowhere, and `sheet`, the index of the style sheet it's in, or `null` for the document.
fn compile_error(error: pencil_core::Error) -> JsValue {
    let js = js_sys::Error::new(&error.to_string());
    let set = |key: &str, value: JsValue| {
        let _ = js_sys::Reflect::set(&js, &JsValue::from_str(key), &value);
    };
    set("line", JsValue::from_f64(error.line as f64));
    set("col", JsValue::from_f64(error.col as f64));
    set("text", JsValue::from_str(&error.message));
    set("sheet", error.sheet.map_or(JsValue::NULL, |i| JsValue::from_f64(i as f64)));
    js.into()
}

#[wasm_bindgen]
impl Player {
    /// Sets up the GPU for a canvas. Takes the canvas's current size. `force_gl`
    /// skips WebGPU, for testing the WebGL2 path.
    pub async fn create(canvas: HtmlCanvasElement, force_gl: bool) -> Result<Player, JsError> {
        console_error_panic_hook::set_once();
        let (width, height) = (canvas.width().max(1), canvas.height().max(1));
        let backends = if force_gl {
            wgpu::Backends::GL
        } else {
            wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
        };
        let instance = wgpu::util::new_instance_with_webgpu_detection(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        })
        .await;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(js_error)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(js_error)?;
        let info = adapter.get_info();
        let backend = format!("{:?}", info.backend);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(js_error)?;

        let caps = surface.get_capabilities(&adapter);
        // The shader encodes sRGB itself, so the surface must not encode it again.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .ok_or_else(|| js_error(format!("no non-sRGB surface format in {:?}", caps.formats)))?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            color_space: Default::default(),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        Ok(Player {
            device,
            queue,
            surface,
            config,
            backend,
            figure: None,
            params: Vec::new(),
            grabbed: None,
            measure: None,
            measured: RefCell::new(HashMap::new()),
            em: None,
        })
    }

    /// The wgpu backend in use: `BrowserWebGpu` or `Gl`.
    #[wasm_bindgen(getter)]
    pub fn backend(&self) -> String {
        self.backend.clone()
    }

    /// Compiles new sources. On an error the previous figure stays on screen, so a
    /// half-typed edit doesn't blank the canvas.
    /// The error says where it is (see `compile_error`).
    pub fn load(&mut self, doc: &str, sheets: Vec<String>) -> Result<(), JsValue> {
        let refs: Vec<&str> = sheets.iter().map(String::as_str).collect();
        let composition = compose(doc, &refs).map_err(compile_error)?;
        let pipelines = composition
            .panels
            .iter()
            .map(|panel| FigurePipeline::new(&self.device, &panel.figure, self.config.format))
            .collect();
        self.figure = Some((composition, pipelines));
        self.params.clear();
        self.grabbed = None;
        Ok(())
    }

    /// Takes a param over from its time binding, at this value.
    pub fn set_param(&mut self, name: &str, value: f64) {
        self.params.retain(|(id, _)| id != name);
        self.params.push((name.to_string(), value));
    }

    /// Hands a param back to its time binding.
    pub fn release_param(&mut self, name: &str) {
        self.params.retain(|(id, _)| id != name);
    }

    /// Which panel a device pixel is in, and where in it.
    fn panel_at(&self, x: f64, y: f64) -> Option<(usize, f64, f64)> {
        let (composition, _) = self.figure.as_ref()?;
        (0..composition.panels.len()).find_map(|i| {
            let [px, py, w, h] = composition.rect_px(i, self.config.width, self.config.height);
            let (lx, ly) = (x - px as f64, y - py as f64);
            (lx >= 0.0 && ly >= 0.0 && lx < w as f64 && ly < h as f64).then_some((i, lx, ly))
        })
    }

    /// Starts a drag if a handle is within `reach` of this device pixel. Returns whether
    /// one was.
    pub fn grab(&mut self, time: f32, x: f64, y: f64, reach: f64) -> Result<bool, JsError> {
        let inputs = self.inputs(time);
        let (Some((panel, lx, ly)), Some((composition, _))) = (self.panel_at(x, y), &self.figure)
        else {
            self.grabbed = None;
            return Ok(false);
        };
        let [_, _, w, h] = composition.rect_px(panel, self.config.width, self.config.height);
        let handle = frame::handle_at(
            &composition.panels[panel].figure,
            &inputs,
            w,
            h,
            lx,
            ly,
            reach,
        )
        .map_err(js_error)?;
        self.grabbed = handle.map(|handle| (panel, handle));
        Ok(self.grabbed.is_some())
    }

    /// Moves the grabbed handle to this device pixel, through its inverse.
    pub fn drag(&mut self, time: f32, x: f64, y: f64) -> Result<(), JsError> {
        let (Some((panel, handle)), Some((composition, _))) = (self.grabbed, &self.figure) else {
            return Ok(());
        };
        let inputs = self.inputs(time);
        // A drag can leave its panel, and the inverse still answers.
        let [px, py, w, h] = composition.rect_px(panel, self.config.width, self.config.height);
        let (param, value) = frame::drag_value(
            &composition.panels[panel].figure,
            &inputs,
            handle,
            w,
            h,
            x - px as f64,
            y - py as f64,
        )
        .map_err(js_error)?;
        self.set_param(&param, value);
        Ok(())
    }

    pub fn release(&mut self) {
        self.grabbed = None;
    }

    /// The figure's params and duration, as JSON.
    pub fn info(&self) -> String {
        let Some((composition, _)) = &self.figure else {
            return "null".to_string();
        };
        let figure = &composition.panels[0].figure;
        serde_json::json!({
            "size": composition.size,
            "panels": composition.panels.iter().map(|p| serde_json::json!({
                "id": p.id, "title": p.title, "formula": p.formula, "rect": p.rect,
            })).collect::<Vec<_>>(),
            "duration": figure.scene.duration,
            "viewport": {
                "x": figure.scene.viewport.x,
                "y": figure.scene.viewport.y,
                "width": figure.scene.viewport.design_width,
                "height": figure.scene.viewport.design_height,
            },
            "beats": figure.scene.beats,
            "beatLengths": figure.scene.beat_lengths,
            "params": figure.scene.params.iter().map(|p| serde_json::json!({
                "id": p.id, "min": p.range[0], "max": p.range[1],
            })).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn inputs(&self, time: f32) -> Inputs {
        Inputs {
            time: time as f64,
            params: self.params.clone(),
        }
    }

    /// Gives the player the page's way to measure text, for laying out expressions:
    /// `(text, size in device pixels, is math) => width in device pixels`.
    pub fn set_measure(&mut self, measure: js_sys::Function) {
        self.measure = Some(measure);
        self.measured.borrow_mut().clear();
    }

    /// A binding's value this frame, or NaN if it doesn't exist this frame.
    pub fn value(&self, name: &str, time: f32) -> Result<f64, JsError> {
        let Some((composition, _)) = &self.figure else {
            return Err(js_error("no figure"));
        };
        let figure = &composition.panels[0].figure;
        let env = eval::frame_env(figure, &self.inputs(time)).map_err(js_error)?;
        // A binding that doesn't exist this frame reads as NaN.
        if env.is_absent(name) {
            return Ok(f64::NAN);
        }
        env.get(name)
            .ok_or_else(|| js_error(format!("no binding named `{name}`")))
    }

    /// The earliest time a param's own binding reaches a value, so play can carry on
    /// from where a reader left it. The binding is sampled, not inverted.
    pub fn time_for(&self, name: &str, value: f64) -> Result<f64, JsError> {
        let Some((composition, _)) = &self.figure else {
            return Err(js_error("no figure"));
        };
        let figure = &composition.panels[0].figure;
        let duration = figure.scene.duration.unwrap_or(1.0);
        let steps = 2000;
        let mut best = (f64::INFINITY, 0.0);
        for i in 0..steps {
            let t = duration * i as f64 / steps as f64;
            let env = eval::frame_env(figure, &Inputs::at(t)).map_err(js_error)?;
            let v = env.get(name).unwrap_or(0.0);
            if (v - value).abs() + 1e-9 < best.0 {
                best = ((v - value).abs(), t);
            }
        }
        Ok(best.1)
    }

    /// Computes the CPU side of a frame without drawing, to measure that cost. Returns
    /// the number of stroke segments.
    pub fn prepare_only(&self, time: f32, dpr: f32) -> Result<usize, JsError> {
        let Some((composition, _)) = &self.figure else {
            return Ok(0);
        };
        let frames = composition
            .prepare(
                &self.inputs(time),
                self.config.width,
                self.config.height,
                dpr as f64,
                self.em,
                &Estimate,
            )
            .map_err(js_error)?;
        Ok(frames
            .iter()
            .flat_map(|f| &f.prepared.strokes)
            .map(|s| s.len() / pencil_core::stroke::FLOATS_PER_SEGMENT)
            .sum())
    }

    /// The page's text size where the figure is, in CSS pixels, which an `em` is.
    pub fn set_em(&mut self, em: f64) {
        self.em = (em > 0.0).then_some(em);
    }

    /// Follows the canvas to a new size in device pixels, as the page lays it out again. What
    /// depends on the size is made again on the next draw.
    pub fn resize(&mut self, width: u32, height: u32) {
        let most = self.device.limits().max_texture_dimension_2d;
        let (width, height) = (width.clamp(1, most), height.clamp(1, most));
        if (width, height) != (self.config.width, self.config.height) {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Draws a frame, and returns its labels as JSON, for the page's text overlay.
    pub fn draw(&mut self, time: f32, dpr: f32) -> Result<String, JsError> {
        let inputs = self.inputs(time);
        let Some((composition, pipelines)) = &mut self.figure else {
            return Ok("[]".to_string());
        };
        let widths = PageWidths {
            measure: &self.measure,
            measured: &self.measured,
        };
        let frames = composition
            .prepare(
                &inputs,
                self.config.width,
                self.config.height,
                dpr as f64,
                self.em,
                &widths,
            )
            .map_err(js_error)?;
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => {
                self.surface.configure(&self.device, &self.config);
                return Err(js_error(format!("no surface texture: {other:?}")));
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for (i, (pipeline, panel)) in pipelines.iter_mut().zip(&frames).enumerate() {
            let uniforms = FrameInputs {
                time,
                width: panel.width,
                height: panel.height,
                dpr: panel.prepared.dpr as f32,
                em: panel.prepared.em as f32,
                origin: panel.origin,
            };
            pipeline.draw(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                uniforms,
                &panel.prepared,
                (i == 0).then_some(panel.prepared.paper),
            );
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        let paper = frames[0].prepared.paper;
        let labels = composition
            .labels(&frames, &inputs, &widths)
            .map_err(js_error)?;
        let labels: Vec<_> = labels
            .iter()
            .map(|l| {
                serde_json::json!({
                    "id": l.id, "text": l.text, "x": l.x, "y": l.y, "size": l.size,
                    "color": css(l.color), "halo": l.halo, "haloColor": css(paper),
                    "opacity": l.opacity,
                    "math": l.font == pencil_core::style::Font::Math,
                    "anchorX": l.anchor[0], "anchorY": l.anchor[1],
                    "settles": l.place == pencil_core::style::Place::Settle,
                    "leader": l.leader.map(|d| serde_json::json!({ "width": d.width, "gap": d.gap })),
                })
            })
            .collect();
        Ok(serde_json::Value::Array(labels).to_string())
    }
}

fn css(c: [f64; 3]) -> String {
    format!(
        "rgb({} {} {})",
        (eval::srgb_encode(c[0]) * 255.0).round(),
        (eval::srgb_encode(c[1]) * 255.0).round(),
        (eval::srgb_encode(c[2]) * 255.0).round(),
    )
}

/// The colors these sheets name, as a JSON object of CSS colors, so a page can match the figure
/// it holds without its own copy of the palette.
#[wasm_bindgen]
pub fn named_colors(sheets: Vec<String>) -> Result<String, JsError> {
    let refs: Vec<&str> = sheets.iter().map(String::as_str).collect();
    let colors: serde_json::Map<String, serde_json::Value> = pencil_core::named_colors(&refs)
        .map_err(js_error)?
        .into_iter()
        .map(|(name, color)| (name, css(color).into()))
        .collect();
    Ok(serde_json::Value::Object(colors).to_string())
}

/// A document's design size, as JSON `[width, height]`, to shape its canvas before a player
/// exists to draw on it.
#[wasm_bindgen]
pub fn figure_size(doc: &str, sheets: Vec<String>) -> Result<String, JsValue> {
    let refs: Vec<&str> = sheets.iter().map(String::as_str).collect();
    let composition = compose(doc, &refs).map_err(compile_error)?;
    Ok(serde_json::json!(composition.size).to_string())
}
