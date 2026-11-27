//! The symbolic IR. Expressions stay symbolic and serialize as MathJSON: numbers,
//! symbol strings, and `[head, ...args]` arrays.
//!
//! A mark keeps the form it was written in. Nothing is normalized to one
//! representation, because each form has its own fast path: a parametric curve is
//! sampled directly, an SDF is evaluated per pixel, and only an implicit curve pays
//! for tracing. See `style::Draw` for how a style chooses among them.

use serde_json::{Value, json};

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Expr {
    Num(f64),
    Sym(String),
    App(String, Vec<Expr>),
}

/// As an S-expression, `(Multiply 2 pi)`, so a statement that holds one stays readable.
impl std::fmt::Debug for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Num(n) => write!(f, "{n}"),
            Expr::Sym(s) => write!(f, "{s}"),
            Expr::App(head, args) => {
                write!(f, "({head}")?;
                for a in args {
                    write!(f, " {a:?}")?;
                }
                write!(f, ")")
            }
        }
    }
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
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Binding {
    pub id: String,
    pub expr: Expr,
}

/// A root that time or a reader can drive. Its binding holds the time expression,
/// and an input overrides that for the frame.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Param {
    pub id: String,
    /// The widest its range gets. With no `limits`, this is its range.
    pub range: [f64; 2],
    /// An end of its range that follows other params, as `0..q` does. The range in a frame is
    /// read from them, and `range` holds the widest that can be.
    pub limits: [Option<Expr>; 2],
    /// The param only takes the values its range's start plus a whole number of these, as a
    /// whole number does with a step of 1.
    pub step: Option<f64>,
    /// For a param that's one of a list of values, the value's name and what each option says.
    /// The param itself is which one, counting from 0.
    pub choice: Option<Choice>,
    /// What its control says, as the document writes it: `param q "arms" = 8 in 3..16`. Without
    /// one, a control says the param's name.
    pub caption: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Choice {
    /// The name the document reads, which is the chosen value.
    pub of: String,
    /// Each option as written, which is how a page labels it.
    pub options: Vec<String>,
}

impl Param {
    /// The value it takes in a range, however it's driven: within the range, and on a step
    /// counted from the range's start. The range's end counts only if it's on a step, and a
    /// range that ends before it starts leaves only its start.
    pub fn settle(&self, value: f64, [lo, hi]: [f64; 2]) -> f64 {
        let top = match self.step {
            Some(step) => lo + ((hi - lo) / step + 1e-9).floor() * step,
            None => hi,
        };
        let value = value.max(lo).min(top.max(lo));
        match self.step {
            Some(step) => lo + ((value - lo) / step).round() * step,
            None => value,
        }
    }

    /// The names its range reads.
    pub fn follows(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for limit in self.limits.iter().flatten() {
            limit.visit_syms(&mut |s| {
                if !out.iter().any(|known| known == s) {
                    out.push(s.to_string());
                }
            });
        }
        out
    }
}

/// A piece of a region's boundary.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Part {
    Point([Expr; 2]),
    Arc(Arc),
}

/// A curve given by a parameter over a range.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Arc {
    pub var: String,
    pub point: [Expr; 2],
    pub range: [Expr; 2],
    /// The point's third coordinate, for a curve of points in space. `point` is where it's drawn,
    /// through the view, and this is its `z` as written, which a style reads as `self.z`.
    pub depth: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
        /// Signed curvature, for a circle, which says which side is its disk: the negative one,
        /// unless this is below 0. A line is 0.
        curvature: Option<Expr>,
        /// How to follow the curve as a line, if that's known, so that dashes and tapers
        /// don't have to trace it. A construction's circle knows.
        line: Option<Arc>,
    },
    Parametric(Arc),
    Polyline {
        points: Vec<[Expr; 2]>,
        closed: bool,
    },
    Region(Vec<Part>),
    Dot([Expr; 2]),
    /// A shaft and a head, as one line. The head's size is in pixels, so it's a style's.
    Arrow {
        from: [Expr; 2],
        to: [Expr; 2],
    },
    /// One mark drawn many times, as one line stroke, however many that is this frame.
    /// A variable counts in whole numbers, and includes both ends of its range.
    Many {
        vars: Vec<(String, [Expr; 2])>,
        /// Named once per instance, in order, so a value used many times is worked out once.
        lets: Vec<(String, Expr)>,
        shape: Shape,
    },
    /// One part of an expression, as a mark so styles can target it. `terms.rs` says where.
    Term {
        expression: usize,
        key: String,
        text: String,
    },
    Label {
        /// With a `{}` wherever a value goes.
        text: String,
        values: Vec<Shown>,
        /// What the label is about, if it's about one value.
        subject: Option<Subject>,
        /// Where it's drawn. None is a readout, said beside the figure as text.
        at: Option<[Expr; 2]>,
        offset: [Expr; 2],
        /// Which way it sits from its point, in the figure's own coordinates, at the distance
        /// its style sets. Replaces `offset`.
        away: Option<[Expr; 2]>,
    },
    /// The rectangle another panel is looking at. Compiling all the panels fills in its view.
    Outline {
        panel: String,
        view: Option<Box<Viewport>>,
    },
}

