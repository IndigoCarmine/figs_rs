//! Tests for the framework prelude (`prelude.rhai`): the concise, opinionated
//! layer over the explicit core builders. Covers semantic text and its base-size
//! hook, bare-string sugar, `vbox`/`hbox`, `grid`, panel labels, page presets,
//! and that core setters still chain on everything the framework returns.
//!
//! Labeled panels overlay via the core `stack` / `.at`.

#![cfg(feature = "script")]

use figs_core::geom::{Color, CrossAxisAlign, MainAxisAlign, Rect, Size, TextAlign};
use figs_core::layout::{ComputedLayout, PaintContent};
use figs_core::schema::{ImageProps, NodeKind, PageHeight, RawDocument, RawNode, TextProps};
use figs_core::units::Unit;
use figs_core::{eval_script, layout, Document, LeafMeasure, NullMeasurer};

/// Every text block measures `TEXT_W` x `TEXT_H` pt, so overlay positions that
/// depend on the label's size are exact.
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

fn lay_fixed(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &FixedText)
}

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-3, "expected {b}, got {a}");
}

fn raw(src: &str) -> RawDocument {
    eval_script(src).unwrap_or_else(|e| panic!("script failed: {e}"))
}

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &NullMeasurer)
}

fn rect_of(l: &ComputedLayout, id: &str) -> Rect {
    l.nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("node `{id}` not in layout"))
        .rect
}

/// Pre-order index of a lowered node id (`root` = 0, `nodeN` = N), i.e. its
/// paint order: later nodes paint over earlier ones.
fn order(id: &str) -> usize {
    id.strip_prefix("node").map_or(0, |n| n.parse().unwrap())
}

/// Id of the topmost (last-painted) text node with this content (a label
/// background box paints under its text, so this is the foreground).
fn text_id(d: &RawDocument, content: &str) -> String {
    d.nodes
        .iter()
        .filter(|(_, n)| n.kind == NodeKind::Text && n.content.as_deref() == Some(content))
        .max_by_key(|(id, _)| order(id))
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| panic!("no text node with content {content:?}"))
}

/// The raw (topmost) text node with this content.
fn text_node<'a>(d: &'a RawDocument, content: &str) -> &'a RawNode {
    &d.nodes[&text_id(d, content)]
}

/// Id of the foreground text of a label tag node: the node itself when it is
/// a (possibly outlined) text, else the last child of its background stack.
fn tag_text(d: &RawDocument, id: &str) -> String {
    let n = &d.nodes[id];
    match n.kind {
        NodeKind::Text => id.to_string(),
        _ => tag_text(d, n.children.last().expect("tag has children")),
    }
}

/// The raw children of node `id`, in order.
fn children<'a>(d: &'a RawDocument, id: &str) -> Vec<(&'a str, &'a RawNode)> {
    d.nodes[id]
        .children
        .iter()
        .map(|c| (c.as_str(), &d.nodes[c]))
        .collect()
}

/// Text contents of the root's children, in order.
fn root_texts(d: &RawDocument) -> Vec<String> {
    children(d, "root")
        .into_iter()
        .map(|(_, n)| n.content.clone().unwrap_or_default())
        .collect()
}

// ---- semantic text ---------------------------------------------------------

