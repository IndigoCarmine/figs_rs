//! Connectors: lines (or wide bands) between two laid-out nodes.
//!
//! A `connector` takes no room in its parent (resolve moves it out of the
//! flow, see `NodeType::overlays`); arrange emits it in its place in the
//! paint order with an empty box, and once every node has its final box
//! [`place_connectors`] draws it between points of its two ends' boxes, in
//! page coordinates — so the ends may sit in any containers, rotated or
//! scaled.
//!
//! - **Attachment**: a fraction `[fx, fy]` of the end's (unrotated) box,
//!   mapped through its transform, or `auto`: the middle of the side facing
//!   the other end (boxes apart horizontally → left/right sides, else
//!   vertically → top/bottom, else by the larger centre offset). Two `auto`
//!   ends whose boxes overlap across the chosen direction meet at the middle
//!   of the overlap, so the link runs straight. `facing-1` / `facing-2` are
//!   the first / second corner (top or left first) of that facing side —
//!   two connectors so attached join a region to its magnified copy. The
//!   box may be narrowed to a sub-box of the node (`from_box` / `to_box`).
//! - **Route**: `straight`; `elbow` — horizontal/vertical runs leaving each
//!   end along its side's normal (both horizontal: H-V-H through the middle;
//!   both vertical: V-H-V; mixed: one corner), corners rounded by `radius`;
//!   `curve` — a cubic leaving and entering along the normals.
//!   Two sides facing the same way (left → left) get a U running
//!   [`U_REACH`] (plus the radius) past the outer end.
//! - **Images**: on an image fitted with `contain`, fractions and sides are
//!   those of the visible picture, not of the (letterboxed) box.
//! - **Gap**: each end is pulled back by its gap (along the line for a
//!   straight route, along the normal otherwise).
//! - **Band**: a filled outline `band` wide around the route (curves
//!   flattened), mitred joins (bevelled past 4 ×), with triangular heads.

use std::collections::HashMap;

use crate::geom::{Affine, ArrowHead, PathSegment, Point, Rect};
use crate::schema::{Attach, ConnectorProps, ConnectorRoute, Document, EndShape, NodeType};

use super::computed::{ComputedNode, PaintContent, PathPaint};

/// A placed end: its local box (the part of it the connector attaches to)
/// and local → page transform.
#[derive(Clone, Copy)]
struct End {
    rect: Rect,
    xf: Option<Affine>,
}

impl End {
    /// The end on node `n`, narrowed to the sub-box `sub` (fractions
    /// `[fx0, fy0, fx1, fy1]` of its box).
    fn of(n: &ComputedNode, sub: Option<[f32; 4]>) -> End {
        let r = n.rect;
        let rect = match sub {
            Some([x0, y0, x1, y1]) => Rect::new(
                r.x + x0 * r.w,
                r.y + y0 * r.h,
                (x1 - x0) * r.w,
                (y1 - y0) * r.h,
            ),
            None => r,
        };
        End {
            rect,
            xf: n.transform,
        }
    }

    /// The page point at fractions `f` of the box.
    fn point(&self, f: [f32; 2]) -> Point {
        let p = Point::new(
            self.rect.x + f[0] * self.rect.w,
            self.rect.y + f[1] * self.rect.h,
        );
        self.xf.map_or(p, |t| t.apply(p))
    }

