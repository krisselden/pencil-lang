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
//!
//! Each cell's line is checked against the curve: against the curve's direction where it crosses
//! the cell's edges, and the field's sign either side of its middle. Where it strays by more than
//! a quarter of a pixel, as across a tip narrower than a cell, it's bent onto the curve, halved at
//! the curve until every piece is near it, without leaving its cell by more than a fraction of a
//! pixel, so no two cells' lines follow the same piece of the curve. A bend that doesn't settle is
//! a line from one part of the curve to another, where two parts pass through one cell, or a tip
//! reaches past the cell, and that cell is split in four, and so on down to a sixteenth of a cell.
//! Each neighbour that gains a corner on its side is looked at again, since the curve may cross
//! there too, which is how splitting follows two arms that run close together, or a tip into the
//! next cell. Only cells whose line strays cost more, and a budget for each trace keeps a curve
//! with more detail than the grid can follow from costing more than a few times what its grid
//! does. Coarse cells are looked at before finer ones, and of those, the furthest astray first, so
//! what the budget reaches is where the trace strayed most, wherever that is in the frame.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};

use crate::error::Error;
use crate::eval::{self, Env};
use crate::fast;
use crate::ir::{Expr, Viewport};
use crate::stroke::CELL_PX;

const NEWTON_STEPS: usize = 3;
/// How many times an edge whose ends differ in sign is halved, to see whether the field goes to
/// zero there: to 2⁻¹² of the edge.
const HALVINGS: usize = 12;
/// How far a cell's line may stray from the curve, in cells: a quarter of a device pixel, which a
/// stroke's edge shows as a quarter of a pixel's coverage at most.
const STRAY: f64 = 0.25 / CELL_PX;
/// How many times a cell whose line won't bend onto the curve can be split in four: to a
/// sixteenth of a cell, under 0.4 pixel, which is how close two parts of a curve can come and
/// still be told apart.
const DEPTH: u32 = 4;
/// A grid cell's side in the finest cells, which corners and edges are counted in.
const FINE: u32 = 1 << DEPTH;
/// How many times a line that strays is halved at the curve to bend it.
const BENDS: usize = 8;
/// Checking, bending and splitting lines work the field out at most about this many times for
/// each of the grid's corners, a gradient counting twice, so a curve with more detail than the
/// grid can follow, as one zoomed far out has, costs a few times what its grid does and not
/// hundreds. Past that, a line is left as the grid found it. The grid's own work isn't counted:
/// its corners, whether each edge that changes sign goes to zero, and putting each of its
/// crossings on the curve. The tips of `sin(30·x) = y` in `0 ± 2` need 5 at 375 pixels. Long
/// needles and close arms in a small frame need more than there is: `sin(80·x) = y` needs 13 at
/// 812 pixels, and `1/sin(x)² = y + 2` in `0 ± 51` needs 8.4 at 812 pixels and 48 at 400. Each
/// unit costs a figure that strays everywhere about a fifth of what its grid does, so 8 keeps
/// one zoomed far out under three times what it cost before refining.
const BUDGET: usize = 8;
/// About how many times the field is worked out to look at a part a split makes: its new edges'
/// zeros and crossings. The budget keeps this much for each such part waiting to be looked at,
/// or what such parts have cost so far if that's more, so the work done once it's spent is small.
const PART: usize = 24;

#[derive(Clone, Debug, PartialEq)]
pub struct Polyline {
    /// World coordinates. A closed polyline repeats its first point at the end.
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
    /// A parametric curve's parameter at each point, for what a style works out along it.
    /// Empty for a line that has none.
    pub params: Vec<f64>,
}

/// A stretch of grid line between two corners with none between them, counted in the finest
/// cells: `kind` 0 runs right from `(i, j)` for `len`, 1 runs up. A grid edge no split cell has
/// given another corner is one of these `FINE` long, and they sort as the grid's edges do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Edge {
    kind: u8,
    i: u32,
    j: u32,
    len: u32,
}

impl Edge {
    /// The grid's edge from its corner `(i, j)`.
    fn whole(kind: u8, i: u32, j: u32) -> Edge {
        Edge {
            kind,
            i: i * FINE,
            j: j * FINE,
            len: FINE,
        }
    }

    /// The edge between two corners on one line of the grid, either way round.
    fn between(a: (u32, u32), b: (u32, u32)) -> Edge {
        if a.1 == b.1 {
            Edge {
                kind: 0,
                i: a.0.min(b.0),
                j: a.1,
                len: a.0.abs_diff(b.0),
            }
        } else {
            Edge {
                kind: 1,
                i: a.0,
                j: a.1.min(b.1),
                len: a.1.abs_diff(b.1),
            }
        }
    }

    fn ends(&self) -> [(u32, u32); 2] {
        let to = if self.kind == 0 {
            (self.i + self.len, self.j)
        } else {
            (self.i, self.j + self.len)
        };
        [(self.i, self.j), to]
    }
}

/// Where the curve crosses an edge, with the field's gradient there in cells, where it has one.
#[derive(Clone, Copy, Debug)]
struct Vertex {
    at: [f64; 2],
    grad: Option<[f64; 2]>,
}

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
    /// How many times the field has been worked out, its gradient counting twice.
    evals: usize,
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
            evals: 0,
        }
    }

    fn at(&mut self, p: [f64; 2]) -> Result<f64, Error> {
        self.evals += 1;
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
        self.evals += 2;
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
    /// `|f| / |∇f|`, can't tell them apart: it goes to 0 at a pole too. With a zero, where it is, to
    /// within a sixteenth of the way from `a` to `b` or better.
    fn goes_to_zero(
        &mut self,
        mut a: [f64; 2],
        mut b: [f64; 2],
        mut fa: f64,
        mut fb: f64,
    ) -> Result<Option<[f64; 2]>, Error> {
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
                return Ok(Some([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]));
            }
        }
        Ok(None)
    }
}

/// Where the grid is. Its corners are counted in the finest cells.
struct Grid {
    origin: [f64; 2],
    cell: [f64; 2],
    margin: f64,
    /// Its cells across and up, the margin's included.
    cells: [u32; 2],
}

impl Grid {
    fn at(&self, (i, j): (u32, u32)) -> [f64; 2] {
        [
            self.origin[0] + (i as f64 / FINE as f64 - self.margin) * self.cell[0],
            self.origin[1] + (j as f64 / FINE as f64 - self.margin) * self.cell[1],
        ]
    }

    /// A step in the world, in cells.
    fn cells(&self, v: [f64; 2]) -> [f64; 2] {
        [v[0] / self.cell[0], v[1] / self.cell[1]]
    }

    /// A gradient per cell, from one per unit of the world.
    fn per_cell(&self, g: [f64; 2]) -> [f64; 2] {
        [g[0] * self.cell[0], g[1] * self.cell[1]]
    }
}

/// A grid cell split in four, and so on, as a list of its parts, the first being the whole cell.
struct Part {
    /// Its lower left corner and its side, in the finest cells.
    at: (u32, u32),
    size: u32,
    /// Where its four quarters start in the list, once it's split.
    quarters: Option<usize>,
    /// Its lines, from crossing to crossing, while it's whole.
    joins: Vec<(Edge, Edge)>,
}

impl Part {
    fn whole((i, j): (u32, u32)) -> Part {
        Part {
            at: (i * FINE, j * FINE),
            size: FINE,
            quarters: None,
            joins: Vec::new(),
        }
    }
}

/// A hash for the grid's small whole-number keys, quicker than the default one, which resists
/// attacks nothing here has to. The maps it's for are only looked in, never walked, so its order
/// can't reach the lines traced.
#[derive(Clone, Copy, Default)]
struct Quick;

impl BuildHasher for Quick {
    type Hasher = Mix;

    fn build_hasher(&self) -> Mix {
        Mix(0)
    }
}

struct Mix(u64);

impl Hasher for Mix {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_u64(u64::from(*b));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.write_u64(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.write_u64(u64::from(n));
    }

