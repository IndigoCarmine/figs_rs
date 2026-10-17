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

// --- Overlays, schemes, side notes (see "Overlays" and "Schemes" below) ---

// Distance from the panel edge to an `overlay` position (cm): the label inset.
fn overlay_inset() { style_get("overlay_inset", label_inset()) }

// Between several nodes at one `overlay` position (cm): 0.08 cm at 8 pt.
fn overlay_gap() { style_get("overlay_gap", base_size() * 0.01) }

// Between the items and arrows of a `scheme` (cm): the default gap.
fn scheme_gap() { style_get("scheme_gap", gap()) }

// Length of a scheme arrow (cm): 1.6 cm at the default 8 pt, 4.8 cm on a
// 24 pt poster. `arrow_step(#{ length: "fill" })` stretches one instead.
fn scheme_arrow_length() { style_get("scheme_arrow_length", base_size() * 0.2) }

// Stroke width of scheme arrows (cm): the annotation stroke.
fn scheme_arrow_width() { style_get("scheme_arrow_width", annotation_width()) }

// Scheme text (`step` captions, arrow labels): size (pt) and its gap to the
// node or arrow it belongs to (cm).
fn scheme_label_size() { style_get("scheme_label_size", base_size()) }
fn scheme_label_gap() { style_get("scheme_label_gap", caption_gap()) }

// `side(main, note)`: the note's share of the width (a fraction; "5cm" or
// "auto" also work) and the gap between the two (cm).
fn side_width() { style_get("side_width", 0.4) }
fn side_gap() { style_get("side_gap", gap()) }

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
        "scheme_arrow_width", "scheme_label_size", "scheme_label_gap", "side_width", "side_gap",
        // plots
        "plot_line_width", "series_width", "palette", "tick_length", "tick_direction",
        "minor_ticks", "max_ticks", "marker_size", "marker_open", "marker_stroke_width",
        "marker_ring", "plot_frame", "legend_corner", "legend_style", "bar_width",
        "plot_aspect", "plot_label_room", "plot_pad", "ylabel_rotated", "plot_title_size",
        "plot_frame_box", "plot_font", "tick_label_size", "tick_label_font", "tick_label_pad",
        "axis_label_size", "axis_label_font", "axis_label_pad", "legend_size", "legend_font",
        "plot_title_font", "xtick_label_pad", "ytick_label_pad", "tick_label_pad_mode",
        "xlabel_pad", "ylabel_pad", "xlabel_offset", "ylabel_offset",
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
        "poster_symbol_font", "block_gap", "block_title_gap",
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

// Empty band at the top of a *labelled* plot that the panel label fits in
// (cm): the label inset plus one label line. Only `labeled` / `panels` add it
// (inside the plot, under its aspect); a plot on its own reserves nothing.
// 0 lets labels overlap the plot's top-left corner.
fn plot_label_room() {
    style_get("plot_label_room", label_inset() + base_size() * 1.25 * 1.3 * 2.54 / 72.0)
}

// Clear space between a plot's outermost marks (tick labels, axis labels)
// and its box (cm), so labels never touch the panel edge.
fn plot_pad() { style_get("plot_pad", base_size() * 0.01) }

// Width of an `inset(..)` as a fraction of the space it sits in (the data
// frame, or the panel, less `inset_pad()`); its height follows the node's
// aspect (a plot's `plot_aspect()`, an image's pixels). A `width` option (cm)
// overrides it.
fn inset_size() { style_get("inset_size", 0.4) }

// Where `inset(sub)` sits in a plot: "auto" (the emptiest corner the legend
// left free) or a corner name.
fn inset_corner() { style_get("inset_corner", "auto") }

// Gap between a corner inset and the data frame (cm), as for the legend.
fn inset_pad() { style_get("inset_pad", base_size() * 0.03) }

// Text size inside `inset_plot(..)` (pt): ticks, axis labels, legend, title.
fn inset_text_size() { style_get("inset_text_size", base_size() * 0.75) }

// At most this many labelled ticks per axis of an `inset_plot(..)`.
fn inset_max_ticks() { style_get("inset_max_ticks", 4) }

// Fill under a whole `inset_plot(..)` (labels included), so the host's data
// doesn't show through: the plot / page background, else white.
fn inset_bg() {
    style_get("inset_bg", if plot_bg() != "" { plot_bg() } else if page_bg() != "" { page_bg() } else { "#ffffff" })
}

// The y axis label turned 90° (reading upwards) left of the y tick labels and
// centred on the axis (true), or horizontal above the axis (false).
fn ylabel_rotated() { style_get("ylabel_rotated", true) }

// Size (pt) of a plot's `title` (centred above the data area): the base size,
// like the axis labels.
fn plot_title_size() { style_get("plot_title_size", base_size()) }

// What a plot's box is: false (default) — everything, the data area shrinking
// until tick labels, axis labels and title fit inside; true — exactly the data
// frame, with tick labels, axis labels and title hanging outside it, so a plot
// is placed by its axes (`.at(x, y).width(w).height(h)` puts the frame there).
// The plot option `frame_box` overrides it per plot.
fn plot_frame_box() { style_get("plot_frame_box", false) }

// Font of all plot text: tick and axis labels, legend, title, `plot_text`.
// "" (default) uses the page font. A family name or a fallback list.
fn plot_font() { style_get("plot_font", "") }

// Tick labels: size (pt), font, and the gap between a tick's end (or the axis,
// for inward ticks) and its label (cm, 0.034 cm ~ 1 pt at the default 8 pt).
fn tick_label_size() { style_get("tick_label_size", base_size()) }
fn tick_label_font() { style_get("tick_label_font", plot_font()) }
fn tick_label_pad() { style_get("tick_label_pad", base_size() * 0.012) }

// The same gap per axis (cm; plot options of the same names override them):
// x tick labels below the axis, y tick labels left of it. Both follow
// `tick_label_pad()`. A negative pad moves the label box onto the tick (the
// tick room plus the pad can't be negative: the box never crosses the axis).
fn xtick_label_pad() { style_get("xtick_label_pad", tick_label_pad()) }
fn ytick_label_pad() { style_get("ytick_label_pad", tick_label_pad()) }

// What `xtick_label_pad` reaches to: "box" (default) — the top of the x tick
// labels' line box (half-leading and the font's ascent above the digits
// included); "baseline" — their first baseline, measured on the shaped text
// with the core baseline anchors, so the pad is font-exact: the digits' ink
// starts their height above it (~0.716 em in Arial / Helvetica), i.e. ink
// gap = pad − 0.716 × size. The y tick labels are always measured to the
// right edge of their box (the end of the label's advance) and centred on
// their tick by box; category names (which wrap in their slots) by box.
fn tick_label_pad_mode() { style_get("tick_label_pad_mode", "box") }

// Axis labels (`xlabel` / `ylabel`): size (pt), font, and their gap to the
// tick labels (cm; the turned y label sits twice that away).
fn axis_label_size() { style_get("axis_label_size", base_size()) }
fn axis_label_font() { style_get("axis_label_font", plot_font()) }
fn axis_label_pad() { style_get("axis_label_pad", base_size() * 0.012) }

// The axis-label gaps per axis, box to box (cm, >= 0): the x label below
// the x tick labels (default `axis_label_pad()`), the turned y label left of
// the y tick labels (default twice that). Negative pads are an error — to
// bring a label closer than its box allows, place it by its baseline with
// the offsets below.
fn xlabel_pad() { style_get("xlabel_pad", axis_label_pad()) }
fn ylabel_pad() { style_get("ylabel_pad", axis_label_pad() * 2) }

// Exact axis-label placement (cm > 0, or () — the default — to follow the
// tick labels by the pads above): the x label's first baseline this far
// below the x axis; the turned y label's baseline this far left of the y
// axis (an unrotated y label: its baseline this far above the top of the
// axis, left-aligned to it). Independent of the tick labels, so nothing
// moves out of the way: a small offset overlaps them.
fn xlabel_offset() { style_get("xlabel_offset", ()) }
fn ylabel_offset() { style_get("ylabel_offset", ()) }

// Legend labels: size (pt) and font.
fn legend_size() { style_get("legend_size", base_size()) }
fn legend_font() { style_get("legend_font", plot_font()) }

// Font of a plot's `title` (its size is `plot_title_size()`).
fn plot_title_font() { style_get("plot_title_font", plot_font()) }

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

// Rich text from inline markup: `*italic*`, `**bold**`, `__underline__`,
// `~~strike~~`, `H_2O` / `ΔG_{300K}` (subscript), `Debye^†` / `x^{2}`
// (superscript), `{#f0a030 bold|10 μM}` (style group), `\*` (literal). The
// result is an ordinary core text node of spans (`text(markup(s))`), so every
// text setter chains: `rich("ΔG_{300K}").size(base_size())`. The semantic
// helpers take markup through it too: `body(markup("H_2O"))`.
fn rich(s) { text(markup(s)) }

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

