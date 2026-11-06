//! The symbolic IR. Expressions stay symbolic and serialize as MathJSON: numbers,
//! symbol strings, and `[head, ...args]` arrays.
//!
//! A mark keeps the form it was written in. Nothing is normalized to one
//! representation, because each form has its own fast path: a parametric curve is
//! sampled directly, an SDF is evaluated per pixel, and only an implicit curve pays
//! for tracing. See `style::Draw` for how a style chooses among them.

use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    Sym(String),
    App(String, Vec<Expr>),
}

impl Expr {
    pub fn sym(name: impl Into<String>) -> Self {
        Expr::Sym(name.into())
    }

    pub fn app(head: &str, args: Vec<Expr>) -> Self {
        Expr::App(head.to_string(), args)
    }

    pub fn to_mathjson(&self) -> Value {
        match self {
            Expr::Num(n) => json!(n),
            Expr::Sym(s) => json!(s),
            Expr::App(head, args) => {
                let mut items = vec![json!(head)];
                items.extend(args.iter().map(Expr::to_mathjson));
                Value::Array(items)
            }
        }
    }

    /// Every symbol the expression references.
    pub fn visit_syms(&self, f: &mut impl FnMut(&str)) {
        match self {
            Expr::Num(_) => {}
            Expr::Sym(s) => f(s),
            Expr::App(_, args) => args.iter().for_each(|a| a.visit_syms(f)),
        }
    }
}

/// A named scalar, which is what mints identity (SPEC/15). A point-valued binding
/// `P` appears as the two scalars `P.x` and `P.y`.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub id: String,
    pub expr: Expr,
}

/// A root that time or a reader can drive. Its binding holds the time expression,
/// and an input overrides that for the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub id: String,
    pub range: [f64; 2],
}

/// A piece of a region's boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Point([Expr; 2]),
    Arc(Arc),
}

/// A curve given by a parameter over a range.
#[derive(Clone, Debug, PartialEq)]
pub struct Arc {
    pub var: String,
    pub point: [Expr; 2],
    pub range: [Expr; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub enum Form {
    /// The zero set of `field`. Orientation comes from the gradient convention: +∇
    /// points to the positive side, so the negative side is inside. Writing an
    /// implicit equation the other way round flips which side that is.
    Field {
        vars: [String; 2],
        field: Expr,
        grad: [Expr; 2],
        /// `sdf` declares a true distance, so per-pixel strokes are trustworthy.
        exact: bool,
    },
    Parametric(Arc),
    Polyline {
        points: Vec<[Expr; 2]>,
        closed: bool,
    },
    Region(Vec<Part>),
    Dot([Expr; 2]),
    Label {
        text: String,
        at: [Expr; 2],
        offset: [Expr; 2],
    },
}

/// Something drawn, in document order.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub id: String,
    pub form: Form,
    /// Words selectors can match. A mark's type (`circle`, `line`, `dot`…) is one.
    pub tags: Vec<String>,
    /// Which beat this mark belongs to, if any.
    pub beat: Option<usize>,
}

/// A mark a reader can drag. `inverse` maps `pointer.x`, `pointer.y` to the param, and
/// the param's range is the constraint: an explicit inverse, in SPEC/16's terms.
#[derive(Clone, Debug, PartialEq)]
pub struct Handle {
    pub mark: String,
    pub param: String,
    pub at: [Expr; 2],
    pub inverse: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Viewport {
    pub x: [f64; 2],
    pub y: [f64; 2],
    /// The width the figure's `px` sizes were designed at. With one, sizes scale with
    /// the output, as an SVG's do with its viewBox. Without one, a `px` is a CSS pixel.
    pub design_width: Option<f64>,
    pub design_height: Option<f64>,
    /// Centers and radii the view fits each frame, ignoring any param a handle has
    /// taken over, so a drag doesn't refit it. Empty for a fixed view.
    pub fit: Vec<[Expr; 3]>,
    pub pad: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: [-2.0, 2.0],
            y: [-2.0, 2.0],
            design_width: None,
            design_height: None,
            fit: Vec::new(),
            pad: 0.1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub name: Option<String>,
    pub params: Vec<Param>,
    pub bindings: Vec<Binding>,
    pub marks: Vec<Mark>,
    pub viewport: Viewport,
    /// How long one pass of the time bindings takes, for looping and video.
    pub duration: Option<f64>,
    pub beats: Vec<String>,
    pub handles: Vec<Handle>,
}

fn pair(p: &[Expr; 2]) -> Value {
    json!([p[0].to_mathjson(), p[1].to_mathjson()])
}

fn arc_json(a: &Arc) -> Value {
    json!({
        "var": a.var,
        "point": pair(&a.point),
        "range": pair(&a.range),
    })
}

impl Scene {
    pub fn to_json(&self) -> Value {
        let mark = |m: &Mark| {
            let form = match &m.form {
                Form::Field {
                    vars,
                    field,
                    grad,
                    exact,
                } => json!({
                    "kind": if *exact { "sdf" } else { "implicit" },
                    "vars": vars,
                    "field": field.to_mathjson(),
                    "grad": pair(grad),
                }),
                Form::Parametric(a) => json!({ "kind": "parametric", "arc": arc_json(a) }),
                Form::Polyline { points, closed } => json!({
                    "kind": "polyline",
                    "points": points.iter().map(pair).collect::<Vec<_>>(),
                    "closed": closed,
                }),
                Form::Region(parts) => json!({
                    "kind": "region",
                    "parts": parts.iter().map(|p| match p {
                        Part::Point(q) => pair(q),
                        Part::Arc(a) => arc_json(a),
                    }).collect::<Vec<_>>(),
                }),
                Form::Dot(p) => json!({ "kind": "dot", "at": pair(p) }),
                Form::Label { text, at, offset } => json!({
                    "kind": "label", "text": text, "at": pair(at), "offset": pair(offset),
                }),
            };
            json!({ "id": m.id, "form": form, "tags": m.tags, "beat": m.beat })
        };
        json!({
            "doc": self.name,
            "duration": self.duration,
            "beats": self.beats,
            "handles": self.handles.iter().map(|h| json!({
                "mark": h.mark,
                "param": h.param,
                "inverse": h.inverse.to_mathjson(),
            })).collect::<Vec<_>>(),
            "params": self.params.iter().map(|p| json!({ "id": p.id, "range": p.range })).collect::<Vec<_>>(),
            "bindings": self.bindings.iter().map(|b| json!({
                "id": b.id,
                "expr": b.expr.to_mathjson(),
            })).collect::<Vec<_>>(),
            "marks": self.marks.iter().map(mark).collect::<Vec<_>>(),
            "viewport": {
                "x": self.viewport.x,
                "y": self.viewport.y,
                "width": self.viewport.design_width,
                "height": self.viewport.design_height,
                "fit": self.viewport.fit.len(),
                "pad": self.viewport.pad,
            },
        })
    }
}
