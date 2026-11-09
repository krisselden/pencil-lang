//! Emits a WGSL fragment shader from the IR and resolved styles. It implements the
//! same shading as `eval::Frame::shade`, and `pencil check` compares the two.
//!
//! Bindings are evaluated on the CPU each frame and arrive as uniforms, so both paths
//! see the same values and a shader only computes what varies per pixel.

use crate::Figure;
use crate::eval::TANH_LIMIT;
use crate::eval::{MIN_GRADIENT, SNAP_POWER};
use crate::frame::{
    DEFAULT_DOT_RADIUS, DEFAULT_STROKE_WIDTH, GEOMETRY_VEC4S, SIDE_TEXTURE_WIDTH, VALUE_VEC4S,
};
use crate::ir::{Expr, Form};
use crate::style::{Blend, MarkStyle, Tone};

const PRELUDE: &str = r#"
fn oklch(l: f32, c: f32, h: f32) -> vec3<f32> {
    let a = c * cos(radians(h));
    let b = c * sin(radians(h));
    let lm = l + 0.3963377774 * a + 0.2158037573 * b;
    let mm = l - 0.1055613458 * a - 0.0638541728 * b;
    let sm = l - 0.0894841775 * a - 1.2914855480 * b;
    let l3 = lm * lm * lm;
    let m3 = mm * mm * mm;
    let s3 = sm * sm * sm;
    return vec3<f32>(
        4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3,
        -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3,
        -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3,
    );
}

fn hue(k: f32) -> vec3<f32> { return oklch(0.68, 0.14, 360.0 * k); }

// WGSL's pow is undefined for a negative base, so whole exponents go through these.
fn square(x: f32) -> f32 { return x * x; }
fn cube(x: f32) -> f32 { return x * x * x; }
fn powi(x: f32, n: f32) -> f32 {
    let even = abs(n % 2.0) < 0.5;
    let magnitude = pow(abs(x), abs(n));
    let signed = select(sign(x) * magnitude, magnitude, even);
    return select(signed, 1.0 / signed, n < 0.0);
}

fn fmod(x: f32, y: f32) -> f32 { return x - y * floor(x / y); }
fn reveal(age: f32, now: f32, later: f32) -> f32 {
    return select(select(later, now, age < 0.5), 0.0, age < -0.5);
}
fn snap(x: f32, power: f32) -> f32 { return 1.0 - pow(1.0 - clamp(x, 0.0, 1.0), power); }
fn ease(a: f32, b: f32, t: f32) -> f32 {
    let x = clamp((t - a) / (b - a), 0.0, 1.0);
    return x * x * (3.0 - 2.0 * x);
}

fn encode(c: f32) -> f32 {
    let v = clamp(c, 0.0, 1.0);
    return select(1.055 * pow(v, 1.0 / 2.4) - 0.055, 12.92 * v, v <= 0.0031308);
}
fn decode(c: f32) -> f32 {
    return select(pow((c + 0.055) / 1.055, 2.4), c / 12.92, c <= 0.04045);
}
fn lift(c: f32) -> f32 {
    let v = max(c, 0.0);
    return select(1.055 * pow(v, 1.0 / 2.4) - 0.055, 12.92 * v, v <= 0.0031308);
}
fn lift3(c: vec3<f32>) -> vec3<f32> { return vec3<f32>(lift(c.r), lift(c.g), lift(c.b)); }
fn limited(x: f32) -> f32 { return clamp(x, -TANH_LIMIT, TANH_LIMIT); }
fn encode3(c: vec3<f32>) -> vec3<f32> { return vec3<f32>(encode(c.r), encode(c.g), encode(c.b)); }
fn decode3(c: vec3<f32>) -> vec3<f32> { return vec3<f32>(decode(c.r), decode(c.g), decode(c.b)); }
fn rgb(r: f32, g: f32, b: f32) -> vec3<f32> { return decode3(vec3<f32>(r, g, b) / 255.0); }

