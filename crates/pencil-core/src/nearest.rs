//! True distances to an implicit curve, measured to the curve as traced, since `f / |∇f|` is only
//! right next to the curve and goes infinite wherever the gradient vanishes.
//!
//! The oracle looks at every traced segment. The GPU gets, per tile of pixels, only the segments
//! that could be nearest to some pixel in that tile, so both find the same nearest segment.

use crate::ir::{Form, Mark};
use crate::style::MarkStyle;

/// A tile's side, in device pixels.
pub const TILE_PX: f64 = 32.0;
/// The data texture's width, in texels. WebGL2 only promises 2048 texels a side.
pub const TEXTURE_WIDTH: usize = 1024;
/// Its most rows.
pub const TEXTURE_ROWS: usize = 2048;

/// Whether a mark's distance is measured to its traced curve: an implicit curve that isn't
/// already an exact distance, where the distance is read at all, by a fill or a field stroke.
pub fn measures(mark: &Mark, style: &MarkStyle) -> bool {
    matches!(mark.form, Form::Field { exact: false, .. })
        && (style.negative.is_some()
            || style.positive.is_some()
            || style.inside.is_some()
            || style.outside.is_some()
            || (style.stroke.is_some() && !style.stroke_is_line(&mark.form)))
}

/// The squared distance from a point to a segment `x0 y0 x1 y1`, and the way from the segment's
/// nearest point to it.
fn to_segment(s: &[f32; 4], px: f64, py: f64) -> (f64, (f64, f64)) {
    let [ax, ay, bx, by] = s.map(f64::from);
    let (ex, ey) = (bx - ax, by - ay);
    let (wx, wy) = (px - ax, py - ay);
    let len2 = ex * ex + ey * ey;
    let h = if len2 > 0.0 {
        ((wx * ex + wy * ey) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (vx, vy) = (wx - ex * h, wy - ey * h);
    (vx * vx + vy * vy, (vx, vy))
}

/// The distance from a device pixel to the nearest segment, and the way from it to the pixel.
/// None if there are no segments.
pub fn nearest(segments: &[[f32; 4]], px: f64, py: f64) -> Option<(f64, (f64, f64))> {
    let mut best: Option<(f64, (f64, f64))> = None;
    for s in segments {
        let (d2, toward) = to_segment(s, px, py);
        if best.is_none_or(|(b, _)| d2 < b) {
            best = Some((d2, toward));
        }
    }
    best.map(|(d2, toward)| (d2.sqrt(), toward))
}

/// The texture the shader reads. First a header texel per measured mark, `table tiles_x tiles_y
/// segments`, then each mark's table of `start count _ _` per tile, row by row, then the
/// segments each tile lists.
///
/// A tile lists every segment within `d + 2h` of its center, where `d` is the nearest one's
/// distance from there and `h` is half the tile's diagonal. Any other is farther than `d + h`
/// from every pixel of the tile, and the nearest to the center is within that of all of them.
pub fn texture(marks: &[Vec<[f32; 4]>], width: u32, height: u32) -> Vec<f32> {
    let tiles_x = (width as f64 / TILE_PX).ceil().max(1.0) as usize;
    let tiles_y = (height as f64 / TILE_PX).ceil().max(1.0) as usize;
    let half = TILE_PX / 2.0 * std::f64::consts::SQRT_2;
    let mut texels: Vec<[f32; 4]> = vec![[0.0; 4]; marks.len()];
    for (k, segments) in marks.iter().enumerate() {
        if segments.is_empty() {
            continue;
        }
        let table = texels.len();
        texels[k] = [
            table as f32,
            tiles_x as f32,
            tiles_y as f32,
            segments.len() as f32,
        ];
        texels.resize(table + tiles_x * tiles_y, [0.0; 4]);
        let mut distances = vec![0.0; segments.len()];
        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                let cx = (tx as f64 + 0.5) * TILE_PX;
                let cy = (ty as f64 + 0.5) * TILE_PX;
                let mut d = f64::INFINITY;
                for (s, out) in segments.iter().zip(&mut distances) {
                    *out = to_segment(s, cx, cy).0.sqrt();
                    d = d.min(*out);
                }
                // A little slack, since the GPU measures in f32.
                let reach = d + 2.0 * half + 1e-3;
                let start = texels.len();
                for (s, &distance) in segments.iter().zip(&distances) {
                    if distance <= reach {
                        texels.push(*s);
                    }
                }
                texels[table + ty * tiles_x + tx] =
                    [start as f32, (texels.len() - start) as f32, 0.0, 0.0];
            }
        }
    }
    let rows = texels.len().div_ceil(TEXTURE_WIDTH).max(1);
    let mut data = texels.into_iter().flatten().collect::<Vec<_>>();
    data.resize(rows * TEXTURE_WIDTH * 4, 0.0);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<[f32; 4]> {
        vec![
            [10.0, 10.0, 90.0, 10.0],
            [90.0, 10.0, 90.0, 90.0],
            [90.0, 90.0, 10.0, 90.0],
            [10.0, 90.0, 10.0, 10.0],
        ]
    }

    #[test]
    fn the_nearest_segment_gives_the_distance_and_the_way() {
        let (d, (vx, vy)) = nearest(&square(), 30.0, 40.0).unwrap();
        assert_eq!(d, 20.0);
        assert_eq!((vx, vy), (20.0, 0.0));
        assert!(nearest(&[], 0.0, 0.0).is_none());
    }

    /// Every pixel's nearest segment, among the ones its tile lists, is as near as any.
    #[test]
    fn a_tile_lists_every_segment_that_could_be_nearest() {
        // A spiral of short segments, so tiles see very different neighbourhoods.
        let points: Vec<[f32; 2]> = (0..200)
            .map(|i| {
                let a = i as f32 * 0.09;
                let r = 8.0 + 0.45 * i as f32;
                [100.0 + r * a.cos(), 70.0 + r * a.sin()]
            })
            .collect();
        let segments: Vec<[f32; 4]> = points
            .windows(2)
            .map(|w| [w[0][0], w[0][1], w[1][0], w[1][1]])
            .collect();
        let (width, height) = (210, 150);
        let data = texture(&[Vec::new(), segments.clone()], width, height);
        let texel = |i: usize| [0, 1, 2, 3].map(|k| data[i * 4 + k]);
        assert_eq!(texel(0)[3], 0.0, "a mark with no curve says so");
        let [table, tiles_x, _, count] = texel(1);
        assert_eq!(count as usize, segments.len());
        let mut listed = 0;
        for py in 0..height {
            for px in 0..width {
                let (x, y) = (px as f64 + 0.5, py as f64 + 0.5);
                let tile = (py as f64 / TILE_PX) as usize * tiles_x as usize
                    + (px as f64 / TILE_PX) as usize;
                let [start, n, ..] = texel(table as usize + tile);
                let tiled: Vec<[f32; 4]> = (0..n as usize)
                    .map(|i| texel(start as usize + i))
                    .collect();
                listed += tiled.len();
                let all = nearest(&segments, x, y).unwrap().0;
                let some = nearest(&tiled, x, y).unwrap().0;
                assert!((all - some).abs() < 1e-9, "({px}, {py}): {all} vs {some}");
            }
        }
        // And the lists are short, or there'd be no point.
        let average = listed as f64 / (width * height) as f64;
        assert!(average < 60.0, "{average} segments a pixel");
    }
}
