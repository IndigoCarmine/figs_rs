//! SVG backend: a hand-written SVG 1.1 string builder (no dependencies beyond
//! `ttf-parser` for glyph outlines).
//!
//! The root `<svg>` is `width × height` **pt** with a `viewBox` in points, so
//! one user unit is one point and IR coordinates (top-left origin) are written
//! unchanged.
//!
//! - **Shapes** use native elements — `<rect>` (with `rx`/`ry`), `<ellipse>`
//!   and `<path>` — with `stroke-dasharray` / `stroke-linecap`; arrowheads are
//!   filled `<path>` triangles from the geometry shared with PNG/PDF.
//! - **Text** is drawn as **glyph outlines**, not `<text>`: each glyph cosmic-text
//!   shaped for layout is defined once (unhinted, in font units) in `<defs>` and
//!   placed with `<use>`. The output is therefore exact and identical on every
//!   viewer, with no dependence on installed fonts; the cost is that text isn't
//!   selectable (the group carries an `aria-label` with the content). Bitmap-only
//!   glyphs (e.g. colour emoji) have no outline and are skipped. A text
//!   `stroke` is a single stroked `<path>` along the placed glyph outlines,
//!   written *before* the glyph group (paint order stroke → fill for the whole
//!   string, not per glyph — so a neighbour's outline never covers a glyph,
//!   and a translucent outline stays uniform where outlines overlap).
//! - **Images** are embedded as base64 `data:` URIs (PNG/JPEG bytes verbatim,
//!   other formats re-encoded as PNG); `cover` is clipped to the node rect.
//! - **Rotation**: each rotated node's elements are wrapped in
//!   `<g transform="matrix(a b c d e f)">` (its absolute IR transform), so the
//!   elements inside keep their unrotated coordinates.
//! - **Gradients** are `<linearGradient>` / `<radialGradient>` in `<defs>`
//!   (unique ids, `gradientUnits="userSpaceOnUse"`): their coordinates are the
//!   referencing element's user space — the node's local frame inside its
//!   `<g transform>` — so they turn with the node.
//! - **Clips** are `<g clip-path="url(#…)">` groups around the clipped nodes,
//!   with the `<clipPath>` outline in page coordinates.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use crate::geom::{
    Affine, Brush, Color, ImageFit, LineCap, LineJoin, PathSegment, Point, Rect, StrokeStyle,
    TextAlign,
};
use crate::layout::{ClipRegion, ComputedLayout, PageGeom, PathPaint, TextBox};
use crate::text::{DecorationKind, FontStore, ShapedText};

use super::geometry;
use super::outline::OutlineCache;
use super::{paint, RenderOptions, Renderer};

type FaceId = cosmic_text::fontdb::ID;

/// An SVG renderer accumulating markup. Build via [`render_svg`].
pub struct SvgRenderer<'a> {
    fonts: Option<&'a FontStore>,
    base_dir: &'a Path,
    width: f32,
    height: f32,
    defs: String,
    body: String,
    next_id: usize,
    outlines: OutlineCache,
    /// `<defs>` id of each glyph outline (`None` = no outline, e.g. a space).
    glyphs: HashMap<(FaceId, u16), Option<(String, f32)>>,
}

impl<'a> SvgRenderer<'a> {
    pub fn new(opts: &RenderOptions<'a>) -> Self {
        SvgRenderer {
            fonts: opts.fonts,
            base_dir: opts.base_dir,
            width: 0.0,
            height: 0.0,
            defs: String::new(),
            body: String::new(),
            next_id: 0,
            outlines: OutlineCache::new(),
            glyphs: HashMap::new(),
        }
    }

