//! Compares ways of drawing text with no page to overlay it on, as video needs.
//!
//! Every renderer draws the same glyphs at the same fractional positions. Each is scored
//! against an exact reference: 16×16 point samples per pixel, by nonzero winding on finely
//! flattened outlines, which is a different code path from all of them.
//!
//! - `fontdue`: Pencil's first stopgap, which rounds glyph positions to whole pixels.
//! - `ab_glyph`: a CPU area rasterizer that honors fractional positions, now used by Pencil.
//! - `integral-cpu`, `integral-gpu`: the closed-form winding integral per pixel described
//!   by the windfoil algorithm, written here from its published formula.
//! - `vello-area`, `vello-msaa16`: Vello's compute renderer, drawing the same outlines.

use std::error::Error;
use std::fs;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

use ab_glyph::{Font as _, FontRef, GlyphId, OutlineCurve, ScaleFont as _, point};
use serde_json::json;
use vello::kurbo::{Affine, BezPath, Point};
use vello::peniko::{Color, Fill};
use vello::wgpu;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 460;
const SERIF: (&str, u32) = ("/System/Library/Fonts/Supplemental/Charter.ttc", 1);
const SANS: (&str, u32) = ("/System/Library/Fonts/Helvetica.ttc", 0);

/// `(serif?, size, x, baseline, text)`, in device pixels, off the pixel grid on purpose.
const LINES: [(bool, f32, f32, f32, &str); 7] = [
    (true, 34.0, 24.37, 48.4, "O P Q a 1"),
    (true, 30.0, 300.13, 48.4, "a + b = c"),
    (false, 34.0, 24.61, 100.3, "triangle 1.798 + wedge 1.344"),
    (false, 24.0, 24.21, 140.7, "triangle 1.798 + wedge 1.344 = 3.142"),
    (false, 17.0, 24.43, 170.2, "Unwind a string from a circle, and its end traces the involute."),
    (false, 13.0, 24.77, 196.6, "the quick brown fox jumps over the lazy dog 0123456789"),
    (true, 17.0, 24.11, 226.5, "Unwind a string from a circle, and its end traces the involute."),
];
const DISPLAY: (bool, f32, f32, f32, &str) = (true, 150.0, 24.3, 400.2, "Qag & 8");

type Quad = [[f64; 2]; 3];

/// `ab_glyph` scales by line height, but a font size means the em, as it does in CSS.
fn em_scale(font: &FontRef<'_>, size: f32) -> f32 {
    size * font.height_unscaled() / font.units_per_em().unwrap_or(1000.0)
}

struct Placed {
    serif: bool,
    size: f32,
    id: GlyphId,
    character: char,
    x: f32,
    y: f32,
}

struct Fonts<'a> {
    serif: FontRef<'a>,
    sans: FontRef<'a>,
    serif_fontdue: fontdue::Font,
    sans_fontdue: fontdue::Font,
}

