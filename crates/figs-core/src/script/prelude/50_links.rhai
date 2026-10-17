// ---------------------------------------------------------------------------
// Links (core connectors between laid-out nodes), piles and backdrops
// ---------------------------------------------------------------------------
//
// The core `connect(a, b, opts)` draws a line (or a wide band) between two
// nodes AFTER layout, wherever they end up — `a` / `b` are node values (or
// `.id` names) — and takes no room in its parent. The helpers below return
// such connectors (or arrays of them) to drop anywhere in the tree: an array
// among a container's children is spliced in,
//   col([row([photo, detail]), zoom_link(photo, detail, #{ region: r })])
// and where they sit only decides what they are painted over.

// `zoom_link`: colour and width of the region outline and the lines to the
// magnified copy; the lines' dash (cm) and the outline's ([] = solid);
// whether the copy gets a frame in the same colour.
fn zoom_color() { style_get("zoom_color", accent()) }
fn zoom_width() { style_get("zoom_width", annotation_width()) }
fn zoom_dash() { style_get("zoom_dash", annotation_dash()) }
fn zoom_region_dash() { style_get("zoom_region_dash", []) }
fn zoom_copy_frame() { style_get("zoom_copy_frame", true) }
// The region's outline: "rect" (default), "circle" (the inscribed ellipse;
// the lines leave from its extreme points) or "none"; the link itself:
// "lines" (default: two lines) or "cone" (a translucent filled quad from
// the region to the copy, `zoom_cone_opacity()` of the colour).
fn zoom_outline() { style_get("zoom_outline", "rect") }
fn zoom_style() { style_get("zoom_style", "lines") }
fn zoom_cone_opacity() { style_get("zoom_cone_opacity", 0.25) }

// `callout_to`: where the label goes (an overlay position inside the item,
// or "left" / "right" / "above" / "below" beside it), the leader's colour,
// width, the clear space between label and leader (cm), the label size (pt)
// and the label-to-item gap of an outside label (cm).
fn callout_position() { style_get("callout_position", "top-right") }
fn callout_color() { style_get("callout_color", plot_fg()) }
fn callout_width() { style_get("callout_width", annotation_width() * 0.75) }
fn callout_gap() { style_get("callout_gap", caption_gap() * 0.5) }
fn callout_size() { style_get("callout_size", base_size()) }
fn callout_side_gap() { style_get("callout_side_gap", gap()) }

// `flow_link`: colour, width, corner radius (cm) of the elbow, and the gap
// left at each end (cm).
fn flow_link_color() { style_get("flow_link_color", plot_fg()) }
fn flow_link_width() { style_get("flow_link_width", annotation_width()) }
fn flow_link_radius() { style_get("flow_link_radius", base_size() * 0.02) }
fn flow_link_gap() { style_get("flow_link_gap", 0) }

// `block_link` / `block_arrow_bent`: band width (cm), fill (a colour or a
// gradient) and opacity of wide arrows between or behind nodes.
fn block_link_width() { style_get("block_link_width", base_size() * 0.06) }
fn block_link_color() { style_get("block_link_color", accent()) }
fn block_link_opacity() { style_get("block_link_opacity", 0.35) }

// `pile`: offset of each copy behind the one in front ([dx, dy] cm; up and
// to the right by default) and the opacity of the copies behind.
fn pile_offset() { style_get("pile_offset", [base_size() * 0.012, -base_size() * 0.012]) }
fn pile_opacity() { style_get("pile_opacity", 1.0) }

// `backdrop`: panel colour (translucent white), padding and corner radius
// (cm).
fn backdrop_color() { style_get("backdrop_color", "#ffffffcc") }
fn backdrop_pad() { style_get("backdrop_pad", base_size() * 0.02) }
fn backdrop_radius() { style_get("backdrop_radius", base_size() * 0.015) }

// `zoom_link`: how the lines / cone attach — "facing" (the facing sides'
// corners; right for side-by-side or stacked boxes) or "hull" (the outer
// tangents of region and copy together, for diagonal placements too).
fn zoom_attach() { style_get("zoom_attach", "facing") }

// Options of a link helper: `defaults` with the user's `opts` over them
// (`connect` checks the keys).
fn link_opts(who, defaults, opts) {
    if type_of(opts) != "map" { throw `${who}(): options must be a map (got ${type_of(opts)})`; }
    for k in opts.keys() { defaults[k] = opts[k]; }
    defaults
}

