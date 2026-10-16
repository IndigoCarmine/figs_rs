//! PDF backend built on [krilla](https://docs.rs/krilla).
//!
//! The page is exactly `page.width_pt × page.height_pt` points. krilla works in
//! a top-left-origin, y-down space and flips to PDF's bottom-left origin
//! itself, so IR coordinates are passed through unchanged.
//!
//! - **Shapes** are true vector paths (fills, strokes, dashes, caps, rounded
//!   corners, ellipses, arrowheads — geometry shared with the PNG backend via
//!   [`super::geometry`]).
//! - **Text** is real, selectable PDF text: the glyphs cosmic-text shaped for
//!   layout are emitted with their exact positions, and each font is embedded
//!   as a subset (krilla also writes a ToUnicode map, so copy/paste and search
//!   work). A text `stroke` is drawn first as a plain vector path along the
//!   glyph outlines (not as text), so the text layer holds each string once.
//! - **Images**: JPEGs are embedded as-is (DCT passthrough, no recompression);
//!   other formats are decoded and embedded losslessly. `cover` is clipped to
//!   the node rect. Unreadable images get the same placeholder box as PNG.
//! - **Rotation**: a rotated node is drawn inside a `q … cm … Q` block
//!   (krilla `push_transform` / `pop`), so rotated text stays real text.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use krilla::color::rgb;
use krilla::geom::{Path as KPath, PathBuilder, Point as KPoint, Size as KSize, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, LineCap as KLineCap, LineJoin, Stroke, StrokeDash};
use krilla::surface::Surface;
use krilla::text::{Font, Glyph, GlyphId};
use krilla::Document;

use crate::geom::{
    Affine, Color, ImageFit, LineCap, LineJoin as GLineJoin, PathSegment, Rect, StrokeStyle,
    TextAlign,
};
use crate::layout::{ComputedLayout, PageGeom, PathPaint, TextBox};
use crate::text::{DecorationKind, FontStore, ShapedText};

use super::geometry;
use super::outline::OutlineCache;
use super::{paint, RenderOptions, Renderer};

/// Errors from PDF rendering.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("page has an invalid size ({0} x {1} pt)")]
    BadPageSize(f32, f32),
    #[error("failed to write PDF: {0}")]
    Write(String),
}

/// Miter limit used by every backend (tiny-skia's and SVG's default).
const MITER_LIMIT: f32 = 4.0;

/// A krilla-backed renderer drawing onto one PDF page surface. Build via
/// [`render_pdf`].
pub struct PdfRenderer<'s, 'a> {
    surface: Surface<'s>,
    fonts: Option<&'a FontStore>,
    base_dir: &'a Path,
    /// Embedded fonts per cosmic-text face (`None` = unusable face).
    font_cache: HashMap<cosmic_text::fontdb::ID, Option<Font>>,
    /// Decoded images per resolved path (`None` = unreadable → placeholder).
    image_cache: HashMap<PathBuf, Option<(Image, f32, f32)>>,
    /// Glyph outlines, for text strokes.
    outlines: OutlineCache,
}

impl<'s, 'a> PdfRenderer<'s, 'a> {
    /// Wrap a page surface. The surface must belong to a page of the layout's
    /// page size.
    pub fn new(surface: Surface<'s>, opts: &RenderOptions<'a>) -> Self {
        PdfRenderer {
            surface,
            fonts: opts.fonts,
            base_dir: opts.base_dir,
            font_cache: HashMap::new(),
            image_cache: HashMap::new(),
            outlines: OutlineCache::new(),
        }
    }

    /// Finish drawing (flushes the surface into its page).
    pub fn finish(self) {
        self.surface.finish();
    }

    /// Fill and/or stroke a shared-geometry outline.
    fn draw_segs(
        &mut self,
        segs: &[PathSegment],
        fill: Option<Color>,
        stroke: Option<&StrokeStyle>,
    ) {
        if fill.is_none() && stroke.is_none() {
            return; // krilla would default to a black fill
        }
        let Some(path) = k_path(segs) else {
            return;
        };
        self.surface.set_fill(fill.map(k_fill));
        self.surface.set_stroke(stroke.map(k_stroke));
        self.surface.draw_path(&path);
    }

