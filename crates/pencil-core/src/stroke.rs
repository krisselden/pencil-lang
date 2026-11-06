//! Turns marks into lines of stroke segments, each by its own form's fast path, and
//! defines their coverage. The GPU pass in `wgsl::COVERAGE_SHADER` and `rasterize`
//! here implement the same coverage.
//!
//! Segments overlap at joints, so coverage combines by maximum. That keeps a joint
//! from darkening where two segments both cover it.

use crate::Figure;
use crate::compile::CONSTANTS;
use crate::error::Error;
use crate::eval::{self, Env};
use crate::frame::{self, DEFAULT_STROKE_WIDTH};
use crate::ir::{Arc, Expr, Form, Part, Viewport};
use crate::style::{Cap, MarkStyle, SELF_T};
use crate::trace::{Polyline, trace};

pub const FLOATS_PER_SEGMENT: usize = 12;
/// Grid cell size for tracing an implicit curve, in device pixels.
pub const CELL_PX: f64 = 6.0;
/// How far a chord may stray from a parametric curve, in device pixels.
const FLATNESS_PX: f64 = 0.05;
const INITIAL_INTERVALS: usize = 32;
const MAX_DEPTH: usize = 8;

/// The fraction of a pixel on the near side of a straight edge: the exact box
/// filter. `t` is the edge's signed offset from the pixel center along unit normal
/// `n`, in pixels. A diagonal edge ramps over √2 pixels, not one, and thin strokes
/// are the difference of two of these.
pub fn box_coverage(t: f64, nx: f64, ny: f64) -> f64 {
    let (u, v) = (nx.abs().min(ny.abs()), nx.abs().max(ny.abs()));
    let (a, b) = ((v - u) / 2.0, (v + u) / 2.0);
    let at = t.abs();
    if at >= b {
        return if t > 0.0 { 1.0 } else { 0.0 };
    }
    if at <= a {
        return 0.5 + t / v;
    }
    let tail = (b - at) * (b - at) / (2.0 * u * v);
    if t > 0.0 { 1.0 - tail } else { tail }
}

/// Samples a parametric curve directly, subdividing until chords are flat enough. No
/// field is involved, which is the point of keeping this form as written.
pub fn sample_arc(
    arc: &Arc,
    env: &Env,
    view: &Viewport,
    width: u32,
    height: u32,
) -> Result<Vec<[f64; 2]>, Error> {
    let (lo, hi) = (
        eval::real(&arc.range[0], env)?,
        eval::real(&arc.range[1], env)?,
    );
    if hi <= lo {
        return Ok(Vec::new());
    }
    let mut env = env.clone();
    let mut at = |t: f64| -> Result<[f64; 2], Error> {
        env.set(&arc.var, t);
        Ok([
            eval::real(&arc.point[0], &env)?,
            eval::real(&arc.point[1], &env)?,
        ])
    };
    let device = |p: [f64; 2]| frame::device(p[0], p[1], view, width, height);

    struct Span {
        t0: f64,
        p0: [f64; 2],
        t1: f64,
        p1: [f64; 2],
        depth: usize,
    }
    let mut out = vec![at(lo)?];
    let mut stack = Vec::new();
    let mut previous = out[0];
    for i in 1..=INITIAL_INTERVALS {
        let t = lo + (hi - lo) * i as f64 / INITIAL_INTERVALS as f64;
        let p = at(t)?;
        stack.push(Span {
            t0: lo + (hi - lo) * (i - 1) as f64 / INITIAL_INTERVALS as f64,
            p0: previous,
            t1: t,
            p1: p,
            depth: 0,
        });
        previous = p;
    }
    stack.reverse();
    while let Some(span) = stack.pop() {
        let tm = (span.t0 + span.t1) / 2.0;
        let pm = at(tm)?;
        let (a, b, m) = (device(span.p0), device(span.p1), device(pm));
        let stray = (m.0 - (a.0 + b.0) / 2.0).hypot(m.1 - (a.1 + b.1) / 2.0);
        if stray > FLATNESS_PX && span.depth < MAX_DEPTH {
            stack.push(Span {
                t0: tm,
                p0: pm,
                t1: span.t1,
                p1: span.p1,
                depth: span.depth + 1,
            });
            stack.push(Span {
                t0: span.t0,
                p0: span.p0,
                t1: tm,
                p1: pm,
                depth: span.depth + 1,
            });
        } else {
            out.push(span.p1);
        }
    }
    Ok(out)
}

