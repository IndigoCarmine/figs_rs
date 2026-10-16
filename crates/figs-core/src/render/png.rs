//! PNG / RGBA backend built on tiny-skia. Points are scaled to pixels by
//! `dpi / 72`; the page background, rectangles, vector shapes, text glyphs and
//! images are rasterized with anti-aliasing and the pixmap is encoded to PNG or read back
//! as straight RGBA for on-screen preview.
//!
//! Gradient fills are tiny-skia `LinearGradient` / `RadialGradient` shaders
//! in pixel space (the brush's local coordinates × dpi/72), so the fill's
//! node transform turns them with the shape. Clipping (`clip = true`) is done
//! by snapshot: [`Renderer::push_clip`] saves the canvas and rasterizes the
//! clip outline into an anti-aliased mask; [`Renderer::pop_clip`] restores
//! every pixel outside it (blending by coverage at the edge). That clips all
//! drawing — glyph blits included — without threading a mask through every
//! draw call, at the cost of one canvas copy per clipped container.
//!
//! Effects: a group `opacity` draws the group onto a fresh transparent layer
//! and composites it back at that alpha (only over the group's bounds);
//! shadows are rasterized at the output resolution by [`super::effects`] and
//! blended under the node at whole-pixel positions. `smooth = false` images
//! are sampled nearest-neighbour.

use std::collections::HashMap;
use std::path::Path;

use cosmic_text::{CacheKey, CacheKeyFlags, SwashCache, SwashContent, SwashImage};
use tiny_skia::{
    ColorU8, FillRule, FilterQuality, GradientStop as SkStop, IntSize, LineCap as SkLineCap,
    LineJoin as SkLineJoin, LinearGradient as SkLinear, Mask, Paint, PathBuilder, Pixmap,
    PixmapPaint, Point as SkPoint, PremultipliedColorU8, RadialGradient as SkRadial,
    Rect as SkRect, Shader, SpreadMode, Stroke, StrokeDash, Transform,
};

use crate::geom::{
    Affine, Brush, Color, ImageFit, LineCap, LineJoin, PathSegment, Point, Rect, Shadow,
    StrokeStyle, TextAlign,
};
use crate::layout::{ClipRegion, ComputedLayout, ComputedNode, PageGeom, PathPaint, TextBox};
use crate::text::{DecorationKind, FontStore, ShapedGlyph, ShapedText};

use super::geometry;
use super::outline::OutlineCache;
use super::{paint, RenderOptions, Renderer};

/// Errors from raster rendering.
#[derive(Debug, thiserror::Error)]
pub enum PngError {
    #[error("page is too large or has zero size ({0}x{1} px)")]
    BadCanvasSize(u32, u32),
    #[error("failed to encode PNG: {0}")]
    Encode(String),
}

/// Straight (un-premultiplied) RGBA image, ready to upload to a GPU texture
/// (e.g. the egui preview).
#[derive(Debug, Clone)]
pub struct RenderedImage {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA8, 4 bytes per pixel, alpha not premultiplied.
    pub rgba: Vec<u8>,
}

/// A tiny-skia raster renderer. Build via [`render_png`] / [`render_rgba`] or
/// drive manually with [`paint`].
pub struct TinySkiaRenderer<'a> {
    pixmap: Pixmap,
    /// Points -> pixels scale (dpi / 72).
    scale: f32,
    /// Font database for text rasterization. Text is skipped when `None`.
    fonts: Option<&'a FontStore>,
    /// Glyph bitmap cache.
    swash: SwashCache,
    /// Glyph outlines, for text strokes.
    outlines: OutlineCache,
    /// Resolves relative image `src` paths.
    base_dir: &'a Path,
    /// Page-space transform of the node being drawn (see
    /// [`Renderer::push_transform`]); `None` = identity.
    xf: Option<Affine>,
    /// Glyph outlines in font units (y up) plus units-per-em, per face and
    /// glyph, for text drawn under a rotation (`None` = no outline).
    glyph_paths: HashMap<(FaceId, u16), Option<(tiny_skia::Path, f32)>>,
    /// Open clips: the canvas as it was at each `push_clip` and the clip's
    /// coverage mask (`None` = degenerate clip, hides everything).
    clips: Vec<(Pixmap, Option<Mask>)>,
    /// Open opacity layers: the canvas under the layer, the layer's alpha,
    /// and the pixel box it may have drawn in (`x0, y0, x1, y1`).
    layers: Vec<(Pixmap, f32, [u32; 4])>,
}

type FaceId = cosmic_text::fontdb::ID;

