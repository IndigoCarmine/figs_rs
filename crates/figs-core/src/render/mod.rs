//! Rendering: a single [`Renderer`] trait fed the [`ComputedLayout`] IR, with
//! one implementation per output format. Driving every backend from the same
//! `paint()` walk guarantees PNG and (later) PDF stay pixel/point-identical.
//!
//! This milestone implements rectangles, container backgrounds and the page
//! background via [`png::TinySkiaRenderer`]. Text and image painting arrive in
//! later milestones as additional trait methods with no-op defaults, so adding
//! them won't break existing renderers.

pub mod png;

pub use png::{render_png, PngError, TinySkiaRenderer};

use crate::geom::{Color, Rect};
use crate::layout::{ComputedLayout, PageGeom, PaintContent, PaintText};
use crate::text::FontStore;

/// A drawing backend. Coordinates are points with a top-left origin; each
/// renderer maps them to its own space.
pub trait Renderer {
    /// Begin a page: size the canvas and paint the page background.
    fn begin_page(&mut self, page: &PageGeom);

    /// Fill and/or stroke a rectangle, optionally with rounded corners.
    fn fill_rect(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
        corner_radius: f32,
    );

    /// Draw shaped text within a node's border box `rect`. The default is a
    /// no-op so backends can opt in as they gain text support.
    fn draw_text(&mut self, rect: Rect, text: &PaintText, fonts: &FontStore) {
        let _ = (rect, text, fonts);
    }
}

/// Walk a computed layout in paint order, dispatching each node to the renderer.
/// Image content is skipped until the image backend lands.
pub fn paint<R: Renderer>(layout: &ComputedLayout, fonts: &FontStore, r: &mut R) {
    r.begin_page(&layout.page);
    for node in &layout.nodes {
        match &node.content {
            PaintContent::Container => {}
            PaintContent::Rect {
                fill,
                stroke,
                stroke_width,
                corner_radius,
            } => r.fill_rect(node.rect, *fill, *stroke, *stroke_width, *corner_radius),
            PaintContent::Text(text) => r.draw_text(node.rect, text, fonts),
            PaintContent::Image { .. } => {
                tracing::debug!(id = %node.id, "skipping image content (not yet implemented)");
            }
        }
    }
}
