// --- the page ---

// `poster(header, body)` / `poster(preset, header, body)`: a `poster_size()`
// (or `preset`) page on `poster_bg()`; the header — a node, a map for
// `poster_header`, or () for none — above the body — an array of rows for
// `rows`, or one node — `poster_margin()` in from the edges.
fn poster(header, body) { poster(poster_size(), header, body) }
fn poster(preset, header, body) {
    if type_of(header) == "map" { header = poster_header(header); }
    let content = if type_of(body) == "array" { rows(body) } else { as_node(body) };
    let parts = if type_of(header) == "()" { [content.flex(1)] } else { [header, content.flex(1)] };
    let root = col(parts).spacing(poster_row_gap()).pad(poster_margin()).stretch();
    let d = page(preset, root);
    let bg = poster_bg();
    if poster_has_paint(bg) { d.background(bg) } else { d }
}
