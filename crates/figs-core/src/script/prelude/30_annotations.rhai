// ---------------------------------------------------------------------------
// Annotation hooks
// ---------------------------------------------------------------------------

// Colour of callouts, highlights and spots: a warm red that reads on both
// light plots and dark micrographs.
fn accent() { style_get("accent", "#ff5a3c") }

// Stroke width of callout arrows, highlights and spots (cm): 0.04 cm (~1.1 pt)
// at the default 8 pt, so annotations scale with the text.
fn annotation_width() { style_get("annotation_width", base_size() * 0.005) }

// Dash pattern of `highlight` / `spot` (on, off, … in cm), tied to the stroke.
fn annotation_dash() {
    style_get("annotation_dash", [annotation_width() * 3, annotation_width() * 2])
}

// Scale bar and its label colour. White is the micrograph convention (TEM/SEM/
// fluorescence images are mostly dark); `scalebar_halo()` keeps it legible on
// bright regions.
fn scalebar_color() { style_get("scalebar_color", "#ffffff") }

// Bar thickness (cm): 0.08 cm (~2.3 pt) at the default 8 pt.
fn scalebar_thickness() { style_get("scalebar_thickness", base_size() * 0.01) }

// Corner of the panel the scale bar sits in (same names as `label_corner()`).
fn scalebar_corner() { style_get("scalebar_corner", "bottom-right") }

// Distance from the panel corner to the scale bar (cm); follows the label inset.
fn scalebar_inset() { style_get("scalebar_inset", label_inset()) }

// Thin outline around the bar and its label, for contrast on bright image
// regions; "" draws neither.
fn scalebar_halo() { style_get("scalebar_halo", "#000000a0") }

// Side inset of a field-scaled `scalebar(len, field, s)` as a fraction of the
// panel width (that form is laid out in fractions, so a cm inset can't apply).
fn scalebar_frac_inset() { style_get("scalebar_frac_inset", 0.025) }
// A backing box behind the label and bar (`scalebar(len, s, #{ box })`): its
// fill ("" = none, the default: a halo instead), padding (cm) and radius.
fn scalebar_box() { style_get("scalebar_box", "") }
fn scalebar_box_pad() { style_get("scalebar_box_pad", scalebar_thickness() * 1.5) }
fn scalebar_box_radius() { style_get("scalebar_box_radius", 0) }

// ---------------------------------------------------------------------------
// Annotations (overlays on a panel; coordinates are cm from its top-left)
// ---------------------------------------------------------------------------
//
// Each annotation is one core node meant to be a child of a `stack` over the
// image — `annotate(item, marks)` builds that stack. Colours, widths and dashes
// come from the hooks in the Defaults block (`accent()`, `annotation_width()`,
// `scalebar_*()`), and every result still takes core setters.

// `item` with annotation node(s) `marks` overlaid: `stack([item, ..marks])`.
// `marks` is an array or a single node. Composes with labels:
// `labeled(annotate(img, marks), "(a)")`, or `panels([annotate(..), ..])`.
fn annotate(item, marks) {
    stack([as_node(item)] + as_array(marks))
}

// The scale bar's label: bold, base size, centered, in `scalebar_color()`,
// outlined in `scalebar_halo()` (a `halo` reaching a quarter of the bar
// thickness past the glyphs).
fn scalebar_label(s) {
    let t = text(s).size(base_size()).bold().center().color(scalebar_color());
    halo_if(t, scalebar_halo(), scalebar_thickness() * 0.25)
}

// The bar itself: a filled rect `scalebar_thickness()` tall, outlined in
// `scalebar_halo()`. Width is set by the caller (cm or a flex share).
fn scalebar_rect() {
    let t = scalebar_thickness();
    let bar = rect().height(t).fill(scalebar_color());
    let halo = scalebar_halo();
    if halo == "" { bar } else { bar.stroke(halo).stroke_width(t * 0.15) }
}

