//! A circle through three points, which has a handedness and which can be a line.

use pencil_core::build;
use pencil_core::eval::{self, Frame};
use pencil_core::frame::Inputs;
use pencil_core::ir::{Expr, Form, Mark};

fn doc(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> String {
    format!(
        "viewport {{ x: -8..8, y: -8..8 }}
         let A = ({}, {})
         let B = ({}, {})
         let C = ({}, {})
         curve c = circle(A, B, C)",
        a[0], a[1], b[0], b[1], c[0], c[1]
    )
}

struct Read {
    env: pencil_core::eval::Env,
    mark: Mark,
}

impl Read {
    fn of(doc: &str) -> Read {
        let figure = build(doc, &[]).unwrap();
        let env = eval::frame_env(&figure, &Inputs::at(0.0)).unwrap();
        let mark = figure
            .scene
            .marks
            .iter()
            .find(|m| m.id == "c")
            .unwrap()
            .clone();
        Read { env, mark }
    }
    fn get(&self, name: &str) -> f64 {
        eval::real(&Expr::sym(format!("c.{name}")), &self.env).unwrap()
    }
    fn exists(&self, name: &str) -> bool {
        eval::real(&Expr::sym(format!("c.{name}")), &self.env).is_ok_and(|v| v.is_finite())
    }
    /// The signed distance the field reports at a point, and its gradient.
    fn at(&self, p: [f64; 2]) -> (f64, [f64; 2]) {
        let Form::Field { field, grad, .. } = &self.mark.form else {
            panic!("not a field");
        };
        let mut env = self.env.clone();
        env.set("x", p[0]);
        env.set("y", p[1]);
        (
            eval::real(field, &env).unwrap(),
            [
                eval::real(&grad[0], &env).unwrap(),
                eval::real(&grad[1], &env).unwrap(),
            ],
        )
    }
}

/// The ordinary center and radius, for checking against.
fn circumcircle(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> ([f64; 2], f64) {
    let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
    let (vx, vy) = (c[0] - a[0], c[1] - a[1]);
    let (uu, vv) = (ux * ux + uy * uy, vx * vx + vy * vy);
    let d = 2.0 * (ux * vy - uy * vx);
    let (wx, wy) = ((vy * uu - uy * vv) / d, (ux * vv - vx * uu) / d);
    ([a[0] + wx, a[1] + wy], wx.hypot(wy))
}

const TRIPLES: [[[f64; 2]; 3]; 4] = [
    [[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0]],
    [[3.5, -1.0], [4.0, 2.0], [-2.0, 2.5]],
    [[-6.0, -6.0], [6.0, -5.0], [0.0, 7.0]],
    [[0.25, 0.5], [0.5, 0.75], [0.125, 1.0]],
];

#[test]
fn it_passes_through_its_points_at_a_true_distance() {
    for [a, b, c] in TRIPLES {
        let read = Read::of(&doc(a, b, c));
        for p in [a, b, c] {
            assert!(read.at(p).0.abs() < 1e-12, "{:?}", read.at(p).0);
        }
        let (center, r) = circumcircle(a, b, c);
        for p in [
            [7.0, 3.0],
            [-2.0, 0.5],
            [center[0] + 0.01, center[1]],
            [0.3, -4.0],
        ] {
            let want = (p[0] - center[0]).hypot(p[1] - center[1]) - r;
            let (got, grad) = read.at(p);
            assert!((got - want).abs() < 1e-12, "{got} against {want}");
            assert!((grad[0].hypot(grad[1]) - 1.0).abs() < 1e-9);
        }
        let (bx, by, ca, cc) = (read.get("bx"), read.get("by"), read.get("a"), read.get("c"));
        assert!((bx * bx + by * by - ca * cc - 1.0).abs() < 1e-12);
    }
}

#[test]
fn clockwise_points_turn_it_inside_out() {
    for [a, b, c] in TRIPLES {
        let forward = Read::of(&doc(a, b, c));
        let backward = Read::of(&doc(c, b, a));
        assert!(forward.get("a") > 0.0, "these all run counterclockwise");
        assert!((forward.get("a") + backward.get("a")).abs() < 1e-12);
        let p = [2.0, -3.0];
        assert!((forward.at(p).0 + backward.at(p).0).abs() < 1e-12);
        // A center and a radius don't know which way round it runs.
        assert!((forward.get("r") - backward.get("r")).abs() < 1e-12);
        assert!((forward.get("x") - backward.get("x")).abs() < 1e-12);
        assert!((forward.get("y") - backward.get("y")).abs() < 1e-12);
    }
}

#[test]
fn flip_turns_it_inside_out() {
    let [a, b, c] = TRIPLES[1];
    let read = Read::of(&format!("{}\ncurve d = flip(c)", doc(a, b, c)));
    let env = &read.env;
    let name = |s: &str| eval::real(&Expr::sym(s), env).unwrap();
    assert!((name("c.a") + name("d.a")).abs() < 1e-15);
    assert!((name("c.bx") + name("d.bx")).abs() < 1e-15);
    assert!((name("c.by") + name("d.by")).abs() < 1e-15);
    assert!((name("c.c") + name("d.c")).abs() < 1e-15);

    // And a circle from a center and a radius: its disk becomes the positive side.
    let read = Read::of(
        "viewport { x: -8..8, y: -8..8 }
         let O = (1, 2)
         let d = circle(O, (1, 5))
         curve c = flip(d)",
    );
    assert!((read.get("a") + 1.0 / 3.0).abs() < 1e-15);
    assert!((read.at([1.0, 2.0]).0 - 3.0).abs() < 1e-12);
    assert!((read.at([1.0, 7.0]).0 + 2.0).abs() < 1e-12);
}

#[test]
fn it_is_continuous_as_it_passes_through_a_line() {
    // C crosses the line through A and B, so the circle grows, straightens and comes back from
    // the other side. What a fixed point measures shouldn't notice.
    let (a, b) = ([-1.0, 0.0], [1.0, 0.0]);
    let p = [0.5, 2.0];
    let mut last = None;
    for step in -40..=40 {
        let t = step as f64 * 1e-3;
        let read = Read::of(&doc(a, b, [5.0, t]));
        let (d, _) = read.at(p);
        if let Some(last) = last {
            let jump: f64 = d - last;
            assert!(jump.abs() < 0.02, "jumped by {jump} at {t}");
        }
        last = Some(d);
    }
}

#[test]
fn three_points_in_a_line_are_a_line() {
    let read = Read::of(&doc([-3.0, 1.0], [0.0, 1.0], [5.0, 1.0]));
    assert_eq!(read.get("a"), 0.0);
    // Going left to right has its negative side on the left of travel, as a counterclockwise circle does.
    assert!((read.at([2.0, 3.0]).0 + 2.0).abs() < 1e-12);
    assert!((read.at([-7.0, -2.0]).0 - 3.0).abs() < 1e-12);
    // It has no center and no radius, rather than infinite ones.
    assert!(!read.exists("x"));
    assert!(!read.exists("y"));
    assert!(!read.exists("r"));
}

#[test]
fn it_is_drawn_when_it_is_a_line() {
    let figure = build(
        &doc([-3.0, 1.0], [0.0, 1.0], [5.0, 1.0]),
        &["style paper { fill: white } style c { stroke: black; width: 2px }"],
    )
    .unwrap();
    let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
    // y = 1 in a view of ±8 is at row 70.
    assert_eq!(frame.shade(20.5, 70.0).unwrap(), [0.0, 0.0, 0.0]);
    assert_eq!(frame.shade(20.5, 60.5).unwrap(), [1.0, 1.0, 1.0]);
}

#[test]
fn it_is_absent_when_its_points_meet() {
    let read = Read::of(&doc([1.0, 2.0], [1.0, 2.0], [5.0, 1.0]));
    assert!(!read.exists("a"));
    let figure = build(
        &doc([1.0, 2.0], [1.0, 2.0], [5.0, 1.0]),
        &[
            "style paper { fill: white } style c { stroke: black; width: 2px }
           style c::inside { fill: black }",
        ],
    )
    .unwrap();
    let frame = Frame::new(&figure, &Inputs::at(0.0), 40, 40).unwrap();
    assert_eq!(frame.shade(20.5, 20.5).unwrap(), [1.0, 1.0, 1.0]);
}

#[test]
fn inside_is_the_disk_and_negative_is_the_sign() {
    let sheet =
        |facet: &str| format!("style paper {{ fill: white }} style c::{facet} {{ fill: black }}");
    let [a, b, c] = TRIPLES[0];
    let center = (80.5, 80.5); // The unit circle's middle in a view of ±8.
    let far = (10.5, 10.5);
    let dark = |doc: &str, facet: &str, at: (f64, f64)| {
        let figure = build(doc, &[&sheet(facet)]).unwrap();
        let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
        frame.shade(at.0, at.1).unwrap()[0] == 0.0
    };
    let (forward, backward) = (doc(a, b, c), doc(c, b, a));
    for (doc, facet, disk) in [
        (&forward, "inside", true),
        (&forward, "outside", false),
        (&forward, "negative", true),
        (&forward, "positive", false),
        (&backward, "inside", true),
        (&backward, "outside", false),
        (&backward, "negative", false),
        (&backward, "positive", true),
    ] {
        assert_eq!(dark(doc, facet, center), disk, "{facet} at the center");
        assert_eq!(dark(doc, facet, far), !disk, "{facet} far away");
    }
}

#[test]
fn the_named_side_wins_over_the_sign_it_shares() {
    let [a, b, c] = TRIPLES[0];
    let sheet = "style paper { fill: white }
                 style c::inside { fill: black }
                 style c::positive { fill: rgb(255, 0, 0) }
                 style c::negative { fill: rgb(0, 0, 255) }";
    let color = |doc: &str, at: (f64, f64)| {
        let figure = build(doc, &[sheet]).unwrap();
        let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
        frame.shade(at.0, at.1).unwrap()
    };
    let (center, far) = ((80.5, 80.5), (10.5, 10.5));
    let black = [0.0, 0.0, 0.0];
    // Counterclockwise: the disk is black, and the rest is positive.
    assert_eq!(color(&doc(a, b, c), center), black);
    let forward_rest = color(&doc(a, b, c), far);
    // Clockwise: the disk is still black, and the rest is negative.
    assert_eq!(color(&doc(c, b, a), center), black);
    let backward_rest = color(&doc(c, b, a), far);
    assert_ne!(forward_rest, backward_rest);
    assert_ne!(forward_rest, black);
    assert_ne!(backward_rest, black);
}

#[test]
fn a_style_can_read_its_curvature() {
    let [a, b, c] = TRIPLES[0];
    // Rim only, when it's been turned inside out: the disk fills only while it's positive.
    let sheet = "style paper { fill: white }
                 style c::inside { fill: black; }
                 style c { fill_opacity: max(sign(self.curvature), 0) }";
    let dark = |doc: &str| {
        let figure = build(doc, &[sheet]).unwrap();
        let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
        frame.shade(80.5, 80.5).unwrap()[0] == 0.0
    };
    assert!(dark(&doc(a, b, c)));
    assert!(!dark(&doc(c, b, a)));
    let err = build(
        "viewport { x: -1..1, y: -1..1 }
         curve c = sdf (x, y): x",
        &["style c { fill_opacity: self.curvature }"],
    )
    .unwrap_err();
    assert!(err.to_string().contains("isn't a circle"), "{err}");
}

/// The plain center of a circle has no gradient, and it isn't a problem.
#[test]
fn its_center_is_as_far_from_the_rim_as_anywhere() {
    let [a, b, c] = TRIPLES[0];
    let read = Read::of(&doc(a, b, c));
    assert!((read.at([0.0, 0.0]).0 + 1.0).abs() < 1e-9);
    let sheet = "style paper { fill: black }
                 style c::inside { fill: rgb(self.dist * -255, 0, 0) }";
    let figure = build(&doc(a, b, c), &[sheet]).unwrap();
    let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
    let there = frame.shade(80.0, 80.0).unwrap();
    assert!(there[0] > 0.9, "{there:?}");
}

/// A nearly straight circle in single precision, as the shader sees it. Its center and radius
/// have lost the digits this needs, and its coefficients haven't.
#[test]
fn a_nearly_straight_circle_keeps_its_place_in_single_precision() {
    let r = 1468.0_f64;
    let center = [1468.0_f64 + 1.0, 0.0];
    let read = Read::of(
        "viewport { x: -8..8, y: -8..8 }
         let O = (1469, 0)
         let d = circle(O, (1, 0))
         curve c = flip(d)",
    );
    let (a, bx, by, cc) = (
        read.get("a") as f32,
        read.get("bx") as f32,
        read.get("by") as f32,
        read.get("c") as f32,
    );
    for p in [[1.25_f32, 0.5], [-3.0, 4.0], [0.5, -7.5]] {
        let f = a * (p[0] * p[0] + p[1] * p[1]) - 2.0 * (bx * p[0] + by * p[1]) + cc;
        let got = f / (1.0 + (1.0 + a * f).max(0.0).sqrt());
        let exact = (p[0] as f64 - center[0]).hypot(p[1] as f64 - center[1]) - r;
        assert!(
            (got as f64 + exact).abs() < 1e-3,
            "{got} against {}",
            -exact
        );
    }
}

/// The limaçon's shader, by hand. `col += damp * glow * tanh(zoom * pixel / abs(d))`, then `tanh(col)`.
#[test]
fn the_limacon_is_what_its_shader_adds_up() {
    let read = |name: &str| {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/012-limacon/");
        std::fs::read_to_string(format!("{path}{name}")).unwrap()
    };
    let figure = build(&read("limacon.pen"), &[&read("plain.style")]).unwrap();
    let size = 64u32;
    let (a, tau) = (8.0_f64, std::f64::consts::TAU);
    let x0 = (a + 1.0 / a) / 2.0;
    let x1 = (x0 + 1.0) / 2.0;
    let r1 = (a + 1.0) / 2.0 - x1;
    let zoom = 6.0;
    let pixel = 1.0 / size as f64;
    let mut compared = 0;
    for time in [0.0, 1.0, 2.5] {
        let frame = Frame::new(&figure, &Inputs::at(time), size, size).unwrap();
        for py in 0..size {
            for px in 0..size {
                let (fx, fy) = (px as f64 + 0.5, (size - py) as f64 - 0.5);
                let x = zoom * pixel * (2.0 * fx - size as f64) + 4.5;
                let y = zoom * pixel * (2.0 * fy - size as f64);
                let mut col = [0.0_f64; 3];
                let mut damp = 1.0;
                for i in 0..24 {
                    let angle = time - i as f64 * tau / 24.0;
                    let (cx, cy) = (r1 * angle.cos() + x1, r1 * angle.sin());
                    let r = (cx - 1.0).hypot(cy);
                    let d = (x - cx).hypot(y - cy) - r;
                    let glow = if d < 0.0 {
                        [5.0, 1.0, 2.0]
                    } else {
                        [2.0, 1.0, 6.0]
                    };
                    let k = (zoom * pixel / d.abs()).tanh();
                    for c in 0..3 {
                        col[c] += damp * glow[c] * k;
                    }
                    damp *= 0.9;
                }
                let want = col.map(f64::tanh);
                let got = frame
                    .shade(px as f64 + 0.5, py as f64 + 0.5)
                    .unwrap()
                    .map(|c| {
                        if c <= 0.0031308 {
                            12.92 * c
                        } else {
                            1.055 * c.powf(1.0 / 2.4) - 0.055
                        }
                    });
                // Right on a rim, the shader changes color all at once, and this has an edge.
                let near_a_rim = (0..24).any(|i| {
                    let angle = time - i as f64 * tau / 24.0;
                    let (cx, cy) = (r1 * angle.cos() + x1, r1 * angle.sin());
                    let r = (cx - 1.0).hypot(cy);
                    ((x - cx).hypot(y - cy) - r).abs() < 2.0 * zoom * pixel
                });
                if near_a_rim {
                    continue;
                }
                compared += 1;
                for c in 0..3 {
                    assert!(
                        (got[c] - want[c]).abs() < 2e-3,
                        "{px}, {py} at {time}: {got:?} against {want:?}"
                    );
                }
            }
        }
    }
    assert!(compared > 2000, "{compared} pixels");
}

const FLIPPED: &str = "viewport { x: -8..8, y: -8..8 }
    circles c = circle((s, 1), (0, 2), (-s, 1)) for s in -1..1 with u = s + 2";

#[test]
fn a_collection_has_both_kinds_of_side() {
    // s = -1 runs clockwise, s = 0 is three points in one place, and s = 1 runs counterclockwise,
    // both around (0, 1). Each has the disk's value it was given.
    let shade = |sheet: &str, at: (f64, f64)| {
        let figure = build(FLIPPED, &[sheet]).unwrap();
        let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
        frame.shade(at.0, at.1).unwrap()
    };
    let (inner, far) = ((80.5, 70.5), (5.5, 5.5));
    let clear = "style paper { fill: white } ";
    // Both are the same disk, so `::inside` covers it twice, and the rest not at all.
    let sheet = format!("{clear} style c::inside {{ fill: black }}");
    assert_eq!(shade(&sheet, inner), [0.0; 3]);
    assert_eq!(shade(&sheet, far), [1.0; 3]);
    // The negative side is the disk for one, and everything else for the other.
    let sheet = format!("{clear} style c::negative {{ fill: black }}");
    assert_eq!(shade(&sheet, inner), [0.0; 3]);
    assert_eq!(shade(&sheet, far), [0.0; 3]);
    // Only the one whose curvature is positive.
    let sheet = format!(
        "{clear} style c::negative {{ fill: black }} style c {{ fill_opacity: max(sign(self.curvature), 0) }}"
    );
    assert_eq!(shade(&sheet, inner), [0.0; 3]);
    assert_eq!(shade(&sheet, far), [1.0; 3]);
    // What a style reads per instance: the last one painted is the one with u = 3, over u = 1.
    let sheet = format!("{clear} style c::inside {{ fill: rgb(u·85, 0, 0) }}");
    let got = shade(&sheet, inner);
    assert!((got[0] - 1.0).abs() < 1e-6, "{got:?}");
    let sheet =
        format!("{clear} style c::inside {{ fill: black }} style c {{ fill_opacity: 3 - u }}");
    assert_eq!(shade(&sheet, inner), [0.0; 3]);
}

#[test]
fn a_line_in_a_collection_is_drawn() {
    let figure = build(
        "viewport { x: -8..8, y: -8..8 }
         circles c = circle((-3, s), (0, s), (5, s)) for s in 1..1",
        &["style paper { fill: white } style c { stroke: black; width: 2px }"],
    )
    .unwrap();
    let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
    assert_eq!(frame.shade(20.5, 70.0).unwrap(), [0.0, 0.0, 0.0]);
    assert_eq!(frame.shade(120.5, 69.5).unwrap(), [0.0, 0.0, 0.0]);
    assert_eq!(frame.shade(20.5, 60.5).unwrap(), [1.0, 1.0, 1.0]);
}

#[test]
fn too_many_circles_with_sides_is_an_error_and_not_a_gap() {
    let doc = "viewport { x: -8..8, y: -8..8 }
         circles c = circle((s, 0), (s, 1)) for s in 1..300";
    let figure = build(doc, &["style c::inside { fill: black }"]).unwrap();
    let err = pencil_core::frame::prepare(&figure, &Inputs::at(0.0), 40, 40, 1.0).unwrap_err();
    assert!(err.to_string().contains("the limit is 256"), "{err}");
    // Its rims alone are as cheap as they ever were.
    let figure = build(doc, &["style c { stroke: black }"]).unwrap();
    pencil_core::frame::prepare(&figure, &Inputs::at(0.0), 40, 40, 1.0).unwrap();
}

#[test]
fn what_a_collection_cant_do_is_an_error() {
    for (doc, sheet, wanted) in [
        (
            FLIPPED,
            "style c { stroke: mix(white, black, self.dist) }",
            "can't use `self`",
        ),
        (FLIPPED, "style c { fill: black }", "needs a facet"),
        (
            "viewport { x: -8..8, y: -8..8 }\ndots c = (s, 0) for s in 1..3",
            "style c::inside { fill: black }",
            "has no sides",
        ),
        (
            "viewport { x: -8..8, y: -8..8 }\ndots c = (s, 0) for s in 1..3",
            "style c { fill_opacity: self.curvature }",
            "isn't a circle",
        ),
    ] {
        let err = build(doc, &[sheet]).unwrap_err();
        assert!(err.to_string().contains(wanted), "{sheet}: {err}");
    }
}
