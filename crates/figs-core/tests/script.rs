//! Tests for the Rhai scripting front-end: a script evaluates to a `RawDocument`
//! that flows unchanged through `resolve`/`layout`. Covers structure, coercion,
//! variables/arithmetic, component reuse, value semantics, errors and the sandbox.
//!
//! Properties are set by chaining methods (`.size(24).bold()`), so scripts need no
//! map literals; hex colors still use `r##"..."##` so the `"#` doesn't close the
//! raw string.

#![cfg(feature = "script")]

use figs_core::geom::{Axis, Color, MainAxisAlign, TextAlign};
use figs_core::schema::{NodeKind, NodeType, RectProps, TextProps};
use figs_core::units::Unit;
use figs_core::{eval_script, layout, Document, NullMeasurer};

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-3, "expected {b}, got {a}");
}

fn doc(src: &str) -> Document {
    Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"))
}

/// First resolved text node whose content matches.
fn text_of<'a>(d: &'a Document, content: &str) -> &'a TextProps {
    d.nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeType::Text(t) if t.content == content => Some(t),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text node with content {content:?}"))
}

fn first_rect(d: &Document) -> &RectProps {
    d.nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeType::Rect(r) => Some(r),
            _ => None,
        })
        .expect("a rect node")
}

#[test]
fn lowers_structure_and_ids() {
    let raw = eval_script(
        r##"
        page(10, 10, col([
            text("hi"),
            rect().fill("#000000"),
        ]).spacing(2)).unit("cm")
        "##,
    )
    .unwrap();

    assert_eq!(raw.page.root, "root");
    assert_eq!(raw.nodes.len(), 3);
    let root = &raw.nodes["root"];
    assert_eq!(root.kind, NodeKind::Column);
    assert_eq!(root.spacing, 2.0); // raw, pre-resolve (page units)
    assert_eq!(root.children.len(), 2);
    for c in &root.children {
        assert!(raw.nodes.contains_key(c), "child id `{c}` missing");
    }
}

