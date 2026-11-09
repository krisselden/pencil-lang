//! Moves labels out of the way without jumping or wiggling.
//!
//! Each label with `place: settle` is tied to its home by a spring, and pushed by smooth
//! bumps around what's drawn and around other labels. It starts from home every frame and
//! takes a fixed number of steps, so it has no memory. That alone still leaps when a home
//! crosses a line, so a second spring smooths the result: a critically damped spring is a
//! filter over the past, which `spring` computes from earlier times alone, keeping a frame
//! a pure function of its inputs. A page has a clock and a reader's history instead, so it
//! runs the same spring itself. `lab/labels` has the comparison that chose this.

use std::collections::HashMap;

use crate::Figure;
use crate::error::Error;
use crate::eval::{self, Env, MIN_GRADIENT};
use crate::frame::{self, Inputs, Label, Prepared};
use crate::ir::Form;
use crate::stroke::FLOATS_PER_SEGMENT;
use crate::style::Place;

/// How far a bump reaches, in multiples of a label's size.
const REACH: f64 = 2.4;
const PAIR_REACH: f64 = 1.3;
/// How strongly bumps push against the spring home. Above 1 a label can clear a line, and
/// below about 1 it can't: this is the value Kris chose by eye in `lab/labels`.
const FIRMNESS: f64 = 1.8;
const PAIR_FIRMNESS: f64 = 3.0 * FIRMNESS;
const ITERATIONS: usize = 32;
const STEP: f64 = 0.22;
/// Clear space around a label's text, in style pixels.
const MARGIN: f64 = 3.0;
const WALL: f64 = 2.0;
/// Being soft, the bumps don't need a font's real widths, so a page and a video place labels
/// alike.
use crate::terms::EM_PER_CHAR;
const LINE_PER_EM: f64 = 1.25;

/// The second spring's rate, per second, as in `lab/labels`.
pub const SPRING_RATE: f64 = 12.0;
const SPRING_SAMPLES_PER_SECOND: f64 = 60.0;
const SPRING_WINDOW: usize = 48;

/// Something drawn that labels avoid, in style pixels.
enum Obstacle {
    /// One mark's stroke: `x0 y0 x1 y1` per segment, and its bounds.
    Line {
        segments: Vec<[f64; 4]>,
        bounds: [f64; 4],
    },
    Point([f64; 2]),
    /// A curve drawn per pixel, which is only known by its distance. An index among them.
    Field(usize),
}

/// A curve near a label, taken as its tangent there. Evaluating a field is the slow part
/// of settling, so it's done every few steps, and always at the same ones.
#[derive(Clone, Copy)]
struct Edge {
    at: [f64; 2],
    distance: f64,
    normal: [f64; 2],
}
const EDGE_EVERY: usize = 8;

fn opacity_of(style: &crate::style::MarkStyle, env: &Env) -> Result<f64, Error> {
    match &style.opacity {
        Some(o) => eval::real(o, env),
        None => Ok(1.0),
    }
}

