// --- header ---

// The author line as runs `[text, superscript?]` (adjacent plain runs merged):
// `authors` are names or maps #{ name, aff: [1, 2] (or 1), presenter: true };
// affiliation numbers are only shown with more than one affiliation.
fn author_runs(authors, affiliations) {
    let runs = [];
    let numbered = affiliations.len() > 1;
    let i = 0;
    for a in authors {
        if i > 0 { runs = runs_push(runs, author_sep(), false); }
        i += 1;
        if type_of(a) == "string" || is_rich(a) {
            runs = runs_push(runs, a, false);
            continue;
        }
        if type_of(a) != "map" || !a.contains("name") {
            throw `poster_header(): an author is a name or #{ name, aff, presenter } (got ${type_of(a)})`;
        }
        if opt(a, "presenter", false) {
            let m = presenter_mark();
            // A mark of its own (a span in `poster_symbol_font()`) when set.
            runs = runs_push(runs, if m == "" || font_unset(poster_symbol_font()) { m }
                                   else { [poster_mark_font(span(m))] }, false);
        }
        runs = runs_push(runs, a.name, false);
        let aff = opt(a, "aff", []);
        if type_of(aff) != "array" { aff = [aff]; }
        if numbered && aff.len() > 0 {
            let s = "";
            for k in aff {
                if s != "" { s += ","; }
                s += `${k}`;
            }
            runs = runs_push(runs, s, true);
        }
    }
    runs
}

// The affiliations as runs: "Affil. A¹・Affil. B²" (unnumbered when single).
fn affiliation_runs(affiliations) {
    let runs = [];
    let n = affiliations.len();
    for i in 0..n {
        if i > 0 { runs = runs_push(runs, author_sep(), false); }
        runs = runs_push(runs, affiliations[i], false);
        if n > 1 { runs = runs_push(runs, `${i + 1}`, true); }
    }
    runs
}

// `runs` with run [s, sup] appended (merged into a preceding plain run). `s`
// is a string or rich text (spans, kept as a run of its own).
fn runs_push(runs, s, sup) {
    if type_of(s) == "array" {
        if s.len() > 0 { runs.push([s, sup]); }
        return runs;
    }
    if s == "" { return runs; }
    let n = runs.len();
    if n > 0 && !sup && !runs[n - 1][1] && type_of(runs[n - 1][0]) == "string" {
        runs[n - 1][0] += s;
    } else {
        runs.push([s, sup]);
    }
    runs
}

// One centred text node of `runs` at `size` pt: plain runs as spans, the
// affiliation numbers as superscript spans (the core script size and
// shift), so the line wraps and breaks like any text. Rich runs keep their
// spans (strings among them become plain spans).
fn runs_line(runs, size) {
    let spans = [];
    for r in runs {
        if type_of(r[0]) == "array" {
            for x in r[0] { spans.push(if type_of(x) == "string" { span(x) } else { x }); }
        } else {
            spans.push(if r[1] { span(r[0]).sup() } else { span(r[0]) });
        }
    }
    let t = fg_text(spans).size(size).center();
    let f = author_font();
    if f == "" { t } else { t.font(f) }
}

// The author line(s) of a poster header. `affiliation_position()` "before":
// "（Affil. A¹・Affil. B²）○Author¹・Author²" on one line; "below": the
// authors, and the numbered affiliations a size smaller under them. Names
// and affiliations are strings or rich text (`markup("…")`, span arrays):
// e.g. a name in another font, an affiliation with an italic part.
fn author_line(authors, affiliations) {
    if type_of(authors) != "array" || is_rich(authors) { authors = [authors]; }
    if type_of(affiliations) != "array" || is_rich(affiliations) { affiliations = [affiliations]; }
    let size = author_size();
    let who = author_runs(authors, affiliations);
    if affiliations.len() == 0 { return runs_line(who, size); }
    let where = affiliation_position();
    if where == "before" {
        // "（" affiliations "）" then the authors.
        let runs = runs_push([], "（", false);
        for r in affiliation_runs(affiliations) { runs = runs_push(runs, r[0], r[1]); }
        runs = runs_push(runs, "）", false);
        for r in who { runs = runs_push(runs, r[0], r[1]); }
        return runs_line(runs, size);
    }
    if where != "below" {
        throw `affiliation_position(): must be "before" or "below" (got "${where}")`;
    }
    col([runs_line(who, size), runs_line(affiliation_runs(affiliations), size * 0.8)])
        .spacing(size * 0.005).stretch()
}