    /// Assemble the finished document.
    pub fn finish(self) -> String {
        let (w, h) = (num(self.width), num(self.height));
        let mut out = String::with_capacity(self.defs.len() + self.body.len() + 256);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        let _ = writeln!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             version=\"1.1\" width=\"{w}pt\" height=\"{h}pt\" viewBox=\"0 0 {w} {h}\">"
        );
        if !self.defs.is_empty() {
            out.push_str("<defs>\n");
            out.push_str(&self.defs);
            out.push_str("</defs>\n");
        }
        out.push_str(&self.body);
        out.push_str("</svg>\n");
        out
    }

    fn fresh_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    /// `<defs>` id and font-units → points scale factor numerator (units per
    /// em) of a glyph outline, defining it on first use.
    fn glyph_def(&mut self, fonts: &FontStore, face: FaceId, gid: u16) -> Option<(String, f32)> {
        if let Some(hit) = self.glyphs.get(&(face, gid)) {
            return hit.clone();
        }
        let outline = self
            .outlines
            .glyph(fonts, face, gid)
            .map(|o| (write_d(&o.segments, ""), o.units_per_em));
        let entry = outline.map(|(d, upem)| {
            let id = self.fresh_id("g");
            let _ = writeln!(self.defs, "<path id=\"{id}\" d=\"{d}\"/>");
            (id, upem)
        });
        self.glyphs.insert((face, gid), entry.clone());
        entry
    }

    /// `<rect>`s for the `kind` decoration bars of a shaped text box.
    fn decorations(
        &mut self,
        shaped: &ShapedText,
        rect: Rect,
        align: TextAlign,
        kind: DecorationKind,
    ) {
        for (r, color) in geometry::decoration_rects(shaped, rect, align, kind) {
            self.rect_element(r, Some(&Brush::Solid(color)), None, 0.0);
        }
    }

    /// Write an image element (and its clip, for `cover`). Returns false when
    /// the image can't be read so the caller draws a placeholder.
    fn try_image(&mut self, rect: Rect, path: &Path, fit: ImageFit) -> bool {
        let Ok(bytes) = std::fs::read(path) else {
            return false;
        };
        let Ok(decoded) = image::load_from_memory(&bytes) else {
            return false;
        };
        let (iw, ih) = (decoded.width(), decoded.height());
        if iw == 0 || ih == 0 {
            return false;
        }
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return true;
        }
        let (mime, payload) = match image::guess_format(&bytes) {
            Ok(image::ImageFormat::Png) => ("image/png", bytes),
            Ok(image::ImageFormat::Jpeg) => ("image/jpeg", bytes),
            _ => {
                let mut png = Vec::new();
                if decoded
                    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .is_err()
                {
                    return false;
                }
                ("image/png", png)
            }
        };
        let dest = geometry::image_placement(rect, iw as f32, ih as f32, fit);
        let clip = if fit == ImageFit::Cover {
            let id = self.fresh_id("clip");
            let _ = writeln!(
                self.defs,
                "<clipPath id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>",
                num(rect.x),
                num(rect.y),
                num(rect.w),
                num(rect.h)
            );
            format!(" clip-path=\"url(#{id})\"")
        } else {
            String::new()
        };
        let _ = writeln!(
            self.body,
            "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\"{clip} \
             xlink:href=\"data:{mime};base64,{}\"/>",
            num(dest.x),
            num(dest.y),
            num(dest.w),
            num(dest.h),
            base64(&payload)
        );
        true
    }

    /// Append the fill and stroke attributes for a brush fill: a gradient is
    /// defined in `<defs>` (fresh id) and referenced with `url(#…)`.
    fn push_brush_paint(
        &mut self,
        el: &mut String,
        fill: Option<&Brush>,
        stroke: Option<&StrokeStyle>,
    ) {
        match fill {
            None => push_fill(el, None),
            Some(b) => match self.gradient_def(b) {
                Some(id) => {
                    let _ = write!(el, " fill=\"url(#{id})\"");
                }
                None => push_fill(el, Some(b.as_solid().unwrap_or(b.fallback_color()))),
            },
        }
        push_stroke(el, stroke);
    }

    /// Define the gradient of `b` in `<defs>`; `None` for a solid brush or a
    /// degenerate gradient (drawn flat in its fallback colour, as SVG would).
    fn gradient_def(&mut self, b: &Brush) -> Option<String> {
        let k = |v: f32| fmt_decimals(v, 6);
        let open = match b {
            Brush::Solid(_) => return None,
            Brush::Linear { start, end, .. } => {
                if (end.x - start.x).hypot(end.y - start.y) < 1e-6 {
                    return None;
                }
                format!(
                    "linearGradient gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"",
                    num(start.x),
                    num(start.y),
                    num(end.x),
                    num(end.y)
                )
            }
            Brush::Radial {
                center,
                focal,
                radius,
                transform: t,
                ..
            } => {
                if *radius <= 0.0 || (t.a * t.d - t.b * t.c).abs() < 1e-12 {
                    return None;
                }
                let mut s = format!(
                    "radialGradient gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\"",
                    k(center.x),
                    k(center.y),
                    k(*radius)
                );
                if focal != center {
                    let _ = write!(s, " fx=\"{}\" fy=\"{}\"", k(focal.x), k(focal.y));
                }
                if !t.is_identity() {
                    let _ = write!(
                        s,
                        " gradientTransform=\"matrix({} {} {} {} {} {})\"",
                        k(t.a),
                        k(t.b),
                        k(t.c),
                        k(t.d),
                        num(t.e),
                        num(t.f)
                    );
                }
                s
            }
        };
        let tag = if matches!(b, Brush::Linear { .. }) {
            "linearGradient"
        } else {
            "radialGradient"
        };
        let id = self.fresh_id("grad");
        let _ = writeln!(self.defs, "<{open} id=\"{id}\">");
        for s in b.stops() {
            let _ = write!(
                self.defs,
                "<stop offset=\"{}\" stop-color=\"{}\"",
                fmt_decimals(s.offset, 4),
                hex(s.color)
            );
            if s.color.a < 1.0 {
                let _ = write!(self.defs, " stop-opacity=\"{}\"", num(s.color.a.max(0.0)));
            }
            self.defs.push_str("/>\n");
        }
        let _ = writeln!(self.defs, "</{tag}>");
        Some(id)
    }

    fn rect_element(
        &mut self,
        rect: Rect,
        fill: Option<&Brush>,
        stroke: Option<&StrokeStyle>,
        radius: f32,
    ) {
        let radius = radius.min(rect.w / 2.0).min(rect.h / 2.0);
        let mut el = format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
            num(rect.x),
            num(rect.y),
            num(rect.w),
            num(rect.h)
        );
        if radius > 0.0 {
            let r = num(radius);
            let _ = write!(el, " rx=\"{r}\" ry=\"{r}\"");
        }
        self.push_brush_paint(&mut el, fill, stroke);
        el.push_str("/>\n");
        self.body.push_str(&el);
    }

    fn path_element(
        &mut self,
        segs: &[PathSegment],
        fill: Option<&Brush>,
        stroke: Option<&StrokeStyle>,
    ) {
        if segs.is_empty() {
            return;
        }
        let mut el = format!("<path d=\"{}\"", path_d(segs));
        self.push_brush_paint(&mut el, fill, stroke);
        el.push_str("/>\n");
        self.body.push_str(&el);
    }
}

