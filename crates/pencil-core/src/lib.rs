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
pub mod fast;
pub mod frame;
pub mod ir;
pub mod parse;
pub mod place;
pub mod presence;
pub mod stroke;
pub mod style;
pub mod terms;
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
    /// Its systems' last answers. See `eval::Solved`. Panels with the same systems share them.
    pub solved: std::sync::Arc<eval::Solved>,
}

/// Where one collection's sides are in the frame's data texture.
#[derive(Debug, Clone)]
pub struct Sided {
    pub mark: usize,
    /// Where its count is in `Prepared::values`.
    pub slot: usize,
    /// The texel it starts at.
    pub base: usize,
    /// Texels per instance: `a bx by c`, then what its style reads, four to a texel.
    pub stride: usize,
    /// What its style reads per instance.
    pub names: Vec<String>,
}

impl Figure {
    /// The collections of circles with colored sides, in mark order. Each has a place in the
    /// frame's data texture, and a count in `Prepared::values`, after the presence.
    pub fn sided(&self) -> Vec<Sided> {
        let mut base = 0;
        let mut slot = self.scene.bindings.len() + self.presence_slots().count();
        let mut out = Vec::new();
        for (mark, (m, style)) in self.scene.marks.iter().zip(&self.styles.marks).enumerate() {
            let ir::Form::Many {
                vars,
                lets,
                shape: ir::Shape::Circles { .. } | ir::Shape::Cycles { .. },
            } = &m.form
            else {
                continue;
            };
            if !style.has_sides() {
                continue;
            }
            let mut read = std::collections::HashSet::new();
            for e in style.side_exprs() {
                e.visit_syms(&mut |name| {
                    read.insert(name.to_string());
                });
            }
            let names: Vec<String> = vars
                .iter()
                .map(|(name, _)| name)
                .chain(lets.iter().map(|(name, _)| name))
                .filter(|name| read.contains(*name))
                .cloned()
                .collect();
            let stride = 1 + names.len().div_ceil(4);
            out.push(Sided {
                mark,
                slot,
                base,
                stride,
                names,
            });
            base += stride * frame::MAX_SIDED_CIRCLES;
            slot += 1;
        }
        out
    }

