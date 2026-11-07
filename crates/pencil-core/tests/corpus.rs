//! The corpus is the contract: each case's IR is checked against a golden file.
//! After an intended change, run with `UPDATE_GOLDENS=1` and review the diff.

use std::path::Path;

use pencil_core::{build, compile::compile, parse::parse};

/// A golden file, with each expression on one line, so a diff shows which binding changed.
fn golden_text(value: &serde_json::Value) -> String {
    fn has_object(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Object(_) => true,
            serde_json::Value::Array(items) => items.iter().any(has_object),
            _ => false,
        }
    }
    fn write(v: &serde_json::Value, indent: usize, out: &mut String) {
        let pad = "  ".repeat(indent + 1);
        match v {
            serde_json::Value::Object(map) if !map.is_empty() => {
                out.push_str("{\n");
                for (i, (key, item)) in map.iter().enumerate() {
                    out.push_str(&format!("{pad}{}: ", serde_json::to_string(key).unwrap()));
                    write(item, indent + 1, out);
                    out.push_str(if i + 1 < map.len() { ",\n" } else { "\n" });
                }
                out.push_str(&"  ".repeat(indent));
                out.push('}');
            }
            serde_json::Value::Array(items) if has_object(v) => {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    out.push_str(&pad);
                    write(item, indent + 1, out);
                    out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
                }
                out.push_str(&"  ".repeat(indent));
                out.push(']');
            }
            other => out.push_str(&serde_json::to_string(other).unwrap()),
        }
    }
    let mut out = String::new();
    write(value, 0, &mut out);
    out.push('\n');
    out
}

fn corpus() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/001-circle")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(corpus().join(name)).unwrap()
}

