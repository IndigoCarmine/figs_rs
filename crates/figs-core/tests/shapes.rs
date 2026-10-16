//! Vector primitives (`line`, `arrow`, `ellipse`, `path`): resolve defaults and
//! validation, layout (intrinsic size from points, explicit size, absolute
//! paint coordinates), Rhai lowering, and PNG rasterization.

use figs_core::geom::{ArrowHead, Color, LineCap, Point, Rect};
use figs_core::layout::{ComputedLayout, PaintContent, PathPaint};
use figs_core::schema::{NodeType, PathProps};
use figs_core::{layout, render_png, Document, NullMeasurer, RenderOptions};
use tiny_skia::Pixmap;

fn lay(src: &str) -> ComputedLayout {
    let doc = Document::from_toml(src).expect("document should resolve");
    layout(&doc, &NullMeasurer)
}

fn node<'a>(l: &'a ComputedLayout, id: &str) -> &'a figs_core::layout::ComputedNode {
    l.nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("node `{id}` not found"))
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
fn assert_rect(actual: Rect, x: f32, y: f32, w: f32, h: f32) {
    let ok = [(actual.x, x), (actual.y, y), (actual.w, w), (actual.h, h)]
        .iter()
        .all(|(a, b)| (a - b).abs() < 0.01);
    assert!(ok, "expected ({x}, {y}, {w}, {h}), got {actual:?}");
}

// ---- resolve ---------------------------------------------------------------

#[test]
fn resolve_defaults_and_unit_conversion() {
    let d = Document::from_toml(
        r##"
        [page]
        width = 10
        height = 10
        unit = "in"
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["l", "a", "p", "q", "e"]
        [nodes.l]
        type = "line"
        points = [[0, 0], [1, 0.5]]
        [nodes.a]
        type = "arrow"
        points = [[0, 0], [1, 0]]
        stroke_width = 0.1
        dash = [0.25]
        [nodes.p]
        type = "path"
        points = [[0, 0], [1, 0], [1, 1]]
        fill = "#ff0000"
        [nodes.q]
        type = "path"
        points = [[0, 0], [1, 1]]
        [nodes.e]
        type = "ellipse"
        "##,
    )
    .unwrap();

    // line: black 1pt stroke, butt caps, no head; points converted to pt.
    let l = props(&d, "l");
    let s = l.stroke.as_ref().expect("line is stroked by default");
    assert_eq!(s.color, Color::BLACK);
    assert_eq!(s.width, 1.0);
    assert_eq!(s.cap, LineCap::Butt);
    assert_eq!(l.head, ArrowHead::None);
    assert_eq!(l.points, vec![Point::new(0.0, 0.0), Point::new(72.0, 36.0)]);

    // arrow: head at the end, sized 5x the stroke; odd dash list repeated.
    let a = props(&d, "a");
    let s = a.stroke.as_ref().unwrap();
    assert!((s.width - 7.2).abs() < 1e-4);
    assert_eq!(s.dash, vec![18.0, 18.0]);
    assert_eq!(a.head, ArrowHead::End);
    assert!((a.head_size - 36.0).abs() < 1e-3);

    // A filled path isn't stroked unless asked; a bare one gets a black stroke.
    assert!(props(&d, "p").stroke.is_none());
    assert!(props(&d, "q").stroke.is_some());
    match &d.nodes.iter().find(|n| n.id == "e").unwrap().kind {
        NodeType::Ellipse(e) => assert_eq!(e.stroke.as_ref().unwrap().color, Color::BLACK),
        _ => panic!("ellipse"),
    }
}

fn resolve_err(body: &str) -> String {
    let src = format!("[page]\nwidth = 1\nheight = 1\nroot = \"n\"\n[nodes.n]\n{body}");
    Document::from_toml(&src).unwrap_err().to_string()
}

#[test]
fn resolve_rejects_bad_shapes() {
    let e = resolve_err("type = \"line\"");
    assert!(e.contains("line requires field `points`"), "{e}");
    let e = resolve_err("type = \"arrow\"\npoints = [[0, 0], [1, 1], [2, 2]]");
    assert!(e.contains("arrow needs exactly 2 points, got 3"), "{e}");
    let e = resolve_err("type = \"path\"\npoints = [[0, 0]]");
    assert!(e.contains("path needs at least 2 points"), "{e}");
    let e = resolve_err("type = \"line\"\npoints = [[0, 0], [1, 1]]\ndash = [0.1, -1]");
    assert!(e.contains("invalid `dash`"), "{e}");
    let e = resolve_err("type = \"line\"\npoints = [[0, 0], [1, 1]]\nhead = \"middle\"");
    assert!(e.contains("parse"), "{e}");
}

// ---- layout ----------------------------------------------------------------

