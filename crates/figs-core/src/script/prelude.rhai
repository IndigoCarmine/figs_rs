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
        throw "grid(): ncols must be a positive integer";
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

// Overlay label `s` at the top-left corner of `item`.
fn labeled(item, s) { stack([as_node(item), label(s).at(0.0, 0.0)]) }

// Label each item in order with `label_style()` ("(a)", "(b)", …).
fn panels(items) { panels(label_style(), items) }

fn panels(style, items) {
    let out = [];
    let i = 0;
    for x in items {
        out.push(labeled(x, panel_label(style, i)));
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Page presets
// ---------------------------------------------------------------------------

// `[width, height]` in cm for a named preset. Journal widths get a 4:3
// height; append "-landscape" to swap the sides of any preset.
fn paper_size(name) {
    if type_of(name) != "string" {
        throw "page(): use page(width, height, root) or page(preset, root)";
    }
    let landscape = name.ends_with("-landscape");
    let key = if landscape { name.sub_string(0, name.len() - 10) } else { name };
    let sizes = #{
        single: [8.5, 6.375],    // single journal column
        double: [17.5, 13.125],  // full page width (two columns)
        a5: [14.8, 21.0],
        a4: [21.0, 29.7],
        a3: [29.7, 42.0],
        a0: [84.1, 118.9],       // poster
        letter: [21.59, 27.94],
    };
    if !sizes.contains(key) {
        throw `page(): unknown preset "${name}" (single, double, a5, a4, a3, a0, letter[-landscape])`;
    }
    let s = sizes[key];
    if landscape { [s[1], s[0]] } else { s }
}

// A page sized by preset name, in cm: `page("double", root)`.
fn page(preset, root) {
    let s = paper_size(preset);
    page(s[0], s[1], root).unit("cm")
}
