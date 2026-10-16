// figs framework prelude — opinionated components on top of the core builders.
//
// This file is merged into every script before it runs (see `prelude.rs`), so
// all functions below are callable without an `import`. Everything here is
// plain Rhai built from the core builders (`text`/`image`/`rect`/`col`/`row`/
// `stack`/`page`), and every component returns an ordinary core node or doc, so
// core setters chain on the result as usual (`grid(2, xs).pad(0.5)`).
//
// Defaults are *hook functions*: a script that defines a function with the same
// name and arity replaces the prelude's (e.g. `fn base_size() { 10 }` rescales
// all semantic text), and the components below pick the override up.
//
// Lengths are in cm (the unit `page(preset, root)` sets); font sizes in pt.

// ---------------------------------------------------------------------------
// Defaults (overridable hooks)
// ---------------------------------------------------------------------------

// Base font size (pt) every semantic text style is derived from.
fn base_size() { 8 }

// Default gap between siblings in framework containers (page units, cm).
fn gap() { 0.2 }

// Panel label template used by `panels(items)`; see `panel_label`.
fn label_style() { "(a)" }

// Distance from the panel corner to the label (cm). Scales with the text so
// the label never sits flush against the edge: 0.16 cm at the default 8 pt.
fn label_inset() { base_size() * 0.02 }

// Panel corner the label sits in: "top-left" (default), "top-right",
// "bottom-left" or "bottom-right".
fn label_corner() { "top-left" }

// Fill colour of a box behind the label (e.g. "#ffffff" for readability over
// dark images); "" (default) draws no box.
fn label_background() { "" }

// Padding between the label text and its background box (cm).
fn label_pad() { base_size() * 0.01 }

// Space between a panel and its caption in captioned panels (cm). Tighter than
// `gap()` so a caption reads as belonging to the panel above it.
fn caption_gap() { gap() * 0.5 }

// ---------------------------------------------------------------------------
// Semantic text
// ---------------------------------------------------------------------------

// Running text; also what a bare string in a framework container becomes.
fn body(s) { text(s).size(base_size()) }

// Figure/poster title: 1.5x base, bold, centered.
fn title(s) { text(s).size(base_size() * 1.5).bold().center() }

// Caption under a panel: base size, centered.
fn caption(s) { text(s).size(base_size()).center() }

// Panel label ("(a)"): 1.25x base, bold.
fn label(s) { text(s).size(base_size() * 1.25).bold() }

// ---------------------------------------------------------------------------
// Bare-string sugar
// ---------------------------------------------------------------------------

// A bare string becomes `body(s)`; anything else passes through untouched.
fn as_node(x) {
    if type_of(x) == "string" { body(x) } else { x }
}

// `as_node` over an array — the bridge to core containers:
// `col(nodes(["Results", image("a.png")]))`.
fn nodes(items) {
    let out = [];
    for x in items { out.push(as_node(x)); }
    out
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
    for x in items {
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
    col(rows).spacing(g).stretch().flex(1)
}

// ---------------------------------------------------------------------------
// Shapes (sugar over the core `ellipse`)
// ---------------------------------------------------------------------------

// A circle of radius `r` (a square `ellipse`).
fn circle(r) { ellipse().width(2 * r).height(2 * r) }

// A circle of radius `r` centered on (cx, cy) of the enclosing `stack`.
fn circle(cx, cy, r) { circle(r).at(cx - r, cy - r) }

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

// The label node: `label(s)`, boxed in `label_background()` when that is set
// (a stack of a filling rect under the text, padded via the text's margin).
fn label_tag(s) {
    let t = label(s);
    let bg = label_background();
    if bg == "" { return t; }
    stack([rect().fill(bg), t.at(0.0, 0.0).margin(label_pad())])
}

// Overlay label `s` on `item`, `label_inset()` in from its `label_corner()`.
//
// Stack offsets only measure from the top-left, so "top-left" pins the label
// with `.at(inset, inset)`. The other corners put it in a column that fills the
// stack (no offset, no size), padded by the inset and aligned to the corner
// with main/cross "end" — plain core nodes either way.
fn labeled(item, s) {
    let tag = label_tag(s);
    let inset = label_inset();
    let corner = label_corner();
    // [main, cross] alignment of the filling column, per corner.
    let align = #{
        "top-right": ["start", "end"],
        "bottom-left": ["end", "start"],
        "bottom-right": ["end", "end"],
    };
    let overlay = if corner == "top-left" {
        tag.at(inset, inset)
    } else if align.contains(corner) {
        let a = align[corner];
        col([tag]).pad(inset).main(a[0]).cross(a[1])
    } else {
        throw `label_corner(): unknown corner "${corner}" (top-left, top-right, bottom-left, bottom-right)`;
    };
    stack([as_node(item), overlay])
}

// `item` with caption `c` (a string becomes `caption(c)`) below it, `caption_gap()`
// apart. The item takes the cell's spare height; the caption spans its width.
fn captioned(item, c) {
    let cap = if type_of(c) == "string" { caption(c) } else { c };
    col([as_node(item).flex(1), cap]).spacing(caption_gap()).stretch()
}

// Label each item in order with `label_style()` ("(a)", "(b)", …).
fn panels(items) { panels(label_style(), items) }

// `panels(style, items)`, or `panels(items, captions)`: labeled + captioned.
fn panels(style, items) {
    if type_of(style) == "array" { return panels(label_style(), style, items); }
    let out = [];
    let i = 0;
    for x in items {
        out.push(labeled(x, panel_label(style, i)));
        i += 1;
    }
    out
}

// Labeled panels, each with its caption below: `captioned(labeled(..), c)`.
fn panels(style, items, captions) {
    if items.len() != captions.len() {
        throw `panels(): ${items.len()} items but ${captions.len()} captions`;
    }
    let out = [];
    let i = 0;
    for x in panels(style, items) {
        out.push(captioned(x, captions[i]));
        i += 1;
    }
    out
}

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
        single: 8.5,    // single journal column
        double: 17.5,   // full page width (two columns)
    };
    let paper = #{
        a5: [14.8, 21.0],
        a4: [21.0, 29.7],
        a3: [29.7, 42.0],
        a0: [84.1, 118.9],       // poster
        letter: [21.59, 27.94],
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
        throw `page(): unknown preset "${name}" (single, double, a5, a4, a3, a0, letter; paper sizes take -landscape)`;
    }
    let s = paper[key];
    if landscape { [s[1], s[0]] } else { s }
}

// A page sized by preset name, in cm: `page("double", root)`.
fn page(preset, root) {
    let s = paper_size(preset);
    page(s[0], s[1], root).unit("cm")
}
