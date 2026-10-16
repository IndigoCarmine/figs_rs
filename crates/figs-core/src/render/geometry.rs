//! Backend-neutral drawing geometry shared by every renderer (PNG, PDF, SVG).
//!
//! Everything here works in points with a top-left origin, exactly like the
//! [`ComputedLayout`](crate::layout::ComputedLayout) IR. Backends only translate
//! the resulting [`PathSegment`] lists / rects into their own primitives, so
//! rounded corners, ellipses, text alignment and image fitting are computed
//! once and come out identical in every output format. (Path/arrowhead
//! geometry lives in [`crate::path`], via [`PathPaint::stroke_geometry`].)
//!
//! [`PathPaint::stroke_geometry`]: crate::layout::PathPaint::stroke_geometry

use std::path::{Path, PathBuf};

use crate::geom::{Color, ImageFit, PathSegment, Point, Rect, StrokeAlign, TextAlign};
use crate::layout::TextBox;
use crate::schema::TextProps;
use crate::text::{DecorationKind, ShapedText};

/// Cubic control-point factor for a quarter-circle approximation.
pub const KAPPA: f32 = 0.552_284_8;

/// Closed polygon through `pts` (e.g. an arrowhead triangle).
pub fn polygon(pts: &[Point]) -> Vec<PathSegment> {
    let Some((first, rest)) = pts.split_first() else {
        return Vec::new();
    };
    let mut segs = Vec::with_capacity(pts.len() + 1);
    segs.push(PathSegment::MoveTo(*first));
    segs.extend(rest.iter().map(|p| PathSegment::LineTo(*p)));
    segs.push(PathSegment::Close);
    segs
}

/// Open polyline through `pts` (e.g. an open arrowhead chevron).
pub fn polyline(pts: &[Point]) -> Vec<PathSegment> {
    let Some((first, rest)) = pts.split_first() else {
        return Vec::new();
    };
    let mut segs = Vec::with_capacity(pts.len());
    segs.push(PathSegment::MoveTo(*first));
    segs.extend(rest.iter().map(|p| PathSegment::LineTo(*p)));
    segs
}

/// Outline of `r` with corners rounded by `radius`, clamped to half the
/// shorter side so corners never overlap. A non-positive radius gives a plain
/// rectangle.
pub fn rounded_rect(r: Rect, radius: f32) -> Vec<PathSegment> {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
    let (l, t, rt, b) = (r.x, r.y, r.x + r.w, r.y + r.h);
    let p = Point::new;
    if radius <= 0.0 {
        return vec![
            PathSegment::MoveTo(p(l, t)),
            PathSegment::LineTo(p(rt, t)),
            PathSegment::LineTo(p(rt, b)),
            PathSegment::LineTo(p(l, b)),
            PathSegment::Close,
        ];
    }
    let k = radius * KAPPA;
    vec![
        PathSegment::MoveTo(p(l + radius, t)),
        PathSegment::LineTo(p(rt - radius, t)),
        PathSegment::CubicTo(
            p(rt - radius + k, t),
            p(rt, t + radius - k),
            p(rt, t + radius),
        ),
        PathSegment::LineTo(p(rt, b - radius)),
        PathSegment::CubicTo(
            p(rt, b - radius + k),
            p(rt - radius + k, b),
            p(rt - radius, b),
        ),
        PathSegment::LineTo(p(l + radius, b)),
        PathSegment::CubicTo(p(l + radius - k, b), p(l, b - radius + k), p(l, b - radius)),
        PathSegment::LineTo(p(l, t + radius)),
        PathSegment::CubicTo(p(l, t + radius - k), p(l + radius - k, t), p(l + radius, t)),
        PathSegment::Close,
    ]
}

/// The centre line of a `width`-wide stroke on the (rounded) box `rect`
/// with corner `radius`, placed per `align`: the box grown by
/// [`StrokeAlign::centre_offset`] (shrunk for `inside`), with a rounded
/// radius grown/shrunk by the same amount (a square corner stays square, as
/// the miter of its offset). `None` when an inside stroke is wider than the
/// box. Also the box to stroke for an ellipse (ignore the radius).
pub fn aligned_rect(
    rect: Rect,
    radius: f32,
    align: StrokeAlign,
    width: f32,
) -> Option<(Rect, f32)> {
    let d = align.centre_offset(width);
    let line = Rect::new(rect.x - d, rect.y - d, rect.w + 2.0 * d, rect.h + 2.0 * d);
    if line.w < 0.0 || line.h < 0.0 {
        return None;
    }
    let radius = if radius > 0.0 {
        (radius + d).max(0.0)
    } else {
        0.0
    };
    Some((line, radius))
}

