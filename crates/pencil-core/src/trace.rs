//! Finds an implicit curve's zero set as polylines: marching squares for topology,
//! then Newton steps along the gradient so every vertex lands on the curve.
//!
//! Polylines keep the orientation convention: the negative side is on the left of
//! travel, so a counterclockwise loop encloses negative. Results are deterministic,
//! so a frame stays a pure function of its inputs.
//!
//! A change of sign along a grid edge is a crossing only where the field goes to zero. Across a
//! pole, as `y - tan(x)` has at x = π/2, it goes through infinity instead, and across a jump it
//! skips zero, so neither is a crossing and the curve breaks there.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::Error;
use crate::eval::{self, Env};
use crate::fast;
use crate::ir::{Expr, Viewport};

const NEWTON_STEPS: usize = 3;
/// How many times an edge whose ends differ in sign is halved, to see whether the field goes to
/// zero there: to 2⁻¹² of a cell.
const HALVINGS: usize = 12;

#[derive(Clone, Debug, PartialEq)]
pub struct Polyline {
    /// World coordinates. A closed polyline repeats its first point at the end.
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
    /// A parametric curve's parameter at each point, for what a style works out along it.
    /// Empty for a line that has none.
    pub params: Vec<f64>,
}

/// A grid edge: `(0, i, j)` runs right from corner `(i, j)`, `(1, i, j)` runs up.
type EdgeId = (u8, u32, u32);

struct Field<'a> {
    vars: &'a [String; 2],
    field: &'a Expr,
    grad: &'a [Expr; 2],
    env: Env,
    /// The field and its gradient resolved once, since a trace works them out at every corner of
    /// its grid, every frame. `eval` stays the definition, and is asked whenever these can't say.
    slots: fast::Slots,
    at: [u32; 2],
    quick: Option<[fast::Node; 3]>,
}

impl<'a> Field<'a> {
    fn new(vars: &'a [String; 2], field: &'a Expr, grad: &'a [Expr; 2], env: &Env) -> Self {
        let mut slots = fast::Slots::new();
        let at = [slots.variable(&vars[0]), slots.variable(&vars[1])];
        let quick = (|| {
            Some([
                fast::compile(field, &mut slots, env)?,
                fast::compile(&grad[0], &mut slots, env)?,
                fast::compile(&grad[1], &mut slots, env)?,
            ])
        })();
        Field {
            vars,
            field,
            grad,
            env: env.clone(),
            slots,
            at,
            quick,
        }
    }

    fn at(&mut self, p: [f64; 2]) -> Result<f64, Error> {
        if let Some([field, ..]) = &self.quick {
            self.slots.set(self.at[0], p[0]);
            self.slots.set(self.at[1], p[1]);
            if let Ok(value) = fast::run(field, &self.slots) {
                return Ok(value);
            }
        }
        self.env.set(&self.vars[0], p[0]);
        self.env.set(&self.vars[1], p[1]);
        eval::real(self.field, &self.env)
    }

    /// The gradient at `p`.
    fn slope(&mut self, p: [f64; 2]) -> Result<[f64; 2], Error> {
        if let Some([_, gx, gy]) = &self.quick {
            self.slots.set(self.at[0], p[0]);
            self.slots.set(self.at[1], p[1]);
            if let (Ok(gx), Ok(gy)) = (fast::run(gx, &self.slots), fast::run(gy, &self.slots)) {
                return Ok([gx, gy]);
            }
        }
        self.env.set(&self.vars[0], p[0]);
        self.env.set(&self.vars[1], p[1]);
        Ok([
            eval::real(&self.grad[0], &self.env)?,
            eval::real(&self.grad[1], &self.env)?,
        ])
    }

