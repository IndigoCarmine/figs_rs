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
// Logos in boxes of one size ([w, h] cm), each fitted and centred in its
// box, so logos of different shapes line up (() = each at
// `poster_logo_height()`, its own width).
fn poster_logo_box() { style_get("poster_logo_box", ()) }
// The poster number's distance in from the right edge of the header (cm).
fn poster_number_inset() { style_get("poster_number_inset", 0) }

// Clear space (cm) either side of the poster title — a number, or
// [left, right] — so a long title wraps short of the logos / number.
fn poster_title_inset() { style_get("poster_title_inset", 0) }

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

// Where the bar title sits: () (default) — optically centred on the bar: the
// middle of its first line's capitals (the core "cap_middle" anchor, from
// the font's cap height) on the bar's middle, centred across, so a Latin
// title set in a face with tall CJK metrics doesn't sit low; or a length —
// its first baseline that many cm below the bar top (the core baseline
// anchor, font-exact; still centred across). A node title
// (`section(text(..), ..)`, or a numbered one, a row) is centred by the cap
// middle of its first text (by its box when it has none) by default, and by
// its box with a length.
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
// Extra space (cm) left and right of a section's content, over `section_pad()`.
fn section_body_inset() { style_get("section_body_inset", 0) }
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
fn ref_font() { style_get("ref_font", "") }

// What a section does when its content doesn't fit the height it gets (a
// share of a fixed poster): "visible" (default: it runs on, with a warning
// naming the section), "clip", "shrink" (scaled down to fit; the core
// `overflow`) or "fit" (scaled down to fit, or up to fill the body). A section with a footer clips by default, so the footer is
// never overlapped. `section(.., #{ overflow })` sets it for one section.
fn section_overflow() { style_get("section_overflow", "visible") }
// Where a section's content sits in the height it gets: "top" (default),
// "center", "bottom" or "spread" (the items spaced out to fill it).
fn section_valign() { style_get("section_valign", "top") }

// Where `poster_header`'s poster number goes: "title" (default: at the right
// end of the title row) or "above" (a line of its own above the title, right
// aligned, so a long title keeps the full width).
fn poster_number_position() { style_get("poster_number_position", "title") }

// The author / affiliation lines' font ("" = the page font).
fn author_font() { style_get("author_font", "") }

// Named style variants per component (name -> map of style keys; see
// `variant_push`): `frame(x, "box")`, `note(s, "quiet")`, …
fn frame_variants() { style_get("frame_variants", #{}) }
fn note_variants() { style_get("note_variants", #{}) }
fn ref_variants() { style_get("ref_variants", #{}) }
fn table_variants() { style_get("table_variants", #{}) }
fn subheading_variants() { style_get("subheading_variants", #{}) }
fn section_variants() { style_get("section_variants", #{}) }

// `bullets`: mark, text size (pt), hanging indent (cm; 1.58 cm at 36 pt) and
// gap between items (cm).
fn bullet_mark() { style_get("bullet_mark", "•") }
fn bullet_size() { style_get("bullet_size", base_size()) }
fn bullet_indent() { style_get("bullet_indent", bullet_size() * 0.044) }
fn bullet_gap() { style_get("bullet_gap", bullet_size() * 0.004) }
// Space (cm) left of a bulleted list's marks (nested lists: per level).
fn bullet_inset() { style_get("bullet_inset", 0) }

// The bullet mark's size (pt), independent of the text (default the same);
// a mark of another size stays on the first line's baseline.
fn bullet_mark_size() { style_get("bullet_mark_size", bullet_size()) }

// The distance (cm) between the first baselines of consecutive items, which
// then replaces `bullet_gap()` (the core column `baseline_pitch`): a list on
// a fixed line grid whatever its marks, fonts and wrapped lines; () (default)
// for none. `bullets(items, #{ pitch })` sets it for one list.
fn bullet_pitch() { style_get("bullet_pitch", ()) }

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
// ("" for none). `table_rule_mode()`: "space" (default) — each rule is a
// filled strip between the rows / cells, taking its width; "overlay" — the
// rules are drawn over the cell boundaries (inner ones centred on them, the
// outer frame just inside the table's edge), so they take no space: the
// rows sit edge to edge and the table is as tall (wide) as its cells.
fn table_size() { style_get("table_size", base_size()) }
fn table_rule() {
    let f = fg();
    style_get("table_rule", if f == "" { "#000000" } else { f })
}
fn table_rule_width() { style_get("table_rule_width", base_size() * 0.00147) }
fn table_header_rule_width() { style_get("table_header_rule_width", table_rule_width() * 3.0) }
fn table_rules() { style_get("table_rules", "grid") }
fn table_rule_mode() { style_get("table_rule_mode", "space") }
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
