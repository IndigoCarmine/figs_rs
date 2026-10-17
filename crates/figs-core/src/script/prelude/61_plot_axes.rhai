// --- numbers ---

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
