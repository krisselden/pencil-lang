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
    let (center, r) = instance(lets, shape, &mut local).unwrap().unwrap();
    (center, r.unwrap())
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
    assert!(env.get("end_point.x").unwrap().abs() < 1e-12);
    assert!((env.get("end_point.y").unwrap() - std::f64::consts::TAU).abs() < 1e-12);
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

/// A thing is the same thing in every panel: same name, wherever its map sends it. Only what it's
/// called changes with the space. So 0 and A are two points until exp makes them one, at 1.
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
        (
            env.get(&format!("{name}.x")).unwrap(),
            env.get(&format!("{name}.y")).unwrap(),
        )
    };
    for (q, p) in [(7.0, 2.0), (12.0, 7.0), (3.0, 0.0), (5.0, 5.0)] {
        // Apart in the flat views.
        assert_eq!(point("flat", "start", q, p), (0.0, 0.0));
        assert_eq!(point("log", "start", q, p), (0.0, 0.0));
        let end = point("log", "end_point", q, p);
        assert!(
            end.0.abs() < 1e-12 && (end.1 - std::f64::consts::TAU).abs() < 1e-12,
            "{end:?}"
        );
        // One point, 1, in both of the spiral's.
        for panel in ["whole", "detail"] {
            for name in ["start", "end_point"] {
                let at = point(panel, name, q, p);
                assert!(
                    (at.0 - 1.0).abs() < 1e-12 && at.1.abs() < 1e-12,
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
                .find(|l| l.id == "name_start")
                .unwrap();
            (panel.id.as_str(), label.text.as_str())
        })
        .collect();
    assert_eq!(
        names,
        [("flat", "0"), ("log", "0"), ("whole", "1"), ("detail", "1")]
    );
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
