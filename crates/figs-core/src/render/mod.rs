//! Rendering: a single [`Renderer`] trait fed the [`ComputedLayout`] IR, with
//! one implementation per output format. Driving every backend from the same
//! `paint()` walk — and computing arrowheads, rounded corners, ellipses, text
//! alignment and image fitting once in [`geometry`] — keeps PNG, PDF and SVG
//! point-identical.
//!
//! - [`png::TinySkiaRenderer`] — raster (PNG / RGBA preview).
//! - [`pdf::PdfRenderer`] — vector PDF with embedded, subset fonts as real
//!   text (feature `pdf`, on by default).
//! - [`svg::SvgRenderer`] — vector SVG with text as glyph outlines.
//!
//! Text, image and shape painting are trait methods with no-op defaults, so a
//! new backend compiles before it opts in to everything.

pub mod geometry;
#[cfg(feature = "pdf")]
pub mod pdf;
pub mod png;
pub mod svg;

#[cfg(feature = "pdf")]
pub use pdf::{render_pdf, PdfError, PdfRenderer};
pub use png::{image_size, render_png, render_rgba, PngError, RenderedImage, TinySkiaRenderer};
pub use svg::{render_svg, SvgRenderer};

use std::path::Path;

use crate::geom::{Color, ImageFit, Rect, StrokeStyle};
use crate::layout::{ComputedLayout, PageGeom, PaintContent, PathPaint, TextBox};
use crate::text::FontStore;

/// Options shared by every render entry point.
///
/// `fonts` supplies the font database needed to rasterize text (without it,
/// text is skipped). `base_dir` resolves relative image `src` paths.
pub struct RenderOptions<'a> {
    pub fonts: Option<&'a FontStore>,
    pub base_dir: &'a Path,
}

impl Default for RenderOptions<'_> {
    fn default() -> Self {
        RenderOptions {
            fonts: None,
            base_dir: Path::new("."),
        }
    }
}

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

    /// Draw a shaped text block within `rect`. Default: no-op.
    fn draw_text(&mut self, _rect: Rect, _text: &TextBox) {}

    /// Draw an image (resolved against the render base dir) within `rect`,
    /// honouring `fit`. Default: no-op.
    fn draw_image(&mut self, _rect: Rect, _src: &Path, _fit: ImageFit) {}

    /// Fill/stroke a vector path (absolute line/Bézier segments) and its
    /// arrowheads: fill `path.segments`, stroke `path.stroke_geometry().shaft`
    /// and fill its `heads` in the stroke color. Default: no-op.
    fn draw_path(&mut self, _path: &PathPaint) {}

    /// Fill/stroke the ellipse inscribed in `rect`. Default: no-op.
    fn draw_ellipse(&mut self, _rect: Rect, _fill: Option<Color>, _stroke: Option<&StrokeStyle>) {}
}

/// Walk a computed layout in paint order, dispatching each node to the renderer.
pub fn paint<R: Renderer>(layout: &ComputedLayout, r: &mut R) {
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
            PaintContent::Text(tb) => r.draw_text(node.rect, tb),
            PaintContent::Image { src, fit } => r.draw_image(node.rect, src, *fit),
            PaintContent::Path(path) => r.draw_path(path),
            PaintContent::Ellipse { fill, stroke } => {
                r.draw_ellipse(node.rect, *fill, stroke.as_ref())
            }
        }
    }
}
