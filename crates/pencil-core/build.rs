//! Fingerprints the core's source, so `compiled` can tell a figure written by this build from one
//! written by any other. Every `.rs` file under `src/` and `Cargo.toml` go into one FNV-1a hash,
//! by path and then contents, in sorted order so the same tree gives the same number on any
//! machine. Nothing is kept by hand, so there is no version to forget to bump.
//!
//! It also reads the math font the package ships, `packages/figure/fonts`, and writes the
//! metrics `formula` lays a label's mathematics out with: each glyph's advance, ink and italic
//! correction, and the font's own constants for scripts, fractions and roots. The command and a
//! page draw with that same file, so core can place its glyphs without either telling it how
//! wide they are, and both get the same boxes. The WASM carries the numbers, not the font.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

fn feed(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(PRIME);
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets this"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files)?;
    files.push(root.join("Cargo.toml"));
    files.sort();
    let mut hash = OFFSET;
    for file in &files {
        let name = file.strip_prefix(&root).unwrap_or(file);
        // A path and a length before each file, so moving text between files changes the hash.
        let text = fs::read(file)?;
        feed(
            &mut hash,
            name.to_string_lossy().replace('\\', "/").as_bytes(),
        );
        feed(&mut hash, &(text.len() as u64).to_le_bytes());
        feed(&mut hash, &text);
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets this"));
    fs::write(
        out.join("fingerprint.rs"),
        format!("pub const FINGERPRINT: u64 = {hash:#018x};\n"),
    )?;
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
    math_metrics(&root, &out)
}

/// The characters a label's mathematics can be set from: Latin, Greek, punctuation, the
/// letterlike and operator blocks, arrows, brackets, and the mathematical alphabets.
const RANGES: [(u32, u32); 12] = [
    (0x20, 0x17f),
    (0x370, 0x3ff),
    (0x2000, 0x206f),
    (0x2070, 0x209f),
    (0x2100, 0x214f),
    (0x2190, 0x21ff),
    (0x2200, 0x22ff),
    (0x2300, 0x23ff),
    (0x27c0, 0x27ff),
    (0x2980, 0x29ff),
    (0x2a00, 0x2aff),
    (0x1d400, 0x1d7ff),
];

fn math_metrics(root: &Path, out: &Path) -> io::Result<()> {
    let file = root.join("../../packages/figure/fonts/LibertinusMath-Regular.otf");
    println!("cargo:rerun-if-changed={}", file.display());
    let bytes = fs::read(&file)?;
    let bad = |what: &str| io::Error::other(format!("{}: {what}", file.display()));
    let face = ttf_parser::Face::parse(&bytes, 0).map_err(|e| bad(&e.to_string()))?;
    let math = face.tables().math.ok_or_else(|| bad("no MATH table"))?;
    let constants = math.constants.ok_or_else(|| bad("no math constants"))?;
    let italics = math.glyph_info.and_then(|info| info.italic_corrections);

    let mut text = String::from("// Written by build.rs from the package's math font.\n");
    let mut constant = |name: &str, value: i32| {
        text.push_str(&format!("pub const {name}: f64 = {value}.0;\n"));
    };
    constant("UNITS_PER_EM", i32::from(face.units_per_em()));
    constant("ASCENDER", i32::from(face.ascender()));
    constant("DESCENDER", i32::from(face.descender()));
    constant("X_HEIGHT", i32::from(face.x_height().unwrap_or(430)));
    constant(
        "SCRIPT_PERCENT",
        i32::from(constants.script_percent_scale_down()),
    );
    constant(
        "SCRIPT_SCRIPT_PERCENT",
        i32::from(constants.script_script_percent_scale_down()),
    );
    let values = [
        ("AXIS_HEIGHT", constants.axis_height()),
        ("SUBSCRIPT_SHIFT_DOWN", constants.subscript_shift_down()),
        ("SUBSCRIPT_TOP_MAX", constants.subscript_top_max()),
        (
            "SUBSCRIPT_BASELINE_DROP_MIN",
            constants.subscript_baseline_drop_min(),
        ),
        ("SUPERSCRIPT_SHIFT_UP", constants.superscript_shift_up()),
        ("SUPERSCRIPT_BOTTOM_MIN", constants.superscript_bottom_min()),
        (
            "SUPERSCRIPT_BASELINE_DROP_MAX",
            constants.superscript_baseline_drop_max(),
        ),
        (
            "SUB_SUPERSCRIPT_GAP_MIN",
            constants.sub_superscript_gap_min(),
        ),
        ("SPACE_AFTER_SCRIPT", constants.space_after_script()),
        (
            "FRACTION_NUMERATOR_SHIFT_UP",
            constants.fraction_numerator_shift_up(),
        ),
        (
            "FRACTION_DENOMINATOR_SHIFT_DOWN",
            constants.fraction_denominator_shift_down(),
        ),
        (
            "FRACTION_NUMERATOR_GAP_MIN",
            constants.fraction_numerator_gap_min(),
        ),
        (
            "FRACTION_DENOMINATOR_GAP_MIN",
            constants.fraction_denominator_gap_min(),
        ),
        (
            "FRACTION_RULE_THICKNESS",
            constants.fraction_rule_thickness(),
        ),
        ("RADICAL_VERTICAL_GAP", constants.radical_vertical_gap()),
        ("RADICAL_RULE_THICKNESS", constants.radical_rule_thickness()),
        ("RADICAL_EXTRA_ASCENDER", constants.radical_extra_ascender()),
    ];
    for (name, value) in values {
        constant(name, i32::from(value.value));
    }

    // `(character, advance, ink bottom, ink top, ink right, italic correction)`, in font units
    // and in order, so a character is found by halving.
    text.push_str("pub const GLYPHS: &[(u32, i16, i16, i16, i16, i16)] = &[\n");
    for (from, to) in RANGES {
        for code in from..=to {
            let Some(character) = char::from_u32(code) else {
                continue;
            };
            let Some(glyph) = face.glyph_index(character) else {
                continue;
            };
            let advance = face.glyph_hor_advance(glyph).unwrap_or(0);
            let (bottom, top, right) = face
                .glyph_bounding_box(glyph)
                .map_or((0, 0, 0), |b| (b.y_min, b.y_max, b.x_max));
            let italic = italics
                .and_then(|values| values.get(glyph))
                .map_or(0, |v| v.value);
            text.push_str(&format!(
                "    ({code:#x}, {advance}, {bottom}, {top}, {right}, {italic}),\n"
            ));
        }
    }
    text.push_str("];\n");
    fs::write(out.join("math_metrics.rs"), text)
}
