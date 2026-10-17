//! Framework helpers on core connectors: `zoom_link`, `callout_to`,
//! `flow_link`, `block_link` / `behind(.., block_arrow_bent(..))`,
//! `plot_shift_arrows`, and the small items `pile`, `backdrop`,
//! `labeled(.., #{ position })`.

#![cfg(feature = "script")]

use figs_core::geom::{ArrowHead, PathSegment, Point, Rect};
use figs_core::layout::{ComputedLayout, PaintContent, PathPaint};
use figs_core::schema::NodeKind;
use figs_core::{eval_script, layout, Document, NullMeasurer};

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &NullMeasurer)
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

fn pt(s: &PathSegment) -> Point {
    match *s {
        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => p,
        PathSegment::QuadTo(_, p) | PathSegment::CubicTo(_, _, p) => p,
        PathSegment::Close => panic!("close"),
    }
}

fn ends(p: &PathPaint) -> (Point, Point) {
    (pt(&p.segments[0]), pt(p.segments.last().unwrap()))
}

fn near(p: Point, x: f32, y: f32) -> bool {
    (p.x - x).abs() < 0.02 && (p.y - y).abs() < 0.02
}

fn sized(l: &ComputedLayout, w: f32, h: f32) -> Rect {
    l.nodes
        .iter()
        .map(|n| n.rect)
        .find(|r| (r.w - w).abs() < 1e-3 && (r.h - h).abs() < 1e-3)
        .unwrap_or_else(|| panic!("no {w} x {h} box"))
}

fn connectors(src: &str) -> usize {
    let raw = eval_script(src).unwrap();
    raw.nodes
        .values()
        .filter(|n| n.kind == NodeKind::Connector)
        .count()
}

