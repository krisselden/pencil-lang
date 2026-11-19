//! The blog's draggable points, written as Pencil handles. Each blog figure keeps its point in a
//! region with a `constrain` function from the pointer to the nearest allowed point
//! (`krisselden_com`, `src/lib/plot.ts`). Here each region is a handle: a curve is one param and
//! its expression, a two-dimensional region is two params, whose ranges give a box in their own
//! coordinates, and anything more is in the two expressions, which are the constrain function
//! written out. Each test drags the Pencil handle and compares where its point lands with the
//! blog's function, ported to Rust (`docs/migration.md`, "How a point is dragged in two
//! dimensions", lists them).

use pencil_core::Figure;
use pencil_core::build;
use pencil_core::frame::{Inputs, drag_value, handle_places, view_for};

const W: u32 = 600;
const H: u32 = 400;

fn inputs(params: &[(String, f64)]) -> Inputs {
    Inputs {
        time: 0.0,
        params: params.to_vec(),
    }
}

/// Drags a handle to a point of the figure, with these params held, and gives what it sets.
fn drag(f: &Figure, handle: usize, held: &[(String, f64)], x: f64, y: f64) -> Vec<(String, f64)> {
    let at = inputs(held);
    let v = view_for(f, &at, W, H).unwrap();
    let px = (x - v.x[0]) / (v.x[1] - v.x[0]) * W as f64;
    let py = (v.y[1] - y) / (v.y[1] - v.y[0]) * H as f64;
    drag_value(f, &at, handle, W, H, px, py).unwrap()
}

/// Where a handle's point is, in the figure's coordinates, with these params.
fn mark(f: &Figure, handle: usize, params: &[(String, f64)]) -> (f64, f64) {
    let at = inputs(params);
    let v = view_for(f, &at, W, H).unwrap();
    let (px, py) = handle_places(f, &at, W, H).unwrap()[handle].unwrap();
    (
        v.x[0] + px / W as f64 * (v.x[1] - v.x[0]),
        v.y[1] - py / H as f64 * (v.y[1] - v.y[0]),
    )
}

/// Drags and then reads where the point went, with what the drag set held over `held`.
fn dragged(f: &Figure, handle: usize, held: &[(String, f64)], x: f64, y: f64) -> (f64, f64) {
    let mut params = held.to_vec();
    params.extend(drag(f, handle, held, x, y));
    mark(f, handle, &params)
}

fn close(got: (f64, f64), want: (f64, f64), what: &str) {
    assert!(
        (got.0 - want.0).abs() < 1e-9 && (got.1 - want.1).abs() < 1e-9,
        "{what}: got {got:?}, want {want:?}"
    );
}

/// Pointer positions across a figure, inside its view and past its edges.
fn sweep(x: [f64; 2], y: [f64; 2], n: usize) -> impl Iterator<Item = (f64, f64)> {
    (0..=n).flat_map(move |i| {
        (0..=n).map(move |j| {
            (
                x[0] + (x[1] - x[0]) * i as f64 / n as f64,
                y[0] + (y[1] - y[0]) * j as f64 / n as f64,
            )
        })
    })
}

/// `FloorCell`'s free point: a rectangle in the plane, which is a box in the two params.
#[test]
fn floor_cell_a_rectangle() {
    // gHalf = 3.2 · 380/560, and the point stays 0.3 inside it.
    let half = 3.2 * 380.0 / 560.0 - 0.3;
    let doc = format!(
        "viewport {{ x: -3.2..3.2, y: -2.2..2.2 }}
         param px = 0.35 in -2.6..2.6
         param py = 0.3 in -{half}..{half}
         let P = px + py·i
         dot P
         handle P sets px = pointer.x, py = pointer.y"
    );
    let f = build(&doc, &[]).unwrap();
    let blog = |x: f64, y: f64| (x.clamp(-2.6, 2.6), y.clamp(-half, half));
    for (x, y) in sweep([-3.1, 3.1], [-2.1, 2.1], 12) {
        close(
            dragged(&f, 0, &[], x, y),
            blog(x, y),
            &format!("({x}, {y})"),
        );
    }
}

