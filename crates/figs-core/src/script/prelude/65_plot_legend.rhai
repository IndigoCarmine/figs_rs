// --- legend ---

// The key drawn before a legend label: a short line (with its marker), a
// marker, or a filled square for bars. All keys share one width.
fn legend_key(s) {
    let h = marker_size();
    let w = base_size() * 0.06;
    let parts = [];
    if s.kind == "line" {
        let lw = if type_of(s.width) == "()" { series_width() } else { s.width };
        parts.push(line(0, h / 2, w, h / 2).stroke(s.color).stroke_width(lw).dash(s.dash));
    }
    if s.kind == "bars" {
        let a = h * 1.25;
        parts.push(rect().fill(s.color).width(a).height(a).at(w / 2, h / 2).anchor(0.5, 0.5));
    } else if s.kind == "errorbars" {
        parts.push(line(w / 2, 0, w / 2, h).stroke(s.color).stroke_width(plot_line_width()));
    } else if s.marker != "none" {
        let d = if type_of(s.size) == "()" { marker_size() } else { s.size };
        parts.push(plot_marker(s.marker, d, s.color, s.open).at(w / 2, h / 2).anchor(0.5, 0.5));
    }
    stack(parts).width(w).height(h)
}

// A legend for `series` (labelled ones only): one key + label row each — or,
// with `legend_style()` "text", the labels alone in their series colours.
// Usable on its own, e.g. beside a row of plots: `legend([line_plot(..), ..])`;
// `legend(style, series)` picks the style ("swatch" or "text").
fn legend(series) { legend(legend_style(), series) }
fn legend(style, series) { legend_box(plot_colored(plot_only_series(series)), style) }

fn legend_box(series, style) {
    if style != "swatch" && style != "text" {
        throw `legend(): unknown style "${style}" (swatch, text)`;
    }
    let rows = [];
    for s in series {
        if s.label == "" { continue; }
        if style == "text" {
            rows.push(with_font(text(s.label).size(legend_size()).color(s.color), legend_font()));
        } else {
            rows.push(row([legend_key(s), plot_label(s.label, legend_size(), legend_font())])
                .cross("center").spacing(legend_size() * 0.02));
        }
    }
    col(rows).spacing(legend_size() * 0.008)
}

// `series` (an array or one series) without annotation descriptors.
fn plot_only_series(series) {
    if type_of(series) != "array" { series = [series]; }
    series.filter(|x| !plot_is_annotation(x))
}