// `frame::polygon_distance`: signed pixels by nonzero winding, then its direction.
fn polygon(region: i32, p: vec2<f32>) -> vec3<f32> {
    let start = i32(geometry[region].x);
    let count = i32(geometry[region].y);
    if count < 3 { return vec3<f32>(1e9, 1.0, 0.0); }
    var best = 1e30;
    var toward = vec2<f32>(1.0, 0.0);
    var winding = 0;
    var a = geometry[start + count - 1].xy;
    for (var i = 0; i < count; i++) {
        let b = geometry[start + i].xy;
        let e = b - a;
        let w = p - a;
        let len2 = dot(e, e);
        let h = select(0.0, clamp(dot(w, e) / len2, 0.0, 1.0), len2 > 0.0);
        let v = w - e * h;
        let d2 = dot(v, v);
        if d2 < best {
            best = d2;
            toward = v;
        }
        let cross2 = e.x * w.y - e.y * w.x;
        if a.y <= p.y {
            if b.y > p.y && cross2 > 0.0 { winding += 1; }
        } else if b.y <= p.y && cross2 < 0.0 {
            winding -= 1;
        }
        a = b;
    }
    let rho = sqrt(best);
    let s = select(1.0, -1.0, winding != 0);
    let n = select(vec2<f32>(1.0, 0.0), s * toward / rho, rho > 1e-6);
    return vec3<f32>(s * rho, n);
}

"#;

/// `stroke::box_coverage`, shared by both shaders.
const BOX_COVERAGE: &str = r#"fn box_coverage(t: f32, n: vec2<f32>) -> f32 {
    let m = abs(n);
    let lo = min(m.x, m.y);
    let hi = max(m.x, m.y);
    let a = (hi - lo) / 2.0;
    let b = (hi + lo) / 2.0;
    let at = abs(t);
    if at >= b { return select(0.0, 1.0, t > 0.0); }
    if at <= a { return 0.5 + t / hi; }
    let tail = (b - at) * (b - at) / (2.0 * lo * hi);
    return select(tail, 1.0 - tail, t > 0.0);
}

"#;

/// Draws stroke segments into a coverage layer, implementing
/// `stroke::segment_coverage`. Blending by maximum combines overlapping segments.
pub const COVERAGE_SHADER: &str = concat!(
    r#"struct Uniforms { time: f32, width: f32, height: f32, dpr: f32 }
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) ab: vec4<f32>,
    @location(1) @interpolate(flat) rs: vec4<f32>,
    @location(2) @interpolate(flat) dash: vec4<f32>,
}

@vertex
fn vs(
    @builtin(vertex_index) i: u32,
    @location(0) ab: vec4<f32>,
    @location(1) rs: vec4<f32>,
    @location(2) dash: vec4<f32>,
) -> Varying {
    let a = ab.xy;
    let b = ab.zw;
    let len = length(b - a);
    let dir = select(vec2<f32>(1.0, 0.0), (b - a) / len, len > 1e-6);
    let normal = vec2<f32>(-dir.y, dir.x);
    let pad = max(rs.x, rs.y) + 1.5;
    let along = select(a - dir * pad, b + dir * pad, (i & 1u) == 1u);
    let p = along + normal * select(-pad, pad, (i & 2u) == 2u);
    var out: Varying;
    out.position = vec4<f32>(p.x / u.width * 2.0 - 1.0, 1.0 - p.y / u.height * 2.0, 0.0, 1.0);
    out.ab = ab;
    out.rs = rs;
    out.dash = dash;
    return out;
}

"#,
    r#"fn box_coverage(t: f32, n: vec2<f32>) -> f32 {
    let m = abs(n);
    let lo = min(m.x, m.y);
    let hi = max(m.x, m.y);
    let a = (hi - lo) / 2.0;
    let b = (hi + lo) / 2.0;
    let at = abs(t);
    if at >= b { return select(0.0, 1.0, t > 0.0); }
    if at <= a { return 0.5 + t / hi; }
    let tail = (b - at) * (b - at) / (2.0 * lo * hi);
    return select(tail, 1.0 - tail, t > 0.0);
}

"#,
    r#"@fragment
