//! PDF backend built on printpdf 0.8. Consumes the same [`ComputedLayout`] as
//! the PNG backend, so output matches. PDF user space is points with a
//! bottom-left origin, so the only mapping is a y-flip at this boundary.
//!
//! - rects -> filled/stroked polygons (vector),
//! - images -> embedded XObjects (cropped for `cover`),
//! - text -> embedded subset fonts with glyph-id show ops (`WriteCodepoints`),
//!   keeping text selectable/copyable and visually identical to the PNG.

use std::collections::HashMap;

use printpdf::{
    Color as PdfColor, FontId, Mm, Op, ParsedFont, PdfDocument, PdfPage, PdfSaveOptions, Point,
    Polygon, PolygonRing, Pt, RawImage, RawImageData, RawImageFormat, Rgb, XObjectTransform,
};

use crate::assets::Assets;
use crate::geom::{Color, ImageFit, Rect, TextAlign};
use crate::image::DecodedImage;
use crate::layout::{ComputedLayout, PageGeom, PaintText};
use crate::text::FontStore;

use super::{paint, Renderer};

/// Errors from PDF rendering.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("PDF generation produced warnings that prevented output")]
    Failed,
}

const PT_PER_MM: f32 = 72.0 / 25.4;

/// printpdf renderer accumulating page operations.
pub struct PdfRenderer {
    doc: PdfDocument,
    ops: Vec<Op>,
    page_w: f32,
    page_h: f32,
    fonts: HashMap<cosmic_text::fontdb::ID, FontId>,
}

impl PdfRenderer {
    pub fn new() -> Self {
        PdfRenderer {
            doc: PdfDocument::new("figs"),
            ops: Vec::new(),
            page_w: 0.0,
            page_h: 0.0,
            fonts: HashMap::new(),
        }
    }

    /// Flip a top-left y coordinate into PDF's bottom-left space.
    fn flip_y(&self, y_top: f32) -> f32 {
        self.page_h - y_top
    }

    /// Embed (once) the font behind `id`, returning its printpdf handle.
    fn font_for(&mut self, id: cosmic_text::fontdb::ID, fonts: &FontStore) -> Option<FontId> {
        if let Some(fid) = self.fonts.get(&id) {
            return Some(fid.clone());
        }
        let (bytes, index) = fonts.font_data(id)?;
        let mut warnings = Vec::new();
        let parsed = ParsedFont::from_bytes(&bytes, index as usize, &mut warnings)?;
        let fid = self.doc.add_font(&parsed);
        self.fonts.insert(id, fid.clone());
        Some(fid)
    }

    /// Finish the document into PDF bytes.
    pub fn into_pdf(mut self) -> Result<Vec<u8>, PdfError> {
        let ops = std::mem::take(&mut self.ops);
        let page = PdfPage::new(
            Mm(self.page_w / PT_PER_MM),
            Mm(self.page_h / PT_PER_MM),
            ops,
        );
        let mut warnings = Vec::new();
        self.doc.with_pages(vec![page]);
        Ok(self.doc.save(&PdfSaveOptions::default(), &mut warnings))
    }
}

impl Default for PdfRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn pdf_color(c: Color) -> PdfColor {
    PdfColor::Rgb(Rgb {
        r: c.r.clamp(0.0, 1.0),
        g: c.g.clamp(0.0, 1.0),
        b: c.b.clamp(0.0, 1.0),
        icc_profile: None,
    })
}

impl Renderer for PdfRenderer {
    fn begin_page(&mut self, page: &PageGeom) {
        self.page_w = page.width_pt;
        self.page_h = page.height_pt;
        if let Some(bg) = page.background {
            self.fill_rect(
                Rect::new(0.0, 0.0, page.width_pt, page.height_pt),
                Some(bg),
                None,
                0.0,
                0.0,
            );
        }
    }

    fn fill_rect(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f32,
        _corner_radius: f32,
    ) {
        // Corners (y-up). Rounded corners are approximated as sharp in PDF.
        let top = self.flip_y(rect.y);
        let bottom = self.flip_y(rect.y + rect.h);
        let corner = |x: f32, y: f32| Point {
            x: Pt(x),
            y: Pt(y),
        };
        let ring = PolygonRing {
            points: vec![
                printpdf::LinePoint { p: corner(rect.x, top), bezier: false },
                printpdf::LinePoint { p: corner(rect.x + rect.w, top), bezier: false },
                printpdf::LinePoint { p: corner(rect.x + rect.w, bottom), bezier: false },
                printpdf::LinePoint { p: corner(rect.x, bottom), bezier: false },
            ],
        };

        if let Some(fill) = fill {
            self.ops.push(Op::SetFillColor { col: pdf_color(fill) });
            self.ops.push(Op::DrawPolygon {
                polygon: Polygon {
                    rings: vec![ring.clone()],
                    mode: printpdf::PaintMode::Fill,
                    winding_order: printpdf::WindingOrder::NonZero,
                },
            });
        }
        if let Some(stroke) = stroke {
            if stroke_width > 0.0 {
                self.ops.push(Op::SetOutlineColor { col: pdf_color(stroke) });
                self.ops.push(Op::SetOutlineThickness { pt: Pt(stroke_width) });
                self.ops.push(Op::DrawPolygon {
                    polygon: Polygon {
                        rings: vec![ring],
                        mode: printpdf::PaintMode::Stroke,
                        winding_order: printpdf::WindingOrder::NonZero,
                    },
                });
            }
        }
    }

