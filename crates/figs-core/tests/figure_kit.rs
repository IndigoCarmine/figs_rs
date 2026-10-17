//! The framework's figure kit (`prelude/45_figure_kit.rhai`): `labware`
//! glassware icons, `vector_note` arrows across a panel, `colorbar` scales
//! and `cycle_arrows`. Structure is checked on the raw document, geometry on
//! the layout (fixed text metrics, page units in pt for exact numbers).

#![cfg(feature = "script")]

use figs_core::geom::{ArrowHead, Paint, Rect, Size};
use figs_core::layout::{ComputedLayout, PaintContent};
use figs_core::schema::{ImageProps, NodeKind, RawDocument, RawNode, TextProps};
use figs_core::{eval_script, layout, Document, LeafMeasure};

/// Every text block measures `TEXT_W` x `TEXT_H` pt.
struct FixedText;
const TEXT_W: f32 = 20.0;
const TEXT_H: f32 = 10.0;
impl LeafMeasure for FixedText {
    fn measure_text(&self, _t: &TextProps, _max_width: f32) -> Size {
        Size::new(TEXT_W, TEXT_H)
    }
    fn measure_image(&self, _i: &ImageProps) -> Size {
        Size::default()
    }
}

fn raw(src: &str) -> RawDocument {
    eval_script(src).unwrap_or_else(|e| panic!("script failed: {e}"))
}

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &FixedText)
}

fn fail(src: &str) -> String {
    eval_script(src).expect_err("script should fail").message
}

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-3, "expected {b}, got {a}");
}

fn text_rect(l: &ComputedLayout, content: &str) -> Rect {
    l.nodes
        .iter()
        .find(|n| matches!(&n.content, PaintContent::Text(t) if t.content == content))
        .unwrap_or_else(|| panic!("no text {content:?} in layout"))
        .rect
}

fn rect_of(l: &ComputedLayout, id: &str) -> Rect {
    l.nodes.iter().find(|n| n.id == id).unwrap().rect
}

fn nodes(d: &RawDocument, f: impl Fn(&RawNode) -> bool) -> Vec<(&str, &RawNode)> {
    let mut v: Vec<_> = d
        .nodes
        .iter()
        .filter(|(_, n)| f(n))
        .map(|(k, n)| (k.as_str(), n))
        .collect();
    v.sort_by_key(|(k, _)| {
        k.strip_prefix("node")
            .map_or(0, |n| n.parse::<usize>().unwrap())
    });
    v
}

fn solid(p: &Option<Paint>) -> Option<String> {
    match p {
        Some(Paint::Solid(c)) => Some(c.to_hex()),
        _ => None,
    }
}

// ---- labware --------------------------------------------------------------------

const KINDS: [&str; 9] = [
    "vial",
    "tube",
    "flask",
    "erlenmeyer",
    "beaker",
    "funnel",
    "dish",
    "cuvette",
    "syringe",
];

