//! Smoke test: every bundled example resolves and lays out without panicking.
//! Text/image leaves size to zero under `NullMeasurer`, which is fine here —
//! we only assert the structure is valid and the engine is total.

use figs_core::{layout, Document, NullMeasurer};

fn check(path: &str, src: &str) {
    let doc = Document::from_toml(src)
        .unwrap_or_else(|e| panic!("example `{path}` failed to resolve: {e}"));
    let l = layout(&doc, &NullMeasurer);
    assert!(
        !l.nodes.is_empty(),
        "example `{path}` produced no computed nodes"
    );
    // Root is painted first and fills (at most) the page.
    let root = &l.nodes[0];
    assert!(root.rect.w <= l.page.width_pt + 0.01);
    assert!(root.rect.h <= l.page.height_pt + 0.01);
}

#[test]
fn four_panel_example() {
    check(
        "examples/four_panel.toml",
        include_str!("../../../examples/four_panel.toml"),
    );
}

#[test]
fn poster_example() {
    check(
        "examples/poster.toml",
        include_str!("../../../examples/poster.toml"),
    );
}

#[test]
fn grid_4panel_example() {
    check(
        "examples/grid_4panel.toml",
        include_str!("../../../examples/grid_4panel.toml"),
    );
}
