//! Node rotation (`rotate`): quarter turns swap the layout footprint, other
//! angles are paint-only; the IR carries an absolute per-node transform that
//! every backend (PNG / PDF / SVG) and the editor's hit-test honour.

use figs_core::geom::{quarter_turns, Affine, Point, Rect};
use figs_core::layout::{ComputedLayout, ComputedNode};
use figs_core::{layout, render_png, render_svg, Document, FontStore, NullMeasurer, RenderOptions};
use tiny_skia::Pixmap;

/// A `w × h` pt page (72 dpi, white) with root `root` and the given nodes.
fn src(w: f32, h: f32, nodes: &str) -> String {
    format!(
        "[page]\nwidth = {w}\nheight = {h}\nunit = \"pt\"\ndpi = 72\nroot = \"root\"\n\
         background = \"#ffffff\"\n{nodes}"
    )
}

fn lay(w: f32, h: f32, nodes: &str) -> ComputedLayout {
    let doc = Document::from_toml(&src(w, h, nodes)).expect("document should resolve");
    layout(&doc, &NullMeasurer)
}

fn node<'a>(l: &'a ComputedLayout, id: &str) -> &'a ComputedNode {
    l.nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("node `{id}` not found"))
}

#[track_caller]
fn assert_rect(actual: Rect, x: f32, y: f32, w: f32, h: f32) {
    let eps = 0.01;
    let ok = (actual.x - x).abs() < eps
        && (actual.y - y).abs() < eps
        && (actual.w - w).abs() < eps
        && (actual.h - h).abs() < eps;
    assert!(
        ok,
        "rect mismatch: expected ({x}, {y}, {w}, {h}), got ({}, {}, {}, {})",
        actual.x, actual.y, actual.w, actual.h
    );
}

