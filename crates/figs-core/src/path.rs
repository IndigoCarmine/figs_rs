//! Backend-neutral geometry for [`PathSegment`] lists: arc → cubic conversion,
//! exact bounds, and arrowhead/shaft construction. Renderers only ever see
//! `MoveTo` / `LineTo` / `QuadTo` / `CubicTo` / `Close`; everything that needs
//! curve math (so every backend draws the same thing) lives here.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::geom::{ArrowHead, HeadStyle, PathSegment, Point};

const EPS: f32 = 1e-6;

fn lerp(a: Point, b: Point, t: f32) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn dist(a: Point, b: Point) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

// ---------------------------------------------------------------------------
// Arcs
// ---------------------------------------------------------------------------

/// Convert an SVG-style endpoint arc from `from` to `to` into cubic Béziers
/// (one per ≤ 90° piece, so the radial error stays below ~0.03 %).
///
/// `rx`/`ry` are the ellipse radii, `rotation_deg` the rotation of its x axis
/// (degrees, clockwise on a y-down page), `large` selects the arc spanning
/// more than 180° and `clockwise` the turning direction on the page (SVG's
/// `sweep-flag = 1`). Radii too small to reach are scaled up uniformly (SVG
/// F.6.6). Coincident endpoints yield no segment; a zero radius yields a
/// straight line (both as in SVG).
pub fn arc_to_cubics(
    from: Point,
    to: Point,
    rx: f32,
    ry: f32,
    rotation_deg: f32,
    large: bool,
    clockwise: bool,
) -> Vec<PathSegment> {
    if dist(from, to) <= EPS {
        return Vec::new();
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx <= EPS || ry <= EPS {
        return vec![PathSegment::LineTo(to)];
    }
    let phi = rotation_deg.to_radians();
    let (sin_p, cos_p) = phi.sin_cos();

    // F.6.5.1: the start point in the ellipse's (un-rotated) frame, relative
    // to the chord midpoint.
    let hx = (from.x - to.x) / 2.0;
    let hy = (from.y - to.y) / 2.0;
    let x1 = cos_p * hx + sin_p * hy;
    let y1 = -sin_p * hx + cos_p * hy;

    // F.6.6: scale radii up if the ellipse can't span the chord.
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    // F.6.5.2: center in the ellipse frame.
    let (rx2, ry2) = (rx * rx, ry * ry);
    let num = rx2 * ry2 - rx2 * y1 * y1 - ry2 * x1 * x1;
    let den = rx2 * y1 * y1 + ry2 * x1 * x1;
    let mut coef = if den > 0.0 {
        (num / den).max(0.0).sqrt()
    } else {
        0.0
    };
    if large == clockwise {
        coef = -coef;
    }
    let cxp = coef * rx * y1 / ry;
    let cyp = -coef * ry * x1 / rx;

    // F.6.5.3: center on the page.
    let cx = cos_p * cxp - sin_p * cyp + (from.x + to.x) / 2.0;
    let cy = sin_p * cxp + cos_p * cyp + (from.y + to.y) / 2.0;

    // F.6.5.5/6: start angle and sweep on the unit circle.
    let angle = |ux: f32, uy: f32| uy.atan2(ux);
    let theta1 = angle((x1 - cxp) / rx, (y1 - cyp) / ry);
    let theta2 = angle((-x1 - cxp) / rx, (-y1 - cyp) / ry);
    let mut delta = (theta2 - theta1) % TAU;
    if clockwise && delta < 0.0 {
        delta += TAU;
    } else if !clockwise && delta > 0.0 {
        delta -= TAU;
    }

    // Unit-circle point -> page point.
    let map = |ux: f32, uy: f32| {
        let (x, y) = (rx * ux, ry * uy);
        Point::new(cos_p * x - sin_p * y + cx, sin_p * x + cos_p * y + cy)
    };

    let n = ((delta.abs() / FRAC_PI_2) - 1e-4).ceil().max(1.0) as usize;
    let step = delta / n as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut out = Vec::with_capacity(n);
    let mut a = theta1;
    for i in 0..n {
        let b = a + step;
        let (sa, ca) = a.sin_cos();
        let (sb, cb) = b.sin_cos();
        let c1 = map(ca - k * sa, sa + k * ca);
        let c2 = map(cb + k * sb, sb - k * cb);
        let end = if i + 1 == n { to } else { map(cb, sb) };
        out.push(PathSegment::CubicTo(c1, c2, end));
        a = b;
    }
    out
}

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// Exact axis-aligned bounds `(min, max)` of the drawn outline (Bézier
/// extrema included, control points that the curve doesn't reach excluded).
/// Ignores stroke width and arrowheads. `None` for an empty list.
pub fn bounds(segments: &[PathSegment]) -> Option<(Point, Point)> {
    let mut lo = Point::new(f32::INFINITY, f32::INFINITY);
    let mut hi = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut add = |p: Point| {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    };
    for piece in pieces(segments) {
        add(piece.start());
        add(piece.end());
        for t in piece.extrema() {
            add(piece.eval(t));
        }
    }
    // A lone MoveTo still has a position.
    if let Some(PathSegment::MoveTo(p)) = segments.first() {
        add(*p);
    }
    lo.x.is_finite().then_some((lo, hi))
}

// ---------------------------------------------------------------------------
// Pieces: segments with their start point attached
// ---------------------------------------------------------------------------

/// A drawable segment including its start point.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Piece {
    Line(Point, Point),
    Quad(Point, Point, Point),
    Cubic(Point, Point, Point, Point),
}