impl<'a> TinySkiaRenderer<'a> {
    /// Allocate a canvas for a page. Returns an error for a degenerate size.
    pub fn new(page: &PageGeom, opts: &RenderOptions<'a>) -> Result<Self, PngError> {
        let scale = page.dpi / crate::units::PT_PER_INCH;
        let w = (page.width_pt * scale).round() as u32;
        let h = (page.height_pt * scale).round() as u32;
        let pixmap = Pixmap::new(w, h).ok_or(PngError::BadCanvasSize(w, h))?;
        Ok(TinySkiaRenderer {
            pixmap,
            scale,
            fonts: opts.fonts,
            swash: SwashCache::new(),
            outlines: OutlineCache::new(),
            base_dir: opts.base_dir,
            xf: None,
            glyph_paths: HashMap::new(),
            clips: Vec::new(),
            layers: Vec::new(),
        })
    }

    /// A transparent `w` x `h` canvas at `scale` pixels per point (no page,
    /// no background) — for rendering a group's silhouette.
    pub(crate) fn offscreen(w: u32, h: u32, scale: f32, opts: &RenderOptions<'a>) -> Option<Self> {
        Some(TinySkiaRenderer {
            pixmap: Pixmap::new(w, h)?,
            scale,
            fonts: opts.fonts,
            swash: SwashCache::new(),
            outlines: OutlineCache::new(),
            base_dir: opts.base_dir,
            xf: None,
            glyph_paths: HashMap::new(),
            clips: Vec::new(),
            layers: Vec::new(),
        })
    }

    /// The canvas (premultiplied).
    pub(crate) fn into_pixmap(self) -> Pixmap {
        self.pixmap
    }

    /// The canvas extent in points.
    fn canvas_rect(&self) -> Rect {
        Rect::new(
            0.0,
            0.0,
            self.pixmap.width() as f32 / self.scale,
            self.pixmap.height() as f32 / self.scale,
        )
    }

    /// The pixel box `[x0, y0, x1, y1]` (clamped to the canvas) covering
    /// everything `group` may draw.
    fn pixel_bounds(&self, group: &[ComputedNode]) -> [u32; 4] {
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let Some(ink) = super::effects::ink_bounds(group) else {
            return [0, 0, 0, 0];
        };
        let s = self.scale;
        let clamp = |v: f32, hi: u32| (v.max(0.0) as u32).min(hi);
        [
            clamp((ink.x * s).floor() - 1.0, w),
            clamp((ink.y * s).floor() - 1.0, h),
            clamp(((ink.x + ink.w) * s).ceil() + 1.0, w),
            clamp(((ink.y + ink.h) * s).ceil() + 1.0, h),
        ]
    }

    /// Encode the current canvas as PNG bytes.
    pub fn into_png(self) -> Result<Vec<u8>, PngError> {
        self.pixmap
            .encode_png()
            .map_err(|e| PngError::Encode(e.to_string()))
    }

