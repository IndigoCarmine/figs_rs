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
//
// Every hook reads its own key from the per-script style registry first
// (`style_get(key, default)`; see "Style presets" below), so `style("nature")`
// or `style(#{ base_size: 9 })` switches them all at once; the second argument
// is the built-in default. A user `fn` with the same name replaces the whole
// hook, so it always wins over any preset.

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

// Widths (cm) of the "single" (one journal column) and "double" (full page
// width) page presets.
fn single_width() { style_get("single_width", 8.5) }
fn double_width() { style_get("double_width", 17.5) }

// --- Annotations (see the "Annotations" section below) ---

// Colour of callouts, highlights and spots: a warm red that reads on both
// light plots and dark micrographs.
fn accent() { style_get("accent", "#ff5a3c") }

// Stroke width of callout arrows, highlights and spots (cm): 0.04 cm (~1.1 pt)
// at the default 8 pt, so annotations scale with the text.
fn annotation_width() { style_get("annotation_width", base_size() * 0.005) }

// Dash pattern of `highlight` / `spot` (on, off, … in cm), tied to the stroke.
fn annotation_dash() {
    style_get("annotation_dash", [annotation_width() * 3, annotation_width() * 2])
}

// Scale bar and its label colour. White is the micrograph convention (TEM/SEM/
// fluorescence images are mostly dark); `scalebar_halo()` keeps it legible on
// bright regions.
fn scalebar_color() { style_get("scalebar_color", "#ffffff") }

// Bar thickness (cm): 0.08 cm (~2.3 pt) at the default 8 pt.
fn scalebar_thickness() { style_get("scalebar_thickness", base_size() * 0.01) }

// Corner of the panel the scale bar sits in (same names as `label_corner()`).
fn scalebar_corner() { style_get("scalebar_corner", "bottom-right") }

// Distance from the panel corner to the scale bar (cm); follows the label inset.
fn scalebar_inset() { style_get("scalebar_inset", label_inset()) }

// Thin outline around the bar and its label, for contrast on bright image
// regions; "" draws neither.
fn scalebar_halo() { style_get("scalebar_halo", "#000000a0") }

// Side inset of a field-scaled `scalebar(len, field, s)` as a fraction of the
// panel width (that form is laid out in fractions, so a cm inset can't apply).
fn scalebar_frac_inset() { style_get("scalebar_frac_inset", 0.025) }

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
    let poster = #{ base_size: 24, title_size: 60, gap: 1.0 };
    let dark = #{
        fg: "#ffffff", palette: dark_palette(), page_bg: "#0f1a2e",
        label_halo: "#000000", marker_ring: "#0f1a2e",
    };
    // The dark conference-poster look (see the "Poster" section): a navy
    // page gradient lit from the top, top-lit section bars with a hairline
    // pale-cyan border, amber notes, table cells a shade off the page.
    let poster_dark = #{
        poster_bg: linear(90, [[0, "#28314c"], [0.12, "#181e2e"], [0.38, "#10141f"], [1, "#080a10"]]),
        section_bar_fill: linear(90, [[0, "#3d4a73"], [0.56, "#181d2d"], [1, "#080a10"]]),
        section_bar_border: "#caeefb", note_color: "#ffc000", table_fill: "#121622",
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
        // plots
        "plot_line_width", "series_width", "palette", "tick_length", "tick_direction",
        "minor_ticks", "max_ticks", "marker_size", "marker_open", "marker_stroke_width",
        "marker_ring", "plot_frame", "legend_corner", "legend_style", "bar_width",
        "plot_aspect", "plot_label_room", "plot_pad", "ylabel_rotated", "plot_title_size",
        // colours (theme)
        "fg", "plot_fg", "plot_bg", "page_bg",
        // posters
        "poster_size", "poster_bg", "poster_margin", "poster_gutter", "poster_row_gap",
        "poster_title_size", "poster_title_font", "author_size", "sup_scale",
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

// "a, b, c" from an array of strings (for error messages).
fn list_names(xs) {
    let out = "";
    for x in xs {
        if out != "" { out += ", "; }
        out += x;
    }
    out
}

// --- Plots (see the "Plots" section below) ---

// Stroke width of plot axes and ticks (cm): ~0.8 pt at the default 8 pt.
fn plot_line_width() { style_get("plot_line_width", base_size() * 0.0035) }

// Width of data lines (cm): ~1.4 pt at the default 8 pt.
fn series_width() { style_get("series_width", plot_line_width() * 1.75) }

// Series colours, assigned in order to series without an explicit colour:
// Okabe–Ito, distinguishable with the common colour-vision deficiencies and
// legible in print. Yellow and black come last. (`dark_palette()` is the
// variant for dark backgrounds that the "dark" presets switch to.)
fn palette() {
    style_get("palette",
        ["#0072B2", "#E69F00", "#009E73", "#D55E00", "#56B4E9", "#CC79A7", "#F0E442", "#000000"])
}

// Length of the tick marks (cm): 0.12 cm at the default 8 pt.
fn tick_length() { style_get("tick_length", base_size() * 0.015) }

// Ticks point "out" of the data area (default, the journal convention) or
// "in" (into it, the instrument-software look).
fn tick_direction() { style_get("tick_direction", "out") }

// Unlabelled minor ticks between the labelled ones on linear axes: this many
// intervals per step (e.g. 2 halves it, 5 for fifths); 0 (default) for none.
// Log axes always get their 2–9 × 10ⁿ minor ticks.
fn minor_ticks() { style_get("minor_ticks", 0) }

// At most this many labelled ticks per axis; steps are 1, 2 or 5 x 10^n.
fn max_ticks() { style_get("max_ticks", 7) }

// Marker diameter (cm): 0.16 cm (~4.5 pt) at the default 8 pt.
fn marker_size() { style_get("marker_size", base_size() * 0.02) }

// Default for the series option `open`: hollow markers (an outline in the
// series colour) instead of filled ones.
fn marker_open() { style_get("marker_open", false) }

// Outline width of hollow markers (cm): ~1 pt at the default 8 pt, a little
// heavier than the axes so open circles stay visible over a fit line.
fn marker_stroke_width() { style_get("marker_stroke_width", plot_line_width() * 1.25) }

// Ring around filled markers, in the background colour, so overlapping
// markers stay apart; "" for none.
fn marker_ring() { style_get("marker_ring", "#ffffff") }

// Plot frame: "open" (left + bottom axes, the usual journal style), "box"
// (all four sides) or "none".
fn plot_frame() { style_get("plot_frame", "open") }

// Legend placement when series are labelled: "auto" (the corner with the
// least data in it), a corner name ("top-right", …), or "" for no legend.
fn legend_corner() { style_get("legend_corner", "auto") }

// Legend look: "swatch" (a line/marker key before each label) or "text" (the
// labels alone, each in its series colour — the poster/slide convention).
fn legend_style() { style_get("legend_style", "swatch") }

// Bar width as a fraction of the spacing between bars (or categories).
fn bar_width() { style_get("bar_width", 0.7) }

// Width / height of a whole plot panel (axes and labels included), so plots
// get a height on auto-height pages; 0 leaves the size to the layout.
fn plot_aspect() { style_get("plot_aspect", 4.0 / 3.0) }

// Empty band at the top of a plot that a panel label fits in (cm): the label
// inset plus one label line. Set it to 0 for plots that are never labelled.
fn plot_label_room() {
    style_get("plot_label_room", label_inset() + base_size() * 1.25 * 1.3 * 2.54 / 72.0)
}

// Clear space between a plot's outermost marks (tick labels, axis labels)
// and its box (cm), so labels never touch the panel edge.
fn plot_pad() { style_get("plot_pad", base_size() * 0.01) }

// The y axis label turned 90° (reading upwards) left of the y tick labels and
// centred on the axis (true), or horizontal above the axis (false).
fn ylabel_rotated() { style_get("ylabel_rotated", true) }

// Size (pt) of a plot's `title` (centred above the data area): the base size,
// like the axis labels.
fn plot_title_size() { style_get("plot_title_size", base_size()) }

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

// Series colours for dark backgrounds: Okabe–Ito hues lifted so each has at
// least 6:1 contrast (WCAG) on a dark navy (#0f1a2e) — readable as legend
// text — and stays apart under deuteranopia/protanopia. White comes last.
fn dark_palette() {
    ["#FF9F40", "#5CC8FF", "#3DDC97", "#FF6B5A", "#F28CB8", "#F0E442", "#FFFFFF"]
}

// Colour `i` (0-based, wrapping) of `palette()`, to share one colour between
// series — e.g. data markers and their fit line. A series' `color` option
// also takes the index directly: `#{ color: 1 }`.
fn palette_color(i) {
    let p = palette();
    if type_of(i) != "i64" || p.len() == 0 {
        throw `palette_color(): expected an integer index (got ${type_of(i)} ${i})`;
    }
    p[((i % p.len()) + p.len()) % p.len()]
}

// ---------------------------------------------------------------------------
// Semantic text
// ---------------------------------------------------------------------------

// `text(s)` in the theme's `fg()` colour (no colour set while it is "").
fn fg_text(s) {
    let c = fg();
    if c == "" { text(s) } else { text(s).color(c) }
}

// Running text; also what a bare string in a framework container becomes.
fn body(s) { fg_text(s).size(base_size()) }

// Figure/poster title: `title_size()` (1.5x base), bold, centered.
fn title(s) { fg_text(s).size(title_size()).bold().center() }

// Caption under a panel: base size, centered.
fn caption(s) { fg_text(s).size(base_size()).center() }

// Panel label ("(a)"): `label_size()` (1.25x base), bold.
fn label(s) { fg_text(s).size(label_size()).bold() }

