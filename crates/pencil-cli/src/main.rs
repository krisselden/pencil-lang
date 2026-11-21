//! The `pencil` command: compile, render frames and video, and referee GPU against CPU.

use std::cell::RefCell;
use std::error::Error;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod text;

use pencil_core::compiled;
use pencil_core::frame::{self, Inputs};
use pencil_core::ir::Form;
use pencil_core::terms::{Estimate, TextWidths};
use pencil_core::{
    Composition, PanelFrame, Source, Sources, clock, eval::Frame, parse, place, wgsl,
};
use pencil_render::{FigurePipeline, Offscreen};

const USAGE: &str = "\
usage:
  pencil compile DOC.pen [--style FILE]... --out FIGURE.pfig
  pencil build  DOC.pen [--style FILE]... [--t SECONDS] [--param NAME=VALUE]... [--size WIDTH] [--card] --out DIR
  pencil ir     DOC.pen
  pencil wgsl   DOC.pen [--style FILE]...
  pencil render DOC.pen [--style FILE]... [--t SECONDS] [--param NAME=VALUE]... [--size WIDTH] [--cpu] --out FRAME.png
  pencil svg    DOC.pen [--style FILE]... [--t SECONDS] [--param NAME=VALUE]... [--size WIDTH] --out FIGURE.svg
  pencil check  DOC.pen [--style FILE]... [--t SECONDS]... [--param NAME=VALUE]... [--size WIDTH] [--out DIR]
  pencil video  DOC.pen [--style FILE]... [--param NAME=VALUE]... [--size WIDTH] [--fps N] [--seconds S] [--font FILE] --out MOVIE.mp4
  pencil latex  FILE.tex...
  pencil help

ir, wgsl, render, svg, check and video take a compiled figure (the file `compile` writes, which has
its sheets in it) in place of DOC.pen.

build writes what a page needs for one figure under one set of sheets: the compiled figure, a
poster at its design size and at twice it, and a .json that says what they are and what the figure
is. The poster is the moment the document's `poster` names, unless --t or --param say another.

--param NAME=VALUE sets one of the figure's params, and a name that isn't one is an error. A param
that follows time is set for a frame in place of a time, and not with one: with --t it's an error,
since the time gives it its value, and so it is in a video, which holds a param for every frame.
With --t, a param the document's `poster` holds follows the time too, if time drives it.

Labels are set in the fonts built into this command, the ones a page draws with. --font FILE, on
any command that draws, sets their words in another font. Their mathematics keeps the built-in one.

svg writes one frame as vector graphics, at the poster's moment and the figure's design size
unless told another: strokes, flat fills, dots and arrowheads as shapes, and labels as text. A
style that is worked out per pixel has no vector form: a fill or stroke that reads `self.dist`,
`blend: add`, and the paper's `tone`. What has none is left out, and a line on stderr names each
mark and the property.
";

/// The most any channel may differ from the CPU oracle, out of 255. The GPU is
/// f32 and the oracle f64, and anti-aliased edges are where they drift.
const MAX_CHANNEL_DIFF: u8 = 3;
const MAX_MEAN_DIFF: f64 = 0.25;

struct Args {
    doc: PathBuf,
    styles: Vec<PathBuf>,
    times: Vec<f64>,
    params: Vec<(String, f64)>,
    size: u32,
    fps: u32,
    seconds: Option<f64>,
    font: Option<PathBuf>,
    cpu: bool,
    /// Whether `--size` was given, for a command with a size of its own otherwise.
    sized: bool,
    card: bool,
    out: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Args, Box<dyn Error>> {
    let mut parsed = Args {
        doc: PathBuf::new(),
        styles: Vec::new(),
        times: Vec::new(),
        params: Vec::new(),
        size: 512,
        fps: 60,
        seconds: None,
        font: None,
        cpu: false,
        sized: false,
        card: false,
        out: None,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = || iter.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--style" => parsed.styles.push(value()?.into()),
            "--t" => parsed.times.push(value()?.parse()?),
            "--param" => {
                let text = value()?;
                let (name, number) = text
                    .split_once('=')
                    .ok_or_else(|| format!("--param needs NAME=VALUE, found `{text}`"))?;
                parsed.params.push((name.to_string(), number.parse()?));
            }
            "--size" => {
                parsed.size = value()?.parse()?;
                parsed.sized = true;
            }
            "--card" => parsed.card = true,
            "--fps" => parsed.fps = value()?.parse()?,
            "--seconds" => parsed.seconds = Some(value()?.parse()?),
            "--font" => parsed.font = Some(value()?.into()),
            "--out" => parsed.out = Some(value()?.into()),
            "--cpu" => parsed.cpu = true,
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}").into()),
            path => parsed.doc = path.into(),
        }
    }
    if parsed.doc.as_os_str().is_empty() {
        return Err("no document given".into());
    }
    Ok(parsed)
}

