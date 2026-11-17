//! Burns labels into a frame, for output with no page to overlay text on.
//!
//! Glyphs keep fractional positions, so a moving label glides instead of jumping between
//! whole pixels. `lab/text` compared this with the alternatives: it's within 0.02 RMS
//! coverage of exact at about 0.5 ms for 269 glyphs. Kerning comes from the font's `kern`
//! table, and there's no shaping yet.
//!
//! The fonts are the package's, `packages/figure/fonts`, built into the command, so a frame is
//! the same on any machine and the same faces a page draws with.

use std::error::Error;
use std::path::Path;

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont, point};
use pencil_core::eval::srgb_encode;
use pencil_core::formula::{Face, Rule};
use pencil_core::frame::Label;
use pencil_core::style::Font as FontRole;
use pencil_core::terms::TextWidths;

/// Libertinus 7.051, under the SIL Open Font License 1.1 (`packages/figure/fonts/OFL.txt`).
const SANS: &[u8] = include_bytes!("../../../packages/figure/fonts/LibertinusSans-Regular.otf");
const ITALIC: &[u8] = include_bytes!("../../../packages/figure/fonts/LibertinusSerif-Italic.otf");
const MATH: &[u8] = include_bytes!("../../../packages/figure/fonts/LibertinusMath-Regular.otf");

/// A role's faces, in order: a character is drawn by the first that has it, so a text face
/// can leave `ℝ`, `∇` and the arrows to a math font behind it.
pub struct Role {
    faces: Vec<FontArc>,
}

/// One glyph of a line: which face drew it, and where its pen is from the line's start.
struct Placed {
    face: usize,
    id: GlyphId,
    pen: f32,
}

impl Role {
    fn new(faces: Vec<FontArc>) -> Self {
        assert!(!faces.is_empty());
        Self { faces }
    }

    /// The first face with the character. With none, the first face's missing glyph.
    fn glyph(&self, c: char) -> (usize, GlyphId) {
        self.faces
            .iter()
            .enumerate()
            .map(|(i, face)| (i, face.glyph_id(c)))
            .find(|(_, id)| id.0 != 0)
            .unwrap_or((0, GlyphId(0)))
    }

    /// A line's glyphs and its advance. Kerning is each face's own, between its own glyphs.
    fn line(&self, text: &str, size: f32) -> (Vec<Placed>, f32) {
        let mut advance = 0.0f32;
        let mut placed: Vec<Placed> = Vec::with_capacity(text.len());
        for c in text.chars() {
            let (face, id) = self.glyph(c);
            let font = &self.faces[face];
            let scaled = font.as_scaled(em_scale(font, size));
            if let Some(last) = placed.last()
                && last.face == face
            {
                advance += scaled.kern(last.id, id);
            }
            placed.push(Placed {
                face,
                id,
                pen: advance,
            });
            advance += scaled.h_advance(id);
        }
        (placed, advance)
    }

    /// The line box is the first face's, whatever a fallback's is.
    fn ascent_descent(&self, size: f32) -> (f32, f32) {
        let font = &self.faces[0];
        let scaled = font.as_scaled(em_scale(font, size));
        (scaled.ascent(), scaled.descent())
    }
}

pub struct Fonts {
    sans: Role,
    math: Role,
    /// The math font, which a label's mathematics is set in whatever its role.
    formula: FontArc,
}

fn bundled(bytes: &'static [u8]) -> FontArc {
    FontArc::try_from_slice(bytes).expect("the fonts in packages/figure/fonts are fonts")
}

impl Fonts {
    /// `--font` replaces the text face of both roles. The math font stays behind it, for what
    /// it lacks.
    pub fn load(path: Option<&Path>) -> Result<Self, Box<dyn Error>> {
        let math = bundled(MATH);
        let (sans, italic) = match path {
            None => (bundled(SANS), bundled(ITALIC)),
            Some(p) => {
                let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
                let font =
                    FontArc::try_from_vec(bytes).map_err(|e| format!("{}: {e}", p.display()))?;
                (font.clone(), font)
            }
        };
        Ok(Self {
            sans: Role::new(vec![sans, math.clone()]),
            math: Role::new(vec![italic, math.clone()]),
            formula: math,
        })
    }

    fn of(&self, role: FontRole) -> &Role {
        match role {
            FontRole::Sans => &self.sans,
            FontRole::Math => &self.math,
        }
    }
}

impl TextWidths for Fonts {
    fn width(&self, text: &str, size: f64, font: FontRole) -> f64 {
        self.of(font).line(text, size as f32).1 as f64
    }
}

