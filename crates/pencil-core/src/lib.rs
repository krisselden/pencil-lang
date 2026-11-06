//! Pencil's core: `.pen` text to a symbolic IR, styles, and evaluation.
//!
//! The pipeline is `parse` → `compile` (the document's IR) → `style` (resolved
//! style expressions, separate from the IR) → `eval` (the CPU oracle) or `wgsl`
//! (the GPU path). Both evaluation paths implement the same shading, defined in
//! one place each and compared by `pencil check`.

pub mod compile;
pub mod diff;
pub mod error;
pub mod eval;
pub mod frame;
pub mod ir;
pub mod parse;
pub mod stroke;
pub mod style;
pub mod trace;
pub mod wgsl;

pub use error::Error;

/// A document compiled against its stylesheets, ready to evaluate or emit.
#[derive(Debug)]
pub struct Figure {
    pub scene: ir::Scene,
    pub styles: style::Styled,
}

/// Compile a document and apply stylesheets, later sheets overriding earlier ones.
/// The document's own `style` rules come last, as the `document` layer of SPEC/15.
pub fn build(doc: &str, sheets: &[&str]) -> Result<Figure, Error> {
    let document = parse::parse(doc)?;
    let scene = compile::compile(&document)?;
    let mut parsed = Vec::with_capacity(sheets.len() + 1);
    for sheet in sheets {
        parsed.push(parse::parse(sheet)?);
    }
    parsed.push(document);
    let styles = style::resolve(&scene, &parsed)?;
    Ok(Figure { scene, styles })
}
