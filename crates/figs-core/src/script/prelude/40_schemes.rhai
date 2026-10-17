// ---------------------------------------------------------------------------
// Schemes (reaction schemes, process flows: a row of nodes and arrows)
// ---------------------------------------------------------------------------
//
// `scheme([a, arrow_step("THF, 60 °C"), b])` lays a sequence out as a row
// with every item centred on one axis: nodes (molecule images, icons),
// `step`s (a node with captions) and `arrow_step` arrows with their
// conditions above / below. Nothing is positioned by hand: the scheme lines
// its items up by their *axes* (the core `.cross("axis")`), and a step or an
// arrow marks its node (`.axis()`) as the axis, so each node and arrow stays
// on the centre line however long its captions are — and a caption takes
// only its own room. `scheme_v` is the same down the page.

// Between the items and arrows of a `scheme` (cm): the default gap.
fn scheme_gap() { style_get("scheme_gap", gap()) }

// Length of a scheme arrow (cm): 1.6 cm at the default 8 pt, 4.8 cm on a
// 24 pt poster. `arrow_step(#{ length: "fill" })` stretches one instead.
fn scheme_arrow_length() { style_get("scheme_arrow_length", base_size() * 0.2) }

// Stroke width of scheme arrows (cm): the annotation stroke.
fn scheme_arrow_width() { style_get("scheme_arrow_width", annotation_width()) }
// The shortest a "fill" arrow gets (cm) when its scheme has no spare room
// to share (a scheme that isn't stretched across its parent).
fn scheme_fill_min_length() { style_get("scheme_fill_min_length", scheme_arrow_length()) }

// Scheme text (`step` captions, arrow labels): size (pt) and its gap to the
// node or arrow it belongs to (cm).
fn scheme_label_size() { style_get("scheme_label_size", base_size()) }
fn scheme_label_gap() { style_get("scheme_label_gap", caption_gap()) }