    fn write_usize(&mut self, n: usize) {
        self.write_u64(n as u64);
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

/// A part to look at. Coarser parts come first, and of parts as coarse, the one whose line strays
/// furthest, so where the budget runs out, what's been refined is where the trace strayed most
/// across the frame, and not the rows the grid happened to reach first.
#[derive(Clone, Copy, Debug)]
struct Task {
    size: u32,
    strays: f64,
    cell: (u32, u32),
    index: usize,
    /// Whether a split made it, or gave it another corner, so the budget has to keep enough to
    /// look at it.
    owed: bool,
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Task {}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Task {
    /// Greater is sooner.
    fn cmp(&self, other: &Self) -> Ordering {
        self.size
            .cmp(&other.size)
            .then(self.strays.total_cmp(&other.strays))
            .then((other.cell, other.index).cmp(&(self.cell, self.index)))
    }
}

/// The parts to look at, each once until it's looked at, and how many of them splits made.
#[derive(Default)]
struct Queue {
    tasks: BinaryHeap<Task>,
    queued: HashSet<((u32, u32), usize), Quick>,
    owed: usize,
}

impl Queue {
    fn push(&mut self, task: Task) {
        if self.queued.insert((task.cell, task.index)) {
            self.owed += usize::from(task.owed);
            self.tasks.push(task);
        }
    }

    fn pop(&mut self) -> Option<Task> {
        let task = self.tasks.pop()?;
        self.queued.remove(&(task.cell, task.index));
        self.owed -= usize::from(task.owed);
        Some(task)
    }
}

/// How many times over to take a Newton step, from the lengths of the steps so far. Where the last
/// three shrank by about the same ratio r, the zero is a multiple one, of about 1/(1 − r), as where
/// the field is squared or cubed, and each step goes only that part of the way to it.
fn multiple(lengths: &[f64]) -> f64 {
    let &[.., a, b, c] = lengths else {
        return 1.0;
    };
    let (r, s) = (b / a, c / b);
    let steady = (0.3..0.95).contains(&r) && (0.3..0.95).contains(&s) && (s / r - 1.0).abs() < 0.1;
    if steady {
        (1.0 / (1.0 - s)).round().max(1.0)
    } else {
        1.0
    }
}

struct Tracer<'a> {
    field: Field<'a>,
    grid: Grid,
    /// The field at the grid's corners, row by row.
    values: Vec<f64>,
    /// The field at the corners split cells add, and which of those are on each line of the grid,
    /// across and up.
    finer: HashMap<(u32, u32), f64, Quick>,
    across: BTreeMap<u32, BTreeSet<u32>>,
    up: BTreeMap<u32, BTreeSet<u32>>,
    /// The grid's edges that change sign without going through zero, across a pole or a jump.
    poles: BTreeSet<Edge>,
    /// For each edge that changes sign, where it goes to zero, near enough to tell a crossing
    /// Newton steps took onto another part of the curve, or nothing across a pole or a jump.
    zeros: HashMap<Edge, Option<[f64; 2]>, Quick>,
    vertices: HashMap<Edge, Vertex, Quick>,
    /// How far each line from one crossing to another was found to stray, in cells.
    strayed: HashMap<(Edge, Edge), f64, Quick>,
    /// The points a line is bent through, where it strayed.
    bent: HashMap<(Edge, Edge), Vec<[f64; 2]>, Quick>,
    /// The lines kept, bent or not, and those that wouldn't bend, so neither is bent again.
    kept: HashSet<(Edge, Edge), Quick>,
    failed: HashSet<(Edge, Edge), Quick>,
    /// The grid cells that are split, or whose sides a split neighbour gave another corner.
    split: BTreeMap<(u32, u32), Vec<Part>>,
    queue: Queue,
    /// How many times the field may be worked out to check, bend and split lines, and how many of
    /// the times it has been were the grid's own work: its corners, the edges that change sign,
    /// and the three Newton steps that take each of its crossings onto the curve.
    budget: usize,
    free: usize,
    /// How many parts splits made have been looked at, and what that cost.
    owed: [usize; 2],
}

impl<'a> Tracer<'a> {
    fn new(mut field: Field<'a>, grid: Grid) -> Result<Self, Error> {
        let [nx, ny] = grid.cells;
        let mut values = Vec::with_capacity(((nx + 1) * (ny + 1)) as usize);
        for j in 0..=ny {
            for i in 0..=nx {
                values.push(field.at(grid.at((i * FINE, j * FINE)))?);
            }
        }
        let value = |i: u32, j: u32| values[(j * (nx + 1) + i) as usize];
        // Only an edge that changes sign costs anything more.
        let (mut poles, mut zeros) = (BTreeSet::new(), HashMap::default());
        for j in 0..=ny {
            for i in 0..=nx {
                for (kind, to) in [(0, (i + 1, j)), (1, (i, j + 1))] {
                    if to.0 > nx || to.1 > ny {
                        continue;
                    }
                    let (fa, fb) = (value(i, j), value(to.0, to.1));
                    if (fa < 0.0) == (fb < 0.0) {
                        continue;
                    }
                    let (a, b) = (
                        grid.at((i * FINE, j * FINE)),
                        grid.at((to.0 * FINE, to.1 * FINE)),
                    );
                    let zero = field.goes_to_zero(a, b, fa, fb)?;
                    if zero.is_none() {
                        poles.insert(Edge::whole(kind, i, j));
                    }
                    zeros.insert(Edge::whole(kind, i, j), zero);
                }
            }
        }
        let (budget, free) = (BUDGET * values.len(), field.evals);
        Ok(Tracer {
            field,
            grid,
            values,
            finer: HashMap::default(),
            across: BTreeMap::new(),
            up: BTreeMap::new(),
            poles,
            zeros,
            vertices: HashMap::default(),
            strayed: HashMap::default(),
            bent: HashMap::default(),
            kept: HashSet::default(),
            failed: HashSet::default(),
            split: BTreeMap::new(),
            queue: Queue::default(),
            budget,
            free,
            owed: [0, 0],
        })
    }

    /// How many times the field has been worked out for more than the grid's own work.
    fn spent(&self) -> usize {
        self.field.evals - self.free
    }

    /// Whether there's budget left to check, bend and split lines, keeping what looking at the
    /// parts splits have made takes.
    fn refining(&self) -> bool {
        let each = PART.max(self.owed[1] / self.owed[0].max(1));
        self.spent() + each * self.queue.owed < self.budget
    }

    fn value(&self, (i, j): (u32, u32)) -> f64 {
        if i % FINE == 0 && j % FINE == 0 {
            self.values[((j / FINE) * (self.grid.cells[0] + 1) + i / FINE) as usize]
        } else {
            self.finer[&(i, j)]
        }
    }

    /// Whether the field goes to zero on an edge whose ends differ in sign, and not through a
    /// pole or a jump.
    fn is_zero(&mut self, e: Edge) -> Result<bool, Error> {
        Ok(self.zero(e)?.is_some())
    }

    /// Where an edge whose ends differ in sign goes to zero, to a sixteenth of the edge or
    /// better, or nothing across a pole or a jump.
    fn zero(&mut self, e: Edge) -> Result<Option<[f64; 2]>, Error> {
        if let Some(zero) = self.zeros.get(&e) {
            return Ok(*zero);
        }
        let [a, b] = e.ends();
        let (pa, pb) = (self.grid.at(a), self.grid.at(b));
        let (fa, fb) = (self.value(a), self.value(b));
        let zero = self.field.goes_to_zero(pa, pb, fa, fb)?;
        self.zeros.insert(e, zero);
        Ok(zero)
    }

    /// Where the curve crosses an edge whose ends differ in sign: where the field would be zero if
    /// it changed evenly along it, taken onto the curve by Newton steps. Where the steps don't
    /// settle, or settle further away than the edge is long or off its ends, they've left for
    /// another part of the curve, as they do where the field bends sharply between the corners,
    /// and the crossing is found along the edge instead, where it changes sign. That's done
    /// whatever the budget, since a crossing in the wrong place draws a line across the frame, and
    /// for the grid's own edges it's the grid's own work, which the budget doesn't count: twelve
    /// Newton steps and forty along the edge at most, and most crossings take three and none.
    fn vertex(&mut self, e: Edge) -> Result<Vertex, Error> {
        if let Some(v) = self.vertices.get(&e) {
            return Ok(*v);
        }
        let before = self.field.evals;
        let [a, b] = e.ends();
        let (pa, pb) = (self.grid.at(a), self.grid.at(b));
        let (fa, fb) = (self.value(a), self.value(b));
        let t = fa / (fa - fb);
        let start = [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t];
        let (at, grad, settled) = self.settle(start)?;
        let zero = self.zero(e)?;
        let cell = self.grid.cell;
        let apart =
            |p: [f64; 2], q: [f64; 2]| ((p[0] - q[0]) / cell[0]).hypot((p[1] - q[1]) / cell[1]);
        let length = f64::from(e.len) / f64::from(FINE);
        // How far along the edge a point is, as a fraction of it.
        let along_edge = [pb[0] - pa[0], pb[1] - pa[1]];
        let along = |p: [f64; 2]| {
            ((p[0] - pa[0]) * along_edge[0] + (p[1] - pa[1]) * along_edge[1])
                / (along_edge[0].powi(2) + along_edge[1].powi(2))
        };
        // The point of the curve nearest where the steps started is no further from the edge's
        // zero than twice as far as they started from it, so one further than that, with a
        // sixteenth of the edge for where the zero was found, is on another part of the curve.
        let near = zero.is_none_or(|z| apart(at, z) <= 2.0 * apart(start, z) + length / 16.0);
        let landed = settled
            && near
            && apart(at, start) <= length
            && (-1e-3..=1.0 + 1e-3).contains(&along(at));
        let v = if landed {
            Vertex { at, grad }
        } else {
            let guess = along(zero.unwrap_or(at)).clamp(0.0, 1.0);
            self.zero_between(pa, along_edge, [0.0, 1.0], [fa, fb], guess, fa < 0.0)?
                .unwrap_or(Vertex { at, grad })
        };
        self.vertices.insert(e, v);
        if e.len == FINE {
            self.free += self.field.evals - before;
        }
        Ok(v)
    }

    /// Newton steps along the gradient from `p`: the three a crossing has always had, and more, up
    /// to twelve, until one moves less than a millionth of a cell. Where
    /// the steps shrink by a steady ratio the zero is a multiple one, and each is taken as many
    /// times over as that says (`multiple`). With the point, the gradient in cells where the last
    /// step started, which is the curve's normal once the steps have closed in on it, and whether
    /// they did.
    #[allow(clippy::type_complexity)]
    fn settle(&mut self, mut p: [f64; 2]) -> Result<([f64; 2], Option<[f64; 2]>, bool), Error> {
        let (mut normal, mut settled) = (None, false);
        let (mut lengths, mut last) = (Vec::new(), f64::NAN);
        for step in 0..12 {
            let f = self.field.at(p)?;
            let [gx, gy] = self.field.slope(p)?;
            let g2 = gx * gx + gy * gy;
            // Where the gradient vanishes or is infinite, as at a cusp, a step goes nowhere useful.
            // It vanishes on the curve too where the zero is a multiple one, which the field falling
            // by a thousand since the last step shows.
            if !(1e-12..f64::INFINITY).contains(&g2) {
                settled = step >= NEWTON_STEPS && f.abs() <= last.abs() * 1e-3;
                break;
            }
            last = f;
            normal = Some(self.grid.per_cell([gx, gy]));
            let plain = [f * gx / g2, f * gy / g2];
            let length = self.grid.cells(plain);
            let length = length[0].hypot(length[1]);
            lengths.push(length);
            let times = if step >= NEWTON_STEPS {
                multiple(&lengths)
            } else {
                1.0
            };
            let next = [p[0] - times * plain[0], p[1] - times * plain[1]];
            if !(next[0].is_finite() && next[1].is_finite()) {
                break;
            }
            p = next;
            if step + 1 >= NEWTON_STEPS && times * length < 1e-6 {
                settled = true;
                break;
            }
        }
        Ok((p, normal, settled))
    }

    /// How far the curve between two vertices strays from the line between them, in cells, as
    /// near as can be told, which is under `STRAY` where it doesn't. An arc that turns evenly
    /// leaves its chord by `c/2·tan(α/2)` at most, c being the chord's length and α the angle the
    /// curve makes with it at either end, so the curve's direction at the ends says most of it:
    /// across a tip, or from one part of a curve to another, it turns right back. Where the curve
    /// runs along the line at both ends and leaves it between them, the field shows it: `STRAY`
    /// to either side of the line's middle, it has one sign. That holds where the field's gradient
    /// vanishes on the curve, as it does where the field is squared, and a distance worked out
    /// from the gradient doesn't.
    fn stray(&mut self, p: Vertex, q: Vertex) -> Result<f64, Error> {
        let d = self.grid.cells([q.at[0] - p.at[0], q.at[1] - p.at[1]]);
        let c = d[0].hypot(d[1]);
        if !(c > 0.0 && c.is_finite()) {
            return Ok(0.0);
        }
        let u = [d[0] / c, d[1] / c];
        let mut most = 0.0f64;
        for g in [p.grad, q.grad].into_iter().flatten() {
            let n = g[0].hypot(g[1]);
            if !(n > 0.0 && n.is_finite()) {
                continue;
            }
            // The curve's direction, with negative on its left.
            let t = [-g[1] / n, g[0] / n];
            let along = u[0] * t[0] + u[1] * t[1];
            let off = (u[0] * t[1] - u[1] * t[0]).abs();
            most = most.max(if along > 0.0 {
                c / 2.0 * off / (1.0 + along)
            } else {
                c.max(2.0 * STRAY)
            });
        }
        if most > STRAY {
            return Ok(most);
        }
        let middle = [(p.at[0] + q.at[0]) / 2.0, (p.at[1] + q.at[1]) / 2.0];
        let side = [
            -u[1] * STRAY * self.grid.cell[0],
            u[0] * STRAY * self.grid.cell[1],
        ];
        let left = self.field.at([middle[0] + side[0], middle[1] + side[1]])?;
        let right = self.field.at([middle[0] - side[0], middle[1] - side[1]])?;
        Ok(if (left < 0.0) != (right < 0.0) {
            most
        } else {
            2.0 * STRAY
        })
    }

    /// How far the line from `start` to `end` strays, worked out once.
    fn strays(&mut self, start: Edge, end: Edge) -> Result<f64, Error> {
        if let Some(e) = self.strayed.get(&(start, end)) {
            return Ok(*e);
        }
        let (p, q) = (self.vertex(start)?, self.vertex(end)?);
        let e = self.stray(p, q)?;
        self.strayed.insert((start, end), e);
        Ok(e)
    }

    /// A part's square in the world, with two finest cells more on every side. A line bent in it
    /// stays within that, so no two parts' lines follow the same piece of the curve, and a tip
    /// the finest parts don't see the end of is still bent round its end.
    fn bounds(&self, (i, j): (u32, u32), s: u32) -> [[f64; 2]; 2] {
        let (lo, hi) = (self.grid.at((i, j)), self.grid.at((i + s, j + s)));
        let m = [
            2.0 * self.grid.cell[0] / f64::from(FINE),
            2.0 * self.grid.cell[1] / f64::from(FINE),
        ];
        [[lo[0] - m[0], lo[1] - m[1]], [hi[0] + m[0], hi[1] + m[1]]]
    }

    /// Whether to keep the line a part joins from `start` to `end`. Where it strays from the curve,
    /// it's bent onto it within `bounds`, and kept if that settles, if the part can't be split, or
    /// as it is once the budget is spent.
    fn keep(
        &mut self,
        start: Edge,
        end: Edge,
        can_split: bool,
        bounds: [[f64; 2]; 2],
    ) -> Result<bool, Error> {
        // A part looked at again keeps what it kept, and a line that wouldn't bend still won't.
        if self.kept.contains(&(start, end)) {
            return Ok(true);
        }
        if can_split && self.failed.contains(&(start, end)) {
            return Ok(false);
        }
        let keep = if !self.refining() || self.strays(start, end)? <= STRAY {
            true
        } else {
            let (p, q) = (self.vertex(start)?, self.vertex(end)?);
            let mut points = Vec::new();
            let settled = self.bend(p, q, BENDS, can_split, bounds, &mut points)?;
            if settled || !can_split {
                self.bent.insert((start, end), points);
            } else {
                self.failed.insert((start, end));
            }
            settled || !can_split
        };
        if keep {
            self.kept.insert((start, end));
        }
        Ok(keep)
    }

    /// Bends the line from `p` to `q` onto the curve, halving it at the curve while it strays,
    /// adds the points that takes, and says whether it settled: whether every piece is near the
    /// curve. A line from one part of the curve to another doesn't, since the point halfway along
    /// it isn't on the curve, or the pieces still turn back. With `give_up`, it stops at the first
    /// piece that won't.
    fn bend(
        &mut self,
        p: Vertex,
        q: Vertex,
        depth: usize,
        give_up: bool,
        bounds: [[f64; 2]; 2],
        out: &mut Vec<[f64; 2]>,
    ) -> Result<bool, Error> {
        if self.stray(p, q)? <= STRAY {
            return Ok(true);
        }
        if depth == 0 || !self.refining() {
            return Ok(false);
        }
        let Some(r) = self.halfway(p, q, bounds)? else {
            return Ok(false);
        };
        let before = self.bend(p, r, depth - 1, give_up, bounds, out)?;
        if give_up && !before {
            return Ok(false);
        }
        out.push(r.at);
        Ok(self.bend(r, q, depth - 1, give_up, bounds, out)? && before)
    }

    /// Where the curve crosses the line square to the one from `p` to `q` through its middle,
    /// which is between them, so the line through it doesn't double back. The curve from `p` to
    /// `q` has negative on its left, so where the middle is negative it bulges to the right of the
    /// line, and to the left where it isn't. Steps out that way, doubling, find where the field
    /// changes sign, as far as `bounds` go, and Newton steps kept between the last two close in.
    fn halfway(
        &mut self,
        p: Vertex,
        q: Vertex,
        bounds: [[f64; 2]; 2],
    ) -> Result<Option<Vertex>, Error> {
        let d = self.grid.cells([q.at[0] - p.at[0], q.at[1] - p.at[1]]);
        let c = d[0].hypot(d[1]);
        let middle = [(p.at[0] + q.at[0]) / 2.0, (p.at[1] + q.at[1]) / 2.0];
        let fm = self.field.at(middle)?;
        // A step of one along it is a cell's length.
        let side = if fm < 0.0 { 1.0 } else { -1.0 };
        let out = [
            side * d[1] / c * self.grid.cell[0],
            -side * d[0] / c * self.grid.cell[1],
        ];
        // How far it can go before it leaves the bounds.
        let mut reach = f64::INFINITY;
        for k in 0..2 {
            let edge = if out[k] > 0.0 {
                bounds[1][k]
            } else {
                bounds[0][k]
            };
            if out[k] != 0.0 {
                reach = reach.min((edge - middle[k]) / out[k]);
            }
        }
        if reach.is_nan() || reach <= 0.0 {
            return Ok(None);
        }
        let at = |s: f64| [middle[0] + s * out[0], middle[1] + s * out[1]];
        let (mut a, mut b) = (0.0, (c / 4.0).min(reach));
        let (mut fa, mut fb) = (fm, self.field.at(at(b))?);
        while (fb < 0.0) == (fm < 0.0) {
            if b >= reach {
                return Ok(None);
            }
            (a, fa) = (b, fb);
            b = (2.0 * b).min(reach);
            fb = self.field.at(at(b))?;
        }
        self.zero_between(middle, out, [a, b], [fa, fb], (a + b) / 2.0, fm < 0.0)
    }

    /// Where the field is zero on the line from `from` along `step`, between `a` and `b` steps
    /// along, whose signs differ and where it's `ends`: Newton steps from `guess`, taken as many times over as
    /// a multiple zero needs (`multiple`), kept between the last points found on either side and
    /// halving where they'd leave, to a billionth of a step. With the gradient there in cells.
    /// `None` where the sign changes across a pole or a jump: the field there didn't fall below
    /// half the most it was at the ends, as `goes_to_zero` tells them apart.
    fn zero_between(
        &mut self,
        from: [f64; 2],
        step: [f64; 2],
        [mut a, mut b]: [f64; 2],
        ends: [f64; 2],
        guess: f64,
        negative_at_a: bool,
    ) -> Result<Option<Vertex>, Error> {
        let at = |s: f64| [from[0] + s * step[0], from[1] + s * step[1]];
        let peak = ends[0].abs().max(ends[1].abs());
        let mut s = if a < guess && guess < b {
            guess
        } else {
            (a + b) / 2.0
        };
        let (mut g, mut last) = ([f64::NAN; 2], f64::NAN);
        let mut lengths = Vec::new();
        for _ in 0..40 {
            let f = self.field.at(at(s))?;
            last = f;
            if f == 0.0 {
                break;
            }
            if (f < 0.0) == negative_at_a {
                a = s;
            } else {
                b = s;
            }
            g = self.field.slope(at(s))?;
            let plain = f / (g[0] * step[0] + g[1] * step[1]);
            lengths.push(plain.abs());
            let newton = s - multiple(&lengths) * plain;
            let next = if a < newton && newton < b {
                newton
            } else {
                (a + b) / 2.0
            };
            let settled = (next - s).abs() < 1e-9;
            s = next;
            if settled {
                break;
            }
        }
        if last.is_nan() || last.abs() >= peak / 2.0 {
            return Ok(None);
        }
        Ok(Some(Vertex {
            at: at(s),
            grad: Some(self.grid.per_cell(g)),
        }))
    }

    /// The lines through a whole part of a split cell, from crossing to crossing on its sides,
    /// which hold its own corners and any a smaller neighbour has added. `None` where it should be
    /// split: where its line won't bend onto the curve, or two parts of the curve pass through it.
    fn joins(&mut self, (i, j): (u32, u32), s: u32) -> Result<Option<Vec<(Edge, Edge)>>, Error> {
        let mut corners = vec![(i, j)];
        if let Some(on) = self.across.get(&j) {
            corners.extend(on.range(i + 1..i + s).map(|&x| (x, j)));
        }
        corners.push((i + s, j));
        if let Some(on) = self.up.get(&(i + s)) {
            corners.extend(on.range(j + 1..j + s).map(|&y| (i + s, y)));
        }
        corners.push((i + s, j + s));
        if let Some(on) = self.across.get(&(j + s)) {
            corners.extend(on.range(i + 1..i + s).rev().map(|&x| (x, j + s)));
        }
        corners.push((i, j + s));
        if let Some(on) = self.up.get(&i) {
            corners.extend(on.range(j + 1..j + s).rev().map(|&y| (i, y)));
        }
        let f: Vec<f64> = corners.iter().map(|c| self.value(*c)).collect();
        let n = corners.len();
        let mut crossings = Vec::new();
        for k in 0..n {
            let (a, b) = (k, (k + 1) % n);
            if (f[a] < 0.0) != (f[b] < 0.0) {
                let e = Edge::between(corners[a], corners[b]);
                // A part a pole or a jump runs through joins nothing, as a whole cell doesn't.
                if !self.is_zero(e)? {
                    return Ok(Some(Vec::new()));
                }
                crossings.push((e, f[a] < 0.0));
            }
        }
        let can_split = s > 1 && self.refining();
        if crossings.len() > 2 && can_split {
            return Ok(None);
        }
        let middle = [(i, j), (i + s, j), (i + s, j + s), (i, j + s)]
            .iter()
            .map(|c| self.value(*c))
            .sum::<f64>();
        let joins = pair(&crossings, middle < 0.0);
        let bounds = self.bounds((i, j), s);
        for &(start, end) in &joins {
            if !self.keep(start, end, can_split, bounds)? {
                return Ok(None);
            }
        }
        Ok(Some(joins))
    }

    /// Splits a part in four, adds the corners that takes, and looks again at the parts whose
    /// sides those corners are on, which come in the order of the part, `strays`.
    fn split(&mut self, cell: (u32, u32), index: usize, strays: f64) -> Result<(), Error> {
        let Some(parts) = self.split.get_mut(&cell) else {
            return Ok(());
        };
        let ((i, j), s) = (parts[index].at, parts[index].size);
        let h = s / 2;
        let first = parts.len();
        parts[index].quarters = Some(first);
        parts[index].joins.clear();
        for (di, dj) in [(0, 0), (h, 0), (0, h), (h, h)] {
            parts.push(Part {
                at: (i + di, j + dj),
                size: h,
                quarters: None,
                joins: Vec::new(),
            });
        }
        for k in 0..4 {
            self.queue.push(Task {
                size: h,
                strays,
                cell,
                index: first + k,
                owed: true,
            });
        }
        let [w, t] = [self.grid.cells[0] * FINE, self.grid.cells[1] * FINE];
        // The middle of each side, with the finest cell across it, in the neighbour that side
        // gains a corner if it's no smaller.
        let sides = [
            ((i + h, j), j.checked_sub(1).map(|y| (i + h, y))),
            ((i + s, j + h), (i + s < w).then_some((i + s, j + h))),
            ((i + h, j + s), (j + s < t).then_some((i + h, j + s))),
            ((i, j + h), i.checked_sub(1).map(|x| (x, j + h))),
        ];
        for (corner, beyond) in sides {
            if self.add(corner)?
                && let Some(beyond) = beyond
            {
                self.look_again(beyond, strays);
            }
        }
        self.add((i + h, j + h))?;
        Ok(())
    }

    /// Works the field out at a corner a split adds, unless it's there already.
    fn add(&mut self, c: (u32, u32)) -> Result<bool, Error> {
        if self.finer.contains_key(&c) {
            return Ok(false);
        }
        let value = self.field.at(self.grid.at(c))?;
        self.finer.insert(c, value);
        self.across.entry(c.1).or_default().insert(c.0);
        self.up.entry(c.0).or_default().insert(c.1);
        Ok(true)
    }

    /// Queues the whole part that holds the finest cell at `c`, taking its grid cell in with the
    /// split ones if it isn't yet.
    fn look_again(&mut self, c: (u32, u32), strays: f64) {
        let cell = (c.0 / FINE, c.1 / FINE);
        let parts = self
            .split
            .entry(cell)
            .or_insert_with(|| vec![Part::whole(cell)]);
        let mut index = 0;
        while let Some(first) = parts[index].quarters {
            let (at, h) = (parts[index].at, parts[index].size / 2);
            index = first + usize::from(c.0 >= at.0 + h) + 2 * usize::from(c.1 >= at.1 + h);
        }
        let size = parts[index].size;
        self.queue.push(Task {
            size,
            strays,
            cell,
            index,
            owed: true,
        });
    }

    fn lines(&mut self) -> Result<Vec<Polyline>, Error> {
        let [nx, ny] = self.grid.cells;
        // Each segment runs from where the perimeter (walked counterclockwise) goes
        // negative to positive, to where it goes positive to negative. That keeps the
        // negative side on the left.
        let mut joins = Vec::new();
        for j in 0..ny {
            for i in 0..nx {
                let corner = |di: u32, dj: u32| self.value(((i + di) * FINE, (j + dj) * FINE));
                let f = [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)];
                let edges = [
                    Edge::whole(0, i, j),
                    Edge::whole(1, i + 1, j),
                    Edge::whole(0, i, j + 1),
                    Edge::whole(1, i, j),
                ];
                let mut crossings = Vec::new();
                for k in 0..4 {
                    if (f[k] < 0.0) != (f[(k + 1) % 4] < 0.0) {
                        crossings.push((edges[k], f[k] < 0.0));
                    }
                }
                // A cell a pole or a jump runs through joins nothing. A crossing beside it is
                // closer to it than the grid can resolve, so which side the curve goes on can't
                // be told, and a line between them would run along it. The curve ends at the
                // cell's edge instead.
                if crossings.is_empty() || crossings.iter().any(|c| self.poles.contains(&c.0)) {
                    continue;
                }
                // Where two parts of the curve pass through a cell, it's split first.
                let line = pair(&crossings, false);
                let strays = match line[..] {
                    [(start, end)] if self.refining() => self.strays(start, end)?,
                    [_] => 0.0,
                    _ => f64::INFINITY,
                };
                if let [(start, end)] = line[..]
                    && strays <= STRAY
                {
                    joins.push(((i, j), start, end));
                    continue;
                }
                // Its line strays, or two parts of the curve pass through it: it's bent or split,
                // the furthest astray first.
                self.split.insert((i, j), vec![Part::whole((i, j))]);
                self.queue.push(Task {
                    size: FINE,
                    strays,
                    cell: (i, j),
                    index: 0,
                    owed: false,
                });
            }
        }

        while let Some(task) = self.queue.pop() {
            let before = self.field.evals;
            let part = &self.split[&task.cell][task.index];
            if part.quarters.is_some() {
                continue;
            }
            let (at, size) = (part.at, part.size);
            match self.joins(at, size)? {
                Some(lines) => {
                    if let Some(parts) = self.split.get_mut(&task.cell) {
                        parts[task.index].joins = lines;
                    }
                }
                None => self.split(task.cell, task.index, task.strays)?,
            }
            if task.owed {
                self.owed = [self.owed[0] + 1, self.owed[1] + self.field.evals - before];
            }
        }

        let mut next: BTreeMap<Edge, Edge> = BTreeMap::new();
        for (cell, start, end) in joins {
            if !self.split.contains_key(&cell) {
                next.insert(start, end);
            }
        }
        for parts in self.split.values() {
            for part in parts.iter().filter(|p| p.quarters.is_none()) {
                for &(start, end) in &part.joins {
                    next.insert(start, end);
                }
            }
        }

        // The finest cell's side, in the world, which a loop narrower than is too thin to start
        // from its area.
        let thin = self.grid.cell[0].min(self.grid.cell[1]) / f64::from(FINE);
        let targets: BTreeSet<Edge> = next.values().copied().collect();
        let mut lines = Vec::new();
        let mut visited: BTreeSet<Edge> = BTreeSet::new();
        // Open curves first, from the edges nothing leads into.
        let starts: Vec<Edge> = next
            .keys()
            .filter(|id| !targets.contains(id))
            .copied()
            .chain(next.keys().copied())
            .collect();
        for start in starts {
            if visited.contains(&start) {
                continue;
            }
            let mut points = vec![self.vertex(start)?.at];
            visited.insert(start);
            let mut at = start;
            let mut closed = false;
            while let Some(&to) = next.get(&at) {
                if let Some(bent) = self.bent.get(&(at, to)) {
                    points.extend(bent);
                }
                if to == start {
                    closed = true;
                    break;
                }
                points.push(self.vertex(to)?.at);
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
                points = canonical_start(points, &mut self.field, thin)?;
            }
            lines.push(Polyline {
                points,
                closed,
                params: Vec::new(),
            });
        }
        Ok(lines)
    }
}

/// Joins a cell's crossings, listed counterclockwise round it, each with whether the field goes
/// from negative to not there. Each line runs from where it leaves the negative side to a
/// crossing beside it where it comes back: the next one round if the cell's middle is negative,
/// and the one before if not, which is what settles a saddle.
fn pair(crossings: &[(Edge, bool)], middle_negative: bool) -> Vec<(Edge, Edge)> {
    let n = crossings.len();
    let Some(first) = crossings.iter().position(|c| c.1) else {
        return Vec::new();
    };
    (0..n / 2)
        .map(|k| {
            let a = (first + 2 * k) % n;
            let b = if middle_negative {
                (a + 1) % n
            } else {
                (a + n - 1) % n
            };
            (crossings[a].0, crossings[b].0)
        })
        .collect()
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
    tracer(vars, expr, grad, env, view, cols, rows, margin)?.lines()
}

/// The tracer for `trace`, with its grid's corners worked out.
#[allow(clippy::too_many_arguments)]
fn tracer<'a>(
    vars: &'a [String; 2],
    expr: &'a Expr,
    grad: &'a [Expr; 2],
    env: &Env,
    view: &Viewport,
    cols: u32,
    rows: u32,
    margin: u32,
) -> Result<Tracer<'a>, Error> {
    let margin = margin.max(1);
    let grid = Grid {
        origin: [view.x[0], view.y[0]],
        cell: [
            (view.x[1] - view.x[0]) / cols as f64,
            (view.y[1] - view.y[0]) / rows as f64,
        ],
        margin: margin as f64,
        cells: [cols + 2 * margin, rows + 2 * margin],
    };
    Tracer::new(Field::new(vars, expr, grad, env), grid)
}