/// `FollowPixel`: w is a rectangle on the grid, and z in the spiral sets w through the inverse
/// map, log and then linear, clamped to the same rectangle. The map's growth and shear come from
/// a `solve` in the figure (`007-lattice`), and are numbers here.
#[test]
fn follow_pixel_a_rectangle_and_its_image_in_the_spiral() {
    let (growth, shear) = (0.3, 0.1);
    let doc = format!(
        "viewport {{ x: -3..3, y: -3..3 }}
         let omega = -1/2 + sqrt(3)/2·i
         let A = -4 + 8·omega
         let growth = {growth}
         let shear = {shear}
         fn center(w) = exp(growth·real(2·π·i·w/A) + (shear·real(2·π·i·w/A) + imag(2·π·i·w/A))·i)
         fn to_grid(z) = A·(real(ln(z))/growth + (imag(ln(z)) - shear·real(ln(z))/growth)·i)/(2·π·i)
         param wx = 0.55 in -1.25..1.25
         param wy = 0.42 in -1.1..1.1
         let w = wx + wy·i
         let z = center(w)
         dot w, z
         handle w sets wx = pointer.x, wy = pointer.y
         handle z sets wx = real(to_grid(pointer)), wy = imag(to_grid(pointer))"
    );
    let f = build(&doc, &[]).unwrap();
    // The blog's `doyleMap`, with the same period and skew.
    let (ax, ay) = (-4.0 - 4.0, 8.0 * 3f64.sqrt() / 2.0);
    let div = |(a, b): (f64, f64), (c, d): (f64, f64)| {
        let n = c * c + d * d;
        ((a * c + b * d) / n, (b * c - a * d) / n)
    };
    let mul = |(a, b): (f64, f64), (c, d): (f64, f64)| (a * c - b * d, a * d + b * c);
    let turn = (0.0, 2.0 * std::f64::consts::PI);
    let center = |w: (f64, f64)| {
        let (x, y) = mul(turn, div(w, (ax, ay)));
        let (lx, ly) = (growth * x, shear * x + y);
        (lx.exp() * ly.cos(), lx.exp() * ly.sin())
    };
    let to_grid = |(zx, zy): (f64, f64)| {
        let (lx, ly) = (zx.hypot(zy).ln(), zy.atan2(zx));
        let x = lx / growth;
        mul((ax, ay), div((x, ly - shear * x), turn))
    };
    let clamp_grid = |(x, y): (f64, f64)| (x.clamp(-1.25, 1.25), y.clamp(-1.1, 1.1));

    // w, dragged on the grid.
    for (x, y) in sweep([-2.0, 2.0], [-2.0, 2.0], 10) {
        close(
            dragged(&f, 0, &[], x, y),
            clamp_grid((x, y)),
            &format!("w at ({x}, {y})"),
        );
    }
    // z, dragged in the spiral, away from the negative real axis, where the log's angle jumps
    // by a turn in Pencil as in the blog.
    for (x, y) in sweep([-1.5, 2.5], [0.2, 2.5], 8).chain(sweep([0.2, 2.5], [-2.5, -0.2], 6)) {
        let held = drag(&f, 1, &[], x, y);
        let w = (held[0].1, held[1].1);
        close(
            w,
            clamp_grid(to_grid((x, y))),
            &format!("w from z at ({x}, {y})"),
        );
        close(mark(&f, 1, &held), center(w), &format!("z at ({x}, {y})"));
    }
}

/// `PowerViews`' z: a rectangle with two disks taken out, about 0 and about 1, where the point
/// is pushed out along the radius. The rectangle is the params' ranges, and the pushes are the
/// expressions: `max(1, r/|p − c|)` scales p − c up to the disk's edge if it's inside.
#[test]
fn power_views_a_rectangle_with_two_holes() {
    let (x_end, y_end) = (2.6, 2.6 * 340.0 / 360.0);
    let doc = format!(
        "viewport {{ x: -{x_end}..{x_end}, y: -{y_end}..{y_end} }}
         fn out(p, c, r) = c + (p - c)·max(1, r/max(abs(p - c), 1e-12))
         fn kept(p) = out(out(p, 0, 0.25), 1, 0.1)
         param zx = 0.94 in {}..{}
         param zy = 0.83 in {}..{}
         let z = zx + zy·i
         dot z
         handle z sets zx = real(kept(pointer)), zy = imag(kept(pointer))",
        -x_end + 0.2,
        x_end - 0.2,
        -y_end + 0.2,
        y_end - 0.2
    );
    let f = build(&doc, &[]).unwrap();
    // The blog's `clampZ`. It clamps to the rectangle first and the handle last, which is the
    // same wherever the disks are clear of the rectangle's edges, as they are.
    let blog = |x: f64, y: f64| {
        let mut p = (
            x.clamp(-x_end + 0.2, x_end - 0.2),
            y.clamp(-y_end + 0.2, y_end - 0.2),
        );
        let r = p.0.hypot(p.1);
        if r < 0.25 {
            let r = if r == 0.0 { 1.0 } else { r };
            p = (p.0 / r * 0.25, p.1 / r * 0.25);
        }
        let d = (p.0 - 1.0).hypot(p.1);
        if d < 0.1 {
            let d = if d == 0.0 { 1.0 } else { d };
            p = (1.0 + (p.0 - 1.0) / d * 0.1, p.1 / d * 0.1);
        }
        p
    };
    // Across the view and past it, and finely over both holes.
    let points = sweep([-2.8, 2.8], [-2.7, 2.7], 14)
        .chain(sweep([-0.3, 0.3], [-0.3, 0.3], 12))
        .chain(sweep([0.88, 1.12], [-0.12, 0.12], 12));
    for (x, y) in points {
        close(
            dragged(&f, 0, &[], x, y),
            blog(x, y),
            &format!("({x}, {y})"),
        );
    }
}

