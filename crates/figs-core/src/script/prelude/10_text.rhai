// ---------------------------------------------------------------------------
// Semantic text (`body`, `title`, `caption`, `label`, `rich`, `halo`) and the
// text builders components share (scheme / overlay / side / caption text)
// ---------------------------------------------------------------------------

// `text(s)` in the theme's `fg()` colour (no colour set while it is "").
fn fg_text(s) { colored(text(s), fg()) }

// `t` in colour `c`, or as is while `c` is "" (no colour of its own).
fn colored(t, c) { if c == "" { t } else { t.color(c) } }

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
// result is running text in the theme (`body(markup(s))`: `base_size()`,
// `fg()`), an ordinary core text node of spans, so every text setter chains:
// `rich("ΔG_{300K}").size(12)`. The other semantic helpers take markup the
// same way: `caption(markup("H_2O"))`.
fn rich(s) { body(markup(s)) }

// `t` (a text node) with a halo: the core text outline (`stroke`) in `color`,
// reaching `r` (cm) past the glyph edges with round joins. The outline is
// painted behind the glyphs, so they keep their exact shape, and it is purely
// visual: the result lays out exactly like `t` alone (same box, same corner
// placement). Works for any overlay text: `halo(body("50 °C"), "#000000", 0.03)`.
fn halo(t, color, r) { t.stroke(color).stroke_width(2.0 * r) }

// `halo(t, color, r)`, or `t` as is while `color` is "" (no halo).
fn halo_if(t, color, r) { if color == "" { t } else { halo(t, color, r) } }

// Text node `n` aligned "start" (as is), "center" or "end" (right-aligned).
fn align_text(n, align) {
    if align == "center" { n.center() } else if align == "end" { n.align("right") } else { n }
}

// `x` when it is a node; else an error of function `who` (`what` describes
// what `x` may be).
fn node_or_throw(who, x, what) {
    if type_of(x) != "Node" { throw `${who}(): ${what} (got ${type_of(x)})`; }
    x
}

// Scheme / overlay / side text: a string or spans becomes text at `size`
// (pt) in `fg()`, aligned `align` ("start", "center", "end"); a node passes
// through; anything else is an error of function `who`.
fn text_or_node(who, x, size, align) {
    if is_text(x) { return align_text(fg_text(x).size(size), align); }
    node_or_throw(who, x, "expected a string, spans or a node")
}

// Overlay text: a string (inline markup) or spans in the overlay text style
// (`overlay_text_size()`, `overlay_text_color()`, a `overlay_text_halo()`),
// aligned `align` ("start", "center", "end"); a node passes through.
fn overlay_text(who, x, align) {
    if is_text(x) {
        let n = text(if type_of(x) == "string" { markup(x) } else { x }).size(overlay_text_size());
        n = align_text(colored(n, overlay_text_color()), align);
        return halo_if(n, overlay_text_halo(), overlay_text_halo_width());
    }
    node_or_throw(who, x, "expected a string, spans or a node")
}
