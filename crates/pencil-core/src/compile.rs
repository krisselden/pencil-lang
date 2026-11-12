//! Lowers a parsed document to the scene IR. Functions are inlined, and point algebra
//! and constructions are expanded here, so downstream only sees scalar expressions,
//! still symbolic.

use std::collections::{HashMap, HashSet};

use crate::diff::derivative;
use crate::error::Error;
use crate::eval::{self, Env};
use crate::ir::{
    Arc, Binding, Choice, Expr, Expression, Form, Handle, Hang, Mark, Param, Part, Piece, Scene, Spot,
    System, Viewport,
};
use crate::parse::{CurveSyntax, Document, Item, ManyKind, PartSyntax};

/// Names every expression can use without declaring them.
/// Beyond this, `z^n` goes through exp and ln rather than multiplying out.
const MAX_COMPLEX_POWER: f64 = 8.0;
pub const SIGNALS: [&str; 1] = ["time"];
pub const CONSTANTS: [&str; 3] = ["pi", "π", "tau"];
/// The discrete param that beats declare.
pub const BEAT_PARAM: &str = "beat";

/// Builtin heads that take and return scalars.
const SCALAR_HEADS: [&str; 20] = [
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
    "Snap",
    "Timeline",
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
    /// A circle with a handedness, which is a line when `a` is 0: the zeros of
    /// `a|p|² − 2(bx·x + by·y) + c`, scaled so that `bx² + by² − ac` is 1. Its negative side is
    /// the inside of a counterclockwise circle. `a` is signed curvature.
    Cycle {
        a: Expr,
        bx: Expr,
        by: Expr,
        c: Expr,
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
            Val::Circle { .. } | Val::Cycle { .. } => "a circle",
            Val::Line { .. } => "a line",
        }
    }

    fn fields(&self) -> Vec<Expr> {
        match self {
            Val::Scalar(e) => vec![e.clone()],
            Val::Point(x, y) => vec![x.clone(), y.clone()],
            Val::Circle { x, y, r } => vec![x.clone(), y.clone(), r.clone()],
            Val::Cycle { a, bx, by, c } => vec![a.clone(), bx.clone(), by.clone(), c.clone()],
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
            Val::Cycle { .. } => Val::Cycle {
                a: next(),
                bx: next(),
                by: next(),
                c: next(),
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
    Cycle,
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
// Complex arithmetic folds literal 0s and 1s, since `i` is one of each, so `2·i·w` costs what
// it would to write it out.
fn is_num(e: &Expr, n: f64) -> bool {
    matches!(e, Expr::Num(v) if *v == n)
}
fn times(a: &Expr, b: &Expr) -> Expr {
    if is_num(a, 0.0) || is_num(b, 0.0) {
        num(0.0)
    } else if is_num(a, 1.0) {
        b.clone()
    } else if is_num(b, 1.0) {
        a.clone()
    } else {
        mul(a.clone(), b.clone())
    }
}
fn plus(a: Expr, b: Expr) -> Expr {
    if is_num(&a, 0.0) {
        b
    } else if is_num(&b, 0.0) {
        a
    } else {
        add(a, b)
    }
}
fn less(a: Expr, b: Expr) -> Expr {
    if is_num(&b, 0.0) {
        a
    } else if is_num(&a, 0.0) {
        negated(b)
    } else {
        sub(a, b)
    }
}
fn negated(a: Expr) -> Expr {
    if is_num(&a, 0.0) {
        a
    } else {
        Expr::app("Negate", vec![a])
    }
}
fn is_sum_of_squares(e: &Expr) -> bool {
    let squared = |e: &Expr| matches!(e, Expr::App(head, args) if head == "Power" && args.get(1).is_some_and(|n| is_num(n, 2.0)));
    matches!(e, Expr::App(head, args) if head == "Add" && args.len() == 2 && squared(&args[0]) && squared(&args[1]))
}
/// (a + bi)(c + di).
fn complex_times(a: &Expr, b: &Expr, c: &Expr, d: &Expr) -> [Expr; 2] {
    [
        less(times(a, c), times(b, d)),
        plus(times(a, d), times(b, c)),
    ]
}
/// (a + bi) / (c + di), multiplying through by the conjugate.
fn complex_over(a: &Expr, b: &Expr, c: &Expr, d: &Expr) -> [Expr; 2] {
    let norm = add(square(c.clone()), square(d.clone()));
    [
        div(plus(times(a, c), times(b, d)), norm.clone()),
        div(less(times(b, c), times(a, d)), norm),
    ]
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
            Shape::Cycle => Some(Val::Cycle {
                a: field("a"),
                bx: field("bx"),
                by: field("by"),
                c: field("c"),
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
                } else if s == "i" {
                    // Anything named `i` comes first, so a loop can still count with it.
                    Val::Point(num(0.0), num(1.0))
                } else if let Some((base, axis)) = s.split_once('.') {
                    match (locals.get(base), axis) {
                        // A number is a point on the real axis.
                        (Some(Val::Scalar(x)), "x") => Val::Scalar(x.clone()),
                        (Some(Val::Scalar(_)), "y") => Val::Scalar(num(0.0)),
                        (None, "x") if self.scalars.contains(base) => Val::Scalar(Expr::sym(base)),
                        (None, "y") if self.scalars.contains(base) => Val::Scalar(num(0.0)),
                        (Some(Val::Point(x, _)), "x") | (Some(Val::Circle { x, .. }), "x") => {
                            Val::Scalar(x.clone())
                        }
                        (Some(Val::Point(_, y)), "y") | (Some(Val::Circle { y, .. }), "y") => {
                            Val::Scalar(y.clone())
                        }
                        (Some(Val::Circle { r, .. }), "r") => Val::Scalar(r.clone()),
                        (Some(Val::Cycle { a, .. }), "a") => Val::Scalar(a.clone()),
                        (Some(Val::Cycle { a, bx, .. }), "x") => Val::Scalar(cycle_center(a, bx)),
                        (Some(Val::Cycle { a, by, .. }), "y") => Val::Scalar(cycle_center(a, by)),
                        (Some(Val::Cycle { a, .. }), "r") => Val::Scalar(cycle_radius(a)),
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
                if head == PUSHED {
                    return self.pushed(args, locals, depth);
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

    /// Where a direction at a point goes through a map: the derivative of `map(at + s·direction)`
    /// at `s` = 0, exactly.
    fn pushed(&self, args: &[Expr], locals: &Locals, depth: usize) -> Result<Val, Error> {
        let [Expr::Sym(map), at, direction] = args else {
            unreachable!("`through` writes these");
        };
        let mut inner = locals.clone();
        inner.insert(STEP.to_string(), Val::Scalar(Expr::sym(STEP)));
        let moved = add(at.clone(), mul(Expr::sym(STEP), direction.clone()));
        let image = Expr::app(map, vec![moved]);
        let Val::Point(x, y) = self.lower(&image, &inner, depth + 1)? else {
            return Err(Error::new(format!("`{map}` has to give a point")));
        };
        fn at_zero(e: &Expr) -> Expr {
            match e {
                Expr::Sym(s) if s == STEP => num(0.0),
                Expr::App(head, args) => {
                    Expr::App(head.clone(), args.iter().map(at_zero).collect())
                }
                other => other.clone(),
            }
        }
        Ok(Val::Point(
            at_zero(&crate::diff::derivative(&x, STEP)?),
            at_zero(&crate::diff::derivative(&y, STEP)?),
        ))
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
        use Val::{Circle, Cycle, Line, Point, Scalar};
        // Where a function wants points, a number is one on the real axis.
        let low = if matches!(
            head,
            "Circle" | "Line" | "Ray" | "Distance" | "Dot" | "Midpoint"
        ) {
            low.into_iter()
                .map(|v| match v {
                    Scalar(x) => Point(x, num(0.0)),
                    other => other,
                })
                .collect()
        } else {
            low
        };
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
            ("Add", [Point(ax, ay), Point(bx, by)]) => {
                Point(plus(ax.clone(), bx.clone()), plus(ay.clone(), by.clone()))
            }
            ("Subtract", [Point(ax, ay), Point(bx, by)]) => {
                Point(less(ax.clone(), bx.clone()), less(ay.clone(), by.clone()))
            }
            ("Add" | "Subtract" | "Multiply" | "Divide", [Scalar(a), Scalar(b)]) => {
                Scalar(Expr::app(head, vec![a.clone(), b.clone()]))
            }
            // A point is a complex number, and a number is one on the real axis.
            ("Add", [Scalar(k), Point(x, y)]) | ("Add", [Point(x, y), Scalar(k)]) => {
                Point(plus(x.clone(), k.clone()), y.clone())
            }
            ("Subtract", [Point(x, y), Scalar(k)]) => Point(less(x.clone(), k.clone()), y.clone()),
            ("Subtract", [Scalar(k), Point(x, y)]) => {
                Point(less(k.clone(), x.clone()), negated(y.clone()))
            }
            ("Multiply", [Scalar(k), Point(x, y)]) | ("Multiply", [Point(x, y), Scalar(k)]) => {
                Point(times(k, x), times(k, y))
            }
            ("Multiply", [Point(a, b), Point(c, d)]) => point(complex_times(a, b, c, d)),
            ("Divide", [Point(x, y), Scalar(k)]) => {
                Point(div(x.clone(), k.clone()), div(y.clone(), k.clone()))
            }
            ("Divide", [Point(a, b), Point(c, d)]) => point(complex_over(a, b, c, d)),
            ("Divide", [Scalar(k), Point(c, d)]) => point(complex_over(k, &num(0.0), c, d)),
            // abs(z)²: the root of a sum of squares, squared, is that sum.
            ("Power", [Scalar(Expr::App(root, inner)), Scalar(Expr::Num(n))])
                if root == "Sqrt" && *n == 2.0 && inner.first().is_some_and(is_sum_of_squares) =>
            {
                Scalar(inner[0].clone())
            }
            // Hyperbolic functions are exp and ln, for numbers and complex numbers alike, so
            // every evaluator and shader already has them.
            (
                "Sinh" | "Cosh" | "Tanh" | "Coth" | "Sech" | "Csch",
                [v @ (Scalar(_) | Point(..))],
            ) => {
                let up = self.builtin("Exp", vec![v.clone()])?;
                let flipped = self.builtin("Negate", vec![v.clone()])?;
                let down = self.builtin("Exp", vec![flipped])?;
                let sum = self.builtin("Add", vec![up.clone(), down.clone()])?;
                let difference = self.builtin("Subtract", vec![up, down])?;
                let two = Scalar(num(2.0));
                match head {
                    "Sinh" => self.builtin("Divide", vec![difference, two])?,
                    "Cosh" => self.builtin("Divide", vec![sum, two])?,
                    "Tanh" => self.builtin("Divide", vec![difference, sum])?,
                    "Coth" => self.builtin("Divide", vec![sum, difference])?,
                    "Sech" => self.builtin("Divide", vec![two, sum])?,
                    _ => self.builtin("Divide", vec![two, difference])?,
                }
            }
            ("Arctanh", [v @ (Scalar(_) | Point(..))]) => {
                let one = Scalar(num(1.0));
                let above = self.builtin("Add", vec![one.clone(), v.clone()])?;
                let below = self.builtin("Subtract", vec![one, v.clone()])?;
                let ratio = self.builtin("Divide", vec![above, below])?;
                let log = self.builtin("Ln", vec![ratio])?;
                self.builtin("Divide", vec![log, Scalar(num(2.0))])?
            }
            ("Real", [Point(x, _)]) | ("Real", [Scalar(x)]) => Scalar(x.clone()),
            ("Imag", [Point(_, y)]) => Scalar(y.clone()),
            ("Imag", [Scalar(_)]) => Scalar(num(0.0)),
            ("Conj", [Point(x, y)]) => Point(x.clone(), negated(y.clone())),
            ("Conj", [Scalar(x)]) => Scalar(x.clone()),
            ("Abs", [Point(x, y)]) => Scalar(length2(&[x.clone(), y.clone()])),
            ("Arg", [Point(x, y)]) => Scalar(Expr::app("Arctan2", vec![y.clone(), x.clone()]))
                .only_where(&sub(
                    add(square(x.clone()), square(y.clone())),
                    num(COINCIDENT),
                )),
            ("Arg", [Scalar(x)]) => Scalar(Expr::app("Arctan2", vec![num(0.0), x.clone()])),
            ("Exp", [Point(x, y)]) => {
                let grown = Expr::app("Exp", vec![x.clone()]);
                Point(
                    times(&grown, &Expr::app("Cos", vec![y.clone()])),
                    times(&grown, &Expr::app("Sin", vec![y.clone()])),
                )
            }
            // The principal value, which doesn't exist at 0.
            ("Ln", [Point(x, y)]) => Point(
                Expr::app("Ln", vec![length2(&[x.clone(), y.clone()])]),
                Expr::app("Arctan2", vec![y.clone(), x.clone()]),
            )
            .only_where(&sub(
                add(square(x.clone()), square(y.clone())),
                num(COINCIDENT),
            )),
            // The principal root: the half with a nonnegative real part.
            ("Sqrt", [Point(x, y)]) => {
                let r = length2(&[x.clone(), y.clone()]);
                let half = |e: Expr| sqrt(div(e, num(2.0)));
                let side = Expr::app("IfNonnegative", vec![y.clone(), num(1.0), num(-1.0)]);
                Point(
                    half(add(r.clone(), x.clone())),
                    mul(side, half(sub(r, x.clone()))),
                )
            }
            ("Power", [Point(x, y), Scalar(Expr::Num(n))])
                if n.fract() == 0.0 && n.abs() <= MAX_COMPLEX_POWER =>
            {
                let mut out = [num(1.0), num(0.0)];
                for _ in 0..(n.abs() as usize) {
                    out = complex_times(&out[0], &out[1], x, y);
                }
                if *n < 0.0 {
                    out = complex_over(&num(1.0), &num(0.0), &out[0], &out[1]);
                }
                point(out)
            }
            // Anything else is exp(w·ln z), on the principal branch.
            ("Power", [z, w]) if matches!(z, Point(..)) || matches!(w, Point(..)) => {
                let base = match z {
                    Scalar(k) => Point(k.clone(), num(0.0)),
                    other => other.clone(),
                };
                let log = self.builtin("Ln", vec![base])?;
                let product = self.builtin("Multiply", vec![w.clone(), log])?;
                self.builtin("Exp", vec![product])?
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
            // Through three points, in order: counterclockwise has its inside negative. Worked out
            // from the first point, so nothing is divided until it's scaled, and three points in a
            // line are a line.
            ("Circle", [Point(ax, ay), Point(bx, by), Point(cx, cy)]) => {
                let u = [sub(bx.clone(), ax.clone()), sub(by.clone(), ay.clone())];
                let v = [sub(cx.clone(), ax.clone()), sub(cy.clone(), ay.clone())];
                let (uu, vv) = (dot2(&u, &u), dot2(&v, &v));
                let twice = mul(
                    num(2.0),
                    sub(
                        mul(u[0].clone(), v[1].clone()),
                        mul(u[1].clone(), v[0].clone()),
                    ),
                );
                // The center from the first point, over its denominator: the blog's w, before dividing.
                let wx = sub(mul(uu.clone(), v[1].clone()), mul(vv.clone(), u[1].clone()));
                let wy = sub(mul(vv.clone(), u[0].clone()), mul(uu.clone(), v[0].clone()));
                let scale = sqrt(add(square(wx.clone()), square(wy.clone())));
                let exists = sub(
                    add(square(wx.clone()), square(wy.clone())),
                    num(COINCIDENT * COINCIDENT),
                );
                let at = [ax.clone(), ay.clone()];
                // F(p) = a|p − A|² − 2w·(p − A), written out from the origin.
                let world_bx = add(wx.clone(), mul(twice.clone(), ax.clone()));
                let world_by = add(wy.clone(), mul(twice.clone(), ay.clone()));
                let world_c = add(
                    mul(twice.clone(), dot2(&at, &at)),
                    mul(num(2.0), add(mul(wx, ax.clone()), mul(wy, ay.clone()))),
                );
                Cycle {
                    a: div(twice, scale.clone()),
                    bx: div(world_bx, scale.clone()),
                    by: div(world_by, scale.clone()),
                    c: div(world_c, scale),
                }
                .only_where(&exists)
            }
            ("Flip", [Cycle { a, bx, by, c }]) => Cycle {
                a: negated(a.clone()),
                bx: negated(bx.clone()),
                by: negated(by.clone()),
                c: negated(c.clone()),
            },
            ("Flip", [Circle { x, y, r }]) => Cycle {
                a: negated(div(num(1.0), r.clone())),
                bx: negated(div(x.clone(), r.clone())),
                by: negated(div(y.clone(), r.clone())),
                c: negated(div(
                    sub(add(square(x.clone()), square(y.clone())), square(r.clone())),
                    r.clone(),
                )),
            },
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
            Val::Scalar(x) => Ok([x, num(0.0)]),
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
            Val::Cycle { a, bx, by, c } => {
                let (x, y, r) = (
                    cycle_center(&a, &bx),
                    cycle_center(&a, &by),
                    cycle_radius(&a),
                );
                push(Some("a"), a, &mut self.scalars);
                push(Some("bx"), bx, &mut self.scalars);
                push(Some("by"), by, &mut self.scalars);
                push(Some("c"), c, &mut self.scalars);
                // Only a circle has a center and a radius: a line doesn't.
                push(Some("x"), x, &mut self.scalars);
                push(Some("y"), y, &mut self.scalars);
                push(Some("r"), r, &mut self.scalars);
                Some(Shape::Cycle)
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

/// The beats with lengths, and the pauses between them, as the document tells them.
#[derive(Default)]
struct Story {
    steps: Vec<Step>,
    moves: Vec<crate::parse::Move>,
    /// Each block's beat, and the marks it made.
    blocks: Vec<(usize, std::ops::Range<usize>)>,
    /// The block being read: its beat, and where its marks start.
    open: Option<(usize, usize)>,
    caption: String,
}

enum Step {
    Move(f64),
    Pause(f64),
}

fn declare_beat(scene: &mut Scene, lowerer: &mut Lowerer) {
    scene.params.push(Param {
        id: BEAT_PARAM.to_string(),
        range: [0.0, 0.0],
        step: None,
        choice: None,
    });
    scene.bindings.push(Binding {
        id: BEAT_PARAM.to_string(),
        expr: num(0.0),
    });
    lowerer.scalars.insert(BEAT_PARAM.to_string());
}

/// How far through its move beat `k` is, from 0 to 1. `beat` is whole while a figure holds,
/// so a pause changes nothing, and between two whole numbers while it moves.
fn progress(k: usize) -> Expr {
    Expr::app(
        "Clamp",
        vec![
            sub(Expr::sym(BEAT_PARAM), num(k as f64)),
            num(0.0),
            num(1.0),
        ],
    )
}

/// Times the beats from their lengths, and works out from them when each mark is there and how
/// much it's in focus. Everything is in terms of `beat`, so a reader stepping through and a
/// video playing it see the same thing.
fn finish_story(scene: &mut Scene, story: &Story) -> Result<(), Error> {
    let count = story.moves.len();
    if let Some(param) = scene.params.iter_mut().find(|p| p.id == BEAT_PARAM) {
        param.range = [0.0, count as f64];
    }
    let mut now = 0.0;
    let mut windows = vec![Expr::sym("time")];
    for step in &story.steps {
        match step {
            Step::Pause(length) => now += length,
            Step::Move(length) => {
                windows.push(num(now));
                now += length;
                windows.push(num(now));
                scene.beat_lengths.push(*length);
            }
        }
    }
    if let Some(binding) = scene.bindings.iter_mut().find(|b| b.id == BEAT_PARAM) {
        binding.expr = Expr::app("Timeline", windows);
    }
    match scene.duration {
        None => scene.duration = Some(now),
        Some(d) if d + 1e-9 < now => {
            return Err(Error::new(format!(
                "the beats and pauses take {now} seconds, more than the `duration` of {d}"
            )));
        }
        Some(_) => {}
    }

    let marks = &scene.marks;
    let find = |word: &str, clause: &str, line: usize| -> Result<Vec<usize>, Error> {
        let found: Vec<usize> = match word.strip_prefix('.') {
            Some(tag) => (0..marks.len())
                .filter(|&i| marks[i].tags.iter().any(|t| t == tag))
                .collect(),
            None => (0..marks.len()).filter(|&i| marks[i].id == word).collect(),
        };
        if found.is_empty() {
            let what = match word.strip_prefix('.') {
                Some(tag) => format!("no mark is tagged `{tag}`"),
                None => format!("no mark is named `{word}`"),
            };
            return Err(Error::at(line, 1, format!("`{clause} {word}`: {what}")));
        }
        Ok(found)
    };

    // When each mark arrives and leaves, in beat order.
    let mut events: Vec<Vec<(usize, bool, usize)>> = vec![Vec::new(); marks.len()];
    for (k, range) in &story.blocks {
        for i in range.clone() {
            events[i].push((*k, true, story.moves[*k].line));
        }
    }
    for (k, written) in story.moves.iter().enumerate() {
        for word in &written.show {
            for i in find(word, "show", written.line)? {
                events[i].push((k, true, written.line));
            }
        }
        for word in &written.drop {
            for i in find(word, "drop", written.line)? {
                events[i].push((k, false, written.line));
            }
        }
    }
    let mut lives = Vec::with_capacity(marks.len());
    for (i, mut list) in events.into_iter().enumerate() {
        if list.is_empty() {
            lives.push(None);
            continue;
        }
        list.sort_by_key(|e| e.0);
        // A mark that's ever shown isn't there until it is.
        let mut there = !list.iter().any(|e| e.1);
        let mut life = num(if there { 1.0 } else { 0.0 });
        let mut last = None;
        for (k, show, line) in list {
            let id = &marks[i].id;
            if last == Some(k) {
                return Err(Error::at(
                    line,
                    1,
                    format!("`{id}` arrives or leaves twice in one beat"),
                ));
            }
            if show == there {
                let doing = if show {
                    "shows `{id}`, which is already there"
                } else {
                    "drops `{id}`, which isn't there"
                };
                return Err(Error::at(
                    line,
                    1,
                    format!("this beat {}", doing.replace("{id}", id)),
                ));
            }
            life = if show {
                add(life, progress(k))
            } else {
                sub(life, progress(k))
            };
            there = show;
            last = Some(k);
        }
        lives.push(Some(life));
    }

    // Focus holds until a beat names another, and starts on everything.
    let mut focuses: Vec<Option<Expr>> = vec![None; marks.len()];
    let mut was = vec![1.0; marks.len()];
    for (k, written) in story.moves.iter().enumerate() {
        if written.focus.is_empty() {
            continue;
        }
        let mut now = vec![0.0; marks.len()];
        for word in &written.focus {
            for i in find(word, "focus", written.line)? {
                now[i] = 1.0;
            }
        }
        for i in 0..marks.len() {
            if now[i] != was[i] {
                let change = mul(num(now[i] - was[i]), progress(k));
                focuses[i] = Some(match focuses[i].take() {
                    Some(f) => add(f, change),
                    None => add(num(1.0), change),
                });
            }
        }
        was = now;
    }

    for ((mark, life), focus) in scene.marks.iter_mut().zip(lives).zip(focuses) {
        mark.life = life;
        mark.focus = focus;
    }
    Ok(())
}

/// A value that holds for the whole figure. Evaluating it at time 0 would quietly freeze
/// anything that reads a signal, so that's an error instead.
pub fn constant(e: &Expr, what: &str) -> Result<f64, Error> {
    let mut signal = None;
    e.visit_syms(&mut |s| {
        if SIGNALS.contains(&s) {
            signal = Some(s.to_string());
        }
    });
    if let Some(s) = signal {
        return Err(Error::new(format!(
            "{what} must be constant, and it uses `{s}`"
        )));
    }
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
        curvature: None,
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
        curvature: Some(div(num(1.0), Expr::sym(format!("{name}.r")))),
        line: Some(Arc {
            var: var.to_string(),
            point: [around("x", "Cos"), around("y", "Sin")],
            range: [num(0.0), Expr::sym("tau")],
        }),
    })
}

/// An oriented circle's exact signed distance, which a line needs no special case for, since
/// nothing here divides by `a`. Its gradient is a unit vector.
fn cycle_form(name: &str) -> Form {
    let f = |field: &str| Expr::sym(format!("{name}.{field}"));
    let (x, y) = (Expr::sym("x"), Expr::sym("y"));
    let (a, bx, by, c) = (f("a"), f("bx"), f("by"), f("c"));
    let value = add(
        sub(
            mul(a.clone(), add(square(x.clone()), square(y.clone()))),
            mul(
                num(2.0),
                add(mul(bx.clone(), x.clone()), mul(by.clone(), y.clone())),
            ),
        ),
        c,
    );
    let inner = add(num(1.0), mul(a.clone(), value.clone()));
    let field = div(
        value,
        add(
            num(1.0),
            sqrt(Expr::app("Max", vec![inner.clone(), num(0.0)])),
        ),
    );
    // Its center has no direction, and this keeps that from dividing by 0.
    let root = sqrt(Expr::app("Max", vec![inner, num(1e-12)]));
    let grad = [
        div(sub(mul(a.clone(), x), bx), root.clone()),
        div(sub(mul(a.clone(), y), by), root),
    ];
    let var = "t";
    let around = |center: &str, wave: &str| {
        add(
            f(center),
            mul(f("r"), Expr::app(wave, vec![Expr::sym(var)])),
        )
    };
    Form::Field {
        vars: ["x".to_string(), "y".to_string()],
        field,
        grad,
        exact: true,
        curvature: Some(a),
        line: Some(Arc {
            var: var.to_string(),
            point: [around("x", "Cos"), around("y", "Sin")],
            range: [num(0.0), Expr::sym("tau")],
        }),
    }
}

/// Below this curvature, an oriented circle has no center or radius of its own.
const STRAIGHT: f64 = 1e-12;
fn cycle_center(a: &Expr, b: &Expr) -> Expr {
    Expr::app(
        "Where",
        vec![
            sub(square(a.clone()), num(STRAIGHT * STRAIGHT)),
            div(b.clone(), a.clone()),
        ],
    )
}
fn cycle_radius(a: &Expr) -> Expr {
    Expr::app(
        "Where",
        vec![
            sub(square(a.clone()), num(STRAIGHT * STRAIGHT)),
            div(num(1.0), Expr::app("Abs", vec![a.clone()])),
        ],
    )
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

/// One view of a document, as its own scene.
pub struct PanelScene {
    pub id: String,
    pub title: Option<String>,
    pub formula: Option<String>,
    pub scene: Scene,
}

/// A name, with a prefix if a group declared it.
fn rename_name(name: &str, names: &HashMap<String, String>) -> String {
    names.get(name).cloned().unwrap_or_else(|| name.to_string())
}

/// An expression, with the names a group declared given its prefix. `p.x` follows `p`.
fn rename(e: &Expr, names: &HashMap<String, String>) -> Expr {
    match e {
        Expr::Num(_) => e.clone(),
        Expr::Sym(s) => match s.split_once('.') {
            Some((base, field)) if names.contains_key(base) => {
                Expr::sym(format!("{}.{field}", names[base]))
            }
            _ => Expr::sym(rename_name(s, names)),
        },
        Expr::App(head, args) => Expr::App(
            rename_name(head, names),
            args.iter().map(|a| rename(a, names)).collect(),
        ),
    }
}

/// `use g(…) as up`: what the group declares becomes `up_name`, and each mark is tagged with
/// its plain name and `up`, so a style can reach all the copies of something, or one set.
fn prefixed(items: &[Item], prefix: &str, out: &mut Vec<Item>, line: usize) -> Result<(), Error> {
    let mut names = HashMap::new();
    for item in items {
        let declared: Vec<&String> = match item {
            Item::Let { name, .. }
            | Item::Fn { name, .. }
            | Item::Curve { name, .. }
            | Item::Segment { name, .. }
            | Item::Arrow { name, .. }
            | Item::Label { name, .. }
            | Item::ArcSegment { name, .. }
            | Item::Many { name, .. } => vec![name],
            Item::Solve { unknowns, .. } => unknowns.iter().collect(),
            _ => Vec::new(),
        };
        for name in declared {
            names.insert(name.clone(), format!("{prefix}_{name}"));
        }
    }
    renamed_items(items, &names, Some(prefix), out, line)
}

/// Statements, with these names changed throughout.
fn renamed_items(
    items: &[Item],
    names: &HashMap<String, String>,
    prefix: Option<&str>,
    out: &mut Vec<Item>,
    line: usize,
) -> Result<(), Error> {
    let without = |vars: &[&String]| {
        let mut local = names.clone();
        for var in vars {
            local.remove(*var);
        }
        local
    };
    for item in items {
        let renamed = |name: &String| rename_name(name, names);
        let (next, mark) = match item.clone() {
            Item::Dots { names: dots, line } => (
                Item::Dots {
                    names: dots.iter().map(renamed).collect(),
                    line,
                },
                None,
            ),
            Item::Let { name, expr, line } => (
                Item::Let {
                    name: renamed(&name),
                    expr: rename(&expr, names),
                    line,
                },
                None,
            ),
            Item::Fn {
                name,
                params,
                body,
                line,
            } => {
                let local = without(&params.iter().collect::<Vec<_>>());
                (
                    Item::Fn {
                        name: renamed(&name),
                        params,
                        body: rename(&body, &local),
                        line,
                    },
                    None,
                )
            }
            Item::Curve { name, form, line } => {
                let form = match form {
                    CurveSyntax::Parametric { var, point, range } => {
                        let local = without(&[&var]);
                        CurveSyntax::Parametric {
                            point: rename(&point, &local),
                            range: [rename(&range[0], names), rename(&range[1], names)],
                            var,
                        }
                    }
                    CurveSyntax::Value(e) => CurveSyntax::Value(rename(&e, names)),
                    _ => {
                        return Err(Error::at(
                            line,
                            1,
                            "a group that takes arguments, or `use … as`, can hold parametric curves and constructions for now",
                        ));
                    }
                };
                let tag = name.clone();
                (
                    Item::Curve {
                        name: renamed(&name),
                        form,
                        line,
                    },
                    Some(tag),
                )
            }
            Item::Segment {
                name,
                points,
                closed,
                line,
            } => {
                let tag = name.clone();
                (
                    Item::Segment {
                        name: renamed(&name),
                        points: points.iter().map(|p| rename(p, names)).collect(),
                        closed,
                        line,
                    },
                    Some(tag),
                )
            }
            Item::Arrow {
                name,
                from,
                to,
                line,
            } => {
                let tag = name.clone();
                (
                    Item::Arrow {
                        name: renamed(&name),
                        from: rename(&from, names),
                        to: rename(&to, names),
                        line,
                    },
                    Some(tag),
                )
            }
            Item::Label {
                name,
                text,
                at,
                offset,
                away,
                line,
            } => {
                let tag = name.clone();
                (
                    Item::Label {
                        name: renamed(&name),
                        text,
                        at: rename(&at, names),
                        offset: offset.as_ref().map(|o| rename(o, names)),
                        away: away.as_ref().map(|d| rename(d, names)),
                        line,
                    },
                    Some(tag),
                )
            }
            Item::Many {
                name,
                kind,
                value,
                param,
                vars,
                lets,
                line,
            } => {
                let mut bound: Vec<&String> = vars.iter().map(|(v, _)| v).collect();
                bound.extend(lets.iter().map(|(v, _)| v));
                let local = without(&bound);
                // A curve's parameter is only its own, in its point.
                let param = param.map(|(var, range)| {
                    (var, [rename(&range[0], &local), rename(&range[1], &local)])
                });
                let mut own = bound.clone();
                own.extend(param.iter().map(|(var, _)| var));
                let value = rename(&value, &without(&own));
                let tag = name.clone();
                (
                    Item::Many {
                        name: renamed(&name),
                        kind,
                        value,
                        param,
                        vars: vars
                            .iter()
                            .map(|(v, r)| {
                                (v.clone(), [rename(&r[0], &local), rename(&r[1], &local)])
                            })
                            .collect(),
                        lets: lets
                            .iter()
                            .map(|(v, e)| (v.clone(), rename(e, &local)))
                            .collect(),
                        line,
                    },
                    Some(tag),
                )
            }
            Item::Text { name, text, line } => (
                Item::Text {
                    name: renamed(&name),
                    text,
                    line,
                },
                None,
            ),
            Item::DotAt { name, at, line } => {
                let tag = name.clone();
                (
                    // A dot has its point's name, so its own name can't be renamed through `at`.
                    Item::DotAt {
                        name: match prefix {
                            Some(prefix) => format!("{prefix}_{name}"),
                            None => renamed(&name),
                        },
                        at: rename(&at, names),
                        line,
                    },
                    Some(tag),
                )
            }
            Item::ArcSegment {
                name,
                var,
                point,
                line,
            } => {
                let tag = name.clone();
                let local = without(&[&var]);
                (
                    Item::ArcSegment {
                        name: renamed(&name),
                        point: rename(&point, &local),
                        var,
                        line,
                    },
                    Some(tag),
                )
            }
            tags @ Item::Tag(_) => (tags, None),
            _ => {
                return Err(Error::at(
                    line,
                    1,
                    "a group that takes arguments, or `use … as`, can hold lets, functions, curves, dots, segments, arrows, labels and collections for now",
                ));
            }
        };
        out.push(next);
        if let (Some(tag), Some(prefix)) = (mark, prefix) {
            out.push(Item::Tag(vec![tag, prefix.to_string()]));
        }
    }
    Ok(())
}

/// A variable of ours, which the `#` keeps from ever meeting a name someone wrote.
const SEGMENT_VAR: &str = "segment#t";
/// `pushed#(map, at, direction)`: where a direction at a point goes through a map.
const PUSHED: &str = "pushed#";
const STEP: &str = "step#";

/// What a group declares, drawn through a function. Everything is written in the group's own
/// coordinates, and only marks move: its `let`s stay as they were written. This is what a
/// thing being the same thing in every panel comes down to: one declaration, through a map.
fn through(items: Vec<Item>, map: &str) -> Result<Vec<Item>, Error> {
    let image = |e: Expr| Expr::app(map, vec![e]);
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Item::Let { .. }
            | Item::Fn { .. }
            | Item::Solve { .. }
            | Item::Tag(_)
            | Item::Text { .. } => out.push(item),
            Item::Curve {
                name,
                form: CurveSyntax::Parametric { var, point, range },
                line,
            } => out.push(Item::Curve {
                name,
                form: CurveSyntax::Parametric {
                    var,
                    point: image(point),
                    range,
                },
                line,
            }),
            Item::Curve { name, line, .. } => {
                return Err(Error::at(
                    line,
                    1,
                    format!(
                        "`{name}` can't go through `{map}`: the image of a construction isn't one, so write it as a parametric curve"
                    ),
                ));
            }
            Item::Segment {
                name,
                points,
                closed,
                line,
            } => {
                let [a, b] = points.as_slice() else {
                    return Err(Error::at(
                        line,
                        1,
                        format!("`{name}` can't go through `{map}`: only two points can, for now"),
                    ));
                };
                if closed {
                    return Err(Error::at(
                        line,
                        1,
                        format!("`{name}` can't go through `{map}` closed, for now"),
                    ));
                }
                out.push(Item::ArcSegment {
                    name,
                    var: SEGMENT_VAR.to_string(),
                    point: image(add(
                        a.clone(),
                        mul(Expr::sym(SEGMENT_VAR), sub(b.clone(), a.clone())),
                    )),
                    line,
                });
            }
            Item::ArcSegment {
                name,
                var,
                point,
                line,
            } => out.push(Item::ArcSegment {
                name,
                var,
                point: image(point),
                line,
            }),
            Item::Dots { names, line } => {
                for name in names {
                    let at = image(Expr::sym(&name));
                    out.push(Item::DotAt { name, at, line });
                }
            }
            Item::DotAt { name, at, line } => out.push(Item::DotAt {
                name,
                at: image(at),
                line,
            }),
            Item::Label {
                name,
                text,
                at,
                offset,
                away,
                line,
            } => out.push(Item::Label {
                name,
                text,
                away: away.map(|d| Expr::app(PUSHED, vec![Expr::sym(map), at.clone(), d])),
                at: image(at),
                offset,
                line,
            }),
            Item::Many {
                name,
                kind: kind @ (ManyKind::Dots | ManyKind::Curves),
                value,
                param,
                vars,
                lets,
                line,
            } => out.push(Item::Many {
                name,
                kind,
                value: image(value),
                param,
                vars,
                lets,
                line,
            }),
            Item::Many {
                name,
                kind: ManyKind::Circles,
                line,
                ..
            } => {
                return Err(Error::at(
                    line,
                    1,
                    format!(
                        "`{name}` can't go through `{map}`: the image of a circle isn't a circle, so map its center"
                    ),
                ));
            }
            _ => {
                return Err(Error::new(format!(
                    "a group drawn through `{map}` can hold lets, functions, parametric curves, segments, dots, labels, and `dots` and `curves` collections for now"
                )));
            }
        }
    }
    Ok(out)
}

/// Replaces each `use` with its group's statements, and passes a group's parameters by
/// defining them first.
fn expanded(items: &[Item], doc: &Document, out: &mut Vec<Item>) -> Result<(), Error> {
    for item in items {
        match item {
            Item::Panel { .. } | Item::Group { .. } => {}
            Item::Use {
                name,
                args,
                through: map,
                alias,
                line,
            } => {
                let Some((params, inner)) = doc.items.iter().find_map(|i| match i {
                    Item::Group {
                        name: n,
                        params,
                        items,
                    } if n == name => Some((params, items)),
                    _ => None,
                }) else {
                    return Err(Error::at(*line, 1, format!("no group named `{name}`")));
                };
                if params.len() != args.len() {
                    return Err(Error::at(
                        *line,
                        1,
                        format!(
                            "`{name}` takes {} arguments, found {}",
                            params.len(),
                            args.len()
                        ),
                    ));
                }
                // A group's parameters are its own, so two groups can both have a `k`. The
                // `#` keeps these from ever meeting a name someone wrote.
                let mut body = Vec::new();
                let mut scoped = HashMap::new();
                for (param, arg) in params.iter().zip(args) {
                    let own = format!("{param}#{}", out.len());
                    scoped.insert(param.clone(), own.clone());
                    body.push(Item::Let {
                        name: own,
                        expr: arg.clone(),
                        line: *line,
                    });
                }
                let mut statements = Vec::new();
                expanded(inner, doc, &mut statements)?;
                if scoped.is_empty() {
                    body.append(&mut statements);
                } else {
                    renamed_items(&statements, &scoped, None, &mut body, *line)?;
                }
                if let Some(map) = map {
                    body = through(body, map)?;
                }
                match alias {
                    Some(prefix) => prefixed(&body, prefix, out, *line)?,
                    None => out.append(&mut body),
                }
            }
            other => out.push(other.clone()),
        }
    }
    Ok(())
}

/// Gives each label its text in this panel. One with none is a thing this space has no name
/// for, so it isn't drawn, and the tags written after it go with it.
fn with_texts(items: &[Item]) -> Result<Vec<Item>, Error> {
    let texts: HashMap<&str, (&str, usize)> = items
        .iter()
        .filter_map(|item| match item {
            Item::Text { name, text, line } => Some((name.as_str(), (text.as_str(), *line))),
            _ => None,
        })
        .collect();
    for (name, (_, line)) in &texts {
        let found = items
            .iter()
            .any(|item| matches!(item, Item::Label { name: n, .. } if n == name));
        if !found {
            return Err(Error::at(*line, 1, format!("no label named `{name}`")));
        }
    }
    let mut out = Vec::with_capacity(items.len());
    let mut dropping = false;
    for item in items {
        match item {
            Item::Text { .. } => continue,
            Item::Tag(_) if dropping => continue,
            Item::Label {
                name,
                text,
                at,
                offset,
                away,
                line,
            } => {
                let text = texts
                    .get(name.as_str())
                    .map(|(text, _)| text.to_string())
                    .or_else(|| text.clone());
                dropping = text.is_none();
                if !dropping {
                    out.push(Item::Label {
                        name: name.clone(),
                        text,
                        at: at.clone(),
                        offset: offset.clone(),
                        away: away.clone(),
                        line: *line,
                    });
                }
                continue;
            }
            _ => {}
        }
        dropping = false;
        out.push(item.clone());
    }
    Ok(out)
}

/// A document with no panels: one scene.
pub fn compile(doc: &Document) -> Result<Scene, Error> {
    if doc.items.iter().any(|i| matches!(i, Item::Panel { .. })) {
        return Err(Error::new("this document has panels, so it's many scenes"));
    }
    let mut items = Vec::new();
    expanded(&doc.items, doc, &mut items)?;
    compile_flat(doc.name.clone(), &items)
}

/// A document with panels: a scene for each, from what's shared and what's its own.
/// Each has its own shader and its own view, and all of them see the same bindings.
pub fn compile_panels(doc: &Document) -> Result<Vec<PanelScene>, Error> {
    let mut scenes = Vec::new();
    for item in &doc.items {
        let Item::Panel {
            name,
            title,
            formula,
            items,
        } = item
        else {
            continue;
        };
        let mut flat = Vec::new();
        expanded(&doc.items, doc, &mut flat)?;
        expanded(items, doc, &mut flat)?;
        let mut scene = compile_flat(doc.name.clone(), &flat)?;
        for mark in &mut scene.marks {
            mark.tags.push(name.clone());
        }
        scenes.push(PanelScene {
            id: name.clone(),
            title: title.clone(),
            formula: formula.clone(),
            scene,
        });
    }
    fill_outlines(scenes.iter_mut().map(|p| (&p.id, &mut p.scene)))?;
    Ok(scenes)
}

/// Puts bindings in the order they have to be worked out in. What's written in order stays in
/// order, so this only moves what has to move. A name has to be written before it's read, but a
/// `bind` runs where it's needed, not where its `param` is, so `bind b = grown` can read a
/// `let grown` written after `param b`.
fn in_dependency_order(scene: &mut Scene) -> Result<(), Error> {
    let known: HashSet<String> = scene.bindings.iter().map(|b| b.id.clone()).collect();
    let needs: Vec<Vec<String>> = scene
        .bindings
        .iter()
        .map(|binding| {
            let mut found = Vec::new();
            binding.expr.visit_syms(&mut |name| {
                if known.contains(name) && name != binding.id {
                    found.push(name.to_string());
                }
            });
            if let Expr::App(head, args) = &binding.expr
                && head == "Solved"
                && let Some(Expr::Num(system)) = args.first()
            {
                for name in &scene.systems[*system as usize].depends {
                    if known.contains(name) {
                        found.push(name.clone());
                    }
                }
            }
            found
        })
        .collect();
    let mut done: HashSet<String> = HashSet::with_capacity(known.len());
    let mut order = Vec::with_capacity(needs.len());
    let mut waiting: Vec<usize> = (0..needs.len()).collect();
    while !waiting.is_empty() {
        let before = waiting.len();
        waiting.retain(|&i| {
            let ready = needs[i].iter().all(|name| done.contains(name));
            if ready {
                done.insert(scene.bindings[i].id.clone());
                order.push(i);
            }
            !ready
        });
        if waiting.len() == before {
            let names: Vec<&str> = waiting
                .iter()
                .map(|&i| scene.bindings[i].id.as_str())
                .collect();
            return Err(Error::new(format!(
                "these are defined in terms of each other: {}",
                names.join(", ")
            )));
        }
    }
    let mut old: Vec<Option<Binding>> = std::mem::take(&mut scene.bindings)
        .into_iter()
        .map(Some)
        .collect();
    scene.bindings = order
        .into_iter()
        .map(|i| old[i].take().expect("each once"))
        .collect();
    Ok(())
}

/// An outline shows another panel's view, so it can only be filled in once they all exist,
/// and again once a style has settled what each shows.
pub fn fill_outlines<'a>(
    panels: impl Iterator<Item = (&'a String, &'a mut Scene)>,
) -> Result<(), Error> {
    let mut panels: Vec<_> = panels.collect();
    let views: Vec<(String, Viewport)> = panels
        .iter()
        .map(|(id, scene)| ((*id).clone(), scene.viewport.clone()))
        .collect();
    for (_, scene) in &mut panels {
        for mark in &mut scene.marks {
            if let Form::Outline {
                panel: target,
                view,
            } = &mut mark.form
            {
                let Some((_, found)) = views.iter().find(|(id, _)| id == target) else {
                    return Err(Error::new(format!("no panel named `{target}` to outline")));
                };
                *view = Some(Box::new(found.clone()));
            }
        }
    }
    Ok(())
}

fn compile_flat(name: Option<String>, items: &[Item]) -> Result<Scene, Error> {
    let items = &with_texts(items)?;
    let mut lowerer = Lowerer::default();
    let mut scene = Scene {
        name,
        params: Vec::new(),
        bindings: Vec::new(),
        marks: Vec::new(),
        viewport: Viewport::default(),
        duration: None,
        beats: Vec::new(),
        beat_lengths: Vec::new(),
        handles: Vec::new(),
        spots: Vec::new(),
        expressions: Vec::new(),
        systems: Vec::new(),
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
    // Beats with lengths, in order with the pauses between them.
    let mut story = Story::default();
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
                life: None,
                focus: None,
            });
        }};
    }

    for item in items {
        let marks_before = scene.marks.len();
        match item {
            Item::Tag(tags) => {
                for mark in &mut scene.marks[last_marks.clone()] {
                    mark.tags.extend(tags.iter().cloned());
                }
                continue;
            }
            Item::Move(written) => {
                if !scene.beats.is_empty() && story.moves.is_empty() {
                    return Err(Error::at(
                        written.line,
                        1,
                        "a beat with a length can't follow one without: give them all lengths",
                    ));
                }
                if scene.beats.is_empty() {
                    claim(BEAT_PARAM, 0)?;
                    declare_beat(&mut scene, &mut lowerer);
                }
                let k = story.moves.len();
                let length =
                    constant(&written.length, "a beat's length").map_err(at_line(written.line))?;
                if length < 0.0 {
                    return Err(Error::at(
                        written.line,
                        1,
                        "a beat can't take less than no time",
                    ));
                }
                if let Some(name) = &written.name {
                    claim(name, written.line)?;
                    lowerer.bind(&mut scene, name, Val::Scalar(progress(k)));
                }
                // A beat with no caption keeps the one before it, and `""` clears it.
                if let Some(caption) = &written.caption {
                    story.caption = caption.clone();
                }
                scene.beats.push(story.caption.clone());
                story.steps.push(Step::Move(length));
                story.moves.push(written.clone());
                if written.block {
                    beat = Some(k);
                    beat_tags = written.tags.clone();
                    story.open = Some((k, scene.marks.len()));
                } else if !written.tags.is_empty() {
                    return Err(Error::at(
                        written.line,
                        1,
                        "a beat's `tag` is for the marks in its block, and it has none",
                    ));
                }
                continue;
            }
            Item::Pause(e) => {
                let length = constant(e, "a pause")?;
                if length < 0.0 {
                    return Err(Error::new("a pause can't take less than no time"));
                }
                story.steps.push(Step::Pause(length));
                continue;
            }
            Item::Beat { caption, tags } => {
                if !story.moves.is_empty() {
                    return Err(Error::new(
                        "a beat without a length can't follow one with: give them all lengths",
                    ));
                }
                if scene.beats.is_empty() {
                    claim(BEAT_PARAM, 0)?;
                    declare_beat(&mut scene, &mut lowerer);
                }
                beat = Some(scene.beats.len());
                scene.beats.push(caption.clone());
                beat_tags = tags.clone();
                continue;
            }
            Item::EndBeat => {
                if let Some((k, start)) = story.open.take() {
                    story.blocks.push((k, start..scene.marks.len()));
                }
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
            Item::Choice {
                name,
                default,
                options,
                line,
            } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let labels: Vec<String> = options.iter().map(|(_, text)| text.clone()).collect();
                let first = match default {
                    None => 0,
                    Some(text) => {
                        let written = |s: &str| s.split_whitespace().collect::<String>();
                        labels
                            .iter()
                            .position(|label| written(label) == written(text))
                            .ok_or_else(|| {
                                Error::at(
                                    *line,
                                    1,
                                    format!(
                                        "`{name}` starts at {text}, which isn't one of its options: {}",
                                        labels.join(", ")
                                    ),
                                )
                            })?
                    }
                };
                let mut values = Vec::new();
                for (option, _) in options {
                    values.push(lowerer.lower(option, &none, 0).map_err(&fail)?);
                }
                // A number is a point on the real axis, so numbers and points can be offered together.
                if values.iter().any(|v| matches!(v, Val::Point(..))) {
                    for v in &mut values {
                        if let Val::Scalar(x) = v {
                            *v = Val::Point(x.clone(), num(0.0));
                        }
                    }
                }
                if let Some(other) = values.iter().find(|v| !v.same_kind(&values[0])) {
                    return Err(Error::at(
                        *line,
                        1,
                        format!(
                            "`{name}` offers {} and {}, and its options have to be of one kind",
                            values[0].kind(),
                            other.kind()
                        ),
                    ));
                }
                // The param is which option, and the name is that option's value.
                let index = format!("{name}.index");
                scene.params.push(Param {
                    id: index.clone(),
                    range: [0.0, (values.len() - 1) as f64],
                    step: Some(1.0),
                    choice: Some(Choice {
                        of: name.clone(),
                        options: labels,
                    }),
                });
                scene.bindings.push(Binding {
                    id: index.clone(),
                    expr: num(first as f64),
                });
                lowerer.scalars.insert(index.clone());
                let which = Expr::sym(&index);
                let mut fields = values[0].fields();
                for (k, value) in values.iter().enumerate().skip(1) {
                    // From option k on, it's that one's.
                    let reached = sub(which.clone(), num(k as f64 - 0.5));
                    fields = fields
                        .into_iter()
                        .zip(value.fields())
                        .map(|(before, here)| Expr::app("IfNonnegative", vec![reached.clone(), here, before]))
                        .collect();
                }
                let chosen = values[0].with_fields(fields);
                lowerer.bind(&mut scene, name, chosen);
            }
            Item::Param {
                name,
                default,
                range,
                step,
                line,
            } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let range = [
                    constant(&range[0], "a param's range").map_err(&fail)?,
                    constant(&range[1], "a param's range").map_err(&fail)?,
                ];
                let step = match step {
                    Some(step) => {
                        let step = constant(step, "a param's step").map_err(&fail)?;
                        if !(step > 0.0 && step.is_finite()) {
                            return Err(Error::at(
                                *line,
                                1,
                                format!("`{name}` steps by {step}, and a step has to be above 0"),
                            ));
                        }
                        Some(step)
                    }
                    None => None,
                };
                let default = lowerer.scalar(default, &none).map_err(&fail)?;
                scene.params.push(Param {
                    id: name.clone(),
                    range,
                    step,
                    choice: None,
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
                            Val::Circle { .. } | Val::Cycle { .. } => (None, "circle"),
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
                            Some(Val::Cycle { .. }) => cycle_form(name),
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
            Item::DotAt { name, at, line } => {
                let p = lowerer.point(at, &none).map_err(at_line(*line))?;
                mark!(name.clone(), Form::Dot(p), "dot");
            }
            Item::ArcSegment {
                name,
                var,
                point,
                line,
            } => {
                claim(name, *line)?;
                let form = lowerer
                    .arc(var, point, &[num(0.0), num(1.0)])
                    .map_err(at_line(*line))?;
                mark!(name.clone(), Form::Parametric(form), "segment");
            }
            Item::Label {
                name,
                text,
                at,
                offset,
                away,
                line,
            } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let at = lowerer.point(at, &none).map_err(&fail)?;
                let offset = match offset {
                    Some(e) => lowerer.point(e, &none).map_err(&fail)?,
                    None => [num(0.0), num(0.0)],
                };
                let away = away
                    .as_ref()
                    .map(|e| lowerer.point(e, &none).map_err(&fail))
                    .transpose()?;
                // `{expr}` in the text is a value, worked out each frame.
                let mut written = String::new();
                let mut values = Vec::new();
                let mut rest = text.as_deref().unwrap_or_default();
                while let Some((before, after)) = rest.split_once('{') {
                    let Some((inner, tail)) = after.split_once('}') else {
                        return Err(fail(Error::new("a `{` in a label's text isn't closed")));
                    };
                    let value = crate::parse::parse_expression(inner).map_err(&fail)?;
                    values.push(lowerer.scalar(&value, &none).map_err(&fail)?);
                    written.push_str(before);
                    written.push_str("{}");
                    rest = tail;
                }
                written.push_str(rest);
                mark!(
                    name.clone(),
                    Form::Label {
                        text: written,
                        values,
                        at,
                        offset,
                        away,
                    },
                    "label"
                );
            }
            Item::Text { .. } | Item::Panel { .. } | Item::Group { .. } | Item::Use { .. } => {
                unreachable!("`expanded` took these out")
            }
            Item::Outline { name, panel, line } => {
                claim(name, *line)?;
                mark!(
                    name.clone(),
                    Form::Outline {
                        panel: panel.clone(),
                        view: None,
                    },
                    "outline"
                );
            }
            Item::Solve {
                unknowns,
                start,
                equations,
                line,
            } => {
                let fail = at_line(*line);
                if unknowns.len() != equations.len() || unknowns.len() != start.len() {
                    return Err(fail(Error::new(format!(
                        "`solve` needs a start and an equation for each unknown, and has {} unknowns, {} starts and {} equations",
                        unknowns.len(),
                        start.len(),
                        equations.len()
                    ))));
                }
                let starts = start
                    .iter()
                    .map(|e| lowerer.scalar(e, &none))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(&fail)?;
                let mut locals = Locals::new();
                for unknown in unknowns {
                    claim(unknown, *line)?;
                    locals.insert(unknown.clone(), Val::Scalar(Expr::sym(unknown)));
                }
                let lowered = equations
                    .iter()
                    .map(|(lhs, rhs)| {
                        Ok(sub(
                            lowerer.scalar(lhs, &locals)?,
                            lowerer.scalar(rhs, &locals)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, Error>>()
                    .map_err(&fail)?;
                // The IR is symbolic, so Newton's method gets exact derivatives.
                let mut jacobian = Vec::with_capacity(unknowns.len() * unknowns.len());
                for equation in &lowered {
                    for unknown in unknowns {
                        jacobian.push(derivative(equation, unknown).map_err(&fail)?);
                    }
                }
                let mut depends = Vec::new();
                for e in starts.iter().chain(&lowered) {
                    e.visit_syms(&mut |name| {
                        if !unknowns.iter().any(|u| u == name)
                            && !depends.iter().any(|d: &String| d == name)
                        {
                            depends.push(name.to_string());
                        }
                    });
                }
                let index = scene.systems.len();
                scene.systems.push(System {
                    unknowns: unknowns.clone(),
                    start: starts,
                    equations: lowered,
                    jacobian,
                    depends,
                });
                for (k, unknown) in unknowns.iter().enumerate() {
                    scene.bindings.push(Binding {
                        id: unknown.clone(),
                        expr: Expr::app("Solved", vec![num(index as f64), num(k as f64)]),
                    });
                    lowerer.scalars.insert(unknown.clone());
                }
            }
            Item::Many {
                name,
                kind,
                value,
                param,
                vars,
                lets,
                line,
            } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                // A later variable's range can use an earlier one.
                let mut locals = Locals::new();
                let mut lowered = Vec::with_capacity(vars.len());
                for (var, range) in vars {
                    lowered.push((
                        var.clone(),
                        [
                            lowerer.scalar(&range[0], &locals).map_err(&fail)?,
                            lowerer.scalar(&range[1], &locals).map_err(&fail)?,
                        ],
                    ));
                    locals.insert(var.clone(), Val::Scalar(Expr::sym(var)));
                }
                let mut named = Vec::with_capacity(lets.len());
                for (local, e) in lets {
                    match lowerer.lower(e, &locals, 0).map_err(&fail)? {
                        Val::Scalar(e) => {
                            named.push((local.clone(), e));
                            locals.insert(local.clone(), Val::Scalar(Expr::sym(local)));
                        }
                        Val::Point(x, y) => {
                            let (fx, fy) = (format!("{local}.x"), format!("{local}.y"));
                            named.push((fx.clone(), x));
                            named.push((fy.clone(), y));
                            locals.insert(local.clone(), Val::Point(Expr::sym(fx), Expr::sym(fy)));
                        }
                        other => {
                            return Err(fail(Error::new(format!(
                                "`with` names a number or a point, and `{local}` is {}",
                                other.kind()
                            ))));
                        }
                    }
                }
                let curve = param.as_ref().map(|(var, range)| {
                    // Its range can read what counts the curves, but not its own parameter.
                    let range = [
                        lowerer.scalar(&range[0], &locals)?,
                        lowerer.scalar(&range[1], &locals)?,
                    ];
                    let mut own = locals.clone();
                    own.insert(var.clone(), Val::Scalar(Expr::sym(var)));
                    Ok::<_, Error>(Arc {
                        var: var.clone(),
                        point: lowerer.point(value, &own)?,
                        range,
                    })
                });
                let lowered_value = match curve {
                    Some(_) => Val::Scalar(num(0.0)),
                    None => lowerer.lower(value, &locals, 0).map_err(&fail)?,
                };
                let curve = curve.transpose().map_err(&fail)?;
                let (shape, tag) = match (kind, lowered_value) {
                    (ManyKind::Curves, _) => (
                        crate::ir::Shape::Curves(
                            curve.expect("the parser gives `curves` a parameter"),
                        ),
                        "curves",
                    ),
                    // Named, so a style can size a dot by where it is, wherever it was mapped to.
                    (ManyKind::Dots, Val::Point(x, y)) => {
                        named.push(("self.x".to_string(), x));
                        named.push(("self.y".to_string(), y));
                        (
                            crate::ir::Shape::Dots([Expr::sym("self.x"), Expr::sym("self.y")]),
                            "dots",
                        )
                    }
                    (ManyKind::Circles, Val::Circle { x, y, r }) => {
                        (crate::ir::Shape::Circles { center: [x, y], r }, "circles")
                    }
                    (ManyKind::Circles, Val::Cycle { a, bx, by, c }) => {
                        (crate::ir::Shape::Cycles { a, b: [bx, by], c }, "circles")
                    }
                    (ManyKind::Dots, Val::Scalar(x)) => {
                        named.push(("self.x".to_string(), x));
                        named.push(("self.y".to_string(), num(0.0)));
                        (
                            crate::ir::Shape::Dots([Expr::sym("self.x"), Expr::sym("self.y")]),
                            "dots",
                        )
                    }
                    (ManyKind::Dots, other) => {
                        return Err(fail(Error::new(format!(
                            "`dots` needs a point, found {}",
                            other.kind()
                        ))));
                    }
                    (ManyKind::Circles, other) => {
                        return Err(fail(Error::new(format!(
                            "`circles` needs a circle, found {}",
                            other.kind()
                        ))));
                    }
                };
                mark!(
                    name.clone(),
                    Form::Many {
                        vars: lowered,
                        lets: named,
                        shape,
                    },
                    tag
                );
            }
            Item::Place {
                name,
                at,
                offset,
                hang,
                line,
            } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let at = lowerer.point(at, &none).map_err(&fail)?;
                let offset = match offset {
                    Some(e) => lowerer.point(e, &none).map_err(&fail)?,
                    None => [num(0.0), num(0.0)],
                };
                let hang = match hang.as_deref() {
                    None | Some("start") => Hang::Start,
                    Some("middle") => Hang::Middle,
                    Some("end") => Hang::End,
                    Some(other) => {
                        return Err(fail(Error::new(format!(
                            "`hang` is `start`, `middle` or `end`, not `{other}`"
                        ))));
                    }
                };
                scene.spots.push(Spot {
                    id: name.clone(),
                    at,
                    offset,
                    hang,
                });
            }
            Item::Expression {
                name,
                by,
                states,
                line,
            } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let by = lowerer.scalar(by, &none).map_err(&fail)?;
                let index = scene.expressions.len();
                let mut written = Vec::with_capacity(states.len());
                let mut seen: Vec<String> = Vec::new();
                // `slow:2ab` is a term named `slow` that prints `2ab`. The name is what it is,
                // so it's what moves between states and what a style or a beat can name.
                let mut printed: HashMap<String, String> = HashMap::new();
                for state in states {
                    let mut pieces = Vec::with_capacity(state.len());
                    let mut in_state: Vec<String> = Vec::new();
                    for (text, place) in state {
                        let Some(spot) = scene.spots.iter().position(|s| s.id == *place) else {
                            return Err(fail(Error::new(format!(
                                "no place named `{place}`. Places come first"
                            ))));
                        };
                        let mut keys: Vec<String> = Vec::new();
                        for word in text.split_whitespace() {
                            let (key, shown) = match word.split_once(':') {
                                Some((key, shown)) if !key.is_empty() && !shown.is_empty() => {
                                    (key, shown)
                                }
                                _ => (word, crate::terms::base(word)),
                            };
                            if let Some(before) = printed.get(key)
                                && before != shown
                            {
                                return Err(fail(Error::new(format!(
                                    "the term `{key}` prints `{before}` in one state of `{name}` and `{shown}` in another. A term is one thing, so give the other a name of its own"
                                ))));
                            }
                            printed.insert(key.to_string(), shown.to_string());
                            keys.push(key.to_string());
                        }
                        for key in keys.iter().map(String::as_str) {
                            // A term is one thing, so a second one in a state would take the
                            // first's place, and one of them would quietly vanish.
                            if in_state.iter().any(|k| k == key) {
                                let written = |c: &str| {
                                    in_state.iter().any(|k| k == c) || keys.iter().any(|k| k == c)
                                };
                                let copy = (1..)
                                    .map(|k| format!("{}.{k}", crate::terms::base(key)))
                                    .find(|c| !written(c))
                                    .expect("an unused copy");
                                let fix = if crate::terms::base(&copy) == copy {
                                    format!(
                                        ", and `{copy}` would be a number, so a number can't have a copy yet"
                                    )
                                } else {
                                    format!(". Write the second as `{copy}`")
                                };
                                return Err(fail(Error::new(format!(
                                    "`{key}` is written twice in one state of `{name}`, and a term \
                                     can only be in one place{fix}"
                                ))));
                            }
                            in_state.push(key.to_string());
                        }
                        for key in &keys {
                            if !seen.contains(key) {
                                seen.push(key.clone());
                            }
                        }
                        pieces.push(Piece { spot, keys });
                    }
                    written.push(pieces);
                }
                scene.expressions.push(Expression {
                    id: name.clone(),
                    by,
                    states: written,
                });
                for key in seen {
                    let text = printed[&key].clone();
                    let mut tags = vec!["term".to_string(), name.clone(), text.clone()];
                    // A named term can be styled and focused by its name, with what it names.
                    let named = crate::terms::base(&key);
                    if named != text {
                        tags.push(named.to_string());
                    }
                    tags.extend(beat_tags.iter().cloned());
                    scene.marks.push(Mark {
                        id: format!("{name}.{key}"),
                        form: Form::Term {
                            expression: index,
                            key,
                            text,
                        },
                        tags,
                        beat,
                        life: None,
                        focus: None,
                    });
                }
            }
            Item::Arrow {
                name,
                from,
                to,
                line,
            } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let from = lowerer.point(from, &none).map_err(&fail)?;
                let to = lowerer.point(to, &none).map_err(&fail)?;
                mark!(name.clone(), Form::Arrow { from, to }, "arrow");
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
            Item::Viewport { x, y, fit, pad } => {
                let bound =
                    |range: &Option<[Expr; 2]>, fallback: [f64; 2]| -> Result<[f64; 2], Error> {
                        match range {
                            Some(r) => {
                                // A fixed window can't follow anything, and `fit` can.
                                let bound = |e| {
                                    constant(e, "a viewport bound").map_err(|mut err| {
                                        err.message += ". To follow something that moves, `fit` it";
                                        err
                                    })
                                };
                                Ok([bound(&r[0])?, bound(&r[1])?])
                            }
                            None => Ok(fallback),
                        }
                    };
                let optional = |e: &Option<Expr>, what: &str| -> Result<Option<f64>, Error> {
                    e.as_ref().map(|e| constant(e, what)).transpose()
                };
                scene.viewport = Viewport {
                    x: bound(x, [-2.0, 2.0])?,
                    y: bound(y, [-2.0, 2.0])?,
                    // A sheet's `style figure` sets these, or a `style panel`.
                    design_width: None,
                    design_height: None,
                    em: crate::ir::DEFAULT_EM,
                    fit: Vec::new(),
                    fit_curves: Vec::new(),
                    fit_families: Vec::new(),
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
        let curve = scene.marks.iter().find_map(|m| match &m.form {
            Form::Parametric(arc) if m.id == name => Some(arc.clone()),
            _ => None,
        });
        if let Some(arc) = curve {
            scene.viewport.fit_curves.push(arc);
            continue;
        }
        let family = scene.marks.iter().find_map(|m| match &m.form {
            Form::Many {
                vars,
                lets,
                shape: crate::ir::Shape::Curves(arc),
            } if m.id == name => Some(crate::ir::Family {
                vars: vars.clone(),
                lets: lets.clone(),
                arc: arc.clone(),
            }),
            _ => None,
        });
        if let Some(family) = family {
            scene.viewport.fit_families.push(family);
            continue;
        }
        let entry = match lowerer.named(&name) {
            Some(Val::Circle { x, y, r }) => [x, y, r],
            // What doesn't exist has no bounds, so a line doesn't add any.
            Some(Val::Cycle { .. }) => [
                Expr::sym(format!("{name}.x")),
                Expr::sym(format!("{name}.y")),
                Expr::sym(format!("{name}.r")),
            ],
            Some(Val::Point(x, y)) => [x, y, num(0.0)],
            _ => {
                return Err(Error::new(format!(
                    "`fit: {name}` needs a point, circle or parametric curve named `{name}`"
                )));
            }
        };
        scene.viewport.fit.push(entry);
    }
    if !story.moves.is_empty() {
        finish_story(&mut scene, &story)?;
    } else if !scene.beats.is_empty() {
        // Steps with no lengths: `beat` is which one it's on, a whole number.
        let count = scene.beats.len() as f64;
        if let Some(param) = scene.params.iter_mut().find(|p| p.id == BEAT_PARAM) {
            param.range = [0.0, count - 1.0];
        }
        // With a duration, they share it evenly and follow time, so the timing is said once.
        // Without one there's no clock, and a reader steps through them.
        if let Some(duration) = scene.duration {
            let share = duration / count;
            if let Some(binding) = scene.bindings.iter_mut().find(|b| b.id == BEAT_PARAM) {
                binding.expr = Expr::app(
                    "Clamp",
                    vec![
                        Expr::app("Floor", vec![div(Expr::sym("time"), num(share))]),
                        num(0.0),
                        num(count - 1.0),
                    ],
                );
            }
        }
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
    in_dependency_order(&mut scene)?;
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
        eval::scene_env(s, &crate::frame::Inputs::at(time), None).unwrap()
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
    fn a_whole_figure_value_cannot_read_time() {
        // Evaluated once, at time 0, it would stay there without a word.
        let err = scene("viewport { x: -2 - time..2 + time, y: -2..2 }").unwrap_err();
        assert!(err.message.contains("uses `time`"), "{err}");
        assert!(err.message.contains("`fit` it"), "{err}");
        let err = scene("let zoom = 2 + time\nviewport { x: -zoom..zoom, y: -2..2 }").unwrap_err();
        assert!(err.message.contains("must be constant"), "{err}");
        let err = scene("duration time").unwrap_err();
        assert!(err.message.contains("uses `time`"), "{err}");
        let err = scene("param a in 0..time").unwrap_err();
        assert!(err.message.contains("uses `time`"), "{err}");
        assert!(
            scene("viewport { x: -2·pi..2·pi, y: -2..2 } style figure { width: 640px }").is_ok()
        );
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
                Form::Arrow { .. } => "arrow",
                Form::Many { .. } => "many",
                Form::Outline { .. } => "outline",
                Form::Term { .. } => "term",
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
    fn solve_finds_one_unknown() {
        let s = scene(
            "solve x from 1: cos(x) = x
             let twice = 2·x",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        let x = env.get("x").unwrap();
        assert!((x - 0.7390851332151607).abs() < 1e-12, "{x}");
        assert!((env.get("twice").unwrap() - 2.0 * x).abs() < 1e-15);
    }

    /// The unknowns are the right point of two: which one is the start's business.
    #[test]
    fn solve_finds_several_unknowns_from_a_start() {
        let doc = |start: &str| {
            format!(
                "fn circle_eq(x, y) = x² + y²
                 solve (a, b) from {start}: circle_eq(a, b) = 4, a = b"
            )
        };
        let env = env_of(&scene(&doc("1, 2")).unwrap(), 0.0);
        let root = 2f64.sqrt();
        assert!(near(env.get("a").unwrap(), root) && near(env.get("b").unwrap(), root));
        let env = env_of(&scene(&doc("-1, -2")).unwrap(), 0.0);
        assert!(near(env.get("a").unwrap(), -root) && near(env.get("b").unwrap(), -root));
    }

    #[test]
    fn solve_follows_a_param() {
        let s = scene(
            "param c = 2 in 0..9
             solve x from 1: x² = c",
        )
        .unwrap();
        for value in [2.0, 9.0, 0.25] {
            let inputs = crate::frame::Inputs {
                time: 0.0,
                params: vec![("c".to_string(), value)],
            };
            let x = eval::scene_env(&s, &inputs, None)
                .unwrap()
                .get("x")
                .unwrap();
            assert!(near(x, value.sqrt()), "{value}: {x}");
        }
    }

    /// No answer isn't a number: what depends on it just isn't there.
    #[test]
    fn a_system_with_no_solution_is_absent() {
        let s = scene(
            "solve x from 1: x² = -1
             let y = x + 1
             let there = exists(y)",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert!(env.is_absent("x") && env.is_absent("y"));
        assert_eq!(env.get("there"), Some(0.0));
        // Singular at its start, as x = 0 is for x² = 1, it has nowhere to go.
        let s = scene("solve x from 0: x² = 1").unwrap();
        assert!(env_of(&s, 0.0).is_absent("x"));
    }

    #[test]
    fn solve_says_what_is_wrong() {
        let err = scene("solve (a, b) from 1, 2: a = 1").unwrap_err();
        assert!(err.message.contains("for each unknown"), "{err}");
        let err = scene("solve a from 1, 2: a = 1").unwrap_err();
        assert!(err.message.contains("for each unknown"), "{err}");
        let err = scene("let a = 1\nsolve a from 1: a = 2").unwrap_err();
        assert!(err.message.contains("already defined"), "{err}");
        let err = scene("solve a from nope: a = 2").unwrap_err();
        assert!(err.message.contains("unknown name"), "{err}");
        let err = scene("solve a from 1: snap(a) = 0.5").unwrap_err();
        assert!(err.message.contains("can't differentiate `Snap`"), "{err}");
    }

    #[test]
    fn collections_are_one_mark_with_their_own_variables() {
        let s = scene(
            "param n = 3 in 0..9
             fn at(i, j) = (i, j)
             dots d = at(i, j) for i in 1..n, j in 0..i
             circles c = circle(at(i, 0), at(i, 1)) for i in 0..n",
        )
        .unwrap();
        assert_eq!(s.marks.len(), 2);
        assert!(s.marks[0].tags.contains(&"dots".to_string()));
        assert!(s.marks[1].tags.contains(&"circles".to_string()));
        let Form::Many { vars, .. } = &s.marks[0].form else {
            panic!();
        };
        assert_eq!(vars.len(), 2);
        let s = scene("dots d = z·2 for i in 0..3 with z = (i, 0)\ncircles c = circle((0, 0), (k, 0)) for i in 0..3 with k = i + 1").unwrap();
        let Form::Many { lets, .. } = &s.marks[0].form else {
            panic!();
        };
        assert_eq!(
            lets.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            ["z.x", "z.y", "self.x", "self.y"]
        );
        let err =
            scene("dots d = (0, 0) for i in 0..1 with c = circle((0, 0), (1, 0))").unwrap_err();
        assert!(err.message.contains("names a number or a point"), "{err}");
        // Variables only exist in their own collection.
        assert!(scene("dots d = (n, 0) for n in 0..1\nlet x = n").is_err());
        let err = scene("dots d = circle((0, 0), (1, 0)) for i in 0..1").unwrap_err();
        assert!(err.message.contains("needs a point"), "{err}");
        let err = scene("circles c = (i, 0) for i in 0..1").unwrap_err();
        assert!(err.message.contains("needs a circle"), "{err}");
    }

    /// The value of `P` in a figure, as (re, im).
    fn complex_value(expr: &str) -> (f64, f64) {
        let s = scene(&format!(
            "let z = (3, 4)
             let w = (-2, 0.5)
             let t = 0.7
             let P = {expr}"
        ))
        .unwrap_or_else(|e| panic!("{expr}: {e}"));
        let env = env_of(&s, 0.0);
        (env.get("P.x").unwrap(), env.get("P.y").unwrap())
    }

    /// Each worked out here by hand, independently of how the compiler does it.
    #[test]
    fn a_point_is_a_complex_number() {
        let close = |expr: &str, want: (f64, f64)| {
            let got = complex_value(expr);
            assert!(
                (got.0 - want.0).abs() < 1e-12 && (got.1 - want.1).abs() < 1e-12,
                "{expr}: {got:?}, wanted {want:?}"
            );
        };
        close("i", (0.0, 1.0));
        close("i·i", (-1.0, 0.0));
        close("i²", (-1.0, 0.0));
        close("i·z", (-4.0, 3.0));
        close("z·w", (3.0 * -2.0 - 4.0 * 0.5, 3.0 * 0.5 + 4.0 * -2.0));
        close("z·w / w", (3.0, 4.0));
        close("1 / i", (0.0, -1.0));
        close("1 / z", (3.0 / 25.0, -4.0 / 25.0));
        close("z / 2", (1.5, 2.0));
        close("1 + i", (1.0, 1.0));
        close("i + 1", (1.0, 1.0));
        close("1 - i", (1.0, -1.0));
        close("z - 1", (2.0, 4.0));
        close("2 + 3·i", (2.0, 3.0));
        close("conj(z)", (3.0, -4.0));
        close("z·conj(z)", (25.0, 0.0));
        close("z²", (3.0 * 3.0 - 4.0 * 4.0, 2.0 * 3.0 * 4.0));
        close("z³", (-117.0, 44.0));
        close("z^-1", (3.0 / 25.0, -4.0 / 25.0));
        close("z^0", (1.0, 0.0));
        close("exp(i·pi)", (-1.0, 0.0));
        close("exp(i·t)", (0.7f64.cos(), 0.7f64.sin()));
        close("exp(1 + i·pi / 2)", (0.0, std::f64::consts::E));
        close("exp(ln(z))", (3.0, 4.0));
        close("ln(z)", (5f64.ln(), 4f64.atan2(3.0)));
        close("ln(i)", (0.0, std::f64::consts::FRAC_PI_2));
        close("sqrt(i)", (0.5f64.sqrt(), 0.5f64.sqrt()));
        close("sqrt(z)", (2.0, 1.0));
        close("sqrt(conj(z))", (2.0, -1.0));
        close("sqrt((-4, 0))", (0.0, 2.0));
        close(
            "z^2.5",
            (
                5f64.powf(2.5) * (2.5 * 4f64.atan2(3.0)).cos(),
                5f64.powf(2.5) * (2.5 * 4f64.atan2(3.0)).sin(),
            ),
        );
        close(
            "z^i",
            (
                (-(4f64.atan2(3.0))).exp() * 5f64.ln().cos(),
                (-(4f64.atan2(3.0))).exp() * 5f64.ln().sin(),
            ),
        );
        close("2^i", (2f64.ln().cos(), 2f64.ln().sin()));
        // Scaling and adding points still mean what they did.
        close("z·2", (6.0, 8.0));
        close("z + w", (1.0, 4.5));
    }

    #[test]
    fn hyperbolic_functions_take_numbers_and_complex_numbers() {
        let number = |expr: &str| {
            let s = scene(&format!("let v = {expr}")).unwrap();
            env_of(&s, 0.0).get("v").unwrap()
        };
        let near = |a: f64, b: f64, what: &str| assert!((a - b).abs() < 1e-12, "{what}: {a} {b}");
        near(number("sinh(0.7)"), 0.7f64.sinh(), "sinh");
        near(number("cosh(0.7)"), 0.7f64.cosh(), "cosh");
        near(number("tanh(0.7)"), 0.7f64.tanh(), "tanh");
        near(number("coth(0.7)"), 1.0 / 0.7f64.tanh(), "coth");
        near(number("sech(0.7)"), 1.0 / 0.7f64.cosh(), "sech");
        near(number("csch(0.7)"), 1.0 / 0.7f64.sinh(), "csch");
        near(number("arctanh(0.7)"), 0.7f64.atanh(), "arctanh");
        // cosh(iy) = cos y, sinh(iy) = i·sin y, and coth(iy) = −i·cot y.
        let close = |expr: &str, want: (f64, f64)| {
            let got = complex_value(expr);
            assert!(
                (got.0 - want.0).abs() < 1e-12 && (got.1 - want.1).abs() < 1e-12,
                "{expr}: {got:?}, wanted {want:?}"
            );
        };
        close("cosh(i·t)", (0.7f64.cos(), 0.0));
        close("sinh(i·t)", (0.0, 0.7f64.sin()));
        close("coth(i·t)", (0.0, -1.0 / 0.7f64.tan()));
        // cosh² − sinh² = 1, and arctanh undoes tanh.
        close("cosh(z)² - sinh(z)²", (1.0, 0.0));
        close("tanh(arctanh(w / 9))", (-2.0 / 9.0, 0.5 / 9.0));
    }

    #[test]
    fn real_imag_abs_and_arg_read_a_complex_number() {
        let number = |expr: &str| {
            let s = scene(&format!("let z = (3, 4)\nlet v = {expr}")).unwrap();
            env_of(&s, 0.0).get("v").unwrap()
        };
        assert_eq!(number("real(z)"), 3.0);
        assert_eq!(number("imag(z)"), 4.0);
        assert_eq!(number("abs(z)"), 5.0);
        assert_eq!(number("abs(-5)"), 5.0);
        assert_eq!(number("arg(z)"), 4f64.atan2(3.0));
        assert_eq!(number("real(2)"), 2.0);
        assert_eq!(number("imag(2)"), 0.0);
        assert_eq!(number("arg(-2)"), std::f64::consts::PI);
    }

    #[test]
    fn i_gives_way_to_anything_named_i() {
        let s = scene("let i = 5\nlet v = i·2").unwrap();
        assert_eq!(env_of(&s, 0.0).get("v"), Some(10.0));
        let s = scene("dots d = (i, 0) for i in 1..2").unwrap();
        assert!(matches!(s.marks[0].form, Form::Many { .. }));
        let s = scene("fn f(i) = i·2\nlet v = f(3)").unwrap();
        assert_eq!(env_of(&s, 0.0).get("v"), Some(6.0));
    }

    #[test]
    fn a_number_is_a_point_on_the_real_axis() {
        let s = scene("curve c = parametric (t): -t for t in 0..1\nlabel n = \"N\" at 2").unwrap();
        let Form::Parametric(arc) = &s.marks[0].form else {
            panic!();
        };
        assert_eq!(arc.point[1], num(0.0));
        let Form::Label { at, .. } = &s.marks[1].form else {
            panic!();
        };
        assert_eq!(at, &[num(2.0), num(0.0)]);
        // It answers as a point does, whether it's passed in or named.
        let s = scene("fn f(w) = (w.y, w.x)\nlet k = 3\nlet P = f(2)\nlet a = k.x\nlet b = k.y")
            .unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!((env.get("P.x"), env.get("P.y")), (Some(0.0), Some(2.0)));
        assert_eq!((env.get("a"), env.get("b")), (Some(3.0), Some(0.0)));
    }

    #[test]
    fn constructions_take_numbers_as_points() {
        let s =
            scene("curve c = circle(0, 2)\nlet d = dist(1, 4)\nlet m = midpoint(0, 4)").unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!(env.get("c.r"), Some(2.0));
        assert_eq!(env.get("d"), Some(3.0));
        assert_eq!(env.get("m.x"), Some(2.0));
    }

    #[test]
    fn where_a_declaration_is_written_doesnt_matter() {
        // `bind` reads a `let` written after its `param`, and another `let` reads the param.
        let s = scene(
            "param b = 0 in 0..9
             let twice = b·2
             let later = 3
             bind b = later + 1",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!(env.get("b"), Some(4.0));
        assert_eq!(env.get("twice"), Some(8.0));
        // What's already in order stays in order.
        let s = scene("let a = 1\nlet b = a + 1\nlet c = 2").unwrap();
        let ids: Vec<&str> = s.bindings.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        // A solve's answers wait for what its equations read.
        let s = scene(
            "param q = 1 in 0..9
             solve (u) from 1: u² = q
             let later = 4
             bind q = later",
        )
        .unwrap();
        assert!((env_of(&s, 0.0).get("u").unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_circle_of_definitions_is_a_mistake_when_compiled() {
        let err = scene(
            "param a = 0 in 0..9
             param b = 0 in 0..9
             bind a = b + 1
             bind b = a + 1",
        )
        .unwrap_err();
        assert!(err.message.contains("in terms of each other"), "{err}");
    }

    #[test]
    fn what_a_complex_number_cant_do_says_so() {
        let err = scene("let v = sin((1, 2))").unwrap_err();
        assert!(
            err.message.contains("needs a number, found a point"),
            "{err}"
        );
        let err = scene("let v = min((1, 2), 3)").unwrap_err();
        assert!(err.message.contains("needs a number"), "{err}");
    }

    #[test]
    fn logs_at_zero_dont_exist() {
        let s = scene("let z = (0, 0)\nlet P = ln(z)\nlet Q = otherwise(P, (7, 8))").unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!(env.get("Q.x"), Some(7.0));
    }

    /// Exact derivatives have to see through what complex arithmetic writes.
    #[test]
    fn complex_values_can_be_solved_and_pushed_through_maps() {
        let s = scene(
            "fn square(w) = w²
             solve (u) from 1: abs(exp(i·u) + 1)² = 2
             group g() { label n = \"N\" at (1, 1) away (1, 0) }
             use g() through square",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        // |e^{iu} + 1|² = 2 + 2cos u, so u is π/2.
        assert!((env.get("u").unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        let Form::Label { at, away, .. } = &s.marks[0].form else {
            panic!();
        };
        // (1 + i)² = 2i, and the derivative of w² along 1 is 2w.
        assert_eq!(crate::eval::real(&at[1], &env).unwrap(), 2.0);
        let away = away.as_ref().unwrap();
        assert_eq!(crate::eval::real(&away[0], &env).unwrap(), 2.0);
        assert_eq!(crate::eval::real(&away[1], &env).unwrap(), 2.0);
    }

    #[test]
    fn groups_take_values() {
        let s = scene(
            "group view(k) {
               let P = (k, 0)
             }
             use view(3)",
        )
        .unwrap();
        assert_eq!(env_of(&s, 0.0).get("P.x"), Some(3.0));
        let err = scene("use nothing()").unwrap_err();
        assert!(err.message.contains("no group named"), "{err}");
        let err = scene("group g(a) { let b = a }\nuse g()").unwrap_err();
        assert!(err.message.contains("takes 1 arguments"), "{err}");
    }

    const MAPPED: &str = "fn twice(w) = 2·w
         fn bend(w) = (w.x, w.x² + w.y)
         let O = (1, 2)
         group things() {
           curve c = parametric (t): (t, 0) for t in 0..1
           segment s = O, (3, 2)
           dots d = (i, 0) for i in 1..2
           dot O
           label n = \"N\" at O away (1, 0)
         }\n";

    /// Evaluates a point of a mark, with this value for its variable.
    fn mark_point(s: &crate::ir::Scene, id: &str, var: f64) -> (f64, f64) {
        let mut env = env_of(s, 0.0);
        let pair = |p: &[Expr; 2], env: &crate::eval::Env| {
            (
                crate::eval::real(&p[0], env).unwrap(),
                crate::eval::real(&p[1], env).unwrap(),
            )
        };
        let mark = s.marks.iter().find(|m| m.id == id).unwrap();
        match &mark.form {
            Form::Parametric(arc) => {
                env.set(&arc.var, var);
                pair(&arc.point, &env)
            }
            Form::Dot(at) => pair(at, &env),
            Form::Label { at, .. } => pair(at, &env),
            Form::Many { lets, shape, .. } => {
                env.set("i", var);
                for (name, e) in lets {
                    let value = crate::eval::real(e, &env).unwrap();
                    env.set(name, value);
                }
                let crate::ir::Shape::Dots(at) = shape else {
                    panic!();
                };
                pair(at, &env)
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn through_sends_every_kind_of_mark_through_a_map() {
        let s = scene(&format!("{MAPPED}use things() through twice")).unwrap();
        assert_eq!(mark_point(&s, "c", 0.5), (1.0, 0.0));
        assert_eq!(mark_point(&s, "s", 0.5), (4.0, 4.0));
        assert_eq!(mark_point(&s, "d", 2.0), (4.0, 0.0));
        assert_eq!(mark_point(&s, "O", 0.0), (2.0, 4.0));
        assert_eq!(mark_point(&s, "n", 0.0), (2.0, 4.0));
        // Only marks move: the point itself is still what was written.
        assert_eq!(env_of(&s, 0.0).get("O.x"), Some(1.0));
        // A segment keeps its type, so style for curves doesn't reach it.
        let segment = s.marks.iter().find(|m| m.id == "s").unwrap();
        assert!(segment.tags.contains(&"segment".to_string()));
        assert!(!segment.tags.contains(&"curve".to_string()));
    }

    #[test]
    fn through_bends_a_segment() {
        let s = scene(&format!("{MAPPED}use things() through bend")).unwrap();
        // Halfway along, from (1, 2) to (3, 2), is (2, 2): straight lines don't stay straight.
        assert_eq!(mark_point(&s, "s", 0.5), (2.0, 6.0));
    }

    #[test]
    fn a_direction_goes_through_a_map_exactly() {
        let s = scene(&format!("{MAPPED}use things() through bend")).unwrap();
        let Form::Label { away, .. } = &s.marks.iter().find(|m| m.id == "n").unwrap().form else {
            panic!();
        };
        let env = env_of(&s, 0.0);
        let away = away.as_ref().unwrap();
        // At x = 1, a step along x moves bend's image by (1, 2x).
        assert_eq!(crate::eval::real(&away[0], &env).unwrap(), 1.0);
        assert_eq!(crate::eval::real(&away[1], &env).unwrap(), 2.0);
    }

    #[test]
    fn maps_compose_when_groups_nest() {
        let s = scene(
            "fn twice(w) = 2·w
             fn shift(w) = w + (1, 0)
             group inner() {
               label n = \"N\" at (1, 1) away (0, 1)
               curve c = parametric (t): (t, 0) for t in 0..1
             }
             group outer() { use inner() through shift }
             use outer() through twice",
        )
        .unwrap();
        // twice(shift(1, 1)), not shift(twice(1, 1)).
        assert_eq!(mark_point(&s, "n", 0.0), (4.0, 2.0));
        assert_eq!(mark_point(&s, "c", 1.0), (4.0, 0.0));
        let Form::Label { away, .. } = &s.marks[0].form else {
            panic!();
        };
        let env = env_of(&s, 0.0);
        assert_eq!(
            crate::eval::real(&away.as_ref().unwrap()[1], &env).unwrap(),
            2.0
        );
    }

    #[test]
    fn through_can_be_drawn_more_than_once() {
        let s = scene(&format!(
            "{MAPPED}use things() through twice as up
             use things() through bend"
        ))
        .unwrap();
        assert_eq!(mark_point(&s, "up_c", 1.0), (2.0, 0.0));
        assert_eq!(mark_point(&s, "c", 1.0), (1.0, 1.0));
        let copy = s.marks.iter().find(|m| m.id == "up_d").unwrap();
        assert!(copy.tags.contains(&"d".to_string()) && copy.tags.contains(&"up".to_string()));
    }

    #[test]
    fn what_has_no_image_says_so() {
        for (group, says) in [
            ("curve c = circle((0, 0), (1, 0))", "isn't one"),
            (
                "circles c = circle((i, 0), (i, 1)) for i in 0..1",
                "isn't a circle",
            ),
            ("segment s = (0, 0), (1, 0), (1, 1)", "only two points"),
        ] {
            let err = scene(&format!(
                "fn f(w) = w\ngroup g() {{ {group} }}\nuse g() through f"
            ))
            .unwrap_err();
            assert!(err.message.contains(says), "{group}: {err}");
        }
        let err =
            scene("group g() { dots d = (i, 0) for i in 0..1 }\nuse g() through nope").unwrap_err();
        assert!(err.message.contains("nope"), "{err}");
    }

    #[test]
    fn a_label_says_what_this_panel_calls_it() {
        let doc = parse(
            "group names() {
               label n at (0, 0) tag first
               label m at (1, 0) tag second
             }
             panel a {
               viewport { x: 0..1, y: 0..1 }
               use names()
               text n = \"0\"
             }
             panel b {
               viewport { x: 0..1, y: 0..1 }
               use names()
               text n = \"1\"
               text m = \"{1 + 1}\"
             }",
        )
        .unwrap();
        let panels = compile_panels(&doc).unwrap();
        let text = |s: &crate::ir::Scene, id: &str| {
            s.marks.iter().find(|m| m.id == id).map(|m| match &m.form {
                Form::Label { text, .. } => text.clone(),
                other => panic!("{other:?}"),
            })
        };
        assert_eq!(text(&panels[0].scene, "n").as_deref(), Some("0"));
        assert_eq!(text(&panels[1].scene, "n").as_deref(), Some("1"));
        assert_eq!(text(&panels[1].scene, "m").as_deref(), Some("{}"));
        // This space has no name for `m`, so it isn't drawn, and its tag doesn't land on `n`.
        assert_eq!(text(&panels[0].scene, "m"), None);
        let n = panels[0].scene.marks.iter().find(|m| m.id == "n").unwrap();
        assert!(n.tags.contains(&"first".to_string()));
        assert!(!n.tags.contains(&"second".to_string()));

        let err = scene("label n at (0, 0)\ntext nope = \"x\"").unwrap_err();
        assert!(err.message.contains("no label named `nope`"), "{err}");
    }

    #[test]
    fn panels_share_what_is_outside_them_and_keep_their_own_marks() {
        let doc = parse(
            "param a = 2 in 0..9
             group view(k) {
               let P = (k·a, 0)
               dot P
             }
             panel one \"First\" {
               use view(1)
               segment extra = (0, 0), (1, 1)
             }
             panel two {
               use view(2)
             }",
        )
        .unwrap();
        let panels = compile_panels(&doc).unwrap();
        assert_eq!(panels.len(), 2);
        assert_eq!(panels[0].title.as_deref(), Some("First"));
        assert_eq!(panels[1].title, None);
        assert_eq!(env_of(&panels[0].scene, 0.0).get("P.x"), Some(2.0));
        assert_eq!(env_of(&panels[1].scene, 0.0).get("P.x"), Some(4.0));
        // Each knows only its own marks, and they carry the panel's name to style by.
        assert_eq!(panels[0].scene.marks.len(), 2);
        assert_eq!(panels[1].scene.marks.len(), 1);
        assert!(panels[1].scene.marks[0].tags.contains(&"two".to_string()));
        assert!(compile(&doc).is_err());
    }

    #[test]
    fn a_group_can_be_drawn_more_than_once_under_a_prefix() {
        let s = scene(
            "group edge(height) {
               let P = (1, height)
               curve side = parametric (t): (t, height) for t in 0..1
               dots steps = z for i in 0..2 with z = (i, height)
               label name = \"n\" at P
             }
             use edge(0)
             use edge(10) as above",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!(env.get("P.y"), Some(0.0));
        assert_eq!(env.get("above_P.y"), Some(10.0));
        let ids: Vec<&str> = s.marks.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "side",
                "steps",
                "name",
                "above_side",
                "above_steps",
                "above_name"
            ]
        );
        // A style can reach both copies by the plain name, or one set by the prefix.
        let copy = &s.marks[3];
        assert!(copy.tags.contains(&"side".to_string()));
        assert!(copy.tags.contains(&"above".to_string()));
    }

    #[test]
    fn a_groups_parameters_are_its_own() {
        let s = scene(
            "group one(k) { let P = k }
             group two(k) { let Q = k }
             use one(2)
             use two(3)",
        )
        .unwrap();
        let env = env_of(&s, 0.0);
        assert_eq!(env.get("P"), Some(2.0));
        assert_eq!(env.get("Q"), Some(3.0));
    }

    /// A choice is one of its values: the param is which one, and a page gets each as written.
    #[test]
    fn a_choice_is_one_of_its_values() {
        let s = scene(
            "let ω = -1 / 2 + sqrt(3) / 2·i
             param axis in {ω, 1, 1 + ω}
             param k = 3 in {2, 3,  5}
             let P = 2·axis",
        )
        .unwrap();
        let index = s.params.iter().find(|p| p.id == "axis.index").unwrap();
        assert_eq!(index.range, [0.0, 2.0]);
        assert_eq!(index.step, Some(1.0));
        let choice = index.choice.as_ref().unwrap();
        assert_eq!(choice.of, "axis");
        assert_eq!(choice.options, ["ω", "1", "1 + ω"]);
        let at = |which: f64| {
            let inputs = crate::frame::Inputs {
                time: 0.0,
                params: vec![("axis.index".into(), which), ("k.index".into(), which)],
            };
            let env = eval::scene_env(&s, &inputs, None).unwrap();
            [env.get("P.x").unwrap(), env.get("P.y").unwrap(), env.get("k").unwrap()]
        };
        let h = 3f64.sqrt();
        // The number 1 is a point on the real axis, offered alongside ω.
        for (which, want) in [(0.0, [-1.0, h, 2.0]), (1.0, [2.0, 0.0, 3.0]), (2.0, [1.0, h, 5.0])] {
            let got = at(which);
            assert!(got.iter().zip(want).all(|(g, w)| near(*g, w)), "{which}: {got:?}");
        }
        // Between options, a reader's value goes to the nearest.
        assert!(near(at(1.4)[0], 2.0));
        // The default is named by an option, as written, and spaces don't matter.
        let env = env_of(&s, 0.0);
        assert_eq!(env.get("axis.index"), Some(0.0));
        assert_eq!(env.get("k"), Some(3.0));
    }

    #[test]
    fn a_choice_says_what_went_wrong() {
        let err = scene("param k = 4 in {2, 3, 5}").unwrap_err().to_string();
        assert!(err.contains("starts at 4, which isn't one of its options: 2, 3, 5"), "{err}");
        let err = scene("let c = circle((0, 0), 1)\nparam k in {c, 1}").unwrap_err().to_string();
        assert!(err.contains("offers a circle and a number"), "{err}");
    }

    #[test]
    fn a_labels_text_can_hold_values() {
        let s = scene(
            "param q = 7 in 0..9
             label n = \"−{q} and {q - 1}\" at (0, 0)",
        )
        .unwrap();
        let Form::Label { text, values, .. } = &s.marks[0].form else {
            panic!();
        };
        assert_eq!(text, "−{} and {}");
        assert_eq!(values.len(), 2);
        let err = scene("label n = \"{q\" at (0, 0)").unwrap_err();
        assert!(err.message.contains("isn't closed"), "{err}");
        let err = scene("label n = \"{nope}\" at (0, 0)").unwrap_err();
        assert!(err.message.contains("unknown name"), "{err}");
    }

    #[test]
    fn an_outline_needs_a_panel_that_exists() {
        let doc = parse(
            "panel a { viewport { x: 0..1, y: 0..1 } style figure { width: 10px; height: 10px } outline o = b }
             panel b { viewport { x: 1..2, y: 3..4 } style figure { width: 10px; height: 10px } }",
        )
        .unwrap();
        let panels = compile_panels(&doc).unwrap();
        let Form::Outline {
            view: Some(view), ..
        } = &panels[0].scene.marks[0].form
        else {
            panic!();
        };
        assert_eq!(view.x, [1.0, 2.0]);
        let doc = parse("panel a { outline o = b }").unwrap();
        assert!(compile_panels(&doc).is_err());
    }

    #[test]
    fn a_view_can_fit_a_curve() {
        let s = scene(
            "viewport { fit: c, pad: 0 }
             curve c = parametric (t): (t, 0) for t in 0..3",
        )
        .unwrap();
        assert_eq!(s.viewport.fit_curves.len(), 1);
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
        let err = scene("let a = circle(1, line((0, 0), (1, 1)))").unwrap_err();
        assert!(
            err.message.contains("`Circle` can't take a point, a line"),
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

    /// Beats with no lengths share a figure's duration, so its timing is said once, and with no
    /// duration they're steps for a reader. `beat` gets their range wherever it's declared.
    #[test]
    fn beats_with_no_lengths_share_the_duration() {
        let doc = |head: &str| {
            scene(&format!(
                "{head}
                 param q = 6 in 4..12 step 1
                 let A = (0, 0)
                 let B = (1, 0)
                 let C = (2, 0)
                 beat \"One\" {{
                   dot A
                 }}
                 beat \"Two\" {{
                   dot B
                 }}
                 beat \"Three\" {{
                   dot C
                 }}"
            ))
            .unwrap()
        };
        let s = doc("duration 12");
        let range = |s: &Scene, id: &str| s.params.iter().find(|p| p.id == id).unwrap().range;
        assert_eq!(range(&s, "q"), [4.0, 12.0]);
        assert_eq!(range(&s, BEAT_PARAM), [0.0, 2.0]);
        let beat = Expr::sym(BEAT_PARAM);
        for (time, want) in [(0.0, 0.0), (3.9, 0.0), (4.0, 1.0), (11.9, 2.0), (12.0, 2.0)] {
            assert_eq!(at_time(&s, time, &beat), want, "at {time} s");
        }
        // A longer duration spreads them out, rather than leaving the last one to hold.
        assert_eq!(at_time(&doc("duration 30"), 10.0, &beat), 1.0);
        // No duration, no clock.
        assert_eq!(at_time(&doc(""), 20.0, &beat), 0.0);
        // Something else can still drive them.
        assert_eq!(at_time(&doc("duration 12\nbind beat = 2"), 0.0, &beat), 2.0);
    }

    /// A scene's value for `name` at a time, with every binding worked out in order.
    fn at_time(s: &Scene, time: f64, e: &Expr) -> f64 {
        let mut env = Env::new(time);
        for binding in &s.bindings {
            if let Ok(v) = eval::real(&binding.expr, &env) {
                env.set(&binding.id, v);
            }
        }
        eval::real(e, &env).unwrap()
    }

    const STORY: &str = "
        pause 0.5
        beat grow \"A circle.\" for 1 show c
        beat spin for 2 show P focus P
        pause 1
        beat gone \"\" for 0.5 drop c, P
        let P = (cos(tau·spin), sin(tau·spin))
        curve c = circle((0, 0), (grow, 0))
        dot P
        segment axis = (-1, 0), (1, 0)";

    #[test]
    fn beats_with_lengths_are_timed_from_them() {
        let s = scene(STORY).unwrap();
        assert_eq!(s.duration, Some(5.0));
        assert_eq!(s.params[0].range, [0.0, 3.0]);
        assert_eq!(s.beat_lengths, [1.0, 2.0, 0.5]);
        // No caption keeps the one before, and `""` clears it.
        assert_eq!(s.beats, ["A circle.", "A circle.", ""]);
        let beat = Expr::sym(BEAT_PARAM);
        let cases = [
            (0.0, 0.0),
            (0.5, 0.0),
            (1.0, 0.5),
            (1.5, 1.0),
            (2.5, 1.5),
            (4.0, 2.0),
            (4.25, 2.0),
            (4.75, 2.5),
            (5.0, 3.0),
        ];
        for (time, want) in cases {
            assert!((at_time(&s, time, &beat) - want).abs() < 1e-12, "at {time}");
        }
        // A beat's name is its progress, which holds at 1 once it's done.
        let spin = Expr::sym("spin");
        assert_eq!(at_time(&s, 1.0, &spin), 0.0);
        assert!((at_time(&s, 2.5, &spin) - 0.5).abs() < 1e-12);
        assert_eq!(at_time(&s, 4.0, &spin), 1.0);
    }

    #[test]
    fn beats_show_and_drop_marks_and_say_what_they_are_about() {
        let s = scene(STORY).unwrap();
        let mark = |id: &str| s.marks.iter().find(|m| m.id == id).unwrap();
        let life = |id: &str, t: f64| at_time(&s, t, mark(id).life.as_ref().unwrap());
        // Not there until shown, arriving as its beat moves, and leaving as the dropping one does.
        assert_eq!(life("c", 0.0), 0.0);
        assert!((life("c", 1.0) - 0.5).abs() < 1e-12);
        assert_eq!(life("c", 3.0), 1.0);
        assert_eq!(life("c", 4.25), 1.0);
        assert!((life("c", 4.75) - 0.5).abs() < 1e-12);
        assert_eq!(life("c", 5.0), 0.0);
        assert_eq!(life("P", 1.5), 0.0);
        assert_eq!(life("P", 4.0), 1.0);
        // A mark no beat names is always there.
        assert!(mark("axis").life.is_none());
        // Focus starts on everything and holds once a beat names it.
        let focus = |id: &str, t: f64| at_time(&s, t, mark(id).focus.as_ref().unwrap());
        assert_eq!(focus("c", 1.5), 1.0);
        assert!((focus("c", 2.5) - 0.5).abs() < 1e-12);
        assert_eq!(focus("c", 5.0), 0.0);
        assert_eq!(focus("axis", 5.0), 0.0);
        assert!(mark("P").focus.is_none());
    }

    #[test]
    fn a_beats_block_shows_its_marks() {
        let s = scene(
            "beat first for 1
             beat second for 1 tag late {
               segment s = (0, 0), (1, 0)
             }",
        )
        .unwrap();
        let m = &s.marks[0];
        assert_eq!(m.beat, Some(1));
        assert!(m.tags.contains(&"late".to_string()));
        let life = m.life.as_ref().unwrap();
        assert_eq!(at_time(&s, 1.0, life), 0.0);
        assert!((at_time(&s, 1.5, life) - 0.5).abs() < 1e-12);
        assert_eq!(at_time(&s, 2.0, life), 1.0);
    }

    #[test]
    fn a_story_that_cannot_be_told_is_an_error() {
        let cases = [
            ("beat a for 1 show nothing", "no mark is named `nothing`"),
            ("beat a for 1 focus .nothing", "no mark is tagged `nothing`"),
            (
                "let P = (0, 0)\ndot P\nbeat a for 1 show P\nbeat b for 1 show P",
                "already there",
            ),
            (
                "let P = (0, 0)\ndot P\nbeat a for 1 drop P\nbeat b for 1 drop P",
                "isn't there",
            ),
            (
                "let P = (0, 0)\ndot P\nbeat a for 1 show P drop P",
                "twice in one beat",
            ),
            ("duration 1\nbeat a for 2", "more than the `duration`"),
            ("beat a for -1", "less than no time"),
            ("beat a for time", "uses `time`"),
            ("beat a for 1 tag t", "has none"),
            ("beat \"old\" { }\nbeat a for 1", "give them all lengths"),
            ("beat a for 1\nbeat \"old\" { }", "give them all lengths"),
            ("let a = 1\nbeat a for 1", "already defined"),
        ];
        for (src, want) in cases {
            let err = scene(src).unwrap_err();
            assert!(err.message.contains(want), "{src}: {err}");
        }
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
            "viewport { fit: c P, pad: 0.05 }
             let P = (5, 5)
             curve c = circle((0, 0), (1, 0))",
        )
        .unwrap();
        assert_eq!(s.viewport.fit.len(), 2);
        assert_eq!(s.viewport.pad, 0.05);
        let err = scene("viewport { fit: nope }").unwrap_err();
        assert!(err.message.contains("needs a point, circle"), "{err}");
    }
}
