//! The browser host: compiles `.pen` text in WASM and draws it with wgpu, on
//! WebGPU where it's available and WebGL2 otherwise.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use pencil_core::formula;
use pencil_core::frame::{self, Inputs};
use pencil_core::style::Font;
use pencil_core::terms::{Estimate, TextWidths};
use pencil_core::{Composition, Source, Sources, css, eval};
use pencil_render::{FigurePipeline, FrameInputs};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

// What each player tells its page when its GPU device is lost, by player. wgpu's device-lost
// callback has to be `Send`, so it can't hold a page's function, and finds it here by the player's
// number instead. A page has one thread.
// Each player's device too, so a page can let go of one whose player trapped: such a player
// can't be borrowed again, so its own `destroy` can't run.
thread_local! {
    static ON_LOST: RefCell<HashMap<u32, js_sys::Function>> = RefCell::new(HashMap::new());
    static DEVICES: RefCell<HashMap<u32, wgpu::Device>> = RefCell::new(HashMap::new());
}

/// Lets go of a player's GPU device by its number (`Player.id`), and forgets what it was to say
/// when that's lost. For a player that can't be used any more, after a trap: its device would
/// otherwise live on, and the page's callback with it.
#[wasm_bindgen]
pub fn let_go(id: u32) {
    ON_LOST.with(|all| all.borrow_mut().remove(&id));
    if let Some(device) = DEVICES.with(|all| all.borrow_mut().remove(&id)) {
        device.destroy();
    }
}
static PLAYERS: AtomicU32 = AtomicU32::new(0);

#[wasm_bindgen]
pub struct Player {
    /// This player's number, for `ON_LOST`.
    id: u32,
    /// Whether the browser has lost the GPU device, as it does after sleep or when its GPU
    /// process restarts. Nothing drawn on it shows again, and it can't be got back: a page makes
    /// a new player.
    lost: Arc<AtomicBool>,
    /// A GPU error nothing asked about, which wgpu would otherwise panic on, for `draw` to return.
    gpu_error: Arc<Mutex<Option<String>>>,
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

impl Drop for Player {
    fn drop(&mut self) {
        ON_LOST.with(|all| all.borrow_mut().remove(&self.id));
        DEVICES.with(|all| all.borrow_mut().remove(&self.id));
    }
}

/// How many texts' widths are kept at most before they're forgotten.
const MEASURED: usize = 4096;

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
        let mut measured = self.measured.borrow_mut();
        // A value that changes every frame is new text every frame, as a readout's is.
        if measured.len() > MEASURED {
            measured.clear();
        }
        measured.insert(key, width);
        width
    }
}

/// A name as the document reads it, so a page can say `x1` for `x_1` as a figure can.
fn named(name: &str) -> String {
    name.split('.')
        .map(pencil_core::parse::subscripted)
        .collect::<Vec<_>>()
        .join(".")
}

/// A document and its sheets, with the files they use, as a page hands them over. `names` are
/// where the document and then each sheet came from, which their files are found from: any it
/// leaves out have none. `files` is an object from a used file's name to its text, as
/// `uses` named it.
fn sources(
    doc: &str,
    sheets: &[String],
    names: &[String],
    files: &JsValue,
) -> Result<Sources, pencil_core::Error> {
    let name = |i: usize| names.get(i).map_or("", String::as_str);
    let sheets: Vec<Source> = sheets
        .iter()
        .enumerate()
        .map(|(i, text)| Source {
            name: name(i + 1),
            text,
        })
        .collect();
    let read = |file: &str| {
        js_sys::Reflect::get(files, &JsValue::from_str(file))
            .ok()?
            .as_string()
    };
    Sources::read(
        Source {
            name: name(0),
            text: doc,
        },
        &sheets,
        &read,
    )
}

fn js_error(message: impl std::fmt::Display) -> JsError {
    JsError::new(&message.to_string())
}