#[test]
fn semantic_text_defaults() {
    let d = raw(r#"page("single", vbox([title("T"), caption("C"), label("L"), body("B")]))"#);

    let t = text_node(&d, "T");
    assert_eq!(t.font_size, Some(12.0)); // 1.5 x base 8
    assert_eq!(t.font_weight, Some(700));
    assert_eq!(t.align, Some(TextAlign::Center));

    let c = text_node(&d, "C");
    assert_eq!(c.font_size, Some(8.0));
    assert_eq!(c.font_weight, None);
    assert_eq!(c.align, Some(TextAlign::Center));

    let l = text_node(&d, "L");
    assert_eq!(l.font_size, Some(10.0)); // 1.25 x base
    assert_eq!(l.font_weight, Some(700));

    assert_eq!(text_node(&d, "B").font_size, Some(8.0));
}

#[test]
fn base_size_hook_rescales_all_semantic_text() {
    // A user `fn` replaces the prelude's hook, and the components pick it up.
    let d = raw(r#"
        fn base_size() { 10 }
        page("single", vbox([title("T"), caption("C"), label("L"), "bare"]))
        "#);
    assert_eq!(text_node(&d, "T").font_size, Some(15.0));
    assert_eq!(text_node(&d, "C").font_size, Some(10.0));
    assert_eq!(text_node(&d, "L").font_size, Some(12.5));
    assert_eq!(text_node(&d, "bare").font_size, Some(10.0));
}

#[test]
fn core_setters_chain_on_framework_results() {
    // The framework never takes away core freedom: its results are plain nodes.
    let d = raw(
        r##"page("single", vbox([title("T").size(30).color("#ff0000").align("left")]).pad(1))"##,
    );
    let t = text_node(&d, "T");
    assert_eq!(t.font_size, Some(30.0));
    assert_eq!(t.align, Some(TextAlign::Left));
    assert!(t.color.is_some());
    approx(d.nodes["root"].padding.top, 1.0);
}

// ---- bare-string sugar & vbox/hbox ------------------------------------------

#[test]
fn bare_strings_become_body_text() {
    let d = raw(r#"page("single", vbox(["Results", rect(), "More"]))"#);
    let kids = children(&d, "root");
    assert_eq!(kids.len(), 3);
    assert_eq!(kids[0].1.kind, NodeKind::Text);
    assert_eq!(kids[0].1.content.as_deref(), Some("Results"));
    assert_eq!(kids[0].1.font_size, Some(8.0));
    assert_eq!(kids[1].1.kind, NodeKind::Rect);
    assert_eq!(kids[2].1.kind, NodeKind::Text);
}

#[test]
fn nodes_bridges_bare_strings_into_core_containers() {
    // Core `col`/`row` stay strict; `nodes(..)` is the opt-in bridge.
    let d = raw(r#"page(10, 10, row(nodes(["a", "b"]))).unit("cm")"#);
    assert_eq!(d.nodes["root"].kind, NodeKind::Row);
    assert_eq!(d.nodes["root"].spacing, 0.0); // core defaults untouched
    assert_eq!(root_texts(&d), ["a", "b"]);

    let e = eval_script(r#"page(10, 10, col(["a"])).unit("cm")"#).unwrap_err();
    assert!(e.to_string().contains("must be nodes"), "{e}");
}

#[test]
fn vbox_hbox_defaults() {
    let d = raw(r#"page("single", vbox([hbox(["a", "b"]), "c"]))"#);
    let root = &d.nodes["root"];
    assert_eq!(root.kind, NodeKind::Column);
    approx(root.spacing, 0.2);
    assert_eq!(root.cross_axis_alignment, Some(CrossAxisAlign::Stretch));
    let (hid, h) = children(&d, "root")[0];
    assert_eq!(h.kind, NodeKind::Row);
    approx(h.spacing, 0.2);
    assert_eq!(children(&d, hid).len(), 2);
}

#[test]
fn gap_hook_changes_default_spacing() {
    let d = raw(r#"
        fn gap() { 0.5 }
        page("single", vbox(["a", "b"]))
        "#);
    approx(d.nodes["root"].spacing, 0.5);
}

// ---- grid ------------------------------------------------------------------

#[test]
fn grid_structure_pads_last_row() {
    let d = raw(r#"page("single", grid(2, ["a", "b", "c"]))"#);
    let root = &d.nodes["root"];
    assert_eq!(root.kind, NodeKind::Column);
    approx(root.spacing, 0.2);
    let rows = children(&d, "root");
    assert_eq!(rows.len(), 2);
    for (rid, r) in &rows {
        assert_eq!(r.kind, NodeKind::Row);
        assert_eq!(r.flex, Some(1.0));
        let cells = children(&d, rid);
        assert_eq!(cells.len(), 2, "every row has ncols cells");
        assert!(cells.iter().all(|(_, c)| c.flex == Some(1.0)));
    }
    // Last row: "c" plus an empty filler rect.
    let last = children(&d, rows[1].0);
    assert_eq!(last[0].1.content.as_deref(), Some("c"));
    assert_eq!(last[1].1.kind, NodeKind::Rect);
    assert_eq!(last[1].1.fill, None);
}

#[test]
fn grid_layout_equal_cells() {
    // 100pt square page, explicit 10pt gap: 2 cols x 2 rows of 45pt cells.
    let src = r#"page(100, 100, grid(2, 10, [rect(), rect(), rect()])).unit("pt")"#;
    let d = raw(src);
    let rows = children(&d, "root");
    let r0 = children(&d, rows[0].0);
    let r1 = children(&d, rows[1].0);
    let l = lay(src);
    let a = rect_of(&l, r0[0].0);
    let b = rect_of(&l, r0[1].0);
    let c = rect_of(&l, r1[0].0);
    let filler = rect_of(&l, r1[1].0);
    for (r, x, y) in [
        (a, 0.0, 0.0),
        (b, 55.0, 0.0),
        (c, 0.0, 55.0),
        (filler, 55.0, 55.0),
    ] {
        approx(r.x, x);
        approx(r.y, y);
        approx(r.w, 45.0);
        approx(r.h, 45.0);
    }
}

#[test]
fn grid_rejects_bad_ncols() {
    let e = eval_script(r#"page("single", grid(0, [rect()]))"#).unwrap_err();
    assert!(e.to_string().contains("ncols"), "{e}");
}

// ---- panel labels ----------------------------------------------------------

fn labels(style: &str, idx: &[i64]) -> Vec<String> {
    let items: Vec<String> = idx
        .iter()
        .map(|i| format!("panel_label(\"{style}\", {i})"))
        .collect();
    let d = raw(&format!(r#"page("single", vbox([{}]))"#, items.join(", ")));
    root_texts(&d)
}

#[test]
fn panel_label_styles() {
    assert_eq!(labels("(a)", &[0, 1, 2]), ["(a)", "(b)", "(c)"]);
    assert_eq!(labels("A", &[0, 3]), ["A", "D"]);
    assert_eq!(labels("a.", &[1]), ["b."]);
    assert_eq!(labels("Fig. 1", &[0, 9]), ["Fig. 1", "Fig. 10"]);
    // The *last* counter character wins, so prose around it is safe.
    assert_eq!(labels("Panel A", &[1]), ["Panel B"]);
    // Past z: bijective base-26.
    assert_eq!(labels("(a)", &[25, 26, 27]), ["(z)", "(aa)", "(ab)"]);
}

#[test]
fn panel_label_requires_counter() {
    let e = eval_script(r#"page("single", vbox([panel_label("()", 0)]))"#).unwrap_err();
    assert!(e.to_string().contains("no counter"), "{e}");
}

#[test]
fn panels_overlay_labels_top_left() {
    // Default inset: base_size() * 0.02 = 0.16 page units (cm with presets).
    let src = r#"page(100, 100, grid(2, 0, panels([rect(), rect()]))).unit("pt")"#;
    let d = raw(src);
    let l = lay(src);
    let row = children(&d, children(&d, "root")[0].0);
    assert_eq!(row.len(), 2);
    for ((sid, s), want) in row.iter().zip(["(a)", "(b)"]) {
        assert_eq!(s.kind, NodeKind::Stack);
        assert_eq!(s.flex, Some(1.0));
        let kids = children(&d, sid);
        assert_eq!(kids.len(), 2, "panel + label");
        assert_eq!(kids[0].1.kind, NodeKind::Rect);
        // The label tag (by default an outlined text) is pinned at the inset.
        assert_eq!(kids[1].1.x, Some(0.16));
        assert_eq!(kids[1].1.y, Some(0.16));
        let fg = tag_text(&d, kids[1].0);
        assert_eq!(d.nodes[&fg].content.as_deref(), Some(want));
        assert_eq!(d.nodes[&fg].font_weight, Some(700));
        // The panel fills its cell; the label sits inset from its top-left.
        let cell = rect_of(&l, sid);
        let panel = rect_of(&l, kids[0].0);
        let label = rect_of(&l, &fg);
        approx(panel.w, cell.w);
        approx(panel.h, cell.h);
        approx(label.x, cell.x + 0.16);
        approx(label.y, cell.y + 0.16);
    }
}

#[test]
fn label_inset_follows_base_size_and_hook() {
    // Derived from base_size() by default …
    let d = raw(r#"
        fn base_size() { 10 }
        page("single", labeled(rect(), "(a)"))
        "#);
    let tag = children(&d, "root")[1].1;
    approx(tag.x.unwrap(), 0.2);
    approx(tag.y.unwrap(), 0.2);

    // … and replaced outright by a user `label_inset()`.
    let src = r#"
        fn label_inset() { 5 }
        page(100, 100, labeled(rect(), "(a)")).unit("pt")
        "#;
    let d = raw(src);
    let l = lay(src);
    let label = rect_of(&l, &tag_text(&d, children(&d, "root")[1].0));
    approx(label.x, 5.0);
    approx(label.y, 5.0);
}

/// Layout rect of label `s` (the text node) in `src`.
fn label_rect(src: &str, s: &str) -> Rect {
    rect_of(&lay_fixed(src), &text_id(&raw(src), s))
}

#[test]
fn label_corners() {
    // 100 x 80 pt panel, inset 5, label 20 x 10.
    let at = |corner: &str| {
        label_rect(
            &format!(
                r#"
                fn label_inset() {{ 5 }}
                fn label_corner() {{ "{corner}" }}
                page(100, 80, labeled(rect(), "(a)")).unit("pt")
                "#
            ),
            "(a)",
        )
    };
    for (corner, x, y) in [
        ("top-left", 5.0, 5.0),
        ("top-right", 75.0, 5.0),
        ("bottom-left", 5.0, 65.0),
        ("bottom-right", 75.0, 65.0),
    ] {
        let r = at(corner);
        approx(r.x, x);
        approx(r.y, y);
        approx(r.w, TEXT_W); // the label keeps its natural size in every corner
        approx(r.h, TEXT_H);
    }
}

#[test]
fn label_corner_structure() {
    // Non top-left corners: a filling column (no offset, no size) padded by the
    // inset and aligned to the corner — only core nodes.
    let d = raw(r#"
        fn label_corner() { "bottom-right" }
        page("single", labeled(rect(), "(a)"))
        "#);
    let kids = children(&d, "root");
    assert_eq!(kids[0].1.kind, NodeKind::Rect);
    let (cid, c) = kids[1];
    assert_eq!(c.kind, NodeKind::Column);
    assert_eq!((c.x, c.y, c.width, c.height), (None, None, None, None));
    approx(c.padding.top, 0.16);
    approx(c.padding.right, 0.16);
    assert_eq!(c.main_axis_alignment, Some(MainAxisAlign::End));
    assert_eq!(c.cross_axis_alignment, Some(CrossAxisAlign::End));
    let tag = tag_text(&d, children(&d, cid)[0].0);
    assert_eq!(d.nodes[&tag].content.as_deref(), Some("(a)"));

    let e = eval_script(
        r#"
        fn label_corner() { "middle" }
        page("single", labeled(rect(), "(a)"))
        "#,
    )
    .unwrap_err();
    assert!(e.to_string().contains("unknown corner"), "{e}");
}

#[test]
fn label_background_box() {
    // Off by default: the label is a bare text node (checked above). Set, it is
    // a stack of a filled rect under the text, padded by `label_pad()`.
    let src = r##"
        fn label_inset() { 5 }
        fn label_background() { "#ffffff" }
        fn label_pad() { 2 }
        page(100, 80, labeled(rect(), "(a)")).unit("pt")
        "##;
    let d = raw(src);
    let (bid, badge) = children(&d, "root")[1];
    assert_eq!(badge.kind, NodeKind::Stack);
    assert_eq!((badge.x, badge.y), (Some(5.0), Some(5.0)));
    let parts = children(&d, bid);
    assert_eq!(parts[0].1.kind, NodeKind::Rect);
    assert!(parts[0].1.fill.is_some());
    assert_eq!(parts[1].1.content.as_deref(), Some("(a)"));

    let l = lay_fixed(src);
    let bg = rect_of(&l, parts[0].0);
    let text = rect_of(&l, parts[1].0);
    for (got, want) in [(bg.x, 5.0), (bg.y, 5.0), (bg.w, 24.0), (bg.h, 14.0)] {
        approx(got, want);
    }
    approx(text.x, 7.0);
    approx(text.y, 7.0);

    // Works in the other corners too: the box hugs the label.
    let r = label_rect(
        &src.replace(
            "fn label_pad()",
            r#"fn label_corner() { "bottom-right" } fn label_pad()"#,
        ),
        "(a)",
    );
    approx(r.x, 100.0 - 5.0 - 2.0 - TEXT_W);
    approx(r.y, 80.0 - 5.0 - 2.0 - TEXT_H);
}

// ---- captioned panels -----------------------------------------------------

#[test]
fn captioned_panels_structure() {
    let d = raw(r#"page("double", grid(2, panels([rect(), "b"], ["First", "Second"])))"#);
    let row = children(&d, children(&d, "root")[0].0);
    assert_eq!(row.len(), 2);
    for ((cid, cell), (want_label, want_cap)) in
        row.iter().zip([("(a)", "First"), ("(b)", "Second")])
    {
        assert_eq!(cell.kind, NodeKind::Column);
        assert_eq!(cell.flex, Some(1.0)); // grid cell
        approx(cell.spacing, 0.1); // caption_gap() = gap() / 2
        assert_eq!(cell.cross_axis_alignment, Some(CrossAxisAlign::Stretch));
        let kids = children(&d, cid);
        assert_eq!(kids.len(), 2, "labeled panel + caption");
        let (sid, panel) = kids[0];
        assert_eq!(panel.kind, NodeKind::Stack);
        assert_eq!(panel.flex, Some(1.0));
        let tag = tag_text(&d, children(&d, sid)[1].0);
        assert_eq!(d.nodes[&tag].content.as_deref(), Some(want_label));
        let cap = kids[1].1;
        assert_eq!(cap.content.as_deref(), Some(want_cap));
        assert_eq!(cap.font_size, Some(8.0)); // caption(): base size, centered
        assert_eq!(cap.align, Some(TextAlign::Center));
    }
}

#[test]
fn captioned_panels_layout() {
    // 100 x 100 pt, no grid gap, caption gap 4, captions 10 tall.
    let src = r#"
        fn caption_gap() { 4 }
        page(100, 100, grid(2, 0, panels("A", [rect(), rect()], ["x", "y"]))).unit("pt")
        "#;
    let d = raw(src);
    let l = lay_fixed(src);
    let row = children(&d, children(&d, "root")[0].0);
    for (i, (cid, _)) in row.iter().enumerate() {
        let kids = children(&d, cid);
        let panel = rect_of(&l, kids[0].0);
        let cap = rect_of(&l, kids[1].0);
        let x0 = 50.0 * i as f32;
        for (got, want) in [
            (panel.x, x0),
            (panel.y, 0.0),
            (panel.w, 50.0),
            (panel.h, 86.0),
        ] {
            approx(got, want);
        }
        // The caption spans the cell width right under the panel.
        for (got, want) in [(cap.x, x0), (cap.y, 90.0), (cap.w, 50.0), (cap.h, TEXT_H)] {
            approx(got, want);
        }
    }
    let _ = text_node(&d, "A");
    let _ = text_node(&d, "B");
}

#[test]
fn captioned_accepts_nodes_and_checks_lengths() {
    let d = raw(r##"page("single", captioned(rect(), text("custom").color("#ff0000")))"##);
    let kids = children(&d, "root");
    assert_eq!(kids[0].1.kind, NodeKind::Rect);
    assert_eq!(kids[0].1.flex, Some(1.0));
    let cap = text_node(&d, "custom");
    assert!(cap.color.is_some());
    assert_eq!(cap.align, None); // a node caption is used as given

    let e = eval_script(r#"page("single", grid(2, panels([rect(), rect()], ["only one"])))"#)
        .unwrap_err();
    assert!(e.to_string().contains("2 items but 1 captions"), "{e}");
}

#[test]
fn captioned_panels_example() {
    let src = include_str!("../../../examples/captioned_panels.figs");
    let l = lay(src);
    assert!(!l.nodes.is_empty());
    assert!(l.nodes[0].rect.w <= l.page.width_pt + 0.01);
}

#[test]
fn panels_style_and_hook() {
    let d = raw(r#"page("single", hbox(panels("A", [rect(), "text panel"])))"#);
    let _ = text_node(&d, "A");
    let _ = text_node(&d, "B");
    let _ = text_node(&d, "text panel"); // bare strings work as panels too

    let d = raw(r#"
        fn label_style() { "a" }
        page("single", hbox(panels([rect(), rect()])))
        "#);
    let _ = text_node(&d, "a");
    let _ = text_node(&d, "b");
}

#[test]
fn framework_demo_example() {
    let src = include_str!("../../../examples/framework_demo.figs");
    let l = lay(src);
    assert!(!l.nodes.is_empty());
    assert!(l.nodes[0].rect.w <= l.page.width_pt + 0.01);
}

// ---- page presets ----------------------------------------------------------

#[test]
fn page_presets_default_to_cm() {
    let d = raw(r#"page("single", rect())"#);
    assert_eq!(d.page.unit, Unit::Cm);
    approx(d.page.width, 8.5);
    assert_eq!(d.page.height, PageHeight::Auto); // journal presets fit content

    let d = raw(r#"page("double", rect())"#);
    approx(d.page.width, 17.5);
    assert_eq!(d.page.height, PageHeight::Auto);

    let d = raw(r#"page("a0", rect())"#);
    approx(d.page.width, 84.1);
    approx(d.page.height.fixed().expect("fixed height"), 118.9);
}

#[test]
fn page_preset_landscape_and_override() {
    let d = raw(r##"page("a4-landscape", rect()).background("#000000").unit("mm")"##);
    approx(d.page.width, 29.7);
    approx(d.page.height.fixed().expect("fixed height"), 21.0);
    assert_eq!(d.page.unit, Unit::Mm); // core page setters still chain
    assert!(d.page.background.is_some());
}

#[test]
fn double_preset_fits_a_2x2_grid() {
    // 17.5 cm page, default 0.2 cm gap, images with no aspect -> the 4:3
    // fallback (NullMeasurer knows no image sizes): cell width
    // w = (17.5 - 0.2) / 2, row height w * 3/4, page = 2 rows + 1 gap.
    let l = lay(r#"page("double", grid(2, [
        image("a.png"), image("b.png"),
        image("c.png"), image("d.png"),
    ]))"#);
    let cm = Unit::Cm.to_pt();
    let w = (17.5 - 0.2) / 2.0 * cm;
    let h = w * 0.75;
    assert!(l.page.auto_height);
    approx(l.page.width_pt, 17.5 * cm);
    approx(l.page.height_pt, 2.0 * h + 0.2 * cm);
    let images: Vec<Rect> = l
        .nodes
        .iter()
        .filter(|n| matches!(n.content, figs_core::layout::PaintContent::Image { .. }))
        .map(|n| n.rect)
        .collect();
    assert_eq!(images.len(), 4);
    for r in images {
        approx(r.w, w);
        approx(r.h, h);
    }
}

#[test]
fn double_preset_with_aspect_and_panels() {
    // Square images under labeled panels: rows are exactly one cell tall.
    let l = lay(r#"page("double", grid(2, panels([
        image("a.png").aspect(1.0), image("b.png").aspect(1.0),
        image("c.png").aspect(1.0), image("d.png").aspect(1.0),
    ])))"#);
    let cm = Unit::Cm.to_pt();
    let w = (17.5 - 0.2) / 2.0 * cm;
    approx(l.page.height_pt, 2.0 * w + 0.2 * cm);
}

#[test]
fn paper_presets_stay_fixed() {
    let d = raw(r#"page("a4", rect())"#);
    assert_eq!(d.page.height, PageHeight::Fixed(29.7));
    let l = lay(r#"page("a4", grid(2, [image("a.png"), image("b.png")]))"#);
    assert!(!l.page.auto_height);
    approx(l.page.height_pt, 29.7 * Unit::Cm.to_pt());
}

#[test]
fn paper_size_of_journal_preset_is_auto() {
    let d = raw(r#"let s = paper_size("single"); page(s[0], s[1], rect()).unit("cm")"#);
    approx(d.page.width, 8.5);
    assert_eq!(d.page.height, PageHeight::Auto);
    let e = eval_script(r#"page("double-landscape", rect())"#).unwrap_err();
    assert!(e.to_string().contains("no landscape form"), "{e}");
}

#[test]
fn paper_size_drops_down_to_core_page() {
    let d = raw(r#"let s = paper_size("double"); page(s[0], 5, rect()).unit("cm")"#);
    approx(d.page.width, 17.5);
    approx(d.page.height.fixed().expect("fixed height"), 5.0);
}

#[test]
fn page_preset_errors() {
    let e = eval_script(r#"page("b7", rect())"#).unwrap_err();
    assert!(e.to_string().contains("unknown preset"), "{e}");
    let e = eval_script(r#"page(10, rect())"#).unwrap_err();
    assert!(e.to_string().contains("page(preset, root)"), "{e}");
}

// ---- errors point at the user's script ----------------------------------------

/// Evaluate a script that must fail; return (line, column, message).
fn fail(src: &str) -> (Option<usize>, Option<usize>, String) {
    let e = eval_script(src).expect_err("script should fail");
    (e.line, e.column, e.message)
}

#[test]
fn prelude_error_reports_user_call_site() {
    // `grid` throws deep inside prelude.rhai; the error points at the user's call.
    let (line, col, msg) = fail("let xs = [rect()];\npage(\"single\",\n    grid(0, xs))");
    assert_eq!((line, col), (Some(3), Some(5)), "{msg}");
    assert_eq!(
        msg, "line 3, col 5: grid: ncols must be a positive integer (got 0)",
        "named after the framework function, no prelude line numbers"
    );
}

#[test]
fn prelude_error_inside_user_fn_reports_the_inner_call() {
    let src = "fn figure(n) {\n    let cells = [rect(), rect()];\n    grid(n, cells)\n}\npage(\"single\", figure(0))";
    let (line, col, msg) = fail(src);
    // The user line that called into the framework, not the outer `figure(0)`.
    assert_eq!((line, col), (Some(3), Some(5)), "{msg}");
    assert!(msg.starts_with("line 3, col 5: grid: ncols"), "{msg}");
    // The user-side call trace is kept.
    assert!(
        msg.contains("in call to function 'figure' (line 5, col 16)"),
        "{msg}"
    );
}

#[test]
fn prelude_errors_name_the_called_function() {
    let (line, col, msg) = fail("\npage(\"b7\", rect())");
    assert_eq!((line, col), (Some(2), Some(1)));
    assert!(
        msg.starts_with("line 2, col 1: page: unknown preset \"b7\""),
        "{msg}"
    );

    // A helper's own `fn(): ` prefix is kept behind the user's entry point.
    let (_, col, msg) = fail(r#"page("single", hbox(panels("()", [rect()])))"#);
    assert_eq!(col, Some(21)); // `panels(`
    assert!(
        msg.contains("panels: panel_label: style \"()\" has no counter"),
        "{msg}"
    );

    // Non-throw errors raised in prelude code are attributed the same way.
    let (_, col, msg) = fail(r#"page("single", grid(2, [42]))"#);
    assert_eq!(col, Some(16));
    assert!(
        msg.starts_with("line 1, col 16: grid: Function not found: flex"),
        "{msg}"
    );
}

#[test]
fn error_in_user_hook_called_by_framework_points_at_the_hook() {
    let src = "fn base_size() { throw \"boom\" }\npage(\"single\", vbox([title(\"T\")]))";
    let (line, col, msg) = fail(src);
    assert_eq!((line, col), (Some(1), Some(18)), "{msg}");
    assert!(msg.contains("boom"), "{msg}");
    assert!(
        msg.contains("in call to function 'title' (line 2, col 22)"),
        "{msg}"
    );
}

#[test]
fn core_errors_keep_rhai_messages() {
    // Native core builder errors in user code are not rewritten.
    let (line, col, msg) = fail(r##"page(1, 1, text("x").color("#xyz"))"##);
    assert_eq!((line, col), (Some(1), Some(22)));
    assert!(msg.contains("invalid color"), "{msg}");
    assert!(!msg.contains("text:") && !msg.contains("page:"), "{msg}");
}

#[test]
fn core_page_unchanged() {
    // The 3-arg core page keeps its exact semantics (default unit, no presets).
    let d = raw(r#"page(10, 20, rect())"#);
    assert_eq!(d.page.unit, Unit::default());
    approx(d.page.width, 10.0);
    approx(d.page.height.fixed().expect("fixed height"), 20.0);
}

// ---- annotations -------------------------------------------------------------

/// Hooks that make annotation geometry exact on a pt page: inset 5, bar 4 pt
/// thick (label gap = half of it = 2), no halo (the label is a bare text node).
const PLAIN_BAR: &str = r#"
    fn scalebar_inset() { 5 }
    fn scalebar_thickness() { 4 }
    fn scalebar_halo() { "" }
"#;

/// Id of the only node matching `pred`.
fn find_id(d: &RawDocument, pred: impl Fn(&RawNode) -> bool) -> String {
    let ids: Vec<&String> = d
        .nodes
        .iter()
        .filter(|(_, n)| pred(n))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids.len(), 1, "expected exactly one match, got {ids:?}");
    ids[0].clone()
}

/// The scale bar: the only rect with a fill.
fn bar_id(d: &RawDocument) -> String {
    find_id(d, |n| n.kind == NodeKind::Rect && n.fill.is_some())
}

#[track_caller]
fn assert_xywh(r: Rect, x: f32, y: f32, w: f32, h: f32) {
    for (got, want) in [(r.x, x), (r.y, y), (r.w, w), (r.h, h)] {
        approx(got, want);
    }
}

fn hex(s: &str) -> Option<Color> {
    Color::parse_hex(s).ok()
}

#[test]
fn annotate_stacks_marks_over_item() {
    let d = raw(
        r#"page("single", annotate(image("a.png"), [spot(1, 1, 0.5), highlight(0, 0, 1, 1)]))"#,
    );
    assert_eq!(d.nodes["root"].kind, NodeKind::Stack);
    let kinds: Vec<NodeKind> = children(&d, "root").iter().map(|(_, n)| n.kind).collect();
    assert_eq!(kinds, [NodeKind::Image, NodeKind::Ellipse, NodeKind::Path]);

    // A single mark needs no array; bare strings still become body text.
    let d = raw(r#"page("single", annotate("plot", scalebar(1, "1 cm")))"#);
    let kids = children(&d, "root");
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0].1.content.as_deref(), Some("plot"));
    assert_eq!(kids[1].1.kind, NodeKind::Column);
}

#[test]
fn annotate_composes_with_labels_and_panels() {
    let d = raw(r#"page("double", grid(2, panels([
            annotate(rect(), scalebar(1, "1 cm")),
            annotate(rect(), spot(1, 1, 0.3)),
        ])))"#);
    let row = children(&d, children(&d, "root")[0].0);
    for ((sid, s), want) in row.iter().zip(["(a)", "(b)"]) {
        // labeled stack: [annotated stack, label]
        assert_eq!(s.kind, NodeKind::Stack);
        let kids = children(&d, sid);
        assert_eq!(kids[0].1.kind, NodeKind::Stack);
        assert_eq!(children(&d, kids[0].0)[0].1.kind, NodeKind::Rect);
        let tag = tag_text(&d, kids[1].0);
        assert_eq!(d.nodes[&tag].content.as_deref(), Some(want));
    }
}

#[test]
fn scalebar_cm_bottom_right_layout() {
    // 100 x 80 pt panel, 30 pt bar, label 20 x 10 centered above it, 2 apart.
    let src =
        format!(r#"{PLAIN_BAR} page(100, 80, annotate(rect(), scalebar(30, "1 um"))).unit("pt")"#);
    let d = raw(&src);
    let l = lay_fixed(&src);
    assert_xywh(rect_of(&l, &bar_id(&d)), 65.0, 71.0, 30.0, 4.0);
    assert_xywh(label_rect(&src, "1 um"), 70.0, 59.0, TEXT_W, TEXT_H);
    // Default colour: white bar and label.
    assert_eq!(d.nodes[&bar_id(&d)].fill, hex("#ffffff").map(Into::into));
    assert_eq!(text_node(&d, "1 um").color, hex("#ffffff"));
}

#[test]
fn scalebar_corner_and_no_label() {
    let src = format!(
        r#"{PLAIN_BAR} fn scalebar_corner() {{ "top-left" }}
        page(100, 80, annotate(rect(), scalebar(30, "1 um"))).unit("pt")"#
    );
    let d = raw(&src);
    assert_xywh(rect_of(&lay_fixed(&src), &bar_id(&d)), 5.0, 17.0, 30.0, 4.0);
    assert_xywh(label_rect(&src, "1 um"), 10.0, 5.0, TEXT_W, TEXT_H);

    // "" draws the bar alone, flush in the corner.
    let src = format!(
        r#"{PLAIN_BAR} fn scalebar_corner() {{ "bottom-left" }}
        page(100, 80, annotate(rect(), scalebar(30, ""))).unit("pt")"#
    );
    let d = raw(&src);
    assert!(d.nodes.values().all(|n| n.kind != NodeKind::Text));
    assert_xywh(rect_of(&lay_fixed(&src), &bar_id(&d)), 5.0, 71.0, 30.0, 4.0);
}

#[test]
fn scalebar_halo_defaults() {
    // Default halo: an outlined bar and a single label text outlined (core
    // text stroke) in the halo colour, reaching thickness / 4 past the glyphs.
    let d = raw(r#"page("double", annotate(rect(), scalebar(2, "5 um")))"#);
    let bar = &d.nodes[&bar_id(&d)];
    approx(bar.width.unwrap(), 2.0);
    approx(bar.height.unwrap(), 0.08); // base_size() * 0.01
    assert!(bar.stroke.is_some());
    let texts: Vec<&RawNode> = d
        .nodes
        .values()
        .filter(|n| n.content.as_deref() == Some("5 um"))
        .collect();
    assert_eq!(texts.len(), 1, "one label node, no shadow copy");
    let t = texts[0];
    assert_eq!((t.font_weight, t.font_size), (Some(700), Some(8.0)));
    assert_eq!(t.color, hex("#ffffff"));
    assert_eq!(t.stroke, hex("#000000a0"));
    approx(t.stroke_width.unwrap(), 0.04); // 2 x thickness / 4
    assert_eq!((t.x, t.y), (None, None));
    assert_eq!(bar.stroke, t.stroke);
}

#[test]
fn scalebar_field_units_layout() {
    // 10 of a 50-wide field = 1/5 of the panel; side inset 10% of the width.
    let at = |w: u32, corner: &str| {
        let src = format!(
            r#"{PLAIN_BAR} fn scalebar_frac_inset() {{ 0.1 }}
            fn scalebar_corner() {{ "{corner}" }}
            page({w}, 80, annotate(rect(), scalebar(10, 50, "1 um"))).unit("pt")"#
        );
        let d = raw(&src);
        (
            rect_of(&lay_fixed(&src), &bar_id(&d)),
            label_rect(&src, "1 um"),
        )
    };
    let (bar, label) = at(100, "bottom-right");
    assert_xywh(bar, 70.0, 71.0, 20.0, 4.0);
    // The label cell is centered on the bar (60..100) and the text spans it.
    assert_xywh(label, 60.0, 59.0, 40.0, TEXT_H);
    approx(label.x + label.w / 2.0, bar.x + bar.w / 2.0);

    // The bar stays exactly 1/5 of the panel whatever its width.
    let (bar, _) = at(200, "bottom-right");
    assert_xywh(bar, 140.0, 71.0, 40.0, 4.0);

    let (bar, label) = at(100, "bottom-left");
    assert_xywh(bar, 10.0, 71.0, 20.0, 4.0);
    approx(label.x + label.w / 2.0, 20.0);

    let (bar, label) = at(100, "top-right");
    assert_xywh(bar, 70.0, 5.0 + TEXT_H + 2.0, 20.0, 4.0);
    approx(label.y, 5.0);
}

#[test]
fn scalebar_errors() {
    let e = eval_script(r#"page("single", annotate(rect(), scalebar(60, 50, "x")))"#).unwrap_err();
    assert!(
        e.to_string().contains("scalebar: need 0 < len <= field"),
        "{e}"
    );
    let e = eval_script(
        r#"fn scalebar_corner() { "middle" }
        page("single", annotate(rect(), scalebar(1, "x")))"#,
    )
    .unwrap_err();
    assert!(e.to_string().contains("unknown corner \"middle\""), "{e}");
    let e = eval_script(r#"page("single", annotate(rect(), pin(rect(), "centre")))"#).unwrap_err();
    assert!(e.to_string().contains("pin: unknown corner"), "{e}");
}

#[test]
fn pin_places_any_node_in_a_corner() {
    let src = r#"page(100, 80, stack([rect(), pin(text("T"), "bottom-right", 5)])).unit("pt")"#;
    assert_xywh(label_rect(src, "T"), 75.0, 65.0, TEXT_W, TEXT_H);
    // Default inset: label_inset().
    let d = raw(r#"fn label_inset() { 3 } page(1, 1, stack([pin(text("T"), "top-left")]))"#);
    let t = text_node(&d, "T");
    assert_eq!((t.x, t.y), (Some(3.0), Some(3.0)));
}

/// Paint points of the only arrow in `src`.
fn arrow_points(src: &str) -> Vec<(f32, f32)> {
    let d = raw(src);
    let id = find_id(&d, |n| n.kind == NodeKind::Arrow);
    let l = lay_fixed(src);
    match &l.nodes.iter().find(|n| n.id == id).unwrap().content {
        PaintContent::Path(p) => p
            .segments
            .iter()
            .filter_map(|s| s.end())
            .map(|p| (p.x, p.y))
            .collect(),
        other => panic!("not a path: {other:?}"),
    }
}

#[test]
fn callout_layout() {
    // base_size 100 on a pt page: gap 1, a 10.6 tall box centering the label.
    let at = |x: i32, y: i32| {
        let src = format!(
            r#"fn base_size() {{ 100 }}
            page(100, 80, annotate(rect(), callout(50, 40, {x}, {y}, "t"))).unit("pt")"#
        );
        (label_rect(&src, "t"), arrow_points(&src))
    };
    // Target to the right: the label ends 1 left of the tail, centered on y = 40.
    let (label, pts) = at(80, 20);
    assert_xywh(label, 29.0, 35.0, TEXT_W, TEXT_H);
    assert_eq!(pts, [(50.0, 40.0), (80.0, 20.0)]);
    // Target to the left: the label starts 1 right of the tail.
    let (label, pts) = at(10, 70);
    assert_xywh(label, 51.0, 35.0, TEXT_W, TEXT_H);
    assert_eq!(pts, [(50.0, 40.0), (10.0, 70.0)]);
}

#[test]
fn callout_style_hooks() {
    let d = raw(r#"page("single", annotate(rect(), callout(1, 1, 2, 2, "t")))"#);
    let a = &d.nodes[&find_id(&d, |n| n.kind == NodeKind::Arrow)];
    assert_eq!(a.stroke, hex("#ff5a3c")); // accent()
    approx(a.stroke_width.unwrap(), 0.04); // base_size() * 0.005
    let t = text_node(&d, "t");
    assert_eq!(
        (t.color, t.font_size, t.font_weight),
        (hex("#ff5a3c"), Some(8.0), Some(700))
    );

    let d = raw(r##"
        fn accent() { "#00ff00" }
        fn annotation_width() { 0.1 }
        page("single", annotate(rect(), [
            callout(1, 1, 2, 2, "t"), spot(1, 1, 1), highlight(0, 0, 1, 1),
        ]))
        "##);
    let marks: Vec<&RawNode> = d
        .nodes
        .values()
        .filter(|n| matches!(n.kind, NodeKind::Arrow | NodeKind::Ellipse | NodeKind::Path))
        .collect();
    assert_eq!(marks.len(), 3);
    for n in marks {
        assert_eq!(n.stroke, hex("#00ff00"));
        approx(n.stroke_width.unwrap(), 0.1);
    }
}

#[test]
fn highlight_and_spot() {
    let src = r#"fn annotation_width() { 2 }
        page(100, 80, annotate(rect(), [highlight(10, 20, 30, 15), spot(70, 40, 5)])).unit("pt")"#;
    let d = raw(src);
    let l = lay_fixed(src);
    let hid = find_id(&d, |n| n.kind == NodeKind::Path);
    let h = &d.nodes[&hid];
    assert!(h.closed);
    assert_eq!(
        h.points,
        [[0.0, 0.0], [30.0, 0.0], [30.0, 15.0], [0.0, 15.0]]
    );
    assert_eq!(h.dash, [6.0, 4.0]); // annotation_dash(): 3x / 2x the width
    assert_eq!(h.fill, None);
    assert_xywh(rect_of(&l, &hid), 10.0, 20.0, 30.0, 15.0);

    let sid = find_id(&d, |n| n.kind == NodeKind::Ellipse);
    assert_eq!(d.nodes[&sid].dash, [6.0, 4.0]);
    assert_xywh(rect_of(&l, &sid), 65.0, 35.0, 10.0, 10.0);

    // Core setters still chain: a solid highlight.
    let d = raw(r#"page("single", annotate(rect(), highlight(0, 0, 1, 1).dash([])))"#);
    assert!(d.nodes[&find_id(&d, |n| n.kind == NodeKind::Path)]
        .dash
        .is_empty());
}

#[test]
fn annotated_figure_example() {
    let src = include_str!("../../../examples/annotated_figure.figs");
    let l = lay(src);
    assert!(!l.nodes.is_empty());
    assert!(l.nodes[0].rect.w <= l.page.width_pt + 0.01);
}

// ---- style presets -----------------------------------------------------------

/// Values of the hook expressions `exprs` after the script lines `prelude`
/// (e.g. `style("nature");` or a user hook), as Rhai prints them.
fn hooks(prelude: &str, exprs: &[&str]) -> Vec<String> {
    let parts: Vec<String> = exprs.iter().map(|e| format!("`${{{e}}}`")).collect();
    let src = format!(
        "{prelude}\nlet s = \"\";\nfor x in [{}] {{ s += x + \"|\"; }}\npage(1, 1, text(s))",
        parts.join(", ")
    );
    let d = raw(&src);
    let s = d.nodes["root"].content.clone().unwrap();
    s.trim_end_matches('|')
        .split('|')
        .map(String::from)
        .collect()
}

const STYLE_HOOKS: &[&str] = &[
    "base_size()",
    "label_size()",
    "label_style()",
    "font_family()",
    "single_width()",
    "double_width()",
    "title_size()",
    "gap()",
];

#[test]
fn style_presets_set_hook_values() {
    let at = |p: &str| hooks(&format!("style(\"{p}\");"), STYLE_HOOKS);
    // base, label size, label, font, single, double, title, gap
    let want: [(&str, [&str; 8]); 6] = [
        (
            "default",
            ["8", "10.0", "(a)", "", "8.5", "17.5", "12.0", "0.2"],
        ),
        (
            "nature",
            ["7", "8", "a", "Arial", "8.9", "18.3", "10.5", "0.2"],
        ),
        (
            "science",
            ["7", "8", "A", "Arial", "5.7", "18.3", "10.5", "0.2"],
        ),
        (
            "acs",
            ["7", "8", "(a)", "Arial", "8.25", "17.78", "10.5", "0.2"],
        ),
        (
            "rsc",
            ["7", "8", "(a)", "Arial", "8.3", "17.1", "10.5", "0.2"],
        ),
        (
            "poster",
            ["24", "30.0", "(a)", "", "8.5", "17.5", "60", "1.0"],
        ),
    ];
    for (preset, values) in want {
        assert_eq!(at(preset), values, "preset {preset}");
    }
    // No `style` call at all = "default".
    assert_eq!(hooks("", STYLE_HOOKS), at("default"));
}

#[test]
fn style_preset_applies_to_components_and_page() {
    let d = raw(r#"
        style("nature");
        page("single", hbox(panels([rect(), rect()])))
        "#);
    approx(d.page.width, 8.9);
    assert_eq!(d.page.font_family.as_deref(), Some("Arial"));
    let a = text_node(&d, "a");
    assert_eq!(a.font_size, Some(8.0));
    assert_eq!(a.font_weight, Some(700));
    let _ = text_node(&d, "b");

    // The page font is a plain core setting: chaining `.font` overrides it,
    // and the default style sets none (the renderer's default sans).
    let d = raw(r#"style("nature"); page("single", rect()).font("Times")"#);
    assert_eq!(d.page.font_family.as_deref(), Some("Times"));
    assert_eq!(raw(r#"page("single", rect())"#).page.font_family, None);
}

#[test]
fn user_hook_beats_preset() {
    let v = hooks(
        r#"fn base_size() { 11 } fn label_style() { "i." } style("nature");"#,
        &[
            "base_size()",
            "label_style()",
            "label_size()",
            "font_family()",
        ],
    );
    // User hooks win; the preset still sets everything else.
    assert_eq!(v, ["11", "i.", "8", "Arial"]);
}

#[test]
fn style_map_overrides() {
    let v = hooks(
        r#"style(#{ base_size: 9, label_style: "A" });"#,
        &[
            "base_size()",
            "label_style()",
            "label_size()",
            "label_inset()",
        ],
    );
    // Derived hooks follow the overridden base.
    assert_eq!(v, ["9", "A", "11.25", "0.18"]);

    // Merged over a preset: one call or two.
    for prelude in [
        r#"style("nature", #{ base_size: 6 });"#,
        r#"style("nature"); style(#{ base_size: 6 });"#,
    ] {
        assert_eq!(
            hooks(prelude, &["base_size()", "label_style()", "font_family()"]),
            ["6", "a", "Arial"],
            "{prelude}"
        );
    }
    // A later preset starts over.
    let v = hooks(
        r#"style(#{ base_size: 6 }); style("science");"#,
        &["base_size()"],
    );
    assert_eq!(v, ["7"]);
}

#[test]
fn every_style_key_drives_its_hook() {
    // Each key `style(map)` accepts is read by the hook of the same name, and
    // every preset only uses such keys.
    let src = r#"
        let bad = [];
        for k in style_keys() {
            style("default");
            let m = #{};
            m[k] = "__set__";
            style(m);
            if Fn(k).call() != "__set__" { bad.push(k); }
        }
        for p in style_presets().values() {
            for k in p.keys() { if !style_keys().contains(k) { bad.push(k); } }
        }
        let s = "";
        for k in bad { s += k + " "; }
        page(1, 1, text(s))
    "#;
    assert_eq!(raw(src).nodes["root"].content.as_deref(), Some(""));
}

#[test]
fn style_errors() {
    let (line, col, msg) = fail("\nstyle(\"natur\");\npage(\"single\", rect())");
    assert_eq!((line, col), (Some(2), Some(1)), "{msg}");
    assert_eq!(
        msg,
        "line 2, col 1: style: unknown preset \"natur\" (acs, default, nature, poster, rsc, science)"
    );

    let (_, _, msg) = fail(r#"style(#{ base_sise: 9 }); page("single", rect())"#);
    assert!(
        msg.contains("style: unknown key \"base_sise\" (keys are hook names: base_size,"),
        "{msg}"
    );
    let (_, _, msg) = fail(r#"style(7); page("single", rect())"#);
    assert!(
        msg.contains("style: expected a preset name or a map"),
        "{msg}"
    );
}

#[test]
fn style_is_reset_per_evaluation() {
    let _ = raw(r#"style("poster"); page("single", rect())"#);
    assert_eq!(hooks("", &["base_size()"]), ["8"]);
}

// ---- label halo ----------------------------------------------------------------

#[test]
fn label_halo_structure() {
    let d = raw(r#"page("single", labeled(rect(), "(a)"))"#);
    // The label is a single text node carrying a core outline (`stroke`) in
    // the halo colour, reaching `r` past the glyphs: width 2r, round joins by
    // default (resolve), no copies, no margins.
    let (_, t) = children(&d, "root")[1];
    assert_eq!(t.kind, NodeKind::Text);
    assert_eq!(t.content.as_deref(), Some("(a)"));
    assert_eq!(t.color, None, "the label keeps the default text colour");
    assert_eq!(t.stroke, hex("#ffffff"));
    let r = 10.0 * 0.0035; // label_size() * 0.0035
    approx(t.stroke_width.unwrap(), 2.0 * r);
    assert_eq!(t.join, None);
    assert!(t.margin.is_zero());
    assert_eq!(
        d.nodes
            .values()
            .filter(|n| n.content.as_deref() == Some("(a)"))
            .count(),
        1
    );
    let doc = Document::from_script(r#"page("single", labeled(rect(), "(a)"))"#).unwrap();
    let props = doc
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            figs_core::schema::NodeType::Text(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let s = props.stroke.as_ref().expect("resolved outline");
    assert_eq!(s.join, figs_core::geom::LineJoin::Round);
    approx(s.width, 2.0 * r * Unit::Cm.to_pt());

    // "" draws a bare label; a background box replaces the halo.
    let d = raw(r#"fn label_halo() { "" } page("single", labeled(rect(), "(a)"))"#);
    assert_eq!(children(&d, "root")[1].1.kind, NodeKind::Text);
    let d = raw(r##"
        fn label_background() { "#ffffff" }
        page("single", labeled(rect(), "(a)"))
        "##);
    assert!(d.nodes.values().all(|n| n.color.is_none()));
}

#[test]
fn halo_lays_out_like_the_bare_label() {
    // The outline is purely visual: the label lands exactly where a bare
    // label would, in every corner — also with an integer width.
    for corner in ["top-left", "top-right", "bottom-left", "bottom-right"] {
        let src = |halo: &str| {
            format!(
                r#"fn label_inset() {{ 5 }} fn label_corner() {{ "{corner}" }}
                {halo}
                page(100, 80, labeled(rect(), "(a)")).unit("pt")"#
            )
        };
        let bare = label_rect(&src(r#"fn label_halo() { "" }"#), "(a)");
        for (halo, w) in [("", 10.0 * 0.0035), ("fn label_halo_width() { 2 }", 2.0)] {
            let src = src(halo);
            let r = label_rect(&src, "(a)");
            assert_xywh(r, bare.x, bare.y, bare.w, bare.h);
            // … carrying an outline reaching `w` past the glyphs.
            let d = raw(&src);
            approx(text_node(&d, "(a)").stroke_width.unwrap(), 2.0 * w);
        }
    }
}

// ---- labeled == pin ------------------------------------------------------------

/// Layout of `src` (fixed text metrics) as (id, rect) pairs, sorted by id.
fn rects(src: &str) -> Vec<(String, Rect)> {
    let mut v: Vec<_> = lay_fixed(src)
        .nodes
        .into_iter()
        .map(|n| (n.id, n.rect))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn labeled_is_pin_of_the_label_tag() {
    for corner in ["top-left", "top-right", "bottom-left", "bottom-right"] {
        let hooks = format!(r#"fn label_inset() {{ 5 }} fn label_corner() {{ "{corner}" }}"#);
        let a = rects(&format!(
            r#"{hooks} page(100, 80, labeled(rect(), "(a)")).unit("pt")"#
        ));
        let b = rects(&format!(
            r#"{hooks} page(100, 80, stack([rect(),
                pin(label_tag("(a)"), label_corner(), label_inset())])).unit("pt")"#
        ));
        assert_eq!(a.len(), b.len(), "{corner}");
        for ((ia, ra), (ib, rb)) in a.iter().zip(&b) {
            assert_eq!(ia, ib, "{corner}");
            assert_xywh(*ra, rb.x, rb.y, rb.w, rb.h);
        }
    }
}

// ---- caption centering ---------------------------------------------------------

#[test]
fn captions_center_under_their_panels() {
    // Natural-height panels in a row (as in examples/poster.figs): each
    // caption box spans its panel, so centered text sits under its middle.
    let src = r#"
        page(100, 100, row([
            captioned(rect().height(30), "x").flex(1),
            captioned(rect().height(20), "y").flex(2),
        ]).spacing(10).cross("center")).unit("pt")
        "#;
    let d = raw(src);
    let l = lay_fixed(src);
    for (cid, _) in children(&d, "root") {
        let kids = children(&d, cid);
        let panel = rect_of(&l, kids[0].0);
        let cap = rect_of(&l, kids[1].0);
        approx(cap.x, panel.x);
        approx(cap.w, panel.w);
        approx(cap.x + cap.w / 2.0, panel.x + panel.w / 2.0);
        approx(cap.y, panel.y + panel.h + 0.1); // caption_gap() right under it
        assert_eq!(kids[1].1.align, Some(TextAlign::Center));
    }
}

#[test]
fn style_presets_example() {
    let src = include_str!("../../../examples/style_presets.figs");
    let d = raw(src);
    assert_eq!(d.page.font_family.as_deref(), Some("Arial"));
    let l = lay(src);
    assert!(l.nodes[0].rect.w <= l.page.width_pt + 0.01);
}

#[test]
fn poster_example_uses_the_poster_style() {
    let d = raw(include_str!("../../../examples/poster.figs"));
    let title = text_node(&d, "Free Energy Surface Analysis");
    assert_eq!(title.font_size, Some(60.0));
    let cap = text_node(&d, "(a) FES 2D Landscape");
    assert_eq!(cap.font_size, Some(24.0));
    assert_eq!(cap.align, Some(TextAlign::Center));
}
