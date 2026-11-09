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
use crate::fast;
use crate::frame::{self, DEFAULT_STROKE_WIDTH};
use crate::ir::{Arc, Expr, Form, Part, Shape, Viewport};
use crate::style::{Cap, MarkStyle, SELF_SCALE, SELF_T};
use crate::trace::{Polyline, trace};

pub const FLOATS_PER_SEGMENT: usize = 12;
/// More than this in one collection is surely a mistake in its ranges.
const MANY_LIMIT: usize = 200_000;
/// A circle smaller than this, in device pixels, doesn't show.
const MIN_CIRCLE_PX: f64 = 0.35;
const MAX_CIRCLE_PIECES: usize = 1440;
const DEFAULT_ARROW_HEAD: f64 = 14.0;
/// An arrow head's half width, as a fraction of its length.
const ARROW_HEAD_WIDE: f64 = 0.45;
/// Grid cell size for tracing an implicit curve, in device pixels.
pub const CELL_PX: f64 = 6.0;
/// How far a chord may stray from a parametric curve, in device pixels.
const FLATNESS_PX: f64 = 0.05;
const INITIAL_INTERVALS: usize = 32;
const MAX_DEPTH: usize = 8;
/// A chord still this far from flat at the deepest subdivision isn't the curve bending. The curve
/// has gone through infinity or jumped, so the two ends aren't joined.
const JUMP_PX: f64 = 2.0;
/// How far to chase the point where a curve stops existing: 2⁻²⁴ of a starting interval.
const EDGE_DEPTH: usize = 24;

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

/// A parametric curve's samples. Where its point doesn't exist the curve has a gap, so it
/// can come in pieces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sampled {
    pub pieces: Vec<Vec<[f64; 2]>>,
    /// Whether the point exists across the whole range, as far as sampling can tell.
    pub complete: bool,
}

/// Samples a parametric curve directly, subdividing until chords are flat enough. No
/// field is involved, which is the point of keeping this form as written.
pub fn sample_arc(
    arc: &Arc,
    env: &Env,
    view: &Viewport,
    width: u32,
    height: u32,
) -> Result<Sampled, Error> {
    sample_within(arc, env, view, width, height, false)
}

/// `sample_arc`. With `frame_only`, a piece that's off the frame isn't refined, since nothing
/// there shows. Its length comes out shorter, so that's not for what measures along the line.
fn sample_within(
    arc: &Arc,
    env: &Env,
    view: &Viewport,
    width: u32,
    height: u32,
    frame_only: bool,
) -> Result<Sampled, Error> {
    let (lo, hi) = (
        eval::real(&arc.range[0], env)?,
        eval::real(&arc.range[1], env)?,
    );
    if hi <= lo {
        return Ok(Sampled {
            pieces: Vec::new(),
            complete: true,
        });
    }
    let mut slots = fast::Slots::new();
    let var = slots.variable(&arc.var);
    let x = fast::compile(&arc.point[0], &mut slots, env);
    let y = fast::compile(&arc.point[1], &mut slots, env);
    let mut slow_env = env.clone();
    let mut at = |t: f64| -> Result<Option<[f64; 2]>, Error> {
        if let (Some(x), Some(y)) = (&x, &y) {
            slots.set(var, t);
            match (fast::run(x, &slots), fast::run(y, &slots)) {
                // A point at infinity, such as a pole, isn't a point to draw through.
                (Ok(x), Ok(y)) => return Ok((x.is_finite() && y.is_finite()).then_some([x, y])),
                (Err(fast::Stop::Absent), _) | (_, Err(fast::Stop::Absent)) => return Ok(None),
                // `eval` says what's wrong.
                _ => {}
            }
        }
        slow_env.set(&arc.var, t);
        let point = eval::real(&arc.point[0], &slow_env)
            .and_then(|x| Ok([x, eval::real(&arc.point[1], &slow_env)?]));
        match point {
            Ok(p) => Ok(p.iter().all(|v| v.is_finite()).then_some(p)),
            Err(e) if e.absent => Ok(None),
            Err(e) => Err(e),
        }
    };
    let device = |p: [f64; 2]| frame::device(p[0], p[1], view, width, height);

    struct Span {
        t0: f64,
        p0: Option<[f64; 2]>,
        t1: f64,
        p1: Option<[f64; 2]>,
        depth: usize,
    }
    let first = at(lo)?;
    let mut current: Vec<[f64; 2]> = first.into_iter().collect();
    let mut pieces = Vec::new();
    let mut complete = first.is_some();
    let mut stack = Vec::new();
    let mut previous = first;
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
        if span.p0.is_none() && span.p1.is_none() {
            complete = false;
            continue;
        }
        let tm = (span.t0 + span.t1) / 2.0;
        let pm = at(tm)?;
        let mut jump = false;
        let divide = match (span.p0, span.p1, pm) {
            // Both ends exist, so this is the usual flatness test, unless the middle doesn't.
            (Some(p0), Some(p1), Some(pm)) => {
                let (a, b, m) = (device(p0), device(p1), device(pm));
                let stray = (m.0 - (a.0 + b.0) / 2.0).hypot(m.1 - (a.1 + b.1) / 2.0);
                // All three past the same edge, and by more than the curve bends here.
                let (w, h) = (width as f64, height as f64);
                let off = frame_only
                    && ((a.0.max(b.0).max(m.0) < -stray)
                        || (a.1.max(b.1).max(m.1) < -stray)
                        || (a.0.min(b.0).min(m.0) > w + stray)
                        || (a.1.min(b.1).min(m.1) > h + stray));
                jump = !off && stray > JUMP_PX && span.depth >= MAX_DEPTH;
                !off && stray > FLATNESS_PX && span.depth < MAX_DEPTH
            }
            // One end doesn't exist: close in on where the curve stops.
            _ => span.depth < EDGE_DEPTH,
        };
        if divide {
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
            continue;
        }
        match (span.p0, span.p1) {
            (Some(_), Some(p1)) if jump => {
                pieces.push(std::mem::take(&mut current));
                current = vec![p1];
            }
            (Some(_), Some(p1)) => current.push(p1),
            (Some(_), None) => {
                complete = false;
                pieces.push(std::mem::take(&mut current));
            }
            (None, Some(p1)) => {
                complete = false;
                current = vec![p1];
            }
            (None, None) => {}
        }
    }
    pieces.push(current);
    pieces.retain(|piece| piece.len() >= 2);
    Ok(Sampled { pieces, complete })
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
    frame_only: bool,
) -> Result<Vec<Polyline>, Error> {
    match form {
        // A curve that knows its own path doesn't need tracing.
        Form::Field {
            line: Some(arc), ..
        } => lines(
            &Form::Parametric(arc.clone()),
            env,
            view,
            width,
            height,
            frame_only,
        ),
        Form::Field {
            vars, field, grad, ..
        } => {
            let cols = (width as f64 / CELL_PX).ceil() as u32;
            let rows = (height as f64 / CELL_PX).ceil() as u32;
            trace(vars, field, grad, env, view, cols, rows)
        }
        Form::Parametric(arc) => arc_lines(arc, env, view, width, height, frame_only),
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
                    Part::Arc(arc) => {
                        let sampled = sample_arc(arc, env, view, width, height)?;
                        world.extend(sampled.pieces.into_iter().flatten());
                    }
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
        // An arrow is built in device pixels, where its head has its size.
        Form::Dot(_)
        | Form::Label { .. }
        | Form::Term { .. }
        | Form::Arrow { .. }
        | Form::Outline { .. }
        | Form::Many { .. } => Ok(Vec::new()),
    }
}

