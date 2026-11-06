//! Resolves stylesheets against a scene. Style values are expressions, so a fill
//! can depend on `time`, a param or `self.dist`. Nothing here changes the scene's IR.
//!
//! Facets: `::negative` and `::positive` are the field's raw sign. `::inside` and
//! `::outside` are aliases for them under the gradient convention, where the
//! negative side is inside. Later declarations win (SPEC/15's layers, without
//! specificity).

use crate::compile::{CONSTANTS, SIGNALS};
use crate::error::Error;
use crate::ir::{Expr, Form, Scene};
use crate::parse::{Document, Item};

/// The distance to a mark, per pixel. Only legal in spatial properties.
pub const SELF_DIST: &str = "self.dist";
/// Arc-length position along a stroke, 0 to 1. Only legal in `width`.
pub const SELF_T: &str = "self.t";
pub const COLORS: [&str; 2] = ["white", "black"];

/// The space a mark's edges blend in. The default is gamma-encoded sRGB, because
/// that's what SVG and CSS do and SVG's look is the quality bar. Linear light makes
/// dark lines on a light ground visibly thinner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Blend {
    #[default]
    Gamma,
    Linear,
}

/// A rendering hint. It chooses how a stroke is drawn, never what it means, so every
/// choice has to agree with the others within tolerance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Draw {
    /// The form's fast path: an exact SDF per pixel, anything else as a line.
    #[default]
    Auto,
    /// From the field's distance per pixel. Only for curves with a field.
    Field,
    /// As a line of segments: sampled if parametric, traced if implicit.
    Line,
}

/// How a line ends. Square is SVG's default, and round is what the blog uses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Cap {
    #[default]
    Butt,
    Round,
}

/// A label's typeface, as a role. What each role maps to belongs to the renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Font {
    #[default]
    Sans,
    /// Italic serif, for math.
    Math,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkStyle {
    pub negative: Option<Expr>,
    pub positive: Option<Expr>,
    pub stroke: Option<Expr>,
    /// In style pixels. Doesn't vary per pixel.
    pub width: Option<Expr>,
    pub draw: Draw,
    pub dash: Option<Expr>,
    pub gap: Option<Expr>,
    pub blend: Blend,
    pub cap: Cap,
    pub font: Font,
    pub opacity: Option<Expr>,
    /// A dot's radius in style pixels.
    pub radius: Option<Expr>,
    /// A label's text size in style pixels.
    pub size: Option<Expr>,
    /// The width of a label's halo in style pixels.
    pub halo: Option<Expr>,
}

impl MarkStyle {
    /// Whether this mark's stroke needs a line of segments, for this form.
    pub fn stroke_is_line(&self, form: &Form) -> bool {
        if self.stroke.is_none() {
            return false;
        }
        match form {
            Form::Field { exact, .. } => match self.draw {
                Draw::Line => true,
                Draw::Field => false,
                Draw::Auto => !exact || self.needs_arc_length(),
            },
            Form::Parametric(_) | Form::Polyline { .. } | Form::Region(_) => true,
            Form::Dot(_) | Form::Label { .. } => false,
        }
    }