    /// Read the canvas back as straight (un-premultiplied) RGBA.
    pub fn into_rgba(self) -> RenderedImage {
        let (width, height) = (self.pixmap.width(), self.pixmap.height());
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for px in self.pixmap.pixels() {
            let c = px.demultiply();
            rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        RenderedImage {
            width,
            height,
            rgba,
        }
    }

    fn px(&self, v: f32) -> f32 {
        v * self.scale
    }

    /// The current node transform in pixel space: paths are built in pixels
    /// (points x scale), so the page-space rotation keeps its linear part and
    /// scales its translation.
    fn sk_xf(&self) -> Transform {
        match &self.xf {
            Some(t) => Transform::from_row(t.a, t.b, t.c, t.d, t.e * self.scale, t.f * self.scale),
            None => Transform::identity(),
        }
    }

    /// The current node transform in page space (points), as tiny-skia.
    fn page_xf(&self) -> Transform {
        match &self.xf {
            Some(t) => Transform::from_row(t.a, t.b, t.c, t.d, t.e, t.f),
            None => Transform::identity(),
        }
    }

    /// Outline of glyph `gid` of `face` in font units (y up) and the face's
    /// units per em, cached.
    fn glyph_outline(
        &mut self,
        fonts: &FontStore,
        face: FaceId,
        gid: u16,
    ) -> Option<(tiny_skia::Path, f32)> {
        self.glyph_paths
            .entry((face, gid))
            .or_insert_with(|| {
                fonts.with_system(|sys| {
                    sys.db()
                        .with_face_data(face, |data, index| {
                            let parsed = ttf_parser::Face::parse(data, index).ok()?;
                            let mut b = OutlineToPath(PathBuilder::new());
                            parsed.outline_glyph(ttf_parser::GlyphId(gid), &mut b)?;
                            let upem = parsed.units_per_em() as f32;
                            if upem <= 0.0 {
                                return None;
                            }
                            Some((b.0.finish()?, upem))
                        })
                        .flatten()
                })
            })
            .clone()
    }

    /// Text under a rotation: glyph bitmaps can only be blitted axis-aligned,
    /// so each glyph is filled from its outline through the full transform
    /// (font units -> pen position -> page transform -> pixels). Glyphs
    /// without an outline (bitmap-only, e.g. colour emoji) are rasterized
    /// upright and drawn as a transformed image.
    fn draw_text_transformed(&mut self, fonts: &FontStore, rect: Rect, text: &TextBox) {
        let shaped = fonts.shape(&geometry::text_props(text), rect.w);
        // Outline first, as in `draw_text`; `stroke_shape` applies the node
        // transform itself.
        if let Some(stroke) = &text.stroke {
            let segs = self.outlines.text_outline(fonts, &shaped, rect, text.align);
            if let Some(p) = self.sk_path(&segs) {
                self.stroke_shape(&p, stroke);
            }
        }
        self.draw_decorations(&shaped, rect, text.align, DecorationKind::Underline);
        let device = Transform::from_scale(self.scale, self.scale).pre_concat(self.page_xf());
        let mut paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        let shear = geometry::italic_shear();

        for line in &shaped.lines {
            let align_off = geometry::align_offset(text.align, line.width, rect.w);
            for g in &line.glyphs {
                // Pen position in the node's local frame (points).
                let pen_x = rect.x + align_off + g.x + g.x_offset;
                let pen_y = rect.y + line.baseline - g.y_offset;
                if let Some((path, upem)) = self.glyph_outline(fonts, g.font_id, g.glyph_id) {
                    let k = g.font_size / upem;
                    let kx = if g.synthetic_italic { k * shear } else { 0.0 };
                    paint.set_color(sk_color(g.color));
                    let t = device.pre_concat(Transform::from_row(k, 0.0, kx, -k, pen_x, pen_y));
                    self.pixmap
                        .fill_path(&path, &paint, FillRule::Winding, t, None);
                    continue;
                }
                let (key, _, _) = CacheKey::new(
                    g.font_id,
                    g.glyph_id,
                    g.font_size * self.scale,
                    (0.0, 0.0),
                    glyph_flags(g),
                );
                let image = fonts.with_system(|sys| self.swash.get_image(sys, key).clone());
                let Some(glyph) = image.as_ref().and_then(|img| glyph_pixmap(img, g.color)) else {
                    continue;
                };
                let img = image.as_ref().expect("checked above");
                // Bitmap pixels -> upright device pixels around the pen ->
                // local points -> page -> device.
                let inv = 1.0 / self.scale;
                let t = device
                    .pre_concat(Transform::from_translate(pen_x, pen_y))
                    .pre_concat(Transform::from_scale(inv, inv))
                    .pre_concat(Transform::from_translate(
                        img.placement.left as f32,
                        -img.placement.top as f32,
                    ));
                let pp = PixmapPaint {
                    quality: FilterQuality::Bilinear,
                    ..Default::default()
                };
                self.pixmap.draw_pixmap(0, 0, glyph.as_ref(), &pp, t, None);
            }
        }
        self.draw_decorations(&shaped, rect, text.align, DecorationKind::Strike);
    }

    /// Fill the decoration bars of one kind (underlines go under the glyphs,
    /// line-throughs over them), under the node transform.
    fn draw_decorations(
        &mut self,
        shaped: &ShapedText,
        rect: Rect,
        align: TextAlign,
        kind: DecorationKind,
    ) {
        for (r, color) in geometry::decoration_rects(shaped, rect, align, kind) {
            if let Some(p) = self.sk_path(&geometry::rounded_rect(r, 0.0)) {
                self.fill_shape(&p, color);
            }
        }
    }

    fn sk_rect(&self, r: Rect) -> Option<SkRect> {
        SkRect::from_xywh(self.px(r.x), self.px(r.y), self.px(r.w), self.px(r.h))
    }

    /// The tiny-skia shader for `brush`, in pixel space; a degenerate
    /// gradient (zero-length line, empty box) falls back to a flat colour.
    fn shader(&self, brush: &Brush) -> Shader<'static> {
        let s = self.scale;
        let stops = || -> Vec<SkStop> {
            brush
                .stops()
                .iter()
                .map(|st| SkStop::new(st.offset, sk_color(st.color)))
                .collect()
        };
        let shader = match brush {
            Brush::Solid(c) => return Shader::SolidColor(sk_color(*c)),
            Brush::Linear { start, end, .. } => SkLinear::new(
                SkPoint::from_xy(start.x * s, start.y * s),
                SkPoint::from_xy(end.x * s, end.y * s),
                stops(),
                SpreadMode::Pad,
                Transform::identity(),
            ),
            Brush::Radial {
                center,
                focal,
                radius,
                transform: t,
                ..
            } => SkRadial::new(
                SkPoint::from_xy(focal.x, focal.y),
                SkPoint::from_xy(center.x, center.y),
                *radius,
                stops(),
                SpreadMode::Pad,
                // Gradient space -> local points -> pixels.
                Transform::from_row(t.a * s, t.b * s, t.c * s, t.d * s, t.e * s, t.f * s),
            ),
        };
        shader.unwrap_or_else(|| Shader::SolidColor(sk_color(brush.fallback_color())))
    }

