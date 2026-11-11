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
    /// The param only takes the values its range's start plus a whole number of these, as a
    /// whole number does with a step of 1.
    pub step: Option<f64>,
}

impl Param {
    /// The value it takes: the nearest step, if it has steps.
    pub fn stepped(&self, value: f64) -> f64 {
        match self.step {
            Some(step) => self.range[0] + ((value - self.range[0]) / step).round() * step,
            None => value,
        }
    }

    /// A reader's value for it: within its range, and on a step. The range's end counts only if
    /// it's on one.
    pub fn settle(&self, value: f64) -> f64 {
        let [lo, hi] = self.range;
        let top = match self.step {
            Some(step) => lo + ((hi - lo) / step + 1e-9).floor() * step,
            None => hi,
        };
        self.stepped(value.clamp(lo, top.max(lo)))
    }
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
        values: Vec<Expr>,
        at: [Expr; 2],
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

/// Something drawn, in document order.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct System {
    pub unknowns: Vec<String>,
    pub start: Vec<Expr>,
    pub equations: Vec<Expr>,
    pub jacobian: Vec<Expr>,
    /// Every other name it reads. The answer depends on these and nothing else.
    pub depends: Vec<String>,
}

/// What a collection draws, for each combination of its variables.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Hang {
    #[default]
    Start,
    Middle,
    End,
}

/// Where an expression can sit: a moving point, and an offset from it in `px`.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    pub id: String,
    pub at: [Expr; 2],
    pub offset: [Expr; 2],
    pub hang: Hang,
}

/// One written expression in a state.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub spot: usize,
    pub keys: Vec<String>,
}

/// A list of states, driven by a binding: at 3.4 it's 40% of the way from state 3 to state 4
/// by time, before easing. State 0 is the empty one before the first that's written.
#[derive(Clone, Debug, PartialEq)]
pub struct Expression {
    pub id: String,
    pub by: Expr,
    pub states: Vec<Vec<Piece>>,
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
pub struct Family {
    pub vars: Vec<(String, [Expr; 2])>,
    pub lets: Vec<(String, Expr)>,
    pub arc: Arc,
}

/// `1em` where nothing says otherwise: the site's size for naming a point.
pub const DEFAULT_EM: f64 = 17.0;

#[derive(Clone, Debug, PartialEq)]
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
    /// Each beat's move in seconds, when beats have lengths, and empty when they don't.
    pub beat_lengths: Vec<f64>,
    pub handles: Vec<Handle>,
    pub spots: Vec<Spot>,
    pub expressions: Vec<Expression>,
    pub systems: Vec<System>,
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
                    at,
                    offset,
                    away,
                } => {
                    let mut label = json!({
                        "kind": "label",
                        "text": text,
                        "values": values.iter().map(Expr::to_mathjson).collect::<Vec<_>>(),
                        "at": pair(at),
                        "offset": pair(offset),
                    });
                    if let Some(away) = away {
                        label["away"] = pair(away);
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
        json!({
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
                "param": h.param,
                "inverse": h.inverse.to_mathjson(),
            })).collect::<Vec<_>>(),
            "params": self.params.iter().map(|p| match p.step {
                Some(step) => json!({ "id": p.id, "range": p.range, "step": step }),
                None => json!({ "id": p.id, "range": p.range }),
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
        })
    }
}
