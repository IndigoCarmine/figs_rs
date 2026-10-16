//! Backend-neutral drawing geometry shared by every renderer (PNG, PDF, SVG).
//!
//! Everything here works in points with a top-left origin, exactly like the
//! [`ComputedLayout`](crate::layout::ComputedLayout) IR. Backends only translate
//! the resulting [`PathSeg`] lists / rects into their own primitives, so
//! arrowheads, rounded corners, ellipses, text alignment and image fitting are
//! computed once and come out identical in every output format.

use std::path::{Path, PathBuf};

use crate::geom::{Color, ImageFit, Point, Rect, TextAlign};
use crate::layout::{PathPaint, TextBox};
use crate::schema::TextProps;

/// One segment of a vector outline. Coordinates are absolute points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSeg {
    MoveTo(Point),
    LineTo(Point),
    /// Cubic Bézier: two control points, then the end point.
    CubicTo(Point, Point, Point),
    Close,
}

/// Cubic control-point factor for a quarter-circle approximation.
pub const KAPPA: f32 = 0.552_284_8;

/// Outline of a polyline/polygon through `pts`. This is the single place a
/// point list becomes path segments, so richer segment kinds (curves) only
/// need to be taught here.
pub fn polyline(pts: &[Point], closed: bool) -> Vec<PathSeg> {
    let Some((first, rest)) = pts.split_first() else {
        return Vec::new();
    };
    let mut segs = Vec::with_capacity(pts.len() + 1);
    segs.push(PathSeg::MoveTo(*first));
    segs.extend(rest.iter().map(|p| PathSeg::LineTo(*p)));
    if closed {
        segs.push(PathSeg::Close);
    }
    segs
}

/// Outline of `r` with corners rounded by `radius`, clamped to half the
/// shorter side so corners never overlap. A non-positive radius gives a plain
/// rectangle.
pub fn rounded_rect(r: Rect, radius: f32) -> Vec<PathSeg> {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
    let (l, t, rt, b) = (r.x, r.y, r.x + r.w, r.y + r.h);
    let p = Point::new;
    if radius <= 0.0 {
        return vec![
            PathSeg::MoveTo(p(l, t)),
            PathSeg::LineTo(p(rt, t)),
            PathSeg::LineTo(p(rt, b)),
            PathSeg::LineTo(p(l, b)),
            PathSeg::Close,
        ];
    }
    let k = radius * KAPPA;
    vec![
        PathSeg::MoveTo(p(l + radius, t)),
        PathSeg::LineTo(p(rt - radius, t)),
        PathSeg::CubicTo(p(rt - radius + k, t), p(rt, t + radius - k), p(rt, t + radius)),
        PathSeg::LineTo(p(rt, b - radius)),
        PathSeg::CubicTo(p(rt, b - radius + k), p(rt - radius + k, b), p(rt - radius, b)),
        PathSeg::LineTo(p(l + radius, b)),
        PathSeg::CubicTo(p(l + radius - k, b), p(l, b - radius + k), p(l, b - radius)),
        PathSeg::LineTo(p(l, t + radius)),
        PathSeg::CubicTo(p(l, t + radius - k), p(l + radius - k, t), p(l + radius, t)),
        PathSeg::Close,
    ]
}

/// Outline of the ellipse inscribed in `r` (four cubic quarter arcs, starting
/// at the right-most point and running clockwise on screen).
pub fn ellipse(r: Rect) -> Vec<PathSeg> {
    let (rx, ry) = (r.w / 2.0, r.h / 2.0);
    let (cx, cy) = (r.x + rx, r.y + ry);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let p = Point::new;
    vec![
        PathSeg::MoveTo(p(cx + rx, cy)),
        PathSeg::CubicTo(p(cx + rx, cy + ky), p(cx + kx, cy + ry), p(cx, cy + ry)),
        PathSeg::CubicTo(p(cx - kx, cy + ry), p(cx - rx, cy + ky), p(cx - rx, cy)),
        PathSeg::CubicTo(p(cx - rx, cy - ky), p(cx - kx, cy - ry), p(cx, cy - ry)),
        PathSeg::CubicTo(p(cx + kx, cy - ry), p(cx + rx, cy - ky), p(cx + rx, cy)),
        PathSeg::Close,
    ]
}