    /// Anti-aliased fill of `path` with `brush` (colour or gradient).
    fn fill_brush(&mut self, path: &tiny_skia::Path, brush: &Brush) {
        let paint = Paint {
            shader: self.shader(brush),
            anti_alias: true,
            ..Paint::default()
        };
        self.pixmap
            .fill_path(path, &paint, FillRule::Winding, self.sk_xf(), None);
    }

    /// Anti-aliased solid fill of `path`.
    fn fill_shape(&mut self, path: &tiny_skia::Path, color: Color) {
        let mut paint = Paint::default();
        paint.set_color(sk_color(color));
        paint.anti_alias = true;
        self.pixmap
            .fill_path(path, &paint, FillRule::Winding, self.sk_xf(), None);
    }

    /// Anti-aliased stroke of `path` with width/dash/cap from `style` (points).
    fn stroke_shape(&mut self, path: &tiny_skia::Path, style: &StrokeStyle) {
        let mut paint = Paint::default();
        paint.set_color(sk_color(style.color));
        paint.anti_alias = true;
        let dash = if style.dash.is_empty() {
            None
        } else {
            StrokeDash::new(style.dash.iter().map(|d| self.px(*d)).collect(), 0.0)
        };
        let sk_stroke = Stroke {
            width: self.px(style.width),
            line_cap: match style.cap {
                LineCap::Butt => SkLineCap::Butt,
                LineCap::Round => SkLineCap::Round,
                LineCap::Square => SkLineCap::Square,
            },
            line_join: match style.join {
                LineJoin::Miter => SkLineJoin::Miter,
                LineJoin::Round => SkLineJoin::Round,
                LineJoin::Bevel => SkLineJoin::Bevel,
            },
            dash,
            ..Stroke::default()
        };
        self.pixmap
            .stroke_path(path, &paint, &sk_stroke, self.sk_xf(), None);
    }

    /// Build a pixel-space path from IR segments (points).
    fn sk_path(&self, segments: &[PathSegment]) -> Option<tiny_skia::Path> {
        let mut pb = PathBuilder::new();
        for s in segments {
            match *s {
                PathSegment::MoveTo(p) => pb.move_to(self.px(p.x), self.px(p.y)),
                PathSegment::LineTo(p) => pb.line_to(self.px(p.x), self.px(p.y)),
                PathSegment::QuadTo(c, p) => {
                    pb.quad_to(self.px(c.x), self.px(c.y), self.px(p.x), self.px(p.y))
                }
                PathSegment::CubicTo(c1, c2, p) => pb.cubic_to(
                    self.px(c1.x),
                    self.px(c1.y),
                    self.px(c2.x),
                    self.px(c2.y),
                    self.px(p.x),
                    self.px(p.y),
                ),
                PathSegment::Close => pb.close(),
            }
        }
        pb.finish()
    }

    /// Build a closed pixel-space polygon through `pts` (points).
    fn polygon(&self, pts: &[Point]) -> Option<tiny_skia::Path> {
        let (first, rest) = pts.split_first()?;
        let mut pb = PathBuilder::new();
        pb.move_to(self.px(first.x), self.px(first.y));
        for p in rest {
            pb.line_to(self.px(p.x), self.px(p.y));
        }
        pb.close();
        pb.finish()
    }

