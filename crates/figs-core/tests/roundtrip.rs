//! TOML round-trip: the `Serialize` impls must be lossless, so parsing a
//! document, re-serializing it and parsing again yields an equivalent document.

use std::collections::BTreeMap;

use figs_core::schema::parse::{parse_str, RawDocument, RawNode, RawPage};
use figs_core::schema::resolve::resolve;
use figs_core::schema::{Document, NodeType};
use serde::Serialize;

/// Mirrors how the editor app serializes: nodes in a stable, sorted order.
#[derive(Serialize)]
struct DocView<'a> {
    page: &'a RawPage,
    nodes: BTreeMap<&'a String, &'a RawNode>,
}

fn to_toml(doc: &RawDocument) -> String {
    let view = DocView {
        page: &doc.page,
        nodes: doc.nodes.iter().collect(),
    };
    toml::to_string_pretty(&view).expect("serialize")
}

fn kind_tag(t: &NodeType) -> &'static str {
    match t {
        NodeType::Container { .. } => "container",
        NodeType::Stack { .. } => "stack",
        NodeType::Image(_) => "image",
        NodeType::Text(_) => "text",
        NodeType::Rect(_) => "rect",
    }
}

fn kinds(doc: &Document) -> Vec<(String, &'static str)> {
    let mut v: Vec<_> = doc
        .nodes
        .iter()
        .map(|n| (n.id.clone(), kind_tag(&n.kind)))
        .collect();
    v.sort();
    v
}

#[test]
fn poster_round_trips() {
    let src = include_str!("../../../examples/poster.toml");

    let raw1 = parse_str(src).expect("parse 1");
    let reserialized = to_toml(&raw1);
    let raw2 = parse_str(&reserialized).expect("parse 2");

    let d1 = resolve(raw1).expect("resolve 1");
    let d2 = resolve(raw2).expect("resolve 2");

    // Page geometry survives.
    assert!((d1.page.width_pt - d2.page.width_pt).abs() < 1e-3);
    assert!((d1.page.height_pt.unwrap() - d2.page.height_pt.unwrap()).abs() < 1e-3);
    assert_eq!(d1.page.dpi, d2.page.dpi);
    assert_eq!(d1.page.background, d2.page.background);

    // Same set of nodes and kinds.
    assert_eq!(d1.nodes.len(), d2.nodes.len());
    assert_eq!(kinds(&d1), kinds(&d2));
}

#[test]
fn page_font_family_round_trips() {
    let src = r#"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "t"
        font_family = "Liberation Sans"
        [nodes.t]
        type = "text"
        content = "hi"
    "#;
    let raw1 = parse_str(src).expect("parse 1");
    assert_eq!(raw1.page.font_family.as_deref(), Some("Liberation Sans"));
    let raw2 = parse_str(&to_toml(&raw1)).expect("parse 2");
    let d2 = resolve(raw2).expect("resolve 2");
    assert_eq!(d2.page.font_family.as_deref(), Some("Liberation Sans"));
}

#[test]
fn stack_and_offsets_round_trip() {
    let src = r#"
        [page]
        width = 10
        height = 10
        unit = "cm"
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["base", "label"]
        [nodes.base]
        type = "rect"
        [nodes.label]
        type = "text"
        content = "(a)"
        x = 0.25
        y = 1.5
    "#;
    let raw1 = parse_str(src).expect("parse 1");
    let out = to_toml(&raw1);
    // Unset offsets stay out of the saved file.
    let x_lines = out.lines().filter(|l| l.trim_start().starts_with("x =")).count();
    assert_eq!(x_lines, 1, "{out}");
    let raw2 = parse_str(&out).expect("parse 2");
    assert_eq!(raw2.nodes["s"].kind, figs_core::schema::NodeKind::Stack);
    assert_eq!(raw2.nodes["label"].x, Some(0.25));
    assert_eq!(raw2.nodes["label"].y, Some(1.5));
    assert_eq!(raw2.nodes["base"].x, None);

    let d1 = resolve(raw1).expect("resolve 1");
    let d2 = resolve(raw2).expect("resolve 2");
    assert_eq!(kinds(&d1), kinds(&d2));
}

/// The editor lowers scripts to TOML: a `stack` + `.at()` script must survive it.
#[cfg(feature = "script")]
#[test]
fn script_stack_lowers_to_toml_and_back() {
    let raw1 = figs_core::eval_script(
        r#"page(10, 10, stack([ rect(), text("(a)").at(1, 2) ])).unit("cm")"#,
    )
    .expect("eval");
    let raw2 = parse_str(&to_toml(&raw1)).expect("parse lowered TOML");
    let label = raw2
        .nodes
        .values()
        .find(|n| n.content.as_deref() == Some("(a)"))
        .expect("label node");
    assert_eq!((label.x, label.y), (Some(1.0), Some(2.0)));
    let d1 = resolve(raw1).expect("resolve 1");
    let d2 = resolve(raw2).expect("resolve 2");
    assert_eq!(kinds(&d1), kinds(&d2));
}

#[test]
fn four_panel_round_trips() {
    let src = include_str!("../../../examples/four_panel.toml");
    let raw1 = parse_str(src).expect("parse 1");
    let raw2 = parse_str(&to_toml(&raw1)).expect("parse 2");
    let d1 = resolve(raw1).expect("resolve 1");
    let d2 = resolve(raw2).expect("resolve 2");
    assert_eq!(d1.nodes.len(), d2.nodes.len());
    assert_eq!(kinds(&d1), kinds(&d2));
}

#[test]
fn auto_page_height_round_trips() {
    let src = r#"
        [page]
        width = 17.5
        height = "auto"
        unit = "cm"
        root = "r"
        [nodes.r]
        type = "rect"
    "#;
    let raw1 = parse_str(src).expect("parse 1");
    assert_eq!(raw1.page.height, figs_core::schema::PageHeight::Auto);
    let out = to_toml(&raw1);
    assert!(out.contains(r#"height = "auto""#), "{out}");
    let raw2 = parse_str(&out).expect("parse 2");
    assert_eq!(raw2.page.height, figs_core::schema::PageHeight::Auto);
    assert_eq!(resolve(raw2).expect("resolve").page.height_pt, None);

    // A fixed height still serializes as a plain number (integers parse too).
    let raw = parse_str("[page]\nwidth = 10\nheight = 20\nroot = \"r\"\n[nodes.r]\ntype = \"rect\"\n")
        .expect("parse fixed");
    assert_eq!(raw.page.height, figs_core::schema::PageHeight::Fixed(20.0));
    let out = to_toml(&raw);
    assert!(out.contains("height = 20"), "{out}");
}