    fn draw_text(&mut self, rect: Rect, text: &PaintText, fonts: &FontStore) {
        let content_x = rect.x + text.padding.left;
        let content_y = rect.y + text.padding.top;
        let content_w = (rect.w - text.padding.horizontal()).max(0.0);

        for line in &text.shaped.lines {
            if line.glyphs.is_empty() {
                continue;
            }
            let align_off = match text.align {
                TextAlign::Left | TextAlign::Justify => 0.0,
                TextAlign::Center => ((content_w - line.width) * 0.5).max(0.0),
                TextAlign::Right => (content_w - line.width).max(0.0),
            };
            let baseline_y = self.flip_y(content_y + line.baseline);

            self.ops.push(Op::StartTextSection);
            self.ops.push(Op::SetFillColor { col: pdf_color(text.color) });

            // Group consecutive glyphs by font (handles script fallback within a
            // line), emitting each run at its own cursor with its own font.
            let mut i = 0;
            while i < line.glyphs.len() {
                let id = line.glyphs[i].font_id;
                let mut j = i;
                while j < line.glyphs.len() && line.glyphs[j].font_id == id {
                    j += 1;
                }
                if let Some(fid) = self.font_for(id, fonts) {
                    let run = &line.glyphs[i..j];
                    let size = run[0].font_size;
                    let x = content_x + align_off + run[0].x;
                    self.ops.push(Op::SetFontSize { size: Pt(size), font: fid.clone() });
                    self.ops.push(Op::SetTextCursor {
                        pos: Point { x: Pt(x), y: Pt(baseline_y) },
                    });
                    self.ops.push(Op::WriteCodepoints {
                        font: fid,
                        cp: run.iter().map(|g| (g.glyph_id, g.ch)).collect(),
                    });
                }
                i = j;
            }
            self.ops.push(Op::EndTextSection);
        }
    }

    fn draw_image(&mut self, rect: Rect, image: &DecodedImage, fit: ImageFit) {
        if image.width == 0 || image.height == 0 || rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        let (iw, ih) = (image.width as f32, image.height as f32);

        // Compute the source pixels to embed and the placed pt box (top-left).
        let (rgba, src_w, src_h, draw) = match fit {
            ImageFit::Fill => (image.rgba.clone(), image.width, image.height, rect),
            ImageFit::Contain => {
                let s = (rect.w / iw).min(rect.h / ih);
                let (dw, dh) = (iw * s, ih * s);
                let placed = Rect::new(
                    rect.x + (rect.w - dw) / 2.0,
                    rect.y + (rect.h - dh) / 2.0,
                    dw,
                    dh,
                );
                (image.rgba.clone(), image.width, image.height, placed)
            }
            ImageFit::Cover => {
                let s = (rect.w / iw).max(rect.h / ih);
                let cw = (rect.w / s).min(iw).max(1.0);
                let ch = (rect.h / s).min(ih).max(1.0);
                let cx = ((iw - cw) / 2.0).max(0.0);
                let cy = ((ih - ch) / 2.0).max(0.0);
                let (cropped, cwu, chu) =
                    crop_rgba(image, cx as u32, cy as u32, cw as u32, ch as u32);
                (cropped, cwu, chu, rect)
            }
        };

        let raw = RawImage {
            pixels: RawImageData::U8(rgba),
            width: src_w as usize,
            height: src_h as usize,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new(),
        };
        let id = self.doc.add_image(&raw);

        // With dpi=72, one source pixel maps to one pt before scaling.
        let scale_x = draw.w / src_w as f32;
        let scale_y = draw.h / src_h as f32;
        let bottom_left_y = self.flip_y(draw.y + draw.h);
        self.ops.push(Op::UseXobject {
            id,
            transform: XObjectTransform {
                translate_x: Some(Pt(draw.x)),
                translate_y: Some(Pt(bottom_left_y)),
                rotate: None,
                scale_x: Some(scale_x),
                scale_y: Some(scale_y),
                dpi: Some(72.0),
            },
        });
    }
}

/// Crop a centered sub-rectangle out of a decoded image's RGBA buffer.
fn crop_rgba(image: &DecodedImage, x: u32, y: u32, w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let w = w.min(image.width.saturating_sub(x)).max(1);
    let h = h.min(image.height.saturating_sub(y)).max(1);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let src_y = y + row;
        let start = ((src_y * image.width + x) * 4) as usize;
        let end = start + (w * 4) as usize;
        out.extend_from_slice(&image.rgba[start..end]);
    }
    (out, w, h)
}

/// Render a computed layout to PDF bytes.
pub fn render_pdf(layout: &ComputedLayout, assets: &Assets) -> Result<Vec<u8>, PdfError> {
    let mut r = PdfRenderer::new();
    paint(layout, assets, &mut r);
    r.into_pdf()
}
