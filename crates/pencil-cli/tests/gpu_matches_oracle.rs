//! The GPU against the CPU oracle, where they're easiest to tell apart.

use std::process::{Command, Output};

/// Runs `pencil check` on a document and a sheet at 320 wide, at time 0. None on a machine with no
/// GPU, which can't say either way.
fn check(name: &str, doc: &str, sheet: &str) -> Option<Output> {
    check_at(name, doc, sheet, "0")
}

/// `check` at this time.
fn check_at(name: &str, doc: &str, sheet: &str, time: &str) -> Option<Output> {
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
        .args(["--t", time, "--size", "320"])
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

/// Curves that run out to poles, whose pieces are 1e17 pixels long and more, which single
/// precision can't measure to a pixel: a sample lands on the pole at π, and at 0 over the wider
/// range. Plain, dashed and tapered, which measure along all of each piece.
#[test]
fn curves_through_poles_draw_alike() {
    for (span, range) in [("narrow", "0..2·π"), ("wide", "-2·π..2·π")] {
        for (name, look) in [
            ("plain", "width: 2px"),
            ("dashed", "width: 2px; dash: 7px; gap: 5px"),
            ("tapered", "width: 0.5px + 6px·self.t"),
        ] {
            passes(check(
                &format!("poles-{name}-{span}"),
                &format!(
                    "viewport {{ x: 0 ± 7, y: -1..7 }}
                     curve b = parametric (x): (x, 1/sin(x)^2) for x in {range}
                     curve f = parametric (x): (x, 1/x) for x in -7..7"
                ),
                &format!(
                    "style paper {{ fill: white }}
                     style b {{ stroke: black; {look} }}
                     style f {{ stroke: black; {look} }}"
                ),
            ));
        }
    }
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

/// The default ink turns from black to white as a paper darkens past the luminance where they
/// contrast equally, which is at t = 3.28348486… here. The shader's time is single precision, a
/// step of 2.4e-7 s there, so within about 1e-7 s of that moment the two can still be a frame
/// apart. Either side of that, they turn over together.
#[test]
fn the_default_ink_turns_over_with_the_paper_on_both() {
    let doc = "viewport { x: -2..2, y: -2..2 }
               curve c = circle((0, 0), (1, 0))
               let z = (0, 1)
               dot z";
    let sheet = "style paper { fill: mix(white, black, time/4) }";
    for (name, time) in [
        ("ink-light", "0"),
        ("ink-before", "3.2834845"),
        ("ink-after", "3.2834852"),
        ("ink-dark", "4"),
    ] {
        passes(check_at(name, doc, sheet, time));
    }
}

/// Each line of a collection in its own color: where lines of two colors cross, the one covering
/// a pixel more shows there, and the first drawn of two that cover it alike, on the GPU by a depth
/// test and on the CPU by comparing. Dots and circles' rims too, blended as SVG does and by adding
/// light, on a dark paper where it shows.
#[test]
fn lines_of_their_own_color_cross_alike() {
    for (blend, paper) in [("gamma", "white"), ("add", "rgb(20, 20, 30)")] {
        passes(check(
            &format!("inked-{blend}"),
            "viewport { x: -4..4, y: -4..4 }
             curves rays = parametric (t): t·exp(i·(k·π / 7)) for t in -4..4 for k in 0..6
             curves rings = parametric (t): (0.5 + 0.4·k)·exp(i·t) for t in 0..2·π for k in 0..6
             dots d = (0.6·k - 2, 0.3·k - 1) for k in 0..6
             circles c = circle((k - 3, 2.5), (k - 3.35, 2.5)) for k in 0..6",
            &format!(
                "style paper {{ fill: {paper} }}
                 style rays {{ stroke: hue(k / 7) / 3; width: 1px + 0.5px·k; blend: {blend} }}
                 style rings {{ stroke: hue(k / 7 + 0.5) / 3; width: 2.5px; blend: {blend} }}
                 style d {{ fill: hue(k / 7 + 0.25); radius: 4px; blend: {blend} }}
                 style c {{ stroke: mix(black, hue(k / 7), 0.7); width: 2px; blend: {blend} }}"
            ),
        ));
    }
}
