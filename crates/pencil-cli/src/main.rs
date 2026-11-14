//! The `pencil` command: compile, render frames and video, and referee GPU against CPU.

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
use pencil_core::{Composition, PanelFrame, Source, Sources, eval::Frame, place, wgsl};
use pencil_render::{FigurePipeline, Offscreen};

const USAGE: &str = "\
usage:
  pencil compile DOC.pen [--style FILE]... --out FIGURE.pfig
  pencil ir     DOC.pen
  pencil wgsl   DOC.pen [--style FILE]...
  pencil render DOC.pen [--style FILE]... [--t SECONDS] [--param NAME=VALUE]... [--size WIDTH] [--cpu] --out FRAME.png
  pencil check  DOC.pen [--style FILE]... [--t SECONDS]... [--param NAME=VALUE]... [--size WIDTH] [--out DIR]
  pencil video  DOC.pen [--style FILE]... [--size WIDTH] [--fps N] [--seconds S] [--font FILE] --out MOVIE.mp4
  pencil latex  FILE.tex...

ir, wgsl, render, check and video take a compiled figure (the file `compile` writes, which has its
sheets in it) in place of DOC.pen.
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
            "--size" => parsed.size = value()?.parse()?,
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
    let read = |p: &Path| fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let name = |p: &Path| p.to_string_lossy().into_owned();
    let bytes = fs::read(&args.doc).map_err(|e| format!("{}: {e}", args.doc.display()))?;
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
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("{}: not a Pencil document or a compiled figure", args.doc.display()))?;
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
    Sources::read(source(&doc), &sheet_sources, &|file| fs::read_to_string(file).ok())
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

fn render(args: &Args) -> Result<(), Box<dyn Error>> {
    let composition = load(args)?;
    let time = args.times.first().copied().unwrap_or(0.0);
    let out = args.out.as_ref().ok_or("render needs --out")?;
    let (width, height) = (args.size, height_for(&composition, args.size));
    let inputs = Inputs {
        time,
        params: args.params.clone(),
    };
    // A figure with no text shouldn't need the machine to have these fonts.
    let fonts = has_text(&composition)
        .then(|| text::Fonts::load(args.font.as_deref()))
        .transpose()?;
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
    let mut rgba = if args.cpu {
        if composition.panels.len() > 1 {
            return Err("--cpu draws one figure, and this has panels".into());
        }
        Frame::new(&composition.panels[0].figure, &inputs, width, height)?.rgba()?
    } else {
        let gpu = Offscreen::new()?;
        println!("gpu: {}", gpu.adapter_name);
        let mut pipelines = pipelines(&gpu, &composition)?;
        gpu.render_panels(
            &mut pipelines,
            &on_canvas(&frames),
            time as f32,
            width,
            height,
        )?
    };
    if let Some(fonts) = &fonts {
        text::draw_labels(
            &mut rgba,
            width,
            height,
            &composition.labels(&frames, &inputs, fonts)?,
            frames[0].prepared.paper,
            fonts,
        );
    }
    write_png(out, &rgba, width, height)?;
    println!("wrote {}", out.display());
    Ok(())
}

fn video(args: &Args) -> Result<(), Box<dyn Error>> {
    let composition = load(args)?;
    let out = args.out.as_ref().ok_or("video needs --out")?;
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
    // Only a shortcut: what's in it depends only on the time, so it can't change a frame.
    let mut springs: Vec<_> = composition
        .panels
        .iter()
        .map(|_| Default::default())
        .collect();
    for n in 0..frames {
        // Every frame is computed from its own time alone, so frames could be rendered
        // in any order. Nothing carries over from the frame before.
        let inputs = Inputs::at(n as f64 / args.fps as f64);
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
            params: args.params.clone(),
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
/// as Pencil, which checks that what's printed means what was read.
fn latex(paths: &[String]) -> Result<(), Box<dyn Error>> {
    if paths.is_empty() {
        return Err(format!("`latex` takes a file\n{USAGE}").into());
    }
    let (mut read, mut refused, mut unparsed) = (0, 0, 0);
    for path in paths {
        let source = std::fs::read_to_string(path)?;
        let defined = pencil_core::latex::definitions(&source);
        let functions: Vec<&str> = defined.iter().map(String::as_str).collect();
        for line in source.lines().filter(|l| !l.trim().is_empty()) {
            match pencil_core::latex::read(line, &functions) {
                Ok(text) => {
                    let parses = text.split(" = ").all(|side| {
                        pencil_core::parse::parse_whole_expression(side).is_ok()
                    });
                    if parses {
                        read += 1;
                        println!("{text}");
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
    eprintln!("{read} read, {refused} with no Pencil form, {unparsed} that don't read back");
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        print!("{USAGE}");
        return Ok(());
    };
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
        "render" => render(&parsed)?,
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