/// Whether an expression can't change between frames: it uses no bindings or signals.
fn is_constant(e: &Expr) -> bool {
    let mut constant = true;
    e.visit_syms(&mut |s| constant &= CONSTANTS.contains(&s));
    constant
}

/// The lines a mark's stroke follows, in world coordinates.
pub fn lines(
    form: &Form,
    env: &Env,
    view: &Viewport,
    width: u32,
    height: u32,
) -> Result<Vec<Polyline>, Error> {
    let close_enough = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9;
    match form {
        Form::Field {
            vars, field, grad, ..
        } => {
            let cols = (width as f64 / CELL_PX).ceil() as u32;
            let rows = (height as f64 / CELL_PX).ceil() as u32;
            trace(vars, field, grad, env, view, cols, rows)
        }
        Form::Parametric(arc) => {
            let points = sample_arc(arc, env, view, width, height)?;
            if points.len() < 2 {
                return Ok(Vec::new());
            }
            // Only a fixed range makes a loop. A moving range whose ends happen to meet is
            // still an open line for that frame, or its dashes would jump as it passes.
            let fixed = arc.range.iter().all(is_constant);
            let closed = fixed && close_enough(points[0], points[points.len() - 1]);
            Ok(vec![Polyline { points, closed }])
        }
        Form::Polyline { points, closed } => {
            let mut world = Vec::with_capacity(points.len() + 1);
            for p in points {
                world.push([eval::real(&p[0], env)?, eval::real(&p[1], env)?]);
            }
            if *closed {
                world.push(world[0]);
            }
            Ok(vec![Polyline {
                points: world,
                closed: *closed,
            }])
        }
        Form::Region(parts) => {
            let mut world = Vec::new();
            for part in parts {
                match part {
                    Part::Point(p) => {
                        world.push([eval::real(&p[0], env)?, eval::real(&p[1], env)?]);
                    }
                    Part::Arc(arc) => world.extend(sample_arc(arc, env, view, width, height)?),
                }
            }
            if world.is_empty() {
                return Ok(Vec::new());
            }
            world.push(world[0]);
            Ok(vec![Polyline {
                points: world,
                closed: true,
            }])
        }
        Form::Dot(_) | Form::Label { .. } => Ok(Vec::new()),
    }
}

/// Builds one segment list per line stroke, in mark order, in device pixels:
/// `x0 y0 x1 y1  r0 r1 s0 s1  dash_on dash_period line_length 0`. A nonzero length
/// clips coverage to the line's arc length, so it ends square as in SVG, even where
/// an earlier segment's round joint would overhang the end.
pub fn build(
    figure: &Figure,
    env: &Env,
    width: u32,
    height: u32,
    dpr: f64,
) -> Result<Vec<Vec<f32>>, Error> {
    let view = &figure.scene.viewport;
    let mut layers = Vec::new();
    for (mark, style) in figure.scene.marks.iter().zip(&figure.styles.marks) {
        if !style.stroke_is_line(&mark.form) {
            continue;
        }
        let mut segments = Vec::new();
        let to_device = |p: [f64; 2]| {
            let (x, y) = frame::device(p[0], p[1], view, width, height);
            [x, y]
        };
        for line in lines(&mark.form, env, view, width, height)? {
            push_line(&line, style, env, dpr, &to_device, &mut segments)?;
        }
        layers.push(segments);
    }
    Ok(layers)
}

fn pixels(e: &Expr, env: &Env, dpr: f64) -> Result<f64, Error> {
    Ok(eval::real(e, env)? * dpr)
}

