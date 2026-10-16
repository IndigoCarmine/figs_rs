//! Poster components of the framework prelude (`prelude.rhai`, "Poster"):
//! `poster` / `poster_header` / `author_line`, auto-numbered `section`s,
//! `columns` / `rows` with dashed separators in the gutters, `frame`, `note`,
//! `ref`, `subheading`, `bullets`, `table`, and the `"poster"` /
//! `"poster-dark"` styles. Structure is checked on the raw document, geometry
//! on the layout (fixed text metrics, page units in pt for exact numbers).

#![cfg(feature = "script")]

use std::path::Path;

use figs_core::geom::{Color, Paint, PathSegment, Rect, Size, TextAlign};
use figs_core::layout::{ComputedLayout, PaintContent};
use figs_core::schema::span::Script;
use figs_core::schema::{ImageProps, NodeKind, RawDocument, RawNode, TextProps};
use figs_core::units::Unit;
use figs_core::{eval_script, layout, Document, LeafMeasure, NullMeasurer};

/// Every text block measures `TEXT_W` x `TEXT_H` pt (an explicit width wins).
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

fn lay_fixed(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &FixedText)
}

fn fail(src: &str) -> String {
    eval_script(src).expect_err("script should fail").message
}

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-3, "expected {b}, got {a}");
}

fn assert_xywh(r: Rect, x: f32, y: f32, w: f32, h: f32) {
    approx(r.x, x);
    approx(r.y, y);
    approx(r.w, w);
    approx(r.h, h);
}

fn hex(s: &str) -> Option<Color> {
    Color::parse_hex(s).ok()
}

fn solid(p: &Option<Paint>) -> String {
    p.clone()
        .and_then(|p| p.as_solid())
        .map(|c| c.to_hex())
        .unwrap_or_default()
}

const CM: f32 = 72.0 / 2.54;

/// Raw children of `id`, in order.
fn children<'a>(d: &'a RawDocument, id: &str) -> Vec<(&'a str, &'a RawNode)> {
    d.nodes[id]
        .children
        .iter()
        .map(|c| (c.as_str(), &d.nodes[c]))
        .collect()
}

/// Text contents of the text nodes, in paint (pre-order) order.
fn texts(d: &RawDocument) -> Vec<String> {
    let mut v: Vec<(usize, String)> = d
        .nodes
        .iter()
        .filter(|(_, n)| n.kind == NodeKind::Text)
        .map(|(id, n)| (order(id), plain(n)))
        .collect();
    v.sort();
    v.into_iter().map(|(_, s)| s).collect()
}

/// The text of a text node: its content, or its spans joined.
fn plain(n: &RawNode) -> String {
    n.content
        .clone()
        .unwrap_or_else(|| n.spans.iter().map(|s| s.text.as_str()).collect())
}

/// The spans of a text node as (text, superscript?) pairs.
fn spans(n: &RawNode) -> Vec<(String, bool)> {
    n.spans
        .iter()
        .map(|s| (s.text.clone(), s.script == Some(Script::Super)))
        .collect()
}

fn order(id: &str) -> usize {
    id.strip_prefix("node").map_or(0, |n| n.parse().unwrap())
}

/// The (first) raw text node with this content, and its id.
fn text_node<'a>(d: &'a RawDocument, content: &str) -> (&'a str, &'a RawNode) {
    let mut v: Vec<_> = d
        .nodes
        .iter()
        .filter(|(_, n)| n.kind == NodeKind::Text && n.content.as_deref() == Some(content))
        .collect();
    v.sort_by_key(|(id, _)| order(id));
    let (id, n) = v
        .first()
        .unwrap_or_else(|| panic!("no text node {content:?}"));
    (id.as_str(), n)
}

/// Layout rect of the (first) text with this content.
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

/// Rects of the filled `rect` leaves (table rules), in paint order.
fn filled_rects(l: &ComputedLayout) -> Vec<Rect> {
    l.nodes
        .iter()
        .filter(|n| matches!(&n.content, PaintContent::Rect { fill: Some(_), .. }))
        .map(|n| n.rect)
        .collect()
}

