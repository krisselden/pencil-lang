//! Lowers a parsed document to the scene IR. Functions are inlined and point algebra
//! is expanded here, so downstream only sees scalar expressions, still symbolic.

use std::collections::{HashMap, HashSet};

use crate::diff::derivative;
use crate::error::Error;
use crate::eval::{self, Env};
use crate::ir::{Arc, Binding, Expr, Form, Mark, Param, Part, Scene, Viewport};
use crate::parse::{CurveSyntax, Document, Item, PartSyntax};

/// Names every expression can use without declaring them.
pub const SIGNALS: [&str; 1] = ["time"];
pub const CONSTANTS: [&str; 3] = ["pi", "π", "tau"];

/// Builtin heads that take and return scalars.
const SCALAR_HEADS: [&str; 15] = [
    "Sqrt",
    "Sin",
    "Cos",
    "Tan",
    "Exp",
    "Ln",
    "Abs",
    "Sign",
    "Arctan2",
    "Min",
    "Max",
    "Clamp",
    "Mod",
    "Smoothstep",
    "Power",
];
const MAX_INLINE_DEPTH: usize = 32;

#[derive(Clone, Debug)]
enum Val {
    Scalar(Expr),
    Point(Expr, Expr),
}

struct Function {
    params: Vec<String>,
    body: Expr,
}

#[derive(Default)]
struct Lowerer {
    functions: HashMap<String, Function>,
    points: HashSet<String>,
    /// Names known at this point in the document.
    scalars: HashSet<String>,
}

fn add(a: Expr, b: Expr) -> Expr {
    Expr::app("Add", vec![a, b])
}

fn square(a: Expr) -> Expr {
    Expr::app("Power", vec![a, Expr::Num(2.0)])
}

type Locals = HashMap<String, Val>;

impl Lowerer {
    fn lower(&self, e: &Expr, locals: &Locals, depth: usize) -> Result<Val, Error> {
        if depth > MAX_INLINE_DEPTH {
            return Err(Error::new("functions nest too deeply, or recurse"));
        }
        let scalar = |v: Val, what: &str| match v {
            Val::Scalar(s) => Ok(s),
            Val::Point(..) => Err(Error::new(format!("{what} needs a number, found a point"))),
        };
        Ok(match e {
            Expr::Num(_) => Val::Scalar(e.clone()),
            Expr::Sym(s) => {
                if let Some(v) = locals.get(s) {
                    v.clone()
                } else if self.points.contains(s) {
                    Val::Point(Expr::sym(format!("{s}.x")), Expr::sym(format!("{s}.y")))
                } else if self.scalars.contains(s)
                    || SIGNALS.contains(&s.as_str())
                    || CONSTANTS.contains(&s.as_str())
                {
                    Val::Scalar(e.clone())
                } else if let Some((base, axis)) = s.split_once('.') {
                    match (locals.get(base), axis) {
                        (Some(Val::Point(x, _)), "x") => Val::Scalar(x.clone()),
                        (Some(Val::Point(_, y)), "y") => Val::Scalar(y.clone()),
                        _ => return Err(Error::new(format!("unknown name `{s}`"))),
                    }
                } else {
                    return Err(Error::new(format!("unknown name `{s}`")));
                }
            }
            Expr::App(head, args) => {
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
                use Val::{Point, Scalar};
                match (head.as_str(), low.as_slice()) {
                    ("Tuple", [Scalar(x), Scalar(y)]) => Point(x.clone(), y.clone()),
                    ("Add" | "Subtract", [Point(ax, ay), Point(bx, by)]) => Point(
                        Expr::app(head, vec![ax.clone(), bx.clone()]),
                        Expr::app(head, vec![ay.clone(), by.clone()]),
                    ),
                    ("Add" | "Subtract" | "Multiply" | "Divide", [Scalar(a), Scalar(b)]) => {
                        Scalar(Expr::app(head, vec![a.clone(), b.clone()]))
                    }
                    ("Multiply", [Scalar(k), Point(x, y)])
                    | ("Multiply", [Point(x, y), Scalar(k)]) => Point(
                        Expr::app("Multiply", vec![k.clone(), x.clone()]),
                        Expr::app("Multiply", vec![k.clone(), y.clone()]),
                    ),
                    ("Divide", [Point(x, y), Scalar(k)]) => Point(
                        Expr::app("Divide", vec![x.clone(), k.clone()]),
                        Expr::app("Divide", vec![y.clone(), k.clone()]),
                    ),
                    ("Negate", [Scalar(a)]) => Scalar(Expr::app("Negate", vec![a.clone()])),
                    ("Negate", [Point(x, y)]) => Point(
                        Expr::app("Negate", vec![x.clone()]),
                        Expr::app("Negate", vec![y.clone()]),
                    ),
                    ("Length", [Point(x, y)]) => Scalar(Expr::app(
                        "Sqrt",
                        vec![add(square(x.clone()), square(y.clone()))],
                    )),
                    ("Distance", [Point(ax, ay), Point(bx, by)]) => {
                        let dx = Expr::app("Subtract", vec![bx.clone(), ax.clone()]);
                        let dy = Expr::app("Subtract", vec![by.clone(), ay.clone()]);
                        Scalar(Expr::app("Sqrt", vec![add(square(dx), square(dy))]))
                    }
                    ("Dot", [Point(ax, ay), Point(bx, by)]) => Scalar(add(
                        Expr::app("Multiply", vec![ax.clone(), bx.clone()]),
                        Expr::app("Multiply", vec![ay.clone(), by.clone()]),
                    )),
                    (head, _) if SCALAR_HEADS.contains(&head) => {
                        let mut scalars = Vec::with_capacity(low.len());
                        for v in low {
                            scalars.push(scalar(v, head)?);
                        }
                        Scalar(Expr::app(head, scalars))
                    }
                    (head, _) if head.chars().next().is_some_and(char::is_uppercase) => {
                        return Err(Error::new(format!(
                            "`{head}` can't take these arguments here"
                        )));
                    }
                    (head, _) => {
                        return Err(Error::new(format!("unknown function `{head}`")));
                    }
                }
            }
        })
    }

