//! Curved path segments (`segments` with quad / cubic / arc): TOML round-trip,
//! resolve (arc → cubic, validation), layout bounds, PNG rasterization and the
//! Rhai builders + prelude curve sugar.

use std::collections::BTreeMap;

use figs_core::geom::{ArcSweep, ArrowHead, PathSegment, Point};
use figs_core::layout::{ComputedLayout, PaintContent, PathPaint};
use figs_core::schema::parse::{parse_str, RawDocument, RawNode, RawPage};
use figs_core::schema::{ArcRadius, NodeType, PathProps, RawSegment};
use figs_core::{layout, render_png, Document, NullMeasurer, RenderOptions};
use serde::Serialize;
use tiny_skia::Pixmap;

fn p(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

fn doc(nodes: &str) -> String {
    format!("[page]\nwidth = 100\nheight = 100\nunit = \"pt\"\nroot = \"s\"\n{nodes}")
}

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_toml(src).expect("document should resolve");
    layout(&d, &NullMeasurer)
}

fn node<'a>(l: &'a ComputedLayout, id: &str) -> &'a figs_core::layout::ComputedNode {
    l.nodes.iter().find(|n| n.id == id).expect("node")
}

fn path_of<'a>(l: &'a ComputedLayout, id: &str) -> &'a PathPaint {
    match &node(l, id).content {
        PaintContent::Path(p) => p,
        other => panic!("`{id}` is not a path: {other:?}"),
    }
}

fn props<'a>(d: &'a Document, id: &str) -> &'a PathProps {
    match &d.nodes.iter().find(|n| n.id == id).expect("node").kind {
        NodeType::Path(p) => p,
        _ => panic!("`{id}` is not a path"),
    }
}

#[track_caller]
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.01, "{a} != {b}");
}

// ---- TOML round-trip -------------------------------------------------------

#[derive(Serialize)]
struct DocView<'a> {
    page: &'a RawPage,
    nodes: BTreeMap<&'a String, &'a RawNode>,
}

fn to_toml(d: &RawDocument) -> String {
    let view = DocView {
        page: &d.page,
        nodes: d.nodes.iter().collect(),
    };
    toml::to_string_pretty(&view).expect("serialize")
}

const CURVES: &str = r##"
[page]
width = 10
height = 10
unit = "cm"
root = "s"
[nodes.s]
type = "stack"
children = ["p", "a"]
[nodes.p]
type = "path"
points = [[0, 0], [1, 0]]
segments = [
  { to = [2, 0] },
  { quad = [3, -1], to = [4, 0] },
  { cubic = [[5, 1], [6, -1]], to = [7, 0] },
  { arc = 1.5, to = [9, 0] },
  { arc = [2, 1], rotation = 30, large = true, sweep = "ccw", to = [9, 3] },
]
closed = true
fill = "#6aa9e0"
[nodes.a]
type = "arrow"
points = [[0, 5]]
segments = [{ quad = [2, 3], to = [4, 5] }]
"##;

#[test]
fn segments_round_trip() {
    let raw1 = parse_str(CURVES).expect("parse 1");
    let segs = &raw1.nodes["p"].segments;
    assert_eq!(segs.len(), 5);
    assert_eq!(segs[0], RawSegment::line([2.0, 0.0]));
    assert_eq!(segs[1].quad, Some([3.0, -1.0]));
    assert_eq!(segs[2].cubic, Some([[5.0, 1.0], [6.0, -1.0]]));
    assert_eq!(segs[3].arc, Some(ArcRadius::Circle(1.5)));
    assert_eq!(segs[4].arc, Some(ArcRadius::Ellipse([2.0, 1.0])));
    assert_eq!(
        (segs[4].rotation, segs[4].large, segs[4].sweep),
        (Some(30.0), true, Some(ArcSweep::Ccw))
    );

    let out = to_toml(&raw1);
    // Defaults stay out of the saved file.
    assert_eq!(out.matches("large").count(), 1, "{out}");
    assert_eq!(out.matches("sweep").count(), 1, "{out}");
    let raw2 = parse_str(&out).unwrap_or_else(|e| panic!("parse 2: {e}\n{out}"));
    for id in ["p", "a"] {
        assert_eq!(raw1.nodes[id].points, raw2.nodes[id].points, "{id}");
        assert_eq!(raw1.nodes[id].segments, raw2.nodes[id].segments, "{id}");
    }
    // And the re-serialization is stable.
    assert_eq!(out, to_toml(&raw2));
}

