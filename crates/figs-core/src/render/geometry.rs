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

use crate::geom::{Color, ImageFit, PathSegment, Point, Rect, TextAlign};
use crate::layout::TextBox;
use crate::schema::TextProps;

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
        PathSegment::CubicTo(p(rt - radius + k, t), p(rt, t + radius - k), p(rt, t + radius)),
        PathSegment::LineTo(p(rt, b - radius)),
        PathSegment::CubicTo(p(rt, b - radius + k), p(rt - radius + k, b), p(rt - radius, b)),
        PathSegment::LineTo(p(l + radius, b)),
        PathSegment::CubicTo(p(l + radius - k, b), p(l, b - radius + k), p(l, b - radius)),
        PathSegment::LineTo(p(l, t + radius)),
        PathSegment::CubicTo(p(l, t + radius - k), p(l + radius - k, t), p(l + radius, t)),
        PathSegment::Close,
    ]
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
/// box of width `box_w`, per the text alignment.
pub fn align_offset(align: TextAlign, line_w: f32, box_w: f32) -> f32 {
    match align {
        TextAlign::Left | TextAlign::Justify => 0.0,
        TextAlign::Center => ((box_w - line_w) / 2.0).max(0.0),
        TextAlign::Right => (box_w - line_w).max(0.0),
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
    }
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
            Rect::new(rect.x + (rect.w - w) / 2.0, rect.y + (rect.h - h) / 2.0, w, h)
        }
    }
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
        let segs = polygon(&[Point::new(0.0, 0.0), Point::new(1.0, 0.0), Point::new(0.0, 1.0)]);
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[3], PathSegment::Close);
    }
}
