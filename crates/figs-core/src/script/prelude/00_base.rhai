// figs framework prelude — opinionated components on top of the core builders.
//
// The prelude is split into the files of this directory, concatenated in
// file-name order (see `prelude.rs`) and merged into every script before it
// runs, so all functions are callable without an `import`. Everything here is
// plain Rhai built from the core builders (`text`/`image`/`rect`/`col`/`row`/
// `stack`/`page`), and every component returns an ordinary core node or doc,
// so core setters chain on the result as usual (`grid(2, xs).pad(0.5)`).
//
// Defaults are *hook functions*: a script that defines a function with the
// same name and arity replaces the prelude's (e.g. `fn base_size() { 10 }`
// rescales all semantic text), and the components pick the override up.
// Every hook reads its own key from the per-script style registry first
// (`style_get(key, default)`), so `style("nature")` or `style(#{ base_size: 9
// })` switches them all at once; the second argument is the built-in default.
// A user `fn` with the same name replaces the whole hook, so it always wins
// over any preset. Each file starts with the hooks of its components.
//
// Lengths are in cm (the unit `page(preset, root)` sets); font sizes in pt.
//
// This file: the base hooks and theme colours, the style registry, and the
// shared helpers every component uses (options, values, lists, positions).

// ---------------------------------------------------------------------------
// Base hooks and theme colours
// ---------------------------------------------------------------------------
// Base font size (pt) every semantic text style is derived from.
fn base_size() { style_get("base_size", 8) }

// Font family of the page (`page(preset, root)` applies it with the core
// `.font`); "" (default) keeps the renderer's default sans-serif. A family
// that isn't installed falls back to the system's default font, so a preset
// naming "Arial" still renders (in another face) where Arial is missing.
fn font_family() { style_get("font_family", "") }

// Title size (pt): 1.5x base.
fn title_size() { style_get("title_size", base_size() * 1.5) }

// Panel label size (pt): 1.25x base.
fn label_size() { style_get("label_size", base_size() * 1.25) }

// Default gap between siblings in framework containers (page units, cm).
fn gap() { style_get("gap", 0.2) }

// --- Colours (theme) ---

// Foreground colour of framework text — semantic text (`body`, `title`, …)
// and, through `plot_fg()`, everything plots draw that isn't data. "" (default)
// keeps the renderer's default black and sets no colour at all.
fn fg() { style_get("fg", "") }

// Plot foreground: axes, frame, ticks, tick and axis labels, titles, legend
// text of the swatch legend, error bars without a colour of their own,
// `plot_text` / `plot_arrow` annotations. Follows `fg()` (black by default).
fn plot_fg() {
    let f = fg();
    style_get("plot_fg", if f == "" { "#000000" } else { f })
}

// Fill behind a whole plot panel (axes, labels and all); "" (default) leaves
// it transparent over whatever is behind (the page background).
fn plot_bg() { style_get("plot_bg", "") }

// Page background of `page(preset, root)`; "" (default) sets none (white).
fn page_bg() { style_get("page_bg", "") }

// ---------------------------------------------------------------------------
// Style presets
// ---------------------------------------------------------------------------
//
// `style("nature");` at the top of a script switches a coherent bundle of the
// hooks above (font, sizes, label format, column widths) in one line;
// `style(#{ base_size: 9, label_style: "A" })` sets individual keys (merged
// over the current style), and `style("nature", #{ .. })` does both. Keys are
// hook names. The registry lives on the Rust side (`style_set` / `style_get` /
// `style_reset`, fresh for every evaluation) because hooks are functions, and
// Rhai functions can't see script variables.
//
// The journal presets are *approximations* of each publisher's author
// guidelines (which change, and differ between titles), not certified styles.
// All use Arial: the Helvetica-class sans the guidelines ask for that is most
// widely installed (a missing font falls back to the system default).
//   nature   7 pt text (guide: 5-7 pt), 8 pt bold lowercase "a" labels,
//            89 / 183 mm columns.
//   science  7 pt text, 8 pt bold uppercase "A" labels, 5.7 / 18.3 cm columns.
//   acs      7 pt text (guide: >= 4.5 pt), 8 pt "(a)" labels, 3.25 / 7 in
//            columns.
//   rsc      7 pt text, 8 pt "(a)" labels, 8.3 / 17.1 cm columns.
//   poster   24 pt body (readable from ~2 m), 60 pt titles, 1 cm gaps; strokes,
//            insets and halos scale up with the text.
//   dark     the default sizes on a dark background: white text and plot
//            lines (`fg`), `dark_palette()` series colours, a dark navy
//            (#0f1a2e) page for `page(preset, root)`, black label halos and
//            marker rings.
//   poster-dark  poster sizes + the dark theme (slides, dark posters).
// Line widths need no key of their own: annotation strokes follow the base
// size (7 pt -> 0.035 cm ~ 1 pt, inside the usual 0.25-1.5 pt range).