    /// The page-space bounding box.
    fn bounds(&self) -> Rect {
        let pts = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].map(|f| self.point(f));
        let (mut x0, mut y0, mut x1, mut y1) = (pts[0].x, pts[0].y, pts[0].x, pts[0].y);
        for p in &pts[1..] {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    /// The outward normal (page space, unit) of the side a fraction lies
    /// on; `None` inside the box.
    fn normal(&self, f: [f32; 2]) -> Option<Point> {
        let local = side_normal(f)?;
        let n = match self.xf {
            Some(t) => {
                let o = t.apply(Point::new(0.0, 0.0));
                let q = t.apply(local);
                Point::new(q.x - o.x, q.y - o.y)
            }
            None => local,
        };
        unit(n)
    }
}

/// The outward normal of the side of a box that fractions `f` lie on (left
/// and right before top and bottom); `None` inside.
fn side_normal(f: [f32; 2]) -> Option<Point> {
    const EPS: f32 = 1e-4;
    Some(if f[0] <= EPS {
        Point::new(-1.0, 0.0)
    } else if f[0] >= 1.0 - EPS {
        Point::new(1.0, 0.0)
    } else if f[1] <= EPS {
        Point::new(0.0, -1.0)
    } else if f[1] >= 1.0 - EPS {
        Point::new(0.0, 1.0)
    } else {
        return None;
    })
}

/// Which side of `a` faces `b` (both page boxes), as a fraction of `a`'s
/// box — see the module docs (boxes apart both ways: the axis with the
/// wider gap).
fn facing_side(a: Rect, b: Rect) -> [f32; 2] {
    // Apart both ways: the axis with the wider gap.
    let gap_x = (b.x - (a.x + a.w)).max(a.x - (b.x + b.w));
    let gap_y = (b.y - (a.y + a.h)).max(a.y - (b.y + b.h));
    if gap_x >= 0.0 && gap_y > gap_x {
        return [0.5, if b.y >= a.y + a.h { 1.0 } else { 0.0 }];
    }
    if b.x >= a.x + a.w {
        [1.0, 0.5]
    } else if b.x + b.w <= a.x {
        [0.0, 0.5]
    } else if b.y >= a.y + a.h {
        [0.5, 1.0]
    } else if b.y + b.h <= a.y {
        [0.5, 0.0]
    } else {
        let dx = (b.x + b.w / 2.0) - (a.x + a.w / 2.0);
        let dy = (b.y + b.h / 2.0) - (a.y + a.h / 2.0);
        if dx.abs() >= dy.abs() {
            [if dx >= 0.0 { 1.0 } else { 0.0 }, 0.5]
        } else {
            [0.5, if dy >= 0.0 { 1.0 } else { 0.0 }]
        }
    }
}

/// Where one end attaches: a page point and the outward normal there. On an
/// `ellipse` end the facing corners are the ellipse's extreme points across
/// the facing direction (top / bottom for a side facing left / right).
fn attach_end(
    at: Attach,
    me: &End,
    other: Rect,
    shape: EndShape,
) -> (Point, Option<Point>, Option<[f32; 2]>) {
    match at {
        Attach::Frac(f) => (me.point(f), me.normal(f), None),
        Attach::Auto | Attach::Facing(_) | Attach::Hull(_) => {
            // Chosen on the page box, so the point lies on it.
            let r = me.bounds();
            let side = facing_side(r, other);
            let f = match at {
                Attach::Facing(k) if shape == EndShape::Ellipse => {
                    let second = if k == 2 { 1.0 } else { 0.0 };
                    if side[1] == 0.5 {
                        [0.5, second]
                    } else {
                        [second, 0.5]
                    }
                }
                Attach::Facing(k) => {
                    let second = if k == 2 { 1.0 } else { 0.0 };
                    if side[1] == 0.5 {
                        [side[0], second]
                    } else {
                        [second, side[1]]
                    }
                }
                _ => side,
            };
            let p = Point::new(r.x + f[0] * r.w, r.y + f[1] * r.h);
            (p, side_normal(side), Some(side))
        }
    }
}

/// Where both ends of a connector lie and their outward normals.
fn endpoints(c: &ConnectorProps, a: End, b: End) -> [(Point, Option<Point>); 2] {
    // Hull ends: the ends of one outer tangent of both outlines.
    if let Some(k) = [c.from_at, c.to_at].iter().find_map(|at| match at {
        Attach::Hull(k) => Some(*k),
        _ => None,
    }) {
        if let Some(h) = Hull::of(&a, c.from_shape, &b, EndShape::Box) {
            let (p, q) = h.bridge(k);
            return [(p, None), (q, None)];
        }
    }
    let (ba, bb) = (a.bounds(), b.bounds());
    // An end that is not chosen by side faces the other end's point.
    let toward = |at: Attach, e: &End, r: Rect| match at {
        Attach::Frac(f) => {
            let p = e.point(f);
            Rect::new(p.x, p.y, 0.0, 0.0)
        }
        _ => r,
    };
    let (mut pa, na, sa) = attach_end(c.from_at, &a, toward(c.to_at, &b, bb), c.from_shape);
    let (mut pb, nb, sb) = attach_end(c.to_at, &b, toward(c.from_at, &a, ba), EndShape::Box);
    // Two auto ends on facing sides: meet in the middle of the overlap.
    if let (Attach::Auto, Attach::Auto, Some(fa), Some(fb)) = (c.from_at, c.to_at, sa, sb) {
        if fa[1] == 0.5 && fb[1] == 0.5 {
            let (lo, hi) = (ba.y.max(bb.y), (ba.y + ba.h).min(bb.y + bb.h));
            if hi >= lo {
                pa.y = (lo + hi) / 2.0;
                pb.y = pa.y;
            }
        } else if fa[0] == 0.5 && fb[0] == 0.5 {
            let (lo, hi) = (ba.x.max(bb.x), (ba.x + ba.w).min(bb.x + bb.w));
            if hi >= lo {
                pa.x = (lo + hi) / 2.0;
                pb.x = pa.x;
            }
        }
    }
    [(pa, na), (pb, nb)]
}

/// How far (points, plus the corner radius) an elbow between two sides
/// facing the same way runs out past the outer end.
const U_REACH: f32 = 8.0;

fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}

