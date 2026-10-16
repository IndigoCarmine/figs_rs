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
//! A text node's `stroke` (outline) is the same geometry everywhere: the glyph
//! outlines from [`outline::OutlineCache::text_outline`], stroked as one path
//! *before* the glyphs are filled (paint order stroke → fill).
//!
//! Text, image and shape painting are trait methods with no-op defaults, so a
//! new backend compiles before it opts in to everything.
//!
//! **Rotation** is a per-node absolute transform in the IR
//! ([`ComputedNode::transform`](crate::layout::ComputedNode::transform)):
//! [`paint`] wraps each transformed node in [`Renderer::push_transform`] /
//! [`Renderer::pop_transform`], and the drawing calls in between use the
//! node's local (unrotated) coordinates unchanged — so text, image and shape
//! code never needs to know about rotation.
//!
//! **Paints**: fills are [`Brush`]es — a colour or a linear / radial gradient
//! already placed in the node's local frame (see [`crate::paint`]). Backends
//! map them to native gradients (tiny-skia shaders, krilla shadings, SVG
//! `<linearGradient>` / `<radialGradient>` in `userSpaceOnUse`), drawn under
//! the node transform like the shape itself, so gradients turn with it.
//!
//! **Container decoration** ([`PaintContent::Frame`]) is drawn here from the
//! shared outlines of [`FramePaint`] through [`Renderer::draw_shape`].
//!
//! **Clipping** (`clip = true` containers): every node carries its clip
//! regions ([`ComputedNode::clips`](crate::layout::ComputedNode::clips));
//! [`paint`] brackets each run of nodes with the same list by
//! [`Renderer::push_clip`] / [`Renderer::pop_clip`] (outside any node
//! transform; clip outlines are page-space).

pub mod geometry;
pub mod outline;
#[cfg(feature = "pdf")]
pub mod pdf;
pub mod png;
pub mod svg;

#[cfg(feature = "pdf")]
pub use pdf::{render_pdf, PdfError, PdfRenderer};
pub use png::{image_size, render_png, render_rgba, PngError, RenderedImage, TinySkiaRenderer};
pub use svg::{render_svg, SvgRenderer};

use std::path::Path;

use crate::geom::{Affine, Brush, Color, ImageFit, PathSegment, Rect, StrokeStyle};
use crate::layout::{
    ClipRegion, ComputedLayout, FramePaint, PageGeom, PaintContent, PathPaint, TextBox,
};
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
        fill: Option<&Brush>,
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
    fn draw_ellipse(&mut self, _rect: Rect, _fill: Option<&Brush>, _stroke: Option<&StrokeStyle>) {}

    /// Fill and/or stroke a closed outline (local frame, shared geometry) —
    /// a container's background and border. Default: no-op.
    fn draw_shape(
        &mut self,
        _segs: &[PathSegment],
        _fill: Option<&Brush>,
        _stroke: Option<&StrokeStyle>,
    ) {
    }

    /// Clip everything drawn until the matching [`Renderer::pop_clip`] to
    /// `clip` (intersected with any clip already pushed). Pushed outside any
    /// node transform: use [`ClipRegion::page_outline`]. Default: no-op.
    fn push_clip(&mut self, _clip: &ClipRegion) {}

    /// End the clip started by [`Renderer::push_clip`].
    fn pop_clip(&mut self) {}

    /// Draw everything until the matching [`Renderer::pop_transform`] under
    /// the page-space transform `t` (points → points): the node's coordinates
    /// are its local frame, `t` maps them onto the page. [`paint`] brackets
    /// each node that has a [`ComputedNode::transform`] with one push/pop pair;
    /// the transform is absolute (already accumulated over rotated ancestors),
    /// so pushes never nest. Default: no-op (rotation ignored).
    ///
    /// [`ComputedNode::transform`]: crate::layout::ComputedNode::transform
    fn push_transform(&mut self, _t: &Affine) {}

    /// End the transform started by [`Renderer::push_transform`].
    fn pop_transform(&mut self) {}
}

/// Walk a computed layout in paint order, dispatching each node to the renderer.
pub fn paint<R: Renderer>(layout: &ComputedLayout, r: &mut R) {
    r.begin_page(&layout.page);
    let mut active: &[ClipRegion] = &[];
    for node in &layout.nodes {
        if node.clips[..] != *active {
            // Keep the shared outer clips; swap the rest.
            let keep = active
                .iter()
                .zip(&node.clips)
                .take_while(|(a, b)| a == b)
                .count();
            for _ in keep..active.len() {
                r.pop_clip();
            }
            for c in &node.clips[keep..] {
                r.push_clip(c);
            }
            active = &node.clips;
        }
        // Containers paint nothing, so they need no transform either.
        let xf = node
            .transform
            .as_ref()
            .filter(|_| !matches!(node.content, PaintContent::Container));
        if let Some(t) = xf {
            r.push_transform(t);
        }
        match &node.content {
            PaintContent::Container => {}
            PaintContent::Frame(frame) => draw_frame(r, node.rect, frame),
            PaintContent::Rect {
                fill,
                stroke,
                stroke_width,
                corner_radius,
            } => r.fill_rect(
                node.rect,
                fill.as_ref(),
                *stroke,
                *stroke_width,
                *corner_radius,
            ),
            PaintContent::Text(tb) => r.draw_text(node.rect, tb),
            PaintContent::Image { src, fit } => r.draw_image(node.rect, src, *fit),
            PaintContent::Path(path) => r.draw_path(path),
            PaintContent::Ellipse { fill, stroke } => {
                r.draw_ellipse(node.rect, fill.as_ref(), stroke.as_ref())
            }
        }
        if xf.is_some() {
            r.pop_transform();
        }
    }
    for _ in 0..active.len() {
        r.pop_clip();
    }
}

/// A container's background, then its border on top (both from the shared
/// [`FramePaint`] outlines).
fn draw_frame<R: Renderer>(r: &mut R, rect: Rect, frame: &FramePaint) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    if let Some(fill) = &frame.fill {
        r.draw_shape(&frame.fill_outline(rect), Some(fill), None);
    }
    if let Some(segs) = frame.stroke_outline(rect) {
        r.draw_shape(&segs, None, frame.stroke.as_ref());
    }
}