impl Fonts<'_> {
    fn of(&self, serif: bool) -> &FontRef<'_> {
        if serif { &self.serif } else { &self.sans }
    }

    fn layout(&self, line: (bool, f32, f32, f32, &str), dx: f32, out: &mut Vec<Placed>) {
        let (serif, size, x, y, text) = line;
        let font = self.of(serif);
        let scaled = font.as_scaled(em_scale(font, size));
        let mut pen = x + dx;
        let mut last = None;
        for character in text.chars() {
            let id = font.glyph_id(character);
            if let Some(previous) = last {
                pen += scaled.kern(previous, id);
            }
            // SNAP tests how Chrome positions glyphs: quarter pixels across, whole down.
            let (px, py) = if std::env::var_os("SNAP").is_some() {
                ((pen * 4.0).round() / 4.0, y.round())
            } else {
                (pen, y)
            };
            out.push(Placed { serif, size, id, character, x: px, y: py });
            pen += scaled.h_advance(id);
            last = Some(id);
        }
    }

    /// A glyph's outline as quadratics in device pixels.
    fn quads(&self, glyph: &Placed) -> Vec<Quad> {
        let font = self.of(glyph.serif);
        let scaled = font.as_scaled(em_scale(font, glyph.size));
        let (sx, sy) = (scaled.h_scale_factor() as f64, scaled.v_scale_factor() as f64);
        let Some(outline) = font.outline(glyph.id) else {
            return Vec::new();
        };
        let at = |p: ab_glyph::Point| [glyph.x as f64 + p.x as f64 * sx, glyph.y as f64 - p.y as f64 * sy];
        let mut out = Vec::new();
        for curve in &outline.curves {
            match *curve {
                OutlineCurve::Line(a, b) => {
                    let (a, b) = (at(a), at(b));
                    out.push([a, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], b]);
                }
                OutlineCurve::Quad(a, b, c) => out.push([at(a), at(b), at(c)]),
                OutlineCurve::Cubic(a, b, c, d) => {
                    // Approximated by four quadratics, which is far below a pixel here.
                    let (a, b, c, d) = (at(a), at(b), at(c), at(d));
                    let point = |t: f64| -> [f64; 2] {
                        let u = 1.0 - t;
                        std::array::from_fn(|i| {
                            u * u * u * a[i] + 3.0 * u * u * t * b[i] + 3.0 * u * t * t * c[i] + t * t * t * d[i]
                        })
                    };
                    let tangent = |t: f64| -> [f64; 2] {
                        let u = 1.0 - t;
                        std::array::from_fn(|i| {
                            3.0 * u * u * (b[i] - a[i]) + 6.0 * u * t * (c[i] - b[i]) + 3.0 * t * t * (d[i] - c[i])
                        })
                    };
                    for k in 0..4 {
                        let (t0, t1) = (k as f64 / 4.0, (k + 1) as f64 / 4.0);
                        let (p0, p1) = (point(t0), point(t1));
                        let tm = tangent(t0);
                        let control = [p0[0] + tm[0] * (t1 - t0) / 2.0, p0[1] + tm[1] * (t1 - t0) / 2.0];
                        out.push([p0, control, p1]);
                    }
                }
            }
        }
        out
    }
}

fn flatten(quad: &Quad, steps: usize) -> Vec<[f64; 2]> {
    (0..=steps)
        .map(|k| {
            let t = k as f64 / steps as f64;
            let u = 1.0 - t;
            std::array::from_fn(|i| u * u * quad[0][i] + 2.0 * u * t * quad[1][i] + t * t * quad[2][i])
        })
        .collect()
}

/// Glyphs combine by maximum in every renderer, so overlaps score the same way.
fn plot(buffer: &mut [f32], x: i64, y: i64, value: f32) {
    if x < 0 || y < 0 || x >= WIDTH as i64 || y >= HEIGHT as i64 {
        return;
    }
    let slot = &mut buffer[(y * WIDTH as i64 + x) as usize];
    *slot = slot.max(value);
}

fn bounds(quads: &[Quad]) -> Option<(i64, i64, i64, i64)> {
    let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for q in quads {
        for p in q {
            b = (b.0.min(p[0]), b.1.min(p[1]), b.2.max(p[0]), b.3.max(p[1]));
        }
    }
    if b.0 > b.2 {
        return None;
    }
    Some((b.0.floor() as i64 - 1, b.1.floor() as i64 - 1, b.2.ceil() as i64 + 1, b.3.ceil() as i64 + 1))
}

/// The exact reference: point samples by nonzero winding on flattened outlines.
fn reference(fonts: &Fonts, glyphs: &[Placed]) -> Vec<f32> {
    const SAMPLES: usize = 16;
    let mut out = vec![0.0; (WIDTH * HEIGHT) as usize];
    for glyph in glyphs {
        let quads = fonts.quads(glyph);
        let Some((x0, y0, x1, y1)) = bounds(&quads) else { continue };
        let lines: Vec<[[f64; 2]; 2]> = quads
            .iter()
            .flat_map(|q| {
                let points = flatten(q, 24);
                points.windows(2).map(|w| [w[0], w[1]]).collect::<Vec<_>>()
            })
            .collect();
        for y in y0..y1 {
            for x in x0..x1 {
                let mut hits = 0;
                for j in 0..SAMPLES {
                    for i in 0..SAMPLES {
                        let px = x as f64 + (i as f64 + 0.5) / SAMPLES as f64;
                        let py = y as f64 + (j as f64 + 0.5) / SAMPLES as f64;
                        let mut winding = 0;
                        for [a, b] in &lines {
                            let cross = (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0]);
                            if a[1] <= py {
                                if b[1] > py && cross > 0.0 {
                                    winding += 1;
                                }
                            } else if b[1] <= py && cross < 0.0 {
                                winding -= 1;
                            }
                        }
                        if winding != 0 {
                            hits += 1;
                        }
                    }
                }
                plot(&mut out, x, y, hits as f32 / (SAMPLES * SAMPLES) as f32);
            }
        }
    }
    out
}