fn add(a: Point, b: Point, k: f32) -> Point {
    Point::new(a.x + b.x * k, a.y + b.y * k)
}

fn len(v: Point) -> f32 {
    (v.x * v.x + v.y * v.y).sqrt()
}

fn unit(v: Point) -> Option<Point> {
    let l = len(v);
    (l > 1e-6).then(|| Point::new(v.x / l, v.y / l))
}

/// The dominant axis direction of `v` (a unit vector along x or y).
fn axis_dir(v: Point) -> Point {
    if v.x.abs() >= v.y.abs() {
        Point::new(v.x.signum(), 0.0)
    } else {
        Point::new(0.0, v.y.signum())
    }
}

/// The centre line of a connector: page-space segments (a `MoveTo` first).
fn route(c: &ConnectorProps, a: End, b: End) -> Vec<PathSegment> {
    let [(mut p0, n0), (mut p1, n1)] = endpoints(c, a, b);
    match c.route {
        ConnectorRoute::Straight => {
            if let Some(d) = unit(sub(p1, p0)) {
                let total = len(sub(p1, p0));
                let (g0, g1) = (c.gap[0], c.gap[1]);
                if g0 + g1 < total {
                    p0 = add(p0, d, g0);
                    p1 = add(p1, d, -g1);
                }
            }
            vec![PathSegment::MoveTo(p0), PathSegment::LineTo(p1)]
        }
        ConnectorRoute::Elbow => {
            let d = sub(p1, p0);
            let n0 = n0.map_or_else(|| axis_dir(d), axis_dir);
            let n1 = n1.map_or_else(|| axis_dir(Point::new(-d.x, -d.y)), axis_dir);
            p0 = add(p0, n0, c.gap[0]);
            p1 = add(p1, n1, c.gap[1]);
            let h0 = n0.y == 0.0;
            let h1 = n1.y == 0.0;
            let mut pts = vec![p0];
            match (h0, h1) {
                // Both ends facing the same way (left -> left): a U round
                // the outer end.
                (true, true) if n0.x == n1.x => {
                    let reach = U_REACH + c.radius;
                    let mx = if n0.x < 0.0 {
                        p0.x.min(p1.x) - reach
                    } else {
                        p0.x.max(p1.x) + reach
                    };
                    pts.push(Point::new(mx, p0.y));
                    pts.push(Point::new(mx, p1.y));
                }
                (false, false) if n0.y == n1.y => {
                    let reach = U_REACH + c.radius;
                    let my = if n0.y < 0.0 {
                        p0.y.min(p1.y) - reach
                    } else {
                        p0.y.max(p1.y) + reach
                    };
                    pts.push(Point::new(p0.x, my));
                    pts.push(Point::new(p1.x, my));
                }
                (true, true) => {
                    let mx = (p0.x + p1.x) / 2.0;
                    pts.push(Point::new(mx, p0.y));
                    pts.push(Point::new(mx, p1.y));
                }
                (false, false) => {
                    let my = (p0.y + p1.y) / 2.0;
                    pts.push(Point::new(p0.x, my));
                    pts.push(Point::new(p1.x, my));
                }
                (true, false) => pts.push(Point::new(p1.x, p0.y)),
                (false, true) => pts.push(Point::new(p0.x, p1.y)),
            }
            pts.push(p1);
            rounded_polyline(&simplify(&pts), c.radius)
        }
        // Areas, not lines (see `area`): their straight chord as a fallback.
        ConnectorRoute::Cone | ConnectorRoute::Outline => {
            vec![PathSegment::MoveTo(p0), PathSegment::LineTo(p1)]
        }
        ConnectorRoute::Curve => {
            let d = sub(p1, p0);
            let dist = len(d);
            let k = dist * 0.4;
            let u = unit(d).unwrap_or(Point::new(1.0, 0.0));
            let n0 = n0.unwrap_or(u);
            let n1 = n1.unwrap_or(Point::new(-u.x, -u.y));
            p0 = add(p0, n0, c.gap[0]);
            p1 = add(p1, n1, c.gap[1]);
            vec![
                PathSegment::MoveTo(p0),
                PathSegment::CubicTo(add(p0, n0, k), add(p1, n1, k), p1),
            ]
        }
    }
}