impl Piece {
    fn start(&self) -> Point {
        match *self {
            Piece::Line(a, _) | Piece::Quad(a, _, _) | Piece::Cubic(a, _, _, _) => a,
        }
    }
    fn end(&self) -> Point {
        match *self {
            Piece::Line(_, b) | Piece::Quad(_, _, b) | Piece::Cubic(_, _, _, b) => b,
        }
    }
    /// Control polygon, start to end.
    fn points(&self) -> Vec<Point> {
        match *self {
            Piece::Line(a, b) => vec![a, b],
            Piece::Quad(a, c, b) => vec![a, c, b],
            Piece::Cubic(a, c1, c2, b) => vec![a, c1, c2, b],
        }
    }
    fn eval(&self, t: f32) -> Point {
        match *self {
            Piece::Line(a, b) => lerp(a, b, t),
            Piece::Quad(a, c, b) => lerp(lerp(a, c, t), lerp(c, b, t), t),
            Piece::Cubic(a, c1, c2, b) => {
                let (p, q, r) = (lerp(a, c1, t), lerp(c1, c2, t), lerp(c2, b, t));
                lerp(lerp(p, q, t), lerp(q, r, t), t)
            }
        }
    }
    /// The part over `[0, t]` (de Casteljau).
    fn head_part(&self, t: f32) -> Piece {
        match *self {
            Piece::Line(a, b) => Piece::Line(a, lerp(a, b, t)),
            Piece::Quad(a, c, b) => {
                let ac = lerp(a, c, t);
                Piece::Quad(a, ac, lerp(ac, lerp(c, b, t), t))
            }
            Piece::Cubic(a, c1, c2, b) => {
                let (p, q, r) = (lerp(a, c1, t), lerp(c1, c2, t), lerp(c2, b, t));
                let (pq, qr) = (lerp(p, q, t), lerp(q, r, t));
                Piece::Cubic(a, p, pq, lerp(pq, qr, t))
            }
        }
    }
    fn reversed(&self) -> Piece {
        match *self {
            Piece::Line(a, b) => Piece::Line(b, a),
            Piece::Quad(a, c, b) => Piece::Quad(b, c, a),
            Piece::Cubic(a, c1, c2, b) => Piece::Cubic(b, c2, c1, a),
        }
    }
    fn segment(&self) -> PathSegment {
        match *self {
            Piece::Line(_, b) => PathSegment::LineTo(b),
            Piece::Quad(_, c, b) => PathSegment::QuadTo(c, b),
            Piece::Cubic(_, c1, c2, b) => PathSegment::CubicTo(c1, c2, b),
        }
    }
    /// Parameters in (0, 1) where x or y has a local extremum.
    fn extrema(&self) -> Vec<f32> {
        let mut ts = Vec::new();
        match *self {
            Piece::Line(..) => {}
            Piece::Quad(a, c, b) => {
                for (p0, p1, p2) in [(a.x, c.x, b.x), (a.y, c.y, b.y)] {
                    let den = p0 - 2.0 * p1 + p2;
                    if den.abs() > EPS {
                        ts.push((p0 - p1) / den);
                    }
                }
            }
            Piece::Cubic(a, c1, c2, b) => {
                for (p0, p1, p2, p3) in [(a.x, c1.x, c2.x, b.x), (a.y, c1.y, c2.y, b.y)] {
                    // B'(t)/3 = A t^2 + B t + C.
                    let (d0, d1, d2) = (p1 - p0, p2 - p1, p3 - p2);
                    let qa = d0 - 2.0 * d1 + d2;
                    let qb = 2.0 * (d1 - d0);
                    let qc = d0;
                    if qa.abs() <= EPS {
                        if qb.abs() > EPS {
                            ts.push(-qc / qb);
                        }
                    } else {
                        let disc = qb * qb - 4.0 * qa * qc;
                        if disc >= 0.0 {
                            let s = disc.sqrt();
                            ts.push((-qb + s) / (2.0 * qa));
                            ts.push((-qb - s) / (2.0 * qa));
                        }
                    }
                }
            }
        }
        ts.retain(|t| *t > 0.0 && *t < 1.0);
        ts
    }
}

