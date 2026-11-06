//! Lowers a parsed document to the scene IR. Functions are inlined, and point algebra
//! and constructions are expanded here, so downstream only sees scalar expressions,
//! still symbolic.

use std::collections::{HashMap, HashSet};

use crate::diff::derivative;
use crate::error::Error;
use crate::eval::{self, Env};
use crate::ir::{Arc, Binding, Expr, Form, Handle, Mark, Param, Part, Scene, Viewport};
use crate::parse::{CurveSyntax, Document, Item, PartSyntax};

/// Names every expression can use without declaring them.
pub const SIGNALS: [&str; 1] = ["time"];
pub const CONSTANTS: [&str; 3] = ["pi", "π", "tau"];
/// The discrete param that beats declare.
pub const BEAT_PARAM: &str = "beat";

/// Builtin heads that take and return scalars.
const SCALAR_HEADS: [&str; 18] = [
    "Mix",
    "Sqrt",
    "Sin",
    "Cos",
    "Tan",
    "Arccos",
    "Exp",
    "Ln",
    "Abs",
    "Sign",
    "Floor",
    "Arctan2",
    "Min",
    "Max",
    "Clamp",
    "Mod",
    "Smoothstep",
    "Power",
];
const MAX_INLINE_DEPTH: usize = 32;
/// How far a line or ray runs, in world units. A constant, so its dashes hold still.
const LINE_REACH: f64 = 100.0;
/// Below this squared distance, in world units, two points are the same point. It's above
/// the noise where a construction brings two points together: at a tangency they're about
/// 1e-8 apart, in a direction that means nothing.
const COINCIDENT: f64 = 1e-12;
/// How far short of touching still counts, as a fraction of the terms that cancel. A
/// tangent by construction must not flicker with rounding.
const TANGENT: f64 = 1e-12;
/// Below this squared sine, two lines are parallel.
const PARALLEL: f64 = 1e-24;
/// How far behind a ray's start still counts, in lengths of its direction. It lets a ray
/// begin exactly on what it meets.
const RAY_START: f64 = 1e-9;

/// What an expression evaluates to, in symbolic scalars.
#[derive(Clone, Debug)]
enum Val {
    Scalar(Expr),
    Point(Expr, Expr),
    Circle {
        x: Expr,
        y: Expr,
        r: Expr,
    },
    Line {
        a: [Expr; 2],
        b: [Expr; 2],
        ray: bool,
    },
}

