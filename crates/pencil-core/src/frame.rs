//! Everything a frame needs that's computed on the CPU: bindings, region boundaries,
//! stroke segments and label positions. The GPU and the oracle both consume this.

use crate::Figure;
use crate::error::Error;
use crate::eval::{self, Env};
use crate::ir::{Expr, Form, Part, Viewport};
use crate::stroke;
use crate::style::{Font, MarkStyle};

/// Sizes of the uniform arrays in `wgsl.rs`.
pub const VALUE_VEC4S: usize = 64;
pub const GEOMETRY_VEC4S: usize = 512;

pub const DEFAULT_LABEL_SIZE: f64 = 17.0;
pub const DEFAULT_DOT_RADIUS: f64 = 3.0;
pub const DEFAULT_STROKE_WIDTH: f64 = 1.0;

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
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prepared {
    /// One per binding, in order.
    pub values: Vec<f32>,
    /// A table of `start count _ _` per region, then their vertices in device pixels.
    pub geometry: Vec<f32>,
    /// One segment list per line stroke, in mark order.
    pub strokes: Vec<Vec<f32>>,
    pub labels: Vec<Label>,
    pub paper: [f64; 3],
    /// Device pixels per style pixel.
    pub dpr: f64,
    /// This frame's world window, which a `fit` viewport computes.
    pub view: Viewport,
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

pub fn dot_radius(style: &MarkStyle, env: &Env) -> Result<f64, Error> {
    match &style.radius {
        Some(r) => eval::real(r, env),
        None => Ok(DEFAULT_DOT_RADIUS),
    }
}

pub fn stroke_width(style: &MarkStyle, env: &Env) -> Result<f64, Error> {
    match &style.width {
        Some(w) => eval::real(w, env),
        None => Ok(DEFAULT_STROKE_WIDTH),
    }
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
    if view.fit.is_empty() {
        return Ok(view.clone());
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
    let (mut x0, mut x1, mut y0, mut y1) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for [x, y, r] in &view.fit {
        let (x, y, r) = (
            eval::real(x, &env)?,
            eval::real(y, &env)?,
            eval::real(r, &env)?,
        );
        x0 = x0.min(x - r);
        x1 = x1.max(x + r);
        y0 = y0.min(y - r);
        y1 = y1.max(y + r);
    }
    let aspect = height as f64 / width as f64;
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
        let (x, y) = pair(&handle.at, &env)?;
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
    let value = eval::real(&handle.inverse, &env)?;
    let range = figure
        .scene
        .params
        .iter()
        .find(|p| p.id == handle.param)
        .map_or([f64::NEG_INFINITY, f64::INFINITY], |p| p.range);
    Ok((handle.param.clone(), value.clamp(range[0], range[1])))
}

/// Computes a frame's CPU side. `dpr` is device pixels per style pixel.
pub fn prepare(
    figure: &Figure,
    inputs: &Inputs,
    width: u32,
    height: u32,
    dpr: f64,
) -> Result<Prepared, Error> {
    let env = eval::frame_env(figure, inputs)?;
    let scene = &figure.scene;
    let view_now = view_for(figure, inputs, width, height)?;
    let view = &view_now;

    let values = scene
        .bindings
        .iter()
        .map(|b| env.get(&b.id).unwrap_or(0.0) as f32)
        .collect::<Vec<_>>();
    if values.len() > VALUE_VEC4S * 4 {
        return Err(Error::new(format!(
            "too many bindings: {}, and the limit is {} for now",
            values.len(),
            VALUE_VEC4S * 4
        )));
    }

    let mut regions: Vec<Vec<[f64; 2]>> = Vec::new();
    for mark in &scene.marks {
        if let Form::Region(parts) = &mark.form {
            let mut vertices = Vec::new();
            for part in parts {
                match part {
                    Part::Point(p) => {
                        let (x, y) = pair(p, &env)?;
                        let (dx, dy) = device(x, y, view, width, height);
                        vertices.push([dx, dy]);
                    }
                    Part::Arc(arc) => {
                        for p in stroke::sample_arc(arc, &env, view, width, height)? {
                            let (dx, dy) = device(p[0], p[1], view, width, height);
                            vertices.push([dx, dy]);
                        }
                    }
                }
            }
            regions.push(vertices);
        }
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

    let strokes = stroke::build(figure, &env, view, width, height, dpr)?;
    let paper = match &figure.styles.paper {
        Some(paper) => eval::color(paper, &env)?,
        None => [1.0; 3],
    };

    let mut labels = Vec::new();
    for (mark, style) in scene.marks.iter().zip(&figure.styles.marks) {
        let Form::Label { text, at, offset } = &mark.form else {
            continue;
        };
        let (x, y) = pair(at, &env)?;
        let (dx, dy) = device(x, y, view, width, height);
        let (ox, oy) = pair(offset, &env)?;
        labels.push(Label {
            id: mark.id.clone(),
            text: text.clone(),
            x: dx + ox * dpr,
            y: dy + oy * dpr,
            size: match &style.size {
                Some(s) => eval::real(s, &env)?,
                None => DEFAULT_LABEL_SIZE,
            } * dpr,
            color: match &style.negative {
                Some(c) => eval::color(c, &env)?,
                None => [0.0; 3],
            },
            halo: match &style.halo {
                Some(h) => eval::real(h, &env)?,
                None => 0.0,
            } * dpr,
            opacity: match &style.opacity {
                Some(o) => eval::real(o, &env)?.clamp(0.0, 1.0),
                None => 1.0,
            },
            font: style.font,
        });
    }

    Ok(Prepared {
        values,
        geometry,
        strokes,
        labels,
        paper,
        dpr,
        view: view_now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;

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
        let doc = "viewport { x: 0..4, y: 0..4, width: 200 }
                   label n = \"P\" at (2, 2)";
        let f = build(doc, &["style n { size: 10px }"]).unwrap();
        let scale = design_scale(&f, 400).unwrap();
        assert_eq!(scale, 2.0);
        let p = prepare(&f, &Inputs::at(0.0), 400, 400, scale).unwrap();
        assert_eq!(p.labels[0].size, 20.0);
        assert_eq!(p.dpr, 2.0);
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