/// The closed outline of an area connector: a `cone` — the quadrilateral
/// from `from`'s facing corners to `to`'s (on an ellipse region, its extreme
/// points) — or an `outline` of `from`'s box / its inscribed ellipse.
fn area(c: &ConnectorProps, a: End, b: End) -> Option<Vec<PathSegment>> {
    match c.route {
        ConnectorRoute::Cone => {
            let hull = matches!(c.from_at, Attach::Hull(_)) || matches!(c.to_at, Attach::Hull(_));
            if let Some(h) = hull
                .then(|| Hull::of(&a, c.from_shape, &b, EndShape::Box))
                .flatten()
            {
                return Some(h.cone());
            }
            let (ba, bb) = (a.bounds(), b.bounds());
            let at = |k, e: &End, other, shape| attach_end(Attach::Facing(k), e, other, shape).0;
            let p1 = at(1, &a, bb, c.from_shape);
            let p2 = at(2, &a, bb, c.from_shape);
            let q1 = at(1, &b, ba, EndShape::Box);
            let q2 = at(2, &b, ba, EndShape::Box);
            Some(vec![
                PathSegment::MoveTo(p1),
                PathSegment::LineTo(q1),
                PathSegment::LineTo(q2),
                PathSegment::LineTo(p2),
                PathSegment::Close,
            ])
        }
        ConnectorRoute::Outline => Some(match c.from_shape {
            EndShape::Box => {
                let pts = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].map(|f| a.point(f));
                vec![
                    PathSegment::MoveTo(pts[0]),
                    PathSegment::LineTo(pts[1]),
                    PathSegment::LineTo(pts[2]),
                    PathSegment::LineTo(pts[3]),
                    PathSegment::Close,
                ]
            }
            EndShape::Ellipse => {
                // Four cubics (the usual circle constant), in fractions of
                // the box, mapped through its transform.
                const K: f32 = 0.552_284_8 / 2.0;
                let p = |x: f32, y: f32| a.point([x, y]);
                vec![
                    PathSegment::MoveTo(p(1.0, 0.5)),
                    PathSegment::CubicTo(p(1.0, 0.5 + K), p(0.5 + K, 1.0), p(0.5, 1.0)),
                    PathSegment::CubicTo(p(0.5 - K, 1.0), p(0.0, 0.5 + K), p(0.0, 0.5)),
                    PathSegment::CubicTo(p(0.0, 0.5 - K), p(0.5 - K, 0.0), p(0.5, 0.0)),
                    PathSegment::CubicTo(p(0.5 + K, 0.0), p(1.0, 0.5 - K), p(1.0, 0.5)),
                    PathSegment::Close,
                ]
            }
        }),
        _ => None,
    }
}

