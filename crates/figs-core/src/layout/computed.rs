//! [`ComputedLayout`] — the backend-agnostic intermediate representation that
//! the layout engine produces and every renderer (PNG, PDF, the egui preview)
//! consumes. Every node carries its stable `id` so a canvas hit-test can map
//! straight back to a TOML node (needed by the Phase 2 editor).

use crate::geom::{Color, Edges, ImageFit, Rect, TextAlign};
use crate::text::ShapedText;

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
/// Text is shaped during layout (at the node's final content width), so PNG and
/// PDF receive identical glyph geometry.
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
    Text(PaintText),
}

/// Shaped text positioned relative to its node's content box (the node rect
/// inset by `padding`).
#[derive(Debug, Clone)]
pub struct PaintText {
    pub shaped: ShapedText,
    pub color: Color,
    pub align: TextAlign,
    /// Padding of the owning node, used to find the content origin within rect.
    pub padding: Edges,
}
