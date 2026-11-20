//! Everything a frame needs that's computed on the CPU: bindings, region boundaries,
//! stroke segments and label positions. The GPU and the oracle both consume this.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::Figure;
use crate::error::Error;
use crate::eval::{self, Env};
use crate::fast;
use crate::formula;
use crate::ir::{Expr, Form, Part, Shown, Subject, Viewport};
use crate::nearest;
use crate::place;
use crate::presence;
use crate::stroke;
use crate::style::{Font, MarkStyle, Place, TextPart};
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
    /// Its text laid out, if it has mathematics in it. `text` then still has the `$` around
    /// each span, and `words` is how it reads.
    pub laid: Option<formula::Laid>,
}

impl Label {
    /// Half the width and height of its text's box: what was laid out, or a line of text as
    /// wide as the renderer's font sets it.
    pub fn half(&self, widths: &dyn terms::TextWidths) -> [f64; 2] {
        let line = self.size * TEXT_HALF_HEIGHT;
        match &self.laid {
            Some(laid) => [laid.width / 2.0, line.max(laid.above).max(laid.below)],
            None => [widths.width(&self.text, self.size, self.font) / 2.0, line],
        }
    }

    /// Its text as it reads, without the marks around its mathematics.
    pub fn words(&self) -> String {
        match formula::has_math(&self.text) {
            true => self
                .text
                .replace("$$", "\u{0}")
                .replace('$', "")
                .replace('\u{0}', "$"),
            false => self.text.clone(),
        }
    }
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
            .filter(|(id, _)| !handles.iter().any(|h| h.sets(id)))
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

/// Where each handle's mark is, in device pixels, for a page to put something a reader can focus
/// there. A handle whose mark doesn't exist this frame has no place.
pub fn handle_places(
    figure: &Figure,
    inputs: &Inputs,
    width: u32,
    height: u32,
) -> Result<Vec<Option<(f64, f64)>>, Error> {
    if figure.scene.handles.is_empty() {
        return Ok(Vec::new());
    }
    let env = eval::frame_env(figure, inputs)?;
    let view = view_for(figure, inputs, width, height)?;
    figure
        .scene
        .handles
        .iter()
        .map(|handle| match pair(&handle.at, &env) {
            Ok((x, y)) => Ok(Some(device(x, y, &view, width, height))),
            Err(e) if e.absent => Ok(None),
            Err(e) => Err(e),
        })
        .collect()
}

/// What a key sets a handle's params to: its inverses at where the mark is, moved by this many
/// device pixels, as a drag from the mark's own place would. `None` for a handle whose mark
/// doesn't exist this frame.
///
/// A param with steps might not reach its next value in one move, and a key that did nothing
/// would leave a reader stuck. So the move is doubled until it's one the key can make, or until
/// it has gone as far as the canvas is wide, where the direction has no next value.
///
/// A handle that sets one param moves its mark along one path, so any move with some of the
/// key's direction in it will do, or a stepped param reaching its next value. A handle that sets
/// two can go any way on the screen, and its move has to be nearer the key's direction than any
/// other arrow's, within 45° of it, whatever its steps: otherwise a lattice whose second vector
/// leans far over would take Up for mostly Left. If no move that far is, the key does nothing,
/// and the result is empty.
pub fn nudge_value(
    figure: &Figure,
    inputs: &Inputs,
    handle: usize,
    width: u32,
    height: u32,
    dx: f64,
    dy: f64,
) -> Result<Option<Vec<(String, f64)>>, Error> {
    let Some(place) = handle_places(figure, inputs, width, height)?
        .get(handle)
        .copied()
        .flatten()
    else {
        return Ok(None);
    };
    let env = eval::frame_env(figure, inputs)?;
    let params: Vec<&str> = figure.scene.handles[handle].params().collect();
    let now: Vec<Option<f64>> = params.iter().map(|param| env.get(param)).collect();
    let steps: Vec<bool> = params
        .iter()
        .map(|name| {
            figure
                .scene
                .params
                .iter()
                .any(|p| p.id == *name && p.step.is_some())
        })
        .collect();
    let stepped = steps.iter().any(|s| *s);
    let far = width.max(height) as f64;
    let mut reach = 1.0;
    loop {
        let values = drag_value(
            figure,
            inputs,
            handle,
            width,
            height,
            place.0 + dx * reach,
            place.1 + dy * reach,
        )?;
        let stepped_changed = values
            .iter()
            .zip(&now)
            .zip(&steps)
            .filter(|(_, stepped)| **stepped)
            .any(|(((_, value), now), _)| now.is_none_or(|now| (value - now).abs() > 1e-9));
        // How far the mark would move, and how far of that is the way the key points.
        let moved = || -> Result<Option<(f64, f64)>, Error> {
            let mut moved = inputs.clone();
            moved
                .params
                .retain(|(id, _)| !params.contains(&id.as_str()));
            moved.params.extend(values.iter().cloned());
            Ok(
                handle_places(figure, &moved, width, height)?[handle].map(|(x, y)| {
                    let (mx, my) = (x - place.0, y - place.1);
                    (mx.hypot(my), (mx * dx + my * dy) / dx.hypot(dy))
                }),
            )
        };
        let toward = || Ok::<_, Error>(moved()?.is_some_and(|(_, along)| along > 1e-6));
        let within = || {
            Ok::<_, Error>(moved()?.is_some_and(|(length, along)| {
                along > 1e-6 && along > length * std::f64::consts::FRAC_1_SQRT_2
            }))
        };
        reach *= 2.0;
        let done = if params.len() > 1 {
            within()?
        } else {
            // With no steps, the first move is the move.
            !stepped || stepped_changed || toward()?
        };
        if done {
            return Ok(Some(values));
        }
        if dx.hypot(dy) * reach > far {
            return Ok(Some(if params.len() > 1 { Vec::new() } else { values }));
        }
    }
}

/// What a drag sets a handle's params to: each one's explicit inverse at the pointer, kept
/// within that param's range, which is what constrains the drag. Every inverse reads the
/// pointer and the params as they were before the drag, so a handle that sets two sets them
/// together, and each stops at its own range while the other goes on. The values are then
/// settled as a frame settles them, in dependency order, so a range that follows the other
/// param is the one it has after the drag, and what's held is what's drawn.
pub fn drag_value(
    figure: &Figure,
    inputs: &Inputs,
    handle: usize,
    width: u32,
    height: u32,
    px: f64,
    py: f64,
) -> Result<Vec<(String, f64)>, Error> {
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
    let raw = handle
        .sets
        .iter()
        .map(|(name, inverse)| {
            let value = match eval::real(inverse, &env) {
                Ok(value) => value,
                // The pointer is somewhere this inverse has no answer, so its param stays put.
                Err(e) if e.absent => match env.get(name) {
                    Some(value) => value,
                    None => return Err(e),
                },
                Err(e) => return Err(e),
            };
            Ok((name.clone(), value))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    // Held at those values, the frame keeps each in its range as it stands, in order.
    let mut held = inputs.clone();
    held.params
        .retain(|(id, _)| !raw.iter().any(|(name, _)| name == id));
    held.params.extend(raw.iter().cloned());
    let settled = eval::frame_env(figure, &held)?;
    Ok(raw
        .into_iter()
        .map(|(name, value)| {
            let value = settled.get(&name).unwrap_or(value);
            (name, value)
        })
        .collect())
}

/// What a label says: its caption, or what its style's `text` makes of the value it's about,
/// as `expr = value`. A label with no words of its own shows its value. Parts that come out
/// the same aren't said twice, so a param's `name = expr = value` is `q = 3`.
///
/// A value's name, its expression and the value are mathematics, so they're marked as it and
/// laid out as a label's own `$…$` is, whatever the label's font. The font is for its words.
fn label_text(
    text: &str,
    values: &[Shown],
    subject: Option<&Subject>,
    style: &MarkStyle,
    env: &Env,
) -> Result<String, Error> {
    let digits = match &style.digits {
        Some(d) => eval::real(d, env)?.round().clamp(0.0, 10.0) as usize,
        None => 2,
    };
    let caption = |subject: Option<&Subject>| -> Result<String, Error> {
        match subject {
            Some(subject) if text.is_empty() => Ok(subject.name.clone()),
            _ => filled(text, values, style, digits, env),
        }
    };
    let default = [if text.is_empty() {
        TextPart::Value
    } else {
        TextPart::Caption
    }];
    let parts = style.text.as_deref().unwrap_or(&default);
    // Each part, and whether it's mathematics rather than the label's own words.
    let mut said: Vec<(String, bool)> = Vec::new();
    for part in parts {
        let piece = match (part, subject) {
            (TextPart::Caption, Some(subject)) if text.is_empty() => (subject.name.clone(), true),
            (TextPart::Caption, _) => (caption(subject)?, false),
            // A label that isn't about a value just says what it says.
            (_, None) => (caption(None)?, false),
            (TextPart::Name, Some(subject)) => (subject.name.replace('-', "−"), true),
            (TextPart::Expr, Some(subject)) => (subject.expr.replace('-', "−"), true),
            (TextPart::Value, Some(subject)) => {
                (written(&subject.value, style, digits, env)?, true)
            }
        };
        if said.last().map(|(last, _)| last) != Some(&piece.0) {
            said.push(piece);
        }
    }
    let mut out = String::new();
    // Whether a span of mathematics is open. An equals sign between two pieces of it is in it.
    let mut open = false;
    for (i, (piece, is_math)) in said.iter().enumerate() {
        let marked = *is_math;
        if open && !marked {
            out.push('$');
        }
        if i > 0 {
            out.push_str(" = ");
        }
        if marked && !open {
            out.push('$');
        }
        out.push_str(piece);
        open = marked;
    }
    if open {
        out.push('$');
    }
    Ok(out)
}

/// A value as a label writes it: to `digits` places, and a point in the style's `format`.
fn written(value: &Shown, style: &MarkStyle, digits: usize, env: &Env) -> Result<String, Error> {
    let printed = match value {
        Shown::Number(v) => number(eval::real(v, env)?, digits),
        Shown::Point([x, y]) => {
            let (x, y) = (eval::real(x, env)?, eval::real(y, env)?);
            written_point(x, y, style.format, digits)
        }
    };
    Ok(printed.replace('-', "−"))
}

/// A label's own words, with its values in them.
fn filled(
    text: &str,
    values: &[Shown],
    style: &MarkStyle,
    digits: usize,
    env: &Env,
) -> Result<String, Error> {
    let mut out = String::new();
    let mut pieces = text.split("{}");
    out.push_str(pieces.next().unwrap_or(""));
    for (value, piece) in values.iter().zip(pieces) {
        out.push_str(&written(value, style, digits, env)?);
        out.push_str(piece);
    }
    Ok(out)
}

/// A number to `digits` places, or whole if it's a whole number.
fn number(v: f64, digits: usize) -> String {
    let printed = if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.digits$}")
    };
    // Rounding can leave a sign on nothing.
    if printed
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        printed.trim_start_matches('-').to_string()
    } else {
        printed
    }
}