/// Builds one segment list per line stroke, in mark order, in device pixels:
/// `x0 y0 x1 y1  r0 r1 s0 s1  dash_on dash_period line_length 0`. A nonzero length
/// clips coverage to the line's arc length, so it ends square as in SVG, even where
/// an earlier segment's round joint would overhang the end.
pub fn build(
    figure: &Figure,
    env: &Env,
    view: &Viewport,
    width: u32,
    height: u32,
    dpr: f64,
    shown: &[bool],
) -> Result<Vec<Vec<f32>>, Error> {
    let mut layers = Vec::new();
    for (i, (mark, style)) in figure
        .scene
        .marks
        .iter()
        .zip(&figure.styles.marks)
        .enumerate()
    {
        if !style.stroke_is_line(&mark.form) {
            continue;
        }
        let mut segments = Vec::new();
        // An absent mark keeps its layer, empty, so the rest stay where the shader expects.
        if !shown[i] {
            layers.push(segments);
            continue;
        }
        let to_device = |p: [f64; 2]| {
            let (x, y) = frame::device(p[0], p[1], view, width, height);
            [x, y]
        };
        if let Form::Many {
            vars,
            lets,
            shape: shape @ Shape::Curves(arc),
        } = &mark.form
        {
            let mut local = env.clone();
            local.set(SELF_SCALE, width as f64 / (view.x[1] - view.x[0]) / dpr);
            let mut count = 0;
            for_each(vars, &mut local, &mut count, &mut |env| {
                if instance(lets, shape, env)?.is_none() {
                    return Ok(());
                }
                // A range that doesn't exist for this one means there's no curve.
                for bound in &arc.range {
                    match eval::real(bound, env) {
                        Ok(_) => {}
                        Err(e) if e.absent => return Ok(()),
                        Err(e) => return Err(e),
                    }
                }
                let Some(arc) = trimmed(arc, style, env)? else {
                    return Ok(());
                };
                push_arc(&arc, style, env, view, width, height, dpr, &mut segments)
            })?;
            layers.push(segments);
            continue;
        }
        if let Form::Many { vars, lets, shape } = &mark.form {
            let mut local = env.clone();
            local.set(SELF_SCALE, width as f64 / (view.x[1] - view.x[0]) / dpr);
            let frame = Target {
                view,
                width,
                height,
                dpr,
            };
            let mut run = FastInstances::new(vars, lets, shape, style, &local);
            let mut count = 0;
            for_each(vars, &mut local, &mut count, &mut |env| {
                push_instance(
                    vars,
                    lets,
                    shape,
                    style,
                    env,
                    &frame,
                    &mut run,
                    &mut segments,
                )
            })?;
            layers.push(segments);
            continue;
        }
        // How much of it to draw, from its start.
        let trim = match &style.trim {
            Some(t) => eval::real(t, env)?.clamp(0.0, 1.0),
            None => 1.0,
        };
        if trim <= 0.0 {
            layers.push(segments);
            continue;
        }
        if let Form::Arrow { from, to } = &mark.form {
            let a = to_device([eval::real(&from[0], env)?, eval::real(&from[1], env)?]);
            let b = to_device([eval::real(&to[0], env)?, eval::real(&to[1], env)?]);
            // It grows from its tail, head first.
            let b = [a[0] + (b[0] - a[0]) * trim, a[1] + (b[1] - a[1]) * trim];
            push_arrow(a, b, style, env, dpr, &mut segments)?;
            layers.push(segments);
            continue;
        }
        if let Form::Outline {
            view: Some(other), ..
        } = &mark.form
        {
            push_outline(other, style, env, view, width, height, dpr, &mut segments)?;
            layers.push(segments);
            continue;
        }
        // A curve that knows its own path, such as a circle drawn as a line, is trimmed along it.
        let path = match &mark.form {
            Form::Parametric(arc) => Some(arc),
            Form::Field {
                line: Some(arc), ..
            } if style.trim.is_some() => Some(arc),
            _ => None,
        };
        if let Some(arc) = path {
            if let Some(arc) = trimmed(arc, style, env)? {
                push_arc(&arc, style, env, view, width, height, dpr, &mut segments)?;
            }
            layers.push(segments);
            continue;
        }
        // Dashes and tapers measure along the line, so they need all of it.
        let frame_only = !style.needs_arc_length();
        for line in lines(&mark.form, env, view, width, height, frame_only)? {
            let line = if trim < 1.0 {
                trim_line(&line, trim)
            } else {
                line
            };
            push_line(&line, style, env, dpr, &to_device, &mut segments)?;
        }
        layers.push(segments);
    }
    Ok(layers)
}

/// The part of a curve a style's `trim` draws: from the start of its parameter, that fraction of
/// the way to its end. Fully drawn, it's the curve as it was, so a loop stays a loop. None if
/// nothing of it is drawn.
fn trimmed(arc: &Arc, style: &MarkStyle, env: &Env) -> Result<Option<Arc>, Error> {
    let Some(trim) = &style.trim else {
        return Ok(Some(arc.clone()));
    };
    let trim = eval::real(trim, env)?.clamp(0.0, 1.0);
    if trim <= 0.0 {
        return Ok(None);
    }
    if trim >= 1.0 {
        return Ok(Some(arc.clone()));
    }
    let lo = eval::real(&arc.range[0], env)?;
    let hi = eval::real(&arc.range[1], env)?;
    Ok(Some(Arc {
        var: arc.var.clone(),
        point: arc.point.clone(),
        range: [Expr::Num(lo), Expr::Num(lo + (hi - lo) * trim)],
    }))
}