/// Pencil's current stopgap: whole-pixel positions, as `pencil-cli`'s `text.rs` does.
fn draw_fontdue(fonts: &Fonts, glyphs: &[Placed]) -> Vec<f32> {
    let mut out = vec![0.0; (WIDTH * HEIGHT) as usize];
    for glyph in glyphs {
        let font = if glyph.serif { &fonts.serif_fontdue } else { &fonts.sans_fontdue };
        let (metrics, bitmap) = font.rasterize(glyph.character, glyph.size);
        let left = (glyph.x + metrics.xmin as f32).round() as i64;
        let top = (glyph.y - metrics.height as f32 - metrics.ymin as f32).round() as i64;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let value = bitmap[row * metrics.width + col] as f32 / 255.0;
                plot(&mut out, left + col as i64, top + row as i64, value);
            }
        }
    }
    out
}

fn draw_ab_glyph(fonts: &Fonts, glyphs: &[Placed]) -> Vec<f32> {
    let mut out = vec![0.0; (WIDTH * HEIGHT) as usize];
    for glyph in glyphs {
        let font = fonts.of(glyph.serif);
        let g = glyph.id.with_scale_and_position(em_scale(font, glyph.size), point(glyph.x, glyph.y));
        if let Some(outlined) = font.outline_glyph(g) {
            let b = outlined.px_bounds();
            outlined.draw(|x, y, c| plot(&mut out, b.min.x as i64 + x as i64, b.min.y as i64 + y as i64, c));
        }
    }
    out
}

