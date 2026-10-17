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
// unit viewbox by its cap middle (see `section_title_baseline`), so it stays
// centred and keeps its size even when its line box (a CJK font's tall
// ascent + descent) is taller than the bar.
fn section_bar(s) {
    let t = if type_of(s) == "Node" { row([s]) } else { section_title(s) };
    t = poster_effect("section_title_glow", t, "glow", section_title_glow(), section_title_glow_look());
    t = poster_effect("section_title_shadow", t, "shadow", section_title_shadow(), section_title_shadow_look());
    let h = section_bar_height();
    let b = section_title_baseline();
    let bar = if type_of(b) == "()" {
        stack([t.at(0.5, 0.5).anchor(0.5, "cap_middle")]).viewbox(0, 0, 1, 1)
    } else if type_of(s) == "Node" {
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
//   overflow  "visible", "clip", "shrink" or "fit" (default
//           `section_overflow()`; see there)
//   fit     "both": the content is scaled up — or down — uniformly to fill
//           the body on one axis, centred (= overflow "fit"); "shrink":
//           only down (= overflow "shrink"); "none"
//   valign  where the content sits in the body: "top", "center", "bottom",
//           "spread" (default `section_valign()`; "center" with fit "both")
//   variant a style variant (`section_variants`)
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
    poster_check_opts("section", opts, ["footer", "overflow", "variant", "fit", "valign"]);
    if opts.contains("variant") {
        variant_push("section", opts.variant);
        opts.remove("variant");
        let s = section_make(n, title, content, opts);
        style_pop();
        return s;
    }
    let node = type_of(title) == "Node";
    if type_of(title) != "string" && !node && !poster_is_rich(title) {
        throw `section(): the title must be a string, spans or a text node (got ${type_of(title)})`;
    }
    let label = if type_of(n) == "string" { n } else {
        let st = section_label_style();
        if st == "" { "" } else { panel_label(st, n - 1) }
    };
    let s = if label == "" { title } else if node {
        // The number in the title style, a space (0.28 em) before the node,
        // the two optically centred on each other.
        row([section_title(label), title]).cross("cap_middle")
            .spacing(section_title_size() * 0.28 * 2.54 / 72.0)
    } else { poster_prefix(`${label} `, title) };
    // The name overflow warnings give the section (and its parts).
    let tag = if type_of(title) == "string" { `section "${title}"` }
              else if label != "" { `section ${label}` } else { "section" };
    let ov = plot_opt(opts, "overflow", section_overflow());
    if !["visible", "clip", "shrink", "fit"].contains(ov) {
        throw `section(): unknown overflow "${ov}" (visible, clip, shrink, fit)`;
    }
    if opts.contains("fit") {
        let f = opts.fit;
        // "flex": the flexible items (plots with plot_aspect "fill") take
        // the spare height or give it up; only past their minimum is the
        // content shrunk uniformly.
        let modes = #{ both: "fit", shrink: "shrink", none: "visible", flex: "shrink" };
        if type_of(f) != "string" || !modes.contains(f) {
            throw `section(): unknown fit "${f}" (both, shrink, flex, none)`;
        }
        if opts.contains("overflow") {
            throw "section(): give fit or overflow, not both";
        }
        ov = modes[f];
        opts.overflow = ov;
    }
    let va = plot_opt(opts, "valign", if ov == "fit" && !opts.contains("valign") && section_valign() == "top" { "center" } else { section_valign() });
    let valigns = #{ top: "start", center: "center", bottom: "end", spread: "space_between" };
    if type_of(va) != "string" || !valigns.contains(va) {
        throw `section(): unknown valign "${va}" (top, center, bottom, spread)`;
    }
    let main = valigns[va];
    let items = if type_of(content) == "array" { nodes(content) } else { [as_node(content)] };
    let footer = plot_opt(opts, "footer", ());
    let body_ov = ov;
    if type_of(footer) != "()" {
        // The content takes the spare height, so the footer ends the section;
        // content that doesn't fit is clipped above it (or shrunk), never
        // drawn over it — unless `overflow: "visible"` is asked for.
        let fs = if type_of(footer) == "array" && !poster_is_rich(footer) { nodes(footer) }
                 else { [scheme_text("section", footer, base_size(), "start")] };
        let cov = if ov == "visible" && !opts.contains("overflow") { "clip" } else { ov };
        items = [col(items).spacing(section_gap()).stretch().flex(1).name(`${tag} (content)`).overflow(cov)
                     .main(main),
                 col(fs).spacing(section_gap()).stretch().name(`${tag} (footer)`)];
        body_ov = "visible";
        main = "start";
    }
    let sp = section_pad();
    let bi = section_body_inset();
    if !plot_finite(bi) || bi < 0 { throw `section_body_inset(): a length in cm >= 0 (got ${bi})`; }
    let pad = if bi == 0 { sp } else { #{ top: sp, bottom: sp, left: sp + bi, right: sp + bi } };
    let body = col(items).spacing(section_gap()).stretch().pad(pad).flex(1)
        .name(`${tag} (body)`).overflow(body_ov).main(main);
    col([section_bar(s), body]).stretch().name(tag)
}
