//! The GPU against the CPU oracle, where they're easiest to tell apart.

use std::process::Command;

/// A pixel center almost exactly on a thin diagonal line. In single precision, the shader's
/// distance to the line there is only rounding error, so its direction is too, and a thin stroke's
/// coverage depends on that direction. Every line here runs through a pixel center, far enough from the next not to be hidden by it.
#[test]
fn a_thin_line_through_a_pixel_center_has_the_same_coverage() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("thin-lines");
    std::fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("thin.pen");
    let style = dir.join("thin.style");
    std::fs::write(
        &doc,
        "viewport { x: 0..320, y: 0..320, width: 320, height: 320 }
         curves c = parametric (t): (9·s + 4.5 + 0.9·t, 9·r + 4.5 + 0.4359·t) for t in -3.3..3.7 for s in 0..34, r in 0..34",
    )
    .unwrap();
    std::fs::write(
        &style,
        "style paper { fill: white }
         style c { stroke: black; width: 1px }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pencil"))
        .args([
            "check",
            doc.to_str().unwrap(),
            "--style",
            style.to_str().unwrap(),
        ])
        .args(["--t", "0", "--size", "320"])
        .output()
        .unwrap();
    let (out, err) = (
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    // A machine with no GPU can't say either way.
    if !out.contains("gpu:") {
        eprintln!("skipped, with no GPU: {err}");
        return;
    }
    assert!(output.status.success(), "{out}\n{err}");
}