/// Compiles the document against its sheets, or reads a figure that was compiled already. A file
/// one of them uses is read from where its name says, which is its path from here, since the
/// document's and sheets' names are theirs.
fn load(args: &Args) -> Result<Composition, Box<dyn Error>> {
    load_listing(args, &RefCell::new(Vec::new()))
}

/// As `load`, and lists every file it read into `read_files`: the document, each sheet, and each
/// file they use. A build writes the list down, so a host knows when the figure is out of date.
fn load_listing(
    args: &Args,
    read_files: &RefCell<Vec<String>>,
) -> Result<Composition, Box<dyn Error>> {
    let read = |p: &Path| {
        read_files
            .borrow_mut()
            .push(p.to_string_lossy().into_owned());
        fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))
    };
    let name = |p: &Path| p.to_string_lossy().into_owned();
    let bytes = fs::read(&args.doc).map_err(|e| format!("{}: {e}", args.doc.display()))?;
    read_files.borrow_mut().push(name(&args.doc));
    if compiled::is_compiled(&bytes) {
        if !args.styles.is_empty() {
            return Err(format!(
                "{} is compiled, and has its sheets in it: compile it again to change them",
                args.doc.display()
            )
            .into());
        }
        return Composition::from_bytes(&bytes)
            .map_err(|e| format!("{}: {e}", args.doc.display()).into());
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        format!(
            "{}: not a Pencil document or a compiled figure",
            args.doc.display()
        )
    })?;
    let doc = (name(&args.doc), text);
    let sheets = args
        .styles
        .iter()
        .map(|p| Ok((name(p), read(p)?)))
        .collect::<Result<Vec<_>, String>>()?;
    fn source((name, text): &(String, String)) -> Source<'_> {
        Source { name, text }
    }
    let sheet_sources: Vec<Source> = sheets.iter().map(source).collect();
    let used = |file: &str| {
        let text = fs::read_to_string(file).ok();
        if text.is_some() {
            read_files.borrow_mut().push(file.to_string());
        }
        text
    };
    Sources::read(source(&doc), &sheet_sources, &used)
        .and_then(|sources| sources.compose())
        .map_err(|e| match e.file {
            // A used file's error says its own name.
            Some(_) => e.to_string().into(),
            None => {
                let file = e.sheet.map_or(&args.doc, |i| &args.styles[i]);
                format!("{}:{e}", file.display()).into()
            }
        })
}

/// What `--param` sets, each under its param's own name. A name can be written with its subscript
/// another way, as in a document: `r1` is `r_1`. One that isn't a param is an error. It would
/// change nothing, or stand in for a `let`, which is the document's to say.
fn held_params(
    composition: &Composition,
    args: &Args,
) -> Result<Vec<(String, f64)>, Box<dyn Error>> {
    let scene = &composition.panels[0].figure.scene;
    let mut held: Vec<(String, f64)> = Vec::new();
    for (name, value) in &args.params {
        let id = parse::subscripted(name);
        if !scene.params.iter().any(|p| p.id == id) {
            let params: Vec<String> = scene.params.iter().map(|p| format!("`{}`", p.id)).collect();
            let has = match params.is_empty() {
                true => "the figure has no params".to_string(),
                false => format!("the figure's params: {}", params.join(", ")),
            };
            return Err(match scene.bindings.iter().any(|b| b.id == id) {
                true => format!(
                    "--param {name}: `{name}` isn't a param, and only a `param` can be set ({has})"
                ),
                false => format!("--param {name}: no param named `{name}` ({has})"),
            }
            .into());
        }
        held.retain(|(other, _)| *other != id);
        held.push((id, *value));
    }
    Ok(held)
}