/// Splits a quadratic where it turns, so each piece is monotone in both axes.
fn monotone(quad: &Quad) -> Vec<Quad> {
    let mut cuts = vec![0.0, 1.0];
    for axis in 0..2 {
        let (a, b, c) = (quad[0][axis], quad[1][axis], quad[2][axis]);
        let denominator = a - 2.0 * b + c;
        if denominator.abs() > 1e-12 {
            let t = (a - b) / denominator;
            if t > 1e-9 && t < 1.0 - 1e-9 {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    let at = |t: f64| -> [f64; 2] {
        let u = 1.0 - t;
        std::array::from_fn(|i| u * u * quad[0][i] + 2.0 * u * t * quad[1][i] + t * t * quad[2][i])
    };
    let derivative = |t: f64| -> [f64; 2] {
        let u = 1.0 - t;
        std::array::from_fn(|i| 2.0 * u * (quad[1][i] - quad[0][i]) + 2.0 * t * (quad[2][i] - quad[1][i]))
    };
    cuts.windows(2)
        .map(|w| {
            let (p0, p2, d) = (at(w[0]), at(w[1]), derivative(w[0]));
            let mut control = [p0[0] + d[0] * (w[1] - w[0]) / 2.0, p0[1] + d[1] * (w[1] - w[0]) / 2.0];
            // Rounding mustn't leave a control point outside a monotone piece.
            for i in 0..2 {
                control[i] = control[i].clamp(p0[i].min(p2[i]), p0[i].max(p2[i]));
            }
            [p0, control, p2]
        })
        .collect()
}

/// Where a monotone quadratic reaches `target`, saturating at its ends.
fn solve(q1: f64, q2: f64, q3: f64, target: f64) -> f64 {
    if q1 <= q3 {
        if target <= q1 {
            return 0.0;
        }
        if target >= q3 {
            return 1.0;
        }
    } else {
        if target >= q1 {
            return 0.0;
        }
        if target <= q3 {
            return 1.0;
        }
    }
    let (a, b, c) = (q1 - 2.0 * q2 + q3, 2.0 * (q2 - q1), q1 - target);
    if a.abs() < 1e-9 * (b.abs() + 1e-9) {
        return (-c / b).clamp(0.0, 1.0);
    }
    let disc = (b * b - 4.0 * a * c).max(0.0).sqrt();
    let q = -0.5 * (b + if b >= 0.0 { disc } else { -disc });
    let r1 = q / a;
    let r2 = if q != 0.0 { c / q } else { r1 };
    let r = if (0.0..=1.0).contains(&r1) { r1 } else { r2 };
    r.clamp(0.0, 1.0)
}

/// The area one monotone piece contributes to the pixel centered at the origin: Green's
/// theorem with the x-clamp, following the windfoil algorithm's published decomposition.
/// The integrand is cubic in t, so Simpson's rule is exact on the crossing zone.
fn integrate(q: &Quad) -> f64 {
    let (y1, y2, y3) = (q[0][1], q[1][1], q[2][1]);
    let (x1, x2, x3) = (q[0][0], q[1][0], q[2][0]);
    let (a, b) = (solve(y1, y2, y3, -0.5), solve(y1, y2, y3, 0.5));
    let (lo, hi) = (a.min(b), a.max(b));
    if hi <= lo {
        return 0.0;
    }
    let (left, right) = (solve(x1, x2, x3, -0.5), solve(x1, x2, x3, 0.5));
    let ta = left.min(right).clamp(lo, hi);
    let tb = left.max(right).clamp(lo, hi);
    let y = |t: f64| {
        let u = 1.0 - t;
        u * u * y1 + 2.0 * u * t * y2 + t * t * y3
    };
    let x = |t: f64| {
        let u = 1.0 - t;
        u * u * x1 + 2.0 * u * t * x2 + t * t * x3
    };
    let dy = |t: f64| 2.0 * (1.0 - t) * (y2 - y1) + 2.0 * t * (y3 - y2);
    let f = |t: f64| (x(t).clamp(-0.5, 0.5) + 0.5) * dy(t);
    let inside = (tb - ta) / 6.0 * (f(ta) + 4.0 * f((ta + tb) / 2.0) + f(tb));
    // Fully right of the pixel, a piece sweeps the pixel's whole width.
    let right_zone = if x1 <= x3 { y(hi) - y(tb) } else { y(ta) - y(lo) };
    inside + right_zone
}

fn monotone_pieces(fonts: &Fonts, glyph: &Placed) -> Vec<Quad> {
    fonts.quads(glyph).iter().flat_map(monotone).collect()
}

fn draw_integral_cpu(fonts: &Fonts, glyphs: &[Placed]) -> Vec<f32> {
    let mut out = vec![0.0; (WIDTH * HEIGHT) as usize];
    for glyph in glyphs {
        let pieces = monotone_pieces(fonts, glyph);
        let Some((x0, y0, x1, y1)) = bounds(&pieces) else { continue };
        for y in y0..y1 {
            for x in x0..x1 {
                let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
                let mut area = 0.0;
                for q in &pieces {
                    let local = [
                        [q[0][0] - cx, q[0][1] - cy],
                        [q[1][0] - cx, q[1][1] - cy],
                        [q[2][0] - cx, q[2][1] - cy],
                    ];
                    area += integrate(&local);
                }
                plot(&mut out, x, y, area.abs().min(1.0) as f32);
            }
        }
    }
    out
}

const INTEGRAL_SHADER: &str = r#"
struct Glyph { x0: f32, y0: f32, x1: f32, y1: f32, start: u32, count: u32, pad0: u32, pad1: u32 }
@group(0) @binding(0) var<uniform> size: vec4<f32>;
@group(0) @binding(1) var<storage, read> glyphs: array<Glyph>;
@group(0) @binding(2) var<storage, read> points: array<vec2<f32>>;

struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) range: vec2<u32>,
}

@vertex
fn vs(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> Varying {
    let g = glyphs[i];
    let x = select(g.x0, g.x1, (v & 1u) == 1u);
    let y = select(g.y0, g.y1, (v & 2u) == 2u);
    var out: Varying;
    out.position = vec4<f32>(x / size.x * 2.0 - 1.0, 1.0 - y / size.y * 2.0, 0.0, 1.0);
    out.range = vec2<u32>(g.start, g.count);
    return out;
}

fn solve(q1: f32, q2: f32, q3: f32, level: f32) -> f32 {
    if q1 <= q3 {
        if level <= q1 { return 0.0; }
        if level >= q3 { return 1.0; }
    } else {
        if level >= q1 { return 0.0; }
        if level <= q3 { return 1.0; }
    }
    let a = q1 - 2.0 * q2 + q3;
    let b = 2.0 * (q2 - q1);
    let c = q1 - level;
    if abs(a) < 1e-6 * (abs(b) + 1e-6) { return clamp(-c / b, 0.0, 1.0); }
    let disc = sqrt(max(b * b - 4.0 * a * c, 0.0));
    let q = -0.5 * (b + select(-disc, disc, b >= 0.0));
    let r1 = q / a;
    let r2 = select(r1, c / q, q != 0.0);
    return clamp(select(r2, r1, r1 >= 0.0 && r1 <= 1.0), 0.0, 1.0);
}

fn along(a: f32, b: f32, c: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    return u * u * a + 2.0 * u * t * b + t * t * c;
}

fn inside_term(q1: vec2<f32>, q2: vec2<f32>, q3: vec2<f32>, t: f32) -> f32 {
    let x = clamp(along(q1.x, q2.x, q3.x, t), -0.5, 0.5) + 0.5;
    let dy = 2.0 * (1.0 - t) * (q2.y - q1.y) + 2.0 * t * (q3.y - q2.y);
    return x * dy;
}

fn integrate(q1: vec2<f32>, q2: vec2<f32>, q3: vec2<f32>) -> f32 {
    let a = solve(q1.y, q2.y, q3.y, -0.5);
    let b = solve(q1.y, q2.y, q3.y, 0.5);
    let lo = min(a, b);
    let hi = max(a, b);
    if hi <= lo { return 0.0; }
    let l = solve(q1.x, q2.x, q3.x, -0.5);
    let r = solve(q1.x, q2.x, q3.x, 0.5);
    let ta = clamp(min(l, r), lo, hi);
    let tb = clamp(max(l, r), lo, hi);
    let simpson = (tb - ta) / 6.0 * (inside_term(q1, q2, q3, ta) + 4.0 * inside_term(q1, q2, q3, (ta + tb) / 2.0) + inside_term(q1, q2, q3, tb));
    var right: f32;
    if q1.x <= q3.x {
        right = along(q1.y, q2.y, q3.y, hi) - along(q1.y, q2.y, q3.y, tb);
    } else {
        right = along(q1.y, q2.y, q3.y, ta) - along(q1.y, q2.y, q3.y, lo);
    }
    return simpson + right;
}

@fragment
fn fs(in: Varying) -> @location(0) vec4<f32> {
    let c = in.position.xy;
    var area = 0.0;
    for (var i = 0u; i < in.range.y; i++) {
        let k = in.range.x + i * 3u;
        area += integrate(points[k] - c, points[k + 1u] - c, points[k + 2u] - c);
    }
    return vec4<f32>(min(abs(area), 1.0), 0.0, 0.0, 1.0);
}
"#;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    name: String,
}

fn make_gpu() -> Result<Gpu, Box<dyn Error>> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    let name = adapter.get_info().name;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))?;
    Ok(Gpu { device, queue, name })
}

