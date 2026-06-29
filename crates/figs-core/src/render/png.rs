//! PNG / RGBA backend built on tiny-skia. Points are scaled to pixels by
//! `dpi / 72`; the page background, rectangles, text glyphs and images are
//! rasterized with anti-aliasing and the pixmap is encoded to PNG or read back
//! as straight RGBA for on-screen preview.

use std::path::Path;

use cosmic_text::{CacheKey, CacheKeyFlags, SwashCache, SwashContent, SwashImage};
use tiny_skia::{
    ColorU8, FillRule, FilterQuality, IntSize, Mask, Paint, PathBuilder, Pixmap, PixmapPaint,
    PremultipliedColorU8, Rect as SkRect, Stroke, Transform,
};

use crate::geom::{Color, ImageFit, Rect};
use crate::layout::{ComputedLayout, PageGeom, TextBox};
use crate::schema::TextProps;
use crate::text::FontStore;

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
    /// Resolves relative image `src` paths.
    base_dir: &'a Path,
}

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
            base_dir: opts.base_dir,
        })
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

    fn sk_rect(&self, r: Rect) -> Option<SkRect> {
        SkRect::from_xywh(self.px(r.x), self.px(r.y), self.px(r.w), self.px(r.h))
    }

    /// Decode `path` and draw it scaled into `rect`. Returns false on any
    /// failure so the caller can paint a placeholder.
    fn try_draw_image(&mut self, rect: Rect, path: &Path, fit: ImageFit) -> bool {
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

        let (rx, ry, rw, rh) = (self.px(rect.x), self.px(rect.y), self.px(rect.w), self.px(rect.h));
        if rw <= 0.0 || rh <= 0.0 {
            return true; // nothing to draw, but not an error
        }
        let (iwf, ihf) = (iw as f32, ih as f32);
        let (sx, sy, tx, ty) = match fit {
            ImageFit::Fill => (rw / iwf, rh / ihf, rx, ry),
            ImageFit::Contain => {
                let s = (rw / iwf).min(rh / ihf);
                (s, s, rx + (rw - iwf * s) / 2.0, ry + (rh - ihf * s) / 2.0)
            }
            ImageFit::Cover => {
                let s = (rw / iwf).max(rh / ihf);
                (s, s, rx + (rw - iwf * s) / 2.0, ry + (rh - ihf * s) / 2.0)
            }
        };

        // Clip to the node rect so `Cover` doesn't bleed into siblings.
        let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height());
        if let (Some(mask), Some(skr)) = (mask.as_mut(), SkRect::from_xywh(rx, ry, rw, rh)) {
            let path = PathBuilder::from_rect(skr);
            mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
        }

        let pp = PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..Default::default()
        };
        self.pixmap.draw_pixmap(
            0,
            0,
            src.as_ref(),
            &pp,
            Transform::from_row(sx, 0.0, 0.0, sy, tx, ty),
            mask.as_ref(),
        );
        true
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
                    let (r, g, b, a) =
                        (img.data[i], img.data[i + 1], img.data[i + 2], img.data[i + 3]);
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