fn obstacles(
    figure: &Figure,
    env: &Env,
    prepared: &Prepared,
    width: u32,
    height: u32,
) -> Result<(Vec<Obstacle>, Vec<usize>), Error> {
    let dpr = prepared.dpr;
    let mut out = Vec::new();
    let mut fields = Vec::new();
    let mut layer = 0;
    for (i, (mark, style)) in figure
        .scene
        .marks
        .iter()
        .zip(&figure.styles.marks)
        .enumerate()
    {
        let is_line = style.stroke_is_line(&mark.form);
        let this_layer = layer;
        layer += usize::from(is_line);
        if !prepared.shown[i]
            // A collection is a backdrop, as lattice points and a spiral's circles are, and
            // pushing against thousands of pieces of it would be slow and look unsettled.
            || matches!(
                mark.form,
                Form::Label { .. } | Form::Term { .. } | Form::Many { .. } | Form::Outline { .. }
            )
            || opacity_of(style, env)? < 0.01
        {
            continue;
        }
        if is_line {
            let mut segments = Vec::new();
            let mut bounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for s in prepared.strokes[this_layer].chunks(FLOATS_PER_SEGMENT) {
                let at = [s[0], s[1], s[2], s[3]].map(|v| v as f64 / dpr);
                bounds[0] = bounds[0].min(at[0]).min(at[2]);
                bounds[1] = bounds[1].min(at[1]).min(at[3]);
                bounds[2] = bounds[2].max(at[0]).max(at[2]);
                bounds[3] = bounds[3].max(at[1]).max(at[3]);
                segments.push(at);
            }
            if !segments.is_empty() {
                out.push(Obstacle::Line { segments, bounds });
            }
            continue;
        }
        match &mark.form {
            Form::Dot(at) => {
                let (x, y) = frame::device(
                    eval::real(&at[0], env)?,
                    eval::real(&at[1], env)?,
                    &prepared.view,
                    width,
                    height,
                );
                out.push(Obstacle::Point([x / dpr, y / dpr]));
            }
            Form::Field { .. } if style.stroke.is_some() => {
                out.push(Obstacle::Field(fields.len()));
                fields.push(i);
            }
            _ => {}
        }
    }
    Ok((out, fields))
}

