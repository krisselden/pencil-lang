//! Finds an implicit curve's zero set as polylines: marching squares for topology,
//! then Newton steps along the gradient so every vertex lands on the curve.
//!
//! Polylines keep the orientation convention: the negative side is on the left of
//! travel, so a counterclockwise loop encloses negative. Results are deterministic,
//! so a frame stays a pure function of its inputs.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::Error;
use crate::eval::{self, Env};
use crate::ir::{Expr, Viewport};

const NEWTON_STEPS: usize = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct Polyline {
    /// World coordinates. A closed polyline repeats its first point at the end.
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
}

/// A grid edge: `(0, i, j)` runs right from corner `(i, j)`, `(1, i, j)` runs up.
type EdgeId = (u8, u32, u32);

struct Field<'a> {
    vars: &'a [String; 2],
    field: &'a Expr,
    grad: &'a [Expr; 2],
    env: Env,
}

impl Field<'_> {
    fn at(&mut self, p: [f64; 2]) -> Result<f64, Error> {
        self.env.set(&self.vars[0], p[0]);
        self.env.set(&self.vars[1], p[1]);
        eval::real(self.field, &self.env)
    }

    /// Newton steps along the gradient, which is safe where the curve crosses zero.
    fn project(&mut self, mut p: [f64; 2]) -> Result<[f64; 2], Error> {
        for _ in 0..NEWTON_STEPS {
            let f = self.at(p)?;
            let gx = eval::real(&self.grad[0], &self.env)?;
            let gy = eval::real(&self.grad[1], &self.env)?;
            let g2 = gx * gx + gy * gy;
            // Where the gradient vanishes or is infinite, as at a cusp, a step goes nowhere useful.
            if !(1e-12..f64::INFINITY).contains(&g2) {
                break;
            }
            let next = [p[0] - f * gx / g2, p[1] - f * gy / g2];
            if !(next[0].is_finite() && next[1].is_finite()) {
                break;
            }
            p = next;
        }
        Ok(p)
    }
}

