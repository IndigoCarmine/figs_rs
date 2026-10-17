// Layout sugar: page presets, boxes, grids, `fill_row`, `side`.
// Pixel density of every image without its own `.dpi(n)` (its natural size
// = pixels / dpi inches), set on the page by `page(preset, root)` /
// `poster(..)`; () = none (images fill the room they get). "file": the
// density each file records (PNG pHYs, JPEG JFIF) — images whose file
// records none have no natural size.
fn image_dpi() { style_get("image_dpi", ()) }

// Widths (cm) of the "single" (one journal column) and "double" (full page
// width) page presets.
fn single_width() { style_get("single_width", 8.5) }
fn double_width() { style_get("double_width", 17.5) }

// ---------------------------------------------------------------------------
// Page presets
// ---------------------------------------------------------------------------

// `[width, height]` in cm for a named preset. Journal presets fix only the
// width — their height is "auto" (the page fits the content); paper presets are
// fixed on both sides, and "-landscape" swaps them. A journal preset has no
// landscape form (there is no fixed height to swap).
fn paper_size(name) {
    if type_of(name) != "string" {
        throw "page(): use page(width, height, root) or page(preset, root)";
    }
    let journal = #{
        single: single_width(),   // one journal column (default 8.5 cm)
        double: double_width(),   // full page width, two columns (default 17.5 cm)
    };
    let paper = #{
        a5: [14.8, 21.0],
        a4: [21.0, 29.7],
        a3: [29.7, 42.0],
        a0: [84.1, 118.9],       // poster
        letter: [21.59, 27.94],
        a2: [42.0, 59.4],        // posters
        a1: [59.4, 84.1],
    };
    let landscape = name.ends_with("-landscape");
    let key = if landscape { name.sub_string(0, name.len() - 10) } else { name };
    if journal.contains(key) {
        if landscape {
            throw `page(): "${name}": journal presets have an auto height and no landscape form; use page(w, h, root) for a fixed size`;
        }
        return [journal[key], "auto"];
    }
    if !paper.contains(key) {
        throw `page(): unknown preset "${name}" (single, double, a5, a4, a3, a2, a1, a0, letter; paper sizes take -landscape)`;
    }
    let s = paper[key];
    if landscape { [s[1], s[0]] } else { s }
}

// A page sized by preset name, in cm: `page("double", root)`, in the style's
// `font_family()` when one is set (chain `.font(name)` to override), on the
// style's `page_bg()` background when one is set, its images at the style's
// `image_dpi()` when one is set.
fn page(preset, root) {
    let s = paper_size(preset);
    let d = page(s[0], s[1], root).unit("cm");
    let bg = page_bg();
    if bg != "" { d = d.background(bg); }
    let dpi = image_dpi();
    if type_of(dpi) != "()" { d = d.image_dpi(dpi); }
    let f = font_family();
    if f == "" { d } else { d.font(f) }
}

// Column / row with the default gap, stretched cross axis and bare strings.
fn vbox(items) { col(nodes(items)).spacing(gap()).stretch() }
fn hbox(items) { row(nodes(items)).spacing(gap()).stretch() }

// ---------------------------------------------------------------------------
// Grid
// ---------------------------------------------------------------------------

// `ncols` equal-flex cells per row, rows sharing the height equally. The last
// row is padded with empty cells so every cell keeps the same width.
fn grid(ncols, items) { grid(ncols, gap(), items) }

fn grid(ncols, g, items) {
    if type_of(ncols) != "i64" || ncols < 1 {
        throw `grid(): ncols must be a positive integer (got ${ncols})`;
    }
    let rows = [];
    let cells = [];
    let s = split_links(items);
    for x in s.items {
        cells.push(as_node(x).flex(1));
        if cells.len() == ncols {
            rows.push(row(cells).spacing(g).stretch().flex(1));
            cells = [];
        }
    }
    if cells.len() > 0 {
        while cells.len() < ncols { cells.push(rect().flex(1)); }
        rows.push(row(cells).spacing(g).stretch().flex(1));
    }
    col(rows + s.links).spacing(g).stretch().flex(1)
}

// The gap between the items of `fill_row` (cm).
fn fill_row_gap() { style_get("fill_row_gap", gap()) }