// ---- resolve ---------------------------------------------------------------

#[test]
fn resolve_builds_segments_and_converts_arcs() {
    let d = Document::from_toml(CURVES).unwrap();
    let s = &props(&d, "p").segments;
    let cm = 72.0 / 2.54;
    assert_eq!(s[0], PathSegment::MoveTo(p(0.0, 0.0)));
    assert_eq!(s[1], PathSegment::LineTo(p(cm, 0.0)));
    assert_eq!(s[2], PathSegment::LineTo(p(2.0 * cm, 0.0)));
    assert!(matches!(s[3], PathSegment::QuadTo(..)));
    assert!(matches!(s[4], PathSegment::CubicTo(..)));
    // r = 1.5 from x=7 to x=9: a 2-cm chord, < 180° -> one cubic, ending exactly.
    assert!(matches!(s[5], PathSegment::CubicTo(_, _, e) if e == p(9.0 * cm, 0.0)));
    // The large ccw elliptical arc needs several cubics; only cubics remain.
    let tail = &s[6..s.len() - 1];
    assert!(tail.len() >= 3, "{tail:?}");
    assert!(tail.iter().all(|x| matches!(x, PathSegment::CubicTo(..))));
    assert!(
        matches!(tail.last(), Some(PathSegment::CubicTo(_, _, e)) if *e == p(9.0 * cm, 3.0 * cm))
    );
    assert_eq!(s.last(), Some(&PathSegment::Close));

    // A one-segment arrow: MoveTo + QuadTo, head at the end.
    let a = props(&d, "a");
    assert_eq!(a.segments.len(), 2);
    assert_eq!(a.head, ArrowHead::End);
}

fn resolve_err(body: &str) -> String {
    let src = format!("[page]\nwidth = 1\nheight = 1\nroot = \"n\"\n[nodes.n]\n{body}");
    match Document::from_toml(&src) {
        Ok(_) => panic!("expected an error for:\n{body}"),
        Err(e) => e.to_string(),
    }
}

fn parse_err(body: &str) -> String {
    let src = format!("[page]\nwidth = 1\nheight = 1\nroot = \"n\"\n[nodes.n]\n{body}");
    parse_str(&src).unwrap_err().0.to_string()
}

#[test]
fn resolve_rejects_bad_segments() {
    let e = resolve_err("type = \"path\"\nsegments = [{ to = [1, 1] }]");
    assert!(e.contains("path requires field `points`"), "{e}");
    let e = resolve_err(
        "type = \"path\"\npoints = [[0, 0]]\nsegments = [{ quad = [1, 1], cubic = [[0, 0], [1, 1]], to = [2, 2] }]",
    );
    assert!(e.contains("segment 1: give at most one of"), "{e}");
    let e = resolve_err("type = \"path\"\npoints = [[0, 0]]\nsegments = [{ to = [1, 1] }, { large = true, to = [2, 2] }]");
    assert!(
        e.contains("segment 2: `large`, `sweep` and `rotation` only apply to an `arc`"),
        "{e}"
    );
    let e =
        resolve_err("type = \"path\"\npoints = [[0, 0]]\nsegments = [{ arc = 0, to = [2, 2] }]");
    assert!(e.contains("arc radius must be a positive number"), "{e}");
    let e = resolve_err(
        "type = \"path\"\npoints = [[0, 0]]\nsegments = [{ arc = [1, -1], to = [2, 2] }]",
    );
    assert!(e.contains("arc radius must be a positive number"), "{e}");
    let e =
        resolve_err("type = \"arrow\"\npoints = [[0, 0], [1, 1]]\nsegments = [{ to = [2, 2] }]");
    assert!(e.contains("arrow is a single segment"), "{e}");
    let e = resolve_err(
        "type = \"line\"\npoints = [[0, 0]]\nsegments = [{ to = [1, 1] }, { to = [2, 2] }]",
    );
    assert!(e.contains("line is a single segment"), "{e}");
    // Typos inside a segment are parse errors (deny_unknown_fields).
    let e = parse_err(
        "type = \"path\"\npoints = [[0, 0]]\nsegments = [{ quadratic = [1, 1], to = [2, 2] }]",
    );
    assert!(e.contains("unknown field `quadratic`"), "{e}");
    let e = parse_err("type = \"path\"\npoints = [[0, 0]]\nsegments = [{ arc = 1, sweep = \"left\", to = [2, 2] }]");
    assert!(e.contains("unknown variant `left`"), "{e}");
    let e = parse_err("type = \"path\"\npoints = [[0, 0]]\nsegments = [{ quad = [1, 1] }]");
    assert!(e.contains("missing field `to`"), "{e}");
}

