//! The CPU oracle: naive and double precision, and meant to stay obviously correct.
//! Performance work belongs in the GPU path. `wgsl.rs` must implement the same
//! shading as `Frame::shade`, and `pencil check` compares the two.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::Figure;
use crate::error::Error;
use crate::frame::{self, Inputs, Prepared};
use crate::ir::{Expr, Form, Scene, System};
use crate::stroke::{self, box_coverage};
use crate::style::{Blend, MarkStyle, SELF_CURVATURE, SELF_DIST, SELF_PIXEL, Tone};

/// How punchy a move is: it gets near its destination fast, then eases in, leaving time to
/// take it in. 5 is Kris's choice in `lab/expressions`. Above it, `za` joining `+zbi` on their
/// way to `za + zbi` goes by too quickly to see. The blog's is 4.
pub const SNAP_POWER: f64 = 5.0;

/// 0 at or before 0, 1 at or after 1, getting there fast and easing in.
pub fn snap(x: f64, power: Option<f64>) -> f64 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powf(power.unwrap_or(SNAP_POWER))
}

/// Below this gradient length the field's distance estimate is meaningless.
pub const MIN_GRADIENT: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Real(f64),
    Color([f64; 3]),
}

#[derive(Clone, Debug)]
pub struct Env {
    /// `None` is a binding that doesn't exist this frame.
    vars: HashMap<String, Option<f64>>,
}

impl Env {
    pub fn new(time: f64) -> Self {
        let mut vars = HashMap::new();
        vars.insert("time".to_string(), Some(time));
        Self { vars }
    }

    pub fn set(&mut self, name: &str, value: f64) {
        // Collections and curves set their variables a lot, so don't allocate for a known name.
        if let Some(slot) = self.vars.get_mut(name) {
            *slot = Some(value);
        } else {
            self.vars.insert(name.to_string(), Some(value));
        }
    }

    pub fn set_absent(&mut self, name: &str) {
        self.vars.insert(name.to_string(), None);
    }

    /// A value, or nothing if it's unknown or doesn't exist this frame.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.vars.get(name).copied().flatten()
    }

    pub fn is_absent(&self, name: &str) -> bool {
        matches!(self.vars.get(name), Some(None))
    }
}

/// Converts OKLCH (hue in degrees) to linear sRGB, unclamped.
pub fn oklch(l: f64, c: f64, h: f64) -> [f64; 3] {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let lm = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let mm = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let sm = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        4.0767416621 * lm - 3.3077115913 * mm + 0.2309699292 * sm,
        -1.2684380046 * lm + 2.6097574011 * mm - 0.3413193965 * sm,
        -0.0041960863 * lm - 0.7034186147 * mm + 1.7076147010 * sm,
    ]
}

/// A hue index in 0..1 mapped around a perceptual ramp, so that equal steps look equal.
pub fn hue(k: f64) -> [f64; 3] {
    oklch(0.68, 0.14, 360.0 * k)
}

