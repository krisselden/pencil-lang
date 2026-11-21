//! `--param`: it names one of the figure's params, and a video holds it for every frame.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn corpus(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(file)
}

fn out(file: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("params");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(file)
}

fn pencil(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pencil"))
        .args(args)
        .output()
        .unwrap()
}

/// The squaring figure: `n` is a reader's, 3.3 until it's set, and `p` follows time.
fn squaring(command: &str, more: &[&str], to: &Path) -> Output {
    let (doc, theme) = (corpus("009-squaring/squaring.pen"), corpus("theme.style"));
    let sheet = corpus("009-squaring/plain.style");
    let mut args = vec![command, doc.to_str().unwrap()];
    args.extend(["--style", theme.to_str().unwrap()]);
    args.extend(["--style", sheet.to_str().unwrap()]);
    args.extend(more);
    args.extend(["--out", to.to_str().unwrap()]);
    pencil(&args)
}

fn error(output: &Output) -> String {
    assert!(!output.status.success(), "it ran");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_name_that_is_no_param_is_an_error_and_not_ignored() {
    let to = out("no-param.png");
    let said = error(&squaring("render", &["--cpu", "--param", "m=2"], &to));
    assert_eq!(
        said,
        "error: --param m: no param named `m` (the figure's params: `n`, `p`)\n"
    );
    assert!(!to.exists(), "nothing is drawn");
    // On every command that takes one.
    for command in ["svg", "build", "check", "video"] {
        let to = out("no-param").join(command);
        let said = error(&squaring(command, &["--param", "m=2"], &to));
        assert!(said.contains("no param named `m`"), "{command}: {said}");
        assert!(!to.exists(), "{command} wrote something");
    }
}

/// A `let` is the document's to say. Before, an input by its name stood in for it.
#[test]
fn a_let_is_not_set() {
    let (doc, to) = (out("let.pen"), out("let.png"));
    std::fs::write(
        &doc,
        "doc d\nviewport { x: -2..2, y: -2..2 }\nlet r = 1\ncurve c = implicit (x, y): x² + y² = r²\n",
    )
    .unwrap();
    let args = ["render", doc.to_str().unwrap(), "--cpu", "--param", "r=1.5"];
    let said = error(&pencil(
        &[&args[..], &["--out", to.to_str().unwrap()]].concat(),
    ));
    assert_eq!(
        said,
        "error: --param r: `r` isn't a param, and only a `param` can be set (the figure has no params)\n"
    );
}

/// `r1`, `r₁` and `r_1` are one name in a document, and so they are here.
#[test]
fn a_subscript_is_written_any_of_its_ways() {
    let doc = out("subscript.pen");
    std::fs::write(
        &doc,
        "doc d\nviewport { x: -2..2, y: -2..2 }\nparam r_1 = 1 in 0.5..1.5\ncurve c = implicit (x, y): x² + y² = r_1²\n",
    )
    .unwrap();
    let draw = |file: &str, param: &[&str]| {
        let to = out(file);
        let args = ["render", doc.to_str().unwrap(), "--cpu", "--size", "64"];
        let output = pencil(&[&args[..], param, &["--out", to.to_str().unwrap()]].concat());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read(&to).unwrap()
    };
    let (plain, written) = (
        draw("sub-plain.png", &[]),
        draw("sub-written.png", &["--param", "r_1=1.5"]),
    );
    assert_ne!(plain, written, "the param moves the circle");
    assert_eq!(draw("sub-digit.png", &["--param", "r1=1.5"]), written);
    assert_eq!(draw("sub-glyph.png", &["--param", "r₁=1.5"]), written);
}

/// It's refused before anything is made, so this needs no GPU and no ffmpeg.
#[test]
fn a_video_refuses_a_param_that_follows_time() {
    let to = out("refused/movie.mp4");
    let said = error(&squaring("video", &["--param", "p=1.5"], &to));
    assert_eq!(
        said,
        "error: --param p: `p` follows time in this figure, so a video can't hold it at one value. \
         `render` and `svg` take it for one frame, without --t\n"
    );
    assert!(!to.parent().unwrap().exists(), "nothing is written");
    // A still takes it: that's a reader's hand on it, for one frame.
    let still = out("still.png");
    let output = squaring(
        "render",
        &["--cpu", "--size", "64", "--param", "p=1.5"],
        &still,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `--t` asks for the frame at a time, and a param that follows time has its value there. So the
/// two together are refused, on each command that draws a still, and either alone is taken.
#[test]
fn a_still_at_a_time_refuses_a_param_that_follows_time() {
    for command in ["render", "svg", "build", "check"] {
        let to = out("at-a-time").join(command);
        let said = error(&squaring(command, &["--t", "1", "--param", "p=1.5"], &to));
        assert_eq!(
            said,
            "error: --param p: `p` follows time in this figure, and --t says the time, which gives \
             it its value. Leave one of the two out\n",
            "{command}"
        );
        assert!(!to.exists(), "{command} wrote something");
    }
    // A param time doesn't drive goes with a time.
    let to = out("at-a-time.svg");
    let output = squaring("svg", &["--t", "1", "--param", "n=5"], &to);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A document's poster can hold a param that follows time. `--t` asks for another moment, and
/// the param follows it there: left held, the time would change nothing the param draws.
#[test]
fn a_time_releases_a_param_the_poster_holds() {
    let figure = |poster: &str| {
        format!(
            "doc d\nduration 4\nviewport {{ x: -2..2, y: -2..2 }}\nparam r in 0.5..1.5\n\
             bind r = 0.5 + time / 4\n{poster}curve c = implicit (x, y): x² + y² = r²\n"
        )
    };
    let draw = |name: &str, poster: &str, more: &[&str]| {
        let (doc, to) = (out(&format!("{name}.pen")), out(&format!("{name}.svg")));
        std::fs::write(&doc, figure(poster)).unwrap();
        let args = ["svg", doc.to_str().unwrap(), "--size", "200"];
        let output = pencil(&[&args[..], more, &["--out", to.to_str().unwrap()]].concat());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read_to_string(&to).unwrap()
    };
    let held = "poster { r: 1.5 }\n";
    // With no time said, the poster's `r`, which is the circle at the end of the figure's time.
    assert_eq!(draw("poster", held, &[]), draw("end", "", &["--t", "4"]));
    // At a time, the circle of that time, as with no poster.
    let at = draw("poster-at", held, &["--t", "1"]);
    assert_eq!(at, draw("plain-at", "", &["--t", "1"]));
    assert_ne!(at, draw("poster", held, &[]));
}

/// A file's frame as RGB, by ffmpeg, which is what wrote the video.
fn rgb(file: &Path) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-i", file.to_str().unwrap()])
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let total: u64 = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u64).sum();
    total as f64 / a.len() as f64
}

/// A video's first frame, with `n` held at 5, is the still at time 0 with `n` at 5, and isn't the
/// still where `n` is its own 3.3. Skipped on a machine with no GPU or no ffmpeg.
#[test]
fn a_video_holds_a_param_time_does_not_drive() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("skipped, with no ffmpeg");
        return;
    }
    let size = ["--size", "256"];
    let movie = out("held.mp4");
    let held = [
        &size[..],
        &["--param", "n=5", "--fps", "2", "--seconds", "1"],
    ]
    .concat();
    let output = squaring("video", &held, &movie);
    if !String::from_utf8_lossy(&output.stdout).contains("gpu:") {
        eprintln!(
            "skipped, with no GPU: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let still = |file: &str, param: &[&str]| {
        let to = out(file);
        let output = squaring("render", &[&size[..], &["--t", "0"], param].concat(), &to);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        rgb(&to)
    };
    let frame = rgb(&movie);
    let same = mean_difference(&frame, &still("held-5.png", &["--param", "n=5"]));
    let other = mean_difference(&frame, &still("held-own.png", &[]));
    // What's left between a frame and its still is the encoder's.
    assert!(same < 2.5, "the frame is {same} from its still, of 255");
    assert!(
        other > 2.0 * same,
        "{other} from the still without it, {same} from the one with"
    );
}