/// The stroked shaft of a path plus its filled arrowhead triangles.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrowGeometry {
    /// Points to stroke (the path's points, with arrowed ends pulled back).
    pub shaft: Vec<Point>,
    /// Arrowhead triangles, filled with the stroke color.
    pub heads: Vec<[Point; 3]>,
}

/// Split a path into its shaft and arrowheads. The shaft is pulled back into
/// each head (by half its length) so a butt/square/round cap never pokes out
/// past the tip. Closed paths and `head_size <= 0` have no heads.
pub fn arrow_geometry(path: &PathPaint) -> ArrowGeometry {
    let mut shaft = path.points.clone();
    let mut heads = Vec::new();
    if !path.closed && path.head_size > 0.0 && shaft.len() >= 2 {
        let n = shaft.len();
        if path.head.at_end() {
            if let Some(h) = arrow_head(&path.points, n - 1, -1, path.head_size) {
                shaft[n - 1] = h.shaft_end;
                heads.push(h.triangle);
            }
        }
        if path.head.at_start() {
            if let Some(h) = arrow_head(&path.points, 0, 1, path.head_size) {
                shaft[0] = h.shaft_end;
                heads.push(h.triangle);
            }
        }
    }
    ArrowGeometry { shaft, heads }
}

/// An arrowhead triangle plus where the shaft should now end.
struct Head {
    triangle: [Point; 3],
    shaft_end: Point,
}

/// Arrowhead of length `len` at `pts[tip]`, pointing away from the nearest
/// distinct neighbour found by walking `step` (+1/-1). Its base is 0.8x `len`
/// wide. `None` when every other point coincides with the tip.
fn arrow_head(pts: &[Point], tip: usize, step: isize, len: f32) -> Option<Head> {
    let t = pts[tip];
    let mut i = tip as isize + step;
    let (ux, uy, seg) = loop {
        let p = *pts.get(usize::try_from(i).ok()?)?;
        let (dx, dy) = (t.x - p.x, t.y - p.y);
        let d = (dx * dx + dy * dy).sqrt();
        if d > 1e-6 {
            break (dx / d, dy / d, d);
        }
        i += step;
    };
    let base = Point::new(t.x - ux * len, t.y - uy * len);
    let half = 0.4 * len;
    let back = (0.5 * len).min(seg);
    Some(Head {
        triangle: [
            t,
            Point::new(base.x - uy * half, base.y + ux * half),
            Point::new(base.x + uy * half, base.y - ux * half),
        ],
        shaft_end: Point::new(t.x - ux * back, t.y - uy * back),
    })
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
    use crate::geom::{ArrowHead, Color, LineCap, StrokeStyle};

    fn arrow(points: Vec<Point>, head: ArrowHead) -> PathPaint {
        PathPaint {
            points,
            closed: false,
            fill: None,
            stroke: Some(StrokeStyle {
                color: Color::BLACK,
                width: 1.0,
                dash: vec![],
                cap: LineCap::Butt,
            }),
            head,
            head_size: 10.0,
        }
    }

    #[test]
    fn end_arrowhead_points_along_the_shaft() {
        let g = arrow_geometry(&arrow(
            vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
            ArrowHead::End,
        ));
        assert_eq!(g.heads.len(), 1);
        let [tip, a, b] = g.heads[0];
        assert_eq!(tip, Point::new(100.0, 0.0));
        assert_eq!(a, Point::new(90.0, 4.0));
        assert_eq!(b, Point::new(90.0, -4.0));
        assert_eq!(g.shaft[1], Point::new(95.0, 0.0));
    }

    #[test]
    fn closed_paths_have_no_heads() {
        let mut p = arrow(
            vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(0.0, 10.0)],
            ArrowHead::Both,
        );
        p.closed = true;
        assert!(arrow_geometry(&p).heads.is_empty());
    }

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
        assert_eq!(segs[0], PathSeg::MoveTo(Point::new(2.0, 0.0)));
    }
}