// `fill_row(items)` / `fill_row(items, opts)`: images, plots and panels
// built on them (labeled, overlaid, captioned) side by side at ONE shared
// height that makes the row exactly fill its width — widths in proportion
// to their aspect ratios, worked out at layout time from the real image
// sizes (the core `row(..).equal_height()`). Text items keep their natural
// width. Options:
//   gap       cm between the items (default `fill_row_gap()`)
//   align     "start" (default, tops), "center", "end"
//   captions  "overhang": the captions of `captioned` / `panels(..,
//             captions)` / `step` items keep their one-line width, centred
//             on their item and overhanging into the gaps, so a long
//             caption neither widens its item nor wraps; "wrap": they wrap
//             to the item's width; "auto": overhang into free room, else
//             wrap (default: as each item was built, see
//             `caption_overhang()`)
fn fill_row(items) { fill_row(items, #{}) }
fn fill_row(items, opts) {
    if type_of(items) != "array" { throw `fill_row(): expected an array of items (got ${type_of(items)})`; }
    shape_opts("fill_row", opts, ["gap", "align", "captions"]);
    let align = plot_opt(opts, "align", "start");
    if !["start", "center", "end"].contains(align) {
        throw `fill_row(): unknown align "${align}" (start, center, end)`;
    }
    let ns = nodes(items);
    let caps = plot_opt(opts, "captions", ());
    if type_of(caps) != "()" {
        if caps != "overhang" && caps != "wrap" && caps != "auto" {
            throw `fill_row(): unknown captions "${caps}" (overhang, wrap, auto)`;
        }
        let over = if caps == "auto" { "auto" } else { caps == "overhang" };
        let out = [];
        for n in ns { out.push(recaption(n, over)); }
        ns = out;
    }
    row(ns).equal_height().spacing(plot_opt(opts, "gap", fill_row_gap())).cross(align)
}

// `n` rebuilt with its captions overhanging (`over`) or wrapping, when it
// was built by `captioned` or `step` (which remember their parts); any
// other node as is.
fn recaption(n, over) {
    if type_of(n) != "Node" { return n; }
    let c = n.meta("captioned");
    if type_of(c) == "array" { return captioned(c[0], c[1], #{ overhang: over }); }
    let s = n.meta("step");
    if type_of(s) == "array" {
        let o = s[1];
        if type_of(o) != "map" { o = #{ below: o }; }
        o.overhang = over;
        return step(s[0], o);
    }
    n
}

// `side(main, note)`: the note's share of the width (a fraction; "5cm" or
// "auto" also work) and the gap between the two (cm).
fn side_width() { style_get("side_width", 0.4) }
fn side_gap() { style_get("side_gap", gap()) }

// `side(main, note)` / `side(main, note, opts)`: a panel (a plot, an image)
// with a text block beside it — `note` is a string or spans (`body`), a node,
// or an array of those (a column `caption_gap()` apart). Right of the panel
// and centred on it by default; the note takes `side_width()` of the width
// (the panel the rest), `side_gap()` apart. Options:
//   side   "right" (default) or "left"
//   width  the note's share: a fraction in (0, 1), "5cm", or "auto" (its
//          natural width; the panel takes the rest)
//   gap    cm between panel and note
//   align  "center" (default), "start" (tops), "end", "stretch"
fn side(main, note) { side(main, note, #{}) }
fn side(main, note, opts) {
    shape_opts("side", opts, ["side", "width", "gap", "align"]);
    let where = plot_opt(opts, "side", "right");
    if where != "right" && where != "left" { throw `side(): unknown side "${where}" (right, left)`; }
    let align = plot_opt(opts, "align", "center");
    if !["center", "start", "end", "stretch"].contains(align) {
        throw `side(): unknown align "${align}" (center, start, end, stretch)`;
    }
    let n = if type_of(note) == "array" && !poster_is_rich(note) {
        let xs = [];
        for x in note { xs.push(scheme_text("side", x, base_size(), "start")); }
        col(xs).spacing(caption_gap()).stretch()
    } else { scheme_text("side", note, base_size(), "start") };
    let m = as_node(main);
    let w = plot_opt(opts, "width", side_width());
    let t = type_of(w);
    if t == "i64" || t == "f64" {
        if !(w > 0 && w < 1) { throw `side(): width must be a fraction in (0, 1), "5cm" or "auto" (got ${w})`; }
        m = m.flex(1.0 - w);
        n = n.flex(w);
    } else if w == "auto" {
        m = m.flex(1);
    } else {
        let v = poster_cm("side", w);
        if type_of(v) == "()" { throw `side(): width must be a fraction in (0, 1), "5cm" or "auto" (got ${w})`; }
        m = m.flex(1);
        n = n.width(v);
    }
    let g = plot_opt(opts, "gap", side_gap());
    row(if where == "right" { [m, n] } else { [n, m] }).spacing(g).cross(align)
}
