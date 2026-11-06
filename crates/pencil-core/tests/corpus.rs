//! The corpus is the contract: each case's IR is checked against a golden file.
//! After an intended change, run with `UPDATE_GOLDENS=1` and review the diff.

use std::path::Path;

use pencil_core::{build, compile::compile, parse::parse};

fn corpus() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/001-circle")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(corpus().join(name)).unwrap()
}

#[test]
fn circle_ir_matches_golden() {
    let scene = compile(&parse(&read("circle.pen")).unwrap()).unwrap();
    let actual = serde_json::to_string_pretty(&scene.to_json()).unwrap() + "\n";
    let golden = corpus().join("expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

fn unroll_read(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/003-unroll")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn unroll_ir_matches_golden() {
    let scene = compile(&parse(&unroll_read("unroll.pen")).unwrap()).unwrap();
    let actual = serde_json::to_string_pretty(&scene.to_json()).unwrap() + "\n";
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/003-unroll/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

/// The site's identity, from the figure: the triangle and wedge always total π.
#[test]
fn unrolls_area_is_always_pi() {
    use pencil_core::eval::frame_env;
    use pencil_core::frame::Inputs;
    let figure = build(&unroll_read("unroll.pen"), &[&unroll_read("blog.style")]).unwrap();
    let pi = std::f64::consts::PI;
    for time in [0.0, 1.5, 3.0, 4.0, 6.0, 7.5, 10.0, 12.0] {
        let env = frame_env(&figure, &Inputs::at(time)).unwrap();
        let total = env.get("triangle_area").unwrap() + env.get("wedge_area").unwrap();
        assert!((total - pi).abs() < 1e-9, "t={time}: {total}");
    }
    // Held at the full turn, the triangle alone is the circle.
    let env = frame_env(&figure, &Inputs::at(8.0)).unwrap();
    assert!((env.get("triangle_area").unwrap() - pi).abs() < 1e-9);
}

/// Hints choose how a stroke is drawn, never what it means, so every form of one shape
/// and every `draw` choice has to agree, apart from small differences on edge pixels.
#[test]
fn every_form_and_hint_draws_the_same_circle() {
    use pencil_core::eval::Frame;
    use pencil_core::frame::Inputs;
    let implicit = "curve c = implicit (x, y): x^2 + y^2 = 1.3225";
    let sdf = "curve c = sdf (x, y): sqrt(x^2 + y^2) - 1.15";
    let parametric = "curve c = parametric (t): (1.15 * cos(t), 1.15 * sin(t)) for t in 0..tau";
    let plain = "style c { stroke: black; width: 3px }";
    let line = "style c { stroke: black; width: 3px; draw: line }";
    let field = "style c { stroke: black; width: 3px; draw: field }";
    let cases = [
        ("implicit traced", implicit, plain),
        ("implicit by field", implicit, field),
        ("sdf by field", sdf, plain),
        ("sdf traced", sdf, line),
        ("parametric", parametric, plain),
    ];
    let render = |doc: &str, sheet: &str| {
        let figure = build(doc, &[sheet]).unwrap();
        Frame::new(&figure, &Inputs::at(0.0), 256, 256)
            .unwrap()
            .rgba()
            .unwrap()
    };
    let reference = render(sdf, plain);
    for (name, doc, sheet) in cases {
        let image = render(doc, sheet);
        let mut total = 0u64;
        let mut worst = 0u8;
        for (a, b) in image.iter().zip(&reference) {
            let d = a.abs_diff(*b);
            total += d as u64;
            worst = worst.max(d);
        }
        let mean = total as f64 / image.len() as f64;
        // Measured at 256px: tracing is the loosest, at a mean of 0.11 and at most 26/255
        // on edge pixels, because its fixed grid gives coarser chords than the others.
        assert!(mean < 0.15, "{name}: mean difference {mean}");
        assert!(worst < 32, "{name}: largest difference {worst}");
    }
}

/// Restyling changes the look, never the document's IR.
#[test]
fn unroll_restyles_without_touching_the_ir() {
    let doc = unroll_read("unroll.pen");
    let blog = build(&doc, &[&unroll_read("blog.style")]).unwrap();
    let night = build(&doc, &[&unroll_read("night.style")]).unwrap();
    assert_eq!(blog.scene, night.scene);
    assert_ne!(blog.styles, night.styles);
}

/// Both style files build, and neither changes the document's IR.
#[test]
fn restyling_leaves_the_ir_alone() {
    let doc = read("circle.pen");
    let plain = build(&doc, &[&read("circle.style")]).unwrap();
    let bands = build(&doc, &[&read("bands.style")]).unwrap();
    assert_eq!(plain.scene, bands.scene);
    assert_ne!(plain.styles, bands.styles);
}