/// The dashed separators: (start, end) of every stroked 2-point path.
fn separators(l: &ComputedLayout) -> Vec<([f32; 2], [f32; 2], Vec<f32>)> {
    l.nodes
        .iter()
        .filter_map(|n| match &n.content {
            PaintContent::Path(p) => match p.segments.as_slice() {
                [PathSegment::MoveTo(a), PathSegment::LineTo(b)] => Some((
                    [a.x, a.y],
                    [b.x, b.y],
                    p.stroke
                        .as_ref()
                        .map(|s| s.dash.clone())
                        .unwrap_or_default(),
                )),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

// ---- sections ------------------------------------------------------------------

#[test]
fn section_is_a_bar_over_padded_content() {
    let d = raw(r#"style("poster"); page(30, 20, section("Intro", [rect(), "text"])).unit("cm")"#);
    let kids = children(&d, "root");
    assert_eq!(kids.len(), 2);
    let (bar_id, bar) = kids[0];
    // The bar: a unit-viewbox stack with the title anchored on its centre,
    // 53.52 pt titles on a 2.3 cm bar at poster scale, square, gradient fill.
    assert_eq!(bar.kind, NodeKind::Stack);
    assert_eq!(bar.viewbox, Some([0.0, 0.0, 1.0, 1.0]));
    approx(bar.height.unwrap(), 24.0 * 2.23 * 0.043);
    assert!(matches!(bar.fill, Some(Paint::Linear { .. })));
    assert_eq!(bar.stroke, None, "the light bar has no border");
    assert_eq!(bar.corner_radius, None, "square by default");
    let (_, title) = children(&d, bar_id)[0];
    assert_eq!(title.content.as_deref(), Some("1. Intro"));
    approx(title.font_size.unwrap(), 24.0 * 2.23);
    assert_eq!(title.color, hex("#ffffff"));
    assert_eq!(title.anchor, Some([0.5.into(), 0.5.into()]));
    assert_eq!((title.x, title.y), (Some(0.5), Some(0.5)));
    // The content: padded `section_pad()`, items `section_gap()` apart, the
    // bare string a body text.
    let (body_id, body) = kids[1];
    approx(body.padding.left, 24.0 * 0.0125);
    approx(body.spacing, 24.0 * 0.02);
    assert_eq!(body.flex, Some(1.0));
    let items = children(&d, body_id);
    assert_eq!(items[0].1.kind, NodeKind::Rect);
    assert_eq!(items[1].1.font_size, Some(24.0));
}

#[test]
fn section_geometry_title_centred_on_the_bar() {
    let src = r#"
        fn section_bar_height() { 30 }
        fn section_pad() { 4 }
        page(200, 100, section("A", rect())).unit("pt")
    "#;
    let l = lay_fixed(src);
    let d = raw(src);
    let kids = children(&d, "root");
    assert_xywh(rect_of(&l, kids[0].0), 0.0, 0.0, 200.0, 30.0);
    assert_xywh(rect_of(&l, kids[1].0), 0.0, 30.0, 200.0, 70.0);
    // The title's box is centred on the bar.
    let t = text_rect(&l, "1. A");
    approx(t.x + t.w / 2.0, 100.0);
    approx(t.y + t.h / 2.0, 15.0);
    // Content inside the padding.
    // (A bare `rect()` keeps its natural height: content isn't stretched.)
    let item = children(&d, kids[1].0)[0].0;
    assert_xywh(rect_of(&l, item), 4.0, 34.0, 192.0, 0.0);
}

#[test]
fn sections_number_themselves_in_build_order() {
    let d = raw(r#"page(10, 10, col([
        section("A", rect()), section("B", rect()),
        section(7, "C", rect()), section("D", rect()),
        section("", "E", rect()), section("Ref.", "F", rect()), section("G", rect()),
    ])).unit("cm")"#);
    assert_eq!(
        texts(&d),
        ["1. A", "2. B", "7. C", "8. D", "E", "Ref. F", "9. G"]
    );
    // Other templates, or none at all.
    let d = raw(
        r#"style(#{ section_label_style: "(1)" }); page(10, 10, col([section("A", rect()), section("B", rect())]))"#,
    );
    assert_eq!(texts(&d), ["(1) A", "(2) B"]);
    let d = raw(r#"fn section_label_style() { "" } page(10, 10, section("A", rect()))"#);
    assert_eq!(texts(&d), ["A"]);
    // Fresh per evaluation.
    let d = raw(r#"page(10, 10, section("A", rect()))"#);
    assert_eq!(texts(&d), ["1. A"]);
}

#[test]
fn section_bar_hooks_and_style_keys() {
    for prelude in [
        r##"style(#{ section_bar_height: 3, section_bar_fill: "#123456", section_bar_border: "#ff0000",
                    section_bar_border_width: 0.1, section_bar_radius: 0.5, section_title_color: "#000000",
                    section_title_size: 40, section_title_font: "Arial" });"##,
        r##"fn section_bar_height() { 3 } fn section_bar_fill() { "#123456" }
            fn section_bar_border() { "#ff0000" } fn section_bar_border_width() { 0.1 }
            fn section_bar_radius() { 0.5 } fn section_title_color() { "#000000" }
            fn section_title_size() { 40 } fn section_title_font() { "Arial" }"##,
    ] {
        let d = raw(&format!("{prelude} page(10, 10, section(\"A\", rect()))"));
        let (bar_id, bar) = children(&d, "root")[0];
        approx(bar.height.unwrap(), 3.0);
        assert_eq!(solid(&bar.fill), "#123456");
        assert_eq!(bar.stroke, hex("#ff0000"));
        approx(bar.stroke_width.unwrap(), 0.1);
        approx(bar.corner_radius.unwrap(), 0.5);
        let (_, t) = children(&d, bar_id)[0];
        assert_eq!(t.color, hex("#000000"));
        assert_eq!(t.font_size, Some(40.0));
        assert_eq!(t.font_family.as_deref(), Some("Arial"));
    }
}

#[test]
fn poster_dark_bars_follow_the_reference_look() {
    let d = raw(r#"style("poster-dark"); page(30, 20, section("A", rect())).unit("cm")"#);
    let (_, bar) = children(&d, "root")[0];
    // Top-lit 3-stop vertical gradient, hairline pale-cyan border, square.
    match &bar.fill {
        Some(Paint::Linear { stops, .. }) => {
            assert_eq!(stops.len(), 3);
            assert_eq!(stops[0].color.to_hex(), "#3d4a73");
            approx(stops[1].offset, 0.56);
            assert_eq!(stops[2].color.to_hex(), "#080a10");
        }
        other => panic!("expected a linear gradient, got {other:?}"),
    }
    assert_eq!(bar.stroke, hex("#caeefb"));
    approx(bar.stroke_width.unwrap(), 24.0 * 0.000367); // 0.25 pt
    assert_eq!(bar.corner_radius, None);
}

#[test]
fn section_errors() {
    assert!(fail(r#"page(1, 1, section(1.5, "A", rect()))"#).contains("integer or a string"));
    assert!(fail(r#"page(1, 1, section(3, rect()))"#).contains("the title must be a string"));
}

// ---- columns, rows, separators -----------------------------------------------

#[test]
fn columns_put_a_dashed_separator_in_each_gutter() {
    let src = r#"
        fn poster_gutter() { 10 }
        fn separator_inset() { 5 }
        fn separator_width() { 1 }
        page(170, 50, columns([rect(), rect(), rect()])).unit("pt")
    "#;
    let d = raw(src);
    let l = lay_fixed(src);
    let kids = children(&d, "root");
    assert_eq!(kids.len(), 5, "3 columns + 2 gutters");
    // (170 - 2 * 10) / 3 = 50 per column.
    for (i, x) in [(0, 0.0), (2, 60.0), (4, 120.0)] {
        assert_xywh(rect_of(&l, kids[i].0), x, 0.0, 50.0, 50.0);
    }
    for (i, x) in [(1, 50.0), (3, 110.0)] {
        assert_xywh(rect_of(&l, kids[i].0), x, 0.0, 10.0, 50.0);
    }
    // The lines run down the middle of the gutters, from the inset to the
    // bottom, dashed 8:3 of their width.
    let seps = separators(&l);
    assert_eq!(seps.len(), 2);
    for ((a, b, dash), x) in seps.iter().zip([55.0, 115.0]) {
        approx(a[0], x);
        approx(b[0], x);
        approx(a[1], 5.0);
        approx(b[1], 50.0);
        assert_eq!(dash, &vec![8.0, 3.0]);
    }
    let line = d.nodes.values().find(|n| n.kind == NodeKind::Line).unwrap();
    assert_eq!(line.stroke, hex("#7f7f7f"), "grey on light posters");
    approx(line.stroke_width.unwrap(), 1.0);
}

#[test]
fn column_widths_are_flex_weights_or_lengths() {
    let src = r#"
        fn poster_gutter() { 1 }
        page(31, 10, columns([rect(), rect(), rect()], [1, "10cm", 2])).unit("cm")
    "#;
    let d = raw(src);
    let l = lay_fixed(src);
    let kids = children(&d, "root");
    // 31 - 2 gutters - 10 = 19 shared 1 : 2.
    approx(rect_of(&l, kids[2].0).w, 10.0 * CM);
    approx(rect_of(&l, kids[0].0).w, 19.0 / 3.0 * CM);
    approx(rect_of(&l, kids[4].0).w, 38.0 / 3.0 * CM);
    approx(rect_of(&l, kids[4].0).x, (31.0 - 38.0 / 3.0) * CM);

    assert!(fail(r#"page(1, 1, columns([rect()], [1, 2]))"#).contains("1 items but 2 sizes"));
    assert!(fail(r#"page(1, 1, columns([rect()], ["wide"]))"#).contains("flex weights"));
    assert!(fail(r#"page(1, 1, columns([rect()], ["xcm"]))"#).contains("bad length"));
}

#[test]
fn separators_can_be_switched_off() {
    let src = r#"
        style(#{ separators: false, poster_gutter: 10 });
        page(110, 50, columns([rect(), rect()])).unit("pt")
    "#;
    let d = raw(src);
    let l = lay_fixed(src);
    assert_eq!(children(&d, "root").len(), 2);
    approx(d.nodes["root"].spacing, 10.0);
    assert!(separators(&l).is_empty());
    approx(rect_of(&l, children(&d, "root")[1].0).x, 60.0);
}

#[test]
fn rows_share_the_height_and_hsep_spans_its_parent() {
    // Lengths are cm (the poster unit); other units are refused.
    assert!(fail(r#"page(1, 1, rows([rect()], ["100pt"]))"#).contains("flex weights"));

    let src = r#"
        fn poster_row_gap() { 10 }
        fn separator_width() { 1 }
        page(100, 210, rows([rect(), col([rect().height(20), hsep(8), rect()]).stretch()], [1, 3])).unit("pt")
    "#;
    let d = raw(src);
    let l = lay_fixed(src);
    let kids = children(&d, "root");
    assert_xywh(rect_of(&l, kids[0].0), 0.0, 0.0, 100.0, 50.0);
    assert_xywh(rect_of(&l, kids[1].0), 0.0, 60.0, 100.0, 150.0);
    // hsep: a full-width dashed line centred in its 8 pt band.
    let seps = separators(&l);
    assert_eq!(seps.len(), 1);
    let (a, b, _) = &seps[0];
    assert_eq!((a[0], b[0]), (0.0, 100.0));
    approx(a[1], 60.0 + 20.0 + 4.0);
    approx(b[1], a[1]);

    // Row separators: an hsep in each gap instead of spacing.
    let src = r#"
        style(#{ row_separators: true, poster_row_gap: 10 });
        page(100, 110, rows([rect(), rect()])).unit("pt")
    "#;
    let d = raw(src);
    assert_eq!(children(&d, "root").len(), 3);
    let l = lay_fixed(src);
    let seps = separators(&l);
    assert_eq!(seps.len(), 1);
    approx(seps[0].0[1], 55.0);
}

// ---- tables ----------------------------------------------------------------------

#[test]
fn table_grid_geometry() {
    let src = r#"
        fn table_rule_width() { 1 }
        fn table_header_rule_width() { 3 }
        fn table_pad() { 2 }
        page(101, 100, table([["a", "b"], ["c", "d"], ["e", 5]], #{ align: ["left", "right"] })).unit("pt")
    "#;
    let l = lay_fixed(src);
    // Cells: (101 - 3 rules) / 2 = 49 wide; rows 10 + 2 * 2 = 14 tall.
    assert_xywh(text_rect(&l, "a"), 3.0, 3.0, 45.0, 10.0);
    assert_xywh(text_rect(&l, "b"), 53.0, 3.0, 45.0, 10.0);
    assert_xywh(text_rect(&l, "c"), 3.0, 1.0 + 14.0 + 3.0 + 2.0, 45.0, 10.0);
    assert_xywh(text_rect(&l, "e"), 3.0, 18.0 + 14.0 + 1.0 + 2.0, 45.0, 10.0);
    let _ = text_rect(&l, "5"); // numbers become text
                                // Horizontal rules: top, header (3 pt), inner, bottom — all full width.
    let rules = filled_rects(&l);
    let horiz: Vec<_> = rules.iter().filter(|r| r.w > 50.0).collect();
    let ys: Vec<(f32, f32)> = horiz.iter().map(|r| (r.y, r.h)).collect();
    assert_eq!(ys, [(0.0, 1.0), (15.0, 3.0), (32.0, 1.0), (47.0, 1.0)]);
    for r in &horiz {
        assert_eq!((r.x, r.w), (0.0, 101.0));
    }
    // Vertical rules at x = 0, 50, 100, one per row, spanning each row.
    let vert: Vec<_> = rules.iter().filter(|r| r.w < 50.0).collect();
    assert_eq!(vert.len(), 9);
    for (k, r) in vert.iter().enumerate() {
        assert_eq!(r.x, [0.0, 50.0, 100.0][k % 3]);
        assert_eq!((r.w, r.h), (1.0, 14.0));
    }

    let d = raw(src);
    let (_, a) = text_node(&d, "a");
    assert_eq!(a.font_weight, Some(700), "header row bold");
    assert_eq!(a.align, Some(TextAlign::Left));
    let (_, c) = text_node(&d, "c");
    assert_eq!(c.font_weight, None);
    assert_eq!(text_node(&d, "d").1.align, Some(TextAlign::Right));
}

#[test]
fn table_rule_styles_and_widths() {
    let base = r#"
        fn table_rule_width() { 1 } fn table_header_rule_width() { 3 } fn table_pad() { 2 }
    "#;
    let count = |rules: &str| {
        let l = lay_fixed(&format!(
            r#"{base} page(101, 100, table([["a", "b"], ["c", "d"], ["e", "f"]], #{{ rules: "{rules}" }})).unit("pt")"#
        ));
        let r = filled_rects(&l);
        (
            r.iter().filter(|r| r.w > 50.0).count(),
            r.iter().filter(|r| r.w < 50.0).count(),
        )
    };
    assert_eq!(count("grid"), (4, 9));
    assert_eq!(count("rows"), (4, 0));
    assert_eq!(count("booktabs"), (3, 0));
    assert_eq!(count("none"), (0, 0));

    // No header: no bold, no thick rule.
    let l = lay_fixed(&format!(
        r#"{base} page(101, 100, table([["a", "b"]], #{{ header: false }})).unit("pt")"#
    ));
    assert!(filled_rects(&l).iter().all(|r| r.h <= 14.0 && r.w <= 101.0));
    assert!(filled_rects(&l).iter().all(|r| r.h != 3.0));

    // All-absolute widths: the table is exactly as wide as its columns.
    let d = raw(r#"page(30, 10, table([["a", "b"]], #{ widths: ["4cm", "6cm"] })).unit("cm")"#);
    approx(d.nodes["root"].width.unwrap(), 10.0 + 3.0 * 8.0 * 0.00147);

    // Cell fills (the dark poster's cells sit a shade off the page).
    let d = raw(r#"style("poster-dark"); page(30, 10, table([["a"], ["b"]]))"#);
    let fills: Vec<_> = d
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Column && n.fill.is_some())
        .map(|n| solid(&n.fill))
        .collect();
    assert_eq!(fills, ["#121622", "#121622"]);
    let rule = d.nodes.values().find(|n| n.kind == NodeKind::Rect).unwrap();
    assert_eq!(solid(&rule.fill), "#ffffff", "white rules on dark posters");

    assert!(fail(r#"page(1, 1, table([["a"], ["b", "c"]]))"#).contains("rows differ in length"));
    assert!(fail(r#"page(1, 1, table([["a"]], #{ rule: "grid" }))"#).contains("unknown option"));
    assert!(fail(r#"page(1, 1, table([["a"]], #{ rules: "x" }))"#).contains("unknown rules"));
}

// ---- bullets, frames, notes ------------------------------------------------------

#[test]
fn bullets_hang_their_text() {
    let src = r#"
        fn bullet_indent() { 15 }
        fn bullet_gap() { 4 }
        page(100, 100, bullets(["one", "two", ["sub"], text("node")])).unit("pt")
    "#;
    let l = lay_fixed(src);
    assert_xywh(text_rect(&l, "one"), 15.0, 0.0, 85.0, 10.0);
    assert_xywh(text_rect(&l, "two"), 15.0, 14.0, 85.0, 10.0);
    // A nested list: indented once more, its own mark at the first indent.
    assert_xywh(text_rect(&l, "sub"), 30.0, 28.0, 70.0, 10.0);
    assert_xywh(text_rect(&l, "node"), 15.0, 42.0, 85.0, 10.0);
    let marks: Vec<Rect> = l
        .nodes
        .iter()
        .filter(|n| matches!(&n.content, PaintContent::Text(t) if t.content == "•"))
        .map(|n| n.rect)
        .collect();
    let xs: Vec<f32> = marks.iter().map(|r| r.x).collect();
    assert_eq!(xs, [0.0, 0.0, 15.0, 0.0]);
    for m in &marks {
        approx(m.w, 15.0);
    }
    let d = raw(r#"style(#{ bullet_mark: "–", bullet_size: 30 }); page(10, 10, bullets(["x"]))"#);
    assert_eq!(text_node(&d, "–").1.font_size, Some(30.0));
    assert_eq!(text_node(&d, "x").1.font_size, Some(30.0));
}

#[test]
fn frames_notes_refs_and_subheadings() {
    let d = raw(r#"style("poster"); page(30, 30, col([
        frame(["a", rect()]), dashed_frame(rect()), note("N"), ref("R"), subheading("S"),
    ]))"#);
    let kids = children(&d, "root");
    let f = kids[0].1;
    assert_eq!(f.stroke, hex("#7f7f7f"));
    approx(f.stroke_width.unwrap(), 24.0 * 0.0022);
    approx(f.corner_radius.unwrap(), 24.0 * 0.011);
    approx(f.padding.top, 24.0 * 0.015);
    assert!(f.dash.is_empty());
    assert_eq!(children(&d, kids[0].0).len(), 2);
    assert_eq!(kids[1].1.dash.len(), 2);
    let n = kids[2].1;
    assert_eq!(n.color, hex("#b85c00"));
    assert_eq!(n.align, Some(TextAlign::Center));
    assert_eq!(n.font_size, Some(24.0));
    let r = kids[3].1;
    approx(r.font_size.unwrap(), 24.0 * 0.835);
    assert_eq!(r.color, None);
    let s = kids[4].1;
    assert_eq!(s.content.as_deref(), Some("■ S"));
    approx(s.font_size.unwrap(), 24.0 * 1.335);

    // The dark theme: white frames, amber notes, white refs.
    let d = raw(r#"style("poster-dark"); page(30, 30, col([frame(rect()), note("N"), ref("R")]))"#);
    let kids = children(&d, "root");
    assert_eq!(kids[0].1.stroke, hex("#ffffff"));
    assert_eq!(kids[1].1.color, hex("#ffc000"));
    assert_eq!(kids[2].1.color, hex("#ffffff"));
    // Core setters still chain (a square grey box).
    let d = raw(r##"page(30, 30, frame(rect()).radius(0).stroke("#123456"))"##);
    assert_eq!(d.nodes["root"].stroke, hex("#123456"));
    approx(d.nodes["root"].corner_radius.unwrap(), 0.0);
    let d = raw(r#"fn subheading_mark() { "" } page(1, 1, subheading("S"))"#);
    assert_eq!(d.nodes["root"].content.as_deref(), Some("S"));
}

// ---- header ----------------------------------------------------------------------

#[test]
fn author_line_runs_and_superscripts() {
    let d = raw(r#"page(80, 10, author_line(
        [#{ name: "Alice", aff: 1, presenter: true }, #{ name: "Bob", aff: [1, 2] }, "Carol"],
        ["Univ A", "Univ B"]))"#);
    // One centred text node of spans; the affiliation numbers are
    // superscript spans (adjacent plain runs merged).
    let root = &d.nodes["root"];
    assert_eq!(root.kind, NodeKind::Text);
    assert_eq!(root.align, Some(TextAlign::Center));
    approx(root.font_size.unwrap(), 8.0 * 1.665);
    assert_eq!(
        spans(root),
        [
            ("（Univ A", false),
            ("1", true),
            ("・Univ B", false),
            ("2", true),
            ("）○Alice", false),
            ("1", true),
            ("・Bob", false),
            ("1,2", true),
            ("・Carol", false),
        ]
        .map(|(s, sup)| (s.to_string(), sup))
    );

    // A single affiliation is not numbered.
    let d = raw(r#"page(80, 10, author_line([#{ name: "Alice", aff: 1 }, "Bob"], ["Univ A"]))"#);
    assert_eq!(texts(&d), ["（Univ A）Alice・Bob"]);
    // "below": the authors, then the affiliations under them.
    let d = raw(r#"
        style(#{ affiliation_position: "below", author_sep: ", " });
        page(80, 10, author_line([#{ name: "Alice", aff: 2 }, #{ name: "Bob", aff: 1 }], ["U1", "U2"]))"#);
    assert_eq!(texts(&d), ["Alice2, Bob1", "U11, U22"]);
    let kids = children(&d, "root");
    assert_eq!(
        spans(kids[1].1),
        [("U1", false), ("1", true), (", U2", false), ("2", true)].map(|(s, b)| (s.to_string(), b))
    );
    approx(kids[1].1.font_size.unwrap(), 8.0 * 1.665 * 0.8);
    assert_eq!(d.nodes["root"].kind, NodeKind::Column);
    assert!(fail(
        r#"style(#{ affiliation_position: "left" }); page(1, 1, author_line(["A"], ["U"]))"#
    )
    .contains("\"before\" or \"below\""));
}

#[test]
fn poster_header_layout() {
    let src = r#"
        fn poster_logo_height() { 30 }
        fn poster_gutter() { 5 }
        fn poster_header_rule() { "" }
        page(200, 100, col([poster_header(#{
            logos: [image("a.png").width(40)], title: "T", number: "N",
        })]).stretch()).unit("pt")
    "#;
    let l = lay_fixed(src);
    let d = raw(src);
    let img = d
        .nodes
        .iter()
        .find(|(_, n)| n.kind == NodeKind::Image)
        .unwrap();
    assert_xywh(rect_of(&l, img.0), 0.0, 0.0, 40.0, 30.0);
    // The title spans the space between the logos and the number, centred,
    // mid-height (the author line below would get the full width).
    assert_xywh(text_rect(&l, "T"), 45.0, 10.0, 130.0, 10.0);
    assert_eq!(text_node(&d, "T").1.align, Some(TextAlign::Center));
    approx(text_node(&d, "T").1.font_size.unwrap(), 8.0 * 3.33);
    // The poster number at the right end of the title row, bold.
    assert_xywh(text_rect(&l, "N"), 180.0, 10.0, 20.0, 10.0);
    assert_eq!(text_node(&d, "N").1.font_weight, Some(700));

    // The rule under the header spans it.
    let src = r#"
        fn poster_row_gap() { 4 }
        fn poster_header_rule_width() { 1 }
        page(200, 100, col([poster_header(#{ title: "T" })]).stretch()).unit("pt")
    "#;
    let l = lay_fixed(src);
    let rule = filled_rects(&l);
    assert_eq!(rule.len(), 1);
    assert_xywh(rule[0], 0.0, 12.0, 200.0, 1.0);

    assert!(fail(r#"page(1, 1, poster_header(#{ titel: "T" }))"#).contains("unknown key \"titel\""));
}

// ---- the page ---------------------------------------------------------------------

#[test]
fn poster_page_setup() {
    let d = raw(r#"style("poster-dark"); poster(#{ title: "T" }, [section("A", rect())])"#);
    approx(d.page.width, 84.1);
    approx(d.page.height.fixed().unwrap(), 118.9);
    assert_eq!(d.page.unit, Unit::Cm);
    match &d.page.background {
        Some(Paint::Linear { stops, .. }) => {
            let c: Vec<_> = stops.iter().map(|s| s.color.to_hex()).collect();
            assert_eq!(c, ["#28314c", "#181e2e", "#10141f", "#080a10"]);
        }
        other => panic!("expected the page gradient, got {other:?}"),
    }
    let root = &d.nodes["root"];
    approx(root.padding.left, 24.0 * 0.021);
    approx(root.spacing, 24.0 * 0.0225);
    assert_eq!(root.children.len(), 2, "header + rows");

    // Another preset, no header, a single node body; light style: white.
    let d = raw(r#"style("poster"); poster("a1-landscape", (), section("A", rect()))"#);
    approx(d.page.width, 84.1);
    approx(d.page.height.fixed().unwrap(), 59.4);
    assert_eq!(d.page.background, None);
    assert_eq!(d.nodes["root"].children.len(), 1);
    // poster_bg / poster_size hooks.
    let d = raw(r##"style(#{ poster_size: "a2", poster_bg: "#202020" }); poster((), [rect()])"##);
    approx(d.page.width, 42.0);
    assert_eq!(solid(&d.page.background), "#202020");
}

#[test]
fn poster_lays_out_on_the_page() {
    let l = lay_fixed(
        r#"style("poster"); poster(#{ title: "T", authors: ["A"], affiliations: ["U"] }, rows([
            columns([section("A", rect()), section("B", rect())], [2, 1]),
            section("C", bullets(["x"])),
        ], [3, 1]))"#,
    );
    let margin = 24.0 * 0.021 * CM;
    let root = l.nodes[0].rect;
    assert_xywh(root, 0.0, 0.0, 84.1 * CM, 118.9 * CM);
    let bar = text_rect(&l, "1. A");
    assert!(bar.y > margin);
    // The three sections sit inside the margins, numbered in reading order.
    let c = text_rect(&l, "3. C");
    assert!(c.y > text_rect(&l, "2. B").y);
    let seps = separators(&l);
    assert_eq!(seps.len(), 1, "one gutter in the first row");
    assert!(seps[0].0[0] > margin && seps[0].0[0] < 84.1 * CM - margin);
}

#[test]
fn poster_demo_example() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let src = std::fs::read_to_string(examples.join("poster_demo.figs")).unwrap();
    let doc = Document::from_script_in(&src, Some(&examples)).unwrap();
    let l = layout(&doc, &NullMeasurer);
    approx(l.page.width_pt, 84.1 * CM);
    let d = figs_core::eval_script_in(&src, Some(&examples)).unwrap();
    let bars: Vec<_> = texts(&d)
        .into_iter()
        .filter(|s| s.starts_with(|c: char| c.is_ascii_digit()) && s.contains(". "))
        .collect();
    assert_eq!(bars.len(), 7, "{bars:?}");
    assert!(bars[6].starts_with("7. "));
    assert!(l.nodes[0].rect.h <= l.page.height_pt + 0.01);
}

#[test]
fn components_take_rich_text() {
    let d = raw(r#"page(30, 30, col([
        section(markup("H_2O *uptake*"), [
            subheading(markup("**Bold** head")),
            bullets([markup("item *one*"), ["nested"]]),
            table([[markup("**T**_m"), "b"], ["1", markup("x^2")]]),
            ref(markup("A. Author *et al.*")),
            note(markup("note **here**")),
        ]),
    ]))"#);
    let all = texts(&d);
    for want in [
        "1. H2O uptake",
        "■ Bold head",
        "item one",
        "nested",
        "Tm",
        "x2",
        "A. Author et al.",
        "note here",
    ] {
        assert!(all.iter().any(|s| s == want), "{want:?} in {all:?}");
    }
    let bar_title = d
        .nodes
        .values()
        .find(|n| plain(n) == "1. H2O uptake")
        .unwrap();
    assert_eq!(bar_title.spans[0].text, "1. ");
    assert!(bar_title
        .spans
        .iter()
        .any(|s| s.script == Some(Script::Sub) && s.text == "2"));
}

// ---- section effects, node titles ---------------------------------------------

/// The (dx, dy, blur, spread, colour) of a raw node's shadow.
fn shadow_of(n: &RawNode) -> (f32, f32, f32, f32, String) {
    let s = n.shadow.as_ref().expect("a shadow");
    (
        s.dx,
        s.dy,
        s.blur,
        s.spread,
        s.color.map(|c| c.to_hex()).unwrap_or_default(),
    )
}

fn assert_shadow(n: &RawNode, dx: f32, dy: f32, blur: f32, spread: f32, color: &str) {
    let (a, b, c, d, e) = shadow_of(n);
    approx(a, dx);
    approx(b, dy);
    approx(c, blur);
    approx(d, spread);
    assert_eq!(e, color);
}

#[test]
fn section_effects_are_off_in_poster_and_on_in_poster_dark() {
    let d = raw(r#"style("poster"); page(30, 20, section("A", rect())).unit("cm")"#);
    let (bar_id, bar) = children(&d, "root")[0];
    let (_, t) = children(&d, bar_id)[0];
    assert!(bar.shadow.is_none() && bar.glow.is_none());
    assert!(t.shadow.is_none() && t.glow.is_none());

    // The PowerPoint look: a subtle dark glow and a tight soft shadow on the
    // title, a soft 40 % shadow under the bar.
    let d = raw(r#"style("poster-dark"); page(30, 20, section("A", rect())).unit("cm")"#);
    let (bar_id, bar) = children(&d, "root")[0];
    let (_, t) = children(&d, bar_id)[0];
    assert_eq!(
        t.content.as_deref(),
        Some("1. A"),
        "the title is still the text"
    );
    assert_shadow(bar, 0.06, 0.07, 0.2, 0.0, "#00000066");
    assert!(bar.glow.is_none());
    let g = t.glow.as_ref().expect("a title glow");
    approx(g.blur, 0.1);
    assert_eq!(g.color, hex("#00000099"));
    assert_shadow(t, 0.04, 0.05, 0.08, 0.0, "#000000b3");

    // The looks scale with the text.
    let d = raw(
        r#"style("poster-dark", #{ base_size: 12 }); page(30, 20, section("A", rect())).unit("cm")"#,
    );
    let (bar_id, bar) = children(&d, "root")[0];
    approx(bar.shadow.as_ref().unwrap().blur, 0.1);
    approx(children(&d, bar_id)[0].1.glow.as_ref().unwrap().blur, 0.05);
}

#[test]
fn section_effect_hooks_take_maps() {
    for prelude in [
        r##"style(#{ section_title_glow: #{ blur: 0.3, color: "#ff0000" },
                    section_title_shadow: #{ dx: 1, dy: 2, blur: 3, spread: 0.5 },
                    section_bar_shadow: #{ dy: 0.2, color: "#0000ff80" } });"##,
        r##"fn section_title_glow() { #{ blur: 0.3, color: "#ff0000" } }
            fn section_title_shadow() { #{ dx: 1, dy: 2, blur: 3, spread: 0.5 } }
            fn section_bar_shadow() { #{ dy: 0.2, color: "#0000ff80" } }"##,
    ] {
        let d = raw(&format!(
            "{prelude} page(30, 20, section(\"A\", rect())).unit(\"cm\")"
        ));
        let (bar_id, bar) = children(&d, "root")[0];
        let (_, t) = children(&d, bar_id)[0];
        let g = t.glow.as_ref().unwrap();
        approx(g.blur, 0.3);
        assert_eq!(g.color, hex("#ff0000"));
        assert_shadow(t, 1.0, 2.0, 3.0, 0.5, "#00000080");
        assert_shadow(bar, 0.0, 0.2, 0.0, 0.0, "#0000ff80");
    }
    // `true` switches the look on in the light style; `false` off in the dark.
    let d = raw(
        r#"style("poster", #{ section_bar_shadow: true }); page(30, 20, section("A", rect())).unit("cm")"#,
    );
    assert_shadow(children(&d, "root")[0].1, 0.06, 0.07, 0.2, 0.0, "#00000066");
    let d = raw(
        r#"style("poster-dark", #{ section_title_glow: false }); page(30, 20, section("A", rect())).unit("cm")"#,
    );
    let bar_id = children(&d, "root")[0].0;
    assert!(children(&d, bar_id)[0].1.glow.is_none());

    assert!(
        fail(r#"style(#{ section_title_glow: 3 }); page(1, 1, section("A", rect()))"#)
            .contains("expected false, true or a map")
    );
    assert!(fail(
        r#"style(#{ section_title_glow: #{ dx: 1 } }); page(1, 1, section("A", rect()))"#
    )
    .contains("unknown key \"dx\""));
}

#[test]
fn section_takes_a_text_node_title() {
    let src = r##"
        fn section_bar_height() { 30 }
        fn section_title_size() { 72 }
        page(200, 100, section(text("X").color("#ff0000"), rect())).unit("pt")
    "##;
    let d = raw(src);
    let l = lay_fixed(src);
    // The node keeps its own look; the number goes before it in the title
    // style, a space (0.28 em) apart, the pair centred on the bar.
    let (_, x) = text_node(&d, "X");
    assert_eq!(x.font_size, None);
    assert_eq!(x.color, hex("#ff0000"));
    let (_, n) = text_node(&d, "1.");
    assert_eq!(n.font_size, Some(72.0));
    assert_eq!(n.color, hex("#ffffff"));
    let a = text_rect(&l, "1.");
    let b = text_rect(&l, "X");
    approx(b.x - (a.x + a.w), 72.0 * 0.28 * 2.54 / 72.0);
    approx((a.x + b.x + b.w) / 2.0, 100.0);
    approx(a.y + a.h / 2.0, 15.0);
    approx(b.y + b.h / 2.0, 15.0);
    // Effects go on a row around the title, so the node's own stay.
    let d = raw(
        r#"style("poster-dark"); page(30, 20, section("", text("X").glow(0.5), rect())).unit("cm")"#,
    );
    let (bar_id, _) = children(&d, "root")[0];
    let (wrap_id, wrap) = children(&d, bar_id)[0];
    assert_eq!(wrap.kind, NodeKind::Row);
    assert!(wrap.glow.is_some() && wrap.shadow.is_some());
    approx(children(&d, wrap_id)[0].1.glow.as_ref().unwrap().blur, 0.5);
    assert_eq!(texts(&d), ["X"]);
    // `section_title` is the bar's title style, to chain setters on.
    let d = raw(r#"style("poster"); page(30, 20, section_title("T"))"#);
    approx(d.nodes["root"].font_size.unwrap(), 24.0 * 2.23);
    assert_eq!(d.nodes["root"].align, Some(TextAlign::Center));
}

// ---- grid options ---------------------------------------------------------------

#[test]
fn columns_take_per_gutter_separators_and_gutters() {
    let src = r#"
        fn separator_inset() { 5 }
        fn separator_width() { 1 }
        page(190, 50, columns([rect(), rect(), rect(), rect()], #{
            gutter: [10, 20, 10],
            separators: [20, false, #{ start: 2, end: 30 }],
        })).unit("pt")
    "#;
    let d = raw(src);
    let l = lay_fixed(src);
    let kids = children(&d, "root");
    assert_eq!(kids.len(), 7, "4 columns + 3 gutters");
    assert_eq!(d.nodes["root"].spacing, 0.0);
    // (190 - 40) / 4 = 37.5 per column.
    for (i, x, w) in [
        (0, 0.0, 37.5),
        (1, 37.5, 10.0),
        (2, 47.5, 37.5),
        (3, 85.0, 20.0),
        (4, 105.0, 37.5),
        (5, 142.5, 10.0),
        (6, 152.5, 37.5),
    ] {
        let r = rect_of(&l, kids[i].0);
        approx(r.x, x);
        approx(r.w, w);
    }
    // A partial separator from the inset (the middle of the bars) down to
    // 20; none in the plain gutter; one from 2 to 30.
    let seps = separators(&l);
    assert_eq!(seps.len(), 2);
    let (a, b, _) = &seps[0];
    assert_eq!((a[0], a[1], b[0], b[1]), (42.5, 5.0, 42.5, 20.0));
    let (a, b, _) = &seps[1];
    assert_eq!((a[0], a[1], b[0], b[1]), (147.5, 2.0, 147.5, 30.0));

    // Quoted "from" / "until" are aliases.
    let l = lay_fixed(
        r#"fn separator_inset() { 5 }
           page(110, 50, columns([rect(), rect()], #{ gutter: 10, separators: [#{ "until": 25 }] })).unit("pt")"#,
    );
    let (a, b, _) = &separators(&l)[0];
    assert_eq!((a[1], b[1]), (5.0, 25.0));

    // Options as the second argument; one gutter and no separators stays
    // plain spacing.
    let d = raw(
        r#"page(110, 50, columns([rect(), rect()], #{ gutter: 4, separators: false })).unit("pt")"#,
    );
    assert_eq!(children(&d, "root").len(), 2);
    approx(d.nodes["root"].spacing, 4.0);

    assert!(
        fail(r#"page(1, 1, columns([rect(), rect(), rect()], #{ separators: [true] }))"#)
            .contains("3 items have 2 gaps but separators has 1 values")
    );
    assert!(
        fail(r#"page(1, 1, columns([rect(), rect()], #{ separators: ["x"] }))"#)
            .contains("a separator is true, false")
    );
    assert!(
        fail(r#"page(1, 1, columns([rect(), rect()], #{ separators: [#{ stop: 1 }] }))"#)
            .contains("unknown separator key")
    );
    assert!(fail(
        r#"page(1, 1, columns([rect(), rect()], #{ separators: [#{ start: 3, end: 2 }] }))"#
    )
    .contains("must end below its start"));
    assert!(fail(r#"page(1, 1, columns([rect()], [1], #{ gap: 1 }))"#)
        .contains("unknown option \"gap\""));
}

#[test]
fn rows_take_per_gap_sizes_and_separators() {
    let src = r#"
        fn separator_width() { 1 }
        page(100, 230, rows([rect(), rect(), rect()], [1, 1, 1], #{
            gaps: [10, 20], separators: [false, true],
        })).unit("pt")
    "#;
    let d = raw(src);
    let l = lay_fixed(src);
    let kids = children(&d, "root");
    assert_eq!(kids.len(), 5);
    let h = 200.0 / 3.0;
    approx(rect_of(&l, kids[0].0).y, 0.0);
    approx(rect_of(&l, kids[2].0).y, h + 10.0);
    approx(rect_of(&l, kids[4].0).y, 2.0 * h + 30.0);
    approx(rect_of(&l, kids[4].0).h, h);
    // The separator is centred in the 20 pt gap.
    let seps = separators(&l);
    assert_eq!(seps.len(), 1);
    approx(seps[0].0[1], 2.0 * h + 20.0);

    let d = raw(r#"page(100, 100, rows([rect(), rect()], #{ gaps: 5 })).unit("pt")"#);
    assert_eq!(children(&d, "root").len(), 2);
    approx(d.nodes["root"].spacing, 5.0);
    assert!(
        fail(r#"page(1, 1, rows([rect(), rect()], #{ gaps: [1, 2] }))"#).contains("has 2 values")
    );
    assert!(
        fail(r#"page(1, 1, rows([rect(), rect()], #{ separators: [3] }))"#)
            .contains("separators are true or false")
    );
}

// ---- table padding and spans ------------------------------------------------------

#[test]
fn table_padding_per_axis() {
    let base =
        r#"fn table_rule_width() { 1 } fn table_header_rule_width() { 3 } fn table_pad() { 2 }"#;
    // Option: 6 left/right, 2 top/bottom.
    let l = lay_fixed(&format!(
        r#"{base} page(101, 100, table([["a", "b"], ["c", "d"]], #{{ pad_x: 6 }})).unit("pt")"#
    ));
    assert_xywh(text_rect(&l, "a"), 7.0, 3.0, 37.0, 10.0);
    assert_xywh(text_rect(&l, "b"), 57.0, 3.0, 37.0, 10.0);
    assert_xywh(text_rect(&l, "c"), 7.0, 20.0, 37.0, 10.0);
    // Hook: 5 top/bottom, the sides keep `table_pad()`.
    let l = lay_fixed(&format!(
        r#"{base} fn table_pad_y() {{ 5 }} page(101, 100, table([["a", "b"], ["c", "d"]])).unit("pt")"#
    ));
    assert_xywh(text_rect(&l, "a"), 3.0, 6.0, 45.0, 10.0);
    assert_xywh(text_rect(&l, "c"), 3.0, 1.0 + 20.0 + 3.0 + 5.0, 45.0, 10.0);
    // `pad` sets both; style keys too.
    let l = lay_fixed(&format!(
        r#"{base} style(#{{ table_pad_x: 4 }}); page(101, 100, table([["a"]], #{{ pad: 1 }})).unit("pt")"#
    ));
    assert_xywh(text_rect(&l, "a"), 2.0, 2.0, 97.0, 10.0);
    let l = lay_fixed(&format!(
        r#"{base} style(#{{ table_pad_x: 4 }}); page(101, 100, table([["a"]])).unit("pt")"#
    ));
    assert_xywh(text_rect(&l, "a"), 5.0, 3.0, 91.0, 10.0);
}

#[test]
fn table_cells_span_columns() {
    let base =
        r#"fn table_rule_width() { 1 } fn table_header_rule_width() { 3 } fn table_pad() { 2 }"#;
    let l = lay_fixed(&format!(
        r#"{base} page(151, 100, table([["a", "b", "c"], [colspan(2, "ab"), "c2"], ["x", colspan(2, "yz")]])).unit("pt")"#
    ));
    // Flex columns: (151 - 4 rules) / 3 = 49; a two-column cell covers
    // 49 + 1 + 49 = 99: the covered rule as two 0.5 blanks around a 98 cell.
    assert_xywh(text_rect(&l, "ab"), 1.5 + 2.0, 20.0, 94.0, 10.0);
    assert_xywh(text_rect(&l, "c2"), 103.0, 20.0, 45.0, 10.0);
    assert_xywh(text_rect(&l, "yz"), 51.5 + 2.0, 35.0, 94.0, 10.0);
    // Vertical rules: 4 in the header, 3 in each spanning row.
    let vert = filled_rects(&l)
        .into_iter()
        .filter(|r| r.w == 1.0 && r.h == 14.0)
        .count();
    assert_eq!(vert, 10);
    // Length columns: the span's width is theirs plus the rule between.
    let l = lay_fixed(
        r#"fn table_rule_width() { 1 } fn table_pad() { 2 }
           page(200, 100, table([["a", "b"], [colspan(2, "ab")]], #{ widths: ["2cm", "3cm"] })).unit("cm")"#,
    );
    approx(text_rect(&l, "ab").w, (5.0 + 1.0 - 4.0) * CM);
    let d = raw(r#"page(10, 10, table([[colspan(2, "h")], ["a", "b"]]))"#);
    assert_eq!(text_node(&d, "h").1.font_weight, Some(700));

    assert!(fail(r#"page(1, 1, table([["a", "b"], [colspan(3, "x")]]))"#).contains("rows differ"));
    assert!(fail(r#"page(1, 1, table([[colspan(0, "x")]]))"#).contains("positive integer"));
    assert!(fail(r#"page(1, 1, table([["a"], [#{ text: "x" }]]))"#).contains("colspan, content"));
    assert!(fail(
        r#"page(1, 1, table([["a", "b"], [colspan(2, "x")]], #{ widths: ["auto", 1] }))"#
    )
    .contains("\"auto\" columns"));
    assert!(
        fail(r#"page(1, 1, table([["a", "b"], [colspan(2, "x")]], #{ widths: ["2cm", 1] }))"#)
            .contains("only lengths or only flex")
    );
}

// ---- marks, author spans ------------------------------------------------------------

#[test]
fn subheading_and_bullet_marks_style_on_their_own() {
    let d = raw(
        r#"style("poster", #{ subheading_mark_bold: true, subheading_mark_size: 40 }); page(10, 10, subheading(markup("S *x*")))"#,
    );
    let root = &d.nodes["root"];
    approx(root.font_size.unwrap(), 24.0 * 1.335);
    assert_eq!(root.spans[0].text, "■ ");
    assert_eq!(root.spans[0].font_weight, Some(700));
    assert_eq!(root.spans[0].font_size, Some(40.0));
    assert_eq!(plain(root), "■ S x");
    assert_eq!(root.spans[1].font_weight, None);
    // Bold only: the size is left to the node.
    let d = raw(r#"style(#{ subheading_mark_bold: true }); page(10, 10, subheading("S"))"#);
    let root = &d.nodes["root"];
    assert_eq!(root.spans[0].font_size, None);
    assert_eq!(root.spans[1].text, "S");

    // A bigger / smaller bullet mark is moved so the baselines meet.
    let k = 0.93 * 2.54 / 72.0;
    let src = |m: f32| {
        format!(
            r#"fn bullet_indent() {{ 15 }} fn bullet_size() {{ 24 }} fn bullet_mark_size() {{ {m} }}
               page(100, 100, bullets(["one"])).unit("pt")"#
        )
    };
    let l = lay_fixed(&src(12.0));
    approx(text_rect(&l, "•").y, 12.0 * k);
    approx(text_rect(&l, "one").y, 0.0);
    approx(text_rect(&l, "•").w, 15.0);
    let l = lay_fixed(&src(36.0));
    approx(text_rect(&l, "•").y, 0.0);
    approx(text_rect(&l, "one").y, 12.0 * k);
    let d = raw(&src(36.0));
    assert_eq!(text_node(&d, "•").1.font_size, Some(36.0));
    assert_eq!(text_node(&d, "one").1.font_size, Some(24.0));
}

#[test]
fn author_line_takes_rich_names_and_affiliations() {
    let d = raw(r##"page(80, 10, author_line(
        [#{ name: markup("*A*"), aff: 2, presenter: true }, markup("{#ff0000|B}")],
        [markup("U*1*"), "U2"]))"##);
    let root = &d.nodes["root"];
    assert_eq!(
        spans(root),
        [
            ("（", false),
            ("U", false),
            ("1", false),
            ("1", true),
            ("・U2", false),
            ("2", true),
            ("）○", false),
            ("A", false),
            ("2", true),
            ("・", false),
            ("B", false),
        ]
        .map(|(s, sup)| (s.to_string(), sup))
    );
    assert!(root.spans[2].font_style.is_some(), "italic kept");
    assert_eq!(root.spans[10].color, hex("#ff0000"));
    // A single rich affiliation (not a list of spans) is one affiliation.
    let d = raw(r#"page(80, 10, author_line(["A"], markup("Univ *X*")))"#);
    assert_eq!(texts(&d), ["（Univ X）A"]);
    // In the header too.
    let d =
        raw(r#"page(80, 10, poster_header(#{ authors: [markup("**A**")], affiliations: ["U"] }))"#);
    assert!(texts(&d).iter().any(|s| s == "（U）A"));
}
