//! The `pencil` command: compile, render frames and video, and referee GPU against CPU.

use std::error::Error;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod text;

use pencil_core::frame::{self, Inputs};
use pencil_core::{Figure, build, eval::Frame, wgsl};
use pencil_render::Offscreen;

const USAGE: &str = "\
usage:
  pencil ir     DOC.pen
  pencil wgsl   DOC.pen [--style FILE]...
  pencil render DOC.pen [--style FILE]... [--t SECONDS] [--param NAME=VALUE]... [--size WIDTH] [--cpu] --out FRAME.png
  pencil check  DOC.pen [--style FILE]... [--t SECONDS]... [--param NAME=VALUE]... [--size WIDTH] [--out DIR]
  pencil video  DOC.pen [--style FILE]... [--size WIDTH] [--fps N] [--seconds S] [--font FILE] --out MOVIE.mp4
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

fn load(args: &Args) -> Result<Figure, Box<dyn Error>> {
    let read = |p: &Path| fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let doc = read(&args.doc)?;
    let sheets = args
        .styles
        .iter()
        .map(|p| read(p))
        .collect::<Result<Vec<_>, _>>()?;
    let refs: Vec<&str> = sheets.iter().map(String::as_str).collect();
    build(&doc, &refs).map_err(|e| format!("{}:{e}", args.doc.display()).into())
}

/// The frame's height for a width, following the viewport's shape. Even, because
/// video encoders need that.
fn height_for(figure: &Figure, width: u32) -> u32 {
    let view = &figure.scene.viewport;
    let aspect = match (view.design_width, view.design_height) {
        (Some(w), Some(h)) => h / w,
        _ => (view.y[1] - view.y[0]) / (view.x[1] - view.x[0]),
    };
    let height = (width as f64 * aspect).round() as u32;
    height + height % 2
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
    let figure = load(args)?;
    let time = args.times.first().copied().unwrap_or(0.0);
    let out = args.out.as_ref().ok_or("render needs --out")?;
    let (width, height) = (args.size, height_for(&figure, args.size));
    let inputs = Inputs {
        time,
        params: args.params.clone(),
    };
    let dpr = frame::design_scale(&figure, width).unwrap_or(1.0);
    let prepared = frame::prepare(&figure, &inputs, width, height, dpr)?;
    let mut rgba = if args.cpu {
        Frame::new(&figure, &inputs, width, height)?.rgba()?
    } else {
        let gpu = Offscreen::new()?;
        println!("gpu: {}", gpu.adapter_name);
        let mut pipeline = gpu.pipeline(&figure)?;
        gpu.render(&mut pipeline, &prepared, time as f32, width, height)?
    };
    if !prepared.labels.is_empty() {
        let fonts = text::Fonts::load(args.font.as_deref())?;
        text::draw_labels(
            &mut rgba,
            width,
            height,
            &prepared.labels,
            prepared.paper,
            &fonts,
        );
    }
    write_png(out, &rgba, width, height)?;
    println!("wrote {}", out.display());
    Ok(())
}

fn video(args: &Args) -> Result<(), Box<dyn Error>> {
    let figure = load(args)?;
    let out = args.out.as_ref().ok_or("video needs --out")?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let seconds = args
        .seconds
        .or(figure.scene.duration)
        .ok_or("video needs --seconds, or a `duration` in the document")?;
    let (width, height) = (args.size, height_for(&figure, args.size));
    let frames = (seconds * args.fps as f64).round() as u32;
    let dpr = frame::design_scale(&figure, width).unwrap_or(1.0);

    let gpu = Offscreen::new()?;
    println!(
        "gpu: {}, {frames} frames at {width}x{height}",
        gpu.adapter_name
    );
    let mut pipeline = gpu.pipeline(&figure)?;
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
    for n in 0..frames {
        // Every frame is computed from its own time alone, so frames could be rendered
        // in any order. Nothing carries over from the frame before.
        let inputs = Inputs::at(n as f64 / args.fps as f64);
        let prepared = frame::prepare(&figure, &inputs, width, height, dpr)?;
        let mut rgba = gpu.render(&mut pipeline, &prepared, inputs.time as f32, width, height)?;
        text::draw_labels(
            &mut rgba,
            width,
            height,
            &prepared.labels,
            prepared.paper,
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
fn check(args: &Args) -> Result<(), Box<dyn Error>> {
    let figure = load(args)?;
    let gpu = Offscreen::new()?;
    println!("gpu: {}", gpu.adapter_name);
    let mut pipeline = gpu.pipeline(&figure)?;
    let (width, height) = (args.size, height_for(&figure, args.size));
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
        let cpu = Frame::new(&figure, &inputs, width, height)?.rgba()?;
        let dpr = frame::design_scale(&figure, width).unwrap_or(1.0);
        let prepared = frame::prepare(&figure, &inputs, width, height, dpr)?;
        let image = gpu.render(&mut pipeline, &prepared, time as f32, width, height)?;
        let diff = compare(&cpu, &image, width);
        let ok = diff.max <= MAX_CHANNEL_DIFF && diff.mean <= MAX_MEAN_DIFF;
        println!(
            "t={time:<6} max {:>3}/255 at {:?}, mean {:.4}, {} px over {}: {}",
            diff.max,
            diff.worst,
            diff.mean,
            diff.over,
            MAX_CHANNEL_DIFF,
            if ok { "ok" } else { "FAIL" },
        );
        failed |= !ok;
        if let Some(dir) = &args.out {
            write_png(&dir.join(format!("t{time}.gpu.png")), &image, width, height)?;
            write_png(&dir.join(format!("t{time}.cpu.png")), &cpu, width, height)?;
        }
    }
    if failed {
        return Err("the GPU disagrees with the CPU oracle".into());
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        print!("{USAGE}");
        return Ok(());
    };
    let parsed = parse_args(rest)?;
    match command.as_str() {
        "ir" => println!(
            "{}",
            serde_json::to_string_pretty(&load(&parsed)?.scene.to_json())?
        ),
        "wgsl" => print!("{}", wgsl::emit(&load(&parsed)?)),
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