// A block arrow from (x1, y1) to its tip (x2, y2): a shaft `shaft_w` wide,
// then a triangular head `head_w` across and `head_len` long. One closed
// `path` (7 corners) filled with `accent()` — chain `.fill(linear(0, [...]))`
// for a gradient, `.stroke(..)` for an outline, `.shadow(..)` for depth.
// `opts`: `both: true` puts a head on (x1, y1) too (10 corners, ⇔).
fn block_arrow(x1, y1, x2, y2, shaft_w, head_w, head_len) {
    block_arrow(x1, y1, x2, y2, shaft_w, head_w, head_len, #{})
}

fn block_arrow(x1, y1, x2, y2, shaft_w, head_w, head_len, opts) {
    shape_opts("block_arrow", opts, ["both"]);
    let both = plot_opt(opts, "both", false);
    let f = chord_frame("block_arrow", x1, y1, x2, y2);
    let nx = f[0];
    let ny = f[1];
    let len = f[2];
    let hl = head_len.to_float();
    let room = if both { len / 2.0 } else { len };
    if hl > room { hl = room; }
    // Unit along the chord: (-ny, nx); bases of the heads on the chord.
    let bx = x2 - (-ny) * hl;
    let by = y2 - nx * hl;
    let ax = x1 + (-ny) * hl;
    let ay = y1 + nx * hl;
    let s = shaft_w / 2.0;
    let h = head_w / 2.0;
    let pts = if both {
        [[x1, y1], [ax + nx * h, ay + ny * h], [ax + nx * s, ay + ny * s]]
    } else {
        [[x1 + nx * s, y1 + ny * s]]
    };
    pts += [
        [bx + nx * s, by + ny * s], [bx + nx * h, by + ny * h],
        [x2, y2],
        [bx - nx * h, by - ny * h], [bx - nx * s, by - ny * s],
    ];
    pts += if both {
        [[ax - nx * s, ay - ny * s], [ax - nx * h, ay - ny * h]]
    } else {
        [[x1 - nx * s, y1 - ny * s]]
    };
    path(pts).closed().fill(accent())
}

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

// The equilibrium arrows ⇌ between (x1, y1) and (x2, y2), `gap` apart, the
// upper one pointing at (x2, y2), the lower one back at (x1, y1), barbs on
// the outside; in `fg()` (black by default), `annotation_width()` wide.
// `equilibrium(x1, y1, x2, y2, gap)` or `(.., opts)`, a map of
//   gap         between the two arrows (default 4 × the width)
//   width       stroke width (default `annotation_width()`)
//   head        the head on each tip: "barb" (a half head on the outer
//               side, the core `head_style("barb_left")` — the harpoon),
//               "triangle" (a full filled head) or "none"; default "barb",
//               or "none" with barb "hook"
//   head_size   the head's length along the shaft (core default 5 × width)
//   head_width  its width across, a length or "3x" of the width (core
//               default 0.8 × head_size)
//   barb        "straight" (default): no barb beyond the head; "hook": a
//               curved barb leaving the shaft and bending outwards (the
//               PowerPoint ⇌), 1.5 × the width with round caps — on plain
//               shafts by default, or behind a `head`
//   hook_size   the hook's length back along the shaft from where it starts
//               (default 0.4 × the chord, at most 20 × width)
//   hook_width  how far out it reaches, a length or "3x" of the width
//               (default 0.4 × hook_size)
//   hook_start  how far behind the tip it leaves the shaft (default 0, or
//               0.6 × the head's length behind a head)
// Deprecated: with no head (barb "hook" and head "none"), head_size /
// head_width size the hook, as before hook_size / hook_width existed.
// A `stack` of the arrows (and hooks): core setters chain on it.
fn equilibrium(x1, y1, x2, y2) { equilibrium(x1, y1, x2, y2, #{}) }

fn equilibrium(x1, y1, x2, y2, opts) {
    if type_of(opts) != "map" { opts = #{ gap: opts }; }
    shape_opts("equilibrium", opts, ["gap", "width", "barb", "head", "head_size", "head_width",
                                     "hook_size", "hook_width", "hook_start"]);
    let w = plot_opt(opts, "width", annotation_width()).to_float();
    let gap = plot_opt(opts, "gap", w * 4.0);
    let barb = plot_opt(opts, "barb", "straight");
    if barb != "straight" && barb != "hook" {
        throw `equilibrium(): unknown barb "${barb}" (straight, hook)`;
    }
    let head = plot_opt(opts, "head", if barb == "hook" { "none" } else { "barb" });
    if !["none", "triangle", "barb"].contains(head) {
        throw `equilibrium(): unknown head "${head}" (none, triangle, barb)`;
    }
    let f = chord_frame("equilibrium", x1, y1, x2, y2);
    let nx = f[0];
    let ny = f[1];
    let ox = nx * gap / 2.0;
    let oy = ny * gap / 2.0;
    let c = fg();
    let parts = [];
    // The two shafts: the upper one to (x2, y2), the lower one back.
    for s in [[x1 + ox, y1 + oy, x2 + ox, y2 + oy], [x2 - ox, y2 - oy, x1 - ox, y1 - oy]] {
        if head == "none" {
            parts.push(line(s[0], s[1], s[2], s[3]).stroke_width(w));
            continue;
        }
        let a = arrow(s[0], s[1], s[2], s[3]).stroke_width(w)
            .head_style(if head == "barb" { "barb_left" } else { "triangle" });
        if opts.contains("head_size") { a = a.head_size(opts.head_size); }
        if opts.contains("head_width") { a = a.head_width(opts.head_width); }
        parts.push(a);
    }
    if barb == "hook" {
        // Without a head, head_size / head_width are the hook's (deprecated).
        let alias = head == "none";
        let hs = if opts.contains("hook_size") { opts.hook_size }
                 else if alias && opts.contains("head_size") { opts.head_size }
                 else if f[2] * 0.4 < w * 20.0 { f[2] * 0.4 } else { w * 20.0 };
        hs = hs.to_float();
        let hw = if opts.contains("hook_width") { shape_len("equilibrium", opts.hook_width, w) }
                 else if alias && opts.contains("head_width") { shape_len("equilibrium", opts.head_width, w) }
                 else { hs * 0.4 };
        let hl = if opts.contains("head_size") { opts.head_size.to_float() } else { w * 5.0 };
        let h0 = plot_opt(opts, "hook_start", if alias { 0.0 } else { hl * 0.6 }).to_float();
        // Where each hook leaves its shaft, the unit back along the shaft,
        // and outwards.
        let ends = [
            [x2 + ox + ny * h0, y2 + oy - nx * h0, ny, -nx, nx, ny],
            [x1 - ox - ny * h0, y1 - oy + nx * h0, -ny, nx, -nx, -ny],
        ];
        for e in ends {
            // Local [back, out] -> page: tangent to the shaft where it
            // starts, curling out to (hook_size, hook_width).
            let c1 = hook_pt(e, hs * 0.57, 0.0);
            let c2 = hook_pt(e, hs * 1.01, hw * 0.41);
            let q = hook_pt(e, hs, hw);
            parts.push(path([[e[0], e[1]]]).cubic(c1[0], c1[1], c2[0], c2[1], q[0], q[1])
                .stroke_width(w * 1.5).cap("round"));
        }
    }
    if c != "" {
        let out = [];
        for x in parts { out.push(x.stroke(c)); }
        parts = out;
    }
    stack(parts)
}

// Point `a` back along and `o` out from a tip `e` = [x, y, back.., out..].
fn hook_pt(e, a, o) { [e[0] + e[2] * a + e[4] * o, e[1] + e[3] * a + e[5] * o] }

// The point at `deg` degrees (0 = +x, clockwise on the page) on the circle of
// radius `rad` about (cx, cy).
fn arc_pt(cx, cy, rad, deg) {
    let t = deg.to_float() * PI() / 180.0;
    [cx + rad * t.cos(), cy + rad * t.sin()]
}

// A circular block arrow: a band `w` wide centred on the circle of radius `r`
// about (cx, cy), from angle `a0` to its tip at `a1` (degrees, 0 = pointing
// right, increasing clockwise on the page as for `rotate`; `a1 < a0` runs
// anticlockwise; less than a full turn). The head is `head_width` across
// (default 2 × w) and `head_len` long along the circle (default w), its tip
// on the centre line. One closed `path` of arcs and corners, filled with
// `accent()`. `opts`: head_width, head_len, both (a head at `a0` too).
fn arc_arrow(cx, cy, r, a0, a1, w) { arc_arrow(cx, cy, r, a0, a1, w, #{}) }

fn arc_arrow(cx, cy, r, a0, a1, w, opts) {
    shape_opts("arc_arrow", opts, ["head_width", "head_len", "both"]);
    let r = r.to_float();
    let w = w.to_float();
    if !(w > 0.0) || !(r > w / 2.0) {
        throw `arc_arrow(): need 0 < w < 2 r (got r = ${r}, w = ${w})`;
    }
    let span = (a1 - a0).to_float();
    if span == 0.0 || span.abs() >= 360.0 {
        throw `arc_arrow(): the angles must differ by less than a full turn (got ${a0} to ${a1})`;
    }
    let dir = if span > 0.0 { 1.0 } else { -1.0 };
    let hw = plot_opt(opts, "head_width", w * 2.0).to_float();
    let hl = plot_opt(opts, "head_len", w).to_float();
    let both = plot_opt(opts, "both", false);
    // Head length as an angle, leaving the band at least a sliver.
    let dh = hl / r * 180.0 / PI();
    let room = span.abs() / (if both { 2.0 } else { 1.0 }) * 0.999;
    if dh > room { dh = room; }
    let ah = a1 - dir * dh;
    let a_s = if both { a0 + dir * dh } else { a0 };
    let ro = r + w / 2.0;
    let ri = r - w / 2.0;
    let large = (ah - a_s).abs() > 180.0;
    let fwd = if dir > 0.0 { "cw" } else { "ccw" };
    let back = if dir > 0.0 { "ccw" } else { "cw" };
    let p = if both {
        let t = arc_pt(cx, cy, r, a0);
        let c = arc_pt(cx, cy, r + hw / 2.0, a_s);
        let b = arc_pt(cx, cy, ro, a_s);
        path([t]).to(c[0], c[1]).to(b[0], b[1])
    } else {
        path([arc_pt(cx, cy, ro, a0)])
    };
    let e = arc_pt(cx, cy, ro, ah);
    p = p.arc(ro, e[0], e[1], #{ sweep: fwd, large: large });
    for q in [arc_pt(cx, cy, r + hw / 2.0, ah), arc_pt(cx, cy, r, a1),
              arc_pt(cx, cy, r - hw / 2.0, ah), arc_pt(cx, cy, ri, ah)] {
        p = p.to(q[0], q[1]);
    }
    let e = arc_pt(cx, cy, ri, a_s);
    p = p.arc(ri, e[0], e[1], #{ sweep: back, large: large });
    if both {
        let c = arc_pt(cx, cy, r - hw / 2.0, a_s);
        p = p.to(c[0], c[1]);
    }
    p.closed().fill(accent())
}

// A bent block arrow along a centre line: a band `w` wide following `points`
// with a triangular head at the last point. `points` start with [x, y]; each
// further entry is a straight run to [x, y], a quadratic [cx, cy, x, y] or a
// cubic [c1x, c1y, c2x, c2y, x, y] (the arguments of the `path` builder's
// `.quad` / `.cubic`). The outline is the centre line (curves flattened)
// offset by w / 2 on both sides: mitred joins, bevelled past a miter of 4 ×
// (corners sharper than ~30°), so keep curves wider than the band. Options:
//   radius      round every corner between straight runs with this radius
//               (at most half of either run); 0 (default) keeps them sharp
//   head_width  across the head (default 2 × w)
//   head_len    along the line (default w)
//   both        a head at the first point too
// One closed `path`, filled with `accent()`.
fn block_arrow_along(points, w) { block_arrow_along(points, w, #{}) }

fn block_arrow_along(points, w, opts) {
    let who = "block_arrow_along";
    shape_opts(who, opts, ["radius", "head_width", "head_len", "both"]);
    let w = w.to_float();
    if !(w > 0.0) { throw `${who}(): the width must be positive (got ${w})`; }
    let hw = plot_opt(opts, "head_width", w * 2.0).to_float();
    let hl = plot_opt(opts, "head_len", w).to_float();
    let both = plot_opt(opts, "both", false);
    let entries = along_round(along_entries(who, points), plot_opt(opts, "radius", 0).to_float());
    let cl = along_flatten(entries);
    if cl.len() < 2 { throw `${who}(): the centre line has no length`; }
    let cum = [0.0];
    for i in 1..cl.len() { cum.push(cum[i - 1] + along_dist(cl[i - 1], cl[i])); }
    let total = cum[cum.len() - 1];
    let room = total / (if both { 2.0 } else { 1.0 }) * 0.999;
    if hl > room { hl = room; }
    let s0 = if both { hl } else { 0.0 };
    let shaft = along_cut(cl, cum, s0, total - hl);
    let h = w / 2.0;
    // Normals: the head axes at the ends, the segments in between.
    let tip = cl[cl.len() - 1];
    let base = shaft[shaft.len() - 1];
    let n_end = along_normal(base, tip);
    let n_start = if both { along_normal(cl[0], shaft[0]) } else { along_normal(shaft[0], shaft[1]) };
    let left = [along_off(shaft[0], n_start, h)];
    let right = [along_off(shaft[0], n_start, -h)];
    for i in 1..shaft.len() - 1 {
        let q = shaft[i];
        let na = along_normal(shaft[i - 1], q);
        let nb = along_normal(q, shaft[i + 1]);
        let k = 1.0 + na[0] * nb[0] + na[1] * nb[1];
        if k < 1e-6 {
            // Doubling back: a flat end on both sides.
            left += [along_off(q, na, h), along_off(q, nb, h)];
            right += [along_off(q, na, -h), along_off(q, nb, -h)];
            continue;
        }
        let m = [(na[0] + nb[0]) / k, (na[1] + nb[1]) / k];
        if (m[0] * m[0] + m[1] * m[1]).sqrt() <= 4.0 {
            left.push(along_off(q, m, h));
            right.push(along_off(q, m, -h));
        } else {
            // Past the miter limit: bevel the outer side.
            let turns_left = (shaft[i + 1][0] - q[0]) * na[0] + (shaft[i + 1][1] - q[1]) * na[1] > 0.0;
            if turns_left {
                left.push(along_off(q, m, h));
                right += [along_off(q, na, -h), along_off(q, nb, -h)];
            } else {
                left += [along_off(q, na, h), along_off(q, nb, h)];
                right.push(along_off(q, m, -h));
            }
        }
    }
    left.push(along_off(base, n_end, h));
    right.push(along_off(base, n_end, -h));
    let pts = [];
    if both {
        pts += [cl[0], along_off(shaft[0], n_start, hw / 2.0)];
    }
    pts += left;
    pts += [along_off(base, n_end, hw / 2.0), tip, along_off(base, n_end, -hw / 2.0)];
    right.reverse();
    pts += right;
    if both { pts.push(along_off(shaft[0], n_start, -hw / 2.0)); }
    path(pts).closed().fill(accent())
}

// The entries of a `block_arrow_along` centre line, as floats, checked.
fn along_entries(who, points) {
    if type_of(points) != "array" || points.len() < 2 {
        throw `${who}(): expected an array of at least two points`;
    }
    let out = [];
    for e in points {
        let n = if type_of(e) == "array" { e.len() } else { 0 };
        if (out.len() == 0 && n != 2) || ![2, 4, 6].contains(n) {
            throw `${who}(): entries are [x, y], [cx, cy, x, y] or [c1x, c1y, c2x, c2y, x, y]; the first is [x, y]`;
        }
        let v = [];
        for x in e { v.push(x.to_float()); }
        out.push(v);
    }
    out
}

// Corners between straight runs rounded with radius `rad`: the corner point
// becomes a quadratic from `rad` before it to `rad` after it.
fn along_round(entries, rad) {
    if !(rad > 0.0) { return entries; }
    let out = [entries[0]];
    let n = entries.len();
    for i in 1..n {
        let v = entries[i];
        if v.len() == 2 && i + 1 < n && entries[i + 1].len() == 2 {
            let prev = entries[i - 1];
            let p = [prev[prev.len() - 2], prev[prev.len() - 1]];
            let nx = entries[i + 1];
            let d1 = along_dist(p, v);
            let d2 = along_dist(v, nx);
            if d1 > 0.0 && d2 > 0.0 {
                let r = rad;
                if r > d1 / 2.0 { r = d1 / 2.0; }
                if r > d2 / 2.0 { r = d2 / 2.0; }
                let a = [v[0] + (p[0] - v[0]) / d1 * r, v[1] + (p[1] - v[1]) / d1 * r];
                let b = [v[0] + (nx[0] - v[0]) / d2 * r, v[1] + (nx[1] - v[1]) / d2 * r];
                out.push(a);
                out.push([v[0], v[1], b[0], b[1]]);
                continue;
            }
        }
        out.push(v);
    }
    out
}

// The centre line as a polyline: curves sampled at 24 points each,
// repeated points dropped.
fn along_flatten(entries) {
    let pts = [entries[0]];
    let cur = entries[0];
    for i in 1..entries.len() {
        let e = entries[i];
        let end = [e[e.len() - 2], e[e.len() - 1]];
        if e.len() == 2 {
            pts.push(end);
        } else {
            let k = 24;
            for j in 1..=k {
                let t = j.to_float() / k.to_float();
                let u = 1.0 - t;
                pts.push(if e.len() == 4 {
                    [u * u * cur[0] + 2.0 * u * t * e[0] + t * t * end[0],
                     u * u * cur[1] + 2.0 * u * t * e[1] + t * t * end[1]]
                } else {
                    [u * u * u * cur[0] + 3.0 * u * u * t * e[0] + 3.0 * u * t * t * e[2] + t * t * t * end[0],
                     u * u * u * cur[1] + 3.0 * u * u * t * e[1] + 3.0 * u * t * t * e[3] + t * t * t * end[1]]
                });
            }
        }
        cur = end;
    }
    let out = [pts[0]];
    for p in pts {
        if along_dist(out[out.len() - 1], p) > 1e-9 { out.push(p); }
    }
    out
}

fn along_dist(a, b) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

// Unit left normal of a -> b (as `chord_frame`: upwards for a rightward run).
fn along_normal(a, b) {
    let d = along_dist(a, b);
    [(b[1] - a[1]) / d, -(b[0] - a[0]) / d]
}

fn along_off(p, n, d) { [p[0] + n[0] * d, p[1] + n[1] * d] }

// The part of polyline `cl` (cumulative lengths `cum`) from arc length s0
// to s1, with interpolated ends.
fn along_cut(cl, cum, s0, s1) {
    let out = [along_at(cl, cum, s0)];
    for i in 0..cl.len() {
        if cum[i] > s0 + 1e-9 && cum[i] < s1 - 1e-9 { out.push(cl[i]); }
    }
    out.push(along_at(cl, cum, s1));
    out
}

fn along_at(cl, cum, s) {
    for i in 1..cl.len() {
        if s <= cum[i] || i == cl.len() - 1 {
            let seg = cum[i] - cum[i - 1];
            let t = if seg > 0.0 { (s - cum[i - 1]) / seg } else { 0.0 };
            return [cl[i - 1][0] + (cl[i][0] - cl[i - 1][0]) * t, cl[i - 1][1] + (cl[i][1] - cl[i - 1][1]) * t];
        }
    }
    cl[0]
}

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
// An item that asks for label room (a plot notes `plot_label_room()` as
// `.meta("label_room")`) first gets that empty band prepended inside it, so
// the label sits clear of its content.
fn labeled(item, s) {
    let tag = pin_in("label_corner", label_tag(s), label_corner(), label_inset());
    let item = as_node(item);
    let room = item.meta("label_room");
    if type_of(room) != "()" && room > 0.0 {
        item = item.prepend(rect().height(room)).meta("label_room", ());
    }
    stack([item, tag])
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

// The positions of `overlay`, in reading order, and their [main, cross]
// alignment in a filling column (main is vertical, cross horizontal).
fn overlay_positions() {
    [
        ["top-left", "start", "start"], ["top", "start", "center"], ["top-right", "start", "end"],
        ["left", "center", "start"], ["center", "center", "center"], ["right", "center", "end"],
        ["bottom-left", "end", "start"], ["bottom", "end", "center"], ["bottom-right", "end", "end"],
    ]
}

// `overlay(item, map)` / `overlay(item, map, opts)`: nodes at their natural
// size over `item` (any panel) by position name — `#{ "top-right": tag,
// bottom: [a, b] }`. Keys: top-left, top, top-right, left, center, right,
// bottom-left, bottom, bottom-right; a value is a node, a string (`body`),
// spans, or an array of those, stacked `overlay_gap()` apart and aligned to
// the position's side. Each position is a filling column padded
// `overlay_inset()` (the label inset) and aligned like `pin`, so nothing
// takes coordinates and it follows the panel's size. `opts`: `inset`, `gap`
// (cm). The result is a core `stack`: `labeled(overlay(img, ..), "(a)")`.
fn overlay(item, map) { overlay(item, map, #{}) }
fn overlay(item, map, opts) {
    if type_of(map) != "map" {
        throw `overlay(): expected a map of position -> node(s) (got ${type_of(map)})`;
    }
    shape_opts("overlay", opts, ["inset", "gap"]);
    let inset = plot_opt(opts, "inset", overlay_inset());
    let g = plot_opt(opts, "gap", overlay_gap());
    let names = [];
    for p in overlay_positions() { names.push(p[0]); }
    for k in map.keys() {
        if !names.contains(k) { throw `overlay(): unknown position "${k}" (${list_names(names)})`; }
    }
    let layers = [as_node(item)];
    for p in overlay_positions() {
        if !map.contains(p[0]) { continue; }
        let v = map[p[0]];
        let xs = if type_of(v) == "array" && !poster_is_rich(v) { v } else { [v] };
        let ns = [];
        for x in xs { ns.push(scheme_text("overlay", x, base_size(), p[2])); }
        layers.push(col(ns).spacing(g).cross(p[2]).pad(inset).main(p[1]));
    }
    stack(layers)
}

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

// Scheme / overlay / side text: a string or spans becomes text at `size`
// (pt) in `fg()`, aligned `align` ("start", "center", "end"); a node passes
// through; anything else is an error of function `who`.
fn scheme_text(who, x, size, align) {
    let t = type_of(x);
    if t == "string" || poster_is_rich(x) {
        let n = fg_text(x).size(size);
        return if align == "center" { n.center() } else if align == "end" { n.align("right") } else { n };
    }
    if t != "Node" {
        throw `${who}(): expected a string, spans or a node (got ${t})`;
    }
    x
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
// Schemes (reaction schemes, process flows: a row of nodes and arrows)
// ---------------------------------------------------------------------------
//
// `scheme([a, arrow_step("THF, 60 °C"), b])` lays a sequence out as a row
// with every item centred on one axis: nodes (molecule images, icons),
// `step`s (a node with captions) and `arrow_step` arrows with their
// conditions above / below. Nothing is positioned by hand: a caption is
// balanced by an invisible copy on the opposite side (opacity 0, which no
// backend paints), so each node and arrow stays on the centre line however
// long its texts are. `scheme_v` is the same down the page.

// `step(node, #{ above, below, left, right })`: `node` with captions (a
// string, spans or a node each; strings at `scheme_label_size()`, centred),
// `scheme_label_gap()` from it; `step(node, "caption")` puts one below. The
// node stays centred in the step, so in a scheme it sits on the axis.
fn step(node) { as_node(node) }
fn step(node, opts) {
    if type_of(opts) == "string" || poster_is_rich(opts) { opts = #{ below: opts }; }
    shape_opts("step", opts, ["above", "below", "left", "right"]);
    scheme_dress("step", as_node(node), opts, false)
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
    shape_opts("arrow_step", o, ["above", "below", "left", "right", "length", "head", "equilibrium",
                                 "color", "width", "head_size", "head_width"]);
    let head = plot_opt(o, "head", "end");
    if !["end", "both", "start", "none"].contains(head) {
        throw `arrow_step(): unknown head "${head}" (end, both, start, none)`;
    }
    let eq = plot_opt(o, "equilibrium", false);
    if type_of(eq) == "bool" {
        eq = if eq { #{} } else { () };
    } else if type_of(eq) != "map" {
        throw `arrow_step(): equilibrium must be true, false or a map of equilibrium options (got ${type_of(eq)})`;
    }
    let len = plot_opt(o, "length", ());
    if type_of(len) != "()" { len = scheme_length("arrow_step", len); }
    #{ scheme_arrow: true, opts: o, head: head, eq: eq, length: len }
}

// `scheme(items)` / `scheme(items, opts)`: `items` (nodes, strings such as
// "+", `step`s, `arrow_step`s) in a row `scheme_gap()` apart, centred on one
// axis and in the row. Options:
//   direction     "right" (default) or "down" (a column, as `scheme_v`)
//   gap           between items, cm (default `scheme_gap()`)
//   align         across the flow: "center" (default), "start", "end"
//   arrow_length  of every arrow without its own: cm or "fill"
//                 (default `scheme_arrow_length()`)
// Images need a size (`image("a.png").height(2)`: the width follows).
fn scheme(items) { scheme(items, #{}) }
fn scheme(items, opts) {
    if type_of(items) != "array" { throw `scheme(): expected an array of items (got ${type_of(items)})`; }
    shape_opts("scheme", opts, ["direction", "gap", "align", "arrow_length"]);
    let dir = plot_opt(opts, "direction", "right");
    if dir != "right" && dir != "down" { throw `scheme(): unknown direction "${dir}" (right, down)`; }
    let vertical = dir == "down";
    let align = plot_opt(opts, "align", "center");
    if !["center", "start", "end"].contains(align) {
        throw `scheme(): unknown align "${align}" (center, start, end)`;
    }
    let len = scheme_length("scheme", plot_opt(opts, "arrow_length", scheme_arrow_length()));
    let out = [];
    for x in items {
        if type_of(x) == "map" {
            if !x.contains("scheme_arrow") { throw "scheme(): a map item must be an arrow_step(..)"; }
            out.push(scheme_arrow(x, vertical, len));
        } else {
            out.push(scheme_text("scheme", x, base_size(), "center"));
        }
    }
    let c = if vertical { col(out) } else { row(out) };
    c.spacing(plot_opt(opts, "gap", scheme_gap())).cross(align).main("center")
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
// missing for none), each balanced by an invisible copy of the opposite one.
// `stretch`: the column / row around `n` stretches it across (a "fill"
// arrow) instead of centring it.
fn scheme_dress(who, n, labels, stretch) {
    let lab = #{};
    for k in ["above", "below", "left", "right"] {
        let v = plot_opt(labels, k, ());
        let al = if k == "left" { "end" } else if k == "right" { "start" } else { "center" };
        lab[k] = if type_of(v) == "()" { () } else { scheme_text(who, v, scheme_label_size(), al) };
    }
    let g = scheme_label_gap();
    if type_of(lab.above) != "()" || type_of(lab.below) != "()" {
        let c = col([scheme_slot(lab.above, lab.below, "end", "center"), n,
                     scheme_slot(lab.below, lab.above, "start", "center")]).spacing(g);
        n = if stretch { c.stretch() } else { c.cross("center") };
    }
    if type_of(lab.left) != "()" || type_of(lab.right) != "()" {
        let r = row([scheme_slot(lab.left, lab.right, "center", "end"), n,
                     scheme_slot(lab.right, lab.left, "center", "start")]).spacing(g);
        n = if stretch { r.stretch() } else { r.cross("center") };
    }
    n
}

// A caption slot: `x` aligned [main, cross] in it, over an invisible copy of
// `other` (the caption on the opposite side), so opposite slots are equal.
fn scheme_slot(x, other, main, cross) {
    let parts = [];
    if type_of(other) != "()" { parts.push(other.opacity(0)); }
    if type_of(x) != "()" { parts.push(col([x]).main(main).cross(cross)); }
    stack(parts)
}

// The node of arrow descriptor `a` in a scheme (`vertical`: pointing down),
// `len` cm long or "fill" (the scheme's default when `a` has no length),
// with its labels.
fn scheme_arrow(a, vertical, len) {
    let o = a.opts;
    let w = plot_opt(o, "width", scheme_arrow_width()).to_float();
    let c = plot_opt(o, "color", fg());
    if type_of(a.length) != "()" { len = a.length; }
    let fill = type_of(len) == "string";
    let hs = plot_opt(o, "head_size", w * 5.0).to_float();
    let hw = if o.contains("head_width") { shape_len("arrow_step", o.head_width, w) } else { hs * 0.8 };
    let node = ();
    if type_of(a.eq) == "map" {
        let eo = a.eq;
        if !eo.contains("width") { eo.width = w; }
        for k in ["head_size", "head_width"] {
            if o.contains(k) && !eo.contains(k) { eo[k] = o[k]; }
        }
        let ew = eo.width.to_float();
        let g = plot_opt(eo, "gap", ew * 4.0).to_float();
        let ehs = plot_opt(eo, "head_size", ew * 5.0).to_float();
        let ehw = if eo.contains("head_width") { shape_len("arrow_step", eo.head_width, ew) } else { ehs * 0.8 };
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
    let labels = if vertical {
        #{ right: plot_opt(o, "right", plot_opt(o, "above", ())),
           left: plot_opt(o, "left", plot_opt(o, "below", ())) }
    } else {
        #{ above: plot_opt(o, "above", ()), below: plot_opt(o, "below", ()) }
    };
    let n = scheme_dress("arrow_step", node, labels, fill);
    if fill { n.flex(1) } else { n }
}

// The ⇌ pair of a "fill" arrow, `t` cm thick, its shafts `g` apart: two
// arrows in a viewbox that is a fraction along the flow and cm across, so
// they stretch with the arrow (equilibrium options gap, width, head,
// head_size, head_width; a hooked barb needs a fixed length).
fn scheme_equilibrium_fill(eo, t, g, vertical) {
    shape_opts("equilibrium", eo, ["gap", "width", "head", "head_size", "head_width", "barb"]);
    if plot_opt(eo, "barb", "straight") != "straight" {
        throw "arrow_step(): a hooked equilibrium needs a fixed length (not \"fill\")";
    }
    let head = plot_opt(eo, "head", "barb");
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
// from `base_size()` — or, with `frame_box`, outside a plot node that *is*
// the data frame, so a plot can be placed by its axes. A series is a map built by
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
               marker: if kind == "scatter" { "o" } else { "none" }, size: (), yerr: (), open: () };
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
    if type_of(s.open) != "bool" && type_of(s.open) != "()" {
        throw `${kind}(): open must be true or false (got ${s.open})`;
    }
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
        // Defaults left open by the series builders, read here so a plot's
        // `style:` option (in force while it draws) applies to them.
        if type_of(s.open) == "()" { s.open = marker_open(); }
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

// Plot text `s` (a string, or spans such as `markup("*T* [K]")`) at `size`
// (pt) in `font` ("" or [] for the page font), in `plot_fg()`.
fn plot_label(s, size, font) { plot_font_on(text(s).size(size).color(plot_fg()), font) }

// `t` in `font`, unless it is "" / [] / () (then no font of its own).
fn plot_font_on(t, font) {
    let k = type_of(font);
    if k == "()" || (k == "string" && font == "") || (k == "array" && font.len() == 0) { t } else { t.font(font) }
}

// Is a label option set: a non-empty string, or spans (an array)?
fn plot_has_label(v) {
    let k = type_of(v);
    if k == "string" { return v != ""; }
    if k == "array" { return v.len() > 0; }
    if k == "()" { return false; }
    throw `plot(): labels must be strings or markup spans (got ${k})`;
}

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
//   font    a family or fallback list (default `plot_font()`: the page font)
//   bold    true for bold
//   italic  true for italic
//   markup  true: `s` is inline markup (`*T*_{1/2}`, `{#ff0000|hot}`, …;
//           see `rich`) — rich spans in one text node
//   anchor  [ax, ay]: which point of the text sits at (x, y), fractions of
//           its box from its top-left (default [0.5, 0.5], centred)
fn plot_text(x, y, s) { plot_text(x, y, s, #{}) }
fn plot_text(x, y, s, opts) {
    plot_annotation("plot_text", "text", [x, y], s, opts,
                    ["color", "size", "font", "bold", "italic", "markup", "anchor"])
}

// `plot_arrow(x1, y1, x2, y2)` / `(…, opts)`: an arrow from data point
// (x1, y1) to (x2, y2), in `plot_fg()`, `series_width()` wide. Options:
// `color` (or a palette index), `width` (cm), `head` ("end" default, "start",
// "both"), `dash`.
fn plot_arrow(x1, y1, x2, y2) { plot_arrow(x1, y1, x2, y2, #{}) }
fn plot_arrow(x1, y1, x2, y2, opts) {
    plot_annotation("plot_arrow", "arrow", [x1, y1, x2, y2], "", opts, ["color", "width", "head", "dash"])
}

// `plot_dimension(x1, x2, y, label)` / `(…, opts)`: a dimension line — a
// double-headed arrow from data x1 to x2 at height y — with `label` (a
// string or spans) centred above it, `tick_label_pad()` clear of the line;
// in `plot_fg()`, `plot_line_width()` wide. Options: `color` (or a palette
// index), `width` (cm), `below: true` (the label under the line), `size`
// (pt), `gap` (cm, line to label), `dash`.
fn plot_dimension(x1, x2, y, s) { plot_dimension(x1, x2, y, s, #{}) }
fn plot_dimension(x1, x2, y, s, opts) {
    plot_annotation("plot_dimension", "dimension", [x1, y, x2, y], s, opts,
                    ["color", "width", "below", "size", "gap", "dash"])
}

// `plot_dimension_v(x, y1, y2, label)` / `(…, opts)`: the same upright, from
// y1 to y2 at data x, the label right of its middle (`left: true`: left).
fn plot_dimension_v(x, y1, y2, s) { plot_dimension_v(x, y1, y2, s, #{}) }
fn plot_dimension_v(x, y1, y2, s, opts) {
    let a = plot_annotation("plot_dimension_v", "dimension", [x, y1, x, y2], s, opts,
                            ["color", "width", "left", "size", "gap", "dash"]);
    a.vertical = true;
    a
}

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
    if type_of(a.width) != "()" { return pin_in("inset", a.node, corner, a.pad); }
    let gap = rect().flex(1.0 - a.size);
    let n = a.node.flex(a.size);
    let r = if corner.ends_with("right") { row([gap, n]) } else { row([n, gap]) };
    r.cross(if corner.starts_with("top") { "start" } else { "end" }).pad(a.pad)
}

// The descriptor of `inset(sub, opts)`: an annotation (`mark: "inset"`) the
// plot places once its axes exist, holding the sized node.
fn inset_spec(sub, opts) {
    if type_of(opts) != "map" { throw `inset(): options must be a map (got ${type_of(opts)})`; }
    let known = ["corner", "at", "anchor", "size", "width", "height", "pad"];
    for k in opts.keys() {
        if !known.contains(k) { throw `inset(): unknown option "${k}" (${list_names(known)})`; }
    }
    let n = as_node(sub);
    let width = plot_opt(opts, "width", ());
    if type_of(width) != "()" { n = n.width(width); }
    if opts.contains("height") { n = n.height(opts.height); }
    let size = plot_opt(opts, "size", inset_size()).to_float();
    if !(size > 0.0 && size <= 1.0) { throw `inset(): size must be a fraction in (0, 1] (got ${size})`; }
    let a = #{ mark: "inset", node: n, at: (), anchor: [0, 0], size: size, width: width,
               corner: plot_opt(opts, "corner", inset_corner()), pad: plot_opt(opts, "pad", inset_pad()) };
    if opts.contains("at") {
        a.at = plot_numbers("inset", "`at`", opts.at);
        if a.at.len() != 2 { throw "inset(): `at` must be [x, y]"; }
    }
    a.anchor = plot_numbers("inset", "`anchor`", plot_opt(opts, "anchor", a.anchor));
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
    let own = plot_opt(opts, "style", #{});
    if type_of(own) != "map" { throw `inset_plot(): style must be a map of style keys (got ${type_of(own)})`; }
    for k in own.keys() { st[k] = own[k]; }
    opts.style = st;
    plot(opts, series)
}

// An annotation descriptor (`plot` turns it into nodes once the axes exist).
fn plot_annotation(name, kind, pts, s, opts, known) {
    if type_of(opts) != "map" { throw `${name}(): options must be a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        if !known.contains(k) { throw `${name}(): unknown option "${k}" (${list_names(known)})`; }
    }
    // Unset style-dependent defaults stay () until the plot draws the
    // annotation, so its `style:` option applies to them.
    let a = #{ mark: kind, pts: plot_numbers(name, "coordinates", pts), s: s, color: () };
    if opts.contains("color") {
        a.color = if type_of(opts.color) == "i64" { palette_color(opts.color) } else { opts.color };
    }
    if kind == "text" {
        if type_of(s) != "string" { throw `${name}(): text must be a string (got ${type_of(s)})`; }
        a.size = plot_opt(opts, "size", ());
        a.font = plot_opt(opts, "font", ());
        for k in ["bold", "italic", "markup"] {
            a[k] = plot_opt(opts, k, false);
            if type_of(a[k]) != "bool" { throw `${name}(): ${k} must be true or false (got ${a[k]})`; }
        }
        // Parse markup now, so a malformed string is reported at this call.
        if a.markup { a.s = markup(s); }
        a.anchor = plot_numbers(name, "anchor", plot_opt(opts, "anchor", [0.5, 0.5]));
        if a.anchor.len() != 2 { throw `${name}(): anchor must be [ax, ay]`; }
    } else if kind == "dimension" {
        if type_of(s) != "string" && !poster_is_rich(s) {
            throw `${name}(): the label must be a string or spans (got ${type_of(s)})`;
        }
        a.width = plot_opt(opts, "width", ());
        a.size = plot_opt(opts, "size", ());
        a.gap = plot_opt(opts, "gap", ());
        a.dash = plot_opt(opts, "dash", []);
        a.vertical = false;
        // The label's side: above / right (false) or below / left (true).
        a.flip = plot_opt(opts, "below", plot_opt(opts, "left", false));
        if type_of(a.flip) != "bool" { throw `${name}(): below / left must be true or false (got ${a.flip})`; }
    } else {
        a.width = plot_opt(opts, "width", ());
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
    if a.mark == "inset" {
        let px = plot_tf(a.at[0], xlog);
        let py = plot_tf(a.at[1], ylog);
        if type_of(px) == "()" || type_of(py) == "()" { return (); }
        if type_of(a.width) != "()" { return a.node.at(px, py).anchor(a.anchor[0], a.anchor[1]); }
        // `size` of the frame width: a full-width row at height `py` whose
        // spacers put the inset's left edge where `anchor` asks.
        let u = ((px - x0) / (x1 - x0) - a.anchor[0] * a.size).max(0.0).min(1.0 - a.size);
        let parts = [];
        if u > 0.0 { parts.push(rect().flex(u)); }
        parts.push(a.node.flex(a.size));
        if 1.0 - u - a.size > 1e-9 { parts.push(rect().flex(1.0 - u - a.size)); }
        return row(parts).y(py).anchor(0, a.anchor[1]);
    }
    let p = [];
    for i in 0..a.pts.len() {
        let v = plot_tf(a.pts[i], if i % 2 == 0 { xlog } else { ylog });
        if type_of(v) == "()" { return (); }   // off a log axis: nothing to draw
        p.push(v);
    }
    let c = if type_of(a.color) == "()" { plot_fg() } else { a.color };
    if a.mark == "text" {
        let size = if type_of(a.size) == "()" { base_size() } else { a.size };
        let t = plot_font_on(text(a.s).size(size).color(c),
                             if type_of(a.font) == "()" { plot_font() } else { a.font });
        if a.bold { t = t.bold(); }
        if a.italic { t = t.italic(); }
        return t.at(p[0], p[1]).anchor(a.anchor[0], a.anchor[1]);
    }
    if a.mark == "dimension" {
        // The line, and the label in a column padded by the gap on the
        // line's side, anchored on the line's middle.
        let w = if type_of(a.width) == "()" { plot_line_width() } else { a.width };
        let line = arrow(p[0], p[1], p[2], p[3]).viewbox(x0, y1, x1, y0).stroke(c)
            .stroke_width(w).head("both").dash(a.dash);
        let size = if type_of(a.size) == "()" { base_size() } else { a.size };
        let g = if type_of(a.gap) == "()" { tick_label_pad() } else { a.gap };
        let t = plot_font_on(text(a.s).size(size).color(c), plot_font()).nowrap();
        let mx = (p[0] + p[2]) / 2.0;
        let my = (p[1] + p[3]) / 2.0;
        let lab = if a.vertical {
            if a.flip { col([t]).pad(#{ right: g }).at(mx, my).anchor(1, 0.5) }
            else { col([t]).pad(#{ left: g }).at(mx, my).anchor(0, 0.5) }
        } else if a.flip {
            col([t]).pad(#{ top: g }).at(mx, my).anchor(0.5, 0)
        } else {
            col([t]).pad(#{ bottom: g }).at(mx, my).anchor(0.5, 1)
        };
        return [line, lab];
    }
    let w = if type_of(a.width) == "()" { series_width() } else { a.width };
    arrow(p[0], p[1], p[2], p[3]).viewbox(x0, y1, x1, y0).stroke(c)
        .stroke_width(w).head(a.head).dash(a.dash)
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
            rows.push(plot_font_on(text(s.label).size(legend_size()).color(s.color), legend_font()));
        } else {
            rows.push(row([legend_key(s), plot_label(s.label, legend_size(), legend_font())])
                .cross("center").spacing(legend_size() * 0.02));
        }
    }
    col(rows).spacing(legend_size() * 0.008)
}

// `series` (an array or one series) without annotation descriptors.
fn plot_only_series(series) {
    if type_of(series) != "array" { series = [series]; }
    series.filter(|x| !plot_is_annotation(x))
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
//   xminor, yminor minor intervals per labelled step on that axis (default
//                  minor_ticks())
//   xtick_format, ytick_format
//                  tick label decimals (2 -> "0.25") or a function of the
//                  value returning the label (default: shared decimals)
//   xtick_direction, ytick_direction
//                  "out" or "in" for that axis (default tick_direction())
//   frame_box      true: the node's box is the data frame and labels hang
//                  outside it (default plot_frame_box())
//   xtick_label_pad, ytick_label_pad, tick_label_pad_mode
//                  tick -> tick label gaps per axis (cm) and what the x one
//                  reaches to, "box" or "baseline" (default the hooks)
//   xlabel_pad, ylabel_pad
//                  tick labels -> axis label, box to box (cm, >= 0)
//   xlabel_offset, ylabel_offset
//                  axis -> axis label baseline (cm > 0): exact placement
//   style          a map of style keys in force for this plot only
//                  (`#{ base_size: 10, tick_label_size: 9 }`)
// Labels (`xlabel`, `ylabel`, `title`) take markup spans too:
// `xlabel: markup("*T* [K]")`.
// `series` may hold `plot_text` / `plot_arrow` annotations too.
// The result is an ordinary node (a column; the frame stack itself with
// `frame_box`) — chain `.flex`, `.height`, `.at`, ….
fn plot(series) { plot(#{}, series) }

fn plot(opts, series) {
    if type_of(opts) == "array" && type_of(series) == "map" && !plot_is_annotation(series) {
        throw "plot(): use plot(opts, series) — options first";
    }
    if type_of(opts) != "map" { throw `plot(): options must be a map (got ${type_of(opts)})`; }
    let known = ["x", "y", "xlabel", "ylabel", "xscale", "yscale", "xticks", "yticks",
                 "legend", "frame", "marks", "title", "xminor", "yminor", "xtick_format",
                 "ytick_format", "xtick_direction", "ytick_direction", "frame_box", "style",
                 "xtick_label_pad", "ytick_label_pad", "tick_label_pad_mode", "xlabel_pad",
                 "ylabel_pad", "xlabel_offset", "ylabel_offset"];
    for k in opts.keys() {
        if !known.contains(k) {
            throw `plot(): unknown option "${k}" (${list_names(known)})`;
        }
    }
    // A per-plot style: pushed over the current one while the plot is
    // built, then the registry is restored exactly (nothing leaks out).
    let st = plot_opt(opts, "style", ());
    if type_of(st) == "()" { return plot_build(opts, series); }
    if type_of(st) != "map" { throw `plot(): style must be a map of style keys (got ${type_of(st)})`; }
    let keys = style_keys();
    for k in st.keys() {
        if !keys.contains(k) {
            throw `plot(): unknown style key "${k}" (keys are hook names: ${list_names(keys)})`;
        }
    }
    style_push(st);
    let node = plot_build(opts, series);
    style_pop();
    node
}

// `plot(opts, series)` once its options are checked and its `style` is in
// force.
fn plot_build(opts, series) {
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
    xa = plot_relabel(xa, plot_opt(opts, "xtick_format", ()), xlog, "x");
    ya = plot_relabel(ya, plot_opt(opts, "ytick_format", ()), ylog, "y");
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
    let xin = plot_tick_inward(opts, "xtick_direction", dir);
    let yin = plot_tick_inward(opts, "ytick_direction", dir);
    // Outward ticks hang below / left of the axis; inward ones point into the
    // area (anchored at their far end instead).
    let xan = if xin { 1 } else { 0 };
    let yan = if yin { 0 } else { 1 };
    let xminor = xa.minor + plot_minor(xa, xlog, plot_opt(opts, "xminor", minor_ticks()), "xminor");
    let yminor = ya.minor + plot_minor(ya, ylog, plot_opt(opts, "yminor", minor_ticks()), "yminor");
    if xa.marks {
        for t in xa.ticks { marks.push(line(0, 0, 0, tl).stroke(fg).stroke_width(lw).at(t, y0).anchor(0, xan)); }
        for t in xminor { marks.push(line(0, 0, 0, tl * 0.6).stroke(fg).stroke_width(lw * 0.8).at(t, y0).anchor(0, xan)); }
    }
    for t in ya.ticks { marks.push(line(0, 0, tl, 0).stroke(fg).stroke_width(lw).at(x0, t).anchor(yan, 0)); }
    for t in yminor { marks.push(line(0, 0, tl * 0.6, 0).stroke(fg).stroke_width(lw * 0.8).at(x0, t).anchor(yan, 0)); }
    let corner_insets = [];
    for a in notes {
        if a.mark == "inset" && type_of(a.at) == "()" { corner_insets.push(a); continue; }
        let n = plot_annotation_node(a, xlog, ylog, x0, x1, y0, y1);
        if type_of(n) == "array" { extra += n; } else if type_of(n) != "()" { extra.push(n); }
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
    let taken = [];      // corners holding the legend / an inset
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
        taken.push(where);
    }

    // Corner insets, over the legend's corner only if asked for by name.
    for a in corner_insets {
        let c = if a.corner == "auto" { plot_auto_corner(samples, taken) } else { a.corner };
        taken.push(c);
        area.push(inset_in_corner(a, c));
    }

    let parts = [stack(area)];

    // Everything else hangs off the data area in the same data coordinates,
    // in a `contain` stack: the area shrinks until all of it fits the plot's
    // box, so each margin is exactly what its labels measure — the widest y
    // tick label, the tallest x tick label, the half labels overhanging the
    // axis ends — and nothing spills past the panel.
    let xpad = plot_len(opts, "xtick_label_pad", xtick_label_pad());   // tick -> label (cm)
    let ypad = plot_len(opts, "ytick_label_pad", ytick_label_pad());
    let mode = plot_opt(opts, "tick_label_pad_mode", tick_label_pad_mode());
    if mode != "box" && mode != "baseline" {
        throw plot_who(opts, "tick_label_pad_mode") + ` must be "box" or "baseline" (got "${mode}")`;
    }
    let apad = axis_label_pad();                   // title <-> unrotated y label
    if !plot_finite(apad) || apad < 0 {
        throw `axis_label_pad(): must be a length >= 0 cm (got ${apad}); place a label closer with xlabel_offset / ylabel_offset`;
    }
    let xlpad = plot_label_pad(opts, "xlabel_pad", xlabel_pad(), "xlabel_offset");
    let ylpad = plot_label_pad(opts, "ylabel_pad", ylabel_pad(), "ylabel_offset");
    let xoff = plot_label_offset(opts, "xlabel_offset", xlabel_offset());
    let yoff = plot_label_offset(opts, "ylabel_offset", ylabel_offset());
    let tsize = tick_label_size();
    let tfont = tick_label_font();
    let asize = axis_label_size();
    let afont = axis_label_font();
    let xtl = if xin { 0.0 } else { tl };          // room the ticks take outside
    let ytl = if yin { 0.0 } else { tl };
    // The label box (or baseline) may move onto the tick, not past the axis.
    for a in [["x", xtl, xpad], ["y", ytl, ypad]] {
        if a[1] + a[2] < -1e-12 {
            throw plot_who(opts, `${a[0]}tick_label_pad`) +
                ` ${a[2]} would put the ${a[0]} tick labels across the axis (tick room ${a[1]} cm + pad must be >= 0)`;
        }
    }
    let baseline = mode == "baseline";
    let xl = [];
    for i in 0..xa.ticks.len() {
        if xa.ticks[i] >= x0 - 1e-9 && xa.ticks[i] <= x1 + 1e-9 {
            let t = plot_label(xa.labels[i], tsize, tfont).center();
            xl.push(if baseline {
                // Its first baseline `xtl + xpad` below the axis, in a plain
                // stack that measures down to the label's box bottom (so
                // what follows clears it), centred on the tick.
                stack([t.at(0, xtl + xpad).anchor(0, "baseline")]).at(xa.ticks[i], 0).anchor(0.5, 0)
            } else {
                t.at(xa.ticks[i], 0).anchor(0.5, 0)
            });
        }
    }
    let yl = [];
    for i in 0..ya.ticks.len() {
        if ya.ticks[i] >= y0 - 1e-9 && ya.ticks[i] <= y1 + 1e-9 {
            yl.push(plot_label(ya.labels[i], tsize, tfont).at(1, ya.ticks[i]).anchor(1, 0.5));
        }
    }
    // Left band, as tall as the area and right-aligned to the axis: the y tick
    // labels in a viewbox stack spanning the axis (as wide as the widest
    // label), then room for the ticks — after the y label, when it is rotated
    // (`ylabel_rotated()`): a quarter turn swaps its footprint, so in the
    // stretched row it is one line wide and as long as the axis, centred on it.
    let ylabel = plot_opt(opts, "ylabel", "");
    let rotated = plot_has_label(ylabel) && ylabel_rotated();
    let yband = [];
    if rotated {
        // Unwrapped: a label longer than the axis overflows (with a layout
        // warning) instead of wrapping mid-word.
        let t = plot_label(ylabel, asize, afont).center().nowrap().rotate(-90);
        if type_of(yoff) == "()" {
            yband.push(t);
            yband.push(rect().width(ylpad));
        } else {
            // Its baseline `yoff` left of the axis: a stack that wide, in cm
            // from its right edge (on the axis) across, fractions of the
            // axis along; the label's ascent hangs out to the left.
            parts.push(stack([t.at(-yoff, 0.5).anchor("baseline", 0.5)])
                .viewbox(-yoff, 0, 0, 1).width(yoff).x(x0).anchor(1, 0));
        }
    }
    yband.push(stack(yl).viewbox(0, y1, 1, y0));
    yband.push(rect().width(ytl + ypad));
    parts.push(row(yband).stretch().x(x0).anchor(1, 0));
    // Bottom band, as wide as the area: room for the ticks, the x tick labels
    // (a viewbox stack spanning the axis), then the x label centred under it.
    // Categories fill equal slots across the axis, so their labels are cells
    // of a row instead, one per slot: long names wrap to the slot width rather
    // than run into each other.
    let xband = if cats.len() > 0 {
        let g = if xpad > 0.0 { xpad * 0.5 } else { 0.0 };
        let cells = [];
        for c in cats { cells.push(col([plot_label(c, tsize, tfont).center()]).stretch().pad(g).flex(1)); }
        [rect().height(xtl + xpad - g), row(cells)]
    } else if baseline {
        [stack(xl).viewbox(x0, 0, x1, 1)]
    } else {
        [rect().height(xtl + xpad), stack(xl).viewbox(x0, 0, x1, 1)]
    };
    let xlabel = plot_opt(opts, "xlabel", "");
    if plot_has_label(xlabel) {
        let t = plot_label(xlabel, asize, afont).center();
        if type_of(xoff) == "()" {
            xband += [rect().height(xlpad), t];
        } else {
            // Its first baseline `xoff` below the axis (a stack that tall,
            // in cm down, fractions across); the descent hangs below it.
            parts.push(stack([t.at(0.5, xoff).anchor(0.5, "baseline")])
                .viewbox(0, 0, 1, xoff).height(xoff).y(y0));
        }
    }
    parts.push(col(xband).stretch().y(y0));
    // An unrotated y label sits above the axis, its bottom edge half its own
    // height up: clear of the top tick label's overhang (same text size).
    // A title goes above it all, centred on the data area: a band as wide as
    // the area (no x offset), holding the title and the unrotated y label
    // under it, its bottom a quarter of the band (two lines) or half the
    // title (one) above the axis — clear of the top tick label's overhang
    // as long as the title is no smaller than the tick labels.
    let ptitle = plot_opt(opts, "title", "");
    let top = [];
    if plot_has_label(ptitle) {
        top.push(plot_label(ptitle, plot_title_size(), plot_title_font()).center());
    }
    if plot_has_label(ylabel) && !rotated {
        if type_of(yoff) != "()" {
            // Its baseline `yoff` above the top of the axis, left-aligned.
            parts.push(stack([plot_label(ylabel, asize, afont).at(0, 0).anchor(0, "baseline")])
                .viewbox(0, 0, 1, yoff).height(yoff).y(y1).anchor(0, 1));
        } else if top.len() == 0 {
            parts.push(plot_label(ylabel, asize, afont).at(x0, y1).anchor(0, 1.5));
        } else {
            top.push(plot_label(ylabel, asize, afont));
        }
    }
    if top.len() > 0 {
        let k = if top.len() > 1 { 1.25 } else { 1.5 };
        parts.push(col(top).spacing(apad).stretch().y(y1).anchor(0, k));
    }

    let fb = plot_opt(opts, "frame_box", plot_frame_box());
    if type_of(fb) != "bool" { throw `plot(): frame_box must be true or false (got ${fb})`; }
    let body = stack(parts).viewbox(x0, y1, x1, y0);
    let node = if fb {
        // The node *is* the data frame: no `contain`, so the bands hang
        // outside its box (the layout doesn't clip), and `.at(..)` / sizes
        // set on the plot place and size the axes themselves.
        body
    } else {
        // Everything inside: `contain` shrinks the area until it fits. The
        // note asks `labeled` / `panels` for `plot_label_room()` above it,
        // so a label sits clear of everything — standalone plots keep none.
        col([body.contain().pad(plot_pad()).flex(1)]).stretch().meta("label_room", plot_label_room())
    };
    // A fill under the whole plot box (`plot_bg()`).
    let bg = plot_bg();
    if bg != "" { node = node.fill(bg); }
    let ar = plot_aspect();
    if ar.to_float() > 0.0 { node = node.aspect(ar); }
    node
}

// Minor tick positions of linear axis `ax` (`minor_ticks()` intervals per
// labelled step), skipping the labelled ticks; none for log/category axes.
fn plot_minor(ax, log) { plot_minor(ax, log, minor_ticks(), "") }

// `plot_minor` with `n` intervals per step; `opt` names the plot option it
// came from ("" for the `minor_ticks()` hook), for errors.
fn plot_minor(ax, log, n, opt) {
    if type_of(n) != "i64" || n < 0 {
        if opt == "" { throw `minor_ticks(): must be an integer >= 0 (got ${n})`; }
        throw `plot(): ${opt} must be an integer >= 0 (got ${n})`;
    }
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

// Do the ticks of one axis point in? Plot option `opt` ("xtick_direction" /
// "ytick_direction"), else `dir` (`tick_direction()`).
fn plot_tick_inward(opts, opt, dir) {
    let d = plot_opt(opts, opt, dir);
    if d != "out" && d != "in" { throw `plot(): ${opt} must be "out" or "in" (got "${d}")`; }
    d == "in"
}

// Who set plot setting `key`: "plot(): key" when the option is given, else
// the hook "key()" — for errors.
fn plot_who(opts, key) { if opts.contains(key) { `plot(): ${key}` } else { `${key}():` } }

// Plot length `key` (cm): the option when given, else the hook value `v`,
// as a float; anything but a finite number is an error.
fn plot_len(opts, key, v) {
    let x = plot_opt(opts, key, v);
    if !plot_finite(x) { throw plot_who(opts, key) + ` must be a length in cm (got ${x})`; }
    x.to_float()
}

// An axis-label pad (cm, box to box): `plot_len`, and >= 0 — a label closer
// than its box allows is placed by its baseline with `offset` instead.
fn plot_label_pad(opts, key, v, offset) {
    let x = plot_len(opts, key, v);
    if x < 0.0 {
        throw plot_who(opts, key) + ` must be >= 0 cm (got ${x}); to bring the label closer, place its baseline with ${offset}`;
    }
    x
}

// An axis-label offset: () (follow the tick labels), or a length > 0 (cm
// from the axis to the label's baseline).
fn plot_label_offset(opts, key, v) {
    let x = plot_opt(opts, key, v);
    if type_of(x) == "()" { return x; }
    if !plot_finite(x) || !(x > 0) {
        throw plot_who(opts, key) + ` must be a length > 0 cm from the axis to the label's baseline, or () (got ${x})`;
    }
    x.to_float()
}

// Axis `ax` with its tick labels made by `fmt` (an `xtick_format` /
// `ytick_format` option): a number of decimals (2 -> "0.25", with a real
// minus sign) or a function of the tick value returning its label (e.g.
// `|v| `${v.to_int()} K``); () keeps the automatic labels. Log axes pass the
// value itself (100, not 2).
fn plot_relabel(ax, fmt, log, name) {
    let k = type_of(fmt);
    if k == "()" { return ax; }
    if !ax.marks { throw `plot(): ${name}tick_format needs a numeric axis (not categories)`; }
    if k == "i64" {
        if fmt < 0 || fmt > 12 { throw `plot(): ${name}tick_format decimals must be 0..12 (got ${fmt})`; }
    } else if k != "Fn" {
        throw `plot(): ${name}tick_format must be a number of decimals or a function (got ${k})`;
    }
    ax.labels = [];
    for t in ax.ticks {
        let v = if log { 10.0 ** t } else { t };
        let s = if k == "i64" { format_num(v, fmt) } else { fmt.call(v) };
        ax.labels.push(if type_of(s) == "string" || type_of(s) == "array" { s } else { `${s}` });
    }
    ax
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

// Author line (pt; 40 pt at 24 pt); affiliation numbers are superscript spans.
fn author_size() { style_get("author_size", base_size() * 1.665) }

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

// Where the bar title sits: () (default) — its box centred on the bar (both
// ways), or a length — its first baseline that many cm below the bar top
// (placed with the core baseline anchor, so it is font-exact; still
// centred across). A node title (`section(text(..), ..)`, or a numbered one,
// a row) has no baseline and stays centred by its box.
fn section_title_baseline() { style_get("section_title_baseline", ()) }

// Bar fill (colour or gradient), border colour ("" for none), border width
// (cm, 0.25 pt at 24 pt) and corner radius (cm; square by default).
fn section_bar_fill() {
    style_get("section_bar_fill", linear(90, [[0, "#4a6597"], [1, "#1f3359"]]))
}
fn section_bar_border() { style_get("section_bar_border", "") }
fn section_bar_border_width() { style_get("section_bar_border_width", base_size() * 0.000367) }
fn section_bar_radius() { style_get("section_bar_radius", 0) }

// Effects of the section titles and bars: `false` (default in "poster") for
// none, `true` for the PowerPoint look ("poster-dark"), or a map for your
// own — #{ blur, color } for the glow, #{ dx, dy, blur, spread, color } for
// a shadow (cm; colours with alpha; see the core `.glow` / `.shadow`).
//   section_title_glow    a subtle dark halo hugging the glyphs: blur
//                         0.1 cm, 60 % black (at 24 pt; scales with the text)
//   section_title_shadow  a tight soft drop shadow: (0.04, 0.05) cm, blur
//                         0.08 cm, 70 % black
//   section_bar_shadow    the bars' soft drop shadow: (0.06, 0.07) cm, blur
//                         0.2 cm, 40 % black
fn section_title_glow() { style_get("section_title_glow", false) }
fn section_title_shadow() { style_get("section_title_shadow", false) }
fn section_bar_shadow() { style_get("section_bar_shadow", false) }

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

// How far a column separator sits right of its gutter's centre (cm; negative
// = left; default 0, centred). Per gutter: `separators: [#{ offset: .. }]`.
fn separator_offset() { style_get("separator_offset", 0) }

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

// The bullet mark's size (pt), independent of the text (default the same);
// a mark of another size stays on the first line's baseline.
fn bullet_mark_size() { style_get("bullet_mark_size", bullet_size()) }

// `subheading`: the mark before it ("" for none) and its size (pt; 32 pt at 24).
fn subheading_mark() { style_get("subheading_mark", "■") }
fn subheading_size() { style_get("subheading_size", base_size() * 1.335) }

// The subheading mark on its own: bold (default false: as the text) and its
// size (pt; default the subheading's), e.g. a bold "■" as in PowerPoint.
fn subheading_mark_bold() { style_get("subheading_mark_bold", false) }
fn subheading_mark_size() { style_get("subheading_mark_size", subheading_size()) }

// `block`: the gap between its side-by-side items (cm; the section gap) and
// under its subheading (cm; half that).
fn block_gap() { style_get("block_gap", section_gap()) }
fn block_title_gap() { style_get("block_title_gap", section_gap() * 0.5) }

// The `symbol` font slot of the poster's marks — the subheading mark, the
// bullet mark, the presenter mark: ■ ● ○ • – — and the other "ambiguous"
// symbols in them are set in this family (a name or fallback list) instead
// of following their neighbours (which makes "■ Samples" take the Latin
// face and "■ 試料" the Japanese one). Only those characters of the marks:
// letters, digits and the text keep the page font. "" (default) sets no slot;
// the "poster" presets pick a Japanese gothic, as PowerPoint does for marks
// typed in Japanese (a family that isn't installed is skipped).
fn poster_symbol_font() { style_get("poster_symbol_font", "") }

// `x` (a text node or a span) with `poster_symbol_font()` as its `symbol`
// slot, when one is set.
fn poster_mark_font(x) {
    let f = poster_symbol_font();
    if table_no_font(f) { x } else { x.font(#{ symbol: f }) }
}

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

// Cell padding left and right / top and bottom (cm); both follow `table_pad()`.
fn table_pad_x() { style_get("table_pad_x", table_pad()) }
fn table_pad_y() { style_get("table_pad_y", table_pad()) }
fn table_fill() { style_get("table_fill", "") }
fn table_header_fill() { style_get("table_header_fill", "") }

// Font of the text cells (a family, fallback list or per-script map; "" =
// the page font) and of the header row (default the cells').
fn table_font() { style_get("table_font", "") }
fn table_header_font() { style_get("table_header_font", table_font()) }

// Is font spec `f` unset ("" / [] / ())?
fn table_no_font(f) {
    let k = type_of(f);
    k == "()" || (k == "string" && f == "") || (k == "array" && f.len() == 0)
}

// --- helpers ---

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
fn vsep(w) { poster_vsep(w, separator_inset(), (), separator_offset()) }

// `vsep` from `y0` cm below the top down to `y1` cm below it (() for
// the bottom), `off` cm right of the gutter's centre.
fn poster_vsep(w, y0, y1) { poster_vsep(w, y0, y1, separator_offset()) }
fn poster_vsep(w, y0, y1, off) {
    if !plot_finite(off) { throw `columns(): a separator offset is a length in cm (got ${off})`; }
    let l = if off == 0 {
        line(0.5, 0, 0.5, 1).viewbox(0, 0, 1, 1)
    } else {
        // Across in cm from the gutter's left edge, so the offset is exact.
        if !(w > 0) { throw `columns(): an offset separator needs a gutter wider than 0 (got ${w})`; }
        let x = w / 2.0 + off;
        line(x, 0, x, 1).viewbox(0, 0, w, 1)
    };
    l = l.stroke(separator_color()).stroke_width(separator_width()).dash(separator_dash());
    if type_of(y1) == "()" {
        l = l.flex(1);
    } else {
        if !(y1 > y0) {
            throw `columns(): a separator must end below its start (from ${y0} until ${y1} cm)`;
        }
        l = l.height(y1 - y0);
    }
    col(if y0 > 0 { [rect().height(y0), l] } else { [l] }).stretch().width(w)
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
// `opts` (also as the second argument, `columns(items, #{ .. })`):
//   gutter      the gutter (cm), or one per gutter (default `poster_gutter()`)
//   separators  true / false for all gutters (default `separators()`), or one
//               per gutter: true, false, a number — a partial separator
//               ending that many cm below the top of the row — or
//               #{ start, end, offset } (cm below the top of the row; `start`
//               defaults to `separator_inset()`, the middle of the section
//               bars, `end` to the bottom; `offset` cm right of the gutter
//               centre, default `separator_offset()`). ("from" / "until",
//               quoted, are aliases: `until` is a Rhai keyword.)
fn columns(items) { columns(items, (), #{}) }
fn columns(items, widths) {
    if type_of(widths) == "map" { return columns(items, (), widths); }
    columns(items, widths, #{})
}
fn columns(items, widths, opts) {
    if type_of(items) != "array" {
        throw `columns(): expected an array of nodes (got ${type_of(items)})`;
    }
    poster_check_opts("columns", opts, ["gutter", "separators"]);
    let n = items.len();
    let ws = poster_weights("columns", n, widths);
    let gs = poster_per_gap("columns", "gutter", plot_opt(opts, "gutter", poster_gutter()), n);
    let ss = poster_per_gap("columns", "separators", plot_opt(opts, "separators", separators()), n);
    let uniform = poster_uniform(gs, ss);
    let out = [];
    for i in 0..n {
        if i > 0 && !uniform {
            let s = ss[i - 1];
            let g = gs[i - 1];
            let t = type_of(s);
            out.push(if t == "bool" {
                if s { vsep(g) } else { rect().width(g) }
            } else if t == "i64" || t == "f64" {
                poster_vsep(g, separator_inset(), s)
            } else if t == "map" {
                for k in s.keys() {
                    if !["start", "end", "offset", "from", "until"].contains(k) {
                        throw `columns(): unknown separator key "${k}" (start, end, offset)`;
                    }
                }
                let y0 = plot_opt(s, "start", plot_opt(s, "from", separator_inset()));
                poster_vsep(g, y0, plot_opt(s, "end", plot_opt(s, "until", ())),
                            plot_opt(s, "offset", separator_offset()))
            } else {
                throw `columns(): a separator is true, false, an end (cm) or #{ start, end, offset } (got ${t})`;
            });
        }
        out.push(poster_sized("columns", as_node(items[i]), ws[i], false));
    }
    row(out).spacing(if uniform && gs.len() > 0 { gs[0] } else { 0 }).stretch()
}

// Rows of sections (or `columns`) down the page, sharing the height equally
// or by `heights` (flex weights, "12cm", "auto"), `poster_row_gap()` apart (a
// dashed `hsep` in each gap while `row_separators()` is on). `opts` (also as
// the second argument):
//   gaps        the gap (cm), or one per gap (default `poster_row_gap()`)
//   separators  true / false, or one per gap (default `row_separators()`)
fn rows(items) { rows(items, (), #{}) }
fn rows(items, heights) {
    if type_of(heights) == "map" { return rows(items, (), heights); }
    rows(items, heights, #{})
}
fn rows(items, heights, opts) {
    if type_of(items) != "array" {
        throw `rows(): expected an array of nodes (got ${type_of(items)})`;
    }
    poster_check_opts("rows", opts, ["gaps", "separators"]);
    let n = items.len();
    let hs = poster_weights("rows", n, heights);
    let gs = poster_per_gap("rows", "gaps", plot_opt(opts, "gaps", poster_row_gap()), n);
    let ss = poster_per_gap("rows", "separators", plot_opt(opts, "separators", row_separators()), n);
    for s in ss {
        if type_of(s) != "bool" { throw `rows(): separators are true or false (got ${type_of(s)})`; }
    }
    let uniform = poster_uniform(gs, ss);
    let out = [];
    for i in 0..n {
        if i > 0 && !uniform {
            out.push(if ss[i - 1] { hsep(gs[i - 1]) } else { rect().height(gs[i - 1]) });
        }
        out.push(poster_sized("rows", as_node(items[i]), hs[i], true));
    }
    col(out).spacing(if uniform && gs.len() > 0 { gs[0] } else { 0 }).stretch()
}

// `opts` checked against the option names `keys` of `who`.
fn poster_check_opts(who, opts, keys) {
    if type_of(opts) != "map" { throw `${who}(): options must be a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        if !keys.contains(k) { throw `${who}(): unknown option "${k}" (${list_names(keys)})`; }
    }
}

// Option `key` of `who` for the gaps between `n` items: one value for all,
// or an array of n - 1.
fn poster_per_gap(who, key, v, n) {
    let m = if n > 0 { n - 1 } else { 0 };
    if type_of(v) == "array" {
        if v.len() != m { throw `${who}(): ${n} items have ${m} gaps but ${key} has ${v.len()} values`; }
        return v;
    }
    let out = [];
    for i in 0..m { out.push(v); }
    out
}

// Can the gaps be the container's spacing: every gap the same, and either
// all with a separator (`true`) or none?
fn poster_uniform(gs, ss) {
    if gs.len() == 0 { return true; }
    for g in gs { if g != gs[0] { return false; } }
    for s in ss { if type_of(s) != "bool" || s { return false; } }
    true
}

// --- sections ---

// The title text of a section bar: `s` (a string or spans) at
// `section_title_size()` in `section_title_color()` (`section_title_font()`),
// centred — without effects (the bar adds those, see `section_bar`). Chain
// setters on it for a one-off title: `section(section_title("A").color(..), ..)`.
fn section_title(s) {
    let t = text(s).size(section_title_size()).color(section_title_color()).center();
    let f = section_title_font();
    if f == "" { t } else { t.font(f) }
}

// A section bar: `s` centred (both ways) on a `section_bar_height()` bar
// filled with `section_bar_fill()`, bordered in `section_bar_border()`, with
// `section_bar_shadow()`. `s` is a string or spans (`section_title(s)`) or a
// node used as is; the title gets `section_title_glow()` /
// `section_title_shadow()` (a node title through a row around it, so effects
// it carries itself stay). The title is anchored on the bar's centre in a
// unit viewbox, so it stays centred and keeps its size even when its line
// box (a CJK font's tall ascent + descent) is taller than the bar.
fn section_bar(s) {
    let t = if type_of(s) == "Node" { row([s]) } else { section_title(s) };
    t = poster_effect("section_title_glow", t, "glow", section_title_glow(), section_title_glow_look());
    t = poster_effect("section_title_shadow", t, "shadow", section_title_shadow(), section_title_shadow_look());
    let h = section_bar_height();
    let b = section_title_baseline();
    let bar = if type_of(b) == "()" || type_of(s) == "Node" {
        stack([t.at(0.5, 0.5).anchor(0.5, 0.5)]).viewbox(0, 0, 1, 1)
    } else {
        // The title's first baseline `b` cm below the bar top (cm down,
        // fractions across), centred across.
        if !plot_finite(b) || !(h > 0) {
            throw `section_title_baseline(): must be a length in cm below the bar top, or () (got ${b})`;
        }
        stack([t.at(0.5, b).anchor(0.5, "baseline")]).viewbox(0, 0, 1, h)
    };
    bar = bar.height(h);
    let fill = section_bar_fill();
    if poster_has_paint(fill) { bar = bar.fill(fill); }
    let b = section_bar_border();
    if b != "" { bar = bar.stroke(b).stroke_width(section_bar_border_width()); }
    let r = section_bar_radius();
    if r > 0 { bar = bar.radius(r); }
    poster_effect("section_bar_shadow", bar, "shadow", section_bar_shadow(), section_bar_shadow_look())
}

// The `true` looks of the section effects (cm, scaled from 24 pt).
fn section_title_glow_look() { #{ blur: base_size() * 0.1 / 24.0, color: "#00000099" } }
fn section_title_shadow_look() {
    let k = base_size() / 24.0;
    #{ dx: 0.04 * k, dy: 0.05 * k, blur: 0.08 * k, color: "#000000b3" }
}
fn section_bar_shadow_look() {
    let k = base_size() / 24.0;
    #{ dx: 0.06 * k, dy: 0.07 * k, blur: 0.2 * k, color: "#00000066" }
}

// `n` with the effect `kind` ("glow" or "shadow") of hook `who`'s value `v`:
// false / () / "" for none, true for `look`, or a map (glow: blur, color;
// shadow: dx, dy, blur, spread, color).
fn poster_effect(who, n, kind, v, look) {
    let t = type_of(v);
    if t == "()" || (t == "bool" && !v) || (t == "string" && v == "") { return n; }
    if t == "bool" { v = look; t = "map"; }
    if t != "map" { throw `${who}(): expected false, true or a map (got ${t})`; }
    let keys = if kind == "glow" { ["blur", "color"] } else { ["dx", "dy", "blur", "spread", "color"] };
    for k in v.keys() {
        if !keys.contains(k) { throw `${who}(): unknown key "${k}" (${list_names(keys)})`; }
    }
    if kind == "glow" {
        return n.glow(plot_opt(v, "blur", 0), plot_opt(v, "color", "#000000"));
    }
    n.shadow(plot_opt(v, "dx", 0), plot_opt(v, "dy", 0), plot_opt(v, "blur", 0),
             plot_opt(v, "spread", 0), plot_opt(v, "color", "#00000080"))
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
// continues from it) or a string used as is ("" for none). The title is a
// string, spans (`markup(..)`) or a text node (its own look; the number
// goes before it in the bar's title style).
// `section(title, content, opts)` / `section(number, title, content, opts)`:
//   footer  item(s) pushed to the bottom of the section (references, a
//           take-home `note`), below the content's spare height and at least
//           `section_gap()` under it
fn section(title, content) { section_make(section_number(()), title, content, #{}) }
fn section(a, b, c) {
    if type_of(c) == "map" { return section_make(section_number(()), a, b, c); }
    section(a, b, c, #{})
}
fn section(number, title, content, opts) {
    if type_of(number) == "i64" { return section_make(section_number(number), title, content, opts); }
    if type_of(number) != "string" {
        throw `section(): the number must be an integer or a string (got ${type_of(number)})`;
    }
    section_make(number, title, content, opts)
}

fn section_make(n, title, content, opts) {
    poster_check_opts("section", opts, ["footer"]);
    let node = type_of(title) == "Node";
    if type_of(title) != "string" && !node && !poster_is_rich(title) {
        throw `section(): the title must be a string, spans or a text node (got ${type_of(title)})`;
    }
    let label = if type_of(n) == "string" { n } else {
        let st = section_label_style();
        if st == "" { "" } else { panel_label(st, n - 1) }
    };
    let s = if label == "" { title } else if node {
        // The number in the title style, a space (0.28 em) before the node.
        row([section_title(label), title]).cross("center")
            .spacing(section_title_size() * 0.28 * 2.54 / 72.0)
    } else { poster_prefix(`${label} `, title) };
    let items = if type_of(content) == "array" { nodes(content) } else { [as_node(content)] };
    let footer = plot_opt(opts, "footer", ());
    if type_of(footer) != "()" {
        // The content takes the spare height, so the footer ends the section.
        let fs = if type_of(footer) == "array" && !poster_is_rich(footer) { nodes(footer) }
                 else { [scheme_text("section", footer, base_size(), "start")] };
        items = [col(items).spacing(section_gap()).stretch().flex(1),
                 col(fs).spacing(section_gap()).stretch()];
    }
    let body = col(items).spacing(section_gap()).stretch().pad(section_pad()).flex(1);
    col([section_bar(s), body]).stretch()
}

// `block(title, items)` / `block(title, items, opts)`: a `subheading(title)`
// (a node title is used as is; "" for none) over `items` side by side —
// equal flex shares by default — `block_title_gap()` below the heading and
// `block_gap()` apart. `items` is an array (strings become `body`) or one
// node. Options:
//   widths     per item: flex weights, "4cm" lengths or "auto"
//   gap        between the items (cm)
//   title_gap  between the heading and the items (cm)
//   align      the items across: "start" (default, tops), "center", "end",
//              "stretch", "baseline"
fn block(title, items) { block(title, items, #{}) }
fn block(title, items, opts) {
    poster_check_opts("block", opts, ["widths", "gap", "title_gap", "align"]);
    let xs = if type_of(items) == "array" { items } else { [items] };
    let ws = poster_weights("block", xs.len(), plot_opt(opts, "widths", ()));
    let cells = [];
    for i in 0..xs.len() { cells.push(poster_sized("block", as_node(xs[i]), ws[i], false)); }
    let align = plot_opt(opts, "align", "start");
    if !["start", "center", "end", "stretch", "baseline"].contains(align) {
        throw `block(): unknown align "${align}" (start, center, end, stretch, baseline)`;
    }
    let r = row(cells).spacing(plot_opt(opts, "gap", block_gap())).cross(align);
    let none = type_of(title) == "()" || (type_of(title) == "string" && title == "");
    if none { return r; }
    let head = if type_of(title) == "Node" { title } else { subheading(title) };
    col([head, r]).spacing(plot_opt(opts, "title_gap", block_title_gap())).stretch()
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

// A small heading inside a section: "■ " + `s` at `subheading_size()`; the
// mark bold (`subheading_mark_bold()`) and sized (`subheading_mark_size()`)
// on its own when those differ from the text (one text node of spans).
fn subheading(s) {
    let m = subheading_mark();
    let size = subheading_size();
    if m == "" { return fg_text(s).size(size); }
    let bold = subheading_mark_bold();
    let ms = subheading_mark_size();
    let sym = !table_no_font(poster_symbol_font());
    if !bold && ms == size && !sym { return fg_text(poster_prefix(`${m} `, s)).size(size); }
    let mark = poster_mark_font(span(`${m} `));
    if bold { mark = mark.bold(); }
    if ms != size { mark = mark.size(ms); }
    fg_text([mark] + (if type_of(s) == "string" { [span(s)] } else { s })).size(size)
}

// A bulleted list with a hanging indent: each item (a string or a node) in a
// cell `bullet_indent()` right of its `bullet_mark()`, so wrapped lines align
// with the first. An array item is a nested list under the previous item.
// Each item is a baseline-aligned row (`cross("baseline")`), so the mark sits
// on the first line's baseline — measured on the shaped text, exact for any
// font and for a mark of another size or font (`bullet_mark_size()`,
// `poster_symbol_font()`); a node item uses its first text's baseline (one
// without text sits on the mark's baseline by its bottom edge).
fn bullets(items) {
    if type_of(items) != "array" {
        throw `bullets(): expected an array of items (got ${type_of(items)})`;
    }
    let ind = bullet_indent();
    let size = bullet_size();
    let ms = bullet_mark_size();
    let out = [];
    for x in items {
        if type_of(x) == "array" && !poster_is_rich(x) {
            out.push(row([rect().width(ind), bullets(x).flex(1)]).stretch());
            continue;
        }
        let item = if type_of(x) == "string" || poster_is_rich(x) { fg_text(x).size(size) } else { x };
        let mark = poster_mark_font(fg_text(bullet_mark()).size(ms)).width(ind);
        out.push(row([mark, item.flex(1)]).cross("baseline"));
    }
    col(out).spacing(bullet_gap()).stretch()
}

// `table(rows)` / `table(rows, opts)`: `rows` is an array of arrays of cells
// (strings, numbers, spans or nodes; `colspan(n, x)` spans n columns), all
// as wide in columns; the first row is the header (bold, above a
// `table_header_rule_width()` rule). Options:
//   header  false: no header row (default true)
//   widths  per column: flex weights, "4cm" lengths or "auto" (default equal)
//   align   "left" (default), "center", "right", or one per column
//   rules   "grid", "rows", "booktabs" or "none" (default `table_rules()`)
//   pad     cell padding, cm (default `table_pad()`); pad_x / pad_y for
//           left-right / top-bottom (default `table_pad_x()` / `table_pad_y()`)
//   font    the font of every text cell (a family, a fallback list or a
//           per-script map; default `table_font()`, header `table_header_font()`)
//   header_font  the header row's font (default `font`, else `table_header_font()`)
//   fonts   one font per column ("" / () for the table's), over `font`
// The rules are filled rects between the cells (`table_rule()`,
// `table_rule_width()`), so the grid is exact at any size; cells are filled
// `table_fill()` / `table_header_fill()`. Node cells keep their own font.
fn table(rows) { table(rows, #{}) }
fn table(rows, opts) {
    poster_check_opts("table", opts, ["header", "widths", "align", "rules", "pad", "pad_x", "pad_y",
                                      "font", "header_font", "fonts"]);
    if type_of(rows) != "array" || rows.len() == 0 { throw "table(): expected a non-empty array of rows"; }
    let nc = 0;
    for r in rows {
        if type_of(r) != "array" { throw `table(): each row must be an array of cells (got ${type_of(r)})`; }
        let k = 0;
        for c in r { k += table_span(c); }
        if nc == 0 { nc = k; }
        if k != nc || nc == 0 { throw `table(): rows differ in length (${nc} vs ${k} columns)`; }
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
    let pad = plot_opt(opts, "pad", ());
    let px = plot_opt(opts, "pad_x", if type_of(pad) == "()" { table_pad_x() } else { pad });
    let py = plot_opt(opts, "pad_y", if type_of(pad) == "()" { table_pad_y() } else { pad });
    // Fonts: per column, then the table's `font`, then the hooks.
    let cf = plot_opt(opts, "fonts", ());
    if type_of(cf) != "()" && (type_of(cf) != "array" || cf.len() != nc) {
        throw `table(): fonts must be one font per column (${nc}; "" for the table font)`;
    }
    let body_font = plot_opt(opts, "font", table_font());
    let head_font = plot_opt(opts, "header_font", plot_opt(opts, "font", table_header_font()));
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
        let j = 0;
        for x in rows[i] {
            let k = table_span(x);
            let content = if type_of(x) == "map" { x.content } else { x };
            let font = if head { head_font } else { body_font };
            if type_of(cf) != "()" && !table_no_font(cf[j]) && !(head && opts.contains("header_font")) {
                font = cf[j];
            }
            let cell = table_cell(content, head, al[j], px, py, font);
            if k == 1 {
                cells.push(poster_sized("table", cell, ws[j], false));
            } else {
                cells += table_spanned(cell, ws, j, k, if vertical { rw } else { 0.0 }, table_cell_fill(head));
            }
            j += k;
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

// A cell spanning `n` columns: `#{ colspan: n, content: x }` in a table row.
fn colspan(n, x) {
    if type_of(n) != "i64" || n < 1 { throw `colspan(): n must be a positive integer (got ${n})`; }
    #{ colspan: n, content: x }
}

// How many columns table cell `x` spans.
fn table_span(x) {
    if type_of(x) != "map" { return 1; }
    if !x.contains("colspan") || !x.contains("content") {
        throw "table(): a map cell is #{ colspan, content } (see colspan(n, x))";
    }
    x.colspan
}

// `cell` sized over columns j .. j + k - 1 of widths `ws`: all lengths (cm)
// → their sum plus the `rw` rules it covers; all flex weights → their sum,
// with the covered rules' width as filled blanks either side (fixed, so the
// flex shares match the rows without a span).
fn table_spanned(cell, ws, j, k, rw, fill) {
    let sum = 0.0;
    let cm = 0;
    for c in j..(j + k) {
        let w = ws[c];
        let v = poster_cm("table", w);
        if type_of(v) != "()" { sum += v; cm += 1; }
        else if type_of(w) == "i64" || type_of(w) == "f64" { sum += w.to_float(); }
        else { throw "table(): a colspan can't cover \"auto\" columns"; }
    }
    let extra = rw * (k - 1).to_float();
    if cm == k { return [cell.width(sum + extra)]; }
    if cm > 0 { throw "table(): a colspan must cover only lengths or only flex weights"; }
    if extra == 0.0 { return [cell.flex(sum)]; }
    let blank = rect().width(extra / 2.0);
    if poster_has_paint(fill) { blank = blank.fill(fill); }
    [blank, cell.flex(sum), blank]
}

// The fill of a header / body cell ("" for none).
fn table_cell_fill(head) {
    let f = if head { table_header_fill() } else { "" };
    if poster_has_paint(f) { f } else { table_fill() }
}

// One table cell: `x` (a string or number becomes text, bold in the header)
// padded `table_pad_x()` / `table_pad_y()` (or `px` / `py`), vertically
// centred, aligned `align`, filled `table_header_fill()` (header) or
// `table_fill()`.
fn table_cell(x, head, align) { table_cell(x, head, align, table_pad_x(), table_pad_y()) }
fn table_cell(x, head, align, px, py) {
    table_cell(x, head, align, px, py, if head { table_header_font() } else { table_font() })
}
fn table_cell(x, head, align, px, py, font) {
    let t = type_of(x);
    let n = if t == "string" || t == "i64" || t == "f64" || poster_is_rich(x) {
        let s = fg_text(if poster_is_rich(x) { x } else { `${x}` }).size(table_size()).align(align);
        s = plot_font_on(s, font);
        if head { s.bold() } else { s }
    } else { x };
    // One padding on all sides; the larger axis gets the rest as spacers.
    let p = if px < py { px } else { py };
    let c = col([n]).pad(p).main("center").stretch();
    if px > py {
        let s = rect().width(px - py);
        c = row([s, c.flex(1), s]).stretch();
    } else if py > px {
        let s = rect().height(py - px);
        c = col([s, c.flex(1), s]).stretch();
    }
    let f = table_cell_fill(head);
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
        if type_of(a) == "string" || poster_is_rich(a) {
            runs = runs_push(runs, a, false);
            continue;
        }
        if type_of(a) != "map" || !a.contains("name") {
            throw `poster_header(): an author is a name or #{ name, aff, presenter } (got ${type_of(a)})`;
        }
        if plot_opt(a, "presenter", false) {
            let m = presenter_mark();
            // A mark of its own (a span in `poster_symbol_font()`) when set.
            runs = runs_push(runs, if m == "" || table_no_font(poster_symbol_font()) { m }
                                   else { [poster_mark_font(span(m))] }, false);
        }
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

// `runs` with run [s, sup] appended (merged into a preceding plain run). `s`
// is a string or rich text (spans, kept as a run of its own).
fn runs_push(runs, s, sup) {
    if type_of(s) == "array" {
        if s.len() > 0 { runs.push([s, sup]); }
        return runs;
    }
    if s == "" { return runs; }
    let n = runs.len();
    if n > 0 && !sup && !runs[n - 1][1] && type_of(runs[n - 1][0]) == "string" {
        runs[n - 1][0] += s;
    } else {
        runs.push([s, sup]);
    }
    runs
}

// One centred text node of `runs` at `size` pt: plain runs as spans, the
// affiliation numbers as superscript spans (the core script size and
// shift), so the line wraps and breaks like any text. Rich runs keep their
// spans (strings among them become plain spans).
fn runs_line(runs, size) {
    let spans = [];
    for r in runs {
        if type_of(r[0]) == "array" {
            for x in r[0] { spans.push(if type_of(x) == "string" { span(x) } else { x }); }
        } else {
            spans.push(if r[1] { span(r[0]).sup() } else { span(r[0]) });
        }
    }
    fg_text(spans).size(size).center()
}

// The author line(s) of a poster header. `affiliation_position()` "before":
// "（Affil. A¹・Affil. B²）○Author¹・Author²" on one line; "below": the
// authors, and the numbered affiliations a size smaller under them. Names
// and affiliations are strings or rich text (`markup("…")`, span arrays):
// e.g. a name in another font, an affiliation with an italic part.
fn author_line(authors, affiliations) {
    if type_of(authors) != "array" || poster_is_rich(authors) { authors = [authors]; }
    if type_of(affiliations) != "array" || poster_is_rich(affiliations) { affiliations = [affiliations]; }
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
// Text (titles, items, cells) may be a string or rich-text spans
// (`markup("…")`, `[span(..), ..]`) throughout the poster components.
// The title (with the number at its right end) and the author line are
// centred in the space right of the logos; a `poster_header_rule()` runs
// under it all.
fn poster_header(opts) {
    if type_of(opts) != "map" { throw `poster_header(): expected a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        if !["title", "authors", "affiliations", "logos", "number"].contains(k) {
            throw `poster_header(): unknown key "${k}" (title, authors, affiliations, logos, number)`;
        }
    }
    let title = plot_opt(opts, "title", "");
    let tnode = if type_of(title) != "string" && !poster_is_rich(title) {
        title
    } else if type_of(title) != "string" || title != "" {
        let t = fg_text(title).size(poster_title_size()).bold().center();
        let f = poster_title_font();
        if f == "" { t } else { t.font(f) }
    } else { () };
    // The number at the right end of the title row, so a long title wraps
    // before it instead of running under it, while the author line below
    // keeps the full width.
    let num = plot_opt(opts, "number", "");
    if num != "" {
        let n = fg_text(num).size(poster_number_size()).bold();
        let t = if type_of(tnode) == "()" { rect() } else { tnode };
        tnode = row([t.flex(1), n]).cross("start").spacing(poster_gutter());
    }
    let main = if type_of(tnode) == "()" { [] } else { [tnode] };
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
