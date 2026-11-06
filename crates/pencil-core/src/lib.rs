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
pub mod place;
pub mod presence;
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
    /// For each mark, whether it can stop existing. Most can't, and those cost nothing.
    pub conditional: Vec<bool>,
}

impl Figure {
    /// Where the shader finds each conditional mark's presence: mark index, then index
    /// in `Prepared::values`, after the bindings. Labels are drawn off the GPU.
    pub fn presence_slots(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.conditional
            .iter()
            .enumerate()
            .filter(|(i, conditional)| {
                **conditional && !matches!(self.scene.marks[*i].form, ir::Form::Label { .. })
            })
            .enumerate()
            .map(|(slot, (i, _))| (i, self.scene.bindings.len() + slot))
    }
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
    let conditional = presence::conditional_marks(&scene, &styles)?;
    Ok(Figure {
        scene,
        styles,
        conditional,
    })
}