/// A point as a label writes it, with ASCII minus signs, which the caller turns into `−`.
fn written_point(x: f64, y: f64, format: crate::style::Format, digits: usize) -> String {
    use crate::style::Format;
    // A part that would print as zero is left out of a sum, as `i` rather than `0 + 1i`.
    let zero = |v: f64| {
        number(v, digits)
            .chars()
            .all(|c| c == '0' || c == '.' || c == '-')
    };
    let imaginary = |v: f64| match number(v, digits).as_str() {
        "1" => "i".to_string(),
        "-1" => "-i".to_string(),
        n => format!("{n}i"),
    };
    match format {
        Format::Point => format!("({}, {})", number(x, digits), number(y, digits)),
        Format::Complex => match (zero(x), zero(y)) {
            (_, true) => number(x, digits),
            (true, false) => imaginary(y),
            (false, false) => {
                let sign = if y < 0.0 { '-' } else { '+' };
                format!("{} {sign} {}", number(x, digits), imaginary(y.abs()))
            }
        },
        Format::Polar => {
            let r = x.hypot(y);
            if zero(r) {
                return "0".into();
            }
            let modulus = match number(r, digits).as_str() {
                "1" => String::new(),
                n => n.to_string(),
            };
            let angle = y.atan2(x);
            // A fraction of π with a small denominator is written as one: e^(2πi/3).
            let turn = (1..=12).find_map(|q| {
                let p = angle / std::f64::consts::PI * q as f64;
                ((p - p.round()).abs() < 1e-9).then(|| (p.round() as i64, q))
            });
            let exponent = match turn {
                Some((0, _)) => return number(r, digits),
                Some((p, q)) => {
                    let p = match p {
                        1 => String::new(),
                        -1 => "-".into(),
                        p => p.to_string(),
                    };
                    if q == 1 {
                        format!("{p}πi")
                    } else {
                        format!("{p}πi/{q}")
                    }
                }
                None => format!("{}i", number(angle, digits)),
            };
            format!("{modulus}e^({exponent})")
        }
    }
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
/// What a figure's last frame worked out for a mark, and every value that depended on, kept as
/// `eval::Solved` keeps a system's answer. Tracing an implicit curve and working out a
/// collection's circles cost the same every frame whether or not anything they read has changed,
/// and in most figures most of them haven't: a grid stays where it is while a point moves over
/// it. The key is everything the work reads, so keeping it can't change what a frame shows.
#[derive(Debug, Default)]
pub struct Kept {
    traces: Memo<Vec<crate::trace::Polyline>>,
    sides: Memo<Vec<f32>>,
    /// The traced curves the last distance texture was tiled from, at what size, and the texture.
    nearest: Mutex<Option<(Tiled, Vec<f32>)>>,
}

/// What `nearest::texture` reads: each measured mark's segments, and the frame's size.
type Tiled = (Vec<Vec<[f32; 4]>>, u32, u32);

/// For each mark, the key its answer was worked out under, and the answer.
type Memo<T> = Mutex<HashMap<usize, (Vec<u64>, T)>>;

/// The values these expressions read, and `also`, as a key: the same key means the same answer.
fn kept_key<'a>(exprs: impl IntoIterator<Item = &'a Expr>, env: &Env, also: &[f64]) -> Vec<u64> {
    let mut key: Vec<u64> = also.iter().map(|v| v.to_bits()).collect();
    for e in exprs {
        e.visit_syms(&mut |name| key.push(env.get(name).map_or(u64::MAX, f64::to_bits)));
    }
    key
}

