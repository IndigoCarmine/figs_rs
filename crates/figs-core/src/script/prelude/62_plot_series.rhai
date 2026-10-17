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