// The poster header; `opts` is a map of
//   title         the title (a string or a node): `poster_title_size()`, bold
//   authors       names or #{ name, aff, presenter } (see `author_line`)
//   affiliations  affiliation names, numbered from 1
//   logos         images (or paths) left of the title, `poster_logo_height()` tall
//   number        the poster number, top-right
//   number_position  "title" (at the title row's right end) or "above" (a
//                 right-aligned line above the title); default
//                 `poster_number_position()`
//   number_inset  the number's distance in from the right edge, cm (default
//                 `poster_number_inset()`)
//   logo_box      [w, h] cm: every logo fitted and centred in a box of that
//                 size, so logos of different shapes line up (default
//                 `poster_logo_box()`; () = each `poster_logo_height()` tall)
// Text (titles, items, cells) may be a string or rich-text spans
// (`markup("…")`, `[span(..), ..]`) throughout the poster components.
// The title (with the number at its right end) and the author line are
// centred in the space right of the logos; a `poster_header_rule()` runs
// under it all.
fn poster_header(opts) {
    if type_of(opts) != "map" { throw `poster_header(): expected a map (got ${type_of(opts)})`; }
    for k in opts.keys() {
        let keys = ["title", "authors", "affiliations", "logos", "number", "number_position",
                    "number_inset", "logo_box", "title_inset"];
        if !keys.contains(k) {
            throw `poster_header(): unknown key "${k}" (${list_names(keys)})`;
        }
    }
    let title = opt(opts, "title", "");
    let tnode = if type_of(title) != "string" && !is_rich(title) {
        title
    } else if type_of(title) != "string" || title != "" {
        let t = fg_text(title).size(poster_title_size()).bold().center();
        let f = poster_title_font();
        if f == "" { t } else { t.font(f) }
    } else { () };
    let ti = opt(opts, "title_inset", poster_title_inset());
    if type_of(ti) != "array" { ti = [ti, ti]; }
    if ti.len() != 2 || !finite_num(ti[0]) || !finite_num(ti[1]) || ti[0] < 0 || ti[1] < 0 {
        throw `poster_header(): title_inset is a length in cm >= 0 or [left, right] (got ${ti})`;
    }
    if type_of(tnode) != "()" && (ti[0] > 0 || ti[1] > 0) {
        tnode = tnode.margin(#{ left: ti[0], right: ti[1] });
    }
    // The number at the right end of the title row, so a long title wraps
    // before it instead of running under it, while the author line below
    // keeps the full width.
    let num = opt(opts, "number", "");
    let where = opt(opts, "number_position", poster_number_position());
    if !["title", "above"].contains(where) {
        throw `poster_header(): unknown number_position "${where}" (title, above)`;
    }
    let above = ();
    if num != "" {
        let n = fg_text(num).size(poster_number_size()).bold();
        let ni = opt(opts, "number_inset", poster_number_inset());
        if !finite_num(ni) || ni < 0 { throw `poster_header(): number_inset is a length in cm >= 0 (got ${ni})`; }
        if ni > 0 { n = n.margin(#{ right: ni }); }
        if where == "above" {
            above = n.align("right");
        } else {
            let t = if type_of(tnode) == "()" { rect() } else { tnode };
            tnode = row([t.flex(1), n]).cross("start").spacing(poster_gutter());
        }
    }
    let main = if type_of(tnode) == "()" { [] } else { [tnode] };
    if type_of(above) != "()" { main.insert(0, above); }
    let authors = opt(opts, "authors", []);
    if type_of(authors) != "array" || authors.len() > 0 {
        main.push(author_line(authors, opt(opts, "affiliations", [])));
    }
    let parts = [];
    let lb = opt(opts, "logo_box", poster_logo_box());
    if type_of(lb) != "()" {
        lb = numbers("poster_header", "`logo_box`", lb);
        if lb.len() != 2 || !(lb[0] > 0.0) || !(lb[1] > 0.0) {
            throw "poster_header(): logo_box is [w, h] in cm";
        }
    }
    for l in opt(opts, "logos", []) {
        let n = if type_of(l) == "string" { image(l) } else { l };
        parts.push(if type_of(lb) == "()" { n.height(poster_logo_height()) } else {
            // A fixed box, the logo contained and centred in it.
            if n.kind() == "image" { n = n.fit("contain"); }
            col([n.flex(1)]).width(lb[0]).height(lb[1]).stretch()
        });
    }
    parts.push(col(main).main("center").stretch().spacing(author_size() * 0.008).flex(1));
    let head = row(parts).stretch().spacing(poster_gutter());
    let rule = poster_header_rule();
    if rule == "" { return head; }
    col([head, rect().height(poster_header_rule_width()).fill(rule)])
        .spacing(poster_row_gap() * 0.5).stretch()
}
