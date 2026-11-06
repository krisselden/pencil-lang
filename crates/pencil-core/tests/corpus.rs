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

fn bounds_read(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/004-bounds")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn bounds_ir_matches_golden() {
    let scene = compile(&parse(&bounds_read("bounds.pen")).unwrap()).unwrap();
    let actual = serde_json::to_string_pretty(&scene.to_json()).unwrap() + "\n";
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/004-bounds/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

fn bounds_env(params: &[(&str, f64)]) -> pencil_core::eval::Env {
    use pencil_core::frame::Inputs;
    let figure = build(&bounds_read("bounds.pen"), &[&bounds_read("blog.style")]).unwrap();
    let inputs = Inputs {
        time: 0.0,
        params: params.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
    };
    pencil_core::eval::frame_env(&figure, &inputs).unwrap()
}

/// Whether two named circles touch from outside.
fn touching(env: &pencil_core::eval::Env, a: &str, b: &str) -> f64 {
    let v = |n: &str, f: &str| env.get(&format!("{n}.{f}")).unwrap();
    let centers = (v(a, "x") - v(b, "x")).hypot(v(a, "y") - v(b, "y"));
    (centers - v(a, "r") - v(b, "r")).abs()
}

/// The construction's geometry, not just its drawing: neighbors really touch, at any θ.
#[test]
fn bounds_neighbors_touch_the_circle_at_one_at_any_angle() {
    for theta in [0.3, 0.6, 1.0, 1.2, 1.5, 1.8] {
        let env = bounds_env(&[("theta", theta)]);
        let k = (theta / 2.0_f64).sin();
        assert!((env.get("d.r").unwrap() - k).abs() < 1e-9, "k at {theta}");
        for name in ["t", "c_1", "d_1", "p", "r", "q_1", "s"] {
            let gap = touching(&env, "d", name);
            assert!(gap < 1e-9, "{name} misses d by {gap} at θ = {theta}");
        }
    }
}

/// Every circle the same ratio k to its distance from 0, as the caption says.
#[test]
fn bounds_circles_keep_the_ratio_to_their_distance() {
    for theta in [0.4, 0.9, 1.4] {
        let env = bounds_env(&[("theta", theta)]);
        let k = (theta / 2.0_f64).sin();
        for name in [
            "d", "t", "c_1", "d_1", "t_m", "e_1", "g_1", "p", "r", "q_1", "s", "p_m", "r_m",
        ] {
            let v = |f: &str| env.get(&format!("{name}.{f}")).unwrap();
            let ratio = v("r") / v("x").hypot(v("y"));
            assert!(
                (ratio - k).abs() < 1e-9,
                "{name} at θ = {theta}: {ratio} vs {k}"
            );
        }
    }
}

/// The closing formulas in `closing`, from the blog's `closingTheta`.
#[test]
fn bounds_closes_where_it_should() {
    use std::f64::consts::{PI, TAU};
    for q in 4..=12 {
        let q = q as f64;
        let env = bounds_env(&[("q", q), ("ring", 1.0)]);
        assert!((env.get("theta").unwrap() - TAU / q).abs() < 1e-9);
        assert!((env.get("ring_count").unwrap() - q).abs() < 1e-9);
        // For q = 2p, the neighbors sit at 360°/q, so π/φ is p.
        let env = bounds_env(&[("q", q), ("ring", 0.0)]);
        assert!(
            (env.get("zoom_count").unwrap() - q / 2.0).abs() < 1e-9,
            "q = {q}"
        );
        let _ = PI;
    }
}

/// A drag can go anywhere in the range, so nothing there may become undefined.
#[test]
fn bounds_is_defined_across_its_whole_range() {
    let figure = build(&bounds_read("bounds.pen"), &[&bounds_read("blog.style")]).unwrap();
    for i in 0..=100 {
        let theta = 0.2 + (1.85 - 0.2) * i as f64 / 100.0;
        let env = bounds_env(&[("theta", theta)]);
        for binding in &figure.scene.bindings {
            let v = env.get(&binding.id).unwrap();
            assert!(v.is_finite(), "{} is {v} at θ = {theta}", binding.id);
        }
    }
}

#[test]
fn bounds_beats_step_with_time_for_video_and_hold_at_the_end() {
    use pencil_core::eval::frame_env;
    use pencil_core::frame::Inputs;
    let figure = build(&bounds_read("bounds.pen"), &[&bounds_read("blog.style")]).unwrap();
    assert_eq!(figure.scene.beats.len(), 10);
    let at = |t: f64| {
        frame_env(&figure, &Inputs::at(t))
            .unwrap()
            .get("beat")
            .unwrap()
    };
    assert_eq!((at(0.0), at(3.5), at(29.9), at(90.0)), (0.0, 1.0, 9.0, 9.0));
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