    fn scalar(&self, e: &Expr, locals: &Locals) -> Result<Expr, Error> {
        match self.lower(e, locals, 0)? {
            Val::Scalar(s) => Ok(s),
            Val::Point(..) => Err(Error::new("expected a number, found a point")),
        }
    }

    fn point(&self, e: &Expr, locals: &Locals) -> Result<[Expr; 2], Error> {
        match self.lower(e, locals, 0)? {
            Val::Point(x, y) => Ok([x, y]),
            Val::Scalar(_) => Err(Error::new("expected a point, found a number")),
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
    // Bindings can't use coordinates, so they're constant here and the derivative
    // treats them as symbols, which keeps the gradient small.
    let grad = [derivative(&field, &vars[0])?, derivative(&field, &vars[1])?];
    Ok(Form::Field {
        vars: vars.clone(),
        field,
        grad,
        exact,
    })
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
    };
    let none = Locals::new();
    let mut ids: HashSet<String> = HashSet::new();
    let mut claim = |name: &str, line: usize| -> Result<(), Error> {
        if SIGNALS.contains(&name) || CONSTANTS.contains(&name) || !ids.insert(name.to_string()) {
            return Err(Error::at(line, 1, format!("`{name}` is already defined")));
        }
        Ok(())
    };
    let mut binds: HashMap<&str, Expr> = HashMap::new();

    for item in &doc.items {
        match item {
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
                if binds.insert(name, low).is_some() {
                    return Err(Error::at(*line, 1, format!("`{name}` is already bound")));
                }
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
                match lowerer.lower(expr, &none, 0).map_err(at_line(*line))? {
                    Val::Scalar(expr) => {
                        scene.bindings.push(Binding {
                            id: name.clone(),
                            expr,
                        });
                        lowerer.scalars.insert(name.clone());
                    }
                    Val::Point(x, y) => {
                        scene.bindings.push(Binding {
                            id: format!("{name}.x"),
                            expr: x,
                        });
                        scene.bindings.push(Binding {
                            id: format!("{name}.y"),
                            expr: y,
                        });
                        lowerer.points.insert(name.clone());
                        lowerer.scalars.insert(format!("{name}.x"));
                        lowerer.scalars.insert(format!("{name}.y"));
                    }
                }
            }
            Item::Duration(e) => scene.duration = Some(constant(e, "`duration`")?),
            Item::Curve { name, form, line } => {
                claim(name, *line)?;
                let fail = at_line(*line);
                let form = match form {
                    CurveSyntax::Implicit { vars, lhs, rhs } => {
                        let field = Expr::app("Subtract", vec![lhs.clone(), rhs.clone()]);
                        field_form(&lowerer, vars, &field, false)
                    }
                    CurveSyntax::Sdf { vars, field } => field_form(&lowerer, vars, field, true),
                    CurveSyntax::Parametric { var, point, range } => {
                        lowerer.arc(var, point, range).map(Form::Parametric)
                    }
                }
                .map_err(fail)?;
                scene.marks.push(Mark {
                    id: name.clone(),
                    form,
                });
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
                scene.marks.push(Mark {
                    id: name.clone(),
                    form: Form::Polyline {
                        points,
                        closed: *closed,
                    },
                });
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
                let points = [a, corner, c]
                    .iter()
                    .map(|p| lowerer.point(p, &locals))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(&fail)?;
                scene.marks.push(Mark {
                    id: name.clone(),
                    form: Form::Polyline {
                        points,
                        closed: false,
                    },
                });
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
                scene.marks.push(Mark {
                    id: name.clone(),
                    form: Form::Region(parts),
                });
            }
            Item::Dots { names, line } => {
                for name in names {
                    let p = lowerer
                        .point(&Expr::sym(name), &none)
                        .map_err(at_line(*line))?;
                    scene.marks.push(Mark {
                        id: name.clone(),
                        form: Form::Dot(p),
                    });
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
                    None => [Expr::Num(0.0), Expr::Num(0.0)],
                };
                scene.marks.push(Mark {
                    id: name.clone(),
                    form: Form::Label {
                        text: text.clone(),
                        at,
                        offset,
                    },
                });
            }
            Item::Viewport { x, y, width } => {
                scene.viewport = Viewport {
                    x: [
                        constant(&x[0], "a viewport bound")?,
                        constant(&x[1], "a viewport bound")?,
                    ],
                    y: [
                        constant(&y[0], "a viewport bound")?,
                        constant(&y[1], "a viewport bound")?,
                    ],
                    design_width: match width {
                        Some(w) => Some(constant(w, "a viewport's width")?),
                        None => None,
                    },
                };
                let v = &scene.viewport;
                if v.x[0] >= v.x[1] || v.y[0] >= v.y[1] {
                    return Err(Error::new("a viewport's bounds must increase"));
                }
            }
            Item::Style(_) => {}
        }
    }