    /// Newton steps along the gradient, which is safe where the curve crosses zero.
    fn project(&mut self, mut p: [f64; 2]) -> Result<[f64; 2], Error> {
        for _ in 0..NEWTON_STEPS {
            let f = self.at(p)?;
            let [gx, gy] = self.slope(p)?;
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

    /// Whether the field goes to zero where it changes sign between `a` and `b`, which hold `fa`
    /// and `fb`. Halving keeps the change of sign between the two ends, so they close in on where
    /// it is, and it's a zero once the field there has fallen below half the largest the edge has
    /// shown. At a zero it falls toward 0, by about 2¹² where the field is smooth. At a pole it
    /// only grows, and across a jump it keeps about the size of the jump. A distance,
    /// `|f| / |∇f|`, can't tell them apart: it goes to 0 at a pole too.
    fn goes_to_zero(
        &mut self,
        mut a: [f64; 2],
        mut b: [f64; 2],
        mut fa: f64,
        mut fb: f64,
    ) -> Result<bool, Error> {
        // `max` passes over a NaN, where the field has no value, which is on the side that isn't
        // negative, as at the corners.
        let mut peak = fa.abs().max(fb.abs());
        for halving in 1..=HALVINGS {
            let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            let fm = self.at(m)?;
            peak = peak.max(fm.abs());
            if (fm < 0.0) == (fa < 0.0) {
                (a, fa) = (m, fm);
            } else {
                (b, fb) = (m, fm);
            }
            // Where the field changes in step with the distance, it's down to a quarter by the
            // fourth halving, so most zeros are settled there. On either side of a pole the
            // field grows toward it, and on either side of a jump it keeps about the jump's size,
            // so what is below half by then is still below half at the end, and stopping early
            // doesn't change the answer.
            if halving % 4 != 0 || fa.abs().max(fb.abs()) >= peak / 2.0 {
                continue;
            }
            // An end can land on the very point of a jump, as a halving lands on x = 0, where
            // `sign(x)` is neither side's value. A step past each end shows the side it stands for.
            let step = [b[0] - a[0], b[1] - a[1]];
            let past = [
                self.at([a[0] - step[0], a[1] - step[1]])?,
                self.at([b[0] + step[0], b[1] + step[1]])?,
            ];
            if !past.iter().any(|f| f.abs() >= peak / 2.0) {
                return Ok(true);
            }
        }
        Ok(false)
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
    let mut field = Field::new(vars, expr, grad, env);
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

    // The edges that change sign without going through zero, across a pole or a jump. Only an
    // edge that changes sign costs anything more.
    let mut poles: BTreeSet<EdgeId> = BTreeSet::new();
    for j in 0..=ny {
        for i in 0..=nx {
            for (kind, to) in [(0, (i + 1, j)), (1, (i, j + 1))] {
                if to.0 > nx || to.1 > ny {
                    continue;
                }
                let (fa, fb) = (value(i, j), value(to.0, to.1));
                if (fa < 0.0) != (fb < 0.0)
                    && !field.goes_to_zero(corner(i, j), corner(to.0, to.1), fa, fb)?
                {
                    poles.insert((kind, i, j));
                }
            }
        }
    }

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
            // A cell a pole or a jump runs through joins nothing. A crossing beside it is closer to
            // it than the grid can resolve, so which side the curve goes on can't be told, and a
            // line between them would run along it. The curve ends at the cell's edge instead.
            if crossings.iter().any(|k| poles.contains(&edges[*k])) {
                continue;
            }
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
        lines.push(Polyline {
            points,
            closed,
            params: Vec::new(),
        });
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

    const WIDE: &str = "viewport { x: 0 ± 4, y: 0 ± 3 }\n";
    const NARROW: &str = "viewport { x: 0 ± 1, y: 0 ± 0.75 }\n";
    /// Grids with a corner on the origin and without, so a pole at x = 0 falls on a corner, where
    /// the field is infinite, or inside a cell.
    const GRIDS: [u32; 3] = [47, 64, 101];

    /// The farthest any traced vertex is from the curve, by `distance`, which doesn't use the trace.
    fn farthest(found: &[Polyline], distance: impl Fn([f64; 2]) -> f64) -> f64 {
        found
            .iter()
            .flat_map(|l| &l.points)
            .map(|p| distance(*p))
            .fold(0.0, f64::max)
    }

    /// The distance from `p` to the graph y = g(x), sampled densely within 0.01 of it either way
    /// in x, and most densely near `p`, so a steep graph is sampled finely enough too.
    fn to_graph(g: impl Fn(f64) -> f64, p: [f64; 2]) -> f64 {
        (0..=4000)
            .map(|k| p[0] + 0.01 * (k as f64 / 2000.0 - 1.0).powi(3))
            .map(|x| (x - p[0]).hypot(g(x) - p[1]))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether any traced segment runs from one side of the line x = `c` to the other.
    fn crosses(found: &[Polyline], c: f64) -> bool {
        found
            .iter()
            .any(|l| l.points.windows(2).any(|w| (w[0][0] < c) != (w[1][0] < c)))
    }

    /// Across a pole the field changes sign through infinity, with no zero between, so nothing is
    /// traced along it, and each branch is one line. `x·y = 1` has no pole, since its field is
    /// `x·y - 1`, and is here as the same curve written without one.
    #[test]
    fn a_pole_is_not_traced() {
        use std::f64::consts::FRAC_PI_2;
        type Case = (&'static str, fn(f64) -> f64, &'static [f64], usize);
        let cases: [Case; 4] = [
            ("y = tan(x)", f64::tan, &[-FRAC_PI_2, FRAC_PI_2], 3),
            ("y = 1/x", |x| 1.0 / x, &[0.0], 2),
            ("x·y = 1", |x| 1.0 / x, &[0.0], 2),
            ("y = 1/(x² - 1)", |x| 1.0 / (x * x - 1.0), &[-1.0, 1.0], 3),
        ];
        for (curve, g, asymptotes, branches) in cases {
            for cells in GRIDS {
                let found = lines(&format!("{WIDE}curve c = implicit (x, y): {curve}"), cells);
                let off = farthest(&found, |p| to_graph(g, p));
                assert!(
                    off < 1e-3,
                    "{curve}, {cells} cells: a vertex {off} off the curve"
                );
                for c in asymptotes {
                    assert!(
                        !crosses(&found, *c),
                        "{curve}, {cells} cells: a line across x = {c}"
                    );
                }
                assert_eq!(found.len(), branches, "{curve}, {cells} cells");
            }
        }
    }

    /// A jump that skips zero isn't a crossing either: not the cut where `atan2` turns from π to
    /// −π, nor a staircase's risers, nor a jump the field falls toward from both sides.
    #[test]
    fn a_jump_is_not_traced() {
        let at =
            |curve: &str, cells| lines(&format!("{WIDE}curve c = implicit (x, y): {curve}"), cells);
        for cells in GRIDS {
            let ray = at("atan2(y, x) = 1", cells);
            assert!(!ray.is_empty());
            let (u, v) = (1f64.cos(), 1f64.sin());
            // Across the ray, and back past its start.
            let off = farthest(&ray, |p| {
                (p[0] * v - p[1] * u).abs().max(-(p[0] * u + p[1] * v))
            });
            assert!(off < 1e-3, "{cells} cells: a vertex {off} off the ray");

            let stairs = at("y = floor(x)", cells);
            assert!(stairs.len() > 4, "{cells} cells");
            let off = farthest(&stairs, |p| (p[1] - p[0].floor()).abs());
            assert!(off < 1e-3, "{cells} cells: a vertex {off} off the steps");
            for line in &stairs {
                let y = line.points[0][1];
                assert!(
                    line.points.iter().all(|p| p[1] == y),
                    "{cells} cells: a riser"
                );
            }

            let split = at("y = x + sign(x)", cells);
            assert!(!crosses(&split, 0.0), "{cells} cells");
            let off = farthest(&split, |p| to_graph(|x| x + x.signum(), p));
            assert!(off < 1e-3, "{cells} cells: a vertex {off} off the curve");
        }
    }

    /// Zeros the field crosses steeply, flatly or with a vertical tangent are still crossings, so
    /// each of these is one whole line from the bottom of the grid to the top.
    #[test]
    fn steep_and_flat_zeros_trace_whole() {
        type Case = (&'static str, &'static str, Option<fn(f64) -> f64>, f64);
        let cases: [Case; 3] = [
            (WIDE, "y = 1000·x", Some(|x| 1000.0 * x), 3.0),
            (NARROW, "y = x^3", Some(|x| x * x * x), 0.75),
            // Its field falls to zero as the cube root of the distance across its vertical
            // tangent. Newton steps there move away from the curve, as they do for any cube root,
            // so only that the line is whole is checked.
            (NARROW, "y = sign(x)·abs(x)^(1/3)", None, 0.75),
        ];
        for (view, curve, g, top) in cases {
            for cells in GRIDS {
                let found = lines(&format!("{view}curve c = implicit (x, y): {curve}"), cells);
                assert_eq!(found.len(), 1, "{curve}, {cells} cells");
                let ys = found[0].points.iter().map(|p| p[1]);
                let (low, high) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), y| {
                    (l.min(y), h.max(y))
                });
                assert!(
                    low < -top && high > top,
                    "{curve}, {cells} cells: {low}..{high}"
                );
                if let Some(g) = g {
                    let off = farthest(&found, |p| to_graph(g, p));
                    assert!(
                        off < 1e-3,
                        "{curve}, {cells} cells: a vertex {off} off the curve"
                    );
                }
            }
        }
        // A circle whose field is level away from it and steep at it, from −1 to 1 within a cell
        // or two. Newton steps from where the field is level go far, so here too only that the
        // loop is whole is checked.
        for cells in GRIDS {
            let doc = "viewport { x: 0 ± 2, y: 0 ± 1.5 }\n\
                       curve c = implicit (x, y): tanh(50·(x² + y² - 1)) = 0";
            let found = lines(doc, cells);
            assert_eq!(found.len(), 1, "{cells} cells");
            assert!(found[0].closed, "{cells} cells");
        }
    }

    /// The lemniscate's double point, on a corner of the grid or inside a cell, leaves its loops
    /// closed, and every vertex is on it, by its rational parametrization.
    #[test]
    fn a_double_point_traces_whole() {
        let doc = "viewport { x: 0 ± 1.2, y: 0 ± 0.9 }\n\
                   curve c = implicit (x, y): (x² + y²)² = x² - y²";
        let samples: Vec<[f64; 2]> = (0..50_000)
            .map(|k| k as f64 / 50_000.0 * std::f64::consts::TAU)
            .map(|t| {
                let s = 1.0 + t.sin().powi(2);
                [t.cos() / s, t.sin() * t.cos() / s]
            })
            .collect();
        for cells in GRIDS {
            let found = lines(doc, cells);
            assert!(!found.is_empty());
            assert!(found.iter().all(|l| l.closed), "{cells} cells");
            let off = farthest(&found, |p| {
                samples
                    .iter()
                    .map(|q| (q[0] - p[0]).hypot(q[1] - p[1]))
                    .fold(f64::INFINITY, f64::min)
            });
            assert!(off < 1e-3, "{cells} cells: a vertex {off} off the curve");
            // Its length is 2ϖ, ϖ being the lemniscate constant.
            let length: f64 = found
                .iter()
                .flat_map(|l| l.points.windows(2))
                .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
                .sum();
            assert!(
                (length / 5.244_115_108_584_24 - 1.0).abs() < 0.01,
                "{cells}: {length}"
            );
        }
    }

    /// Whether `goes_to_zero` takes the edge from `a` to `b` for a zero of the curve's field.
    fn zero_between(curve: &str, a: [f64; 2], b: [f64; 2]) -> bool {
        let figure = build(&format!("curve c = implicit (x, y): {curve}"), &[]).unwrap();
        let env = eval::frame_env(&figure, &crate::frame::Inputs::at(0.0)).unwrap();
        let Form::Field {
            vars, field, grad, ..
        } = &figure.scene.marks[0].form
        else {
            panic!("not a field");
        };
        let mut f = Field::new(vars, field, grad, &env);
        let (fa, fb) = (f.at(a).unwrap(), f.at(b).unwrap());
        assert!(
            (fa < 0.0) != (fb < 0.0),
            "{curve}: {fa} and {fb} have one sign"
        );
        f.goes_to_zero(a, b, fa, fb).unwrap()
    }

    #[test]
    fn a_zero_falls_and_a_pole_grows() {
        // Poles inside an edge, and at its end, where the field is infinite.
        assert!(!zero_between("y = 1/x", [-0.1, 0.5], [0.07, 0.5]));
        assert!(!zero_between("y = 1/x", [-0.1, 0.5], [0.0, 0.5]));
        assert!(!zero_between("y = tan(x)", [1.5, 0.3], [1.6, 0.3]));
        assert!(!zero_between("y = 1/(x - 0.3)^3", [0.25, 0.0], [0.4, 0.0]));
        // Jumps that skip zero.
        assert!(!zero_between("y = sign(x)", [-0.1, 0.5], [0.07, 0.5]));
        assert!(!zero_between("atan2(y, x) = 1", [-1.0, -0.1], [-1.0, 0.07]));
        // Zeros, steep and flat, with a vertical tangent, with the field level either side, at
        // the end of the edge, and beside a pole at the end.
        assert!(zero_between("y = 1000·x", [-0.1, 0.5], [0.07, 0.5]));
        assert!(zero_between("y = x^3", [-0.1, 0.0001], [0.07, 0.0001]));
        assert!(zero_between(
            "y = sign(x)·abs(x)^(1/3)",
            [-0.1, 0.0],
            [0.07, 0.0]
        ));
        assert!(zero_between("tanh(1000·x) = 0", [-0.1, 0.5], [0.07, 0.5]));
        assert!(zero_between("y = x", [-0.1, 0.07], [0.08, 0.07]));
        assert!(zero_between("1/x² = 4", [-1.0, 0.0], [0.0, 0.0]));
        // A zero next to one end, with the field rising far above both ends between them, which
        // is why the field is compared with the largest the edge shows and not its ends.
        assert!(zero_between("sin(x) = y", [-0.001, 0.0], [3.13, 0.0]));
    }
}
