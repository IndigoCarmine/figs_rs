//! Rendering: a single [`Renderer`] trait fed the [`ComputedLayout`] IR, with
//! one implementation per output format. Driving every backend from the same
//! `paint()` walk guarantees PNG and (later) PDF stay pixel/point-identical.
//!
//! Rectangles, text, images and vector shapes are all painted through
//! [`png::TinySkiaRenderer`]. Text, image and shape painting are trait methods
//! with no-op defaults, so future backends (e.g. PDF) keep compiling until they
//! opt in.

pub mod png;

pub use png::{image_size, render_png, render_rgba, PngError, RenderedImage, TinySkiaRenderer};

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
