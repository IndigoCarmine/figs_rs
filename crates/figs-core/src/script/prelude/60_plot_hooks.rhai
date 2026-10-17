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
// "fill": no aspect — the plot takes the spare height of its column
// (`.flex(1)`, at least `plot_min_height()`), so in a section that must
// grow or shrink the plots absorb the difference (see `section(.., #{ fit:
// "flex" })`); wrappers (`labeled`, `captioned`, `overlay`, `annotate`)
// pass the fill on.
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
// their tick as `ytick_label_align()` says; category names (which wrap in
// their slots) by box.
fn tick_label_pad_mode() { style_get("tick_label_pad_mode", "box") }

// How a y tick label is centred on its tick: "cap_middle" (default) — the
// middle of its digits' height (the core "cap_middle" anchor: half the
// font's cap height above the baseline, which digits share in lining
// figures), so the number looks centred whatever the font's ascent and
// descent; "box" — the middle of its line box (before 2026-10).
fn ytick_label_align() { style_get("ytick_label_align", "cap_middle") }

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

// Plots with `plot_aspect()` "fill": the least height (cm) they shrink to.
fn plot_min_height() { style_get("plot_min_height", base_size() * 0.25) }

// `plot_shift_arrows`: the part of each arrow's length left off at both
// ends (a fraction), so heads stop short of the curves.
fn shift_arrow_trim() { style_get("shift_arrow_trim", 0.06) }
// Arrows shorter than this (data units of y) are left out (default 0: all
// drawn); where they start: "gap" (default: `trim` off both ends) or
// "curve" (on the first series, only the head end trimmed).
fn shift_arrow_min_length() { style_get("shift_arrow_min_length", 0) }
fn shift_arrow_anchor() { style_get("shift_arrow_anchor", "gap") }

// The band an `inset(.., #{ avoid: "bottom" })` keeps clear along an edge
// (cm): one line of overlay text and its insets — a caption strip.
fn inset_avoid_band() {
    style_get("inset_avoid_band", overlay_text_size() * 1.25 * 2.54 / 72.0 + overlay_inset() * 2.0)
}

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