fn push_line(
    line: &Polyline,
    style: &MarkStyle,
    env: &Env,
    dpr: f64,
    to_device: &impl Fn([f64; 2]) -> [f64; 2],
    segments: &mut Vec<f32>,
) -> Result<(), Error> {
    let points: Vec<[f64; 2]> = line.points.iter().map(|p| to_device(*p)).collect();
    let mut lengths = Vec::with_capacity(points.len());
    let mut total = 0.0;
    lengths.push(0.0);
    for w in points.windows(2) {
        total += (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        lengths.push(total);
    }
    if total <= 0.0 {
        return Ok(());
    }

    let mut env = env.clone();
    let mut radii = Vec::with_capacity(points.len());
    for s in &lengths {
        env.set(SELF_T, s / total);
        let width = match &style.width {
            Some(w) => pixels(w, &env, dpr)?,
            None => DEFAULT_STROKE_WIDTH * dpr,
        };
        radii.push((width / 2.0).max(0.0));
    }

    let (mut on, mut period) = (0.0, 0.0);
    if let Some(dash) = &style.dash {
        let dash = pixels(dash, &env, dpr)?;
        let gap = match &style.gap {
            Some(gap) => pixels(gap, &env, dpr)?,
            None => dash,
        };
        (on, period) = (dash, dash + gap);
        // A loop gets a whole number of dashes, stretched to fit, so it has no seam.
        if line.closed && period > 0.0 {
            let stretch = total / ((total / period).round().max(1.0) * period);
            on *= stretch;
            period *= stretch;
        }
    }

    // A loop of constant width has no ends. A taper's seam does: the width jumps there.
    let seamless = line.closed && (radii[0] - radii[radii.len() - 1]).abs() < 1e-6;
    let clip = if seamless || style.cap == Cap::Round {
        0.0
    } else {
        total
    };
    for i in 0..points.len() - 1 {
        segments.extend_from_slice(&[
            points[i][0] as f32,
            points[i][1] as f32,
            points[i + 1][0] as f32,
            points[i + 1][1] as f32,
            radii[i] as f32,
            radii[i + 1] as f32,
            lengths[i] as f32,
            lengths[i + 1] as f32,
            on as f32,
            period as f32,
            clip as f32,
            0.0,
        ]);
    }
    Ok(())
}

/// One segment's coverage of the pixel centered at `(px, py)`.
pub fn segment_coverage(s: &[f32], px: f64, py: f64) -> f64 {
    let f = |i: usize| s[i] as f64;
    let (ax, ay, bx, by) = (f(0), f(1), f(2), f(3));
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let along = if len2 > 1e-12 {
        ((px - ax) * dx + (py - ay) * dy) / len2
    } else {
        0.0
    };
    let t = along.clamp(0.0, 1.0);
    let (cx, cy) = (ax + dx * t, ay + dy * t);
    let (vx, vy) = (px - cx, py - cy);
    let rho = vx.hypot(vy);
    // On the centerline the normal is the segment's own.
    let len = len2.sqrt().max(1e-6);
    let (nx, ny) = if rho > 1e-6 {
        (vx / rho, vy / rho)
    } else {
        (-dy / len, dx / len)
    };
    let half = f(4) + (f(5) - f(4)) * t;
    let mut coverage = box_coverage(half - rho, nx, ny) - box_coverage(-half - rho, nx, ny);
    let (on, period) = (f(8), f(9));
    if period > 0.0 {
        // Unclamped, so a joint's round overhang still reads as the gap it lies in.
        let phase = (f(6) + (f(7) - f(6)) * along).rem_euclid(period);
        let center = on / 2.0;
        let mut away = (phase - center).abs();
        away = away.min(period - away);
        coverage *= (0.5 + center - away).clamp(0.0, 1.0);
    }
    if f(10) > 0.0 {
        let position = f(6) + (f(7) - f(6)) * along;
        coverage *= (0.5 + position).clamp(0.0, 1.0);
        coverage *= (0.5 + f(10) - position).clamp(0.0, 1.0);
    }
    coverage
}

/// The CPU equivalent of the coverage pass: each segment's quad, combined by maximum.
pub fn rasterize(segments: &[f32], width: u32, height: u32) -> Vec<f64> {
    let mut out = vec![0.0f64; (width * height) as usize];
    for s in segments.chunks(FLOATS_PER_SEGMENT) {
        let pad = (s[4].max(s[5]) + 2.0) as f64;
        let (ax, ay, bx, by) = (s[0] as f64, s[1] as f64, s[2] as f64, s[3] as f64);
        let x0 = (ax.min(bx) - pad).floor().max(0.0) as u32;
        let y0 = (ay.min(by) - pad).floor().max(0.0) as u32;
        let x1 = ((ax.max(bx) + pad).ceil().max(0.0) as u32).min(width);
        let y1 = ((ay.max(by) + pad).ceil().max(0.0) as u32).min(height);
        for y in y0..y1 {
            for x in x0..x1 {
                let c = segment_coverage(s, x as f64 + 0.5, y as f64 + 0.5);
                let slot = &mut out[(y * width + x) as usize];
                *slot = slot.max(c);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build as build_figure;
    use crate::frame::{Inputs, prepare};

    fn supersampled(t: f64, nx: f64, ny: f64) -> f64 {
        // Pixel centered at 0, covered where n·q <= t.
        let n = 512;
        let mut inside = 0;
        for j in 0..n {
            for i in 0..n {
                let qx = (i as f64 + 0.5) / n as f64 - 0.5;
                let qy = (j as f64 + 0.5) / n as f64 - 0.5;
                if nx * qx + ny * qy <= t {
                    inside += 1;
                }
            }
        }
        inside as f64 / (n * n) as f64
    }

    #[test]
    fn box_coverage_matches_a_supersampled_edge() {
        let mut box_worst = 0.0f64;
        let mut ramp_worst = 0.0f64;
        for angle in [0.0f64, 7.0, 22.5, 45.0, 60.0, 89.0, 133.0] {
            let (ny, nx) = angle.to_radians().sin_cos();
            for t in [-0.9, -0.6, -0.3, -0.1, 0.0, 0.2, 0.45, 0.7] {
                let truth = supersampled(t, nx, ny);
                box_worst = box_worst.max((box_coverage(t, nx, ny) - truth).abs());
                ramp_worst = ramp_worst.max(((0.5 + t).clamp(0.0, 1.0) - truth).abs());
            }
        }
        assert!(box_worst < 3e-3, "box filter error {box_worst}");
        // The common one-pixel ramp is measurably worse, which is why it isn't used.
        assert!(
            ramp_worst > 10.0 * box_worst,
            "ramp {ramp_worst}, box {box_worst}"
        );
    }

    const CIRCLE: &str = "curve c = implicit (x, y): x^2 + y^2 = 1";

    fn layer_of(doc: &str, sheet: &str, size: u32) -> Vec<f32> {
        let figure = build_figure(doc, &[sheet]).unwrap();
        prepare(&figure, &Inputs::at(0.0), size, size, 1.0)
            .unwrap()
            .strokes
            .remove(0)
    }

    fn layer(sheet: &str, size: u32) -> Vec<f32> {
        layer_of(CIRCLE, sheet, size)
    }

    #[test]
    fn segments_chain_and_measure_arc_length() {
        let s = layer("style c { stroke: black; width: 2px }", 400);
        let segments: Vec<&[f32]> = s.chunks(FLOATS_PER_SEGMENT).collect();
        for pair in segments.windows(2) {
            assert_eq!(pair[0][2..4], pair[1][0..2]);
            assert_eq!(pair[0][7], pair[1][6]);
        }
        // A unit circle at 100 px per unit.
        let total = segments.last().unwrap()[7];
        assert!((total - 628.3).abs() < 0.5, "{total}");
        assert!(segments.iter().all(|g| g[4] == 1.0 && g[5] == 1.0));
    }

    #[test]
    fn taper_follows_self_t() {
        let s = layer("style c { stroke: black; width: 1px + 9px * self.t }", 400);
        let segments: Vec<&[f32]> = s.chunks(FLOATS_PER_SEGMENT).collect();
        assert!((segments[0][4] - 0.5).abs() < 1e-3);
        assert!((segments.last().unwrap()[5] - 5.0).abs() < 1e-3);
    }

    #[test]
    fn loops_get_a_whole_number_of_dashes() {
        let s = layer(
            "style c { stroke: black; width: 2px; dash: 10px; gap: 7px }",
            400,
        );
        let segments: Vec<&[f32]> = s.chunks(FLOATS_PER_SEGMENT).collect();
        let (total, on, period) = (
            segments.last().unwrap()[7] as f64,
            segments[0][8] as f64,
            segments[0][9] as f64,
        );
        let count = total / period;
        assert!((count - count.round()).abs() < 1e-3, "{count}");
        assert!((on / period - 10.0 / 17.0).abs() < 1e-4);
    }

    #[test]
    fn coverage_is_full_on_the_stroke_and_zero_off_it() {
        let s = layer("style c { stroke: black; width: 4px }", 400);
        let c = rasterize(&s, 400, 400);
        let at = |x: u32, y: u32| c[(y * 400 + x) as usize];
        assert_eq!(at(300, 200), 1.0);
        assert_eq!(at(305, 200), 0.0);
        assert_eq!(at(200, 200), 0.0);
    }

    #[test]
    fn dash_ends_are_square_despite_round_joints() {
        // A segment ending just inside a dash must not overhang into the gap.
        let segment = [
            0.0, 10.0, 20.0, 10.0, 4.0, 4.0, 0.0, 20.0, 21.0, 40.0, 0.0, 0.0,
        ];
        assert_eq!(segment_coverage(&segment, 19.5, 10.5), 1.0);
        assert_eq!(segment_coverage(&segment, 23.0, 10.5), 0.0);
    }

    #[test]
    fn line_ends_are_square() {
        let mut segment = [
            10.0, 10.0, 20.0, 10.0, 4.0, 4.0, 0.0, 10.0, 0.0, 0.0, 0.0, 0.0,
        ];
        assert!(segment_coverage(&segment, 7.5, 10.5) > 0.99);
        assert!(segment_coverage(&segment, 22.5, 10.5) > 0.99);
        segment[10] = 10.0;
        assert_eq!(segment_coverage(&segment, 7.5, 10.5), 0.0);
        assert_eq!(segment_coverage(&segment, 10.5, 10.5), 1.0);
        assert_eq!(segment_coverage(&segment, 22.5, 10.5), 0.0);
    }

    /// A joint before the end of a line must not overhang past that end.
    #[test]
    fn the_whole_line_clips_to_its_length() {
        // Ends 8 pixels past this segment, so it mustn't reach beyond that.
        let segment = [
            10.0, 10.0, 20.0, 10.0, 12.0, 12.0, 92.0, 102.0, 0.0, 0.0, 110.0, 0.0,
        ];
        assert_eq!(segment_coverage(&segment, 26.5, 10.5), 1.0);
        assert_eq!(segment_coverage(&segment, 30.5, 10.5), 0.0);
    }

    /// Segments overlap at joints, and that must not show as a darker knot.
    #[test]
    fn joints_dont_double_cover() {
        let s = layer("style c { stroke: black; width: 4px }", 400);
        let c = rasterize(&s, 400, 400);
        assert!(c.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn parametric_curves_land_on_the_curve_and_subdivide_where_curved() {
        let doc = "curve p = parametric (t): (cos(t), sin(t)) for t in 0..tau";
        let s = layer_of(doc, "style p { stroke: black; width: 1px }", 400);
        let segments: Vec<&[f32]> = s.chunks(FLOATS_PER_SEGMENT).collect();
        for g in &segments {
            let (x, y) = ((g[0] as f64 - 200.0) / 100.0, (200.0 - g[1] as f64) / 100.0);
            assert!((x.hypot(y) - 1.0).abs() < 1e-5, "{x} {y}");
        }
        assert!(segments.len() > INITIAL_INTERVALS, "{}", segments.len());
        // Its ends meet, so a constant-width loop has no clip.
        assert_eq!(segments[0][10], 0.0);
    }

    #[test]
    fn straight_parametric_curves_stay_cheap() {
        let doc = "curve p = parametric (t): (t, 0) for t in -1..1";
        let s = layer_of(doc, "style p { stroke: black; width: 1px }", 400);
        assert_eq!(s.len() / FLOATS_PER_SEGMENT, INITIAL_INTERVALS);
    }

    #[test]
    fn an_empty_range_draws_nothing() {
        let doc = "curve p = parametric (t): (t, 0) for t in 0..0";
        let s = layer_of(doc, "style p { stroke: black; width: 1px }", 400);
        assert!(s.is_empty());
    }

    /// Reported on Unroll's night style: the wound string's ends meet at exactly a = 0,
    /// and that moment mustn't change how its dashes are laid out.
    #[test]
    fn a_moving_range_keeps_its_dashes_when_its_ends_momentarily_meet() {
        let doc = "param a in 0..tau
                   curve c = parametric (t): (cos(t), sin(t)) for t in a..tau";
        let sheet = "style c { stroke: black; width: 2px; dash: 7px; gap: 5px }";
        let figure = build_figure(doc, &[sheet]).unwrap();
        let at = |a: f64| {
            let inputs = Inputs {
                time: 0.0,
                params: vec![("a".to_string(), a)],
            };
            prepare(&figure, &inputs, 400, 400, 1.0)
                .unwrap()
                .strokes
                .remove(0)
        };
        let (touching, just_after) = (at(0.0), at(0.0005));
        // The same period either side, and no switch between open and closed.
        assert_eq!(touching[8], just_after[8]);
        assert_eq!(touching[9], just_after[9]);
        assert!(touching[10] > 0.0 && just_after[10] > 0.0);
        let (a, b) = (
            rasterize(&touching, 400, 400),
            rasterize(&just_after, 400, 400),
        );
        let moved: f64 = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum();
        // Sliding every dash by 0.05 pixels moves far less coverage than rearranging them.
        assert!(moved < 60.0, "coverage moved by {moved}");
    }

    /// A range that can't move stays a real loop, with a whole number of dashes.
    #[test]
    fn a_fixed_range_that_closes_is_a_loop() {
        let doc = "curve c = parametric (t): (cos(t), sin(t)) for t in 0..tau";
        let s = layer_of(
            doc,
            "style c { stroke: black; width: 2px; dash: 7px; gap: 5px }",
            400,
        );
        let segments: Vec<&[f32]> = s.chunks(FLOATS_PER_SEGMENT).collect();
        let count = segments.last().unwrap()[7] as f64 / segments[0][9] as f64;
        assert!((count - count.round()).abs() < 1e-3, "{count}");
        assert_eq!(segments[0][10], 0.0);
    }

    #[test]
    fn round_caps_keep_the_segments_round_ends() {
        let doc = "viewport { x: 0..4, y: 0..4 }
                   segment s = (1, 2), (3, 2)";
        let butt = layer_of(doc, "style s { stroke: black; width: 8px }", 400);
        let round = layer_of(
            doc,
            "style s { stroke: black; width: 8px; cap: round }",
            400,
        );
        // World x = 1 is pixel 100, and this pixel is 2.5 past the end, on the centerline.
        let past = |s: &[f32]| rasterize(s, 400, 400)[(200 * 400 + 97) as usize];
        assert_eq!(past(&butt), 0.0);
        assert_eq!(past(&round), 1.0);
    }

    #[test]
    fn polylines_keep_their_corners() {
        let doc = "viewport { x: 0..4, y: 0..4 }
                   path p = (1, 1), (3, 1), (3, 3)";
        let s = layer_of(doc, "style p { stroke: black; width: 2px }", 400);
        assert_eq!(s.len() / FLOATS_PER_SEGMENT, 2);
        // An open line clips to its length, so both ends are square.
        assert!(s[10] > 0.0);
        let c = rasterize(&s, 400, 400);
        assert_eq!(c[(300 * 400 + 200) as usize], 1.0);
        assert_eq!(c[(300 * 400 + 95) as usize], 0.0);
    }
}
