//! The browser host: compiles `.pen` text in WASM and draws it with wgpu, on
//! WebGPU where it's available and WebGL2 otherwise.
#![cfg(target_arch = "wasm32")]

use pencil_core::frame::{self, Inputs};
use pencil_core::{Figure, build, eval};
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
    figure: Option<(Figure, FigurePipeline)>,
    /// Params a reader has taken over from their time bindings.
    params: Vec<(String, f64)>,
}

fn js_error(message: impl std::fmt::Display) -> JsError {
    JsError::new(&message.to_string())
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
        })
    }

    /// The wgpu backend in use: `BrowserWebGpu` or `Gl`.
    #[wasm_bindgen(getter)]
    pub fn backend(&self) -> String {
        self.backend.clone()
    }

    /// Compiles new sources. On an error the previous figure stays on screen, so a
    /// half-typed edit doesn't blank the canvas.
    pub fn load(&mut self, doc: &str, sheets: Vec<String>) -> Result<(), JsError> {
        let refs: Vec<&str> = sheets.iter().map(String::as_str).collect();
        let figure = build(doc, &refs).map_err(js_error)?;
        let pipeline = FigurePipeline::new(&self.device, &figure, self.config.format);
        self.figure = Some((figure, pipeline));
        self.params.clear();
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

    /// The figure's params and duration, as JSON.
    pub fn info(&self) -> String {
        let Some((figure, _)) = &self.figure else {
            return "null".to_string();
        };
        serde_json::json!({
            "duration": figure.scene.duration,
            "viewport": {
                "x": figure.scene.viewport.x,
                "y": figure.scene.viewport.y,
                "width": figure.scene.viewport.design_width,
            },
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

    /// A binding's value this frame.
    pub fn value(&self, name: &str, time: f32) -> Result<f64, JsError> {
        let Some((figure, _)) = &self.figure else {
            return Err(js_error("no figure"));
        };
        let env = eval::frame_env(figure, &self.inputs(time)).map_err(js_error)?;
        env.get(name)
            .ok_or_else(|| js_error(format!("no binding named `{name}`")))
    }

    /// The earliest time a param's own binding reaches a value, so play can carry on
    /// from where a reader left it. The binding is sampled, not inverted.
    pub fn time_for(&self, name: &str, value: f64) -> Result<f64, JsError> {
        let Some((figure, _)) = &self.figure else {
            return Err(js_error("no figure"));
        };
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
        let Some((figure, _)) = &self.figure else {
            return Ok(0);
        };
        let prepared = frame::prepare(
            figure,
            &self.inputs(time),
            self.config.width,
            self.config.height,
            dpr as f64,
        )
        .map_err(js_error)?;
        Ok(prepared
            .strokes
            .iter()
            .map(|s| s.len() / pencil_core::stroke::FLOATS_PER_SEGMENT)
            .sum())
    }

    /// Draws a frame, and returns its labels as JSON, for the page's text overlay.
    pub fn draw(&mut self, time: f32, dpr: f32) -> Result<String, JsError> {
        let inputs = self.inputs(time);
        let Some((figure, pipeline)) = &mut self.figure else {
            return Ok("[]".to_string());
        };
        let prepared = frame::prepare(
            figure,
            &inputs,
            self.config.width,
            self.config.height,
            dpr as f64,
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
        let uniforms = FrameInputs {
            time,
            width: self.config.width,
            height: self.config.height,
            dpr,
        };
        pipeline.draw(
            &self.device,
            &self.queue,
            &mut encoder,
            &view,
            uniforms,
            &prepared,
        );
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        let css = |c: [f64; 3]| {
            format!(
                "rgb({} {} {})",
                (eval::srgb_encode(c[0]) * 255.0).round(),
                (eval::srgb_encode(c[1]) * 255.0).round(),
                (eval::srgb_encode(c[2]) * 255.0).round(),
            )
        };
        let labels: Vec<_> = prepared
            .labels
            .iter()
            .map(|l| {
                serde_json::json!({
                    "id": l.id, "text": l.text, "x": l.x, "y": l.y, "size": l.size,
                    "color": css(l.color), "halo": l.halo, "haloColor": css(prepared.paper),
                    "opacity": l.opacity,
                    "math": l.font == pencil_core::style::Font::Math,
                })
            })
            .collect();
        Ok(serde_json::Value::Array(labels).to_string())
    }
}