/// Traces a curve over a `cols` by `rows` grid covering the viewport, with `margin` extra cells
/// on every side, at least one, so open curves run past the edge instead of ending at it.
#[allow(clippy::too_many_arguments)]
pub fn trace(
    vars: &[String; 2],
    expr: &Expr,
    grad: &[Expr; 2],
    env: &Env,
    view: &Viewport,
    cols: u32,
    rows: u32,
    margin: u32,
) -> Result<Vec<Polyline>, Error> {
    let margin = margin.max(1);
    let mut field = Field {
        vars,
        field: expr,
        grad,
        env: env.clone(),
    };
    let (cell_x, cell_y) = (
        (view.x[1] - view.x[0]) / cols as f64,
        (view.y[1] - view.y[0]) / rows as f64,
    );
    let (nx, ny) = (cols + 2 * margin, rows + 2 * margin);
    let corner = |i: u32, j: u32| {
        [
            view.x[0] + (i as f64 - margin as f64) * cell_x,
            view.y[0] + (j as f64 - margin as f64) * cell_y,
        ]
    };

    let mut values = Vec::with_capacity(((nx + 1) * (ny + 1)) as usize);
    for j in 0..=ny {
        for i in 0..=nx {
            values.push(field.at(corner(i, j))?);
        }
    }
    let value = |i: u32, j: u32| values[(j * (nx + 1) + i) as usize];

    // Each segment runs from where the perimeter (walked counterclockwise) goes
    // negative to positive, to where it goes positive to negative. That keeps the
    // negative side on the left.
    let mut next: BTreeMap<EdgeId, EdgeId> = BTreeMap::new();
    for j in 0..ny {
        for i in 0..nx {
            let f = [
                value(i, j),
                value(i + 1, j),
                value(i + 1, j + 1),
                value(i, j + 1),
            ];
            let edges: [EdgeId; 4] = [(0, i, j), (1, i + 1, j), (0, i, j + 1), (1, i, j)];
            let crossing = |k: usize| (f[k] < 0.0) != (f[(k + 1) % 4] < 0.0);
            let leaves = |k: usize| f[k] < 0.0 && f[(k + 1) % 4] >= 0.0;
            let crossings: Vec<usize> = (0..4).filter(|k| crossing(*k)).collect();
            match crossings.as_slice() {
                [] => {}
                [a, b] => {
                    let (start, end) = if leaves(*a) { (*a, *b) } else { (*b, *a) };
                    next.insert(edges[start], edges[end]);
                }
                _ => {
                    // A saddle: the center's sign says which corners connect.
                    let center_negative = f.iter().sum::<f64>() < 0.0;
                    let first = (0..4).find(|k| leaves(*k)).unwrap_or(0);
                    let (a, b, c, d) = (first, first + 1, first + 2, first + 3);
                    let pairs = if center_negative {
                        [(a, b % 4), (c % 4, d % 4)]
                    } else {
                        [(a, d % 4), (c % 4, b % 4)]
                    };
                    for (start, end) in pairs {
                        next.insert(edges[start], edges[end]);
                    }
                }
            }
        }
    }

    let targets: BTreeSet<EdgeId> = next.values().copied().collect();
    let mut vertices: BTreeMap<EdgeId, [f64; 2]> = BTreeMap::new();
    let mut vertex = |id: EdgeId, field: &mut Field| -> Result<[f64; 2], Error> {
        if let Some(p) = vertices.get(&id) {
            return Ok(*p);
        }
        let (kind, i, j) = id;
        let (a, b) = (
            corner(i, j),
            if kind == 0 {
                corner(i + 1, j)
            } else {
                corner(i, j + 1)
            },
        );
        let (fa, fb) = (
            value(i, j),
            if kind == 0 {
                value(i + 1, j)
            } else {
                value(i, j + 1)
            },
        );
        let t = fa / (fa - fb);
        let p = field.project([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])?;
        vertices.insert(id, p);
        Ok(p)
    };

    let mut lines = Vec::new();
    let mut visited: BTreeSet<EdgeId> = BTreeSet::new();
    // Open curves first, from the edges nothing leads into.
    let starts: Vec<EdgeId> = next
        .keys()
        .filter(|id| !targets.contains(id))
        .copied()
        .chain(next.keys().copied())
        .collect();
    for start in starts {
        if visited.contains(&start) {
            continue;
        }
        let mut points = vec![vertex(start, &mut field)?];
        visited.insert(start);
        let mut at = start;
        let mut closed = false;
        while let Some(&to) = next.get(&at) {
            if to == start {
                closed = true;
                break;
            }
            points.push(vertex(to, &mut field)?);
            if !visited.insert(to) {
                break;
            }
            at = to;
        }
        points.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-12);
        if points.len() < 2 {
            continue;
        }
        if closed {
            points = canonical_start(points, &mut field)?;
        }
        lines.push(Polyline { points, closed });
    }
    Ok(lines)
}

