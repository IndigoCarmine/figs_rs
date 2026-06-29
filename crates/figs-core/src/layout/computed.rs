//! [`ComputedLayout`] — the backend-agnostic intermediate representation that
//! the layout engine produces and every renderer (PNG, PDF, the egui preview)
//! consumes. Every node carries its stable `id` so a canvas hit-test can map
//! straight back to a TOML node (needed by the Phase 2 editor).

use crate::geom::{Color, ImageFit, Rect, TextAlign};

/// A laid-out page: geometry plus a flat list of nodes in paint order
/// (a parent always appears before its children).
#[derive(Debug, Clone)]
pub struct ComputedLayout {
    pub page: PageGeom,
    pub nodes: Vec<ComputedNode>,
}

#[derive(Debug, Clone)]
pub struct PageGeom {
    pub width_pt: f32,
    pub height_pt: f32,
    pub dpi: f32,
    pub background: Option<Color>,
}

/// A single placed node: stable id, absolute rect (points, top-left origin),
/// and what to paint there.
#[derive(Debug, Clone)]
pub struct ComputedNode {
    pub id: String,
    pub rect: Rect,
    pub content: PaintContent,
}

/// What a node paints. Containers paint nothing themselves.
///
/// Text currently carries its resolved properties; once the text backend lands
/// (M3) this variant will instead hold fully shaped glyph runs so PNG and PDF
/// draw identical geometry.
#[derive(Debug, Clone)]
pub enum PaintContent {
    Container,
    Rect {
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
        corner_radius: f32,
    },
    Image {
        src: std::path::PathBuf,
        fit: ImageFit,
    },
    Text(TextBox),
}

/// Resolved text ready to be shaped/painted within its node rect.
#[derive(Debug, Clone)]
pub struct TextBox {
    pub content: String,
    pub font_size: f32,
    pub font_family: Option<String>,
    pub font_weight: Option<u16>,
    pub color: Color,
    pub align: TextAlign,
    pub line_height: f32,
}