/// The convex hull around two ends' outlines (their boxes' corners, or
/// points round an ellipse end), with its two **bridges** — the hull edges
/// from one end to the other, the outer tangents of the pair.
struct Hull {
    /// Each end's outline, in hull order.
    a: Vec<Point>,
    b: Vec<Point>,
    /// Bridge 1 runs from `a[i]` to `b[j]`, bridge 2 from `b[k]` to `a[l]`.
    i: usize,
    j: usize,
    k: usize,
    l: usize,
}

impl Hull {
    /// The hull of ends `a` and `b`; `None` when one outline holds the
    /// other (no bridges).
    fn of(a: &End, sa: EndShape, b: &End, sb: EndShape) -> Option<Hull> {
        let pa = convex(outline_points(a, sa));
        let pb = convex(outline_points(b, sb));
        if pa.len() < 2 || pb.len() < 2 {
            return None;
        }
        let tagged: Vec<(Point, bool)> = pa
            .iter()
            .map(|&p| (p, true))
            .chain(pb.iter().map(|&p| (p, false)))
            .collect();
        let hull = convex_tagged(tagged);
        let n = hull.len();
        let (mut ab, mut ba) = (None, None);
        for m in 0..n {
            let (p, from_a) = hull[m];
            let (q, to_a) = hull[(m + 1) % n];
            if from_a && !to_a {
                ab = Some((p, q));
            } else if !from_a && to_a {
                ba = Some((p, q));
            }
        }
        let ((p1, q1), (q2, p2)) = (ab?, ba?);
        let find = |v: &[Point], p: Point| v.iter().position(|&x| x == p);
        Some(Hull {
            i: find(&pa, p1)?,
            j: find(&pb, q1)?,
            k: find(&pb, q2)?,
            l: find(&pa, p2)?,
            a: pa,
            b: pb,
        })
    }

    /// Bridge `k` (1 or 2) as (point on `a`, point on `b`).
    fn bridge(&self, k: u8) -> (Point, Point) {
        if k == 2 {
            (self.a[self.l], self.b[self.k])
        } else {
            (self.a[self.i], self.b[self.j])
        }
    }

    /// The area between the two outlines: bridge 1, `b`'s near side (the
    /// part off the hull) back to bridge 2, and `a`'s near side.
    fn cone(&self) -> Vec<PathSegment> {
        let back = |v: &[Point], from: usize, to: usize| {
            let n = v.len();
            let mut out = vec![v[from]];
            let mut m = from;
            while m != to {
                m = (m + n - 1) % n;
                out.push(v[m]);
            }
            out
        };
        let mut pts = back(&self.a, self.l, self.i);
        pts.extend(back(&self.b, self.j, self.k));
        let mut segs = vec![PathSegment::MoveTo(pts[0])];
        segs.extend(pts[1..].iter().map(|&p| PathSegment::LineTo(p)));
        segs.push(PathSegment::Close);
        segs
    }
}

