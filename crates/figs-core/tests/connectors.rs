//! Connectors: a `connector` node joins two other nodes after layout — any
//! two nodes on the page, in any containers — at named points or fractions
//! of their boxes, along a straight / elbow / curved route, as a stroke or a
//! wide band. It takes no room in its parent and paints where it sits.

#![cfg(feature = "script")]

use figs_core::geom::{PathSegment, Point, Rect};
use figs_core::layout::{ComputedLayout, PaintContent, PathPaint};
use figs_core::RenderOptions;
use figs_core::{eval_script, layout, render_png, render_svg, Document, NullMeasurer};

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &NullMeasurer)
}

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-2, "expected {b}, got {a}");
}

fn approx_pt(p: Point, x: f32, y: f32) {
    assert!(
        (p.x - x).abs() < 1e-2 && (p.y - y).abs() < 1e-2,
        "expected ({x}, {y}), got ({}, {})",
        p.x,
        p.y
    );
}

/// The rect of the node whose box is `w` x `h` (unique on a test page).
fn sized(l: &ComputedLayout, w: f32, h: f32) -> Rect {
    l.nodes
        .iter()
        .map(|n| n.rect)
        .find(|r| (r.w - w).abs() < 1e-3 && (r.h - h).abs() < 1e-3)
        .unwrap_or_else(|| panic!("no {w} x {h} box"))
}

/// The connectors' paths, in paint order.
fn paths(l: &ComputedLayout) -> Vec<&PathPaint> {
    l.nodes
        .iter()
        .filter_map(|n| match &n.content {
            PaintContent::Path(p) => Some(p),
            _ => None,
        })
        .collect()
}

fn end_point(s: &PathSegment) -> Point {
    match *s {
        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => p,
        PathSegment::QuadTo(_, p) | PathSegment::CubicTo(_, _, p) => p,
        PathSegment::Close => panic!("close has no point"),
    }
}

fn first_last(p: &PathPaint) -> (Point, Point) {
    (
        end_point(&p.segments[0]),
        end_point(p.segments.last().unwrap()),
    )
}