/// What `--param` sets where the time is said too, by `--t` or by a video's frames. A param that
/// follows time, itself, through what it reads or through the range it's kept in, has its value
/// from that time, so setting it as well asks for two frames at once. `clash` is the rest of the
/// sentence, after "`a` follows time in this figure, ".
fn params_beside_time(
    composition: &Composition,
    args: &Args,
    clash: &str,
) -> Result<Vec<(String, f64)>, Box<dyn Error>> {
    let params = held_params(composition, args)?;
    let driven = clock::time_driven(&composition.panels[0].figure.scene);
    match params.iter().find(|(name, _)| driven.contains(name)) {
        Some((name, _)) => {
            Err(format!("--param {name}: `{name}` follows time in this figure, {clash}").into())
        }
        None => Ok(params),
    }
}

/// What `--param` sets for a still. With no `--t`, a param that follows time can be set, as a
/// reader's hand sets it. With one, the frame asked for is the figure at that time.
fn still_params(
    composition: &Composition,
    args: &Args,
) -> Result<Vec<(String, f64)>, Box<dyn Error>> {
    match args.times.is_empty() {
        true => held_params(composition, args),
        false => params_beside_time(
            composition,
            args,
            "and --t says the time, which gives it its value. Leave one of the two out",
        ),
    }
}

/// The frame's height for a width, following the figure's shape. Even, because video
/// encoders need that.
fn height_for(composition: &Composition, width: u32) -> u32 {
    let height = (width as f64 * composition.size[1] / composition.size[0]).round() as u32;
    height + height % 2
}

/// Whether anything in it needs a font.
fn has_text(composition: &Composition) -> bool {
    composition.panels.iter().any(|panel| {
        panel
            .figure
            .scene
            .marks
            .iter()
            .any(|m| matches!(m.form, Form::Label { .. } | Form::Term { .. }))
            || panel.title.is_some()
            || panel.formula.is_some()
    })
}

/// A frame's panels, in the shape the GPU takes them.
fn on_canvas(frames: &[PanelFrame]) -> Vec<(&frame::Prepared, [u32; 2], u32, u32)> {
    frames
        .iter()
        .map(|f| (&f.prepared, f.origin, f.width, f.height))
        .collect()
}

/// Puts the second spring on settling labels, for what's not on a page.
fn spring_all(
    composition: &Composition,
    frames: &mut [PanelFrame],
    inputs: &Inputs,
    caches: &mut [place::SpringCache],
) -> Result<(), Box<dyn Error>> {
    for ((panel, frame), cache) in composition.panels.iter().zip(frames).zip(caches) {
        place::spring(
            &panel.figure,
            inputs,
            frame.width,
            frame.height,
            &mut frame.prepared,
            cache,
        )?;
    }
    Ok(())
}

fn pipelines(
    gpu: &Offscreen,
    composition: &Composition,
) -> Result<Vec<FigurePipeline>, Box<dyn Error>> {
    composition
        .panels
        .iter()
        .map(|panel| Ok(gpu.pipeline(&panel.figure)?))
        .collect()
}

fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = BufWriter::new(fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

/// How close a WebP poster is to its frame, of 100. At 90 a figure's lines and labels show no
/// ringing at the sizes a page draws them.
const WEBP_QUALITY: f32 = 90.0;

/// A frame as lossy WebP. Chroma is taken sharply, since a figure is thin colored lines on flat
/// paper, which the usual halving of chroma smears.
fn webp(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut config = webp::WebPConfig::new().map_err(|_| "libwebp wouldn't start")?;
    config.quality = WEBP_QUALITY;
    config.method = 6;
    config.use_sharp_yuv = 1;
    let encoded = webp::Encoder::from_rgba(rgba, width, height)
        .encode_advanced(&config)
        .map_err(|e| format!("libwebp couldn't encode a {width}×{height} frame: {e:?}"))?;
    Ok(encoded.to_vec())
}

/// What `still` draws: a frame's RGBA, its height in pixels, and its paper's color.
type Still = (Vec<u8>, u32, [f64; 3]);

/// One frame of a figure, as RGBA, with its labels drawn, and its paper's color. `dpr` is the
/// device pixels to a CSS pixel, as on a screen: a figure with no design width draws its `px` that
/// many pixels wide.
fn still(
    composition: &Composition,
    inputs: &Inputs,
    width: u32,
    dpr: f64,
    args: &Args,
    gpu: Option<&Offscreen>,
) -> Result<Still, Box<dyn Error>> {
    let height = height_for(composition, width);
    // A figure with no text shouldn't need the machine to have these fonts.
    let fonts = has_text(composition)
        .then(|| text::Fonts::load(args.font.as_deref()))
        .transpose()?;
    let widths: &dyn TextWidths = match &fonts {
        Some(fonts) => fonts,
        None => &Estimate,
    };
    let mut frames = composition.prepare(inputs, width, height, dpr, None, widths)?;
    let mut caches: Vec<_> = composition
        .panels
        .iter()
        .map(|_| Default::default())
        .collect();
    spring_all(composition, &mut frames, inputs, &mut caches)?;
    let mut rgba = match gpu {
        None => {
            if composition.panels.len() > 1 {
                return Err("--cpu draws one figure, and this has panels".into());
            }
            Frame::new(&composition.panels[0].figure, inputs, width, height)?.rgba()?
        }
        Some(gpu) => {
            let mut pipelines = pipelines(gpu, composition)?;
            gpu.render_panels(
                &mut pipelines,
                &on_canvas(&frames),
                inputs.time as f32,
                width,
                height,
            )?
        }
    };
    let paper = frames[0].prepared.paper;
    if let Some(fonts) = &fonts {
        text::draw_labels(
            &mut rgba,
            width,
            height,
            &composition.labels(&frames, inputs, fonts)?,
            paper,
            fonts,
        );
    }
    Ok((rgba, height, paper))
}

fn render(args: &Args) -> Result<(), Box<dyn Error>> {
    let composition = load(args)?;
    let time = args.times.first().copied().unwrap_or(0.0);
    let out = args.out.as_ref().ok_or("render needs --out")?;
    let inputs = Inputs {
        time,
        params: still_params(&composition, args)?,
    };
    let gpu = match args.cpu {
        true => None,
        false => {
            let gpu = Offscreen::new()?;
            println!("gpu: {}", gpu.adapter_name);
            Some(gpu)
        }
    };
    let (rgba, height, _) = still(&composition, &inputs, args.size, 1.0, args, gpu.as_ref())?;
    write_png(out, &rgba, args.size, height)?;
    println!("wrote {}", out.display());
    Ok(())
}

/// The moment a still shows: the document's poster, and a flag can say another.
fn still_moment(composition: &Composition, args: &Args) -> Result<Inputs, Box<dyn Error>> {
    let mut inputs = composition.poster();
    if let Some(time) = args.times.first() {
        inputs.time = *time;
        // The frame at that time is what's asked for, so a param the poster holds follows it
        // where time drives it. Left held, `--t` would change nothing the param draws.
        let driven = clock::time_driven(&composition.panels[0].figure.scene);
        inputs.params.retain(|(held, _)| !driven.contains(held));
    }
    for (name, value) in still_params(composition, args)? {
        inputs.params.retain(|(held, _)| *held != name);
        inputs.params.push((name, value));
    }
    Ok(inputs)
}

/// How wide a still is. A figure with a design width is drawn at it. One without is as wide as
/// it's told. Panels are laid out in design pixels, so their size is one. A lone figure's size is
/// only its shape unless its sheet gives a height too, and its width is the sheet's.
fn still_width(composition: &Composition, args: &Args) -> u32 {
    let design = match composition.panels.len() {
        1 => composition.panels[0].figure.scene.viewport.design_width,
        _ => Some(composition.size[0]),
    };
    match (design, args.sized) {
        (Some(design), false) => design.round() as u32,
        _ => args.size,
    }
}

/// Writes one frame as SVG, from what a frame prepares for the renderers, with labels as text.
/// What a style leaves with no vector form isn't drawn, and each such mark is named on stderr.
fn svg(args: &Args) -> Result<(), Box<dyn Error>> {
    let composition = load(args)?;
    let out = args.out.as_ref().ok_or("svg needs --out")?;
    let inputs = still_moment(&composition, args)?;
    let width = still_width(&composition, args);
    let height = height_for(&composition, width);
    // Text is written as text, so a font is only for where labels settle and where a leader
    // stops. Without the machine's, an estimate does, unless a font was asked for by name.
    let fonts = match (has_text(&composition), &args.font) {
        (false, _) => None,
        (true, Some(font)) => Some(text::Fonts::load(Some(font))?),
        (true, None) => text::Fonts::load(None).ok(),
    };
    let widths: &dyn TextWidths = match &fonts {
        Some(fonts) => fonts,
        None => &Estimate,
    };
    let mut frames = composition.prepare(&inputs, width, height, 1.0, None, widths)?;
    let mut caches: Vec<_> = composition
        .panels
        .iter()
        .map(|_| Default::default())
        .collect();
    spring_all(&composition, &mut frames, &inputs, &mut caches)?;
    let labels = composition.labels(&frames, &inputs, widths)?;
    let drawn = composition.svg(&frames, &inputs, width, height, &labels, widths)?;
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, &drawn.svg).map_err(|e| format!("{}: {e}", out.display()))?;
    for skipped in &drawn.skipped {
        eprintln!("svg: {skipped}");
    }
    println!(
        "wrote {} ({width}×{height}, {} bytes)",
        out.display(),
        drawn.svg.len()
    );
    Ok(())
}