/// Starts a loop where it crosses the horizontal through its centroid on the +x
/// side, and closes it. Grid vertices move as the curve moves, so starting at one
/// would make dashes jitter. For a circle this start is exactly angle 0.
fn canonical_start(points: Vec<[f64; 2]>, field: &mut Field) -> Result<Vec<[f64; 2]>, Error> {
    let n = points.len();
    let cy = points.iter().map(|p| p[1]).sum::<f64>() / n as f64;
    let mut best: Option<(usize, [f64; 2])> = None;
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        if (a[1] < cy) != (b[1] < cy) {
            let t = (cy - a[1]) / (b[1] - a[1]);
            let p = [a[0] + (b[0] - a[0]) * t, cy];
            if best.is_none_or(|(_, q)| p[0] > q[0]) {
                best = Some((i, p));
            }
        }
    }
    let Some((i, p)) = best else {
        let mut closed = points;
        closed.push(closed[0]);
        return Ok(closed);
    };
    let start = field.project(p)?;
    let mut out = Vec::with_capacity(n + 2);
    out.push(start);
    for k in 1..=n {
        out.push(points[(i + k) % n]);
    }
    out.push(start);
    out.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-12);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::ir::Form;

    fn lines(doc: &str, cells: u32) -> Vec<Polyline> {
        let figure = build(doc, &[]).unwrap();
        let env = eval::frame_env(&figure, &crate::frame::Inputs::at(0.0)).unwrap();
        let Form::Field {
            vars, field, grad, ..
        } = &figure.scene.marks[0].form
        else {
            panic!("not a field");
        };
        trace(
            vars,
            field,
            grad,
            &env,
            &figure.scene.viewport,
            cells,
            cells,
            1,
        )
        .unwrap()
    }

    fn signed_area(points: &[[f64; 2]]) -> f64 {
        points
            .windows(2)
            .map(|w| w[0][0] * w[1][1] - w[1][0] * w[0][1])
            .sum::<f64>()
            / 2.0
    }

    const CIRCLE: &str = "curve c = implicit (x, y): x^2 + y^2 = 1.3225";

    /// An astroid's gradient is infinite at its cusps, which lie on grid lines here, and a Newton
    /// step from there used to land on NaN.
    #[test]
    fn a_cusp_keeps_every_vertex_finite() {
        let doc = "viewport { x: -1.6..1.6, y: -1.6..1.6 }\n\
                   curve c = implicit (x, y): abs(x)^(2/3) + abs(y)^(2/3) = 1";
        let found = lines(doc, 64);
        assert!(!found.is_empty());
        for p in found.iter().flat_map(|l| &l.points) {
            assert!(p[0].is_finite() && p[1].is_finite(), "{p:?}");
        }
    }

    #[test]
    fn vertices_land_on_the_curve() {
        let found = lines(CIRCLE, 64);
        assert_eq!(found.len(), 1);
        assert!(found[0].closed);
        for p in &found[0].points {
            assert!((p[0].hypot(p[1]) - 1.15).abs() < 1e-10, "{p:?}");
        }
    }

    #[test]
    fn loops_run_counterclockwise_around_negative() {
        let found = lines(CIRCLE, 64);
        let area = signed_area(&found[0].points);
        assert!(
            (area - std::f64::consts::PI * 1.3225).abs() < 0.01,
            "{area}"
        );
    }

    /// Writing the equation the other way round reverses travel, as the spec says.
    #[test]
    fn swapping_the_equation_reverses_the_loop() {
        let found = lines("curve c = implicit (x, y): 1.3225 = x^2 + y^2", 64);
        assert!(signed_area(&found[0].points) < 0.0);
    }

    #[test]
    fn a_circle_starts_at_angle_zero_at_any_grid_size() {
        for cells in [32, 47, 64, 101] {
            let found = lines(CIRCLE, cells);
            let p = found[0].points[0];
            assert!(
                p[1].abs() < 1e-3 && (p[0] - 1.15).abs() < 1e-3,
                "{cells}: {p:?}"
            );
            assert_eq!(found[0].points.first(), found[0].points.last());
        }
    }

    #[test]
    fn open_curves_run_past_the_viewport_with_negative_on_the_left() {
        let found = lines("curve c = implicit (x, y): y = 0.5 * sin(3 * x)", 64);
        assert_eq!(found.len(), 1);
        let line = &found[0];
        assert!(!line.closed);
        let (first, last) = (line.points[0], line.points[line.points.len() - 1]);
        // Negative is below, so with it on the left, travel runs right to left.
        assert!(first[0] > 2.0 && last[0] < -2.0, "{first:?} {last:?}");
    }

    #[test]
    fn saddles_connect_without_crossing() {
        // Both signs meet at the origin, which lands exactly on a grid corner.
        let found = lines("curve c = implicit (x, y): x * y = 0.01", 16);
        assert_eq!(found.len(), 2);
        // Off-center grids hit the saddle cell itself.
        let found = lines("curve c = implicit (x, y): x * y = 0.0001", 7);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn tracing_is_deterministic() {
        assert_eq!(lines(CIRCLE, 64), lines(CIRCLE, 64));
    }

    #[test]
    fn a_curve_outside_the_view_traces_to_nothing() {
        assert!(lines("curve c = implicit (x, y): x^2 + y^2 = 100", 16).is_empty());
    }
}
