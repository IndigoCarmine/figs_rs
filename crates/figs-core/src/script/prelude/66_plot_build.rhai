// --- the plot ---

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
    let st = opt(opts, "style", ());
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
    let xlog = opt(opts, "xscale", "linear");
    let ylog = opt(opts, "yscale", "linear");
    for sc in [xlog, ylog] {
        if sc != "linear" && sc != "log" { throw `plot(): scale must be "linear" or "log" (got "${sc}")`; }
    }
    xlog = xlog == "log";
    ylog = ylog == "log";
    if type_of(series) != "array" { series = [series]; }
    // An array among the series (`plot_shift_arrows(..)`) is spliced in.
    let flat = [];
    for x in series { if type_of(x) == "array" { flat += x; } else { flat.push(x); } }
    series = flat;
    let notes = series.filter(|x| plot_is_annotation(x));
    let series = plot_colored(plot_only_series(series));
    let extra = opt(opts, "marks", []);
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
                for v in s.x { if finite_num(v) { bar_xs.push(v); } }
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
            if type_of(s.yerr) != "()" && finite_num(s.yerr[i]) {
                let e = s.yerr[i].to_float().abs();
                let lo = plot_tf(s.y[i] - e, ylog);
                let hi = plot_tf(s.y[i] + e, ylog);
                errs.push([lo, hi]);
                ymin = plot_min(ymin, lo); ymax = plot_max(ymax, hi);
            } else { errs.push(()); }
        }
        prepared.push(#{ s: s, pts: pts, errs: errs, bw: bw });
    }

    let xa = plot_axis("x", xmin, xmax, opt(opts, "x", ()), xlog, opt(opts, "xticks", ()), cats);
    let ya = plot_axis("y", ymin, ymax, opt(opts, "y", ()), ylog, opt(opts, "yticks", ()), []);
    xa = plot_relabel(xa, opt(opts, "xtick_format", ()), xlog, "x");
    ya = plot_relabel(ya, opt(opts, "ytick_format", ()), ylog, "y");
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
    let frame = opt(opts, "frame", plot_frame());
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
    let xminor = xa.minor + plot_minor(xa, xlog, opt(opts, "xminor", minor_ticks()), "xminor");
    let yminor = ya.minor + plot_minor(ya, ylog, opt(opts, "yminor", minor_ticks()), "yminor");
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
    let where = opt(opts, "legend", legend_corner());
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
        lstyle = opt(where, "style", lstyle);
        lat = opt(where, "at", ());
        lanchor = opt(where, "anchor", lanchor);
        where = opt(where, "corner", legend_corner());
    } else if where == "text" || where == "swatch" {
        lstyle = where;
        where = legend_corner();
    }
    let labelled = false;
    for s in series { if s.label != "" { labelled = true; } }
    let taken = [];      // corners holding the legend / an inset
    if labelled && type_of(lat) != "()" {
        let p = numbers("plot", "legend `at`", lat);
        let la = numbers("plot", "legend `anchor`", lanchor);
        if p.len() != 2 || la.len() != 2 { throw "plot(): legend `at` and `anchor` must be [x, y]"; }
        let px = plot_tf(p[0], xlog);
        let py = plot_tf(p[1], ylog);
        if type_of(px) == "()" || type_of(py) == "()" { throw `plot(): legend at ${lat} is off the log axis`; }
        area[0] = stack(marks + extra + [legend_box(series, lstyle).at(px, py).anchor(la[0], la[1])])
            .viewbox(x0, y1, x1, y0);
    } else if where != "" && labelled {
        if where == "auto" { where = auto_corner(samples, []); }
        area.push(pin(legend_box(series, lstyle), where, base_size() * 0.03));
        taken.push(where);
    }

    // Corner insets, over the legend's corner only if asked for by name.
    for a in corner_insets {
        let c = if a.corner == "auto" { auto_corner(samples, taken) } else { a.corner };
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
    let mode = opt(opts, "tick_label_pad_mode", tick_label_pad_mode());
    if mode != "box" && mode != "baseline" {
        throw plot_who(opts, "tick_label_pad_mode") + ` must be "box" or "baseline" (got "${mode}")`;
    }
    let apad = axis_label_pad();                   // title <-> unrotated y label
    if !finite_num(apad) || apad < 0 {
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
    let yal = ytick_label_align();
    if yal != "cap_middle" && yal != "box" {
        throw `ytick_label_align(): must be "cap_middle" or "box" (got ${yal})`;
    }
    let yay = if yal == "box" { 0.5 } else { "cap_middle" };
    let yl = [];
    for i in 0..ya.ticks.len() {
        if ya.ticks[i] >= y0 - 1e-9 && ya.ticks[i] <= y1 + 1e-9 {
            yl.push(plot_label(ya.labels[i], tsize, tfont).at(1, ya.ticks[i]).anchor(1, yay));
        }
    }
    // Left band, as tall as the area and right-aligned to the axis: the y tick
    // labels in a viewbox stack spanning the axis (as wide as the widest
    // label), then room for the ticks — after the y label, when it is rotated
    // (`ylabel_rotated()`): a quarter turn swaps its footprint, so in the
    // stretched row it is one line wide and as long as the axis, centred on it.
    let ylabel = opt(opts, "ylabel", "");
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
    let xlabel = opt(opts, "xlabel", "");
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
    let ptitle = opt(opts, "title", "");
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

    let fb = opt(opts, "frame_box", plot_frame_box());
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
    if type_of(ar) == "string" {
        if ar != "fill" { throw `plot_aspect(): a width / height ratio, 0 or "fill" (got "${ar}")`; }
        // No aspect: the spare height of its column, at least the minimum.
        return node.flex(1).min_height(plot_min_height()).meta("fill", true);
    }
    if ar.to_float() > 0.0 { node = node.aspect(ar); }
    node
}

// Do the ticks of one axis point in? Plot option `opt` ("xtick_direction" /
// "ytick_direction"), else `dir` (`tick_direction()`).
fn plot_tick_inward(opts, opt, dir) {
    let d = opt(opts, opt, dir);
    if d != "out" && d != "in" { throw `plot(): ${opt} must be "out" or "in" (got "${d}")`; }
    d == "in"
}

// Who set plot setting `key`: "plot(): key" when the option is given, else
// the hook "key()" — for errors.
fn plot_who(opts, key) { if opts.contains(key) { `plot(): ${key}` } else { `${key}():` } }

// Plot length `key` (cm): the option when given, else the hook value `v`,
// as a float; anything but a finite number is an error.
fn plot_len(opts, key, v) {
    let x = opt(opts, key, v);
    if !finite_num(x) { throw plot_who(opts, key) + ` must be a length in cm (got ${x})`; }
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
    let x = opt(opts, key, v);
    if type_of(x) == "()" { return x; }
    if !finite_num(x) || !(x > 0) {
        throw plot_who(opts, key) + ` must be a length > 0 cm from the axis to the label's baseline, or () (got ${x})`;
    }
    x.to_float()
}
