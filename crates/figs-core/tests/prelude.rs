//! Tests for the framework prelude (`prelude.rhai`): the concise, opinionated
//! layer over the explicit core builders. Covers semantic text and its base-size
//! hook, bare-string sugar, `vbox`/`hbox`, `grid`, panel labels, page presets,
//! and that core setters still chain on everything the framework returns.
//!
//! Labeled panels overlay via the core `stack` / `.at`.

#![cfg(feature = "script")]

use figs_core::geom::{CrossAxisAlign, MainAxisAlign, Rect, Size, TextAlign};
use figs_core::layout::ComputedLayout;
use figs_core::schema::{ImageProps, NodeKind, RawDocument, RawNode, TextProps};
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

/// The raw text node with this content.
fn text_node<'a>(d: &'a RawDocument, content: &str) -> &'a RawNode {
    d.nodes
        .values()
        .find(|n| n.kind == NodeKind::Text && n.content.as_deref() == Some(content))
        .unwrap_or_else(|| panic!("no text node with content {content:?}"))
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
        assert_eq!(kids[1].1.content.as_deref(), Some(want));
        assert_eq!(kids[1].1.font_weight, Some(700));
        // The panel fills its cell; the label sits inset from its top-left.
        assert_eq!(kids[1].1.x, Some(0.16));
        assert_eq!(kids[1].1.y, Some(0.16));
        let cell = rect_of(&l, sid);
        let panel = rect_of(&l, kids[0].0);
        let label = rect_of(&l, kids[1].0);
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
    let t = text_node(&d, "(a)");
    approx(t.x.unwrap(), 0.2);
    approx(t.y.unwrap(), 0.2);

    // … and replaced outright by a user `label_inset()`.
    let src = r#"
        fn label_inset() { 5 }
        page(100, 100, labeled(rect(), "(a)")).unit("pt")
        "#;
    let d = raw(src);
    let l = lay(src);
    let label = rect_of(&l, children(&d, "root")[1].0);
    approx(label.x, 5.0);
    approx(label.y, 5.0);
}

/// Layout rect of label `s` (the text node) in `src`.
fn label_rect(src: &str, s: &str) -> Rect {
    let d = raw(src);
    let id = d
        .nodes
        .iter()
        .find(|(_, n)| n.kind == NodeKind::Text && n.content.as_deref() == Some(s))
        .map(|(id, _)| id.clone())
        .unwrap();
    rect_of(&lay_fixed(src), &id)
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
    assert_eq!(children(&d, cid)[0].1.content.as_deref(), Some("(a)"));

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
        assert_eq!(children(&d, sid)[1].1.content.as_deref(), Some(want_label));
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
    approx(d.page.height, 6.375);

    let d = raw(r#"page("double", rect())"#);
    approx(d.page.width, 17.5);

    let d = raw(r#"page("a0", rect())"#);
    approx(d.page.width, 84.1);
    approx(d.page.height, 118.9);
}

#[test]
fn page_preset_landscape_and_override() {
    let d = raw(r##"page("a4-landscape", rect()).background("#000000").unit("mm")"##);
    approx(d.page.width, 29.7);
    approx(d.page.height, 21.0);
    assert_eq!(d.page.unit, Unit::Mm); // core page setters still chain
    assert!(d.page.background.is_some());
}

#[test]
fn paper_size_drops_down_to_core_page() {
    let d = raw(r#"let s = paper_size("double"); page(s[0], 5, rect()).unit("cm")"#);
    approx(d.page.width, 17.5);
    approx(d.page.height, 5.0);
}

#[test]
fn page_preset_errors() {
    let e = eval_script(r#"page("b7", rect())"#).unwrap_err();
    assert!(e.to_string().contains("unknown preset"), "{e}");
    let e = eval_script(r#"page(10, rect())"#).unwrap_err();
    assert!(e.to_string().contains("page(preset, root)"), "{e}");
}

#[test]
fn core_page_unchanged() {
    // The 3-arg core page keeps its exact semantics (default unit, no presets).
    let d = raw(r#"page(10, 20, rect())"#);
    assert_eq!(d.page.unit, Unit::default());
    approx(d.page.width, 10.0);
    approx(d.page.height, 20.0);
}
