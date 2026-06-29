//! PNG backend built on tiny-skia. Points are scaled to pixels by
//! `dpi / 72`; the page background and rectangles are rasterized with
//! anti-aliasing and the pixmap is encoded to PNG.

use tiny_skia::{
    FillRule, Paint, PathBuilder, Pixmap, Rect as SkRect, Stroke, Transform,
};

use crate::geom::{Color, Rect, TextAlign};
use crate::layout::{ComputedLayout, PageGeom, PaintText};
use crate::text::{FontStore, GlyphBitmap};

use super::{paint, Renderer};

/// Errors from PNG rendering.
#[derive(Debug, thiserror::Error)]
pub enum PngError {
    #[error("page is too large or has zero size ({0}x{1} px)")]
    BadCanvasSize(u32, u32),
    #[error("failed to encode PNG: {0}")]
    Encode(String),
}

/// A tiny-skia raster renderer. Build via [`render_png`] or drive manually with
/// [`paint`].
pub struct TinySkiaRenderer {
    pixmap: Pixmap,
    /// Points -> pixels scale (dpi / 72).
    scale: f32,
}

impl TinySkiaRenderer {
    /// Allocate a canvas for a page. Returns an error for a degenerate size.
    pub fn new(page: &PageGeom) -> Result<Self, PngError> {
        let scale = page.dpi / crate::units::PT_PER_INCH;
        let w = (page.width_pt * scale).round() as u32;
        let h = (page.height_pt * scale).round() as u32;
        let pixmap = Pixmap::new(w, h).ok_or(PngError::BadCanvasSize(w, h))?;
        Ok(TinySkiaRenderer { pixmap, scale })
    }

    /// Encode the current canvas as PNG bytes.
    pub fn into_png(self) -> Result<Vec<u8>, PngError> {
        self.pixmap
            .encode_png()
            .map_err(|e| PngError::Encode(e.to_string()))
    }

    fn px(&self, v: f32) -> f32 {
        v * self.scale
    }

    fn sk_rect(&self, r: Rect) -> Option<SkRect> {
        SkRect::from_xywh(self.px(r.x), self.px(r.y), self.px(r.w), self.px(r.h))
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

impl Renderer for TinySkiaRenderer {
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

    fn draw_text(&mut self, rect: Rect, text: &PaintText, fonts: &FontStore) {
        let scale = self.scale;
        let content_x = rect.x + text.padding.left;
        let content_y = rect.y + text.padding.top;
        let content_w = (rect.w - text.padding.horizontal()).max(0.0);

        for line in &text.shaped.lines {
            let align_off = match text.align {
                TextAlign::Left | TextAlign::Justify => 0.0,
                TextAlign::Center => ((content_w - line.width) * 0.5).max(0.0),
                TextAlign::Right => (content_w - line.width).max(0.0),
            };
            let baseline_px = (content_y + line.baseline) * scale;
            for g in &line.glyphs {
                let pen_x_px = (content_x + align_off + g.x) * scale;
                let px_size = g.font_size * scale;
                if let Some(bm) = fonts.glyph_alpha(g.font_id, g.glyph_id, px_size) {
                    let x0 = pen_x_px.round() as i32 + bm.left;
                    let y0 = baseline_px.round() as i32 - bm.top;
                    blit_alpha(&mut self.pixmap, &bm, x0, y0, text.color);
                }
            }
        }
    }
}

/// Composite a glyph coverage mask onto the pixmap (premultiplied RGBA, src-over).
fn blit_alpha(pixmap: &mut Pixmap, bm: &GlyphBitmap, x0: i32, y0: i32, color: Color) {
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;
    let (cr, cg, cb, ca) = (
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.a.clamp(0.0, 1.0),
    );
    let data = pixmap.data_mut();
    for gy in 0..bm.height as i32 {
        let py = y0 + gy;
        if py < 0 || py >= ph {
            continue;
        }
        for gx in 0..bm.width as i32 {
            let px = x0 + gx;
            if px < 0 || px >= pw {
                continue;
            }
            let cov = bm.alpha[(gy * bm.width as i32 + gx) as usize] as f32 / 255.0 * ca;
            if cov <= 0.0 {
                continue;
            }
            let idx = ((py * pw + px) * 4) as usize;
            let inv = 1.0 - cov;
            // Source is premultiplied by coverage; blend over destination.
            data[idx] = (cr * cov * 255.0 + data[idx] as f32 * inv).round() as u8;
            data[idx + 1] = (cg * cov * 255.0 + data[idx + 1] as f32 * inv).round() as u8;
            data[idx + 2] = (cb * cov * 255.0 + data[idx + 2] as f32 * inv).round() as u8;
            data[idx + 3] = (cov * 255.0 + data[idx + 3] as f32 * inv).round() as u8;
        }
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

/// Render a computed layout to PNG bytes. `fonts` rasterizes any shaped text;
/// layouts without text ignore it.
pub fn render_png(layout: &ComputedLayout, fonts: &FontStore) -> Result<Vec<u8>, PngError> {
    let mut r = TinySkiaRenderer::new(&layout.page)?;
    paint(layout, fonts, &mut r);
    r.into_png()
}