fn fs(in: Varying) -> @location(0) vec4<f32> {
    let p = in.position.xy;
    let a = in.ab.xy;
    let d = in.ab.zw - a;
    let len2 = dot(d, d);
    let along = select(0.0, dot(p - a, d) / len2, len2 > 1e-12);
    let t = clamp(along, 0.0, 1.0);
    let v = p - (a + d * t);
    let rho = length(v);
    let len = max(sqrt(len2), 1e-6);
    let side = select(vec2<f32>(1.0, 0.0), vec2<f32>(-d.y, d.x) / len, len2 > 1e-12);
    // Along the segment, its normal is exact. So near the centerline, where `v` is only rounding
    // error, a thin stroke's coverage doesn't depend on that error. Only a cap needs `v`.
    let n = select(v / rho, side, (along > 0.0 && along < 1.0) || rho < 0.01);
    let half_width = mix(in.rs.x, in.rs.y, t);
    var c = box_coverage(half_width - rho, n) - box_coverage(-half_width - rho, n);
    let period = in.dash.y;
    if period > 0.0 {
        let s = in.rs.z + (in.rs.w - in.rs.z) * along;
        let phase = s - floor(s / period) * period;
        let center = in.dash.x / 2.0;
        var away = abs(phase - center);
        away = min(away, period - away);
        c *= clamp(0.5 + center - away, 0.0, 1.0);
    }
    if in.dash.z > 0.0 {
        let position = in.rs.z + (in.rs.w - in.rs.z) * along;
        c *= clamp(0.5 + position, 0.0, 1.0);
        c *= clamp(0.5 + in.dash.z - position, 0.0, 1.0);
    }
    return vec4<f32>(c, 0.0, 0.0, 1.0);
}
"#
);

/// A number, typed as `f32`. Left abstract, an expression made of nothing but numbers, such as
/// `mix(0.45, 1.0, 1.0)` from a rule whose `self.focus` is always 1, can't be given a type.
fn literal(n: f64) -> String {
    let text = format!("{:?}f", n as f32);
    if n < 0.0 { format!("({text})") } else { text }
}

/// Source identifiers are full Unicode, and WGSL's aren't all portable.
fn mangle(name: &str) -> String {
    let mut out = String::from("v_");
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push_str(&format!("_u{:x}_", c as u32));
        }
    }
    out
}

fn is_whole(n: f64) -> bool {
    n.fract() == 0.0
}

pub fn emit_expr(e: &Expr) -> String {
    let call = |name: &str, args: &[Expr]| {
        let list: Vec<String> = args.iter().map(emit_expr).collect();
        format!("{name}({})", list.join(", "))
    };
    let infix = |op: &str, a: &Expr, b: &Expr| format!("({} {op} {})", emit_expr(a), emit_expr(b));
    match e {
        Expr::Num(n) => literal(*n),
        Expr::Sym(s) => match s.as_str() {
            "time" => "u.time".to_string(),
            "pi" | "π" => literal(std::f64::consts::PI),
            "tau" => literal(std::f64::consts::TAU),
            "white" => "vec3<f32>(1.0)".to_string(),
            "black" => "vec3<f32>(0.0)".to_string(),
            "self.dist" => "self_dist".to_string(),
            "self.pixel" => "pixel".to_string(),
            "self.curvature" => "self_curvature".to_string(),
            other => mangle(other),
        },
        Expr::App(head, args) => match (head.as_str(), args.as_slice()) {
            ("Add", [a, b]) => infix("+", a, b),
            ("Subtract", [a, b]) => infix("-", a, b),
            ("Multiply", [a, b]) => infix("*", a, b),
            ("Divide", [a, b]) => infix("/", a, b),
            ("Negate", [a]) => format!("(-{})", emit_expr(a)),
            ("Power", [a, Expr::Num(n)]) if *n == 2.0 => format!("square({})", emit_expr(a)),
            ("Power", [a, Expr::Num(n)]) if *n == 3.0 => format!("cube({})", emit_expr(a)),
            ("Power", [a, Expr::Num(n)]) if is_whole(*n) => {
                format!("powi({}, {})", emit_expr(a), literal(*n))
            }
            ("Power", [a, b]) => format!("pow({}, {})", emit_expr(a), emit_expr(b)),
            ("Sqrt", _) => call("sqrt", args),
            ("Sin", _) => call("sin", args),
            ("Cos", _) => call("cos", args),
            ("Tan", _) => call("tan", args),
            ("Arccos", _) => call("acos", args),
            ("Floor", _) => call("floor", args),
            ("Reveal", _) => call("reveal", args),
            ("Sinh", [a]) => format!("sinh(limited({}))", emit_expr(a)),
            ("Cosh", [a]) => format!("cosh(limited({}))", emit_expr(a)),
            ("Tanh", [a]) => format!("tanh(limited({}))", emit_expr(a)),
            ("Exp", _) => call("exp", args),
            ("Ln", _) => call("log", args),
            ("Abs", _) => call("abs", args),
            ("Sign", _) => call("sign", args),
            ("Arctan2", _) => call("atan2", args),
            ("Min", _) => call("min", args),
            ("Max", _) => call("max", args),
            ("Clamp", _) => call("clamp", args),
            ("Mod", _) => call("fmod", args),
            ("Smoothstep", _) => call("ease", args),
            ("Oklch", _) => call("oklch", args),
            ("Rgb", _) => call("rgb", args),
            ("Hue", _) => call("hue", args),
            ("Mix", _) => call("mix", args),
            ("Snap", [x]) => format!("snap({}, {})", emit_expr(x), literal(SNAP_POWER)),
            ("Snap", [x, power]) => format!("snap({}, {})", emit_expr(x), emit_expr(power)),
            ("Pixels", [a]) => emit_expr(a),
            _ => panic!(
                "the type checker admitted `{head}` with {} arguments",
                args.len()
            ),
        },
    }
}