// ---- layout ----------------------------------------------------------------

#[test]
fn intrinsic_size_uses_exact_curve_bounds() {
    // The quad's control point is at y = 40, but the curve only reaches 20; the
    // cubic's controls reach x = 80 while the curve peaks at 0.75 * 80 = 60.
    let l = lay(&doc(r#"
        [nodes.s]
        type = "column"
        children = ["q", "c", "arc"]
        [nodes.q]
        type = "path"
        points = [[0, 0]]
        segments = [{ quad = [20, 40], to = [40, 0] }]
        [nodes.c]
        type = "path"
        points = [[0, 0]]
        segments = [{ cubic = [[80, 0], [80, 10]], to = [0, 10] }]
        [nodes.arc]
        type = "path"
        points = [[0, 0]]
        segments = [{ arc = 1, sweep = "ccw", to = [20, 0] }]
        "#));
    let q = node(&l, "q").rect;
    near(q.w, 40.0);
    near(q.h, 20.0);
    let c = node(&l, "c").rect;
    near(c.w, 60.0);
    near(c.h, 10.0);
    // r = 1 is scaled up to span the 20-pt chord: a semicircle, 10 deep,
    // bulging down (ccw on the page).
    let a = node(&l, "arc").rect;
    near(a.w, 20.0);
    near(a.h, 10.0);
    // Paint segments are absolute: the arc box starts at y = 30.
    let pa = path_of(&l, "arc");
    assert_eq!(pa.segments[0], PathSegment::MoveTo(p(0.0, 30.0)));
}

// ---- render ----------------------------------------------------------------

fn render(nodes: &str) -> Pixmap {
    let src = format!(
        "[page]\nwidth = 100\nheight = 100\nunit = \"pt\"\ndpi = 72\nbackground = \"#ffffff\"\nroot = \"s\"\n{nodes}"
    );
    let d = Document::from_toml(&src).expect("resolve");
    let bytes = render_png(&layout(&d, &NullMeasurer), &RenderOptions::default()).unwrap();
    Pixmap::decode_png(&bytes).unwrap()
}

fn is_red(px: &Pixmap, x: u32, y: u32) -> bool {
    let c = px.pixel(x, y).expect("in bounds");
    c.red() > 200 && c.green() < 80 && c.blue() < 80
}

fn is_white(px: &Pixmap, x: u32, y: u32) -> bool {
    let c = px.pixel(x, y).expect("in bounds");
    c.red() > 245 && c.green() > 245 && c.blue() > 245
}

#[test]
fn quad_curve_is_stroked_along_the_curve() {
    // B(0.5) of (10,80) -> c (50,0) -> (90,80) is (50, 40).
    let px = render(
        r##"
        [nodes.s]
        type = "path"
        points = [[10, 80]]
        segments = [{ quad = [50, 0], to = [90, 80] }]
        stroke = "#ff0000"
        stroke_width = 3
        "##,
    );
    assert!(is_red(&px, 50, 40));
    assert!(is_white(&px, 50, 10), "control point is not drawn");
    assert!(is_white(&px, 50, 79), "the curve is not the chord");
    // B(0.25) = (30, 50).
    assert!(is_red(&px, 30, 50));
}

#[test]
fn arc_is_stroked_on_its_circle() {
    // Clockwise semicircle from (20,60) to (80,60): center (50,60), r = 30, top at (50,30).
    let px = render(
        r##"
        [nodes.s]
        type = "path"
        points = [[20, 60]]
        segments = [{ arc = 30, to = [80, 60] }]
        stroke = "#ff0000"
        stroke_width = 3
        "##,
    );
    assert!(is_red(&px, 50, 30));
    let d = 30.0 / 2f32.sqrt();
    assert!(is_red(&px, (50.0 - d) as u32, (60.0 - d) as u32));
    assert!(is_white(&px, 50, 90), "the lower half is not drawn");
    assert!(is_white(&px, 50, 60));
}

#[test]
fn curved_arrow_head_follows_the_end_tangent() {
    // The curve arrives at (80, 80) heading straight down (control (80, 20)),
    // so the head's base is horizontal above the tip: wide in x, not in y.
    let px = render(
        r##"
        [nodes.s]
        type = "arrow"
        points = [[20, 20]]
        segments = [{ quad = [80, 20], to = [80, 80] }]
        stroke = "#ff0000"
        stroke_width = 2
        head_size = 20
        "##,
    );
    // 10 above the tip the head is 8 wide (the 2-pt shaft only 2): (82, 70)
    // and (77, 70) are inside it, (88, 70) outside.
    assert!(is_red(&px, 82, 70));
    assert!(is_red(&px, 77, 70));
    assert!(is_white(&px, 88, 70));
    // Nothing beyond the tip, and no head sideways (as a chord-aligned head
    // pointing down-right would paint).
    assert!(is_white(&px, 80, 86));
    assert!(is_white(&px, 72, 78));
    // The shaft is still drawn along the curve: B(0.5) = (65, 35).
    assert!(is_red(&px, 65, 35));
}

#[test]
fn dashed_cubic_has_gaps() {
    // A cubic that is a straight horizontal run (collinear controls), dashed 10/10.
    let px = render(
        r##"
        [nodes.s]
        type = "path"
        points = [[10, 50]]
        segments = [{ cubic = [[30, 50], [60, 50]], to = [90, 50] }]
        stroke = "#ff0000"
        stroke_width = 4
        dash = [10, 10]
        "##,
    );
    assert!(is_red(&px, 15, 50));
    assert!(is_white(&px, 25, 50));
    assert!(is_red(&px, 35, 50));
}

#[test]
fn filled_closed_curve() {
    // A closed shape: straight base, arc on top (a half disc).
    let px = render(
        r##"
        [nodes.s]
        type = "path"
        points = [[20, 60], [80, 60]]
        segments = [{ arc = 30, sweep = "ccw", to = [20, 60] }]
        closed = true
        fill = "#ff0000"
        "##,
    );
    assert!(is_red(&px, 50, 45));
    assert!(is_white(&px, 50, 70));
    assert!(is_white(&px, 22, 34));
}

// ---- script ----------------------------------------------------------------

#[cfg(feature = "script")]
mod script {
    use super::*;
    use figs_core::eval_script;
    use figs_core::schema::NodeKind;

    fn kids(raw: &RawDocument) -> Vec<&RawNode> {
        raw.nodes["root"]
            .children
            .iter()
            .map(|c| &raw.nodes[c])
            .collect()
    }

    #[test]
    fn curve_builders_lower_to_segments() {
        let raw = eval_script(
            r#"
            page(100, 100, stack([
                path([[0, 0]]).to(10, 0).quad(15, -5, 20, 0)
                    .cubic(25, 5, 30, -5, 35.5, 0)
                    .arc(5, 45, 0)
                    .arc([4, 2], 45, 10, #{ sweep: "ccw", large: true, rotation: 15 })
                    .closed(),
                arrow(0, 0, 1, 1),
            ])).unit("pt")
            "#,
        )
        .unwrap();
        let k = kids(&raw);
        let s = &k[0].segments;
        assert_eq!(k[0].points, vec![[0.0, 0.0]]);
        assert_eq!(s.len(), 5);
        assert_eq!(s[0], RawSegment::line([10.0, 0.0]));
        assert_eq!((s[1].quad, s[1].to), (Some([15.0, -5.0]), [20.0, 0.0]));
        assert_eq!(s[2].cubic, Some([[25.0, 5.0], [30.0, -5.0]]));
        assert_eq!(s[2].to, [35.5, 0.0]);
        assert_eq!(
            (s[3].arc, s[3].sweep, s[3].large),
            (Some(ArcRadius::Circle(5.0)), None, false)
        );
        assert_eq!(s[4].arc, Some(ArcRadius::Ellipse([4.0, 2.0])));
        assert_eq!(
            (s[4].sweep, s[4].large, s[4].rotation),
            (Some(ArcSweep::Ccw), true, Some(15.0))
        );
        assert!(k[0].closed);
        assert!(k[1].segments.is_empty());
        // Lowered documents resolve.
        figs_core::schema::resolve::resolve(raw).expect("resolves");
    }

    #[test]
    fn curve_builder_errors() {
        let e = eval_script(r#"page(1, 1, rect().quad(0, 0, 1, 1))"#).unwrap_err();
        assert!(e.to_string().contains("continues a path/line/arrow"), "{e}");
        let e = eval_script(r#"page(1, 1, path([[0, 0]]).cubic(0, 0, 1, "x", 1, 1))"#).unwrap_err();
        assert!(e.to_string().contains("`cubic` expects numbers"), "{e}");
        let e = eval_script(r#"page(1, 1, path([[0, 0]]).arc(1, 1, 1, #{ sweep: "up" }))"#)
            .unwrap_err();
        assert!(e.to_string().contains("invalid sweep"), "{e}");
        let e =
            eval_script(r#"page(1, 1, path([[0, 0]]).arc(1, 1, 1, #{ big: true }))"#).unwrap_err();
        assert!(e.to_string().contains("unknown option `big`"), "{e}");
        let e = eval_script(r#"page(1, 1, path([[0, 0]]).arc([1], 1, 1))"#).unwrap_err();
        assert!(
            e.to_string()
                .contains("radius must be a number or [rx, ry]"),
            "{e}"
        );
    }

    #[test]
    fn prelude_curved_arrow_bends_left_of_travel() {
        // Left-to-right with bend 10: control 20 above the chord, apex 10 above.
        let raw =
            eval_script(r#"page(100, 100, stack([curved_arrow(10, 50, 90, 50, 10)])).unit("pt")"#)
                .unwrap();
        let n = kids(&raw)[0];
        assert_eq!(n.kind, NodeKind::Path);
        assert_eq!(n.head, Some(ArrowHead::End));
        assert_eq!(n.points, vec![[10.0, 50.0]]);
        let q = n.segments[0].quad.unwrap();
        near(q[0], 50.0);
        near(q[1], 30.0);
        let d = Document::from_script(
            r#"page(100, 100, stack([curve(10, 50, 90, 50, -10)])).unit("pt")"#,
        )
        .unwrap();
        let l = layout(&d, &NullMeasurer);
        let pp = l.nodes.iter().find_map(|n| match &n.content {
            PaintContent::Path(p) => Some(p),
            _ => None,
        });
        // Negative bend goes below: bounds reach y = 60.
        let (_, hi) = figs_core::path::bounds(&pp.unwrap().segments).unwrap();
        near(hi.y, 60.0);
    }

    #[test]
    fn prelude_brace_and_bracket() {
        let src = r#"page(100, 100, stack([
            brace(10, 50, 90, 50, 8),
            bracket(10, 80, 90, 80, -6),
        ])).unit("pt")"#;
        let d = Document::from_script(src).unwrap();
        let l = layout(&d, &NullMeasurer);
        let paths: Vec<_> = l
            .nodes
            .iter()
            .filter_map(|n| match &n.content {
                PaintContent::Path(p) => Some(p),
                _ => None,
            })
            .collect();
        // Brace: from (10,50) to (90,50), tip 8 above at x = 50.
        let (lo, hi) = figs_core::path::bounds(&paths[0].segments).unwrap();
        near(lo.x, 10.0);
        near(hi.x, 90.0);
        near(lo.y, 42.0);
        near(hi.y, 50.0);
        assert_eq!(
            paths[0].segments.first(),
            Some(&PathSegment::MoveTo(p(10.0, 50.0)))
        );
        assert!(
            matches!(paths[0].segments.last(), Some(PathSegment::CubicTo(_, _, e)) if *e == p(90.0, 50.0))
        );
        // Bracket: negative depth goes below the chord.
        let (lo, hi) = figs_core::path::bounds(&paths[1].segments).unwrap();
        near(lo.y, 80.0);
        near(hi.y, 86.0);
        render_png(&l, &RenderOptions::default()).expect("renders");
    }

    #[test]
    fn curves_figs_example() {
        let src = include_str!("../../../examples/curves.figs");
        let d = Document::from_script(src).unwrap_or_else(|e| panic!("example failed: {e}"));
        let l = layout(&d, &NullMeasurer);
        let curved = l.nodes.iter().any(|n| match &n.content {
            PaintContent::Path(p) => p
                .segments
                .iter()
                .any(|s| matches!(s, PathSegment::CubicTo(..) | PathSegment::QuadTo(..))),
            _ => false,
        });
        assert!(curved);
        render_png(&l, &RenderOptions::default()).expect("renders");
    }
}