#[test]
fn circle_ir_matches_golden() {
    let scene = compile(&parse(&read("circle.pen")).unwrap()).unwrap();
    let actual = golden_text(&scene.to_json());
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
    let actual = golden_text(&scene.to_json());
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
    let actual = golden_text(&scene.to_json());
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
    let actual = golden_text(&scene.to_json());
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/005-tangents/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

fn tangents() -> pencil_core::Figure {
    let theme = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/theme.style"),
    )
    .unwrap();
    build(
        &tangents_read("tangents.pen"),
        &[&theme, &tangents_read("plain.style")],
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

/// How a set of labels moves: the biggest step, and how often a visible dodge turns back on
/// itself, which is what wiggling and indecision look like.
#[derive(Default)]
struct Motion {
    last: std::collections::HashMap<String, [f64; 2]>,
    heading: std::collections::HashMap<String, [f64; 2]>,
    biggest: f64,
    reversals: usize,
    travel: f64,
}

impl Motion {
    fn see(&mut self, labels: &[pencil_core::frame::Label]) {
        for l in labels {
            let offset = [l.x - l.anchor[0], l.y - l.anchor[1]];
            if let Some(was) = self.last.get(&l.id) {
                let step = [offset[0] - was[0], offset[1] - was[1]];
                let size = step[0].hypot(step[1]);
                self.biggest = self.biggest.max(size);
                self.travel += size;
                if size > 0.25 {
                    if let Some(h) = self.heading.get(&l.id)
                        && h[0] * step[0] + h[1] * step[1] < 0.0
                    {
                        self.reversals += 1;
                    }
                    self.heading.insert(l.id.clone(), step);
                }
            }
            self.last.insert(l.id.clone(), offset);
        }
        // A label that vanished comes back at its spot, which isn't motion.
        self.last.retain(|id, _| labels.iter().any(|l| l.id == *id));
    }
}

/// Kris's test: a label moves only smoothly, and doesn't keep changing its mind.
#[test]
fn tangent_labels_glide_and_do_not_change_their_minds() {
    use pencil_core::frame::{Inputs, prepare};
    use pencil_core::place::{SPRING_RATE, SpringCache, spring};
    let f = tangents();
    let mut cache = SpringCache::new();
    let (mut settled, mut sprung) = (Motion::default(), Motion::default());
    for frame in 0..720 {
        let inputs = Inputs::at(frame as f64 / 60.0);
        let mut p = prepare(&f, &inputs, 640, 400, 1.0).unwrap();
        settled.see(&p.labels);
        spring(&f, &inputs, 640, 400, &mut p, &mut cache).unwrap();
        sprung.see(&p.labels);
    }
    println!(
        "settled: biggest {:.1}, reversals {}, travel {:.0}; sprung: biggest {:.1}, reversals {}, travel {:.0}",
        settled.biggest,
        settled.reversals,
        settled.travel,
        sprung.biggest,
        sprung.reversals,
        sprung.travel
    );
    assert!(sprung.travel > 20.0, "the labels do dodge here");
    // The spring's peak speed: rate / e of a leap, per second. Leaps close together add up,
    // so this allows for two at once, however far the spot goes. A label that has just
    // appeared has to obey this too.
    let peak = SPRING_RATE / std::f64::consts::E / 60.0;
    assert!(
        sprung.biggest <= 2.0 * peak * settled.biggest,
        "{} against a leap of {}",
        sprung.biggest,
        settled.biggest
    );
    assert!(
        sprung.reversals * 2 < settled.reversals.max(1),
        "{} reversals, against {} without the spring",
        sprung.reversals,
        settled.reversals
    );
}

/// A dashed construction circle follows its own path instead of being traced.
#[test]
fn a_dashed_circle_is_not_traced() {
    use pencil_core::frame::{Inputs, prepare};
    let doc = "viewport { x: -2..2, y: -2..2 }
               curve c = circle((0, 0), (1, 0))";
    let sheet = "style c { stroke: black; width: 2px; dash: 4px }";
    let f = build(doc, &[sheet]).unwrap();
    let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
    // World radius 1 is 100 px, and these all sit on it.
    for s in p.strokes[0].chunks(pencil_core::stroke::FLOATS_PER_SEGMENT) {
        let r = (s[0] as f64 - 200.0).hypot(s[1] as f64 - 200.0);
        assert!((r - 100.0).abs() < 1e-2, "{r}");
    }
    assert!(!p.strokes[0].is_empty());
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

fn square_read(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

fn square() -> pencil_core::Figure {
    build(
        &square_read("006-square/square.pen"),
        &[
            &square_read("theme.style"),
            &square_read("006-square/plain.style"),
        ],
    )
    .unwrap()
}

#[test]
fn square_ir_matches_golden() {
    let scene = compile(&parse(&square_read("006-square/square.pen")).unwrap()).unwrap();
    let actual = golden_text(&scene.to_json());
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/006-square/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

/// The point of the figure: F.O. + I.L. is z², and its triangle is the first one, scaled by
/// |z| and turned. The geometry is the authority, so this holds without looking at a frame.
#[test]
fn foil_builds_the_similar_triangle() {
    use pencil_core::{eval::frame_env, frame::Inputs};
    let f = square();
    let env = frame_env(&f, &Inputs::at(17.0)).unwrap();
    let get = |n: &str| env.get(n).unwrap();
    let (a, b) = (get("a"), get("b"));
    let (fo, il) = ((get("AZ.x"), get("AZ.y")), (get("BIZ.x"), get("BIZ.y")));
    let z2 = (get("Z2.x"), get("Z2.y"));
    // F.O. is a·z and I.L. is bi·z, so together they're the complex square.
    assert!((z2.0 - (a * a - b * b)).abs() < 1e-12, "{z2:?}");
    assert!((z2.1 - 2.0 * a * b).abs() < 1e-12, "{z2:?}");
    // The legs are square to each other, as a and bi are.
    assert!((fo.0 * il.0 + fo.1 * il.1).abs() < 1e-12);
    // Each side is the small triangle's, scaled by |z|.
    let size = a.hypot(b);
    assert!((fo.0.hypot(fo.1) - a * size).abs() < 1e-12);
    assert!((il.0.hypot(il.1) - b * size).abs() < 1e-12);
    assert!((z2.0.hypot(z2.1) - size * size).abs() < 1e-12);
    // And turned by arg z: twice the angle.
    assert!((z2.1.atan2(z2.0) - 2.0 * b.atan2(a)).abs() < 1e-12);
    // All of which the moving arrows have reached by then.
    assert!((get("tip_a.x") - fo.0).abs() < 1e-12);
    assert!((get("tip_b.x") - z2.0).abs() < 1e-12 && (get("tip_b.y") - z2.1).abs() < 1e-12);
}

/// What matches moves, solidly and without leaping, through the whole of Kris's sequence.
#[test]
fn square_terms_move_without_leaps() {
    use pencil_core::frame::{Inputs, prepare};
    use std::collections::HashMap;
    let f = square();
    let mut last: HashMap<String, (f64, f64)> = HashMap::new();
    let mut biggest = (0.0f64, String::new(), 0.0);
    for frame in 0..(19 * 60) {
        let time = frame as f64 / 60.0;
        let p = prepare(&f, &Inputs::at(time), 340, 235, 1.0).unwrap();
        let mut now = HashMap::new();
        for l in p.labels.iter().filter(|l| l.id.starts_with("work.")) {
            if let Some((x, y)) = last.get(&l.id) {
                let moved = (l.x - x).hypot(l.y - y);
                if moved > biggest.0 {
                    biggest = (moved, l.id.clone(), time);
                }
            }
            now.insert(l.id.clone(), (l.x, l.y));
        }
        last = now;
    }
    println!("biggest term step: {biggest:?}");
    // The blog's jumps 89 px here. The fastest honest move is a term on the sliding arrow,
    // which covers about 225 px with a quintic start.
    assert!(biggest.0 < 40.0, "{biggest:?}");
}

/// A frame depends only on its time, wherever it's sought from.
#[test]
fn square_seeks_in_any_order() {
    use pencil_core::frame::{Inputs, prepare};
    let f = square();
    let labels = |t: f64| prepare(&f, &Inputs::at(t), 340, 235, 1.0).unwrap().labels;
    let forward: Vec<_> = [3.0, 8.5, 13.2, 14.6].map(labels).into();
    let shuffled: Vec<_> = [14.6, 3.0, 13.2, 8.5].map(labels).into();
    assert_eq!(forward[0], shuffled[1]);
    assert_eq!(forward[1], shuffled[3]);
    assert_eq!(forward[2], shuffled[2]);
    assert_eq!(forward[3], shuffled[0]);
}

fn lattice() -> pencil_core::Composition {
    pencil_core::compose(
        &square_read("007-lattice/lattice.pen"),
        &[
            &square_read("theme.style"),
            &square_read("007-lattice/plain.style"),
        ],
    )
    .unwrap()
}

#[test]
fn lattice_ir_matches_golden() {
    let composition = lattice();
    let scenes: Vec<_> = composition
        .panels
        .iter()
        .map(|p| p.figure.scene.to_json())
        .collect();
    let actual = golden_text(&serde_json::Value::Array(scenes));
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/007-lattice/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

/// The lattice under some sheets, on top of its plain one.
fn lattice_with(more: &[&str]) -> pencil_core::Composition {
    let [theme, plain] = [
        square_read("theme.style"),
        square_read("007-lattice/plain.style"),
    ];
    let extra: Vec<String> = more.iter().map(|f| square_read(f)).collect();
    let mut sheets = vec![theme.as_str(), plain.as_str()];
    sheets.extend(extra.iter().map(String::as_str));
    pencil_core::compose(&square_read("007-lattice/lattice.pen"), &sheets).unwrap()
}

fn lattice_labels(composition: &pencil_core::Composition) -> Vec<pencil_core::frame::Label> {
    use pencil_core::{frame::Inputs, terms::Estimate};
    let inputs = Inputs {
        params: vec![("q".into(), 7.0), ("p".into(), 2.0)],
        ..Default::default()
    };
    let [w, h] = composition.size;
    let frames = composition
        .prepare(&inputs, w as u32, h as u32, 1.0, &Estimate)
        .unwrap();
    composition.labels(&frames, &inputs, &Estimate).unwrap()
}

/// WCAG's contrast ratio, from linear colors.
fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let lum = |c: [f64; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The test of a style sheet: the same document, down to every mark, in another theme and shape.
#[test]
fn sheets_restyle_the_lattice_without_changing_a_mark() {
    let plain = lattice_with(&[]);
    for more in [
        &["dark.style"][..],
        &["007-lattice/narrow.style"],
        &["007-lattice/narrow.style", "dark.style"],
    ] {
        let other = lattice_with(more);
        assert_eq!(plain.panels.len(), other.panels.len());
        for (a, b) in plain.panels.iter().zip(&other.panels) {
            assert_eq!(a.id, b.id);
            assert_eq!(
                a.figure.scene.to_json()["marks"],
                b.figure.scene.to_json()["marks"],
                "{more:?} changed a mark in {}",
                a.id
            );
        }
        let ids = |c: &pencil_core::Composition| {
            let mut ids: Vec<String> = lattice_labels(c).into_iter().map(|l| l.id).collect();
            ids.sort();
            ids
        };
        assert_eq!(ids(&plain), ids(&other), "{more:?} changed what's labelled");
    }
}

#[test]
fn a_later_sheet_changes_what_the_colors_mean_for_the_whole_figure() {
    use pencil_core::{frame::Inputs, terms::Estimate};
    for (more, dark) in [(&[][..], false), (&["dark.style"][..], true)] {
        let composition = lattice_with(more);
        let inputs = Inputs {
            params: vec![("q".into(), 7.0), ("p".into(), 2.0)],
            ..Default::default()
        };
        let frames = composition
            .prepare(&inputs, 800, 780, 1.0, &Estimate)
            .unwrap();
        for frame in &frames {
            let paper = frame.prepared.paper;
            let light = 0.2126 * paper[0] + 0.7152 * paper[1] + 0.0722 * paper[2];
            assert_eq!(light < 0.2, dark, "{more:?}: paper is {paper:?}");
        }
        // Every label stays readable on its paper, whichever theme it's in.
        let paper = frames[0].prepared.paper;
        for label in lattice_labels(&composition) {
            let ratio = contrast(label.color, paper);
            assert!(
                ratio >= 4.5,
                "{more:?}: `{}` is only {ratio:.2}:1",
                label.id
            );
        }
    }
}

#[test]
fn a_sheet_sets_how_the_panels_sit() {
    let wide = lattice_with(&[]);
    assert_eq!(wide.size, [814.0, 826.0]);
    assert_eq!(wide.panels[1].rect[0], 414.0 / 814.0);

    let narrow = lattice_with(&["007-lattice/narrow.style"]);
    assert_eq!(narrow.size, [360.0, 4.0 * (300.0 + 24.0) + 3.0 * 12.0]);
    for (i, panel) in narrow.panels.iter().enumerate() {
        assert_eq!(panel.rect[0], 0.0, "{} isn't in the one column", panel.id);
        assert_eq!(panel.rect[2], 1.0);
        let top = (i as f64 * (300.0 + 24.0 + 12.0) + 24.0) / narrow.size[1];
        assert!((panel.rect[1] - top).abs() < 1e-12, "{}", panel.id);
        assert_eq!(panel.figure.scene.viewport.design_height, Some(300.0));
    }
    // An outline shows what its panel shows under this sheet, not under the document alone.
    let outlined = narrow.panels[2]
        .figure
        .scene
        .marks
        .iter()
        .find_map(|m| match &m.form {
            pencil_core::ir::Form::Outline { view, .. } => view.clone(),
            _ => None,
        })
        .unwrap();
    assert_eq!(outlined.design_height, Some(300.0));
    assert_eq!(outlined.pad, 0.08);
    assert_eq!(wide.panels[0].figure.scene.viewport.pad, 0.15);
    assert_eq!(wide.panels[3].figure.scene.viewport.pad, 0.08);
}

#[test]
fn a_panel_rule_is_checked() {
    let doc = square_read("007-lattice/lattice.pen");
    let [theme, plain] = [
        square_read("theme.style"),
        square_read("007-lattice/plain.style"),
    ];
    for (rule, says) in [
        ("style panel.nope { pad: 0.1 }", "no panel named `nope`"),
        ("style panel { width: 300 }", "needs a unit"),
        ("style panels { columns: q }", "q"),
        ("style panels { rows: 2 }", "not `rows`"),
        ("style panel { size: 2px }", "not `size`"),
    ] {
        let err = pencil_core::compose(&doc, &[&theme, &plain, rule]).unwrap_err();
        assert!(err.message.contains(says), "{rule}: {err}");
    }
    // A document draws on its own, at a default size.
    let bare = pencil_core::compose(&doc, &[]).unwrap();
    assert_eq!(
        bare.panels[0].figure.scene.viewport.design_width,
        Some(400.0)
    );
    let err = pencil_core::compose(&doc, &[&theme, &plain, "style panel.flat { width: 300px }"])
        .unwrap_err();
    assert!(err.message.contains("same size"), "{err}");
}

#[test]
fn a_page_can_read_a_themes_colors() {
    let [theme, dark] = [square_read("theme.style"), square_read("dark.style")];
    let light = pencil_core::named_colors(&[&theme]).unwrap();
    let night = pencil_core::named_colors(&[&theme, &dark]).unwrap();
    let names = |c: &[(String, [f64; 3])]| c.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    // The dark theme defines every color the light one does, and each comes out different.
    assert_eq!(names(&light), names(&night));
    assert!(names(&light).contains(&"paper_color".to_string()));
    for (a, b) in light.iter().zip(&night) {
        assert_ne!(a.1, b.1, "`{}` doesn't change in the dark theme", a.0);
    }
}

/// A circle from the spiral's collection, by its grid point.
fn spiral_circle(
    figure: &pencil_core::Figure,
    env: &pencil_core::eval::Env,
    a: f64,
    b: f64,
) -> ([f64; 2], f64) {
    use pencil_core::{ir::Form, stroke::instance};
    let (lets, shape) = figure
        .scene
        .marks
        .iter()
        .find_map(|m| match &m.form {
            Form::Many { lets, shape, .. } if m.id == "spiral_circles" => Some((lets, shape)),
            _ => None,
        })
        .unwrap();
    let mut local = env.clone();
    local.set("a", a);
    local.set("b", b);
    let pencil_core::stroke::Instance::Circle(center, r) =
        instance(lets, shape, &mut local).unwrap().unwrap()
    else {
        panic!("a spiral's circles have centers");
    };
    (center, r)
}

/// What makes it a Doyle spiral: every circle touches its six neighbors. The geometry is the
/// authority, so this checks what `solve` found rather than trusting it, for every q and p
/// that the blog's sliders allow.
#[test]
fn every_circle_touches_its_six_neighbors_for_every_q_and_p() {
    use pencil_core::{eval::frame_env, frame::Inputs};
    let composition = lattice();
    let whole = &composition
        .panels
        .iter()
        .find(|p| p.id == "whole")
        .unwrap()
        .figure;
    let mut checked = 0;
    for q in 3..=12 {
        for p in 0..=q.min(7) {
            let inputs = Inputs {
                time: 0.0,
                params: vec![("q".to_string(), q as f64), ("p".to_string(), p as f64)],
            };
            let env = frame_env(whole, &inputs).unwrap();
            assert!(env.get("mu").is_some(), "q={q}, p={p} has no solution");
            for a in -4..=4 {
                for b in -4..=4 {
                    let (a, b) = (a as f64, b as f64);
                    let (c, r) = spiral_circle(whole, &env, a, b);
                    for (da, db) in [
                        (1.0, 0.0),
                        (0.0, 1.0),
                        (1.0, 1.0),
                        (-1.0, 0.0),
                        (0.0, -1.0),
                        (-1.0, -1.0),
                    ] {
                        let (n, s) = spiral_circle(whole, &env, a + da, b + db);
                        let gap = (c[0] - n[0]).hypot(c[1] - n[1]) - (r + s);
                        // Relative, since the circles span a wide range of sizes.
                        assert!(
                            gap.abs() < 1e-9 * (r + s),
                            "q={q}, p={p}, ({a}, {b}) to ({da}, {db}): {gap}"
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 30_000, "{checked}");
}

/// The same numbers as the site's own solver, at its default.
#[test]
fn solve_agrees_with_the_sites_newton() {
    use pencil_core::{eval::frame_env, frame::Inputs};
    let composition = lattice();
    let env = frame_env(&composition.panels[0].figure, &Inputs::at(0.0)).unwrap();
    let get = |n: &str| env.get(n).unwrap();
    assert!(
        (get("mu") - -0.00018454203190822107).abs() < 1e-9,
        "{}",
        get("mu")
    );
    assert!(
        (get("nu") - 1.0450243194841884).abs() < 1e-9,
        "{}",
        get("nu")
    );
    assert!(
        (get("k") - 0.48205844419983157).abs() < 1e-9,
        "{}",
        get("k")
    );
}

/// Remembering the last answer is only a shortcut: it gives what solving afresh would.
#[test]
fn remembering_a_system_changes_nothing() {
    use pencil_core::{
        eval::{frame_env, scene_env},
        frame::Inputs,
    };
    let composition = lattice();
    let figure = &composition.panels[0].figure;
    for (q, p) in [(7.0, 2.0), (7.0, 2.0), (12.0, 7.0), (3.0, 0.0), (7.0, 2.0)] {
        let inputs = Inputs {
            time: 0.0,
            params: vec![("q".to_string(), q), ("p".to_string(), p)],
        };
        let remembered = frame_env(figure, &inputs).unwrap();
        let fresh = scene_env(&figure.scene, &inputs, None).unwrap();
        for name in ["mu", "nu", "k"] {
            assert_eq!(remembered.get(name), fresh.get(name), "{name} at {q}, {p}");
        }
    }
}

/// Looked at from any order, it's the same frame: it starts over every time, with no memory.
#[test]
fn solve_has_no_memory_of_the_last_answer() {
    use pencil_core::{eval::frame_env, frame::Inputs};
    let composition = lattice();
    let figure = &composition.panels[0].figure;
    let at = |q: f64, p: f64| {
        let inputs = Inputs {
            time: 0.0,
            params: vec![("q".to_string(), q), ("p".to_string(), p)],
        };
        frame_env(figure, &inputs).unwrap().get("nu").unwrap()
    };
    let first = at(7.0, 2.0);
    at(12.0, 7.0);
    at(3.0, 0.0);
    assert_eq!(first.to_bits(), at(7.0, 2.0).to_bits());
}

fn hearts(sheets: &[&str]) -> pencil_core::Figure {
    pencil_core::build(&square_read("008-hearts/hearts.pen"), sheets).unwrap()
}

#[test]
fn hearts_ir_matches_golden() {
    let figure = hearts(&[]);
    let actual = golden_text(&figure.scene.to_json());
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/008-hearts/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

/// A point of a curve in the hearts, with this shift and angle.
fn hearts_curve(
    figure: &pencil_core::Figure,
    id: &str,
    shift: f64,
    theta: f64,
    t: f64,
) -> [f64; 2] {
    use pencil_core::{eval::frame_env, eval::real, frame::Inputs, ir::Form};
    let inputs = Inputs {
        time: 0.0,
        params: vec![("shift".to_string(), shift), ("theta".to_string(), theta)],
    };
    let mut env = frame_env(figure, &inputs).unwrap();
    let arc = figure
        .scene
        .marks
        .iter()
        .find_map(|m| match &m.form {
            Form::Parametric(arc) if m.id == id => Some(arc),
            _ => None,
        })
        .unwrap();
    env.set(&arc.var, t);
    [
        real(&arc.point[0], &env).unwrap(),
        real(&arc.point[1], &env).unwrap(),
    ]
}

/// Against the textbook's form of bipolar coordinates, and against what σ and τ mean, neither
/// of which goes through coth: x = sinh τ / D and y = sin σ / D, with D = cosh τ − cos σ.
#[test]
fn hearts_are_bipolar_coordinates() {
    let figure = hearts(&[]);
    let tau_of = |a: f64| ((a - std::f64::consts::PI).cos() * 0.999999).atanh();
    for shift in [0.0, 0.8, std::f64::consts::FRAC_PI_2, 2.5] {
        for i in 1..200 {
            let t = i as f64 / 200.0 * std::f64::consts::TAU;
            let (sigma, tau) = (t + shift, tau_of(t));
            let d = tau.cosh() - sigma.cos();
            if d < 1e-3 {
                continue; // At the pole.
            }
            let want = [tau.sinh() / d, sigma.sin() / d];
            let got = hearts_curve(&figure, "heart_a", shift, 0.0, t);
            let scale = 1.0 + want[0].abs() + want[1].abs();
            assert!(
                (got[0] - want[0]).abs() < 1e-9 * scale && (got[1] - want[1]).abs() < 1e-9 * scale,
                "shift {shift}, t {t}: {got:?} {want:?}"
            );
            // τ is the log of the ratio of distances to the foci, and σ the angle between them.
            let [x, y] = got;
            let ratio = ((x + 1.0).hypot(y) / (x - 1.0).hypot(y)).ln();
            assert!(
                (ratio - tau).abs() < 1e-7 * (1.0 + tau.abs()),
                "τ at {t}: {ratio} {tau}"
            );
            let turn = y.atan2(x + 1.0) - y.atan2(x - 1.0) + sigma;
            let off = (turn / std::f64::consts::TAU).round() * std::f64::consts::TAU - turn;
            assert!(off.abs() < 1e-7, "σ at {t}: {off}");
        }
    }
}

/// The coordinate curves are the circles they should be. Constant τ has its center at coth τ and
/// radius |csch τ|, and constant σ has its center at i·cot σ and radius |csc σ|.
#[test]
fn hearts_coordinate_curves_are_circles() {
    let figure = hearts(&[]);
    let tau_of = |a: f64| ((a - std::f64::consts::PI).cos() * 0.999999).atanh();
    for theta in [0.4, 1.2, 2.945, 4.0, 5.5] {
        let tau: f64 = tau_of(theta);
        let (center, radius) = (1.0 / tau.tanh(), (1.0 / tau.sinh()).abs());
        let sigma: f64 = theta;
        let (rise, reach) = (sigma.cos() / sigma.sin(), (1.0 / sigma.sin()).abs());
        for i in 1..50 {
            let t = i as f64 / 50.0 * std::f64::consts::TAU;
            let [x, y] = hearts_curve(&figure, "level_tau", 0.0, theta, t);
            assert!(
                ((x - center).hypot(y) - radius).abs() < 1e-9 * (1.0 + radius),
                "τ circle at {theta}"
            );
            let [x, y] = hearts_curve(&figure, "level_sigma_a", 0.0, theta, t);
            assert!(
                (x.hypot(y - rise) - reach).abs() < 1e-9 * (1.0 + reach),
                "σ circle at {theta}"
            );
        }
    }
}

/// Everything drawn in the frame has to be on the curve. A coordinate circle goes through infinity
/// as it turns into a line and back, and a heart can too, and samples either side of a pole
/// mustn't be joined by a chord across the figure. The truth here is dense sampling, not the
/// sampler's own subdivision.
#[test]
fn hearts_draw_no_false_chords_at_a_pole() {
    use pencil_core::{
        eval::{frame_env, real},
        frame::{Inputs, view_for},
        ir::Form,
        stroke::sample_arc,
    };
    use std::collections::HashSet;
    use std::f64::consts::{FRAC_PI_2, PI};
    let figure = hearts(&[]);
    const CELL: f64 = 0.08;
    let cell = |x: f64, y: f64| ((x / CELL).floor() as i64, (y / CELL).floor() as i64);
    for (shift, theta) in [
        (0.0, FRAC_PI_2),
        (0.0, FRAC_PI_2 + 1e-7),
        (0.0, FRAC_PI_2 - 1e-7),
        (0.0, FRAC_PI_2 + 1e-4),
        (0.0, FRAC_PI_2 + 0.02),
        (0.0, 3.0 * FRAC_PI_2 + 1e-7),
        (0.123, FRAC_PI_2 + 1e-7),
        (0.123, FRAC_PI_2 - 3e-7),
        (0.123, FRAC_PI_2 + 2e-8),
        (FRAC_PI_2, 0.9),
        (1.57, 0.9),
        (FRAC_PI_2 + 3e-4, FRAC_PI_2),
        (3.0 * FRAC_PI_2, 2.0),
    ] {
        let inputs = Inputs {
            time: 0.0,
            params: vec![("shift".to_string(), shift), ("theta".to_string(), theta)],
        };
        let env = frame_env(&figure, &inputs).unwrap();
        let view = view_for(&figure, &inputs, 900, 765).unwrap();
        let [x0, x1, y0, y1] = [view.x[0], view.x[1], view.y[0], view.y[1]];
        for id in [
            "level_tau",
            "level_sigma_a",
            "level_sigma_b",
            "heart_a",
            "heart_b",
        ] {
            let arc = figure
                .scene
                .marks
                .iter()
                .find_map(|m| match &m.form {
                    Form::Parametric(arc) if m.id == id => Some(arc),
                    _ => None,
                })
                .unwrap();
            let mut truth = HashSet::new();
            let mut dense = env.clone();
            for k in 0..=6000 {
                dense.set(&arc.var, k as f64 / 6000.0 * 2.0 * PI);
                let (Ok(x), Ok(y)) = (real(&arc.point[0], &dense), real(&arc.point[1], &dense))
                else {
                    continue;
                };
                if x.is_finite() && y.is_finite() {
                    truth.insert(cell(x, y));
                }
            }
            let near = |x: f64, y: f64| {
                let (cx, cy) = cell(x, y);
                (-2..=2).any(|a| (-2..=2).any(|b| truth.contains(&(cx + a, cy + b))))
            };
            let sampled = sample_arc(arc, &env, &view, 900, 765).unwrap();
            for piece in &sampled.pieces {
                for pair in piece.windows(2) {
                    let [a, b] = [pair[0], pair[1]];
                    let steps = ((b[0] - a[0]).hypot(b[1] - a[1]) / 0.02)
                        .ceil()
                        .clamp(1.0, 2e5) as usize;
                    // Only the part inside the frame shows.
                    for k in 0..=steps {
                        let u = k as f64 / steps as f64;
                        let (x, y) = (a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u);
                        let inside = x > x0 && x < x1 && y > y0 && y < y1;
                        assert!(
                            !inside || near(x, y),
                            "shift {shift}, θ {theta}, `{id}`: a chord crosses the frame at ({x:.2}, {y:.2}), from {a:?} to {b:?}"
                        );
                    }
                }
            }
        }
    }
}

fn squaring() -> pencil_core::Figure {
    pencil_core::build(&square_read("009-squaring/squaring.pen"), &[]).unwrap()
}

#[test]
fn squaring_ir_matches_golden() {
    let actual = golden_text(&squaring().scene.to_json());
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/009-squaring/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

/// A curve in the squaring map, at this n and p. `plain_across:0` is the one with s = 0.
fn squaring_point(figure: &pencil_core::Figure, id: &str, n: f64, p: f64, t: f64) -> [f64; 2] {
    use pencil_core::{
        eval::frame_env,
        eval::real,
        frame::Inputs,
        ir::{Form, Shape},
    };
    let (id, s) = match id.split_once(':') {
        Some((id, s)) => (id, s.parse::<f64>().unwrap()),
        None => (id, 0.0),
    };
    let inputs = Inputs {
        time: 0.0,
        params: vec![("n".to_string(), n), ("p".to_string(), p)],
    };
    let mut env = frame_env(figure, &inputs).unwrap();
    let arc = figure
        .scene
        .marks
        .iter()
        .find_map(|m| match &m.form {
            Form::Parametric(arc) if m.id == id => Some(arc),
            Form::Many {
                shape: Shape::Curves(arc),
                ..
            } if m.id == id => Some(arc),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no `{id}`"));
    env.set("s", s);
    env.set(&arc.var, t);
    [
        real(&arc.point[0], &env).unwrap(),
        real(&arc.point[1], &env).unwrap(),
    ]
}

/// The circle through n and 1/n squares to the limaçon r(θ) = 2k(1 + k cos θ) / (1 − k²) about 1,
/// with k = tanh(ln n), written here from the site's `doyle.ts`, not from squaring anything.
#[test]
fn the_circle_of_n_squares_to_the_limacon() {
    let figure = squaring();
    for n in [1.5, 3.3, 5.0f64] {
        let k = n.ln().tanh();
        for i in 0..240 {
            let t = i as f64 / 240.0 * std::f64::consts::TAU;
            let [x, y] = squaring_point(&figure, "square_orbit", n, 1.5, t);
            let (dx, dy) = (x - 1.0, y);
            let theta = dy.atan2(dx);
            let r = 2.0 * k * (1.0 + k * theta.cos()) / (1.0 - k * k);
            assert!(
                (dx.hypot(dy) - r).abs() < 1e-9 * (1.0 + r),
                "n {n}, t {t}: {} from 1, the limaçon has {r}",
                dx.hypot(dy)
            );
        }
        // It reaches n² and 1/n².
        let far = squaring_point(&figure, "square_orbit", n, 1.5, 0.0);
        assert!((far[0] - n * n).abs() < 1e-9, "{far:?}");
        let near = squaring_point(&figure, "square_orbit", n, 1.5, std::f64::consts::PI);
        assert!((near[0] - 1.0 / (n * n)).abs() < 1e-9, "{near:?}");
    }
}

/// z^p is the plain figure at p = 1 and the square at p = 2, and conformal in between, so the grid's
/// lines still cross square.
#[test]
fn the_morph_runs_from_the_figure_to_its_square_and_keeps_right_angles() {
    let figure = squaring();
    let n = 3.3f64;
    for (plain, moving, square) in [
        ("plain_across:0", "moving_across:0", "square_across:0"),
        ("plain_up:2", "moving_up:2", "square_up:2"),
        ("orbit", "moving_orbit", "square_orbit"),
    ] {
        for t in [0.1, 0.5, 1.0] {
            let a = squaring_point(&figure, plain, n, 1.0, t);
            let b = squaring_point(&figure, moving, n, 1.0, t);
            assert!(
                (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12,
                "{plain} at p = 1"
            );
            let c = squaring_point(&figure, moving, n, 2.0, t);
            let d = squaring_point(&figure, square, n, 2.0, t);
            assert!(
                (c[0] - d[0]).abs() < 1e-9 && (c[1] - d[1]).abs() < 1e-9,
                "{moving} at p = 2"
            );
        }
    }
    // Where across(0) meets up(0): x0 on the real axis. Their images cross at 90° for every p.
    let x0 = n.ln().cosh();
    for p in [1.0, 1.3, 1.7, 2.0] {
        let h = 1e-5;
        let along = |id: &str, t: f64| {
            let (a, b) = (
                squaring_point(&figure, id, n, p, t - h),
                squaring_point(&figure, id, n, p, t + h),
            );
            [b[0] - a[0], b[1] - a[1]]
        };
        let v = along("moving_across:0", 0.0);
        let u = along("moving_up:0", x0);
        let cosine = (v[0] * u[0] + v[1] * u[1]) / (v[0].hypot(v[1]) * u[0].hypot(u[1]));
        assert!(cosine.abs() < 1e-6, "p {p}: {cosine}");
    }
}

fn mobius() -> pencil_core::Composition {
    let [theme, plain] = [
        square_read("theme.style"),
        square_read("010-mobius/plain.style"),
    ];
    pencil_core::compose(&square_read("010-mobius/mobius.pen"), &[&theme, &plain]).unwrap()
}

#[test]
fn mobius_ir_matches_golden() {
    let scenes: Vec<_> = mobius()
        .panels
        .iter()
        .map(|p| p.figure.scene.to_json())
        .collect();
    let actual = golden_text(&serde_json::Value::Array(scenes));
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/010-mobius/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

fn circumcircle() -> pencil_core::Figure {
    build(
        &square_read("011-circumcircle/circumcircle.pen"),
        &[&square_read("011-circumcircle/plain.style")],
    )
    .unwrap()
}

#[test]
fn circumcircle_ir_matches_golden() {
    let actual = golden_text(&circumcircle().scene.to_json());
    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/011-circumcircle/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

/// The triangle turns over at about 0.59 s, and the circle goes through a line and comes back
/// from the other side, inside out. Its curvature changes sign, and nothing else notices.
#[test]
fn the_circumcircle_passes_through_a_line_without_a_flash() {
    use pencil_core::eval::{Frame, frame_env};
    use pencil_core::frame::Inputs;
    use pencil_core::ir::Expr;
    let figure = circumcircle();
    let curvature = |time: f64| {
        let env = frame_env(&figure, &Inputs::at(time)).unwrap();
        pencil_core::eval::real(&Expr::sym("around.a"), &env).unwrap()
    };
    let (mut before, mut after) = (0.5, 0.7);
    assert!(curvature(before) > 0.0 && curvature(after) < 0.0);
    for _ in 0..50 {
        let middle = (before + after) / 2.0;
        if curvature(middle) > 0.0 {
            before = middle;
        } else {
            after = middle;
        }
    }
    let (w, h) = (160, 100);
    let frame = |time| Frame::new(&figure, &Inputs::at(time), w, h).unwrap();
    let (near, far) = (frame(before - 1e-4), frame(after + 1e-4));
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let (a, b) = (near.shade(px, py).unwrap(), far.shade(px, py).unwrap());
            for channel in 0..3 {
                assert!(
                    (a[channel] - b[channel]).abs() < 0.02,
                    "{px}, {py}: {a:?} and {b:?}"
                );
            }
        }
    }
    // Which isn't how the disk behaves: it's on the other side now, so the same pixels would swap.
    let sheet = square_read("011-circumcircle/plain.style")
        .replace("::negative", "::inside")
        .replace("::positive", "::outside");
    let disk = build(&square_read("011-circumcircle/circumcircle.pen"), &[&sheet]).unwrap();
    let frame = |time| Frame::new(&disk, &Inputs::at(time), w, h).unwrap();
    let (near, far) = (frame(before - 1e-4), frame(after + 1e-4));
    let (a, b) = (near.shade(5.5, 5.5).unwrap(), far.shade(5.5, 5.5).unwrap());
    assert!((a[0] - b[0]).abs() > 0.05, "{a:?} and {b:?}");
}

#[test]
fn limacon_ir_matches_golden() {
    let figure = build(
        &square_read("012-limacon/limacon.pen"),
        &[&square_read("012-limacon/plain.style")],
    )
    .unwrap();
    let actual = golden_text(&figure.scene.to_json());
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/012-limacon/expected.ir.json");
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    assert_eq!(actual, std::fs::read_to_string(golden).unwrap());
}

// Complex numbers here are worked out by hand, apart from anything the compiler does.
type Z = (f64, f64);
fn z_mul(a: Z, b: Z) -> Z {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
fn z_div(a: Z, b: Z) -> Z {
    let n = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / n, (a.1 * b.0 - a.0 * b.1) / n)
}
fn z_pow(w: Z, m: i32) -> Z {
    let mut out = (1.0, 0.0);
    for _ in 0..m.abs() {
        out = z_mul(out, w);
    }
    if m < 0 { z_div((1.0, 0.0), out) } else { out }
}
const COXETER_W: Z = (-1.7861513777574234, 2.272019649514069);
fn coxeter_k() -> f64 {
    (3.0 * 5f64.sqrt() / 2.0 - 2.5).sqrt()
}
fn z_abs(z: Z) -> f64 {
    z.0.hypot(z.1)
}
/// The map, directly.
fn z_map(map: [f64; 4], z: Z) -> Z {
    let [a, b, c, d] = map;
    z_div((a * z.0 - b, a * z.1), (c * z.0 + d, c * z.1))
}

/// A circle's center and radius, fitted through three points.
fn circle_through(p: Z, q: Z, r: Z) -> (Z, f64) {
    let d = 2.0 * (p.0 * (q.1 - r.1) + q.0 * (r.1 - p.1) + r.0 * (p.1 - q.1));
    let (pp, qq, rr) = (
        p.0 * p.0 + p.1 * p.1,
        q.0 * q.0 + q.1 * q.1,
        r.0 * r.0 + r.1 * r.1,
    );
    let c = (
        (pp * (q.1 - r.1) + qq * (r.1 - p.1) + rr * (p.1 - q.1)) / d,
        (pp * (r.0 - q.0) + qq * (p.0 - r.0) + rr * (q.0 - p.0)) / d,
    );
    (c, (p.0 - c.0).hypot(p.1 - c.1))
}

/// A circle from the image panel's collection, at this power and map.
fn mobius_rim(
    composition: &pencil_core::Composition,
    panel: usize,
    m: f64,
    map: [f64; 4],
) -> Option<(Z, f64)> {
    use pencil_core::{
        eval::frame_env,
        frame::Inputs,
        ir::Form,
        stroke::{Instance, instance},
    };
    let figure = &composition.panels[panel].figure;
    let inputs = Inputs {
        time: 3.5,
        params: ["a", "b", "c", "d"]
            .iter()
            .zip(map)
            .map(|(n, v)| (n.to_string(), v))
            .collect(),
    };
    let mut env = frame_env(figure, &inputs).unwrap();
    let (lets, shape) = figure
        .scene
        .marks
        .iter()
        .find_map(|mark| match &mark.form {
            Form::Many { lets, shape, .. } if mark.id == "rims" => Some((lets, shape)),
            _ => None,
        })
        .unwrap();
    env.set("m", m);
    let Instance::Cycle([a, bx, by, _]) = instance(lets, shape, &mut env).unwrap()? else {
        panic!("rims are circles through three points");
    };
    Some(((bx / a, by / a), 1.0 / a.abs()))
}

/// The sequence itself: every circle touches the ones one, two and three steps along, and no others.
#[test]
fn coxeters_circles_touch_their_neighbors() {
    let composition = mobius();
    // A map that does nothing shows the sequence itself.
    let map = [1.0, 0.0, 0.0, 1.0];
    for m in -4..=4 {
        let (center, r) = mobius_rim(&composition, 0, m as f64, map).unwrap();
        let z = z_pow(COXETER_W, m);
        assert!(
            (center.0 - z.0).abs() < 1e-9 * (1.0 + z_abs(z)),
            "power {m}: {center:?}, {z:?}"
        );
        assert!(
            (r - coxeter_k() * z_abs(z)).abs() < 1e-9 * (1.0 + r),
            "radius of power {m}"
        );
        for step in 1..=3 {
            let (other, s) = mobius_rim(&composition, 0, (m + step) as f64, map).unwrap();
            let gap = (center.0 - other.0).hypot(center.1 - other.1) - (r + s);
            assert!(
                gap.abs() < 1e-9 * (r + s),
                "{m} and {} are {gap} apart",
                m + step
            );
        }
        let (other, s) = mobius_rim(&composition, 0, (m + 4) as f64, map).unwrap();
        assert!(
            (center.0 - other.0).hypot(center.1 - other.1) > r + s + 1e-3,
            "{m} touches its fourth"
        );
    }
}

/// The closed form for an image circle, against three points of the circle sent through the map
/// directly, for maps with the pole inside a circle, outside them all, and nearly on one.
#[test]
fn a_mobius_map_sends_each_circle_to_the_circle_written_for_it() {
    let composition = mobius();
    let k = coxeter_k();
    for map in [
        [1.0, 1.0, 0.5, 2.0],
        [1.0, 1.0, 0.5, 0.8],
        [2.0, 3.0, 1.0, 1.5],
        [1.0, 0.0, 0.0, 1.0],
        [-1.0, 4.0, 2.0, -3.0],
        [3.0, 1.0, -0.25, 2.0],
    ] {
        for m in -6..=6 {
            let z = z_pow(COXETER_W, m);
            let r = k * z_abs(z);
            let on = |t: f64| z_map(map, (z.0 + r * t.cos(), z.1 + r * t.sin()));
            let (center, radius) = circle_through(on(0.3), on(2.2), on(4.4));
            let (got, size) = mobius_rim(&composition, 0, m as f64, map).unwrap();
            let scale = 1.0 + radius + z_abs(center);
            assert!(
                (got.0 - center.0).hypot(got.1 - center.1) < 1e-6 * scale,
                "map {map:?}, power {m}: center {got:?}, wanted {center:?}"
            );
            assert!(
                (size - radius).abs() < 1e-6 * scale,
                "map {map:?}, power {m}: radius {size}, wanted {radius}"
            );
        }
    }
}

/// So they all still touch, inside or out.
#[test]
fn circles_still_touch_after_a_mobius_map() {
    let composition = mobius();
    for map in [
        [1.0, 1.0, 0.5, 2.0],
        [1.0, 1.0, 0.5, 0.8],
        [2.0, 3.0, 1.0, 1.5],
    ] {
        for m in -5..=5 {
            let (p, r) = mobius_rim(&composition, 0, m as f64, map).unwrap();
            for step in 1..=3 {
                let (q, s) = mobius_rim(&composition, 0, (m + step) as f64, map).unwrap();
                let apart = (p.0 - q.0).hypot(p.1 - q.1);
                let off = (apart - (r + s)).abs().min((apart - (r - s).abs()).abs());
                assert!(
                    off < 1e-7 * (1.0 + r + s),
                    "map {map:?}: {m} and {} miss by {off}",
                    m + step
                );
            }
        }
    }
}

/// The pole exactly on a circle makes it a line, the pole at its center is an ordinary circle, and
/// a map with ad + bc = 0 squashes everything to a point. Nothing unusable reaches the GPU.
#[test]
fn a_pole_on_a_circle_draws_nothing_it_cant() {
    use pencil_core::{frame::Inputs, terms::Estimate};
    let composition = mobius();
    let k = coxeter_k();
    let finite = |map: [f64; 4]| {
        let inputs = Inputs {
            time: 3.5,
            params: ["a", "b", "c", "d"]
                .iter()
                .zip(map)
                .map(|(n, v)| (n.to_string(), v))
                .collect(),
        };
        let frames = composition
            .prepare(&inputs, 720, 590, 1.0, &Estimate)
            .unwrap();
        for frame in &frames {
            for (layer, floats) in frame.prepared.strokes.iter().enumerate() {
                assert!(
                    floats.iter().all(|v| v.is_finite()),
                    "map {map:?}, layer {layer}"
                );
            }
            assert!(
                frame.prepared.sides.iter().all(|v| v.is_finite()),
                "map {map:?}, sides"
            );
        }
    };
    // z = 1 has radius k, so these put −d/c on it, and at its center.
    finite([1.0, 1.0, 1.0, -(1.0 + k)]);
    finite([1.0, 1.0, 1.0, -(1.0 - k)]);
    finite([2.0, 1.0, 1.0, -1.0]);
    finite([1.0, -2.0, 1.0, 2.0]);
    finite([0.0, 0.0, 0.0, 0.0]);
    // (2z − 1) / (z − 1) = 2 + 1 / (z − 1), so the circle of radius k around 1 goes to the one of
    // radius 1/k around 2.
    let (center, r) = mobius_rim(&composition, 0, 0.0, [2.0, 1.0, 1.0, -1.0]).unwrap();
    assert!(
        (center.0 - 2.0).abs() < 1e-9 && center.1.abs() < 1e-9,
        "{center:?}"
    );
    assert!((r - 1.0 / k).abs() < 1e-9, "{r}");
}

/// What's drawn, not only what's worked out. Kris's values send two circles out to nearly lines, about
/// 26,000 px across, where any difference between the two axes' scales moves them by tens of pixels. So
/// this asks for a canvas that isn't the shape of the view, and holds every piece of every rim and
/// every disk to circles fitted through points sent through the map directly.
#[test]
fn nearly_straight_circles_are_drawn_where_they_are() {
    use pencil_core::{frame::Inputs, stroke::FLOATS_PER_SEGMENT, terms::Estimate};
    let composition = mobius();
    let map = [3.6, 4.24, -2.53, -0.97];
    let k = coxeter_k();
    let truth: Vec<(Z, f64)> = (-8..=8)
        .map(|m| {
            let z = z_pow(COXETER_W, m);
            let r = k * z_abs(z);
            let on = |t: f64| z_map(map, (z.0 + r * t.cos(), z.1 + r * t.sin()));
            circle_through(on(0.3), on(2.2), on(4.4))
        })
        .collect();
    assert!(
        truth.iter().filter(|(_, r)| *r > 500.0).count() == 2,
        "{truth:?}"
    );
    let inputs = Inputs {
        time: 3.5,
        params: ["a", "b", "c", "d"]
            .iter()
            .zip(map)
            .map(|(n, v)| (n.to_string(), v))
            .collect(),
    };
    let (w, h) = (900u32, 1400u32);
    let frames = composition.prepare(&inputs, w, h, 1.0, &Estimate).unwrap();
    let prepared = &frames[0].prepared;
    let view = &prepared.view;
    let (sx, sy) = (
        w as f64 / (view.x[1] - view.x[0]),
        h as f64 / (view.y[1] - view.y[0]),
    );
    assert!(
        (sx / sy - 1.0).abs() < 1e-12,
        "the view is stretched: {sx} and {sy}"
    );
    let figure = &composition.panels[0].figure;
    let layer_of = |id: &str| {
        figure
            .scene
            .marks
            .iter()
            .zip(&figure.styles.marks)
            .take_while(|(mark, _)| mark.id != id)
            .filter(|(mark, style)| style.stroke_is_line(&mark.form))
            .count()
    };
    let world = |x: f64, y: f64| (view.x[0] + x / sx, view.y[1] - y / sy);
    let rims = &prepared.strokes[layer_of("rims")];
    assert!(
        rims.len() > 100 * FLOATS_PER_SEGMENT,
        "{} floats",
        rims.len()
    );
    for segment in rims.chunks(FLOATS_PER_SEGMENT) {
        for (x, y) in [(segment[0], segment[1]), (segment[2], segment[3])] {
            let (px, py) = world(x as f64, y as f64);
            // Off the frame, it doesn't show.
            if !(px > view.x[0] && px < view.x[1] && py > view.y[0] && py < view.y[1]) {
                continue;
            }
            let off = truth
                .iter()
                .map(|(c, r)| ((px - c.0).hypot(py - c.1) - r).abs() * sx)
                .fold(f64::INFINITY, f64::min);
            assert!(
                off < 0.25,
                "a rim is {off} px from every circle, at ({px}, {py})"
            );
        }
    }
    // The sides come from coefficients, in single precision, as the shader gets them. Each is
    // measured against its true circle at a point on the frame.
    let sided = figure.sided();
    let group = sided
        .iter()
        .find(|g| figure.scene.marks[g.mark].id == "rims")
        .unwrap();
    let count = prepared.values[group.slot] as usize;
    assert_eq!(count, truth.len());
    let (px, py) = (0.4 * view.x[1], 0.3 * view.y[1]);
    let rows = prepared.sides[group.base * 4..].chunks(group.stride * 4);
    for (row, (center, r)) in rows.take(count).zip(&truth) {
        let [a, bx, by, c] = [row[0], row[1], row[2], row[3]];
        let (x, y) = (px as f32, py as f32);
        let f = a * (x * x + y * y) - 2.0 * (bx * x + by * y) + c;
        let d = f / (1.0 + (1.0 + a * f).max(0.0).sqrt());
        let exact = (px - center.0).hypot(py - center.1) - r;
        assert!(
            (d.abs() as f64 - exact.abs()).abs() * sx < 0.5,
            "off by {} px, where its radius is {r}",
            (d.abs() as f64 - exact.abs()) * sx
        );
    }
}

/// A view keeps equal scales, whatever shape it's drawn at, by showing more and cropping nothing.
#[test]
fn an_explicit_view_never_stretches() {
    use pencil_core::frame::{Inputs, view_for};
    let doc = "viewport { x: -2..2, y: -1..1 }\ncurve c = circle(0, 1)";
    let figure = pencil_core::build(doc, &[]).unwrap();
    for (w, h) in [
        (400u32, 200u32),
        (400, 400),
        (800, 200),
        (300, 500),
        (1000, 501),
    ] {
        let view = view_for(&figure, &Inputs::at(0.0), w, h).unwrap();
        let (sx, sy) = (
            w as f64 / (view.x[1] - view.x[0]),
            h as f64 / (view.y[1] - view.y[0]),
        );
        assert!((sx / sy - 1.0).abs() < 1e-12, "{w}×{h}: {sx} and {sy}");
        // All of what was asked for is still there, and it's still centered.
        assert!(
            view.x[0] <= -2.0 && view.x[1] >= 2.0 && view.y[0] <= -1.0 && view.y[1] >= 1.0,
            "{view:?}"
        );
        assert!((view.x[0] + view.x[1]).abs() < 1e-12 && (view.y[0] + view.y[1]).abs() < 1e-12);
    }
    // Exactly the right shape, it's exactly what was written.
    let view = view_for(&figure, &Inputs::at(0.0), 400, 200).unwrap();
    assert_eq!((view.x, view.y), ([-2.0, 2.0], [-1.0, 1.0]));
}

/// The spiral itself runs through the pole when −d/c lands on it, where it crosses the negative real
/// axis. With the pole exactly there, and either side of it, what's drawn is still the curve.
#[test]
fn the_mapped_spiral_draws_no_false_chord_through_the_pole() {
    use pencil_core::{
        eval::{frame_env, real},
        frame::{Inputs, view_for},
        ir::Form,
        stroke::sample_arc,
    };
    use std::collections::HashSet;
    let composition = mobius();
    let figure = &composition.panels[0].figure;
    let arc = figure
        .scene
        .marks
        .iter()
        .find_map(|m| match &m.form {
            Form::Parametric(arc) if m.id == "loxodrome" => Some(arc),
            _ => None,
        })
        .unwrap();
    // w^t is on the negative real axis where t·arg w is an odd multiple of π.
    let arg = COXETER_W.1.atan2(COXETER_W.0);
    let t0 = std::f64::consts::PI / arg;
    let pole = -z_abs(COXETER_W).powf(t0);
    const CELL: f64 = 0.1;
    let cell = |x: f64, y: f64| ((x / CELL).floor() as i64, (y / CELL).floor() as i64);
    for off in [0.0, -1e-9, 1e-7, 0.01] {
        let params = vec![
            ("a".to_string(), 1.0),
            ("b".to_string(), 1.0),
            ("c".to_string(), 1.0),
            ("d".to_string(), -(pole + off)),
        ];
        let inputs = Inputs { time: 3.5, params };
        let env = frame_env(figure, &inputs).unwrap();
        let view = view_for(figure, &inputs, 720, 1180).unwrap();
        let mut truth = HashSet::new();
        let mut dense = env.clone();
        for k in 0..=40_000 {
            dense.set(&arc.var, -8.0 + 16.0 * k as f64 / 40_000.0);
            if let (Ok(x), Ok(y)) = (real(&arc.point[0], &dense), real(&arc.point[1], &dense))
                && x.is_finite()
                && y.is_finite()
            {
                truth.insert(cell(x, y));
            }
        }
        let near = |x: f64, y: f64| {
            let (cx, cy) = cell(x, y);
            (-2..=2).any(|a| (-2..=2).any(|b| truth.contains(&(cx + a, cy + b))))
        };
        let sampled = sample_arc(arc, &env, &view, 720, 1180).unwrap();
        for piece in &sampled.pieces {
            for pair in piece.windows(2) {
                let [p, q] = [pair[0], pair[1]];
                let steps = ((q[0] - p[0]).hypot(q[1] - p[1]) / 0.03)
                    .ceil()
                    .clamp(1.0, 2e5) as usize;
                for i in 0..=steps {
                    let u = i as f64 / steps as f64;
                    let (x, y) = (p[0] + (q[0] - p[0]) * u, p[1] + (q[1] - p[1]) * u);
                    let inside = x > view.x[0] && x < view.x[1] && y > view.y[0] && y < view.y[1];
                    assert!(
                        !inside || near(x, y),
                        "off {off}: a chord crosses the frame at ({x:.2}, {y:.2}), from {p:?} to {q:?}"
                    );
                }
            }
        }
    }
}

/// The panels ask the same question, so a q or p change only answers it once.
#[test]
fn panels_with_the_same_systems_share_their_answers() {
    let composition = lattice();
    let first = &composition.panels[0].figure.solved;
    for panel in &composition.panels {
        assert!(
            std::sync::Arc::ptr_eq(first, &panel.figure.solved),
            "{}",
            panel.id
        );
    }
    // A panel with a system of its own keeps its own answers.
    let own = pencil_core::compose(
        "panel a { viewport { x: 0..1, y: 0..1 }
           solve (u) from 1: u² = 4
         }
         panel b { viewport { x: 0..1, y: 0..1 }
           solve (u) from 1: u² = 9
         }",
        &[],
    )
    .unwrap();
    assert!(!std::sync::Arc::ptr_eq(
        &own.panels[0].figure.solved,
        &own.panels[1].figure.solved
    ));
}

/// The way the post writes it: T(w) = 2πi·w / A, and i turns a thing 90°.
#[test]
fn the_figures_are_written_in_complex_numbers() {
    let lattice = square_read("007-lattice/lattice.pen");
    assert!(lattice.contains("fn to_turn(w) = tau·i·w / A"));
    assert!(lattice.contains("fn grid(a, b) = a + b·omega"));
    for gone in ["times_i", "fn over", "fn polar"] {
        assert!(!lattice.contains(gone), "`{gone}` is back");
    }
    let square = square_read("006-square/square.pen");
    assert!(square.contains("let BIZ = i·BZ"));
    assert!(square.contains("let Z = a + b·i"));
}

/// Where a panel's map sends a point: its dot.
fn dot_at(figure: &pencil_core::Figure, env: &pencil_core::eval::Env, id: &str) -> [f64; 2] {
    use pencil_core::{eval::real, ir::Form};
    let at = figure
        .scene
        .marks
        .iter()
        .find_map(|m| match &m.form {
            Form::Dot(at) if m.id == id => Some(at),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no dot `{id}`"));
    [real(&at[0], env).unwrap(), real(&at[1], env).unwrap()]
}

/// The period closes it: A lands on 2πi in log space, and on 1 in the spiral, where 0 lands too.
#[test]
fn the_period_closes_the_triangle() {
    use pencil_core::{eval::frame_env, frame::Inputs};
    let composition = lattice();
    let log = &composition
        .panels
        .iter()
        .find(|p| p.id == "log")
        .unwrap()
        .figure;
    let env = frame_env(log, &Inputs::at(0.0)).unwrap();
    let end = dot_at(log, &env, "end_point");
    assert!(end[0].abs() < 1e-12);
    assert!((end[1] - std::f64::consts::TAU).abs() < 1e-12);
    let whole = &composition
        .panels
        .iter()
        .find(|p| p.id == "whole")
        .unwrap()
        .figure;
    let env = frame_env(whole, &Inputs::at(0.0)).unwrap();
    let (at_a, _) = spiral_circle(whole, &env, -2.0, 5.0);
    let (at_zero, _) = spiral_circle(whole, &env, 0.0, 0.0);
    assert!(
        (at_a[0] - 1.0).abs() < 1e-12 && at_a[1].abs() < 1e-12,
        "{at_a:?}"
    );
    assert_eq!(at_zero, [1.0, 0.0]);
}

/// A thing is declared once, in the lattice's own coordinates. Panels only choose a map.
#[test]
fn each_thing_in_the_lattice_is_declared_once() {
    let doc = square_read("007-lattice/lattice.pen");
    for declaration in [
        "curve minus_one =",
        "curve along_omega =",
        "curve up_to_a =",
        "curve down_to_a =",
        "dots steps_one =",
        "dots grid_points =",
        "circles spiral_circles =",
        "segment period_line =",
        "label count_one at",
        "label name_zero at",
        "label name_end at",
        "dot zero, end_point",
    ] {
        assert_eq!(
            doc.matches(declaration).count(),
            1,
            "`{declaration}` should be written once"
        );
    }
    // Each panel draws it through a map, so each is the map's image of the same curve.
    use pencil_core::{eval::frame_env, frame::Inputs, ir::Form};
    let composition = lattice();
    let point = |panel: usize, id: &str, t: f64| {
        let figure = &composition.panels[panel].figure;
        let mut env = frame_env(figure, &Inputs::at(0.0)).unwrap();
        let arc = figure
            .scene
            .marks
            .iter()
            .find_map(|m| match &m.form {
                Form::Parametric(arc) if m.id == id => Some(arc),
                _ => None,
            })
            .unwrap();
        env.set(&arc.var, t);
        [
            pencil_core::eval::real(&arc.point[0], &env).unwrap(),
            pencil_core::eval::real(&arc.point[1], &env).unwrap(),
        ]
    };
    // T(w) = 2πi·w / A, worked out here, for q = 7, p = 2, where A = −2 + 5ω.
    let omega = [-0.5, 3f64.sqrt() / 2.0];
    let a = [-2.0 + 5.0 * omega[0], 5.0 * omega[1]];
    let tau = std::f64::consts::TAU;
    let t_of = |w: [f64; 2]| {
        let norm = a[0] * a[0] + a[1] * a[1];
        let over = [
            (w[0] * a[0] + w[1] * a[1]) / norm,
            (w[1] * a[0] - w[0] * a[1]) / norm,
        ];
        [-over[1] * tau, over[0] * tau]
    };
    for t in [0.0, 1.5, 3.0, 7.0] {
        let flat = point(0, "minus_one", t);
        assert_eq!(flat, [-t, 0.0]);
        let log = point(1, "minus_one", t);
        let want = t_of(flat);
        assert!(
            (log[0] - want[0]).abs() < 1e-12 && (log[1] - want[1]).abs() < 1e-12,
            "{log:?} {want:?}"
        );
        // The whole and the close-up look at one image, and differ only in their views.
        assert_eq!(point(2, "minus_one", t), point(3, "minus_one", t));
    }
}

/// A count sits out of the triangle in every panel, because its direction is worked out once in
/// the lattice and carried through each map, which keeps sides.
#[test]
fn a_count_stays_on_the_same_side_of_its_edge_through_every_map() {
    use pencil_core::{eval::frame_env, eval::real, frame::Inputs, ir::Form};
    let composition = lattice();
    let edges = [
        ("count_one", "minus_one", 0.5),
        ("count_omega", "along_omega", 0.5),
        ("count_up", "up_to_a", 0.5),
        ("count_down", "down_to_a", 0.5),
    ];
    for (q, p) in [(7.0, 2.0), (12.0, 7.0), (5.0, 3.0), (9.0, 1.0)] {
        let inputs = Inputs {
            time: 0.0,
            params: vec![("q".to_string(), q), ("p".to_string(), p)],
        };
        for (label, edge, _) in edges {
            let mut sides = Vec::new();
            for panel in &composition.panels {
                let figure = &panel.figure;
                let mut env = frame_env(figure, &inputs).unwrap();
                let marks = &figure.scene.marks;
                let Some(Form::Label { away, .. }) =
                    marks.iter().find(|m| m.id == label).map(|m| &m.form)
                else {
                    panic!("no {label} in {}", panel.id);
                };
                let away = away.as_ref().unwrap();
                let direction = [real(&away[0], &env).unwrap(), real(&away[1], &env).unwrap()];
                let arc = marks
                    .iter()
                    .find_map(|m| match &m.form {
                        Form::Parametric(arc) if m.id == edge => Some(arc),
                        _ => None,
                    })
                    .unwrap();
                let end = real(&arc.range[1], &env).unwrap();
                let mut at = |t: f64| {
                    env.set(&arc.var, t);
                    [
                        real(&arc.point[0], &env).unwrap(),
                        real(&arc.point[1], &env).unwrap(),
                    ]
                };
                let (before, after) = (at(end / 2.0 - 1e-5), at(end / 2.0 + 1e-5));
                let tangent = [after[0] - before[0], after[1] - before[1]];
                let cross = tangent[0] * direction[1] - tangent[1] * direction[0];
                sides.push((panel.id.clone(), cross.signum()));
            }
            assert!(
                sides.iter().all(|(_, s)| *s == sides[0].1),
                "q={q}, p={p}, {label}: {sides:?}"
            );
        }
    }
}

#[test]
fn panels_lay_out_in_a_grid_and_keep_their_own_views() {
    use pencil_core::frame::{Inputs, view_for};
    let composition = lattice();
    assert_eq!(composition.panels.len(), 4);
    assert_eq!(composition.size, [814.0, 826.0]);
    // Second column, second row.
    let rect = composition.rect_px(3, 814, 826);
    assert_eq!(rect, [414, 446, 400, 380]);
    let width = |id: &str| {
        let panel = composition.panels.iter().find(|p| p.id == id).unwrap();
        let view = view_for(&panel.figure, &Inputs::at(0.0), 400, 380).unwrap();
        view.x[1] - view.x[0]
    };
    // The close-up is much tighter than the whole spiral, which is why it's there.
    assert!(
        width("detail") < width("whole") / 4.0,
        "{} {}",
        width("detail"),
        width("whole")
    );
    assert_ne!(width("flat"), width("log"));
}

/// A rule that names a mark only has to find it in one panel, but it has to find it somewhere.
#[test]
fn a_style_for_one_panel_is_not_a_mistake_but_a_typo_is() {
    let doc = square_read("007-lattice/lattice.pen");
    let plain = square_read("007-lattice/plain.style");
    let theme = square_read("theme.style");
    assert!(pencil_core::compose(&doc, &[&theme, &plain]).is_ok());
    let err =
        pencil_core::compose(&doc, &[&theme, "style nothing_here { stroke: black }"]).unwrap_err();
    assert!(
        err.message.contains("no mark named `nothing_here`"),
        "{err}"
    );
}

/// The count follows a param, one layer per collection, whatever it comes to.
#[test]
fn the_lattice_count_follows_q() {
    use pencil_core::frame::Inputs;
    use pencil_core::stroke::FLOATS_PER_SEGMENT;
    let composition = lattice();
    let segments = |q: f64| {
        let inputs = Inputs {
            time: 0.0,
            params: vec![("q".to_string(), q)],
        };
        let frames = composition
            .prepare(&inputs, 814, 826, 1.0, &pencil_core::terms::Estimate)
            .unwrap();
        frames[0].prepared.strokes[0].len() / FLOATS_PER_SEGMENT
    };
    assert!(
        segments(12.0) > segments(4.0),
        "{} {}",
        segments(12.0),
        segments(4.0)
    );
}

/// Kris's catch: exp sends 0 to 1, so the spiral's panels have to say so. The flat ones sign
/// their step counts, and the spiral's leave that to the arrowheads.
#[test]
fn each_panel_names_things_for_its_own_space() {
    use pencil_core::frame::Inputs;
    let composition = lattice();
    let frames = composition
        .prepare(
            &Inputs::at(0.0),
            814,
            826,
            1.0,
            &pencil_core::terms::Estimate,
        )
        .unwrap();
    let texts = |i: usize| -> Vec<String> {
        frames[i]
            .prepared
            .labels
            .iter()
            .map(|l| l.text.clone())
            .collect()
    };
    let has = |i: usize, text: &str| texts(i).iter().any(|t| t == text);
    assert!(
        has(0, "0") && has(0, "A") && has(0, "−7") && has(0, "+5") && has(0, "+7") && has(0, "−2")
    );
    assert!(has(1, "0") && has(1, "2πi") && !has(1, "A"));
    // The whole spiral labels the long arcs, and the close-up the short ones.
    assert!(has(2, "1") && !has(2, "0"));
    assert_eq!(texts(2).iter().filter(|t| *t == "7").count(), 1);
    assert!(has(2, "2") && !has(2, "5"));
    assert!(has(3, "1") && !has(3, "0") && has(3, "5") && !has(3, "2"));
    assert!(has(3, "7"));
}

/// A thing is the same thing in every panel: one declaration, wherever its map sends it. Only what
/// it's called changes with the space. So 0 and A are two points until exp makes them one, at 1.
#[test]
fn the_same_points_are_in_every_panel() {
    use pencil_core::{eval::frame_env, frame::Inputs};
    let composition = lattice();
    let point = |panel: &str, name: &str, q: f64, p: f64| {
        let figure = &composition
            .panels
            .iter()
            .find(|x| x.id == panel)
            .unwrap()
            .figure;
        let inputs = Inputs {
            time: 0.0,
            params: vec![("q".to_string(), q), ("p".to_string(), p)],
        };
        let env = frame_env(figure, &inputs).unwrap();
        dot_at(figure, &env, name)
    };
    for (q, p) in [(7.0, 2.0), (12.0, 7.0), (3.0, 0.0), (5.0, 5.0)] {
        // Apart in the flat views.
        assert_eq!(point("flat", "zero", q, p), [0.0, 0.0]);
        assert_eq!(point("log", "zero", q, p), [0.0, 0.0]);
        let end = point("log", "end_point", q, p);
        assert!(
            end[0].abs() < 1e-12 && (end[1] - std::f64::consts::TAU).abs() < 1e-12,
            "{end:?}"
        );
        // One point, 1, in both of the spiral's.
        for panel in ["whole", "detail"] {
            for name in ["zero", "end_point"] {
                let at = point(panel, name, q, p);
                assert!(
                    (at[0] - 1.0).abs() < 1e-12 && at[1].abs() < 1e-12,
                    "{panel} {name}: {at:?}"
                );
            }
        }
    }
    // The label on that one point keeps its identity across panels, with its space's name.
    let frames = composition
        .prepare(
            &Inputs::at(0.0),
            814,
            826,
            1.0,
            &pencil_core::terms::Estimate,
        )
        .unwrap();
    let names: Vec<(&str, &str)> = composition
        .panels
        .iter()
        .zip(&frames)
        .map(|(panel, frame)| {
            let label = frame
                .prepared
                .labels
                .iter()
                .find(|l| l.id == "name_zero")
                .unwrap();
            (panel.id.as_str(), label.text.as_str())
        })
        .collect();
    assert_eq!(
        names,
        [("flat", "0"), ("log", "0"), ("whole", "1"), ("detail", "1")]
    );
    // The spiral has no second name for the same point, so there's nothing to collide with it.
    for panel in composition
        .panels
        .iter()
        .filter(|p| p.id == "whole" || p.id == "detail")
    {
        assert!(!panel.figure.scene.marks.iter().any(|m| m.id == "name_end"));
    }
}

/// With nothing to step along, an edge has no count and no head, as at p = 0.
#[test]
fn an_edge_with_no_steps_draws_nothing() {
    use pencil_core::frame::Inputs;
    let composition = lattice();
    let inputs = Inputs {
        time: 0.0,
        params: vec![("p".to_string(), 0.0)],
    };
    let frames = composition
        .prepare(&inputs, 814, 826, 1.0, &pencil_core::terms::Estimate)
        .unwrap();
    assert!(
        !frames[0]
            .prepared
            .labels
            .iter()
            .any(|l| l.id == "count_down")
    );
}

/// The wrapped copies are the edges a turn up and a turn down: the same crossings as the spiral.
#[test]
fn log_space_wraps_by_one_turn() {
    use pencil_core::{eval::frame_env, frame::Inputs, ir::Form};
    let composition = lattice();
    let log = &composition.panels[1].figure;
    let ids: Vec<&str> = log.scene.marks.iter().map(|m| m.id.as_str()).collect();
    for id in [
        "up_minus_one",
        "down_minus_one",
        "minus_one",
        "up_steps_up",
        "down_steps_down",
    ] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    let curve = |id: &str| {
        log.scene
            .marks
            .iter()
            .find_map(|m| match &m.form {
                Form::Parametric(arc) if m.id == id => Some(arc.clone()),
                _ => None,
            })
            .unwrap()
    };
    let mut env = frame_env(log, &Inputs::at(0.0)).unwrap();
    env.set("t", 3.0);
    let y = |id: &str| pencil_core::eval::real(&curve(id).point[1], &env).unwrap();
    let tau = std::f64::consts::TAU;
    assert!((y("up_minus_one") - y("minus_one") - tau).abs() < 1e-12);
    assert!((y("down_minus_one") - y("minus_one") + tau).abs() < 1e-12);
}

/// The fast evaluator has to agree with the real one to the bit, on every expression in the
/// corpus, at several states, including ones where things don't exist.
#[test]
fn the_fast_evaluator_agrees_with_the_real_one() {
    use pencil_core::eval::frame_env;
    use pencil_core::fast::agrees;
    use pencil_core::frame::Inputs;
    use pencil_core::ir::{Form, Part, Shape};
    let docs = [
        ("001-circle/circle.pen", ""),
        ("003-unroll/unroll.pen", ""),
        ("004-bounds/bounds.pen", ""),
        ("005-tangents/tangents.pen", "d"),
        ("006-square/square.pen", "step"),
        ("007-lattice/lattice.pen", "q"),
        ("008-hearts/hearts.pen", "shift"),
        ("009-squaring/squaring.pen", "n"),
        ("010-mobius/mobius.pen", "d"),
    ];
    let mut checked = 0;
    for (doc, param) in docs {
        let composition = pencil_core::compose(&square_read(doc), &[]).unwrap();
        for panel in &composition.panels {
            let figure = &panel.figure;
            for value in [0.5, 1.0, 3.0, 7.0, 12.0] {
                let inputs = Inputs {
                    time: value,
                    params: if param.is_empty() {
                        Vec::new()
                    } else {
                        vec![(param.to_string(), value)]
                    },
                };
                let mut env = frame_env(figure, &inputs).unwrap();
                let mut check = |e: &pencil_core::ir::Expr, env: &pencil_core::eval::Env| {
                    agrees(e, env).unwrap();
                    checked += 1;
                };
                for binding in &figure.scene.bindings {
                    check(&binding.expr, &env);
                }
                for system in &figure.scene.systems {
                    for e in system.equations.iter().chain(&system.jacobian) {
                        check(e, &env);
                    }
                }
                for mark in &figure.scene.marks {
                    match &mark.form {
                        Form::Parametric(arc) => {
                            for t in [0.0, 0.25, 1.0, 2.5] {
                                env.set(&arc.var, t);
                                check(&arc.point[0], &env);
                                check(&arc.point[1], &env);
                            }
                        }
                        Form::Region(parts) => {
                            for part in parts {
                                if let Part::Arc(arc) = part {
                                    env.set(&arc.var, 0.5);
                                    check(&arc.point[0], &env);
                                    check(&arc.point[1], &env);
                                }
                            }
                        }
                        Form::Many { vars, lets, shape } => {
                            for (i, j) in [(-2.0, 1.0), (0.0, 0.0), (3.0, 4.0)] {
                                for (k, (name, _)) in vars.iter().enumerate() {
                                    env.set(name, if k == 0 { i } else { j });
                                }
                                for (name, e) in lets {
                                    check(e, &env);
                                    if let Ok(v) = pencil_core::eval::real(e, &env) {
                                        env.set(name, v);
                                    }
                                }
                                match shape {
                                    Shape::Dots(at) => {
                                        check(&at[0], &env);
                                        check(&at[1], &env);
                                    }
                                    Shape::Circles { center, r } => {
                                        check(&center[0], &env);
                                        check(&center[1], &env);
                                        check(r, &env);
                                    }
                                    Shape::Curves(arc) => {
                                        let lo = pencil_core::eval::real(&arc.range[0], &env);
                                        if let Ok(lo) = lo {
                                            env.set(&arc.var, lo + 0.25);
                                            check(&arc.point[0], &env);
                                            check(&arc.point[1], &env);
                                        }
                                    }
                                    Shape::Cycles { a, b, c } => {
                                        check(a, &env);
                                        check(&b[0], &env);
                                        check(&b[1], &env);
                                        check(c, &env);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    assert!(checked > 1000, "{checked}");
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