#[track_caller]
fn assert_point(p: Point, x: f32, y: f32) {
    assert!(
        (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3,
        "expected ({x}, {y}), got ({}, {})",
        p.x,
        p.y
    );
}

// ---------------------------------------------------------------------------
// geometry
// ---------------------------------------------------------------------------

#[test]
fn quarter_turn_detection() {
    assert_eq!(quarter_turns(0.0), Some(0));
    assert_eq!(quarter_turns(90.0), Some(1));
    assert_eq!(quarter_turns(-90.0), Some(3));
    assert_eq!(quarter_turns(270.0), Some(3));
    assert_eq!(quarter_turns(180.0), Some(2));
    assert_eq!(quarter_turns(450.0), Some(1));
    assert_eq!(quarter_turns(-360.0), Some(0));
    assert_eq!(quarter_turns(90.0005), Some(1));
    assert_eq!(quarter_turns(45.0), None);
    assert_eq!(quarter_turns(89.9), None);
    assert_eq!(quarter_turns(f32::NAN), None);
}

#[test]
fn affine_rotation_is_clockwise_on_the_page() {
    // +90 about the origin: right (1, 0) turns to down (0, 1).
    let r = Affine::rotation_about(90.0, 0.0, 0.0);
    assert_eq!(
        (r.a, r.b, r.c, r.d),
        (0.0, 1.0, -1.0, 0.0),
        "exact quarter turn"
    );
    assert_point(r.apply(Point::new(1.0, 0.0)), 0.0, 1.0);
    // About a centre the centre stays put.
    let r = Affine::rotation_about(37.0, 5.0, 7.0);
    assert_point(r.apply(Point::new(5.0, 7.0)), 5.0, 7.0);
    // Composition and inversion.
    let back = r.then(r.invert().unwrap());
    assert!(back.is_identity(), "{back:?}");
    let two = Affine::rotation_about(30.0, 1.0, 2.0).then(Affine::rotation_about(60.0, 1.0, 2.0));
    let ninety = Affine::rotation_about(90.0, 1.0, 2.0);
    for (x, y) in [(0.0, 0.0), (3.0, -1.0)] {
        let (p, q) = (two.apply(Point::new(x, y)), ninety.apply(Point::new(x, y)));
        assert_point(p, q.x, q.y);
    }
}

// ---------------------------------------------------------------------------
// layout: quarter turns swap the footprint
// ---------------------------------------------------------------------------

#[test]
fn quarter_turn_swaps_footprint_in_a_row() {
    for deg in [90, -90, 270] {
        let l = lay(
            100.0,
            60.0,
            &format!(
                r#"
                [nodes.root]
                type = "row"
                children = ["r", "rest"]
                [nodes.r]
                type = "rect"
                width = 40
                height = 10
                rotate = {deg}
                [nodes.rest]
                type = "rect"
                flex = 1
                "#
            ),
        );
        let r = node(&l, "r");
        // The row sees a 10 x 40 footprint; the node's own box is 40 x 10,
        // centred in it.
        assert_rect(r.bounds(), 0.0, 0.0, 10.0, 40.0);
        assert_rect(r.rect, -15.0, 15.0, 40.0, 10.0);
        assert!(r.transform.is_some());
        // The flex sibling starts right after the rotated footprint.
        assert_rect(node(&l, "rest").rect, 10.0, 0.0, 90.0, 0.0);
    }
}

#[test]
fn quarter_turn_transform_values() {
    let l = lay(
        100.0,
        60.0,
        r#"
        [nodes.root]
        type = "row"
        children = ["r"]
        [nodes.r]
        type = "rect"
        width = 40
        height = 10
        rotate = 90
        "#,
    );
    let r = node(&l, "r");
    let t = r.transform.expect("rotated");
    // Clockwise quarter turn about the footprint centre (5, 20).
    assert_eq!(
        t,
        Affine {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            e: 25.0,
            f: 15.0
        }
    );
    // The box's top-left corner lands at the footprint's top-right.
    let c = r.corners();
    assert_point(c[0], 10.0, 0.0);
    assert_point(c[1], 10.0, 40.0);
    assert_point(c[2], 0.0, 40.0);
    assert_point(c[3], 0.0, 0.0);

    // -90: counter-clockwise, top-left corner at the footprint's bottom-left.
    let l = lay(
        100.0,
        60.0,
        r#"
        [nodes.root]
        type = "row"
        children = ["r"]
        [nodes.r]
        type = "rect"
        width = 40
        height = 10
        rotate = -90
        "#,
    );
    let r = node(&l, "r");
    let t = r.transform.unwrap();
    assert_eq!((t.a, t.b, t.c, t.d), (0.0, -1.0, 1.0, 0.0));
    assert_point(r.corners()[0], 0.0, 40.0);
}

#[test]
fn quarter_turn_swaps_footprint_in_a_column() {
    let l = lay(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "column"
        children = ["r", "below"]
        [nodes.r]
        type = "rect"
        width = 30
        height = 10
        rotate = 90
        [nodes.below]
        type = "rect"
        width = 5
        height = 5
        "#,
    );
    assert_rect(node(&l, "r").bounds(), 0.0, 0.0, 10.0, 30.0);
    assert_rect(node(&l, "below").rect, 0.0, 30.0, 5.0, 5.0);
}

#[test]
fn stretched_quarter_turn_takes_the_row_height_as_its_width() {
    // The vertical-axis-label case: a rotated leaf in a stretched row is as
    // long (own width) as the row is tall, and as wide as its own height.
    let l = lay(
        100.0,
        60.0,
        r#"
        [nodes.root]
        type = "row"
        cross_axis_alignment = "stretch"
        children = ["label", "plot"]
        [nodes.label]
        type = "rect"
        height = 8
        rotate = -90
        [nodes.plot]
        type = "rect"
        flex = 1
        "#,
    );
    let label = node(&l, "label");
    assert_rect(label.bounds(), 0.0, 0.0, 8.0, 60.0);
    assert_rect(label.rect, -26.0, 26.0, 60.0, 8.0);
    assert_rect(node(&l, "plot").rect, 8.0, 0.0, 92.0, 60.0);
}

#[test]
fn quarter_turn_in_a_stack_uses_the_footprint_for_offset_and_anchor() {
    let l = lay(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "stack"
        children = ["a", "b"]
        [nodes.a]
        type = "rect"
        width = 30
        height = 10
        x = 5
        y = 5
        rotate = 90
        [nodes.b]
        type = "rect"
        width = 30
        height = 10
        x = 50
        y = 50
        anchor = [0.5, 1]
        rotate = 270
        "#,
    );
    assert_rect(node(&l, "a").bounds(), 5.0, 5.0, 10.0, 30.0);
    // Anchor fractions are of the footprint: bottom-centre at (50, 50).
    assert_rect(node(&l, "b").bounds(), 45.0, 20.0, 10.0, 30.0);
}

#[test]
fn quarter_turn_with_aspect_ratio() {
    // Own box 20 x 10 (aspect 2); in a stack the footprint is 10 x 20.
    let l = lay(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "stack"
        children = ["a"]
        [nodes.a]
        type = "rect"
        width = 20
        aspect_ratio = 2
        x = 0
        y = 0
        rotate = 90
        "#,
    );
    let a = node(&l, "a");
    assert_rect(a.bounds(), 0.0, 0.0, 10.0, 20.0);
    assert_rect(a.rect, -5.0, 5.0, 20.0, 10.0);

    // In a row with flex: the flex share is the footprint width (= own
    // height), and the aspect derives the own width (= footprint height).
    let l = lay(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "row"
        children = ["a", "b"]
        [nodes.a]
        type = "rect"
        flex = 1
        aspect_ratio = 2
        rotate = 90
        [nodes.b]
        type = "rect"
        flex = 3
        "#,
    );
    let a = node(&l, "a");
    assert_rect(a.bounds(), 0.0, 0.0, 25.0, 50.0);
    assert_rect(a.rect, -12.5, 12.5, 50.0, 25.0);
}

#[test]
fn rotated_container_lays_children_out_in_its_own_frame() {
    // A rotated row: its children run along its own (unrotated) main axis and
    // inherit its transform.
    let l = lay(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "column"
        children = ["r"]
        [nodes.r]
        type = "row"
        rotate = 90
        children = ["a", "b"]
        [nodes.a]
        type = "rect"
        width = 20
        height = 10
        [nodes.b]
        type = "rect"
        width = 30
        height = 10
        "#,
    );
    let r = node(&l, "r");
    // Own box 50 x 10, footprint 10 x 50.
    assert_rect(r.bounds(), 0.0, 0.0, 10.0, 50.0);
    let (a, b) = (node(&l, "a"), node(&l, "b"));
    assert_eq!(a.transform, r.transform);
    assert_eq!(b.transform, r.transform);
    assert_rect(a.rect, r.rect.x, r.rect.y, 20.0, 10.0);
    assert_rect(b.rect, r.rect.x + 20.0, r.rect.y, 30.0, 10.0);
    // On the page the row runs downwards: a on top, b below it.
    assert_rect(a.bounds(), 0.0, 0.0, 10.0, 20.0);
    assert_rect(b.bounds(), 0.0, 20.0, 10.0, 30.0);
}

#[test]
fn nested_rotations_compose() {
    // +90 inside -90 cancels out on the page.
    let l = lay(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "stack"
        children = ["outer"]
        [nodes.outer]
        type = "stack"
        width = 40
        height = 20
        x = 10
        y = 10
        rotate = -90
        children = ["inner"]
        [nodes.inner]
        type = "rect"
        width = 10
        height = 6
        x = 0
        y = 0
        rotate = 90
        "#,
    );
    let inner = node(&l, "inner");
    let t = inner.transform.expect("composed transform");
    assert!((t.a - 1.0).abs() < 1e-6 && t.b.abs() < 1e-6, "{t:?}");
    // outer: own 40 x 20 → footprint 20 x 40 at (10, 10).
    assert_rect(node(&l, "outer").bounds(), 10.0, 10.0, 20.0, 40.0);
    // inner's 6 x 10 footprint sits at outer's own top-left corner, which the
    // -90 turn carries to the bottom-left of outer's footprint; turned twice,
    // inner is upright again (10 x 6) on the page.
    let b = inner.bounds();
    assert_rect(b, 10.0, 44.0, 10.0, 6.0);
}

#[test]
fn whole_turns_are_no_rotation() {
    let l = lay(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "row"
        children = ["a", "b"]
        [nodes.a]
        type = "rect"
        width = 40
        height = 10
        rotate = 360
        [nodes.b]
        type = "rect"
        width = 40
        height = 10
        rotate = 180
        "#,
    );
    assert!(node(&l, "a").transform.is_none());
    assert_rect(node(&l, "a").rect, 0.0, 0.0, 40.0, 10.0);
    // A half turn keeps the footprint but does rotate.
    let b = node(&l, "b");
    assert_rect(b.rect, 40.0, 0.0, 40.0, 10.0);
    assert_rect(b.bounds(), 40.0, 0.0, 40.0, 10.0);
    let t = b.transform.unwrap();
    assert_eq!((t.a, t.b, t.c, t.d), (-1.0, 0.0, 0.0, -1.0));
}

#[test]
fn arbitrary_angle_is_paint_only() {
    let l = lay(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "row"
        children = ["a", "b"]
        [nodes.a]
        type = "rect"
        width = 40
        height = 10
        rotate = 30
        [nodes.b]
        type = "rect"
        width = 5
        height = 5
        "#,
    );
    let a = node(&l, "a");
    // Layout uses the unrotated box; the sibling isn't pushed.
    assert_rect(a.rect, 0.0, 0.0, 40.0, 10.0);
    assert_rect(node(&l, "b").rect, 40.0, 0.0, 5.0, 5.0);
    // Turned 30° about its centre (20, 5); the drawing overhangs the box.
    let t = a.transform.unwrap();
    let (s, c) = 30f32.to_radians().sin_cos();
    assert!((t.a - c).abs() < 1e-6 && (t.b - s).abs() < 1e-6);
    assert_point(t.apply(Point::new(20.0, 5.0)), 20.0, 5.0);
    let bb = a.bounds();
    let w = 40.0 * c + 10.0 * s;
    let h = 40.0 * s + 10.0 * c;
    assert_rect(bb, 20.0 - w / 2.0, 5.0 - h / 2.0, w, h);
}

#[test]
fn auto_height_page_uses_the_rotated_footprint() {
    let doc = Document::from_toml(
        r#"
        [page]
        width = 100
        height = "auto"
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "column"
        children = ["r"]
        [nodes.r]
        type = "rect"
        width = 30
        height = 10
        rotate = 90
        "#,
    )
    .unwrap();
    let l = layout(&doc, &NullMeasurer);
    assert!(
        (l.page.height_pt - 30.0).abs() < 0.01,
        "{}",
        l.page.height_pt
    );

    // A rotated column inside an auto-height column still lays its own
    // (bounded) vertical axis out with flex.
    let doc = Document::from_toml(
        r#"
        [page]
        width = 100
        height = "auto"
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "column"
        cross_axis_alignment = "stretch"
        children = ["c"]
        [nodes.c]
        type = "column"
        width = 20
        rotate = 90
        cross_axis_alignment = "stretch"
        children = ["x", "y"]
        [nodes.x]
        type = "rect"
        flex = 1
        [nodes.y]
        type = "rect"
        flex = 1
        "#,
    )
    .unwrap();
    let l = layout(&doc, &NullMeasurer);
    // The column's own height is the page width (stretched footprint width).
    let c = node(&l, "c");
    assert_rect(c.bounds(), 0.0, 0.0, 100.0, 20.0);
    assert!((node(&l, "x").rect.h - 50.0).abs() < 0.01);
    assert!((l.page.height_pt - 20.0).abs() < 0.01);
}

// ---------------------------------------------------------------------------
// hit-testing
// ---------------------------------------------------------------------------

#[test]
fn hit_test_respects_rotation() {
    let l = lay(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "stack"
        children = ["bar"]
        [nodes.bar]
        type = "rect"
        width = 60
        height = 10
        x = 50
        y = 50
        anchor = [0.5, 0.5]
        rotate = 45
        "#,
    );
    let hit = |x: f32, y: f32| l.hit_test(x, y).map(|n| n.id.as_str());
    assert_eq!(hit(50.0, 50.0), Some("bar"));
    // Along the turned long axis (down-right).
    assert_eq!(hit(65.0, 65.0), Some("bar"));
    // Inside the unrotated box but off the turned bar.
    assert_eq!(hit(75.0, 50.0), Some("root"));
    assert_eq!(hit(35.0, 65.0), Some("root"));
}

// ---------------------------------------------------------------------------
// rendering
// ---------------------------------------------------------------------------

fn png(nodes: &str, fonts: Option<&FontStore>) -> Pixmap {
    let doc = Document::from_toml(&src(100.0, 100.0, nodes)).unwrap();
    let l = match fonts {
        Some(f) => layout(&doc, f),
        None => layout(&doc, &NullMeasurer),
    };
    let opts = RenderOptions {
        fonts,
        base_dir: std::path::Path::new("."),
    };
    Pixmap::decode_png(&render_png(&l, &opts).unwrap()).unwrap()
}

fn is_red(p: &Pixmap, x: u32, y: u32) -> bool {
    let c = p.pixel(x, y).unwrap();
    c.red() > 200 && c.green() < 60 && c.blue() < 60
}

fn is_white(p: &Pixmap, x: u32, y: u32) -> bool {
    let c = p.pixel(x, y).unwrap();
    c.red() > 245 && c.green() > 245 && c.blue() > 245
}

const RED_BAR: &str = r##"
    [nodes.root]
    type = "stack"
    children = ["bar"]
    [nodes.bar]
    type = "rect"
    fill = "#ff0000"
    width = 60
    height = 20
    x = 50
    y = 50
    anchor = [0.5, 0.5]
"##;

#[test]
fn png_quarter_turned_rect() {
    let p = png(&format!("{RED_BAR}rotate = 90\n"), None);
    // Now 20 wide and 60 tall, centred: x 40..60, y 20..80.
    assert!(is_red(&p, 50, 22));
    assert!(is_red(&p, 50, 78));
    assert!(is_red(&p, 41, 50));
    assert!(is_red(&p, 58, 50));
    assert!(is_white(&p, 25, 50), "unrotated extent must be gone");
    assert!(is_white(&p, 75, 50));
    assert!(is_white(&p, 50, 18));
}

#[test]
fn png_tilted_rect() {
    let p = png(&format!("{RED_BAR}rotate = 45\n"), None);
    assert!(is_red(&p, 50, 50));
    assert!(is_red(&p, 65, 65), "long axis turned down-right");
    assert!(is_red(&p, 35, 35));
    assert!(is_white(&p, 65, 35));
    assert!(is_white(&p, 76, 50), "unrotated end must be gone");
}

#[test]
fn png_rotated_text_runs_vertically() {
    let fonts = FontStore::new();
    let nodes = |rotate: &str| {
        format!(
            r##"
            [nodes.root]
            type = "stack"
            children = ["t"]
            [nodes.t]
            type = "text"
            content = "HHHHHHHH"
            font_size = 12
            color = "#000000"
            x = 50
            y = 50
            anchor = [0.5, 0.5]
            {rotate}
            "##
        )
    };
    let extent = |p: &Pixmap| {
        let (mut x0, mut y0, mut x1, mut y1, mut n) = (u32::MAX, u32::MAX, 0, 0, 0);
        for y in 0..p.height() {
            for x in 0..p.width() {
                if p.pixel(x, y).unwrap().red() < 128 {
                    n += 1;
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (n, x0, y0, x1, y1)
    };
    let (n0, ux0, uy0, ux1, uy1) = extent(&png(&nodes(""), Some(&fonts)));
    if n0 == 0 {
        eprintln!("no system fonts; skipping");
        return;
    }
    let (n1, x0, y0, x1, y1) = extent(&png(&nodes("rotate = -90"), Some(&fonts)));
    assert!(n1 > 0, "rotated text must be drawn");
    let (uw, uh) = (ux1 - ux0, uy1 - uy0);
    let (w, h) = (x1 - x0, y1 - y0);
    assert!(uw > uh * 2, "upright text is wide ({uw} x {uh})");
    assert!(h > w * 2, "rotated text is tall ({w} x {h})");
    // Same ink, turned: extents swap (within AA/rounding).
    assert!((w as i32 - uh as i32).abs() <= 2 && (h as i32 - uw as i32).abs() <= 2);
    // Centred on the anchor point.
    assert!(((x0 + x1) as i32 / 2 - 50).abs() <= 3);
    assert!(((y0 + y1) as i32 / 2 - 50).abs() <= 3);
    // Roughly the same amount of ink.
    let ratio = n1 as f32 / n0 as f32;
    assert!((0.7..1.3).contains(&ratio), "ink ratio {ratio}");
}

#[test]
fn rotated_text_takes_its_rotated_footprint_in_a_row() {
    let fonts = FontStore::new();
    let doc = Document::from_toml(&src(
        200.0,
        100.0,
        r#"
        [nodes.root]
        type = "row"
        cross_axis_alignment = "center"
        children = ["ylabel", "plot"]
        [nodes.ylabel]
        type = "text"
        content = "Intensity"
        font_size = 10
        rotate = -90
        [nodes.plot]
        type = "rect"
        flex = 1
        "#,
    ))
    .unwrap();
    let l = layout(&doc, &fonts);
    let label = node(&l, "ylabel");
    if label.rect.w <= 0.0 {
        eprintln!("no system fonts; skipping");
        return;
    }
    let b = label.bounds();
    // Footprint = the text box turned: as wide as a line is tall.
    assert!((b.w - label.rect.h).abs() < 0.01 && (b.h - label.rect.w).abs() < 0.01);
    assert!(b.h > b.w, "vertical label");
    assert!((b.x).abs() < 0.01);
    // Cross-centred vertically on the 100 pt row.
    assert!((b.y + b.h / 2.0 - 50.0).abs() < 0.01);
    assert!((node(&l, "plot").rect.x - b.w).abs() < 0.01);
}

#[test]
fn svg_wraps_rotated_nodes_in_a_transform_group() {
    let doc = Document::from_toml(&src(100.0, 100.0, &format!("{RED_BAR}rotate = 90\n"))).unwrap();
    let l = layout(&doc, &NullMeasurer);
    let svg = render_svg(&l, &RenderOptions::default());
    // Centre (50, 50): e = 50 + 50 = 100, f = 50 - 50 = 0.
    assert!(
        svg.contains("<g transform=\"matrix(0 1 -1 0 100 0)\">\n<rect x=\"20\" y=\"40\" width=\"60\" height=\"20\""),
        "{svg}"
    );
    assert_eq!(
        svg.matches("<g transform").count(),
        1,
        "containers get no group"
    );
    let parsed = roxmltree::Document::parse(&svg).expect("well-formed SVG");
    let g = parsed
        .descendants()
        .find(|n| n.has_tag_name("g"))
        .expect("group");
    assert_eq!(g.children().filter(|c| c.is_element()).count(), 1);
}

#[cfg(feature = "pdf")]
#[test]
fn pdf_applies_the_transform_to_rotated_nodes() {
    let doc = Document::from_toml(&src(
        100.0,
        100.0,
        &format!(
            "{RED_BAR}rotate = 90
"
        ),
    ))
    .unwrap();
    let l = layout(&doc, &NullMeasurer);
    let pdf = figs_core::render_pdf(&l, &RenderOptions::default()).unwrap();
    let content = page_content(&pdf);
    // krilla folds its own y-flip (`1 0 0 -1 0 100`) into our matrix
    // `0 1 -1 0 100 0`, then draws the unrotated rect inside a q/Q pair.
    let rotated = "q 0 -1 -1 0 100 100 cm 1 0 0 rg 20 40 m 80 40 l 80 60 l 20 60 l";
    assert!(content.contains(rotated), "{content}");

    // Unrotated: only the flip.
    let doc = Document::from_toml(&src(100.0, 100.0, RED_BAR)).unwrap();
    let pdf =
        figs_core::render_pdf(&layout(&doc, &NullMeasurer), &RenderOptions::default()).unwrap();
    let content = page_content(&pdf);
    assert!(!content.contains("0 -1 -1 0"), "{content}");
    assert!(content.contains("q 1 0 0 -1 0 100 cm 1 0 0 rg 20 40 m"));
}

/// Every stream of the PDF as text, inflated when Flate-compressed (krilla
/// compresses page content).
#[cfg(feature = "pdf")]
fn page_content(pdf: &[u8]) -> String {
    let mut out = String::new();
    let mut rest = pdf;
    while let Some(i) = rest.windows(6).position(|w| w == b"stream") {
        let head = String::from_utf8_lossy(&rest[i.saturating_sub(200)..i]).into_owned();
        let mut body = &rest[i + 6..];
        if body.starts_with(b"\r\n") {
            body = &body[2..];
        } else if body.starts_with(b"\n") {
            body = &body[1..];
        }
        let end = body
            .windows(9)
            .position(|w| w == b"endstream")
            .unwrap_or(body.len());
        let data = &body[..end];
        if !head.contains("FlateDecode") {
            out.push_str(&String::from_utf8_lossy(data));
        } else if let Ok(d) = miniz_oxide::inflate::decompress_to_vec_zlib(data) {
            out.push_str(&String::from_utf8_lossy(&d));
        }
        out.push('\n');
        rest = &body[end..];
    }
    out
}

// ---------------------------------------------------------------------------
// schema, round trip, script
// ---------------------------------------------------------------------------

#[test]
fn rotate_round_trips_through_toml() {
    use figs_core::schema::parse::parse_str;
    let text = src(
        100.0,
        100.0,
        r#"
        [nodes.root]
        type = "row"
        children = ["a"]
        [nodes.a]
        type = "text"
        content = "y"
        rotate = -90
        "#,
    );
    let raw = parse_str(&text).unwrap();
    assert_eq!(raw.nodes["a"].rotate, Some(-90.0));
    assert_eq!(raw.nodes["root"].rotate, None);
    let toml_out = toml::to_string(&raw.nodes["a"]).unwrap();
    assert!(toml_out.contains("rotate = -90"), "{toml_out}");
    assert_eq!(
        toml_out.matches("rotate").count(),
        1,
        "unset rotate is omitted"
    );
    let again: figs_core::schema::RawNode = toml::from_str(&toml_out).unwrap();
    assert_eq!(again.rotate, Some(-90.0));
}

#[test]
fn non_finite_rotate_is_rejected() {
    let text = src(
        100.0,
        100.0,
        "[nodes.root]\ntype = \"rect\"\nrotate = nan\n",
    );
    let err = Document::from_toml(&text).unwrap_err().to_string();
    assert!(err.contains("rotate"), "{err}");
}

#[cfg(feature = "script")]
#[test]
fn script_rotate_setter() {
    let raw = figs_core::eval_script(
        r#"page(100, 60, row([text("Intensity").rotate(-90), rect().flex(1).rotate(12.5)])).unit("pt")"#,
    )
    .unwrap();
    let mut rotations: Vec<Option<f32>> = raw.nodes.values().map(|n| n.rotate).collect();
    rotations.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(rotations, vec![None, Some(-90.0), Some(12.5)]);
    let err = figs_core::eval_script(r#"page(1, 1, rect().rotate("x"))"#).unwrap_err();
    assert!(err.to_string().contains("rotate"), "{err}");
}

#[cfg(feature = "script")]
#[test]
fn rotation_example_lays_out() {
    let src = include_str!("../../../examples/rotation.figs");
    let doc = Document::from_script(src).unwrap_or_else(|e| panic!("example failed: {e}"));
    let l = layout(&doc, &NullMeasurer);
    let turned = l.nodes.iter().filter(|n| n.transform.is_some()).count();
    // y label, tilted note, badge (+ its two children), side caption.
    assert!(turned >= 6, "expected rotated nodes, found {turned}");
}