#[test]
fn flows_through_resolve_with_units() {
    let d = doc(r#"page(10, 10, text("x")).unit("cm")"#);
    // cm -> pt page conversion happens in resolve, unchanged by the script layer.
    approx(d.page.width_pt, Unit::Cm.convert(10.0));
    let root = &d.nodes[d.root];
    assert!(matches!(root.kind, NodeType::Text(_)));
}

#[test]
fn coerces_int_and_float_numbers() {
    let d = doc(
        r#"
        page(10, 10, col([
            text("i").size(16),
            text("f").size(16.0),
        ])).unit("cm")
        "#,
    );
    approx(text_of(&d, "i").font_size, 16.0);
    approx(text_of(&d, "f").font_size, 16.0);
}

#[test]
fn coerces_colors_enums_edges() {
    let d = doc(
        r##"
        page(10, 10, col([
            text("t").color("#ffffff").center(),
            rect().fill("#000000").margin(2).pad(1),
        ]).main("space_between")).unit("cm")
        "##,
    );

    let t = text_of(&d, "t");
    assert_eq!(t.color, Color::WHITE);
    assert_eq!(t.align, TextAlign::Center);

    // Container axis + alignment on the root column.
    assert!(matches!(
        &d.nodes[d.root].kind,
        NodeType::Container {
            axis: Axis::Vertical,
            main_axis: MainAxisAlign::SpaceBetween,
            ..
        }
    ));

    // Edges scaled by the cm factor in resolve (uniform via `.margin`/`.pad`).
    let factor = Unit::Cm.to_pt();
    let rect_node = d
        .nodes
        .iter()
        .find(|n| matches!(n.kind, NodeType::Rect(_)))
        .unwrap();
    approx(rect_node.common.margin.top, 2.0 * factor);
    approx(rect_node.common.padding.top, 1.0 * factor);
    approx(rect_node.common.padding.left, 1.0 * factor);
    assert!(first_rect(&d).fill.is_some());
}

#[test]
fn variables_and_arithmetic() {
    let d = doc(
        r#"
        let base = 16;
        page(10, 10, text("scaled").size(base * 2)).unit("cm")
        "#,
    );
    approx(text_of(&d, "scaled").font_size, 32.0);
}

#[test]
fn component_function_reused() {
    let d = doc(
        r#"
        fn captioned(src, cap) {
            col([ image(src), text(cap) ])
        }
        page(10, 10, row([ captioned("a.png", "A"), captioned("b.png", "B") ])).unit("cm")
        "#,
    );
    // Two independent captioned subtrees → both captions present.
    let _ = text_of(&d, "A");
    let _ = text_of(&d, "B");
    // 1 row + 2 cols + 2 images + 2 texts = 7 nodes.
    assert_eq!(d.nodes.len(), 7);
}

#[test]
fn value_semantics_clone_subtrees() {
    // The same node value reused twice must clone (distinct ids), so resolve's
    // single-parent invariant is not violated.
    let d = doc(
        r##"
        let r = rect().fill("#123456");
        page(10, 10, col([ r, r ])).unit("cm")
        "##,
    );
    let kids = match &d.nodes[d.root].kind {
        NodeType::Container { children, .. } => children.len(),
        _ => panic!("root container"),
    };
    assert_eq!(kids, 2);
    assert_eq!(d.nodes.len(), 3);
}

// ---- errors ----------------------------------------------------------------

#[test]
fn error_invalid_color() {
    let e = eval_script(r##"page(1, 1, text("x").color("#xyz"))"##).unwrap_err();
    assert!(e.to_string().contains("invalid color"), "{e}");
}

#[test]
fn error_unknown_enum_value() {
    let e = eval_script(r#"page(1, 1, text("x").align("centre"))"#).unwrap_err();
    assert!(e.to_string().contains("invalid value"), "{e}");
}

#[test]
fn error_unknown_method() {
    // An unknown property method is a (positioned) runtime error.
    let e = eval_script(r##"page(1, 1, text("x").colour("#fff"))"##).unwrap_err();
    assert!(e.to_string().contains("colour"), "{e}");
}

#[test]
fn error_page_arity() {
    // page() needs width, height and a root node.
    let e = eval_script(r#"page(10, 10)"#).unwrap_err();
    assert!(!e.to_string().is_empty());
}

#[test]
fn error_not_a_document() {
    // A script that doesn't end with page(..) is rejected with a helpful message.
    let e = eval_script("42").unwrap_err();
    assert!(e.to_string().contains("must end with"), "{e}");
}

#[test]
fn error_syntax_reports_position() {
    let e = eval_script("page(10, 10, ").unwrap_err();
    assert!(e.line.is_some(), "expected a line number, got: {e}");
}

// ---- sandbox ---------------------------------------------------------------

#[test]
fn sandbox_blocks_file_import() {
    let e = eval_script(r#"import "std" as s; page(1, 1, text("x"))"#);
    assert!(e.is_err(), "file import should be rejected");
}

#[test]
fn sandbox_bounds_runaway_loops() {
    let e = eval_script(
        r#"
        let mut i = 0;
        while true { i += 1; }
        page(1, 1, text("x"))
        "#,
    );
    assert!(e.is_err(), "an infinite loop should hit the operations limit");
}

// ---- example smoke test ----------------------------------------------------

fn check_script(path: &str, src: &str) {
    let d =
        Document::from_script(src).unwrap_or_else(|e| panic!("example `{path}` failed: {e}"));
    let l = layout(&d, &NullMeasurer);
    assert!(!l.nodes.is_empty(), "example `{path}` produced no nodes");
    let root = &l.nodes[0];
    assert!(root.rect.w <= l.page.width_pt + 0.01);
    assert!(root.rect.h <= l.page.height_pt + 0.01);
}

#[test]
fn hello_figs_example() {
    check_script(
        "examples/hello.figs",
        include_str!("../../../examples/hello.figs"),
    );
}

#[test]
fn scale_figs_example() {
    check_script(
        "examples/scale.figs",
        include_str!("../../../examples/scale.figs"),
    );
}

#[test]
fn poster_figs_example() {
    check_script(
        "examples/poster.figs",
        include_str!("../../../examples/poster.figs"),
    );
}