/// `ab_glyph` scales by line height, but a font size means the em, as it does in CSS.
fn em_scale(font: &FontArc, size: f32) -> f32 {
    size * font.height_unscaled() / font.units_per_em().unwrap_or(1000.0)
}

/// A label's text coverage, on a canvas with room for its halo.
struct Bitmap {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    coverage: Vec<f32>,
}

impl Bitmap {
    /// Room for ink from `left` to `right` and `top` to `bottom`, in frame pixels, and a halo.
    fn new(left: f64, top: f64, right: f64, bottom: f64, halo: f64) -> Self {
        let margin = halo.ceil() as i64 + 3;
        let x = left.floor() as i64 - margin;
        let y = top.floor() as i64 - margin;
        let width = right.ceil() as i64 + margin + 1 - x;
        let height = bottom.ceil() as i64 + margin + 1 - y;
        Self {
            x,
            y,
            width,
            height,
            coverage: vec![0.0; (width * height) as usize],
        }
    }

    /// One glyph with its origin at `(x, y)` in frame pixels, `tall` times its font's height.
    fn glyph(&mut self, font: &FontArc, id: GlyphId, size: f32, tall: f32, x: f64, y: f64) {
        let scale = em_scale(font, size);
        let positioned = id.with_scale_and_position(
            PxScale {
                x: scale,
                y: scale * tall,
            },
            point((x - self.x as f64) as f32, (y - self.y as f64) as f32),
        );
        let Some(outlined) = font.outline_glyph(positioned) else {
            return;
        };
        let bounds = outlined.px_bounds();
        let (width, height) = (self.width, self.height);
        let coverage = &mut self.coverage;
        outlined.draw(|gx, gy, c| {
            let (cx, cy) = (
                bounds.min.x as i64 + gx as i64,
                bounds.min.y as i64 + gy as i64,
            );
            if cx >= 0 && cy >= 0 && cx < width && cy < height {
                let slot = &mut coverage[(cy * width + cx) as usize];
                *slot = slot.max(c);
            }
        });
    }

    /// A rule, by how much of each pixel it covers.
    fn rule(&mut self, rule: &Rule, at: [f64; 2]) {
        let (left, top) = (
            at[0] + rule.x - self.x as f64,
            at[1] + rule.y - self.y as f64,
        );
        let (right, bottom) = (left + rule.width, top + rule.height);
        for y in (top.floor() as i64).max(0)..(bottom.ceil() as i64).min(self.height) {
            let rows = (bottom.min(y as f64 + 1.0) - top.max(y as f64)).clamp(0.0, 1.0);
            for x in (left.floor() as i64).max(0)..(right.ceil() as i64).min(self.width) {
                let columns = (right.min(x as f64 + 1.0) - left.max(x as f64)).clamp(0.0, 1.0);
                let slot = &mut self.coverage[(y * self.width + x) as usize];
                *slot = slot.max((rows * columns) as f32);
            }
        }
    }

    /// A line of a role's text from `left`, on `baseline`.
    fn line(&mut self, role: &Role, text: &str, size: f32, left: f64, baseline: f64) {
        for glyph in &role.line(text, size).0 {
            let font = &role.faces[glyph.face];
            self.glyph(font, glyph.id, size, 1.0, left + glyph.pen as f64, baseline);
        }
    }
}

fn rasterize(label: &Label, fonts: &Fonts) -> Bitmap {
    let role = fonts.of(label.font);
    let size = label.size as f32;
    let Some(laid) = &label.laid else {
        let advance = role.line(&label.text, size).1;
        let (ascent, descent) = role.ascent_descent(size);
        // Center the advance box and line box on the label's point, keeping the fractions.
        let left = label.x - advance as f64 / 2.0;
        let top = label.y - (ascent - descent) as f64 / 2.0;
        let mut bitmap = Bitmap::new(
            left,
            top,
            left + advance as f64,
            top + (ascent - descent) as f64,
            label.halo,
        );
        let baseline = label.y + (ascent + descent) as f64 / 2.0;
        bitmap.line(role, &label.text, size, left, baseline);
        return bitmap;
    };
    // What core laid out: runs and rules at their own places from the label's point.
    let line = label.size * 0.6;
    let mut bitmap = Bitmap::new(
        label.x - laid.width / 2.0,
        label.y - laid.above.max(line),
        label.x + laid.width / 2.0,
        label.y + laid.below.max(line),
        label.halo,
    );
    for run in &laid.runs {
        let (x, y) = (label.x + run.x, label.y + run.y);
        match run.face {
            Face::Text => bitmap.line(role, &run.text, run.size as f32, x, y),
            Face::Math => {
                let font = &fonts.formula;
                let scaled = font.as_scaled(em_scale(font, run.size as f32));
                let mut pen = 0.0f32;
                for c in run.text.chars() {
                    let id = font.glyph_id(c);
                    let tall = run.stretch as f32;
                    bitmap.glyph(font, id, run.size as f32, tall, x + pen as f64, y);
                    pen += scaled.h_advance(id);
                }
            }
        }
    }
    for rule in &laid.rules {
        bitmap.rule(rule, [label.x, label.y]);
    }
    bitmap
}

