//! [`ComputedLayout`] — the backend-agnostic intermediate representation that
//! the layout engine produces and every renderer (PNG, PDF, the egui preview)
//! consumes. Every node carries its stable `id` so a canvas hit-test can map
//! straight back to a TOML node (needed by the Phase 2 editor).

use crate::geom::{
    Affine, ArrowHead, Color, ImageFit, PathSegment, Point, Rect, StrokeStyle, TextAlign,
};

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

impl ComputedLayout {
    /// The deepest (last-painted) node whose box contains the page point
    /// (`x`, `y`) in points — rotation-aware (see [`ComputedNode::contains`]).
    /// This is the canvas hit-test of the editor.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<&ComputedNode> {
        self.nodes.iter().rev().find(|n| n.contains(x, y))
    }
}

/// A single placed node: stable id, rect (points, top-left origin), optional
/// transform, and what to paint there.
#[derive(Debug, Clone)]
pub struct ComputedNode {
    pub id: String,
    /// The node's own border box, in its **local** frame: page coordinates
    /// when `transform` is `None`, else coordinates that `transform` maps onto
    /// the page. For a rotated node this is the *unrotated* box (for a quarter
    /// turn, the parent's slot with width and height swapped about its
    /// centre); [`ComputedNode::bounds`] gives the box actually covered.
    pub rect: Rect,
    /// Absolute local → page transform: the composition of this node's
    /// `rotate` and every rotated ancestor's. `None` = identity (the common
    /// case). It applies to everything the node paints — its rect, text,
    /// image, and path segments (which are in the same local frame) — so a
    /// backend draws the node exactly as unrotated, under this transform.
    /// Children carry their own (accumulated) transform, so each node is
    /// independent of paint order.
    pub transform: Option<Affine>,
    pub content: PaintContent,
}

impl ComputedNode {
    /// The four corners of the node's box on the page (top-left, top-right,
    /// bottom-right, bottom-left of the local rect, transformed).
    pub fn corners(&self) -> [Point; 4] {
        let r = self.rect;
        let pts = [
            Point::new(r.x, r.y),
            Point::new(r.x + r.w, r.y),
            Point::new(r.x + r.w, r.y + r.h),
            Point::new(r.x, r.y + r.h),
        ];
        match &self.transform {
            Some(t) => pts.map(|p| t.apply(p)),
            None => pts,
        }
    }

    /// Axis-aligned page-space bounding box of the (possibly rotated) box.
    /// For a quarter turn this is exactly the footprint the parent laid out.
    pub fn bounds(&self) -> Rect {
        if self.transform.is_none() {
            return self.rect;
        }
        let c = self.corners();
        let (mut x0, mut y0, mut x1, mut y1) = (c[0].x, c[0].y, c[0].x, c[0].y);
        for p in &c[1..] {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    /// Whether the page point (`x`, `y`) lies inside the node's box (edges
    /// included), honouring its rotation: the point is mapped back into the
    /// local frame and tested against `rect`.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let p = match &self.transform {
            None => Point::new(x, y),
            Some(t) => match t.invert() {
                Some(inv) => inv.apply(Point::new(x, y)),
                None => return false,
            },
        };
        // A little slack so a point on a rotated edge still hits.
        const EPS: f32 = 1e-3;
        let r = self.rect;
        p.x >= r.x - EPS && p.x <= r.x + r.w + EPS && p.y >= r.y - EPS && p.y <= r.y + r.h + EPS
    }
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
}