pub fn srgb_encode(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

pub fn srgb_decode(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn eval(e: &Expr, env: &Env) -> Result<Value, Error> {
    use Value::{Color, Real};
    let real = |x: &Expr| self::real(x, env);
    Ok(match e {
        Expr::Num(n) => Real(*n),
        Expr::Sym(s) => match s.as_str() {
            "pi" | "π" => Real(std::f64::consts::PI),
            "tau" => Real(std::f64::consts::TAU),
            "white" => Color([1.0; 3]),
            "black" => Color([0.0; 3]),
            _ => match env.vars.get(s) {
                Some(Some(v)) => Real(*v),
                Some(None) => return Err(Error::absent()),
                None => return Err(Error::new(format!("no value for `{s}`"))),
            },
        },
        Expr::App(head, args) => match (head.as_str(), args.as_slice()) {
            ("Add" | "Subtract" | "Multiply" | "Divide", [a, b]) => {
                match (head.as_str(), eval(a, env)?, eval(b, env)?) {
                    ("Add", Real(a), Real(b)) => Real(a + b),
                    ("Subtract", Real(a), Real(b)) => Real(a - b),
                    ("Multiply", Real(a), Real(b)) => Real(a * b),
                    ("Divide", Real(a), Real(b)) => Real(a / b),
                    ("Add", Color(a), Color(b)) => Color(std::array::from_fn(|i| a[i] + b[i])),
                    ("Subtract", Color(a), Color(b)) => Color(std::array::from_fn(|i| a[i] - b[i])),
                    ("Multiply", Color(c), Real(k)) | ("Multiply", Real(k), Color(c)) => {
                        Color(c.map(|v| v * k))
                    }
                    ("Divide", Color(c), Real(k)) => Color(c.map(|v| v / k)),
                    _ => return Err(Error::new(format!("`{head}` can't take these"))),
                }
            }
            ("Power", [a, b]) => Real(real(a)?.powf(real(b)?)),
            ("Negate", [a]) => Real(-real(a)?),
            ("Sqrt", [a]) => Real(real(a)?.sqrt()),
            ("Sin", [a]) => Real(real(a)?.sin()),
            ("Cos", [a]) => Real(real(a)?.cos()),
            ("Tan", [a]) => Real(real(a)?.tan()),
            ("Arccos", [a]) => Real(real(a)?.acos()),
            ("Floor", [a]) => Real(real(a)?.floor()),
            ("IfNonnegative", [x, a, b]) => {
                if real(x)? >= 0.0 {
                    Real(real(a)?)
                } else {
                    Real(real(b)?)
                }
            }
            // A partial value: it exists where the condition holds. A condition that isn't
            // a number, from dividing by zero, doesn't hold.
            ("Where", [condition, value]) => {
                if real(condition)? >= 0.0 {
                    eval(value, env)?
                } else {
                    return Err(Error::absent());
                }
            }
            // Whether every field of a value exists: the one test that sees absence.
            ("Exists", fields) => {
                let mut all = 1.0;
                for field in fields {
                    match real(field) {
                        Ok(_) => {}
                        Err(e) if e.absent => all = 0.0,
                        Err(e) => return Err(e),
                    }
                }
                Real(all)
            }
            // A beat's strength: nothing before it, `now` during it, `later` after it.
            ("Reveal", [age, now, later]) => {
                let age = real(age)?;
                Real(if age < -0.5 {
                    0.0
                } else if age < 0.5 {
                    real(now)?
                } else {
                    real(later)?
                })
            }
            ("Sinh", [a]) => Real(real(a)?.clamp(-TANH_LIMIT, TANH_LIMIT).sinh()),
            ("Cosh", [a]) => Real(real(a)?.clamp(-TANH_LIMIT, TANH_LIMIT).cosh()),
            ("Tanh", [a]) => Real(real(a)?.clamp(-TANH_LIMIT, TANH_LIMIT).tanh()),
            ("Exp", [a]) => Real(real(a)?.exp()),
            ("Ln", [a]) => Real(real(a)?.ln()),
            ("Abs", [a]) => Real(real(a)?.abs()),
            // WGSL's sign(0) is 0, unlike f64::signum.
            ("Sign", [a]) => {
                let v = real(a)?;
                Real(if v == 0.0 { 0.0 } else { v.signum() })
            }
            ("Arctan2", [y, x]) => Real(real(y)?.atan2(real(x)?)),
            ("Min", [a, b]) => Real(real(a)?.min(real(b)?)),
            ("Max", [a, b]) => Real(real(a)?.max(real(b)?)),
            ("Clamp", [x, lo, hi]) => Real(real(x)?.max(real(lo)?).min(real(hi)?)),
            ("Mod", [x, y]) => {
                let (x, y) = (real(x)?, real(y)?);
                Real(x - y * (x / y).floor())
            }
            ("Smoothstep", [a, b, t]) => {
                let (a, b, t) = (real(a)?, real(b)?, real(t)?);
                let x = ((t - a) / (b - a)).clamp(0.0, 1.0);
                Real(x * x * (3.0 - 2.0 * x))
            }
            ("Snap", [x]) => Real(snap(real(x)?, None)),
            ("Snap", [x, power]) => Real(snap(real(x)?, Some(real(power)?))),
            // Which step of a list of moves a time is in: `start end` for each. A whole number
            // while holding, and between two while moving, linearly, since easing is `snap`'s.
            ("Timeline", [time, moves @ ..]) if !moves.is_empty() && moves.len() % 2 == 0 => {
                let time = real(time)?;
                let mut step = 0.0;
                for window in moves.chunks(2) {
                    let (start, end) = (real(&window[0])?, real(&window[1])?);
                    if time >= end {
                        step += 1.0;
                    } else {
                        if time > start {
                            step += (time - start) / (end - start);
                        }
                        break;
                    }
                }
                Real(step)
            }
            ("Pixels", [a]) => Real(real(a)?),
            ("Oklch", [l, c, h]) => Color(oklch(real(l)?, real(c)?, real(h)?)),
            ("Rgb", [r, g, b]) => Color([
                srgb_decode(real(r)? / 255.0),
                srgb_decode(real(g)? / 255.0),
                srgb_decode(real(b)? / 255.0),
            ]),
            ("Hue", [k]) => Color(hue(real(k)?)),
            ("Mix", [a, b, t]) => {
                let t = real(t)?;
                match (eval(a, env)?, eval(b, env)?) {
                    (Real(a), Real(b)) => Real(a + (b - a) * t),
                    (Color(a), Color(b)) => {
                        Color(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t))
                    }
                    _ => return Err(Error::new("`mix` needs two values of one type")),
                }
            }
            _ => return Err(Error::new(format!("can't evaluate `{head}`"))),
        },
    })
}

/// How many steps Newton's method gets. It's quick or it's not going to work.
const SOLVE_STEPS: usize = 50;
/// How close to zero an equation has to come to count as solved.
const SOLVE_RESIDUAL: f64 = 1e-9;
/// How small a step means it's arrived, relative to the size of the unknowns.
const SOLVE_STEP: f64 = 1e-14;

/// Newton's method, from the start every time: a pure function of the frame, with no memory
/// of the last answer. A system that doesn't settle on a solution is absent, as an
/// intersection that doesn't exist is.
pub fn solve(system: &System, env: &Env) -> Result<Vec<f64>, Error> {
    let System {
        unknowns: names,
        start,
        equations,
        jacobian,
        ..
    } = system;
    let n = names.len();
    let mut at = start
        .iter()
        .map(|e| real(e, env))
        .collect::<Result<Vec<_>, _>>()?;
    let mut local = env.clone();
    for _ in 0..SOLVE_STEPS {
        for (name, value) in names.iter().zip(&at) {
            local.set(name, *value);
        }
        let f = equations
            .iter()
            .map(|e| real(e, &local))
            .collect::<Result<Vec<_>, _>>()?;
        let j = jacobian
            .iter()
            .map(|e| real(e, &local))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(step) = gaussian(j, f.clone(), n) else {
            return Err(Error::absent());
        };
        let size = at.iter().fold(1.0f64, |m, v| m.max(v.abs()));
        let moved = step.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        for (value, change) in at.iter_mut().zip(&step) {
            *value -= change;
        }
        if at.iter().any(|v| !v.is_finite()) {
            return Err(Error::absent());
        }
        if moved <= SOLVE_STEP * size {
            break;
        }
    }
    // It has to have really got there, wherever it stopped.
    for (name, value) in names.iter().zip(&at) {
        local.set(name, *value);
    }
    for equation in equations {
        let left = real(equation, &local)?.abs();
        if left.is_nan() || left > SOLVE_RESIDUAL {
            return Err(Error::absent());
        }
    }
    Ok(at)
}

/// Solves `a x = b` for an `n` by `n` system, with partial pivoting. Nothing if it's singular.
fn gaussian(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    for col in 0..n {
        let pivot = (col..n).max_by(|i, j| {
            a[i * n + col]
                .abs()
                .partial_cmp(&a[j * n + col].abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        let size = a[pivot * n + col].abs();
        if size.is_nan() || size <= 1e-300 {
            return None;
        }
        if pivot != col {
            for k in 0..n {
                a.swap(pivot * n + k, col * n + k);
            }
            b.swap(pivot, col);
        }
        for row in col + 1..n {
            let factor = a[row * n + col] / a[col * n + col];
            for k in col..n {
                a[row * n + k] -= factor * a[col * n + k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let done: f64 = (row + 1..n).map(|k| a[row * n + k] * x[k]).sum();
        x[row] = (b[row] - done) / a[row * n + row];
    }
    Some(x)
}

pub fn real(e: &Expr, env: &Env) -> Result<f64, Error> {
    match eval(e, env)? {
        Value::Real(v) => Ok(v),
        Value::Color(_) => Err(Error::new("expected a number, found a color")),
    }
}

pub fn color(e: &Expr, env: &Env) -> Result<[f64; 3], Error> {
    match eval(e, env)? {
        Value::Color(c) => Ok(c),
        Value::Real(_) => Err(Error::new("expected a color, found a number")),
    }
}

/// One figure's last answer to each of its systems, and what it was the answer to. A system
/// is expensive and a frame asks several times, so this saves repeating it. The key is
/// everything the answer depends on, so it can't change what a frame shows.
#[derive(Debug, Default)]
pub struct Solved(Mutex<Vec<Option<SolvedEntry>>>);

type SolvedEntry = (Vec<u64>, Result<Vec<f64>, Error>);

fn solved(
    system: &System,
    index: usize,
    env: &Env,
    memo: Option<&Solved>,
) -> Result<Vec<f64>, Error> {
    let Some(memo) = memo else {
        return solve(system, env);
    };
    let key: Vec<u64> = system
        .depends
        .iter()
        .map(|name| env.get(name).map_or(u64::MAX, f64::to_bits))
        .collect();
    let mut entries = memo.0.lock().unwrap_or_else(|e| e.into_inner());
    if entries.len() <= index {
        entries.resize_with(index + 1, || None);
    }
    if let Some((known, answer)) = &entries[index]
        && *known == key
    {
        return answer.clone();
    }
    let answer = solve(system, env);
    entries[index] = Some((key, answer.clone()));
    answer
}

/// The values of a frame's bindings and signals. An input overrides a param.
pub fn frame_env(figure: &Figure, inputs: &Inputs) -> Result<Env, Error> {
    scene_env(&figure.scene, inputs, Some(&figure.solved))
}

pub fn scene_env(scene: &Scene, inputs: &Inputs, memo: Option<&Solved>) -> Result<Env, Error> {
    let mut env = Env::new(inputs.time);
    // A system's unknowns are bindings in a row, and one solve answers them all.
    let mut answer: Option<Result<Vec<f64>, Error>> = None;
    for binding in &scene.bindings {
        let value = match (
            inputs.params.iter().find(|(id, _)| *id == binding.id),
            &binding.expr,
        ) {
            (Some((_, value)), _) => Ok(*value),
            (None, Expr::App(head, args)) if head == "Solved" => {
                let [Expr::Num(system), Expr::Num(k)] = args.as_slice() else {
                    return Err(Error::new("a malformed `solve`"));
                };
                if *k == 0.0 {
                    let index = *system as usize;
                    answer = Some(solved(&scene.systems[index], index, &env, memo));
                }
                match answer.as_ref() {
                    Some(Ok(values)) => Ok(values[*k as usize]),
                    Some(Err(e)) => Err(e.clone()),
                    None => Err(Error::new("a malformed `solve`")),
                }
            }
            (None, _) => real(&binding.expr, &env),
        };
        match value {
            Ok(value) => env.set(&binding.id, value),
            Err(e) if e.absent => env.set_absent(&binding.id),
            Err(e) => return Err(e),
        }
    }
    Ok(env)
}

/// Moves a color into the space its mark blends in.
fn to_blend(c: [f64; 3], blend: Blend) -> [f64; 3] {
    match blend {
        Blend::Linear => c,
        Blend::Gamma => c.map(srgb_encode),
        Blend::Add => c.map(srgb_lift),
    }
}

fn from_blend(c: [f64; 3], blend: Blend) -> [f64; 3] {
    match blend {
        Blend::Linear => c,
        Blend::Gamma | Blend::Add => c.map(srgb_decode),
    }
}

/// sRGB's encoding without its limit at 1, for what's adding light.
pub fn srgb_lift(c: f64) -> f64 {
    let c = c.max(0.0);
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// tanh's argument is held here, so that a glow at its own edge, dividing by zero, is 1. Past this
/// it's 1 to well beyond what a color can show.
pub const TANH_LIMIT: f64 = 20.0;

fn over(under: [f64; 3], top: [f64; 3], coverage: f64) -> [f64; 3] {
    std::array::from_fn(|i| under[i] + (top[i] - under[i]) * coverage)
}

/// One frame's evaluation: a pure function of the figure, its inputs and the size.
pub struct Frame<'a> {
    figure: &'a Figure,
    env: Env,
    width: u32,
    height: u32,
    prepared: Prepared,
    sided: Vec<crate::Sided>,
    /// One coverage buffer per line stroke, in mark order.
    coverage: Vec<Vec<f64>>,
    dpr: f64,
}

impl<'a> Frame<'a> {
    pub fn new(
        figure: &'a Figure,
        inputs: &Inputs,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        let dpr = frame::design_scale(figure, width).unwrap_or(1.0);
        let prepared = frame::prepare(figure, inputs, width, height, dpr)?;
        let coverage = prepared
            .strokes
            .iter()
            .map(|segments| stroke::rasterize(segments, width, height))
            .collect();
        Ok(Self {
            figure,
            env: frame_env(figure, inputs)?,
            width,
            height,
            sided: figure.sided(),
            prepared,
            coverage,
            dpr,
        })
    }

    fn line_coverage(&self, layer: usize, px: f64, py: f64) -> f64 {
        let (x, y) = (px.floor() as i64, py.floor() as i64);
        let (w, h) = (self.width as i64, self.height as i64);
        if x < 0 || y < 0 || x >= w || y >= h {
            return 0.0;
        }
        self.coverage[layer][(y * w + x) as usize]
    }

    /// The linear color at a pixel center, shaded in mark order over the paper.
    pub fn shade(&self, px: f64, py: f64) -> Result<[f64; 3], Error> {
        let view = &self.prepared.view;
        let (w, h) = (self.width as f64, self.height as f64);
        let (sx, sy) = (view.x[1] - view.x[0], view.y[1] - view.y[0]);
        let x = view.x[0] + px / w * sx;
        let y = view.y[1] - py / h * sy;
        let pixel = (sx / w + sy / h) / 2.0;

        let mut out = match &self.figure.styles.paper {
            Some(paper) => color(paper, &self.env)?,
            None => [1.0; 3],
        };
        let mut layer = 0;
        let mut region = 0;
        for (i, (mark, style)) in self
            .figure
            .scene
            .marks
            .iter()
            .zip(&self.figure.styles.marks)
            .enumerate()
        {
            // An absent mark draws nothing, but keeps its places in the shared tables.
            if !self.prepared.shown[i] {
                layer += usize::from(style.stroke_is_line(&mark.form));
                region += usize::from(matches!(mark.form, Form::Region(_)));
                continue;
            }
            let mut env = self.env.clone();
            // Distance in device pixels and its direction, where the form has them.
            let measured = match &mark.form {
                Form::Field {
                    vars,
                    field,
                    grad,
                    exact,
                    curvature,
                    ..
                } => {
                    env.set(&vars[0], x);
                    env.set(&vars[1], y);
                    let f = real(field, &env)?;
                    let (gx, gy) = (real(&grad[0], &env)?, real(&grad[1], &env)?);
                    let glen = gx.hypot(gy);
                    if let Some(curvature) = curvature {
                        env.set(SELF_CURVATURE, real(curvature, &env)?);
                    }
                    // A true distance doesn't need its gradient, which is lost at a circle's center.
                    let dist = if *exact {
                        f
                    } else {
                        f / glen.max(MIN_GRADIENT)
                    };
                    let n = if glen > MIN_GRADIENT {
                        (gx / glen, gy / glen)
                    } else {
                        (1.0, 0.0)
                    };
                    Some((dist / pixel, n))
                }
                Form::Region(_) => {
                    let m = frame::polygon_distance(&self.prepared.geometry, region, px, py);
                    region += 1;
                    Some(m)
                }
                Form::Dot(at) => {
                    let (cx, cy) = frame::device(
                        real(&at[0], &env)?,
                        real(&at[1], &env)?,
                        view,
                        self.width,
                        self.height,
                    );
                    let radius = frame::dot_radius(style, &env)? * self.dpr;
                    let (dx, dy) = (px - cx, py - cy);
                    let rho = dx.hypot(dy);
                    let n = if rho > 1e-6 {
                        (dx / rho, dy / rho)
                    } else {
                        (1.0, 0.0)
                    };
                    Some((rho - radius, n))
                }
                Form::Label { .. } | Form::Term { .. } => continue,
                Form::Outline { .. } => None,
                Form::Parametric(_)
                | Form::Polyline { .. }
                | Form::Arrow { .. }
                | Form::Many { .. } => None,
            };
            let opacity = match &style.opacity {
                Some(o) => real(o, &env)?.clamp(0.0, 1.0),
                None => 1.0,
            };
            // Every circle, in order, from the same numbers the shader gets.
            if let Some(group) = self.sided.iter().find(|group| group.mark == i) {
                let count = self.prepared.values[group.slot] as usize;
                let data = &self.prepared.sides[group.base * 4..];
                for row in data.chunks(group.stride * 4).take(count) {
                    let [a, bx, by, c] = [row[0], row[1], row[2], row[3]].map(f64::from);
                    for (name, value) in group.names.iter().zip(&row[4..]) {
                        env.set(name, f64::from(*value));
                    }
                    let f = a * (x * x + y * y) - 2.0 * (bx * x + by * y) + c;
                    let spread = 1.0 + a * f;
                    let dist = f / (1.0 + spread.max(0.0).sqrt());
                    let root = spread.max(1e-12).sqrt();
                    let (gx, gy) = ((a * x - bx) / root, (a * y - by) / root);
                    let glen = gx.hypot(gy);
                    let n = if glen > MIN_GRADIENT {
                        (gx / glen, gy / glen)
                    } else {
                        (1.0, 0.0)
                    };
                    env.set(SELF_CURVATURE, a);
                    env.set(SELF_DIST, dist);
                    env.set(SELF_PIXEL, pixel);
                    let fill_opacity = match &style.fill_opacity {
                        Some(o) => real(o, &env)?.clamp(0.0, 1.0),
                        None => 1.0,
                    };
                    out = self.fill(out, style, &env, dist / pixel, n, opacity * fill_opacity)?;
                }
            }
            if let Some((dist_px, n)) = measured {
                env.set(SELF_DIST, dist_px * pixel);
                env.set(SELF_PIXEL, pixel);
                let fill_opacity = match &style.fill_opacity {
                    Some(o) => real(o, &env)?.clamp(0.0, 1.0),
                    None => 1.0,
                };
                out = self.fill(out, style, &env, dist_px, n, opacity * fill_opacity)?;
            }
            let Some(stroke_color) = &style.stroke else {
                continue;
            };
            let coverage = if style.stroke_is_line(&mark.form) {
                let c = self.line_coverage(layer, px, py);
                layer += 1;
                c
            } else if let Some((dist_px, n)) = measured {
                let half = frame::stroke_width(style, &env)? * self.dpr / 2.0;
                let d = dist_px.abs();
                box_coverage(half - d, n.0, n.1) - box_coverage(-half - d, n.0, n.1)
            } else {
                0.0
            };
            let top = to_blend(color(stroke_color, &env)?, style.blend);
            let under = to_blend(out, style.blend);
            let mixed = if style.blend == Blend::Add {
                std::array::from_fn(|i| under[i] + top[i] * coverage * opacity)
            } else {
                over(under, top, coverage * opacity)
            };
            out = from_blend(mixed, style.blend);
        }
        let rolled = |c: f64, k: f64| {
            let shown = srgb_lift(c);
            let clipped = shown.min(1.0);
            srgb_decode(clipped + (shown.tanh() - clipped) * k)
        };
        match &self.figure.styles.tone {
            Tone::Clip => {}
            Tone::Tanh => out = out.map(|c| rolled(c, 1.0)),
            Tone::Mix(k) => {
                let k = real(k, &self.env)?.clamp(0.0, 1.0);
                out = out.map(|c| rolled(c, k));
            }
        }
        Ok(out)
    }

    fn fill(
        &self,
        out: [f64; 3],
        style: &MarkStyle,
        env: &Env,
        dist_px: f64,
        n: (f64, f64),
        opacity: f64,
    ) -> Result<[f64; 3], Error> {
        // The facets partition the pixel, so they blend together. Layering one over
        // the other would let what's underneath show through along the edge.
        let negative = box_coverage(-dist_px, n.0, n.1);
        let mut covered = 0.0;
        let mut filled = [0.0; 3];
        // `::inside` and `::outside` only exist on a circle, and win over the sign they share.
        let round = style.inside.is_some() || style.outside.is_some();
        let forward = !round || real(&Expr::sym(SELF_CURVATURE), env)? >= 0.0;
        let which = usize::from(!forward);
        let [negative_gates, positive_gates] = style.sign_gates();
        let (negative_gate, positive_gate) = if round {
            (negative_gates[which], positive_gates[which])
        } else {
            (1.0, 1.0)
        };
        let disk = if forward { negative } else { 1.0 - negative };
        for (fill, coverage) in [
            (&style.negative, negative * negative_gate),
            (&style.positive, (1.0 - negative) * positive_gate),
            (&style.inside, disk),
            (&style.outside, 1.0 - disk),
        ] {
            if let Some(fill) = fill {
                let c = to_blend(color(fill, env)?, style.blend);
                covered += coverage * opacity;
                filled = std::array::from_fn(|i| filled[i] + c[i] * coverage * opacity);
            }
        }
        let under = to_blend(out, style.blend);
        let keep = if style.blend == Blend::Add {
            1.0
        } else {
            1.0 - covered
        };
        let mixed = std::array::from_fn(|i| under[i] * keep + filled[i]);
        Ok(from_blend(mixed, style.blend))
    }

    /// Renders the frame to RGBA8, sRGB-encoded. Labels aren't included.
    pub fn rgba(&self) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::with_capacity((self.width * self.height * 4) as usize);
        for py in 0..self.height {
            for px in 0..self.width {
                let c = self.shade(px as f64 + 0.5, py as f64 + 0.5)?;
                for channel in c {
                    bytes.push((srgb_encode(channel) * 255.0).round() as u8);
                }
                bytes.push(255);
            }
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;

    const CIRCLE: &str = "
        viewport { x: -2..2, y: -2..2 }
        let r = 1
        curve c = implicit (x, y): x^2 + y^2 = r^2";
    const STYLE: &str = "
        style c::inside  { fill: black }
        style c::outside { fill: white }";

    /// Both sides of one circle lit at once, warm inside and cool out, as the limaçon shader does:
    /// `col += glow_col * tanh(k / abs(d))`, then `tanh(col)`, all in display space.
    #[test]
    fn a_circle_can_light_both_its_sides() {
        let doc = "viewport { x: -2..2, y: -2..2 }
                   curve c = sdf (x, y): sqrt(x² + y²) - 1";
        let sheet = "style paper { fill: black; tone: tanh }
                     style c { blend: add; fill_opacity: tanh(self.pixel / abs(self.dist)) }
                     style c::inside { fill: rgb(1275, 255, 510) }
                     style c::outside { fill: rgb(510, 255, 1530) }";
        let figure = build(doc, &[sheet]).unwrap();
        let (w, h) = (401u32, 401u32);
        let frame = Frame::new(&figure, &crate::frame::Inputs::at(0.0), w, h).unwrap();
        let pixel = 4.0 / w as f64;
        for (px, py, warm) in [
            (200.5, 200.5 - 90.0, true), // Inside: the circle is about 100 px across.
            (200.5, 200.5 - 80.0, true),
            (200.5, 200.5 - 130.0, false), // Outside.
            (200.5, 200.5 - 180.0, false),
            (200.5 + 90.0, 200.5, true),
            (200.5 + 150.0, 200.5, false),
        ] {
            let (x, y): (f64, f64) = ((px - 200.5) * pixel, (200.5 - py) * pixel);
            let d = x.hypot(y) - 1.0;
            assert_eq!(d < 0.0, warm);
            let glow: [f64; 3] = if warm {
                [5.0, 1.0, 2.0]
            } else {
                [2.0, 1.0, 6.0]
            };
            let k = (pixel / d.abs()).tanh();
            let want = glow.map(|c| (c * k).tanh());
            let got = frame.shade(px, py).unwrap().map(srgb_encode);
            for i in 0..3 {
                assert!(
                    (got[i] - want[i]).abs() < 1e-3,
                    "({px}, {py})[{i}]: {got:?}, wanted {want:?}"
                );
            }
        }
    }

    /// A tone between clip and `tanh` mixes the two in display values, so a figure can turn into
    /// one that adds light, and its value can follow a beat.
    #[test]
    fn a_tone_can_be_between_clip_and_tanh() {
        let doc = "viewport { x: -2..2, y: -2..2 }
                   param k = 0 in 0..1
                   curve c = sdf (x, y): sqrt(x² + y²) - 1";
        let sheet = "style paper { fill: white; tone: k }
                     style c::inside { fill: white·0.5 }";
        let figure = build(doc, &[sheet]).unwrap();
        let at = |k: f64, px: f64| {
            let inputs = crate::frame::Inputs {
                time: 0.0,
                params: vec![("k".to_string(), k)],
            };
            let frame = Frame::new(&figure, &inputs, 101, 101).unwrap();
            frame.shade(px, 50.5).unwrap().map(srgb_encode)[0]
        };
        // Whatever clipping gives, a tone of k moves it k of the way to its tanh, in display
        // values: on the paper and on the disk, which are different values.
        for px in [1.5, 50.5] {
            let clipped = at(0.0, px);
            for k in [0.25, 0.5, 1.0] {
                let want = clipped + (clipped.tanh() - clipped) * k;
                assert!(
                    (at(k, px) - want).abs() < 1e-9,
                    "{px} {k}: {} {want}",
                    at(k, px)
                );
            }
            // Past either end is the end.
            assert!((at(2.0, px) - clipped.tanh()).abs() < 1e-9);
        }
        assert!((at(0.0, 1.5) - at(0.0, 50.5)).abs() > 0.1);
        let err = build(doc, &["style paper { tone: white }"]).unwrap_err();
        assert!(err.message.contains("a number from 0 to 1"), "{err}");
    }

    #[test]
    fn colors_can_be_scaled_and_summed() {
        let env = Env::new(0.0);
        let parse =
            |src: &str| match &crate::parse::parse(&format!("style paper {{ fill: {src} }}"))
                .unwrap()
                .items[0]
            {
                crate::parse::Item::Style(rule) => rule.decls[0].1.clone(),
                other => panic!("{other:?}"),
            };
        assert_eq!(color(&parse("white * 0.5"), &env).unwrap(), [0.5; 3]);
        assert_eq!(color(&parse("0.5 * white"), &env).unwrap(), [0.5; 3]);
        assert_eq!(color(&parse("white / 4"), &env).unwrap(), [0.25; 3]);
        assert_eq!(color(&parse("white + white"), &env).unwrap(), [2.0; 3]);
        assert_eq!(color(&parse("white - white"), &env).unwrap(), [0.0; 3]);
    }

    #[test]
    fn a_line_of_samples_has_no_sides() {
        let doc = "viewport { x: -2..2, y: -2..2 }
                   curve c = parametric (t): (t, t) for t in 0..1";
        let err = build(doc, &["style c::inside { fill: black }"]).unwrap_err();
        assert!(err.message.contains("has no sides"), "{err}");
        let err = build(doc, &["style paper { tone: loud }"]).unwrap_err();
        assert!(err.message.contains("`tone` is"), "{err}");
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-4
    }

    fn frame(f: &Figure, size: u32) -> Frame<'_> {
        Frame::new(f, &Inputs::at(0.0), size, size).unwrap()
    }

    #[test]
    fn oklch_matches_known_srgb_values() {
        assert!(oklch(1.0, 0.0, 0.0).iter().all(|c| near(*c, 1.0)));
        assert!(oklch(0.0, 0.0, 0.0).iter().all(|c| near(*c, 0.0)));
        // sRGB red is oklch(0.62796 0.25768 29.234).
        let red = oklch(0.62796, 0.25768, 29.234);
        assert!(
            near(red[0], 1.0) && near(red[1], 0.0) && near(red[2], 0.0),
            "{red:?}"
        );
    }

    #[test]
    fn inside_is_the_negative_side() {
        let f = build(CIRCLE, &[STYLE]).unwrap();
        let frame = frame(&f, 100);
        assert_eq!(frame.shade(50.0, 50.0).unwrap(), [0.0; 3]);
        assert_eq!(frame.shade(1.0, 1.0).unwrap(), [1.0; 3]);
    }

    /// Writing the equation the other way round flips its orientation.
    #[test]
    fn swapping_the_equation_flips_inside() {
        let doc = "curve c = implicit (x, y): 1 = x^2 + y^2";
        let f = build(doc, &["style c::inside { fill: black }"]).unwrap();
        let frame = frame(&f, 100);
        assert_eq!(frame.shade(50.0, 50.0).unwrap(), [1.0; 3]);
        assert_eq!(frame.shade(1.0, 1.0).unwrap(), [0.0; 3]);
    }

    #[test]
    fn y_points_up() {
        let doc = "curve c = implicit (x, y): y = 0";
        let f = build(doc, &["style c::positive { fill: black }"]).unwrap();
        let frame = frame(&f, 100);
        assert_eq!(frame.shade(50.0, 10.0).unwrap(), [0.0; 3]);
        assert_eq!(frame.shade(50.0, 90.0).unwrap(), [1.0; 3]);
    }

    #[test]
    fn the_edge_is_antialiased_over_one_pixel() {
        let f = build(CIRCLE, &[STYLE]).unwrap();
        let frame = frame(&f, 400);
        // World x = 1 is pixel 300. On the edge, coverage is one half, and by default
        // that half is taken in gamma-encoded values, as SVG does.
        let [v, ..] = frame.shade(300.0, 200.0).unwrap();
        assert!(near(srgb_encode(v), 0.5), "{v}");
        assert_eq!(frame.shade(302.0, 200.0).unwrap(), [1.0; 3]);
        assert_eq!(frame.shade(298.0, 200.0).unwrap(), [0.0; 3]);
    }

    #[test]
    fn linear_blend_is_available() {
        let sheet = format!("{STYLE}\nstyle c {{ blend: linear }}");
        let f = build(CIRCLE, &[&sheet]).unwrap();
        let [v, ..] = frame(&f, 400).shade(300.0, 200.0).unwrap();
        assert!(near(v, 0.5), "{v}");
    }

    #[test]
    fn line_strokes_match_their_rasterized_coverage() {
        let sheet = "style c { stroke: black; width: 4px }";
        let f = build(CIRCLE, &[sheet]).unwrap();
        let frame = frame(&f, 400);
        assert_eq!(frame.shade(300.5, 200.5).unwrap(), [0.0; 3]);
        assert_eq!(frame.shade(305.5, 200.5).unwrap(), [1.0; 3]);
    }

    /// Point purity: seeking in any order gives identical frames.
    #[test]
    fn seeking_is_order_independent() {
        let doc = "
            let r = 1 + 0.25 * sin(time)
            curve c = implicit (x, y): x^2 + y^2 = r^2";
        let sheet = "style c::inside { fill: oklch(0.7, 0.1, 40 * time) }";
        let f = build(doc, &[sheet]).unwrap();
        let at = |t: f64| {
            Frame::new(&f, &Inputs::at(t), 32, 32)
                .unwrap()
                .rgba()
                .unwrap()
        };
        let forward: Vec<_> = [0.0, 1.0, 2.5].map(at).into();
        let shuffled: Vec<_> = [2.5, 0.0, 1.0].map(at).into();
        assert_eq!(forward[0], shuffled[1]);
        assert_eq!(forward[1], shuffled[2]);
        assert_eq!(forward[2], shuffled[0]);
        assert_ne!(forward[0], forward[1]);
    }

    /// A slider and the clock drive the same param, and the slider wins.
    #[test]
    fn an_input_overrides_a_params_time_binding() {
        let doc = "param a in 0..2
                   bind a = time
                   curve c = parametric (t): (t, a) for t in -2..2";
        let f = build(doc, &["style c { stroke: black; width: 4px }"]).unwrap();
        let by_time = Frame::new(&f, &Inputs::at(1.0), 100, 100).unwrap();
        let by_input = Frame::new(
            &f,
            &Inputs {
                time: 0.0,
                params: vec![("a".to_string(), 1.0)],
            },
            100,
            100,
        )
        .unwrap();
        assert_eq!(by_time.rgba().unwrap(), by_input.rgba().unwrap());
        // World y = 1 is row 25.
        assert_eq!(by_time.shade(50.5, 25.5).unwrap(), [0.0; 3]);
    }

    #[test]
    fn functions_for_time_curves_evaluate() {
        let mut env = Env::new(0.0);
        env.set("t", 0.5);
        let parse = |src: &str| {
            let doc = crate::parse::parse(&format!("let v = {src}")).unwrap();
            match &doc.items[0] {
                crate::parse::Item::Let { expr, .. } => expr.clone(),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(real(&parse("mod(7, 3)"), &env).unwrap(), 1.0);
        assert_eq!(real(&parse("mod(-1, 3)"), &env).unwrap(), 2.0);
        assert_eq!(real(&parse("clamp(5, 0, 1)"), &env).unwrap(), 1.0);
        assert_eq!(real(&parse("smoothstep(0, 1, t)"), &env).unwrap(), 0.5);
        assert_eq!(real(&parse("smoothstep(0, 1, 2)"), &env).unwrap(), 1.0);
    }

    #[test]
    fn regions_fill_by_winding_with_a_true_distance() {
        let doc = "viewport { x: 0..4, y: 0..4 }
                   region r = (1, 1), (3, 1), (3, 3), (1, 3)";
        let f = build(doc, &["style r { fill: black }"]).unwrap();
        let frame = frame(&f, 400);
        assert_eq!(frame.shade(200.5, 200.5).unwrap(), [0.0; 3]);
        assert_eq!(frame.shade(50.5, 50.5).unwrap(), [1.0; 3]);
        // World x = 1 is pixel 100. On the edge, half the pixel is covered.
        let [v, ..] = frame.shade(100.0, 200.5).unwrap();
        assert!(near(srgb_encode(v), 0.5), "{v}");
    }

    #[test]
    fn region_arcs_follow_their_parameter() {
        let doc = "region r = (0, 0), (cos(t), sin(t)) for t in 0..tau";
        let f = build(doc, &["style r { fill: black }"]).unwrap();
        let frame = frame(&f, 200);
        assert_eq!(frame.shade(100.5, 60.5).unwrap(), [0.0; 3]);
        assert_eq!(frame.shade(100.5, 40.5).unwrap(), [1.0; 3]);
    }

    #[test]
    fn dots_are_exact_circles_in_pixels() {
        let doc = "let P = (0, 0)\ndot P";
        let f = build(doc, &["style P { fill: black; radius: 5px }"]).unwrap();
        let frame = frame(&f, 100);
        assert_eq!(frame.shade(50.5, 50.5).unwrap(), [0.0; 3]);
        assert_eq!(frame.shade(50.5, 58.5).unwrap(), [1.0; 3]);
    }

    #[test]
    fn opacity_scales_a_marks_coverage() {
        let doc = "region r = (-2, -2), (2, -2), (2, 2), (-2, 2)";
        let f = build(doc, &["style r { fill: black; opacity: 0.25 }"]).unwrap();
        let [v, ..] = frame(&f, 100).shade(50.5, 50.5).unwrap();
        assert!(near(srgb_encode(v), 0.75), "{v}");
    }

    #[test]
    fn paper_can_be_styled() {
        let doc = "let P = (0, 0)";
        let f = build(doc, &["style paper { fill: rgb(255, 0, 0) }"]).unwrap();
        assert_eq!(frame(&f, 10).shade(5.0, 5.0).unwrap(), [1.0, 0.0, 0.0]);
    }
}