impl Val {
    fn kind(&self) -> &'static str {
        match self {
            Val::Scalar(_) => "a number",
            Val::Point(..) => "a point",
            Val::Circle { .. } => "a circle",
            Val::Line { .. } => "a line",
        }
    }

    fn fields(&self) -> Vec<Expr> {
        match self {
            Val::Scalar(e) => vec![e.clone()],
            Val::Point(x, y) => vec![x.clone(), y.clone()],
            Val::Circle { x, y, r } => vec![x.clone(), y.clone(), r.clone()],
            Val::Line { a, b, .. } => vec![a[0].clone(), a[1].clone(), b[0].clone(), b[1].clone()],
        }
    }

    /// The same kind of value with each field replaced, in `fields` order.
    fn with_fields(&self, fields: Vec<Expr>) -> Val {
        let mut f = fields.into_iter();
        let mut next = || f.next().expect("one field for each of `fields`");
        match self {
            Val::Scalar(_) => Val::Scalar(next()),
            Val::Point(..) => Val::Point(next(), next()),
            Val::Circle { .. } => Val::Circle {
                x: next(),
                y: next(),
                r: next(),
            },
            Val::Line { ray, .. } => Val::Line {
                a: [next(), next()],
                b: [next(), next()],
                ray: *ray,
            },
        }
    }

    /// This value, existing only where `condition` is nonnegative.
    fn only_where(&self, condition: &Expr) -> Val {
        self.with_fields(
            self.fields()
                .into_iter()
                .map(|field| Expr::app("Where", vec![condition.clone(), field]))
                .collect(),
        )
    }

    fn same_kind(&self, other: &Val) -> bool {
        match (self, other) {
            (Val::Line { ray: a, .. }, Val::Line { ray: b, .. }) => a == b,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

struct Function {
    params: Vec<String>,
    body: Expr,
}

/// The type of a named binding, whose fields are flattened to scalars.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Point,
    Circle,
    Line,
    Ray,
}

#[derive(Default)]
struct Lowerer {
    functions: HashMap<String, Function>,
    shapes: HashMap<String, Shape>,
    /// Scalar names known at this point in the document.
    scalars: HashSet<String>,
}

// Small builders, so the construction formulas below read like formulas.
fn num(n: f64) -> Expr {
    Expr::Num(n)
}
fn add(a: Expr, b: Expr) -> Expr {
    Expr::app("Add", vec![a, b])
}
fn sub(a: Expr, b: Expr) -> Expr {
    Expr::app("Subtract", vec![a, b])
}
fn mul(a: Expr, b: Expr) -> Expr {
    Expr::app("Multiply", vec![a, b])
}
fn div(a: Expr, b: Expr) -> Expr {
    Expr::app("Divide", vec![a, b])
}
fn sqrt(a: Expr) -> Expr {
    Expr::app("Sqrt", vec![a])
}
fn square(a: Expr) -> Expr {
    Expr::app("Power", vec![a, num(2.0)])
}
fn dot2(a: &[Expr; 2], b: &[Expr; 2]) -> Expr {
    add(
        mul(a[0].clone(), b[0].clone()),
        mul(a[1].clone(), b[1].clone()),
    )
}
fn minus(a: &[Expr; 2], b: &[Expr; 2]) -> [Expr; 2] {
    [
        sub(a[0].clone(), b[0].clone()),
        sub(a[1].clone(), b[1].clone()),
    ]
}
fn plus_scaled(a: &[Expr; 2], d: &[Expr; 2], t: &Expr) -> [Expr; 2] {
    [
        add(a[0].clone(), mul(d[0].clone(), t.clone())),
        add(a[1].clone(), mul(d[1].clone(), t.clone())),
    ]
}
fn length2(d: &[Expr; 2]) -> Expr {
    sqrt(add(square(d[0].clone()), square(d[1].clone())))
}
fn point(p: [Expr; 2]) -> Val {
    let [x, y] = p;
    Val::Point(x, y)
}
fn min(a: Expr, b: Expr) -> Expr {
    Expr::app("Min", vec![a, b])
}
fn ahead(t: &Expr) -> Expr {
    add(t.clone(), num(RAY_START))
}

/// Reflects a point in the line through `a` and `b`.
fn reflect_in_line(p: &[Expr; 2], a: &[Expr; 2], b: &[Expr; 2]) -> [Expr; 2] {
    let d = minus(b, a);
    let t = div(dot2(&minus(p, a), &d), dot2(&d, &d));
    let foot = plus_scaled(a, &d, &t);
    [
        sub(mul(num(2.0), foot[0].clone()), p[0].clone()),
        sub(mul(num(2.0), foot[1].clone()), p[1].clone()),
    ]
}

/// Where a line or ray meets a circle. `which` names a root, in order along the line's
/// direction, not a place among the points that remain: a ray's start passing into the
/// circle takes away root 0, and root 1 stays where it is, so neither jumps.
fn meet_line_circle(
    a: &[Expr; 2],
    b: &[Expr; 2],
    ray: bool,
    c: &[Expr; 2],
    r: &Expr,
    which: usize,
) -> Val {
    let d = minus(b, a);
    let f = minus(a, c);
    let aa = dot2(&d, &d);
    let bb = mul(num(2.0), dot2(&f, &d));
    let cc = sub(dot2(&f, &f), square(r.clone()));
    let disc = sub(square(bb.clone()), mul(mul(num(4.0), aa.clone()), cc));
    // Tangent means the terms of `cc` cancel, so rounding is relative to them, not to `cc`.
    let cancelling = mul(
        mul(num(4.0), aa.clone()),
        add(dot2(&f, &f), square(r.clone())),
    );
    let meets = add(
        disc.clone(),
        mul(num(TANGENT), add(square(bb.clone()), cancelling)),
    );
    let root = sqrt(Expr::app("Max", vec![num(0.0), disc]));
    let minus_b = Expr::app("Negate", vec![bb]);
    let t = if which == 0 {
        div(sub(minus_b, root), mul(num(2.0), aa))
    } else {
        div(add(minus_b, root), mul(num(2.0), aa))
    };
    let exists = if ray { min(meets, ahead(&t)) } else { meets };
    point(plus_scaled(a, &d, &t)).only_where(&exists)
}

/// Where two lines or rays meet. Parallel lines don't, and a ray only meets what's ahead
/// of its start.
fn meet_lines(
    (a, b, first_ray): (&[Expr; 2], &[Expr; 2], bool),
    (p, q, second_ray): (&[Expr; 2], &[Expr; 2], bool),
) -> Val {
    let d = minus(b, a);
    let e = minus(q, p);
    let w = minus(p, a);
    let cross = |u: &[Expr; 2], v: &[Expr; 2]| {
        sub(
            mul(u[0].clone(), v[1].clone()),
            mul(u[1].clone(), v[0].clone()),
        )
    };
    let den = cross(&d, &e);
    let t = div(cross(&w, &e), den.clone());
    let u = div(cross(&w, &d), den.clone());
    let mut exists = sub(
        square(den),
        mul(num(PARALLEL), mul(dot2(&d, &d), dot2(&e, &e))),
    );
    if first_ray {
        exists = min(exists, ahead(&t));
    }
    if second_ray {
        exists = min(exists, ahead(&u));
    }
    point(plus_scaled(a, &d, &t)).only_where(&exists)
}

/// Where two circles meet: first the point left of the line between their centers. They
/// don't if they're apart, if one is inside the other, or if they're the same circle.
fn meet_circles(c1: &[Expr; 2], r1: &Expr, c2: &[Expr; 2], r2: &Expr, which: usize) -> Val {
    let d = minus(c2, c1);
    let big = length2(&d);
    let a = div(
        add(
            sub(square(r1.clone()), square(r2.clone())),
            square(big.clone()),
        ),
        mul(num(2.0), big.clone()),
    );
    let h2 = sub(square(r1.clone()), square(a.clone()));
    let exists = min(
        sub(dot2(&d, &d), num(COINCIDENT)),
        add(h2.clone(), mul(num(TANGENT), square(r1.clone()))),
    );
    let h = sqrt(Expr::app("Max", vec![num(0.0), h2]));
    let base = plus_scaled(c1, &d, &div(a, big.clone()));
    let n = [Expr::app("Negate", vec![d[1].clone()]), d[0].clone()];
    let along = div(h, big);
    let along = if which == 0 {
        along
    } else {
        Expr::app("Negate", vec![along])
    };
    point(plus_scaled(&base, &n, &along)).only_where(&exists)
}

type Locals = HashMap<String, Val>;

impl Lowerer {
    fn named(&self, name: &str) -> Option<Val> {
        let field = |f: &str| Expr::sym(format!("{name}.{f}"));
        match self.shapes.get(name)? {
            Shape::Point => Some(Val::Point(field("x"), field("y"))),
            Shape::Circle => Some(Val::Circle {
                x: field("x"),
                y: field("y"),
                r: field("r"),
            }),
            shape @ (Shape::Line | Shape::Ray) => Some(Val::Line {
                a: [field("ax"), field("ay")],
                b: [field("bx"), field("by")],
                ray: *shape == Shape::Ray,
            }),
        }
    }

    fn lower(&self, e: &Expr, locals: &Locals, depth: usize) -> Result<Val, Error> {
        if depth > MAX_INLINE_DEPTH {
            return Err(Error::new("functions nest too deeply, or recurse"));
        }
        Ok(match e {
            Expr::Num(_) => Val::Scalar(e.clone()),
            Expr::Sym(s) => {
                if let Some(v) = locals.get(s) {
                    v.clone()
                } else if let Some(v) = self.named(s) {
                    v
                } else if self.scalars.contains(s)
                    || SIGNALS.contains(&s.as_str())
                    || CONSTANTS.contains(&s.as_str())
                {
                    Val::Scalar(e.clone())
                } else if let Some((base, axis)) = s.split_once('.') {
                    match (locals.get(base), axis) {
                        (Some(Val::Point(x, _)), "x") | (Some(Val::Circle { x, .. }), "x") => {
                            Val::Scalar(x.clone())
                        }
                        (Some(Val::Point(_, y)), "y") | (Some(Val::Circle { y, .. }), "y") => {
                            Val::Scalar(y.clone())
                        }
                        (Some(Val::Circle { r, .. }), "r") => Val::Scalar(r.clone()),
                        _ => return Err(Error::new(format!("unknown name `{s}`"))),
                    }
                } else {
                    return Err(Error::new(format!("unknown name `{s}`")));
                }
            }
            Expr::App(head, args) => {
                // `intersect` takes a literal index, which picks a formula.
                if head == "Intersect" {
                    return self.intersect(args, locals, depth);
                }
                let mut low = Vec::with_capacity(args.len());
                for a in args {
                    low.push(self.lower(a, locals, depth + 1)?);
                }
                if let Some(function) = self.functions.get(head) {
                    if function.params.len() != args.len() {
                        return Err(Error::new(format!(
                            "`{head}` takes {} arguments, found {}",
                            function.params.len(),
                            args.len()
                        )));
                    }
                    let inner: Locals = function.params.iter().cloned().zip(low).collect();
                    return self.lower(&function.body, &inner, depth + 1);
                }
                self.builtin(head, low)?
            }
        })
    }

    fn intersect(&self, args: &[Expr], locals: &Locals, depth: usize) -> Result<Val, Error> {
        let which = match args.get(2) {
            None => 0,
            Some(Expr::Num(n)) if *n == 0.0 => 0,
            Some(Expr::Num(n)) if *n == 1.0 => 1,
            Some(_) => return Err(Error::new("`intersect` picks point 0 or 1")),
        };
        let (Some(u), Some(v)) = (args.first(), args.get(1)) else {
            return Err(Error::new("`intersect` takes two shapes"));
        };
        let u = self.lower(u, locals, depth + 1)?;
        let v = self.lower(v, locals, depth + 1)?;
        use Val::{Circle, Line};
        Ok(match (&u, &v) {
            (Circle { x, y, r }, Line { a, b, ray }) | (Line { a, b, ray }, Circle { x, y, r }) => {
                meet_line_circle(a, b, *ray, &[x.clone(), y.clone()], r, which)
            }
            (
                Line { a, b, ray },
                Line {
                    a: p,
                    b: q,
                    ray: second_ray,
                },
            ) => meet_lines((a, b, *ray), (p, q, *second_ray)),
            (
                Circle { x, y, r },
                Circle {
                    x: x2,
                    y: y2,
                    r: r2,
                },
            ) => meet_circles(
                &[x.clone(), y.clone()],
                r,
                &[x2.clone(), y2.clone()],
                r2,
                which,
            ),
            _ => {
                return Err(Error::new(format!(
                    "`intersect` can't take {} and {}",
                    u.kind(),
                    v.kind()
                )));
            }
        })
    }

    fn builtin(&self, head: &str, low: Vec<Val>) -> Result<Val, Error> {
        use Val::{Circle, Line, Point, Scalar};
        let reflected_circle = |x: &Expr, y: &Expr, r: &Expr, p: [Expr; 2]| {
            let _ = (x, y);
            let [cx, cy] = p;
            Circle {
                x: cx,
                y: cy,
                r: r.clone(),
            }
        };
        Ok(match (head, low.as_slice()) {
            ("Exists", [v]) => Scalar(Expr::app("Exists", v.fields())),
            ("Otherwise", [a, b]) => {
                if !a.same_kind(b) {
                    return Err(Error::new(format!(
                        "`otherwise` needs two of one kind, found {} and {}",
                        a.kind(),
                        b.kind()
                    )));
                }
                // All of `a` or all of `b`, never a mix of their fields.
                let has_a = sub(Expr::app("Exists", a.fields()), num(0.5));
                a.with_fields(
                    a.fields()
                        .into_iter()
                        .zip(b.fields())
                        .map(|(x, y)| Expr::app("IfNonnegative", vec![has_a.clone(), x, y]))
                        .collect(),
                )
            }
            ("Tuple", [Scalar(x), Scalar(y)]) => Point(x.clone(), y.clone()),
            ("Add" | "Subtract", [Point(ax, ay), Point(bx, by)]) => Point(
                Expr::app(head, vec![ax.clone(), bx.clone()]),
                Expr::app(head, vec![ay.clone(), by.clone()]),
            ),
            ("Add" | "Subtract" | "Multiply" | "Divide", [Scalar(a), Scalar(b)]) => {
                Scalar(Expr::app(head, vec![a.clone(), b.clone()]))
            }
            ("Multiply", [Scalar(k), Point(x, y)]) | ("Multiply", [Point(x, y), Scalar(k)]) => {
                Point(mul(k.clone(), x.clone()), mul(k.clone(), y.clone()))
            }
            ("Divide", [Point(x, y), Scalar(k)]) => {
                Point(div(x.clone(), k.clone()), div(y.clone(), k.clone()))
            }
            ("Negate", [Scalar(a)]) => Scalar(Expr::app("Negate", vec![a.clone()])),
            ("Negate", [Point(x, y)]) => Point(
                Expr::app("Negate", vec![x.clone()]),
                Expr::app("Negate", vec![y.clone()]),
            ),
            ("Length", [Point(x, y)]) => Scalar(length2(&[x.clone(), y.clone()])),
            ("Distance", [Point(ax, ay), Point(bx, by)]) => Scalar(length2(&[
                sub(bx.clone(), ax.clone()),
                sub(by.clone(), ay.clone()),
            ])),
            ("Dot", [Point(ax, ay), Point(bx, by)]) => {
                Scalar(dot2(&[ax.clone(), ay.clone()], &[bx.clone(), by.clone()]))
            }
            ("Midpoint", [Point(ax, ay), Point(bx, by)]) => Point(
                div(add(ax.clone(), bx.clone()), num(2.0)),
                div(add(ay.clone(), by.clone()), num(2.0)),
            ),
            ("Circle", [Point(x, y), Point(tx, ty)]) => Circle {
                x: x.clone(),
                y: y.clone(),
                r: length2(&[sub(tx.clone(), x.clone()), sub(ty.clone(), y.clone())]),
            },
            // Two points only make a line if they're two points.
            ("Line" | "Ray", [Point(ax, ay), Point(bx, by)]) => {
                let a = [ax.clone(), ay.clone()];
                let b = [bx.clone(), by.clone()];
                let d = minus(&b, &a);
                let line = Line {
                    a,
                    b,
                    ray: head == "Ray",
                };
                line.only_where(&sub(dot2(&d, &d), num(COINCIDENT)))
            }
            ("Perpendicular", [Point(x, y), Line { a, b, .. }]) => {
                let d = minus(b, a);
                Line {
                    a: [x.clone(), y.clone()],
                    b: [sub(x.clone(), d[1].clone()), add(y.clone(), d[0].clone())],
                    ray: false,
                }
            }
            ("Mirror", [Point(x, y), Line { a, b, .. }]) => {
                point(reflect_in_line(&[x.clone(), y.clone()], a, b))
            }
            ("Mirror", [Point(x, y), Point(mx, my)]) => Point(
                sub(mul(num(2.0), mx.clone()), x.clone()),
                sub(mul(num(2.0), my.clone()), y.clone()),
            ),
            ("Mirror", [Circle { x, y, r }, Line { a, b, .. }]) => {
                reflected_circle(x, y, r, reflect_in_line(&[x.clone(), y.clone()], a, b))
            }
            (head, _) if SCALAR_HEADS.contains(&head) => {
                let mut scalars = Vec::with_capacity(low.len());
                for v in low {
                    match v {
                        Scalar(s) => scalars.push(s),
                        other => {
                            return Err(Error::new(format!(
                                "{head} needs a number, found {}",
                                other.kind()
                            )));
                        }
                    }
                }
                Scalar(Expr::app(head, scalars))
            }
            (head, _) if head.chars().next().is_some_and(char::is_uppercase) => {
                let kinds: Vec<&str> = low.iter().map(Val::kind).collect();
                return Err(Error::new(format!(
                    "`{head}` can't take {}",
                    kinds.join(", ")
                )));
            }
            (head, _) => {
                return Err(Error::new(format!("unknown function `{head}`")));
            }
        })
    }

    fn scalar(&self, e: &Expr, locals: &Locals) -> Result<Expr, Error> {
        match self.lower(e, locals, 0)? {
            Val::Scalar(s) => Ok(s),
            other => Err(Error::new(format!(
                "expected a number, found {}",
                other.kind()
            ))),
        }
    }

    fn point(&self, e: &Expr, locals: &Locals) -> Result<[Expr; 2], Error> {
        match self.lower(e, locals, 0)? {
            Val::Point(x, y) => Ok([x, y]),
            other => Err(Error::new(format!(
                "expected a point, found {}",
                other.kind()
            ))),
        }
    }

    fn arc(&self, var: &str, point: &Expr, range: &[Expr; 2]) -> Result<Arc, Error> {
        let mut locals = Locals::new();
        locals.insert(var.to_string(), Val::Scalar(Expr::sym(var)));
        Ok(Arc {
            var: var.to_string(),
            point: self.point(point, &locals)?,
            range: [
                self.scalar(&range[0], &Locals::new())?,
                self.scalar(&range[1], &Locals::new())?,
            ],
        })
    }

    /// Binds a name's fields as scalars, so each is computed once per frame.
    fn bind(&mut self, scene: &mut Scene, name: &str, val: Val) {
        let mut push = |field: Option<&str>, expr: Expr, scalars: &mut HashSet<String>| {
            let id = match field {
                Some(f) => format!("{name}.{f}"),
                None => name.to_string(),
            };
            scalars.insert(id.clone());
            scene.bindings.push(Binding { id, expr });
        };
        let shape = match val {
            Val::Scalar(e) => {
                push(None, e, &mut self.scalars);
                None
            }
            Val::Point(x, y) => {
                push(Some("x"), x, &mut self.scalars);
                push(Some("y"), y, &mut self.scalars);
                Some(Shape::Point)
            }
            Val::Circle { x, y, r } => {
                push(Some("x"), x, &mut self.scalars);
                push(Some("y"), y, &mut self.scalars);
                push(Some("r"), r, &mut self.scalars);
                Some(Shape::Circle)
            }
            Val::Line { a, b, ray } => {
                let [ax, ay] = a;
                let [bx, by] = b;
                push(Some("ax"), ax, &mut self.scalars);
                push(Some("ay"), ay, &mut self.scalars);
                push(Some("bx"), bx, &mut self.scalars);
                push(Some("by"), by, &mut self.scalars);
                Some(if ray { Shape::Ray } else { Shape::Line })
            }
        };
        if let Some(shape) = shape {
            self.shapes.insert(name.to_string(), shape);
        }
    }
}

fn constant(e: &Expr, what: &str) -> Result<f64, Error> {
    eval::real(e, &Env::new(0.0))
        .map_err(|err| Error::new(format!("{what} must be constant: {err}")))
}

/// Attaches a line number to an error that lacks one.
fn at_line(line: usize) -> impl Fn(Error) -> Error {
    move |mut e| {
        if e.line == 0 {
            e.line = line;
            e.col = 1;
        }
        e
    }
}

fn is_partial(e: &Expr) -> bool {
    match e {
        Expr::App(head, args) => head == "Where" || head == "Exists" || args.iter().any(is_partial),
        _ => false,
    }
}

fn field_form(
    lowerer: &Lowerer,
    vars: &[String; 2],
    field: &Expr,
    exact: bool,
) -> Result<Form, Error> {
    let mut locals = Locals::new();
    for v in vars {
        locals.insert(v.clone(), Val::Scalar(Expr::sym(v)));
    }
    let field = lowerer.scalar(field, &locals)?;
    // A field is evaluated per pixel, but whether a value exists is decided once per frame.
    if is_partial(&field) {
        return Err(Error::new(
            "a curve's field can't use `intersect`, `line`, `ray`, `exists` or `otherwise` directly. Name the value with `let` first",
        ));
    }
    // Bindings can't use coordinates, so they're constant here and the derivative
    // treats them as symbols, which keeps the gradient small.
    let grad = [derivative(&field, &vars[0])?, derivative(&field, &vars[1])?];
    Ok(Form::Field {
        vars: vars.clone(),
        field,
        grad,
        exact,
        line: None,
    })
}

/// A circle's exact distance field, in the coordinates `x` and `y`.
fn circle_form(name: &str) -> Result<Form, Error> {
    let vars = ["x".to_string(), "y".to_string()];
    let field = sub(
        sqrt(add(
            square(sub(Expr::sym("x"), Expr::sym(format!("{name}.x")))),
            square(sub(Expr::sym("y"), Expr::sym(format!("{name}.y")))),
        )),
        Expr::sym(format!("{name}.r")),
    );
    let grad = [derivative(&field, "x")?, derivative(&field, "y")?];
    let var = "t";
    let around = |center: &str, wave: &str| {
        add(
            Expr::sym(format!("{name}.{center}")),
            mul(
                Expr::sym(format!("{name}.r")),
                Expr::app(wave, vec![Expr::sym(var)]),
            ),
        )
    };
    Ok(Form::Field {
        vars,
        field,
        grad,
        exact: true,
        line: Some(Arc {
            var: var.to_string(),
            point: [around("x", "Cos"), around("y", "Sin")],
            range: [num(0.0), Expr::sym("tau")],
        }),
    })
}

/// A line as a long segment through its points, or a ray from its first.
fn line_form(a: [Expr; 2], b: [Expr; 2], ray: bool) -> Form {
    let d = minus(&b, &a);
    let len = length2(&d);
    let unit = [div(d[0].clone(), len.clone()), div(d[1].clone(), len)];
    let far = plus_scaled(&a, &unit, &num(LINE_REACH));
    let start = if ray {
        a
    } else {
        plus_scaled(&a, &unit, &num(-LINE_REACH))
    };
    Form::Polyline {
        points: vec![start, far],
        closed: false,
    }
}

pub fn compile(doc: &Document) -> Result<Scene, Error> {
    let mut lowerer = Lowerer::default();
    let mut scene = Scene {
        name: doc.name.clone(),
        params: Vec::new(),
        bindings: Vec::new(),
        marks: Vec::new(),
        viewport: Viewport::default(),
        duration: None,
        beats: Vec::new(),
        handles: Vec::new(),
    };
    let none = Locals::new();
    let mut ids: HashSet<String> = HashSet::new();
    let mut claim = |name: &str, line: usize| -> Result<(), Error> {
        if SIGNALS.contains(&name) || CONSTANTS.contains(&name) || !ids.insert(name.to_string()) {
            return Err(Error::at(line, 1, format!("`{name}` is already defined")));
        }
        Ok(())
    };
    let mut binds: Vec<(String, Expr, usize)> = Vec::new();
    let mut beat: Option<usize> = None;
    let mut beat_tags: Vec<String> = Vec::new();
    // The marks the latest statement made, for a trailing `tag`.
    let mut last_marks = 0..0;
    // Fit names resolve at the end, since a viewport comes before what it fits.
    let mut fit_names: Vec<String> = Vec::new();

    macro_rules! mark {
        ($id:expr, $form:expr, $kind:expr) => {{
            let mut tags = vec![$kind.to_string()];
            tags.extend(beat_tags.iter().cloned());
            scene.marks.push(Mark {
                id: $id,
                form: $form,
                tags,
                beat,
            });
        }};
    }

    for item in &doc.items {
        let marks_before = scene.marks.len();
        match item {
            Item::Tag(tags) => {
                for mark in &mut scene.marks[last_marks.clone()] {
                    mark.tags.extend(tags.iter().cloned());
                }
                continue;
            }
            Item::Beat { caption, tags } => {
                if scene.beats.is_empty() {
                    claim(BEAT_PARAM, 0)?;
                    scene.params.push(Param {
                        id: BEAT_PARAM.to_string(),
                        range: [0.0, 0.0],
                    });
                    scene.bindings.push(Binding {
                        id: BEAT_PARAM.to_string(),
                        expr: num(0.0),
                    });
                    lowerer.scalars.insert(BEAT_PARAM.to_string());
                }
                beat = Some(scene.beats.len());
                scene.beats.push(caption.clone());
                beat_tags = tags.clone();
                continue;
            }
            Item::EndBeat => {
                beat = None;
                beat_tags.clear();
                continue;
            }
            Item::Fn {
                name,
                params,
                body,
                line,
            } => {
                claim(name, *line)?;
                lowerer.functions.insert(
                    name.clone(),
                    Function {
                        params: params.clone(),
                        body: body.clone(),
                    },
                );
            }
            Item::Bind { name, expr, line } => {
                let low = lowerer.scalar(expr, &none).map_err(at_line(*line))?;
                if binds.iter().any(|(n, ..)| n == name) {
                    return Err(Error::at(*line, 1, format!("`{name}` is already bound")));
                }
                binds.push((name.clone(), low, *line));
            }
            Item::Param {
                name,
                default,
                range,
                line,
            } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let range = [
                    constant(&range[0], "a param's range").map_err(&fail)?,
                    constant(&range[1], "a param's range").map_err(&fail)?,
                ];
                let default = lowerer.scalar(default, &none).map_err(&fail)?;
                scene.params.push(Param {
                    id: name.clone(),
                    range,
                });
                scene.bindings.push(Binding {
                    id: name.clone(),
                    expr: default,
                });
                lowerer.scalars.insert(name.clone());
            }
            Item::Let { name, expr, line } => {
                claim(name, *line)?;
                let val = lowerer.lower(expr, &none, 0).map_err(at_line(*line))?;
                lowerer.bind(&mut scene, name, val);
            }
            Item::Duration(e) => scene.duration = Some(constant(e, "`duration`")?),
            Item::Curve { name, form, line } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                match form {
                    CurveSyntax::Implicit { vars, lhs, rhs } => {
                        let field = sub(lhs.clone(), rhs.clone());
                        let form = field_form(&lowerer, vars, &field, false).map_err(fail)?;
                        mark!(name.clone(), form, "curve");
                    }
                    CurveSyntax::Sdf { vars, field } => {
                        let form = field_form(&lowerer, vars, field, true).map_err(fail)?;
                        mark!(name.clone(), form, "curve");
                    }
                    CurveSyntax::Parametric { var, point, range } => {
                        let form = lowerer.arc(var, point, range).map_err(fail)?;
                        mark!(name.clone(), Form::Parametric(form), "curve");
                    }
                    CurveSyntax::Value(expr) => {
                        // A construction's value: named fields, drawn as its own form.
                        let val = lowerer.lower(expr, &none, 0).map_err(&fail)?;
                        let (form, kind) = match &val {
                            Val::Circle { .. } => (None, "circle"),
                            Val::Line { ray, .. } => (None, if *ray { "ray" } else { "line" }),
                            other => {
                                return Err(fail(Error::new(format!(
                                    "a `curve` can't be {}",
                                    other.kind()
                                ))));
                            }
                        };
                        let _: Option<Form> = form;
                        lowerer.bind(&mut scene, name, val.clone());
                        let form = match lowerer.named(name) {
                            Some(Val::Circle { .. }) => circle_form(name).map_err(&fail)?,
                            Some(Val::Line { a, b, ray }) => line_form(a, b, ray),
                            _ => unreachable!("just bound"),
                        };
                        mark!(name.clone(), form, kind);
                    }
                }
            }
            Item::Segment {
                name,
                points,
                closed,
                line,
            } => {
                claim(name, *line)?;
                let points = points
                    .iter()
                    .map(|p| lowerer.point(p, &none))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(at_line(*line))?;
                mark!(
                    name.clone(),
                    Form::Polyline {
                        points,
                        closed: *closed,
                    },
                    "segment"
                );
            }
            Item::RightAngle {
                name,
                points,
                size,
                line,
            } => {
                claim(name, *line)?;
                // A square at the vertex, a fixed world size along each side.
                let fail = at_line(*line);
                let locals: Locals = [
                    ("from", lowerer.lower(&points[0], &none, 0).map_err(&fail)?),
                    ("at", lowerer.lower(&points[1], &none, 0).map_err(&fail)?),
                    ("to", lowerer.lower(&points[2], &none, 0).map_err(&fail)?),
                    ("size", lowerer.lower(size, &none, 0).map_err(&fail)?),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
                let sym = |name: &str| Expr::sym(name);
                let toward = |end: &str| {
                    let delta = Expr::app("Subtract", vec![sym(end), sym("at")]);
                    let unit = Expr::app(
                        "Divide",
                        vec![delta.clone(), Expr::app("Length", vec![delta])],
                    );
                    Expr::app("Multiply", vec![sym("size"), unit])
                };
                let a = Expr::app("Add", vec![sym("at"), toward("from")]);
                let c = Expr::app("Add", vec![sym("at"), toward("to")]);
                let corner = Expr::app("Add", vec![a.clone(), toward("to")]);
                // With a side of no length there's no angle to mark.
                let side = |end: &str| -> Result<Expr, Error> {
                    let length = Expr::app("Distance", vec![sym(end), sym("at")]);
                    Ok(sub(
                        square(lowerer.scalar(&length, &locals)?),
                        num(COINCIDENT),
                    ))
                };
                let exists = min(side("from").map_err(&fail)?, side("to").map_err(&fail)?);
                let points = [a, corner, c]
                    .iter()
                    .map(|p| {
                        let [x, y] = lowerer.point(p, &locals)?;
                        Ok([
                            Expr::app("Where", vec![exists.clone(), x]),
                            Expr::app("Where", vec![exists.clone(), y]),
                        ])
                    })
                    .collect::<Result<Vec<_>, Error>>()
                    .map_err(&fail)?;
                mark!(
                    name.clone(),
                    Form::Polyline {
                        points,
                        closed: false,
                    },
                    "decoration"
                );
            }
            Item::Region { name, parts, line } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let parts = parts
                    .iter()
                    .map(|p| match p {
                        PartSyntax::Point(q) => lowerer.point(q, &none).map(Part::Point),
                        PartSyntax::Arc { var, point, range } => {
                            lowerer.arc(var, point, range).map(Part::Arc)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(&fail)?;
                mark!(name.clone(), Form::Region(parts), "region");
            }
            Item::Dots { names, line } => {
                for name in names {
                    let p = lowerer
                        .point(&Expr::sym(name), &none)
                        .map_err(at_line(*line))?;
                    mark!(name.clone(), Form::Dot(p), "dot");
                }
            }
            Item::Label {
                name,
                text,
                at,
                offset,
                line,
            } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let at = lowerer.point(at, &none).map_err(&fail)?;
                let offset = match offset {
                    Some(e) => lowerer.point(e, &none).map_err(&fail)?,
                    None => [num(0.0), num(0.0)],
                };
                mark!(
                    name.clone(),
                    Form::Label {
                        text: text.clone(),
                        at,
                        offset,
                    },
                    "label"
                );
            }
            Item::Handle {
                mark,
                param,
                inverse,
                line,
            } => {
                let fail = at_line(*line);
                let at = lowerer.point(&Expr::sym(mark), &none).map_err(&fail)?;
                let mut locals = Locals::new();
                locals.insert(
                    "pointer".to_string(),
                    Val::Point(Expr::sym("pointer.x"), Expr::sym("pointer.y")),
                );
                let inverse = lowerer.scalar(inverse, &locals).map_err(&fail)?;
                scene.handles.push(Handle {
                    mark: mark.clone(),
                    param: param.clone(),
                    at,
                    inverse,
                });
            }
            Item::Viewport {
                x,
                y,
                width,
                height,
                fit,
                pad,
            } => {
                let bound =
                    |range: &Option<[Expr; 2]>, fallback: [f64; 2]| -> Result<[f64; 2], Error> {
                        match range {
                            Some(r) => Ok([
                                constant(&r[0], "a viewport bound")?,
                                constant(&r[1], "a viewport bound")?,
                            ]),
                            None => Ok(fallback),
                        }
                    };
                let optional = |e: &Option<Expr>, what: &str| -> Result<Option<f64>, Error> {
                    e.as_ref().map(|e| constant(e, what)).transpose()
                };
                scene.viewport = Viewport {
                    x: bound(x, [-2.0, 2.0])?,
                    y: bound(y, [-2.0, 2.0])?,
                    design_width: optional(width, "a viewport's width")?,
                    design_height: optional(height, "a viewport's height")?,
                    fit: Vec::new(),
                    pad: optional(pad, "a viewport's pad")?.unwrap_or(0.1),
                };
                fit_names = fit.clone();
                let v = &scene.viewport;
                if v.x[0] >= v.x[1] || v.y[0] >= v.y[1] {
                    return Err(Error::new("a viewport's bounds must increase"));
                }
            }
            Item::Style(_) => {}
        }
        last_marks = marks_before..scene.marks.len();
    }

    for name in fit_names {
        let entry = match lowerer.named(&name) {
            Some(Val::Circle { x, y, r }) => [x, y, r],
            Some(Val::Point(x, y)) => [x, y, num(0.0)],
            _ => {
                return Err(Error::new(format!(
                    "`fit: {name}` needs a point or circle named `{name}`"
                )));
            }
        };
        scene.viewport.fit.push(entry);
    }
    if !scene.beats.is_empty() {
        let last = scene.beats.len() as f64 - 1.0;
        scene.params[0].range = [0.0, last];
    }
    // Bind expressions were lowered where they were written, and only here replace
    // the param's default.
    for (name, expr, line) in binds {
        let Some(binding) = scene.bindings.iter_mut().find(|b| b.id == name) else {
            return Err(Error::at(
                line,
                1,
                format!("`bind {name}`: no param named `{name}`"),
            ));
        };
        if !scene.params.iter().any(|p| p.id == name) {
            return Err(Error::at(
                line,
                1,
                format!("`bind {name}`: `{name}` isn't a param"),
            ));
        }
        binding.expr = expr;
    }
    for handle in &scene.handles {
        if !scene.params.iter().any(|p| p.id == handle.param) {
            return Err(Error::new(format!(
                "`handle {}` sets `{}`, which isn't a param",
                handle.mark, handle.param
            )));
        }
        if !scene.marks.iter().any(|m| m.id == handle.mark) {
            return Err(Error::new(format!(
                "`handle {0}`: nothing draws `{0}`, so it can't be grabbed",
                handle.mark
            )));
        }
    }
    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn scene(src: &str) -> Result<Scene, Error> {
        compile(&parse(src)?)
    }

    fn field(s: &Scene, i: usize) -> (&Expr, &[Expr; 2]) {
        match &s.marks[i].form {
            Form::Field { field, grad, .. } => (field, grad),
            other => panic!("{other:?}"),
        }
    }

    /// Evaluates a scene's bindings at a time.
    fn env_of(s: &Scene, time: f64) -> Env {
        let mut env = Env::new(time);
        for b in &s.bindings {
            match eval::real(&b.expr, &env) {
                Ok(v) => env.set(&b.id, v),
                Err(e) if e.absent => env.set_absent(&b.id),
                Err(e) => panic!("{e}"),
            }
        }
        env
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn gradients_treat_bindings_as_constant() {
        let s = scene(
            "let k = 3
             let r = k + time
             curve c = implicit (x, y): r * x^2 = 1",
        )
        .unwrap();
        let mut env = Env::new(2.0);
        env.set("r", 5.0);
        env.set("x", 4.0);
        env.set("y", 0.0);
        // ∂/∂x of r·x² − 1.
        assert_eq!(eval::real(&field(&s, 0).1[0], &env).unwrap(), 40.0);
    }

    #[test]
    fn coordinates_shadow_lets() {
        let s = scene(
            "let x = 100
             curve c = implicit (x, y): x^2 + y^2 = 1",
        )
        .unwrap();
        let mut env = Env::new(0.0);
        env.set("x", 3.0);
        env.set("y", 0.0);
        assert_eq!(eval::real(&field(&s, 0).1[0], &env).unwrap(), 6.0);
    }

    #[test]
    fn rejects_unknown_and_duplicate_names() {
        let err = scene("curve c = implicit (x, y): x = q").unwrap_err();
        assert!(err.message.contains("unknown name `q`"), "{err}");
        let err = scene("let a = 1\nlet a = 2").unwrap_err();
        assert!(err.message.contains("already defined"), "{err}");
        // Coordinates aren't ambient: they only exist in their curve.
        let err = scene("let a = x").unwrap_err();
        assert!(err.message.contains("unknown name `x`"), "{err}");
    }

    #[test]
    fn errors_name_their_line() {
        let err = scene("let a = 1\nlet b = frob(1)").unwrap_err();
        assert!(err.message.contains("unknown function `frob`"), "{err}");
        assert_eq!(err.line, 2);
        let err = scene("fn f(x) = f(x)\nlet a = f(1)").unwrap_err();
        assert!(err.message.contains("recurse"), "{err}");
    }

    #[test]
    fn point_algebra_expands_symbolically() {
        let s = scene(
            "let P = (1, 2)
             let Q = 2 * P + (P.y, -P.x)",
        )
        .unwrap();
        let ids: Vec<&str> = s.bindings.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["P.x", "P.y", "Q.x", "Q.y"]);
        let env = env_of(&s, 0.0);
        assert_eq!(env.get("Q.x"), Some(4.0));
        assert_eq!(env.get("Q.y"), Some(3.0));
    }

    #[test]
    fn functions_inline_with_their_own_scope() {
        let s = scene(
            "fn on(t) = (cos(t), sin(t))
             fn scale(p, k) = p * k
             let P = scale(on(0), 3)",
        )
        .unwrap();
        assert_eq!(env_of(&s, 0.0).get("P.x"), Some(3.0));
        let err = scene("fn f(x) = y\nlet y = 1\nlet a = f(1)").err();
        // Bodies see globals known when they're called, not the caller's locals.
        assert!(err.is_none());
    }

    #[test]
    fn a_bind_replaces_the_default_with_a_time_expression() {
        let s = scene("param a = 1 in 0..2\nbind a = time * 2").unwrap();
        assert_eq!(s.params[0].range, [0.0, 2.0]);
        assert_eq!(env_of(&s, 3.0).get("a"), Some(6.0));
        let err = scene("let a = 1\nbind a = time").unwrap_err();
        assert!(err.message.contains("isn't a param"), "{err}");
    }

    #[test]
    fn each_form_keeps_its_own_representation() {
        let s = scene(
            "curve a = implicit (x, y): x = 0
             curve b = sdf (x, y): x
             curve c = parametric (t): (t, t) for t in 0..1
             segment d = (0, 0), (1, 1)
             region e = (0, 0), (t, 1) for t in 0..1
             let P = (0, 0)
             dot P
             label f = \"F\" at P",
        )
        .unwrap();
        let kinds: Vec<&str> = s
            .marks
            .iter()
            .map(|m| match m.form {
                Form::Field { exact: false, .. } => "implicit",
                Form::Field { exact: true, .. } => "sdf",
                Form::Parametric(_) => "parametric",
                Form::Polyline { .. } => "polyline",
                Form::Region(_) => "region",
                Form::Dot(_) => "dot",
                Form::Label { .. } => "label",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "implicit",
                "sdf",
                "parametric",
                "polyline",
                "region",
                "dot",
                "label"
            ]
        );
    }

    #[test]
    fn constructions_lower_to_named_scalars() {
        let s = scene(
            "let O = (0, 0)
             let A = (1, 0)
             let B = midpoint(O, A)
             curve c = circle(O, A)
             curve g = ray(O, B)",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!(env.get("B.x"), Some(0.5));
        assert_eq!(env.get("c.r"), Some(1.0));
        assert_eq!(env.get("g.bx"), Some(0.5));
        // A circle is drawn as an exact field, and a ray as a line.
        assert!(matches!(s.marks[0].form, Form::Field { exact: true, .. }));
        assert!(matches!(s.marks[1].form, Form::Polyline { .. }));
        assert!(s.marks[0].tags.contains(&"circle".to_string()));
        assert!(s.marks[1].tags.contains(&"ray".to_string()));
    }

    fn point_at(env: &Env, name: &str) -> (f64, f64) {
        (
            env.get(&format!("{name}.x")).unwrap(),
            env.get(&format!("{name}.y")).unwrap(),
        )
    }

    fn absent(env: &Env, name: &str) -> bool {
        env.is_absent(&format!("{name}.x")) && env.is_absent(&format!("{name}.y"))
    }

    #[test]
    fn intersections_follow_the_documented_order() {
        let s = scene(
            "let O = (0, 0)
             let R = (3, 0)
             curve c = circle(O, (1, 0))
             curve a = line((-3, 0), R)
             curve g = ray(O, R)
             let first = intersect(c, a)
             let second = intersect(c, a, 1)
             let behind = intersect(c, g, 0)
             let from_inside = intersect(c, g, 1)
             curve d = circle((1, 0), (1, 1))
             let left = intersect(c, d, 0)
             let right = intersect(c, d, 1)
             let meet = intersect(line((0, -1), (0, 1)), a)",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!(point_at(&env, "first"), (-1.0, 0.0));
        assert_eq!(point_at(&env, "second"), (1.0, 0.0));
        // A ray from inside meets the circle once: root 1. Root 0 is behind its start.
        assert!(absent(&env, "behind"));
        assert_eq!(point_at(&env, "from_inside"), (1.0, 0.0));
        // Left of the line between the centers (from c's center, facing +x) is up.
        assert!(point_at(&env, "left").1 > 0.0 && point_at(&env, "right").1 < 0.0);
        assert!(near(point_at(&env, "left").0, 0.5));
        assert!(near(point_at(&env, "meet").0, 0.0) && near(point_at(&env, "meet").1, 0.0));
    }

    #[test]
    fn a_line_that_misses_a_circle_has_no_points() {
        let s = scene(
            "curve c = circle((0, 0), (1, 0))
             curve l = line((-2, 2), (2, 2))
             let a = intersect(c, l, 0)
             let b = intersect(c, l, 1)",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert!(absent(&env, "a") && absent(&env, "b"));
    }

    /// Tangent by construction, at many angles, so rounding lands on both sides of zero.
    #[test]
    fn a_tangent_line_touches_at_every_angle() {
        let s = scene(
            "param a in 0..7
             let O = (0, 0)
             let T = (cos(a), sin(a))
             curve c = circle(O, T)
             curve r = line(O, T)
             curve t = perpendicular(T, r)
             let first = intersect(c, t, 0)
             let second = intersect(c, t, 1)",
        )
        .unwrap();
        for i in 0..2000 {
            let mut env = Env::new(0.0);
            env.set("a", i as f64 * 0.0035);
            for b in s.bindings.iter().filter(|b| b.id != "a") {
                env.set(&b.id, eval::real(&b.expr, &env).unwrap());
            }
            for name in ["first", "second"] {
                let (x, y) = point_at(&env, name);
                let (tx, ty) = point_at(&env, "T");
                assert!((x - tx).hypot(y - ty) < 1e-6, "{name} at step {i}");
            }
        }
    }

    /// As a ray's start passes into a circle, root 0 goes and root 1 stays put.
    #[test]
    fn roots_keep_their_place_as_a_rays_start_moves() {
        let s = scene(
            "param a in -3..3
             curve c = circle((0, 0), (1, 0))
             curve g = ray((a, 0), (a + 1, 0))
             let near = intersect(c, g, 0)
             let far = intersect(c, g, 1)",
        )
        .unwrap();
        let at = |a: f64| {
            let mut env = Env::new(0.0);
            env.set("a", a);
            for b in s.bindings.iter().filter(|b| b.id != "a") {
                match eval::real(&b.expr, &env) {
                    Ok(v) => env.set(&b.id, v),
                    Err(_) => env.set_absent(&b.id),
                }
            }
            env
        };
        let outside = at(-2.0);
        assert_eq!(point_at(&outside, "near"), (-1.0, 0.0));
        assert_eq!(point_at(&outside, "far"), (1.0, 0.0));
        let inside = at(0.0);
        assert!(absent(&inside, "near"));
        assert_eq!(point_at(&inside, "far"), (1.0, 0.0));
        let past = at(2.0);
        assert!(absent(&past, "near") && absent(&past, "far"));
        // A start exactly on the circle still counts.
        assert_eq!(point_at(&at(-1.0), "near"), (-1.0, 0.0));
    }

    #[test]
    fn rays_only_meet_what_is_ahead_of_them() {
        let s = scene(
            "curve up = line((0, -1), (0, 1))
             curve away = ray((1, 0), (2, 0))
             curve toward = ray((1, 0), (0.5, 0))
             let missed = intersect(away, up)
             let hit = intersect(toward, up)
             let other_order = intersect(up, away)
             let as_line = intersect(line((1, 0), (2, 0)), up)",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert!(absent(&env, "missed") && absent(&env, "other_order"));
        assert_eq!(point_at(&env, "hit"), (0.0, 0.0));
        assert_eq!(point_at(&env, "as_line"), (0.0, 0.0));
    }

    #[test]
    fn parallel_lines_do_not_meet() {
        let s = scene(
            "let a = intersect(line((0, 0), (1, 1)), line((0, 1), (3, 4)))
             let b = intersect(line((0, 0), (1, 1)), line((0, 1), (3, 4.001)))",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert!(absent(&env, "a"));
        assert!(!absent(&env, "b"));
    }

    #[test]
    fn circles_meet_only_when_they_cross_or_touch() {
        let s = scene(
            "let O = (0, 0)
             curve c = circle(O, (1, 0))
             let apart = intersect(c, circle((3, 0), (4, 0)))
             let nested = intersect(c, circle((0.1, 0), (0.3, 0)))
             let same = intersect(c, circle(O, (0, 1)))
             let outer = intersect(c, circle((2, 0), (3, 0)))
             let inner = intersect(c, circle((0.5, 0), (1, 0)), 1)",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        for name in ["apart", "nested", "same"] {
            assert!(absent(&env, name), "{name}");
        }
        assert_eq!(point_at(&env, "outer"), (1.0, 0.0));
        assert_eq!(point_at(&env, "inner"), (1.0, 0.0));
    }

    #[test]
    fn two_points_in_one_place_make_no_line() {
        let s = scene(
            "let A = (1, 1)
             curve l = line(A, A)
             let across = mirror((0, 0), l)
             curve square_corner = line((0, 0), (1, 0))
             rightangle m = A, A, (2, 2) size 0.1",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert!(env.is_absent("l.ax"));
        // Whatever is built from something absent is absent.
        assert!(absent(&env, "across"));
        assert!(!env.is_absent("square_corner.ax"));
        match &s.marks[2].form {
            Form::Polyline { points, .. } => {
                assert!(eval::real(&points[0][0], &env).unwrap_err().absent);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn exists_and_otherwise_see_absence() {
        let s = scene(
            "curve c = circle((0, 0), (1, 0))
             curve l = line((-2, 2), (2, 2))
             curve h = line((-2, 0), (2, 0))
             let none = intersect(c, l)
             let some = intersect(c, h, 1)
             let a = exists(none)
             let b = exists(some)
             let c_x = exists(c)
             let d = dist(none, some)
             let e = exists(d)
             let f = otherwise(none, (7, 8))
             let g = otherwise(some, (7, 8))
             let half = otherwise((none.x, 2), (7, 8))
             let number = otherwise(d, 9)",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!(env.get("a"), Some(0.0));
        assert_eq!(env.get("b"), Some(1.0));
        assert_eq!(env.get("c_x"), Some(1.0));
        assert!(env.is_absent("d"));
        assert_eq!(env.get("e"), Some(0.0));
        assert_eq!(point_at(&env, "f"), (7.0, 8.0));
        assert_eq!(point_at(&env, "g"), (1.0, 0.0));
        // A value is all there or not: no mix of one's x and the other's y.
        assert_eq!(point_at(&env, "half"), (7.0, 8.0));
        assert_eq!(env.get("number"), Some(9.0));
        let err = scene("let a = otherwise((0, 0), 1)").unwrap_err();
        assert!(err.message.contains("two of one kind"), "{err}");
    }

    #[test]
    fn a_field_needs_partial_values_named_first() {
        let err = scene(
            "curve c = circle((0, 0), (1, 0))
             curve l = line((0, 0), (1, 0))
             curve f = sdf (x, y): x - intersect(c, l).x",
        );
        // `.x` on a call isn't syntax, so a function carries it, as a figure would write it.
        assert!(err.is_err());
        let err = scene(
            "fn first(p) = p.x
             curve c = circle((0, 0), (1, 0))
             curve f = sdf (x, y): x - first(intersect(c, line((0, 0), (1, 0))))",
        )
        .unwrap_err();
        assert!(err.message.contains("with `let` first"), "{err}");
        assert_eq!(err.line, 3);
        let named = scene(
            "curve c = circle((0, 0), (1, 0))
             let P = intersect(c, line((0, 0), (1, 0)))
             curve f = sdf (x, y): x - P.x",
        );
        assert!(named.is_ok());
    }

    #[test]
    fn mirrors_and_perpendiculars() {
        let s = scene(
            "let O = (0, 0)
             let A = (1, 0)
             curve f = line(O, A)
             let P = (2, 3)
             let flipped = mirror(P, f)
             let through = mirror(A, O)
             curve c = circle(P, (2, 4))
             curve m = mirror(c, f)
             curve h = perpendicular(P, f)
             let foot = intersect(f, h)",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!(point_at(&env, "flipped"), (2.0, -3.0));
        assert_eq!(point_at(&env, "through"), (-1.0, 0.0));
        assert_eq!(point_at(&env, "m"), (2.0, -3.0));
        assert_eq!(env.get("m.r"), Some(1.0));
        assert_eq!(point_at(&env, "foot"), (2.0, 0.0));
    }

    #[test]
    fn wrong_construction_arguments_say_what_they_got() {
        let err = scene("let a = circle(1, 2)").unwrap_err();
        assert!(
            err.message.contains("`Circle` can't take a number"),
            "{err}"
        );
        let err = scene("let a = intersect((0, 0), (1, 1))").unwrap_err();
        assert!(
            err.message.contains("can't take a point and a point"),
            "{err}"
        );
        let err = scene("curve a = intersect(line((0,0),(1,0)), line((0,0),(0,1)))").unwrap_err();
        assert!(err.message.contains("can't be a point"), "{err}");
    }

    #[test]
    fn beats_declare_a_param_and_group_marks() {
        let s = scene(
            "let P = (0, 0)
             beat \"One\" tag blue {
               dot P
             }
             beat \"Two\" {
               curve c = circle(P, (1, 0)) tag aux
             }",
        )
        .unwrap();
        assert_eq!(s.beats, ["One", "Two"]);
        assert_eq!(s.params[0].id, BEAT_PARAM);
        assert_eq!(s.params[0].range, [0.0, 1.0]);
        assert_eq!(s.marks[0].beat, Some(0));
        assert!(s.marks[0].tags.contains(&"blue".to_string()));
        assert_eq!(s.marks[1].beat, Some(1));
        assert!(s.marks[1].tags.contains(&"aux".to_string()));
        assert!(!s.marks[1].tags.contains(&"blue".to_string()));
    }

    #[test]
    fn handles_need_a_param_and_a_drawn_mark() {
        let ok = scene(
            "param a in 0..1
             let P = (a, 0)
             dot P
             handle P sets a = pointer.x",
        )
        .unwrap();
        assert_eq!(ok.handles[0].param, "a");
        let err = scene("let P = (0, 0)\ndot P\nhandle P sets a = pointer.x").unwrap_err();
        assert!(err.message.contains("isn't a param"), "{err}");
        let err =
            scene("param a in 0..1\nlet P = (a, 0)\nhandle P sets a = pointer.x").unwrap_err();
        assert!(err.message.contains("nothing draws"), "{err}");
    }

    #[test]
    fn a_fit_viewport_resolves_later_names() {
        let s = scene(
            "viewport { fit: c P, pad: 0.05, width: 560, height: 400 }
             let P = (5, 5)
             curve c = circle((0, 0), (1, 0))",
        )
        .unwrap();
        assert_eq!(s.viewport.fit.len(), 2);
        assert_eq!(s.viewport.pad, 0.05);
        assert_eq!(s.viewport.design_height, Some(400.0));
        let err = scene("viewport { fit: nope }").unwrap_err();
        assert!(err.message.contains("needs a point or circle"), "{err}");
    }
}
