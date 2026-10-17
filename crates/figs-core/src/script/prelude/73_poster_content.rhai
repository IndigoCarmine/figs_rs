// `block(title, items)` / `block(title, items, opts)`: a `subheading(title)`
// (a node title is used as is; "" for none) over `items` side by side —
// equal flex shares by default — `block_title_gap()` below the heading and
// `block_gap()` apart. `items` is an array (strings become `body`) or one
// node. Options:
//   widths     per item: flex weights, "4cm" lengths or "auto"
//   gap        between the items (cm)
//   title_gap  between the heading and the items (cm)
//   align      the items across: "start" (default, tops), "center", "end",
//              "stretch", "baseline"
fn block(title, items) { block(title, items, #{}) }
fn block(title, items, opts) {
    check_opts("block", opts, ["widths", "gap", "title_gap", "align"]);
    let sl = split_links(items);
    let xs = sl.items;
    let ws = sizes_for("block", xs.len(), opt(opts, "widths", ()));
    let cells = [];
    for i in 0..xs.len() { cells.push(sized("block", as_node(xs[i]), ws[i], false)); }
    cells += sl.links;
    let align = opt(opts, "align", "start");
    if !["start", "center", "end", "stretch", "baseline"].contains(align) {
        throw `block(): unknown align "${align}" (start, center, end, stretch, baseline)`;
    }
    let r = row(cells).spacing(opt(opts, "gap", block_gap())).cross(align);
    let none = type_of(title) == "()" || (type_of(title) == "string" && title == "");
    if none { return r; }
    let head = if type_of(title) == "Node" { title } else { subheading(title) };
    col([head, r]).spacing(opt(opts, "title_gap", block_title_gap())).stretch()
}

// --- content blocks ---

// A thin rounded border box (`frame_color()`, `frame_width()`,
// `frame_radius()`) around `content` (an array `section_gap()` apart, or one
// node), padded `frame_pad()`. Square / grey / heavy variants are core
// setters: `frame(..).radius(0).stroke("#7f7f7f").stroke_width(0.1)`.
// `frame(content, variant)`: in a style variant (see `variant_push`).
fn frame(content) {
    let items = if type_of(content) == "array" { nodes(content) } else { [as_node(content)] };
    col(items).spacing(section_gap()).stretch().pad(frame_pad())
        .stroke(frame_color()).stroke_width(frame_width()).radius(frame_radius())
}
fn frame(content, variant) {
    variant_push("frame", variant);
    let f = frame(content);
    style_pop();
    f
}

// `frame` with a dashed border (`frame_dash()`).
fn dashed_frame(content) { frame(content).dash(frame_dash()) }
fn dashed_frame(content, variant) {
    variant_push("frame", variant);
    let f = dashed_frame(content);
    style_pop();
    f
}

// The highlighted take-home sentence of a section: centred, `note_color()`.
// `note(s, variant)`: in a style variant (`note_variants`).
fn note(s) {
    let t = text(s).size(note_size()).color(note_color()).center();
    let f = note_font();
    if f == "" { t } else { t.font(f) }
}
fn note(s, variant) {
    variant_push("note", variant);
    let t = note(s);
    style_pop();
    t
}

// A filled box for the key message of a section or of the poster: `s` (a
// string, spans or a node) centred at `takeaway_size()` (bold while
// `takeaway_bold()`), in `takeaway_color()` on `takeaway_fill()` (the
// accent), with `frame_radius()` corners, padded `frame_pad()`.
// `takeaway(s, variant)`: in a style variant (`takeaway_variants`).
fn takeaway(s) {
    let t = if type_of(s) == "Node" { s } else {
        let t = text(s).size(takeaway_size()).color(takeaway_color()).center();
        if takeaway_bold() { t.bold() } else { t }
    };
    let box = col([t]).cross("center").stretch().pad(frame_pad()).radius(frame_radius());
    let f = takeaway_fill();
    if has_paint(f) { box.fill(f) } else { box }
}
fn takeaway(s, variant) { in_variant("takeaway", variant, || takeaway(s)) }

// A key figure: `value` large (`stat_size()`, bold, `stat_color()`) over
// `label` (body size, the theme foreground; "" for none), centred on each
// other. Several side by side: `block("", [stat("×12", "faster"),
// stat("95 %", "yield")])`.
fn stat(value, label) {
    let v = text(value).size(stat_size()).color(stat_color()).bold().center();
    if type_of(label) == "string" && label == "" { return v; }
    col([v, fg_text(label).size(base_size()).center()]).cross("center").spacing(section_gap() * 0.25)
}

// A literature reference line: small (`ref_size()`), centred, in
// `ref_color()` and `ref_font()`. `ref(s, variant)`: `ref_variants`.
fn ref(s) {
    let t = text(s).size(ref_size()).center();
    let c = ref_color();
    if c != "" { t = t.color(c); }
    let f = ref_font();
    if f == "" { t } else { t.font(f) }
}
fn ref(s, variant) {
    variant_push("ref", variant);
    let t = ref(s);
    style_pop();
    t
}

// `subheading(s, variant)`: in a style variant (`subheading_variants`).
fn subheading(s, variant) {
    variant_push("subheading", variant);
    let t = subheading(s);
    style_pop();
    t
}

// A small heading inside a section: "■ " + `s` at `subheading_size()`; the
// mark bold (`subheading_mark_bold()`) and sized (`subheading_mark_size()`)
// on its own when those differ from the text (one text node of spans).
fn subheading(s) {
    let m = subheading_mark();
    let size = subheading_size();
    if m == "" { return fg_text(s).size(size); }
    let bold = subheading_mark_bold();
    let ms = subheading_mark_size();
    let sym = !font_unset(poster_symbol_font());
    if !bold && ms == size && !sym { return fg_text(text_prefix(`${m} `, s)).size(size); }
    let mark = poster_mark_font(span(`${m} `));
    if bold { mark = mark.bold(); }
    if ms != size { mark = mark.size(ms); }
    fg_text([mark] + (if type_of(s) == "string" { [span(s)] } else { s })).size(size)
}

// A bulleted list with a hanging indent: each item (a string or a node) in a
// cell `bullet_indent()` right of its `bullet_mark()`, so wrapped lines align
// with the first. An array item is a nested list under the previous item.
// `bullets(items, #{ pitch })`: the items' first baselines `pitch` cm apart
// (default `bullet_pitch()`; nested lists too) instead of `bullet_gap()`.
// Each item is a baseline-aligned row (`cross("baseline")`), so the mark sits
// on the first line's baseline — measured on the shaped text, exact for any
// font and for a mark of another size or font (`bullet_mark_size()`,
// `poster_symbol_font()`); a node item uses its first text's baseline (one
// without text sits on the mark's baseline by its bottom edge).
fn bullets(items) { bullets(items, #{}) }
fn bullets(items, opts) {
    if type_of(items) != "array" {
        throw `bullets(): expected an array of items (got ${type_of(items)})`;
    }
    check_opts("bullets", opts, ["pitch"]);
    let pitch = opt(opts, "pitch", bullet_pitch());
    if type_of(pitch) != "()" && !(finite_num(pitch) && pitch > 0) {
        throw `bullets(): pitch must be a positive length in cm or () (got ${pitch})`;
    }
    let ind = bullet_indent();
    let size = bullet_size();
    let ms = bullet_mark_size();
    let out = [];
    for x in items {
        if type_of(x) == "array" && !is_rich(x) {
            out.push(row([rect().width(ind), bullets(x, #{ pitch: pitch }).flex(1)]).stretch());
            continue;
        }
        let item = if type_of(x) == "string" || is_rich(x) { fg_text(x).size(size) } else { x };
        let mark = poster_mark_font(fg_text(bullet_mark()).size(ms)).width(ind);
        out.push(row([mark, item.flex(1)]).cross("baseline"));
    }
    let c = col(out).spacing(bullet_gap()).stretch();
    let bi = bullet_inset();
    if !finite_num(bi) || bi < 0 { throw `bullet_inset(): a length in cm >= 0 (got ${bi})`; }
    if bi > 0 { c = c.pad(#{ left: bi }); }
    if type_of(pitch) == "()" { c } else { c.baseline_pitch(pitch) }
}

// `table(rows)` / `table(rows, opts)`: `rows` is an array of arrays of cells
// (strings, numbers, spans or nodes; `colspan(n, x)` spans n columns), all
// as wide in columns; the first row is the header (bold, above a
// `table_header_rule_width()` rule). Options:
//   header  false: no header row (default true)
//   widths  per column: flex weights, "4cm" lengths or "auto" (default equal)
//   align   "left" (default), "center", "right", or one per column
//   rules   "grid", "rows", "booktabs" or "none" (default `table_rules()`)
//   pad     cell padding, cm (default `table_pad()`); pad_x / pad_y for
//           left-right / top-bottom (default `table_pad_x()` / `table_pad_y()`)
//   font    the font of every text cell (a family, a fallback list or a
//           per-script map; default `table_font()`, header `table_header_font()`)
//   header_font  the header row's font (default `font`, else `table_header_font()`)
//   fonts   one font per column ("" / () for the table's), over `font`
//   rule_mode  "space" or "overlay" (default `table_rule_mode()`)
// The rules are filled rects (`table_rule()`, `table_rule_width()`), so the
// grid is exact at any size: between the cells ("space"), or laid over the
// cell boundaries without taking room ("overlay"). Cells are filled
// `table_fill()` / `table_header_fill()`. Node cells keep their own font.
fn table(rows) { table(rows, #{}) }
fn table(rows, opts) {
    check_opts("table", opts, ["header", "widths", "align", "rules", "pad", "pad_x", "pad_y",
                                      "font", "header_font", "fonts", "rule_mode", "variant"]);
    if opts.contains("variant") {
        variant_push("table", opts.variant);
        opts.remove("variant");
        let t = table(rows, opts);
        style_pop();
        return t;
    }
    if type_of(rows) != "array" || rows.len() == 0 { throw "table(): expected a non-empty array of rows"; }
    let nc = 0;
    for r in rows {
        if type_of(r) != "array" { throw `table(): each row must be an array of cells (got ${type_of(r)})`; }
        let k = 0;
        for c in r { k += table_span(c); }
        if nc == 0 { nc = k; }
        if k != nc || nc == 0 { throw `table(): rows differ in length (${nc} vs ${k} columns)`; }
    }
    let header = opt(opts, "header", true);
    let ws = sizes_for("table", nc, opt(opts, "widths", ()));
    let al = opt(opts, "align", "left");
    if type_of(al) == "string" {
        let a = [];
        for i in 0..nc { a.push(al); }
        al = a;
    }
    if type_of(al) != "array" || al.len() != nc {
        throw "table(): align must be a string or one per column";
    }
    let pad = opt(opts, "pad", ());
    let px = opt(opts, "pad_x", if type_of(pad) == "()" { table_pad_x() } else { pad });
    let py = opt(opts, "pad_y", if type_of(pad) == "()" { table_pad_y() } else { pad });
    // Fonts: per column, then the table's `font`, then the hooks.
    let cf = opt(opts, "fonts", ());
    if type_of(cf) != "()" && (type_of(cf) != "array" || cf.len() != nc) {
        throw `table(): fonts must be one font per column (${nc}; "" for the table font)`;
    }
    let body_font = opt(opts, "font", table_font());
    let head_font = opt(opts, "header_font", opt(opts, "font", table_header_font()));
    let rules = opt(opts, "rules", table_rules());
    if !["grid", "rows", "booktabs", "none"].contains(rules) {
        throw `table(): unknown rules "${rules}" (grid, rows, booktabs, none)`;
    }
    let mode = opt(opts, "rule_mode", table_rule_mode());
    if mode != "space" && mode != "overlay" {
        throw `table(): unknown rule_mode "${mode}" (space, overlay)`;
    }
    let rc = table_rule();
    let rw = table_rule_width();
    let vertical = rules == "grid";
    let inner = rules == "grid" || rules == "rows";
    let outer = rules != "none";
    if mode == "overlay" {
        let fonts = #{ cf: cf, body: body_font, head: head_font, own_head: opts.contains("header_font") };
        let r = #{ rc: rc, rw: rw, vertical: vertical, inner: inner, outer: outer };
        return table_overlay(rows, header, ws, al, px, py, fonts, r);
    }
    let parts = [];
    if outer { parts.push(rect().height(rw).fill(rc)); }
    for i in 0..rows.len() {
        let head = header && i == 0;
        if i == 1 && header && outer {
            parts.push(rect().height(table_header_rule_width()).fill(rc));
        } else if i > 0 && inner {
            parts.push(rect().height(rw).fill(rc));
        }
        let cells = [];
        if vertical { cells.push(rect().width(rw).fill(rc)); }
        let j = 0;
        for x in rows[i] {
            let k = table_span(x);
            let content = if type_of(x) == "map" { x.content } else { x };
            let font = if head { head_font } else { body_font };
            if type_of(cf) != "()" && !font_unset(cf[j]) && !(head && opts.contains("header_font")) {
                font = cf[j];
            }
            let cell = table_cell(content, head, al[j], px, py, font);
            if k == 1 {
                cells.push(sized("table", cell, ws[j], false));
            } else {
                cells += table_spanned(cell, ws, j, k, if vertical { rw } else { 0.0 }, table_cell_fill(head));
            }
            j += k;
            if vertical { cells.push(rect().width(rw).fill(rc)); }
        }
        parts.push(row(cells).stretch());
    }
    if outer { parts.push(rect().height(rw).fill(rc)); }
    let t = col(parts).stretch();
    // All widths absolute: the table is exactly as wide as its columns.
    let total = 0.0;
    for w in ws {
        let v = cm_length("table", w);
        if type_of(v) == "()" { return t; }
        total += v;
    }
    if vertical { total += rw * (nc + 1).to_float(); }
    t.width(total)
}

// `table` with rule_mode "overlay": the rows edge to edge, each a unit
// viewbox stack with its horizontal rule(s) laid over it — the rule above it
// centred on its top edge (the table's top rule, and the bottom one under the
// last row, just inside) — and each cell likewise with its vertical rule on
// its left edge (and the last one on its right). Later rows and cells paint
// over earlier ones, so every rule lies on top of the cells either side.
// `f`: fonts (cf, body, head, own_head); `r`: rules (rc, rw, vertical,
// inner, outer).
fn table_overlay(rows, header, ws, al, px, py, f, r) {
    let out = [];
    let n = rows.len();
    for i in 0..n {
        let head = header && i == 0;
        let cells = [];
        let j = 0;
        let last = rows[i].len() - 1;
        let ci = 0;
        for x in rows[i] {
            let k = table_span(x);
            let content = if type_of(x) == "map" { x.content } else { x };
            let font = if head { f.head } else { f.body };
            if type_of(f.cf) != "()" && !font_unset(f.cf[j]) && !(head && f.own_head) {
                font = f.cf[j];
            }
            let cell = table_cell(content, head, al[j], px, py, font);
            if r.vertical {
                // Its left rule centred on the boundary (inside at the
                // table's left edge), the table's right edge on the last.
                let layers = [cell, rect().width(r.rw).fill(r.rc).x(0).anchor(if ci == 0 { 0 } else { 0.5 }, 0)];
                if ci == last { layers.push(rect().width(r.rw).fill(r.rc).x(1).anchor(1, 0)); }
                cell = stack(layers).viewbox(0, 0, 1, 1);
            }
            cells += if k == 1 { [sized("table", cell, ws[j], false)] }
                     else { table_spanned(cell, ws, j, k, 0.0, table_cell_fill(head)) };
            j += k;
            ci += 1;
        }
        let layers = [row(cells).stretch()];
        // The rule above this row: the table's top (inside), the header
        // rule or an inner one (centred on the boundary).
        let top = if i == 0 { if r.outer { [r.rw, 0] } else { () } }
                  else if i == 1 && header && r.outer { [table_header_rule_width(), 0.5] }
                  else if r.inner { [r.rw, 0.5] } else { () };
        if type_of(top) != "()" { layers.push(rect().height(top[0]).fill(r.rc).y(0).anchor(0, top[1])); }
        if i == n - 1 && r.outer { layers.push(rect().height(r.rw).fill(r.rc).y(1).anchor(0, 1)); }
        out.push(stack(layers).viewbox(0, 0, 1, 1));
    }
    let t = col(out).stretch();
    // All widths absolute: the table is exactly as wide as its columns.
    let total = 0.0;
    for w in ws {
        let v = cm_length("table", w);
        if type_of(v) == "()" { return t; }
        total += v;
    }
    t.width(total)
}

// A cell spanning `n` columns: `#{ colspan: n, content: x }` in a table row.
fn colspan(n, x) {
    if type_of(n) != "i64" || n < 1 { throw `colspan(): n must be a positive integer (got ${n})`; }
    #{ colspan: n, content: x }
}

// How many columns table cell `x` spans.
fn table_span(x) {
    if type_of(x) != "map" { return 1; }
    if !x.contains("colspan") || !x.contains("content") {
        throw "table(): a map cell is #{ colspan, content } (see colspan(n, x))";
    }
    x.colspan
}

// `cell` sized over columns j .. j + k - 1 of widths `ws`: all lengths (cm)
// → their sum plus the `rw` rules it covers; all flex weights → their sum,
// with the covered rules' width as filled blanks either side (fixed, so the
// flex shares match the rows without a span).
fn table_spanned(cell, ws, j, k, rw, fill) {
    let sum = 0.0;
    let cm = 0;
    for c in j..(j + k) {
        let w = ws[c];
        let v = cm_length("table", w);
        if type_of(v) != "()" { sum += v; cm += 1; }
        else if type_of(w) == "i64" || type_of(w) == "f64" { sum += w.to_float(); }
        else { throw "table(): a colspan can't cover \"auto\" columns"; }
    }
    let extra = rw * (k - 1).to_float();
    if cm == k { return [cell.width(sum + extra)]; }
    if cm > 0 { throw "table(): a colspan must cover only lengths or only flex weights"; }
    if extra == 0.0 { return [cell.flex(sum)]; }
    let blank = rect().width(extra / 2.0);
    if has_paint(fill) { blank = blank.fill(fill); }
    [blank, cell.flex(sum), blank]
}

// The fill of a header / body cell ("" for none).
fn table_cell_fill(head) {
    let f = if head { table_header_fill() } else { "" };
    if has_paint(f) { f } else { table_fill() }
}

// One table cell: `x` (a string or number becomes text, bold in the header)
// padded `table_pad_x()` / `table_pad_y()` (or `px` / `py`), vertically
// centred, aligned `align`, filled `table_header_fill()` (header) or
// `table_fill()`.
fn table_cell(x, head, align) { table_cell(x, head, align, table_pad_x(), table_pad_y()) }
fn table_cell(x, head, align, px, py) {
    table_cell(x, head, align, px, py, if head { table_header_font() } else { table_font() })
}
fn table_cell(x, head, align, px, py, font) {
    let t = type_of(x);
    let n = if t == "string" || t == "i64" || t == "f64" || is_rich(x) {
        let s = fg_text(if is_rich(x) { x } else { `${x}` }).size(table_size()).align(align);
        s = with_font(s, font);
        if head { s.bold() } else { s }
    } else { x };
    // One padding on all sides; the larger axis gets the rest as spacers.
    let p = if px < py { px } else { py };
    let c = col([n]).pad(p).main("center").stretch();
    if px > py {
        let s = rect().width(px - py);
        c = row([s, c.flex(1), s]).stretch();
    } else if py > px {
        let s = rect().height(py - px);
        c = col([s, c.flex(1), s]).stretch();
    }
    let f = table_cell_fill(head);
    if has_paint(f) { c.fill(f) } else { c }
}
