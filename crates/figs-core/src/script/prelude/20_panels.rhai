// ---------------------------------------------------------------------------
// Panels: labels, captions, pins and overlays
// ---------------------------------------------------------------------------
// Panel label template used by `panels(items)`; see `panel_label`.
fn label_style() { style_get("label_style", "(a)") }

// Distance from the panel corner to the label (cm). Scales with the text so
// the label never sits flush against the edge: 0.16 cm at the default 8 pt.
fn label_inset() { style_get("label_inset", base_size() * 0.02) }

// Panel corner the label sits in: "top-left" (default), "top-right",
// "bottom-left" or "bottom-right".
fn label_corner() { style_get("label_corner", "top-left") }

// Fill colour of a box behind the label (e.g. "#ffffff"); "" (default) draws
// no box. A box replaces the halo.
fn label_background() { style_get("label_background", "") }

// Padding between the label text and its background box (cm).
fn label_pad() { style_get("label_pad", base_size() * 0.01) }

// Halo (outline) colour around the label, so the default black "(a)" stays
// legible on dark images: invisible on white/light panels, a crisp rim on dark
// ones. "" draws a bare label. See `halo`.
fn label_halo() { style_get("label_halo", "#ffffff") }

// Halo width (cm): how far the rim reaches past the glyph edges — 0.035 cm
// (~1 pt) at the default 10 pt label.
fn label_halo_width() { style_get("label_halo_width", label_size() * 0.0035) }

// Space between a panel and its caption in captioned panels (cm). Tighter than
// `gap()` so a caption reads as belonging to the panel above it.
fn caption_gap() { style_get("caption_gap", gap() * 0.5) }

// Captions (`captioned`, `panels(.., captions)`, `step`) wider than their
// item overhang it into the gaps — centred, in one line — instead of
// widening the item or wrapping to its width (the core `.overhang()`).
// "auto": they overhang only into free room — the gaps beside the item in
// a row, up to whatever the neighbours draw at the caption's height (an
// arrow's label, a taller structure; half a gap when two captions reach
// into it), within the row — and wrap to that otherwise
// (`.overhang("auto")`), so it is safe as a global setting.
fn caption_overhang() { style_get("caption_overhang", false) }

// `labeled`: where panel labels go — "inside" (in `label_corner()`), a
// corner, "outside-left" or "outside-top" — and the gap of an outside label.
fn label_position() { style_get("label_position", "inside") }
fn label_outside_gap() { style_get("label_outside_gap", caption_gap()) }

// Distance from the panel edge to an `overlay` position (cm): the label inset.
fn overlay_inset() { style_get("overlay_inset", label_inset()) }

// Between several nodes at one `overlay` position (cm): 0.08 cm at 8 pt.
fn overlay_gap() { style_get("overlay_gap", base_size() * 0.01) }
// Plain text (strings, markup, spans) that `overlay` lays over a panel: its
// size (pt), colour ("" = `fg()`), and a halo colour ("" = none) reaching
// `overlay_text_halo_width()` cm past the glyphs — labels over photos need
// no per-call styling: `style(#{ overlay_text_color: "#ffffff",
// overlay_text_halo: "#000000" })`.
fn overlay_text_size() { style_get("overlay_text_size", base_size()) }
fn overlay_text_color() { style_get("overlay_text_color", fg()) }
fn overlay_text_halo() { style_get("overlay_text_halo", "") }
fn overlay_text_halo_width() { style_get("overlay_text_halo_width", overlay_text_size() * 0.0025) }

// ---------------------------------------------------------------------------
// Labeled panels (overlay via the core `stack` / `.at`)
// ---------------------------------------------------------------------------

// The `i`-th (0-based) label for a template: the *last* `a`, `A` or `1` in
// `style` is the counter, so "(a)" -> "(b)", "A" -> "B", "a." -> "b.",
// "Fig. 1" -> "Fig. 2". Letters continue past z as "aa", "ab", ….
fn panel_label(style, i) {
    let pos = -1;
    let kind = "";
    for k in 0..style.len() {
        let c = style[k];
        if c == 'a' || c == 'A' || c == '1' { pos = k; kind = c; }
    }
    if pos < 0 {
        throw `panel_label(): style "${style}" has no counter (a, A or 1)`;
    }
    let token = if kind == '1' {
        `${i + 1}`
    } else {
        let letters = "abcdefghijklmnopqrstuvwxyz";
        let t = "";
        let n = i + 1;
        while n > 0 {
            n -= 1;
            t = `${letters[n % 26]}${t}`;
            n /= 26;
        }
        if kind == 'A' { t.to_upper() } else { t }
    };
    style.sub_string(0, pos) + token + style.sub_string(pos + 1)
}

