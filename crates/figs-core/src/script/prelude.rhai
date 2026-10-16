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

// --- Annotations (see the "Annotations" section below) ---

// Colour of callouts, highlights and spots: a warm red that reads on both
// light plots and dark micrographs.
fn accent() { "#ff5a3c" }

// Stroke width of callout arrows, highlights and spots (cm): 0.04 cm (~1.1 pt)
// at the default 8 pt, so annotations scale with the text.
fn annotation_width() { base_size() * 0.005 }

// Dash pattern of `highlight` / `spot` (on, off, … in cm), tied to the stroke.
fn annotation_dash() { [annotation_width() * 3, annotation_width() * 2] }

// Scale bar and its label colour. White is the micrograph convention (TEM/SEM/
// fluorescence images are mostly dark); `scalebar_halo()` keeps it legible on
// bright regions.
fn scalebar_color() { "#ffffff" }

// Bar thickness (cm): 0.08 cm (~2.3 pt) at the default 8 pt.
fn scalebar_thickness() { base_size() * 0.01 }

// Corner of the panel the scale bar sits in (same names as `label_corner()`).
fn scalebar_corner() { "bottom-right" }

// Distance from the panel corner to the scale bar (cm); follows the label inset.
fn scalebar_inset() { label_inset() }

// Thin outline around the bar and drop shadow under its label, for contrast on
// bright image regions; "" draws neither.
fn scalebar_halo() { "#000000a0" }

// Side inset of a field-scaled `scalebar(len, field, s)` as a fraction of the
// panel width (that form is laid out in fractions, so a cm inset can't apply).
fn scalebar_frac_inset() { 0.025 }

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

// ---------------------------------------------------------------------------
// Annotations (overlays on a panel; coordinates are cm from its top-left)
// ---------------------------------------------------------------------------
//
// Each annotation is one core node meant to be a child of a `stack` over the
// image — `annotate(item, marks)` builds that stack. Colours, widths and dashes
// come from the hooks in the Defaults block (`accent()`, `annotation_width()`,
// `scalebar_*()`), and every result still takes core setters.

// `item` with annotation node(s) `marks` overlaid: `stack([item, ..marks])`.
// `marks` is an array or a single node. Composes with labels:
// `labeled(annotate(img, marks), "(a)")`, or `panels([annotate(..), ..])`.
fn annotate(item, marks) {
    if type_of(marks) != "array" { marks = [marks]; }
    stack([as_node(item)] + marks)
}

// `node` pinned `inset` (default `label_inset()`) in from `corner` of the
// enclosing stack — the same mechanics as `labeled`: "top-left" is a plain
// `.at(inset, inset)`; other corners use a filling column padded by the inset
// and aligned to the corner. Usable for any overlay (an inset, a "50 °C" tag).
fn pin(node, corner) { pin(node, corner, label_inset()) }

fn pin(node, corner, inset) {
    if corner == "top-left" { return node.at(inset, inset); }
    let align = #{
        "top-right": ["start", "end"],
        "bottom-left": ["end", "start"],
        "bottom-right": ["end", "end"],
    };
    if !align.contains(corner) {
        throw `pin(): unknown corner "${corner}" (top-left, top-right, bottom-left, bottom-right)`;
    }
    let a = align[corner];
    col([node]).pad(inset).main(a[0]).cross(a[1])
}

// The scale bar's label: bold, base size, centered, in `scalebar_color()`,
// over a drop shadow in `scalebar_halo()` (both copies span the label's box;
// the shadow is nudged down via its `y` offset, so centring is preserved).
fn scalebar_label(s) {
    let fg = text(s).size(base_size()).bold().center().color(scalebar_color());
    let halo = scalebar_halo();
    if halo == "" { return fg; }
    let shadow = text(s).size(base_size()).bold().center().color(halo);
    stack([shadow.y(scalebar_thickness() * 0.25), fg.y(0)])
}

// The bar itself: a filled rect `scalebar_thickness()` tall, outlined in
// `scalebar_halo()`. Width is set by the caller (cm or a flex share).
fn scalebar_rect() {
    let t = scalebar_thickness();
    let bar = rect().height(t).fill(scalebar_color());
    let halo = scalebar_halo();
    if halo == "" { bar } else { bar.stroke(halo).stroke_width(t * 0.15) }
}