// A scale bar `len` cm long with label `s` centered above it ("" for no label),
// pinned `scalebar_inset()` in from `scalebar_corner()` of the panel.
// `scalebar(len, s, opts)`:
//   box     a fill behind label and bar ("#0d0d0dcc"; true = `scalebar_box()`
//           or a translucent black), padded `scalebar_box_pad()`, corners
//           `scalebar_box_radius()`; a boxed bar needs no halo, so it has none
//   pad, radius  the box's (cm)
//   inset   cm from the panel edges (default `scalebar_inset()`; 0 = flush)
//   corner  default `scalebar_corner()`
//   color   bar and label (default `scalebar_color()`)
fn scalebar(len, s) { scalebar(len, s, #{}) }

// A scale bar in image/physical units: `len` out of a field `field` wide, in
// the same unit — `scalebar(1, 12.5, "1 μm")` for a 12.5 μm field of view, or
// `scalebar(80, 1024, "1 μm")` for 80 px of a 1024 px wide image. The bar is
// exactly len/field of the panel width (flex shares), so it stays correct
// whatever size the layout gives the panel — provided the image spans the full
// panel width (natural aspect, as on auto-height pages; not letterboxed).
// Sideways it sits `scalebar_frac_inset()` of the width in from the edge.
// (A third argument that is a map is `scalebar(len, s, opts)` above.)
fn scalebar(len, field, s) {
    if type_of(s) == "map" { return scalebar_cm(len, field, s); }
    scalebar_field(len, field, s)
}

fn scalebar_cm(len, s, opts) {
    check_opts("scalebar", opts, ["box", "pad", "radius", "inset", "corner", "color"]);
    let box = opt(opts, "box", scalebar_box());
    if type_of(box) == "bool" {
        box = if !box { "" } else if scalebar_box() != "" { scalebar_box() } else { "#000000b3" };
    }
    let corner = opt(opts, "corner", scalebar_corner());
    let st = #{};
    if opts.contains("color") { st.scalebar_color = opts.color; }
    if has_paint(box) { st.scalebar_halo = ""; }
    style_push(st);
    let bar = scalebar_rect().width(len);
    let parts = if s == "" { [bar] } else { [scalebar_label(s), bar] };
    let tag = col(parts).cross("center").spacing(scalebar_thickness() * 0.5);
    if has_paint(box) {
        tag = tag.pad(opt(opts, "pad", scalebar_box_pad())).fill(box);
        let r = opt(opts, "radius", scalebar_box_radius());
        if r > 0 { tag = tag.radius(r); }
    }
    style_pop();
    pin_in("scalebar", tag, corner, opt(opts, "inset", scalebar_inset()))
}

fn scalebar_field(len, field, s) {
    if !(field > 0) || !(len > 0) || len > field {
        throw `scalebar(): need 0 < len <= field (got ${len} of ${field})`;
    }
    let corner = check_corner("scalebar_corner", scalebar_corner());
    let right = corner.ends_with("right");
    let m = scalebar_frac_inset() * field;      // side inset, field units
    let rest = field - len - m;                 // free space on the far side
    if rest < 0 { rest = 0; }
    // Bar row: [rest | bar | m] (mirrored on the left).
    let bar = if right { frac_row(rest, scalebar_rect(), len, m) }
              else { frac_row(m, scalebar_rect(), len, rest) };
    let t = scalebar_thickness();
    let parts = [bar];
    if s != "" {
        // Label cell centered on the bar, reaching the near panel edge.
        let lead = rest - m;
        if lead < 0 { lead = 0; }
        let cell = field - lead;
        let lbl = if right { frac_row(lead, scalebar_label(s), cell, 0) }
                  else { frac_row(0, scalebar_label(s), cell, lead) };
        parts = [lbl, rect().height(t * 0.5), bar];
    }
    let v = rect().height(scalebar_inset());
    if corner.starts_with("bottom") {
        col(parts + [v]).main("end").stretch()
    } else {
        col([v] + parts).stretch()
    }
}

// Row of [spacer, node, spacer] with flex weights; zero-weight spacers dropped.
fn frac_row(lead, node, weight, trail) {
    let parts = [];
    if lead > 0 { parts.push(rect().flex(lead)); }
    parts.push(node.flex(weight));
    if trail > 0 { parts.push(rect().flex(trail)); }
    row(parts)
}

// An arrow from (tx, ty) to (x, y) — the core `arrow(tx, ty, x, y)` — with
// label `s` at its tail, in `accent()`. The label sits beside the tail on the
// side away from the target (left of it when the target is to the right, else
// right of it), vertically centered on `ty`. A node `s` is used as is.
fn callout(tx, ty, x, y, s) {
    let t = if type_of(s) == "string" {
        text(s).size(base_size()).bold().color(accent())
    } else { s };
    let gap = base_size() * 0.01;       // tail to text (cm)
    let k = base_size() * 0.053;        // half the label box: 1.5 font sizes (cm)
    let tag = if x >= tx {
        // Right edge at the tail: a wide right-aligned box hanging off to the
        // left (offsets may be negative), so long labels never wrap.
        let reach = 50;
        col([t]).cross("end").main("center").width(reach).height(2 * k)
            .at(tx - gap - reach, ty - k)
    } else {
        col([t]).main("center").height(2 * k).at(tx + gap, ty - k)
    };
    let a = arrow(tx, ty, x, y).stroke(accent()).stroke_width(annotation_width());
    stack([tag, a])
}

// A dashed box with its top-left at (x, y), `w` x `h` cm, in `accent()`
// (a closed core `path`; `.dash([])` makes it solid).
fn highlight(x, y, w, h) {
    path([[0, 0], [w, 0], [w, h], [0, h]]).closed()
        .stroke(accent()).stroke_width(annotation_width()).dash(annotation_dash())
        .at(x, y)
}

// A dashed circle of radius `r` centered on (cx, cy), in `accent()`.
fn spot(cx, cy, r) {
    circle(cx, cy, r).stroke(accent()).stroke_width(annotation_width())
        .dash(annotation_dash())
}