/// A value a label's text shows: a number, or a point, which the label's style writes as a pair,
/// a complex number or in polar form. A point and a complex number are one thing in Pencil, so
/// which it reads as is presentation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Shown {
    Number(Expr),
    Point([Expr; 2]),
}

impl Shown {
    pub fn exprs(&self) -> &[Expr] {
        match self {
            Shown::Number(e) => std::slice::from_ref(e),
            Shown::Point(p) => p,
        }
    }

    pub fn exprs_mut(&mut self) -> &mut [Expr] {
        match self {
            Shown::Number(e) => std::slice::from_mut(e),
            Shown::Point(p) => p,
        }
    }

    pub fn to_mathjson(&self) -> serde_json::Value {
        match self {
            Shown::Number(e) => e.to_mathjson(),
            Shown::Point([x, y]) => serde_json::json!(["Tuple", x.to_mathjson(), y.to_mathjson()]),
        }
    }
}

/// The one value a label is about: the value it's named after, or the only thing its text
/// shows. A style's `text` writes it as its name, its expression, its value, or an equation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Subject {
    pub name: String,
    /// The expression as written: a `let`'s right side, or what's between a text's braces.
    pub expr: String,
    pub value: Shown,
}

/// Something drawn, in document order.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mark {
    pub id: String,
    pub form: Form,
    /// Words selectors can match. A mark's type (`circle`, `line`, `dot`…) is one.
    pub tags: Vec<String>,
    /// Which beat this mark belongs to, if any.
    pub beat: Option<usize>,
    /// How present it is, from 0 to 1, as beats with lengths show and drop it: an expression in
    /// `beat`. None is always there.
    pub life: Option<Expr>,
    /// How much the beats with lengths are about it, from 0 to 1, in `beat`. None is always 1.
    pub focus: Option<Expr>,
}

/// Unknowns defined by equations, each a binding `["Solved", system, k]`. Newton's method runs
/// on these from `start`, with `jacobian` holding each equation's derivative by each unknown.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct System {
    pub unknowns: Vec<String>,
    pub start: Vec<Expr>,
    pub equations: Vec<Expr>,
    pub jacobian: Vec<Expr>,
    /// Every other name it reads. The answer depends on these and nothing else.
    pub depends: Vec<String>,
}

/// What a collection draws, for each combination of its variables.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Shape {
    Dots([Expr; 2]),
    Circles {
        center: [Expr; 2],
        r: Expr,
    },
    /// Circles with a handedness, any of which can be a line, as the coefficients of
    /// `a|p|² − 2b·p + c`, scaled so that `|b|² − ac` is 1.
    Cycles {
        a: Expr,
        b: [Expr; 2],
        c: Expr,
    },
    /// A parametric curve, whose own parameter runs over its range in every instance.
    Curves(Arc),
}

/// Which end of an expression sits at its place.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Hang {
    #[default]
    Start,
    Middle,
    End,
}

/// Where an expression can sit: a moving point, and an offset from it in `px`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Spot {
    pub id: String,
    pub at: [Expr; 2],
    pub offset: [Expr; 2],
    pub hang: Hang,
}

/// One written expression in a state.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Piece {
    pub spot: usize,
    pub keys: Vec<String>,
}

/// A list of states, driven by a binding: at 3.4 it's 40% of the way from state 3 to state 4
/// by time, before easing. State 0 is the empty one before the first that's written.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Expression {
    pub id: String,
    pub by: Expr,
    pub states: Vec<Vec<Piece>>,
}

/// A mark a reader can drag. It sets one param, or two for a point dragged in two dimensions.
/// Each of `sets` is a param and the expression in `pointer.x`, `pointer.y` that it takes, and
/// each param's range is its constraint: an explicit inverse, in SPEC/16's terms.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Handle {
    pub mark: String,
    pub at: [Expr; 2],
    pub sets: Vec<(String, Expr)>,
}