/// `CircumCot`: B on the line y = 1, its x kept in 0.12..2.1, and C anywhere on the circle about
/// 0 through B, by its angle, so C's circle grows with m.
#[test]
fn circum_cot_a_segment_and_a_circle_that_follows_it() {
    let f = build(
        "viewport { x: -2.7..3.1, y: -2.6..2.6 }
         param m = 1.2 in 0.12..2.1
         param phi = 1.9 in -π..π
         let B = (m, 1)
         let C = sqrt(m² + 1)·(cos(phi), sin(phi))
         dot B, C
         handle B sets m = pointer.x
         handle C sets phi = atan2(pointer.y, pointer.x)",
        &[],
    )
    .unwrap();
    for (x, y) in sweep([-2.5, 2.9], [-2.4, 2.4], 9) {
        close(dragged(&f, 0, &[], x, y), (x.clamp(0.12, 2.1), 1.0), "B");
        for m in [0.5, 1.2, 2.0] {
            let held = [("m".to_string(), m)];
            let (r, a) = ((m * m + 1.0f64).sqrt(), y.atan2(x));
            close(dragged(&f, 1, &held, x, y), (r * a.cos(), r * a.sin()), "C");
        }
    }
}

/// `LimaconTangency`'s z: on the limaçon, by the pointer's angle about 1, so it stays where it
/// is on the curve as k changes. Not the curve's nearest point: the blog's handle is the same.
#[test]
fn limacon_tangency_a_curve_by_its_angle_about_a_point() {
    let f = build(
        "viewport { x: -1..4, y: -2..2 }
         param k = 0.5 in 0.1..0.8
         param angle = 1.2 in -π..π
         let r = 2·k·(1 + k·cos(angle))/(1 - k²)
         let z = (1 + r·cos(angle), r·sin(angle))
         dot z
         handle z sets angle = atan2(pointer.y, pointer.x - 1)",
        &[],
    )
    .unwrap();
    let limacon = |k: f64, t: f64| {
        let r = 2.0 * k * (1.0 + k * t.cos()) / (1.0 - k * k);
        (1.0 + r * t.cos(), r * t.sin())
    };
    for (x, y) in sweep([-0.9, 3.9], [-1.9, 1.9], 9) {
        for k in [0.2, 0.5, 0.7] {
            let held = [("k".to_string(), k)];
            close(
                dragged(&f, 0, &held, x, y),
                limacon(k, y.atan2(x - 1.0)),
                "z",
            );
        }
    }
}

/// `MedianConstruction`: C on an arc of the unit circle, 15° to 40°, and z on a circle by its
/// angle β, whose range follows C's: |β| ≤ 2φ − 1.5°.
#[test]
fn median_construction_an_arc_and_one_whose_range_follows_it() {
    let f = build(
        "viewport { x: -1.5..1.5, y: -1.5..1.5 }
         param phi = 0.5 in 15·π/180..40·π/180
         param beta = 0.3 in -(2·phi - 1.5·π/180)..2·phi - 1.5·π/180
         let C = (cos(phi), sin(phi))
         let z = (cos(beta), sin(beta))
         dot C, z
         handle C sets phi = atan2(pointer.y, pointer.x)
         handle z sets beta = atan2(pointer.y, pointer.x)",
        &[],
    )
    .unwrap();
    let deg = std::f64::consts::PI / 180.0;
    for (x, y) in sweep([-1.4, 1.4], [-1.4, 1.4], 9) {
        let phi = y.atan2(x).clamp(15.0 * deg, 40.0 * deg);
        close(dragged(&f, 0, &[], x, y), (phi.cos(), phi.sin()), "C");
        for phi in [16.0 * deg, 30.0 * deg] {
            let held = [("phi".to_string(), phi)];
            let end = 2.0 * phi - 1.5 * deg;
            let beta = y.atan2(x).clamp(-end, end);
            close(dragged(&f, 1, &held, x, y), (beta.cos(), beta.sin()), "z");
        }
    }
}