/// A share card's size, as the sites that show one expect.
const CARD: (u32, u32) = (1200, 630);
/// How much of a card's width and height its figure may take.
const CARD_FILL: f64 = 0.92;

/// Writes what a page needs for one figure under one set of sheets, into a folder: the compiled
/// figure, a poster at its design size and at twice it, optionally a card for sharing, and a
/// `.json` that names them and holds what the page needs to know about the figure.
fn build(args: &Args) -> Result<(), Box<dyn Error>> {
    let dir = args.out.as_ref().ok_or("build needs --out, a folder")?;
    let read_files = RefCell::new(Vec::new());
    let composition = load_listing(args, &read_files)?;
    // Before anything is written, since a `--param` that names no param is an error.
    let inputs = still_moment(&composition, args)?;
    let mut sources = read_files.into_inner();
    sources.dedup();
    fs::create_dir_all(dir)?;
    let stem = args
        .doc
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or("the document has no name")?;
    let write = |name: &str, bytes: &[u8]| -> Result<(), Box<dyn Error>> {
        let path = dir.join(name);
        fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {} ({} bytes)", path.display(), bytes.len());
        Ok(())
    };

    let bytes = composition.to_bytes()?;
    let figure = format!("{stem}.pfig");
    write(&figure, &bytes)?;

    let width = still_width(&composition, args);
    let gpu = Offscreen::new()?;
    let mut posters = Vec::new();
    // Not `@2x`: a host can answer an `@` in a file's name with a redirect to `%40`, as Cloudflare
    // does, and that's a round trip before every poster on a dense screen.
    for (scale, name) in [(1, format!("{stem}.png")), (2, format!("{stem}-2x.png"))] {
        let wide = width * scale;
        let (rgba, height, _) = still(&composition, &inputs, wide, scale as f64, args, Some(&gpu))?;
        write_png(&dir.join(&name), &rgba, wide, height)?;
        println!("wrote {} ({wide}×{height})", dir.join(&name).display());
        let mut poster =
            serde_json::json!({ "src": name, "width": wide, "height": height, "scale": scale });
        // The same frame as WebP, for a page to offer first, when that's the smaller of the two.
        // A figure of a few flat colors can be smaller as PNG, and then there's only the PNG.
        let lossy = webp(&rgba, wide, height)?;
        let lighter = name.replace(".png", ".webp");
        if (lossy.len() as u64) < fs::metadata(dir.join(&name))?.len() {
            write(&lighter, &lossy)?;
            poster["webp"] = lighter.into();
        } else {
            // One an earlier build left would be a stale picture of the figure.
            let _ = fs::remove_file(dir.join(&lighter));
        }
        posters.push(poster);
    }

    // Every param as the figure stands at its poster: its value, and its range, which other
    // params can move. A page lays its controls out from these, as they'll be when the figure
    // starts there, before any of it runs.
    let first = &composition.panels[0].figure;
    let env = pencil_core::eval::frame_env(first, &inputs)?;
    let mut values = serde_json::Map::new();
    let mut ranges = serde_json::Map::new();
    for param in &first.scene.params {
        values.insert(param.id.clone(), env.get(&param.id).into());
        ranges.insert(
            param.id.clone(),
            serde_json::json!(pencil_core::eval::param_range(param, &env)?),
        );
    }

    let mut manifest = serde_json::json!({
        "figure": figure,
        // Which build of Pencil can read the figure, and what's in it, so a host can tell
        // when it has to be made again and when a reader's copy is stale.
        "build": format!("{:016x}", compiled::FINGERPRINT),
        "hash": format!("{:016x}", fnv(&bytes)),
        // Every file that went into it, as paths from where the build ran.
        "sources": sources,
        "poster": {
            "time": inputs.time,
            "params": inputs.params.iter()
                .map(|(name, value)| (name.clone(), serde_json::Value::from(*value)))
                .collect::<serde_json::Map<_, _>>(),
            "values": values,
            "ranges": ranges,
            "images": posters,
        },
        "info": composition.info(),
    });

    if args.card {
        // The figure as big as fits, in the middle of its own paper.
        let room = (CARD.0 as f64 * CARD_FILL, CARD.1 as f64 * CARD_FILL);
        let shape = composition.size[1] / composition.size[0];
        let wide = (room.0.min(room.1 / shape).floor() as u32).max(2) / 2 * 2;
        let scale = wide as f64 / width as f64;
        let (rgba, height, paper) = still(&composition, &inputs, wide, scale, args, Some(&gpu))?;
        let fill = paper.map(|c| (pencil_core::eval::srgb_encode(c) * 255.0).round() as u8);
        let mut card = [fill[0], fill[1], fill[2], 255].repeat((CARD.0 * CARD.1) as usize);
        let (left, top) = ((CARD.0 - wide) / 2, (CARD.1.saturating_sub(height)) / 2);
        for row in 0..height.min(CARD.1) {
            let from = (row * wide * 4) as usize;
            let to = (((top + row) * CARD.0 + left) * 4) as usize;
            card[to..to + (wide * 4) as usize]
                .copy_from_slice(&rgba[from..from + (wide * 4) as usize]);
        }
        let name = format!("{stem}.card.png");
        write_png(&dir.join(&name), &card, CARD.0, CARD.1)?;
        println!(
            "wrote {} ({}×{})",
            dir.join(&name).display(),
            CARD.0,
            CARD.1
        );
        manifest["card"] = serde_json::json!({ "src": name, "width": CARD.0, "height": CARD.1 });
    }

    write(
        &format!("{stem}.json"),
        serde_json::to_string_pretty(&manifest)?.as_bytes(),
    )
}