/// Where each settling label comes to rest. Others stay put, but still get avoided.
pub fn settle(
    figure: &Figure,
    env: &Env,
    prepared: &mut Prepared,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    if !prepared.labels.iter().any(|l| l.place == Place::Settle) {
        return Ok(());
    }
    let dpr = prepared.dpr;
    let (found, fields) = obstacles(figure, env, prepared, width, height)?;
    let view = prepared.view.clone();
    let (frame_w, frame_h) = (width as f64 / dpr, height as f64 / dpr);
    let extent = |l: &Label| {
        let size = l.size / dpr;
        let half_w = l.text.chars().count() as f64 * EM_PER_CHAR * size / 2.0;
        let half_h = size * LINE_PER_EM / 2.0;
        (half_w, half_h)
    };
    let home: Vec<[f64; 2]> = prepared
        .labels
        .iter()
        .map(|l| [l.x / dpr, l.y / dpr])
        .collect();
    let extents: Vec<(f64, f64)> = prepared.labels.iter().map(extent).collect();
    let moves: Vec<bool> = prepared
        .labels
        .iter()
        .map(|l| l.place == Place::Settle)
        .collect();
    let mut at = home.clone();
    let mut env = env.clone();
    let mut edges: Vec<Vec<Option<Edge>>> = vec![vec![None; fields.len()]; at.len()];
    for step in 0..ITERATIONS {
        if step % EDGE_EVERY == 0 {
            for i in 0..at.len() {
                if !moves[i] {
                    continue;
                }
                let [x, y] = at[i];
                let (sx, sy) = (view.x[1] - view.x[0], view.y[1] - view.y[0]);
                let pixel = (sx / frame_w + sy / frame_h) / 2.0;
                for (k, mark) in fields.iter().enumerate() {
                    let Form::Field {
                        vars, field, grad, ..
                    } = &figure.scene.marks[*mark].form
                    else {
                        continue;
                    };
                    env.set(&vars[0], view.x[0] + x / frame_w * sx);
                    env.set(&vars[1], view.y[1] - y / frame_h * sy);
                    let (gx, gy) = (eval::real(&grad[0], &env)?, eval::real(&grad[1], &env)?);
                    let glen = gx.hypot(gy);
                    edges[i][k] = if glen < MIN_GRADIENT {
                        None
                    } else {
                        // Distance in style pixels, along a normal with y pointing down.
                        Some(Edge {
                            at: [x, y],
                            distance: eval::real(field, &env)? / glen / pixel,
                            normal: [gx / glen, -gy / glen],
                        })
                    };
                }
            }
        }
        let mut next = at.clone();
        for i in 0..at.len() {
            if !moves[i] {
                continue;
            }
            let [x, y] = at[i];
            let (half_w, half_h) = extents[i];
            let (a, b) = (half_w + MARGIN, half_h + MARGIN);
            let weight = FIRMNESS * (REACH * a.min(b)).powi(2) / 4.0;
            let mut force = [home[i][0] - x, home[i][1] - y];
            let (qx, qy) = (x / a, y / b);
            // A bump's push in the label's own units, where it's a unit circle: smooth
            // through zero, so crossing an obstacle needs no direction.
            let mut push = |vx: f64, vy: f64| {
                let s2 = (vx * vx + vy * vy) / (REACH * REACH);
                if s2 < 1.0 {
                    let m = weight * 4.0 * (1.0 - s2) / (REACH * REACH);
                    force[0] += m * vx / a;
                    force[1] += m * vy / b;
                }
            };
            for obstacle in &found {
                match obstacle {
                    Obstacle::Point(p) => push(qx - p[0] / a, qy - p[1] / b),
                    // One curve is one obstacle: its nearest point pushes, not every piece.
                    Obstacle::Line { segments, bounds } => {
                        let (rx, ry) = (REACH * a, REACH * b);
                        if bounds[2] < x - rx
                            || bounds[0] > x + rx
                            || bounds[3] < y - ry
                            || bounds[1] > y + ry
                        {
                            continue;
                        }
                        let mut best = (f64::INFINITY, 0.0, 0.0);
                        for s in segments {
                            let (px, py) = (s[0] / a, s[1] / b);
                            let (ex, ey) = (s[2] / a - px, s[3] / b - py);
                            let len2 = ex * ex + ey * ey;
                            let t = if len2 > 0.0 {
                                (((qx - px) * ex + (qy - py) * ey) / len2).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            let (vx, vy) = (qx - (px + ex * t), qy - (py + ey * t));
                            let d = vx * vx + vy * vy;
                            if d < best.0 {
                                best = (d, vx, vy);
                            }
                        }
                        push(best.1, best.2);
                    }
                    Obstacle::Field(k) => {
                        // A straight edge, as seen in the label's own units.
                        let Some(edge) = edges[i][*k] else { continue };
                        let s = edge.distance
                            + (x - edge.at[0]) * edge.normal[0]
                            + (y - edge.at[1]) * edge.normal[1];
                        let [nx, ny] = edge.normal;
                        let h2 = (nx * a).powi(2) + (ny * b).powi(2);
                        push(s * nx * a / h2, s * ny * b / h2);
                    }
                }
            }
            for j in 0..at.len() {
                if j == i {
                    continue;
                }
                let (sx, sy) = (a + extents[j].0 + MARGIN, b + extents[j].1 + MARGIN);
                let (vx, vy) = ((x - at[j][0]) / sx, (y - at[j][1]) / sy);
                let s2 = (vx * vx + vy * vy) / (PAIR_REACH * PAIR_REACH);
                if s2 < 1.0 {
                    let m = PAIR_FIRMNESS * (1.0 - s2) / (PAIR_REACH * PAIR_REACH);
                    force[0] += m * vx * sx * 0.5;
                    force[1] += m * vy * sy * 0.5;
                }
            }
            force[0] += WALL * (MARGIN + half_w - x).max(0.0);
            force[0] -= WALL * (x + half_w + MARGIN - frame_w).max(0.0);
            force[1] += WALL * (MARGIN + half_h - y).max(0.0);
            force[1] -= WALL * (y + half_h + MARGIN - frame_h).max(0.0);
            next[i] = [x + STEP * force[0], y + STEP * force[1]];
        }
        at = next;
    }
    for (i, (label, p)) in prepared.labels.iter_mut().zip(at).enumerate() {
        if !moves[i] {
            continue;
        }
        // The wall is soft, so a clamp makes it certain. A clamp can't make a jump.
        let (half_w, half_h) = extents[i];
        label.x = p[0].clamp(
            half_w.min(frame_w / 2.0),
            (frame_w - half_w).max(frame_w / 2.0),
        ) * dpr;
        label.y = p[1].clamp(
            half_h.min(frame_h / 2.0),
            (frame_h - half_h).max(frame_h / 2.0),
        ) * dpr;
    }
    Ok(())
}

/// Each settling label's offset from its point at a past time, by label id. A figure's
/// past doesn't change, so a caller may keep these for as long as the inputs hold.
pub type SpringCache = HashMap<i64, Vec<(String, [f64; 2])>>;

/// Puts the second spring on a frame's settling labels: a weighted average over a window of
/// earlier times, on a fixed grid so that it needs no history and any order of frames gives
/// the same result. Time starts at 0, so a figure starts at rest, as does a label that has
/// just appeared.
pub fn spring(
    figure: &Figure,
    inputs: &Inputs,
    width: u32,
    height: u32,
    prepared: &mut Prepared,
    cache: &mut SpringCache,
) -> Result<(), Error> {
    if !prepared.labels.iter().any(|l| l.place == Place::Settle) {
        return Ok(());
    }
    let now = (inputs.time * SPRING_SAMPLES_PER_SECOND).floor() as i64;
    let indices: Vec<i64> = (0..SPRING_WINDOW)
        .map(|j| (now - j as i64).max(0))
        .collect();
    for index in &indices {
        if let std::collections::hash_map::Entry::Vacant(slot) = cache.entry(*index) {
            let past = Inputs {
                time: *index as f64 / SPRING_SAMPLES_PER_SECOND,
                params: inputs.params.clone(),
            };
            let settled = frame::prepare_with(
                figure,
                &past,
                width,
                height,
                prepared.dpr,
                prepared.em,
                &crate::terms::Estimate,
            )?;
            slot.insert(
                settled
                    .labels
                    .iter()
                    .map(|l| (l.id.clone(), [l.x - l.anchor[0], l.y - l.anchor[1]]))
                    .collect(),
            );
        }
    }
    for label in &mut prepared.labels {
        if label.place != Place::Settle {
            continue;
        }
        // Back through time until the label didn't exist. It appeared at rest, so all the
        // time before that counts as being where it first was, as on a page.
        let mut held = [label.x - label.anchor[0], label.y - label.anchor[1]];
        let mut here = true;
        let mut sum = [0.0; 2];
        let mut total = 0.0;
        for (j, index) in indices.iter().enumerate() {
            let found = cache[index].iter().find(|(id, _)| *id == label.id);
            here = here && found.is_some();
            if let (true, Some((_, offset))) = (here, found) {
                held = *offset;
            }
            let tau = j as f64 / SPRING_SAMPLES_PER_SECOND;
            let weight = tau * (-SPRING_RATE * tau).exp();
            sum[0] += weight * held[0];
            sum[1] += weight * held[1];
            total += weight;
        }
        if total > 0.0 {
            label.x = label.anchor[0] + sum[0] / total;
            label.y = label.anchor[1] + sum[1] / total;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::frame::prepare;

    /// A line sweeps across a label's home as `a` goes from −1 to 1.
    const SWEEP: &str = "
        viewport { x: -2..2, y: -2..2 } style figure { width: 400px; height: 400px }
        param a = 0 in -2..2
        bind a = time - 2
        let P = (0, 0)
        segment s = (a, -2), (a, 2)
        label n = \"P\" at P";
    const SETTLE: &str = "
        style s { stroke: black; width: 2px }
        style n { place: settle; size: 16px }";

    fn with(a: f64) -> Inputs {
        Inputs {
            time: 0.0,
            params: vec![("a".to_string(), a)],
        }
    }

    fn label_x(f: &Figure, a: f64) -> f64 {
        prepare(f, &with(a), 400, 400, 1.0).unwrap().labels[0].x
    }

    #[test]
    fn a_fixed_label_stays_at_its_offset() {
        let f = build(SWEEP, &["style s { stroke: black; width: 2px }"]).unwrap();
        assert_eq!(label_x(&f, 0.05), 200.0);
    }

    #[test]
    fn a_settling_label_moves_away_from_a_line() {
        let f = build(SWEEP, &[SETTLE]).unwrap();
        // World x = 0.05 is 5 px right of the label, so it goes left.
        assert!(label_x(&f, 0.05) < 195.0, "{}", label_x(&f, 0.05));
        assert!(label_x(&f, -0.05) > 205.0, "{}", label_x(&f, -0.05));
        // Far away, it's at home.
        assert_eq!(label_x(&f, 1.5), 200.0);
    }

    #[test]
    fn labels_avoid_each_other_and_do_not_depend_on_their_order() {
        let doc = |first: &str, second: &str| {
            format!(
                "viewport {{ x: -2..2, y: -2..2 }} style figure {{ width: 400px; height: 400px }}
                 let P = (0, 0)
                 label {first} = \"A\" at P offset (-1, 0)
                 label {second} = \"B\" at P offset (1, 0)"
            )
        };
        let sheet = "style label { place: settle; size: 16px }";
        let f = build(&doc("u", "v"), &[sheet]).unwrap();
        let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
        let gap = p.labels[1].x - p.labels[0].x;
        assert!(gap > 12.0, "{gap}");
        // Pushed apart equally: nothing favors the one placed first.
        let mean = (p.labels[0].x + p.labels[1].x) / 2.0;
        assert!((mean - 200.0).abs() < 1e-9, "{mean}");
    }

    #[test]
    fn a_label_stays_in_frame() {
        let doc = "viewport { x: -2..2, y: -2..2 } style figure { width: 400px; height: 400px }
                   let P = (1.99, 0)
                   label n = \"Long\" at P";
        let f = build(doc, &["style n { place: settle; size: 16px }"]).unwrap();
        let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
        assert!(p.labels[0].x <= 400.0 - 17.6 + 1e-9, "{}", p.labels[0].x);
    }

    #[test]
    fn it_avoids_a_curve_drawn_per_pixel() {
        let doc = "viewport { x: -2..2, y: -2..2 } style figure { width: 400px; height: 400px }
                   let P = (1, 0)
                   curve c = circle((0, 0), (1, 0))
                   label n = \"P\" at P offset (3, 0)";
        let f = build(
            doc,
            &["style c { stroke: black; width: 2px }
               style n { place: settle; size: 16px }"],
        )
        .unwrap();
        let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
        assert!(p.labels[0].x > 303.0 + 3.0, "{}", p.labels[0].x);
    }

    #[test]
    fn a_hidden_mark_is_not_in_the_way() {
        let f = build(
            SWEEP,
            &["style s { stroke: black; width: 2px; opacity: 0 }
               style n { place: settle; size: 16px }"],
        )
        .unwrap();
        assert_eq!(label_x(&f, 0.05), 200.0);
    }

    /// Kris's test: a label moves only smoothly, even as a line crosses its home.
    #[test]
    fn the_spring_removes_the_leap_and_needs_no_history() {
        let f = build(SWEEP, &[SETTLE]).unwrap();
        let at = |t: f64, cache: &mut SpringCache| {
            let inputs = Inputs::at(t);
            let mut p = prepare(&f, &inputs, 400, 400, 1.0).unwrap();
            spring(&f, &inputs, 400, 400, &mut p, cache).unwrap();
            p.labels[0].x
        };
        let settled = |t: f64| prepare(&f, &Inputs::at(t), 400, 400, 1.0).unwrap().labels[0].x;
        let mut cache = SpringCache::new();
        let (mut leap, mut smooth) = (0.0f64, 0.0f64);
        let (mut was_settled, mut was) = (settled(1.5), at(1.5, &mut cache));
        for i in 1..=60 {
            let t = 1.5 + i as f64 / 60.0;
            let (now_settled, now) = (settled(t), at(t, &mut cache));
            leap = leap.max((now_settled - was_settled).abs());
            smooth = smooth.max((now - was).abs());
            was_settled = now_settled;
            was = now;
        }
        assert!(leap > 8.0, "the crossing leaps without it: {leap}");
        assert!(smooth < 3.0, "and glides with it: {smooth}");
        // Seeking straight to a time gives what playing up to it gave.
        let played = at(2.0, &mut cache);
        let sought = at(2.0, &mut SpringCache::new());
        assert_eq!(played, sought);
    }
}