/// Starts a loop where it crosses the horizontal through its centroid on the +x
/// side, and closes it. Grid vertices move as the curve moves, so starting at one
/// would make dashes jitter. For a circle this start is exactly angle 0.
///
/// The centroid is the area's the loop encloses, which moves only as the curve does. The mean of
/// its points moves wherever points are added, and bending a tip adds them there, one more or
/// fewer from one frame to the next as the curve moves, and the tips of a long loop are where
/// that moves the mean most. A loop too thin to enclose much takes the centroid of its line.
fn canonical_start(
    points: Vec<[f64; 2]>,
    field: &mut Field,
    thin: f64,
) -> Result<Vec<[f64; 2]>, Error> {
    let n = points.len();
    let (mut area, mut area_moment, mut length, mut line_moment) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let cross = a[0] * b[1] - b[0] * a[1];
        area += cross / 2.0;
        area_moment += (a[1] + b[1]) * cross / 6.0;
        let piece = (b[0] - a[0]).hypot(b[1] - a[1]);
        line_moment += piece * (a[1] + b[1]) / 2.0;
        length += piece;
    }
    // Thin, here, is narrower on average than the least two parts of a curve can be apart.
    let cy = if area.abs() > length * thin {
        area_moment / area
    } else if length > 0.0 {
        line_moment / length
    } else {
        points.iter().map(|p| p[1]).sum::<f64>() / n as f64
    };
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
    // Newton steps can leave for another part of the curve where it bends sharply, and further
    // than the line they start on is long they have, so the point on the line is kept then.
    let projected = field.project(p)?;
    let (a, b) = (points[i], points[(i + 1) % n]);
    let start =
        if (projected[0] - p[0]).hypot(projected[1] - p[1]) <= (b[0] - a[0]).hypot(b[1] - a[1]) {
            projected
        } else {
            p
        };
    let mut out = Vec::with_capacity(n + 2);
    out.push(start);
    for k in 1..=n {
        out.push(points[(i + k) % n]);
    }
    let near = |a: &[f64; 2], b: &[f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-12;
    out.dedup_by(|a, b| near(a, b));
    // It ends where it starts, exactly, whatever point next to the start was too near it to keep.
    while out.len() > 1 && out.last().is_some_and(|p| near(p, &start)) {
        out.pop();
    }
    out.push(start);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::build;
    use crate::ir::Form;
    use crate::stroke::CELL_PX;

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
        f.goes_to_zero(a, b, fa, fb).unwrap().is_some()
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

    /// A trace at a size in device pixels, made as `stroke::lines` makes one, of a frame moved by
    /// `shift` of a cell, so the grid falls elsewhere on the curve.
    struct Drawn {
        lines: Vec<Polyline>,
        view: Viewport,
        size: [f64; 2],
    }

    impl Drawn {
        fn px(&self, p: [f64; 2]) -> [f64; 2] {
            let (x, y) = (self.view.x, self.view.y);
            [
                (p[0] - x[0]) / (x[1] - x[0]) * self.size[0],
                (y[1] - p[1]) / (y[1] - y[0]) * self.size[1],
            ]
        }

        /// Device pixels to a unit of the world, the same across and up in these frames.
        fn scale(&self) -> f64 {
            self.size[0] / (self.view.x[1] - self.view.x[0])
        }

        fn in_frame(&self, q: [f64; 2]) -> bool {
            (0.0..=self.size[0]).contains(&q[0]) && (0.0..=self.size[1]).contains(&q[1])
        }

        fn segments(&self) -> impl Iterator<Item = [[f64; 2]; 2]> + '_ {
            self.lines
                .iter()
                .flat_map(|l| l.points.windows(2))
                .map(|w| [self.px(w[0]), self.px(w[1])])
        }
    }

    fn drawn(curve: &str, view: [[f64; 2]; 2], size: [u32; 2], shift: [f64; 2]) -> Drawn {
        worked(curve, view, size, shift).0
    }

    /// How much work a trace took: how many times it worked the field out in all, and past the
    /// grid's own work, and how many parts it split and lines it bent.
    #[derive(Clone, Copy, Debug)]
    struct Work {
        evals: usize,
        spent: usize,
        splits: usize,
        bends: usize,
    }

    fn worked(curve: &str, view: [[f64; 2]; 2], size: [u32; 2], shift: [f64; 2]) -> (Drawn, Work) {
        let figure = build(&format!("curve c = implicit (x, y): {curve}"), &[]).unwrap();
        let env = eval::frame_env(&figure, &crate::frame::Inputs::at(0.0)).unwrap();
        let Form::Field {
            vars, field, grad, ..
        } = &figure.scene.marks[0].form
        else {
            panic!("not a field");
        };
        let cols = (f64::from(size[0]) / CELL_PX).ceil() as u32;
        let rows = (f64::from(size[1]) / CELL_PX).ceil() as u32;
        let cell = [
            (view[0][1] - view[0][0]) / f64::from(cols),
            (view[1][1] - view[1][0]) / f64::from(rows),
        ];
        let mut v = figure.scene.viewport.clone();
        v.x = view[0].map(|x| x + shift[0] * cell[0]);
        v.y = view[1].map(|y| y + shift[1] * cell[1]);
        let mut tracer = tracer(vars, field, grad, &env, &v, cols, rows, 1).unwrap();
        let lines = tracer.lines().unwrap();
        let work = Work {
            evals: tracer.field.evals,
            spent: tracer.spent(),
            splits: tracer
                .split
                .values()
                .flatten()
                .filter(|p| p.quarters.is_some())
                .count(),
            bends: tracer.bent.values().filter(|b| !b.is_empty()).count(),
        };
        let drawn = Drawn {
            lines,
            view: v,
            size: [f64::from(size[0]), f64::from(size[1])],
        };
        (drawn, work)
    }

    /// Points along `curve` for its parameter in `range`, none more than a tenth of a pixel from
    /// the next, worked out without the tracer. Where the curve has no point, NaN, there's none,
    /// and the parts well outside the frame are left out.
    fn dense(d: &Drawn, curve: impl Fn(f64) -> [f64; 2], range: [f64; 2]) -> Vec<[f64; 2]> {
        type Curve<'a> = &'a dyn Fn(f64) -> [f64; 2];
        fn fill(
            d: &Drawn,
            curve: Curve,
            a: (f64, [f64; 2]),
            b: (f64, [f64; 2]),
            out: &mut Vec<[f64; 2]>,
            depth: u32,
        ) {
            let (p, q) = (d.px(a.1), d.px(b.1));
            let (has_p, has_q) = (
                p.iter().all(|v| v.is_finite()),
                q.iter().all(|v| v.is_finite()),
            );
            let outside = |k: usize| {
                let (lo, hi) = (-20.0, d.size[k] + 20.0);
                (p[k] < lo && q[k] < lo) || (p[k] > hi && q[k] > hi)
            };
            if !has_p && !has_q || has_p && has_q && (outside(0) || outside(1)) {
                return;
            }
            if depth == 0 || has_p && has_q && (p[0] - q[0]).hypot(p[1] - q[1]) <= 0.1 {
                if has_q {
                    out.push(b.1);
                }
                return;
            }
            let t = (a.0 + b.0) / 2.0;
            let m = (t, curve(t));
            fill(d, curve, a, m, out, depth - 1);
            fill(d, curve, m, b, out, depth - 1);
        }
        let at = |k: usize| range[0] + (range[1] - range[0]) * k as f64 / 1000.0;
        let mut out: Vec<[f64; 2]> = vec![curve(range[0])];
        out.retain(|p| p.iter().all(|v| v.is_finite()));
        for k in 0..1000 {
            let (a, b) = (at(k), at(k + 1));
            fill(d, &curve, (a, curve(a)), (b, curve(b)), &mut out, 40);
        }
        out
    }

    /// How far apart a trace and the curve, given by `samples`, are in device pixels, each way:
    /// the farthest a traced vertex or the middle of a traced segment is from the curve, and the
    /// farthest a point of the curve in the frame is from the trace, each with where it is in
    /// pixels. Anything 2 pixels off or more is infinitely far.
    fn apart(d: &Drawn, samples: &[[f64; 2]]) -> [(f64, [f64; 2]); 2] {
        const BIN: f64 = 2.0;
        type Bins<T> = HashMap<(i64, i64), Vec<T>>;
        let bin = |q: [f64; 2]| ((q[0] / BIN).floor() as i64, (q[1] / BIN).floor() as i64);
        let around = |q: [f64; 2]| {
            let (i, j) = bin(q);
            (i - 1..=i + 1).flat_map(move |a| (j - 1..=j + 1).map(move |b| (a, b)))
        };
        let mut curve: Bins<[f64; 2]> = Bins::new();
        for p in samples {
            let q = d.px(*p);
            curve.entry(bin(q)).or_default().push(q);
        }
        let mut trace: Bins<[[f64; 2]; 2]> = Bins::new();
        for s in d.segments() {
            let lo = bin([s[0][0].min(s[1][0]), s[0][1].min(s[1][1])]);
            let hi = bin([s[0][0].max(s[1][0]), s[0][1].max(s[1][1])]);
            for i in lo.0..=hi.0.min(lo.0 + 1000) {
                for j in lo.1..=hi.1.min(lo.1 + 1000) {
                    trace.entry((i, j)).or_default().push(s);
                }
            }
        }
        let to_segment = |q: [f64; 2], s: &[[f64; 2]; 2]| {
            let (a, b) = (s[0], s[1]);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let l2 = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if l2 > 0.0 {
                (((q[0] - a[0]) * ab[0] + (q[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (a[0] + ab[0] * t - q[0]).hypot(a[1] + ab[1] * t - q[1])
        };
        let mut off = (0.0, [0.0; 2]);
        for s in d.segments() {
            for q in [s[0], [(s[0][0] + s[1][0]) / 2.0, (s[0][1] + s[1][1]) / 2.0]] {
                let near = around(q)
                    .filter_map(|k| curve.get(&k))
                    .flatten()
                    .map(|c| (c[0] - q[0]).hypot(c[1] - q[1]))
                    .fold(f64::INFINITY, f64::min);
                let near = if near < BIN { near } else { f64::INFINITY };
                if near > off.0 {
                    off = (near, q);
                }
            }
        }
        let mut missed = (0.0, [0.0; 2]);
        for p in samples {
            let q = d.px(*p);
            if !d.in_frame(q) {
                continue;
            }
            let near = around(q)
                .filter_map(|k| trace.get(&k))
                .flatten()
                .map(|s| to_segment(q, s))
                .fold(f64::INFINITY, f64::min);
            let near = if near < BIN { near } else { f64::INFINITY };
            if near > missed.0 {
                missed = (near, q);
            }
        }
        [off, missed]
    }

    /// Where the grid falls under a frame, as a shift of it in cells.
    const SHIFTS: [[f64; 2]; 2] = [[0.0, 0.0], [0.37, 0.81]];
    /// How far, in device pixels, a trace may be from its curve either way in these tests: what
    /// the tracer aims for, a quarter of a pixel, and a little for its estimates.
    const NEAR_PX: f64 = 0.3;

    fn assert_near(d: &Drawn, samples: &[[f64; 2]], what: &str) {
        let [off, missed] = apart(d, samples);
        eprintln!(
            "{what}: off {:.3} at {:?}, missed {:.3} at {:?}",
            off.0, off.1, missed.0, missed.1
        );
        assert!(
            off.0 <= NEAR_PX,
            "{what}: the trace is {:.3} px off the curve at {:?} px",
            off.0,
            off.1
        );
        assert!(
            missed.0 <= NEAR_PX,
            "{what}: the curve is {:.3} px off the trace at {:?} px",
            missed.0,
            missed.1
        );
    }

    /// A peak of `sin(30·x)` turns in under half a pixel at 1633 pixels, and one of `sin(80·x)` in
    /// a sixteenth, where the grid's cells are 6. Each is traced to its tip, and the trace and the
    /// curve are within a fraction of a pixel of each other both ways, at a phone's size and a
    /// wide window's, wherever the grid falls.
    #[test]
    fn sharp_tips_are_traced_to_the_tip() {
        use std::f64::consts::{PI, TAU};
        for (k, sizes) in [(30.0, &[375, 812, 1633][..]), (80.0, &[1100, 1633][..])] {
            for &size in sizes {
                for shift in SHIFTS {
                    let what = format!("sin({k}·x), {size} px, shifted {shift:?}");
                    let d = drawn(
                        &format!("sin({k}·x) = y"),
                        [[-2.0, 2.0], [-2.0, 2.0]],
                        [size, size],
                        shift,
                    );
                    let samples = dense(&d, |x| [x, (k * x).sin()], [-2.2, 2.2]);
                    assert_near(&d, &samples, &what);
                    // Each peak's top and each trough's bottom, as the trace's highest and lowest
                    // point within a pixel of it.
                    let x = d.view.x;
                    for n in -20..=20 {
                        let n = f64::from(n);
                        for (at, sign) in [
                            ((PI / 2.0 + TAU * n) / k, 1.0),
                            ((-PI / 2.0 + TAU * n) / k, -1.0),
                        ] {
                            if !(x[0] < at && at < x[1]) {
                                continue;
                            }
                            let reach = d
                                .lines
                                .iter()
                                .flat_map(|l| &l.points)
                                .filter(|p| ((p[0] - at) * d.scale()).abs() < 1.0)
                                .map(|p| p[1] * sign)
                                .fold(f64::NEG_INFINITY, f64::max);
                            let short = (1.0 - reach) * d.scale();
                            assert!(
                                short <= NEAR_PX,
                                "{what}: the tip at x = {at} is {short} px short"
                            );
                        }
                    }
                }
            }
        }
    }

    /// `1/sin(x)² = y + 2` rises to a pole at each multiple of π from both sides, as two arms that
    /// close in on each other and never meet. In this frame they come within a pixel or a few of
    /// each other at its top. Each arm is traced up to the top, and nothing across the spike
    /// between them.
    #[test]
    fn narrow_spikes_keep_both_arms() {
        use std::f64::consts::PI;
        for size in [1100, 1633] {
            for shift in SHIFTS {
                let what = format!("{size} px, shifted {shift:?}");
                let d = drawn(
                    "1/sin(x)^2 = y + 2",
                    [[-51.0, 51.0], [-51.0, 51.0]],
                    [size, size],
                    shift,
                );
                let mut samples = Vec::new();
                for k in -17..=17 {
                    for side in [-1.0, 1.0] {
                        let pole = f64::from(k) * PI;
                        let arm = |y: f64| [pole + side * (1.0 / (y + 2.0).sqrt()).asin(), y];
                        samples.extend(dense(&d, arm, [-1.0, 56.0]));
                    }
                }
                assert_near(&d, &samples, &what);
                for line in &d.lines {
                    for w in line.points.windows(2) {
                        assert_eq!(
                            (w[0][0] / PI).floor(),
                            (w[1][0] / PI).floor(),
                            "{what}: a line across the spike at {:?} px",
                            d.px(w[0])
                        );
                    }
                }
            }
        }
    }

    /// Two unit circles 0.003 apart, which is 1.1 pixels at 1633 pixels across and 0.55 at 812,
    /// are two loops, each on its own circle, and close up, where they're 2.4 and 1.2 pixels apart,
    /// two arcs that don't meet.
    #[test]
    fn close_branches_stay_apart() {
        use std::f64::consts::TAU;
        let curve = "(x² + y² - 1)·((x - 2.003)² + y² - 1) = 0";
        let middle = 1.0015;
        type Case = ([[f64; 2]; 2], [[u32; 2]; 2], Option<usize>);
        let cases: [Case; 2] = [
            (
                [[middle - 2.2, middle + 2.2], [-1.2375, 1.2375]],
                [[1633, 919], [812, 457]],
                Some(2),
            ),
            (
                [[middle - 0.5, middle + 0.5], [-0.28125, 0.28125]],
                [[800, 450], [400, 225]],
                None,
            ),
        ];
        for (view, sizes, loops) in cases {
            for size in sizes {
                for shift in SHIFTS {
                    let what = format!("{size:?} px, shifted {shift:?}");
                    let d = drawn(curve, view, size, shift);
                    let mut samples = dense(&d, |t| [t.cos(), t.sin()], [0.0, TAU]);
                    samples.extend(dense(&d, |t| [2.003 + t.cos(), t.sin()], [0.0, TAU]));
                    assert_near(&d, &samples, &what);
                    for line in &d.lines {
                        let left = line.points[0][0] < middle;
                        assert!(
                            line.points.iter().all(|p| (p[0] < middle) == left),
                            "{what}: a line from one circle to the other"
                        );
                    }
                    if let Some(loops) = loops {
                        assert_eq!(d.lines.len(), loops, "{what}");
                        assert!(d.lines.iter().all(|l| l.closed), "{what}");
                    }
                }
            }
        }
    }

    /// `sin(x + y) - cos(x·y) + 1 = 0` is `sin(u) = cos(v) - 1` in u = x + y and v = x·y, so it's
    /// where v = 2πk ± acos(1 + sin(u)), for u where sin(u) ≤ 0, and x and y are the two roots of
    /// t² - u·t + v, either way round. Its points from that, which don't use the tracer.
    fn lenses(d: &Drawn) -> Vec<[f64; 2]> {
        use std::f64::consts::{PI, TAU};
        let mut samples = Vec::new();
        for m in -4..=4 {
            let u = [(2.0 * f64::from(m) - 1.0) * PI, 2.0 * f64::from(m) * PI];
            for k in -20..=20 {
                for side in [-1.0, 1.0] {
                    for sheet in [-1.0, 1.0] {
                        let point = |u: f64| {
                            let turn = (1.0 + u.sin()).clamp(-1.0, 1.0).acos();
                            let v = TAU * f64::from(k) + side * turn;
                            // NaN where x and y aren't real.
                            let root = (u * u - 4.0 * v).sqrt();
                            [(u + sheet * root) / 2.0, (u - sheet * root) / 2.0]
                        };
                        samples.extend(dense(d, point, u));
                    }
                }
            }
        }
        samples
    }

    /// Which piece of the curve each sample is on, as a number: samples closer than `link`
    /// pixels are on the same piece.
    fn pieces(d: &Drawn, samples: &[[f64; 2]], link: f64) -> Vec<usize> {
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let px: Vec<[f64; 2]> = samples.iter().map(|p| d.px(*p)).collect();
        let bin = |q: [f64; 2]| ((q[0] / link).floor() as i64, (q[1] / link).floor() as i64);
        let mut bins: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, q) in px.iter().enumerate() {
            bins.entry(bin(*q)).or_default().push(i);
        }
        let mut parent: Vec<usize> = (0..px.len()).collect();
        for (i, q) in px.iter().enumerate() {
            let (a, b) = bin(*q);
            for key in (a - 1..=a + 1).flat_map(|x| (b - 1..=b + 1).map(move |y| (x, y))) {
                for &j in bins.get(&key).into_iter().flatten() {
                    let r = px[j];
                    if j > i && (r[0] - q[0]).hypot(r[1] - q[1]) <= link {
                        let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                        parent[ri] = rj;
                    }
                }
            }
        }
        (0..px.len()).map(|i| root(&mut parent, i)).collect()
    }

    /// `sin(x + y) - cos(x·y) + 1 = 0` in `0 ± 10` is loops, many of them long and thin toward the
    /// corners, narrowing to tips under a pixel wide. Each loop wholly in the frame is traced as
    /// one closed line that reaches round its tip, and not as a loop cut short with an island
    /// beyond it, and the trace and the curve are near each other both ways.
    #[test]
    fn thin_loops_are_traced_whole() {
        for shift in SHIFTS {
            let what = format!("1100 px, shifted {shift:?}");
            let d = drawn(
                "sin(x + y) - cos(x·y) + 1 = 0",
                [[-10.0, 10.0], [-10.0, 10.0]],
                [1100, 1100],
                shift,
            );
            let samples = lenses(&d);
            assert_near(&d, &samples, &what);
            let piece = pieces(&d, &samples, 0.12);
            let mut inside: BTreeMap<usize, bool> = BTreeMap::new();
            for (p, k) in samples.iter().zip(&piece) {
                *inside.entry(*k).or_insert(true) &= d.in_frame(d.px(*p));
            }
            // The piece each traced vertex is on, by its nearest sample.
            let bin = |q: [f64; 2]| ((q[0] / 2.0).floor() as i64, (q[1] / 2.0).floor() as i64);
            let mut bins: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
            for (i, p) in samples.iter().enumerate() {
                bins.entry(bin(d.px(*p))).or_default().push(i);
            }
            let mut lines_on: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
            let mut on_lines: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); d.lines.len()];
            for (n, line) in d.lines.iter().enumerate() {
                for p in &line.points {
                    let q = d.px(*p);
                    let (a, b) = bin(q);
                    let nearest = (a - 1..=a + 1)
                        .flat_map(|x| (b - 1..=b + 1).map(move |y| (x, y)))
                        .filter_map(|key| bins.get(&key))
                        .flatten()
                        .map(|&i| {
                            let r = d.px(samples[i]);
                            ((r[0] - q[0]).hypot(r[1] - q[1]), i)
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0));
                    if let Some((_, i)) = nearest {
                        lines_on.entry(piece[i]).or_default().insert(n);
                        on_lines[n].insert(piece[i]);
                    }
                }
            }
            let whole: Vec<usize> = inside
                .iter()
                .filter(|(_, i)| **i)
                .map(|(k, _)| *k)
                .collect();
            assert!(
                whole.len() > 40,
                "{what}: {} loops in the frame",
                whole.len()
            );
            for k in whole {
                let lines = &lines_on[&k];
                assert_eq!(
                    lines.len(),
                    1,
                    "{what}: a loop traced as {} lines",
                    lines.len()
                );
                let n = *lines.first().unwrap();
                assert!(d.lines[n].closed, "{what}: a loop traced open");
                assert_eq!(on_lines[n].len(), 1, "{what}: a line on two loops");
            }
        }
    }

    /// Zoomed out to `0 ± 20`, the same loops have more detail than the grid can follow, and the
    /// trace runs out of budget for them, but what it draws stays where the curve is: no line runs
    /// further than a few cells, as lines did where Newton steps left for another part of the curve.
    #[test]
    fn detail_past_the_grid_stays_where_the_curve_is() {
        let d = drawn(
            "sin(x + y) - cos(x·y) + 1 = 0",
            [[-20.0, 20.0], [-20.0, 20.0]],
            [1100, 1100],
            [0.0, 0.0],
        );
        let cell = 1100.0 / (1100.0 / CELL_PX).ceil();
        for s in d.segments() {
            let length = (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]);
            assert!(
                length <= 3.0 * cell,
                "a line {length} px long at {:?} px",
                s[0]
            );
        }
    }

    /// A long thin ellipse that turns and drifts a little each frame, with tips sharp enough to be
    /// bent, starts where the curve takes its start: from one frame to the next it moves about as
    /// far as the curve does, and not by the pixel or more that bending a tip once more or once
    /// fewer moved it when the start was found from the mean of the line's points.
    #[test]
    fn a_loop_keeps_its_start_as_it_moves() {
        for minor in [0.1, 0.02] {
            let mut last: Option<[f64; 2]> = None;
            let mut worst = (0.0f64, 0);
            for frame in 0..=40 {
                let t = f64::from(frame) * 0.05;
                let (a, u) = (0.5 + 0.002 * t, 0.003 * t);
                let curve = format!(
                    "(((x - {u})·cos({a}) + y·sin({a}))/1.5)² + ((-(x - {u})·sin({a}) + y·cos({a}))/{minor})² = 1"
                );
                let d = drawn(&curve, [[-2.0, 2.0], [-2.0, 2.0]], [812, 812], [0.0, 0.0]);
                assert_eq!(d.lines.len(), 1, "{minor}, frame {frame}");
                assert!(d.lines[0].closed, "{minor}, frame {frame}");
                let start = d.px(d.lines[0].points[0]);
                if let Some(last) = last {
                    let moved = (start[0] - last[0]).hypot(start[1] - last[1]);
                    if moved > worst.0 {
                        worst = (moved, frame);
                    }
                }
                last = Some(start);
            }
            eprintln!(
                "minor {minor}: the start moved {:.4} px at most, into frame {}",
                worst.0, worst.1
            );
            assert!(
                worst.0 <= 0.1,
                "minor axis {minor}: the start moved {} px into frame {}",
                worst.0,
                worst.1
            );
        }
    }

    /// `sin(100·x)·sin(100·y) = 0` is lines a cell or so apart, crossing everywhere, more than the
    /// budget can refine. No two traced lines run along one piece of it, since a line is bent only
    /// near its own cell, so two cells can't both follow the same piece.
    #[test]
    fn no_two_lines_follow_one_piece() {
        for shift in [[0.0, 0.0], [0.5, 0.5]] {
            let d = drawn(
                "sin(100·x)·sin(100·y) = 0",
                [[-2.0, 2.0], [-2.0, 2.0]],
                [812, 812],
                shift,
            );
            let mut pieces = Vec::new();
            for (n, line) in d.lines.iter().enumerate() {
                for (k, w) in line.points.windows(2).enumerate() {
                    pieces.push((n, k, [d.px(w[0]), d.px(w[1])]));
                }
            }
            let bin = |q: [f64; 2]| (q[0].floor() as i64, q[1].floor() as i64);
            let mut bins: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
            for (i, (_, _, s)) in pieces.iter().enumerate() {
                let (lo, hi) = (
                    bin([s[0][0].min(s[1][0]), s[0][1].min(s[1][1])]),
                    bin([s[0][0].max(s[1][0]), s[0][1].max(s[1][1])]),
                );
                for x in lo.0..=hi.0 {
                    for y in lo.1..=hi.1 {
                        bins.entry((x, y)).or_default().push(i);
                    }
                }
            }
            let direction = |s: &[[f64; 2]; 2]| {
                let d = [s[1][0] - s[0][0], s[1][1] - s[0][1]];
                let l = d[0].hypot(d[1]);
                [d[0] / l, d[1] / l]
            };
            for (n, k, s) in &pieces {
                let middle = [(s[0][0] + s[1][0]) / 2.0, (s[0][1] + s[1][1]) / 2.0];
                let (u, (bx, by)) = (direction(s), bin(middle));
                for i in (bx - 1..=bx + 1)
                    .flat_map(|x| (by - 1..=by + 1).map(move |y| (x, y)))
                    .filter_map(|key| bins.get(&key))
                    .flatten()
                {
                    let (m, j, t) = &pieces[*i];
                    if m == n && j.abs_diff(*k) <= 1 {
                        continue;
                    }
                    let v = direction(t);
                    if (u[0] * v[0] + u[1] * v[1]).abs() < 0.9 {
                        continue;
                    }
                    // How far the middle is from the line through the other, and how much of
                    // that line the two have in common.
                    let off = ((middle[0] - t[0][0]) * v[1] - (middle[1] - t[0][1]) * v[0]).abs();
                    let at = |q: [f64; 2]| (q[0] - t[0][0]) * v[0] + (q[1] - t[0][1]) * v[1];
                    let (s0, s1) = (at(s[0]).min(at(s[1])), at(s[0]).max(at(s[1])));
                    let (t0, t1) = (0.0f64.min(at(t[1])), 0.0f64.max(at(t[1])));
                    let common = s1.min(t1) - s0.max(t0);
                    assert!(
                        !(off < 0.05 && common > 0.05),
                        "shifted {shift:?}: lines {n} and {m} both run along {middle:?} px"
                    );
                }
            }
        }
    }

    /// Refining near a pole or a jump that changes the field's sign keeps the rule of issue 28: no
    /// line is traced across one, and the curve stops within a cell or so of it. Away from it the
    /// trace and the curve are near each other both ways, at sizes where tips are bent and cells
    /// split: `y = tan(x)` zoomed out, whose branches turn in about two cells; a sine with tips
    /// that `floor` shifts at each whole number; and a pole along y = 0.99, just below the tips.
    #[test]
    fn refining_keeps_poles_and_jumps() {
        use std::f64::consts::{FRAC_PI_2, PI};
        type Graph = fn(f64) -> f64;
        // Each curve, its view and size, its pieces as graphs over ranges of x, and its poles and
        // jumps, as lines x = c or y = c.
        type Case = (
            &'static str,
            [[f64; 2]; 2],
            [u32; 2],
            Graph,
            Vec<[f64; 2]>,
            Vec<(usize, f64)>,
        );
        let cases: [Case; 3] = [
            (
                "y = tan(x)",
                [[-80.0, 80.0], [-80.0, 80.0]],
                [812, 812],
                f64::tan,
                (-27..=26)
                    .map(|k| [f64::from(k) * PI - FRAC_PI_2, f64::from(k) * PI + FRAC_PI_2])
                    .collect(),
                (-27..=26)
                    .map(|k| (0, f64::from(k) * PI + FRAC_PI_2))
                    .collect(),
            ),
            (
                "y = sin(30·x) + 2·floor(x)",
                [[-2.0, 2.0], [-5.0, 3.0]],
                [600, 1200],
                |x| (30.0 * x).sin() + 2.0 * x.floor(),
                (-3..=2).map(|k| [f64::from(k), f64::from(k + 1)]).collect(),
                (-2..=2).map(|k| (0, f64::from(k))).collect(),
            ),
            (
                "(y - sin(30·x))/(0.99 - y) = 0",
                [[-2.0, 2.0], [-2.0, 2.0]],
                [812, 812],
                |x| (30.0 * x).sin(),
                vec![[-2.2, 2.2]],
                vec![(1, 0.99)],
            ),
        ];
        for (curve, view, size, graph, pieces, cuts) in cases {
            for shift in SHIFTS {
                let what = format!("{curve}, shifted {shift:?}");
                let (d, work) = worked(curve, view, size, shift);
                assert!(work.bends + work.splits > 0, "{what}: nothing refined");
                // Within a cell and a half of a pole or a jump, the curve may stop.
                let cell = d.size[0] / (d.size[0] / CELL_PX).ceil() * 1.5 / d.scale();
                let near = |p: [f64; 2]| cuts.iter().any(|&(k, c)| (p[k] - c).abs() < cell);
                let mut samples = Vec::new();
                for range in &pieces {
                    let inside = [range[0] + 1e-9, range[1] - 1e-9];
                    samples.extend(dense(&d, |x| [x, graph(x)], inside));
                }
                samples.retain(|p| !near(*p));
                let [off, missed] = apart(&d, &samples);
                // The trace may stop short of a cut, so only the curve away from them is checked
                // against it, and every traced point is checked against all the curve.
                assert!(
                    missed.0 <= NEAR_PX,
                    "{what}: the curve is {} px off the trace at {:?} px",
                    missed.0,
                    missed.1
                );
                let all: Vec<[f64; 2]> = pieces
                    .iter()
                    .flat_map(|r| dense(&d, |x| [x, graph(x)], [r[0] + 1e-9, r[1] - 1e-9]))
                    .collect();
                let [off_all, _] = apart(&d, &all);
                assert!(
                    off_all.0 <= NEAR_PX,
                    "{what}: the trace is {} px off the curve at {:?} px ({:?})",
                    off_all.0,
                    off_all.1,
                    off.1
                );
                for line in &d.lines {
                    for w in line.points.windows(2) {
                        for &(k, c) in &cuts {
                            assert!(
                                (w[0][k] < c) == (w[1][k] < c),
                                "{what}: a line across {} = {c} at {:?} px",
                                ["x", "y"][k],
                                d.px(w[0])
                            );
                        }
                    }
                }
            }
        }
    }

    /// A refined trace is the same each time it's made.
    #[test]
    fn refining_is_deterministic() {
        let (curve, view) = (
            "sin(x + y) - cos(x·y) + 1 = 0",
            [[-10.0, 10.0], [-10.0, 10.0]],
        );
        let (first, work) = worked(curve, view, [812, 812], [0.0, 0.0]);
        assert!(work.bends > 0 && work.splits > 0, "{work:?}");
        let (second, _) = worked(curve, view, [812, 812], [0.0, 0.0]);
        assert_eq!(first.lines, second.lines);
    }

    /// A curve whose trace doesn't stray is bent and split nowhere, and the only work past the
    /// grid's own is checking its lines. One whose field's gradient vanishes on it, as a squared
    /// or cubed field's does, costs about what the plain one does, and is traced as near.
    #[test]
    fn a_trace_that_does_not_stray_costs_what_the_grid_does() {
        use std::f64::consts::TAU;
        let view = [[-2.0, 2.0], [-2.0, 2.0]];
        let (d, plain) = worked("x² + y² = 1.5625", view, [1633, 1633], [0.0, 0.0]);
        let points: usize = d.lines.iter().map(|l| l.points.len()).sum();
        assert_eq!((plain.splits, plain.bends), (0, 0), "{plain:?}");
        assert!(plain.spent <= 3 * points, "{plain:?} for {points} points");
        for curve in [
            "(x² + y² - 1.5625)·abs(x² + y² - 1.5625) = 0",
            "(x² + y² - 1.5625)^3 = 0",
        ] {
            let (d, work) = worked(curve, view, [1633, 1633], [0.0, 0.0]);
            assert_eq!((work.splits, work.bends), (0, 0), "{curve}: {work:?}");
            assert!(
                work.evals as f64 <= 1.25 * plain.evals as f64,
                "{curve}: {} times the field, where the plain circle takes {}",
                work.evals,
                plain.evals
            );
            let samples = dense(&d, |t| [1.25 * t.cos(), 1.25 * t.sin()], [0.0, TAU]);
            assert_near(&d, &samples, curve);
        }
    }
}
