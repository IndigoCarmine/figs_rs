//! Tests for the framework prelude (`prelude.rhai`): the concise, opinionated
//! layer over the explicit core builders. Covers semantic text and its base-size
//! hook, bare-string sugar, `vbox`/`hbox`, `grid`, panel labels, page presets,
//! and that core setters still chain on everything the framework returns.
//!
//! Labeled panels overlay via the core `stack` / `.at`.

#![cfg(feature = "script")]

use figs_core::geom::{CrossAxisAlign, Rect, TextAlign};
use figs_core::layout::ComputedLayout;
use figs_core::schema::{NodeKind, RawDocument, RawNode};
use figs_core::units::Unit;
use figs_core::{eval_script, layout, Document, NullMeasurer};

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
        // The panel fills its cell; the label sits at the cell's top-left.
        let cell = rect_of(&l, sid);
        let panel = rect_of(&l, kids[0].0);
        let label = rect_of(&l, kids[1].0);
        approx(panel.w, cell.w);
        approx(panel.h, cell.h);
        approx(label.x, cell.x);
        approx(label.y, cell.y);
    }
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
    approx(d.page.height, 20.0);
}