    // Bind expressions were lowered where they were written, and only here replace
    // the param's default.
    for (name, expr) in binds {
        let Some(binding) = scene.bindings.iter_mut().find(|b| b.id == name) else {
            return Err(Error::new(format!(
                "`bind {name}`: no param named `{name}`"
            )));
        };
        if !scene.params.iter().any(|p| p.id == name) {
            return Err(Error::new(format!("`bind {name}`: `{name}` isn't a param")));
        }
        binding.expr = expr;
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
        let mut env = Env::new(0.0);
        for b in &s.bindings {
            let v = eval::real(&b.expr, &env).unwrap();
            env.set(&b.id, v);
        }
        assert_eq!(eval::real(&Expr::sym("Q.x"), &env).unwrap(), 4.0);
        assert_eq!(eval::real(&Expr::sym("Q.y"), &env).unwrap(), 3.0);
    }

    #[test]
    fn functions_inline_with_their_own_scope() {
        let s = scene(
            "fn on(t) = (cos(t), sin(t))
             fn scale(p, k) = p * k
             let P = scale(on(0), 3)",
        )
        .unwrap();
        let env = Env::new(0.0);
        assert_eq!(eval::real(&s.bindings[0].expr, &env).unwrap(), 3.0);
        let err = scene("fn f(x) = y\nlet y = 1\nlet a = f(1)").err();
        // Bodies see globals known when they're called, not the caller's locals.
        assert!(err.is_none());
    }

    #[test]
    fn a_bind_replaces_the_default_with_a_time_expression() {
        let s = scene("param a = 1 in 0..2\nbind a = time * 2").unwrap();
        assert_eq!(s.params[0].range, [0.0, 2.0]);
        let env = Env::new(3.0);
        assert_eq!(eval::real(&s.bindings[0].expr, &env).unwrap(), 6.0);
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
}