#[test]
fn labware_draws_every_kind_at_one_height() {
    for kind in KINDS {
        // Default: `labware_size()` tall (8 pt base: 1.2 cm), empty.
        let d = raw(&format!(r##"page(10, 10, labware("{kind}"))"##));
        let root = &d.nodes["root"];
        assert_eq!(root.kind, NodeKind::Stack, "{kind}");
        let h = root.height.unwrap();
        let w = root.width.unwrap();
        let sw = 8.0 * 0.0045;
        if kind == "dish" {
            approx(h, 0.42 * (1.2 - sw) + sw);
        } else {
            approx(h, 1.2);
        }
        assert!(w > 0.2 && w < 2.0, "{kind}: width {w}");
        let liquid = nodes(&d, |n| solid(&n.fill).as_deref() == Some("#9cc8ea"));
        assert!(liquid.is_empty(), "{kind}: empty by default");
        // Every outline in the line colour and width, inside the box.
        let lines = nodes(&d, |n| n.stroke.is_some());
        assert!(!lines.is_empty(), "{kind}");
        for (_, n) in &lines {
            assert_eq!(n.stroke.unwrap().to_hex(), "#000000");
            for p in &n.points {
                assert!(
                    p[0] >= 0.0 && p[0] <= w + 1e-4 && p[1] >= 0.0 && p[1] <= h + 1e-4,
                    "{kind}: {p:?}"
                );
            }
        }
        // With liquid: one filled shape in the liquid colour, drawn first.
        let d = raw(&format!(
            r##"page(10, 10, labware("{kind}", #{{ level: 0.5, liquid: "#ff0000", size: 2, stroke: "#0000ff", stroke_width: 0.05 }}))"##
        ));
        let root = &d.nodes["root"];
        if kind != "dish" {
            approx(root.height.unwrap(), 2.0);
        }
        let wet = nodes(&d, |n| solid(&n.fill).as_deref() == Some("#ff0000"));
        assert_eq!(wet.len(), 1, "{kind}");
        assert_eq!(root.children[0], wet[0].0, "{kind}: liquid first");
        for (_, n) in nodes(&d, |n| n.stroke.is_some()) {
            assert_eq!(n.stroke.unwrap().to_hex(), "#0000ff");
            // Full-weight outlines, thinner marks (graduations, a needle).
            let w = n.stroke_width.unwrap();
            assert!(w > 0.0 && w <= 0.05 + 1e-6, "{kind}: {w}");
        }
    }
}

#[test]
fn labware_hooks_and_errors() {
    let d = raw(
        r##"style(#{ labware_size: 3, labware_liquid: "#00ff00", labware_level: 0.4, labware_stroke: "#123456" });
           page(10, 10, labware("beaker"))"##,
    );
    approx(d.nodes["root"].height.unwrap(), 3.0);
    assert_eq!(
        nodes(&d, |n| solid(&n.fill).as_deref() == Some("#00ff00")).len(),
        1
    );
    assert!(nodes(&d, |n| n.stroke.is_some())
        .iter()
        .all(|(_, n)| n.stroke.unwrap().to_hex() == "#123456"));
    // A dish without its lid draws one outline fewer.
    let lines = |src: &str| nodes(&raw(src), |n| n.stroke.is_some()).len();
    assert_eq!(
        lines(r##"page(10, 10, labware("dish"))"##),
        lines(r##"page(10, 10, labware("dish", #{ lid: false }))"##) + 1
    );
    assert!(fail(r##"page(1, 1, labware("retort"))"##).contains("unknown kind"));
    assert!(fail(r##"page(1, 1, labware("vial", #{ level: 2 }))"##).contains("level"));
    assert!(fail(r##"page(1, 1, labware("vial", #{ lid: false }))"##).contains("dish"));
    assert!(
        fail(r##"page(1, 1, labware("vial", #{ colour: "#000000" }))"##).contains("unknown option")
    );
    assert!(fail(r##"page(1, 1, labware("vial", #{ size: 0.01 }))"##).contains("size"));
}

#[test]
fn labware_lines_up_in_a_scheme() {
    // Icons of one height on one axis: equal tops in a centred scheme.
    let l = lay(
        r##"page(400, 200, scheme([labware("vial"), arrow_step(), labware("flask"),
                   arrow_step(), labware("syringe")])).unit("pt")"##,
    );
    let d = raw(
        r##"page(400, 200, scheme([labware("vial"), arrow_step(), labware("flask"),
                   arrow_step(), labware("syringe")])).unit("pt")"##,
    );
    let icons: Vec<Rect> = d.nodes["root"]
        .children
        .iter()
        .step_by(2)
        .map(|id| rect_of(&l, id))
        .collect();
    assert_eq!(icons.len(), 3);
    for r in &icons {
        approx(r.y, icons[0].y);
        approx(r.h, icons[0].h);
    }
}

// ---- vector_note -----------------------------------------------------------------

/// The arrow of a vector note (the one stroked arrow in the document).
fn arrow(d: &RawDocument) -> &RawNode {
    let a = nodes(d, |n| n.kind == NodeKind::Arrow);
    assert_eq!(a.len(), 1);
    a[0].1
}

#[test]
fn vector_note_draws_an_arrow_across_the_item() {
    let src = r##"page(200, 100, vector_note(rect().width(100).height(80), "m")).unit("pt")"##;
    let d = raw(src);
    let a = arrow(&d);
    // Default: a left-to-right diagonal upwards, in fractions of the item.
    assert_eq!(a.points, [[0.2, 0.75], [0.8, 0.25]]);
    assert_eq!(a.viewbox, Some([0.0, 0.0, 1.0, 1.0]));
    assert_eq!(a.head, Some(ArrowHead::End));
    assert_eq!(a.stroke.unwrap().to_hex(), "#ff5a3c", "accent()");
    let m = nodes(&d, |n| n.content.as_deref() == Some("m"));
    assert_eq!(m[0].1.color.unwrap().to_hex(), "#ff5a3c");
    assert!(m[0].1.stroke.is_some(), "haloed like a panel label");
    // Layout: the label sits beyond the head, inside the item's box (the
    // overlay gives up what it would hang out by).
    let l = lay(src);
    let item = nodes(&d, |n| n.kind == NodeKind::Rect && n.width == Some(100.0))[0].0;
    let item = rect_of(&l, item);
    let t = text_rect(&l, "m");
    assert!(
        t.x >= item.x - 1e-3 && t.x + t.w <= item.x + item.w + 1e-3,
        "{t:?} in {item:?}"
    );
    assert!(t.y >= item.y - 1e-3, "{t:?} in {item:?}");
    let ar = l
        .nodes
        .iter()
        .find(|n| matches!(&n.content, PaintContent::Path(_)))
        .unwrap();
    // The head (0.8, 0.25 of the arrow's box) is left of the label, which
    // sits above its middle (the arrow comes from the lower left).
    let (hx, hy) = (ar.rect.x + 0.8 * ar.rect.w, ar.rect.y + 0.25 * ar.rect.h);
    assert!(hx <= t.x + 1e-3, "{:?} vs {t:?}", ar.rect);
    assert!(
        t.y + t.h / 2.0 < hy && t.y + t.h > hy - 1e-3,
        "{:?} vs {t:?}",
        ar.rect
    );
    // Centred on a bigger box, hugging the item.
    approx(item.x + item.w / 2.0, 100.0);
}

#[test]
fn vector_note_options_and_errors() {
    let d = raw(
        r##"style(#{ vector_note_color: "#0000ff", vector_note_width: 0.2 });
           page(10, 10, vector_note(rect(), "", #{ from: [0.5, 1], to: [0.5, 0], head: "both" }))"##,
    );
    let a = arrow(&d);
    assert_eq!(a.points, [[0.5, 1.0], [0.5, 0.0]]);
    assert_eq!(a.head, Some(ArrowHead::Both));
    assert_eq!(a.stroke.unwrap().to_hex(), "#0000ff");
    assert_eq!(a.stroke_width, Some(0.2));
    assert!(
        nodes(&d, |n| n.kind == NodeKind::Text).is_empty(),
        "no label"
    );
    let d = raw(r##"page(10, 10, vector_note(rect(), text("N"), #{ color: "#00ff00" }))"##);
    assert_eq!(arrow(&d).stroke.unwrap().to_hex(), "#00ff00");
    for (bad, msg) in [
        (r##"vector_note(rect(), "a", #{ to: [1] })"##, "[fx, fy]"),
        (
            r##"vector_note(rect(), "a", #{ from: [0.5, 0.5], to: [0.5, 0.5] })"##,
            "coincide",
        ),
        (
            r##"vector_note(rect(), "a", #{ head: "x" })"##,
            "unknown head",
        ),
        (
            r##"vector_note(rect(), "a", #{ angle: 3 })"##,
            "unknown option",
        ),
        (r##"vector_note(rect(), 5)"##, "label"),
    ] {
        assert!(fail(&format!("page(1, 1, {bad})")).contains(msg), "{bad}");
    }
}

// ---- colorbar --------------------------------------------------------------------

#[test]
fn colorbar_horizontal_spreads_its_labels_along_the_bar() {
    let src = r##"fn colorbar_thickness() { 6 } fn colorbar_gap() { 2 } fn colorbar_length() { 100 }
        page(200, 100, col([colorbar(["#000000", "#ffffff"], [0, 50, 100])])).unit("pt")"##;
    let d = raw(src);
    let bar = nodes(&d, |n| n.kind == NodeKind::Rect)[0].1;
    assert!(matches!(bar.fill, Some(Paint::Linear { .. })), "a gradient");
    assert_eq!(bar.height, Some(6.0));
    assert!(bar.stroke.is_some(), "a border by default");
    let l = lay(src);
    let b = rect_of(&l, nodes(&d, |n| n.kind == NodeKind::Rect)[0].0);
    assert_eq!((b.x, b.y, b.w, b.h), (0.0, 0.0, 100.0, 6.0));
    // Ends flush with the bar's ends, the middle centred, all 2 pt below.
    let (lo, mid, hi) = (
        text_rect(&l, "0"),
        text_rect(&l, "50"),
        text_rect(&l, "100"),
    );
    approx(lo.x, 0.0);
    approx(mid.x + mid.w / 2.0, 50.0);
    approx(hi.x + hi.w, 100.0);
    for t in [lo, mid, hi] {
        approx(t.y, 8.0);
    }
}

#[test]
fn colorbar_vertical_runs_low_to_high_upwards() {
    let src = r##"fn colorbar_thickness() { 6 } fn colorbar_gap() { 2 }
        page(200, 100, col([colorbar(["#000000", "#ffffff"], ["lo", "hi"],
            #{ direction: "vertical", length: 80, border: "" })])).unit("pt")"##;
    let d = raw(src);
    let bar = nodes(&d, |n| n.kind == NodeKind::Rect)[0];
    assert_eq!(bar.1.width, Some(6.0));
    assert_eq!(bar.1.stroke, None);
    let Some(Paint::Linear { .. }) = &bar.1.fill else {
        panic!("gradient")
    };
    let l = lay(src);
    let b = rect_of(&l, bar.0);
    assert_eq!((b.w, b.h), (6.0, 80.0));
    let (lo, hi) = (text_rect(&l, "lo"), text_rect(&l, "hi"));
    approx(lo.y + lo.h, b.y + b.h);
    approx(hi.y, b.y);
    approx(lo.x, b.x + 8.0);
}

#[test]
fn colorbar_discrete_title_fill_and_errors() {
    let d = raw(
        r##"page(200, 100, col([colorbar(["#ff0000", "#00ff00", "#0000ff"], ["A", "B", "C"],
            #{ discrete: true, title: "Phase", length: "fill" })]).stretch()).unit("pt")"##,
    );
    let fills: Vec<String> = nodes(&d, |n| n.kind == NodeKind::Rect)
        .iter()
        .filter_map(|(_, n)| solid(&n.fill))
        .collect();
    assert_eq!(fills, ["#ff0000", "#00ff00", "#0000ff"]);
    assert!(!nodes(&d, |n| n.content.as_deref() == Some("Phase")).is_empty());
    // "fill": as wide as its parent gives it.
    let l = lay(
        r##"page(200, 100, col([colorbar(["#ff0000", "#0000ff"], ["A", "B"], #{ length: "fill" })]).stretch()).unit("pt")"##,
    );
    approx(text_rect(&l, "B").x + TEXT_W, 200.0);
    for (bad, msg) in [
        (
            r##"colorbar(["#000000"], [], #{ direction: "up" })"##,
            "unknown direction",
        ),
        (r##"colorbar(["#000000"], [], #{ length: -1 })"##, "length"),
        (
            r##"colorbar("#000000", [], #{ discrete: true })"##,
            "discrete",
        ),
        (
            r##"colorbar(["#000000"], [], #{ ticks: 3 })"##,
            "unknown option",
        ),
    ] {
        assert!(fail(&format!("page(1, 1, {bad})")).contains(msg), "{bad}");
    }
}

// ---- cycle_arrows ----------------------------------------------------------------

/// The arcs of `cycle_arrows`: (id, start, end) of each curved arrow.
fn arcs(d: &RawDocument) -> Vec<(&str, [f32; 2], [f32; 2], &RawNode)> {
    nodes(d, |n| {
        n.head == Some(ArrowHead::End) && !n.segments.is_empty()
    })
    .into_iter()
    .map(|(id, n)| (id, n.points[0], n.segments[0].to, n))
    .collect()
}

#[test]
fn cycle_arrows_turn_around_the_item() {
    let src = r##"fn cycle_arrow_depth() { 10 } fn cycle_arrow_gap() { 2 }
        page(200, 200, cycle_arrows(rect().width(60).height(40),
            #{ colors: ["#ff0000", "#0000ff"], labels: ["UV", "Vis"] })).unit("pt")"##;
    let d = raw(src);
    let a = arcs(&d);
    assert_eq!(a.len(), 2);
    // Clockwise: over the top left to right, under the bottom right to left.
    assert!(a[0].1[0] < a[0].2[0] && a[0].1[1] == 1.0, "{:?}", a[0].1);
    assert!(a[1].1[0] > a[1].2[0] && a[1].1[1] == 0.0, "{:?}", a[1].1);
    assert_eq!(a[0].3.stroke.unwrap().to_hex(), "#ff0000");
    assert_eq!(a[1].3.stroke.unwrap().to_hex(), "#0000ff");
    let l = lay(src);
    let item = rect_of(&l, nodes(&d, |n| n.width == Some(60.0))[0].0);
    let top = rect_of(&l, a[0].0);
    let bottom = rect_of(&l, a[1].0);
    approx(top.y + top.h, item.y);
    approx(bottom.y, item.y + item.h);
    approx(top.h, 10.0);
    // The arcs span the item; the labels beyond them, centred, in colour.
    approx(top.w, 60.0);
    let (uv, vis) = (text_rect(&l, "UV"), text_rect(&l, "Vis"));
    approx(uv.y + uv.h + 2.0, top.y);
    approx(vis.y, bottom.y + bottom.h + 2.0);
    approx(uv.x + uv.w / 2.0, item.x + item.w / 2.0);
    let uvn = nodes(&d, |n| n.content.as_deref() == Some("UV"))[0].1;
    assert_eq!(uvn.color.unwrap().to_hex(), "#ff0000");
    // Centred in its box.
    approx(item.x + item.w / 2.0, 100.0);
}

#[test]
fn cycle_arrows_anticlockwise_sideways_and_errors() {
    let d = raw(r##"page(10, 10, cycle_arrows("A", #{ direction: "ccw" }))"##);
    let a = arcs(&d);
    assert!(a[0].1[0] > a[0].2[0], "ccw: the top arc runs right to left");
    let src = r##"fn cycle_arrow_depth() { 10 }
        page(200, 200, cycle_arrows(rect().width(60).height(40), #{ sides: "left-right" })).unit("pt")"##;
    let d = raw(src);
    let a = arcs(&d);
    // Clockwise: up the left side, down the right.
    assert!(a[0].1[1] > a[0].2[1] && a[0].1[0] == 1.0);
    assert!(a[1].1[1] < a[1].2[1] && a[1].1[0] == 0.0);
    let l = lay(src);
    let item = rect_of(&l, nodes(&d, |n| n.width == Some(60.0))[0].0);
    let left = rect_of(&l, a[0].0);
    approx(left.x + left.w, item.x);
    approx(left.w, 10.0);
    for (bad, msg) in [
        (
            r##"cycle_arrows("A", #{ direction: "up" })"##,
            "unknown direction",
        ),
        (r##"cycle_arrows("A", #{ sides: "top" })"##, "unknown sides"),
        (r##"cycle_arrows("A", #{ colors: ["#000000"] })"##, "colors"),
        (r##"cycle_arrows("A", #{ labels: "x" })"##, "labels"),
        (r##"cycle_arrows("A", #{ span: 2 })"##, "span"),
        (r##"cycle_arrows("A", #{ radius: 2 })"##, "unknown option"),
    ] {
        assert!(fail(&format!("page(1, 1, {bad})")).contains(msg), "{bad}");
    }
}

#[test]
fn kit_hooks_are_style_keys() {
    raw(
        r##"style(#{ labware_size: 1, labware_stroke: "#000000", labware_stroke_width: 0.02,
            labware_liquid: "#ffffff", labware_level: 0.5, vector_note_color: "#000000",
            vector_note_width: 0.1, vector_note_size: 9, vector_note_gap: 0.1,
            colorbar_length: 3, colorbar_thickness: 0.2, colorbar_label_size: 7,
            colorbar_gap: 0.1, colorbar_border: "", colorbar_border_width: 0.01,
            cycle_arrow_color: "#000000", cycle_arrow_width: 0.03, cycle_arrow_depth: 0.3,
            cycle_arrow_span: 0.5, cycle_arrow_gap: 0.1, bullet_pitch: 0.6,
            table_rule_mode: "overlay", ytick_label_align: "box" });
           page(1, 1, rect())"##,
    );
}