    /// Decode `path` and draw it scaled into `rect`. Returns false on any
    /// failure so the caller can paint a placeholder.
    fn try_draw_image(&mut self, rect: Rect, path: &Path, fit: ImageFit, smooth: bool) -> bool {
        let decoded = match image::open(path) {
            Ok(img) => img.to_rgba8(),
            Err(_) => return false,
        };
        let (iw, ih) = (decoded.width(), decoded.height());
        if iw == 0 || ih == 0 {
            return false;
        }

        // Premultiply into the layout tiny-skia expects.
        let raw = decoded.into_raw();
        let mut premul = vec![0u8; raw.len()];
        for (i, px) in raw.chunks_exact(4).enumerate() {
            let c = ColorU8::from_rgba(px[0], px[1], px[2], px[3]).premultiply();
            let o = i * 4;
            premul[o] = c.red();
            premul[o + 1] = c.green();
            premul[o + 2] = c.blue();
            premul[o + 3] = c.alpha();
        }
        let Some(size) = IntSize::from_wh(iw, ih) else {
            return false;
        };
        let Some(src) = Pixmap::from_vec(premul, size) else {
            return false;
        };

        let (rx, ry, rw, rh) = (
            self.px(rect.x),
            self.px(rect.y),
            self.px(rect.w),
            self.px(rect.h),
        );
        if rw <= 0.0 || rh <= 0.0 {
            return true; // nothing to draw, but not an error
        }
        let (iwf, ihf) = (iw as f32, ih as f32);
        let dest = geometry::image_placement(rect, iwf, ihf, fit);
        let (sx, sy) = (self.px(dest.w) / iwf, self.px(dest.h) / ihf);
        let (tx, ty) = (self.px(dest.x), self.px(dest.y));

        // Clip to the node rect so `Cover` doesn't bleed into siblings.
        let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height());
        if let (Some(mask), Some(skr)) = (mask.as_mut(), SkRect::from_xywh(rx, ry, rw, rh)) {
            let path = PathBuilder::from_rect(skr);
            mask.fill_path(&path, FillRule::Winding, true, self.sk_xf());
        }

        let pp = PixmapPaint {
            quality: if smooth {
                FilterQuality::Bilinear
            } else {
                FilterQuality::Nearest
            },
            ..Default::default()
        };
        self.pixmap.draw_pixmap(
            0,
            0,
            src.as_ref(),
            &pp,
            self.sk_xf()
                .pre_concat(Transform::from_row(sx, 0.0, 0.0, sy, tx, ty)),
            mask.as_ref(),
        );
        true
    }
}

/// Raster flags of a shaped glyph (synthetic italic).
fn glyph_flags(g: &ShapedGlyph) -> CacheKeyFlags {
    if g.synthetic_italic {
        CacheKeyFlags::FAKE_ITALIC
    } else {
        CacheKeyFlags::empty()
    }
}

fn sk_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        c.r.clamp(0.0, 1.0),
        c.g.clamp(0.0, 1.0),
        c.b.clamp(0.0, 1.0),
        c.a.clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::BLACK)
}