    /// Fill the `kind` decoration bars of a shaped text box.
    fn draw_decorations(
        &mut self,
        shaped: &ShapedText,
        rect: Rect,
        align: TextAlign,
        kind: DecorationKind,
    ) {
        for (r, color) in geometry::decoration_rects(shaped, rect, align, kind) {
            self.draw_segs(&geometry::rounded_rect(r, 0.0), Some(color), None);
        }
    }

    /// The embedded font for a cosmic-text face, loading it on first use.
    fn font(&mut self, fonts: &FontStore, id: cosmic_text::fontdb::ID) -> Option<Font> {
        self.font_cache
            .entry(id)
            .or_insert_with(|| {
                let (data, index) =
                    fonts.with_system(|sys| sys.db().with_face_data(id, |d, i| (d.to_vec(), i)))?;
                let font = Font::new(data.into(), index);
                if font.is_none() {
                    tracing::warn!(
                        ?id,
                        "font could not be embedded in PDF; skipping its glyphs"
                    );
                }
                font
            })
            .clone()
    }

    /// Decoded image + natural pixel size for `path`, cached.
    fn image(&mut self, path: &Path) -> Option<(Image, f32, f32)> {
        self.image_cache
            .entry(path.to_path_buf())
            .or_insert_with(|| load_image(path))
            .clone()
    }
}

/// Read and decode an image for embedding. JPEGs keep their original DCT
/// stream; everything else is embedded from decoded RGBA.
fn load_image(path: &Path) -> Option<(Image, f32, f32)> {
    let bytes = std::fs::read(path).ok()?;
    // Decode up front (as the PNG backend does) so a corrupt file becomes a
    // placeholder instead of a PDF write error.
    let decoded = image::load_from_memory(&bytes).ok()?;
    let (w, h) = (decoded.width(), decoded.height());
    if w == 0 || h == 0 {
        return None;
    }
    let is_jpeg = image::guess_format(&bytes).ok() == Some(image::ImageFormat::Jpeg);
    let img = if is_jpeg {
        Image::from_jpeg(bytes.into(), false).ok()
    } else {
        None
    }
    .unwrap_or_else(|| Image::from_rgba8(decoded.into_rgba8().into_raw(), w, h));
    Some((img, w as f32, h as f32))
}

fn k_color(c: Color) -> rgb::Color {
    let u = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    rgb::Color::new(u(c.r), u(c.g), u(c.b))
}

fn k_opacity(a: f32) -> NormalizedF32 {
    NormalizedF32::new(a.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE)
}

fn k_fill(c: Color) -> Fill {
    Fill {
        paint: k_color(c).into(),
        opacity: k_opacity(c.a),
        rule: FillRule::NonZero,
    }
}

fn k_stroke(s: &StrokeStyle) -> Stroke {
    Stroke {
        paint: k_color(s.color).into(),
        width: s.width,
        miter_limit: MITER_LIMIT,
        line_cap: match s.cap {
            LineCap::Butt => KLineCap::Butt,
            LineCap::Round => KLineCap::Round,
            LineCap::Square => KLineCap::Square,
        },
        line_join: match s.join {
            GLineJoin::Miter => LineJoin::Miter,
            GLineJoin::Round => LineJoin::Round,
            GLineJoin::Bevel => LineJoin::Bevel,
        },
        opacity: k_opacity(s.color.a),
        dash: (!s.dash.is_empty()).then(|| StrokeDash {
            array: s.dash.clone(),
            offset: 0.0,
        }),
    }
}

/// Solid stroke of `width` (rect borders).
fn plain_stroke(color: Color, width: f32) -> StrokeStyle {
    StrokeStyle {
        color,
        width,
        dash: Vec::new(),
        cap: LineCap::Butt,
        join: GLineJoin::Miter,
    }
}

/// Translate shared segments into a krilla path (same point space).
fn k_path(segs: &[PathSegment]) -> Option<KPath> {
    let mut pb = PathBuilder::new();
    for seg in segs {
        match *seg {
            PathSegment::MoveTo(p) => pb.move_to(p.x, p.y),
            PathSegment::LineTo(p) => pb.line_to(p.x, p.y),
            PathSegment::QuadTo(c, p) => pb.quad_to(c.x, c.y, p.x, p.y),
            PathSegment::CubicTo(c1, c2, p) => pb.cubic_to(c1.x, c1.y, c2.x, c2.y, p.x, p.y),
            PathSegment::Close => pb.close(),
        }
    }
    pb.finish()
}