    /// Dashes and tapers depend on position along the curve, which a field can't give.
    pub fn needs_arc_length(&self) -> bool {
        self.dash.is_some()
            || self.gap.is_some()
            || self.width.as_ref().is_some_and(|w| uses(w, SELF_T))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Styled {
    /// One entry per `scene.marks`, in the same order.
    pub marks: Vec<MarkStyle>,
    pub paper: Option<Expr>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ty {
    Real,
    Color,
}

fn has_pixels(e: &Expr) -> bool {
    match e {
        Expr::App(head, args) => head == "Pixels" || args.iter().any(has_pixels),
        _ => false,
    }
}

fn uses(e: &Expr, name: &str) -> bool {
    let mut found = false;
    e.visit_syms(&mut |s| found |= s == name);
    found
}

/// Checks an expression's type, and that it only uses names in scope.
pub fn type_of(e: &Expr, names: &[&str], spatial: bool) -> Result<Ty, String> {
    match e {
        Expr::Num(_) => Ok(Ty::Real),
        Expr::Sym(s) if s == SELF_DIST => {
            if spatial {
                Ok(Ty::Real)
            } else {
                Err("`self.dist` is only legal in spatial properties (`fill`, `stroke`)".into())
            }
        }
        Expr::Sym(s) if COLORS.contains(&s.as_str()) => Ok(Ty::Color),
        Expr::Sym(s)
            if names.contains(&s.as_str())
                || SIGNALS.contains(&s.as_str())
                || CONSTANTS.contains(&s.as_str()) =>
        {
            Ok(Ty::Real)
        }
        Expr::Sym(s) => Err(format!("unknown name `{s}`")),
        Expr::App(head, args) => {
            let tys = args
                .iter()
                .map(|a| type_of(a, names, spatial))
                .collect::<Result<Vec<_>, _>>()?;
            use Ty::{Color, Real};
            match (head.as_str(), tys.as_slice()) {
                ("Oklch" | "Rgb", [Real, Real, Real]) => Ok(Color),
                ("Hue", [Real]) => Ok(Color),
                ("Mix", [Color, Color, Real]) => Ok(Color),
                ("Mix" | "Clamp" | "Smoothstep", [Real, Real, Real]) => Ok(Real),
                (
                    "Add" | "Subtract" | "Multiply" | "Divide" | "Power" | "Min" | "Max" | "Mod"
                    | "Arctan2",
                    [Real, Real],
                ) => Ok(Real),
                (
                    "Negate" | "Sqrt" | "Sin" | "Cos" | "Tan" | "Exp" | "Ln" | "Abs" | "Sign"
                    | "Pixels",
                    [Real],
                ) => Ok(Real),
                _ => Err(format!("`{head}` doesn't take {tys:?}")),
            }
        }
    }
}

pub fn resolve(scene: &Scene, sheets: &[Document]) -> Result<Styled, Error> {
    let mut styled = Styled {
        marks: vec![MarkStyle::default(); scene.marks.len()],
        paper: None,
    };
    let names: Vec<&str> = scene.bindings.iter().map(|b| b.id.as_str()).collect();

    for sheet in sheets {
        for item in &sheet.items {
            let Item::Style(rule) = item else { continue };
            let at = |message: String| Error::at(rule.line, 1, message);
            for selector in &rule.selectors {
                if selector.target == "paper" {
                    for (prop, value) in &rule.decls {
                        if prop != "fill" || selector.facet.is_some() {
                            return Err(at(format!("`paper` only takes `fill`, not `{prop}`")));
                        }
                        if type_of(value, &names, false).map_err(&at)? != Ty::Color {
                            return Err(at("`fill` needs a Color".into()));
                        }
                        styled.paper = Some(value.clone());
                    }
                    continue;
                }
                let Some(index) = scene.marks.iter().position(|m| m.id == selector.target) else {
                    return Err(at(format!("no mark named `{}`", selector.target)));
                };
                let form = &scene.marks[index].form;
                let spatial_ok =
                    matches!(form, Form::Field { .. } | Form::Region(_) | Form::Dot(_));
                let target = &mut styled.marks[index];
                for (prop, value) in &rule.decls {
                    if let (None, "draw" | "blend" | "cap" | "font") =
                        (selector.facet.as_deref(), prop.as_str())
                    {
                        let word = match value {
                            Expr::Sym(word) => word.as_str(),
                            _ => return Err(at(format!("`{prop}` needs a keyword"))),
                        };
                        match (prop.as_str(), word) {
                            ("draw", "auto") => target.draw = Draw::Auto,
                            ("draw", "line") => target.draw = Draw::Line,
                            ("draw", "field") => {
                                if !matches!(form, Form::Field { .. }) {
                                    return Err(at(format!(
                                        "`{}` has no field, so it can't use `draw: field`",
                                        selector.target
                                    )));
                                }
                                target.draw = Draw::Field;
                            }
                            ("blend", "linear") => target.blend = Blend::Linear,
                            ("blend", "gamma") => target.blend = Blend::Gamma,
                            ("cap", "butt") => target.cap = Cap::Butt,
                            ("cap", "round") => target.cap = Cap::Round,
                            ("font", "sans") => target.font = Font::Sans,
                            ("font", "math") => target.font = Font::Math,
                            _ => return Err(at(format!("`{prop}` can't be `{word}`"))),
                        }
                        continue;
                    }
                    let measure = matches!(
                        prop.as_str(),
                        "width" | "dash" | "gap" | "opacity" | "radius" | "size" | "halo"
                    );
                    let mut scope = names.clone();
                    if prop == "width" {
                        scope.push(SELF_T);
                    }
                    let ty = type_of(value, &scope, !measure).map_err(&at)?;
                    let want = if measure { Ty::Real } else { Ty::Color };
                    if ty != want {
                        return Err(at(format!("`{prop}` needs a {want:?}, found a {ty:?}")));
                    }
                    if !measure && !spatial_ok && uses(value, SELF_DIST) {
                        return Err(at(format!(
                            "`{}` has no distance, so it can't use `self.dist`",
                            selector.target
                        )));
                    }
                    let slot = match (selector.facet.as_deref(), prop.as_str()) {
                        (Some("inside" | "negative"), "fill") => &mut target.negative,
                        (Some("outside" | "positive"), "fill") => &mut target.positive,
                        (None, "stroke") => &mut target.stroke,
                        (None, "opacity") => &mut target.opacity,
                        (None, "width" | "dash" | "gap" | "radius" | "size" | "halo") => {
                            if !has_pixels(value) {
                                return Err(at(format!("`{prop}` needs a unit, such as `2px`")));
                            }
                            match prop.as_str() {
                                "width" => &mut target.width,
                                "dash" => &mut target.dash,
                                "gap" => &mut target.gap,
                                "radius" => &mut target.radius,
                                "size" => &mut target.size,
                                _ => &mut target.halo,
                            }
                        }
                        (None, "fill") => {
                            if matches!(form, Form::Field { .. }) {
                                return Err(at(
                                    "`fill` needs a facet, such as `::inside`, for now".into(),
                                ));
                            }
                            &mut target.negative
                        }
                        (Some(facet), _)
                            if !matches!(facet, "inside" | "outside" | "negative" | "positive") =>
                        {
                            return Err(at(format!("unknown facet `::{facet}`")));
                        }
                        (Some(facet), prop) => {
                            return Err(at(format!("`{prop}` doesn't apply to `::{facet}`")));
                        }
                        (None, prop) => {
                            return Err(at(format!("unknown property `{prop}`")));
                        }
                    };
                    *slot = Some(value.clone());
                }
            }
        }
    }
    for (mark, style) in scene.marks.iter().zip(&styled.marks) {
        if style.needs_arc_length() && style.draw == Draw::Field {
            return Err(Error::new(format!(
                "`{}`: dashes and tapers need position along the curve, so they can't use `draw: field`",
                mark.id
            )));
        }
    }
    Ok(styled)
}

#[cfg(test)]
mod tests {
    use super::{Blend, Draw};
    use crate::Error;
    use crate::build;
    use crate::ir::Expr;

    const DOC: &str = "curve c = implicit (x, y): x^2 + y^2 = 1";

    fn styled(sheet: &str) -> Result<crate::Figure, Error> {
        build(DOC, &[sheet])
    }

    #[test]
    fn aliases_write_the_same_slot_and_later_rules_win() {
        let f = styled(
            "style c::inside { fill: white }
             style c::negative { fill: black }",
        )
        .unwrap();
        assert_eq!(f.styles.marks[0].negative, Some(Expr::sym("black")));
        assert_eq!(f.styles.marks[0].positive, None);
    }

    #[test]
    fn later_sheets_win_and_the_document_comes_last() {
        let doc = format!("{DOC}\nstyle c::inside {{ fill: black }}");
        let f = build(&doc, &["style c::inside { fill: white }"]).unwrap();
        assert_eq!(f.styles.marks[0].negative, Some(Expr::sym("black")));
        let f = build(
            DOC,
            &[
                "style c::inside { fill: black }",
                "style c::inside { fill: white }",
            ],
        )
        .unwrap();
        assert_eq!(f.styles.marks[0].negative, Some(Expr::sym("white")));
    }

    #[test]
    fn a_stylesheet_never_changes_the_scene() {
        let plain = build(DOC, &[]).unwrap();
        let styled = styled("style c::inside { fill: mix(white, black, 0.5) }").unwrap();
        assert_eq!(plain.scene, styled.scene);
    }

    #[test]
    fn style_values_can_use_signals() {
        styled("style c::inside { fill: oklch(0.5 + 0.2 * sin(time), 0.1, 40) }").unwrap();
    }

    #[test]
    fn self_dist_is_spatial_only() {
        styled("style c::inside { fill: mix(white, black, self.dist) }").unwrap();
        let err = styled("style c { width: 2px + self.dist }").unwrap_err();
        assert!(err.message.contains("spatial"), "{err}");
    }

    #[test]
    fn stroke_properties_resolve() {
        let f = styled(
            "style c { stroke: black; width: 1px + 9px * self.t; dash: 4px; gap: 2px;
                       draw: line; blend: linear }",
        )
        .unwrap();
        let s = &f.styles.marks[0];
        assert!(s.dash.is_some() && s.gap.is_some());
        assert_eq!(s.draw, Draw::Line);
        assert_eq!(s.blend, Blend::Linear);
    }

    #[test]
    fn draw_defaults_to_each_forms_fast_path() {
        let doc = "curve i = implicit (x, y): x = 0
                   curve s = sdf (x, y): x
                   curve p = parametric (t): (t, t) for t in 0..1";
        let sheet = "style i, s, p { stroke: black; width: 1px }";
        let f = build(doc, &[sheet]).unwrap();
        let line = |i: usize| f.styles.marks[i].stroke_is_line(&f.scene.marks[i].form);
        assert!(line(0), "implicit strokes trace");
        assert!(!line(1), "an exact SDF strokes per pixel");
        assert!(line(2), "parametric strokes sample");

        // Dashes need arc length, even on an SDF.
        let f = build(doc, &[&format!("{sheet}\nstyle s {{ dash: 4px }}")]).unwrap();
        assert!(f.styles.marks[1].stroke_is_line(&f.scene.marks[1].form));
    }

    #[test]
    fn hints_can_override_the_default() {
        let doc = "curve i = implicit (x, y): x = 0\ncurve s = sdf (x, y): x";
        let f = build(
            doc,
            &["style i { stroke: black; width: 1px; draw: field }
               style s { stroke: black; width: 1px; draw: line }"],
        )
        .unwrap();
        assert!(!f.styles.marks[0].stroke_is_line(&f.scene.marks[0].form));
        assert!(f.styles.marks[1].stroke_is_line(&f.scene.marks[1].form));
    }

    #[test]
    fn mistakes_fail_loudly() {
        for (sheet, expected) in [
            (
                "style c { dash: 4px; draw: field }",
                "can't use `draw: field`",
            ),
            (
                "style c { width: 1px + 2px * self.t; draw: field }",
                "can't use `draw: field`",
            ),
            ("style c { dash: 4 }", "needs a unit"),
            ("style c { dash: self.t * 1px }", "unknown name `self.t`"),
            ("style c { draw: sketchy }", "can't be `sketchy`"),
            ("style c { cap: wobbly }", "can't be `wobbly`"),
            ("style c { blend: 2 }", "needs a keyword"),
            ("style nope { stroke: white }", "no mark named"),
            ("style c::sideways { fill: white }", "unknown facet"),
            ("style c { fill: white }", "needs a facet"),
            ("style c { width: 2 }", "needs a unit"),
            ("style c::inside { fill: 2 }", "needs a Color"),
            ("style c::inside { fill: oklch(1, 2) }", "doesn't take"),
            ("style c { glow: white }", "unknown property"),
            ("style c::inside { fill: mystery }", "unknown name"),
            ("style paper { stroke: white }", "only takes `fill`"),
        ] {
            let err = styled(sheet).expect_err(sheet);
            assert!(err.message.contains(expected), "{sheet}: {err}");
        }
    }

    #[test]
    fn forms_without_a_field_or_distance_say_so() {
        let doc = "curve p = parametric (t): (t, t) for t in 0..1";
        let err = build(doc, &["style p { draw: field }"]).unwrap_err();
        assert!(err.message.contains("has no field"), "{err}");
        let err = build(doc, &["style p { stroke: mix(white, black, self.dist) }"]).unwrap_err();
        assert!(err.message.contains("has no distance"), "{err}");
    }
}
