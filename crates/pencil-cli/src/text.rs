//! Burns labels into a frame, for output with no page to overlay text on.
//!
//! Glyphs keep fractional positions, so a moving label glides instead of jumping between
//! whole pixels. `lab/text` compared this with the alternatives: it's within 0.02 RMS
//! coverage of exact at about 0.5 ms for 269 glyphs. Kerning comes from the font's `kern`
//! table, and there's no shaping yet.

use std::error::Error;
use std::path::Path;

use ab_glyph::{Font, FontVec, GlyphId, ScaleFont, point};
use pencil_core::eval::srgb_encode;
use pencil_core::frame::Label;
use pencil_core::style::Font as FontRole;

/// System fonts close to the blog's labels. Not redistributable, so a real pipeline would
/// take fonts from the document or style. `(path, index in a font collection)`.
const SANS: [(&str, u32); 3] = [
    ("/System/Library/Fonts/SFNS.ttf", 0),
    ("/System/Library/Fonts/Helvetica.ttc", 0),
    ("/System/Library/Fonts/Supplemental/Arial.ttf", 0),
];
const MATH: [(&str, u32); 3] = [
    ("/System/Library/Fonts/Supplemental/Charter.ttc", 1),
    ("/System/Library/Fonts/Supplemental/Georgia Italic.ttf", 0),
    (
        "/System/Library/Fonts/Supplemental/Times New Roman Italic.ttf",
        0,
    ),
];

pub struct Fonts {
    sans: FontVec,
    math: FontVec,
}

fn first_font(candidates: &[(&str, u32)]) -> Result<FontVec, Box<dyn Error>> {
    for (path, index) in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if let Ok(font) = FontVec::try_from_vec_and_index(bytes, *index) {
            return Ok(font);
        }
    }
    Err("no system font found, so pass --font".into())
}

impl Fonts {
    /// `--font` replaces both roles, for machines without these system fonts.
    pub fn load(path: Option<&Path>) -> Result<Self, Box<dyn Error>> {
        if let Some(p) = path {
            let read = || -> Result<FontVec, Box<dyn Error>> {
                let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
                Ok(FontVec::try_from_vec(bytes)?)
            };
            return Ok(Self {
                sans: read()?,
                math: read()?,
            });
        }
        Ok(Self {
            sans: first_font(&SANS)?,
            math: first_font(&MATH)?,
        })
    }

    fn of(&self, role: FontRole) -> &FontVec {
        match role {
            FontRole::Sans => &self.sans,
            FontRole::Math => &self.math,
        }
    }
}

/// `ab_glyph` scales by line height, but a font size means the em, as it does in CSS.
fn em_scale(font: &FontVec, size: f32) -> f32 {
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

fn rasterize(label: &Label, font: &FontVec) -> Bitmap {
    let px = em_scale(font, label.size as f32);
    let scaled = font.as_scaled(px);
    let ids: Vec<GlyphId> = label.text.chars().map(|c| font.glyph_id(c)).collect();
    let mut advance = 0.0f32;
    let mut pens = Vec::with_capacity(ids.len());
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            advance += scaled.kern(ids[i - 1], *id);
        }
        pens.push(advance);
        advance += scaled.h_advance(*id);
    }
    let (ascent, descent) = (scaled.ascent(), scaled.descent());

    // Center the advance box and line box on the label's point, keeping the fractions.
    let left = label.x - advance as f64 / 2.0;
    let baseline = label.y + (ascent + descent) as f64 / 2.0;
    let margin = label.halo.ceil() as i64 + 3;
    let x = left.floor() as i64 - margin;
    let y = (label.y - (ascent - descent) as f64 / 2.0).floor() as i64 - margin;
    let width = advance.ceil() as i64 + margin * 2 + 2;
    let height = (ascent - descent).ceil() as i64 + margin * 2 + 2;
    let mut coverage = vec![0.0f32; (width * height) as usize];

    for (id, pen) in ids.iter().zip(&pens) {
        let glyph = id.with_scale_and_position(
            px,
            point(
                (left + *pen as f64 - x as f64) as f32,
                (baseline - y as f64) as f32,
            ),
        );
        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
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
    Bitmap {
        x,
        y,
        width,
        height,
        coverage,
    }
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

pub fn draw_labels(
    frame: &mut [u8],
    width: u32,
    height: u32,
    labels: &[Label],
    paper: [f64; 3],
    fonts: &Fonts,
) {
    for label in labels {
        if label.opacity <= 0.0 {
            continue;
        }
        let bitmap = rasterize(label, fonts.of(label.font));
        // Like SVG's stroke-width, `halo` is the full width, centered on the outline.
        let halo = dilate(&bitmap, label.halo / 2.0);
        if !halo.is_empty() {
            blend(frame, width, height, &bitmap, &halo, paper, label.opacity);
        }
        blend(
            frame,
            width,
            height,
            &bitmap,
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
        }
    }

    /// These use the machine's fonts, so they say so and pass where there are none.
    fn fonts() -> Option<Fonts> {
        let found = Fonts::load(None).ok();
        if found.is_none() {
            eprintln!("skipped: no system fonts");
        }
        found
    }

    /// Where a label's ink is centered in the frame, in pixels.
    fn ink_center(fonts: &Fonts, label: &Label) -> (f64, f64) {
        let bitmap = rasterize(label, fonts.of(label.font));
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

    /// The bug this replaced: whole-pixel placement made moving labels jump.
    #[test]
    fn labels_move_by_fractions_of_a_pixel() {
        let Some(fonts) = fonts() else { return };
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
        let Some(fonts) = fonts() else { return };
        let font = fonts.of(FontRole::Sans);
        let scaled = font.as_scaled(em_scale(font, 100.0));
        let em = scaled.ascent() - scaled.descent();
        let expected = font.height_unscaled() / font.units_per_em().unwrap() * 100.0;
        assert!((em - expected).abs() < 0.01);
        // A capital is most of an em in any ordinary font, which it isn't at the wrong scale.
        let bitmap = rasterize(&label("H", 100.0, 100.0, 100.0), font);
        let rows = (0..bitmap.height)
            .filter(|y| {
                (0..bitmap.width).any(|x| bitmap.coverage[(y * bitmap.width + x) as usize] > 0.5)
            })
            .count();
        assert!((66..=80).contains(&rows), "cap height {rows}");
    }

    #[test]
    fn labels_center_on_their_point() {
        let Some(fonts) = fonts() else { return };
        let (x, _) = ink_center(&fonts, &label("H", 200.0, 100.0, 40.0));
        assert!((x - 200.0).abs() < 1.5, "{x}");
    }
}