// `zoom_link(item, copy, opts)`: the classic magnification inset — the
// region `region` of `item` (an image, a panel; [fx0, fy0, fx1, fy1]
// fractions of its box) outlined, and two lines from the region's corners
// to the nearest corners of `copy` (the magnified view: an `inset(..)`, an
// `overlay(..)` item, or a panel beside it), chosen after layout so they
// never cross the boxes. An array of connectors. Options:
//   region       [fx0, fy0, fx1, fy1] (default [0.4, 0.4, 0.6, 0.6])
//   color        outline and lines (default `zoom_color()`)
//   width        cm (default `zoom_width()`)
//   dash         the lines' dash (default `zoom_dash()`; [] solid)
//   region_dash  the outline's dash (default `zoom_region_dash()`: solid)
//   frame        frame the copy too (default `zoom_copy_frame()`)
//   outline      the region's outline: "rect", "circle" (the inscribed
//                ellipse; the lines leave from its top and bottom — or
//                left and right — points) or "none" (default
//                `zoom_outline()`)
//   style        "lines" (two lines) or "cone": a translucent filled quad
//                from the region to the copy, a light / projection cone
//                (default `zoom_style()`)
//   opacity      of the cone's fill (default `zoom_cone_opacity()`)
//   attach       "facing" or "hull": the lines / cone along the outer
//                tangents of region and copy together — right when the
//                copy sits diagonally from the region (default
//                `zoom_attach()`)
// outline "none" draws no region at all: the lines / cone still start at it.
fn zoom_link(item, copy) { zoom_link(item, copy, #{}) }
fn zoom_link(item, copy, opts) {
    let who = "zoom_link";
    check_opts(who, opts, ["region", "color", "width", "dash", "region_dash", "frame",
                           "outline", "style", "opacity", "attach"]);
    let r = numbers(who, "`region`", opt(opts, "region", [0.4, 0.4, 0.6, 0.6]));
    if r.len() != 4 || !(r[2] > r[0]) || !(r[3] > r[1]) {
        throw `${who}(): region is [fx0, fy0, fx1, fy1] fractions of the item's box (fx0 < fx1, fy0 < fy1)`;
    }
    let shape = opt(opts, "outline", zoom_outline());
    if !["rect", "circle", "none"].contains(shape) {
        throw `${who}(): unknown outline "${shape}" (rect, circle, none)`;
    }
    let st = opt(opts, "style", zoom_style());
    if !["lines", "cone"].contains(st) {
        throw `${who}(): unknown style "${st}" (lines, cone)`;
    }
    let att = opt(opts, "attach", zoom_attach());
    if !["facing", "hull"].contains(att) {
        throw `${who}(): unknown attach "${att}" (facing, hull)`;
    }
    let color = opt(opts, "color", zoom_color());
    let base = #{ stroke: color, width: opt(opts, "width", zoom_width()) };
    let rd = opt(opts, "region_dash", zoom_region_dash());
    let fs = if shape == "circle" { "ellipse" } else { "box" };
    let out = [];
    // The cone first: drawn under the outlines.
    if st == "cone" {
        let c = #{ route: "cone", from_box: r, from_shape: fs, fill: color,
                   opacity: opt(opts, "opacity", zoom_cone_opacity()) };
        if att == "hull" { c.from = "hull-1"; c.to = "hull-1"; }
        out.push(connect(item, copy, c));
    }
    if shape == "rect" {
        out += link_box(item, r, base + #{ dash: rd });
    } else if shape == "circle" {
        out.push(connect(item, item, base + #{ route: "outline", from_box: r, from_shape: "ellipse", dash: rd }));
    }
    if opt(opts, "frame", zoom_copy_frame()) {
        out += link_box(copy, (), base + #{ dash: rd });
    }
    if st == "lines" {
        let d = opt(opts, "dash", zoom_dash());
        let ks = if att == "hull" { ["hull-1", "hull-2"] } else { ["facing-1", "facing-2"] };
        for k in ks {
            out.push(connect(item, copy, base + #{ from_box: r, from_shape: fs, from: k, to: k, dash: d }));
        }
    }
    out
}

// The outline of box `r` of `node` (() = the whole box) as four connectors
// (square caps close the corners).
fn link_box(node, r, o) {
    let cs = ["top-left", "top-right", "bottom-right", "bottom-left"];
    let out = [];
    for i in 0..4 {
        let e = o + #{ from: cs[i], to: cs[(i + 1) % 4], cap: "square" };
        if type_of(r) != "()" { e.from_box = r; e.to_box = r; }
        out.push(connect(node, node, e));
    }
    out
}

// `callout_to(item, label)` / `(…, opts)`: `label` (a string — inline
// markup —, spans or a node) placed in a free spot of `item` or beside it,
// with a leader line to the point `at` of the item. The item with the label
// and the leader (a core stack, or a row / column for an outside label).
// Options:
//   at        [fx, fy] the point the leader ends at, fractions of the item's
//             box (default [0.5, 0.5])
//   position  an overlay position over the item ("top-left", "top", …,
//             "bottom-right") or "left" / "right" / "above" / "below" beside
//             it (default `callout_position()`)
//   color     leader colour (default `callout_color()`)
//   width     cm (default `callout_width()`)
//   gap       clear space between label and leader (default `callout_gap()`)
//   head      "none" (default), "end" (an arrow at the point)
//   size      label size, pt (default `callout_size()`)
//   route     "straight" (default), "elbow", "curve"
// Inside the item the label is in the overlay text style; nest calls for
// several callouts: `callout_to(callout_to(img, "A", ..), "B", ..)`.
fn callout_to(item, label) { callout_to(item, label, #{}) }
fn callout_to(item, label, opts) {
    let who = "callout_to";
    check_opts(who, opts, ["at", "position", "color", "width", "gap", "head", "size", "route"]);
    let at = numbers(who, "`at`", opt(opts, "at", [0.5, 0.5]));
    if at.len() != 2 { throw `${who}(): at is [fx, fy] fractions of the item's box`; }
    let pos = opt(opts, "position", callout_position());
    let size = opt(opts, "size", callout_size());
    let sides = ["left", "right", "above", "below"];
    let inside = ();
    for p in position_table() { if p[0] == pos { inside = p; } }
    if type_of(inside) == "()" && !sides.contains(pos) {
        let names = [];
        for p in position_table() { names.push(p[0]); }
        throw `${who}(): unknown position "${pos}" (${list_names(names + sides)})`;
    }
    let n = as_node(item);
    let lab = if type_of(inside) != "()" {
        let t = overlay_text(who, label, inside[2]);
        if type_of(label) == "string" || is_rich(label) { t = t.size(size); }
        t
    } else {
        text_or_node(who, label, size, if pos == "left" { "end" } else { "start" })
    };
    let leader = connect(lab, n, #{
        to: at, stroke: opt(opts, "color", callout_color()),
        width: opt(opts, "width", callout_width()), gap: [opt(opts, "gap", callout_gap()), 0],
        head: opt(opts, "head", "none"), route: opt(opts, "route", "straight") });
    let g = callout_side_gap();
    if pos == "right" { return row([n, lab, leader]).spacing(g).cross("center"); }
    if pos == "left" { return row([lab, n, leader]).spacing(g).cross("center"); }
    if pos == "above" { return col([lab, n, leader]).spacing(g).cross("center"); }
    if pos == "below" { return col([n, lab, leader]).spacing(g).cross("center"); }
    let m = #{};
    m[pos] = lab;
    stack([overlay(n, m), leader])
}

// `flow_link(a, b)` / `(…, opts)`: a flowchart link from node `a` to node
// `b` — an elbow route (horizontal / vertical runs from the facing sides,
// corners rounded by `flow_link_radius()`) with an arrow head, in
// `flow_link_color()`, `flow_link_width()` wide. `opts` are `connect`
// options over these (`route: "straight"`, `from: "bottom"`, `dash`, …).
fn flow_link(a, b) { flow_link(a, b, #{}) }
fn flow_link(a, b, opts) {
    connect(a, b, link_opts("flow_link", #{
        route: "elbow", head: "end", stroke: flow_link_color(), width: flow_link_width(),
        radius: flow_link_radius(), gap: flow_link_gap() }, opts))
}

// `block_link(a, b)` / `(…, opts)`: a wide translucent arrow (a filled band
// with a triangular head) from node `a` to node `b`, `block_link_width()`
// wide, filled with `block_link_color()` at `block_link_opacity()`. Options
// are `connect` options, with `width` the band's width and `color` its fill
// (a colour or a gradient): `#{ route: "elbow", from: "bottom", to: "left",
// width: 0.8, color: linear(0, ["#ff000040", "#ff0000"]) }`.
fn block_link(a, b) { block_link(a, b, #{}) }
fn block_link(a, b, opts) {
    if type_of(opts) != "map" { throw `block_link(): options must be a map (got ${type_of(opts)})`; }
    let o = #{};
    for k in opts.keys() {
        if k == "width" { o.band = opts.width; } else if k == "color" { o.fill = opts.color; } else { o[k] = opts[k]; }
    }
    let w = opt(o, "band", block_link_width());
    connect(a, b, link_opts("block_link", #{
        band: w, fill: block_link_color(), opacity: block_link_opacity(), head: "end",
        radius: w }, o))
}

// `block_arrow_bent(opts)`: a wide arrow through a group, for `behind(group,
// ..)`: from the point `from` of the group's box to `to` (sides, corners or
// [fx, fy]; default "left" -> "bottom"), along an elbow (or `route`) —
// across the group's middle when the two sides meet at a corner. Options as
// for `block_link` (`width`, `color`, `opacity`, `head`, `radius`, `gap`:
// how far the ends reach out of the box, …).
fn block_arrow_bent() { block_arrow_bent(#{}) }
fn block_arrow_bent(opts) {
    if type_of(opts) != "map" { throw `block_arrow_bent(): options must be a map (got ${type_of(opts)})`; }
    #{ mark: "block_arrow_bent", opts: opts }
}

// `behind(group, layers)`: `layers` (a node, a `block_arrow_bent(..)`, or an
// array of them) drawn behind `group`, which keeps its own size: a core
// `stack([..layers, group])`.
fn behind(group, layers) {
    let sl = split_links(layers);
    layers = sl.items + sl.links;
    let g = as_node(group);
    let out = [];
    for x in layers {
        if type_of(x) == "map" && x.contains("mark") && x.mark == "block_arrow_bent" {
            out.push(block_link(g, g, link_opts("block_arrow_bent", #{
                from: "left", to: "bottom", route: "elbow" }, x.opts)));
        } else if type_of(x) == "Node" {
            out.push(x);
        } else {
            throw `behind(): a layer is a node or block_arrow_bent(..) (got ${type_of(x)})`;
        }
    }
    stack(out + [g])
}

// `pile(node, n)` / `(…, opts)`: `n` copies of `node` stacked, each behind
// the one in front shifted by `offset` — stacked discs, sheets, cuvettes.
// The front copy is the node itself (a connector to `node` attaches to it).
// Options: `offset` [dx, dy] cm per copy (default `pile_offset()`: up and to
// the right), `opacity` of the copies behind (default `pile_opacity()`).
// Give `node` a size (`.width` / `.height`, or an icon).
fn pile(node, n) { pile(node, n, #{}) }
fn pile(node, n, opts) {
    check_opts("pile", opts, ["offset", "opacity"]);
    if type_of(n) != "i64" || n < 1 { throw `pile(): n must be a positive integer (got ${n})`; }
    let off = numbers("pile", "`offset`", opt(opts, "offset", pile_offset()));
    if off.len() != 2 { throw "pile(): offset is [dx, dy] cm"; }
    let op = opt(opts, "opacity", pile_opacity()).to_float();
    let x0 = if off[0] < 0.0 { -(n - 1).to_float() * off[0] } else { 0.0 };
    let y0 = if off[1] < 0.0 { -(n - 1).to_float() * off[1] } else { 0.0 };
    let m = as_node(node);
    let parts = [];
    for i in 0..n {
        let k = (n - 1 - i).to_float();   // copies from the back
        let c = m.at(x0 + k * off[0], y0 + k * off[1]);
        if k > 0.0 && op < 1.0 { c = c.opacity(op); }
        parts.push(c);
    }
    stack(parts)
}

// `backdrop(node)` / `(…, opts)`: a translucent rounded panel behind `node`
// (over a photo or a busy plot), hugging it. Options: `color` (default
// `backdrop_color()`), `pad` (cm or [v, h], `backdrop_pad()`), `radius`
// (`backdrop_radius()`), `stroke` / `stroke_width` for a border.
fn backdrop(node) { backdrop(node, #{}) }
fn backdrop(node, opts) {
    check_opts("backdrop", opts, ["color", "pad", "radius", "stroke", "stroke_width"]);
    let b = col([as_node(node)]).pad(opt(opts, "pad", backdrop_pad()))
        .fill(opt(opts, "color", backdrop_color())).radius(opt(opts, "radius", backdrop_radius()))
        .cross("center").main("center");
    if opts.contains("stroke") {
        b = b.stroke(opts.stroke).stroke_width(opt(opts, "stroke_width", annotation_width()));
    }
    b
}