/// The answer kept under `key` for this mark, or `work`'s, which is then kept.
fn kept<T: Clone>(
    memo: &Memo<T>,
    mark: usize,
    key: Vec<u64>,
    work: impl FnOnce() -> Result<T, Error>,
) -> Result<T, Error> {
    let mut entries = memo.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((known, answer)) = entries.get(&mark)
        && *known == key
    {
        return Ok(answer.clone());
    }
    let answer = work()?;
    entries.insert(mark, (key, answer.clone()));
    Ok(answer)
}

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
        let found = match &mark.form {
            Form::Many { vars, lets, shape } => {
                let ranges = vars.iter().flat_map(|(_, range)| range);
                let exprs = ranges
                    .chain(lets.iter().map(|(_, e)| e))
                    .chain(stroke::shape_exprs(shape));
                // What a style reads per instance can be something of the frame's, too.
                let read: Vec<f64> = group
                    .names
                    .iter()
                    .map(|name| env.get(name).unwrap_or(f64::NAN))
                    .collect();
                kept(
                    &figure.kept.sides,
                    group.mark,
                    kept_key(exprs, &env, &read),
                    || stroke::sided_instances(mark, group, &env),
                )?
            }
            _ => stroke::sided_instances(mark, group, &env)?,
        };
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
            let trace = || stroke::lines(&mark.form, &env, view, width, height, true, reach);
            let frame = [
                view.x[0],
                view.x[1],
                view.y[0],
                view.y[1],
                f64::from(width),
                f64::from(height),
                reach,
            ];
            let lines = match &mark.form {
                Form::Field { field, grad, .. } => {
                    let exprs = [field, &grad[0], &grad[1]];
                    kept(&figure.kept.traces, i, kept_key(exprs, &env, &frame), trace)?
                }
                Form::Parametric(arc) => {
                    let exprs = arc.point.iter().chain(&arc.range).chain(&arc.depth);
                    kept(&figure.kept.traces, i, kept_key(exprs, &env, &frame), trace)?
                }
                _ => trace()?,
            };
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
    let nearest = {
        let mut last = figure
            .kept
            .nearest
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match &*last {
            Some(((from, w, h), texture)) if *from == traced && (*w, *h) == (width, height) => {
                texture.clone()
            }
            _ => {
                let texture = nearest::texture(&traced, width, height);
                *last = Some(((traced.clone(), width, height), texture.clone()));
                texture
            }
        }
    };
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
            subject,
            at,
            offset,
            away,
        } = &mark.form
        else {
            continue;
        };
        // Text with `fill: none` has no paint.
        if !shown[i] || style.no_fill {
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
        let text = label_text(text, values, subject.as_ref(), style, &env)?;
        let size = length_or_em(&style.size, DEFAULT_LABEL_SIZE, &env)?;
        labels.push(Label {
            id: mark.id.clone(),
            laid: formula::lay(&text, size, style.font, widths),
            text,
            x: dx + ox,
            y: dy + oy,
            size,
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
        figure, &env, view, width, height, dpr, &shown,
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

    /// Kris's figure: a cube root of unity, as each format writes it.
    #[test]
    fn a_label_writes_a_complex_value_as_its_style_says() {
        let doc = "fn σ(n) = exp(2πi·(n/q))\nlet q = 3\nlet root = σ(2)\n\
                   label n = \"{root}\" at (0, 0)\nlabel half = \"{root/2}\" at (0, 0)\nlabel one = \"{σ(0)}\" at (0, 0)\n\
                   label i = \"{σ(3/4)}\" at (0, 0)";
        let shown = |sheet: &str| -> Vec<String> {
            let figure = crate::build(doc, &[sheet]).unwrap();
            let prepared = prepare(&figure, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
            prepared.labels.iter().map(|l| l.text.clone()).collect()
        };
        assert_eq!(
            shown(""),
            ["(−0.50, −0.87)", "(−0.25, −0.43)", "(1, 0)", "(0, 1)"]
        );
        assert_eq!(
            shown("style label { format: complex }"),
            ["−0.50 − 0.87i", "−0.25 − 0.43i", "1", "i"]
        );
        assert_eq!(
            shown("style label { format: polar }"),
            ["e^(−2πi/3)", "0.50e^(−2πi/3)", "1", "e^(πi/2)"]
        );
        assert_eq!(
            shown("style label { format: complex; digits: 3 }")[0],
            "−0.500 − 0.866i"
        );
    }

    /// The sheet says what a label shows of the value it's about.
    #[test]
    fn a_style_says_what_a_label_shows() {
        let doc = "fn σ(n) = exp(2πi·(n/q))\nparam q = 3 in 1..9\nlet root = σ(2)\n\
                   label root at root\nlabel q = \"arms\" at (0, 0)\nlabel n = \"{q - 1}\" at (0, 0)\n\
                   label m = \"{q} of them\" at (0, 0)";
        let shown = |sheet: &str| -> Vec<String> {
            let sheet = format!("style label {{ format: complex }} {sheet}");
            let figure = crate::build(doc, &[&sheet]).unwrap();
            let prepared = prepare(&figure, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
            prepared.labels.iter().map(|l| l.words()).collect()
        };
        // With no `text`, a label says its own words, or its value if it has none.
        assert_eq!(shown(""), ["−0.50 − 0.87i", "arms", "2", "3 of them"]);
        assert_eq!(
            shown("style label { text: expr = value }"),
            ["σ(2) = −0.50 − 0.87i", "q = 3", "q − 1 = 2", "3 of them"]
        );
        assert_eq!(
            shown("style label { text: name = value; digits: 1 }"),
            ["root = −0.5 − 0.9i", "q = 3", "q − 1 = 2", "3 of them"]
        );
        assert_eq!(
            shown("style label { text: caption }"),
            ["root", "arms", "2", "3 of them"]
        );
        assert_eq!(
            shown(
                "style label { text: caption = value } style root { text: name = expr = value; format: polar }"
            ),
            ["root = σ(2) = e^(−2πi/3)", "arms = 3", "2", "3 of them"]
        );
        let err = crate::build(doc, &["style label { text: nope }"]).unwrap_err();
        assert!(err.message.contains("made of `caption`"), "{err}");
        // A rule that names the value reaches its dot and its label, each taking what's its own.
        let figure = crate::build(
            "let P = (1, 2)\ndot P\nlabel P at P",
            &["style P { radius: 4px; text: name = value; digits: 1; leader: 1px }"],
        )
        .unwrap();
        let prepared = prepare(&figure, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
        assert_eq!(prepared.labels[0].words(), "P = (1, 2)");
        // With no label beside it, a dot still can't take a label's property.
        let err = crate::build("let P = (1, 2)\ndot P", &["style P { text: name }"]).unwrap_err();
        assert!(err.message.contains("isn't a label"), "{err}");
    }

    #[test]
    fn a_number_rounded_to_nothing_has_no_sign() {
        assert_eq!(super::number(-0.001, 2), "0.00");
        assert_eq!(super::number(-1e-12, 2), "0");
        assert_eq!(
            super::written_point(1e-17, -2.0, crate::style::Format::Complex, 2),
            "-2i"
        );
    }

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

    fn label_of(doc: &str, sheet: &str, id: &str) -> Label {
        let figure = crate::build(doc, &[sheet]).unwrap();
        let prepared = prepare(&figure, &Inputs::default(), 400, 400, 1.0).unwrap();
        prepared.labels.iter().find(|l| l.id == id).unwrap().clone()
    }

    #[test]
    fn a_labels_mathematics_is_laid_out_and_its_words_arent() {
        let doc = "viewport { x: -2..2, y: -2..2 }
                   let n = 3
                   label plain = \"arg z\" at (0, 0)
                   label power = \"area = $x^{n}/2$\" at (0, 0)
                   label price = \"$$5\" at (0, 0)";
        let plain = label_of(doc, "", "plain");
        assert!(plain.laid.is_none());
        assert_eq!(
            plain.half(&terms::Estimate)[1],
            plain.size * TEXT_HALF_HEIGHT
        );
        // A value goes into a span as it does into words, and is laid out with it.
        let power = label_of(doc, "", "power");
        assert_eq!(power.text, "area = $x^3/2$");
        assert_eq!(power.words(), "area = x^3/2");
        let laid = power.laid.as_ref().expect("laid out");
        assert_eq!(laid.rules.len(), 1);
        assert!(laid.runs.iter().any(|r| r.text == "3"));
        // A fraction is taller than a line, and the label's box says so.
        let [half_width, half_height] = power.half(&terms::Estimate);
        assert_eq!(half_width, laid.width / 2.0);
        assert!(half_height > power.size * TEXT_HALF_HEIGHT);
        // Two dollar signs are one, in words.
        assert_eq!(label_of(doc, "", "price").words(), "$5");
        let open = "label n = \"area = $x^2\" at (0, 0)";
        let err = crate::build(open, &[]).unwrap_err();
        assert!(err.message.contains("no `$` closes"), "{err}");
    }

    /// What Pencil writes about a value is mathematics, set as it whatever the label's font.
    #[test]
    fn a_values_label_is_mathematics_in_any_font() {
        let doc = "viewport { x: -2..2, y: -2..2 }
                   let root = (-0.5, sqrt(3)/2)
                   label root at root
                   label note = \"the root\" at root";
        let sheet = |font: &str| {
            format!("style label {{ font: {font}; text: name = value; format: polar }}")
        };
        let math = label_of(doc, &sheet("math"), "root");
        assert_eq!(math.text, "$root = e^(2πi/3)$");
        assert_eq!(math.words(), "root = e^(2πi/3)");
        // The power is a real superscript: smaller, and above the line.
        let laid = math.laid.as_ref().expect("laid out");
        let (base, power) = (&laid.runs[0], laid.runs.last().unwrap());
        assert!(power.size < base.size && power.y < base.y, "{laid:?}");
        // In sans it's laid out the same: the font is for a label's own words, which aren't.
        let sans = label_of(doc, &sheet("sans"), "root");
        assert_eq!((&sans.text, &sans.laid), (&math.text, &math.laid));
        let note = label_of(doc, &sheet("math"), "note");
        assert_eq!((note.text.as_str(), &note.laid), ("the root", &None));
    }

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

    /// A param with steps lands on one however it's driven: its default, time, a reader, a drag.
    #[test]
    fn a_stepped_param_only_takes_its_steps() {
        let doc = "
            viewport { x: -2..2, y: -2..2 }
            param a = 1 in 0.2..1.85 step 0.25
            param n = 2.6 in 1..10 step 1
            bind n = 1 + time
            let P = (cos(a), sin(a))
            dot P
            handle P sets a = atan2(max(pointer.y, 0), pointer.x)";
        let f = build(doc, &[]).unwrap();
        let env = eval::frame_env(&f, &Inputs::at(2.4)).unwrap();
        assert_eq!(
            env.get("a"),
            Some(0.95),
            "the default goes to the nearest step"
        );
        assert_eq!(env.get("n"), Some(3.0), "so does a binding");
        let read = Inputs {
            time: 0.0,
            params: vec![("a".into(), 1.3)],
        };
        assert_eq!(eval::frame_env(&f, &read).unwrap().get("a"), Some(1.2));
        let drag = |px: f64, py: f64| {
            only(drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap()).1
        };
        // At 45°, π/4 is nearest 0.7.
        assert!((drag(300.0, 100.0) - 0.7).abs() < 1e-12);
        // The range's end isn't a step, so a drag stops at the last one before it.
        assert!((drag(1.0, 201.0) - 1.7).abs() < 1e-12);
        assert_eq!(drag(399.0, 199.0), 0.2);
    }

    #[test]
    fn a_step_has_to_be_above_zero() {
        let err = build("param n = 1 in 0..4 step 0", &[]).unwrap_err();
        assert!(
            err.to_string().contains("a step has to be above 0"),
            "{err}"
        );
    }

    #[test]
    fn a_drag_follows_the_inverse_within_the_range() {
        let f = build(DRAG, &[]).unwrap();
        let drag =
            |px: f64, py: f64| only(drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap());
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

    /// A point on the unit circle, dragged by its angle, and one on a line that only takes
    /// whole numbers.
    const KEYED: &str = "viewport { x: -2..2, y: -2..2 }
        param theta = 0 in 0..π
        param n = 0 in -1..1 step 1
        param shown = 1 in 0..1 step 1
        let P = (cos(theta), sin(theta))
        let N = (n, -1.5)
        // Where a line through the origin meets y = 1: nowhere, once it's turned flat.
        let Q = intersect(line((0, 1), (1, 1)), line((0, 0), (1 - shown, shown)))
        dot P, N, Q
        handle P sets theta = atan2(max(pointer.y, 0), pointer.x)
        handle N sets n = pointer.x
        handle Q sets shown = pointer.x";

    /// The one param a one-param handle set.
    fn only(mut values: Vec<(String, f64)>) -> (String, f64) {
        assert_eq!(values.len(), 1, "{values:?}");
        values.remove(0)
    }

    fn held(params: &[(&str, f64)]) -> Inputs {
        Inputs {
            time: 0.0,
            params: params.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
        }
    }

    #[test]
    fn a_handle_says_where_it_is() {
        let f = build(KEYED, &[]).unwrap();
        let places = handle_places(&f, &Inputs::at(0.0), 400, 400).unwrap();
        // P is at (1, 0) and N at (0, −1.5), in a view 4 across drawn 400 wide.
        assert_eq!(
            places,
            [
                Some((300.0, 200.0)),
                Some((200.0, 350.0)),
                Some((200.0, 100.0))
            ]
        );
        // A mark that doesn't exist has no place, and a key on it does nothing.
        let hidden = held(&[("shown", 0.0)]);
        assert_eq!(handle_places(&f, &hidden, 400, 400).unwrap()[2], None);
        assert_eq!(
            nudge_value(&f, &hidden, 2, 400, 400, 4.0, 0.0).unwrap(),
            None
        );
        // A figure with no handles has none.
        let plain = build("let P = (0, 0)\ndot P", &[]).unwrap();
        assert!(
            handle_places(&plain, &Inputs::at(0.0), 400, 400)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_key_moves_a_handle_through_its_inverse() {
        let f = build(KEYED, &[]).unwrap();
        let at = held(&[("theta", std::f64::consts::FRAC_PI_2)]);
        // At the top of the circle, left is counterclockwise: the mark is at (200, 100), and 10
        // pixels left of it is the angle atan2(1, −0.1).
        let (name, value) = only(
            nudge_value(&f, &at, 0, 400, 400, -10.0, 0.0)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(name, "theta");
        assert!((value - 1.0f64.atan2(-0.1)).abs() < 1e-12, "{value}");
        // Up is straight out from the circle, so the angle stays.
        let (_, value) = only(
            nudge_value(&f, &at, 0, 400, 400, 0.0, -10.0)
                .unwrap()
                .unwrap(),
        );
        assert!(
            (value - std::f64::consts::FRAC_PI_2).abs() < 1e-12,
            "{value}"
        );
        // The param's range stops it: past the end of the half circle, it stays at π.
        let end = held(&[("theta", std::f64::consts::PI)]);
        let (_, value) = only(
            nudge_value(&f, &end, 0, 400, 400, 0.0, 10.0)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(value, std::f64::consts::PI);
    }

    #[test]
    fn a_key_reaches_the_next_step_of_a_param_that_has_steps() {
        let f = build(KEYED, &[]).unwrap();
        // N's steps are 100 pixels apart, and 4 pixels doesn't round to the next one. The move
        // grows until it does.
        let (name, value) = only(
            nudge_value(&f, &Inputs::at(0.0), 1, 400, 400, 4.0, 0.0)
                .unwrap()
                .unwrap(),
        );
        assert_eq!((name.as_str(), value), ("n", 1.0));
        let (_, value) = only(
            nudge_value(&f, &Inputs::at(0.0), 1, 400, 400, -4.0, 0.0)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(value, -1.0);
        // At the end of its range there's no next step, and it stays.
        let (_, value) = only(
            nudge_value(&f, &held(&[("n", 1.0)]), 1, 400, 400, 4.0, 0.0)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(value, 1.0);
        // Up and down don't move it at all.
        let (_, value) = only(
            nudge_value(&f, &Inputs::at(0.0), 1, 400, 400, 0.0, 4.0)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(value, 0.0);
    }

    /// A point written from two params, dragged by setting both, each in its own range.
    const PLANE: &str = "viewport { x: -2..2, y: -2..2 }
        param a = 0 in -1.5..1.5
        param b = 0 in -1..1 step 0.25
        let z = a + b·i
        dot z
        handle z sets a = pointer.x, b = pointer.y";

    /// Where a handle's mark is in the figure's own coordinates, with these params.
    fn mark_at(f: &Figure, handle: usize, params: &[(String, f64)]) -> (f64, f64) {
        let inputs = Inputs {
            time: 0.0,
            params: params.to_vec(),
        };
        pair(
            &f.scene.handles[handle].at,
            &eval::frame_env(f, &inputs).unwrap(),
        )
        .unwrap()
    }

    /// The device pixel of a point in a view 400 pixels square.
    fn pixel(view: f64, x: f64, y: f64) -> (f64, f64) {
        (
            (x + view) / (2.0 * view) * 400.0,
            (view - y) / (2.0 * view) * 400.0,
        )
    }

    #[test]
    fn a_two_param_drag_sets_both_and_each_stops_at_its_own_range() {
        let f = build(PLANE, &[]).unwrap();
        let drag = |x: f64, y: f64| {
            let (px, py) = pixel(2.0, x, y);
            drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap()
        };
        let named = |values: Vec<(String, f64)>| {
            let names: Vec<String> = values.iter().map(|(n, _)| n.clone()).collect();
            assert_eq!(names, ["a", "b"]);
            (values[0].1, values[1].1)
        };
        // Inside both ranges the point is where the pointer is, with b on its steps.
        let (a, b) = named(drag(0.3, 0.5));
        assert!((a - 0.3).abs() < 1e-12 && b == 0.5, "{a} {b}");
        assert_eq!(named(drag(-0.7, 0.3)).1, 0.25, "b takes its nearest step");
        // Past a's range only a stops, and b goes on following the pointer.
        let (a, b) = named(drag(1.9, -0.5));
        assert_eq!((a, b), (1.5, -0.5));
        let (a, b) = named(drag(1.9, 0.75));
        assert_eq!((a, b), (1.5, 0.75));
        // Past b's range only b stops.
        let (a, b) = named(drag(-0.4, 1.8));
        assert!((a + 0.4).abs() < 1e-12 && b == 1.0, "{a} {b}");
        // Past both, it's the corner.
        assert_eq!(named(drag(-1.9, -1.9)), (-1.5, -1.0));
    }

    #[test]
    fn both_inverses_read_the_params_as_they_were_before_the_drag() {
        // b's inverse reads a. It sees a before this drag sets it, not after.
        let f = build(
            "viewport { x: -2..2, y: -2..2 }
             param a = 0.5 in -2..2
             param b = 0 in -2..2
             let P = (a, b - a)
             dot P
             handle P sets a = pointer.x, b = pointer.y + a",
            &[],
        )
        .unwrap();
        let (px, py) = pixel(2.0, 1.0, 0.0);
        let values = drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap();
        assert_eq!(values, [("a".to_string(), 1.0), ("b".to_string(), 0.5)]);
    }

    #[test]
    fn an_inverse_with_no_answer_leaves_only_its_own_param() {
        // b is where the line from the origin through the pointer meets y = 1, which a level
        // pointer never does.
        let f = build(
            "viewport { x: -2..2, y: -2..2 }
             param a = 0.5 in -2..2
             param b = 0.5 in -2..2
             let P = (a, b)
             dot P
             handle P sets a = pointer.x, b = real(intersect(line((0, 0), pointer), line((0, 1), (1, 1))))",
            &[],
        )
        .unwrap();
        let drag = |x: f64, y: f64| {
            let (px, py) = pixel(2.0, x, y);
            drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap()
        };
        let pairs = |a: f64, b: f64| vec![("a".to_string(), a), ("b".to_string(), b)];
        assert_eq!(drag(1.0, 0.0), pairs(1.0, 0.5));
        assert_eq!(drag(1.0, 1.0), pairs(1.0, 1.0));
    }

    /// Kris's lattice: z = a + bω, and the handle turns the pointer into the two coefficients.
    fn lattice(step: &str) -> String {
        format!(
            "viewport {{ x: -4..4, y: -4..4 }}
             param a = 0 in -3..3{step}
             param b = 0 in -3..3{step}
             let ω = e^(i·π/3)
             let z = a + b·ω
             dot z
             handle z sets a = pointer.x - pointer.y·real(ω)/imag(ω), b = pointer.y/imag(ω)"
        )
    }

    /// Points whose coefficients are inside both ranges.
    const INSIDE: [(f64, f64); 6] = [
        (0.0, 0.0),
        (0.3, 0.2),
        (-1.7, 1.1),
        (1.0, -2.5),
        (-2.9, -0.4),
        (2.5, 2.0),
    ];

    #[test]
    fn a_lattice_point_follows_the_pointer_through_its_inverse() {
        let f = build(&lattice(""), &[]).unwrap();
        for (x, y) in INSIDE {
            let (px, py) = pixel(4.0, x, y);
            let values = drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap();
            let (zx, zy) = mark_at(&f, 0, &values);
            assert!(
                (zx - x).abs() < 1e-12 && (zy - y).abs() < 1e-12,
                "at ({x}, {y}) z is ({zx}, {zy})"
            );
        }
    }

    #[test]
    fn a_stepped_lattice_point_lands_on_the_nearest_lattice_point_by_its_coefficients() {
        let f = build(&lattice(" step 1"), &[]).unwrap();
        let (h, w) = (3f64.sqrt() / 2.0, 0.5);
        for (x, y) in INSIDE {
            let (px, py) = pixel(4.0, x, y);
            let values = drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap();
            // The coefficients worked out here, apart from the figure, and rounded.
            let (b, a) = ((y / h).round(), (x - y * w / h).round());
            assert_eq!(values, [("a".to_string(), a), ("b".to_string(), b)]);
            let (zx, zy) = mark_at(&f, 0, &values);
            assert!(
                (zx - (a + b * w)).abs() < 1e-12 && (zy - b * h).abs() < 1e-12,
                "at ({x}, {y}) z is ({zx}, {zy})"
            );
        }
    }

    #[test]
    fn a_key_moves_a_two_param_handle_on_the_screen() {
        // On the plane, right is a and up is b.
        let f = build(&PLANE.replace(" step 0.25", ""), &[]).unwrap();
        let key = |dx, dy| nudge_value(&f, &Inputs::at(0.0), 0, 400, 400, dx, dy).unwrap();
        let near = |got: Option<Vec<(String, f64)>>, a: f64, b: f64| {
            let got = got.unwrap();
            assert!(
                (got[0].1 - a).abs() < 1e-12 && (got[1].1 - b).abs() < 1e-12,
                "{got:?}"
            );
        };
        near(key(10.0, 0.0), 0.1, 0.0);
        near(key(-10.0, 0.0), -0.1, 0.0);
        near(key(0.0, -10.0), 0.0, 0.1);
        near(key(0.0, 10.0), 0.0, -0.1);
        // On the lattice, up moves z straight up, which changes both coefficients.
        let f = build(&lattice(""), &[]).unwrap();
        let values = nudge_value(&f, &Inputs::at(0.0), 0, 400, 400, 0.0, -10.0)
            .unwrap()
            .unwrap();
        let (zx, zy) = mark_at(&f, 0, &values);
        assert!(zx.abs() < 1e-12 && (zy - 0.2).abs() < 1e-12, "{zx} {zy}");
        assert!(values.iter().all(|(_, v)| v.abs() > 0.01), "{values:?}");
        // With steps, a key goes on until a coefficient reaches its next one: right is 1, and
        // up is ω.
        let f = build(&lattice(" step 1"), &[]).unwrap();
        let key = |dx, dy| {
            nudge_value(&f, &Inputs::at(0.0), 0, 400, 400, dx, dy)
                .unwrap()
                .unwrap()
        };
        let pairs = |a: f64, b: f64| [("a".to_string(), a), ("b".to_string(), b)];
        assert_eq!(key(4.0, 0.0), pairs(1.0, 0.0));
        assert_eq!(key(0.0, -4.0), pairs(0.0, 1.0));
    }

    #[test]
    fn a_range_that_follows_the_other_param_is_the_one_it_has_after_the_drag() {
        // p can't pass q, and one drag sets both. At (0.3, 0.9) q is 0.3, so p is too, as the
        // frame draws it, whichever is declared first.
        let follows = [
            "param q = 1 in 0..2\nparam p = 0.5 in 0..q",
            "param p = 0.5 in 0..q\nparam q = 1 in 0..2",
        ];
        for params in follows {
            let doc = format!(
                "viewport {{ x: -2..2, y: -2..2 }}\n{params}\nlet z = q + p·i\ndot z\n\
                 handle z sets q = pointer.x, p = pointer.y"
            );
            let f = build(&doc, &[]).unwrap();
            let (px, py) = pixel(2.0, 0.3, 0.9);
            let values = drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap();
            assert!(
                (values[0].1 - 0.3).abs() < 1e-12 && (values[1].1 - 0.3).abs() < 1e-12,
                "{params}: {values:?}"
            );
            // Held at what the drag gave, the frame draws the same values.
            let env =
                eval::frame_env(&f, &held(&[("q", values[0].1), ("p", values[1].1)])).unwrap();
            assert_eq!(
                (env.get("q"), env.get("p")),
                (Some(values[0].1), Some(values[1].1))
            );
            // Below q, p is where the pointer is.
            let (px, py) = pixel(2.0, 1.5, 0.9);
            let values = drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap();
            assert!(
                (values[0].1 - 1.5).abs() < 1e-12 && (values[1].1 - 0.9).abs() < 1e-12,
                "{params}: {values:?}"
            );
        }
        // Neither following the other, each stops at its own end.
        let f = build(
            "viewport { x: -2..2, y: -2..2 }\nparam q = 1 in 0..2\nparam p = 0.5 in 0..1\n\
             let z = q + p·i\ndot z\nhandle z sets q = pointer.x, p = pointer.y",
            &[],
        )
        .unwrap();
        let (px, py) = pixel(2.0, 0.3, 1.5);
        let values = drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap();
        assert!(
            (values[0].1 - 0.3).abs() < 1e-12 && values[1].1 == 1.0,
            "{values:?}"
        );
    }

    #[test]
    fn a_key_moves_a_lattice_point_nearer_its_own_way_than_any_other_keys() {
        const KEYS: [(&str, f64, f64); 4] = [
            ("right", 4.0, 0.0),
            ("left", -4.0, 0.0),
            ("up", 0.0, -4.0),
            ("down", 0.0, 4.0),
        ];
        for angle in [20.0, 60.0, 170.0] {
            for (a, b) in [
                ("", ""),
                (" step 1", ""),
                ("", " step 1"),
                (" step 1", " step 1"),
            ] {
                let doc = format!(
                    "viewport {{ x: -4..4, y: -4..4 }}
                     param a = 0 in -3..3{a}
                     param b = 0 in -3..3{b}
                     let ω = e^(i·{angle}·π/180)
                     let z = a + b·ω
                     dot z
                     handle z sets a = pointer.x - pointer.y·real(ω)/imag(ω), b = pointer.y/imag(ω)"
                );
                let f = build(&doc, &[]).unwrap();
                for from in [
                    vec![],
                    vec![("a".to_string(), 1.0), ("b".to_string(), -1.0)],
                ] {
                    let inputs = Inputs {
                        time: 0.0,
                        params: from.clone(),
                    };
                    let start = mark_at(&f, 0, &from);
                    let mut moves = 0;
                    for (key, dx, dy) in KEYS {
                        let values = nudge_value(&f, &inputs, 0, 400, 400, dx, dy)
                            .unwrap()
                            .unwrap();
                        if values.is_empty() {
                            continue;
                        }
                        moves += 1;
                        let (x, y) = mark_at(&f, 0, &values);
                        // In the figure's own coordinates, where up is +y.
                        let (mx, my) = (x - start.0, y - start.1);
                        let along = |(_, kx, ky): (&str, f64, f64)| (mx * kx - my * ky) / 4.0;
                        let own = along((key, dx, dy));
                        assert!(
                            own > 1e-6
                                && KEYS
                                    .iter()
                                    .filter(|other| other.0 != key)
                                    .all(|&other| own > along(other)),
                            "ω at {angle}°, a{a}, b{b}, from {from:?}: {key} moved the mark by \
                             ({mx}, {my})"
                        );
                    }
                    // Unstepped, inside both ranges, every key moves it.
                    if a.is_empty() && b.is_empty() {
                        assert_eq!(moves, 4, "ω at {angle}°, from {from:?}");
                    } else {
                        assert!(moves >= 2, "ω at {angle}°, a{a}, b{b}, from {from:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_key_with_no_move_its_way_does_nothing() {
        // At a's right end and b's top, with ω at 20°, a can only go down and b down, and either
        // takes the mark mostly left: up, right and down would slide it along an edge, the
        // wrong way.
        let doc = "viewport { x: -4..4, y: -4..4 }
             param a = 1 in -1..1
             param b = 1 in -1..1
             let ω = e^(i·20·π/180)
             let z = a + b·ω
             dot z
             handle z sets a = pointer.x - pointer.y·real(ω)/imag(ω), b = pointer.y/imag(ω)";
        let f = build(doc, &[]).unwrap();
        let key = |dx, dy| {
            nudge_value(&f, &Inputs::at(0.0), 0, 400, 400, dx, dy)
                .unwrap()
                .unwrap()
        };
        assert_eq!(key(0.0, -4.0), []);
        assert_eq!(key(4.0, 0.0), []);
        assert_eq!(key(0.0, 4.0), []);
        assert!(!key(-4.0, 0.0).is_empty());
    }

    #[test]
    fn two_names_for_one_point_can_each_have_a_handle_and_a_drag_takes_the_later() {
        // The one-handle-per-mark check goes by name, and w is another name for z.
        let f = build(
            "viewport { x: -2..2, y: -2..2 }
             param a = 0.5 in -1..1
             param b = 0 in -1..1
             let z = a + b·i
             let w = z
             dot z, w
             handle z sets a = pointer.x
             handle w sets b = pointer.y",
            &[],
        )
        .unwrap();
        let (px, py) = pixel(2.0, 0.5, 0.0);
        assert_eq!(
            handle_at(&f, &Inputs::at(0.0), 400, 400, px, py, 10.0).unwrap(),
            Some(1)
        );
    }

    #[test]
    fn a_handle_can_read_the_pointer_as_a_point() {
        // The log of the pointer, as a complex number: its real part is the log of its
        // distance, and its imaginary part its angle.
        let f = build(
            "viewport { x: -4..4, y: -4..4 }
             param s = 0 in -1..1
             param t = 0 in -3..3
             let z = exp(s + t·i)
             dot z
             handle z sets s = real(ln(pointer)), t = imag(ln(pointer))",
            &[],
        )
        .unwrap();
        let (x, y) = (0.5f64.exp() * 1f64.cos(), 0.5f64.exp() * 1f64.sin());
        let (px, py) = pixel(4.0, x, y);
        let values = drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py).unwrap();
        assert!((values[0].1 - 0.5).abs() < 1e-12, "{values:?}");
        assert!((values[1].1 - 1.0).abs() < 1e-12, "{values:?}");
        // The one-param form reads it the same way.
        let f = build(
            "viewport { x: -4..4, y: -4..4 }
             param r = 1 in 0..3
             let P = (r, 0)
             dot P
             handle P sets r = abs(pointer)",
            &[],
        )
        .unwrap();
        let (px, py) = pixel(4.0, 1.2, 1.6);
        let [(_, r)] = drag_value(&f, &Inputs::at(0.0), 0, 400, 400, px, py)
            .unwrap()
            .try_into()
            .unwrap();
        assert!((r - 2.0).abs() < 1e-12, "{r}");
    }

    #[test]
    fn a_fit_view_ignores_both_params_a_handle_sets() {
        let f = build(
            "viewport { fit: c, pad: 0 }
             param a = 1 in 0..3
             param b = 0 in 0..3
             param k = 1 in 1..3
             let P = (a, b)
             curve c = circle((0, 0), k·P)
             dot P
             handle P sets a = pointer.x, b = pointer.y",
            &[],
        )
        .unwrap();
        let x = |params: &[(&str, f64)]| view_for(&f, &held(params), 100, 100).unwrap().x;
        assert_eq!(x(&[]), [-1.0, 1.0]);
        assert_eq!(x(&[("a", 2.0), ("b", 2.0)]), [-1.0, 1.0]);
        assert_eq!(x(&[("k", 2.0)]), [-2.0, 2.0]);
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
