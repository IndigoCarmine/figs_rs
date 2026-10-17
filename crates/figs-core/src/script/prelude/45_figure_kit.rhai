// ---------------------------------------------------------------------------
// Figure kit (lab icons, vector notes, colour bars, cycle arrows)
// ---------------------------------------------------------------------------
//
// Recurring figure furniture as functions: nothing takes coordinates. The
// icons are simple line art drawn with core paths and rects on a common
// height (`labware_size()`), so they line up in a `scheme`; the notes, bars
// and arrows size themselves from hooks and the panel they are given.

// `labware`: icon height (cm; 1.2 cm at 8 pt — a dish is 0.42 of it), line
// colour ("" = `fg()`, black), line width (cm), liquid colour and the
// default fill level (0 = empty .. 1 = full).
fn labware_size() { style_get("labware_size", base_size() * 0.15) }
fn labware_stroke() { style_get("labware_stroke", fg_or("#000000")) }
fn labware_stroke_width() { style_get("labware_stroke_width", base_size() * 0.0045) }
fn labware_liquid() { style_get("labware_liquid", "#9cc8ea") }
fn labware_level() { style_get("labware_level", 0) }

// `vector_note`: arrow colour (the label's too), width (cm), label size (pt)
// and the clear space around the label (cm).
fn vector_note_color() { style_get("vector_note_color", accent()) }
fn vector_note_width() { style_get("vector_note_width", annotation_width() * 1.5) }
fn vector_note_size() { style_get("vector_note_size", label_size()) }
fn vector_note_gap() { style_get("vector_note_gap", caption_gap() * 0.5) }

// `colorbar`: length (cm, or "fill": the space it is given), thickness (cm),
// label size (pt), gap between bar and labels (cm), border colour ("" for
// none) and width (cm).
fn colorbar_length() { style_get("colorbar_length", base_size() * 0.4) }
fn colorbar_thickness() { style_get("colorbar_thickness", base_size() * 0.03) }
fn colorbar_label_size() { style_get("colorbar_label_size", base_size()) }
fn colorbar_gap() { style_get("colorbar_gap", caption_gap() * 0.5) }
fn colorbar_border() { style_get("colorbar_border", fg_or("#000000")) }
fn colorbar_border_width() { style_get("colorbar_border_width", plot_line_width()) }

// `cycle_arrows`: arrow colour (both, unless `colors` is given), width (cm),
// how far the arcs bulge out from the panel (cm), how much of the panel's
// side they span (a fraction), and the gap to their labels (cm).
fn cycle_arrow_color() { style_get("cycle_arrow_color", accent()) }
fn cycle_arrow_width() { style_get("cycle_arrow_width", annotation_width()) }
fn cycle_arrow_depth() { style_get("cycle_arrow_depth", base_size() * 0.04) }
fn cycle_arrow_span() { style_get("cycle_arrow_span", 0.7) }
fn cycle_arrow_gap() { style_get("cycle_arrow_gap", caption_gap() * 0.5) }

// The kinds `labware` draws.
fn labware_kinds() {
    ["vial", "tube", "flask", "erlenmeyer", "beaker", "funnel", "dish", "cuvette", "syringe"]
}

