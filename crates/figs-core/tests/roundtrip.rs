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
    assert!((d1.page.height_pt - d2.page.height_pt).abs() < 1e-3);
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
fn four_panel_round_trips() {
    let src = include_str!("../../../examples/four_panel.toml");
    let raw1 = parse_str(src).expect("parse 1");
    let raw2 = parse_str(&to_toml(&raw1)).expect("parse 2");
    let d1 = resolve(raw1).expect("resolve 1");
    let d2 = resolve(raw2).expect("resolve 2");
    assert_eq!(d1.nodes.len(), d2.nodes.len());
    assert_eq!(kinds(&d1), kinds(&d2));
}