fn read_back(gpu: &Gpu, texture: &wgpu::Texture, channels: u32) -> Result<Vec<u8>, Box<dyn Error>> {
    let row = WIDTH * channels;
    let aligned = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (aligned * HEIGHT) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(aligned),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    gpu.queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
    let mapped = slice.get_mapped_range()?;
    let mut out = Vec::with_capacity((row * HEIGHT) as usize);
    for line in mapped.chunks(aligned as usize) {
        out.extend_from_slice(&line[..row as usize]);
    }
    Ok(out)
}

struct IntegralGpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    target: wgpu::Texture,
}

impl IntegralGpu {
    fn new(gpu: &Gpu) -> Self {
        let module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("integral"),
            source: wgpu::ShaderSource::Wgsl(INTEGRAL_SHADER.into()),
        });
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry { binding, visibility, ty, count: None };
        let storage = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(
                    0,
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(1, wgpu::ShaderStages::VERTEX, storage),
                entry(2, wgpu::ShaderStages::FRAGMENT, storage),
            ],
        });
        let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let max = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        };
        let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("integral"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: Some(wgpu::BlendState { color: max, alpha: max }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("integral target"),
            size: wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Self { pipeline, layout, target }
    }

    fn draw(&self, gpu: &Gpu, fonts: &Fonts, glyphs: &[Placed]) -> Result<Vec<f32>, Box<dyn Error>> {
        use wgpu::util::DeviceExt;
        let mut records: Vec<u8> = Vec::new();
        let mut points: Vec<u8> = Vec::new();
        let mut count = 0u32;
        let mut start = 0u32;
        for glyph in glyphs {
            let pieces = monotone_pieces(fonts, glyph);
            let Some((x0, y0, x1, y1)) = bounds(&pieces) else { continue };
            for q in &pieces {
                for p in q {
                    points.extend_from_slice(&(p[0] as f32).to_le_bytes());
                    points.extend_from_slice(&(p[1] as f32).to_le_bytes());
                }
            }
            for v in [x0, y0, x1, y1] {
                records.extend_from_slice(&(v as f32).to_le_bytes());
            }
            for v in [start, pieces.len() as u32, 0, 0] {
                records.extend_from_slice(&v.to_le_bytes());
            }
            start += pieces.len() as u32 * 3;
            count += 1;
        }
        let device = &gpu.device;
        let init = |contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents, usage })
        };
        let mut size = Vec::new();
        for v in [WIDTH as f32, HEIGHT as f32, 0.0, 0.0] {
            size.extend_from_slice(&v.to_le_bytes());
        }
        let (u, g, p) = (
            init(&size, wgpu::BufferUsages::UNIFORM),
            init(&records, wgpu::BufferUsages::STORAGE),
            init(&points, wgpu::BufferUsages::STORAGE),
        );
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: u.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: g.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: p.as_entire_binding() },
            ],
        });
        let view = self.target.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..4, 0..count);
        }
        gpu.queue.submit([encoder.finish()]);
        let bytes = read_back(gpu, &self.target, 1)?;
        Ok(bytes.iter().map(|b| *b as f32 / 255.0).collect())
    }
}