/// Segments -> pieces (a `Close` becomes a line back to the start).
fn pieces(segments: &[PathSegment]) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut start = None;
    let mut cur = None;
    for s in segments {
        match (*s, cur) {
            (PathSegment::MoveTo(p), _) => {
                start = Some(p);
                cur = Some(p);
                continue;
            }
            (PathSegment::LineTo(p), Some(c)) => out.push(Piece::Line(c, p)),
            (PathSegment::QuadTo(q, p), Some(c)) => out.push(Piece::Quad(c, q, p)),
            (PathSegment::CubicTo(c1, c2, p), Some(c)) => out.push(Piece::Cubic(c, c1, c2, p)),
            (PathSegment::Close, Some(c)) => {
                if let Some(s0) = start {
                    out.push(Piece::Line(c, s0));
                }
                cur = start;
                continue;
            }
            (_, None) => continue,
        }
        cur = s.end();
    }
    out
}

// ---------------------------------------------------------------------------
// Arrowheads
// ---------------------------------------------------------------------------

/// The shape of the arrowheads of a path: style, length (tip to base) and
/// base width, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadShape {
    pub style: HeadStyle,
    pub length: f32,
    pub width: f32,
}

impl HeadShape {
    /// The classic head: a filled triangle `length` long and `0.8 × length`
    /// wide.
    pub fn triangle(length: f32) -> Self {
        HeadShape {
            style: HeadStyle::Triangle,
            length,
            width: 0.8 * length,
        }
    }
}

/// What to stroke and fill for an open path with arrowheads.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeGeometry {
    /// The path to stroke: the input with each headed end pulled back into
    /// its head (along the curve) so no line cap pokes out past the tip.
    pub shaft: Vec<PathSegment>,
    /// Filled head polygons (closed; paint them in the stroke color, solid
    /// even when the stroke is dashed): a triangle `[tip, base corner, base
    /// corner]`, a stealth head `[tip, corner, notch, corner]`, or a barb
    /// `[tip, outer corner, inner base]`.
    pub heads: Vec<Vec<Point>>,
    /// Open heads `[corner, tip, corner]`: polylines to stroke in the stroke
    /// color and width (solid, mitered).
    pub open_heads: Vec<[Point; 3]>,
}