/// A halo is the text grown by its radius, with soft edges from the text's own coverage.
fn dilate(bitmap: &Bitmap, radius: f64) -> Vec<f32> {
    if radius <= 0.0 {
        return Vec::new();
    }
    let reach = radius.ceil() as i64;
    let mut out = vec![0.0f32; bitmap.coverage.len()];
    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            let value = bitmap.coverage[(y * bitmap.width + x) as usize];
            if value <= 0.0 {
                continue;
            }
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= bitmap.width || ny >= bitmap.height {
                        continue;
                    }
                    let edge = (radius + 0.5 - ((dx * dx + dy * dy) as f64).sqrt()).clamp(0.0, 1.0);
                    let slot = &mut out[(ny * bitmap.width + nx) as usize];
                    *slot = slot.max(value * edge as f32);
                }
            }
        }
    }
    out
}

/// Blends over RGBA8 sRGB bytes, so this is gamma-space blending, as SVG's text does.
fn blend(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    bitmap: &Bitmap,
    mask: &[f32],
    color: [f64; 3],
    opacity: f64,
) {
    let color = color.map(|c| srgb_encode(c) * 255.0);
    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            let (fx, fy) = (bitmap.x + x, bitmap.y + y);
            if fx < 0 || fy < 0 || fx >= frame_width as i64 || fy >= frame_height as i64 {
                continue;
            }
            let a = mask[(y * bitmap.width + x) as usize] as f64 * opacity;
            if a <= 0.0 {
                continue;
            }
            let at = ((fy * frame_width as i64 + fx) * 4) as usize;
            for (i, c) in color.iter().enumerate() {
                let under = frame[at + i] as f64;
                frame[at + i] = (under + (c - under) * a).round() as u8;
            }
        }
    }
}

/// A label's leader, by the same exact coverage as every other line, in its text's color.
fn draw_leader(frame: &mut [u8], width: u32, height: u32, label: &Label, ends: [[f64; 2]; 2]) {
    let Some(leader) = label.leader else { return };
    let [a, b] = ends;
    let length = (b[0] - a[0]).hypot(b[1] - a[1]);
    let r = (leader.width / 2.0) as f32;
    let segment = [
        a[0] as f32,
        a[1] as f32,
        b[0] as f32,
        b[1] as f32,
        r,
        r,
        0.0,
        length as f32,
        0.0,
        0.0,
        length as f32,
        0.0,
    ];
    let color = label.color.map(|c| srgb_encode(c) * 255.0);
    let pad = leader.width / 2.0 + 2.0;
    let x0 = (a[0].min(b[0]) - pad).floor().max(0.0) as u32;
    let y0 = (a[1].min(b[1]) - pad).floor().max(0.0) as u32;
    let x1 = ((a[0].max(b[0]) + pad).ceil().max(0.0) as u32).min(width);
    let y1 = ((a[1].max(b[1]) + pad).ceil().max(0.0) as u32).min(height);
    for y in y0..y1 {
        for x in x0..x1 {
            let c = pencil_core::stroke::segment_coverage(&segment, x as f64 + 0.5, y as f64 + 0.5)
                * label.opacity;
            if c <= 0.0 {
                continue;
            }
            let at = ((y * width + x) * 4) as usize;
            for (i, v) in color.iter().enumerate() {
                let under = frame[at + i] as f64;
                frame[at + i] = (under + (v - under) * c).round() as u8;
            }
        }
    }
}