// `t` (a text node) with a halo: the core text outline (`stroke`) in `color`,
// reaching `r` (cm) past the glyph edges with round joins. The outline is
// painted behind the glyphs, so they keep their exact shape, and it is purely
// visual: the result lays out exactly like `t` alone (same box, same corner
// placement). Works for any overlay text: `halo(body("50 °C"), "#000000", 0.03)`.
fn halo(t, color, r) { t.stroke(color).stroke_width(2.0 * r) }

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
fn labeled(item, s) {
    let tag = pin_in("label_corner", label_tag(s), label_corner(), label_inset());
    stack([as_node(item), tag])
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
// style's `page_bg()` background when one is set.
fn page(preset, root) {
    let s = paper_size(preset);
    let d = page(s[0], s[1], root).unit("cm");
    let bg = page_bg();
    if bg != "" { d = d.background(bg); }
    let f = font_family();
    if f == "" { d } else { d.font(f) }
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

fn pin(node, corner, inset) { pin_in("pin", node, corner, inset) }

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

// The scale bar's label: bold, base size, centered, in `scalebar_color()`,
// outlined in `scalebar_halo()` (a `halo` reaching a quarter of the bar
// thickness past the glyphs).
fn scalebar_label(s) {
    let fg = text(s).size(base_size()).bold().center().color(scalebar_color());
    let h = scalebar_halo();
    if h == "" { fg } else { halo(fg, h, scalebar_thickness() * 0.25) }
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

// ---------------------------------------------------------------------------
// Plots (data -> core nodes through `viewbox` stacks/paths and `anchor`)
// ---------------------------------------------------------------------------
//
// `plot(opts, series)` lays out axes of whatever size the layout gives it.
// The data area is a core `stack` with `.viewbox(xlo, yhi, xhi, ylo)`: data
// lines and bars are `path`s with the same viewbox, markers and ticks are
// children placed `.at(x, y)` in data units and centred with `.anchor`. So the
// mapping data -> page is done by the layout, at whatever size the plot ends
// up, and everything is ordinary core nodes. Tick and axis labels hang off the
// area in the same data units, inside a `.contain()` stack that shrinks the
// area until they fit: every margin is measured by the layout, never guessed
// from `base_size()`. A series is a map built by
// `line_plot` / `scatter` / `bars` / `errorbars`; styling comes from the hooks
// in the Defaults block (`plot_line_width()`, `palette()`, `tick_length()`, …).
// Lengths are cm, as everywhere in the framework.

// --- numbers ---

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

// Axis transform: identity, or log10 for log axes (non-positive -> gap).
fn plot_tf(v, log) {
    if !plot_finite(v) { return (); }
    let f = v.to_float();
    if log { if f > 0.0 { f.log() } else { () } } else { f }
}

// The fewest decimals (<= 6) that print every value in `vals` exactly.
fn tick_decimals(vals) {
    for d in 0..7 {
        let m = 10.0 ** d.to_float();
        let ok = true;
        for v in vals {
            let x = v * m;
            if (x - x.round()).abs() > 1e-6 * (if x.abs() > 1.0 { x.abs() } else { 1.0 }) {
                ok = false;
                break;
            }
        }
        if ok { return d; }
    }
    6
}

// `v` with `d` decimals and a typographic minus sign: format_num(-0.5, 1) == "−0.5".
fn format_num(v, d) {
    let m = (v.abs() * 10.0 ** d.to_float()).round();
    let s = `${m.to_int()}`;
    if d > 0 {
        while s.len() <= d { s = "0" + s; }
        s = s.sub_string(0, s.len() - d) + "." + s.sub_string(s.len() - d);
    }
    if v < 0.0 && m > 0.0 { "−" + s } else { s }
}

// Integer `n` in superscript digits ("⁻³").
fn superscript(n) {
    let digits = "⁰¹²³⁴⁵⁶⁷⁸⁹";
    let s = if n < 0 { "⁻" } else { "" };
    for c in `${n.abs()}` { s += digits["0123456789".index_of(c)]; }
    s
}

// Label of the decade 10^e: plain ("0.01", "1000") or a power ("10⁻⁵").
fn log_label(e, plain) {
    if plain {
        format_num(10.0 ** e.to_float(), if e < 0 { -e } else { 0 })
    } else {
        "10" + superscript(e)
    }
}

// --- ticks ---

// A "nice" axis over [lo, hi] (lo < hi): `[a, b, step]` with the smallest step
// of 1, 2 or 5 x 10^n giving at most `max_ticks()` ticks. With `snap` the
// range grows outwards to whole steps (automatic ranges end on a tick);
// without it [lo, hi] is kept.
fn nice_axis(lo, hi, snap) {
    let lo = lo.to_float();
    let hi = hi.to_float();
    if !(hi > lo) { throw `nice_axis(): need lo < hi (got ${lo}, ${hi})`; }
    let n = max_ticks();
    if type_of(n) != "i64" || n < 2 { throw `max_ticks(): must be an integer >= 2 (got ${n})`; }
    let e = ((hi - lo) / n.to_float()).log().floor() - 1.0;
    for k in 0..6 {
        for f in [1.0, 2.0, 5.0] {
            let step = f * 10.0 ** (e + k.to_float());
            let a = if snap { (lo / step + 1e-9).floor() * step } else { lo };
            let b = if snap { (hi / step - 1e-9).ceiling() * step } else { hi };
            let count = (b / step + 1e-9).floor() - (a / step - 1e-9).ceiling() + 1.0;
            if count <= n.to_float() { return [a, b, step]; }
        }
    }
    [lo, hi, hi - lo]
}

// The multiples of `step` within [lo, hi].
fn nice_ticks(lo, hi, step) {
    let i0 = (lo / step - 1e-9).ceiling().to_int();
    let i1 = (hi / step + 1e-9).floor().to_int();
    let out = [];
    for i in i0..=i1 { out.push(i.to_float() * step); }
    out
}

// An axis description: `#{ lo, hi, ticks, labels, minor, marks }`, in
// transformed units (log10 for log axes). `range` is [lo, hi] or (); `ticks`
// explicit tick values or (); `cats` category names (a categorical axis).
fn plot_axis(name, dlo, dhi, range, log, ticks, cats) {
    let ax = #{ lo: 0.0, hi: 1.0, ticks: [], labels: [], minor: [], marks: true };
    if cats.len() > 0 {
        ax.lo = -0.5;
        ax.hi = cats.len().to_float() - 0.5;
        for i in 0..cats.len() { ax.ticks.push(i.to_float()); ax.labels.push(cats[i]); }
        ax.marks = false;
        return ax;
    }
    let fixed = type_of(range) != "()";
    if fixed {
        let r = plot_numbers("plot", `${name} range`, range);
        if r.len() != 2 || !(r[1] > r[0]) {
            throw `plot(): ${name} range must be [lo, hi] with lo < hi (got ${range})`;
        }
        if log && r[0] <= 0.0 { throw `plot(): ${name} range of a log axis must be positive (got ${range})`; }
        dlo = plot_tf(r[0], log);
        dhi = plot_tf(r[1], log);
    }
    if type_of(dlo) == "()" {
        // No data at all: a unit range.
        dlo = 0.0;
        dhi = 1.0;
        fixed = true;
    }
    if log {
        let lo = if fixed { dlo } else { (dlo + 1e-9).floor() };
        let hi = if fixed { dhi } else { (dhi - 1e-9).ceiling() };
        if hi <= lo { hi = lo + 1.0; }
        ax.lo = lo;
        ax.hi = hi;
        let e0 = (lo - 1e-9).ceiling().to_int();
        let e1 = (hi + 1e-9).floor().to_int();
        let every = ((e1 - e0 + 1).to_float() / max_ticks().to_float()).ceiling().to_int();
        if every < 1 { every = 1; }
        if type_of(ticks) == "()" {
            for e in e0..=e1 {
                // One style per axis: plain numbers while every decade is in
                // 0.01 … 1000, powers of ten otherwise.
                let plain = e0 >= -2 && e1 <= 3;
                if (e - e0) % every == 0 { ax.ticks.push(e.to_float()); ax.labels.push(log_label(e, plain)); }
            }
        } else {
            for v in plot_numbers("plot", `${name}ticks`, ticks) {
                let t = plot_tf(v, true);
                if type_of(t) != "()" { ax.ticks.push(t); ax.labels.push(format_num(v, tick_decimals([v]))); }
            }
        }
        // Minor ticks at 2..9 x 10^e.
        for e in (e0 - 1)..=e1 {
            for m in 2..10 {
                let v = e.to_float() + m.to_float().log();
                if v >= lo && v <= hi { ax.minor.push(v); }
            }
        }
        return ax;
    }
    let lo = dlo;
    let hi = dhi;
    if !(hi > lo) {
        // Constant data: open up a range around it.
        let d = if lo == 0.0 { 1.0 } else { lo.abs() * 0.5 };
        lo -= d;
        hi += d;
    } else if !fixed {
        // A small margin so extreme points don't sit on the frame — except
        // at 0, a natural bound (time from 0, bars from 0).
        let m = (hi - lo) * 0.02;
        if lo != 0.0 { lo -= m; }
        if hi != 0.0 { hi += m; }
    }
    let a = nice_axis(lo, hi, !fixed);
    ax.lo = a[0];
    ax.hi = a[1];
    let tv = if type_of(ticks) == "()" { nice_ticks(a[0], a[1], a[2]) }
             else { plot_numbers("plot", `${name}ticks`, ticks) };
    let d = tick_decimals(tv);
    for v in tv {
        ax.ticks.push(v);
        ax.labels.push(format_num(v, d));
    }
    ax
}

// --- series ---

// A series map (what `plot` draws): kind, x, y and the options below.
fn plot_series(kind, xs, ys, opts) {
    if type_of(opts) == "string" { opts = #{ label: opts }; }
    if type_of(opts) != "map" {
        throw `${kind}(): options must be a label string or a map (got ${type_of(opts)})`;
    }
    if type_of(xs) != "array" || type_of(ys) != "array" {
        throw `${kind}(): x and y must be arrays`;
    }
    if xs.len() != ys.len() {
        throw `${kind}(): x and y differ in length (${xs.len()} vs ${ys.len()})`;
    }
    let ys = plot_numbers(kind, "y", ys);
    let cats = xs.len() > 0 && type_of(xs[0]) == "string";
    if cats {
        for v in xs {
            if type_of(v) != "string" { throw `${kind}(): x mixes category names and numbers`; }
        }
    } else {
        xs = plot_numbers(kind, "x", xs);
    }
    let s = #{ kind: if kind == "line_plot" { "line" } else { kind }, x: xs, y: ys, label: "", color: "", width: (), dash: [],
               marker: if kind == "scatter" { "o" } else { "none" }, size: (), yerr: (), open: marker_open() };
    let known = ["label", "color", "width", "dash", "marker", "size", "yerr", "open"];
    for k in opts.keys() {
        if !known.contains(k) {
            throw `${kind}(): unknown option "${k}" (label, color, width, dash, marker, size, yerr, open)`;
        }
        s[k] = opts[k];
    }
    if type_of(s.color) == "i64" { s.color = palette_color(s.color); }
    if type_of(s.color) != "string" {
        throw `${kind}(): color must be "#rrggbb" or a palette index (got ${type_of(s.color)})`;
    }
    if type_of(s.open) != "bool" { throw `${kind}(): open must be true or false (got ${s.open})`; }
    if type_of(s.yerr) != "()" {
        s.yerr = plot_numbers(kind, "yerr", s.yerr);
        if s.yerr.len() != ys.len() {
            throw `${kind}(): yerr has ${s.yerr.len()} values for ${ys.len()} points`;
        }
    }
    if !["o", "s", "^", "d", "none"].contains(s.marker) {
        throw `${kind}(): unknown marker "${s.marker}" (o, s, ^, d, none)`;
    }
    s
}

// A line through the points; `opts` is a legend label or a map of
// label/color/width/dash/marker/size/yerr/open.
fn line_plot(xs, ys) { plot_series("line_plot", xs, ys, #{}) }
fn line_plot(xs, ys, opts) { plot_series("line_plot", xs, ys, opts) }

// Markers at the points (marker "o"; also "s", "^", "d"); `open: true` draws
// them hollow.
fn scatter(xs, ys) { plot_series("scatter", xs, ys, #{}) }
fn scatter(xs, ys, opts) { plot_series("scatter", xs, ys, opts) }

// Bars from 0 to each y; `xs` are numbers or category names. Several bar
// series in one plot are grouped side by side.
fn bars(xs, ys) { plot_series("bars", xs, ys, #{}) }
fn bars(xs, ys, opts) { plot_series("bars", xs, ys, opts) }

// Error bars only: y ± errs (in `plot_fg()` unless `color` is given).
fn errorbars(xs, ys, errs) { errorbars(xs, ys, errs, #{}) }
fn errorbars(xs, ys, errs, opts) {
    if type_of(opts) == "string" { opts = #{ label: opts }; }
    opts.yerr = errs;
    plot_series("errorbars", xs, ys, opts)
}

// Series with their colours filled in from `palette()` (in order; error-bar
// only series default to `plot_fg()` and take no palette slot).
fn plot_colored(series) {
    if type_of(series) == "map" { series = [series]; }
    let pal = palette();
    let out = [];
    let i = 0;
    for s in series {
        if type_of(s) != "map" || !s.contains("kind") {
            throw `plot(): series must be line_plot/scatter/bars/errorbars (got ${type_of(s)})`;
        }
        if s.kind == "errorbars" {
            if s.color == "" { s.color = plot_fg(); }
        } else {
            if s.color == "" { s.color = pal[i % pal.len()]; }
            i += 1;
        }
        out.push(s);
    }
    out
}

// --- marks ---

// Plot text: base size, in `plot_fg()`, in the page font (no font of its
// own, so CJK labels and the style's `font_family()` apply as everywhere).
fn plot_text(s) { text(s).size(base_size()).color(plot_fg()) }

// A marker of `shape` ("o", "s", "^", "d") and diameter `d`, filled with `c`
// and ringed in `marker_ring()` so overlapping markers stay apart — or, when
// `open`, hollow: outlined in `c`, `marker_stroke_width()` wide.
fn plot_marker(shape, d, c, open) {
    let m = plot_marker_shape(shape, d);
    if open { return m.stroke(c).stroke_width(marker_stroke_width()); }
    let ring = marker_ring();
    if ring == "" { m.fill(c) } else { m.fill(c).stroke(ring).stroke_width(plot_line_width() * 0.5) }
}

fn plot_marker_shape(shape, d) {
    let m = if shape == "s" {
        let a = d * 0.88;
        path([[0, 0], [a, 0], [a, a], [0, a]]).closed()
    } else if shape == "^" {
        let h = d * 0.9;
        path([[0, h], [d * 0.55, 0], [d * 1.1, h]]).closed()
    } else if shape == "d" {
        let r = d * 0.62;
        path([[r, 0], [2 * r, r], [r, 2 * r], [0, r]]).closed()
    } else {
        circle(d / 2)
    };
    m
}

// --- annotations in data coordinates ---

// `plot_text(x, y, s)` / `plot_text(x, y, s, opts)`: text `s` at the data
// point (x, y) of the plot it is given to — in `marks`, or among the series —
// centred on it unless `anchor` says otherwise. Data values as for the series
// (the plot applies log axes), so it stays put when ranges change. Options:
//   color   "#rrggbb" or a palette index (default `plot_fg()`)
//   size    pt (default `base_size()`)
//   bold    true for bold
//   anchor  [ax, ay]: which point of the text sits at (x, y), fractions of
//           its box from its top-left (default [0.5, 0.5], centred)
fn plot_text(x, y, s) { plot_text(x, y, s, #{}) }
fn plot_text(x, y, s, opts) {
    plot_annotation("plot_text", "text", [x, y], s, opts, ["color", "size", "bold", "anchor"])
}

// `plot_arrow(x1, y1, x2, y2)` / `(…, opts)`: an arrow from data point
// (x1, y1) to (x2, y2), in `plot_fg()`, `series_width()` wide. Options:
// `color` (or a palette index), `width` (cm), `head` ("end" default, "start",
// "both"), `dash`.
fn plot_arrow(x1, y1, x2, y2) { plot_arrow(x1, y1, x2, y2, #{}) }
fn plot_arrow(x1, y1, x2, y2, opts) {
    plot_annotation("plot_arrow", "arrow", [x1, y1, x2, y2], "", opts, ["color", "width", "head", "dash"])
}

// An annotation descriptor (`plot` turns it into nodes once the axes exist).
fn plot_annotation(name, kind, pts, s, opts, known) {
    if type_of(opts) != "map" { throw `${name}(): options must be a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        if !known.contains(k) { throw `${name}(): unknown option "${k}" (${list_names(known)})`; }
    }
    let a = #{ mark: kind, pts: plot_numbers(name, "coordinates", pts), s: s, color: plot_fg() };
    if opts.contains("color") {
        a.color = if type_of(opts.color) == "i64" { palette_color(opts.color) } else { opts.color };
    }
    if kind == "text" {
        if type_of(s) != "string" { throw `${name}(): text must be a string (got ${type_of(s)})`; }
        a.size = plot_opt(opts, "size", base_size());
        a.bold = plot_opt(opts, "bold", false);
        a.anchor = plot_numbers(name, "anchor", plot_opt(opts, "anchor", [0.5, 0.5]));
        if a.anchor.len() != 2 { throw `${name}(): anchor must be [ax, ay]`; }
    } else {
        a.width = plot_opt(opts, "width", series_width());
        a.head = plot_opt(opts, "head", "end");
        a.dash = plot_opt(opts, "dash", []);
    }
    a
}

// Is `x` an annotation descriptor (from `plot_text` / `plot_arrow`)?
fn plot_is_annotation(x) { type_of(x) == "map" && x.contains("mark") }

// The node of annotation `a` in a data area spanning [x0, x1] x [y0, y1]
// (viewbox coordinates; `xlog`/`ylog` take log10 of the values).
fn plot_annotation_node(a, xlog, ylog, x0, x1, y0, y1) {
    let p = [];
    for i in 0..a.pts.len() {
        let v = plot_tf(a.pts[i], if i % 2 == 0 { xlog } else { ylog });
        if type_of(v) == "()" { return (); }   // off a log axis: nothing to draw
        p.push(v);
    }
    if a.mark == "text" {
        let t = text(a.s).size(a.size).color(a.color);
        if a.bold { t = t.bold(); }
        return t.at(p[0], p[1]).anchor(a.anchor[0], a.anchor[1]);
    }
    arrow(p[0], p[1], p[2], p[3]).viewbox(x0, y1, x1, y0).stroke(a.color)
        .stroke_width(a.width).head(a.head).dash(a.dash)
}

// Polyline runs of `pts` (arrays [x, y] or () for gaps) clipped to the box
// [x0, x1] x [y0, y1] (Liang–Barsky per segment).
fn plot_runs(pts, x0, x1, y0, y1) {
    let runs = [];
    let run = [];
    let prev = ();
    for p in pts {
        if type_of(p) == "()" || type_of(prev) == "()" {
            if run.len() > 1 { runs.push(run); }
            run = [];
            if type_of(p) != "()" && p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1 {
                run = [p];
            }
            prev = p;
            continue;
        }
        let dx = p[0] - prev[0];
        let dy = p[1] - prev[1];
        let t0 = 0.0;
        let t1 = 1.0;
        let ok = true;
        for pq in [[-dx, prev[0] - x0], [dx, x1 - prev[0]], [-dy, prev[1] - y0], [dy, y1 - prev[1]]] {
            if pq[0] == 0.0 {
                if pq[1] < 0.0 { ok = false; }
            } else {
                let r = pq[1] / pq[0];
                if pq[0] < 0.0 { if r > t1 { ok = false; } else if r > t0 { t0 = r; } }
                else { if r < t0 { ok = false; } else if r < t1 { t1 = r; } }
            }
        }
        if ok {
            let a = [prev[0] + t0 * dx, prev[1] + t0 * dy];
            let b = [prev[0] + t1 * dx, prev[1] + t1 * dy];
            if t0 > 0.0 || run.len() == 0 {
                if run.len() > 1 { runs.push(run); }
                run = [a];
            }
            run.push(b);
            if t1 < 1.0 { runs.push(run); run = []; }
        } else {
            if run.len() > 1 { runs.push(run); }
            run = [];
        }
        prev = p;
    }
    if run.len() > 1 { runs.push(run); }
    runs
}

// --- legend ---

// The key drawn before a legend label: a short line (with its marker), a
// marker, or a filled square for bars. All keys share one width.
fn legend_key(s) {
    let h = marker_size();
    let w = base_size() * 0.06;
    let parts = [];
    if s.kind == "line" {
        let lw = if type_of(s.width) == "()" { series_width() } else { s.width };
        parts.push(line(0, h / 2, w, h / 2).stroke(s.color).stroke_width(lw).dash(s.dash));
    }
    if s.kind == "bars" {
        let a = h * 1.25;
        parts.push(rect().fill(s.color).width(a).height(a).at(w / 2, h / 2).anchor(0.5, 0.5));
    } else if s.kind == "errorbars" {
        parts.push(line(w / 2, 0, w / 2, h).stroke(s.color).stroke_width(plot_line_width()));
    } else if s.marker != "none" {
        let d = if type_of(s.size) == "()" { marker_size() } else { s.size };
        parts.push(plot_marker(s.marker, d, s.color, s.open).at(w / 2, h / 2).anchor(0.5, 0.5));
    }
    stack(parts).width(w).height(h)
}

// A legend for `series` (labelled ones only): one key + label row each — or,
// with `legend_style()` "text", the labels alone in their series colours.
// Usable on its own, e.g. beside a row of plots: `legend([line_plot(..), ..])`;
// `legend(style, series)` picks the style ("swatch" or "text").
fn legend(series) { legend(legend_style(), series) }
fn legend(style, series) { legend_box(plot_colored(plot_only_series(series)), style) }

fn legend_box(series, style) {
    if style != "swatch" && style != "text" {
        throw `legend(): unknown style "${style}" (swatch, text)`;
    }
    let rows = [];
    for s in series {
        if s.label == "" { continue; }
        if style == "text" {
            rows.push(text(s.label).size(base_size()).color(s.color));
        } else {
            rows.push(row([legend_key(s), plot_text(s.label)])
                .cross("center").spacing(base_size() * 0.02));
        }
    }
    col(rows).spacing(base_size() * 0.008)
}

// `series` (an array or one series) without annotation descriptors.
fn plot_only_series(series) {
    if type_of(series) != "array" { series = [series]; }
    series.filter(|x| !plot_is_annotation(x))
}

// The corner of the [0, 1]² data area with the fewest `samples` ([u, v],
// v up) in it, preferring top-right, top-left, bottom-right, bottom-left.
fn legend_auto_corner(samples) {
    let corners = ["top-right", "top-left", "bottom-right", "bottom-left"];
    let best = corners[0];
    let best_n = -1;
    for c in corners {
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

// --- the plot ---

// `opts[k]`, or `d` when absent.
fn plot_opt(opts, k, d) { if opts.contains(k) { opts[k] } else { d } }

// Running min/max that ignores gaps (`()`).
fn plot_min(a, v) {
    if type_of(v) == "()" { a } else if type_of(a) == "()" || v < a { v } else { a }
}
fn plot_max(a, v) {
    if type_of(v) == "()" { a } else if type_of(a) == "()" || v > a { v } else { a }
}

fn plot_inside(p, x0, x1, y0, y1) {
    p[0] >= x0 - 1e-9 && p[0] <= x1 + 1e-9 && p[1] >= y0 - 1e-9 && p[1] <= y1 + 1e-9
}

// Data point -> [u, v] in the unit square of the area (v up).
fn plot_norm(x0, x1, y0, y1, p) { [(p[0] - x0) / (x1 - x0), (p[1] - y0) / (y1 - y0)] }

// `plot(series)` / `plot(opts, series)`: axes holding the series (an array, or
// one series). Options (all optional):
//   x, y           [lo, hi] ranges (default: the data, widened to whole ticks)
//   xlabel, ylabel axis labels (the y label turned 90° left of the tick
//                  labels; horizontal above the axis if `ylabel_rotated()`
//                  is false)
//   xscale, yscale "linear" (default) or "log"
//   xticks, yticks explicit tick values
//   title          a title centred above the data area (`plot_title_size()`)
//   legend         a corner, "auto", "text" (a text legend, `legend_style()`
//                  "text", at `legend_corner()`), "" / false for none, or a
//                  map #{ style, corner } / #{ style, at: [x, y], anchor }
//                  (`at` in data units; default legend_corner())
//   frame          "open", "box" or "none" (default plot_frame())
//   marks          extra nodes for the data area, placed `.at(x, y)` in data
//                  units (log10 of the value on log axes), and/or
//                  `plot_text` / `plot_arrow` annotations (data values)
// `series` may hold `plot_text` / `plot_arrow` annotations too.
// The result is an ordinary node (a column) — chain `.flex`, `.height`, ….
fn plot(series) { plot(#{}, series) }

fn plot(opts, series) {
    if type_of(opts) == "array" && type_of(series) == "map" && !plot_is_annotation(series) {
        throw "plot(): use plot(opts, series) — options first";
    }
    if type_of(opts) != "map" { throw `plot(): options must be a map (got ${type_of(opts)})`; }
    let known = ["x", "y", "xlabel", "ylabel", "xscale", "yscale", "xticks", "yticks",
                 "legend", "frame", "marks", "title"];
    for k in opts.keys() {
        if !known.contains(k) {
            throw `plot(): unknown option "${k}" (${list_names(known)})`;
        }
    }
    let xlog = plot_opt(opts, "xscale", "linear");
    let ylog = plot_opt(opts, "yscale", "linear");
    for sc in [xlog, ylog] {
        if sc != "linear" && sc != "log" { throw `plot(): scale must be "linear" or "log" (got "${sc}")`; }
    }
    xlog = xlog == "log";
    ylog = ylog == "log";
    if type_of(series) != "array" { series = [series]; }
    let notes = series.filter(|x| plot_is_annotation(x));
    let series = plot_colored(plot_only_series(series));
    let extra = plot_opt(opts, "marks", []);
    if type_of(extra) != "array" { extra = [extra]; }
    notes += extra.filter(|x| plot_is_annotation(x));
    extra = extra.filter(|x| !plot_is_annotation(x));

    // Categories (first appearance order) of string-x series.
    let cats = [];
    for s in series {
        if s.x.len() > 0 && type_of(s.x[0]) == "string" {
            if xlog { throw "plot(): category names need a linear x axis"; }
            for c in s.x { if !cats.contains(c) { cats.push(c); } }
        }
    }

    // Bar geometry (x units): slot = spacing of bar positions, grouped.
    let nbars = 0;
    let bar_xs = [];
    for s in series {
        if s.kind == "bars" {
            if xlog || ylog { throw "plot(): bars need linear axes"; }
            nbars += 1;
            if s.x.len() > 0 && type_of(s.x[0]) != "string" {
                for v in s.x { if plot_finite(v) { bar_xs.push(v); } }
            }
        }
    }
    let slot = 1.0;
    if cats.len() == 0 && bar_xs.len() > 1 {
        bar_xs.sort();
        let gap = ();
        for i in 1..bar_xs.len() {
            let d = bar_xs[i] - bar_xs[i - 1];
            if d > 1e-12 && (type_of(gap) == "()" || d < gap) { gap = d; }
        }
        if type_of(gap) != "()" { slot = gap; }
    }
    let group_w = bar_width() * slot;

    // Transformed points per series: [x, y] or () for gaps; error extents.
    let xmin = (); let xmax = (); let ymin = (); let ymax = ();
    let bi = 0;
    let prepared = [];
    for s in series {
        let pts = [];
        let errs = [];
        let off = 0.0;
        let bw = 0.0;
        if s.kind == "bars" {
            bw = group_w / nbars.to_float();
            off = (bi.to_float() - (nbars - 1).to_float() / 2.0) * bw;
            bi += 1;
        }
        for i in 0..s.y.len() {
            let xv = if type_of(s.x[i]) == "string" { cats.index_of(s.x[i]).to_float() } else { s.x[i] };
            let x = plot_tf(xv, xlog);
            let y = plot_tf(s.y[i], ylog);
            if type_of(x) == "()" || type_of(y) == "()" { pts.push(()); errs.push(()); continue; }
            x += off;
            pts.push([x, y]);
            xmin = plot_min(xmin, x); xmax = plot_max(xmax, x);
            ymin = plot_min(ymin, y); ymax = plot_max(ymax, y);
            if s.kind == "bars" {
                xmin = plot_min(xmin, x - bw / 2.0); xmax = plot_max(xmax, x + bw / 2.0);
                ymin = plot_min(ymin, 0.0); ymax = plot_max(ymax, 0.0);
            }
            if type_of(s.yerr) != "()" && plot_finite(s.yerr[i]) {
                let e = s.yerr[i].to_float().abs();
                let lo = plot_tf(s.y[i] - e, ylog);
                let hi = plot_tf(s.y[i] + e, ylog);
                errs.push([lo, hi]);
                ymin = plot_min(ymin, lo); ymax = plot_max(ymax, hi);
            } else { errs.push(()); }
        }
        prepared.push(#{ s: s, pts: pts, errs: errs, bw: bw });
    }

    let xa = plot_axis("x", xmin, xmax, plot_opt(opts, "x", ()), xlog, plot_opt(opts, "xticks", ()), cats);
    let ya = plot_axis("y", ymin, ymax, plot_opt(opts, "y", ()), ylog, plot_opt(opts, "yticks", ()), []);
    let x0 = xa.lo; let x1 = xa.hi; let y0 = ya.lo; let y1 = ya.hi;

    // Data area children (all positions in data units).
    let lw = plot_line_width();
    let tl = tick_length();
    let marks = [];
    let samples = [];    // [u, v] in [0, 1]², for legend placement
    let base = if y0 > 0.0 { y0 } else if y1 < 0.0 { y1 } else { 0.0 };
    if ylog { base = y0; }
    for pr in prepared {
        let s = pr.s;
        let c = s.color;
        if s.kind == "bars" {
            for p in pr.pts {
                if type_of(p) == "()" { continue; }
                let a = p[0] - pr.bw / 2.0;
                let b = p[0] + pr.bw / 2.0;
                if b < x0 || a > x1 { continue; }
                if a < x0 { a = x0; }
                if b > x1 { b = x1; }
                let top = if p[1] > y1 { y1 } else if p[1] < y0 { y0 } else { p[1] };
                marks.push(path([[a, base], [b, base], [b, top], [a, top]]).closed().fill(c)
                    .viewbox(x0, y1, x1, y0));
                for k in 0..5 { samples.push(plot_norm(x0, x1, y0, y1, [p[0], base + (top - base) * k.to_float() / 4.0])); }
            }
        }
        if s.kind == "line" {
            let w = if type_of(s.width) == "()" { series_width() } else { s.width };
            for r in plot_runs(pr.pts, x0, x1, y0, y1) {
                marks.push(path(r).viewbox(x0, y1, x1, y0).stroke(c).stroke_width(w)
                    .cap("round").dash(s.dash));
                for i in 1..r.len() {
                    for k in 0..4 {
                        let t = k.to_float() / 4.0;
                        samples.push(plot_norm(x0, x1, y0, y1, [r[i - 1][0] + (r[i][0] - r[i - 1][0]) * t,
                                                r[i - 1][1] + (r[i][1] - r[i - 1][1]) * t]));
                    }
                }
            }
        }
        // Error bars: `plot_fg()` on bars (the bar's colour would vanish into it).
        let ec = if s.kind == "bars" { plot_fg() } else { c };
        let cap = marker_size();
        for i in 0..pr.errs.len() {
            let e = pr.errs[i];
            let p = pr.pts[i];
            if type_of(e) == "()" || type_of(p) == "()" || p[0] < x0 || p[0] > x1 { continue; }
            let lo = if type_of(e[0]) == "()" || e[0] < y0 { y0 } else { e[0] };
            let hi = if type_of(e[1]) == "()" || e[1] > y1 { y1 } else { e[1] };
            if hi <= lo { continue; }
            marks.push(path([[p[0], lo], [p[0], hi]]).viewbox(x0, y1, x1, y0).stroke(ec).stroke_width(lw));
            for yv in [e[0], e[1]] {
                if type_of(yv) != "()" && yv >= y0 && yv <= y1 {
                    marks.push(line(0, 0, cap, 0).stroke(ec).stroke_width(lw).at(p[0], yv).anchor(0.5, 0));
                }
            }
            samples.push(plot_norm(x0, x1, y0, y1, [p[0], lo]));
            samples.push(plot_norm(x0, x1, y0, y1, [p[0], hi]));
        }
        if s.kind != "bars" && s.kind != "errorbars" && s.marker != "none" {
            let d = if type_of(s.size) == "()" { marker_size() } else { s.size };
            for p in pr.pts {
                if type_of(p) == "()" || !plot_inside(p, x0, x1, y0, y1) { continue; }
                marks.push(plot_marker(s.marker, d, c, s.open).at(p[0], p[1]).anchor(0.5, 0.5));
                samples.push(plot_norm(x0, x1, y0, y1, p));
            }
        }
    }

    // Frame and ticks, on top of the data.
    let fg = plot_fg();
    let frame = plot_opt(opts, "frame", plot_frame());
    if frame == "open" {
        marks.push(path([[x0, y1], [x0, y0], [x1, y0]]).viewbox(x0, y1, x1, y0)
            .stroke(fg).stroke_width(lw).cap("square"));
    } else if frame == "box" {
        marks.push(path([[x0, y0], [x1, y0], [x1, y1], [x0, y1]]).closed().viewbox(x0, y1, x1, y0)
            .stroke(fg).stroke_width(lw));
    } else if frame != "none" {
        throw `plot(): unknown frame "${frame}" (open, box, none)`;
    }
    let dir = tick_direction();
    if dir != "out" && dir != "in" { throw `tick_direction(): must be "out" or "in" (got "${dir}")`; }
    let inward = dir == "in";
    // Outward ticks hang below / left of the axis; inward ones point into the
    // area (anchored at their far end instead).
    let xan = if inward { 1 } else { 0 };
    let yan = if inward { 0 } else { 1 };
    let xminor = xa.minor + plot_minor(xa, xlog);
    let yminor = ya.minor + plot_minor(ya, ylog);
    if xa.marks {
        for t in xa.ticks { marks.push(line(0, 0, 0, tl).stroke(fg).stroke_width(lw).at(t, y0).anchor(0, xan)); }
        for t in xminor { marks.push(line(0, 0, 0, tl * 0.6).stroke(fg).stroke_width(lw * 0.8).at(t, y0).anchor(0, xan)); }
    }
    for t in ya.ticks { marks.push(line(0, 0, tl, 0).stroke(fg).stroke_width(lw).at(x0, t).anchor(yan, 0)); }
    for t in yminor { marks.push(line(0, 0, tl * 0.6, 0).stroke(fg).stroke_width(lw * 0.8).at(x0, t).anchor(yan, 0)); }
    for a in notes {
        let n = plot_annotation_node(a, xlog, ylog, x0, x1, y0, y1);
        if type_of(n) != "()" { extra.push(n); }
    }
    let area = [stack(marks + extra).viewbox(x0, y1, x1, y0)];

    // Legend over the data area.
    let where = plot_opt(opts, "legend", legend_corner());
    if type_of(where) == "bool" { where = if where { legend_corner() } else { "" }; }
    let lstyle = legend_style();
    let lat = ();
    let lanchor = [0, 0];
    if type_of(where) == "map" {
        for k in where.keys() {
            if !["style", "corner", "at", "anchor"].contains(k) {
                throw `plot(): unknown legend option "${k}" (style, corner, at, anchor)`;
            }
        }
        lstyle = plot_opt(where, "style", lstyle);
        lat = plot_opt(where, "at", ());
        lanchor = plot_opt(where, "anchor", lanchor);
        where = plot_opt(where, "corner", legend_corner());
    } else if where == "text" || where == "swatch" {
        lstyle = where;
        where = legend_corner();
    }
    let labelled = false;
    for s in series { if s.label != "" { labelled = true; } }
    if labelled && type_of(lat) != "()" {
        let p = plot_numbers("plot", "legend `at`", lat);
        let la = plot_numbers("plot", "legend `anchor`", lanchor);
        if p.len() != 2 || la.len() != 2 { throw "plot(): legend `at` and `anchor` must be [x, y]"; }
        let px = plot_tf(p[0], xlog);
        let py = plot_tf(p[1], ylog);
        if type_of(px) == "()" || type_of(py) == "()" { throw `plot(): legend at ${lat} is off the log axis`; }
        area[0] = stack(marks + extra + [legend_box(series, lstyle).at(px, py).anchor(la[0], la[1])])
            .viewbox(x0, y1, x1, y0);
    } else if where != "" && labelled {
        if where == "auto" { where = legend_auto_corner(samples); }
        area.push(pin(legend_box(series, lstyle), where, base_size() * 0.03));
    }

    let parts = [stack(area)];

    // Everything else hangs off the data area in the same data coordinates,
    // in a `contain` stack: the area shrinks until all of it fits the plot's
    // box, so each margin is exactly what its labels measure — the widest y
    // tick label, the tallest x tick label, the half labels overhanging the
    // axis ends — and nothing spills past the panel.
    let pad = base_size() * 0.012;                 // tick -> label (cm)
    let tl = if inward { 0.0 } else { tl };        // room the ticks take outside
    let xl = [];
    for i in 0..xa.ticks.len() {
        if xa.ticks[i] >= x0 - 1e-9 && xa.ticks[i] <= x1 + 1e-9 {
            xl.push(plot_text(xa.labels[i]).center().at(xa.ticks[i], 0).anchor(0.5, 0));
        }
    }
    let yl = [];
    for i in 0..ya.ticks.len() {
        if ya.ticks[i] >= y0 - 1e-9 && ya.ticks[i] <= y1 + 1e-9 {
            yl.push(plot_text(ya.labels[i]).at(1, ya.ticks[i]).anchor(1, 0.5));
        }
    }
    // Left band, as tall as the area and right-aligned to the axis: the y tick
    // labels in a viewbox stack spanning the axis (as wide as the widest
    // label), then room for the ticks — after the y label, when it is rotated
    // (`ylabel_rotated()`): a quarter turn swaps its footprint, so in the
    // stretched row it is one line wide and as long as the axis, centred on it.
    let ylabel = plot_opt(opts, "ylabel", "");
    let rotated = ylabel != "" && ylabel_rotated();
    let yband = [];
    if rotated {
        yband.push(plot_text(ylabel).center().rotate(-90));
        yband.push(rect().width(pad * 2));
    }
    yband.push(stack(yl).viewbox(0, y1, 1, y0));
    yband.push(rect().width(tl + pad));
    parts.push(row(yband).stretch().x(x0).anchor(1, 0));
    // Bottom band, as wide as the area: room for the ticks, the x tick labels
    // (a viewbox stack spanning the axis), then the x label centred under it.
    // Categories fill equal slots across the axis, so their labels are cells
    // of a row instead, one per slot: long names wrap to the slot width rather
    // than run into each other.
    let xband = if cats.len() > 0 {
        let g = pad * 0.5;
        let cells = [];
        for c in cats { cells.push(col([plot_text(c).center()]).stretch().pad(g).flex(1)); }
        [rect().height(tl + pad - g), row(cells)]
    } else {
        [rect().height(tl + pad), stack(xl).viewbox(x0, 0, x1, 1)]
    };
    let xlabel = plot_opt(opts, "xlabel", "");
    if xlabel != "" { xband += [rect().height(pad), plot_text(xlabel).center()]; }
    parts.push(col(xband).stretch().y(y0));
    // An unrotated y label sits above the axis, its bottom edge half its own
    // height up: clear of the top tick label's overhang (same text size).
    // A title goes above it all, centred on the data area: a band as wide as
    // the area (no x offset), holding the title and the unrotated y label
    // under it, its bottom a quarter of the band (two lines) or half the
    // title (one) above the axis — clear of the top tick label's overhang
    // as long as the title is no smaller than the tick labels.
    let ptitle = plot_opt(opts, "title", "");
    if type_of(ptitle) != "string" { throw `plot(): title must be a string (got ${type_of(ptitle)})`; }
    let top = [];
    if ptitle != "" {
        top.push(text(ptitle).size(plot_title_size()).color(plot_fg()).center());
    }
    if ylabel != "" && !rotated {
        if top.len() == 0 {
            parts.push(plot_text(ylabel).at(x0, y1).anchor(0, 1.5));
        } else {
            top.push(plot_text(ylabel));
        }
    }
    if top.len() > 0 {
        let k = if top.len() > 1 { 1.25 } else { 1.5 };
        parts.push(col(top).spacing(pad).stretch().y(y1).anchor(0, k));
    }

    // On top: room for a panel label at its inset, so `panels([plot(..), ..])`
    // labels sit clear of everything (`plot_label_room()`).
    let body = stack(parts).viewbox(x0, y1, x1, y0).contain().pad(plot_pad()).flex(1);
    let head = plot_label_room();
    let node = col(if head > 0.0 { [rect().height(head), body] } else { [body] }).stretch();
    // A panel background goes under the whole plot (`plot_bg()`).
    let bg = plot_bg();
    if bg != "" { node = stack([rect().fill(bg), node]); }
    let ar = plot_aspect();
    if ar.to_float() > 0.0 { node = node.aspect(ar); }
    node
}

// Minor tick positions of linear axis `ax` (`minor_ticks()` intervals per
// labelled step), skipping the labelled ticks; none for log/category axes.
fn plot_minor(ax, log) {
    let n = minor_ticks();
    if type_of(n) != "i64" || n < 0 { throw `minor_ticks(): must be an integer >= 0 (got ${n})`; }
    if log || !ax.marks || n < 2 || ax.ticks.len() < 2 { return []; }
    let step = ax.ticks[1] - ax.ticks[0];
    if !(step > 0.0) { return []; }
    let sub = step / n.to_float();
    let out = [];
    let i0 = ((ax.lo - ax.ticks[0]) / sub - 1e-9).ceiling().to_int();
    let i1 = ((ax.hi - ax.ticks[0]) / sub + 1e-9).floor().to_int();
    for i in i0..=i1 {
        if i % n != 0 { out.push(ax.ticks[0] + i.to_float() * sub); }
    }
    out
}

// ---------------------------------------------------------------------------
// Poster (page, header, numbered sections, column grid, frames, tables, …)
// ---------------------------------------------------------------------------
//
// A conference poster in a few lines:
//
//   style("poster-dark");
//   poster(#{ title: "…", authors: [..], affiliations: [..] }, [
//       columns([section("Background", [..]), section("Methods", [..])], [3, 2]),
//       section("Conclusion", bullets(["…", "…"])),
//   ])
//
// `poster` stacks the header (`poster_header`) above `rows` of the body;
// `columns` puts sections side by side with a dashed separator in each
// gutter; `section` is a header bar (auto-numbered "1.", "2.", … in the order
// the sections are built) over padded content. Inside a section: `frame`,
// `note`, `ref`, `bullets`, `table`, `subheading`, `hsep`, plus everything
// else in the framework (plots, panels, grids). Every piece is ordinary core
// nodes, so core setters chain on the results
// (`frame(..).stroke("#7f7f7f").radius(0)`).
//
// The defaults are hooks (and `style` keys) sized from `base_size()`, tuned
// so that `style("poster")` (24 pt body) on an A0 page gives the proportions
// of a typical PowerPoint conference poster: 2.3 cm section bars with
// 53.5 pt titles, 0.54 cm gutters, 1.5 pt dashed separators. The light
// defaults (`style("poster")`) are a navy bar on white; `style("poster-dark")`
// switches to the dark look.

// --- Poster defaults (hooks) ---

// Page preset of `poster(header, body)` ("a0", "a3-landscape", …).
fn poster_size() { style_get("poster_size", "a0") }

// Page background of a poster: a colour or a gradient; defaults to the
// style's `page_bg()` ("" = none, white).
fn poster_bg() { style_get("poster_bg", page_bg()) }

// Clear space between the page edge and the poster content (cm): 0.5 cm at
// 24 pt.
fn poster_margin() { style_get("poster_margin", base_size() * 0.021) }

// Gutter between columns (cm), which a separator runs through: 0.54 cm.
fn poster_gutter() { style_get("poster_gutter", base_size() * 0.0225) }

// Space between rows of sections (and under the header), cm.
fn poster_row_gap() { style_get("poster_row_gap", poster_gutter()) }

// Poster title (pt; 80 pt at 24 pt body) and its font ("" = page font).
fn poster_title_size() { style_get("poster_title_size", base_size() * 3.33) }
fn poster_title_font() { style_get("poster_title_font", "") }

// Author line (pt; 40 pt at 24 pt), affiliation numbers at `sup_scale()` of it.
fn author_size() { style_get("author_size", base_size() * 1.665) }
fn sup_scale() { style_get("sup_scale", 0.667) }

// "before": the Japanese convention, "（Affil. A¹・Affil. B²）○Author¹・…" on one
// line; "below": the authors, then a line of numbered affiliations.
fn affiliation_position() { style_get("affiliation_position", "before") }

// Between authors (and affiliations), and before the presenting author.
fn author_sep() { style_get("author_sep", "・") }
fn presenter_mark() { style_get("presenter_mark", "○") }

// Poster number in the top-right corner (pt, bold).
fn poster_number_size() { style_get("poster_number_size", author_size()) }

// Height of the header logos (cm): 8 cm at 24 pt.
fn poster_logo_height() { style_get("poster_logo_height", base_size() * 0.33) }

// The rule under the header ("" for none) and its width (cm, 1 pt at 24 pt).
fn poster_header_rule() {
    let f = fg();
    style_get("poster_header_rule", if f == "" { "#7f7f7f" } else { f })
}
fn poster_header_rule_width() { style_get("poster_header_rule_width", base_size() * 0.00147) }

// Section numbering template: "1." gives "1. Title" (any `panel_label`
// template: "(1)", "A.", "I-1"); "" for none.
fn section_label_style() { style_get("section_label_style", "1.") }

// Section bar: title size (pt; 53.5 pt at 24 pt), colour, font ("" = page font).
fn section_title_size() { style_get("section_title_size", base_size() * 2.23) }
fn section_title_color() { style_get("section_title_color", "#ffffff") }
fn section_title_font() { style_get("section_title_font", "") }

// Bar height (cm): 2.3 cm for the 53.5 pt title.
fn section_bar_height() { style_get("section_bar_height", section_title_size() * 0.043) }

// Bar fill (colour or gradient), border colour ("" for none), border width
// (cm, 0.25 pt at 24 pt) and corner radius (cm; square by default).
fn section_bar_fill() {
    style_get("section_bar_fill", linear(90, [[0, "#4a6597"], [1, "#1f3359"]]))
}
fn section_bar_border() { style_get("section_bar_border", "") }
fn section_bar_border_width() { style_get("section_bar_border_width", base_size() * 0.000367) }
fn section_bar_radius() { style_get("section_bar_radius", 0) }

// Padding around a section's content (cm) and the gap between its items.
fn section_pad() { style_get("section_pad", base_size() * 0.0125) }
fn section_gap() { style_get("section_gap", base_size() * 0.02) }

// Dashed separators in the column gutters (`separators()`) and between rows
// of sections (`row_separators()`, off by default).
fn separators() { style_get("separators", true) }
fn row_separators() { style_get("row_separators", false) }

// Separator colour, width (cm; 1.5 pt at 24 pt) and dash (12 pt on, 4.5 off).
fn separator_color() {
    let f = fg();
    style_get("separator_color", if f == "" { "#7f7f7f" } else { f })
}
fn separator_width() { style_get("separator_width", base_size() * 0.0022) }
fn separator_dash() {
    style_get("separator_dash", [separator_width() * 8.0, separator_width() * 3.0])
}

// Where a column separator starts below the top of its row (cm): the middle
// of the section bars, so it hangs from them.
fn separator_inset() { style_get("separator_inset", section_bar_height() * 0.5) }

// `frame`: border colour, width (cm), corner radius (cm), padding (cm) and
// the dash of `dashed_frame`.
fn frame_color() {
    let f = fg();
    style_get("frame_color", if f == "" { "#7f7f7f" } else { f })
}
fn frame_width() { style_get("frame_width", separator_width()) }
fn frame_radius() { style_get("frame_radius", base_size() * 0.011) }
fn frame_pad() { style_get("frame_pad", base_size() * 0.015) }
fn frame_dash() { style_get("frame_dash", [frame_width() * 4.0, frame_width() * 3.0]) }

// `note` (the highlighted take-home sentence): colour, size (pt), font.
fn note_color() { style_get("note_color", "#b85c00") }
fn note_size() { style_get("note_size", base_size()) }
fn note_font() { style_get("note_font", "") }

// `ref` (literature references): size (pt; 20 pt at 24 pt) and colour
// ("" = `fg()`).
fn ref_size() { style_get("ref_size", base_size() * 0.835) }
fn ref_color() { style_get("ref_color", fg()) }

// `bullets`: mark, text size (pt), hanging indent (cm; 1.58 cm at 36 pt) and
// gap between items (cm).
fn bullet_mark() { style_get("bullet_mark", "•") }
fn bullet_size() { style_get("bullet_size", base_size()) }
fn bullet_indent() { style_get("bullet_indent", bullet_size() * 0.044) }
fn bullet_gap() { style_get("bullet_gap", bullet_size() * 0.004) }

// `subheading`: the mark before it ("" for none) and its size (pt; 32 pt at 24).
fn subheading_mark() { style_get("subheading_mark", "■") }
fn subheading_size() { style_get("subheading_size", base_size() * 1.335) }

// `table`: text size (pt), rule colour, rule width (cm; 1 pt at 24 pt), the
// header rule (3x), rules ("grid", "rows": horizontal only, "booktabs": top,
// header and bottom, "none"), cell padding (cm), cell fill and header fill
// ("" for none).
fn table_size() { style_get("table_size", base_size()) }
fn table_rule() {
    let f = fg();
    style_get("table_rule", if f == "" { "#000000" } else { f })
}
fn table_rule_width() { style_get("table_rule_width", base_size() * 0.00147) }
fn table_header_rule_width() { style_get("table_header_rule_width", table_rule_width() * 3.0) }
fn table_rules() { style_get("table_rules", "grid") }
fn table_pad() { style_get("table_pad", base_size() * 0.011) }
fn table_fill() { style_get("table_fill", "") }
fn table_header_fill() { style_get("table_header_fill", "") }

// --- helpers ---

// Is paint `p` (a colour string or a gradient) set, i.e. not ""?
fn poster_has_paint(p) { type_of(p) != "string" || p != "" }

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

// `node` sized along a container's main axis by `w`: a number is a flex
// weight, "12cm" an absolute length, "auto" its natural size.
fn poster_sized(who, node, w, vertical) {
    let t = type_of(w);
    if t == "i64" || t == "f64" {
        if !(w > 0) { throw `${who}(): a flex weight must be positive (got ${w})`; }
        return node.flex(w);
    }
    if w == "auto" { return node; }
    let v = poster_cm(who, w);
    if type_of(v) == "()" {
        throw `${who}(): sizes are flex weights (2), lengths ("12cm") or "auto" (got ${w})`;
    }
    if vertical { node.height(v) } else { node.width(v) }
}

// Sizes for `n` items: `ws` checked against `n`, or n ones.
fn poster_weights(who, n, ws) {
    if type_of(ws) == "()" {
        let out = [];
        for i in 0..n { out.push(1); }
        return out;
    }
    if type_of(ws) != "array" { throw `${who}(): sizes must be an array (got ${type_of(ws)})`; }
    if ws.len() != n { throw `${who}(): ${n} items but ${ws.len()} sizes`; }
    ws
}

// --- separators ---

// A dashed vertical separator for a gutter `w` cm wide (default
// `poster_gutter()`): centred in it, from `separator_inset()` below the top
// to the bottom.
fn vsep() { vsep(poster_gutter()) }
fn vsep(w) {
    let l = line(0.5, 0, 0.5, 1).viewbox(0, 0, 1, 1).stroke(separator_color())
        .stroke_width(separator_width()).dash(separator_dash()).flex(1);
    let inset = separator_inset();
    col(if inset > 0 { [rect().height(inset), l] } else { [l] }).stretch().width(w)
}

// A dashed horizontal separator across its parent, centred in a band
// `h` cm tall (default `section_gap()`).
fn hsep() { hsep(section_gap()) }
fn hsep(h) {
    line(0, 0.5, 1, 0.5).viewbox(0, 0, 1, 1).stroke(separator_color())
        .stroke_width(separator_width()).dash(separator_dash()).height(h)
}

// --- grid ---

// Sections (or any nodes) side by side, sharing the width equally or by
// `widths` (flex weights, "12cm" lengths, "auto"), `poster_gutter()` apart
// with a dashed `vsep` in each gutter while `separators()` is on.
fn columns(items) { columns(items, ()) }
fn columns(items, widths) {
    if type_of(items) != "array" {
        throw `columns(): expected an array of nodes (got ${type_of(items)})`;
    }
    let ws = poster_weights("columns", items.len(), widths);
    let g = poster_gutter();
    let sep = separators();
    let out = [];
    for i in 0..items.len() {
        if i > 0 && sep { out.push(vsep(g)); }
        out.push(poster_sized("columns", as_node(items[i]), ws[i], false));
    }
    row(out).spacing(if sep { 0 } else { g }).stretch()
}

// Rows of sections (or `columns`) down the page, sharing the height equally
// or by `heights` (flex weights, "12cm", "auto"), `poster_row_gap()` apart (a
// dashed `hsep` in each gap while `row_separators()` is on).
fn rows(items) { rows(items, ()) }
fn rows(items, heights) {
    if type_of(items) != "array" {
        throw `rows(): expected an array of nodes (got ${type_of(items)})`;
    }
    let hs = poster_weights("rows", items.len(), heights);
    let g = poster_row_gap();
    let sep = row_separators();
    let out = [];
    for i in 0..items.len() {
        if i > 0 && sep { out.push(hsep(g)); }
        out.push(poster_sized("rows", as_node(items[i]), hs[i], true));
    }
    col(out).spacing(if sep { 0 } else { g }).stretch()
}

// --- sections ---

// A section bar: `s` centred (both ways) on a `section_bar_height()` bar
// filled with `section_bar_fill()` and bordered in `section_bar_border()`.
// The title is anchored on the bar's centre in a unit viewbox, so it stays
// centred and keeps its size even when its line box (a CJK font's tall
// ascent + descent) is taller than the bar.
fn section_bar(s) {
    let t = text(s).size(section_title_size()).color(section_title_color()).center();
    let f = section_title_font();
    if f != "" { t = t.font(f); }
    let bar = stack([t.at(0.5, 0.5).anchor(0.5, 0.5)]).viewbox(0, 0, 1, 1)
        .height(section_bar_height());
    let fill = section_bar_fill();
    if poster_has_paint(fill) { bar = bar.fill(fill); }
    let b = section_bar_border();
    if b != "" { bar = bar.stroke(b).stroke_width(section_bar_border_width()); }
    let r = section_bar_radius();
    if r > 0 { bar = bar.radius(r); }
    bar
}

// The next section number (1, 2, … in the order sections are built), or `n`
// when given (the count continues from it). Kept in the style registry, which
// is fresh for every evaluation.
fn section_number(n) {
    if type_of(n) == "()" { n = style_get("__section_no", 0) + 1; }
    style_set(#{ "__section_no": n });
    n
}

// `section(title, content)`: a numbered bar ("1. Title", `section_label_style()`)
// over `content` — an array of items `section_gap()` apart, or one node —
// padded `section_pad()`; the content takes the section's spare height.
// `section(number, title, content)` sets the number: an integer (the count
// continues from it) or a string used as is ("" for none).
fn section(title, content) { section_make(section_number(()), title, content) }
fn section(number, title, content) {
    if type_of(number) == "i64" { return section_make(section_number(number), title, content); }
    if type_of(number) != "string" {
        throw `section(): the number must be an integer or a string (got ${type_of(number)})`;
    }
    section_make(number, title, content)
}

fn section_make(n, title, content) {
    if type_of(title) != "string" {
        throw `section(): the title must be a string (got ${type_of(title)})`;
    }
    let label = if type_of(n) == "string" { n } else {
        let st = section_label_style();
        if st == "" { "" } else { panel_label(st, n - 1) }
    };
    let s = if label == "" { title } else { `${label} ${title}` };
    let items = if type_of(content) == "array" { nodes(content) } else { [as_node(content)] };
    let body = col(items).spacing(section_gap()).stretch().pad(section_pad()).flex(1);
    col([section_bar(s), body]).stretch()
}

// --- content blocks ---

// A thin rounded border box (`frame_color()`, `frame_width()`,
// `frame_radius()`) around `content` (an array `section_gap()` apart, or one
// node), padded `frame_pad()`. Square / grey / heavy variants are core
// setters: `frame(..).radius(0).stroke("#7f7f7f").stroke_width(0.1)`.
fn frame(content) {
    let items = if type_of(content) == "array" { nodes(content) } else { [as_node(content)] };
    col(items).spacing(section_gap()).stretch().pad(frame_pad())
        .stroke(frame_color()).stroke_width(frame_width()).radius(frame_radius())
}

// `frame` with a dashed border (`frame_dash()`).
fn dashed_frame(content) { frame(content).dash(frame_dash()) }

// The highlighted take-home sentence of a section: centred, `note_color()`.
fn note(s) {
    let t = text(s).size(note_size()).color(note_color()).center();
    let f = note_font();
    if f == "" { t } else { t.font(f) }
}

// A literature reference line: small (`ref_size()`), centred.
fn ref(s) {
    let t = text(s).size(ref_size()).center();
    let c = ref_color();
    if c == "" { t } else { t.color(c) }
}

// A small heading inside a section: "■ " + `s` at `subheading_size()`.
fn subheading(s) {
    let m = subheading_mark();
    fg_text(if m == "" { s } else { `${m} ${s}` }).size(subheading_size())
}

// A bulleted list with a hanging indent: each item (a string or a node) in a
// cell `bullet_indent()` right of its `bullet_mark()`, so wrapped lines align
// with the first. An array item is a nested list under the previous item.
fn bullets(items) {
    if type_of(items) != "array" {
        throw `bullets(): expected an array of items (got ${type_of(items)})`;
    }
    let ind = bullet_indent();
    let out = [];
    for x in items {
        if type_of(x) == "array" {
            out.push(row([rect().width(ind), bullets(x).flex(1)]).stretch());
            continue;
        }
        let item = if type_of(x) == "string" { fg_text(x).size(bullet_size()) } else { x };
        let mark = fg_text(bullet_mark()).size(bullet_size()).width(ind);
        out.push(row([mark, item.flex(1)]).cross("start"));
    }
    col(out).spacing(bullet_gap()).stretch()
}

// `table(rows)` / `table(rows, opts)`: `rows` is an array of equally long
// arrays of cells (strings, numbers or nodes); the first row is the header
// (bold, above a `table_header_rule_width()` rule). Options:
//   header  false: no header row (default true)
//   widths  per column: flex weights, "4cm" lengths or "auto" (default equal)
//   align   "left" (default), "center", "right", or one per column
//   rules   "grid", "rows", "booktabs" or "none" (default `table_rules()`)
// The rules are filled rects between the cells (`table_rule()`,
// `table_rule_width()`), so the grid is exact at any size; cells are padded
// `table_pad()` and filled `table_fill()` / `table_header_fill()`.
fn table(rows) { table(rows, #{}) }
fn table(rows, opts) {
    if type_of(opts) != "map" { throw `table(): options must be a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        if !["header", "widths", "align", "rules"].contains(k) {
            throw `table(): unknown option "${k}" (header, widths, align, rules)`;
        }
    }
    if type_of(rows) != "array" || rows.len() == 0 { throw "table(): expected a non-empty array of rows"; }
    let nc = 0;
    for r in rows {
        if type_of(r) != "array" { throw `table(): each row must be an array of cells (got ${type_of(r)})`; }
        if nc == 0 { nc = r.len(); }
        if r.len() != nc || nc == 0 { throw `table(): rows differ in length (${nc} vs ${r.len()})`; }
    }
    let header = plot_opt(opts, "header", true);
    let ws = poster_weights("table", nc, plot_opt(opts, "widths", ()));
    let al = plot_opt(opts, "align", "left");
    if type_of(al) == "string" {
        let a = [];
        for i in 0..nc { a.push(al); }
        al = a;
    }
    if type_of(al) != "array" || al.len() != nc {
        throw "table(): align must be a string or one per column";
    }
    let rules = plot_opt(opts, "rules", table_rules());
    if !["grid", "rows", "booktabs", "none"].contains(rules) {
        throw `table(): unknown rules "${rules}" (grid, rows, booktabs, none)`;
    }
    let rc = table_rule();
    let rw = table_rule_width();
    let vertical = rules == "grid";
    let inner = rules == "grid" || rules == "rows";
    let outer = rules != "none";
    let parts = [];
    if outer { parts.push(rect().height(rw).fill(rc)); }
    for i in 0..rows.len() {
        let head = header && i == 0;
        if i == 1 && header && outer {
            parts.push(rect().height(table_header_rule_width()).fill(rc));
        } else if i > 0 && inner {
            parts.push(rect().height(rw).fill(rc));
        }
        let cells = [];
        if vertical { cells.push(rect().width(rw).fill(rc)); }
        for j in 0..nc {
            cells.push(poster_sized("table", table_cell(rows[i][j], head, al[j]), ws[j], false));
            if vertical { cells.push(rect().width(rw).fill(rc)); }
        }
        parts.push(row(cells).stretch());
    }
    if outer { parts.push(rect().height(rw).fill(rc)); }
    let t = col(parts).stretch();
    // All widths absolute: the table is exactly as wide as its columns.
    let total = 0.0;
    for w in ws {
        let v = poster_cm("table", w);
        if type_of(v) == "()" { return t; }
        total += v;
    }
    if vertical { total += rw * (nc + 1).to_float(); }
    t.width(total)
}

// One table cell: `x` (a string or number becomes text, bold in the header)
// padded `table_pad()`, vertically centred, aligned `align`, filled
// `table_header_fill()` (header) or `table_fill()`.
fn table_cell(x, head, align) {
    let t = type_of(x);
    let n = if t == "string" || t == "i64" || t == "f64" {
        let s = fg_text(`${x}`).size(table_size()).align(align);
        if head { s.bold() } else { s }
    } else { x };
    let c = col([n]).pad(table_pad()).main("center").stretch();
    let f = if head { table_header_fill() } else { "" };
    if !poster_has_paint(f) { f = table_fill(); }
    if poster_has_paint(f) { c.fill(f) } else { c }
}

// --- header ---

// The author line as runs `[text, superscript?]` (adjacent plain runs merged):
// `authors` are names or maps #{ name, aff: [1, 2] (or 1), presenter: true };
// affiliation numbers are only shown with more than one affiliation.
fn author_runs(authors, affiliations) {
    let runs = [];
    let numbered = affiliations.len() > 1;
    let i = 0;
    for a in authors {
        if i > 0 { runs = runs_push(runs, author_sep(), false); }
        i += 1;
        if type_of(a) == "string" {
            runs = runs_push(runs, a, false);
            continue;
        }
        if type_of(a) != "map" || !a.contains("name") {
            throw `poster_header(): an author is a name or #{ name, aff, presenter } (got ${type_of(a)})`;
        }
        if plot_opt(a, "presenter", false) { runs = runs_push(runs, presenter_mark(), false); }
        runs = runs_push(runs, a.name, false);
        let aff = plot_opt(a, "aff", []);
        if type_of(aff) != "array" { aff = [aff]; }
        if numbered && aff.len() > 0 {
            let s = "";
            for k in aff {
                if s != "" { s += ","; }
                s += `${k}`;
            }
            runs = runs_push(runs, s, true);
        }
    }
    runs
}

// The affiliations as runs: "Affil. A¹・Affil. B²" (unnumbered when single).
fn affiliation_runs(affiliations) {
    let runs = [];
    let n = affiliations.len();
    for i in 0..n {
        if i > 0 { runs = runs_push(runs, author_sep(), false); }
        runs = runs_push(runs, affiliations[i], false);
        if n > 1 { runs = runs_push(runs, `${i + 1}`, true); }
    }
    runs
}

// `runs` with run [s, sup] appended (merged into a preceding plain run).
fn runs_push(runs, s, sup) {
    if s == "" { return runs; }
    let n = runs.len();
    if n > 0 && !sup && !runs[n - 1][1] {
        runs[n - 1][0] += s;
    } else {
        runs.push([s, sup]);
    }
    runs
}

// A line of text runs at `size` pt, centred: plain runs at full size,
// superscripts at `sup_scale()` of it, top-aligned in a row so they ride up
// like superscripts. (The one place to switch to rich-text spans.)
fn runs_line(runs, size) {
    let parts = [];
    for r in runs {
        parts.push(fg_text(r[0]).size(if r[1] { size * sup_scale() } else { size }));
    }
    row(parts).cross("start").main("center")
}

// The author line(s) of a poster header. `affiliation_position()` "before":
// "（Affil. A¹・Affil. B²）○Author¹・Author²" on one line; "below": the
// authors, and the numbered affiliations a size smaller under them.
fn author_line(authors, affiliations) {
    if type_of(authors) != "array" { authors = [authors]; }
    if type_of(affiliations) != "array" { affiliations = [affiliations]; }
    let size = author_size();
    let who = author_runs(authors, affiliations);
    if affiliations.len() == 0 { return runs_line(who, size); }
    let where = affiliation_position();
    if where == "before" {
        // "（" affiliations "）" then the authors.
        let runs = runs_push([], "（", false);
        for r in affiliation_runs(affiliations) { runs = runs_push(runs, r[0], r[1]); }
        runs = runs_push(runs, "）", false);
        for r in who { runs = runs_push(runs, r[0], r[1]); }
        return runs_line(runs, size);
    }
    if where != "below" {
        throw `affiliation_position(): must be "before" or "below" (got "${where}")`;
    }
    col([runs_line(who, size), runs_line(affiliation_runs(affiliations), size * 0.8)])
        .spacing(size * 0.005).stretch()
}

// The poster header; `opts` is a map of
//   title         the title (a string or a node): `poster_title_size()`, bold
//   authors       names or #{ name, aff, presenter } (see `author_line`)
//   affiliations  affiliation names, numbered from 1
//   logos         images (or paths) left of the title, `poster_logo_height()` tall
//   number        the poster number, top-right
// The title and author line are centred (both ways) between the logos and the
// number; a `poster_header_rule()` runs under it all.
fn poster_header(opts) {
    if type_of(opts) != "map" { throw `poster_header(): expected a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        if !["title", "authors", "affiliations", "logos", "number"].contains(k) {
            throw `poster_header(): unknown key "${k}" (title, authors, affiliations, logos, number)`;
        }
    }
    let main = [];
    let title = plot_opt(opts, "title", "");
    if type_of(title) != "string" {
        main.push(title);
    } else if title != "" {
        let t = fg_text(title).size(poster_title_size()).bold().center();
        let f = poster_title_font();
        main.push(if f == "" { t } else { t.font(f) });
    }
    let authors = plot_opt(opts, "authors", []);
    if type_of(authors) != "array" || authors.len() > 0 {
        main.push(author_line(authors, plot_opt(opts, "affiliations", [])));
    }
    let parts = [];
    for l in plot_opt(opts, "logos", []) {
        let n = if type_of(l) == "string" { image(l) } else { l };
        parts.push(n.height(poster_logo_height()));
    }
    parts.push(col(main).main("center").stretch().spacing(author_size() * 0.008).flex(1));
    // The number in its own column at the top right, so a long title wraps
    // before it instead of running under it.
    let num = plot_opt(opts, "number", "");
    if num != "" { parts.push(col([fg_text(num).size(poster_number_size()).bold()])); }
    let head = row(parts).stretch().spacing(poster_gutter());
    let rule = poster_header_rule();
    if rule == "" { return head; }
    col([head, rect().height(poster_header_rule_width()).fill(rule)])
        .spacing(poster_row_gap() * 0.5).stretch()
}

// --- the page ---

// `poster(header, body)` / `poster(preset, header, body)`: a `poster_size()`
// (or `preset`) page on `poster_bg()`; the header — a node, a map for
// `poster_header`, or () for none — above the body — an array of rows for
// `rows`, or one node — `poster_margin()` in from the edges.
fn poster(header, body) { poster(poster_size(), header, body) }
fn poster(preset, header, body) {
    if type_of(header) == "map" { header = poster_header(header); }
    let content = if type_of(body) == "array" { rows(body) } else { as_node(body) };
    let parts = if type_of(header) == "()" { [content.flex(1)] } else { [header, content.flex(1)] };
    let root = col(parts).spacing(poster_row_gap()).pad(poster_margin()).stretch();
    let d = page(preset, root);
    let bg = poster_bg();
    if poster_has_paint(bg) { d.background(bg) } else { d }
}