// Preset name -> style map ("default" is the built-in hook defaults).
fn style_presets() {
    let journal = #{ font_family: "Arial", base_size: 7, label_size: 8 };
    // Marks (■ ● ○ • – —) in a deliberate face: a Japanese gothic, as
    // PowerPoint sets marks typed in Japanese (see `poster_symbol_font()`).
    let poster = #{
        base_size: 24, title_size: 60, gap: 1.0,
        poster_symbol_font: ["Yu Gothic", "Hiragino Sans", "Noto Sans CJK JP"],
    };
    let dark = #{
        fg: "#ffffff", palette: dark_palette(), page_bg: "#0f1a2e",
        label_halo: "#000000", marker_ring: "#0f1a2e",
    };
    // The dark conference-poster look (see the "Poster" section): a navy
    // page gradient lit from the top, top-lit section bars with a hairline
    // pale-cyan border, amber notes, table cells a shade off the page; the
    // titles in a subtle dark glow and soft shadow, the bars' soft shadow.
    let poster_dark = #{
        poster_bg: linear(90, [[0, "#28314c"], [0.12, "#181e2e"], [0.38, "#10141f"], [1, "#080a10"]]),
        section_bar_fill: linear(90, [[0, "#3d4a73"], [0.56, "#181d2d"], [1, "#080a10"]]),
        section_bar_border: "#caeefb", note_color: "#ffc000", table_fill: "#121622",
        section_title_glow: true, section_title_shadow: true, section_bar_shadow: true,
    };
    #{
        "default": #{},   // quoted: `default` is reserved in Rhai
        nature: journal + #{ label_style: "a", single_width: 8.9, double_width: 18.3 },
        science: journal + #{ label_style: "A", single_width: 5.7, double_width: 18.3 },
        acs: journal + #{ label_style: "(a)", single_width: 8.25, double_width: 17.78 },
        rsc: journal + #{ label_style: "(a)", single_width: 8.3, double_width: 17.1 },
        poster: poster,
        dark: dark,
        "poster-dark": poster + dark + poster_dark,
    }
}

