//! `pencil svg` on the corpus: a figure that has a vector form, and one whose fill has none.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn corpus(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(file)
}

/// Runs `pencil svg` on a corpus figure under its sheets, and reads back what it wrote.
fn svg(name: &str, doc: &str, sheets: &[&str], more: &[&str]) -> (Output, String) {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.svg"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_pencil"));
    command.arg("svg").arg(corpus(doc));
    for sheet in sheets {
        command.arg("--style").arg(corpus(sheet));
    }
    let output = command.args(more).arg("--out").arg(&out).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (output, std::fs::read_to_string(&out).unwrap())
}

/// The tangents from a point outside a circle: strokes, a dashed circle, dots, a ring and labels,
/// each under its mark's name, at the sheet's design size, with nothing left out.
#[test]
fn a_figure_of_strokes_dots_and_labels_exports_whole() {
    let (output, svg) = svg(
        "tangents",
        "005-tangents/tangents.pen",
        &["theme.style", "005-tangents/plain.style"],
        &["--param", "d=3"],
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert!(
        svg.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"640\" height=\"400\" viewBox=\"0 0 640 400\">\n\
             <rect width=\"640\" height=\"400\" fill=\"#f2f0eb\"/>\n"
        ),
        "{svg}"
    );
    assert!(svg.ends_with("</svg>\n"));
    // The circle about O through A: its center is at 1.6 of 5 units across, and a unit is 128.
    assert!(
        svg.contains("<path id=\"c\" d=\"M332.8 200L"),
        "the circle starts at A: {svg}"
    );
    assert!(svg.contains("<circle id=\"O\" cx=\"204.8\" cy=\"200\" r=\"3\" fill=\"#12110f\"/>"));
    // P is a ring: the paper's color inside, and its stroke on its edge.
    assert!(svg.contains("<g id=\"P\">\n<circle cx=\"565.52\" cy=\"68.33\" r=\"7\" fill=\"#f2f0eb\"/>\n<circle cx=\"565.52\" cy=\"68.33\" r=\"7\" fill=\"none\" stroke=\"#12110f\" stroke-width=\"3\"/>\n</g>"), "{svg}");
    // The dashed circle's 3 and 4 are stretched a little, so a whole number of them goes around.
    assert!(svg.contains("stroke-dasharray=\"3.01 4.01\""), "{svg}");
    for name in ["tangent_t", "tangent_u", "radius_t", "square_u", "T", "U"] {
        assert!(svg.contains(&format!(" id=\"{name}\"")), "no {name}: {svg}");
    }
    // With P outside, there's no chord, and so nothing of it here.
    assert!(!svg.contains("id=\"chord\""), "{svg}");
    assert!(svg.contains("<text id=\"name_p\""), "{svg}");
    assert!(svg.contains(">T</text>"), "{svg}");
}

/// Bands of distance are worked out per pixel, so they're left out and said so, and the circle's
/// plain stroke is still drawn.
#[test]
fn a_fill_that_reads_distance_is_left_out_and_named() {
    let (output, svg) = svg(
        "bands",
        "001-circle/circle.pen",
        &["001-circle/bands.style"],
        &["--t", "1.25"],
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "svg: `c`'s fill is left out: its `fill` reads `self.dist`, which is worked out per pixel\n"
    );
    assert!(svg.contains("<path id=\"c\" d=\"M"), "{svg}");
    assert!(
        svg.contains("fill=\"none\" stroke=\"#ffffff\" stroke-width=\"2.5\""),
        "{svg}"
    );
    assert_eq!(svg.matches("<path").count(), 1, "{svg}");
}