/// Split a path into its stroked shaft and arrowheads. Each head points along
/// the path's tangent at that end (towards the tip from the nearest distinct
/// control point) and is `shape.length` long and `shape.width` wide at its
/// base (see [`HeadStyle`]); `stroke_width` sizes what the shaft needs (a
/// barb's inner edge, an open head's miter). Closed paths, `ArrowHead::None`
/// and non-positive lengths return the path unchanged with no heads.
pub fn stroke_geometry(
    segments: &[PathSegment],
    head: ArrowHead,
    shape: HeadShape,
    stroke_width: f32,
) -> StrokeGeometry {
    let unchanged = || StrokeGeometry {
        shaft: segments.to_vec(),
        heads: Vec::new(),
        open_heads: Vec::new(),
    };
    let closed = segments.iter().any(|s| matches!(s, PathSegment::Close));
    let mut ps = pieces(segments);
    if closed || head == ArrowHead::None || shape.length <= 0.0 || ps.is_empty() {
        return unchanged();
    }
    let mut heads = Vec::new();
    let mut open_heads = Vec::new();
    let mut add = |part: Option<HeadPart>| match part {
        Some(HeadPart::Filled(poly)) => heads.push(poly),
        Some(HeadPart::Open(v)) => open_heads.push(v),
        None => {}
    };
    if head.at_end() {
        add(head_at_end(&mut ps, shape, stroke_width));
    }
    if head.at_start() {
        let mut rev: Vec<Piece> = ps.iter().rev().map(Piece::reversed).collect();
        add(head_at_end(&mut rev, shape, stroke_width));
        ps = rev.iter().rev().map(Piece::reversed).collect();
    }
    let Some(first) = ps.first() else {
        return unchanged();
    };
    let mut shaft = Vec::with_capacity(ps.len() + 1);
    shaft.push(PathSegment::MoveTo(first.start()));
    shaft.extend(ps.iter().map(Piece::segment));
    StrokeGeometry {
        shaft,
        heads,
        open_heads,
    }
}

/// One arrowhead: a polygon to fill or a chevron to stroke.
enum HeadPart {
    Filled(Vec<Point>),
    Open([Point; 3]),
}

/// Where a stealth head's notch sits, as a fraction of its length from the tip.
pub const STEALTH_NOTCH: f32 = 0.6;