/// The first `fraction` of a line by its length. It isn't a loop any more unless it's all there.
fn trim_line(line: &Polyline, fraction: f64) -> Polyline {
    let length = |a: [f64; 2], b: [f64; 2]| (b[0] - a[0]).hypot(b[1] - a[1]);
    let total: f64 = line.points.windows(2).map(|w| length(w[0], w[1])).sum();
    let mut left = total * fraction;
    let mut points = vec![line.points[0]];
    for w in line.points.windows(2) {
        let step = length(w[0], w[1]);
        if step >= left {
            let k = if step > 0.0 { left / step } else { 0.0 };
            points.push([
                w[0][0] + (w[1][0] - w[0][0]) * k,
                w[0][1] + (w[1][1] - w[0][1]) * k,
            ]);
            break;
        }
        left -= step;
        points.push(w[1]);
    }
    Polyline {
        points,
        closed: false,
    }
}

/// A parametric curve as lines, in world units.
fn arc_lines(
    arc: &Arc,
    env: &Env,
    view: &Viewport,
    width: u32,
    height: u32,
    frame_only: bool,
) -> Result<Vec<Polyline>, Error> {
    let close_enough = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9;
    let sampled = sample_within(arc, env, view, width, height, frame_only)?;
    // Only a fixed range makes a loop. A moving range whose ends happen to meet is
    // still an open line for that frame, or its dashes would jump as it passes.
    let fixed = sampled.complete && arc.range.iter().all(is_constant);
    Ok(sampled
        .pieces
        .into_iter()
        .map(|points| {
            let closed = fixed && close_enough(points[0], points[points.len() - 1]);
            Polyline { points, closed }
        })
        .collect())
}

/// One parametric curve's stroke, ending in a head if it has one.
#[allow(clippy::too_many_arguments)]
fn push_arc(
    arc: &Arc,
    style: &MarkStyle,
    env: &Env,
    view: &Viewport,
    width: u32,
    height: u32,
    dpr: f64,
    segments: &mut Vec<f32>,
) -> Result<(), Error> {
    let to_device = |p: [f64; 2]| {
        let (x, y) = frame::device(p[0], p[1], view, width, height);
        [x, y]
    };
    let Some(head) = &style.head else {
        // Dashes and tapers measure along the line, so they need all of it.
        let frame_only = !style.needs_arc_length();
        for line in arc_lines(arc, env, view, width, height, frame_only)? {
            push_line(&line, style, env, dpr, &to_device, segments)?;
        }
        return Ok(());
    };
    let wanted = pixels(head, env, dpr)?;
    for line in arc_lines(arc, env, view, width, height, false)? {
        let points: Vec<[f64; 2]> = line.points.iter().map(|p| to_device(*p)).collect();
        // The head is no longer than 60% of the curve's last stretch, if it's told how
        // long that is, so that it doesn't dwarf a spiral's tightest turns.
        let limit = match &style.head_span {
            Some(span) => {
                let span = eval::real(span, env)?;
                let hi = eval::real(&arc.range[1], env)?;
                let mut local = env.clone();
                local.set(&arc.var, hi - span);
                match (
                    eval::real(&arc.point[0], &local),
                    eval::real(&arc.point[1], &local),
                ) {
                    (Ok(x), Ok(y)) => {
                        let [lx, ly] = to_device([x, y]);
                        let [tx, ty] = points[points.len() - 1];
                        (tx - lx).hypot(ty - ly) * 0.6
                    }
                    _ => wanted,
                }
            }
            None => wanted,
        };
        push_curve_arrow(&points, wanted.min(limit), style, env, dpr, segments)?;
    }
    Ok(())
}

/// The rectangle another panel looks at, kept big enough to read as a marker.
#[allow(clippy::too_many_arguments)]
fn push_outline(
    other: &Viewport,
    style: &MarkStyle,
    env: &Env,
    view: &Viewport,
    width: u32,
    height: u32,
    dpr: f64,
    segments: &mut Vec<f32>,
) -> Result<(), Error> {
    let aspect = match (other.design_width, other.design_height) {
        (Some(w), Some(h)) => h / w,
        _ => 1.0,
    };
    let window = frame::fit_window(other, env, aspect)?;
    let least = match &style.radius {
        Some(r) => pixels(r, env, dpr)?,
        None => 0.0,
    };
    let grow = |a: (f64, f64)| {
        let half = ((a.1 - a.0) / 2.0).max(least);
        let center = (a.0 + a.1) / 2.0;
        (center - half, center + half)
    };
    let corner = |x: f64, y: f64| frame::device(x, y, view, width, height);
    let (x0, y1) = corner(window.x[0], window.y[0]);
    let (x1, y0) = corner(window.x[1], window.y[1]);
    let (x0, x1) = grow((x0, x1));
    let (y0, y1) = grow((y0, y1));
    let line = Polyline {
        points: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]],
        closed: true,
    };
    push_line(&line, style, env, dpr, &|p| p, segments)
}