/// `old` + (`new` − `old`) × `cov`/255 per premultiplied channel (clip edges).
fn lerp_px(old: PremultipliedColorU8, new: PremultipliedColorU8, cov: u8) -> PremultipliedColorU8 {
    let k = cov as u32;
    let mix = |n: u8, o: u8| ((n as u32 * k + o as u32 * (255 - k) + 127) / 255) as u8;
    let a = mix(new.alpha(), old.alpha());
    // Keep the premultiplied invariant (channel <= alpha) despite rounding.
    let c = |n: u8, o: u8| mix(n, o).min(a);
    PremultipliedColorU8::from_rgba(
        c(new.red(), old.red()),
        c(new.green(), old.green()),
        c(new.blue(), old.blue()),
        a,
    )
    .unwrap_or(new)
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Source-over composite a straight (un-premultiplied) color with coverage
/// `a` onto a single premultiplied pixmap pixel.
fn blend_over(pixmap: &mut Pixmap, x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
    if a == 0 {
        return;
    }
    let w = pixmap.width();
    let idx = (y as u32 * w + x as u32) as usize;
    let pixels = pixmap.pixels_mut();
    let dst = pixels[idx];
    let a = a as u32;
    let inv = 255 - a;
    let up = |c: u32| (c + 127) / 255;
    // src premultiplied + dst scaled by inverse source alpha.
    let out_a = (a + up(dst.alpha() as u32 * inv)).min(255);
    let mix = |sc: u32, dc: u32| -> u8 {
        let v = up(sc * a) + up(dc * inv);
        v.min(out_a) as u8 // keep premultiplied invariant (channel <= alpha)
    };
    let out = PremultipliedColorU8::from_rgba(
        mix(r as u32, dst.red() as u32),
        mix(g as u32, dst.green() as u32),
        mix(b as u32, dst.blue() as u32),
        out_a as u8,
    )
    .unwrap_or(dst);
    pixels[idx] = out;
}

/// Blit a rasterized glyph bitmap onto the pixmap at integer origin (`ox`,`oy`).
fn blit_glyph(pixmap: &mut Pixmap, img: &SwashImage, ox: i32, oy: i32, color: Color) {
    let (gx0, gy0) = (ox + img.placement.left, oy - img.placement.top);
    let (w, h) = (img.placement.width as i32, img.placement.height as i32);
    let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
    let (cr, cg, cb) = (to_u8(color.r), to_u8(color.g), to_u8(color.b));
    let ca = color.a.clamp(0.0, 1.0);

    match img.content {
        SwashContent::Mask => {
            for row in 0..h {
                for col in 0..w {
                    let cov = img.data[(row * w + col) as usize];
                    let (px, py) = (gx0 + col, gy0 + row);
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    blend_over(pixmap, px, py, cr, cg, cb, to_u8(cov as f32 / 255.0 * ca));
                }
            }
        }
        SwashContent::Color => {
            for row in 0..h {
                for col in 0..w {
                    let i = ((row * w + col) * 4) as usize;
                    let (r, g, b, a) = (
                        img.data[i],
                        img.data[i + 1],
                        img.data[i + 2],
                        img.data[i + 3],
                    );
                    let (px, py) = (gx0 + col, gy0 + row);
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    blend_over(pixmap, px, py, r, g, b, to_u8(a as f32 / 255.0 * ca));
                }
            }
        }
        // Subpixel masks aren't produced by the default (grayscale) path.
        SwashContent::SubpixelMask => {}
    }
}

/// A glyph bitmap as a premultiplied pixmap (a coverage mask tinted with
/// `color`, or the colour bitmap with `color`'s alpha applied).
fn glyph_pixmap(img: &SwashImage, color: Color) -> Option<Pixmap> {
    let (w, h) = (img.placement.width, img.placement.height);
    let mut pm = Pixmap::new(w, h)?;
    let ca = color.a.clamp(0.0, 1.0);
    let (cr, cg, cb) = (to_u8(color.r), to_u8(color.g), to_u8(color.b));
    for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
        let straight = match img.content {
            SwashContent::Mask => {
                let cov = *img.data.get(i)? as f32 / 255.0;
                ColorU8::from_rgba(cr, cg, cb, to_u8(cov * ca))
            }
            SwashContent::Color => {
                let d = img.data.get(i * 4..i * 4 + 4)?;
                ColorU8::from_rgba(d[0], d[1], d[2], to_u8(d[3] as f32 / 255.0 * ca))
            }
            SwashContent::SubpixelMask => return None,
        };
        *px = straight.premultiply();
    }
    Some(pm)
}

/// Collects a glyph outline into a tiny-skia path, in font units (y up).
struct OutlineToPath(PathBuilder);

impl ttf_parser::OutlineBuilder for OutlineToPath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