// `step(node, #{ above, below, left, right })`: `node` with captions (a
// string, spans or a node each; strings at `scheme_label_size()`, centred),
// `scheme_label_gap()` from it; `step(node, caption)` puts one below (a
// string, spans or a node). The node is the step's axis, so in a scheme (or
// any `.cross("axis")` container) it sits on the axis.
fn step(node) { as_node(node) }
fn step(node, opts) {
    if type_of(opts) != "map" { opts = #{ below: opts }; }
    check_opts("step", opts, ["above", "below", "left", "right", "overhang"]);
    let o = opts;
    o.overhang = overhang_mode("step", opt(o, "overhang", caption_overhang()));
    scheme_dress("step", as_node(node), o, false).meta("step", [node, opts])
}
// `step(node, caption, opts)`: one caption below, with options
// (`overhang`; `above`, `left`, `right` too).
fn step(node, caption, opts) {
    if type_of(opts) != "map" { throw `step(): options must be a map (got ${type_of(opts)})`; }
    let o = opts;
    o.below = caption;
    step(node, o)
}

// `arrow_step(label)` / `arrow_step(opts)`: an arrow for `scheme` with
// `label` (a string, spans or a node) above it. Options:
//   above, below  text above / below the arrow (in a vertical scheme:
//                 right / left of it; `left` / `right` also work there)
//   length        cm (default `scheme_arrow_length()`, or the scheme's
//                 `arrow_length`), or "fill": share the row's spare width
//   head          "end" (default), "both" (↔), "start", "none"
//   equilibrium   true, or a map of `equilibrium` options (gap, head,
//                 head_size, head_width, barb, …): the ⇌ pair instead
//   color         default `fg()` (black); width: cm (`scheme_arrow_width()`)
//   head_size, head_width  as for the core arrow (default 5 × / 4 × width)
// A descriptor (a map) that `scheme` draws.
fn arrow_step() { arrow_step(#{}) }
fn arrow_step(x) {
    let o = if type_of(x) == "map" { x } else { #{ above: x } };
    check_opts("arrow_step", o, ["above", "below", "left", "right", "length", "head", "equilibrium",
                                 "color", "width", "head_size", "head_width"]);
    let head = opt(o, "head", "end");
    if !["end", "both", "start", "none"].contains(head) {
        throw `arrow_step(): unknown head "${head}" (end, both, start, none)`;
    }
    let eq = opt(o, "equilibrium", false);
    if type_of(eq) == "bool" {
        eq = if eq { #{} } else { () };
    } else if type_of(eq) != "map" {
        throw `arrow_step(): equilibrium must be true, false or a map of equilibrium options (got ${type_of(eq)})`;
    }
    let len = opt(o, "length", ());
    if type_of(len) != "()" { len = scheme_length("arrow_step", len); }
    #{ scheme_arrow: true, opts: o, head: head, eq: eq, length: len }
}

// `scheme(items)` / `scheme(items, opts)`: `items` (nodes, strings such as
// "+", `step`s, `arrow_step`s) in a row `scheme_gap()` apart, centred on one
// axis and in the row. Options:
//   direction     "right" (default) or "down" (a column, as `scheme_v`)
//   gap           between items, cm (default `scheme_gap()`)
//   align         across the flow: "center" (default), "start", "end"
//   arrow_length  of every arrow without its own: cm or "fill" — sharing
//                 the spare width of a scheme stretched across its parent;
//                 never shorter than `scheme_fill_min_length()`
//                 (default `scheme_arrow_length()`)
// Images need a size (`image("a.png").height(2)`: the width follows).
fn scheme(items) { scheme(items, #{}) }
fn scheme(items, opts) {
    if type_of(items) != "array" { throw `scheme(): expected an array of items (got ${type_of(items)})`; }
    check_opts("scheme", opts, ["direction", "gap", "align", "arrow_length"]);
    let dir = opt(opts, "direction", "right");
    if dir != "right" && dir != "down" { throw `scheme(): unknown direction "${dir}" (right, down)`; }
    let vertical = dir == "down";
    let align = opt(opts, "align", "center");
    if !["center", "start", "end"].contains(align) {
        throw `scheme(): unknown align "${align}" (center, start, end)`;
    }
    let len = scheme_length("scheme", opt(opts, "arrow_length", scheme_arrow_length()));
    let sl = split_links(items);
    let out = [];
    for x in sl.items {
        if type_of(x) == "map" {
            if !x.contains("scheme_arrow") { throw "scheme(): a map item must be an arrow_step(..)"; }
            out.push(scheme_arrow(x, vertical, len));
        } else {
            out.push(text_or_node("scheme", x, base_size(), "center"));
        }
    }
    out += sl.links;
    let c = if vertical { col(out) } else { row(out) };
    c.spacing(opt(opts, "gap", scheme_gap())).cross(if align == "center" { "axis" } else { align })
        .main("center")
}

// `scheme` down the page: arrows point down, their labels right / left.
fn scheme_v(items) { scheme(items, #{ direction: "down" }) }
fn scheme_v(items, opts) {
    if type_of(opts) != "map" { throw `scheme_v(): options must be a map (got ${type_of(opts)})`; }
    opts.direction = "down";
    scheme(items, opts)
}

// A scheme arrow length: a positive length (cm, as a float) or "fill".
fn scheme_length(who, v) {
    let t = type_of(v);
    if t == "string" && v == "fill" { return v; }
    if (t == "i64" || t == "f64") && v > 0 { return v.to_float(); }
    throw `${who}(): an arrow length is a positive length in cm or "fill" (got ${v})`;
}

// `n` with the captions in `labels` (above, below, left, right; () or
// missing for none): a column (row) of caption, `n`, caption aligned by
// their axes, `n` marked as the axis (`.axis()`), so whoever lines the
// result up by `"axis"` puts `n` on the line. `stretch`: the column / row
// stretches `n` across (a "fill" arrow) instead.
fn scheme_dress(who, n, labels, stretch) {
    let lab = #{};
    for k in ["above", "below", "left", "right"] {
        let v = opt(labels, k, ());
        let al = if k == "left" { "end" } else if k == "right" { "start" } else { "center" };
        lab[k] = if type_of(v) == "()" { () } else { text_or_node(who, v, scheme_label_size(), al) };
    }
    // Captions above / below wider than the node overhang it (one line,
    // centred on it) instead of widening the column ("auto": into free
    // room only).
    let over = opt(labels, "overhang", false);
    for k in ["above", "below"] {
        if type_of(lab[k]) != "()" { lab[k] = overhang_as(lab[k], over); }
    }
    let g = scheme_label_gap();
    let cross = if stretch { "stretch" } else { "axis" };
    let ab = type_of(lab.above) != "()" || type_of(lab.below) != "()";
    let lr = type_of(lab.left) != "()" || type_of(lab.right) != "()";
    if ab || lr { n = n.axis(); }
    if ab {
        let parts = [];
        if type_of(lab.above) != "()" { parts.push(lab.above); }
        parts.push(n);
        if type_of(lab.below) != "()" { parts.push(lab.below); }
        // Centred on the node's box, not on an axis inside it (a step
        // around a row holding steps): the node still gives the axis.
        n = col(parts).spacing(g).cross(if stretch { "stretch" } else { "center" });
    }
    if lr {
        let parts = [];
        if type_of(lab.left) != "()" { parts.push(lab.left); }
        parts.push(n);
        if type_of(lab.right) != "()" { parts.push(lab.right); }
        n = row(parts).spacing(g).cross(cross);
    }
    n
}

// The node of arrow descriptor `a` in a scheme (`vertical`: pointing down),
// `len` cm long or "fill" (the scheme's default when `a` has no length),
// with its labels.
fn scheme_arrow(a, vertical, len) {
    let o = a.opts;
    let w = opt(o, "width", scheme_arrow_width()).to_float();
    let c = opt(o, "color", fg());
    if type_of(a.length) != "()" { len = a.length; }
    let fill = type_of(len) == "string";
    let hs = opt(o, "head_size", w * 5.0).to_float();
    let hw = if o.contains("head_width") { len_or_multiple("arrow_step", o.head_width, w) } else { hs * 0.8 };
    let node = ();
    if type_of(a.eq) == "map" {
        let eo = a.eq;
        if !eo.contains("width") { eo.width = w; }
        for k in ["head_size", "head_width"] {
            if o.contains(k) && !eo.contains(k) { eo[k] = o[k]; }
        }
        let ew = eo.width.to_float();
        let g = opt(eo, "gap", ew * 4.0).to_float();
        let ehs = opt(eo, "head_size", ew * 5.0).to_float();
        let ehw = if eo.contains("head_width") { len_or_multiple("arrow_step", eo.head_width, ew) } else { ehs * 0.8 };
        let t = g + ehw + ew;
        // `equilibrium` strokes in `fg()`: the colour goes in through it.
        style_push(#{ fg: c });
        node = if fill { scheme_equilibrium_fill(eo, t, g, vertical) }
               else if vertical { equilibrium(t / 2.0, 0, t / 2.0, len, eo).width(t).height(len) }
               else { equilibrium(0, t / 2.0, len, t / 2.0, eo).width(len).height(t) };
        style_pop();
    } else {
        let t = if hw > w { hw } else { w };
        node = if fill {
            if vertical { arrow(0.5, 0, 0.5, 1).viewbox(0, 0, 1, 1).width(t) }
            else { arrow(0, 0.5, 1, 0.5).viewbox(0, 0, 1, 1).height(t) }
        } else if vertical {
            arrow(t / 2.0, 0, t / 2.0, len).width(t).height(len)
        } else {
            arrow(0, t / 2.0, len, t / 2.0).width(len).height(t)
        };
        node = node.stroke_width(w).head(a.head);
        if o.contains("head_size") { node = node.head_size(hs); }
        if o.contains("head_width") { node = node.head_width(o.head_width); }
        if c != "" { node = node.stroke(c); }
    }
    if fill {
        // At least `scheme_fill_min_length()` long: a spacer under the
        // arrow, which fills the stack along the flow.
        let m = scheme_fill_min_length();
        node = stack([if vertical { rect().height(m) } else { rect().width(m) }, node]);
    }
    let labels = if vertical {
        #{ right: opt(o, "right", opt(o, "above", ())),
           left: opt(o, "left", opt(o, "below", ())) }
    } else {
        #{ above: opt(o, "above", ()), below: opt(o, "below", ()) }
    };
    let n = scheme_dress("arrow_step", node, labels, fill);
    if fill { n.flex(1) } else { n }
}

// The ⇌ pair of a "fill" arrow, `t` cm thick, its shafts `g` apart: two
// arrows in a viewbox that is a fraction along the flow and cm across, so
// they stretch with the arrow (equilibrium options gap, width, head,
// head_size, head_width; a hooked barb needs a fixed length).
fn scheme_equilibrium_fill(eo, t, g, vertical) {
    check_opts("equilibrium", eo, ["gap", "width", "head", "head_size", "head_width", "barb"]);
    if opt(eo, "barb", "straight") != "straight" {
        throw "arrow_step(): a hooked equilibrium needs a fixed length (not \"fill\")";
    }
    let head = opt(eo, "head", "barb");
    if !["none", "triangle", "barb"].contains(head) {
        throw `equilibrium(): unknown head "${head}" (none, triangle, barb)`;
    }
    let a = t / 2.0 - g / 2.0;
    let b = t / 2.0 + g / 2.0;
    // [x1, y1, x2, y2]: the first towards the end, the second back (barbs
    // outside, as `equilibrium`).
    let shafts = if vertical { [[b, 0, b, 1], [a, 1, a, 0]] } else { [[0, a, 1, a], [1, b, 0, b]] };
    let c = fg();
    let parts = [];
    for s in shafts {
        let p = arrow(s[0], s[1], s[2], s[3]).stroke_width(eo.width);
        p = if vertical { p.viewbox(0, 0, t, 1) } else { p.viewbox(0, 0, 1, t) };
        p = if head == "none" { p.head("none") }
            else { p.head_style(if head == "barb" { "barb_left" } else { "triangle" }) };
        if eo.contains("head_size") { p = p.head_size(eo.head_size); }
        if eo.contains("head_width") { p = p.head_width(eo.head_width); }
        if c != "" { p = p.stroke(c); }
        parts.push(p);
    }
    let s = stack(parts);
    if vertical { s.width(t) } else { s.height(t) }
}
