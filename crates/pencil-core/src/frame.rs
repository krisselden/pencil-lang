//! Everything a frame needs that's computed on the CPU: bindings, region boundaries,
//! stroke segments and label positions. The GPU and the oracle both consume this.

use crate::Figure;
use crate::error::Error;
use crate::eval::{self, Env};
use crate::fast;
use crate::ir::{Expr, Form, Part, Viewport};
use crate::nearest;
use crate::place;
use crate::presence;
use crate::stroke;
use crate::style::{Font, MarkStyle, Place};
use crate::terms;

/// Sizes of the uniform arrays in `wgsl.rs`.
pub const VALUE_VEC4S: usize = 64;
pub const GEOMETRY_VEC4S: usize = 512;

/// A label's size and distance when its style doesn't say, in ems: the reader's text size.
pub const DEFAULT_LABEL_SIZE: f64 = 1.0;
const DEFAULT_LABEL_DISTANCE: f64 = 16.0 / 17.0;
pub const DEFAULT_DOT_RADIUS: f64 = 3.0;
pub const DEFAULT_STROKE_WIDTH: f64 = 1.0;
/// How many pieces a curve is cut into to fit a view around it.
const FIT_SAMPLES: usize = 40;
/// How wide the view is in world units, which `show` can read: the zoom level.
pub const VIEW_WIDTH: &str = "view.width";

/// What drives a frame: the clock, and any params a reader has taken over.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inputs {
    pub time: f64,
    pub params: Vec<(String, f64)>,
}

