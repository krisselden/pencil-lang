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
fn tangents_read(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/005-tangents")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn tangents_ir_matches_golden() {
    let scene = compile(&parse(&tangents_read("tangents.pen")).unwrap()).unwrap();
    let actual = serde_json::to_string_pretty(&scene.to_json()).unwrap() + "\n";
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/005-tangents/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

fn tangents() -> pencil_core::Figure {
    build(
        &tangents_read("tangents.pen"),
        &[&tangents_read("plain.style")],
    )
    .unwrap()
}

fn tangents_env(f: &pencil_core::Figure, d: f64) -> pencil_core::eval::Env {
    let inputs = pencil_core::frame::Inputs {
        time: 0.0,
        params: vec![("d".to_string(), d)],
    };
    pencil_core::eval::frame_env(f, &inputs).unwrap()
}

fn tangents_point(env: &pencil_core::eval::Env, name: &str) -> Option<(f64, f64)> {
    Some((
        env.get(&format!("{name}.x"))?,
        env.get(&format!("{name}.y"))?,
    ))
}

/// The geometry decides what exists: a tangent needs the point outside, a chord inside.
#[test]
fn tangents_exist_outside_the_circle_and_chords_inside() {
    let f = tangents();
    for i in 0..=300 {
        let d = 0.15 + 0.01 * i as f64;
        if (d - 1.0f64).abs() < 1e-6 {
            continue;
        }
        let env = tangents_env(&f, d);
        let (t, u) = (tangents_point(&env, "T"), tangents_point(&env, "U"));
        let (e, g) = (tangents_point(&env, "E"), tangents_point(&env, "F"));
        let p = tangents_point(&env, "P").unwrap();
        if d > 1.0 {
            assert!(e.is_none() && g.is_none(), "d={d}");
            for touch in [t.unwrap(), u.unwrap()] {
                // On the circle, with the radius square to the tangent.
                assert!((touch.0.hypot(touch.1) - 1.0).abs() < 1e-9, "d={d}");
                let square = touch.0 * (p.0 - touch.0) + touch.1 * (p.1 - touch.1);
                assert!(square.abs() < 1e-9, "d={d}: {square}");
                let length = (p.0 - touch.0).hypot(p.1 - touch.1);
                assert!((length - (d * d - 1.0).sqrt()).abs() < 1e-9, "d={d}");
            }
            assert_eq!(env.get("outside"), Some(1.0));
            assert!((env.get("tangent_length").unwrap() - (d * d - 1.0).sqrt()).abs() < 1e-9);
            assert_eq!(env.get("half_chord"), Some(0.0));
        } else {
            assert!(t.is_none() && u.is_none(), "d={d}");
            for end in [e.unwrap(), g.unwrap()] {
                assert!((end.0.hypot(end.1) - 1.0).abs() < 1e-9, "d={d}");
                let length = (p.0 - end.0).hypot(p.1 - end.1);
                assert!((length - (1.0 - d * d).sqrt()).abs() < 1e-9, "d={d}");
            }
            assert_eq!(env.get("outside"), Some(0.0));
            assert_eq!(env.get("tangent_length"), Some(0.0));
            assert!((env.get("half_chord").unwrap() - (1.0 - d * d).sqrt()).abs() < 1e-9);
        }
    }
}

/// On the circle, both constructions land on the point itself and neither flickers out.
#[test]
fn tangents_meet_the_chord_at_the_circle() {
    let f = tangents();
    let env = tangents_env(&f, 1.0);
    let p = tangents_point(&env, "P").unwrap();
    for name in ["T", "U", "E", "F"] {
        let q = tangents_point(&env, name).unwrap_or_else(|| panic!("{name} is absent"));
        assert!((p.0 - q.0).hypot(p.1 - q.1) < 1e-6, "{name}");
    }
}

/// PLAN's M3 test: dense sampling finds no teleports. Each root is one formula, so it
/// holds without a tracker. A point moves fast near the tangency, as √(d − 1), so that
/// stretch gets its own bound.
#[test]
fn tangent_points_do_not_teleport() {
    let f = tangents();
    for name in ["T", "U"] {
        let step = 0.001;
        let mut previous: Option<(f64, (f64, f64))> = None;
        for i in 0..2000 {
            let d = 1.001 + step * i as f64;
            let at = tangents_point(&tangents_env(&f, d), name).unwrap();
            if let Some((was_d, was)) = previous {
                let moved = (at.0 - was.0).hypot(at.1 - was.1);
                // |dT/dd| is 1 / (d·√(d² − 1)), taken at the step's start.
                let limit = 1.1 * step / (was_d * (was_d * was_d - 1.0).sqrt());
                assert!(moved < limit, "{name} at d={d}: {moved} over {limit}");
            }
            previous = Some((d, at));
        }
    }
}

/// Absent or not, a frame is a pure function of its inputs.
#[test]
fn tangents_seek_in_any_order() {
    use pencil_core::eval::Frame;
    use pencil_core::frame::Inputs;
    let f = tangents();
    let at = |t: f64| {
        Frame::new(&f, &Inputs::at(t), 48, 30)
            .unwrap()
            .rgba()
            .unwrap()
    };
    let forward: Vec<_> = [0.0, 4.0, 6.0, 8.0].map(at).into();
    let shuffled: Vec<_> = [6.0, 0.0, 8.0, 4.0].map(at).into();
    assert_eq!(forward[0], shuffled[1]);
    assert_eq!(forward[1], shuffled[3]);
    assert_eq!(forward[2], shuffled[0]);
    assert_eq!(forward[3], shuffled[2]);
    // Time 0 has the point outside, and time 6 inside, so they differ.
    assert_ne!(forward[0], forward[2]);
}

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