// `labware(kind)` / `labware(kind, opts)`: a piece of lab glassware as line
// art, `labware_size()` tall (a dish 0.42 of that), its width following its
// shape: "vial" (capped), "tube" (test tube), "flask" (round-bottom),
// "erlenmeyer", "beaker", "funnel", "dish" (Petri dish, side view),
// "cuvette", "syringe" (needle down). Options:
//   level         liquid fill, 0 (empty) .. 1 (full) of the vessel
//                 (default `labware_level()`)
//   liquid        liquid colour or gradient (default `labware_liquid()`)
//   size          height, cm (default `labware_size()`)
//   stroke        line colour (default `labware_stroke()`)
//   stroke_width  line width, cm (default `labware_stroke_width()`)
//   lid           dish only: draw the lid (default true)
// A `stack` of fixed size (its lines inside the box): use it like an image,
// `step(labware("flask", #{ level: 0.5 }), "Step 1")`.
fn labware(kind) { labware(kind, #{}) }
fn labware(kind, opts) {
    check_opts("labware", opts, ["level", "liquid", "size", "stroke", "stroke_width", "lid"]);
    if !labware_kinds().contains(kind) {
        throw `labware(): unknown kind "${kind}" (${list_names(labware_kinds())})`;
    }
    let lv = opt(opts, "level", labware_level());
    if !finite_num(lv) || lv < 0 || lv > 1 {
        throw `labware(): level must be a number from 0 to 1 (got ${lv})`;
    }
    let size = opt(opts, "size", labware_size());
    let sw = opt(opts, "stroke_width", labware_stroke_width());
    if !finite_num(size) || !finite_num(sw) || !(sw >= 0) || !(size > sw * 2) {
        throw `labware(): size must be a length larger than the line width (got size ${size}, stroke_width ${sw})`;
    }
    let lid = opt(opts, "lid", true);
    if kind != "dish" && opts.contains("lid") { throw `labware(): lid is a "dish" option`; }
    // Unit box (height 1) -> cm: scale k, offset o (half a line in).
    let k = (size - sw).to_float();
    let o = sw.to_float() / 2.0;
    let g = labware_shape(kind, lv.to_float(), lid);
    let ink = opt(opts, "stroke", labware_stroke());
    let wet = opt(opts, "liquid", labware_liquid());
    let parts = [];
    for p in g.liquid { parts.push(lw_node(p, k, o).fill(wet)); }
    for p in g.lines {
        parts.push(lw_node(p, k, o).stroke(ink).stroke_width(sw * p.weight).join("round").cap("round"));
    }
    for p in g.solid { parts.push(lw_node(p, k, o).fill(ink)); }
    stack(parts).width(g.w * k + sw).height(g.h * k + sw)
}

// A drawing instruction in the unit box: #{ pts, arc (optional: [r, x, y,
// sweep, large] after `pts`, then `rest` points), closed, weight }.
fn lw_shape(pts, closed) { #{ pts: pts, arc: (), rest: [], closed: closed, weight: 1.0 } }
fn lw_thin(pts) { #{ pts: pts, arc: (), rest: [], closed: false, weight: 0.6 } }
fn lw_arced(pts, arc, rest, closed) { #{ pts: pts, arc: arc, rest: rest, closed: closed, weight: 1.0 } }

// Shape `p` as a core path, its unit points scaled by `k` and moved by `o`.
fn lw_node(p, k, o) {
    let q = [];
    for v in p.pts { q.push([o + v[0] * k, o + v[1] * k]); }
    let n = path(q);
    if type_of(p.arc) != "()" {
        let a = p.arc;
        n = n.arc(a[0] * k, o + a[1] * k, o + a[2] * k, #{ sweep: a[3], large: a[4] });
    }
    for v in p.rest { n = n.to(o + v[0] * k, o + v[1] * k); }
    if p.closed { n.closed() } else { n }
}

// The circle segment of radius `r` about (cx, cy) below the line y = `yl`
// (a liquid in a round bottom), as a closed shape.
fn lw_segment(cx, cy, r, yl) {
    let d = yl - cy;
    let hw = (r * r - d * d).max(0.0).sqrt();
    lw_arced([[cx - hw, yl]], [r, cx + hw, yl, "ccw", yl < cy], [], true)
}

// The geometry of `kind` in a unit box (height 1, or less for a dish):
// #{ w, h, liquid: [..], lines: [..], solid: [..] } of `lw_*` shapes, with
// the liquid at level `lv`.
fn labware_shape(kind, lv, lid) {
    let liquid = [];
    let lines = [];
    let solid = [];
    let w = 0.0;
    let h = 1.0;
    if kind == "vial" {
        w = 0.46;
        if lv > 0.0 {
            let yl = 1.0 - lv * 0.78;
            liquid.push(lw_shape([[0.04, yl], [0.42, yl], [0.42, 0.95], [0.38, 1.0], [0.08, 1.0], [0.04, 0.95]], true));
        }
        lines.push(lw_shape([[0.04, 0.2], [0.42, 0.2], [0.42, 0.95], [0.38, 1.0], [0.08, 1.0], [0.04, 0.95]], true));
        solid.push(lw_shape([[0.09, 0.0], [0.37, 0.0], [0.37, 0.2], [0.09, 0.2]], true));
    } else if kind == "tube" {
        w = 0.3;
        if lv > 0.0 {
            let yl = 0.96 - lv * 0.9;
            liquid.push(if yl >= 0.86 { lw_segment(0.15, 0.86, 0.1, yl) }
                        else { lw_arced([[0.05, yl], [0.05, 0.86]], [0.1, 0.25, 0.86, "ccw", false], [[0.25, yl]], true) });
        }
        lines.push(lw_arced([[0.0, 0.0], [0.05, 0.04], [0.05, 0.86]], [0.1, 0.25, 0.86, "ccw", false],
                            [[0.25, 0.04], [0.3, 0.0]], false));
    } else if kind == "flask" {
        // A sphere (r 0.38 about (0.4, 0.62)) under a neck 0.14 wide.
        w = 0.8;
        let yj = 0.62 - (0.38 * 0.38 - 0.07 * 0.07).sqrt();
        if lv > 0.0 { liquid.push(lw_segment(0.4, 0.62, 0.38, 1.0 - lv * (1.0 - yj - 0.04))); }
        lines.push(lw_arced([[0.29, 0.0], [0.33, 0.04], [0.33, yj]], [0.38, 0.47, yj, "ccw", true],
                            [[0.47, 0.04], [0.51, 0.0]], false));
    } else if kind == "erlenmeyer" {
        w = 0.8;
        if lv > 0.0 {
            let yl = (1.0 - lv * 0.62).min(0.95);
            let xl = 0.3 - (yl - 0.36) / 0.59 * 0.27;
            liquid.push(lw_shape([[xl, yl], [0.8 - xl, yl], [0.77, 0.95], [0.73, 1.0], [0.07, 1.0], [0.03, 0.95]], true));
        }
        lines.push(lw_shape([[0.27, 0.0], [0.3, 0.04], [0.3, 0.36], [0.03, 0.95], [0.07, 1.0], [0.73, 1.0],
                             [0.77, 0.95], [0.5, 0.36], [0.5, 0.04], [0.53, 0.0]], false));
    } else if kind == "beaker" {
        w = 0.8;
        if lv > 0.0 {
            let yl = 1.0 - lv * 0.9;
            liquid.push(lw_shape([[0.07, yl], [0.73, yl], [0.73, 0.97], [0.7, 1.0], [0.1, 1.0], [0.07, 0.97]], true));
        }
        lines.push(lw_shape([[0.0, 0.0], [0.07, 0.07], [0.07, 0.97], [0.1, 1.0], [0.7, 1.0], [0.73, 0.97],
                             [0.73, 0.04], [0.77, 0.0]], false));
        for y in [0.4, 0.58, 0.76] { lines.push(lw_thin([[0.07, y], [0.2, y]])); }
    } else if kind == "funnel" {
        w = 0.8;
        if lv > 0.0 {
            let yl = 0.5 - lv * 0.46;
            let xl = 0.34 - (0.5 - yl) / 0.5 * 0.32;
            liquid.push(lw_shape([[xl, yl], [0.8 - xl, yl], [0.46, 0.5], [0.34, 0.5]], true));
        }
        lines.push(lw_shape([[0.02, 0.0], [0.78, 0.0], [0.46, 0.5], [0.46, 1.0], [0.34, 0.92], [0.34, 0.5]], true));
    } else if kind == "dish" {
        w = 1.3;
        h = 0.42;
        if lv > 0.0 {
            let yl = 0.42 - lv * 0.3;
            liquid.push(lw_shape([[0.06, yl], [1.24, yl], [1.24, 0.42], [0.06, 0.42]], true));
        }
        lines.push(lw_shape([[0.06, 0.1], [0.06, 0.42], [1.24, 0.42], [1.24, 0.1]], false));
        if lid { lines.push(lw_shape([[0.0, 0.2], [0.0, 0.0], [1.3, 0.0], [1.3, 0.2]], false)); }
    } else if kind == "cuvette" {
        w = 0.42;
        if lv > 0.0 {
            let yl = 1.0 - lv * 0.86;
            liquid.push(lw_shape([[0.04, yl], [0.38, yl], [0.38, 1.0], [0.04, 1.0]], true));
        }
        lines.push(lw_shape([[0.04, 0.0], [0.38, 0.0], [0.38, 1.0], [0.04, 1.0]], true));
        lines.push(lw_thin([[0.04, 0.1], [0.38, 0.1]]));
        for x in [0.11, 0.31] { lines.push(lw_thin([[x, 0.1], [x, 1.0]])); }
    } else if kind == "syringe" {
        w = 0.42;
        if lv > 0.0 {
            let yl = 0.8 - lv * 0.5;
            liquid.push(lw_shape([[0.09, yl], [0.33, yl], [0.33, 0.8], [0.09, 0.8]], true));
        }
        lines.push(lw_shape([[0.21, 0.04], [0.21, 0.24]], false));            // rod
        lines.push(lw_shape([[0.0, 0.16], [0.42, 0.16]], false));             // flange
        lines.push(lw_shape([[0.09, 0.16], [0.33, 0.16], [0.33, 0.8], [0.09, 0.8]], true));
        lines.push(lw_shape([[0.15, 0.8], [0.27, 0.8], [0.24, 0.86], [0.18, 0.86]], true));
        lines.push(lw_thin([[0.21, 0.86], [0.21, 1.0]]));                      // needle
        for y in [0.42, 0.53, 0.64] { lines.push(lw_thin([[0.09, y], [0.15, y]])); }
        solid.push(lw_shape([[0.07, 0.0], [0.35, 0.0], [0.35, 0.04], [0.07, 0.04]], true));
        solid.push(lw_shape([[0.09, 0.24], [0.33, 0.24], [0.33, 0.3], [0.09, 0.3]], true));
    }
    #{ w: w, h: h, liquid: liquid, lines: lines, solid: solid }
}

// `vector_note(item, label)` / `(item, label, opts)`: a straight arrow drawn
// across `item` (a molecule image, a crystal photo: a dipole, a field, an
// axis), its `label` (a string, spans or a node; "" for none) just beyond the
// arrow head, in the arrow's colour (a string label haloed in `label_halo()`,
// as panel labels are). Positions are fractions of the item's
// box, not layout: the arrow fills a unit viewbox over the item, which keeps
// the label inside the item (`contain`: the arrow's box gives up what the
// label would hang out by). Options:
//   from, to     [fx, fy] fractions (default [0.2, 0.75] -> [0.8, 0.25], a
//                left-to-right diagonal upwards)
//   color        arrow and label colour (default `vector_note_color()`)
//   width        line width, cm (default `vector_note_width()`)
//   size         label size, pt (default `vector_note_size()`)
//   gap          clear space around the label, cm (default `vector_note_gap()`)
//   head         "end" (default), "both", "start", "none"
//   head_size, head_width  as for the core arrow
fn vector_note(item, label) { vector_note(item, label, #{}) }
fn vector_note(item, label, opts) {
    check_opts("vector_note", opts, ["from", "to", "color", "width", "size", "gap", "head",
                                     "head_size", "head_width"]);
    let f = numbers("vector_note", "`from`", opt(opts, "from", [0.2, 0.75]));
    let t = numbers("vector_note", "`to`", opt(opts, "to", [0.8, 0.25]));
    if f.len() != 2 || t.len() != 2 { throw "vector_note(): `from` and `to` are [fx, fy] fractions"; }
    let dx = t[0] - f[0];
    let dy = t[1] - f[1];
    let m = if dx.abs() > dy.abs() { dx.abs() } else { dy.abs() };
    if m == 0.0 { throw "vector_note(): `from` and `to` coincide"; }
    let head = opt(opts, "head", "end");
    if !["end", "both", "start", "none"].contains(head) {
        throw `vector_note(): unknown head "${head}" (end, both, start, none)`;
    }
    let c = opt(opts, "color", vector_note_color());
    let a = arrow(f[0], f[1], t[0], t[1]).viewbox(0, 0, 1, 1).stroke(c)
        .stroke_width(opt(opts, "width", vector_note_width())).head(head);
    if opts.contains("head_size") { a = a.head_size(opts.head_size); }
    if opts.contains("head_width") { a = a.head_width(opts.head_width); }
    let layers = [a];
    let none = type_of(label) == "()" || (type_of(label) == "string" && label == "");
    if !none {
        let lt = type_of(label);
        let lab = if lt == "string" || is_rich(label) {
            // Haloed like a panel label, so it reads over the drawing.
            let t = text(label).size(opt(opts, "size", vector_note_size())).color(c);
            let hc = label_halo();
            if hc == "" { t } else { halo(t, hc, label_halo_width()) }
        } else if lt == "Node" { label }
        else { throw `vector_note(): the label must be a string, spans or a node (got ${lt})`; };
        // Beyond the head: the label's box touches the tip from the side the
        // arrow comes from (a corner for a diagonal, an edge's middle for a
        // level or upright arrow).
        let ax = 0.5 - 0.5 * dx / m;
        let ay = 0.5 - 0.5 * dy / m;
        layers.push(col([lab]).pad(opt(opts, "gap", vector_note_gap())).at(t[0], t[1]).anchor(ax, ay));
    }
    // Centred in whatever box it is given, hugging the item (a stretched
    // stack would spread the arrow past a fixed-size item).
    col([stack([as_node(item), stack(layers).viewbox(0, 0, 1, 1).contain()])])
        .main("center").cross("center")
}

// `colorbar(fill, labels)` / `(fill, labels, opts)`: a colour scale — a bar
// filled with `fill` (an array of colours, spread evenly as a gradient, or
// of [offset, colour] stops; or a ready paint, `linear(..)`) with `labels`
// (strings, numbers or spans; "" for a gap) spread evenly along it: the
// first at the start (left / bottom), the last at the end, flush with the
// bar's ends, the others centred on their place. Options:
//   direction  "horizontal" (default; labels below) or "vertical" (low at
//              the bottom; labels right)
//   length     cm, or "fill" (the length its parent gives it); default
//              `colorbar_length()`
//   thickness  cm (default `colorbar_thickness()`)
//   size       label size, pt (default `colorbar_label_size()`)
//   gap        bar to labels, cm (default `colorbar_gap()`)
//   border     colour ("" for none; default `colorbar_border()`)
//   discrete   true: one block per colour instead of a gradient
//   title      a caption above the bar (a string, spans or a node)
fn colorbar(fill, labels) { colorbar(fill, labels, #{}) }
fn colorbar(fill, labels, opts) {
    check_opts("colorbar", opts, ["direction", "length", "thickness", "size", "gap", "border",
                                  "discrete", "title"]);
    let dir = opt(opts, "direction", "horizontal");
    if dir != "horizontal" && dir != "vertical" {
        throw `colorbar(): unknown direction "${dir}" (horizontal, vertical)`;
    }
    let vertical = dir == "vertical";
    if type_of(labels) != "array" || is_rich(labels) { labels = [labels]; }
    let th = opt(opts, "thickness", colorbar_thickness());
    let len = opt(opts, "length", colorbar_length());
    if type_of(len) != "string" || len != "fill" {
        if !finite_num(len) || !(len > 0) { throw `colorbar(): length must be a length in cm or "fill" (got ${len})`; }
    }
    let border = opt(opts, "border", colorbar_border());
    // The bar: a gradient (or solid) rect, or a row / column of blocks.
    let bar = ();
    if opt(opts, "discrete", false) {
        if type_of(fill) != "array" || fill.len() == 0 { throw "colorbar(): discrete needs an array of colours"; }
        let blocks = [];
        for x in fill {
            let col_ = if type_of(x) == "array" { x[1] } else { x };
            blocks.push(rect().fill(col_).flex(1));
        }
        if vertical { blocks.reverse(); }
        bar = if vertical { col(blocks).stretch() } else { row(blocks).stretch() };
        // The border over the blocks (a container's own is painted under them).
        if border != "" {
            bar = stack([bar, rect().stroke(border).stroke_width(colorbar_border_width())]);
        }
    } else {
        let paint = if type_of(fill) == "array" { linear(if vertical { 270 } else { 0 }, fill) } else { fill };
        bar = rect().fill(paint);
        if border != "" { bar = bar.stroke(border).stroke_width(colorbar_border_width()); }
    }
    bar = if vertical { bar.width(th) } else { bar.height(th) };
    // The labels: a unit viewbox stack along the bar (it measures as its
    // largest label across).
    let n = labels.len();
    let size = opt(opts, "size", colorbar_label_size());
    let ls = [];
    for i in 0..n {
        let x = labels[i];
        if type_of(x) == "string" && x == "" { continue; }
        if type_of(x) == "i64" || type_of(x) == "f64" { x = `${x}`; }
        let t = text_or_node("colorbar", x, size, "center");
        let p = if n == 1 { 0.5 } else { i.to_float() / (n - 1).to_float() };
        let e = if n == 1 { 0.5 } else if i == 0 { 0.0 } else if i == n - 1 { 1.0 } else { 0.5 };
        ls.push(if vertical { t.at(0, 1.0 - p).anchor(0, if e == 0.5 { "cap_middle" } else { 1.0 - e }) }
                else { t.at(p, 0).anchor(e, 0) });
    }
    let g = opt(opts, "gap", colorbar_gap());
    let fixed = type_of(len) != "string";
    let body = if vertical {
        let r = row([bar, stack(ls).viewbox(0, 0, 1, 1)]).spacing(g).stretch();
        if fixed { r.height(len) } else { r }
    } else {
        let c = col([bar, stack(ls).viewbox(0, 0, 1, 1)]).spacing(g).stretch();
        if fixed { c.width(len) } else { c }
    };
    let title = opt(opts, "title", ());
    if type_of(title) == "()" { return body; }
    let head = text_or_node("colorbar", title, size, if vertical { "start" } else { "center" });
    // A filling bar fills what is left under its title.
    if !fixed { return col([head, body.flex(1)]).spacing(g).stretch(); }
    col([head, body]).spacing(g).cross(if vertical { "start" } else { "center" })
}

// `cycle_arrows(item)` / `(item, opts)`: two curved arrows around `item` (a
// panel, a structure, or a string: a label) — a rotation, a reversible
// switch. Above and below it by default, both turning one way, each spanning
// `cycle_arrow_span()` of the item's side and bulging `cycle_arrow_depth()`
// out; labels optional, beside each arrow in its colour. Options:
//   direction  "cw" (default) or "ccw"
//   sides      "top-bottom" (default) or "left-right"
//   colors     [first, second] (top/left first), or `color` for both
//              (default `cycle_arrow_color()`)
//   labels     [first, second] (strings, spans or nodes; "" / () for none)
//   width      line width, cm (default `cycle_arrow_width()`)
//   depth      cm (default `cycle_arrow_depth()`); span: a fraction
//   gap        arrow to label, cm (default `cycle_arrow_gap()`)
//   size       label size, pt (default `base_size()`)
// The arcs are core `curved_arrow`s in unit viewboxes, so they follow the
// item's size. They stay inside the box the result is given: beside the
// item ("left-right") it shrinks to leave them room; above and below it
// they add to its height.
fn cycle_arrows(item) { cycle_arrows(item, #{}) }
fn cycle_arrows(item, opts) {
    check_opts("cycle_arrows", opts, ["direction", "sides", "colors", "color", "labels", "width",
                                      "depth", "span", "gap", "size"]);
    let dir = opt(opts, "direction", "cw");
    if dir != "cw" && dir != "ccw" { throw `cycle_arrows(): unknown direction "${dir}" (cw, ccw)`; }
    let sides = opt(opts, "sides", "top-bottom");
    if sides != "top-bottom" && sides != "left-right" {
        throw `cycle_arrows(): unknown sides "${sides}" (top-bottom, left-right)`;
    }
    let c = opt(opts, "color", cycle_arrow_color());
    let cs = opt(opts, "colors", [c, c]);
    if type_of(cs) != "array" || cs.len() != 2 { throw "cycle_arrows(): colors must be [first, second]"; }
    let labels = opt(opts, "labels", [(), ()]);
    if type_of(labels) != "array" || labels.len() != 2 || is_rich(labels) {
        throw "cycle_arrows(): labels must be [first, second]";
    }
    let span = opt(opts, "span", cycle_arrow_span());
    if !finite_num(span) || !(span > 0) || span > 1 { throw `cycle_arrows(): span must be a fraction in (0, 1] (got ${span})`; }
    let a = (1.0 - span) / 2.0;
    let b = 1.0 - a;
    let w = opt(opts, "width", cycle_arrow_width());
    let depth = opt(opts, "depth", cycle_arrow_depth());
    let lr = sides == "left-right";
    // Clockwise chords (the item side of each band), bulging outwards by a
    // positive bend; anticlockwise runs them backwards, bent the other way.
    let chords = if lr { [[1.0, b, 1.0, a], [0.0, a, 0.0, b]] } else { [[a, 1.0, b, 1.0], [b, 0.0, a, 0.0]] };
    let bend = 0.75;
    let bands = [];
    for i in 0..2 {
        let q = chords[i];
        let arc = if dir == "cw" { curved_arrow(q[0], q[1], q[2], q[3], bend) }
                  else { curved_arrow(q[2], q[3], q[0], q[1], -bend) };
        arc = arc.viewbox(0, 0, 1, 1).stroke(cs[i]).stroke_width(w);
        bands.push(if lr { arc.width(depth) } else { arc.height(depth) });
    }
    let size = opt(opts, "size", base_size());
    let labs = [];
    for i in 0..2 {
        let x = labels[i];
        let none = type_of(x) == "()" || (type_of(x) == "string" && x == "");
        labs.push(if none { () } else {
            let t = type_of(x);
            if t == "string" || is_rich(x) { text(x).size(size).color(cs[i]).center() }
            else if t == "Node" { x }
            else { throw `cycle_arrows(): a label must be a string, spans or a node (got ${t})`; }
        });
    }
    let g = opt(opts, "gap", cycle_arrow_gap());
    let n = if type_of(item) == "string" || is_rich(item) { fg_text(item).size(size) } else { as_node(item) };
    let parts = [];
    if lr {
        if type_of(labs[0]) != "()" { parts.push(col([labs[0]]).main("center")); parts.push(rect().width(g)); }
        // The item takes what the arcs and labels leave: it shrinks to
        // keep them inside the box.
        parts += [bands[0], col([n]).main("center").cross("center").flex(1), bands[1]];
        if type_of(labs[1]) != "()" { parts.push(rect().width(g)); parts.push(col([labs[1]]).main("center")); }
        // In a row of its own the arcs' row is sized to its content: the
        // item cell (flex) is its own width, at most what the box leaves.
        return col([row([row(parts).stretch()])]).main("center").cross("center");
    }
    if type_of(labs[0]) != "()" { parts.push(col([labs[0]]).cross("center")); parts.push(rect().height(g)); }
    parts += [bands[0], row([n]).main("center").cross("center"), bands[1]];
    if type_of(labs[1]) != "()" { parts.push(rect().height(g)); parts.push(col([labs[1]]).cross("center")); }
    // Centred in its box, the arcs as wide as the item (or its labels).
    col([col(parts).stretch()]).main("center").cross("center")
}