    /// Where the shader finds each conditional mark's presence: mark index, then index
    /// in `Prepared::values`, after the bindings. Labels are drawn off the GPU.
    pub fn presence_slots(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.conditional
            .iter()
            .enumerate()
            .filter(|(i, conditional)| {
                **conditional
                    && !matches!(
                        self.scene.marks[*i].form,
                        ir::Form::Label { .. } | ir::Form::Term { .. }
                    )
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
        solved: Default::default(),
    })
}

/// One view of a document, and where it sits, as fractions of the whole: `x y width height`.
#[derive(Debug)]
pub struct Panel {
    pub id: String,
    pub title: Option<String>,
    /// A formula beside the title.
    pub formula: Option<String>,
    pub figure: Figure,
    pub rect: [f64; 4],
}

/// What a document draws: one view, or several side by side on one canvas.
#[derive(Debug)]
pub struct Composition {
    pub panels: Vec<Panel>,
    /// The whole thing's design size, for its shape.
    pub size: [f64; 2],
}

fn parsed_sheets(doc: &parse::Document, sheets: &[&str]) -> Result<Vec<parse::Document>, Error> {
    let mut parsed = Vec::with_capacity(sheets.len() + 1);
    for sheet in sheets {
        parsed.push(parse::parse(sheet)?);
    }
    parsed.push(doc.clone());
    Ok(parsed)
}

/// Compiles a document with or without panels, against its stylesheets. A rule that names a
/// mark has to find it in at least one panel.
pub fn compose(doc: &str, sheets: &[&str]) -> Result<Composition, Error> {
    let document = parse::parse(doc)?;
    let has_panels = document
        .items
        .iter()
        .any(|i| matches!(i, parse::Item::Panel { .. }));
    if !has_panels {
        let figure = build(doc, sheets)?;
        let view = &figure.scene.viewport;
        let size = match (view.design_width, view.design_height) {
            (Some(w), Some(h)) => [w, h],
            _ => [view.x[1] - view.x[0], view.y[1] - view.y[0]],
        };
        return Ok(Composition {
            panels: vec![Panel {
                id: String::new(),
                title: None,
                formula: None,
                figure,
                rect: [0.0, 0.0, 1.0, 1.0],
            }],
            size,
        });
    }
    let scenes = compile::compile_panels(&document)?;
    let parsed = parsed_sheets(&document, sheets)?;
    let mut figures = Vec::with_capacity(scenes.len());
    let mut missing: Option<Vec<Error>> = None;
    for panel in scenes {
        let mut unmatched = Vec::new();
        let styles =
            style::resolve_keeping(&panel.scene, Some(&panel.id), &parsed, &mut unmatched)?;
        // Only what every panel lacks is a mistake.
        missing = Some(match missing {
            None => unmatched,
            Some(so_far) => so_far
                .into_iter()
                .filter(|e| unmatched.contains(e))
                .collect(),
        });
        let conditional = presence::conditional_marks(&panel.scene, &styles)?;
        figures.push((
            (panel.id, panel.title, panel.formula),
            Figure {
                scene: panel.scene,
                styles,
                conditional,
                // The same equations and the same params have the same answers.
                solved: Default::default(),
            },
        ));
        let newest = figures.len() - 1;
        let (first, last) = figures.split_at_mut(newest);
        if let Some(((_, shared), (_, figure))) = first.first().zip(last.first_mut())
            && shared.scene.systems == figure.scene.systems
        {
            figure.solved = shared.solved.clone();
        }
    }
    if let Some(error) = missing.and_then(|m| m.into_iter().next()) {
        return Err(error);
    }

    // Where panels sit and how much they show is the style's, and it wins over the viewport.
    let constant = |e: &Option<ir::Expr>| {
        e.as_ref()
            .map(|e| eval::real(e, &eval::Env::new(0.0)))
            .transpose()
    };
    for (_, figure) in &mut figures {
        let style = &figure.styles.panels;
        let (width, height, pad) = (
            constant(&style.width)?,
            constant(&style.height)?,
            constant(&style.pad)?,
        );
        let view = &mut figure.scene.viewport;
        view.design_width = width.or(view.design_width).or(Some(PANEL_SIZE));
        view.design_height = height.or(view.design_height).or(Some(PANEL_SIZE));
        view.pad = pad.unwrap_or(view.pad);
    }
    compile::fill_outlines(
        figures
            .iter_mut()
            .map(|(names, figure)| (&names.0, &mut figure.scene)),
    )?;

    let layout = &figures[0].1.styles.panels;
    let columns = (constant(&layout.columns)?.unwrap_or(1.0) as usize).max(1);
    let gap = constant(&layout.gap)?.unwrap_or(0.0);
    let band = constant(&layout.title)?.unwrap_or(0.0);
    let view = &figures[0].1.scene.viewport;
    let (w, h) = (
        view.design_width.unwrap_or(PANEL_SIZE),
        view.design_height.unwrap_or(PANEL_SIZE),
    );
    if figures.iter().any(|(_, f)| {
        f.scene.viewport.design_width != Some(w) || f.scene.viewport.design_height != Some(h)
    }) {
        return Err(Error::new("every panel has to be the same size, for now"));
    }
    let rows = figures.len().div_ceil(columns);
    let total_w = columns as f64 * w + (columns - 1) as f64 * gap;
    let total_h = rows as f64 * (h + band) + (rows - 1) as f64 * gap;
    let panels = figures
        .into_iter()
        .enumerate()
        .map(|(i, ((id, title, formula), figure))| {
            let (column, row) = (i % columns, i / columns);
            Panel {
                id,
                title,
                formula,
                figure,
                rect: [
                    column as f64 * (w + gap) / total_w,
                    (row as f64 * (h + band + gap) + band) / total_h,
                    w / total_w,
                    h / total_h,
                ],
            }
        })
        .collect();
    Ok(Composition {
        panels,
        size: [total_w, total_h],
    })
}

/// The colors sheets name, for a page to match the figure it holds. Each is what the name
/// means after every sheet, as a figure would draw it.
pub fn named_colors(sheets: &[&str]) -> Result<Vec<(String, [f64; 3])>, Error> {
    let parsed = sheets
        .iter()
        .map(|sheet| parse::parse(sheet))
        .collect::<Result<Vec<_>, _>>()?;
    let mut out: Vec<(String, [f64; 3])> = style::sheet_names(&parsed)?
        .into_iter()
        .filter(|(_, e)| style::type_of(e, &[], false) == Ok(style::Ty::Color))
        .map(|(name, e)| Ok((name, eval::color(&e, &eval::Env::new(0.0))?)))
        .collect::<Result<_, Error>>()?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// A panel's frame, and where on the canvas it goes, in device pixels.
#[derive(Debug)]
pub struct PanelFrame {
    pub origin: [u32; 2],
    pub width: u32,
    pub height: u32,
    pub prepared: frame::Prepared,
}

impl Composition {
    /// A panel's place on a canvas of this size, in device pixels: `x y width height`.
    pub fn rect_px(&self, panel: usize, width: u32, height: u32) -> [u32; 4] {
        let [x, y, w, h] = self.panels[panel].rect;
        let (cw, ch) = (width as f64, height as f64);
        let (x0, y0) = ((x * cw).round(), (y * ch).round());
        let (x1, y1) = (((x + w) * cw).round(), ((y + h) * ch).round());
        [
            x0 as u32,
            y0 as u32,
            ((x1 - x0) as u32).max(1),
            ((y1 - y0) as u32).max(1),
        ]
    }

    /// Computes every panel's CPU side. `dpr` is for a figure with no design width.
    pub fn prepare(
        &self,
        inputs: &frame::Inputs,
        width: u32,
        height: u32,
        dpr: f64,
        widths: &dyn terms::TextWidths,
    ) -> Result<Vec<PanelFrame>, Error> {
        (0..self.panels.len())
            .map(|i| {
                let [x, y, w, h] = self.rect_px(i, width, height);
                let figure = &self.panels[i].figure;
                let scale = frame::design_scale(figure, w).unwrap_or(dpr);
                Ok(PanelFrame {
                    origin: [x, y],
                    width: w,
                    height: h,
                    prepared: frame::prepare_with(figure, inputs, w, h, scale, widths)?,
                })
            })
            .collect()
    }

    /// Every label on the canvas, in its device pixels, with each panel's title. A title sits
    /// at the panel's left, so it needs real text widths.
    pub fn labels(
        &self,
        frames: &[PanelFrame],
        inputs: &frame::Inputs,
        widths: &dyn terms::TextWidths,
    ) -> Result<Vec<frame::Label>, Error> {
        let mut out = Vec::new();
        for (panel, frame) in self.panels.iter().zip(frames) {
            let (ox, oy) = (frame.origin[0] as f64, frame.origin[1] as f64);
            if panel.title.is_some() || panel.formula.is_some() {
                let env = eval::frame_env(&panel.figure, inputs)?;
                let styles = &panel.figure.styles;
                let dpr = frame.prepared.dpr;
                let size = match &styles.title_size {
                    Some(s) => eval::real(s, &env)?,
                    None => TITLE_SIZE,
                } * dpr;
                let y = oy - size * 0.75;
                let mut x = ox;
                let parts = [
                    (&panel.title, "title", style::Font::Sans, &styles.title_fill),
                    (
                        &panel.formula,
                        "formula",
                        style::Font::Math,
                        &styles.formula_fill,
                    ),
                ];
                for (text, part, font, fill) in parts {
                    let Some(text) = text else { continue };
                    let wide = widths.width(text, size, font);
                    let at = [x + wide / 2.0, y];
                    out.push(frame::Label {
                        id: format!("{}/{part}", panel.id),
                        text: text.clone(),
                        x: at[0],
                        y: at[1],
                        size,
                        color: match fill {
                            Some(c) => eval::color(c, &env)?,
                            None => [0.0; 3],
                        },
                        halo: 0.0,
                        opacity: 1.0,
                        font,
                        anchor: at,
                        place: style::Place::Fixed,
                    });
                    x += wide + size * 0.6;
                }
            }
            for label in &frame.prepared.labels {
                let mut label = label.clone();
                if !panel.id.is_empty() {
                    label.id = format!("{}/{}", panel.id, label.id);
                }
                label.x += ox;
                label.y += oy;
                label.anchor = [label.anchor[0] + ox, label.anchor[1] + oy];
                out.push(label);
            }
        }
        Ok(out)
    }
}

const TITLE_SIZE: f64 = 14.0;
/// A panel's width and height until a sheet says otherwise, so a document draws on its own.
const PANEL_SIZE: f64 = 400.0;
