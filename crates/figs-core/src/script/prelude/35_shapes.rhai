// ---------------------------------------------------------------------------
// Shapes (sugar over the core `ellipse` / `path`)
// ---------------------------------------------------------------------------

// A circle of radius `r` (a square `ellipse`).
fn circle(r) { ellipse().width(2 * r).height(2 * r) }

// A circle of radius `r` centered on (cx, cy) of the enclosing `stack`.
fn circle(cx, cy, r) { circle(r).at(cx - r, cy - r) }

// A block arrow from (x1, y1) to its tip (x2, y2): a shaft `shaft_w` wide,
// then a triangular head `head_w` across and `head_len` long. One closed
// `path` (7 corners) filled with `accent()` — chain `.fill(linear(0, [...]))`
// for a gradient, `.stroke(..)` for an outline, `.shadow(..)` for depth.
// `opts`: `both: true` puts a head on (x1, y1) too (10 corners, ⇔).
fn block_arrow(x1, y1, x2, y2, shaft_w, head_w, head_len) {
    block_arrow(x1, y1, x2, y2, shaft_w, head_w, head_len, #{})
}

fn block_arrow(x1, y1, x2, y2, shaft_w, head_w, head_len, opts) {
    check_opts("block_arrow", opts, ["both"]);
    let both = opt(opts, "both", false);
    let f = chord_frame("block_arrow", x1, y1, x2, y2);
    let nx = f[0];
    let ny = f[1];
    let len = f[2];
    let hl = head_len.to_float();
    let room = if both { len / 2.0 } else { len };
    if hl > room { hl = room; }
    // Unit along the chord: (-ny, nx); bases of the heads on the chord.
    let bx = x2 - (-ny) * hl;
    let by = y2 - nx * hl;
    let ax = x1 + (-ny) * hl;
    let ay = y1 + nx * hl;
    let s = shaft_w / 2.0;
    let h = head_w / 2.0;
    let pts = if both {
        [[x1, y1], [ax + nx * h, ay + ny * h], [ax + nx * s, ay + ny * s]]
    } else {
        [[x1 + nx * s, y1 + ny * s]]
    };
    pts += [
        [bx + nx * s, by + ny * s], [bx + nx * h, by + ny * h],
        [x2, y2],
        [bx - nx * h, by - ny * h], [bx - nx * s, by - ny * s],
    ];
    pts += if both {
        [[ax - nx * s, ay - ny * s], [ax - nx * h, ay - ny * h]]
    } else {
        [[x1 - nx * s, y1 - ny * s]]
    };
    path(pts).closed().fill(accent())
}

// The equilibrium arrows ⇌ between (x1, y1) and (x2, y2), `gap` apart, the
// upper one pointing at (x2, y2), the lower one back at (x1, y1), barbs on
// the outside; in `fg()` (black by default), `annotation_width()` wide.
// `equilibrium(x1, y1, x2, y2, gap)` or `(.., opts)`, a map of
//   gap         between the two arrows (default 4 × the width)
//   width       stroke width (default `annotation_width()`)
//   head        the head on each tip: "barb" (a half head on the outer
//               side, the core `head_style("barb_left")` — the harpoon),
//               "triangle" (a full filled head) or "none"; default "barb",
//               or "none" with barb "hook"
//   head_size   the head's length along the shaft (core default 5 × width)
//   head_width  its width across, a length or "3x" of the width (core
//               default 0.8 × head_size)
//   barb        "straight" (default): no barb beyond the head; "hook": a
//               curved barb leaving the shaft and bending outwards (the
//               PowerPoint ⇌), 1.5 × the width with round caps — on plain
//               shafts by default, or behind a `head`
//   hook_size   the hook's length back along the shaft from where it starts
//               (default 0.4 × the chord, at most 20 × width)
//   hook_width  how far out it reaches, a length or "3x" of the width
//               (default 0.4 × hook_size)
//   hook_start  how far behind the tip it leaves the shaft (default 0, or
//               0.6 × the head's length behind a head)
// Deprecated: with no head (barb "hook" and head "none"), head_size /
// head_width size the hook, as before hook_size / hook_width existed.
// A `stack` of the arrows (and hooks): core setters chain on it.
fn equilibrium(x1, y1, x2, y2) { equilibrium(x1, y1, x2, y2, #{}) }

fn equilibrium(x1, y1, x2, y2, opts) {
    if type_of(opts) != "map" { opts = #{ gap: opts }; }
    check_opts("equilibrium", opts, ["gap", "width", "barb", "head", "head_size", "head_width",
                                     "hook_size", "hook_width", "hook_start"]);
    let w = opt(opts, "width", annotation_width()).to_float();
    let gap = opt(opts, "gap", w * 4.0);
    let barb = one_of("equilibrium", "barb", opt(opts, "barb", "straight"), ["straight", "hook"]);
    let head = one_of("equilibrium", "head", opt(opts, "head", if barb == "hook" { "none" } else { "barb" }),
                      equilibrium_heads());
    let f = chord_frame("equilibrium", x1, y1, x2, y2);
    let nx = f[0];
    let ny = f[1];
    let ox = nx * gap / 2.0;
    let oy = ny * gap / 2.0;
    let c = fg();
    let parts = [];
    // The two shafts: the upper one to (x2, y2), the lower one back.
    for s in [[x1 + ox, y1 + oy, x2 + ox, y2 + oy], [x2 - ox, y2 - oy, x1 - ox, y1 - oy]] {
        if head == "none" {
            parts.push(line(s[0], s[1], s[2], s[3]).stroke_width(w));
            continue;
        }
        let a = arrow(s[0], s[1], s[2], s[3]).stroke_width(w)
            .head_style(if head == "barb" { "barb_left" } else { "triangle" });
        if opts.contains("head_size") { a = a.head_size(opts.head_size); }
        if opts.contains("head_width") { a = a.head_width(opts.head_width); }
        parts.push(a);
    }
    if barb == "hook" {
        // Without a head, head_size / head_width are the hook's (deprecated).
        let alias = head == "none";
        let hs = if opts.contains("hook_size") { opts.hook_size }
                 else if alias && opts.contains("head_size") { opts.head_size }
                 else if f[2] * 0.4 < w * 20.0 { f[2] * 0.4 } else { w * 20.0 };
        hs = hs.to_float();
        let hw = if opts.contains("hook_width") { len_or_multiple("equilibrium", opts.hook_width, w) }
                 else if alias && opts.contains("head_width") { len_or_multiple("equilibrium", opts.head_width, w) }
                 else { hs * 0.4 };
        let hl = if opts.contains("head_size") { opts.head_size.to_float() } else { w * 5.0 };
        let h0 = opt(opts, "hook_start", if alias { 0.0 } else { hl * 0.6 }).to_float();
        // Where each hook leaves its shaft, the unit back along the shaft,
        // and outwards.
        let ends = [
            [x2 + ox + ny * h0, y2 + oy - nx * h0, ny, -nx, nx, ny],
            [x1 - ox - ny * h0, y1 - oy + nx * h0, -ny, nx, -nx, -ny],
        ];
        for e in ends {
            // Local [back, out] -> page: tangent to the shaft where it
            // starts, curling out to (hook_size, hook_width).
            let c1 = hook_pt(e, hs * 0.57, 0.0);
            let c2 = hook_pt(e, hs * 1.01, hw * 0.41);
            let q = hook_pt(e, hs, hw);
            parts.push(path([[e[0], e[1]]]).cubic(c1[0], c1[1], c2[0], c2[1], q[0], q[1])
                .stroke_width(w * 1.5).cap("round"));
        }
    }
    if c != "" {
        let out = [];
        for x in parts { out.push(x.stroke(c)); }
        parts = out;
    }
    stack(parts)
}

// The heads of `equilibrium`: "barb" (a half head, the core "barb_left"),
// "triangle" or "none".
fn equilibrium_heads() { ["none", "triangle", "barb"] }

// The core arrow `a` with an `equilibrium` head (a name from
// `equilibrium_heads()`; "none" for a plain line).
fn equilibrium_head(a, head) {
    if head == "none" { a.head("none") }
    else { a.head_style(if head == "barb" { "barb_left" } else { "triangle" }) }
}

// Point `a` back along and `o` out from a tip `e` = [x, y, back.., out..].
fn hook_pt(e, a, o) { [e[0] + e[2] * a + e[4] * o, e[1] + e[3] * a + e[5] * o] }

// The point at `deg` degrees (0 = +x, clockwise on the page) on the circle of
// radius `rad` about (cx, cy).
fn arc_pt(cx, cy, rad, deg) {
    let t = deg.to_float() * PI() / 180.0;
    [cx + rad * t.cos(), cy + rad * t.sin()]
}

// A circular block arrow: a band `w` wide centred on the circle of radius `r`
// about (cx, cy), from angle `a0` to its tip at `a1` (degrees, 0 = pointing
// right, increasing clockwise on the page as for `rotate`; `a1 < a0` runs
// anticlockwise; less than a full turn). The head is `head_width` across
// (default 2 × w) and `head_len` long along the circle (default w), its tip
// on the centre line. One closed `path` of arcs and corners, filled with
// `accent()`. `opts`: head_width, head_len, both (a head at `a0` too).
fn arc_arrow(cx, cy, r, a0, a1, w) { arc_arrow(cx, cy, r, a0, a1, w, #{}) }

fn arc_arrow(cx, cy, r, a0, a1, w, opts) {
    check_opts("arc_arrow", opts, ["head_width", "head_len", "both"]);
    let r = r.to_float();
    let w = w.to_float();
    if !(w > 0.0) || !(r > w / 2.0) {
        throw `arc_arrow(): need 0 < w < 2 r (got r = ${r}, w = ${w})`;
    }
    let span = (a1 - a0).to_float();
    if span == 0.0 || span.abs() >= 360.0 {
        throw `arc_arrow(): the angles must differ by less than a full turn (got ${a0} to ${a1})`;
    }
    let dir = if span > 0.0 { 1.0 } else { -1.0 };
    let hw = opt(opts, "head_width", w * 2.0).to_float();
    let hl = opt(opts, "head_len", w).to_float();
    let both = opt(opts, "both", false);
    // Head length as an angle, leaving the band at least a sliver.
    let dh = hl / r * 180.0 / PI();
    let room = span.abs() / (if both { 2.0 } else { 1.0 }) * 0.999;
    if dh > room { dh = room; }
    let ah = a1 - dir * dh;
    let a_s = if both { a0 + dir * dh } else { a0 };
    let ro = r + w / 2.0;
    let ri = r - w / 2.0;
    let large = (ah - a_s).abs() > 180.0;
    let fwd = if dir > 0.0 { "cw" } else { "ccw" };
    let back = if dir > 0.0 { "ccw" } else { "cw" };
    let p = if both {
        let t = arc_pt(cx, cy, r, a0);
        let c = arc_pt(cx, cy, r + hw / 2.0, a_s);
        let b = arc_pt(cx, cy, ro, a_s);
        path([t]).to(c[0], c[1]).to(b[0], b[1])
    } else {
        path([arc_pt(cx, cy, ro, a0)])
    };
    let e = arc_pt(cx, cy, ro, ah);
    p = p.arc(ro, e[0], e[1], #{ sweep: fwd, large: large });
    for q in [arc_pt(cx, cy, r + hw / 2.0, ah), arc_pt(cx, cy, r, a1),
              arc_pt(cx, cy, r - hw / 2.0, ah), arc_pt(cx, cy, ri, ah)] {
        p = p.to(q[0], q[1]);
    }
    let e = arc_pt(cx, cy, ri, a_s);
    p = p.arc(ri, e[0], e[1], #{ sweep: back, large: large });
    if both {
        let c = arc_pt(cx, cy, r - hw / 2.0, a_s);
        p = p.to(c[0], c[1]);
    }
    p.closed().fill(accent())
}

// A bent block arrow along a centre line: a band `w` wide following `points`
// with a triangular head at the last point. `points` start with [x, y]; each
// further entry is a straight run to [x, y], a quadratic [cx, cy, x, y] or a
// cubic [c1x, c1y, c2x, c2y, x, y] (the arguments of the `path` builder's
// `.quad` / `.cubic`). The outline is the centre line (curves flattened)
// offset by w / 2 on both sides: mitred joins, bevelled past a miter of 4 ×
// (corners sharper than ~30°), so keep curves wider than the band. Options:
//   radius      round every corner between straight runs with this radius
//               (at most half of either run); 0 (default) keeps them sharp
//   head_width  across the head (default 2 × w)
//   head_len    along the line (default w)
//   both        a head at the first point too
// One closed `path`, filled with `accent()`.
fn block_arrow_along(points, w) { block_arrow_along(points, w, #{}) }

fn block_arrow_along(points, w, opts) {
    let who = "block_arrow_along";
    check_opts(who, opts, ["radius", "head_width", "head_len", "both"]);
    let w = w.to_float();
    if !(w > 0.0) { throw `${who}(): the width must be positive (got ${w})`; }
    let hw = opt(opts, "head_width", w * 2.0).to_float();
    let hl = opt(opts, "head_len", w).to_float();
    let both = opt(opts, "both", false);
    let entries = along_round(along_entries(who, points), opt(opts, "radius", 0).to_float());
    let cl = along_flatten(entries);
    if cl.len() < 2 { throw `${who}(): the centre line has no length`; }
    let cum = [0.0];
    for i in 1..cl.len() { cum.push(cum[i - 1] + along_dist(cl[i - 1], cl[i])); }
    let total = cum[cum.len() - 1];
    let room = total / (if both { 2.0 } else { 1.0 }) * 0.999;
    if hl > room { hl = room; }
    let s0 = if both { hl } else { 0.0 };
    let shaft = along_cut(cl, cum, s0, total - hl);
    let h = w / 2.0;
    // Normals: the head axes at the ends, the segments in between.
    let tip = cl[cl.len() - 1];
    let base = shaft[shaft.len() - 1];
    let n_end = along_normal(base, tip);
    let n_start = if both { along_normal(cl[0], shaft[0]) } else { along_normal(shaft[0], shaft[1]) };
    let left = [along_off(shaft[0], n_start, h)];
    let right = [along_off(shaft[0], n_start, -h)];
    for i in 1..shaft.len() - 1 {
        let q = shaft[i];
        let na = along_normal(shaft[i - 1], q);
        let nb = along_normal(q, shaft[i + 1]);
        let k = 1.0 + na[0] * nb[0] + na[1] * nb[1];
        if k < 1e-6 {
            // Doubling back: a flat end on both sides.
            left += [along_off(q, na, h), along_off(q, nb, h)];
            right += [along_off(q, na, -h), along_off(q, nb, -h)];
            continue;
        }
        let m = [(na[0] + nb[0]) / k, (na[1] + nb[1]) / k];
        if (m[0] * m[0] + m[1] * m[1]).sqrt() <= 4.0 {
            left.push(along_off(q, m, h));
            right.push(along_off(q, m, -h));
        } else {
            // Past the miter limit: bevel the outer side.
            let turns_left = (shaft[i + 1][0] - q[0]) * na[0] + (shaft[i + 1][1] - q[1]) * na[1] > 0.0;
            if turns_left {
                left.push(along_off(q, m, h));
                right += [along_off(q, na, -h), along_off(q, nb, -h)];
            } else {
                left += [along_off(q, na, h), along_off(q, nb, h)];
                right.push(along_off(q, m, -h));
            }
        }
    }
    left.push(along_off(base, n_end, h));
    right.push(along_off(base, n_end, -h));
    let pts = [];
    if both {
        pts += [cl[0], along_off(shaft[0], n_start, hw / 2.0)];
    }
    pts += left;
    pts += [along_off(base, n_end, hw / 2.0), tip, along_off(base, n_end, -hw / 2.0)];
    right.reverse();
    pts += right;
    if both { pts.push(along_off(shaft[0], n_start, -hw / 2.0)); }
    path(pts).closed().fill(accent())
}

// The entries of a `block_arrow_along` centre line, as floats, checked.
fn along_entries(who, points) {
    if type_of(points) != "array" || points.len() < 2 {
        throw `${who}(): expected an array of at least two points`;
    }
    let out = [];
    for e in points {
        let n = if type_of(e) == "array" { e.len() } else { 0 };
        if (out.len() == 0 && n != 2) || ![2, 4, 6].contains(n) {
            throw `${who}(): entries are [x, y], [cx, cy, x, y] or [c1x, c1y, c2x, c2y, x, y]; the first is [x, y]`;
        }
        let v = [];
        for x in e { v.push(x.to_float()); }
        out.push(v);
    }
    out
}

// Corners between straight runs rounded with radius `rad`: the corner point
// becomes a quadratic from `rad` before it to `rad` after it.
fn along_round(entries, rad) {
    if !(rad > 0.0) { return entries; }
    let out = [entries[0]];
    let n = entries.len();
    for i in 1..n {
        let v = entries[i];
        if v.len() == 2 && i + 1 < n && entries[i + 1].len() == 2 {
            let prev = entries[i - 1];
            let p = [prev[prev.len() - 2], prev[prev.len() - 1]];
            let nx = entries[i + 1];
            let d1 = along_dist(p, v);
            let d2 = along_dist(v, nx);
            if d1 > 0.0 && d2 > 0.0 {
                let r = rad;
                if r > d1 / 2.0 { r = d1 / 2.0; }
                if r > d2 / 2.0 { r = d2 / 2.0; }
                let a = [v[0] + (p[0] - v[0]) / d1 * r, v[1] + (p[1] - v[1]) / d1 * r];
                let b = [v[0] + (nx[0] - v[0]) / d2 * r, v[1] + (nx[1] - v[1]) / d2 * r];
                out.push(a);
                out.push([v[0], v[1], b[0], b[1]]);
                continue;
            }
        }
        out.push(v);
    }
    out
}

// The centre line as a polyline: curves sampled at 24 points each,
// repeated points dropped.
fn along_flatten(entries) {
    let pts = [entries[0]];
    let cur = entries[0];
    for i in 1..entries.len() {
        let e = entries[i];
        let end = [e[e.len() - 2], e[e.len() - 1]];
        if e.len() == 2 {
            pts.push(end);
        } else {
            let k = 24;
            for j in 1..=k {
                let t = j.to_float() / k.to_float();
                let u = 1.0 - t;
                pts.push(if e.len() == 4 {
                    [u * u * cur[0] + 2.0 * u * t * e[0] + t * t * end[0],
                     u * u * cur[1] + 2.0 * u * t * e[1] + t * t * end[1]]
                } else {
                    [u * u * u * cur[0] + 3.0 * u * u * t * e[0] + 3.0 * u * t * t * e[2] + t * t * t * end[0],
                     u * u * u * cur[1] + 3.0 * u * u * t * e[1] + 3.0 * u * t * t * e[3] + t * t * t * end[1]]
                });
            }
        }
        cur = end;
    }
    let out = [pts[0]];
    for p in pts {
        if along_dist(out[out.len() - 1], p) > 1e-9 { out.push(p); }
    }
    out
}

fn along_dist(a, b) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

// Unit left normal of a -> b (as `chord_frame`: upwards for a rightward run).
fn along_normal(a, b) {
    let d = along_dist(a, b);
    [(b[1] - a[1]) / d, -(b[0] - a[0]) / d]
}

fn along_off(p, n, d) { [p[0] + n[0] * d, p[1] + n[1] * d] }

// The part of polyline `cl` (cumulative lengths `cum`) from arc length s0
// to s1, with interpolated ends.
fn along_cut(cl, cum, s0, s1) {
    let out = [along_at(cl, cum, s0)];
    for i in 0..cl.len() {
        if cum[i] > s0 + 1e-9 && cum[i] < s1 - 1e-9 { out.push(cl[i]); }
    }
    out.push(along_at(cl, cum, s1));
    out
}

fn along_at(cl, cum, s) {
    for i in 1..cl.len() {
        if s <= cum[i] || i == cl.len() - 1 {
            let seg = cum[i] - cum[i - 1];
            let t = if seg > 0.0 { (s - cum[i - 1]) / seg } else { 0.0 };
            return [cl[i - 1][0] + (cl[i][0] - cl[i - 1][0]) * t, cl[i - 1][1] + (cl[i][1] - cl[i - 1][1]) * t];
        }
    }
    cl[0]
}

// ---------------------------------------------------------------------------
// Curves (sugar over the core `path` curve builders .quad/.cubic/.arc)
// ---------------------------------------------------------------------------
//
// Coordinates are page units in the shape's own box, as for `line`/`arrow`.
// `bend`/`depth` offsets are measured perpendicular to the chord from
// (x1, y1) to (x2, y2): positive goes to the *left* of the direction of travel
// (upwards for a left-to-right chord), negative to the right.

// Unit left-normal [nx, ny] of the chord (x1, y1) -> (x2, y2), and its length.
fn chord_frame(name, x1, y1, x2, y2) {
    let dx = (x2 - x1).to_float();
    let dy = (y2 - y1).to_float();
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 { throw `${name}(): start and end points coincide`; }
    [dy / len, -dx / len, len]
}

// A quadratic curve whose midpoint sits `bend` off the chord (0 = straight).
fn curve(x1, y1, x2, y2, bend) {
    let f = chord_frame("curve", x1, y1, x2, y2);
    // A quad's midpoint is halfway between the chord midpoint and the control.
    let cx = (x1 + x2) / 2.0 + 2.0 * bend * f[0];
    let cy = (y1 + y2) / 2.0 + 2.0 * bend * f[1];
    path([[x1, y1]]).quad(cx, cy, x2, y2)
}

// `curve` with an arrowhead at (x2, y2), following the curve's tangent.
fn curved_arrow(x1, y1, x2, y2, bend) { curve(x1, y1, x2, y2, bend).head("end") }

// A curly brace spanning the chord, its tip `depth` off it (at the middle) and
// its ends on (x1, y1) / (x2, y2) — e.g. `brace(0, 0, 6, 0, -0.4)` under a span.
fn brace(x1, y1, x2, y2, depth) {
    let f = chord_frame("brace", x1, y1, x2, y2);
    let nx = f[0];
    let ny = f[1];
    let len = f[2];
    let h = len / 2.0;
    let d = depth.to_float();
    let m = d / 2.0;
    let a = if d.abs() < h { d.abs() } else { h }; // shoulder length along the chord
    let k = 0.5523;                                // quarter-ellipse handle factor
    // Control/end points in the chord frame: [along, out].
    let local = [
        [0.0, k * m], [a - k * a, m], [a, m],                  // curl out of the start
        [h - a, m],                                            // straight run
        [h - a + k * a, m], [h, d - k * m], [h, d],            // up into the tip
        [h, d - k * m], [h + a - k * a, m], [h + a, m],        // back down
        [len - a, m],                                          // straight run
        [len - a + k * a, m], [len, k * m], [len, 0.0],        // curl into the end
    ];
    let q = [];
    for p in local {
        // along * (unit chord = (-ny, nx)) + out * (unit normal = (nx, ny))
        q.push([x1 - ny * p[0] + nx * p[1], y1 + nx * p[0] + ny * p[1]]);
    }
    path([[x1, y1]])
        .cubic(q[0][0], q[0][1], q[1][0], q[1][1], q[2][0], q[2][1])
        .to(q[3][0], q[3][1])
        .cubic(q[4][0], q[4][1], q[5][0], q[5][1], q[6][0], q[6][1])
        .cubic(q[7][0], q[7][1], q[8][0], q[8][1], q[9][0], q[9][1])
        .to(q[10][0], q[10][1])
        .cubic(q[11][0], q[11][1], q[12][0], q[12][1], x2, y2)
}

// A square bracket spanning the chord, its back `depth` off it.
fn bracket(x1, y1, x2, y2, depth) {
    let f = chord_frame("bracket", x1, y1, x2, y2);
    let ox = f[0] * depth;
    let oy = f[1] * depth;
    path([[x1, y1], [x1 + ox, y1 + oy], [x2 + ox, y2 + oy], [x2, y2]])
}
