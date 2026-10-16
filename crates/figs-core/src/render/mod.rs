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
//!
//! **Effects** (`shadow`, `glow`, `opacity`) apply to a node *and its
//! subtree* as one group ([`ComputedNode::effects`], with the subtree's
//! extent in [`ComputedNode::descendants`]). [`paint`] brackets the group by
//! [`Renderer::push_opacity`] / [`Renderer::pop_opacity`] and draws each
//! shadow first through [`Renderer::draw_shadow`]. Shadows are rasters in
//! every backend — the group's silhouette rendered by the tiny-skia
//! backend, grown, blurred and tinted ([`effects`]) — so PNG, PDF and SVG
//! show the same pixels: PNG at the output resolution, PDF/SVG as an
//! embedded image (PDF has no blur; an SVG filter would differ per viewer).
//!
//! [`ComputedNode::effects`]: crate::layout::ComputedNode::effects
//! [`ComputedNode::descendants`]: crate::layout::ComputedNode::descendants

pub mod effects;
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

use crate::geom::{
    Affine, Brush, Color, ImageFit, PathSegment, Rect, Shadow, StrokeAlign, StrokeStyle,
};
use crate::layout::{
    ClipRegion, ComputedLayout, ComputedNode, FramePaint, PageGeom, PaintContent, PathPaint,
    TextBox,
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
    /// honouring `fit`; `smooth = false` scales it nearest-neighbour (blocky
    /// pixels). Default: no-op.
    fn draw_image(&mut self, _rect: Rect, _src: &Path, _fit: ImageFit, _smooth: bool) {}

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

    /// Composite everything drawn until the matching
    /// [`Renderer::pop_opacity`] as one layer at `alpha` (`0..1`). `group`
    /// is what will be drawn (the node and its subtree, page space) — a
    /// bound for the layer. Pushed outside any node transform, inside the
    /// group's clips. Default: no-op (drawn opaque).
    fn push_opacity(&mut self, _alpha: f32, _group: &[ComputedNode]) {}

    /// End the layer started by [`Renderer::push_opacity`].
    fn pop_opacity(&mut self) {}

    /// Paint `shadow` of `group` (a node followed by its whole subtree, as in
    /// the IR): the silhouette of everything it draws, grown, blurred,
    /// tinted and offset (see [`effects`]). Called before the group is
    /// drawn, outside any node transform, inside its clips. Default: no-op.
    fn draw_shadow(&mut self, _group: &[ComputedNode], _shadow: &Shadow) {}
}

/// Walk a computed layout in paint order, dispatching each node to the renderer.
pub fn paint<R: Renderer>(layout: &ComputedLayout, r: &mut R) {
    r.begin_page(&layout.page);
    paint_nodes(r, &layout.nodes, &[]);
}

/// Paint a run of IR nodes (pre-order, each subtree complete) without
/// starting a page. `base` is the clip list already pushed by the caller: the
/// nodes' clip lists all start with it, and only the rest is pushed here.
pub(crate) fn paint_nodes<R: Renderer>(r: &mut R, nodes: &[ComputedNode], base: &[ClipRegion]) {
    let mut active: &[ClipRegion] = base;
    let mut i = 0;
    while i < nodes.len() {
        let node = &nodes[i];
        if node.clips[..] != *active {
            // Keep the shared outer clips; swap the rest.
            let keep = active
                .iter()
                .zip(&node.clips)
                .take_while(|(a, b)| a == b)
                .count()
                .max(base.len())
                .min(node.clips.len());
            for _ in keep..active.len() {
                r.pop_clip();
            }
            for c in &node.clips[keep..] {
                r.push_clip(c);
            }
            active = &node.clips;
        }
        if let Some(fx) = node.effects.as_deref().filter(|fx| !fx.is_none()) {
            let end = (i + 1 + node.descendants).min(nodes.len());
            let group = &nodes[i..end];
            i = end;
            if fx.opacity <= 0.0 {
                continue; // invisible, shadows included
            }
            let layer = fx.opacity < 1.0;
            if layer {
                r.push_opacity(fx.opacity, group);
            }
            for s in &fx.shadows {
                r.draw_shadow(group, s);
            }
            paint_node(r, &group[0]);
            paint_nodes(r, &group[1..], &group[0].clips);
            if layer {
                r.pop_opacity();
            }
            continue;
        }
        paint_node(r, node);
        i += 1;
    }
    for _ in base.len()..active.len() {
        r.pop_clip();
    }
}