/// Arrowhead at the end of `ps`, trimming the last piece back into it.
/// `None` (and `ps` untouched) when every point coincides with the tip.
fn head_at_end(ps: &mut Vec<Piece>, shape: HeadShape, stroke_width: f32) -> Option<HeadPart> {
    let tip = ps.last()?.end();
    // Tangent: from the nearest distinct point of the control polygon.
    let from = ps
        .iter()
        .rev()
        .flat_map(|p| p.points().into_iter().rev())
        .find(|p| dist(*p, tip) > EPS)?;
    let d = dist(from, tip);
    let (ux, uy) = ((tip.x - from.x) / d, (tip.y - from.y) / d);
    // Left of the direction of travel on a y-down page: (uy, -ux).
    let (lx, ly) = (uy, -ux);
    let len = shape.length;
    let half = shape.width.max(0.0) / 2.0;
    // `along` back from `apex` towards the tail, `side` to the left.
    let at = |apex: Point, along: f32, side: f32| {
        Point::new(
            apex.x - ux * along + lx * side,
            apex.y - uy * along + ly * side,
        )
    };
    let (part, back) = match shape.style {
        HeadStyle::Triangle => (
            HeadPart::Filled(vec![tip, at(tip, len, -half), at(tip, len, half)]),
            0.5 * len,
        ),
        HeadStyle::Stealth => (
            HeadPart::Filled(vec![
                tip,
                at(tip, len, -half),
                at(tip, STEALTH_NOTCH * len, 0.0),
                at(tip, len, half),
            ]),
            0.5 * len,
        ),
        HeadStyle::Open => {
            // Pull the chevron back so its mitered apex (half the stroke
            // width / sin(half angle) past the geometric one) sits on the tip.
            let hyp = (len * len + half * half).sqrt();
            let sin = if hyp > EPS { half / hyp } else { 1.0 };
            let inset = if sin > 0.25 {
                (stroke_width / 2.0) / sin
            } else {
                0.0 // past the miter limit (4) the apex is bevelled
            };
            let apex = at(tip, inset, 0.0);
            (
                HeadPart::Open([at(apex, len, half), apex, at(apex, len, -half)]),
                inset,
            )
        }
        HeadStyle::BarbLeft | HeadStyle::BarbRight => {
            let s = if shape.style == HeadStyle::BarbLeft {
                1.0
            } else {
                -1.0
            };
            // The barb's inner base sits on the far edge of the shaft, which
            // is cut at the base: it ends flush and the tip stays sharp.
            (
                HeadPart::Filled(vec![
                    tip,
                    at(tip, len, s * half),
                    at(tip, len, -s * stroke_width / 2.0),
                ]),
                len,
            )
        }
    };

    // Drop trailing pieces collapsed onto the tip, then pull the last real
    // one back so it ends `back` from the tip (measured straight, found by
    // bisection along the curve).
    while ps.len() > 1 && ps.last()?.points().iter().all(|p| dist(*p, tip) <= EPS) {
        ps.pop();
    }
    let last = *ps.last()?;
    let chord = dist(last.start(), tip);
    if chord > EPS && back > 0.0 {
        let back = back.min(chord);
        *ps.last_mut()? = match last {
            // Straight: exact.
            Piece::Line(a, _) => Piece::Line(a, Point::new(tip.x - ux * back, tip.y - uy * back)),
            _ => {
                let (mut lo, mut hi) = (0.0f32, 1.0f32);
                for _ in 0..40 {
                    let mid = 0.5 * (lo + hi);
                    if dist(last.eval(mid), tip) > back {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                last.head_part(lo)
            }
        };
    }
    Some(part)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn semicircle_arc_bounds_and_endpoints() {
        // Clockwise (on a y-down page) half circle from (0,0) to (10,0) bulges up.
        let segs = arc_to_cubics(p(0.0, 0.0), p(10.0, 0.0), 5.0, 5.0, 0.0, false, true);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs.last().unwrap().end(), Some(p(10.0, 0.0)));
        let mut all = vec![PathSegment::MoveTo(p(0.0, 0.0))];
        all.extend(segs);
        let (lo, hi) = bounds(&all).unwrap();
        assert!(
            (lo.y + 5.0).abs() < 0.01 && hi.y.abs() < 0.01,
            "{lo:?} {hi:?}"
        );
        assert!(lo.x.abs() < 0.01 && (hi.x - 10.0).abs() < 0.01);
        // Counter-clockwise bulges down.
        let segs = arc_to_cubics(p(0.0, 0.0), p(10.0, 0.0), 5.0, 5.0, 0.0, false, false);
        let mut all = vec![PathSegment::MoveTo(p(0.0, 0.0))];
        all.extend(segs);
        let (_, hi) = bounds(&all).unwrap();
        assert!((hi.y - 5.0).abs() < 0.01);
    }

    #[test]
    fn small_radius_is_scaled_and_large_flag_picks_long_way() {
        // r = 1 can't span 10: scaled up to a semicircle.
        let segs = arc_to_cubics(p(0.0, 0.0), p(10.0, 0.0), 1.0, 1.0, 0.0, false, true);
        let mut all = vec![PathSegment::MoveTo(p(0.0, 0.0))];
        all.extend(segs);
        let (lo, _) = bounds(&all).unwrap();
        assert!((lo.y + 5.0).abs() < 0.01);
        // r = 10 quarter-ish arc: small vs large.
        let small = arc_to_cubics(p(0.0, 0.0), p(10.0, 0.0), 10.0, 10.0, 0.0, false, true);
        let large = arc_to_cubics(p(0.0, 0.0), p(10.0, 0.0), 10.0, 10.0, 0.0, true, true);
        assert_eq!(small.len(), 1);
        assert_eq!(large.len(), 4);
        // The cubic's midpoint lies on the circle (center (5, 5*sqrt3) for small cw).
        let c = p(5.0, 75.0f32.sqrt());
        let PathSegment::CubicTo(c1, c2, end) = small[0] else {
            panic!("expected a cubic: {small:?}");
        };
        let m = Piece::Cubic(p(0.0, 0.0), c1, c2, end).eval(0.5);
        assert!((dist(m, c) - 10.0).abs() < 0.01, "{m:?}");
    }

    #[test]
    fn cubic_bounds_are_exact() {
        // Symmetric hump: peak at t = 0.5, y = 0.75 * (-10) = -7.5.
        let segs = [
            PathSegment::MoveTo(p(0.0, 0.0)),
            PathSegment::CubicTo(p(0.0, -10.0), p(10.0, -10.0), p(10.0, 0.0)),
        ];
        let (lo, hi) = bounds(&segs).unwrap();
        assert!((lo.y + 7.5).abs() < 1e-4, "{lo:?}");
        assert_eq!(hi, p(10.0, 0.0));
        let q = [
            PathSegment::MoveTo(p(0.0, 0.0)),
            PathSegment::QuadTo(p(5.0, 10.0), p(10.0, 0.0)),
        ];
        let (_, hi) = bounds(&q).unwrap();
        assert!((hi.y - 5.0).abs() < 1e-4);
    }

    #[test]
    fn head_follows_tangent_and_shaft_is_trimmed_along_curve() {
        // Quad ending at (10, 0) arriving straight down from the control (10, -10).
        let segs = [
            PathSegment::MoveTo(p(0.0, -10.0)),
            PathSegment::QuadTo(p(10.0, -10.0), p(10.0, 0.0)),
        ];
        let g = stroke_geometry(&segs, ArrowHead::End, HeadShape::triangle(4.0), 1.0);
        assert_eq!(g.heads.len(), 1);
        let [tip, b1, b2] = g.heads[0][..] else {
            panic!("a triangle: {:?}", g.heads);
        };
        assert_eq!(tip, p(10.0, 0.0));
        // Base is 4 above the tip, horizontal.
        assert!((b1.y + 4.0).abs() < 1e-4 && (b2.y + 4.0).abs() < 1e-4);
        let end = g.shaft.last().unwrap().end().unwrap();
        assert!((dist(end, tip) - 2.0).abs() < 1e-3, "{end:?}");
        // The trimmed piece still starts where the path did.
        assert_eq!(g.shaft[0], PathSegment::MoveTo(p(0.0, -10.0)));

        // Both ends of a single straight piece.
        let segs = [
            PathSegment::MoveTo(p(0.0, 0.0)),
            PathSegment::LineTo(p(10.0, 0.0)),
        ];
        let g = stroke_geometry(&segs, ArrowHead::Both, HeadShape::triangle(2.0), 1.0);
        assert_eq!(g.heads.len(), 2);
        assert_eq!(
            g.shaft,
            vec![
                PathSegment::MoveTo(p(1.0, 0.0)),
                PathSegment::LineTo(p(9.0, 0.0))
            ]
        );
    }

    #[test]
    fn closed_paths_get_no_heads() {
        let segs = [
            PathSegment::MoveTo(p(0.0, 0.0)),
            PathSegment::LineTo(p(10.0, 0.0)),
            PathSegment::LineTo(p(10.0, 10.0)),
            PathSegment::Close,
        ];
        let g = stroke_geometry(&segs, ArrowHead::Both, HeadShape::triangle(2.0), 1.0);
        assert!(g.heads.is_empty());
        assert_eq!(g.shaft, segs.to_vec());
    }
}