/// FNV-1a, to name what a file holds.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ *byte as u64).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn video(args: &Args) -> Result<(), Box<dyn Error>> {
    let composition = load(args)?;
    let out = args.out.as_ref().ok_or("video needs --out")?;
    // A video is the figure with time running, so a param is held for all of it. One that follows
    // time has a value in each frame already, and holding it would be another figure. That's
    // said, and not done or ignored.
    let params = params_beside_time(
        &composition,
        args,
        "so a video can't hold it at one value. `render` and `svg` take it for one frame, \
         without --t",
    )?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let seconds = args
        .seconds
        .or(composition.panels[0].figure.scene.duration)
        .ok_or("video needs --seconds, or a `duration` in the document")?;
    let (width, height) = (args.size, height_for(&composition, args.size));
    let frames = (seconds * args.fps as f64).round() as u32;

    let gpu = Offscreen::new()?;
    println!(
        "gpu: {}, {frames} frames at {width}x{height}",
        gpu.adapter_name
    );
    let mut pipelines = pipelines(&gpu, &composition)?;
    let fonts = text::Fonts::load(args.font.as_deref())?;

    let mut encoder = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
        ])
        .args(["-s", &format!("{width}x{height}")])
        .args(["-r", &args.fps.to_string(), "-i", "-"])
        .args(["-c:v", "libx264", "-preset", "slow", "-crf", "14"])
        .args(["-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .args([
            "-colorspace",
            "bt709",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
        ])
        .arg(out)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start ffmpeg: {e}"))?;
    let mut pipe = encoder.stdin.take().ok_or("no pipe to ffmpeg")?;

    let start = std::time::Instant::now();
    // Only a shortcut: what's in it depends only on the time and the params, which are held,
    // so it can't change a frame.
    let mut springs: Vec<_> = composition
        .panels
        .iter()
        .map(|_| Default::default())
        .collect();
    for n in 0..frames {
        // Every frame is computed from its own time alone, so frames could be rendered
        // in any order. Nothing carries over from the frame before.
        let inputs = Inputs {
            time: n as f64 / args.fps as f64,
            params: params.clone(),
        };
        let mut panels = composition.prepare(&inputs, width, height, 1.0, None, &fonts)?;
        spring_all(&composition, &mut panels, &inputs, &mut springs)?;
        let mut rgba = gpu.render_panels(
            &mut pipelines,
            &on_canvas(&panels),
            inputs.time as f32,
            width,
            height,
        )?;
        text::draw_labels(
            &mut rgba,
            width,
            height,
            &composition.labels(&panels, &inputs, &fonts)?,
            panels[0].prepared.paper,
            &fonts,
        );
        pipe.write_all(&rgba)?;
    }
    drop(pipe);
    let status = encoder.wait()?;
    if !status.success() {
        return Err(format!("ffmpeg failed: {status}").into());
    }
    println!(
        "wrote {} in {:.1}s ({:.1} ms a frame)",
        out.display(),
        start.elapsed().as_secs_f64(),
        start.elapsed().as_secs_f64() * 1000.0 / frames.max(1) as f64,
    );
    Ok(())
}