/// A line in device pixels that ends in a head. The stroke stops inside the head, so its cap
/// doesn't poke past the tip, and the head points along the chord from there, so the stroke
/// ends on its axis even where the curve bends.
fn push_curve_arrow(
    points: &[[f64; 2]],
    head: f64,
    style: &MarkStyle,
    env: &Env,
    dpr: f64,
    segments: &mut Vec<f32>,
) -> Result<(), Error> {
    let Some(&tip) = points.last() else {
        return Ok(());
    };
    let back = head * 0.7;
    let away = |p: [f64; 2]| (p[0] - tip[0]).hypot(p[1] - tip[1]);
    // The last point further from the tip than the stroke's end.
    let Some(outer) = points.iter().rposition(|p| away(*p) > back) else {
        return Ok(());
    };
    let (a, b) = (points[outer], points[outer + 1]);
    let (mut lo, mut hi) = (0.0, 1.0);
    let at = |s: f64| [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s];
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if away(at(mid)) > back {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let end = at(lo);
    let mut stroke: Vec<[f64; 2]> = points[..=outer].to_vec();
    stroke.push(end);
    let line = Polyline {
        points: stroke,
        closed: false,
    };
    push_line(&line, style, env, dpr, &|p| p, segments)?;
    let (dx, dy) = (tip[0] - end[0], tip[1] - end[1]);
    let len = dx.hypot(dy);
    if len <= 0.0 {
        return Ok(());
    }
    let (ux, uy) = (dx / len, dy / len);
    segments.extend_from_slice(&[
        (tip[0] - ux * head) as f32,
        (tip[1] - uy * head) as f32,
        tip[0] as f32,
        tip[1] as f32,
        (head * ARROW_HEAD_WIDE) as f32,
        0.0,
        0.0,
        head as f32,
        0.0,
        0.0,
        head as f32,
        0.0,
    ]);
    Ok(())
}

/// Runs through every combination of a collection's variables, in whole numbers, including
/// both ends of each range. A range that doesn't exist this frame has no instances.
pub fn for_each(
    vars: &[(String, [Expr; 2])],
    env: &mut Env,
    count: &mut usize,
    each: &mut impl FnMut(&mut Env) -> Result<(), Error>,
) -> Result<(), Error> {
    let Some(((var, range), rest)) = vars.split_first() else {
        *count += 1;
        if *count > MANY_LIMIT {
            return Err(Error::new(format!(
                "a collection has over {MANY_LIMIT} instances"
            )));
        }
        return each(env);
    };
    let bound = |e: &Expr, env: &Env| match eval::real(e, env) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.absent => Ok(None),
        Err(e) => Err(e),
    };
    let (Some(lo), Some(hi)) = (bound(&range[0], env)?, bound(&range[1], env)?) else {
        return Ok(());
    };
    let (lo, hi) = (lo.ceil() as i64, hi.floor() as i64);
    for i in lo..=hi {
        env.set(var, i as f64);
        for_each(rest, env, count, each)?;
    }
    Ok(())
}

/// One instance's numbers.
#[derive(Debug, Clone, Copy)]
pub enum Instance {
    Dot([f64; 2]),
    Circle([f64; 2], f64),
    /// `a`, `b` and `c`.
    Cycle([f64; 4]),
    /// A curve has no numbers of its own: it's sampled.
    Curve,
}

impl Instance {
    fn is_finite(&self) -> bool {
        match self {
            Instance::Dot(at) => at.iter().all(|v| v.is_finite()),
            Instance::Circle(at, r) => at.iter().all(|v| v.is_finite()) && r.is_finite(),
            Instance::Cycle(coefficients) => coefficients.iter().all(|v| v.is_finite()),
            Instance::Curve => true,
        }
    }

    fn from_numbers(shape: &Shape, n: &[f64]) -> Instance {
        match shape {
            Shape::Dots(_) => Instance::Dot([n[0], n[1]]),
            Shape::Circles { .. } => Instance::Circle([n[0], n[1]], n[2]),
            Shape::Cycles { .. } => Instance::Cycle([n[0], n[1], n[2], n[3]]),
            Shape::Curves(_) => Instance::Curve,
        }
    }
}

fn shape_exprs(shape: &Shape) -> Vec<&Expr> {
    match shape {
        Shape::Dots(at) => at.iter().collect(),
        Shape::Circles { center, r } => vec![&center[0], &center[1], r],
        Shape::Cycles { a, b, c } => vec![a, &b[0], &b[1], c],
        Shape::Curves(_) => Vec::new(),
    }
}

/// Below this, a circle a view wide is off a straight line by much less than a pixel.
const STRAIGHT_ACROSS_VIEW: f64 = 1e-9;