/// Append `fill`/`stroke` presentation attributes (SVG defaults: black fill,
/// no stroke — so both are always explicit).
fn push_paint(el: &mut String, fill: Option<Color>, stroke: Option<&StrokeStyle>) {
    push_fill(el, fill);
    push_stroke(el, stroke);
}

/// A solid `fill` (with opacity) or `fill="none"`.
fn push_fill(el: &mut String, fill: Option<Color>) {
    match fill {
        Some(c) => {
            let _ = write!(el, " fill=\"{}\"", hex(c));
            if c.a < 1.0 {
                let _ = write!(el, " fill-opacity=\"{}\"", num(c.a.max(0.0)));
            }
        }
        None => el.push_str(" fill=\"none\""),
    }
}

/// Stroke attributes (nothing for `None`: SVG's default is no stroke).
fn push_stroke(el: &mut String, stroke: Option<&StrokeStyle>) {
    if let Some(s) = stroke {
        let _ = write!(
            el,
            " stroke=\"{}\" stroke-width=\"{}\"",
            hex(s.color),
            num(s.width)
        );
        if s.color.a < 1.0 {
            let _ = write!(el, " stroke-opacity=\"{}\"", num(s.color.a.max(0.0)));
        }
        if !s.dash.is_empty() {
            let dash: Vec<String> = s.dash.iter().map(|d| num(*d)).collect();
            let _ = write!(el, " stroke-dasharray=\"{}\"", dash.join(" "));
        }
        match s.cap {
            LineCap::Butt => {}
            LineCap::Round => el.push_str(" stroke-linecap=\"round\""),
            LineCap::Square => el.push_str(" stroke-linecap=\"square\""),
        }
        match s.join {
            LineJoin::Miter => {}
            LineJoin::Round => el.push_str(" stroke-linejoin=\"round\""),
            LineJoin::Bevel => el.push_str(" stroke-linejoin=\"bevel\""),
        }
    }
}

/// SVG path data for shared segments.
fn path_d(segs: &[PathSegment]) -> String {
    write_d(segs, " ")
}