impl Inputs {
    pub fn at(time: f64) -> Self {
        Self {
            time,
            params: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub id: String,
    pub text: String,
    /// The label's center in device pixels.
    pub x: f64,
    pub y: f64,
    pub size: f64,
    /// Linear color.
    pub color: [f64; 3],
    pub halo: f64,
    pub opacity: f64,
    pub font: Font,
    /// The point it labels, in device pixels. `x` and `y` are its offset from this.
    pub anchor: [f64; 2],
    pub place: Place,
    /// A line from near its point to near its text, in its color, where it finally sits.
    pub leader: Option<Leader>,
}

/// A label's leader, in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leader {
    pub width: f64,
    /// How far from the point it stops, so it can meet a ring around it.
    pub gap: f64,
}

/// Where a label's leader runs, given where the label ended up and half its text's width and
/// height: from `gap` off its point to a little short of the text's box, along the line between
/// them. None if there's no room for one.
pub fn leader_ends(label: &Label, half: [f64; 2]) -> Option<[[f64; 2]; 2]> {
    let leader = label.leader?;
    let (dx, dy) = (label.x - label.anchor[0], label.y - label.anchor[1]);
    let apart = dx.hypot(dy);
    if apart <= 0.0 || !apart.is_finite() {
        return None;
    }
    let (ux, uy) = (dx / apart, dy / apart);
    // How far from the text's center its box reaches, back toward the point.
    let to_edge = (half[0] / ux.abs().max(1e-9)).min(half[1] / uy.abs().max(1e-9));
    let end = apart - to_edge - LEADER_MARGIN * label.size;
    if end <= leader.gap {
        return None;
    }
    let at = |d: f64| [label.anchor[0] + ux * d, label.anchor[1] + uy * d];
    Some([at(leader.gap), at(end)])
}

/// How far short of a label's text its leader stops, as a fraction of the text's size.
pub const LEADER_MARGIN: f64 = 0.25;
/// Half a line of text's height, for its box, as a fraction of its size: about half a capital.
pub const TEXT_HALF_HEIGHT: f64 = 0.35;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prepared {
    /// Texels, four floats each, for each collection of `Figure::sided`, at its fixed place.
    /// Empty if there are none.
    pub sides: Vec<f32>,
    /// One per binding, in order, with an absent one as zero, then one per
    /// `Figure::presence_slots`: 1 if that mark exists this frame.
    pub values: Vec<f32>,
    /// One per mark: whether it exists this frame.
    pub shown: Vec<bool>,
    /// A table of `start count _ _` per region, then their vertices in device pixels.
    pub geometry: Vec<f32>,
    /// One segment list per line stroke, in mark order.
    pub strokes: Vec<Vec<f32>>,
    /// The traced curve of each mark whose distance is measured to it, as `nearest::measures`
    /// says, in mark order: segments `x0 y0 x1 y1` in device pixels, empty if it's absent.
    pub traced: Vec<Vec<[f32; 4]>>,
    /// The same, tiled for the GPU, as `nearest::texture`.
    pub nearest: Vec<f32>,
    pub labels: Vec<Label>,
    pub paper: [f64; 3],
    /// Device pixels per `px`, a design pixel.
    pub dpr: f64,
    /// Device pixels per `em`, the reader's text size.
    pub em: f64,
    /// World units per device pixel.
    pub pixel: f64,
    /// This frame's world window, which a `fit` viewport computes.
    pub view: Viewport,
}

/// How many circles in a collection can have colored sides. At 1080² on an M1 Pro, a circle
/// costs about 0.09 ms a frame, so this many is about 23 ms.
pub const MAX_SIDED_CIRCLES: usize = 256;
/// The data texture's width, in texels.
pub const SIDE_TEXTURE_WIDTH: usize = 256;

/// The data texture's height, at least one, so that there's always a texture to bind.
pub fn side_rows(sided: &[crate::Sided]) -> usize {
    let texels: usize = sided
        .iter()
        .map(|group| group.stride * MAX_SIDED_CIRCLES)
        .sum();
    texels.div_ceil(SIDE_TEXTURE_WIDTH).max(1)
}

/// Device pixels per style pixel, for a figure that declares a design width.
pub fn design_scale(figure: &Figure, width: u32) -> Option<f64> {
    figure
        .scene
        .viewport
        .design_width
        .map(|design| width as f64 / design)
}

pub fn device(x: f64, y: f64, view: &Viewport, width: u32, height: u32) -> (f64, f64) {
    (
        (x - view.x[0]) / (view.x[1] - view.x[0]) * width as f64,
        (view.y[1] - y) / (view.y[1] - view.y[0]) * height as f64,
    )
}

/// Puts a frame's units in an env, so that a style's lengths can be worked out there: world
/// units per device pixel, and so per `px` and per `em` as well.
pub fn set_units(env: &mut Env, pixel: f64, dpr: f64, em: f64) {
    env.set(eval::UNIT_DEVICE, pixel);
    env.set(eval::UNIT_PX, pixel * dpr);
    env.set(eval::UNIT_EM, pixel * em);
}

impl Prepared {
    /// Puts this frame's units in an env, as `set_units`.
    pub fn units(&self, env: &mut Env) {
        set_units(env, self.pixel, self.dpr, self.em);
    }
}

/// A style's length, in device pixels. It's worked out in world units, as a bare number is in
/// the figure's own, and `px` and `em` convert.
pub fn length(e: &Expr, env: &Env) -> Result<f64, Error> {
    let pixel = env
        .get(eval::UNIT_DEVICE)
        .ok_or_else(|| Error::new("a length needs a frame to be measured in"))?;
    Ok(eval::real(e, env)? / pixel)
}

/// A style's length, or a default in design pixels, in device pixels.
pub fn length_or(e: &Option<Expr>, px: f64, env: &Env) -> Result<f64, Error> {
    match e {
        Some(e) => length(e, env),
        None => length(&Expr::app("Pixels", vec![Expr::Num(px)]), env),
    }
}

/// A style's length, or a default in ems, in device pixels.
pub fn length_or_em(e: &Option<Expr>, ems: f64, env: &Env) -> Result<f64, Error> {
    match e {
        Some(e) => length(e, env),
        None => length(&Expr::app("Ems", vec![Expr::Num(ems)]), env),
    }
}

/// A dot's radius, in device pixels.
pub fn dot_radius(style: &MarkStyle, env: &Env) -> Result<f64, Error> {
    length_or(&style.radius, DEFAULT_DOT_RADIUS, env)
}

/// A stroke's width, in device pixels.
pub fn stroke_width(style: &MarkStyle, env: &Env) -> Result<f64, Error> {
    length_or(&style.width, DEFAULT_STROKE_WIDTH, env)
}

fn pair(p: &[Expr; 2], env: &Env) -> Result<(f64, f64), Error> {
    Ok((eval::real(&p[0], env)?, eval::real(&p[1], env)?))
}

/// The signed distance from a device pixel to a region's boundary, in pixels, negative
/// inside by nonzero winding, and the direction it grows in.
pub fn polygon_distance(geometry: &[f32], region: usize, px: f64, py: f64) -> (f64, (f64, f64)) {
    let at = |i: usize, k: usize| geometry[i * 4 + k] as f64;
    let start = at(region, 0) as usize;
    let count = at(region, 1) as usize;
    if count < 3 {
        return (1e9, (1.0, 0.0));
    }
    let vertex = |i: usize| (at(start + i, 0), at(start + i, 1));
    let mut best = f64::INFINITY;
    let mut toward = (1.0, 0.0);
    let mut winding = 0;
    let mut a = vertex(count - 1);
    for i in 0..count {
        let b = vertex(i);
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let (wx, wy) = (px - a.0, py - a.1);
        let len2 = ex * ex + ey * ey;
        let h = if len2 > 0.0 {
            ((wx * ex + wy * ey) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (vx, vy) = (wx - ex * h, wy - ey * h);
        let d2 = vx * vx + vy * vy;
        if d2 < best {
            best = d2;
            toward = (vx, vy);
        }
        let cross = ex * wy - ey * wx;
        if a.1 <= py {
            if b.1 > py && cross > 0.0 {
                winding += 1;
            }
        } else if b.1 <= py && cross < 0.0 {
            winding -= 1;
        }
        a = b;
    }
    let rho = best.sqrt();
    let sign = if winding != 0 { -1.0 } else { 1.0 };
    let n = if rho > 1e-6 {
        (sign * toward.0 / rho, sign * toward.1 / rho)
    } else {
        (1.0, 0.0)
    };
    (sign * rho, n)
}

/// The world window for a frame. A `fit` viewport bounds its marks at the values they'd
/// have with no handle dragged, so dragging a point doesn't rescale the figure under the
/// pointer, while other params, such as a slider's, still can.
pub fn view_for(
    figure: &Figure,
    inputs: &Inputs,
    width: u32,
    height: u32,
) -> Result<Viewport, Error> {
    let view = &figure.scene.viewport;
    let aspect = height as f64 / width as f64;
    if view.fit.is_empty() && view.fit_curves.is_empty() && view.fit_families.is_empty() {
        return Ok(equal_scales(view, aspect));
    }
    let handles = &figure.scene.handles;
    let held = Inputs {
        time: inputs.time,
        params: inputs
            .params
            .iter()
            .filter(|(id, _)| !handles.iter().any(|h| h.param == *id))
            .cloned()
            .collect(),
    };
    let env = eval::frame_env(figure, &held)?;
    fit_window(view, &env, aspect)
}

/// A view at this shape, showing what it asks for and more, with one scale for both axes. A figure
/// is never stretched to fit its canvas: distortion is a lot to ask of a reader, and a small one
/// still moves a huge circle by many pixels, since its center and its radius are both scaled.
fn equal_scales(view: &Viewport, aspect: f64) -> Viewport {
    let (wide, tall) = (view.x[1] - view.x[0], view.y[1] - view.y[0]);
    if (tall / wide / aspect - 1.0).abs() < 1e-12 {
        return view.clone();
    }
    let (vw, vh) = if tall / wide > aspect {
        (tall / aspect, tall)
    } else {
        (wide, wide * aspect)
    };
    let (cx, cy) = ((view.x[0] + view.x[1]) / 2.0, (view.y[0] + view.y[1]) / 2.0);
    Viewport {
        x: [cx - vw / 2.0, cx + vw / 2.0],
        y: [cy - vh / 2.0, cy + vh / 2.0],
        ..view.clone()
    }
}

/// The window a `fit` viewport finds, with these bindings, at this shape.
pub fn fit_window(view: &Viewport, env: &Env, aspect: f64) -> Result<Viewport, Error> {
    let (mut x0, mut x1, mut y0, mut y1) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for [x, y, r] in &view.fit {
        // What doesn't exist has no bounds.
        let bounds = (|| {
            Ok::<_, Error>((
                eval::real(x, env)?,
                eval::real(y, env)?,
                eval::real(r, env)?,
            ))
        })();
        let (x, y, r) = match bounds {
            Ok(values) => values,
            Err(e) if e.absent => continue,
            Err(e) => return Err(e),
        };
        x0 = x0.min(x - r);
        x1 = x1.max(x + r);
        y0 = y0.min(y - r);
        y1 = y1.max(y + r);
    }
    let mut bound_arc = |arc: &crate::ir::Arc, env: &Env| {
        let (Ok(lo), Ok(hi)) = (
            eval::real(&arc.range[0], env),
            eval::real(&arc.range[1], env),
        ) else {
            return;
        };
        let mut slots = fast::Slots::new();
        let var = slots.variable(&arc.var);
        let x = fast::compile(&arc.point[0], &mut slots, env);
        let y = fast::compile(&arc.point[1], &mut slots, env);
        let (Some(nx), Some(ny)) = (x, y) else {
            return;
        };
        for i in 0..=FIT_SAMPLES {
            slots.set(var, lo + (hi - lo) * i as f64 / FIT_SAMPLES as f64);
            // What doesn't exist has no bounds.
            if let (Ok(x), Ok(y)) = (fast::run(&nx, &slots), fast::run(&ny, &slots)) {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    };
    for arc in &view.fit_curves {
        bound_arc(arc, env);
    }
    for family in &view.fit_families {
        let mut local = env.clone();
        let mut count = 0;
        stroke::for_each(&family.vars, &mut local, &mut count, &mut |env| {
            for (name, e) in &family.lets {
                match eval::real(e, env) {
                    Ok(value) => env.set(name, value),
                    Err(e) if e.absent => return Ok(()),
                    Err(e) => return Err(e),
                }
            }
            bound_arc(&family.arc, env);
            Ok(())
        })?;
    }
    // With nothing to fit, the declared window stands.
    if x0 > x1 {
        return Ok(equal_scales(view, aspect));
    }
    let wide = (x1 - x0) * (1.0 + 2.0 * view.pad);
    let tall = (y1 - y0) * (1.0 + 2.0 * view.pad);
    let (vw, vh) = if tall / wide.max(1e-12) > aspect {
        (tall / aspect, tall)
    } else {
        (wide, wide * aspect)
    };
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    Ok(Viewport {
        x: [cx - vw / 2.0, cx + vw / 2.0],
        y: [cy - vh / 2.0, cy + vh / 2.0],
        ..view.clone()
    })
}

/// The handle whose mark is nearest a device pixel, within `reach` device pixels.
pub fn handle_at(
    figure: &Figure,
    inputs: &Inputs,
    width: u32,
    height: u32,
    px: f64,
    py: f64,
    reach: f64,
) -> Result<Option<usize>, Error> {
    let env = eval::frame_env(figure, inputs)?;
    let view = view_for(figure, inputs, width, height)?;
    let mut best = None;
    let mut nearest = reach;
    for (i, handle) in figure.scene.handles.iter().enumerate() {
        // A handle whose mark doesn't exist can't be grabbed.
        let (x, y) = match pair(&handle.at, &env) {
            Ok(p) => p,
            Err(e) if e.absent => continue,
            Err(e) => return Err(e),
        };
        let (dx, dy) = device(x, y, &view, width, height);
        let d = (dx - px).hypot(dy - py);
        if d <= nearest {
            nearest = d;
            best = Some(i);
        }
    }
    Ok(best)
}

/// What a drag sets a handle's param to: its explicit inverse at the pointer, kept within
/// the param's range, which is what constrains the drag.
pub fn drag_value(
    figure: &Figure,
    inputs: &Inputs,
    handle: usize,
    width: u32,
    height: u32,
    px: f64,
    py: f64,
) -> Result<(String, f64), Error> {
    let handle = &figure.scene.handles[handle];
    let mut env = eval::frame_env(figure, inputs)?;
    let view = view_for(figure, inputs, width, height)?;
    env.set(
        "pointer.x",
        view.x[0] + px / width as f64 * (view.x[1] - view.x[0]),
    );
    env.set(
        "pointer.y",
        view.y[1] - py / height as f64 * (view.y[1] - view.y[0]),
    );
    let value = match eval::real(&handle.inverse, &env) {
        Ok(value) => value,
        // The pointer is somewhere the inverse has no answer, so the param stays put.
        Err(e) if e.absent => match env.get(&handle.param) {
            Some(value) => value,
            None => return Err(e),
        },
        Err(e) => return Err(e),
    };
    let range = figure
        .scene
        .params
        .iter()
        .find(|p| p.id == handle.param)
        .map_or([f64::NEG_INFINITY, f64::INFINITY], |p| p.range);
    Ok((handle.param.clone(), value.clamp(range[0], range[1])))
}

/// A label's text, with its values in it. A whole number prints as one.
fn filled(text: &str, values: &[Expr], env: &Env) -> Result<String, Error> {
    let mut out = String::new();
    let mut pieces = text.split("{}");
    out.push_str(pieces.next().unwrap_or(""));
    for (value, piece) in values.iter().zip(pieces) {
        let v = eval::real(value, env)?;
        let printed = if (v - v.round()).abs() < 1e-9 {
            format!("{}", v.round() as i64)
        } else {
            format!("{v:.2}")
        };
        out.push_str(&printed.replace('-', "−"));
        out.push_str(piece);
    }
    Ok(out)
}

/// Computes a frame's CPU side, with labels where they settle. For motion that doesn't
/// jump, a caller then applies a spring: `place::spring`, or a page's own over its clock.
/// `dpr` is device pixels per `px`, and an `em` is the sheet's, as in a video.
pub fn prepare(
    figure: &Figure,
    inputs: &Inputs,
    width: u32,
    height: u32,
    dpr: f64,
) -> Result<Prepared, Error> {
    let em = figure.scene.viewport.em * dpr;
    prepare_with(figure, inputs, width, height, dpr, em, &terms::Estimate)
}

/// `prepare`, with a renderer's own text widths, for laying out expressions.
pub fn prepare_with(
    figure: &Figure,
    inputs: &Inputs,
    width: u32,
    height: u32,
    dpr: f64,
    em: f64,
    widths: &dyn terms::TextWidths,
) -> Result<Prepared, Error> {
    let mut env = eval::frame_env(figure, inputs)?;
    let scene = &figure.scene;
    let view_now = view_for(figure, inputs, width, height)?;
    let view = &view_now;
    // The view keeps one scale for both axes.
    let pixel = (view.x[1] - view.x[0]) / width as f64;
    set_units(&mut env, pixel, dpr, em);

    // `show` can ask about the view, and nothing else can.
    let mut seen = env.clone();
    seen.set(VIEW_WIDTH, view.x[1] - view.x[0]);
    let mut shown = Vec::with_capacity(scene.marks.len());
    for ((mark, style), conditional) in scene
        .marks
        .iter()
        .zip(&figure.styles.marks)
        .zip(&figure.conditional)
    {
        shown.push(!conditional || presence::present(mark, style, &seen)?);
    }

    let mut values = scene
        .bindings
        .iter()
        .map(|b| env.get(&b.id).unwrap_or(0.0) as f32)
        .collect::<Vec<_>>();

    let mut regions: Vec<Vec<[f64; 2]>> = Vec::new();
    for (i, mark) in scene.marks.iter().enumerate() {
        if let Form::Region(parts) = &mark.form {
            let mut vertices = Vec::new();
            if shown[i] {
                for part in parts {
                    match part {
                        Part::Point(p) => {
                            let (x, y) = pair(p, &env)?;
                            let (dx, dy) = device(x, y, view, width, height);
                            vertices.push([dx, dy]);
                        }
                        Part::Arc(arc) => {
                            let sampled = stroke::sample_arc(arc, &env, view, width, height)?;
                            // A boundary with a gap in it doesn't enclose anything.
                            shown[i] &= sampled.complete;
                            for p in sampled.pieces.into_iter().flatten() {
                                let (dx, dy) = device(p[0], p[1], view, width, height);
                                vertices.push([dx, dy]);
                            }
                        }
                    }
                }
            }
            if !shown[i] {
                vertices.clear();
            }
            regions.push(vertices);
        }
    }
    // After the regions, which only find out they have a gap by sampling.
    values.extend(
        figure
            .presence_slots()
            .map(|(mark, _)| f32::from(u8::from(shown[mark]))),
    );
    // Every pixel adds up every circle, so this grows with both.
    let sided = figure.sided();
    let mut sides = vec![0.0; side_rows(&sided) * SIDE_TEXTURE_WIDTH * 4];
    if sided.is_empty() {
        sides.clear();
    }
    for group in &sided {
        let mark = &scene.marks[group.mark];
        let found = stroke::sided_instances(mark, group, &env)?;
        let count = found.len() / (group.stride * 4);
        if count > MAX_SIDED_CIRCLES {
            return Err(Error::new(format!(
                "`{}` has {count} circles with sides, and the limit is {MAX_SIDED_CIRCLES}",
                mark.id
            )));
        }
        sides[group.base * 4..][..found.len()].copy_from_slice(&found);
        values.push(count as f32);
    }
    if values.len() > VALUE_VEC4S * 4 {
        return Err(Error::new(format!(
            "too many bindings and vanishing marks: {}, and the limit is {} for now",
            values.len(),
            VALUE_VEC4S * 4
        )));
    }

    let total: usize = regions.iter().map(Vec::len).sum::<usize>() + regions.len();
    if total > GEOMETRY_VEC4S {
        return Err(Error::new(format!(
            "regions need {total} vertices, and the limit is {GEOMETRY_VEC4S} for now"
        )));
    }
    let mut geometry = vec![0.0f32; GEOMETRY_VEC4S * 4];
    let mut start = regions.len();
    for (k, vertices) in regions.iter().enumerate() {
        geometry[k * 4] = start as f32;
        geometry[k * 4 + 1] = vertices.len() as f32;
        for (i, v) in vertices.iter().enumerate() {
            geometry[(start + i) * 4] = v[0] as f32;
            geometry[(start + i) * 4 + 1] = v[1] as f32;
        }
        start += vertices.len();
    }

    // A curve whose distance is measured is traced once, for that and for its stroke.
    let mut curves = vec![None; scene.marks.len()];
    let mut traced = Vec::new();
    for (i, (mark, style)) in scene.marks.iter().zip(&figure.styles.marks).enumerate() {
        if !nearest::measures(mark, style) {
            continue;
        }
        let mut segments = Vec::new();
        if shown[i] {
            // Its stroke shares the trace, so it reaches as far as the stroke needs.
            let reach = if style.stroke_is_line(&mark.form) {
                stroke::half_width(style, &env, dpr)?
            } else {
                0.0
            };
            let lines = stroke::lines(&mark.form, &env, view, width, height, true, reach)?;
            for line in &lines {
                for pair in line.points.windows(2) {
                    let (ax, ay) = device(pair[0][0], pair[0][1], view, width, height);
                    let (bx, by) = device(pair[1][0], pair[1][1], view, width, height);
                    let segment = [ax as f32, ay as f32, bx as f32, by as f32];
                    if segment.iter().all(|v| v.is_finite()) {
                        segments.push(segment);
                    }
                }
            }
            curves[i] = Some(lines);
        }
        traced.push(segments);
    }
    let strokes = stroke::build(figure, &env, view, width, height, dpr, &shown, &curves)?;
    let nearest = nearest::texture(&traced, width, height);
    if nearest.len() > nearest::TEXTURE_WIDTH * nearest::TEXTURE_ROWS * 4 {
        return Err(Error::new(format!(
            "measuring distances to curves needs {} texels, and the limit is {} for now",
            nearest.len() / 4,
            nearest::TEXTURE_WIDTH * nearest::TEXTURE_ROWS
        )));
    }
    let paper = match &figure.styles.paper {
        Some(paper) => eval::color(paper, &env)?,
        None => [1.0; 3],
    };

    let mut labels = Vec::new();
    for (i, (mark, style)) in scene.marks.iter().zip(&figure.styles.marks).enumerate() {
        let Form::Label {
            text,
            values,
            at,
            offset,
            away,
        } = &mark.form
        else {
            continue;
        };
        if !shown[i] {
            continue;
        }
        let (x, y) = pair(at, &env)?;
        let (dx, dy) = device(x, y, view, width, height);
        let (ox, oy) = match away {
            // An offset is in design pixels.
            None => {
                let (ox, oy) = pair(offset, &env)?;
                (ox * dpr, oy * dpr)
            }
            // The document says which way, in its own coordinates, and the style how far.
            Some(direction) => {
                let (ax, ay) = pair(direction, &env)?;
                let (sx, sy) = device(x + ax, y + ay, view, width, height);
                let (along_x, along_y) = (sx - dx, sy - dy);
                let length = along_x.hypot(along_y);
                let distance = length_or_em(&style.distance, DEFAULT_LABEL_DISTANCE, &env)?;
                if length > 0.0 {
                    (along_x / length * distance, along_y / length * distance)
                } else {
                    (0.0, 0.0)
                }
            }
        };
        labels.push(Label {
            id: mark.id.clone(),
            text: filled(text, values, &env)?,
            x: dx + ox,
            y: dy + oy,
            size: length_or_em(&style.size, DEFAULT_LABEL_SIZE, &env)?,
            color: match &style.negative {
                Some(c) => eval::color(c, &env)?,
                None => [0.0; 3],
            },
            halo: length_or(&style.halo, 0.0, &env)?,
            opacity: match &style.opacity {
                Some(o) => eval::real(o, &env)?.clamp(0.0, 1.0),
                None => 1.0,
            },
            font: style.font,
            anchor: [dx, dy],
            place: style.place,
            leader: match &style.leader {
                Some(w) => Some(Leader {
                    width: length(w, &env)?,
                    gap: length_or(&style.leader_gap, 0.0, &env)?,
                }),
                None => None,
            },
        });
    }

    labels.extend(terms::labels(
        figure, &env, view, width, height, dpr, &shown, widths,
    )?);

    let mut prepared = Prepared {
        sides,
        values,
        shown,
        geometry,
        strokes,
        traced,
        nearest,
        labels,
        paper,
        dpr,
        em,
        pixel,
        view: view_now,
    };
    place::settle(figure, &env, &mut prepared, width, height)?;
    Ok(prepared)
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_leader_runs_from_its_gap_to_short_of_its_text() {
        let figure = crate::build(
            "viewport { x: -2..2, y: -2..2 } style figure { width: 400px }\nlabel n = \"P\" at (0, 0) away (1, 0)",
            &["style n { size: 20px; distance: 60px; leader: 1px; leader_gap: 8px; place: fixed }"],
        )
        .unwrap();
        let labels = prepare(&figure, &Inputs::at(0.0), 400, 400, 1.0)
            .unwrap()
            .labels;
        let label = &labels[0];
        assert_eq!(
            label.leader,
            Some(Leader {
                width: 1.0,
                gap: 8.0
            })
        );
        // Along +x from (200, 200): 8 px out, to 60 − 10 (half the text's width) − 5 (a quarter
        // of its size).
        let ends = leader_ends(label, [10.0, 7.0]).unwrap();
        assert_eq!(ends, [[208.0, 200.0], [245.0, 200.0]]);
        // Straight up, the text's height decides where it stops.
        let up = Label {
            x: 200.0,
            y: 140.0,
            ..label.clone()
        };
        assert_eq!(leader_ends(&up, [10.0, 7.0]).unwrap()[1], [200.0, 152.0]);
        // Too close for one, and none asked for.
        let near = Label {
            x: 215.0,
            ..label.clone()
        };
        assert!(leader_ends(&near, [10.0, 7.0]).is_none());
        assert!(
            leader_ends(
                &Label {
                    leader: None,
                    ..label.clone()
                },
                [10.0, 7.0]
            )
            .is_none()
        );
        let err =
            crate::build("segment s = (0, 0), (1, 0)", &["style s { leader: 1px }"]).unwrap_err();
        assert!(err.message.contains("isn't a label"), "{err}");
    }
    use super::*;
    use crate::build;

    #[test]
    fn a_label_sits_away_from_its_point_at_the_distance_its_style_sets() {
        let doc = "viewport { x: -2..2, y: -2..2 }
                   label n = \"P\" at (0, 0) away (3, 4)
                   label m = \"Q\" at (0, 0) away (0, 0)";
        let at = |sheet: &str| {
            let figure = crate::build(doc, &[sheet]).unwrap();
            let prepared = prepare(&figure, &Inputs::default(), 400, 400, 1.0).unwrap();
            let n = prepared
                .labels
                .iter()
                .find(|l| l.id == "n")
                .unwrap()
                .clone();
            let m = prepared
                .labels
                .iter()
                .find(|l| l.id == "m")
                .unwrap()
                .clone();
            (
                n.x - n.anchor[0],
                n.y - n.anchor[1],
                m.x - m.anchor[0],
                m.y - m.anchor[1],
            )
        };
        // Up and to the right on the screen, so y goes down. No direction means no move.
        let (x, y, mx, my) = at("style label { distance: 10px }");
        assert!((x - 6.0).abs() < 1e-9 && (y + 8.0).abs() < 1e-9, "{x} {y}");
        assert_eq!((mx, my), (0.0, 0.0));
        let (x, y, ..) = at("style label { distance: 20px }");
        assert!(
            (x - 12.0).abs() < 1e-9 && (y + 16.0).abs() < 1e-9,
            "{x} {y}"
        );
    }

    #[test]
    fn a_label_takes_offset_or_away() {
        let err =
            crate::parse::parse("label n = \"P\" at (0, 0) away (1, 0) offset (2, 3)").unwrap_err();
        assert!(err.message.contains("not both"), "{err}");
    }

    #[test]
    fn labels_resolve_position_offset_and_style() {
        let doc = "viewport { x: 0..4, y: 0..4 }
                   label n = \"P\" at (1, 3) offset (2, -3)";
        let sheet = "style n { fill: white; size: 10px; halo: 2px; opacity: 0.5 }";
        let f = build(doc, &[sheet]).unwrap();
        let p = prepare(&f, &Inputs::at(0.0), 400, 400, 2.0).unwrap();
        let label = &p.labels[0];
        assert_eq!(label.text, "P");
        assert_eq!((label.x, label.y), (104.0, 94.0));
        assert_eq!((label.size, label.halo, label.opacity), (20.0, 4.0, 0.5));
        assert_eq!(label.color, [1.0; 3]);
    }

    #[test]
    fn a_design_width_scales_sizes_with_the_output() {
        let doc = "viewport { x: 0..4, y: 0..4 } style figure { width: 200px }
                   label n = \"P\" at (2, 2)";
        let f = build(doc, &["style n { size: 10px }"]).unwrap();
        let scale = design_scale(&f, 400).unwrap();
        assert_eq!(scale, 2.0);
        let p = prepare(&f, &Inputs::at(0.0), 400, 400, scale).unwrap();
        assert_eq!(p.labels[0].size, 20.0);
        assert_eq!(p.dpr, 2.0);
    }

    #[test]
    fn a_sheet_sizes_the_figure_and_another_sheet_can_size_it_again() {
        let doc = "viewport { x: 0..4, y: 0..2 }
                   let P = (1, 1)
                   dot P";
        let size = |sheets: &[&str]| crate::compose(doc, sheets).unwrap().size;
        // Without a size, the shape is the window's.
        assert_eq!(size(&[]), [4.0, 2.0]);
        let page = "style figure { width: 640px; height: 320px }";
        let video = "style figure { width: 1280px; height: 720px }";
        assert_eq!(size(&[page]), [640.0, 320.0]);
        assert_eq!(size(&[page, video]), [1280.0, 720.0]);
        assert_eq!(design_scale(&build(doc, &[video]).unwrap(), 640), Some(0.5));

        let err = |sheet: &str| build(doc, &[sheet]).unwrap_err().message;
        assert!(err("style figure { width: 640 }").contains("needs a unit"));
        assert!(err("style figure { pad: 0.1 }").contains("not `pad`"));
        let panels = "panel a { viewport { x: 0..1, y: 0..1 } }
                      panel b { viewport { x: 0..1, y: 0..1 } }";
        let err = crate::compose(panels, &["style figure { width: 300px }"]).unwrap_err();
        assert!(err.message.contains("`style panel`"), "{err}");
    }

    #[test]
    fn a_length_is_in_the_figure_s_units_unless_it_says_px_or_em() {
        let doc = "viewport { x: 0..4, y: 0..4 }
                   let P = (2, 2)
                   dot P
                   label n = \"P\" at P
                   curve c = circle(P, (3, 2))";
        let sheet = "style figure { width: 200px; em: 10px }";
        let f = build(doc, &[sheet]).unwrap();
        // 400 device pixels across 4 units, and drawn at twice its design width.
        let p = prepare(&f, &Inputs::at(0.0), 400, 400, 2.0).unwrap();
        assert_eq!((p.pixel, p.dpr, p.em), (0.01, 2.0, 20.0));
        let mut env = crate::eval::Env::new(0.0);
        p.units(&mut env);
        let device = |text: &str| {
            let f = build(doc, &[sheet, &format!("style c {{ width: {text} }}")]).unwrap();
            let c = f.scene.marks.iter().position(|m| m.id == "c").unwrap();
            (stroke_width(&f.styles.marks[c], &env).unwrap() * 1e9).round() / 1e9
        };
        assert_eq!(device("0.1"), 10.0);
        assert_eq!(device("2px"), 4.0);
        assert_eq!(device("1.5em"), 30.0);
        assert_eq!(device("0.05 + 1px"), 7.0);
        assert_eq!(device("2·(1em - 1px)"), 36.0);

        // A label's size in ems follows the reader's text size, and not the figure's.
        let sized = |em: f64| {
            let f = build(doc, &[sheet, "style n { size: 1em }"]).unwrap();
            prepare_with(&f, &Inputs::at(0.0), 400, 400, 2.0, em, &terms::Estimate)
                .unwrap()
                .labels
                .iter()
                .find(|l| l.id == "n")
                .unwrap()
                .size
        };
        assert_eq!(sized(32.0), 32.0);
        assert_eq!(sized(24.0), 24.0);

        // A figure's own size can't be in ems: they're the reader's, and the figure is the design.
        let err = build(doc, &["style figure { width: 40em }"]).unwrap_err();
        assert!(err.message.contains("can't size a figure"), "{err}");
    }

    const DRAG: &str = "
        viewport { x: -2..2, y: -2..2 }
        param a = 1 in 0.2..1.85
        let P = (cos(a), sin(a))
        dot P
        handle P sets a = atan2(max(pointer.y, 0), pointer.x)";

    #[test]
    fn a_drag_follows_the_inverse_within_the_range() {
        let f = build(DRAG, &[]).unwrap();
        let drag =
            |px: f64, py: f64| drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap();
        // At 45°: world (1, 1) is pixel (300, 100).
        assert_eq!(drag(300.0, 100.0).0, "a");
        assert!((drag(300.0, 100.0).1 - std::f64::consts::FRAC_PI_4).abs() < 1e-9);
        // Past either end, the range holds it.
        assert_eq!(drag(399.0, 199.0).1, 0.2);
        assert_eq!(drag(1.0, 201.0).1, 1.85);
    }

    #[test]
    fn handles_are_found_near_their_mark() {
        let f = build(DRAG, &[]).unwrap();
        let (x, y) = device(1f64.cos(), 1f64.sin(), &f.scene.viewport, 400, 400);
        let at = |px, py| handle_at(&f, &Inputs::at(0.0), 400, 400, px, py, 10.0).unwrap();
        assert_eq!(at(x + 5.0, y), Some(0));
        assert_eq!(at(x + 30.0, y), None);
    }

    const FITTED: &str = "
        viewport { fit: c, pad: 0 }
        param a = 1 in 0..3
        param b = 1 in 0..3
        let P = (a * b, 0)
        curve c = circle((0, 0), P)
        dot P
        handle P sets a = pointer.x";

    #[test]
    fn a_fit_view_ignores_dragged_params_but_follows_others() {
        let f = build(FITTED, &[]).unwrap();
        let with = |params: &[(&str, f64)]| {
            let inputs = Inputs {
                time: 0.0,
                params: params.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
            };
            view_for(&f, &inputs, 100, 100).unwrap().x
        };
        assert_eq!(with(&[]), [-1.0, 1.0]);
        // `a` has a handle, so dragging it holds the view.
        assert_eq!(with(&[("a", 2.0)]), [-1.0, 1.0]);
        // `b` is a plain param, so the view fits it.
        assert_eq!(with(&[("b", 2.0)]), [-2.0, 2.0]);
    }

    #[test]
    fn a_fit_view_keeps_the_frames_shape() {
        let f = build(FITTED, &[]).unwrap();
        let v = view_for(&f, &Inputs::at(0.0), 200, 100).unwrap();
        assert_eq!((v.x, v.y), ([-2.0, 2.0], [-1.0, 1.0]));
    }

    #[test]
    fn too_many_vertices_fail_loudly() {
        let parts: Vec<String> = (0..600).map(|i| format!("({i}, 0)")).collect();
        let doc = format!("region r = {}", parts.join(", "));
        let f = build(&doc, &[]).unwrap();
        let err = prepare(&f, &Inputs::at(0.0), 100, 100, 1.0).unwrap_err();
        assert!(err.message.contains("the limit is"), "{err}");
    }

    #[test]
    fn polygon_distance_is_exact_and_signed() {
        let doc = "viewport { x: 0..100, y: 0..100 }
                   region r = (10, 10), (30, 10), (30, 30), (10, 30)";
        let f = build(doc, &[]).unwrap();
        let p = prepare(&f, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
        // In device pixels, y is flipped: the square spans y 70 to 90.
        let (d, _) = polygon_distance(&p.geometry, 0, 20.0, 80.0);
        assert_eq!(d, -10.0);
        let (d, n) = polygon_distance(&p.geometry, 0, 40.0, 80.0);
        assert_eq!(d, 10.0);
        assert_eq!(n, (1.0, 0.0));
    }

    /// Both orientations of a region fill the same: winding is nonzero either way.
    #[test]
    fn region_winding_is_orientation_independent() {
        let doc = "viewport { x: 0..100, y: 0..100 }
                   region r = (10, 30), (30, 30), (30, 10), (10, 10)";
        let f = build(doc, &[]).unwrap();
        let p = prepare(&f, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
        assert!(polygon_distance(&p.geometry, 0, 20.0, 80.0).0 < 0.0);
    }
}