/// The numbers of one instance, leaving its `with` names in `env` for its style. Nothing if it
/// doesn't exist.
pub fn instance(
    lets: &[(String, Expr)],
    shape: &Shape,
    env: &mut Env,
) -> Result<Option<Instance>, Error> {
    for (name, e) in lets {
        match eval::real(e, env) {
            Ok(value) => env.set(name, value),
            Err(e) if e.absent => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    let env = &*env;
    let mut numbers = Vec::with_capacity(4);
    for e in shape_exprs(shape) {
        match eval::real(e, env) {
            Ok(value) => numbers.push(value),
            Err(e) if e.absent => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    Ok(Some(Instance::from_numbers(shape, &numbers)))
}

/// The instances of a collection with colored sides, each as its circle's coefficients and then
/// what its style reads. One that doesn't exist, or isn't a circle this frame, has no sides.
pub fn sided_instances(
    mark: &crate::ir::Mark,
    group: &crate::Sided,
    env: &Env,
) -> Result<Vec<f32>, Error> {
    let Form::Many { vars, lets, shape } = &mark.form else {
        return Ok(Vec::new());
    };
    let mut local = env.clone();
    let mut out = Vec::new();
    let mut count = 0;
    for_each(vars, &mut local, &mut count, &mut |env| {
        let coefficients = match instance(lets, shape, env)? {
            Some(Instance::Cycle(coefficients)) => coefficients,
            Some(Instance::Circle([x, y], r)) => {
                [1.0 / r, x / r, y / r, (x * x + y * y - r * r) / r]
            }
            _ => return Ok(()),
        };
        if !coefficients.iter().all(|v| v.is_finite()) {
            return Ok(());
        }
        let start = out.len();
        out.extend(coefficients.map(|v| v as f32));
        for name in &group.names {
            out.push(env.get(name).unwrap_or(0.0) as f32);
        }
        out.resize(start + group.stride * 4, 0.0);
        Ok(())
    })?;
    Ok(out)
}

struct Target<'a> {
    view: &'a Viewport,
    width: u32,
    height: u32,
    dpr: f64,
}

/// One collection's expressions, resolved, and which of its sizes depend on the instance.
struct FastInstances {
    slots: fast::Slots,
    vars: Vec<u32>,
    lets: Option<Vec<(u32, fast::Node)>>,
    /// The numbers `Instance::from_numbers` takes, if all of them compile.
    numbers: Option<Vec<fast::Node>>,
    found: Vec<f64>,
    /// A size that's the same for every instance, worked out once.
    radius: Option<f64>,
    half: Option<f64>,
}

impl FastInstances {
    fn new(
        vars: &[(String, [Expr; 2])],
        lets: &[(String, Expr)],
        shape: &Shape,
        style: &MarkStyle,
        env: &Env,
    ) -> Self {
        let mut slots = fast::Slots::new();
        let var_slots = vars.iter().map(|(name, _)| slots.variable(name)).collect();
        let let_slots: Vec<u32> = lets.iter().map(|(name, _)| slots.variable(name)).collect();
        let compiled: Option<Vec<_>> = lets
            .iter()
            .zip(&let_slots)
            .map(|((_, e), slot)| Some((*slot, fast::compile(e, &mut slots, env)?)))
            .collect();
        let numbers: Option<Vec<_>> = shape_exprs(shape)
            .into_iter()
            .map(|e| fast::compile(e, &mut slots, env))
            .collect();
        // Does a size read anything that changes per instance?
        let local = |e: &Expr| {
            let mut found = false;
            e.visit_syms(&mut |name| {
                found |= vars.iter().any(|(v, _)| v == name) || lets.iter().any(|(v, _)| v == name);
            });
            found
        };
        let constant = |e: &Option<Expr>| match e {
            Some(e) if !local(e) => eval::real(e, env).ok(),
            _ => None,
        };
        Self {
            vars: var_slots,
            lets: compiled,
            numbers,
            found: Vec::with_capacity(4),
            radius: constant(&style.radius),
            half: constant(&style.width),
            slots,
        }
    }

    /// The instance's numbers the fast way, or nothing if that's for `eval` to say.
    fn numbers(
        &mut self,
        vars: &[(String, [Expr; 2])],
        shape: &Shape,
        env: &Env,
    ) -> Option<Option<Instance>> {
        let numbers = self.numbers.as_ref()?;
        for ((name, _), slot) in vars.iter().zip(&self.vars) {
            self.slots.set(*slot, env.get(name)?);
        }
        for (slot, node) in self.lets.as_ref()? {
            match fast::run(node, &self.slots) {
                Ok(value) => self.slots.set(*slot, value),
                Err(fast::Stop::Absent) => return Some(None),
                Err(fast::Stop::Unknown) => return None,
            }
        }
        self.found.clear();
        let mut absent = false;
        for node in numbers {
            match fast::run(node, &self.slots) {
                Ok(value) => self.found.push(value),
                Err(fast::Stop::Absent) => absent = true,
                Err(fast::Stop::Unknown) => return None,
            }
        }
        Some((!absent).then(|| Instance::from_numbers(shape, &self.found)))
    }
}

/// Adds one instance, unless it can't be seen: off the frame, or too small to show.
#[allow(clippy::too_many_arguments)]
fn push_instance(
    vars: &[(String, [Expr; 2])],
    lets: &[(String, Expr)],
    shape: &Shape,
    style: &MarkStyle,
    env: &mut Env,
    frame: &Target,
    run: &mut FastInstances,
    segments: &mut Vec<f32>,
) -> Result<(), Error> {
    let numbers = match run.numbers(vars, shape, env) {
        Some(numbers) => numbers,
        None => instance(lets, shape, env)?,
    };
    let Some(numbers) = numbers else {
        return Ok(());
    };
    // At a pole there's no number to draw at, and every comparison below would let it through.
    if !numbers.is_finite() {
        return Ok(());
    }
    let view_width = frame.view.x[1] - frame.view.x[0];
    let (center, radius) = match numbers {
        Instance::Dot(at) => (at, None),
        Instance::Circle(at, r) => (at, Some(r)),
        Instance::Curve => return Ok(()),
        Instance::Cycle([a, bx, by, _]) if (a * view_width).abs() >= STRAIGHT_ACROSS_VIEW => {
            ([bx / a, by / a], Some(1.0 / a.abs()))
        }
        // A line, from the point on it nearest the middle of the view, and long enough to cross it.
        Instance::Cycle([a, bx, by, c]) => {
            let view = frame.view;
            let p = [(view.x[0] + view.x[1]) / 2.0, (view.y[0] + view.y[1]) / 2.0];
            let d = (a * (p[0] * p[0] + p[1] * p[1]) - 2.0 * (bx * p[0] + by * p[1]) + c) / 2.0;
            let foot = [p[0] + d * bx, p[1] + d * by];
            let reach = view_width.hypot(view.y[1] - view.y[0]);
            let end = |s: f64| {
                let (x, y) = frame::device(
                    foot[0] - s * by,
                    foot[1] + s * bx,
                    view,
                    frame.width,
                    frame.height,
                );
                [x, y]
            };
            let line = Polyline {
                points: vec![end(-reach), end(reach)],
                closed: false,
            };
            instance(lets, shape, env)?;
            return push_line(&line, style, env, frame.dpr, &|p| p, segments);
        }
    };
    // A size that reads the instance needs its `with` names, which only `eval` looks at.
    let mut size = |e: &Option<Expr>, known: Option<f64>, default: f64| -> Result<f64, Error> {
        match (known, e) {
            (Some(value), _) => Ok(value * frame.dpr),
            (None, Some(e)) => {
                instance(lets, shape, env)?;
                pixels(e, env, frame.dpr)
            }
            (None, None) => Ok(default * frame.dpr),
        }
    };
    let (w, h) = (frame.width as f64, frame.height as f64);
    let (cx, cy) = frame::device(center[0], center[1], frame.view, frame.width, frame.height);
    match radius {
        None => {
            let r = size(&style.radius, run.radius, frame::DEFAULT_DOT_RADIUS)?;
            if r <= 0.0 || cx < -r || cy < -r || cx > w + r || cy > h + r {
                return Ok(());
            }
            segments.extend_from_slice(&[
                cx as f32, cy as f32, cx as f32, cy as f32, r as f32, r as f32, 0.0, 0.0, 0.0, 0.0,
                0.0, 0.0,
            ]);
        }
        Some(r) => {
            let mut r = r.abs() * w / (frame.view.x[1] - frame.view.x[0]);
            // `radius` caps a circle in pixels, so it can't grow to hide what's around it.
            if style.radius.is_some() {
                r = r.min(size(&style.radius, run.radius, 0.0)?);
            }
            let half = size(&style.width, run.half, DEFAULT_STROKE_WIDTH)? / 2.0;
            if r < MIN_CIRCLE_PX {
                return Ok(());
            }
            let at = |t: f64| [cx + r * t.cos(), cy + r * t.sin()];
            // Enough pieces that a chord strays by no more than the flatness.
            let turn = (1.0 - (FLATNESS_PX / r).min(1.0)).acos() * 2.0;
            if style.dash.is_some() {
                let pieces =
                    ((std::f64::consts::TAU / turn).ceil() as usize).clamp(12, MAX_CIRCLE_PIECES);
                let line = Polyline {
                    points: (0..=pieces)
                        .map(|i| at(std::f64::consts::TAU * i as f64 / pieces as f64))
                        .collect(),
                    closed: true,
                };
                return push_line(&line, style, env, frame.dpr, &|p| p, segments);
            }
            // Only the arc that can be on the frame, as seen from a circle around the frame. A
            // huge circle is nearly all off it, and this keeps what's left to the flatness.
            let (fx, fy) = (w / 2.0 - cx, h / 2.0 - cy);
            let d = fx.hypot(fy);
            let reach = w.hypot(h) / 2.0 + half + 1.0;
            if d > r + reach || r > d + reach {
                return Ok(());
            }
            let (from, span) = if d <= 0.0 {
                (0.0, std::f64::consts::TAU)
            } else {
                let cos = ((d * d + r * r - reach * reach) / (2.0 * d * r)).clamp(-1.0, 1.0);
                let half_span = cos.acos();
                (fy.atan2(fx) - half_span, half_span * 2.0)
            };
            let pieces = ((span / turn).ceil() as usize).max(12);
            let mut a = at(from);
            for i in 1..=pieces {
                let b = at(from + span * i as f64 / pieces as f64);
                let on_frame = a[0].max(b[0]) > -half
                    && a[1].max(b[1]) > -half
                    && a[0].min(b[0]) < w + half
                    && a[1].min(b[1]) < h + half;
                if on_frame {
                    segments.extend_from_slice(&[
                        a[0] as f32,
                        a[1] as f32,
                        b[0] as f32,
                        b[1] as f32,
                        half as f32,
                        half as f32,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                    ]);
                }
                a = b;
            }
        }
    }
    Ok(())
}

/// A shaft up into the head, and the head as one segment tapering to nothing, clipped square
/// at its base: a triangle, in the same coverage as the rest of the line.
fn push_arrow(
    a: [f64; 2],
    b: [f64; 2],
    style: &MarkStyle,
    env: &Env,
    dpr: f64,
    segments: &mut Vec<f32>,
) -> Result<(), Error> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    // Written so a point that isn't a number draws nothing too.
    if len.is_nan() || len <= 1e-3 {
        return Ok(());
    }
    let (ux, uy) = (dx / len, dy / len);
    let wanted = match &style.head {
        Some(h) => pixels(h, env, dpr)?,
        None => DEFAULT_ARROW_HEAD * dpr,
    };
    let head = wanted.min(len / 2.0);
    let shaft = Polyline {
        points: vec![a, [b[0] - ux * head * 0.8, b[1] - uy * head * 0.8]],
        closed: false,
    };
    push_line(&shaft, style, env, dpr, &|p| p, segments)?;
    segments.extend_from_slice(&[
        (b[0] - ux * head) as f32,
        (b[1] - uy * head) as f32,
        b[0] as f32,
        b[1] as f32,
        (head * ARROW_HEAD_WIDE) as f32,
        0.0,
        0.0,
        head as f32,
        0.0,
        0.0,
        head as f32,
        0.0,
    ]);
    Ok(())
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
    // Along the segment, the normal is its own. Only a cap needs the way to this pixel, which near
    // the centerline would be rounding error in the shader's single precision.
    let (nx, ny) = if (along > 0.0 && along < 1.0) || rho < 0.01 {
        if len2 > 1e-12 {
            let len = len2.sqrt();
            (-dy / len, dx / len)
        } else {
            (1.0, 0.0)
        }
    } else {
        (vx / rho, vy / rho)
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

    mod collections {
        const FLATNESS_CHECK: usize = crate::stroke::FLOATS_PER_SEGMENT;
        use crate::build;
        use crate::eval::Frame;
        use crate::frame::{Inputs, prepare};
        use crate::stroke::FLOATS_PER_SEGMENT;

        fn with(n: f64) -> Inputs {
            Inputs {
                time: 0.0,
                params: vec![("n".to_string(), n)],
            }
        }

        const DOTS: &str = "
            viewport { x: 0..10, y: 0..10, width: 100 }
            param n = 3 in 0..9
            dots d = (i, 5) for i in 1..n";

        /// Exactly at the pole, and either side of it, nothing is drawn through infinity.
        #[test]
        fn a_curve_through_a_pole_breaks_there() {
            // x = 1 + 1/t, which runs off to both infinities at t = 0.
            let doc = "viewport { x: -4..4, y: -4..4 }
                   curve c = parametric (t): (1 + 1 / t, t) for t in -3..3";
            let figure = crate::build(doc, &[]).unwrap();
            let env = crate::eval::frame_env(&figure, &crate::frame::Inputs::at(0.0)).unwrap();
            let crate::ir::Form::Parametric(arc) = &figure.scene.marks[0].form else {
                panic!();
            };
            let view =
                crate::frame::view_for(&figure, &crate::frame::Inputs::at(0.0), 400, 400).unwrap();
            let sampled = crate::stroke::sample_arc(arc, &env, &view, 400, 400).unwrap();
            assert_eq!(sampled.pieces.len(), 2, "one each side of the pole");
            for piece in &sampled.pieces {
                assert!(piece.iter().flatten().all(|v| v.is_finite()));
                let side = piece[0][1].signum();
                assert!(
                    piece.iter().all(|p| p[1].signum() == side),
                    "a piece crosses t = 0"
                );
            }
        }

        #[test]
        fn a_collection_is_one_layer_with_a_count_that_follows_its_range() {
            let f = build(DOTS, &["style d { fill: black; radius: 3px }"]).unwrap();
            let count = |n: f64| {
                let p = prepare(&f, &with(n), 100, 100, 1.0).unwrap();
                assert_eq!(p.strokes.len(), 1);
                p.strokes[0].len() / FLOATS_PER_SEGMENT
            };
            // Both ends are included.
            assert_eq!(count(3.0), 3);
            assert_eq!(count(7.0), 7);
            assert_eq!(count(0.0), 0);
        }

        #[test]
        fn dots_cover_a_disc_in_the_color_they_are_filled_with() {
            let f = build(DOTS, &["style d { fill: black; radius: 3px }"]).unwrap();
            let frame = Frame::new(&f, &with(3.0), 100, 100).unwrap();
            // World (2, 5) is pixel (20, 50).
            assert_eq!(frame.shade(20.5, 50.5).unwrap(), [0.0; 3]);
            assert_eq!(frame.shade(20.5, 55.5).unwrap(), [1.0; 3]);
            assert_eq!(frame.shade(5.5, 50.5).unwrap(), [1.0; 3]);
        }

        #[test]
        fn a_size_can_vary_per_instance_and_know_the_scale() {
            // 10 px per world unit, so radius 0.2·i is 2·i px, capped at 5 px.
            let f = build(
                DOTS,
                &["style d { fill: black; radius: min(0.2·i·self.scale·1px, 5px) }"],
            )
            .unwrap();
            let p = prepare(&f, &with(4.0), 100, 100, 1.0).unwrap();
            let radii: Vec<f32> = p.strokes[0]
                .chunks(FLOATS_PER_SEGMENT)
                .map(|s| s[4])
                .collect();
            assert_eq!(radii, [2.0, 4.0, 5.0, 5.0]);
            // That's only for what's worked out per instance.
            let err = build(DOTS, &["style d { fill: black; opacity: i }"]).unwrap_err();
            assert!(err.message.contains("unknown name"), "{err}");
        }

        #[test]
        fn circles_follow_their_radius_within_flatness() {
            let doc = "viewport { x: -2..2, y: -2..2, width: 400 }
                       circles c = circle((0, 0), (1 + i, 0)) for i in 0..0";
            let f = build(doc, &["style c { stroke: black; width: 2px }"]).unwrap();
            let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
            let segments: Vec<&[f32]> = p.strokes[0].chunks(FLOATS_PER_SEGMENT).collect();
            assert!(segments.len() > 40);
            for s in segments {
                let r = (s[0] as f64 - 200.0).hypot(s[1] as f64 - 200.0);
                assert!((r - 100.0).abs() < 1e-3, "{r}");
            }
            let frame = Frame::new(&f, &Inputs::at(0.0), 400, 400).unwrap();
            // On the line, nearly all of the pixel is covered.
            assert!(frame.shade(300.5, 200.5).unwrap()[0] < 0.01);
            assert_eq!(frame.shade(200.5, 200.5).unwrap(), [1.0; 3]);
        }

        #[test]
        fn what_cannot_be_seen_costs_nothing() {
            let doc = "viewport { x: -2..2, y: -2..2, width: 400 }
                       circles far = circle((50, 50), (51, 50)) for i in 0..9
                       circles tiny = circle((0, 0), (0.0001, 0)) for i in 0..9
                       dots away = (50, 50) for i in 0..9";
            let f = build(
                doc,
                &["style far, tiny { stroke: black; width: 1px }
                   style away { fill: black }"],
            )
            .unwrap();
            let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
            assert!(p.strokes.iter().all(Vec::is_empty), "{:?}", p.strokes);
        }

        /// Only the instances that don't exist go, not the whole collection.
        #[test]
        fn an_absent_instance_leaves_a_gap() {
            let doc = "viewport { x: -5..5, y: -5..5, width: 100 }
                       curve c = circle((0, 0), (2, 0))
                       dots d = intersect(c, line((i, 0), (i, 1)), 1) for i in -4..4";
            let f = build(doc, &["style d { fill: black }"]).unwrap();
            let p = prepare(&f, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
            assert_eq!(p.strokes[0].len() / FLOATS_PER_SEGMENT, 5);
        }

        #[test]
        fn a_with_name_is_worked_out_per_instance_and_styles_can_read_it() {
            let doc = "viewport { x: 0..10, y: 0..10, width: 100 }
                       dots d = (z.x, 5) for i in 1..3 with z = (i·2, i), s = z.y";
            let f = build(doc, &["style d { fill: black; radius: s·1px }"]).unwrap();
            let p = prepare(&f, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
            let got: Vec<(f32, f32)> = p.strokes[0]
                .chunks(FLOATS_PER_SEGMENT)
                .map(|s| (s[0], s[4]))
                .collect();
            assert_eq!(got, [(20.0, 1.0), (40.0, 2.0), (60.0, 3.0)]);
        }

        /// A circle far bigger than the frame stays true to the flatness where it's seen.
        #[test]
        fn a_huge_circle_is_only_sampled_where_it_can_be_seen() {
            let doc = "viewport { x: -2..2, y: -2..2, width: 400 }
                       circles c = circle((0, -1000), (0, 0)) for i in 0..0";
            let f = build(doc, &["style c { stroke: black; width: 2px }"]).unwrap();
            let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
            let segments: Vec<&[f32]> = p.strokes[0].chunks(FLATNESS_CHECK).collect();
            // Nearly straight here, so few pieces are enough, where it was over a thousand.
            assert!(
                segments.len() >= 4 && segments.len() < 50,
                "{}",
                segments.len()
            );
            for s in &segments {
                // Mid-chord, it's within the flatness of the true curve.
                let (mx, my) = ((s[0] + s[2]) as f64 / 2.0, (s[1] + s[3]) as f64 / 2.0);
                let r = (mx - 200.0).hypot(my - 100_200.0);
                assert!((r - 100_000.0).abs() < 0.1, "{r}");
            }
            // It crosses the whole frame, along the top of the world's y = 0.
            let frame = Frame::new(&f, &Inputs::at(0.0), 400, 400).unwrap();
            assert!(frame.shade(10.5, 200.5).unwrap()[0] < 0.5);
            assert!(frame.shade(390.5, 200.5).unwrap()[0] < 0.5);
            assert_eq!(frame.shade(200.5, 150.5).unwrap(), [1.0; 3]);
        }

        #[test]
        fn a_radius_caps_a_circle_in_pixels() {
            let doc = "viewport { x: -2..2, y: -2..2, width: 400 }
                       circles c = circle((0, 0), (1, 0)) for i in 0..0";
            let f = build(doc, &["style c { stroke: black; width: 1px; radius: 6px }"]).unwrap();
            let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
            let r = (p.strokes[0][0] as f64 - 200.0).hypot(p.strokes[0][1] as f64 - 200.0);
            assert!((r - 6.0).abs() < 1e-3, "{r}");
        }

        #[test]
        fn a_curve_can_end_in_a_head() {
            let doc = "viewport { x: 0..100, y: 0..100, width: 100 }
                       curve c = parametric (t): (t, 50) for t in 10..90";
            let f = build(doc, &["style c { stroke: black; width: 2px; head: 10px }"]).unwrap();
            let p = prepare(&f, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
            let last = p.strokes[0].chunks(FLOATS_PER_SEGMENT).last().unwrap();
            // The head: from 10 px back, tapering to nothing at the tip.
            assert_eq!((last[0], last[2]), (80.0, 90.0));
            assert_eq!((last[4], last[5]), (4.5, 0.0));
            // The stroke stops inside it.
            let chunks: Vec<_> = p.strokes[0].chunks(FLOATS_PER_SEGMENT).collect();
            let stroke_end = chunks[chunks.len() - 2][2];
            assert!((stroke_end - 83.0).abs() < 1e-3, "{stroke_end}");
            let frame = Frame::new(&f, &Inputs::at(0.0), 100, 100).unwrap();
            assert_eq!(frame.shade(50.5, 50.5).unwrap(), [0.0; 3]);
            assert_eq!(frame.shade(50.5, 55.5).unwrap(), [1.0; 3]);
            assert!(frame.shade(85.5, 50.5).unwrap()[0] < 0.5);
            assert_eq!(frame.shade(85.5, 56.5).unwrap(), [1.0; 3]);
        }

        #[test]
        fn a_head_is_held_to_its_curves_last_stretch() {
            let doc = "viewport { x: 0..100, y: 0..100, width: 100 }
                       curve c = parametric (t): (10 + 80·t / 4, 50) for t in 0..4";
            let f = build(
                doc,
                &["style c { stroke: black; width: 2px; head: 50px; head_span: 1 }"],
            )
            .unwrap();
            let p = prepare(&f, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
            let last = p.strokes[0].chunks(FLOATS_PER_SEGMENT).last().unwrap();
            // One unit of t is 20 px, and 60% of that is 12.
            assert!((last[7] - 12.0).abs() < 1e-3, "{}", last[7]);
        }

        #[test]
        fn a_runaway_range_fails_loudly() {
            let doc = "dots d = (i, 0) for i in 0..9999999";
            let f = build(doc, &["style d { fill: black }"]).unwrap();
            let err = prepare(&f, &Inputs::at(0.0), 10, 10, 1.0).unwrap_err();
            assert!(err.message.contains("instances"), "{err}");
        }

        #[test]
        fn a_collection_is_not_in_a_labels_way() {
            let doc = "viewport { x: -2..2, y: -2..2, width: 400 }
                       dots d = (i, 0) for i in -2..2
                       label n = \"P\" at (0, 0)";
            let f = build(
                doc,
                &["style d { fill: black }
                   style n { place: settle; size: 16px }"],
            )
            .unwrap();
            let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
            assert_eq!((p.labels[0].x, p.labels[0].y), (200.0, 200.0));
        }
    }
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

    /// Where a layer's segments start and end, in device pixels, in order.
    fn ends(s: &[f32]) -> ([f64; 2], [f64; 2]) {
        let first = &s[..FLOATS_PER_SEGMENT];
        let last = &s[s.len() - FLOATS_PER_SEGMENT..];
        (
            [first[0] as f64, first[1] as f64],
            [last[2] as f64, last[3] as f64],
        )
    }

    #[test]
    fn trim_draws_from_a_strokes_start() {
        // A circle starts due east of its center and runs counterclockwise, so a quarter of it
        // ends due north. The view is 4 units across 400 pixels, with y down.
        let layer =
            |sheet: &str, size: u32| layer_of("curve c = circle((0, 0), (1, 0))", sheet, size);
        let quarter = layer("style c { stroke: black; width: 2px; trim: 0.25 }", 400);
        let (start, end) = ends(&quarter);
        assert!(
            (start[0] - 300.0).abs() < 0.5 && (start[1] - 200.0).abs() < 0.5,
            "{start:?}"
        );
        assert!(
            (end[0] - 200.0).abs() < 0.5 && (end[1] - 100.0).abs() < 0.5,
            "{end:?}"
        );
        let length = |s: &[f32]| s[s.len() - FLOATS_PER_SEGMENT + 7] as f64;
        let whole = layer("style c { stroke: black; width: 2px; draw: line }", 400);
        assert!((length(&quarter) / length(&whole) - 0.25).abs() < 0.01);
        // All of it is the circle as it was, and none of it is nothing.
        assert_eq!(
            layer("style c { stroke: black; width: 2px; trim: 1 }", 400),
            whole
        );
        assert!(layer("style c { stroke: black; width: 2px; trim: 0 }", 400).is_empty());
        assert!(layer("style c { stroke: black; width: 2px; trim: -1 }", 400).is_empty());

        // A segment by its length, and an arrow from its tail, with its head at the new tip.
        let doc =
            "viewport { x: 0..4, y: 0..4 }\nsegment s = (0, 2), (4, 2)\narrow a = (0, 1), (4, 1)";
        let figure = build_figure(
            doc,
            &["style s, a { stroke: black; width: 2px; trim: 0.5 }"],
        )
        .unwrap();
        let strokes = prepare(&figure, &Inputs::at(0.0), 400, 400, 1.0)
            .unwrap()
            .strokes;
        let (start, end) = ends(&strokes[0]);
        assert_eq!((start, end), ([0.0, 200.0], [200.0, 200.0]));
        let (_, tip) = ends(&strokes[1]);
        assert_eq!(tip, [200.0, 300.0]);
    }

    #[test]
    fn trim_a_polygon_by_its_length() {
        let line = Polyline {
            points: vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 0.0]],
            closed: true,
        };
        let half = trim_line(&line, 0.4);
        assert!(!half.closed);
        let total = 4.0 + 8f64.sqrt();
        let want = 0.4 * total - 2.0;
        assert_eq!(half.points.len(), 3);
        assert!(
            (half.points[2][1] - want).abs() < 1e-12,
            "{:?}",
            half.points
        );
    }

    #[test]
    fn only_a_stroke_with_a_start_can_be_trimmed() {
        let cases = [
            (
                "let P = (0, 0)\ndot P",
                "style P { fill: black; trim: 0.5 }",
                "no start",
            ),
            (
                "curve c = implicit (x, y): x² + y² = 1",
                "style c { stroke: black; trim: 0.5 }",
                "no start",
            ),
            (
                "region r = (0, 0), (1, 0), (0, 1)",
                "style r { stroke: black; trim: 0.5 }",
                "no start",
            ),
            (
                "circles cs = circle((k, 0), (k + 1, 0)) for k in 0..2",
                "style cs { stroke: black; trim: 0.5 }",
                "no start",
            ),
            (
                "curve c = circle((0, 0), (1, 0))",
                "style c { stroke: black; draw: field; trim: 0.5 }",
                "per pixel",
            ),
        ];
        for (doc, sheet, want) in cases {
            let err = build_figure(doc, &[sheet]).unwrap_err();
            assert!(err.message.contains(want), "{doc}: {err}");
        }
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
