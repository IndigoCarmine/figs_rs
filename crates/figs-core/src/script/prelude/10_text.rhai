// Semantic text (`body`, `title`, `caption`, `label`, `rich`, `halo`) and the
// text builders components share (scheme / overlay / side text).
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

// Overlay text: a string (inline markup) or spans in the overlay text style
// (`overlay_text_size()`, `overlay_text_color()`, a `overlay_text_halo()`),
// aligned `align` ("start", "center", "end"); a node passes through.
fn overlay_text(who, x, align) {
    let t = type_of(x);
    if t == "string" || poster_is_rich(x) {
        let n = text(if t == "string" { markup(x) } else { x }).size(overlay_text_size());
        let c = overlay_text_color();
        if c != "" { n = n.color(c); }
        n = if align == "center" { n.center() } else if align == "end" { n.align("right") } else { n };
        let h = overlay_text_halo();
        return if h == "" { n } else { halo(n, h, overlay_text_halo_width()) };
    }
    if t != "Node" {
        throw `${who}(): expected a string, spans or a node (got ${t})`;
    }
    x
}
