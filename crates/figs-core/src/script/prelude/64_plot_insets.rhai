// --- insets ---

// `inset(sub)` / `inset(sub, opts)`: `sub` (any node — a zoomed
// `inset_plot`, an image, a schematic) inside the plot it is given to, in
// its series or `marks`. Options:
//   corner  "auto" (the emptiest corner the legend left free) or a corner
//           name, `pad` in from the data frame (default `inset_corner()`)
//   at      [x, y]: instead, place it at that data point (data values; the
//           plot applies log axes) — its top-left there unless `anchor`
//   anchor  [ax, ay] with `at`: which point of the inset sits there,
//           fractions of its box (default [0, 0])
//   size    width as a fraction of the frame / panel (default `inset_size()`)
//   width   cm instead of `size`
//   height  cm (default: from the node's aspect, else its natural height)
//   pad     cm between a corner inset and the frame (default `inset_pad()`)
//   avoid   keep a corner inset off edge bands: an edge ("bottom"), edges
//           (["bottom", "left"]) — each clear by `inset_avoid_band()`, a
//           caption strip — or a map edge -> cm (#{ bottom: 1.2 })
// `inset(main, sub)` / `inset(main, sub, opts)`: the same on any panel that
// is not a plot (no data coordinates; "auto" means "top-right"):
// `stack([main, pin(sub, corner, pad)])`.
fn inset(sub) { inset_spec(sub, #{}) }
fn inset(a, b) {
    if type_of(b) == "map" { return inset_spec(a, b); }
    inset(a, b, #{})
}
fn inset(main, sub, opts) {
    let s = inset_spec(sub, opts);
    if type_of(s.at) != "()" {
        throw "inset(main, sub, opts): `at` is in data coordinates — put inset(sub, #{ at: [x, y] }) in a plot's series or marks";
    }
    let c = if s.corner == "auto" { "top-right" } else { s.corner };
    stack([as_node(main), inset_in_corner(s, c)])
}

// Inset `a` in `corner` of the stack it goes into, `a.pad` in: a filling row
// whose flex spacer leaves the inset `a.size` of the width (or a plain `pin`
// when it has a `width` in cm).
fn inset_in_corner(a, corner) {
    let pad = a.pad;
    if a.avoid.len() > 0 {
        pad = #{};
        for e in ["top", "right", "bottom", "left"] { pad[e] = a.pad + opt(a.avoid, e, 0.0); }
    }
    if type_of(a.width) != "()" {
        if type_of(pad) != "map" { return pin_in("inset", a.node, corner, pad); }
        let v = if corner.starts_with("top") { "start" } else { "end" };
        let h = if corner.ends_with("right") { "end" } else { "start" };
        return col([a.node]).pad(pad).main(v).cross(h);
    }
    let gap = rect().flex(1.0 - a.size);
    let n = a.node.flex(a.size);
    let r = if corner.ends_with("right") { row([gap, n]) } else { row([n, gap]) };
    r.cross(if corner.starts_with("top") { "start" } else { "end" }).pad(pad)
}

// The `avoid` option of `inset`: () / an edge / edges / a map edge -> cm,
// as a map edge -> cm.
fn inset_avoid(v) {
    let edges = ["top", "right", "bottom", "left"];
    let t = type_of(v);
    if t == "()" { return #{}; }
    if t == "string" { v = [v]; t = "array"; }
    let out = #{};
    if t == "array" {
        for e in v {
            if !edges.contains(e) { throw `inset(): avoid: unknown edge "${e}" (${list_names(edges)})`; }
            out[e] = inset_avoid_band();
        }
        return out;
    }
    if t != "map" { throw `inset(): avoid is an edge, an array of edges or a map edge -> cm (got ${t})`; }
    for e in v.keys() {
        if !edges.contains(e) { throw `inset(): avoid: unknown edge "${e}" (${list_names(edges)})`; }
        out[e] = v[e];
    }
    out
}

// The descriptor of `inset(sub, opts)`: an annotation (`mark: "inset"`) the
// plot places once its axes exist, holding the sized node.
fn inset_spec(sub, opts) {
    if type_of(opts) != "map" { throw `inset(): options must be a map (got ${type_of(opts)})`; }
    let known = ["corner", "at", "anchor", "size", "width", "height", "pad", "avoid"];
    for k in opts.keys() {
        if !known.contains(k) { throw `inset(): unknown option "${k}" (${list_names(known)})`; }
    }
    let n = as_node(sub);
    let width = opt(opts, "width", ());
    if type_of(width) != "()" { n = n.width(width); }
    if opts.contains("height") { n = n.height(opts.height); }
    let size = opt(opts, "size", inset_size()).to_float();
    if !(size > 0.0 && size <= 1.0) { throw `inset(): size must be a fraction in (0, 1] (got ${size})`; }
    let a = #{ mark: "inset", node: n, at: (), anchor: [0, 0], size: size, width: width,
               corner: opt(opts, "corner", inset_corner()), pad: opt(opts, "pad", inset_pad()),
               avoid: inset_avoid(opt(opts, "avoid", ())) };
    if opts.contains("at") {
        a.at = numbers("inset", "`at`", opts.at);
        if a.at.len() != 2 { throw "inset(): `at` must be [x, y]"; }
    }
    a.anchor = numbers("inset", "`anchor`", opt(opts, "anchor", a.anchor));
    if a.anchor.len() != 2 { throw "inset(): `anchor` must be [ax, ay]"; }
    let corners = ["auto", "top-right", "top-left", "bottom-right", "bottom-left"];
    if !corners.contains(a.corner) {
        throw `inset(): unknown corner "${a.corner}" (${list_names(corners)})`;
    }
    a
}

// `inset_plot(series)` / `inset_plot(opts, series)`: a `plot` dressed for an
// inset — every label at `inset_text_size()`, at most `inset_max_ticks()`
// ticks per axis, shorter ticks, a tighter pad, and `inset_bg()` under the whole box so the host's data stays out. `opts`
// are plot options; its `style` map overrides these defaults.
fn inset_plot(series) { inset_plot(#{}, series) }
fn inset_plot(opts, series) {
    if type_of(opts) != "map" { throw `inset_plot(): options must be a map (got ${type_of(opts)})`; }
    let ts = inset_text_size();
    let st = #{ tick_label_size: ts, axis_label_size: ts, legend_size: ts, plot_title_size: ts,
                tick_length: tick_length() * 0.75, plot_pad: plot_pad() * 0.5, plot_bg: inset_bg(),
                max_ticks: inset_max_ticks() };
    // An inset is sized by its aspect: never "fill".
    if type_of(plot_aspect()) == "string" { st.plot_aspect = 4.0 / 3.0; }
    let own = opt(opts, "style", #{});
    if type_of(own) != "map" { throw `inset_plot(): style must be a map of style keys (got ${type_of(own)})`; }
    for k in own.keys() { st[k] = own[k]; }
    opts.style = st;
    plot(opts, series)
}