/// A positioned glyph. Metrics are stored normalized to the font size (em
/// fractions), as krilla queries them at arbitrary sizes (including 1.0).
struct PdfGlyph {
    id: u16,
    range: Range<usize>,
    x_advance: f32,
    x_offset: f32,
    y_offset: f32,
}

impl Glyph for PdfGlyph {
    fn glyph_id(&self) -> GlyphId {
        GlyphId::new(self.id as u32)
    }
    fn text_range(&self) -> Range<usize> {
        self.range.clone()
    }
    fn x_advance(&self, size: f32) -> f32 {
        self.x_advance * size
    }
    fn x_offset(&self, size: f32) -> f32 {
        self.x_offset * size
    }
    fn y_offset(&self, size: f32) -> f32 {
        self.y_offset * size
    }
    fn y_advance(&self, _size: f32) -> f32 {
        0.0
    }
    fn location(&self) -> Option<krilla::surface::Location> {
        None
    }
}

/// `range` if it is a valid slice of `text`, else an empty range (krilla
/// slices the text by it).
fn checked_range(text: &str, range: &Range<usize>) -> Range<usize> {
    let ok = range.start <= range.end
        && range.end <= text.len()
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end);
    if ok {
        range.clone()
    } else {
        0..0
    }
}

impl Renderer for PdfRenderer<'_, '_> {
    fn push_transform(&mut self, t: &Affine) {
        // krilla's space is the IR's (top-left, y down), so the matrix goes in
        // as is; it composes with krilla's own page flip.
        self.surface
            .push_transform(&Transform::from_row(t.a, t.b, t.c, t.d, t.e, t.f));
    }

    fn pop_transform(&mut self) {
        self.surface.pop();
    }

    fn begin_page(&mut self, page: &PageGeom) {
        // Same default as PNG: white when no background is given.
        let bg = page.background.unwrap_or(Color::WHITE);
        let r = Rect::new(0.0, 0.0, page.width_pt, page.height_pt);
        self.draw_segs(&geometry::rounded_rect(r, 0.0), Some(bg), None);
    }

    fn fill_rect(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
        corner_radius: f32,
    ) {
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        let segs = geometry::rounded_rect(rect, corner_radius);
        let stroke = stroke
            .filter(|_| stroke_width > 0.0)
            .map(|c| plain_stroke(c, stroke_width));
        self.draw_segs(&segs, fill, stroke.as_ref());
    }

    fn draw_text(&mut self, rect: Rect, text: &TextBox) {
        let Some(fonts) = self.fonts else {
            return;
        };
        let shaped = fonts.shape(&geometry::text_props(text), rect.w);
        let content = text.content.as_str();

        // Outline first (paint order stroke → fill), as a vector path along
        // the glyph outlines rather than stroked text: the text layer keeps
        // the string exactly once (copy/paste, search), and the fill below
        // covers the inner half of the stroke.
        if let Some(stroke) = &text.stroke {
            let segs = self.outlines.text_outline(fonts, &shaped, rect, text.align);
            self.draw_segs(&segs, None, Some(stroke));
        }

        self.draw_decorations(&shaped, rect, text.align, DecorationKind::Underline);
        for line in &shaped.lines {
            let align_off = geometry::align_offset(text.align, line.width, rect.w);
            let baseline = rect.y + line.baseline;
            // One glyph run per consecutive (face, size, styled run): a
            // fallback font or a new span (colour, shift, slant) starts a new
            // run. Every run passes the whole `content` with its glyphs'
            // ranges, so the text layer holds the string exactly once.
            for run in line.glyphs.chunk_by(|a, b| {
                a.font_id == b.font_id
                    && a.font_size.to_bits() == b.font_size.to_bits()
                    && a.run == b.run
                    && a.synthetic_italic == b.synthetic_italic
            }) {
                let first = &run[0];
                let Some(font) = self.font(fonts, first.font_id) else {
                    continue;
                };
                // Advances are taken from the shaped pen positions so every
                // glyph lands exactly where layout (and PNG) put it.
                let size = first.font_size;
                if size <= 0.0 {
                    continue;
                }
                let glyphs: Vec<PdfGlyph> = run
                    .iter()
                    .enumerate()
                    .map(|(i, g)| PdfGlyph {
                        id: g.glyph_id,
                        range: checked_range(content, &g.text_range),
                        x_advance: run.get(i + 1).map_or(g.advance, |n| n.x - g.x) / size,
                        x_offset: g.x_offset / size,
                        y_offset: g.y_offset / size,
                    })
                    .collect();
                let origin_x = rect.x + align_off + first.x;
                // Synthetic italic: shear about the run's (shifted) baseline;
                // the glyphs stay real text.
                let slant = first.synthetic_italic.then(|| {
                    let t = geometry::italic_shear();
                    let pivot = baseline - first.y_offset;
                    Transform::from_row(1.0, 0.0, -t, 1.0, t * pivot, 0.0)
                });
                if let Some(xf) = &slant {
                    self.surface.push_transform(xf);
                }
                self.surface.set_stroke(None);
                self.surface.set_fill(Some(k_fill(first.color)));
                self.surface.draw_glyphs(
                    KPoint::from_xy(origin_x, baseline),
                    &glyphs,
                    font,
                    content,
                    size,
                    false,
                );
                if slant.is_some() {
                    self.surface.pop();
                }
            }
        }
        self.draw_decorations(&shaped, rect, text.align, DecorationKind::Strike);
    }