fn opacity_of(style: &MarkStyle) -> String {
    match &style.opacity {
        Some(o) => format!("clamp({}, 0.0, 1.0)", emit_expr(o)),
        None => "1.0".to_string(),
    }
}

pub fn emit(figure: &Figure) -> String {
    let mut out = format!(
        "struct Uniforms {{ time: f32, width: f32, height: f32, dpr: f32, view: vec4<f32>, origin: vec2<f32>, values: array<vec4<f32>, {VALUE_VEC4S}> }}\n\
         @group(0) @binding(0) var<uniform> u: Uniforms;\n\
         @group(0) @binding(1) var coverage: texture_2d_array<f32>;\n\
         @group(0) @binding(2) var<uniform> geometry: array<vec4<f32>, {GEOMETRY_VEC4S}>;\n\
         @group(0) @binding(3) var sides: texture_2d<f32>;\n\n\
         @vertex\n\
         fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{\n    \
             let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));\n    \
             return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);\n\
         }}\n"
    );
    out.push_str(&format!("const TANH_LIMIT: f32 = {TANH_LIMIT:?};\n"));
    out.push_str(PRELUDE);
    out.push_str(BOX_COVERAGE);
    out.push_str(
        "@fragment\nfn fs(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {\n",
    );
    // The view arrives as a uniform, since a `fit` viewport changes it per frame.
    out.push_str("    let sx = u.view.y - u.view.x;\n    let sy = u.view.w - u.view.z;\n");
    // A panel's pixels start at its corner of the canvas.
    out.push_str("    let local = position.xy - u.origin;\n");
    out.push_str("    let world_x = u.view.x + local.x / u.width * sx;\n");
    out.push_str("    let world_y = u.view.w - local.y / u.height * sy;\n");
    out.push_str("    let pixel = (sx / u.width + sy / u.height) / 2.0;\n");
    for (i, binding) in figure.scene.bindings.iter().enumerate() {
        out.push_str(&format!(
            "    let {}: f32 = u.values[{}][{}];\n",
            mangle(&binding.id),
            i / 4,
            i % 4,
        ));
    }
    match &figure.styles.paper {
        Some(paper) => out.push_str(&format!(
            "    var color: vec3<f32> = {};\n",
            emit_expr(paper)
        )),
        None => out.push_str("    var color = vec3<f32>(1.0);\n"),
    }
    let slots: std::collections::HashMap<usize, usize> = figure.presence_slots().collect();
    let sided = figure.sided();
    let mut layer = 0;
    let mut region = 0;
    for (i, (mark, style)) in figure
        .scene
        .marks
        .iter()
        .zip(&figure.styles.marks)
        .enumerate()
    {
        if matches!(mark.form, Form::Label { .. } | Form::Term { .. }) {
            continue;
        }
        // A mark that can vanish skips all its work when it has. Absent bindings arrive as
        // zero, and nothing that could read one runs.
        match slots.get(&i) {
            Some(slot) => out.push_str(&format!(
                "    if u.values[{}][{}] > 0.5 {{ // {}\n",
                slot / 4,
                slot % 4,
                mark.id
            )),
            None => out.push_str(&format!("    {{ // {}\n", mark.id)),
        }
        let mut measured = true;
        match &mark.form {
            Form::Field {
                vars,
                field,
                grad,
                exact,
                curvature,
                ..
            } => {
                out.push_str(&format!("        let {} = world_x;\n", mangle(&vars[0])));
                out.push_str(&format!("        let {} = world_y;\n", mangle(&vars[1])));
                out.push_str(&format!("        let field: f32 = {};\n", emit_expr(field)));
                out.push_str(&format!(
                    "        let gradient = vec2<f32>({}, {});\n",
                    emit_expr(&grad[0]),
                    emit_expr(&grad[1]),
                ));
                out.push_str("        let glen = length(gradient);\n");
                // A true distance doesn't need its gradient, which is lost at a circle's center.
                if *exact {
                    out.push_str("        let self_dist = field;\n");
                } else {
                    out.push_str(&format!(
                        "        let self_dist = field / max(glen, {});\n",
                        literal(MIN_GRADIENT),
                    ));
                }
                if let Some(curvature) = curvature {
                    out.push_str(&format!(
                        "        let self_curvature: f32 = {};\n",
                        emit_expr(curvature)
                    ));
                }
                out.push_str(&format!(
                    "        let normal = select(vec2<f32>(1.0, 0.0), gradient / glen, glen > {});\n",
                    literal(MIN_GRADIENT),
                ));
                out.push_str("        let dist_px = self_dist / pixel;\n");
            }
            Form::Region(_) => {
                out.push_str(&format!(
                    "        let measured = polygon({region}, local);\n"
                ));
                region += 1;
                out.push_str("        let dist_px = measured.x;\n");
                out.push_str("        let normal = measured.yz;\n");
                out.push_str("        let self_dist = dist_px * pixel;\n");
            }
            Form::Dot(at) => {
                out.push_str(&format!(
                    "        let center = vec2<f32>(({} - u.view.x) / sx * u.width, (u.view.w - {}) / sy * u.height);\n",
                    emit_expr(&at[0]),
                    emit_expr(&at[1]),
                ));
                let radius = match &style.radius {
                    Some(r) => emit_expr(r),
                    None => literal(DEFAULT_DOT_RADIUS),
                };
                out.push_str(&format!("        let radius = {radius} * u.dpr;\n"));
                out.push_str("        let offset = local - center;\n");
                out.push_str("        let rho = length(offset);\n");
                out.push_str(
                    "        let normal = select(vec2<f32>(1.0, 0.0), offset / rho, rho > 1e-6);\n",
                );
                out.push_str("        let dist_px = rho - radius;\n");
                out.push_str("        let self_dist = dist_px * pixel;\n");
            }
            _ => measured = false,
        }
        out.push_str(&format!("        let opacity = {};\n", opacity_of(style)));
        let fill_opacity = match &style.fill_opacity {
            Some(o) => format!("opacity * clamp({}, 0.0, 1.0)", emit_expr(o)),
            None => "opacity".to_string(),
        };
        let blend = style.blend;
        let adds = blend == Blend::Add;
        let into = |e: String| match blend {
            Blend::Gamma => format!("encode3({e})"),
            Blend::Add => format!("lift3({e})"),
            Blend::Linear => e,
        };
        let back = |e: String| match blend {
            Blend::Gamma | Blend::Add => format!("decode3({e})"),
            Blend::Linear => e,
        };
        // What one side adds to the color, as `covered` and `filled`, from `dist_px` and `normal`.
        let sides_of = |out: &mut String| {
            out.push_str("        let negative = box_coverage(-dist_px, normal);\n");
            out.push_str("        var covered = 0.0;\n        var filled = vec3<f32>(0.0);\n");
            // `::inside` and `::outside` only exist on a circle, and win over the sign they share.
            let round = style.inside.is_some() || style.outside.is_some();
            let bare_negative = "negative".to_string();
            let bare_positive = "(1.0 - negative)".to_string();
            let gated = |side: String, gates: [f64; 2]| {
                if round {
                    format!(
                        "({side} * select({}, {}, self_curvature >= 0.0))",
                        literal(gates[1]),
                        literal(gates[0]),
                    )
                } else {
                    side
                }
            };
            let [negative_gates, positive_gates] = style.sign_gates();
            let disk = "select(1.0 - negative, negative, self_curvature >= 0.0)".to_string();
            for (fill, weight) in [
                (&style.negative, gated(bare_negative, negative_gates)),
                (&style.positive, gated(bare_positive, positive_gates)),
                (&style.inside, disk.clone()),
                (&style.outside, format!("(1.0 - {disk})")),
            ] {
                if let Some(fill) = fill {
                    out.push_str(&format!("        covered += {weight} * {fill_opacity};\n"));
                    out.push_str(&format!(
                        "        filled += {} * {weight} * {fill_opacity};\n",
                        into(emit_expr(fill)),
                    ));
                }
            }
        };
        if measured {
            sides_of(&mut out);
            out.push_str(&format!(
                "        color = {};\n",
                back(if adds {
                    format!("{} + filled", into("color".to_string()))
                } else {
                    format!("{} * (1.0 - covered) + filled", into("color".to_string()))
                })
            ));
        }
        // Every circle, in order, from the data texture. Going in and out of the blend's own space
        // happens once around all of them.
        if let Some(group) = sided.iter().find(|group| group.mark == i) {
            out.push_str(&format!(
                "        var blended = {};\n",
                into("color".to_string())
            ));
            out.push_str(&format!(
                "        let sided_count = i32(u.values[{}][{}]);\n",
                group.slot / 4,
                group.slot % 4,
            ));
            out.push_str("        for (var instance = 0; instance < sided_count; instance++) {\n");
            out.push_str(&format!(
                "        let texel = {} + instance * {};\n",
                group.base, group.stride,
            ));
            for row in 0..group.stride {
                out.push_str(&format!(
                    "        let row{row} = textureLoad(sides, vec2<i32>((texel + {row}) % {SIDE_TEXTURE_WIDTH}, (texel + {row}) / {SIDE_TEXTURE_WIDTH}), 0);\n",
                ));
            }
            for (k, name) in group.names.iter().enumerate() {
                out.push_str(&format!(
                    "        let {}: f32 = row{}[{}];\n",
                    mangle(name),
                    1 + k / 4,
                    k % 4,
                ));
            }
            out.push_str("        let self_curvature = row0.x;\n");
            out.push_str(
                "        let field = row0.x * (world_x * world_x + world_y * world_y) - 2.0 * (row0.y * world_x + row0.z * world_y) + row0.w;\n",
            );
            out.push_str("        let spread = 1.0 + row0.x * field;\n");
            out.push_str("        let self_dist = field / (1.0 + sqrt(max(spread, 0.0)));\n");
            out.push_str(
                "        let gradient = vec2<f32>(row0.x * world_x - row0.y, row0.x * world_y - row0.z) / sqrt(max(spread, 1e-12));\n",
            );
            out.push_str("        let glen = length(gradient);\n");
            out.push_str(&format!(
                "        let normal = select(vec2<f32>(1.0, 0.0), gradient / glen, glen > {});\n",
                literal(MIN_GRADIENT),
            ));
            out.push_str("        let dist_px = self_dist / pixel;\n");
            sides_of(&mut out);
            out.push_str(if adds {
                "        blended += filled;\n"
            } else {
                "        blended = blended * (1.0 - covered) + filled;\n"
            });
            out.push_str("        }\n");
            out.push_str(&format!(
                "        color = {};\n",
                back("blended".to_string())
            ));
        }
        if let Some(stroke) = &style.stroke {
            if style.stroke_is_line(&mark.form) {
                out.push_str(&format!(
                    "        let stroke_coverage = textureLoad(coverage, vec2<i32>(local), {layer}, 0).r;\n",
                ));
                layer += 1;
            } else if measured {
                let width = match &style.width {
                    Some(w) => emit_expr(w),
                    None => literal(DEFAULT_STROKE_WIDTH),
                };
                out.push_str(&format!(
                    "        let half_width = {width} * u.dpr / 2.0;\n"
                ));
                out.push_str(
                    "        let stroke_coverage = box_coverage(half_width - abs(dist_px), normal) - box_coverage(-half_width - abs(dist_px), normal);\n",
                );
            } else {
                out.push_str("        let stroke_coverage = 0.0;\n");
            }
            out.push_str(&format!(
                "        color = {};\n",
                back(if adds {
                    format!(
                        "{} + {} * stroke_coverage * opacity",
                        into("color".to_string()),
                        into(emit_expr(stroke)),
                    )
                } else {
                    format!(
                        "mix({}, {}, stroke_coverage * opacity)",
                        into("color".to_string()),
                        into(emit_expr(stroke)),
                    )
                })
            ));
        }
        out.push_str("    }\n");
    }
    match &figure.styles.tone {
        Tone::Tanh => out.push_str(
            "    return vec4<f32>(tanh(limited(lift(color.r))), tanh(limited(lift(color.g))), tanh(limited(lift(color.b))), 1.0);\n}\n",
        ),
        Tone::Clip => out.push_str(
            "    return vec4<f32>(encode(color.r), encode(color.g), encode(color.b), 1.0);\n}\n",
        ),
        // `eval::Frame::shade`: clipped and rolled off in display values, mixed.
        Tone::Mix(k) => out.push_str(&format!(
            "    let tone = clamp({}, 0.0, 1.0);\n    return vec4<f32>(mix(encode(color.r), tanh(limited(lift(color.r))), tone), mix(encode(color.g), tanh(limited(lift(color.g))), tone), mix(encode(color.b), tanh(limited(lift(color.b))), tone), 1.0);\n}}\n",
            emit_expr(k)
        )),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;

    fn validate(source: &str) {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{}\n{source}", e.emit_to_string(source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    }

    #[test]
    fn emits_valid_wgsl_for_every_form_and_expression() {
        let doc = "
            param a in 0..1
            bind a = mod(time, 1)
            let π₂ = pi / 2
            let k = sqrt(2) + sin(time) * cos(π₂) - tan(0.1) / exp(1) + ln(2) + abs(-1) + sign(2)
            let s = 2 ^ k + k^4 + k^3 + k^(-3) + atan2(k, 1) + -k + min(1, 2) + max(1, 2)
            let P = (a, s)
            curve c = implicit (x, y): x^2 + y^2 = s
            curve d = implicit (θ, ρ): θ = ρ
            curve e = sdf (x, y): sqrt(x^2 + y^2) - a
            curve f = parametric (t): (t, a) for t in 0..1
            segment g = (0, 0), P
            region h = (0, 0), (cos(t), sin(t)) for t in 0..a
            dot P
            label l = \"L\" at P";
        let sheet = "
            style paper { fill: rgb(242, 240, 235); tone: tanh }
            style c::inside  { fill: oklch(0.7, 0.1, 40 * time) }
            style c::outside { fill: mix(hue(0.2), white, 0.5 + 0.5 * cos(24 * self.dist)) }
            style c          { stroke: black; width: 2px; draw: field; blend: linear }
            style d::negative { fill: black }
            style d          { stroke: black; width: 1px + 2px * self.t; dash: 4px }
            style e          { stroke: black; width: 2px; opacity: smoothstep(0, 1, a); fill_opacity: 0.5 }
            style e::inside  { fill: white }
            style f, g       { stroke: black; width: 2px }
            style h          { fill: mix(white, black, clamp(self.dist, 0, 1)) * 2 + white / 3 - black; stroke: black; blend: add; fill_opacity: tanh(self.pixel / abs(self.dist)) + sinh(1) - cosh(1) }
            style P          { fill: black; radius: 4px; stroke: white; width: 1px }";
        let figure = build(doc, &[sheet]).unwrap();
        validate(&emit(&figure));
    }

    #[test]
    fn the_coverage_shader_is_valid() {
        validate(COVERAGE_SHADER);
    }

    #[test]
    fn both_shaders_share_one_box_filter() {
        assert!(COVERAGE_SHADER.contains(BOX_COVERAGE));
    }

    #[test]
    fn whole_powers_avoid_pow() {
        let square = Expr::app("Power", vec![Expr::sym("x"), Expr::Num(2.0)]);
        assert_eq!(emit_expr(&square), "square(v_x)");
    }

    #[test]
    fn identifiers_are_mangled_stably() {
        assert_eq!(mangle("r"), "v_r");
        assert_eq!(mangle("θ"), "v__u3b8_");
        assert_eq!(mangle("P.x"), "v_P_u2e_x");
    }
}
