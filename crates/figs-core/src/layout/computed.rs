//! [`ComputedLayout`] — the backend-agnostic intermediate representation that
//! the layout engine produces and every renderer (PNG, PDF, the egui preview)
//! consumes. Every node carries its stable `id` so a canvas hit-test can map
//! straight back to a TOML node (needed by the Phase 2 editor).

use crate::geom::{ArrowHead, Color, ImageFit, PathSegment, Rect, StrokeStyle, TextAlign};

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
    /// Final page height. For an auto-height page this is the content-derived
    /// height computed by layout, so renderers and the editor just use it.
    pub height_pt: f32,
    pub dpi: f32,
    pub background: Option<Color>,
    /// True when `height_pt` was derived from the content (`height = "auto"`).
    pub auto_height: bool,
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
    /// A polyline/polygon (`line`, `arrow`, `path`).
    Path(PathPaint),
    /// An ellipse inscribed in the node rect.
    Ellipse {
        fill: Option<Color>,
        stroke: Option<StrokeStyle>,
    },
}

/// A placed vector outline (`line`, `arrow`, `path`). Unlike other content it
/// may paint outside the node rect: segments are absolute page coordinates
/// (points, top-left origin, y down).
#[derive(Debug, Clone)]
pub struct PathPaint {
    /// One subpath: a leading `MoveTo`, then `LineTo` / `QuadTo` / `CubicTo`,
    /// then optionally a trailing `Close` (arcs are already cubics).
    pub segments: Vec<PathSegment>,
    pub fill: Option<Color>,
    pub stroke: Option<StrokeStyle>,
    /// Arrowheads (filled with the stroke color); ignored when closed.
    pub head: ArrowHead,
    /// Arrowhead length; its base is 0.8x as wide.
    pub head_size: f32,
}

impl PathPaint {
    /// Whether the outline is closed (ends with `Close`).
    pub fn is_closed(&self) -> bool {
        matches!(self.segments.last(), Some(PathSegment::Close))
    }

    /// The stroked shaft (pulled back into any arrowheads) and the head
    /// triangles to fill in the stroke color — shared by every backend.
    pub fn stroke_geometry(&self) -> crate::path::StrokeGeometry {
        crate::path::stroke_geometry(&self.segments, self.head, self.head_size)
    }
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
    /// Glyph outline painted *behind* the fill (paint order stroke → fill, as
    /// SVG `paint-order="stroke"`), so the visible glyph shapes are unchanged
    /// and only the outer half of the stroke shows. It may overhang `rect` by
    /// `width / 2`; layout ignores it.
    pub stroke: Option<StrokeStyle>,
}