#[test]
fn straight_link_between_rows_attaches_to_facing_sides() {
    // a (20 x 10) in the first row, b (30 x 20) in the second, offset right:
    // b is below a (their x ranges overlap), so the link runs a's bottom ->
    // b's top, through the middle of the overlap.
    let l = lay(r##"
        let a = rect().width(20).height(10);
        let b = rect().width(30).height(20);
        page(100, 100, col([
            row([a]),
            row([rect().width(10), b]),
            connect(a, b),
        ]).spacing(5)).unit("pt")"##);
    let a = sized(&l, 20.0, 10.0);
    let b = sized(&l, 30.0, 20.0);
    // The connector took no room: the second row is spacing below the first.
    approx(b.y, a.y + a.h + 5.0);
    let p = paths(&l);
    assert_eq!(p.len(), 1);
    let (s, e) = first_last(p[0]);
    // Overlap of [0, 20] and [10, 40] is [10, 20] -> x = 15.
    approx_pt(s, 15.0, a.y + a.h);
    approx_pt(e, 15.0, b.y);
}

#[test]
fn named_points_and_fractions_and_ids() {
    let l = lay(r##"
        let a = rect().width(20).height(10).id("a");
        page(100, 60, row([
            a,
            col([rect().width(10).height(30), rect().width(16).height(8).id("b")]),
            connect("a", "b", #{ from: "top-left", to: [0.25, 1] }),
        ]).spacing(10)).unit("pt")"##);
    let a = sized(&l, 20.0, 10.0);
    let b = sized(&l, 16.0, 8.0);
    let (s, e) = first_last(paths(&l)[0]);
    approx_pt(s, a.x, a.y);
    approx_pt(e, b.x + 4.0, b.y + 8.0);
    // The explicit ids are the document ids.
    assert!(l.nodes.iter().any(|n| n.id == "a"));
    assert!(l.nodes.iter().any(|n| n.id == "b"));
}

#[test]
fn elbow_routes_run_horizontally_and_vertically() {
    // right -> left with a vertical offset: H-V-H through the middle.
    let l = lay(r##"
        let a = rect().width(10).height(10);
        let b = rect().width(10).height(10);
        page(100, 100, stack([
            a.at(0, 0), b.at(50, 40),
            connect(a, b, #{ from: "right", to: "left", route: "elbow" }),
        ])).unit("pt")"##);
    let segs = &paths(&l)[0].segments;
    let pts: Vec<Point> = segs.iter().map(end_point).collect();
    assert_eq!(pts.len(), 4, "{pts:?}");
    approx_pt(pts[0], 10.0, 5.0);
    approx_pt(pts[1], 30.0, 5.0);
    approx_pt(pts[2], 30.0, 45.0);
    approx_pt(pts[3], 50.0, 45.0);
    // bottom -> left: one corner below a, level with b's middle.
    let l = lay(r##"
        let a = rect().width(10).height(10);
        let b = rect().width(10).height(10);
        page(100, 100, stack([
            a.at(0, 0), b.at(50, 40),
            connect(a, b, #{ from: "bottom", to: "left", route: "elbow", gap: [2, 3] }),
        ])).unit("pt")"##);
    let pts: Vec<Point> = paths(&l)[0].segments.iter().map(end_point).collect();
    assert_eq!(pts.len(), 3, "{pts:?}");
    approx_pt(pts[0], 5.0, 12.0);
    approx_pt(pts[1], 5.0, 45.0);
    approx_pt(pts[2], 47.0, 45.0);
}

#[test]
fn elbow_corners_round_with_radius() {
    let l = lay(r##"
        let a = rect().width(10).height(10);
        let b = rect().width(10).height(10);
        page(100, 100, stack([
            a.at(0, 0), b.at(50, 40),
            connect(a, b, #{ from: "bottom", to: "left", route: "elbow", radius: 4 }),
        ])).unit("pt")"##);
    let segs = &paths(&l)[0].segments;
    assert!(
        matches!(segs[2], PathSegment::QuadTo(c, _) if (c.x - 5.0).abs() < 1e-3 && (c.y - 45.0).abs() < 1e-3)
    );
    approx_pt(end_point(&segs[1]), 5.0, 41.0);
    approx_pt(end_point(&segs[2]), 9.0, 45.0);
}

#[test]
fn curve_leaves_along_the_normals() {
    let l = lay(r##"
        let a = rect().width(10).height(10);
        let b = rect().width(10).height(10);
        page(100, 100, stack([
            a.at(0, 0), b.at(50, 0),
            connect(a, b, #{ route: "curve", head: "end" }),
        ])).unit("pt")"##);
    let p = paths(&l)[0];
    let PathSegment::CubicTo(c1, c2, e) = p.segments[1] else {
        panic!("{:?}", p.segments)
    };
    approx_pt(end_point(&p.segments[0]), 10.0, 5.0);
    approx_pt(e, 50.0, 5.0);
    assert!(c1.x > 10.0 && (c1.y - 5.0).abs() < 1e-3);
    assert!(c2.x < 50.0 && (c2.y - 5.0).abs() < 1e-3);
    assert_eq!(p.head, figs_core::geom::ArrowHead::End);
}

#[test]
fn band_is_a_closed_filled_outline_with_a_head() {
    let l = lay(r##"
        let a = rect().width(10).height(10);
        let b = rect().width(10).height(10);
        page(100, 100, stack([
            a.at(0, 0), b.at(50, 0),
            connect(a, b, #{ band: 4, head: "end", fill: "#3366cc80" }),
        ])).unit("pt")"##);
    let p = paths(&l)[0];
    assert!(p.is_closed());
    assert!(p.fill.is_some());
    assert!(p.stroke.is_none());
    // Default head: 1 x band long (4), 2 x band wide (8): its corners at
    // x = 46, y = 5 +- 4; the tip at (50, 5).
    let pts: Vec<Point> = p
        .segments
        .iter()
        .filter(|s| !matches!(s, PathSegment::Close))
        .map(end_point)
        .collect();
    assert!(pts
        .iter()
        .any(|q| (q.x - 50.0).abs() < 1e-3 && (q.y - 5.0).abs() < 1e-3));
    assert!(pts
        .iter()
        .any(|q| (q.x - 46.0).abs() < 1e-3 && (q.y - 1.0).abs() < 1e-3));
    assert!(pts
        .iter()
        .any(|q| (q.x - 46.0).abs() < 1e-3 && (q.y - 9.0).abs() < 1e-3));
    // The shaft is 4 wide.
    assert!(pts
        .iter()
        .any(|q| (q.x - 10.0).abs() < 1e-3 && (q.y - 3.0).abs() < 1e-3));
}

#[test]
fn self_link_bends_around_a_group_in_paint_order() {
    // A band from a group's left middle to its bottom middle, *behind* it:
    // the connector comes first in the stack, so it paints first.
    let l = lay(r##"
        let g = rect().width(40).height(20).fill("#eeeeee");
        page(100, 100, stack([
            connect(g, g, #{ from: "left", to: "bottom", route: "elbow", band: 2 }),
            g.at(10, 10),
        ])).unit("pt")"##);
    let order: Vec<&str> = l
        .nodes
        .iter()
        .map(|n| match n.content {
            PaintContent::Path(_) => "path",
            PaintContent::Rect { .. } => "rect",
            _ => "other",
        })
        .collect();
    assert_eq!(order, ["other", "path", "rect"]);
}

#[test]
fn rotated_end_uses_its_transformed_box() {
    // A 20 x 10 rect turned a quarter turn in a row: its local [1, 0.5]
    // (right middle) is the bottom middle of its footprint.
    let l = lay(r##"
        let a = rect().width(20).height(10).rotate(90);
        let b = rect().width(4).height(4);
        page(100, 100, row([a, b, connect(a, b, #{ from: "right", to: "center" })]).cross("start")).unit("pt")"##);
    let (s, _) = first_last(paths(&l)[0]);
    // Footprint 10 wide, 20 tall at the origin: bottom middle = (5, 20).
    approx_pt(s, 5.0, 20.0);
}

#[test]
fn a_node_used_twice_links_to_its_last_copy() {
    let l = lay(r##"
        let a = rect().width(10).height(10);
        let b = rect().width(5).height(5);
        page(100, 100, stack([
            a.at(0, 0), a.at(40, 0), b.at(40, 50),
            connect(a, b, #{ from: "bottom", to: "top" }),
        ])).unit("pt")"##);
    let (s, _) = first_last(paths(&l)[0]);
    approx_pt(s, 45.0, 10.0);
}

#[test]
fn script_errors_name_the_problem() {
    let e = eval_script(
        r##"let a = rect().id("a");
           page(10, 10, col([a, connect("a", "nope")]))"##,
    )
    .expect_err("unknown id")
    .to_string();
    assert!(
        e.contains("no node has the id \"nope\"") && e.contains("ids on the page: a"),
        "{e}"
    );

    let e = eval_script(
        r##"let a = rect(); let b = rect();
           page(10, 10, col([a, connect(a, b)]))"##,
    )
    .expect_err("off-page node")
    .to_string();
    assert!(e.contains("`to` node is not on the page"), "{e}");

    let e = eval_script(r##"page(10, 10, col([rect().id("x"), rect().id("x")]))"##)
        .expect_err("duplicate id")
        .to_string();
    assert!(e.contains("two different nodes"), "{e}");

    let e = eval_script(r##"page(10, 10, rect().id("node3"))"##)
        .expect_err("reserved id")
        .to_string();
    assert!(e.contains("reserved"), "{e}");

    let e = eval_script(
        r##"let a = rect(); page(10, 10, col([a, connect(a, a, #{ rout: "elbow" })]))"##,
    )
    .expect_err("unknown option")
    .to_string();
    assert!(
        e.contains("unknown option \"rout\"") && e.contains("route"),
        "{e}"
    );

    let e = eval_script(
        r##"let a = rect(); page(10, 10, col([a, connect(a, a, #{ from: "side" })]))"##,
    )
    .expect_err("unknown point")
    .to_string();
    assert!(
        e.contains("unknown `from` point") && e.contains("top-left"),
        "{e}"
    );
}

const TOML_LINK: &str = r##"
    [page]
    width = 100
    height = 50
    unit = "pt"
    root = "root"

    [nodes.root]
    type = "row"
    spacing = 10
    children = ["a", "link", "b"]

    [nodes.a]
    type = "rect"
    width = 20
    height = 20

    [nodes.b]
    type = "rect"
    width = 20
    height = 20

    [nodes.link]
    type = "connector"
    from = "a"
    to = "b"
    to_at = [0, 0.25]
    route = "straight"
    head = "end"
    dash = [2, 1]
    stroke = "#c03030"
"##;

#[test]
fn toml_connector_resolves_and_takes_no_room() {
    let d = Document::from_toml(TOML_LINK).unwrap();
    let l = layout(&d, &NullMeasurer);
    let rects: Vec<Rect> = l
        .nodes
        .iter()
        .filter(|n| matches!(n.content, PaintContent::Rect { .. }))
        .map(|n| n.rect)
        .collect();
    // b follows a by exactly the spacing.
    approx(rects[1].x, 30.0);
    let p = paths(&l)[0];
    let (s, e) = first_last(p);
    approx_pt(s, 20.0, 10.0);
    approx_pt(e, 30.0, 5.0);
    assert_eq!(p.stroke.as_ref().unwrap().dash, vec![2.0, 1.0]);
    // Round trip through TOML keeps the connector.
    let raw = figs_core::schema::parse::parse_str(TOML_LINK).unwrap();
    let again = toml::to_string(&raw).unwrap();
    assert!(again.contains("type = \"connector\""), "{again}");
    assert!(Document::from_toml(&again).is_ok());
}

#[test]
fn toml_errors() {
    let bad = TOML_LINK.replace("to = \"b\"", "to = \"ghost\"");
    let e = Document::from_toml(&bad).unwrap_err().to_string();
    assert!(e.contains("no node has the id `ghost`"), "{e}");
    let bad = TOML_LINK.replace("to_at = [0, 0.25]", "to_at = \"middle-ish\"");
    let e = Document::from_toml(&bad).unwrap_err().to_string();
    assert!(e.contains("unknown point `middle-ish`"), "{e}");
    let bad = TOML_LINK.replace(
        "width = 20\n    height = 20\n\n    [nodes.b]",
        "width = 20\n    height = 20\n    route = \"elbow\"\n\n    [nodes.b]",
    );
    let e = Document::from_toml(&bad).unwrap_err().to_string();
    assert!(e.contains("only a connector has it"), "{e}");
}

#[test]
fn renders_in_every_backend() {
    let d = Document::from_script(
        r##"
        let a = rect().width(1).height(1).fill("#cccccc");
        let b = rect().width(1).height(1).fill("#cccccc");
        page(5, 3, row([a, b,
            connect(a, b, #{ head: "end", dash: [0.1, 0.05], width: 0.03 }),
            connect(a, b, #{ route: "elbow", from: "bottom", to: "bottom", band: 0.2,
                             fill: linear(0, ["#ff000080", "#0000ff80"]), head: "both" }),
        ]).spacing(2).pad(0.5)).unit("cm")"##,
    )
    .unwrap();
    let l = layout(&d, &NullMeasurer);
    let svg = render_svg(&l, &RenderOptions::default());
    assert!(svg.contains("stroke-dasharray"), "{svg}");
    assert!(svg.contains("linearGradient"), "{svg}");
    assert!(!render_png(&l, &RenderOptions::default())
        .unwrap()
        .is_empty());
    #[cfg(feature = "pdf")]
    assert!(!figs_core::render_pdf(&l, &RenderOptions::default())
        .unwrap()
        .is_empty());
}

#[test]
fn facing_corners_join_a_region_to_its_magnified_copy() {
    // The region [0.25, 0.25, 0.5, 0.75] of a 40 x 20 image is (10, 5) to
    // (20, 15); the copy (10 x 10) sits right of it at (60, 0): the links
    // join the region's right corners to the copy's left ones.
    let l = lay(r##"
        let img = rect().width(40).height(20);
        let copy = rect().width(10).height(10);
        let r = [0.25, 0.25, 0.5, 0.75];
        page(100, 100, stack([
            img.at(0, 0), copy.at(60, 0),
            [connect(img, copy, #{ from_box: r, from: "facing-1", to: "facing-1" }),
             connect(img, copy, #{ from_box: r, from: "facing-2", to: "facing-2" })],
        ])).unit("pt")"##);
    let p = paths(&l);
    assert_eq!(p.len(), 2, "an array among the children is spliced in");
    let (s, e) = first_last(p[0]);
    approx_pt(s, 20.0, 5.0);
    approx_pt(e, 60.0, 0.0);
    let (s, e) = first_last(p[1]);
    approx_pt(s, 20.0, 15.0);
    approx_pt(e, 60.0, 10.0);
    // Below instead: the bottom corners to the top ones, left first.
    let l = lay(r##"
        let img = rect().width(40).height(20);
        let copy = rect().width(10).height(10);
        page(100, 100, stack([
            img.at(0, 0), copy.at(30, 50),
            connect(img, copy, #{ from_box: [0, 0, 0.5, 0.5], from: "facing-2", to: "facing-2" }),
        ])).unit("pt")"##);
    let (s, e) = first_last(paths(&l)[0]);
    approx_pt(s, 20.0, 10.0);
    approx_pt(e, 40.0, 50.0);
}

#[test]
fn elbow_between_sides_facing_the_same_way_is_a_u() {
    // left -> left: out 8 pt (+ radius 0) past the outer end, then back in.
    let l = lay(r##"
        let a = rect().width(10).height(10);
        let b = rect().width(10).height(10);
        page(100, 100, stack([
            a.at(20, 0), b.at(30, 40),
            connect(a, b, #{ from: "left", to: "left", route: "elbow" }),
        ])).unit("pt")"##);
    let pts: Vec<Point> = paths(&l)[0].segments.iter().map(end_point).collect();
    assert_eq!(pts.len(), 4, "{pts:?}");
    approx_pt(pts[1], 12.0, 5.0);
    approx_pt(pts[2], 12.0, 45.0);
    approx_pt(pts[3], 30.0, 45.0);
}

/// Images measure as their natural aspect (2:1) and nothing else.
struct WideImages;
impl figs_core::LeafMeasure for WideImages {
    fn measure_text(&self, _t: &figs_core::schema::TextProps, _w: f32) -> figs_core::geom::Size {
        figs_core::geom::Size::default()
    }
    fn measure_image(&self, _i: &figs_core::schema::ImageProps) -> figs_core::geom::Size {
        figs_core::geom::Size::default()
    }
    fn image_aspect(&self, _i: &figs_core::schema::ImageProps) -> Option<f32> {
        Some(2.0)
    }
}

#[test]
fn fractions_of_a_letterboxed_image_are_of_the_picture() {
    // A 2:1 image in a 40 x 40 box ("contain"): the picture is 40 x 20 at
    // y = 10; its top-left corner is (0, 10), not (0, 0).
    let d = Document::from_script(
        r##"
        let img = image("x.png").width(40).height(40);
        let b = rect().width(4).height(4);
        page(100, 100, stack([img, b.at(80, 0),
            connect(img, b, #{ from: "top-left", to: "center" })])).unit("pt")"##,
    )
    .unwrap();
    let l = layout(&d, &WideImages);
    let (s, _) = first_last(paths(&l)[0]);
    approx_pt(s, 0.0, 10.0);
}
