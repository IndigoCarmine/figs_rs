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

// `plot_shift_arrows(from, to, xs)` / `(…, opts)`: at each data x in `xs`,
// an arrow from series `from`'s y to series `to`'s y (both linearly
// interpolated at x) — a spectral change on heating, a shift between two
// conditions. `from` / `to` are series (`line_plot(..)`, `scatter(..)`);
// x must lie within both. An array of `plot_arrow` annotations: put it
// among the series (`plot(opts, [a, b, plot_shift_arrows(a, b, [420, 480])])`)
// or in `marks`. Options: `color` (or a palette index; default `plot_fg()`),
// `width` (cm), `head` ("end" default, "both", "start"), `dash`, `trim` (the
// fraction of each arrow left off at both ends, default `shift_arrow_trim()`),
// `min_length` (leave out arrows shorter than this, y data units; default
// `shift_arrow_min_length()`: 0), `anchor` ("gap": `trim` off both ends;
// "curve": start on the first series, only the head end trimmed; default
// `shift_arrow_anchor()`).
fn plot_shift_arrows(a, b, xs) { plot_shift_arrows(a, b, xs, #{}) }
fn plot_shift_arrows(a, b, xs, opts) {
    let who = "plot_shift_arrows";
    for sr in [a, b] {
        if type_of(sr) != "map" || !sr.contains("kind") {
            throw `${who}(): from and to must be series (line_plot / scatter / bars), got ${type_of(sr)}`;
        }
    }
    check_opts(who, opts, ["color", "width", "head", "dash", "trim", "min_length", "anchor"]);
    if type_of(xs) != "array" { xs = [xs]; }
    let trim = opt(opts, "trim", shift_arrow_trim()).to_float();
    if !(trim >= 0.0 && trim < 0.5) { throw `${who}(): trim is a fraction from 0 to 0.5 (got ${trim})`; }
    let min = opt(opts, "min_length", shift_arrow_min_length());
    if !finite_num(min) || min < 0 { throw `${who}(): min_length is a length in y data units >= 0 (got ${min})`; }
    let anchor = opt(opts, "anchor", shift_arrow_anchor());
    if !["gap", "curve"].contains(anchor) { throw `${who}(): unknown anchor "${anchor}" (gap, curve)`; }
    let o = #{};
    for k in ["color", "width", "head", "dash"] { if opts.contains(k) { o[k] = opts[k]; } }
    let out = [];
    for x in numbers(who, "x", xs) {
        let y0 = plot_interp(who, a, x);
        let y1 = plot_interp(who, b, x);
        let dy = (y1 - y0).to_float();
        if dy == 0.0 || dy.abs() < min.to_float() { continue; }
        let d = (y1 - y0) * trim;
        let s = if anchor == "curve" { y0 } else { y0 + d };
        out.push(plot_arrow(x, s, x, y1 - d, o));
    }
    out
}

// Series `sr`'s y at data x, linearly interpolated (x ascending or descending).
fn plot_interp(who, sr, x) {
    let xs = sr.x;
    let ys = sr.y;
    if xs.len() == 0 || type_of(xs[0]) == "string" {
        throw `${who}(): needs series with numeric x values`;
    }
    for i in 0..xs.len() {
        if xs[i] == x { return ys[i]; }
        if i == 0 { continue; }
        let x0 = xs[i - 1];
        let x1 = xs[i];
        if (x > x0 && x < x1) || (x < x0 && x > x1) {
            return ys[i - 1] + (ys[i] - ys[i - 1]) * (x - x0) / (x1 - x0);
        }
    }
    throw `${who}(): x = ${x} is outside a series' x range (${xs[0]} .. ${xs[xs.len() - 1]})`;
}

// An annotation descriptor (`plot` turns it into nodes once the axes exist).
fn plot_annotation(name, kind, pts, s, opts, known) {
    if type_of(opts) != "map" { throw `${name}(): options must be a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        if !known.contains(k) { throw `${name}(): unknown option "${k}" (${list_names(known)})`; }
    }
    // Unset style-dependent defaults stay () until the plot draws the
    // annotation, so its `style:` option applies to them.
    let a = #{ mark: kind, pts: numbers(name, "coordinates", pts), s: s, color: () };
    if opts.contains("color") {
        a.color = if type_of(opts.color) == "i64" { palette_color(opts.color) } else { opts.color };
    }
    if kind == "text" {
        if type_of(s) != "string" { throw `${name}(): text must be a string (got ${type_of(s)})`; }
        a.size = opt(opts, "size", ());
        a.font = opt(opts, "font", ());
        for k in ["bold", "italic", "markup"] {
            a[k] = opt(opts, k, false);
            if type_of(a[k]) != "bool" { throw `${name}(): ${k} must be true or false (got ${a[k]})`; }
        }
        // Parse markup now, so a malformed string is reported at this call.
        if a.markup { a.s = markup(s); }
        a.anchor = numbers(name, "anchor", opt(opts, "anchor", [0.5, 0.5]));
        if a.anchor.len() != 2 { throw `${name}(): anchor must be [ax, ay]`; }
    } else if kind == "dimension" {
        if type_of(s) != "string" && !is_rich(s) {
            throw `${name}(): the label must be a string or spans (got ${type_of(s)})`;
        }
        a.width = opt(opts, "width", ());
        a.size = opt(opts, "size", ());
        a.gap = opt(opts, "gap", ());
        a.dash = opt(opts, "dash", []);
        a.vertical = false;
        // The label's side: above / right (false) or below / left (true).
        a.flip = opt(opts, "below", opt(opts, "left", false));
        if type_of(a.flip) != "bool" { throw `${name}(): below / left must be true or false (got ${a.flip})`; }
    } else {
        a.width = opt(opts, "width", ());
        a.head = opt(opts, "head", "end");
        a.dash = opt(opts, "dash", []);
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
        let t = with_font(text(a.s).size(size).color(c),
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
        let t = with_font(text(a.s).size(size).color(c), plot_font()).nowrap();
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