/// Largest miter scale [`offset_outline`] uses at a sharp corner (the
/// stroke's own miter limit, as in SVG): beyond it the corner point moves
/// along the bisector only this far, in units of the offset.
const OFFSET_MITER_LIMIT: f32 = 4.0;

/// A closed outline (`MoveTo`, lines / quadratic / cubic Béziers, `Close`)
/// offset by `d` points **outwards** (negative: inwards), with mitered
/// corners — the centre line of an inside/outside-aligned stroke.
///
/// Every on-curve point moves along the miter of the outline's tangents
/// there (its plain normal where the outline is smooth), which is exact for
/// straight edges and their corners. A curve keeps its tangent directions
/// and has its handles scaled by `1 + d·κ` (κ the curvature at that end,
/// positive where it bends inwards), which is exact for circular arcs and
/// close for gentle curves; quadratics are raised to cubics first.
/// "Outwards" is decided by the winding, so both orientations work. Open
/// and degenerate outlines come back unchanged.
pub fn offset_outline(segs: &[PathSegment], d: f32) -> Vec<PathSegment> {
    let closed = matches!(segs.last(), Some(PathSegment::Close));
    if !closed || d == 0.0 || !d.is_finite() {
        return segs.to_vec();
    }
    // Quadratics as (exact) cubics, so every curve is a cubic.
    let mut cur = Point::default();
    let segs: Vec<PathSegment> = segs
        .iter()
        .map(|s| match *s {
            PathSegment::QuadTo(c, p) => {
                let toward = |a: Point| {
                    Point::new(a.x + (c.x - a.x) * 2.0 / 3.0, a.y + (c.y - a.y) * 2.0 / 3.0)
                };
                let cubic = PathSegment::CubicTo(toward(cur), toward(p), p);
                cur = p;
                cubic
            }
            PathSegment::MoveTo(p) | PathSegment::LineTo(p) | PathSegment::CubicTo(_, _, p) => {
                cur = p;
                *s
            }
            PathSegment::Close => *s,
        })
        .collect();
    // The control polygon, in order.
    let mut pts: Vec<Point> = Vec::with_capacity(segs.len() * 3);
    for s in &segs {
        match *s {
            PathSegment::MoveTo(p) | PathSegment::LineTo(p) => pts.push(p),
            PathSegment::QuadTo(c, p) => pts.extend([c, p]),
            PathSegment::CubicTo(c1, c2, p) => pts.extend([c1, c2, p]),
            PathSegment::Close => {}
        }
    }
    let n = pts.len();
    if n < 3 {
        return segs;
    }
    // Shoelace: positive = clockwise on the page (y down), where the
    // right-hand normal (dy, -dx) of a direction points outwards.
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    if area.abs() < 1e-9 {
        return segs;
    }
    let sign = area.signum();
    let outward = |dx: f32, dy: f32| -> Option<(f32, f32)> {
        let len = (dx * dx + dy * dy).sqrt();
        (len > 1e-6).then(|| (sign * dy / len, -sign * dx / len))
    };
    // Outward unit normal of control leg i (pts[i] -> pts[i + 1]); None if
    // degenerate.
    let normals: Vec<Option<(f32, f32)>> = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            outward(b.x - a.x, b.y - a.y)
        })
        .collect();
    if normals.iter().all(Option::is_none) {
        return segs;
    }
    // The nearest non-degenerate legs ending at / starting from point i are
    // the outline's tangents there; on-curve point i moves along their miter.
    let incoming = |i: usize| (1..=n).find_map(|k| normals[(i + n - k) % n]);
    let outgoing = |i: usize| (0..n).find_map(|k| normals[(i + k) % n]);
    let miter = |i: usize| -> Point {
        let (a, b) = (
            incoming(i).unwrap_or_default(),
            outgoing(i).unwrap_or_default(),
        );
        let dot = a.0 * b.0 + a.1 * b.1;
        let (mut mx, mut my) = if 1.0 + dot > 1e-4 {
            ((a.0 + b.0) / (1.0 + dot), (a.1 + b.1) / (1.0 + dot))
        } else {
            b // a U-turn: no miter, move along the outgoing leg's normal
        };
        let scale = (mx * mx + my * my).sqrt();
        if scale > OFFSET_MITER_LIMIT {
            mx *= OFFSET_MITER_LIMIT / scale;
            my *= OFFSET_MITER_LIMIT / scale;
        }
        Point::new(pts[i].x + d * mx, pts[i].y + d * my)
    };
    // Handle-length factor 1 + d·κ at a curve end with forward tangent
    // `t` (= B'/3) and second derivative `dd` (= B''): κ = -(B''·n) / |B'|²
    // along the outward normal n (positive where the curve bends inwards).
    let scale = |t: (f32, f32), dd: (f32, f32)| -> f32 {
        let Some(nrm) = outward(t.0, t.1) else {
            return 1.0;
        };
        let k = -(dd.0 * nrm.0 + dd.1 * nrm.1) / (9.0 * (t.0 * t.0 + t.1 * t.1));
        (1.0 + d * k).max(0.0)
    };
    let mut out = Vec::with_capacity(segs.len());
    let mut i = 0usize; // index in `pts` of the current on-curve point
    let mut prev = (pts[0], miter(0)); // that point: original, moved
    for s in &segs {
        match *s {
            PathSegment::MoveTo(_) => out.push(PathSegment::MoveTo(prev.1)),
            PathSegment::LineTo(p) => {
                i += 1;
                let q = miter(i);
                out.push(PathSegment::LineTo(q));
                prev = (p, q);
            }
            PathSegment::CubicTo(c1, c2, p3) => {
                i += 3;
                let (p0, q0) = prev;
                let q3 = miter(i);
                let s0 = scale(
                    (c1.x - p0.x, c1.y - p0.y),
                    (
                        6.0 * (c2.x - 2.0 * c1.x + p0.x),
                        6.0 * (c2.y - 2.0 * c1.y + p0.y),
                    ),
                );
                let s3 = scale(
                    (p3.x - c2.x, p3.y - c2.y),
                    (
                        6.0 * (p3.x - 2.0 * c2.x + c1.x),
                        6.0 * (p3.y - 2.0 * c2.y + c1.y),
                    ),
                );
                let q1 = Point::new(q0.x + (c1.x - p0.x) * s0, q0.y + (c1.y - p0.y) * s0);
                let q2 = Point::new(q3.x + (c2.x - p3.x) * s3, q3.y + (c2.y - p3.y) * s3);
                out.push(PathSegment::CubicTo(q1, q2, q3));
                prev = (p3, q3);
            }
            // Raised to cubics above.
            PathSegment::QuadTo(..) => {}
            PathSegment::Close => out.push(PathSegment::Close),
        }
    }
    out
}

