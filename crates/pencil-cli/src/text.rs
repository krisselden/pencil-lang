//! Burns labels into a frame, for output with no page to overlay text on. This is a
//! stopgap for video: a CPU rasterizer, without kerning or shaping. In-canvas glyphs are
//! the open question in `docs/goals.md`.

use std::error::Error;
use std::path::Path;

use fontdue::{Font, FontSettings};
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
    sans: Font,
    math: Font,
}

fn first_font(candidates: &[(&str, u32)]) -> Result<Font, Box<dyn Error>> {
    for (path, collection_index) in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let settings = FontSettings {
            collection_index: *collection_index,
            ..FontSettings::default()
        };
        if let Ok(font) = Font::from_bytes(bytes, settings) {
            return Ok(font);
        }
    }
    Err("no system font found, so pass --font".into())
}

impl Fonts {
    /// `--font` replaces both roles, for machines without these system fonts.
    pub fn load(path: Option<&Path>) -> Result<Self, Box<dyn Error>> {
        if let Some(p) = path {
            let read = || -> Result<Font, Box<dyn Error>> {
                let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
                Ok(Font::from_bytes(bytes, FontSettings::default())?)
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

    fn of(&self, role: FontRole) -> &Font {
        match role {
            FontRole::Sans => &self.sans,
            FontRole::Math => &self.math,
        }
    }
}

/// A label's text coverage, on a canvas with room for its halo.
struct Bitmap {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    coverage: Vec<f32>,
}

fn rasterize(label: &Label, font: &Font) -> Bitmap {
    let glyphs: Vec<_> = label
        .text
        .chars()
        .map(|c| font.rasterize(c, label.size as f32))
        .collect();
    let advance: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
    let line = font.horizontal_line_metrics(label.size as f32);
    let (ascent, descent) = line.map_or((label.size as f32, 0.0), |l| (l.ascent, l.descent));
    let margin = label.halo.ceil() as i64 + 2;
    let width = advance.ceil() as i64 + margin * 2;
    let height = (ascent - descent).ceil() as i64 + margin * 2;
    let mut coverage = vec![0.0f32; (width * height) as usize];

    let mut pen = 0.0f32;
    for (metrics, bitmap) in &glyphs {
        let left = margin + (pen + metrics.xmin as f32).round() as i64;
        let top = margin + (ascent - metrics.height as f32 - metrics.ymin as f32).round() as i64;
        for row in 0..metrics.height as i64 {
            for col in 0..metrics.width as i64 {
                let (x, y) = (left + col, top + row);
                if x < 0 || y < 0 || x >= width || y >= height {
                    continue;
                }
                let value = bitmap[(row as usize) * metrics.width + col as usize] as f32 / 255.0;
                let slot = &mut coverage[(y * width + x) as usize];
                *slot = slot.max(value);
            }
        }
        pen += metrics.advance_width;
    }
    // Center the advance box and line box on the label's point.
    Bitmap {
        x: (label.x - advance as f64 / 2.0).round() as i64 - margin,
        y: (label.y - (ascent - descent) as f64 / 2.0).round() as i64 - margin,
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