#[test]
fn zoom_link_outlines_the_region_and_joins_the_copy() {
    // An "image" 40 x 20 and its copy 10 x 10 to the right; region
    // [0.25, 0.25, 0.5, 0.75] = (10, 5)..(20, 15).
    let src = r##"
        let img = rect().width(40).height(20);
        let copy = rect().width(10).height(10);
        page(100, 40, row([img, copy, zoom_link(img, copy, #{ region: [0.25, 0.25, 0.5, 0.75] })])
            .spacing(20).cross("start")).unit("pt")"##;
    // 4 sides of the region, 4 of the copy's frame, 2 lines.
    assert_eq!(connectors(src), 10);
    let l = lay(src);
    let p = paths(&l);
    // The region outline: top side from (10, 5) to (20, 5).
    assert!(p.iter().any(|q| {
        let (s, e) = ends(q);
        near(s, 10.0, 5.0) && near(e, 20.0, 5.0)
    }));
    // The lines: the region's right corners to the copy's left ones, dashed.
    let lines: Vec<&&PathPaint> = p
        .iter()
        .filter(|q| !q.stroke.as_ref().unwrap().dash.is_empty())
        .collect();
    assert_eq!(lines.len(), 2);
    let (s, e) = ends(lines[0]);
    assert!(near(s, 20.0, 5.0) && near(e, 60.0, 0.0), "{s:?} {e:?}");
    let (s, e) = ends(lines[1]);
    assert!(near(s, 20.0, 15.0) && near(e, 60.0, 10.0), "{s:?} {e:?}");
    // The copy took no extra room: it follows the image by the spacing.
    assert!((sized(&l, 10.0, 10.0).x - 60.0).abs() < 1e-3);
    let e = eval_script(
        r#"let a = rect(); page(10, 10, col([a, zoom_link(a, a, #{ region: [0.5, 0, 0.2, 1] })]))"#,
    )
    .expect_err("bad region")
    .to_string();
    assert!(e.contains("fx0 < fx1"), "{e}");
}

#[test]
fn callout_to_leads_from_the_label_to_the_point() {
    let l = lay(r##"
        let img = rect().width(60).height(40);
        page(100, 60, col([callout_to(img, "x", #{ at: [0.5, 0.75], position: "top-left", gap: 0 })]))
            .unit("pt")"##);
    let p = paths(&l);
    assert_eq!(p.len(), 1);
    let (_, e) = ends(p[0]);
    assert!(near(e, 30.0, 30.0), "{e:?}");
    // Beside: the label to the right of the item, in a row.
    let l = lay(r##"
        let img = rect().width(60).height(40);
        page(200, 60, col([callout_to(img, "label", #{ at: [1, 0.5], position: "right" })])
            .cross("start")).unit("pt")"##);
    let (s, e) = ends(paths(&l)[0]);
    assert!(near(e, 60.0, 20.0), "{e:?}");
    assert!(s.x > 60.0, "{s:?}");
    let e = eval_script(r#"page(10, 10, callout_to(rect(), "x", #{ position: "middle" }))"#)
        .expect_err("bad position")
        .to_string();
    assert!(
        e.contains("unknown position \"middle\"") && e.contains("below"),
        "{e}"
    );
}

#[test]
fn flow_link_is_an_elbow_with_a_head() {
    let l = lay(r##"
        let a = rect().width(20).height(10);
        let b = rect().width(20).height(10);
        page(100, 100, col([
            row([a]), row([rect().width(50), b]),
            flow_link(a, b),
        ]).spacing(20)).unit("pt")"##);
    let p = paths(&l)[0];
    assert_eq!(p.head, ArrowHead::End);
    // a is above-left of b, apart both ways; the wider gap (50 - 20 = 30
    // across, 20 down) is across: right side -> left side, H-V-H.
    let (s, e) = ends(p);
    assert!(near(s, 20.0, 5.0) && near(e, 50.0, 35.0), "{s:?} {e:?}");
    assert!(p.segments.len() > 2);
}

#[test]
fn block_arrow_bent_is_a_band_behind_the_group() {
    let l = lay(r##"
        let g = rect().width(40).height(20).fill("#eeeeee");
        page(100, 100, col([behind(g, block_arrow_bent(#{ width: 4 }))]).cross("start")).unit("pt")"##);
    // The band (a closed, filled path at block_link_opacity) paints before
    // the group's rect.
    let i_path = l
        .nodes
        .iter()
        .position(|n| matches!(n.content, PaintContent::Path(_)))
        .unwrap();
    let i_rect = l
        .nodes
        .iter()
        .position(|n| matches!(n.content, PaintContent::Rect { .. }))
        .unwrap();
    assert!(i_path < i_rect);
    let PaintContent::Path(p) = &l.nodes[i_path].content else {
        unreachable!()
    };
    assert!(p.is_closed() && p.fill.is_some());
    assert!(l.nodes[i_path].effects.as_ref().unwrap().opacity < 1.0);
    // From the left middle (0, 10) along y = 10 +- 2.
    assert!(p
        .segments
        .iter()
        .filter(|s| !matches!(s, PathSegment::Close))
        .any(|s| near(pt(s), 0.0, 8.0)));
}

#[test]
fn plot_shift_arrows_interpolate_both_series() {
    let raw = eval_script(
        r##"
        let a = line_plot([0, 10], [0, 10]);
        let b = line_plot([0, 10], [10, 30]);
        let arrows = plot_shift_arrows(a, b, [5], #{ trim: 0 });
        let ok = arrows.len() == 1 && arrows[0].pts == [5.0, 5.0, 5.0, 20.0];
        if !ok { throw `got ${arrows}`; }
        page(10, 10, plot(#{}, [a, b, arrows]))"##,
    );
    assert!(raw.is_ok(), "{:?}", raw.err().map(|e| e.to_string()));
    let e = eval_script(
        r#"let a = line_plot([0, 10], [0, 1]);
           page(10, 10, plot(#{}, [a, plot_shift_arrows(a, a, [20])]))"#,
    )
    .expect_err("outside")
    .to_string();
    assert!(e.contains("outside a series' x range"), "{e}");
}

#[test]
fn pile_stacks_copies_behind_the_front_one() {
    let l = lay(r##"
        let d = rect().width(10).height(4).fill("#88aacc");
        page(100, 100, col([pile(d, 3, #{ offset: [2, -1] })]).cross("start")).unit("pt")"##);
    let rects: Vec<Rect> = l
        .nodes
        .iter()
        .filter(|n| matches!(n.content, PaintContent::Rect { .. }))
        .map(|n| n.rect)
        .collect();
    assert_eq!(rects.len(), 3);
    // Back first, up and to the right; the front copy last at the bottom-left.
    assert!((rects[0].x - 4.0).abs() < 1e-3 && (rects[0].y - 0.0).abs() < 1e-3);
    assert!((rects[2].x - 0.0).abs() < 1e-3 && (rects[2].y - 2.0).abs() < 1e-3);
}

#[test]
fn backdrop_and_outside_labels() {
    let l = lay(r##"
        page(100, 100, col([backdrop(rect().width(10).height(10), #{ pad: 2 })]).cross("start")).unit("pt")"##);
    let frame = l
        .nodes
        .iter()
        .find(|n| matches!(n.content, PaintContent::Frame(_)))
        .expect("a filled panel");
    assert!((frame.rect.w - 14.0).abs() < 1e-3);
    // outside-left: the label column before the item in a row.
    let raw = eval_script(
        r#"page(10, 10, labeled(rect().width(2).height(2), "(a)", #{ position: "outside-left" }))"#,
    )
    .unwrap();
    assert_eq!(raw.nodes["root"].kind, NodeKind::Row);
    let raw = eval_script(
        r#"page(10, 10, labeled(rect().width(2).height(2), "(a)", #{ position: "outside-top" }))"#,
    )
    .unwrap();
    assert_eq!(raw.nodes["root"].kind, NodeKind::Column);
    let e = eval_script(r#"page(10, 10, labeled(rect(), "(a)", #{ position: "under" }))"#)
        .expect_err("bad position")
        .to_string();
    assert!(e.contains("outside-left"), "{e}");
}