#[test]
fn intrinsic_size_comes_from_points() {
    // In a stack, an offset line keeps its intrinsic size: the extent of its
    // points from the box origin. Its paint points are absolute page coords.
    let l = lay(r#"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "s"
        [nodes.s]
        type = "stack"
        padding = 2
        children = ["base", "ln"]
        [nodes.base]
        type = "rect"
        [nodes.ln]
        type = "arrow"
        points = [[5, 0], [30, 20]]
        x = 10
        y = 5
        "#);
    assert_rect(node(&l, "ln").rect, 12.0, 7.0, 30.0, 20.0);
    let p = path_of(&l, "ln");
    assert_eq!(
        p.points,
        vec![Point::new(17.0, 7.0), Point::new(42.0, 27.0)]
    );
    assert_eq!(p.head, ArrowHead::End);
}

#[test]
fn shapes_flow_in_a_column() {
    // Start-aligned column: a path takes its point extent (+ padding), an
    // ellipse its explicit size, and a line with explicit size ignores its
    // points for sizing (they are not scaled).
    let l = lay(r#"
        [page]
        width = 200
        height = 200
        unit = "pt"
        root = "c"
        [nodes.c]
        type = "column"
        children = ["p", "e", "l"]
        [nodes.p]
        type = "path"
        points = [[0, 0], [40, 10], [20, 30]]
        closed = true
        padding = 1
        [nodes.e]
        type = "ellipse"
        width = 50
        height = 25
        [nodes.l]
        type = "line"
        points = [[0, 0], [10, 10]]
        width = 60
        height = 5
        "#);
    assert_rect(node(&l, "p").rect, 0.0, 0.0, 42.0, 32.0);
    assert_eq!(path_of(&l, "p").points[1], Point::new(41.0, 11.0));
    assert!(path_of(&l, "p").closed);
    assert_rect(node(&l, "e").rect, 0.0, 32.0, 50.0, 25.0);
    assert!(matches!(
        node(&l, "e").content,
        PaintContent::Ellipse { .. }
    ));
    assert_rect(node(&l, "l").rect, 0.0, 57.0, 60.0, 5.0);
    assert_eq!(path_of(&l, "l").points[1], Point::new(10.0, 67.0));
}

#[test]
fn unpositioned_shape_fills_stack_and_uses_stack_coordinates() {
    let l = lay(r#"
        [page]
        width = 100
        height = 80
        unit = "pt"
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["ln"]
        [nodes.ln]
        type = "line"
        points = [[10, 20], [90, 70]]
        "#);
    assert_rect(node(&l, "ln").rect, 0.0, 0.0, 100.0, 80.0);
    assert_eq!(path_of(&l, "ln").points[0], Point::new(10.0, 20.0));
}

// ---- script ----------------------------------------------------------------

#[cfg(feature = "script")]
mod script {
    use super::*;
    use figs_core::eval_script;
    use figs_core::schema::NodeKind;

    #[test]
    fn builders_and_setters_lower() {
        let raw = eval_script(
            r##"
            page(100, 100, stack([
                line(0, 0, 10.5, 20).dash([2, 1]).cap("round").stroke("#ff0000"),
                arrow(1, 2, 3, 4).head("both").head_size(6).stroke_width(0.5),
                ellipse().fill("#00ff00").width(10).height(10),
                path([[0, 0], [5, 0.5], [2, 8]]).closed().fill("#0000ff"),
            ])).unit("pt")
            "##,
        )
        .unwrap();
        let kids: Vec<_> = raw.nodes["root"]
            .children
            .iter()
            .map(|c| &raw.nodes[c])
            .collect();
        assert_eq!(kids[0].kind, NodeKind::Line);
        assert_eq!(kids[0].points, vec![[0.0, 0.0], [10.5, 20.0]]);
        assert_eq!(kids[0].dash, vec![2.0, 1.0]);
        assert_eq!(kids[0].cap, Some(LineCap::Round));
        assert_eq!(kids[1].kind, NodeKind::Arrow);
        assert_eq!(kids[1].head, Some(ArrowHead::Both));
        assert_eq!(kids[1].head_size, Some(6.0));
        assert_eq!(kids[1].stroke_width, Some(0.5));
        assert_eq!(kids[2].kind, NodeKind::Ellipse);
        assert_eq!(kids[3].kind, NodeKind::Path);
        assert!(kids[3].closed);
        assert_eq!(kids[3].points.len(), 3);
    }

    #[test]
    fn circle_sugar_is_a_centered_square_ellipse() {
        let raw = eval_script(r#"page(10, 10, stack([ circle(5, 4, 1.5), circle(2) ]))"#).unwrap();
        let kids: Vec<_> = raw.nodes["root"]
            .children
            .iter()
            .map(|c| &raw.nodes[c])
            .collect();
        assert_eq!(kids[0].kind, NodeKind::Ellipse);
        assert_eq!((kids[0].width, kids[0].height), (Some(3.0), Some(3.0)));
        assert_eq!((kids[0].x, kids[0].y), (Some(3.5), Some(2.5)));
        assert_eq!((kids[1].width, kids[1].x), (Some(4.0), None));
    }

    #[test]
    fn bad_arguments_are_reported() {
        let e = eval_script(r#"page(1, 1, path([[0, 0], [1]]))"#).unwrap_err();
        assert!(e.to_string().contains("[x, y] number pairs"), "{e}");
        let e = eval_script(r#"page(1, 1, line(0, 0, "a", 1))"#).unwrap_err();
        assert!(e.to_string().contains("`line` expects numbers"), "{e}");
        let e = eval_script(r#"page(1, 1, arrow(0, 0, 1, 1).head("up"))"#).unwrap_err();
        assert!(e.to_string().contains("`head`: invalid value"), "{e}");
        let e = eval_script(r#"page(1, 1, line(0, 0, 1, 1).dash(["x"]))"#).unwrap_err();
        assert!(e.to_string().contains("`dash` expects"), "{e}");
    }

    #[test]
    fn annotations_figs_example() {
        let src = include_str!("../../../examples/annotations.figs");
        let d = Document::from_script(src).unwrap_or_else(|e| panic!("example failed: {e}"));
        let l = layout(&d, &NullMeasurer);
        assert!(l
            .nodes
            .iter()
            .any(|n| matches!(n.content, PaintContent::Path(_))));
        assert!(l
            .nodes
            .iter()
            .any(|n| matches!(n.content, PaintContent::Ellipse { .. })));
        render_png(&l, &RenderOptions::default()).expect("renders");
    }
}

// ---- render ----------------------------------------------------------------

/// Render at 72 dpi (1 pt == 1 px) on a white page.
fn render(nodes: &str) -> Pixmap {
    let src = format!(
        "[page]\nwidth = 100\nheight = 100\nunit = \"pt\"\ndpi = 72\nbackground = \"#ffffff\"\nroot = \"s\"\n{nodes}"
    );
    let doc = Document::from_toml(&src).expect("resolve");
    let bytes = render_png(&layout(&doc, &NullMeasurer), &RenderOptions::default()).unwrap();
    Pixmap::decode_png(&bytes).unwrap()
}

#[track_caller]
fn assert_pixel(p: &Pixmap, x: u32, y: u32, rgb: (u8, u8, u8)) {
    let px = p.pixel(x, y).expect("in bounds");
    let near = |a: u8, b: u8| (a as i32 - b as i32).abs() <= 2;
    assert!(
        near(px.red(), rgb.0) && near(px.green(), rgb.1) && near(px.blue(), rgb.2),
        "pixel ({x},{y}) = ({},{},{}), expected {rgb:?}",
        px.red(),
        px.green(),
        px.blue()
    );
}

const RED: (u8, u8, u8) = (255, 0, 0);
const WHITE: (u8, u8, u8) = (255, 255, 255);

#[test]
fn line_pixels_are_stroked_and_off_line_is_background() {
    let p = render(
        r##"
        [nodes.s]
        type = "line"
        points = [[10, 50], [90, 50]]
        stroke = "#ff0000"
        stroke_width = 4
        "##,
    );
    assert_pixel(&p, 50, 50, RED);
    assert_pixel(&p, 50, 48, RED);
    assert_pixel(&p, 50, 40, WHITE);
    assert_pixel(&p, 5, 50, WHITE); // butt cap: nothing before the start
}

#[test]
fn dashed_line_has_gaps() {
    let p = render(
        r##"
        [nodes.s]
        type = "line"
        points = [[10, 50], [90, 50]]
        stroke = "#ff0000"
        stroke_width = 4
        dash = [10, 10]
        "##,
    );
    assert_pixel(&p, 15, 50, RED); // first dash: 10..20
    assert_pixel(&p, 25, 50, WHITE); // gap: 20..30
    assert_pixel(&p, 35, 50, RED);
}

#[test]
fn arrow_head_is_filled_and_stops_at_the_tip() {
    let p = render(
        r##"
        [nodes.s]
        type = "arrow"
        points = [[10, 50], [90, 50]]
        stroke = "#ff0000"
        stroke_width = 2
        head_size = 20
        "##,
    );
    // ~10 pt behind the tip the head is ~8 pt wide: (80, 52) is inside it but
    // outside the 2 pt shaft.
    assert_pixel(&p, 80, 52, RED);
    assert_pixel(&p, 80, 58, WHITE);
    assert_pixel(&p, 95, 50, WHITE);
    assert_pixel(&p, 40, 50, RED);
    assert_pixel(&p, 40, 53, WHITE);
}

#[test]
fn ellipse_and_closed_path_fill() {
    let p = render(
        r##"
        [nodes.s]
        type = "stack"
        children = ["e", "tri"]
        [nodes.e]
        type = "ellipse"
        fill = "#ff0000"
        [nodes.tri]
        type = "path"
        points = [[40, 40], [60, 40], [50, 60]]
        closed = true
        fill = "#0000ff"
        "##,
    );
    assert_pixel(&p, 20, 50, RED); // inside the page-sized circle
    assert_pixel(&p, 3, 3, WHITE); // corner is outside it
    assert_pixel(&p, 50, 45, (0, 0, 255)); // triangle on top
}
