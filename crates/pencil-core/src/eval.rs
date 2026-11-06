//! The CPU oracle: naive and double precision, and meant to stay obviously correct.
//! Performance work belongs in the GPU path. `wgsl.rs` must implement the same
//! shading as `Frame::shade`, and `pencil check` compares the two.

use std::collections::HashMap;

use crate::Figure;
use crate::error::Error;
use crate::frame::{self, Inputs, Prepared};
use crate::ir::{Expr, Form};
use crate::stroke::{self, box_coverage};
use crate::style::{Blend, MarkStyle, SELF_DIST};

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
        self.vars.insert(name.to_string(), Some(value));
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
            ("Add", [a, b]) => Real(real(a)? + real(b)?),
            ("Subtract", [a, b]) => Real(real(a)? - real(b)?),
            ("Multiply", [a, b]) => Real(real(a)? * real(b)?),
            ("Divide", [a, b]) => Real(real(a)? / real(b)?),
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

/// The values of a frame's bindings and signals. An input overrides a param.
pub fn frame_env(figure: &Figure, inputs: &Inputs) -> Result<Env, Error> {
    let mut env = Env::new(inputs.time);
    for binding in &figure.scene.bindings {
        let value = match inputs.params.iter().find(|(id, _)| *id == binding.id) {
            Some((_, value)) => Ok(*value),
            None => real(&binding.expr, &env),
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
    }
}

fn from_blend(c: [f64; 3], blend: Blend) -> [f64; 3] {
    match blend {
        Blend::Linear => c,
        Blend::Gamma => c.map(srgb_decode),
    }
}

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
                    vars, field, grad, ..
                } => {
                    env.set(&vars[0], x);
                    env.set(&vars[1], y);
                    let f = real(field, &env)?;
                    let (gx, gy) = (real(&grad[0], &env)?, real(&grad[1], &env)?);
                    let glen = gx.hypot(gy);
                    let dist = f / glen.max(MIN_GRADIENT);
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
                Form::Label { .. } => continue,
                Form::Parametric(_) | Form::Polyline { .. } => None,
            };
            let opacity = match &style.opacity {
                Some(o) => real(o, &env)?.clamp(0.0, 1.0),
                None => 1.0,
            };
            if let Some((dist_px, n)) = measured {
                env.set(SELF_DIST, dist_px * pixel);
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
            out = from_blend(over(under, top, coverage * opacity), style.blend);
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
        for (fill, coverage) in [
            (&style.negative, negative),
            (&style.positive, 1.0 - negative),
        ] {
            if let Some(fill) = fill {
                let c = to_blend(color(fill, env)?, style.blend);
                covered += coverage * opacity;
                filled = std::array::from_fn(|i| filled[i] + c[i] * coverage * opacity);
            }
        }
        let under = to_blend(out, style.blend);
        let mixed = std::array::from_fn(|i| under[i] * (1.0 - covered) + filled[i]);
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