/// Draw one node's own content (not its children) under its transform.
fn paint_node<R: Renderer>(r: &mut R, node: &ComputedNode) {
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
            stroke_align,
        } => draw_rect(
            r,
            node.rect,
            fill.as_ref(),
            stroke.filter(|_| *stroke_width > 0.0),
            *stroke_width,
            *corner_radius,
            *stroke_align,
        ),
        PaintContent::Text(tb) => r.draw_text(node.rect, tb),
        PaintContent::Image { src, fit, smooth } => r.draw_image(node.rect, src, *fit, *smooth),
        PaintContent::Path(path) => match path.aligned_stroke_outline() {
            // Inside/outside stroke on a closed outline: fill the outline,
            // then stroke its offset centre line (closed: no arrowheads).
            Some(line) => {
                if path.fill.is_some() {
                    r.draw_path(&PathPaint {
                        stroke: None,
                        ..path.clone()
                    });
                }
                r.draw_shape(&line, None, path.stroke.as_ref());
            }
            None => r.draw_path(path),
        },
        PaintContent::Ellipse {
            fill,
            stroke,
            stroke_align,
        } => match stroke {
            Some(s) if *stroke_align != StrokeAlign::Center => {
                if fill.is_some() {
                    r.draw_ellipse(node.rect, fill.as_ref(), None);
                }
                if let Some((line, _)) =
                    geometry::aligned_rect(node.rect, 0.0, *stroke_align, s.width)
                {
                    r.draw_ellipse(line, None, Some(s));
                }
            }
            _ => r.draw_ellipse(node.rect, fill.as_ref(), stroke.as_ref()),
        },
    }
    if xf.is_some() {
        r.pop_transform();
    }
}

/// `layout` with every shape stroke (rect, ellipse, line/arrow/path,
/// container border) at least [`PageGeom::min_stroke_px`] device pixels
/// wide at the page `dpi`; borrowed unchanged when that is off (`0`). PDF
/// viewers draw any stroke at least one device pixel wide; this does the
/// same for the raster backend, which otherwise anti-aliases a 0.25 pt line
/// at 100 dpi into a faint 0.35 px smear. It edits the IR before painting,
/// so a widened inside/outside-aligned border keeps its outer/inner edge on
/// the outline (the shared geometry offsets it by half the new width), and
/// shadows see the widened strokes too. Glyph outlines (text `stroke`,
/// synthetic bold) and filled shapes are left alone. Only the PNG / RGBA
/// backend calls it: PDF and SVG keep the exact widths (a PDF viewer applies
/// its own minimum; SVG has no portable one).
pub fn widen_hairlines(layout: &ComputedLayout) -> std::borrow::Cow<'_, ComputedLayout> {
    let px = layout.page.min_stroke_px;
    if !(px > 0.0 && px.is_finite() && layout.page.dpi > 0.0) {
        return std::borrow::Cow::Borrowed(layout);
    }
    let min = px * crate::units::PT_PER_INCH / layout.page.dpi;
    let widen = |w: &mut f32| {
        if *w > 0.0 && *w < min {
            *w = min;
        }
    };
    let mut out = layout.clone();
    for n in &mut out.nodes {
        match &mut n.content {
            PaintContent::Frame(f) => f.stroke.iter_mut().for_each(|s| widen(&mut s.width)),
            PaintContent::Rect {
                stroke: Some(_),
                stroke_width,
                ..
            } => widen(stroke_width),
            PaintContent::Path(p) => p.stroke.iter_mut().for_each(|s| widen(&mut s.width)),
            PaintContent::Ellipse { stroke, .. } => {
                stroke.iter_mut().for_each(|s| widen(&mut s.width))
            }
            _ => {}
        }
    }
    std::borrow::Cow::Owned(out)
}

/// The stroke of an open (chevron) arrowhead: the shaft's colour and width,
/// solid, mitered.
pub(crate) fn open_head_stroke(shaft: &StrokeStyle) -> StrokeStyle {
    StrokeStyle {
        dash: Vec::new(),
        cap: crate::geom::LineCap::Butt,
        join: crate::geom::LineJoin::Miter,
        ..shaft.clone()
    }
}

/// A `rect` node: a centred stroke is one [`Renderer::fill_rect`] call; an
/// inside/outside one fills the rect, then strokes the offset box from
/// [`geometry::aligned_rect`] (same corner treatment as the frame border).
#[allow(clippy::too_many_arguments)]
fn draw_rect<R: Renderer>(
    r: &mut R,
    rect: Rect,
    fill: Option<&Brush>,
    stroke: Option<Color>,
    width: f32,
    radius: f32,
    align: StrokeAlign,
) {
    if stroke.is_none() || align == StrokeAlign::Center {
        r.fill_rect(rect, fill, stroke, width, radius);
        return;
    }
    if fill.is_some() {
        r.fill_rect(rect, fill, None, 0.0, radius);
    }
    if let Some((line, line_radius)) = geometry::aligned_rect(rect, radius, align, width) {
        r.fill_rect(line, None, stroke, width, line_radius);
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