/// Compiles the document against its sheets and writes the result to one file, which a page or
/// `render` can read without parsing anything. It is only readable by this build of Pencil.
fn compile(args: &Args) -> Result<(), Box<dyn Error>> {
    let out = args.out.as_ref().ok_or("compile needs --out")?;
    let bytes = load(args)?.to_bytes()?;
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("wrote {} ({} bytes)", out.display(), bytes.len());
    Ok(())
}

struct Diff {
    max: u8,
    mean: f64,
    over: usize,
    worst: (u32, u32),
}

fn compare(a: &[u8], b: &[u8], width: u32) -> Diff {
    let mut diff = Diff {
        max: 0,
        mean: 0.0,
        over: 0,
        worst: (0, 0),
    };
    let mut total = 0u64;
    for (i, (pa, pb)) in a.chunks(4).zip(b.chunks(4)).enumerate() {
        let channel = pa
            .iter()
            .zip(pb)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        total += pa[..3]
            .iter()
            .zip(&pb[..3])
            .map(|(x, y)| x.abs_diff(*y) as u64)
            .sum::<u64>();
        if channel > MAX_CHANNEL_DIFF {
            diff.over += 1;
        }
        if channel > diff.max {
            diff.max = channel;
            diff.worst = (i as u32 % width, i as u32 / width);
        }
    }
    diff.mean = total as f64 / (a.len() / 4 * 3) as f64;
    diff
}

/// Compares the GPU with the CPU oracle. Labels are text, drawn by neither shader, so
/// they aren't part of this.
/// One panel's part of a canvas's RGBA8.
fn crop(image: &[u8], canvas_width: u32, frame: &PanelFrame) -> Vec<u8> {
    let row = canvas_width as usize * 4;
    let (x, y) = (frame.origin[0] as usize * 4, frame.origin[1] as usize);
    let wide = frame.width as usize * 4;
    let mut out = Vec::with_capacity(wide * frame.height as usize);
    for line in image.chunks(row).skip(y).take(frame.height as usize) {
        out.extend_from_slice(&line[x..x + wide]);
    }
    out
}

/// The GPU's canvas against the oracle, a panel at a time, so where panels sit is tested too.
fn check(args: &Args) -> Result<(), Box<dyn Error>> {
    let composition = load(args)?;
    let params = still_params(&composition, args)?;
    let gpu = Offscreen::new()?;
    println!("gpu: {}", gpu.adapter_name);
    let mut pipelines = pipelines(&gpu, &composition)?;
    let (width, height) = (args.size, height_for(&composition, args.size));
    let times = if args.times.is_empty() {
        vec![0.0, 0.5, 1.25, 2.5]
    } else {
        args.times.clone()
    };
    let mut failed = false;
    for time in times {
        let inputs = Inputs {
            time,
            params: params.clone(),
        };
        let frames = composition.prepare(&inputs, width, height, 1.0, None, &Estimate)?;
        let canvas = gpu.render_panels(
            &mut pipelines,
            &on_canvas(&frames),
            time as f32,
            width,
            height,
        )?;
        for (panel, frame) in composition.panels.iter().zip(&frames) {
            let cpu = Frame::new(&panel.figure, &inputs, frame.width, frame.height)?.rgba()?;
            let image = crop(&canvas, width, frame);
            let diff = compare(&cpu, &image, frame.width);
            let ok = diff.max <= MAX_CHANNEL_DIFF && diff.mean <= MAX_MEAN_DIFF;
            let name = if panel.id.is_empty() {
                String::new()
            } else {
                format!("{} ", panel.id)
            };
            println!(
                "{name}t={time:<6} max {:>3}/255 at {:?}, mean {:.4}, {} px over {}: {}",
                diff.max,
                diff.worst,
                diff.mean,
                diff.over,
                MAX_CHANNEL_DIFF,
                if ok { "ok" } else { "FAIL" },
            );
            failed |= !ok;
            if let Some(dir) = &args.out {
                let (w, h) = (frame.width, frame.height);
                write_png(&dir.join(format!("{name}t{time}.gpu.png")), &image, w, h)?;
                write_png(&dir.join(format!("{name}t{time}.cpu.png")), &cpu, w, h)?;
            }
        }
    }
    if failed {
        return Err("the GPU disagrees with the CPU oracle".into());
    }
    Ok(())
}

