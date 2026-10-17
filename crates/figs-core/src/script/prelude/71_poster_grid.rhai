// `node` sized along a container's main axis by `w`: a number is a flex
// weight, "12cm" an absolute length, "auto" its natural size.
fn sized(who, node, w, vertical) {
    let t = type_of(w);
    if t == "i64" || t == "f64" {
        if !(w > 0) { throw `${who}(): a flex weight must be positive (got ${w})`; }
        return node.flex(w);
    }
    if w == "auto" { return node; }
    let v = cm_length(who, w);
    if type_of(v) == "()" {
        throw `${who}(): sizes are flex weights (2), lengths ("12cm") or "auto" (got ${w})`;
    }
    if vertical { node.height(v) } else { node.width(v) }
}

// Sizes for `n` items: `ws` checked against `n`, or n ones.
fn sizes_for(who, n, ws) {
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
    if !finite_num(off) { throw `columns(): a separator offset is a length in cm (got ${off})`; }
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
    check_opts("columns", opts, ["gutter", "separators"]);
    let sl = split_links(items);
    items = sl.items;
    let n = items.len();
    let ws = sizes_for("columns", n, widths);
    let gs = poster_per_gap("columns", "gutter", opt(opts, "gutter", poster_gutter()), n);
    let ss = poster_per_gap("columns", "separators", opt(opts, "separators", separators()), n);
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
                let y0 = opt(s, "start", opt(s, "from", separator_inset()));
                poster_vsep(g, y0, opt(s, "end", opt(s, "until", ())),
                            opt(s, "offset", separator_offset()))
            } else {
                throw `columns(): a separator is true, false, an end (cm) or #{ start, end, offset } (got ${t})`;
            });
        }
        out.push(sized("columns", as_node(items[i]), ws[i], false));
    }
    row(out + sl.links).spacing(if uniform && gs.len() > 0 { gs[0] } else { 0 }).stretch()
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
    check_opts("rows", opts, ["gaps", "separators"]);
    let sl = split_links(items);
    items = sl.items;
    let n = items.len();
    let hs = sizes_for("rows", n, heights);
    let gs = poster_per_gap("rows", "gaps", opt(opts, "gaps", poster_row_gap()), n);
    let ss = poster_per_gap("rows", "separators", opt(opts, "separators", row_separators()), n);
    for s in ss {
        if type_of(s) != "bool" { throw `rows(): separators are true or false (got ${type_of(s)})`; }
    }
    let uniform = poster_uniform(gs, ss);
    let out = [];
    for i in 0..n {
        if i > 0 && !uniform {
            out.push(if ss[i - 1] { hsep(gs[i - 1]) } else { rect().height(gs[i - 1]) });
        }
        out.push(sized("rows", as_node(items[i]), hs[i], true));
    }
    col(out + sl.links).spacing(if uniform && gs.len() > 0 { gs[0] } else { 0 }).stretch()
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