/// A compile error as a JS `Error` that also says where it is: `line` and `col`, 1-based or 0
/// for nowhere, `sheet`, the index of the style sheet it's in, or `null` for the document, and
/// `file`, the name of the file it's in when that's one the document or sheet uses, or `null`.
fn compile_error(error: pencil_core::Error) -> JsValue {
    let js = js_sys::Error::new(&error.to_string());
    let set = |key: &str, value: JsValue| {
        let _ = js_sys::Reflect::set(&js, &JsValue::from_str(key), &value);
    };
    set("line", JsValue::from_f64(error.line as f64));
    set("col", JsValue::from_f64(error.col as f64));
    set("text", JsValue::from_str(&error.message));
    set(
        "sheet",
        error
            .sheet
            .map_or(JsValue::NULL, |i| JsValue::from_f64(i as f64)),
    );
    set(
        "file",
        error
            .file
            .as_deref()
            .map_or(JsValue::NULL, JsValue::from_str),
    );
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

        let id = PLAYERS.fetch_add(1, Ordering::Relaxed);
        let lost = Arc::new(AtomicBool::new(false));
        let gpu_error = Arc::new(Mutex::new(None));
        {
            let lost = lost.clone();
            device.set_device_lost_callback(move |_reason, message| {
                lost.store(true, Ordering::Relaxed);
                // Taken out first, since the page may make another player as it hears.
                let tell = ON_LOST.with(|all| all.borrow().get(&id).cloned());
                if let Some(tell) = tell {
                    let _ = tell.call1(&JsValue::NULL, &JsValue::from_str(&message));
                }
            });
            let gpu_error = gpu_error.clone();
            // wgpu's own handler panics, which traps the whole call in WASM.
            device.on_uncaptured_error(Arc::new(move |error| {
                if let Ok(mut slot) = gpu_error.lock() {
                    slot.get_or_insert_with(|| error.to_string());
                }
            }));
        }

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
        DEVICES.with(|all| all.borrow_mut().insert(id, device.clone()));
        Ok(Player {
            id,
            lost,
            gpu_error,
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

    /// Destroys the GPU device, as a browser does when it loses it (after sleep, or when its GPU
    /// process restarts), for testing what a figure does then. On WebGL2 a page loses the
    /// context instead, through `WEBGL_lose_context`.
    pub fn lose_device(&self) {
        self.device.destroy();
    }

    /// Panics, which traps in WASM as a panic inside wgpu would, leaving this player unusable,
    /// for testing that a page gets past one.
    pub fn trap(&mut self) {
        panic!("a trap, for a test");
    }

    /// Calls `callback(message)` once the GPU device is lost, so a page can start the figure again
    /// on a new one at once, without waiting for a frame to fail.
    pub fn on_lost(&self, callback: js_sys::Function) {
        ON_LOST.with(|all| all.borrow_mut().insert(self.id, callback));
    }

    /// Whether the GPU device is lost.
    #[wasm_bindgen(getter)]
    pub fn lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    /// Lets go of the GPU now, rather than whenever the page's garbage collector gets to it:
    /// browsers cap how many contexts can be live at once. The player can't be used after.
    pub fn destroy(mut self) {
        self.figure = None;
        // Letting go isn't losing it, so the page isn't told.
        let_go(self.id);
    }

    /// This player's number, for `let_go`.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The wgpu backend in use: `BrowserWebGpu` or `Gl`.
    #[wasm_bindgen(getter)]
    pub fn backend(&self) -> String {
        self.backend.clone()
    }

    /// Compiles new sources. On an error the previous figure stays on screen, so a
    /// half-typed edit doesn't blank the canvas.
    /// The error says where it is (see `compile_error`).
    pub fn load(
        &mut self,
        doc: &str,
        sheets: Vec<String>,
        names: Vec<String>,
        files: &JsValue,
    ) -> Result<(), JsValue> {
        let composition = sources(doc, &sheets, &names, files)
            .and_then(|sources| sources.compose())
            .map_err(compile_error)?;
        self.show(composition);
        Ok(())
    }

    /// Takes a figure that `pencil compile` wrote, in place of sources: nothing is parsed. A
    /// file from another build of Pencil is an error that says to compile it again, and as with
    /// `load`, the previous figure stays on screen.
    pub fn load_compiled(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let composition = Composition::from_bytes(bytes).map_err(compile_error)?;
        self.show(composition);
        Ok(())
    }

    fn show(&mut self, composition: Composition) {
        let pipelines = composition
            .panels
            .iter()
            .map(|panel| FigurePipeline::new(&self.device, &panel.figure, self.config.format))
            .collect();
        self.figure = Some((composition, pipelines));
        self.params.clear();
        self.grabbed = None;
    }

    /// Takes a param over from its time binding, at this value.
    pub fn set_param(&mut self, name: &str, value: f64) {
        let name = &named(name);
        self.params.retain(|(id, _)| id != name);
        self.params.push((name.to_string(), value));
    }

    /// Hands a param back to its time binding.
    pub fn release_param(&mut self, name: &str) {
        let name = &named(name);
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

    /// Moves the grabbed handle to this device pixel, through its inverses. Returns the params
    /// it set, one or two, together, or none if no handle is grabbed.
    pub fn drag(&mut self, time: f32, x: f64, y: f64) -> Result<Vec<String>, JsError> {
        let (Some((panel, handle)), Some((composition, _))) = (self.grabbed, &self.figure) else {
            return Ok(Vec::new());
        };
        let inputs = self.inputs(time);
        // A drag can leave its panel, and the inverse still answers.
        let [px, py, w, h] = composition.rect_px(panel, self.config.width, self.config.height);
        let values = frame::drag_value(
            &composition.panels[panel].figure,
            &inputs,
            handle,
            w,
            h,
            x - px as f64,
            y - py as f64,
        )
        .map_err(js_error)?;
        Ok(self.set_params(values))
    }

    /// Sets the params a handle moved, and says which they were.
    fn set_params(&mut self, values: Vec<(String, f64)>) -> Vec<String> {
        values
            .into_iter()
            .map(|(param, value)| {
                self.set_param(&param, value);
                param
            })
            .collect()
    }

    pub fn release(&mut self) {
        self.grabbed = None;
    }

    /// Which handle is grabbed, counting through every panel's as `info`'s `handles` does, or −1.
    pub fn grabbed(&self) -> i32 {
        let (Some((panel, handle)), Some((composition, _))) = (self.grabbed, &self.figure) else {
            return -1;
        };
        let before: usize = composition.panels[..panel]
            .iter()
            .map(|p| p.figure.scene.handles.len())
            .sum();
        (before + handle) as i32
    }

    /// Which panel a handle is in, and which of that panel's it is, counting as `info` does.
    fn handle(&self, index: usize) -> Option<(usize, usize)> {
        let (composition, _) = self.figure.as_ref()?;
        let mut left = index;
        for (panel, p) in composition.panels.iter().enumerate() {
            let count = p.figure.scene.handles.len();
            if left < count {
                return Some((panel, left));
            }
            left -= count;
        }
        None
    }

    /// Where each handle is this frame, as `x, y` pairs in the canvas's device pixels, in the
    /// order of `info`'s `handles`. A handle whose mark doesn't exist this frame is NaN.
    pub fn handle_places(&self, time: f32) -> Result<Vec<f64>, JsError> {
        let Some((composition, _)) = &self.figure else {
            return Ok(Vec::new());
        };
        let inputs = self.inputs(time);
        let mut out = Vec::new();
        for (i, panel) in composition.panels.iter().enumerate() {
            let [px, py, w, h] = composition.rect_px(i, self.config.width, self.config.height);
            for place in frame::handle_places(&panel.figure, &inputs, w, h).map_err(js_error)? {
                match place {
                    Some((x, y)) => out.extend([px as f64 + x, py as f64 + y]),
                    None => out.extend([f64::NAN, f64::NAN]),
                }
            }
        }
        Ok(out)
    }

    /// Moves a handle by this many device pixels from where it is, through its inverses, as an
    /// arrow key does. Returns the params it set, none if the handle's mark doesn't exist this
    /// frame.
    pub fn nudge(
        &mut self,
        time: f32,
        index: usize,
        dx: f64,
        dy: f64,
    ) -> Result<Vec<String>, JsError> {
        let (Some((panel, handle)), Some((composition, _))) = (self.handle(index), &self.figure)
        else {
            return Ok(Vec::new());
        };
        let [_, _, w, h] = composition.rect_px(panel, self.config.width, self.config.height);
        let moved = frame::nudge_value(
            &composition.panels[panel].figure,
            &self.inputs(time),
            handle,
            w,
            h,
            dx,
            dy,
        )
        .map_err(js_error)?;
        Ok(moved
            .map(|values| self.set_params(values))
            .unwrap_or_default())
    }

    /// The figure's params and duration, as JSON. `timeDriven` names the bindings whose values
    /// change with time, so a page knows whether stepping one seeks time or sets it, and
    /// `usesTime` says whether anything does, so a figure that never moves needs no clock.
    pub fn info(&self) -> String {
        let Some((composition, _)) = &self.figure else {
            return "null".to_string();
        };
        composition.info().to_string()
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

    /// A param's range this frame, as `[min, max]`. An end that follows other params is read
    /// from them, so a page's control can stop where the figure does.
    pub fn range(&self, name: &str, time: f32) -> Result<Vec<f64>, JsError> {
        let name = &named(name);
        let Some((composition, _)) = &self.figure else {
            return Err(js_error("no figure"));
        };
        let figure = &composition.panels[0].figure;
        let Some(param) = figure.scene.params.iter().find(|p| p.id == *name) else {
            return Err(js_error(format!("no param named `{name}`")));
        };
        // A range no other param moves needs no frame.
        if param.limits.iter().all(Option::is_none) {
            return Ok(param.range.to_vec());
        }
        let env = eval::frame_env(figure, &self.inputs(time)).map_err(js_error)?;
        Ok(eval::param_range(param, &env).map_err(js_error)?.to_vec())
    }

    /// A binding's value this frame, or NaN if it doesn't exist this frame.
    pub fn value(&self, name: &str, time: f32) -> Result<f64, JsError> {
        let name = &named(name);
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
        let name = &named(name);
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
            if !self.lost() {
                self.surface.configure(&self.device, &self.config);
            }
        }
    }

    /// What the figure's readouts say at a time, as JSON: for each label with no place, in the
    /// order it's written, its text and what's laid out of it, in ems, for the page to set beside
    /// the figure (`readout_json` in the core).
    pub fn readouts(&self, time: f32) -> Result<String, JsError> {
        let Some((composition, _)) = &self.figure else {
            return Ok("[]".to_string());
        };
        let widths = PageWidths {
            measure: &self.measure,
            measured: &self.measured,
        };
        Ok(composition
            .readouts_json(&self.inputs(time), &widths)
            .map_err(js_error)?
            .to_string())
    }

    /// How much room each readout needs, as JSON `[[width, height], …]` in ems, so that nothing
    /// beside it moves as it changes: the most it takes at the moments a reader can reach that
    /// are tried. A build works out the same with the same fonts.
    pub fn readout_room(&self) -> Result<String, JsError> {
        let Some((composition, _)) = &self.figure else {
            return Ok("[]".to_string());
        };
        let widths = PageWidths {
            measure: &self.measure,
            measured: &self.measured,
        };
        let room = composition.readout_room(&widths).map_err(js_error)?;
        Ok(serde_json::json!(room).to_string())
    }

    /// Draws a frame, and returns its labels as JSON, for the page's text overlay.
    pub fn draw(&mut self, time: f32, dpr: f32) -> Result<String, JsError> {
        // Nothing made on a lost device is drawn, and a browser can throw on it, so it's not tried.
        // A page's error starting "GPU" is one a new player can get past.
        if self.lost() {
            return Err(js_error("GPU device lost"));
        }
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
                // As when the device is lost before the page hears so: a page starts it again.
                return Err(js_error(format!(
                    "GPU surface texture unavailable: {other:?}"
                )));
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
        if let Some(error) = self.gpu_error.lock().ok().and_then(|mut slot| slot.take()) {
            return Err(js_error(format!("GPU error: {error}")));
        }
        let paper = frames[0].prepared.paper;
        let labels = composition
            .labels(&frames, &inputs, &widths)
            .map_err(js_error)?;
        let labels: Vec<_> = labels
            .iter()
            .map(|l| {
                // What core laid out, for the page to set piece by piece: each run by the top of
                // its line box, which is where a page places text from, and each rule.
                let laid = l.laid.as_ref().map(|laid| {
                    let [half_width, half_height] = l.half(&widths);
                    let runs: Vec<_> = laid
                        .runs
                        .iter()
                        .map(|run| {
                            serde_json::json!({
                                "text": run.text, "x": run.x,
                                "top": run.y - (0.5 + formula::BASELINE) * run.size,
                                "size": run.size,
                                "math": run.face == formula::Face::Math,
                                "stretch": run.stretch,
                            })
                        })
                        .collect();
                    let rules: Vec<_> = laid
                        .rules
                        .iter()
                        .map(|rule| {
                            serde_json::json!({
                                "x": rule.x, "y": rule.y,
                                "width": rule.width, "height": rule.height,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "halfWidth": half_width, "halfHeight": half_height,
                        "runs": runs, "rules": rules,
                    })
                });
                serde_json::json!({
                    "id": l.id, "text": l.words(), "laid": laid,
                    "x": l.x, "y": l.y, "size": l.size,
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

/// The colors these sheets name, as a JSON object of CSS colors, so a page can match the figure
/// it holds without its own copy of the palette.
#[wasm_bindgen]
pub fn named_colors(
    sheets: Vec<String>,
    names: Vec<String>,
    files: &JsValue,
) -> Result<String, JsError> {
    // There's no document here, so the names are the sheets' alone.
    let names: Vec<String> = std::iter::once(String::new()).chain(names).collect();
    let colors: serde_json::Map<String, serde_json::Value> = sources("", &sheets, &names, files)
        .and_then(|sources| sources.named_colors())
        .map_err(js_error)?
        .into_iter()
        .map(|(name, color)| (name, css(color).into()))
        .collect();
    Ok(serde_json::Value::Object(colors).to_string())
}

/// A document's design size, as JSON `[width, height]`, to shape its canvas before a player
/// exists to draw on it.
#[wasm_bindgen]
pub fn figure_size(
    doc: &str,
    sheets: Vec<String>,
    names: Vec<String>,
    files: &JsValue,
) -> Result<String, JsValue> {
    let composition = sources(doc, &sheets, &names, files)
        .and_then(|sources| sources.compose())
        .map_err(compile_error)?;
    Ok(serde_json::json!(composition.size).to_string())
}

/// A compiled figure's design size, as `figure_size` gives a document's.
#[wasm_bindgen]
pub fn figure_size_compiled(bytes: &[u8]) -> Result<String, JsValue> {
    let composition = Composition::from_bytes(bytes).map_err(compile_error)?;
    Ok(serde_json::json!(composition.size).to_string())
}

/// The files a text uses itself, by name, each found from `name`, which is where the text came
/// from. A page fetches them, asks the same of each, and hands them all to `load`.
#[wasm_bindgen]
pub fn uses(name: &str, text: &str) -> Vec<String> {
    pencil_core::files::uses(name, text)
}

/// Which build of the core this is, as a compiled figure's `.json` writes it under `build`. Only
/// a figure with the same one can be loaded, so a build checks before a reader finds out.
#[wasm_bindgen]
pub fn fingerprint() -> String {
    format!("{:016x}", pencil_core::compiled::FINGERPRINT)
}

/// How text reads, stage by stage, as JSON: its tokens, then the expression it is or the
/// document's statements and IR. The AST page shows it.
#[wasm_bindgen]
pub fn inspect(src: &str) -> String {
    pencil_core::inspect::inspect(src).to_string()
}

/// The built-in functions as JSON pairs of how they're written and their MathJSON heads, so a page
/// can write a head back as Pencil text.
#[wasm_bindgen]
pub fn functions() -> String {
    let pairs: Vec<serde_json::Value> = pencil_core::parse::FUNCTIONS
        .iter()
        .map(|(written, head)| serde_json::json!([written, head]))
        .collect();
    serde_json::Value::Array(pairs).to_string()
}

/// LaTeX, as Desmos and MathLive copy it, as Pencil text, or the reason it has no Pencil form.
/// `functions` are the names the figure defines with `fn`, which a letter before parentheses
/// calls rather than multiplies. A page passes pasted text here when `is_latex` says it's LaTeX.
#[wasm_bindgen]
pub fn from_latex(latex: &str, functions: Vec<String>) -> Result<String, JsValue> {
    let names: Vec<&str> = functions.iter().map(String::as_str).collect();
    pencil_core::latex::read(latex, &names).map_err(compile_error)
}

/// What an editor writes as symbols are typed, as JSON: `names`, each written for its name after
/// a backslash, as `\to` is `→`, and `digraphs`, each written for what's typed in a label's text,
/// as `->` is. Pencil's text is the symbol, so this is only for a keyboard that lacks it.
#[wasm_bindgen]
pub fn symbols() -> String {
    serde_json::json!({
        "names": pencil_core::formula::symbols(),
        "digraphs": pencil_core::formula::digraphs(),
    })
    .to_string()
}

/// Whether pasted text is LaTeX rather than Pencil.
#[wasm_bindgen]
pub fn is_latex(text: &str) -> bool {
    pencil_core::latex::is_latex(text)
}