impl Handle {
    /// The params a drag of it sets, in the order it says them.
    pub fn params(&self) -> impl Iterator<Item = &str> {
        self.sets.iter().map(|(param, _)| param.as_str())
    }

    /// Whether dragging it sets this param.
    pub fn sets(&self, param: &str) -> bool {
        self.params().any(|p| p == param)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Family {
    pub vars: Vec<(String, [Expr; 2])>,
    pub lets: Vec<(String, Expr)>,
    pub arc: Arc,
}

/// `1em` where nothing says otherwise: the site's size for naming a point.
pub const DEFAULT_EM: f64 = 17.0;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Viewport {
    pub x: [f64; 2],
    pub y: [f64; 2],
    /// The width the figure's `px` sizes were designed at, from a sheet's `style figure` or
    /// `style panel`, so it's set after styling and isn't part of the scene's IR. With one, sizes
    /// scale with the output, as an SVG's do with its viewBox. Without one, a `px` is a CSS pixel.
    pub design_width: Option<f64>,
    pub design_height: Option<f64>,
    /// The reader's text size in design pixels, for a video, where no page says it.
    pub em: f64,
    /// Centers and radii the view fits each frame, ignoring any param a handle has
    /// taken over, so a drag doesn't refit it. Empty for a fixed view.
    pub fit: Vec<[Expr; 3]>,
    /// Curves the view fits, by sampling them.
    pub fit_curves: Vec<Arc>,
    /// Collections of curves the view fits, by sampling every one.
    pub fit_families: Vec<Family>,
    pub pad: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: [-2.0, 2.0],
            y: [-2.0, 2.0],
            design_width: None,
            design_height: None,
            em: DEFAULT_EM,
            fit: Vec::new(),
            fit_curves: Vec::new(),
            fit_families: Vec::new(),
            pad: 0.1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Scene {
    pub name: Option<String>,
    pub params: Vec<Param>,
    pub bindings: Vec<Binding>,
    pub marks: Vec<Mark>,
    pub viewport: Viewport,
    /// How long one pass of the time bindings takes, for looping and video.
    pub duration: Option<f64>,
    pub beats: Vec<String>,
    /// How long each beat takes in seconds, written or not, when the beats are moves, and empty
    /// when they're steps.
    pub beat_lengths: Vec<f64>,
    pub handles: Vec<Handle>,
    pub spots: Vec<Spot>,
    pub expressions: Vec<Expression>,
    pub systems: Vec<System>,
    /// The moment a still of the figure shows, if the document says.
    pub poster: Option<Poster>,
}

/// A moment of a figure: a time, and params held at values, as a reader could set them.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Poster {
    pub time: Option<f64>,
    pub params: Vec<(String, f64)>,
}

fn pair(p: &[Expr; 2]) -> Value {
    json!([p[0].to_mathjson(), p[1].to_mathjson()])
}

fn arc_json(a: &Arc) -> Value {
    let mut arc = json!({
        "var": a.var,
        "point": pair(&a.point),
        "range": pair(&a.range),
    });
    if let Some(depth) = &a.depth {
        arc["depth"] = depth.to_mathjson();
    }
    arc
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
                    curvature,
                    line,
                } => {
                    let mut form = json!({
                        "kind": if *exact { "sdf" } else { "implicit" },
                        "vars": vars,
                        "field": field.to_mathjson(),
                        "grad": pair(grad),
                        "line": line.as_ref().map(arc_json),
                    });
                    if let Some(curvature) = curvature {
                        form["curvature"] = curvature.to_mathjson();
                    }
                    form
                }
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
                Form::Arrow { from, to } => json!({
                    "kind": "arrow", "from": pair(from), "to": pair(to),
                }),
                Form::Many { vars, lets, shape } => json!({
                    "kind": "many",
                    "with": lets.iter().map(|(name, e)| json!({
                        "id": name,
                        "expr": e.to_mathjson(),
                    })).collect::<Vec<_>>(),
                    "vars": vars.iter().map(|(name, range)| json!({
                        "var": name,
                        "range": pair(range),
                    })).collect::<Vec<_>>(),
                    "shape": match shape {
                        Shape::Dots(at) => json!({ "dots": pair(at) }),
                        Shape::Circles { center, r } => json!({
                            "circles": pair(center),
                            "r": r.to_mathjson(),
                        }),
                        Shape::Curves(arc) => json!({ "curves": arc_json(arc) }),
                        Shape::Cycles { a, b, c } => json!({
                            "cycles": {
                                "a": a.to_mathjson(),
                                "b": pair(b),
                                "c": c.to_mathjson(),
                            },
                        }),
                    },
                }),
                Form::Term {
                    expression,
                    key,
                    text,
                } => json!({
                    "kind": "term", "expression": expression, "key": key, "text": text,
                }),
                Form::Label {
                    text,
                    values,
                    subject,
                    at,
                    offset,
                    away,
                } => {
                    let mut label = json!({
                        "kind": "label",
                        "text": text,
                        "values": values.iter().map(Shown::to_mathjson).collect::<Vec<_>>(),
                    });
                    // A readout has no place: it's said beside the figure.
                    match at {
                        Some(at) => {
                            label["at"] = pair(at);
                            label["offset"] = pair(offset);
                        }
                        None => label["readout"] = json!(true),
                    }
                    if let Some(away) = away {
                        label["away"] = pair(away);
                    }
                    if let Some(subject) = subject {
                        label["subject"] = json!({
                            "name": subject.name,
                            "expr": subject.expr,
                            "value": subject.value.to_mathjson(),
                        });
                    }
                    label
                }
                Form::Outline { panel, .. } => json!({ "kind": "outline", "panel": panel }),
            };
            let mut mark = json!({ "id": m.id, "form": form, "tags": m.tags, "beat": m.beat });
            if let Some(life) = &m.life {
                mark["life"] = life.to_mathjson();
            }
            if let Some(focus) = &m.focus {
                mark["focus"] = focus.to_mathjson();
            }
            mark
        };
        let mut scene = json!({
            "doc": self.name,
            "duration": self.duration,
            "beats": self.beats,
            "systems": self.systems.iter().map(|s| json!({
                "unknowns": s.unknowns,
                "start": s.start.iter().map(Expr::to_mathjson).collect::<Vec<_>>(),
                "equations": s.equations.iter().map(Expr::to_mathjson).collect::<Vec<_>>(),
                "jacobian": s.jacobian.iter().map(Expr::to_mathjson).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "places": self.spots.iter().map(|s| json!({
                "id": s.id,
                "at": pair(&s.at),
                "offset": pair(&s.offset),
                "hang": format!("{:?}", s.hang).to_lowercase(),
            })).collect::<Vec<_>>(),
            "expressions": self.expressions.iter().map(|e| json!({
                "id": e.id,
                "by": e.by.to_mathjson(),
                "states": e.states.iter().map(|state| state.iter().map(|p| json!({
                    "place": self.spots[p.spot].id,
                    "terms": p.keys,
                })).collect::<Vec<_>>()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "handles": self.handles.iter().map(|h| json!({
                "mark": h.mark,
                "sets": h.sets.iter().map(|(param, inverse)| json!({
                    "param": param,
                    "inverse": inverse.to_mathjson(),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "params": self.params.iter().map(|p| {
                let mut param = json!({ "id": p.id, "range": p.range });
                if p.limits.iter().any(Option::is_some) {
                    param["limits"] = json!(p.limits.iter()
                        .map(|limit| limit.as_ref().map(Expr::to_mathjson))
                        .collect::<Vec<_>>());
                }
                if let Some(step) = p.step {
                    param["step"] = json!(step);
                }
                if let Some(choice) = &p.choice {
                    param["choice"] = json!({ "of": choice.of, "options": choice.options });
                }
                if let Some(caption) = &p.caption {
                    param["caption"] = json!(caption);
                }
                param
            }).collect::<Vec<_>>(),
            "bindings": self.bindings.iter().map(|b| json!({
                "id": b.id,
                "expr": b.expr.to_mathjson(),
            })).collect::<Vec<_>>(),
            "marks": self.marks.iter().map(mark).collect::<Vec<_>>(),
            "viewport": {
                "x": self.viewport.x,
                "y": self.viewport.y,
                "fit": self.viewport.fit.len()
                    + self.viewport.fit_curves.len()
                    + self.viewport.fit_families.len(),
                "pad": self.viewport.pad,
            },
        });
        // Only a figure that names its poster says so, so the others' IR is as it was.
        if let Some(poster) = &self.poster {
            scene["poster"] = json!({
                "time": poster.time,
                "params": poster.params.iter().map(|(name, value)| json!([name, value])).collect::<Vec<_>>(),
            });
        }
        scene
    }
}