// The label node: `label(s)` over a `label_halo()` halo — or, when
// `label_background()` is set, boxed in it instead (a stack of a filling rect
// under the text, padded via the text's margin).
fn label_tag(s) {
    let t = label(s);
    let bg = label_background();
    if bg != "" {
        return stack([rect().fill(bg), t.at(0.0, 0.0).margin(label_pad())]);
    }
    let h = label_halo();
    if h == "" { t } else { halo(t, h, label_halo_width()) }
}

// Overlay label `s` on `item`, `label_inset()` in from its `label_corner()`:
// `pin(label_tag(s), corner, inset)` over the item, in a core `stack`.
// An item that asks for label room (a plot notes `plot_label_room()` as
// `.meta("label_room")`) first gets that empty band prepended inside it, so
// the label sits clear of its content.
// `labeled(item, s, opts)`: `position` — "inside" (default
// `label_position()`: in `label_corner()`), a corner ("top-right", …),
// "outside-left" (the label left of the item, tops aligned) or
// "outside-top" (above it, left-aligned); `gap` (cm) between an outside
// label and the item (default `label_outside_gap()`).
fn labeled(item, s) { labeled(item, s, #{}) }
fn labeled(item, s, opts) {
    shape_opts("labeled", opts, ["position", "gap"]);
    let pos = plot_opt(opts, "position", label_position());
    if pos == "inside" { return labeled_in(item, s, label_corner(), "label_corner"); }
    if pos == "outside-left" || pos == "outside-top" {
        let g = plot_opt(opts, "gap", label_outside_gap());
        // Beside: the item takes the rest of the width; above: the item
        // keeps its own height (an image stays unletterboxed).
        let n = as_node(item);
        return fill_like(if pos == "outside-left" {
            row([label_tag(s), n.flex(1)]).spacing(g).cross("start")
        } else {
            col([label_tag(s), if n.meta("fill") == true { n.flex(1) } else { n }]).spacing(g).stretch()
        }, n);
    }
    if !["top-left", "top-right", "bottom-left", "bottom-right"].contains(pos) {
        throw `labeled(): unknown position "${pos}" (inside, top-left, top-right, bottom-left, bottom-right, outside-left, outside-top)`;
    }
    labeled_in(item, s, pos, "labeled")
}

// The label pinned in `corner` over the item (`who` names what set it).
fn labeled_in(item, s, corner, who) {
    let tag = pin_in(who, label_tag(s), corner, label_inset());
    let item = as_node(item);
    let room = item.meta("label_room");
    if type_of(room) != "()" && room > 0.0 {
        item = item.prepend(rect().height(room)).meta("label_room", ());
    }
    fill_like(stack([item, tag]), item)
}

// `item` with caption `c` (a string or spans become `caption(c)`) below it,
// `caption_gap()` apart. The item takes the cell's spare height; the
// caption spans its width — or, with `caption_overhang()` (or
// `captioned(item, c, #{ overhang: true })`), keeps its one-line width,
// centred, overhanging into the gaps. The result remembers its parts, so
// `fill_row(.., #{ captions: "overhang" })` can rebuild it.
fn captioned(item, c) { captioned(item, c, #{}) }
fn captioned(item, c, opts) {
    shape_opts("captioned", opts, ["overhang"]);
    let over = overhang_mode("captioned", plot_opt(opts, "overhang", caption_overhang()));
    let cap = if type_of(c) == "string" || poster_is_rich(c) { caption(c) } else { c };
    cap = overhang_as(cap, over);
    fill_like(col([as_node(item).flex(1), cap]).spacing(caption_gap()).stretch()
        .meta("captioned", [item, c]), item)
}

// `wrapper` taking the spare height like `item` when `item` fills (a plot
// with `plot_aspect()` "fill"): `.flex(1)` and the mark passed on.
fn fill_like(wrapper, item) {
    if type_of(item) == "Node" && item.meta("fill") == true { wrapper.flex(1).meta("fill", true) }
    else { wrapper }
}

// An overhang setting checked: true, false or "auto" (`who` for errors).
fn overhang_mode(who, v) {
    if type_of(v) == "bool" || (type_of(v) == "string" && v == "auto") { return v; }
    throw `${who}(): overhang is true, false or "auto" (got ${v})`;
}

// `n` overhanging as `mode` says (true, "auto", or false: as is).
fn overhang_as(n, mode) {
    if type_of(mode) == "string" { n.overhang("auto") } else if mode { n.overhang() } else { n }
}

// Label each item in order with `label_style()` ("(a)", "(b)", …).
fn panels(items) { panels(label_style(), items) }

// `panels(style, items)`, or `panels(items, captions)`: labeled + captioned.
fn panels(style, items) {
    if type_of(style) == "array" { return panels(label_style(), style, items); }
    let sl = split_links(items);
    let out = [];
    let i = 0;
    for x in sl.items {
        out.push(labeled(x, panel_label(style, i)));
        i += 1;
    }
    out + sl.links
}

// Labeled panels, each with its caption below: `captioned(labeled(..), c)`.
fn panels(style, items, captions) {
    let n = split_links(items).items.len();
    if n != captions.len() {
        throw `panels(): ${n} items but ${captions.len()} captions`;
    }
    let out = [];
    let i = 0;
    for x in panels(style, items) {
        if is_link(x) { out.push(x); continue; }
        out.push(captioned(x, captions[i]));
        i += 1;
    }
    out
}

// `node` pinned `inset` (default `label_inset()`) in from `corner` of the
// enclosing stack — the same mechanics as `labeled`: "top-left" is a plain
// `.at(inset, inset)`; other corners use a filling column padded by the inset
// and aligned to the corner. Usable for any overlay (an inset, a "50 °C" tag).
fn pin(node, corner) { pin(node, corner, label_inset()) }

fn pin(node, corner, inset) { pin_in("pin", node, corner, inset) }

// `overlay(item, map)` / `overlay(item, map, opts)`: nodes at their natural
// size over `item` (any panel) by position name — `#{ "top-right": tag,
// bottom: [a, b] }`. Keys: top-left, top, top-right, left, center, right,
// bottom-left, bottom, bottom-right; a value is a node, a string (inline
// markup: "50 *μ*m", "H_2O"), spans, or an array of those, stacked
// `overlay_gap()` apart and aligned to the position's side. Text is set in
// the overlay text style (`overlay_text_size()`, `overlay_text_color()`,
// `overlay_text_halo()`). Each position is a filling column padded
// `overlay_inset()` (the label inset) and aligned like `pin`, so nothing
// takes coordinates and it follows the panel's size. Options:
//   inset  cm for every position, or a map position -> inset (the others
//          keep `overlay_inset()`); an inset is a number, [v, h] or
//          #{ top, right, bottom, left } (unset sides 0):
//          `#{ inset: #{ "top-left": 0, bottom: #{ bottom: 0.1 } } }`
//   gap    cm between the nodes stacked at one position
//   size, color, halo  the overlay text style of this call (pt, colours;
//          default `overlay_text_size()` / `_color()` / `_halo()`)
// The result is a core `stack`: `labeled(overlay(img, ..), "(a)")`.
fn overlay(item, map) { overlay(item, map, #{}) }
fn overlay(item, map, opts) {
    if type_of(map) != "map" {
        throw `overlay(): expected a map of position -> node(s) (got ${type_of(map)})`;
    }
    shape_opts("overlay", opts, ["inset", "gap", "size", "color", "halo"]);
    let look = #{};
    for k in ["size", "color", "halo"] {
        if opts.contains(k) { look[`overlay_text_${k}`] = opts[k]; }
    }
    if look.len() > 0 {
        // This call's text style, in force while its text is built.
        for k in ["size", "color", "halo"] { opts.remove(k); }
        style_push(look);
        let s = overlay(item, map, opts);
        style_pop();
        return s;
    }
    let names = [];
    for p in overlay_positions() { names.push(p[0]); }
    let inset = plot_opt(opts, "inset", overlay_inset());
    let per = #{};
    if type_of(inset) == "map" {
        for k in inset.keys() {
            if !names.contains(k) { throw `overlay(): inset: unknown position "${k}" (${list_names(names)})`; }
        }
        per = inset;
        inset = overlay_inset();
    }
    let g = plot_opt(opts, "gap", overlay_gap());
    for k in map.keys() {
        if !names.contains(k) { throw `overlay(): unknown position "${k}" (${list_names(names)})`; }
    }
    let layers = [as_node(item)];
    for p in overlay_positions() {
        if !map.contains(p[0]) { continue; }
        let v = map[p[0]];
        let sl = split_links(v);
        let ns = [];
        for x in sl.items { ns.push(overlay_text("overlay", x, p[2])); }
        ns += sl.links;
        let pi = if per.contains(p[0]) { per[p[0]] } else { inset };
        layers.push(col(ns).spacing(g).cross(p[2]).pad(pi).main(p[1]));
    }
    fill_like(stack(layers), item)
}