struct Vello {
    renderer: vello::Renderer,
    target: wgpu::Texture,
}

impl Vello {
    fn new(gpu: &Gpu) -> Result<Self, Box<dyn Error>> {
        let renderer = vello::Renderer::new(
            &gpu.device,
            vello::RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::all(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )?;
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vello target"),
            size: wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Ok(Self { renderer, target })
    }

    fn draw(
        &mut self,
        gpu: &Gpu,
        fonts: &Fonts,
        glyphs: &[Placed],
        aa: vello::AaConfig,
        flat: bool,
    ) -> Result<Vec<f32>, Box<dyn Error>> {
        let mut scene = vello::Scene::new();
        for glyph in glyphs {
            let quads = fonts.quads(glyph);
            let mut path = BezPath::new();
            let mut last: Option<[f64; 2]> = None;
            for q in &quads {
                let pt = |p: [f64; 2]| Point::new(p[0], p[1]);
                if last.is_none_or(|l| (l[0] - q[0][0]).abs() + (l[1] - q[0][1]).abs() > 1e-9) {
                    if last.is_some() {
                        path.close_path();
                    }
                    path.move_to(pt(q[0]));
                }
                if flat {
                    // Finely flattened here, so Vello's own flattening has nothing to do.
                    for p in flatten(q, 24).into_iter().skip(1) {
                        path.line_to(pt(p));
                    }
                } else {
                    path.quad_to(pt(q[1]), pt(q[2]));
                }
                last = Some(q[2]);
            }
            if last.is_some() {
                path.close_path();
                scene.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &path);
            }
        }
        let view = self.target.create_view(&Default::default());
        self.renderer.render_to_texture(
            &gpu.device,
            &gpu.queue,
            &scene,
            &view,
            &vello::RenderParams {
                base_color: Color::WHITE,
                width: WIDTH,
                height: HEIGHT,
                antialiasing_method: aa,
            },
        )?;
        let bytes = read_back(gpu, &self.target, 4)?;
        Ok(bytes.chunks(4).map(|p| 1.0 - p[0] as f32 / 255.0).collect())
    }
}

