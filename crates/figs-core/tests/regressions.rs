//! Regressions from a coordinate-free rewrite of a large poster:
//! - arrays (a helper's connectors) among the items of every framework
//!   helper, spliced in as core containers do;
//! - "fill" scheme arrows in a scheme that isn't stretched keep a minimum
//!   length, and a viewbox shape that collapses to a point draws nothing;
//! - an aspect-ratio panel wrapped in a plain container (an inset's
//!   `col([plot])`) keeps a usable size.

#![cfg(feature = "script")]

use figs_core::layout::{ComputedLayout, PaintContent, PathPaint};
use figs_core::schema::NodeKind;
use figs_core::{eval_script, layout, Document, NullMeasurer};

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &NullMeasurer)
}

fn connectors(src: &str) -> usize {
    let raw = eval_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    raw.nodes
        .values()
        .filter(|n| n.kind == NodeKind::Connector)
        .count()
}

fn paths(l: &ComputedLayout) -> Vec<&PathPaint> {
    l.nodes
        .iter()
        .filter_map(|n| match &n.content {
            PaintContent::Path(p) => Some(p),
            _ => None,
        })
        .collect()
}

#[test]
fn helpers_take_arrays_of_items_and_links() {
    let pre = r##"
        let a = rect().width(4).height(3);
        let b = rect().width(4).height(3);
        let z = zoom_link(a, b, #{ region: [0.2, 0.2, 0.5, 0.5] });
    "##;
    // Every list / single-item helper accepts the helper's array.
    for body in [
        r#"block("Zoom", [fill_row([a, b]), z])"#,
        r#"block("Zoom", [fill_row([a, b]), z], #{ widths: [1] })"#,
        r#"grid(2, [a, b, z])"#,
        r#"columns([a, b, z], [1, 2])"#,
        r#"rows([a, b, z], [1, 2])"#,
        r#"vbox([a, [b, z]])"#,
        r#"fill_row([a, b, z])"#,
        r#"scheme([a, arrow_step("x"), b, z])"#,
        r#"col(panels([a, b, z], ["one", "two"]))"#,
        r#"section("S", [row([a, b]), z])"#,
        r#"frame([row([a, b]), z])"#,
        r#"labeled([row([a, b]), z], "(a)")"#,
        r#"captioned([row([a, b]), z], "cap")"#,
        r#"inset([a, z], b)"#,
        r#"overlay(row([a, b]), #{ top: ["t", z] })"#,
        r#"side([row([a, b]), z], "note")"#,
        r#"behind(row([a, b]), [rect(), z])"#,
    ] {
        let src = format!("{pre} page(30, 30, col([{body}])).unit(\"pt\")");
        assert_eq!(connectors(&src), 10, "{body}");
        lay(&src);
    }
    // The connectors don't count as cells: one width for one item.
    let e = eval_script(&format!(
        "{pre} page(30, 30, block(\"Z\", [a, z], #{{ widths: [1, 2] }}))"
    ))
    .expect_err("2 widths for 1 item")
    .to_string();
    assert!(e.contains("1 items but 2 sizes"), "{e}");
}

/// The extent of a path's points along x and y.
fn extent(p: &PathPaint) -> (f32, f32) {
    let pts: Vec<figs_core::geom::Point> =
        p.segments
            .iter()
            .filter_map(|s| match s {
                figs_core::geom::PathSegment::MoveTo(q)
                | figs_core::geom::PathSegment::LineTo(q) => Some(*q),
                _ => None,
            })
            .collect();
    let span = |f: fn(&figs_core::geom::Point) -> f32| {
        let v: Vec<f32> = pts.iter().map(f).collect();
        v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
    };
    (span(|q| q.x), span(|q| q.y))
}

#[test]
fn a_fill_arrow_in_an_unstretched_scheme_keeps_a_length() {
    // In a centred column the scheme (a row with a flex arrow) takes the
    // width it may, and the arrow fills what the items leave.
    let l = lay(r##"
        page(100, "auto", col([
            scheme([rect().width(10).height(10), arrow_step(#{ length: "fill" }),
                    rect().width(10).height(10)], #{ gap: 0 })
        ]).cross("center")).unit("pt")"##);
    let p = paths(&l);
    assert_eq!(p.len(), 1);
    let (w, _) = extent(p[0]);
    assert!(w > 70.0 && w <= 80.0, "{w}");
    // Down an auto-height page there is no height to share: the arrow is
    // `scheme_fill_min_length()` long instead of vanishing.
    let l = lay(r##"
        fn scheme_fill_min_length() { 12 }
        page(100, "auto", col([
            scheme_v([rect().width(10).height(10), arrow_step(#{ length: "fill" }),
                      rect().width(10).height(10)])
        ])).unit("pt")"##);
    let p = paths(&l);
    assert_eq!(p.len(), 1);
    let (_, h) = extent(p[0]);
    assert!((h - 12.0).abs() < 1e-3, "{h}");
}

#[test]
fn a_collapsed_viewbox_shape_draws_nothing() {
    // An arrow in a viewbox over a box with no width: no degenerate path.
    let l = lay(r##"
        page(100, 50, col([arrow(0, 0.5, 1, 0.5).viewbox(0, 0, 1, 1).height(4)]).cross("start"))
            .unit("pt")"##);
    assert!(paths(&l).is_empty());
    // A vertical one in the same box is fine (it has height).
    let l = lay(r##"
        page(100, 50, col([arrow(0.5, 0, 0.5, 1).viewbox(0, 0, 1, 1).height(4)]).cross("start"))
            .unit("pt")"##);
    assert_eq!(paths(&l).len(), 1);
}

#[test]
fn an_aspect_panel_keeps_its_size_inside_a_plain_wrapper() {
    // The stand-in "plot": a column with an aspect ratio and no content
    // size of its own, wrapped in a plain col, as an inset 0.5 of the width.
    let l = lay(r##"
        fn inset_pad() { 0 }
        let p = col([rect()]).aspect(2.0).id("p");
        page(100, "auto", col([
            inset(rect().aspect(1.0), col([p]), #{ size: 0.5 })
        ]).stretch()).unit("pt")"##);
    let p = l.nodes.iter().find(|n| n.id == "p").unwrap().rect;
    assert!(
        (p.w - 50.0).abs() < 1e-3 && (p.h - 25.0).abs() < 1e-3,
        "{p:?}"
    );
    // The page is the main panel's height: the inset didn't grow it.
    assert!(
        (l.page.height_pt - 100.0).abs() < 1e-3,
        "{}",
        l.page.height_pt
    );
    // A real plot the same way is not squashed.
    let l = lay(r##"
        let p = plot(#{}, [line_plot([1, 2, 3], [1, 4, 9])]).id("p");
        page(100, "auto", col([inset(rect().aspect(1.0), col([p]))]).stretch()).unit("pt")"##);
    let p = l.nodes.iter().find(|n| n.id == "p").unwrap().rect;
    assert!(p.w > 30.0 && p.h > 20.0, "{p:?}");
}