/// `LimaconConstruction`'s z₁: on the circle about x₁ through C, at the pointer's angle about x₁,
/// where x₁ is where the tangent at C meets the axis. Its drivers are the same about other points.
#[test]
fn limacon_construction_a_circle_about_a_computed_point() {
    let f = build(
        "viewport { x: -2..3, y: -2..2 }
         param phi = 0.5 in 15·π/180..45·π/180
         param t = 2 in -π..π
         let C = (cos(phi), sin(phi))
         let x1 = (1/cos(phi), 0)
         let z1 = x1 + abs(C - x1)·(cos(t), sin(t))
         dot C, z1
         handle C sets phi = atan2(pointer.y, pointer.x)
         handle z1 sets t = atan2(pointer.y - x1.y, pointer.x - x1.x)",
        &[],
    )
    .unwrap();
    for (x, y) in sweep([-1.9, 2.9], [-1.9, 1.9], 9) {
        for phi in [0.3f64, 0.6] {
            let held = [("phi".to_string(), phi)];
            let x1 = 1.0 / phi.cos();
            let r = (phi.cos() - x1).hypot(phi.sin());
            let t = y.atan2(x - x1);
            close(
                dragged(&f, 1, &held, x, y),
                (x1 + r * t.cos(), r * t.sin()),
                "z1",
            );
        }
    }
}

/// `LimaconConstruction`'s C: an arc of the unit circle, 15° to 45°.
#[test]
fn limacon_construction_an_arc() {
    let f = build(
        "viewport { x: -1.5..1.5, y: -1.5..1.5 }
         param phi = 0.5 in 15·π/180..45·π/180
         let C = (cos(phi), sin(phi))
         dot C
         handle C sets phi = atan2(pointer.y, pointer.x)",
        &[],
    )
    .unwrap();
    let deg = std::f64::consts::PI / 180.0;
    for (x, y) in sweep([-1.4, 1.4], [-1.4, 1.4], 9) {
        let phi = y.atan2(x).clamp(15.0 * deg, 45.0 * deg);
        close(dragged(&f, 0, &[], x, y), (phi.cos(), phi.sin()), "C");
    }
}

/// `TriangleLayers`' eˢ, on y = 0 in 1.4..2.3, and `FloorCell`'s x, on y = 0 in 0.05..5.95: a
/// segment of a line, one param.
#[test]
fn a_segment_of_a_line() {
    for (lo, hi, view) in [(1.4, 2.3, [-1.3, 2.7]), (0.05, 5.95, [-0.3, 6.3])] {
        let f = build(
            &format!(
                "viewport {{ x: {}..{}, y: -1..1 }}
                 param s = {lo} in {lo}..{hi}
                 let E = (s, 0)
                 dot E
                 handle E sets s = pointer.x",
                view[0], view[1]
            ),
            &[],
        )
        .unwrap();
        for (x, y) in sweep([view[0] + 0.1, view[1] - 0.1], [-0.9, 0.9], 8) {
            close(
                dragged(&f, 0, &[], x, y),
                (x.clamp(lo, hi), 0.0),
                "the point",
            );
        }
    }
}

/// `BoundsConstruction`'s P, dragged in the corpus figure itself, `004-bounds`: the upper unit
/// circle by angle, 0.2 to 1.85, with a pointer below the axis read as on it.
#[test]
fn bounds_construction_the_corpus_figure() {
    let f = build(include_str!("../../../corpus/004-bounds/bounds.pen"), &[]).unwrap();
    for (x, y) in sweep([-1.6, 3.4], [-1.4, 1.4], 10) {
        let a = y.max(0.0).atan2(x).clamp(0.2, 1.85);
        close(dragged(&f, 0, &[], x, y), (a.cos(), a.sin()), "P");
    }
}

/// `LogNearest`'s x, dragged in the corpus figure itself, `015-log-nearest`: a segment of the
/// line y = 1.3. The blog's lower end is e^−0.35 + 0.02, about 0.7247, and the corpus's is 0.72,
/// so below 0.7247 they differ by that much and nowhere else (`docs/migration.md`).
#[test]
fn log_nearest_the_corpus_figure() {
    let f = build(
        include_str!("../../../corpus/015-log-nearest/log-nearest.pen"),
        &[],
    )
    .unwrap();
    let blog_low = (-0.35f64).exp() + 0.02;
    assert!((blog_low - 0.7247).abs() < 1e-4, "{blog_low}");
    for (x, y) in sweep([-0.3, 8.1], [-0.9, 2.1], 12) {
        let (px, py) = dragged(&f, 0, &[], x, y);
        assert!((py - 1.3).abs() < 1e-9, "x on its line: {py}");
        let blog = x.clamp(blog_low, 7.9);
        if x >= blog_low {
            assert!((px - blog).abs() < 1e-9, "at {x}: {px}, the blog's {blog}");
        } else {
            assert!((px - x.max(0.72)).abs() < 1e-9, "at {x}: {px}");
            assert!(blog - px <= blog_low - 0.72 + 1e-12);
        }
    }
}