    fn draw_image(&mut self, rect: Rect, src: &Path, fit: ImageFit) {
        let resolved = geometry::resolve_image(self.base_dir, src);
        let Some((img, iw, ih)) = self.image(&resolved) else {
            tracing::warn!(path = %resolved.display(), "image unavailable; drawing placeholder");
            let (fill, border) = geometry::placeholder_colors();
            self.fill_rect(
                rect,
                Some(fill),
                Some(border),
                geometry::PLACEHOLDER_BORDER,
                0.0,
            );
            return;
        };
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        let dest = geometry::image_placement(rect, iw, ih, fit);
        let Some(size) = KSize::from_wh(dest.w, dest.h) else {
            return;
        };
        // Clip to the node rect so `cover` doesn't bleed into siblings.
        let clip = (fit == ImageFit::Cover)
            .then(|| k_path(&geometry::rounded_rect(rect, 0.0)))
            .flatten();
        if let Some(clip) = &clip {
            self.surface.push_clip_path(clip, &FillRule::NonZero);
        }
        self.surface
            .push_transform(&Transform::from_translate(dest.x, dest.y));
        self.surface.draw_image(img, size);
        self.surface.pop();
        if clip.is_some() {
            self.surface.pop();
        }
    }

    fn draw_path(&mut self, path: &PathPaint) {
        if path.segments.len() < 2 {
            return;
        }
        if let Some(fill) = path.fill {
            self.draw_segs(&path.segments, Some(fill), None);
        }
        let Some(stroke) = &path.stroke else {
            return;
        };
        // Shaft pulled back into the heads, then the heads filled in the
        // stroke color (geometry shared with every backend).
        let geo = path.stroke_geometry();
        self.draw_segs(&geo.shaft, None, Some(stroke));
        for tri in &geo.heads {
            self.draw_segs(&geometry::polygon(tri), Some(stroke.color), None);
        }
    }

    fn draw_ellipse(&mut self, rect: Rect, fill: Option<Color>, stroke: Option<&StrokeStyle>) {
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        self.draw_segs(&geometry::ellipse(rect), fill, stroke);
    }
}

/// Render a computed layout to a single-page PDF (page size = the layout's
/// page size in points).
pub fn render_pdf(layout: &ComputedLayout, opts: &RenderOptions) -> Result<Vec<u8>, PdfError> {
    let (w, h) = (layout.page.width_pt, layout.page.height_pt);
    let settings = PageSettings::from_wh(w, h).ok_or(PdfError::BadPageSize(w, h))?;
    let mut doc = Document::new();
    {
        let mut page = doc.start_page_with(settings);
        let mut r = PdfRenderer::new(page.surface(), opts);
        paint(layout, &mut r);
        r.finish();
        page.finish();
    }
    doc.finish().map_err(|e| PdfError::Write(format!("{e:?}")))
}