pub fn draw_labels(
    frame: &mut [u8],
    width: u32,
    height: u32,
    labels: &[Label],
    paper: [f64; 3],
    fonts: &Fonts,
) {
    let visible: Vec<&Label> = labels.iter().filter(|l| l.opacity > 0.0).collect();
    // Leaders under everything, since they stop short of their text anyway.
    for label in &visible {
        let half = label.half(fonts);
        if let Some(ends) = pencil_core::frame::leader_ends(label, half) {
            draw_leader(frame, width, height, label, ends);
        }
    }
    let bitmaps: Vec<_> = visible
        .iter()
        .map(|label| rasterize(label, fonts))
        .collect();
    // Every halo first, so one label's can't eat into a neighbor, as in an expression's terms.
    for (label, bitmap) in visible.iter().zip(&bitmaps) {
        // Like SVG's stroke-width, `halo` is the full width, centered on the outline.
        let halo = dilate(bitmap, label.halo / 2.0);
        if !halo.is_empty() {
            blend(frame, width, height, bitmap, &halo, paper, label.opacity);
        }
    }
    for (label, bitmap) in visible.iter().zip(&bitmaps) {
        blend(
            frame,
            width,
            height,
            bitmap,
            &bitmap.coverage,
            label.color,
            label.opacity,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(text: &str, x: f64, y: f64, size: f64) -> Label {
        Label {
            id: "l".to_string(),
            text: text.to_string(),
            x,
            y,
            size,
            color: [0.0; 3],
            halo: 0.0,
            opacity: 1.0,
            font: FontRole::Sans,
            anchor: [x, y],
            place: Default::default(),
            leader: None,
            laid: None,
        }
    }

    fn fonts() -> Fonts {
        Fonts::load(None).expect("the fonts are built in")
    }

    /// What the system fonts drew as boxes: each role has a face for every one of these.
    #[test]
    fn a_math_figures_glyphs_are_all_there() {
        let fonts = fonts();
        let needed = "αβγθλμπφωΔΣΩ→←↔⇒↦ℝℂℤℕℚ∇∂∞∫∑√−·×±≤≥≠≈∈⁰¹²³⁴⁵⁶⁷⁸⁹ⁱⁿ₀₁₂₃₄₅₆₇₈₉ℯ|°";
        for role in [FontRole::Sans, FontRole::Math] {
            let missing: String = needed
                .chars()
                .filter(|c| fonts.of(role).glyph(*c).1.0 == 0)
                .collect();
            assert!(missing.is_empty(), "{role:?} lacks {missing}");
        }
    }

    /// Where a label's ink is centered in the frame, in pixels.
    fn ink_center(fonts: &Fonts, label: &Label) -> (f64, f64) {
        let bitmap = rasterize(label, fonts);
        let (mut total, mut mx, mut my) = (0.0f64, 0.0f64, 0.0f64);
        for y in 0..bitmap.height {
            for x in 0..bitmap.width {
                let c = bitmap.coverage[(y * bitmap.width + x) as usize] as f64;
                total += c;
                mx += c * ((bitmap.x + x) as f64 + 0.5);
                my += c * ((bitmap.y + y) as f64 + 0.5);
            }
        }
        (mx / total, my / total)
    }

    /// Core lays mathematics out from numbers its build read out of the math font, and this
    /// draws with the font itself, so they have to be the same font's.
    #[test]
    fn cores_metrics_are_this_fonts() {
        let fonts = fonts();
        let font = &fonts.formula;
        let scaled = font.as_scaled(em_scale(font, 1000.0));
        for (source, drawn) in [("sin", "sin"), ("xyz", "𝑥𝑦𝑧"), ("ℝ∇∂", "ℝ∇∂"), ("2π", "2𝜋")]
        {
            let laid = pencil_core::formula::lay_math(source, 1000.0);
            let text: String = laid.runs.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(text, drawn);
            let advance: f32 = drawn
                .chars()
                .map(|c| scaled.h_advance(font.glyph_id(c)))
                .sum();
            // Spacing between atoms isn't glyphs', so these are ones set solid.
            let wide = laid.runs.last().map(|r| r.x).unwrap() - laid.runs[0].x
                + advance_of(&scaled, font, &laid.runs.last().unwrap().text);
            assert!(
                (wide - advance as f64).abs() < 0.01,
                "{source}: {wide} {advance}"
            );
        }
        // And every glyph a run is set in is one the font has.
        let laid = pencil_core::formula::lay_math("f: ℝ → ℝ, ∇f(x_0) ≤ √(a/b) ≠ e^(iπ)", 20.0);
        for run in &laid.runs {
            for c in run.text.chars() {
                assert_ne!(font.glyph_id(c).0, 0, "{c}");
            }
        }
    }

    fn advance_of(scaled: &ab_glyph::PxScaleFont<&FontArc>, font: &FontArc, text: &str) -> f64 {
        text.chars()
            .map(|c| scaled.h_advance(font.glyph_id(c)) as f64)
            .sum()
    }

    /// What `packages/figure/tests/formulas.spec.js` holds a page to: the same widths, from the
    /// same files, with no kerning on either side.
    #[test]
    fn the_widths_a_page_is_held_to() {
        let fonts = fonts();
        let wide = |text: &str, role| (fonts.width(text, 1000.0, role) * 10.0).round() / 10.0;
        assert_eq!(wide("arg z", FontRole::Math), ARG_Z);
        assert_eq!(wide("Pencil", FontRole::Sans), PENCIL);
        // And a run of mathematics, which core measures and both draw.
        assert_eq!(pencil_core::formula::lay_math("sin", 1000.0).width, SIN);
    }
    const ARG_Z: f64 = 2006.0;
    const PENCIL: f64 = 2492.0;
    const SIN: f64 = 1203.0;

    /// A fraction is drawn: ink above its bar, the bar solid across, ink below.
    #[test]
    fn a_fraction_is_drawn_about_its_bar() {
        let fonts = fonts();
        let mut half = label("$1/2$", 100.0, 100.0, 60.0);
        half.laid = pencil_core::formula::lay(&half.text, half.size, half.font, &fonts);
        let laid = half.laid.clone().expect("laid out");
        let bitmap = rasterize(&half, &fonts);
        let bar = laid.rules[0];
        let row = |y: f64| -> f32 {
            let at = (y - bitmap.y as f64) as i64;
            (0..bitmap.width)
                .map(|x| bitmap.coverage[(at * bitmap.width + x) as usize])
                .sum()
        };
        let middle = half.y + bar.y + bar.height / 2.0;
        // The bar is solid for its width, give or take the pixels its ends share.
        assert!(
            (row(middle) as f64 - bar.width).abs() < 1.5,
            "{}",
            row(middle)
        );
        let ink = |from: f64, to: f64| -> f32 {
            ((from.ceil() as i64)..(to.floor() as i64))
                .map(|y| row(y as f64))
                .sum()
        };
        assert!(ink(half.y - laid.above, half.y + bar.y - 2.0) > 50.0);
        assert!(ink(half.y + bar.y + bar.height + 2.0, half.y + laid.below) > 50.0);
    }

    /// A root over something tall is the same glyph drawn taller, down to the same baseline.
    #[test]
    fn a_tall_root_is_its_glyph_stretched() {
        let fonts = fonts();
        let rows = |text: &str| {
            let mut root = label(text, 200.0, 200.0, 60.0);
            root.laid = pencil_core::formula::lay(&root.text, root.size, root.font, &fonts);
            let bitmap = rasterize(&root, &fonts);
            (0..bitmap.height)
                .filter(|y| {
                    (0..bitmap.width)
                        .any(|x| bitmap.coverage[(y * bitmap.width + x) as usize] > 0.5)
                })
                .count()
        };
        assert!(rows("$√(1/2)$") as f64 > rows("$√2$") as f64 * 1.3);
    }

    /// The bug this replaced: whole-pixel placement made moving labels jump.
    #[test]
    fn labels_move_by_fractions_of_a_pixel() {
        let fonts = fonts();
        let base = ink_center(&fonts, &label("e", 100.0, 100.0, 24.0));
        for k in 1..=16 {
            let d = k as f64 / 8.0;
            let moved = ink_center(&fonts, &label("e", 100.0 + d, 100.0 + d, 24.0));
            // Measured: the rasterizer's bias cycles each pixel and peaks at 0.024, far
            // below the 0.33 of whole-pixel placement.
            assert!((moved.0 - base.0 - d).abs() < 0.03, "x at {d}: {moved:?}");
            assert!((moved.1 - base.1 - d).abs() < 0.03, "y at {d}: {moved:?}");
        }
    }

    /// `ab_glyph` scales by line height, so an unconverted size draws about 1.3× too small.
    #[test]
    fn a_font_size_means_the_em() {
        let fonts = fonts();
        let role = fonts.of(FontRole::Sans);
        let font = &role.faces[0];
        let scaled = font.as_scaled(em_scale(font, 100.0));
        let em = scaled.ascent() - scaled.descent();
        let expected = font.height_unscaled() / font.units_per_em().unwrap() * 100.0;
        assert!((em - expected).abs() < 0.01);
        // A capital is most of an em in any ordinary font, which it isn't at the wrong scale.
        let bitmap = rasterize(&label("H", 100.0, 100.0, 100.0), &fonts);
        let rows = (0..bitmap.height)
            .filter(|y| {
                (0..bitmap.width).any(|x| bitmap.coverage[(y * bitmap.width + x) as usize] > 0.5)
            })
            .count();
        assert!((60..=80).contains(&rows), "cap height {rows}");
    }

    #[test]
    fn labels_center_on_their_point() {
        let fonts = fonts();
        let (x, _) = ink_center(&fonts, &label("H", 200.0, 100.0, 40.0));
        assert!((x - 200.0).abs() < 1.5, "{x}");
    }
}