impl Renderer for TinySkiaRenderer<'_> {
    fn begin_page(&mut self, page: &PageGeom) {
        // Default to white when no background is given (print-friendly).
        let bg = page.background.unwrap_or(Color::WHITE);
        self.pixmap.fill(sk_color(bg));
    }

    fn fill_rect(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
        corner_radius: f32,
    ) {
        let Some(skr) = self.sk_rect(rect) else {
            return;
        };
        let radius_px = self.px(corner_radius);
        let path = if radius_px > 0.5 {
            rounded_rect_path(skr, radius_px)
        } else {
            Some(PathBuilder::from_rect(skr))
        };
        let Some(path) = path else { return };

        if let Some(fill) = fill {
            let mut paint = Paint::default();
            paint.set_color(sk_color(fill));
            paint.anti_alias = true;
            self.pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
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
                    .stroke_path(&path, &paint, &sk_stroke, Transform::identity(), None);
            }
        }
    }

    fn draw_text(&mut self, rect: Rect, text: &TextBox) {
        let Some(fonts) = self.fonts else {
            return;
        };
        let props = TextProps {
            content: text.content.clone(),
            font_size: text.font_size,
            font_family: text.font_family.clone(),
            font_weight: text.font_weight,
            color: text.color,
            align: text.align,
            line_height: text.line_height,
        };
        let shaped = fonts.shape(&props, rect.w);
        let scale = self.scale;

        for line in &shaped.lines {
            let align_off = align_offset(text.align, line.width, rect.w);
            for g in &line.glyphs {
                let pen_x = (rect.x + align_off + g.x) * scale;
                let pen_y = (rect.y + line.baseline) * scale;
                let (key, ox, oy) = CacheKey::new(
                    g.font_id,
                    g.glyph_id,
                    g.font_size * scale,
                    (pen_x, pen_y),
                    CacheKeyFlags::empty(),
                );
                // SwashCache wants `&mut FontSystem`; clone the cached image so
                // the font-system borrow is released before we touch the pixmap.
                let image = fonts.with_system(|sys| self.swash.get_image(sys, key).clone());
                if let Some(image) = image {
                    blit_glyph(&mut self.pixmap, &image, ox, oy, text.color);
                }
            }
        }
    }

    fn draw_image(&mut self, rect: Rect, src: &Path, fit: ImageFit) {
        let resolved = if src.is_absolute() {
            src.to_path_buf()
        } else {
            self.base_dir.join(src)
        };
        if self.try_draw_image(rect, &resolved, fit) {
            return;
        }
        tracing::warn!(path = %resolved.display(), "image unavailable; drawing placeholder");
        // Light-gray placeholder box with a thin border so a missing image is
        // obvious in the editor without panicking.
        let fill = Color::parse_hex("#e7e4dd").unwrap_or(Color::WHITE);
        let border = Color::parse_hex("#b6b2a7").unwrap_or(Color::BLACK);
        self.fill_rect(rect, Some(fill), Some(border), 1.0, 0.0);
    }
}

/// Horizontal offset (points) to apply to a line of the given width inside a
/// box of width `box_w`, per the text alignment.
fn align_offset(align: crate::geom::TextAlign, line_w: f32, box_w: f32) -> f32 {
    use crate::geom::TextAlign::*;
    match align {
        Left | Justify => 0.0,
        Center => ((box_w - line_w) / 2.0).max(0.0),
        Right => (box_w - line_w).max(0.0),
    }
}

/// Build a rounded-rectangle path. The radius is clamped to half the shorter
/// side so corners never overlap.
fn rounded_rect_path(r: SkRect, radius: f32) -> Option<tiny_skia::Path> {
    let radius = radius.min(r.width() / 2.0).min(r.height() / 2.0);
    // Cubic control offset for a quarter-circle approximation.
    let k = radius * 0.5522847;
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());

    let mut pb = PathBuilder::new();
    pb.move_to(l + radius, t);
    pb.line_to(rt - radius, t);
    pb.cubic_to(rt - radius + k, t, rt, t + radius - k, rt, t + radius);
    pb.line_to(rt, b - radius);
    pb.cubic_to(rt, b - radius + k, rt - radius + k, b, rt - radius, b);
    pb.line_to(l + radius, b);
    pb.cubic_to(l + radius - k, b, l, b - radius + k, l, b - radius);
    pb.line_to(l, t + radius);
    pb.cubic_to(l, t + radius - k, l + radius - k, t, l + radius, t);
    pb.close();
    pb.finish()
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
pub fn render_rgba(layout: &ComputedLayout, opts: &RenderOptions) -> Result<RenderedImage, PngError> {
    let mut r = TinySkiaRenderer::new(&layout.page, opts)?;
    paint(layout, &mut r);
    Ok(r.into_rgba())
}