/// Reads files of LaTeX, a formula per line as Desmos and MathLive copy them, as Pencil text.
/// A file is one graph: a function any of its lines defines is a call on all of them. A line
/// with no Pencil form comes out as a comment that says why, so the output is still a list of
/// Pencil lines, and the count of each goes to stderr. Each line that is read is also parsed
/// as Pencil, which checks that what's printed means what was read, and so does a line that
/// calls a function LaTeX has and Pencil doesn't yet, which a figure couldn't compile.
fn latex(paths: &[String]) -> Result<(), Box<dyn Error>> {
    if paths.is_empty() {
        return Err(format!("`latex` takes a file\n{USAGE}").into());
    }
    let (mut read, mut refused, mut lacking, mut unparsed) = (0, 0, 0, 0);
    for path in paths {
        let source = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let defined = pencil_core::latex::definitions(&source);
        let functions: Vec<&str> = defined.iter().map(String::as_str).collect();
        for line in source.lines().filter(|l| !l.trim().is_empty()) {
            match pencil_core::latex::read(line, &functions) {
                Ok(text) => {
                    let parses = text
                        .split(" = ")
                        .all(|side| pencil_core::parse::parse_whole_expression(side).is_ok());
                    let lacks = pencil_core::latex::lacking(&text, &functions);
                    if parses && lacks.is_empty() {
                        read += 1;
                        println!("{text}");
                    } else if parses {
                        lacking += 1;
                        println!(
                            "// Pencil has no `{}` yet: {text}  from  {line}",
                            lacks.join("`, `")
                        );
                    } else {
                        unparsed += 1;
                        println!("// doesn't read back as Pencil: {text}  from  {line}");
                    }
                }
                Err(e) => {
                    refused += 1;
                    println!("// {}, column {}: {line}", e.message, e.col);
                }
            }
        }
    }
    eprintln!(
        "{read} read, {refused} with no Pencil form, {lacking} that call a function Pencil lacks, \
         {unparsed} that don't read back"
    );
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        print!("{USAGE}");
        return Ok(());
    };
    // Asked for anywhere, the usage is the answer, and not an error.
    if args
        .iter()
        .any(|a| matches!(a.as_str(), "help" | "--help" | "-h"))
    {
        print!("{USAGE}");
        return Ok(());
    }
    if matches!(command.as_str(), "--version" | "-V") {
        // The build is what a compiled figure is stamped with, and what the package's WASM must match.
        println!(
            "pencil {} (build {:016x})",
            env!("CARGO_PKG_VERSION"),
            compiled::FINGERPRINT
        );
        return Ok(());
    }
    if command == "latex" {
        return latex(rest);
    }
    let parsed = parse_args(rest)?;
    match command.as_str() {
        "ir" => println!(
            "{}",
            serde_json::to_string_pretty(&{
                let composition = load(&parsed)?;
                match composition.panels.as_slice() {
                    [only] => only.figure.scene.to_json(),
                    panels => serde_json::Value::Array(
                        panels.iter().map(|p| p.figure.scene.to_json()).collect(),
                    ),
                }
            })?
        ),
        "wgsl" => {
            for panel in load(&parsed)?.panels {
                print!("{}", wgsl::emit(&panel.figure));
            }
        }
        "compile" => compile(&parsed)?,
        "build" => build(&parsed)?,
        "render" => render(&parsed)?,
        "svg" => svg(&parsed)?,
        "check" => check(&parsed)?,
        "video" => video(&parsed)?,
        other => return Err(format!("unknown command `{other}`\n{USAGE}").into()),
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
