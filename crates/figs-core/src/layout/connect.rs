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
//!   of the overlap, so the link runs straight.
//! - **Route**: `straight`; `elbow` — horizontal/vertical runs leaving each
//!   end along its side's normal (both horizontal: H-V-H through the middle;
//!   both vertical: V-H-V; mixed: one corner), corners rounded by `radius`;
//!   `curve` — a cubic leaving and entering along the normals.
//! - **Gap**: each end is pulled back by its gap (along the line for a
//!   straight route, along the normal otherwise).
//! - **Band**: a filled outline `band` wide around the route (curves
//!   flattened), mitred joins (bevelled past 4 ×), with triangular heads.

use std::collections::HashMap;

use crate::geom::{Affine, ArrowHead, PathSegment, Point, Rect};
use crate::schema::{Attach, ConnectorProps, ConnectorRoute, Document, NodeType};

use super::computed::{ComputedNode, PaintContent, PathPaint};

/// A placed end: its local box and local → page transform.
#[derive(Clone, Copy)]
struct End {
    rect: Rect,
    xf: Option<Affine>,
}

impl End {
    fn of(n: &ComputedNode) -> End {
        End {
            rect: n.rect,
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
        const EPS: f32 = 1e-4;
        let local = if f[0] <= EPS {
            (-1.0, 0.0)
        } else if f[0] >= 1.0 - EPS {
            (1.0, 0.0)
        } else if f[1] <= EPS {
            (0.0, -1.0)
        } else if f[1] >= 1.0 - EPS {
            (0.0, 1.0)
        } else {
            return None;
        };
        let n = match self.xf {
            Some(t) => {
                let o = t.apply(Point::new(0.0, 0.0));
                let q = t.apply(Point::new(local.0, local.1));
                (q.x - o.x, q.y - o.y)
            }
            None => local,
        };
        let len = (n.0 * n.0 + n.1 * n.1).sqrt();
        (len > 0.0).then(|| Point::new(n.0 / len, n.1 / len))
    }
}

/// Which side of `a` faces `b` (both page boxes), as a fraction of `a`'s
/// box (for an unrotated `a`) — see the module docs.
fn facing_side(a: Rect, b: Rect) -> [f32; 2] {
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

/// Where both ends of a connector lie and their outward normals.
fn endpoints(c: &ConnectorProps, a: End, b: End) -> [(Point, Option<Point>); 2] {
    let (ba, bb) = (a.bounds(), b.bounds());
    let fa = match c.from_at {
        Attach::Frac(f) => f,
        Attach::Auto => {
            let other = match c.to_at {
                Attach::Frac(f) => {
                    let p = b.point(f);
                    Rect::new(p.x, p.y, 0.0, 0.0)
                }
                Attach::Auto => bb,
            };
            facing_side(ba, other)
        }
    };
    let fb = match c.to_at {
        Attach::Frac(f) => f,
        Attach::Auto => {
            let p = a.point(fa);
            let other = match c.from_at {
                Attach::Frac(_) => Rect::new(p.x, p.y, 0.0, 0.0),
                Attach::Auto => ba,
            };
            facing_side(bb, other)
        }
    };
    // Auto sides are chosen on the page boxes, so they lie on them.
    let side_point = |e: &End, r: Rect, f: [f32; 2], auto: bool| {
        if auto {
            Point::new(r.x + f[0] * r.w, r.y + f[1] * r.h)
        } else {
            e.point(f)
        }
    };
    let auto_a = c.from_at == Attach::Auto;
    let auto_b = c.to_at == Attach::Auto;
    let mut pa = side_point(&a, ba, fa, auto_a);
    let mut pb = side_point(&b, bb, fb, auto_b);
    let page_normal = |f: [f32; 2]| -> Option<Point> {
        Some(match f {
            [x, _] if x <= 0.0 => Point::new(-1.0, 0.0),
            [x, _] if x >= 1.0 => Point::new(1.0, 0.0),
            [_, y] if y <= 0.0 => Point::new(0.0, -1.0),
            [_, y] if y >= 1.0 => Point::new(0.0, 1.0),
            _ => return None,
        })
    };
    let na = if auto_a {
        page_normal(fa)
    } else {
        a.normal(fa)
    };
    let nb = if auto_b {
        page_normal(fb)
    } else {
        b.normal(fb)
    };
    // Two auto ends on facing sides: meet in the middle of the overlap.
    if auto_a && auto_b {
        let horizontal = fa[1] == 0.5 && fb[1] == 0.5;
        let vertical = fa[0] == 0.5 && fb[0] == 0.5;
        if horizontal {
            let (lo, hi) = (ba.y.max(bb.y), (ba.y + ba.h).min(bb.y + bb.h));
            if hi >= lo {
                pa.y = (lo + hi) / 2.0;
                pb.y = pa.y;
            }
        } else if vertical {
            let (lo, hi) = (ba.x.max(bb.x), (ba.x + ba.w).min(bb.x + bb.w));
            if hi >= lo {
                pa.x = (lo + hi) / 2.0;
                pb.x = pa.x;
            }
        }
    }
    [(pa, na), (pb, nb)]
}

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
pub(crate) fn place_connectors(doc: &Document, nodes: &mut [ComputedNode]) {
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
        let end = |i: usize| by_id.get(&doc.nodes[i].id).map(|&k| End::of(&nodes[k]));
        let (Some(a), Some(b)) = (end(c.from), end(c.to)) else {
            tracing::warn!(
                node = %node.id,
                "connector: an end is not on the page; nothing drawn"
            );
            continue;
        };
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