// The keys `style(map)` accepts: the hooks above that read the registry.
fn style_keys() {
    [
        "base_size", "font_family", "title_size", "label_size", "gap",
        "label_style", "label_inset", "label_corner", "label_background",
        "label_pad", "label_halo", "label_halo_width", "caption_gap",
        "single_width", "double_width", "accent", "annotation_width",
        "annotation_dash", "scalebar_color", "scalebar_thickness",
        "scalebar_corner", "scalebar_inset", "scalebar_halo", "scalebar_frac_inset",
        "overlay_inset", "overlay_gap", "scheme_gap", "scheme_arrow_length",
        "scheme_arrow_width", "scheme_fill_min_length", "scheme_label_size", "scheme_label_gap", "side_width", "side_gap",
        "labware_size", "labware_stroke", "labware_stroke_width", "labware_liquid", "labware_level",
        "vector_note_color", "vector_note_width", "vector_note_size", "vector_note_gap",
        "colorbar_length", "colorbar_thickness", "colorbar_label_size", "colorbar_gap",
        "colorbar_border", "colorbar_border_width", "cycle_arrow_color", "cycle_arrow_width",
        "cycle_arrow_depth", "cycle_arrow_span", "cycle_arrow_gap",
        "scalebar_box", "scalebar_box_pad", "scalebar_box_radius", "overlay_text_size",
        "overlay_text_color", "overlay_text_halo", "overlay_text_halo_width", "inset_avoid_band",
        "fill_row_gap", "caption_overhang", "image_dpi", "zoom_attach", "plot_min_height",
        // links, piles, backdrops
        "zoom_color", "zoom_width", "zoom_dash", "zoom_region_dash", "zoom_copy_frame",
        "zoom_outline", "zoom_style", "zoom_cone_opacity",
        "shift_arrow_min_length", "shift_arrow_anchor",
        "callout_position", "callout_color", "callout_width", "callout_gap", "callout_size",
        "callout_side_gap", "flow_link_color", "flow_link_width", "flow_link_radius",
        "flow_link_gap", "block_link_width", "block_link_color", "block_link_opacity",
        "shift_arrow_trim", "pile_offset", "pile_opacity", "backdrop_color", "backdrop_pad",
        "backdrop_radius", "label_position", "label_outside_gap",
        // plots
        "plot_line_width", "series_width", "palette", "tick_length", "tick_direction",
        "minor_ticks", "max_ticks", "marker_size", "marker_open", "marker_stroke_width",
        "marker_ring", "plot_frame", "legend_corner", "legend_style", "bar_width",
        "plot_aspect", "plot_label_room", "plot_pad", "ylabel_rotated", "plot_title_size",
        "plot_frame_box", "plot_font", "tick_label_size", "tick_label_font", "tick_label_pad",
        "axis_label_size", "axis_label_font", "axis_label_pad", "legend_size", "legend_font",
        "plot_title_font", "xtick_label_pad", "ytick_label_pad", "tick_label_pad_mode",
        "xlabel_pad", "ylabel_pad", "xlabel_offset", "ylabel_offset", "ytick_label_align",
        "inset_size", "inset_corner", "inset_pad", "inset_text_size", "inset_bg",
        "inset_max_ticks",
        // colours (theme)
        "fg", "plot_fg", "plot_bg", "page_bg",
        // posters
        "poster_size", "poster_bg", "poster_margin", "poster_gutter", "poster_row_gap",
        "poster_title_size", "poster_title_font", "author_size",
        "affiliation_position", "author_sep", "presenter_mark", "poster_number_size",
        "poster_logo_height", "poster_header_rule", "poster_header_rule_width",
        "section_label_style", "section_title_size", "section_title_color",
        "section_title_font", "section_bar_height", "section_bar_fill",
        "section_bar_border", "section_bar_border_width", "section_bar_radius",
        "section_pad", "section_gap", "separators", "row_separators", "separator_color",
        "separator_width", "separator_dash", "separator_inset", "frame_color",
        "frame_width", "frame_radius", "frame_pad", "frame_dash", "note_color",
        "note_size", "note_font", "ref_size", "ref_color", "bullet_mark", "bullet_size",
        "bullet_indent", "bullet_gap", "subheading_mark", "subheading_size", "table_size",
        "table_rule", "table_rule_width", "table_header_rule_width", "table_rules",
        "table_pad", "table_fill", "table_header_fill",
        "section_title_glow", "section_title_shadow", "section_bar_shadow", "table_pad_x",
        "table_pad_y", "subheading_mark_bold", "subheading_mark_size", "bullet_mark_size",
        "table_font", "table_header_font", "separator_offset", "section_title_baseline",
        "poster_symbol_font", "block_gap", "block_title_gap", "bullet_pitch", "table_rule_mode",
        "section_overflow", "poster_number_position", "author_font", "ref_font",
        "poster_logo_box", "poster_number_inset", "section_valign", "poster_title_inset",
        "section_body_inset", "bullet_inset",
        // component variants: name -> map of style keys (see `variant_push`)
        "frame_variants", "note_variants", "ref_variants", "table_variants",
        "subheading_variants", "section_variants",
    ]
}

// `style(name)` switches to a preset (dropping earlier `style` calls);
// `style(map)` merges individual keys over the current style.
fn style(x) {
    if type_of(x) == "map" {
        let keys = style_keys();
        for k in x.keys() {
            if !keys.contains(k) {
                throw `style(): unknown key "${k}" (keys are hook names: ${list_names(keys)})`;
            }
        }
        style_set(x);
        return;
    }
    if type_of(x) != "string" {
        throw `style(): expected a preset name or a map of hook values (got ${type_of(x)})`;
    }
    let presets = style_presets();
    if !presets.contains(x) {
        throw `style(): unknown preset "${x}" (${list_names(presets.keys())})`;
    }
    style_reset();
    style_set(presets[x]);
}

// A preset plus overrides: `style("nature", #{ base_size: 6 })`.
fn style(name, overrides) {
    style(name);
    style(overrides);
}

// `styled(map, || nodes)` (or `with_style`): build `nodes` — the function
// is called once — with the style keys of `map` in force (merged over the
// current style, like `style(map)`), then restore the style exactly, even
// when building fails. Nest freely; the result is whatever the function
// returns: `styled(#{ scheme_label_size: 18 }, || scheme([..]))`.
fn styled(m, f) {
    if type_of(m) != "map" { throw `styled(): expected a map of style keys first (got ${type_of(m)})`; }
    if type_of(f) != "Fn" {
        throw `styled(): expected a function building the nodes second, e.g. || scheme([..]) (got ${type_of(f)})`;
    }
    let keys = style_keys();
    for k in m.keys() {
        if !keys.contains(k) {
            throw `styled(): unknown key "${k}" (keys are hook names: ${list_names(keys)})`;
        }
    }
    style_push(m);
    let out = ();
    try {
        out = f.call();
    } catch (e) {
        style_pop();
        throw e;
    }
    style_pop();
    out
}
fn with_style(m, f) { styled(m, f) }