/// Outline of the ellipse inscribed in `r` (four cubic quarter arcs, starting
/// at the right-most point and running clockwise on screen).
pub fn ellipse(r: Rect) -> Vec<PathSegment> {
    let (rx, ry) = (r.w / 2.0, r.h / 2.0);
    let (cx, cy) = (r.x + rx, r.y + ry);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let p = Point::new;
    vec![
        PathSegment::MoveTo(p(cx + rx, cy)),
        PathSegment::CubicTo(p(cx + rx, cy + ky), p(cx + kx, cy + ry), p(cx, cy + ry)),
        PathSegment::CubicTo(p(cx - kx, cy + ry), p(cx - rx, cy + ky), p(cx - rx, cy)),
        PathSegment::CubicTo(p(cx - rx, cy - ky), p(cx - kx, cy - ry), p(cx, cy - ry)),
        PathSegment::CubicTo(p(cx + kx, cy - ry), p(cx + rx, cy - ky), p(cx + rx, cy)),
        PathSegment::Close,
    ]
}

/// Horizontal offset (points) to apply to a line of the given width inside a
/// box of width `box_w`, per the text alignment. A line wider than the box
/// (unwrapped text, `wrap = false`) overflows it on the side(s) its
/// alignment points away from: both sides equally when centred, the left
/// when right-aligned, the right otherwise.
pub fn align_offset(align: TextAlign, line_w: f32, box_w: f32) -> f32 {
    match align {
        TextAlign::Left | TextAlign::Justify => 0.0,
        TextAlign::Center => (box_w - line_w) / 2.0,
        TextAlign::Right => box_w - line_w,
    }
}

/// The shaping input for a painted text box (same props layout measured with).
pub fn text_props(text: &TextBox) -> TextProps {
    TextProps {
        content: text.content.clone(),
        font_size: text.font_size,
        font_family: text.font_family.clone(),
        font_weight: text.font_weight,
        color: text.color,
        align: text.align,
        line_height: text.line_height,
        stroke: text.stroke.clone(),
        font_style: text.font_style,
        underline: text.underline,
        strike: text.strike,
        letter_spacing: text.letter_spacing,
        spans: text.spans.clone(),
        wrap: text.wrap,
        fit_width: text.fit_width,
        typography: text.typography,
    }
}