// A scale bar `len` cm long with label `s` centered above it ("" for no label),
// pinned `scalebar_inset()` in from `scalebar_corner()` of the panel.
fn scalebar(len, s) {
    let bar = scalebar_rect().width(len);
    let parts = if s == "" { [bar] } else { [scalebar_label(s), bar] };
    let tag = col(parts).cross("center").spacing(scalebar_thickness() * 0.5);
    pin(tag, scalebar_corner(), scalebar_inset())
}

// A scale bar in image/physical units: `len` out of a field `field` wide, in
// the same unit — `scalebar(1, 12.5, "1 μm")` for a 12.5 μm field of view, or
// `scalebar(80, 1024, "1 μm")` for 80 px of a 1024 px wide image. The bar is
// exactly len/field of the panel width (flex shares), so it stays correct
// whatever size the layout gives the panel — provided the image spans the full
// panel width (natural aspect, as on auto-height pages; not letterboxed).
// Sideways it sits `scalebar_frac_inset()` of the width in from the edge.
fn scalebar(len, field, s) {
    if !(field > 0) || !(len > 0) || len > field {
        throw `scalebar(): need 0 < len <= field (got ${len} of ${field})`;
    }
    let corner = scalebar_corner();
    if !["top-left", "top-right", "bottom-left", "bottom-right"].contains(corner) {
        throw `scalebar_corner(): unknown corner "${corner}" (top-left, top-right, bottom-left, bottom-right)`;
    }
    let right = corner.ends_with("right");
    let m = scalebar_frac_inset() * field;      // side inset, field units
    let rest = field - len - m;                 // free space on the far side
    if rest < 0 { rest = 0; }
    // Bar row: [rest | bar | m] (mirrored on the left).
    let bar = if right { frac_row(rest, scalebar_rect(), len, m) }
              else { frac_row(m, scalebar_rect(), len, rest) };
    let t = scalebar_thickness();
    let parts = [bar];
    if s != "" {
        // Label cell centered on the bar, reaching the near panel edge.
        let lead = rest - m;
        if lead < 0 { lead = 0; }
        let cell = field - lead;
        let lbl = if right { frac_row(lead, scalebar_label(s), cell, 0) }
                  else { frac_row(0, scalebar_label(s), cell, lead) };
        parts = [lbl, rect().height(t * 0.5), bar];
    }
    let v = rect().height(scalebar_inset());
    if corner.starts_with("bottom") {
        col(parts + [v]).main("end").stretch()
    } else {
        col([v] + parts).stretch()
    }
}

// Row of [spacer, node, spacer] with flex weights; zero-weight spacers dropped.
fn frac_row(lead, node, weight, trail) {
    let parts = [];
    if lead > 0 { parts.push(rect().flex(lead)); }
    parts.push(node.flex(weight));
    if trail > 0 { parts.push(rect().flex(trail)); }
    row(parts)
}

// An arrow from (tx, ty) to (x, y) — the core `arrow(tx, ty, x, y)` — with
// label `s` at its tail, in `accent()`. The label sits beside the tail on the
// side away from the target (left of it when the target is to the right, else
// right of it), vertically centered on `ty`. A node `s` is used as is.
fn callout(tx, ty, x, y, s) {
    let t = if type_of(s) == "string" {
        text(s).size(base_size()).bold().color(accent())
    } else { s };
    let gap = base_size() * 0.01;       // tail to text (cm)
    let k = base_size() * 0.053;        // half the label box: 1.5 font sizes (cm)
    let tag = if x >= tx {
        // Right edge at the tail: a wide right-aligned box hanging off to the
        // left (offsets may be negative), so long labels never wrap.
        let reach = 50;
        col([t]).cross("end").main("center").width(reach).height(2 * k)
            .at(tx - gap - reach, ty - k)
    } else {
        col([t]).main("center").height(2 * k).at(tx + gap, ty - k)
    };
    let a = arrow(tx, ty, x, y).stroke(accent()).stroke_width(annotation_width());
    stack([tag, a])
}

// A dashed box with its top-left at (x, y), `w` x `h` cm, in `accent()`
// (a closed core `path`; `.dash([])` makes it solid).
fn highlight(x, y, w, h) {
    path([[0, 0], [w, 0], [w, h], [0, h]]).closed()
        .stroke(accent()).stroke_width(annotation_width()).dash(annotation_dash())
        .at(x, y)
}

// A dashed circle of radius `r` centered on (cx, cy), in `accent()`.
fn spot(cx, cy, r) {
    circle(cx, cy, r).stroke(accent()).stroke_width(annotation_width())
        .dash(annotation_dash())
}