/// Path data with `sep` between commands (`""` for compact glyph outlines).
fn write_d(segs: &[PathSegment], sep: &str) -> String {
    let mut d = String::new();
    let pt = |d: &mut String, p: Point| {
        let _ = write!(d, "{} {}", num(p.x), num(p.y));
    };
    for seg in segs {
        if !d.is_empty() {
            d.push_str(sep);
        }
        match *seg {
            PathSegment::MoveTo(p) => {
                d.push('M');
                pt(&mut d, p);
            }
            PathSegment::LineTo(p) => {
                d.push('L');
                pt(&mut d, p);
            }
            PathSegment::QuadTo(c, p) => {
                d.push('Q');
                pt(&mut d, c);
                d.push(' ');
                pt(&mut d, p);
            }
            PathSegment::CubicTo(c1, c2, p) => {
                d.push('C');
                pt(&mut d, c1);
                d.push(' ');
                pt(&mut d, c2);
                d.push(' ');
                pt(&mut d, p);
            }
            PathSegment::Close => d.push('Z'),
        }
    }
    d
}

/// Format a coordinate: at most 3 decimals (1/1000 pt), trailing zeros trimmed.
fn num(v: f32) -> String {
    fmt_decimals(v, 3)
}

fn fmt_decimals(v: f32, decimals: usize) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let mut s = format!("{v:.decimals$}");
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.').len();
        s.truncate(trimmed);
    }
    if s == "-0" {
        s = "0".into();
    }
    s
}

/// `#rrggbb` (alpha is written separately as an opacity attribute).
fn hex(c: Color) -> String {
    let u = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", u(c.r), u(c.g), u(c.b))
}

/// Escape text for an XML attribute value.
fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' | '\t' => out.push(' '),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

/// Standard (RFC 4648) base64 with padding.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