fn write_png(path: &Path, coverage: &[f32]) -> Result<(), Box<dyn Error>> {
    let file = BufWriter::new(fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = coverage.iter().map(|c| ((1.0 - c.clamp(0.0, 1.0)) * 255.0).round() as u8).collect();
    encoder.write_header()?.write_image_data(&bytes)?;
    Ok(())
}

/// Error against the reference, over pixels that either side has partly covered. Ink is the
/// total coverage relative to the reference, so a value above 1 means heavier text.
fn score(image: &[f32], truth: &[f32]) -> serde_json::Value {
    let (mut squares, mut count, mut worst, mut ink, mut ink_truth) = (0.0f64, 0u64, 0.0f64, 0.0f64, 0.0f64);
    for (a, b) in image.iter().zip(truth) {
        ink += *a as f64;
        ink_truth += *b as f64;
        let partly = |v: f32| v > 0.0 && v < 1.0;
        if partly(*a) || partly(*b) {
            let e = (*a as f64 - *b as f64).abs();
            squares += e * e;
            count += 1;
            worst = worst.max(e);
        }
    }
    json!({
        "rms": (squares / count.max(1) as f64).sqrt(),
        "max": worst,
        "ink": ink / ink_truth.max(1e-9),
    })
}

fn centroid(image: &[f32]) -> f64 {
    let (mut total, mut moment) = (0.0f64, 0.0f64);
    for y in 0..HEIGHT as usize {
        for x in 0..WIDTH as usize {
            let v = image[y * WIDTH as usize + x] as f64;
            total += v;
            moment += v * (x as f64 + 0.5);
        }
    }
    moment / total.max(1e-9)
}

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = Path::new("out");
    fs::create_dir_all(out_dir)?;
    let serif_bytes = fs::read(SERIF.0)?;
    let sans_bytes = fs::read(SANS.0)?;
    let settings = |index: u32| fontdue::FontSettings { collection_index: index, ..Default::default() };
    let fonts = Fonts {
        serif: FontRef::try_from_slice_and_index(&serif_bytes, SERIF.1)?,
        sans: FontRef::try_from_slice_and_index(&sans_bytes, SANS.1)?,
        serif_fontdue: fontdue::Font::from_bytes(serif_bytes.as_slice(), settings(SERIF.1))?,
        sans_fontdue: fontdue::Font::from_bytes(sans_bytes.as_slice(), settings(SANS.1))?,
    };

    let mut glyphs = Vec::new();
    for line in LINES {
        fonts.layout(line, 0.0, &mut glyphs);
    }
    fonts.layout(DISPLAY, 0.0, &mut glyphs);
    let piece_count: usize = glyphs.iter().map(|g| monotone_pieces(&fonts, g).len()).sum();
    println!("{} glyphs, {piece_count} monotone pieces", glyphs.len());

    let gpu = make_gpu()?;
    println!("gpu: {}", gpu.name);
    let start = Instant::now();
    let integral = IntegralGpu::new(&gpu);
    let integral_setup = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let mut vello = Vello::new(&gpu)?;
    let vello_setup = start.elapsed().as_secs_f64() * 1000.0;

    let truth = reference(&fonts, &glyphs);
    write_png(&out_dir.join("reference.png"), &truth)?;

    type Draw<'a> = Box<dyn FnMut(&[Placed]) -> Result<Vec<f32>, Box<dyn Error>> + 'a>;
    let vello = std::cell::RefCell::new(&mut vello);
    let mut renderers: Vec<(&str, Draw)> = vec![
        ("fontdue", Box::new(|g: &[Placed]| Ok(draw_fontdue(&fonts, g)))),
        ("ab_glyph", Box::new(|g: &[Placed]| Ok(draw_ab_glyph(&fonts, g)))),
        ("integral-cpu", Box::new(|g: &[Placed]| Ok(draw_integral_cpu(&fonts, g)))),
        ("integral-gpu", Box::new(|g: &[Placed]| integral.draw(&gpu, &fonts, g))),
        (
            "vello-area",
            Box::new(|g: &[Placed]| vello.borrow_mut().draw(&gpu, &fonts, g, vello::AaConfig::Area, false)),
        ),
        (
            "vello-area-flat",
            Box::new(|g: &[Placed]| vello.borrow_mut().draw(&gpu, &fonts, g, vello::AaConfig::Area, true)),
        ),
        (
            "vello-msaa16",
            Box::new(|g: &[Placed]| vello.borrow_mut().draw(&gpu, &fonts, g, vello::AaConfig::Msaa16, false)),
        ),
    ];

    // Motion: one glyph sliding by eighths of a pixel. A renderer that honors fractional
    // positions moves its centroid by exactly that much. One glyph, because rounding each
    // glyph of a line separately averages out over the line and would hide the jitter.
    let mut motion_frames = Vec::new();
    for k in 0..=16 {
        let mut frame = Vec::new();
        fonts.layout((false, 24.0, 40.21, 140.7, "e"), k as f32 / 8.0, &mut frame);
        motion_frames.push((k as f64 / 8.0, frame));
    }

    let mut results = serde_json::Map::new();
    for (name, draw) in &mut renderers {
        let first = Instant::now();
        let image = draw(&glyphs)?;
        let first = first.elapsed().as_secs_f64() * 1000.0;
        write_png(&out_dir.join(format!("{name}.png")), &image)?;
        let mut runs = Vec::new();
        for _ in 0..12 {
            let t = Instant::now();
            let _ = draw(&glyphs)?;
            runs.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        runs.sort_by(f64::total_cmp);

        let mut base = None;
        let (mut wander, mut wander_max) = (0.0f64, 0.0f64);
        for (dx, frame) in &motion_frames {
            let c = centroid(&draw(frame)?);
            let origin = *base.get_or_insert(c);
            let e = (c - origin - dx).abs();
            wander += e * e;
            wander_max = wander_max.max(e);
        }
        let mut entry = score(&image, &truth);
        let map = entry.as_object_mut().unwrap();
        map.insert("frame_ms".into(), json!(runs[runs.len() / 2]));
        map.insert("first_ms".into(), json!(first));
        map.insert(
            "motion_rms_px".into(),
            json!((wander / motion_frames.len() as f64).sqrt()),
        );
        map.insert("motion_max_px".into(), json!(wander_max));
        results.insert(name.to_string(), entry);
    }
    let summary = json!({
        "gpu": gpu.name,
        "glyphs": glyphs.len(),
        "pieces": piece_count,
        "setup_ms": { "integral-gpu": integral_setup, "vello": vello_setup },
        "lines": LINES.iter().map(|l| json!({ "serif": l.0, "size": l.1, "x": l.2, "y": l.3, "text": l.4 })).collect::<Vec<_>>(),
        "display": { "serif": DISPLAY.0, "size": DISPLAY.1, "x": DISPLAY.2, "y": DISPLAY.3, "text": DISPLAY.4 },
        "results": results,
    });
    fs::write(out_dir.join("results.json"), serde_json::to_string_pretty(&summary)?)?;
    let table = summary["results"].as_object().unwrap();
    println!("{:<14} {:>7} {:>6} {:>6} {:>9} {:>8} {:>9}", "renderer", "rms", "max", "ink", "motion", "frame", "first");
    for (name, r) in table {
        println!(
            "{:<14} {:>7.4} {:>6.3} {:>6.3} {:>7.3}px {:>6.2}ms {:>7.1}ms",
            name,
            r["rms"].as_f64().unwrap(),
            r["max"].as_f64().unwrap(),
            r["ink"].as_f64().unwrap(),
            r["motion_rms_px"].as_f64().unwrap(),
            r["frame_ms"].as_f64().unwrap(),
            r["first_ms"].as_f64().unwrap(),
        );
    }
    println!("setup: integral-gpu {integral_setup:.1} ms, vello {vello_setup:.1} ms");
    Ok(())
}