impl Renderer for TinySkiaRenderer<'_> {
    fn push_transform(&mut self, t: &Affine) {
        self.xf = Some(*t);
    }

    fn push_opacity(&mut self, alpha: f32, group: &[ComputedNode]) {
        let bounds = self.pixel_bounds(group);
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let Some(layer) = Pixmap::new(w, h) else {
            return;
        };
        let under = std::mem::replace(&mut self.pixmap, layer);
        self.layers.push((under, alpha, bounds));
    }

    fn pop_opacity(&mut self) {
        let Some((under, alpha, [x0, y0, x1, y1])) = self.layers.pop() else {
            return;
        };
        let layer = std::mem::replace(&mut self.pixmap, under);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        // Composite only the box the group can have drawn in.
        let Some(part) = tiny_skia::IntRect::from_xywh(x0 as i32, y0 as i32, x1 - x0, y1 - y0)
            .and_then(|r| layer.clone_rect(r))
        else {
            return;
        };
        let pp = PixmapPaint {
            opacity: alpha,
            ..Default::default()
        };
        self.pixmap.draw_pixmap(
            x0 as i32,
            y0 as i32,
            part.as_ref(),
            &pp,
            Transform::identity(),
            None,
        );
    }

    fn draw_shadow(&mut self, group: &[ComputedNode], shadow: &Shadow) {
        let opts = RenderOptions {
            fonts: self.fonts,
            base_dir: self.base_dir,
        };
        let visible = self.canvas_rect();
        let Some(raster) = super::effects::shadow_raster(group, shadow, visible, self.scale, &opts)
        else {
            return;
        };
        if (raster.scale - self.scale).abs() <= 1e-6 * self.scale {
            // Same grid: a whole-pixel blit.
            self.pixmap.draw_pixmap(
                raster.x0,
                raster.y0,
                raster.pixmap.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
            return;
        }
        // Computed coarser (a huge shadow): scale it up smoothly.
        let k = self.scale / raster.scale;
        let pp = PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..Default::default()
        };
        self.pixmap.draw_pixmap(
            0,
            0,
            raster.pixmap.as_ref(),
            &pp,
            Transform::from_row(k, 0.0, 0.0, k, raster.x0 as f32 * k, raster.y0 as f32 * k),
            None,
        );
    }

    fn pop_transform(&mut self) {
        self.xf = None;
    }

    fn begin_page(&mut self, page: &PageGeom) {
        // Default to white when no background is given (print-friendly).
        match &page.background {
            None => self.pixmap.fill(tiny_skia::Color::WHITE),
            Some(Brush::Solid(c)) => self.pixmap.fill(sk_color(*c)),
            Some(bg) => {
                self.pixmap.fill(tiny_skia::Color::TRANSPARENT);
                let r = Rect::new(0.0, 0.0, page.width_pt, page.height_pt);
                if let Some(path) = self.sk_path(&geometry::rounded_rect(r, 0.0)) {
                    self.fill_brush(&path, bg);
                }
            }
        }
    }

    fn push_clip(&mut self, clip: &ClipRegion) {
        let mask = Mask::new(self.pixmap.width(), self.pixmap.height()).and_then(|mut m| {
            let path = self.sk_path(&clip.page_outline())?;
            m.fill_path(&path, FillRule::Winding, true, Transform::identity());
            Some(m)
        });
        self.clips.push((self.pixmap.clone(), mask));
    }

    fn pop_clip(&mut self) {
        let Some((saved, mask)) = self.clips.pop() else {
            return;
        };
        let Some(mask) = mask else {
            self.pixmap = saved; // degenerate clip: nothing shows
            return;
        };
        // Outside the clip, put the saved pixels back; at its anti-aliased
        // edge, blend by coverage (premultiplied, so a straight lerp).
        let pixels = self.pixmap.pixels_mut().iter_mut().zip(saved.pixels());
        for ((dst, old), &m) in pixels.zip(mask.data()) {
            match m {
                255 => {}
                0 => *dst = *old,
                m => *dst = lerp_px(*old, *dst, m),
            }
        }
    }

    fn draw_shape(
        &mut self,
        segs: &[PathSegment],
        fill: Option<&Brush>,
        stroke: Option<&StrokeStyle>,
    ) {
        let Some(path) = self.sk_path(segs) else {
            return;
        };
        if let Some(fill) = fill {
            self.fill_brush(&path, fill);
        }
        if let Some(stroke) = stroke {
            self.stroke_shape(&path, stroke);
        }
    }

    fn fill_rect(
        &mut self,
        rect: Rect,
        fill: Option<&Brush>,
        stroke: Option<Color>,
        stroke_width: f32,
        corner_radius: f32,
    ) {
        if self.sk_rect(rect).is_none() {
            return;
        }
        // Sub-half-pixel radii are drawn square (indistinguishable at this dpi).
        let radius = if self.px(corner_radius) > 0.5 {
            corner_radius
        } else {
            0.0
        };
        let Some(path) = self.sk_path(&geometry::rounded_rect(rect, radius)) else {
            return;
        };

        if let Some(fill) = fill {
            self.fill_brush(&path, fill);
        }
        if let Some(stroke) = stroke {
            if stroke_width > 0.0 {
                let mut paint = Paint::default();
                paint.set_color(sk_color(stroke));
                paint.anti_alias = true;
                let sk_stroke = Stroke {
                    width: self.px(stroke_width),
                    ..Stroke::default()
                };
                self.pixmap
                    .stroke_path(&path, &paint, &sk_stroke, self.sk_xf(), None);
            }
        }
    }

    fn draw_text(&mut self, rect: Rect, text: &TextBox) {
        let Some(fonts) = self.fonts else {
            return;
        };
        if self.xf.is_some() {
            self.draw_text_transformed(fonts, rect, text);
            return;
        }
        let shaped = fonts.shape(&geometry::text_props(text), rect.w);
        let scale = self.scale;

        // Outline first, as one path along the exact (unhinted) glyph
        // outlines; the glyph bitmaps are then blended on top, so the stroke
        // never eats into the glyph shapes.
        if let Some(stroke) = &text.stroke {
            let segs = self.outlines.text_outline(fonts, &shaped, rect, text.align);
            if let Some(p) = self.sk_path(&segs) {
                self.stroke_shape(&p, stroke);
            }
        }

        self.draw_decorations(&shaped, rect, text.align, DecorationKind::Underline);
        for line in &shaped.lines {
            let align_off = geometry::align_offset(text.align, line.width, rect.w);
            for g in &line.glyphs {
                let pen_x = (rect.x + align_off + g.x + g.x_offset) * scale;
                let pen_y = (rect.y + line.baseline - g.y_offset) * scale;
                // Synthetic italic: swash shears the bitmap about the pen
                // position by the same angle the vector backends use.
                let (key, ox, oy) = CacheKey::new(
                    g.font_id,
                    g.glyph_id,
                    g.font_size * scale,
                    (pen_x, pen_y),
                    glyph_flags(g),
                );
                // SwashCache wants `&mut FontSystem`; clone the cached image so
                // the font-system borrow is released before we touch the pixmap.
                let image = fonts.with_system(|sys| self.swash.get_image(sys, key).clone());
                if let Some(image) = image {
                    blit_glyph(&mut self.pixmap, &image, ox, oy, g.color);
                }
            }
        }
        self.draw_decorations(&shaped, rect, text.align, DecorationKind::Strike);
    }

    fn draw_path(&mut self, path: &PathPaint) {
        if path.segments.len() < 2 {
            return;
        }
        if let Some(fill) = &path.fill {
            if let Some(p) = self.sk_path(&path.segments) {
                self.fill_brush(&p, fill);
            }
        }
        let Some(stroke) = &path.stroke else {
            return;
        };
        // Shaft pulled back into the heads (so no cap pokes past a tip), then
        // the heads filled solid in the stroke color.
        let geo = path.stroke_geometry();
        if let Some(p) = self.sk_path(&geo.shaft) {
            self.stroke_shape(&p, stroke);
        }
        for poly in &geo.heads {
            if let Some(p) = self.polygon(poly) {
                self.fill_shape(&p, stroke.color);
            }
        }
        let chevron = super::open_head_stroke(stroke);
        for v in &geo.open_heads {
            if let Some(p) = self.sk_path(&geometry::polyline(v)) {
                self.stroke_shape(&p, &chevron);
            }
        }
    }

    fn draw_ellipse(&mut self, rect: Rect, fill: Option<&Brush>, stroke: Option<&StrokeStyle>) {
        let Some(path) = self.sk_rect(rect).and_then(PathBuilder::from_oval) else {
            return;
        };
        if let Some(fill) = fill {
            self.fill_brush(&path, fill);
        }
        if let Some(stroke) = stroke {
            self.stroke_shape(&path, stroke);
        }
    }

    fn draw_image(&mut self, rect: Rect, src: &Path, fit: ImageFit, smooth: bool) {
        let resolved = geometry::resolve_image(self.base_dir, src);
        if self.try_draw_image(rect, &resolved, fit, smooth) {
            return;
        }
        tracing::warn!(path = %resolved.display(), "image unavailable; drawing placeholder");
        // Light-gray placeholder box with a thin border so a missing image is
        // obvious in the editor without panicking.
        let (fill, border) = geometry::placeholder_colors();
        self.fill_rect(
            rect,
            Some(&Brush::Solid(fill)),
            Some(border),
            geometry::PLACEHOLDER_BORDER,
            0.0,
        );
    }
}

/// Natural pixel size of an image file, without fully decoding it. Returns
/// `None` if the file can't be read or isn't a supported format. Used by the
/// editor to size newly inserted images.
pub fn image_size(path: &Path) -> Option<(u32, u32)> {
    image::image_dimensions(path).ok()
}

/// Render a computed layout to PNG bytes.
pub fn render_png(layout: &ComputedLayout, opts: &RenderOptions) -> Result<Vec<u8>, PngError> {
    let mut r = TinySkiaRenderer::new(&layout.page, opts)?;
    paint(layout, &mut r);
    r.into_png()
}

/// Render a computed layout to straight RGBA (for on-screen preview).
pub fn render_rgba(
    layout: &ComputedLayout,
    opts: &RenderOptions,
) -> Result<RenderedImage, PngError> {
    let mut r = TinySkiaRenderer::new(&layout.page, opts)?;
    paint(layout, &mut r);
    Ok(r.into_rgba())
}