/// The `kind` bars (underline / line-through) of `shaped` placed in a text
/// box at `rect` (page points): one filled rectangle per decoration, with its
/// colour. Backends fill underlines before the glyphs and line-throughs after.
pub fn decoration_rects(
    shaped: &ShapedText,
    rect: Rect,
    align: TextAlign,
    kind: DecorationKind,
) -> Vec<(Rect, Color)> {
    shaped
        .decorations
        .iter()
        .filter(|d| d.kind == kind)
        .filter_map(|d| {
            let line = shaped.lines.get(d.line)?;
            let off = align_offset(align, line.width, rect.w);
            let r = Rect::new(
                rect.x + off + d.x,
                rect.y + d.y - d.thickness / 2.0,
                d.width,
                d.thickness,
            );
            (r.w > 0.0 && r.h > 0.0).then_some((r, d.color))
        })
        .collect()
}

/// `tan` of the default synthetic-italic slant (14°, swash's raster fake
/// italic). Glyphs carry their own [`crate::text::ShapedGlyph::skew`], which
/// differs when a page or node sets `synthetic_slant`.
pub fn italic_shear() -> f32 {
    crate::schema::span::SYNTHETIC_ITALIC_DEG.to_radians().tan()
}

/// Where an image of `iw`×`ih` pixels lands for a node `rect` under `fit`.
/// For [`ImageFit::Cover`] the result overflows `rect` and must be clipped to
/// it; the other modes stay inside.
pub fn image_placement(rect: Rect, iw: f32, ih: f32, fit: ImageFit) -> Rect {
    match fit {
        ImageFit::Fill => rect,
        ImageFit::Contain | ImageFit::Cover => {
            let (sx, sy) = (rect.w / iw, rect.h / ih);
            let s = if fit == ImageFit::Cover {
                sx.max(sy)
            } else {
                sx.min(sy)
            };
            let (w, h) = (iw * s, ih * s);
            Rect::new(
                rect.x + (rect.w - w) / 2.0,
                rect.y + (rect.h - h) / 2.0,
                w,
                h,
            )
        }
    }
}

/// Resolution an image drawn with `smooth = false` is embedded at in the
/// vector formats (pixels per inch at its drawn size).
pub const PIXELATED_DPI: f32 = 300.0;

/// For a `smooth = false` image of `iw`×`ih` pixels drawn at `dest`
/// (points): the whole factor to upscale it by, nearest-neighbour, before
/// embedding in PDF/SVG, so it reaches about [`PIXELATED_DPI`] and stays
/// blocky whatever interpolation a viewer applies. `None` when smooth, or
/// when the image is already that fine. Capped at 8192 px a side.
pub fn pixelated_factor(smooth: bool, iw: f32, ih: f32, dest: Rect) -> Option<u32> {
    if smooth || iw <= 0.0 || ih <= 0.0 {
        return None;
    }
    let want = PIXELATED_DPI / crate::units::PT_PER_INCH; // px per pt
    let k = (want * dest.w / iw).max(want * dest.h / ih).ceil();
    let cap = (8192.0 / iw.max(ih)).floor();
    let k = k.min(cap).min(64.0);
    (k >= 2.0).then_some(k as u32)
}

/// `img` scaled up `k` times, each pixel becoming a `k`×`k` block.
pub fn upscale_nearest(img: &image::RgbaImage, k: u32) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    image::imageops::resize(img, w * k, h * k, image::imageops::FilterType::Nearest)
}

/// Resolve an image `src` against the render base directory.
pub fn resolve_image(base_dir: &Path, src: &Path) -> PathBuf {
    if src.is_absolute() {
        src.to_path_buf()
    } else {
        base_dir.join(src)
    }
}

/// Fill and border colors of the placeholder box drawn for a missing image.
pub fn placeholder_colors() -> (Color, Color) {
    let fill = Color::parse_hex("#e7e4dd").unwrap_or(Color::WHITE);
    let border = Color::parse_hex("#b6b2a7").unwrap_or(Color::BLACK);
    (fill, border)
}

/// Border width (points) of the missing-image placeholder.
pub const PLACEHOLDER_BORDER: f32 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_overflows_and_contain_fits() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        let cover = image_placement(r, 10.0, 10.0, ImageFit::Cover);
        assert_eq!(cover, Rect::new(0.0, -25.0, 100.0, 100.0));
        let contain = image_placement(r, 10.0, 10.0, ImageFit::Contain);
        assert_eq!(contain, Rect::new(25.0, 0.0, 50.0, 50.0));
    }

    #[test]
    fn radius_is_clamped() {
        let segs = rounded_rect(Rect::new(0.0, 0.0, 10.0, 4.0), 100.0);
        assert_eq!(segs[0], PathSegment::MoveTo(Point::new(2.0, 0.0)));
    }

    #[test]
    fn polygon_is_closed() {
        let segs = polygon(&[
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ]);
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[3], PathSegment::Close);
    }
}