/// An end's outline in page space: its box's corners, or 32 points round
/// its inscribed ellipse.
fn outline_points(e: &End, shape: EndShape) -> Vec<Point> {
    match shape {
        EndShape::Box => [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
            .map(|f| e.point(f))
            .to_vec(),
        EndShape::Ellipse => (0..32)
            .map(|m| {
                let t = m as f32 / 32.0 * std::f32::consts::TAU;
                e.point([0.5 + 0.5 * t.cos(), 0.5 + 0.5 * t.sin()])
            })
            .collect(),
    }
}

/// The convex hull of `pts` (monotone chain), without repeats.
fn convex(pts: Vec<Point>) -> Vec<Point> {
    convex_tagged(pts.into_iter().map(|p| (p, false)).collect())
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

/// The convex hull of tagged points (monotone chain; collinear points
/// dropped), in one fixed turning order.
fn convex_tagged(mut pts: Vec<(Point, bool)>) -> Vec<(Point, bool)> {
    pts.sort_by(|a, b| {
        a.0.x
            .partial_cmp(&b.0.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.0.y
                    .partial_cmp(&b.0.y)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    pts.dedup_by(|a, b| (a.0.x - b.0.x).abs() < 1e-4 && (a.0.y - b.0.y).abs() < 1e-4);
    if pts.len() < 3 {
        return pts;
    }
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut lower: Vec<(Point, bool)> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2].0, lower[lower.len() - 1].0, p.0) <= 1e-6
        {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<(Point, bool)> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2].0, upper[upper.len() - 1].0, p.0) <= 1e-6
        {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Drop repeated points and middle points of straight runs.
fn simplify(pts: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out.last().is_some_and(|q| len(sub(p, *q)) < 1e-4) {
            continue;
        }
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let cross = (b.x - a.x) * (p.y - b.y) - (b.y - a.y) * (p.x - b.x);
            let dot = (b.x - a.x) * (p.x - b.x) + (b.y - a.y) * (p.y - b.y);
            if cross.abs() < 1e-4 && dot >= 0.0 {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

/// A polyline with every inner corner rounded by a quadratic from `r`
/// before to `r` after it (at most half of either run).
fn rounded_polyline(pts: &[Point], r: f32) -> Vec<PathSegment> {
    let mut segs = vec![PathSegment::MoveTo(pts[0])];
    for i in 1..pts.len() {
        let v = pts[i];
        if r > 0.0 && i + 1 < pts.len() {
            let (p, n) = (pts[i - 1], pts[i + 1]);
            let (d1, d2) = (len(sub(v, p)), len(sub(n, v)));
            let rr = r.min(d1 / 2.0).min(d2 / 2.0);
            if rr > 0.0 {
                let a = add(v, sub(p, v), rr / d1);
                let b = add(v, sub(n, v), rr / d2);
                segs.push(PathSegment::LineTo(a));
                segs.push(PathSegment::QuadTo(v, b));
                continue;
            }
        }
        segs.push(PathSegment::LineTo(v));
    }
    segs
}

/// The centre line as a polyline (curves sampled), repeated points dropped.
fn flatten(segs: &[PathSegment]) -> Vec<Point> {
    const K: usize = 24;
    let mut pts: Vec<Point> = Vec::new();
    let mut cur = Point::new(0.0, 0.0);
    for s in segs {
        match *s {
            PathSegment::MoveTo(p) | PathSegment::LineTo(p) => {
                pts.push(p);
                cur = p;
            }
            PathSegment::QuadTo(c, p) => {
                for j in 1..=K {
                    let t = j as f32 / K as f32;
                    let u = 1.0 - t;
                    pts.push(Point::new(
                        u * u * cur.x + 2.0 * u * t * c.x + t * t * p.x,
                        u * u * cur.y + 2.0 * u * t * c.y + t * t * p.y,
                    ));
                }
                cur = p;
            }
            PathSegment::CubicTo(c1, c2, p) => {
                for j in 1..=K {
                    let t = j as f32 / K as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    pts.push(Point::new(
                        a * cur.x + b * c1.x + c * c2.x + d * p.x,
                        a * cur.y + b * c1.y + c * c2.y + d * p.y,
                    ));
                }
                cur = p;
            }
            PathSegment::Close => {}
        }
    }
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last().is_none_or(|q| len(sub(p, *q)) > 1e-5) {
            out.push(p);
        }
    }
    out
}

/// The point at arc length `s` along polyline `cl` (cumulative lengths `cum`).
fn at(cl: &[Point], cum: &[f32], s: f32) -> Point {
    for i in 1..cl.len() {
        if s <= cum[i] || i == cl.len() - 1 {
            let seg = cum[i] - cum[i - 1];
            let t = if seg > 0.0 {
                (s - cum[i - 1]) / seg
            } else {
                0.0
            };
            return add(cl[i - 1], sub(cl[i], cl[i - 1]), t);
        }
    }
    cl[0]
}

/// The left normal of a -> b (unit).
fn left_normal(a: Point, b: Point) -> Point {
    let d = len(sub(b, a)).max(1e-9);
    Point::new((b.y - a.y) / d, -(b.x - a.x) / d)
}

/// The closed outline of a band `w` wide around the centre line `segs`,
/// with triangular heads (`hl` long, `hw` across) where `head` says.
fn band_outline(
    segs: &[PathSegment],
    w: f32,
    head: ArrowHead,
    hl: f32,
    hw: f32,
) -> Vec<PathSegment> {
    let cl = flatten(segs);
    if cl.len() < 2 {
        return Vec::new();
    }
    let mut cum = vec![0.0];
    for i in 1..cl.len() {
        cum.push(cum[i - 1] + len(sub(cl[i], cl[i - 1])));
    }
    let total = *cum.last().unwrap();
    let (start, end) = match head {
        ArrowHead::None => (false, false),
        ArrowHead::Start => (true, false),
        ArrowHead::End => (false, true),
        ArrowHead::Both => (true, true),
    };
    let heads = start as u8 + end as u8;
    let room = if heads > 0 {
        total / heads as f32 * 0.999
    } else {
        0.0
    };
    let hl = hl.min(room).max(0.0);
    let s0 = if start { hl } else { 0.0 };
    let s1 = if end { total - hl } else { total };
    // The shaft: the centre line from s0 to s1.
    let mut shaft = vec![at(&cl, &cum, s0)];
    for i in 0..cl.len() {
        if cum[i] > s0 + 1e-6 && cum[i] < s1 - 1e-6 {
            shaft.push(cl[i]);
        }
    }
    shaft.push(at(&cl, &cum, s1));
    if shaft.len() < 2 || len(sub(shaft[shaft.len() - 1], shaft[0])) < 1e-6 && shaft.len() == 2 {
        shaft = vec![cl[0], cl[cl.len() - 1]];
    }
    let h = w / 2.0;
    let n_first = left_normal(shaft[0], shaft[1]);
    let n_last = left_normal(shaft[shaft.len() - 2], shaft[shaft.len() - 1]);
    let mut left = vec![add(shaft[0], n_first, h)];
    let mut right = vec![add(shaft[0], n_first, -h)];
    for i in 1..shaft.len() - 1 {
        let q = shaft[i];
        let na = left_normal(shaft[i - 1], q);
        let nb = left_normal(q, shaft[i + 1]);
        let k = 1.0 + na.x * nb.x + na.y * nb.y;
        if k < 1e-6 {
            left.extend([add(q, na, h), add(q, nb, h)]);
            right.extend([add(q, na, -h), add(q, nb, -h)]);
            continue;
        }
        let m = Point::new((na.x + nb.x) / k, (na.y + nb.y) / k);
        if len(m) <= 4.0 {
            left.push(add(q, m, h));
            right.push(add(q, m, -h));
        } else {
            let next = shaft[i + 1];
            let turns_left = (next.x - q.x) * na.x + (next.y - q.y) * na.y > 0.0;
            if turns_left {
                left.push(add(q, m, h));
                right.extend([add(q, na, -h), add(q, nb, -h)]);
            } else {
                left.extend([add(q, na, h), add(q, nb, h)]);
                right.push(add(q, m, -h));
            }
        }
    }
    let base = shaft[shaft.len() - 1];
    left.push(add(base, n_last, h));
    right.push(add(base, n_last, -h));
    let mut pts = Vec::new();
    if start {
        pts.push(cl[0]);
        pts.push(add(shaft[0], n_first, hw / 2.0));
    }
    pts.extend(left);
    if end {
        pts.push(add(base, n_last, hw / 2.0));
        pts.push(cl[cl.len() - 1]);
        pts.push(add(base, n_last, -hw / 2.0));
    }
    right.reverse();
    pts.extend(right);
    if start {
        pts.push(add(shaft[0], n_first, -hw / 2.0));
    }
    let mut out = vec![PathSegment::MoveTo(pts[0])];
    out.extend(pts[1..].iter().map(|&p| PathSegment::LineTo(p)));
    out.push(PathSegment::Close);
    out
}

/// Draw every connector in `nodes` (the arranged layout of `doc`) between
/// its ends' final boxes. A connector whose end was not laid out (not on the
/// page) paints nothing, with a warning.
pub(crate) fn place_connectors(
    doc: &Document,
    nodes: &mut [ComputedNode],
    image_aspect: impl Fn(&crate::schema::ImageProps) -> Option<f32>,
) {
    if !doc
        .nodes
        .iter()
        .any(|n| matches!(n.kind, NodeType::Connector(_)))
    {
        return;
    }
    let by_id: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.clone(), i))
        .collect();
    let mut placed = Vec::new();
    for node in &doc.nodes {
        let NodeType::Connector(c) = &node.kind else {
            continue;
        };
        let Some(&me) = by_id.get(&node.id) else {
            continue;
        };
        let end = |i: usize, sub| {
            by_id.get(&doc.nodes[i].id).map(|&k| {
                let mut n = nodes[k].clone();
                // An image fitted inside its box ("contain"): its fractions
                // are those of the visible picture.
                if let NodeType::Image(img) = &doc.nodes[i].kind {
                    if img.fit == crate::geom::ImageFit::Contain {
                        if let Some(a) = image_aspect(img).filter(|a| *a > 0.0) {
                            n.rect = crate::render::geometry::image_placement(
                                n.rect,
                                a,
                                1.0,
                                crate::geom::ImageFit::Contain,
                            );
                        }
                    }
                }
                End::of(&n, sub)
            })
        };
        let (Some(a), Some(b)) = (end(c.from, c.from_box), end(c.to, c.to_box)) else {
            tracing::warn!(
                node = %node.id,
                "connector: an end is not on the page; nothing drawn"
            );
            continue;
        };
        if let Some(outline) = area(c, a, b) {
            let rect = bounds_of(&outline);
            placed.push((
                me,
                PathPaint {
                    fill: c.fill.as_ref().map(|p| p.place(rect)),
                    segments: outline,
                    stroke: c.stroke.clone(),
                    head: ArrowHead::None,
                    head_size: 0.0,
                    head_width: 0.0,
                    head_style: c.head_style,
                    stroke_align: Default::default(),
                },
            ));
            continue;
        }
        let line = route(c, a, b);
        let paint = match c.band {
            Some(w) => {
                let outline = band_outline(&line, w, c.head, c.head_size, c.head_width);
                let rect = bounds_of(&outline);
                PathPaint {
                    fill: c.fill.as_ref().map(|p| p.place(rect)),
                    segments: outline,
                    stroke: c.stroke.clone(),
                    head: ArrowHead::None,
                    head_size: 0.0,
                    head_width: 0.0,
                    head_style: c.head_style,
                    stroke_align: Default::default(),
                }
            }
            None => PathPaint {
                segments: line,
                fill: None,
                stroke: c.stroke.clone(),
                head: c.head,
                head_size: c.head_size,
                head_width: c.head_width,
                head_style: c.head_style,
                stroke_align: Default::default(),
            },
        };
        placed.push((me, paint));
    }
    for (i, paint) in placed {
        let n = &mut nodes[i];
        n.rect = bounds_of(&paint.segments);
        n.transform = None;
        n.content = PaintContent::Path(paint);
    }
}

/// The bounding box of an outline's points (end and control points).
fn bounds_of(segs: &[PathSegment]) -> Rect {
    match crate::path::bounds(segs) {
        Some((lo, hi)) => Rect::new(lo.x, lo.y, hi.x - lo.x, hi.y - lo.y),
        None => Rect::new(0.0, 0.0, 0.0, 0.0),
    }
}