impl Renderer for SvgRenderer<'_> {
    fn push_transform(&mut self, t: &Affine) {
        // Coefficients get more decimals than coordinates: an error of 1e-3 in
        // cos/sin would move a far corner by a visible fraction of a point.
        let k = |v: f32| fmt_decimals(v, 6);
        let _ = writeln!(
            self.body,
            "<g transform=\"matrix({} {} {} {} {} {})\">",
            k(t.a),
            k(t.b),
            k(t.c),
            k(t.d),
            num(t.e),
            num(t.f)
        );
    }

    fn pop_transform(&mut self) {
        self.body.push_str("</g>\n");
    }

    fn begin_page(&mut self, page: &PageGeom) {
        self.width = page.width_pt;
        self.height = page.height_pt;
        // Same default as PNG: white when no background is given.
        let white = Brush::Solid(Color::WHITE);
        let bg = page.background.as_ref().unwrap_or(&white);
        let r = Rect::new(0.0, 0.0, page.width_pt, page.height_pt);
        self.rect_element(r, Some(bg), None, 0.0);
    }

    fn push_clip(&mut self, clip: &ClipRegion) {
        let id = self.fresh_id("clip");
        let _ = writeln!(
            self.defs,
            "<clipPath id=\"{id}\"><path d=\"{}\"/></clipPath>",
            path_d(&clip.page_outline())
        );
        let _ = writeln!(self.body, "<g clip-path=\"url(#{id})\">");
    }

    fn pop_clip(&mut self) {
        self.body.push_str("</g>\n");
    }

    fn draw_shape(
        &mut self,
        segs: &[PathSegment],
        fill: Option<&Brush>,
        stroke: Option<&StrokeStyle>,
    ) {
        if fill.is_some() || stroke.is_some() {
            self.path_element(segs, fill, stroke);
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
        if rect.w <= 0.0 || rect.h <= 0.0 || (fill.is_none() && stroke.is_none()) {
            return;
        }
        let stroke = stroke
            .filter(|_| stroke_width > 0.0)
            .map(|color| StrokeStyle {
                color,
                width: stroke_width,
                dash: Vec::new(),
                cap: LineCap::Butt,
                join: LineJoin::Miter,
            });
        if fill.is_none() && stroke.is_none() {
            return;
        }
        self.rect_element(rect, fill, stroke.as_ref(), corner_radius);
    }

    fn draw_text(&mut self, rect: Rect, text: &TextBox) {
        let Some(fonts) = self.fonts else {
            return;
        };
        let shaped = fonts.shape(&geometry::text_props(text), rect.w);
        // Outline first: one stroked path under the whole glyph group.
        if let Some(stroke) = &text.stroke {
            let segs = self.outlines.text_outline(fonts, &shaped, rect, text.align);
            if !segs.is_empty() {
                let mut el = format!("<path d=\"{}\"", path_d(&segs));
                push_paint(&mut el, None, Some(stroke));
                el.push_str(
                    " aria-hidden=\"true\"/>
",
                );
                self.body.push_str(&el);
            }
        }
        self.decorations(&shaped, rect, text.align, DecorationKind::Underline);
        // (fill colour, `<use>` element) per drawn glyph.
        let mut uses: Vec<(Color, String)> = Vec::new();
        let shear = geometry::italic_shear();
        for line in &shaped.lines {
            let align_off = geometry::align_offset(text.align, line.width, rect.w);
            let baseline = rect.y + line.baseline;
            for g in &line.glyphs {
                let Some((id, upem)) = self.glyph_def(fonts, g.font_id, g.glyph_id) else {
                    continue;
                };
                // Font units (y up) -> points (y down) at the pen position;
                // a synthetic italic adds the shear `x += t·y`.
                let k = g.font_size / upem;
                let s = fmt_decimals(k, 7);
                let c = if g.synthetic_italic {
                    fmt_decimals(k * shear, 7)
                } else {
                    "0".to_string()
                };
                let x = num(rect.x + align_off + g.x + g.x_offset);
                let y = num(baseline - g.y_offset);
                uses.push((
                    g.color,
                    format!(
                        "<use xlink:href=\"#{id}\" transform=\"matrix({s} 0 {c} -{s} {x} {y})\"/>\n"
                    ),
                ));
            }
        }
        if !uses.is_empty() {
            let fill = |c: Color| {
                let mut a = format!(" fill=\"{}\"", hex(c));
                if c.a < 1.0 {
                    let _ = write!(a, " fill-opacity=\"{}\"", num(c.a.max(0.0)));
                }
                a
            };
            let label = escape_attr(&text.content);
            if uses.iter().all(|(c, _)| *c == uses[0].0) {
                // One colour: a single group carries label and fill.
                let _ = writeln!(self.body, "<g aria-label=\"{label}\"{}>", fill(uses[0].0));
                for (_, u) in &uses {
                    self.body.push_str(u);
                }
            } else {
                // Rich text: one labelled group, a fill group per colour run.
                let _ = writeln!(self.body, "<g aria-label=\"{label}\">");
                for chunk in uses.chunk_by(|a, b| a.0 == b.0) {
                    let _ = writeln!(self.body, "<g{}>", fill(chunk[0].0));
                    for (_, u) in chunk {
                        self.body.push_str(u);
                    }
                    self.body.push_str("</g>\n");
                }
            }
            self.body.push_str("</g>\n");
        }
        self.decorations(&shaped, rect, text.align, DecorationKind::Strike);
    }

    fn draw_image(&mut self, rect: Rect, src: &Path, fit: ImageFit) {
        let resolved = geometry::resolve_image(self.base_dir, src);
        if self.try_image(rect, &resolved, fit) {
            return;
        }
        tracing::warn!(path = %resolved.display(), "image unavailable; drawing placeholder");
        let (fill, border) = geometry::placeholder_colors();
        self.fill_rect(
            rect,
            Some(&Brush::Solid(fill)),
            Some(border),
            geometry::PLACEHOLDER_BORDER,
            0.0,
        );
    }

    fn draw_path(&mut self, path: &PathPaint) {
        if path.segments.len() < 2 {
            return;
        }
        if let Some(fill) = &path.fill {
            self.path_element(&path.segments, Some(fill), None);
        }
        let Some(stroke) = &path.stroke else {
            return;
        };
        // Shaft pulled back into the heads, then the heads filled in the
        // stroke color (geometry shared with every backend).
        let geo = path.stroke_geometry();
        self.path_element(&geo.shaft, None, Some(stroke));
        let head = Brush::Solid(stroke.color);
        for tri in &geo.heads {
            self.path_element(&geometry::polygon(tri), Some(&head), None);
        }
    }

    fn draw_ellipse(&mut self, rect: Rect, fill: Option<&Brush>, stroke: Option<&StrokeStyle>) {
        if rect.w <= 0.0 || rect.h <= 0.0 || (fill.is_none() && stroke.is_none()) {
            return;
        }
        let mut el = format!(
            "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"",
            num(rect.x + rect.w / 2.0),
            num(rect.y + rect.h / 2.0),
            num(rect.w / 2.0),
            num(rect.h / 2.0)
        );
        self.push_brush_paint(&mut el, fill, stroke);
        el.push_str("/>\n");
        self.body.push_str(&el);
    }
}

/// Render a computed layout to an SVG document (UTF-8 text).
pub fn render_svg(layout: &ComputedLayout, opts: &RenderOptions) -> String {
    let mut r = SvgRenderer::new(opts);
    paint(layout, &mut r);
    r.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn numbers_are_trimmed() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(1.25), "1.25");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(f32::NAN), "0");
        assert_eq!(fmt_decimals(12.0 / 2048.0, 7), "0.0058594");
    }
}