// "a, b, c" from an array of strings (for error messages).
fn list_names(xs) {
    let out = "";
    for x in xs {
        if out != "" { out += ", "; }
        out += x;
    }
    out
}

// Component variants: `style(#{ frame_variants: #{ box: #{ frame_color:
// "#7f7f7f", frame_radius: 0 } } })` names a bundle of style keys, and
// `frame(x, "box")` builds the frame with them in force (likewise
// `note(s, v)`, `ref(s, v)`, `subheading(s, v)`, `table(rows, #{ variant })`,
// `section(.., #{ variant })`). `v` is a name from `<who>_variants` or a map
// of style keys used directly. Pushes the style (`style_pop()` after).
fn variant_push(who, v) {
    let m = v;
    if type_of(v) == "string" {
        let all = Fn(`${who}_variants`).call();
        if type_of(all) != "map" { throw `${who}_variants(): must be a map of name -> style map`; }
        if !all.contains(v) {
            let known = if all.len() == 0 { "none defined" } else { list_names(all.keys()) };
            throw `${who}(): unknown variant "${v}" (${known}; define it in style(#{ ${who}_variants: #{ .. } }))`;
        }
        m = all[v];
    }
    if type_of(m) != "map" {
        throw `${who}(): a variant is a name from ${who}_variants or a map of style keys (got ${type_of(m)})`;
    }
    let keys = style_keys();
    for k in m.keys() {
        if !keys.contains(k) { throw `${who}(): variant key "${k}" is not a style key (see style_keys())`; }
    }
    style_push(m);
}

// ---------------------------------------------------------------------------
// Shared helpers: options, values, fonts, lists, positions
// ---------------------------------------------------------------------------
// `opts[k]`, or `d` when absent.
fn plot_opt(opts, k, d) { if opts.contains(k) { opts[k] } else { d } }

// `opts` checked against the option names `keys` of shape `who`.
fn shape_opts(who, opts, keys) {
    if type_of(opts) != "map" { throw `${who}(): options must be a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        if !keys.contains(k) { throw `${who}(): unknown option "${k}" (${list_names(keys)})`; }
    }
}

// A length option: a number, or a multiple of `w` as a string ("3x").
fn shape_len(who, v, w) {
    let t = type_of(v);
    if t == "i64" || t == "f64" { return v.to_float(); }
    if t == "string" && v.ends_with("x") {
        let s = v.sub_string(0, v.len() - 1);
        try { return parse_float(s) * w; } catch { }
    }
    throw `${who}(): expected a length or a multiple like "3x" (got ${v})`;
}

// `opts` checked against the option names `keys` of `who`.
fn poster_check_opts(who, opts, keys) {
    if type_of(opts) != "map" { throw `${who}(): options must be a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        if !keys.contains(k) { throw `${who}(): unknown option "${k}" (${list_names(keys)})`; }
    }
}

// A finite float (NaN, infinities and non-numbers are not).
fn plot_finite(v) {
    let t = type_of(v);
    if t == "i64" { return true; }
    t == "f64" && v == v && v.abs() < 1e300
}

// `xs` as an array of floats (NaN for gaps); throws for non-arrays/non-numbers.
fn plot_numbers(name, what, xs) {
    if type_of(xs) != "array" {
        throw `${name}(): ${what} must be an array of numbers (got ${type_of(xs)})`;
    }
    let out = [];
    for v in xs {
        let t = type_of(v);
        if t == "f64" { out.push(v); }
        else if t == "i64" { out.push(v.to_float()); }
        else { throw `${name}(): ${what} must be numbers (got ${t} ${v})`; }
    }
    out
}

// Is paint `p` (a colour string or a gradient) set, i.e. not ""?
fn poster_has_paint(p) { type_of(p) != "string" || p != "" }

// Is `x` rich text: an array holding spans (`[span(..), ..]`, `markup(..)`)?
fn poster_is_rich(x) {
    if type_of(x) != "array" { return false; }
    for v in x { if type_of(v) == "Span" { return true; } }
    false
}

// `prefix` (a string) before `s`, a string or spans.
fn poster_prefix(prefix, s) { if type_of(s) == "string" { prefix + s } else { [span(prefix)] + s } }

