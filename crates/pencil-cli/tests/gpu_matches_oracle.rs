//! The GPU against the CPU oracle, where they're easiest to tell apart.

use std::process::{Command, Output};

/// Runs `pencil check` on a document and a sheet at 320 wide, at time 0. None on a machine with no
/// GPU, which can't say either way.
fn check(name: &str, doc: &str, sheet: &str) -> Option<Output> {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let (doc_path, sheet_path) = (dir.join("doc.pen"), dir.join("sheet.style"));
    std::fs::write(&doc_path, doc).unwrap();
    std::fs::write(&sheet_path, sheet).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pencil"))
        .args([
            "check",
            doc_path.to_str().unwrap(),
            "--style",
            sheet_path.to_str().unwrap(),
        ])
        .args(["--t", "0", "--size", "320"])
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&output.stdout);
    if !out.contains("gpu:") {
        eprintln!(
            "skipped, with no GPU: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }
    Some(output)
}

fn passes(output: Option<Output>) {
    if let Some(output) = output {
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// A pixel center almost exactly on a thin diagonal line. In single precision, the shader's
/// distance to the line there is only rounding error, so its direction is too, and a thin stroke's
/// coverage depends on that direction. Every line here runs through a pixel center, far enough from the next not to be hidden by it.
#[test]
fn a_thin_line_through_a_pixel_center_has_the_same_coverage() {
    passes(check(
        "thin-lines",
        "viewport { x: 0..320, y: 0..320 } style figure { width: 320px; height: 320px }
         curves c = parametric (t): (9·s + 4.5 + 0.9·t, 9·r + 4.5 + 0.4359·t) for t in -3.3..3.7 for s in 0..34, r in 0..34",
        "style paper { fill: white }
         style c { stroke: black; width: 1px }",
    ));
}

/// A rule whose `self.focus` is always 1 leaves an opacity made of nothing but numbers, which the
/// shader has to be able to give a type.
#[test]
fn a_value_made_only_of_numbers_compiles() {
    passes(check(
        "only-numbers",
        "curve c = circle((0, 0), (1, 0))",
        "style paper { fill: white }
         style c { stroke: black; width: 2px; opacity: mix(0.45, 1, self.focus) }",
    ));
}

/// Lengths in all three units, in the shader as on the CPU: a field stroke a world unit wide plus
/// some pixels, a dot sized in ems, and a glow measured in `px`.
#[test]
fn lengths_in_every_unit_agree() {
    passes(check(
        "units",
        "viewport { x: -4..4, y: -3..3 } style figure { width: 320px; height: 240px; em: 12px }
         curve c = circle((0, 0), (2, 0))
         curve g = circle((1, 1), (1.5, 1))
         let P = (-2, -2)
         dot P",
        "style paper { fill: white }
         style c { stroke: black; width: 0.1 + 1px; draw: field }
         style g { blend: add; fill_opacity: tanh(4px / abs(self.dist)) }
         style g::inside { fill: rgb(255, 40, 40) }
         style g::outside { fill: rgb(40, 40, 255) }
         style P { fill: black; radius: 0.5em }",
    ));
}