// The length in cm of a "12cm" / "12 cm" size, or () for anything else.
fn poster_cm(who, w) {
    if type_of(w) != "string" || !w.ends_with("cm") { return (); }
    let s = w.sub_string(0, w.len() - 2);
    s.trim();
    let v = 0.0;
    try { v = parse_float(s); } catch { throw `${who}(): bad length "${w}" (e.g. "12cm")`; }
    if !(v > 0.0) { throw `${who}(): a length must be positive (got "${w}")`; }
    v
}

// Is font spec `f` unset ("" / [] / ())?
fn table_no_font(f) {
    let k = type_of(f);
    k == "()" || (k == "string" && f == "") || (k == "array" && f.len() == 0)
}

// ---------------------------------------------------------------------------
// Bare-string sugar
// ---------------------------------------------------------------------------

// A bare string (or spans) becomes `body(s)`; an array — what helpers such
// as `zoom_link` return, or an item with its links: `[fill_row(..),
// zoom_link(..)]` — becomes one node: its only item (the connectors ride
// along in a column around it, taking no room), or its items in a `vbox`;
// anything else passes through untouched.
fn as_node(x) {
    let t = type_of(x);
    if t == "string" || poster_is_rich(x) { return body(x); }
    if t != "array" { return x; }
    let s = split_links(x);
    let items = [];
    for v in s.items { items.push(as_node(v)); }
    if s.links.len() == 0 && items.len() == 1 { return items[0]; }
    if items.len() == 1 { return col(items + s.links).stretch(); }
    col(items + s.links).spacing(gap()).stretch()
}

// `as_node` over an array — the bridge to core containers:
// `col(nodes(["Results", image("a.png")]))`. Nested arrays are spliced in
// (as core containers do), so a helper's connectors go along.
fn nodes(items) {
    let s = split_links(items);
    let out = [];
    for v in s.items { out.push(as_node(v)); }
    out + s.links
}

// Is `x` a connector (a link drawn after layout, taking no room)?
fn is_link(x) { type_of(x) == "Node" && x.kind() == "connector" }

// `xs` (an array, or one value; nested arrays spliced in, spans kept whole)
// split into `items` — everything that takes room, as given — and `links`
// (connectors, which take none): list helpers size and count the items and
// add the links to the container they build.
fn split_links(xs) {
    let items = [];
    let links = [];
    if type_of(xs) != "array" || poster_is_rich(xs) { xs = [xs]; }
    for x in xs {
        if type_of(x) == "array" && !poster_is_rich(x) {
            let s = split_links(x);
            items.append(s.items);
            links.append(s.links);
        } else if is_link(x) {
            links.push(x);
        } else {
            items.push(x);
        }
    }
    #{ items: items, links: links }
}

// The positions of `overlay`, in reading order, and their [main, cross]
// alignment in a filling column (main is vertical, cross horizontal).
fn overlay_positions() {
    [
        ["top-left", "start", "start"], ["top", "start", "center"], ["top-right", "start", "end"],
        ["left", "center", "start"], ["center", "center", "center"], ["right", "center", "end"],
        ["bottom-left", "end", "start"], ["bottom", "end", "center"], ["bottom-right", "end", "end"],
    ]
}

// `pin` whose corner came from `who` (a function or hook name), so an unknown
// corner is reported against what the user actually set (`label_corner(): …`).
fn pin_in(who, node, corner, inset) {
    if corner == "top-left" { return node.at(inset, inset); }
    let align = #{
        "top-right": ["start", "end"],
        "bottom-left": ["end", "start"],
        "bottom-right": ["end", "end"],
    };
    if !align.contains(corner) {
        throw `${who}(): unknown corner "${corner}" (top-left, top-right, bottom-left, bottom-right)`;
    }
    let a = align[corner];
    col([node]).pad(inset).main(a[0]).cross(a[1])
}

// The corner of the [0, 1]² data area with the fewest `samples` ([u, v],
// v up) in it, preferring top-right, top-left, bottom-right, bottom-left.
fn legend_auto_corner(samples) { plot_auto_corner(samples, []) }

// `legend_auto_corner`, skipping the corners in `taken` (the legend's, an
// earlier inset's); the first free corner if all hold data alike.
fn plot_auto_corner(samples, taken) {
    let corners = ["top-right", "top-left", "bottom-right", "bottom-left"];
    let best = corners[0];
    let best_n = -1;
    for c in corners {
        if taken.contains(c) { continue; }
        let n = 0;
        let right = c.ends_with("right");
        let top = c.starts_with("top");
        for p in samples {
            let inu = if right { p[0] > 0.5 } else { p[0] < 0.5 };
            let inv = if top { p[1] > 0.6 } else { p[1] < 0.4 };
            if inu && inv { n += 1; }
        }
        if best_n < 0 || n < best_n { best = c; best_n = n; }
    }
    best
}
