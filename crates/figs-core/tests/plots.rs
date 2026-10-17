//! Data plots: the core `viewbox` (user coordinates on stacks and shapes) and
//! `anchor` (which point of a stack child sits at its offset) that plots are
//! built on, the prelude's tick generation and `plot` layout, `csv()` loading,
//! and rendering the bundled `examples/plot.figs`.

#![cfg(feature = "script")]

use std::path::{Path, PathBuf};

use figs_core::geom::{PathSegment, Point, Rect, Size};
use figs_core::layout::{ComputedLayout, PaintContent};
use figs_core::schema::{ImageProps, NodeKind, RawDocument, TextProps};
use figs_core::{
    eval_script, eval_script_in, layout, render_png, render_svg, Document, FontStore, LeafMeasure,
    NullMeasurer, RenderOptions,
};

fn lay_toml(src: &str) -> ComputedLayout {
    let d = Document::from_toml(src).unwrap_or_else(|e| panic!("resolve failed: {e}"));
    layout(&d, &NullMeasurer)
}

fn lay_script(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &NullMeasurer)
}

fn rect_of(l: &ComputedLayout, id: &str) -> Rect {
    l.nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("node `{id}` not in layout"))
        .rect
}

fn segs_of<'a>(l: &'a ComputedLayout, id: &str) -> &'a [PathSegment] {
    match &l.nodes.iter().find(|n| n.id == id).unwrap().content {
        PaintContent::Path(p) => &p.segments,
        other => panic!("`{id}` is not a path: {other:?}"),
    }
}

#[track_caller]
fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-3, "expected {b}, got {a}");
}

#[track_caller]
fn assert_rect(r: Rect, x: f32, y: f32, w: f32, h: f32) {
    let ok = [(r.x, x), (r.y, y), (r.w, w), (r.h, h)]
        .iter()
        .all(|(a, b)| (a - b).abs() < 1e-3);
    assert!(ok, "expected ({x}, {y}, {w}, {h}), got {r:?}");
}

// ---- core: viewbox + anchor ------------------------------------------------

/// A 100 x 50 pt stack with user coordinates x 0..10 (left to right), y 0..1
/// bottom to top (reversed, as in a plot).
const VIEWBOX_STACK: &str = r##"
    [page]
    width = 100
    height = 50
    unit = "pt"
    root = "s"
    [nodes.s]
    type = "stack"
    viewbox = [0, 1, 10, 0]
    children = ["dot", "edge", "curve", "plain"]
    [nodes.dot]
    type = "rect"
    width = 4
    height = 2
    x = 5
    y = 0.5
    anchor = [0.5, 0.5]
    [nodes.edge]
    type = "rect"
    width = 6
    height = 6
    x = 10
    y = 0
    [nodes.curve]
    type = "path"
    viewbox = [0, 1, 10, 0]
    points = [[0, 0], [5, 1]]
    segments = [{ quad = [7.5, 1], to = [10, 0.5] }]
    [nodes.plain]
    type = "line"
    points = [[0, 0], [3, 4]]
    x = 2.5
    y = 1
"##;

#[test]
fn viewbox_stack_maps_child_offsets_and_anchor_centres() {
    let l = lay_toml(VIEWBOX_STACK);
    // (5, 0.5) -> (50, 25); a 4 x 2 box centred on it.
    assert_rect(rect_of(&l, "dot"), 48.0, 24.0, 4.0, 2.0);
    // (10, 0) is the bottom-right corner. Children of a viewbox stack are not
    // clamped to the space after their offset: the box keeps its size.
    assert_rect(rect_of(&l, "edge"), 100.0, 50.0, 6.0, 6.0);
    // Offsets map, sizes don't: the line's box and points stay in points.
    assert_rect(rect_of(&l, "plain"), 25.0, 0.0, 3.0, 4.0);
    assert_eq!(
        segs_of(&l, "plain")[1],
        PathSegment::LineTo(Point::new(28.0, 4.0))
    );
}

#[test]
fn viewbox_path_maps_points_and_control_points() {
    let l = lay_toml(VIEWBOX_STACK);
    // An unpositioned path fills the stack; its viewbox maps onto that box.
    assert_rect(rect_of(&l, "curve"), 0.0, 0.0, 100.0, 50.0);
    let s = segs_of(&l, "curve");
    assert_eq!(s[0], PathSegment::MoveTo(Point::new(0.0, 50.0)));
    assert_eq!(s[1], PathSegment::LineTo(Point::new(50.0, 0.0)));
    assert_eq!(
        s[2],
        PathSegment::QuadTo(Point::new(75.0, 0.0), Point::new(100.0, 25.0))
    );
}

#[test]
fn viewbox_follows_the_final_box_size() {
    // The same path in a smaller box maps to that box (layout-time mapping).
    let src = VIEWBOX_STACK.replace(
        "width = 100\n    height = 50",
        "width = 20\n    height = 10",
    );
    let l = lay_toml(&src);
    let s = segs_of(&l, "curve");
    assert_eq!(s[1], PathSegment::LineTo(Point::new(10.0, 0.0)));
    assert_rect(rect_of(&l, "dot"), 8.0, 4.0, 4.0, 2.0);
}

#[test]
fn viewbox_units_are_not_page_units() {
    // In cm, offsets and points under a viewbox are user units (unscaled),
    // while sizes and strokes are still cm.
    let src = r##"
        [page]
        width = 10
        height = 10
        unit = "cm"
        root = "s"
        [nodes.s]
        type = "stack"
        viewbox = [0, 0, 1, 1]
        children = ["d", "p"]
        [nodes.d]
        type = "rect"
        width = 1
        height = 1
        x = 0.5
        y = 0.5
        [nodes.p]
        type = "path"
        viewbox = [0, 0, 1, 1]
        points = [[0, 0], [1, 1]]
        stroke_width = 0.1
    "##;
    let l = lay_toml(src);
    let cm = 72.0 / 2.54;
    assert_rect(rect_of(&l, "d"), 5.0 * cm, 5.0 * cm, cm, cm);
    let end = segs_of(&l, "p")[1];
    let PathSegment::LineTo(p) = end else {
        panic!()
    };
    approx(p.x, 10.0 * cm);
    approx(p.y, 10.0 * cm);
    match &l.nodes.iter().find(|n| n.id == "p").unwrap().content {
        PaintContent::Path(pp) => approx(pp.stroke.as_ref().unwrap().width, 0.1 * cm),
        _ => unreachable!(),
    }
}

#[test]
fn anchor_in_a_plain_stack() {
    // Without a viewbox offsets are page units; the anchor shifts by the
    // child's own size, and the stack's wrap-content extent accounts for it.
    let src = r##"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "c"
        [nodes.c]
        type = "column"
        children = ["s"]
        [nodes.s]
        type = "stack"
        children = ["a"]
        [nodes.a]
        type = "rect"
        width = 10
        height = 4
        x = 20
        y = 10
        anchor = [1, 0.5]
    "##;
    let l = lay_toml(src);
    assert_rect(rect_of(&l, "a"), 10.0, 8.0, 10.0, 4.0);
    // Extent: x 20 - 10 + 10 = 20 wide, y 10 - 2 + 4 = 12 tall.
    assert_rect(rect_of(&l, "s"), 0.0, 0.0, 20.0, 12.0);
}

#[test]
fn viewbox_stack_wraps_its_largest_child() {
    // Offsets are meaningless before the box is known, so a viewbox stack's
    // intrinsic size ignores them.
    let src = r##"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "c"
        [nodes.c]
        type = "column"
        children = ["s"]
        [nodes.s]
        type = "stack"
        viewbox = [0, 0, 1, 1]
        children = ["a", "b"]
        [nodes.a]
        type = "rect"
        width = 12
        height = 3
        x = 0.9
        y = 0.9
        [nodes.b]
        type = "rect"
        width = 5
        height = 7
        x = 0.1
        y = 0.1
    "##;
    let l = lay_toml(src);
    assert_rect(rect_of(&l, "s"), 0.0, 0.0, 12.0, 7.0);
}

#[test]
fn degenerate_or_bad_viewbox_and_anchor_are_rejected() {
    let base = |extra: &str| {
        format!(
            "[page]\nwidth = 1\nheight = 1\nroot = \"s\"\n[nodes.s]\ntype = \"stack\"\n{extra}\n"
        )
    };
    let e = Document::from_toml(&base("viewbox = [0, 0, 0, 1]")).unwrap_err();
    assert!(e.to_string().contains("invalid `viewbox`"), "{e}");
    let e = Document::from_toml(&base("viewbox = [0, 0, 1]")).unwrap_err();
    assert!(!e.to_string().is_empty());
    let e = eval_script("page(1, 1, stack([]).viewbox(0, 1, 1, 1))").unwrap_err();
    assert!(e.message.contains("degenerate"), "{}", e.message);
    let e = eval_script("page(1, 1, rect().viewbox(0, 0, 1, 1))").unwrap_err();
    assert!(e.message.contains("stack/path/line/arrow"), "{}", e.message);
    let e = eval_script(r#"page(1, 1, rect().anchor("a", 0))"#).unwrap_err();
    assert!(e.message.contains("anchor"), "{}", e.message);
}

#[test]
fn script_setters_lower_and_round_trip_through_toml() {
    let raw = eval_script(
        "page(10, 10, stack([ rect().at(5, 0.5).anchor(0.5, 1), \
         path([[0, 0], [1, 1]]).viewbox(0, 1, 1, 0) ]).viewbox(0, 1, 10, 0))",
    )
    .unwrap();
    assert_eq!(raw.nodes["root"].viewbox, Some([0.0, 1.0, 10.0, 0.0]));
    let rect = raw
        .nodes
        .values()
        .find(|n| n.kind == NodeKind::Rect)
        .unwrap();
    assert_eq!(rect.anchor, Some([0.5.into(), 1.0.into()]));

    let toml = toml::to_string(&raw).unwrap();
    assert!(toml.contains("viewbox = [0.0, 1.0, 10.0, 0.0]"), "{toml}");
    assert!(toml.contains("anchor = [0.5, 1.0]"), "{toml}");
    let back: RawDocument = toml::from_str(&toml).unwrap();
    assert_eq!(back.nodes["root"].viewbox, raw.nodes["root"].viewbox);
    // Nodes without them serialize without the keys.
    let plain = toml::to_string(&eval_script("page(1, 1, rect())").unwrap()).unwrap();
    assert!(!plain.contains("viewbox") && !plain.contains("anchor"));
}

// ---- core: contain -----------------------------------------------------------

#[test]
fn contain_insets_a_viewbox_stack_by_its_childrens_overhang() {
    // A 100 x 50 pt stack, x 0..10, y 0..1 downwards. "r" is centred on the
    // right edge and hangs below it (5 pt right, 4 pt down); "l" is
    // right-aligned to the left edge (6 pt left). `contain` shrinks the
    // content box to x 6..95, y 0..46, so both end up inside the stack.
    let src = r##"
        [page]
        width = 100
        height = 50
        unit = "pt"
        root = "s"
        [nodes.s]
        type = "stack"
        viewbox = [0, 0, 10, 1]
        contain = true
        children = ["fill", "r", "l"]
        [nodes.fill]
        type = "path"
        viewbox = [0, 0, 10, 1]
        points = [[0, 0], [10, 1]]
        [nodes.r]
        type = "rect"
        width = 10
        height = 4
        x = 10
        y = 1
        anchor = [0.5, 0]
        [nodes.l]
        type = "rect"
        width = 6
        height = 2
        x = 0
        y = 0.5
        anchor = [1, 0.5]
    "##;
    let l = lay_toml(src);
    assert_rect(rect_of(&l, "s"), 0.0, 0.0, 100.0, 50.0);
    // The viewbox maps onto the inset box: filling children and offsets follow.
    assert_rect(rect_of(&l, "fill"), 6.0, 0.0, 89.0, 46.0);
    assert_eq!(
        segs_of(&l, "fill")[0],
        PathSegment::MoveTo(Point::new(6.0, 0.0))
    );
    assert_rect(rect_of(&l, "r"), 90.0, 46.0, 10.0, 4.0);
    assert_rect(rect_of(&l, "l"), 0.0, 22.0, 6.0, 2.0);

    // Without `contain` the same children hang out of the box.
    let l = lay_toml(&src.replace("contain = true\n", ""));
    assert_rect(rect_of(&l, "fill"), 0.0, 0.0, 100.0, 50.0);
    assert_rect(rect_of(&l, "r"), 95.0, 50.0, 10.0, 4.0);
}

#[test]
fn contain_counts_descendants_and_respects_padding() {
    // The overhang comes from a grandchild (a label centred on the right end
    // of a filling row), and the limit is the padding box.
    let l = lay_script(
        "page(100, 50, stack([ \
            row([ stack([ rect().width(8).height(3).at(1, 0).anchor(0.5, 0) ]).viewbox(0, 0, 1, 1).flex(1) ]) \
                .stretch().y(0), \
        ]).viewbox(0, 0, 1, 1).contain().pad(2)).unit(\"pt\")",
    );
    let r = l
        .nodes
        .iter()
        .find(|n| matches!(n.content, PaintContent::Rect { .. }))
        .unwrap()
        .rect;
    // Content box 2..98 shrinks by the 4 pt overhang: the rect's right edge
    // lands on the padding edge.
    approx(r.x + r.w, 98.0);
    approx(r.x, 90.0);
}

#[test]
fn contain_measures_rotated_children_by_their_turned_box() {
    // A 20 x 4 rect turned 90° covers 4 x 20 on the page: hung off the right
    // edge, it costs the content box 4 pt, not 20.
    let l = lay_script(
        "page(100, 50, stack([ rect().width(20).height(4).rotate(90).at(10, 0) ]) \
         .viewbox(0, 0, 10, 1).contain()).unit(\"pt\")",
    );
    let n = l
        .nodes
        .iter()
        .find(|n| matches!(n.content, PaintContent::Rect { .. }))
        .unwrap();
    assert_rect(n.bounds(), 96.0, 0.0, 4.0, 20.0);
}

#[test]
fn contain_on_a_plain_stack_makes_room_for_negative_offsets() {
    // A plain stack measures the part hanging off its top-left too, and
    // shifts its content so it stays inside.
    let src = r##"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "c"
        [nodes.c]
        type = "column"
        children = ["s"]
        [nodes.s]
        type = "stack"
        contain = true
        children = ["a"]
        [nodes.a]
        type = "rect"
        width = 10
        height = 10
        x = -5
        y = -3
    "##;
    let l = lay_toml(src);
    assert_rect(rect_of(&l, "s"), 0.0, 0.0, 10.0, 10.0);
    assert_rect(rect_of(&l, "a"), 0.0, 0.0, 10.0, 10.0);
}

#[test]
fn contain_round_trips_and_is_stack_only() {
    let raw = eval_script("page(1, 1, stack([]).contain())").unwrap();
    assert!(raw.nodes["root"].contain);
    let toml = toml::to_string(&raw).unwrap();
    assert!(toml.contains("contain = true"), "{toml}");
    let back: RawDocument = toml::from_str(&toml).unwrap();
    assert!(back.nodes["root"].contain);
    let plain = toml::to_string(&eval_script("page(1, 1, stack([]))").unwrap()).unwrap();
    assert!(!plain.contains("contain"), "{plain}");

    let e = eval_script("page(1, 1, rect().contain())").unwrap_err();
    assert!(
        e.message.contains("`contain` applies to a stack"),
        "{}",
        e.message
    );
    let e = Document::from_toml(
        "[page]\nwidth = 1\nheight = 1\nroot = \"r\"\n[nodes.r]\ntype = \"rect\"\ncontain = true\n",
    )
    .unwrap_err();
    assert!(e.to_string().contains("contain"), "{e}");
}

// ---- prelude: ticks and number formatting ------------------------------------

/// Evaluate a Rhai expression (with the prelude) to its string form.
fn rhai(expr: &str) -> String {
    let raw = eval_script(&format!("page(1, 1, text(`${{{expr}}}`))"))
        .unwrap_or_else(|e| panic!("`{expr}` failed: {e}"));
    raw.nodes["root"].content.clone().unwrap()
}

/// `[a, b, step]` of `nice_axis`, rounded for comparison.
fn axis(lo: f64, hi: f64, snap: bool) -> [f64; 3] {
    let s = rhai(&format!("nice_axis({lo:?}, {hi:?}, {snap})"));
    let v: Vec<f64> = s
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|x| (x.trim().parse::<f64>().unwrap() * 1e9).round() / 1e9)
        .collect();
    v.try_into().unwrap()
}

#[test]
fn nice_axis_picks_1_2_5_steps_within_max_ticks() {
    // Snapped (automatic) ranges grow to whole steps.
    assert_eq!(axis(0.0, 1.0, true), [0.0, 1.0, 0.2]);
    assert_eq!(axis(0.05, 1.06, true), [0.0, 1.2, 0.2]); // 7 ticks: at the limit
    assert_eq!(axis(0.0, 7.0, true), [0.0, 8.0, 2.0]); // 1 would give 8 ticks
    assert_eq!(axis(0.0, 12345.0, true), [0.0, 15000.0, 5000.0]);
    assert_eq!(axis(-0.0042, 0.0071, true), [-0.005, 0.01, 0.005]);
    assert_eq!(axis(-3.0, 5.0, true), [-4.0, 6.0, 2.0]);
    assert_eq!(axis(98.0, 102.0, true), [98.0, 102.0, 1.0]);
    // Fixed ranges are kept; only the step is chosen.
    assert_eq!(axis(0.0, 1.0, false), [0.0, 1.0, 0.2]);
    assert_eq!(axis(-1.0, 3.5, false), [-1.0, 3.5, 1.0]);
}

#[test]
fn nice_ticks_are_the_step_multiples_inside() {
    assert_eq!(
        rhai("nice_ticks(-1.0, 3.5, 1.0)"),
        "[-1.0, 0.0, 1.0, 2.0, 3.0]"
    );
    assert_eq!(rhai("nice_ticks(0.0, 0.3, 0.1).len()"), "4"); // 0.3/0.1 rounding
                                                              // A user-defined `max_ticks()` hook changes the choice.
    let raw =
        eval_script("fn max_ticks() { 3 }\npage(1, 1, text(`${nice_axis(0, 1, true)}`))").unwrap();
    assert_eq!(
        raw.nodes["root"].content.as_deref(),
        Some("[0.0, 1.0, 0.5]")
    );
}

#[test]
fn tick_labels_format_with_shared_decimals_and_real_minus() {
    assert_eq!(rhai("format_num(-0.5, 1)"), "−0.5");
    assert_eq!(rhai("format_num(0.30000000000000004, 1)"), "0.3");
    assert_eq!(rhai("format_num(-0.0001, 2)"), "0.00"); // no "−0.00"
    assert_eq!(rhai("format_num(1500.0, 0)"), "1500");
    assert_eq!(rhai("format_num(0.004, 3)"), "0.004");
    assert_eq!(rhai("tick_decimals([0.0, 0.25, 0.5])"), "2");
    assert_eq!(rhai("tick_decimals([0.0, 0.2, 0.4000000000000001])"), "1");
    assert_eq!(rhai("tick_decimals([0, 10, 20])"), "0");
    assert_eq!(rhai("log_label(-5, false)"), "10⁻⁵");
    assert_eq!(rhai("log_label(-2, true)"), "0.01");
    assert_eq!(rhai("log_label(3, true)"), "1000");
    assert_eq!(rhai("superscript(12)"), "¹²");
}

#[test]
fn plot_axis_edge_cases() {
    // Constant data opens up a range around the value; no data gives 0..1.
    let ax = rhai("plot_axis(\"y\", 3.0, 3.0, (), false, (), [])");
    assert!(
        ax.contains("\"lo\": 1.5") && ax.contains("\"hi\": 4.5"),
        "{ax}"
    );
    let ax = rhai("plot_axis(\"y\", (), (), (), false, (), [])");
    assert!(
        ax.contains("\"lo\": 0.0") && ax.contains("\"hi\": 1.0"),
        "{ax}"
    );
    // Log axes span whole decades with labels in one style.
    let ax = rhai("plot_axis(\"y\", -2.5, 1.2, (), true, (), [])");
    assert!(
        ax.contains("\"lo\": -3.0") && ax.contains("\"hi\": 2.0"),
        "{ax}"
    );
    assert!(ax.contains("10⁻³") && ax.contains("10²"), "{ax}");
    // Categories: one slot each, labels are the names, no tick marks.
    let ax = rhai("plot_axis(\"x\", (), (), (), false, (), [\"a\", \"b\"])");
    assert!(
        ax.contains("\"lo\": -0.5")
            && ax.contains("\"hi\": 1.5")
            && ax.contains("\"marks\": false")
    );
    // Explicit ticks are used as given.
    let ax = rhai("plot_axis(\"x\", 0.0, 10.0, (), false, [0, 5, 10], [])");
    assert!(ax.contains("\"labels\": [\"0\", \"5\", \"10\"]"), "{ax}");
}

// ---- prelude: plot layout ----------------------------------------------------

fn find_raw(raw: &RawDocument, f: impl Fn(&figs_core::schema::RawNode) -> bool) -> &str {
    raw.nodes
        .iter()
        .find(|(_, n)| f(n))
        .map(|(id, _)| id.as_str())
        .expect("node not found")
}

#[test]
fn plot_maps_data_to_the_area_at_layout_time() {
    let src = r#"page(12, 8, plot(#{ x: [0, 10], y: [0, 1] }, [
        scatter([5, 10], [0.5, 1]),
        line_plot([0, 10], [0, 1]),
    ]).flex(1)).unit("cm")"#;
    let raw = eval_script(src).unwrap();
    let doc = figs_core::schema::resolve::resolve(raw.clone()).unwrap();
    let l = layout(&doc, &NullMeasurer);

    // The data area: the stack whose viewbox is [x0, y1, x1, y0] (not the
    // `contain` stack around it, which shares the viewbox to place labels).
    let area_id = find_raw(&raw, |n| {
        n.kind == NodeKind::Stack && n.viewbox == Some([0.0, 1.0, 10.0, 0.0]) && !n.contain
    });
    let area = rect_of(&l, area_id);
    assert!(area.w > 100.0 && area.h > 50.0, "area too small: {area:?}");

    // Markers are centred on their data points.
    let markers: Vec<Rect> = l
        .nodes
        .iter()
        .filter(|n| matches!(n.content, PaintContent::Ellipse { .. }))
        .map(|n| n.rect)
        .collect();
    assert_eq!(markers.len(), 2);
    approx(markers[0].x + markers[0].w / 2.0, area.x + area.w / 2.0);
    approx(markers[0].y + markers[0].h / 2.0, area.y + area.h / 2.0);
    approx(markers[1].x + markers[1].w / 2.0, area.x + area.w);
    approx(markers[1].y + markers[1].h / 2.0, area.y);

    // The data line runs corner to corner of the area.
    let line_id = find_raw(&raw, |n| {
        n.kind == NodeKind::Path && n.points == vec![[0.0, 0.0], [10.0, 1.0]]
    });
    let s = segs_of(&l, line_id);
    assert_eq!(
        s[0],
        PathSegment::MoveTo(Point::new(area.x, area.y + area.h))
    );
    let PathSegment::LineTo(end) = s[1] else {
        panic!()
    };
    approx(end.x, area.x + area.w);
    approx(end.y, area.y);
}

#[test]
fn plot_ticks_labels_and_legend() {
    let raw = eval_script(
        r#"page("single", plot(#{ xlabel: "Time (s)", ylabel: "Signal" }, [
            line_plot([0, 1, 2, 3], [0.1, 0.4, 0.2, 0.9], "a"),
            scatter([0, 1, 2, 3], [0.3, 0.3, 0.5, 0.6], "b"),
        ]))"#,
    )
    .unwrap();
    let texts: Vec<&str> = raw
        .nodes
        .values()
        .filter_map(|n| n.content.as_deref())
        .collect();
    for t in [
        "Time (s)", "Signal", "0", "1", "2", "3", "0.0", "0.2", "1.0", "a", "b",
    ] {
        assert!(texts.contains(&t), "missing text {t:?} in {texts:?}");
    }
    // Colours follow palette order.
    let strokes: Vec<String> = raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Path && n.viewbox.is_some() && n.stroke.is_some())
        .map(|n| n.stroke.unwrap().to_hex())
        .collect();
    assert!(
        strokes.iter().any(|c| c.eq_ignore_ascii_case("#0072b2")),
        "{strokes:?}"
    );
    // The whole plot is 4:3 by default, so it has a height on an auto page.
    let d = figs_core::schema::resolve::resolve(raw).unwrap();
    let l = layout(&d, &NullMeasurer);
    approx(l.page.height_pt, l.page.width_pt * 3.0 / 4.0);
}

#[test]
fn plot_option_errors_point_at_the_call() {
    let e = eval_script("let s = [line_plot([1], [1])];\npage(1, 1, plot(#{ xlim: 1 }, s))")
        .unwrap_err();
    assert_eq!(e.line, Some(2), "{}", e.message);
    assert!(
        e.message.contains("plot: unknown option \"xlim\""),
        "{}",
        e.message
    );
    let e = eval_script("page(1, 1, plot([scatter([1, 2], [1])]))").unwrap_err();
    assert!(
        e.message.contains("x and y differ in length"),
        "{}",
        e.message
    );
    let e = eval_script(r#"page(1, 1, plot(#{ yscale: "log", y: [0, 1] }, [scatter([1], [1])]))"#)
        .unwrap_err();
    assert!(e.message.contains("must be positive"), "{}", e.message);
    let e =
        eval_script(r#"page(1, 1, plot([line_plot([1], [1], #{ marker: "x" })]))"#).unwrap_err();
    assert!(e.message.contains("unknown marker"), "{}", e.message);
}

// ---- prelude: measured plot margins ------------------------------------------

/// Text measures `CHAR_W` pt per character by `LINE_H` pt, so every plot
/// margin is predictable from the label strings alone.
struct PerChar;
const CHAR_W: f32 = 5.0;
const LINE_H: f32 = 10.0;
impl LeafMeasure for PerChar {
    fn measure_text(&self, t: &TextProps, _max_width: f32) -> Size {
        Size::new(CHAR_W * t.content.chars().count() as f32, LINE_H)
    }
    fn measure_image(&self, _i: &ImageProps) -> Size {
        Size::default()
    }
}

struct Plotted {
    raw: RawDocument,
    l: ComputedLayout,
}

impl Plotted {
    fn new(src: &str) -> Self {
        let raw = eval_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
        let doc = figs_core::schema::resolve::resolve(raw.clone()).unwrap();
        let l = layout(&doc, &PerChar);
        Plotted { raw, l }
    }
    /// Boxes of the text nodes reading `s`, in paint order.
    fn texts(&self, s: &str) -> Vec<Rect> {
        self.l
            .nodes
            .iter()
            .filter(|n| self.raw.nodes[&n.id].content.as_deref() == Some(s))
            .map(|n| n.rect)
            .collect()
    }
    fn text(&self, s: &str) -> Rect {
        let t = self.texts(s);
        assert_eq!(t.len(), 1, "expected one text {s:?}, got {t:?}");
        t[0]
    }
    /// The data area: the plain stack sharing the `contain` stack's viewbox.
    fn area(&self) -> Rect {
        let body = find_raw(&self.raw, |n| n.contain);
        let vb = self.raw.nodes[body].viewbox;
        assert!(vb.is_some());
        let id = find_raw(&self.raw, |n| {
            n.kind == NodeKind::Stack && n.viewbox == vb && !n.contain
        });
        rect_of(&self.l, id)
    }
}

/// A prelude hook's value, cm -> pt.
fn hook_pt(name: &str) -> f32 {
    rhai(&format!("{name}()")).parse::<f32>().unwrap() * 72.0 / 2.54
}

#[test]
fn plot_margins_are_the_measured_label_sizes() {
    let p = Plotted::new(
        // The horizontal y label (above the axis) of `ylabel_rotated()` false.
        r#"fn ylabel_rotated() { false }
        page(12, 9, plot(#{ x: [0, 10], y: [0, 1], xlabel: "Time", ylabel: "Signal" },
            [line_plot([0, 10], [0, 1])]).flex(1)).unit("cm")"#,
    );
    let panel = rect_of(&p.l, "root");
    let area = p.area();
    let (pp, tl) = (hook_pt("plot_pad"), hook_pt("tick_length"));
    let gap = 8.0 * 0.012 * 72.0 / 2.54; // tick -> label, base_size() * 0.012 cm
    let lw = 3.0 * CHAR_W; // "0.0" … "1.0"

    // Left band: the widest y tick label, then the tick room.
    approx(area.x - (panel.x + pp), lw + tl + gap);
    for s in ["0.0", "0.2", "0.4", "0.6", "0.8", "1.0"] {
        let r = p.text(s);
        approx(r.x + r.w, area.x - tl - gap);
        assert!(r.x >= panel.x + pp - 1e-3, "{s} sticks out left: {r:?}");
    }
    // Right: exactly half the last x tick label, centred on the axis end.
    let last = p.text("10");
    approx(last.x + last.w / 2.0, area.x + area.w);
    approx(last.x + last.w, panel.x + panel.w - pp);
    // Bottom: tick room, one x tick label line, a gap, the x label line.
    approx(
        panel.y + panel.h - pp - (area.y + area.h),
        tl + gap + LINE_H + gap + LINE_H,
    );
    let xl = p.text("Time");
    approx(xl.x + xl.w / 2.0, area.x + area.w / 2.0); // centred under the area
    approx(xl.y + xl.h, panel.y + panel.h - pp);
    // Top: the y label (a standalone plot keeps no panel-label room), its
    // bottom half a line above the axis (clear of the top tick label's
    // half-line overhang).
    let yl = p.text("Signal");
    approx(yl.y, panel.y + pp);
    approx(yl.x, area.x);
    approx(area.y - (yl.y + yl.h), LINE_H / 2.0);
    let top = p.text("1.0");
    assert!(top.y >= yl.y + yl.h - 1e-3, "{top:?} overlaps {yl:?}");
}

#[test]
fn wide_end_labels_widen_their_band() {
    // Narrow y labels ("0", "5") but a 5-character first x label: its half
    // (12.5 pt) overhanging the axis start is wider than the y tick band, so
    // it decides the left margin; the last one the right margin.
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [10000, 20000], y: [0, 5], yticks: [0, 5] },
            [line_plot([10000, 20000], [0, 5])]).flex(1)).unit("cm")"#,
    );
    let panel = rect_of(&p.l, "root");
    let pp = hook_pt("plot_pad");
    let area = p.area();
    let first = p.text("10000");
    approx(first.x, panel.x + pp);
    approx(first.x + first.w / 2.0, area.x);
    let last = p.text("20000");
    approx(last.x + last.w, panel.x + panel.w - pp);
    // The y labels still sit right next to their ticks, inside the panel.
    let tl = hook_pt("tick_length");
    let gap = 8.0 * 0.012 * 72.0 / 2.54;
    approx(p.text("5").x + CHAR_W, area.x - tl - gap);
}

#[test]
fn category_labels_fill_their_slots() {
    let p = Plotted::new(
        r#"page(12, 9, plot(#{}, [bars(["a", "bb", "ccc"], [1, 2, 3])]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let slot = area.w / 3.0;
    for (i, s) in ["a", "bb", "ccc"].iter().enumerate() {
        let r = p.text(s);
        // Each label is a cell of its slot (less a small gap), centred on it.
        approx(r.x + r.w / 2.0, area.x + slot * (i as f32 + 0.5));
        assert!(r.w <= slot && r.w > slot * 0.9, "{s}: {r:?} vs slot {slot}");
    }
}

#[test]
fn rotated_y_label_sits_left_of_the_tick_labels() {
    // The default: the y label turned -90°, its footprint one line wide and
    // as long as the axis, left of the tick labels.
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [0, 10], y: [0, 1], ylabel: "Signal" },
            [line_plot([0, 10], [0, 1])]).flex(1)).unit("cm")"#,
    );
    let panel = rect_of(&p.l, "root");
    let area = p.area();
    let (pp, tl) = (hook_pt("plot_pad"), hook_pt("tick_length"));
    let gap = 8.0 * 0.012 * 72.0 / 2.54;
    let node =
        p.l.nodes
            .iter()
            .find(|n| p.raw.nodes[&n.id].content.as_deref() == Some("Signal"))
            .unwrap();
    assert!(node.transform.is_some(), "y label is not rotated");
    let yl = node.bounds();
    assert_rect(yl, panel.x + pp, area.y, LINE_H, area.h);
    // Then a gap, the widest tick label ("0.0"), the tick room.
    approx(area.x - (yl.x + yl.w), 2.0 * gap + 3.0 * CHAR_W + tl + gap);
    let tick = p.text("0.4");
    assert!(yl.x + yl.w < tick.x, "{yl:?} vs {tick:?}");
    // Nothing above the axis but the top tick label's half-line overhang.
    approx(area.y, panel.y + pp + LINE_H / 2.0);
}

// ---- csv() -------------------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("figs-plots-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn csv_columns_resolve_against_the_script_dir() {
    let dir = scratch("csv");
    std::fs::write(dir.join("d.csv"), "t,v,name\n0,1.5,a\n1,,b\n").unwrap();
    let src = r#"let d = csv("d.csv");
        page(1, 1, text(`${d.t}|${d.v[0]}|${d.v[1]}|${d.name}|${type_of(d.t[0])}`))"#;
    let raw = eval_script_in(src, Some(&dir)).unwrap();
    assert_eq!(
        raw.nodes["root"].content.as_deref(),
        Some("[0.0, 1.0]|1.5|NaN|[\"a\", \"b\"]|f64")
    );
    // Missing values are gaps in plots, not errors.
    let raw = eval_script_in(
        r#"let d = csv("d.csv"); page("single", plot([line_plot(d.t, d.v)]))"#,
        Some(&dir),
    );
    assert!(raw.is_ok(), "{:?}", raw.err());
}

#[test]
fn csv_files_are_watch_dependencies() {
    let dir = scratch("csv-deps");
    std::fs::write(dir.join("d.csv"), "t,v\n0,1\n").unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub").join("e.csv"), "t,v\n0,1\n").unwrap();
    let src = r#"let d = csv("d.csv");
        let e = csv("sub/e.csv");
        page("single", plot([line_plot(d.t, d.v), scatter(e.t, e.v)]))"#;
    let doc = Document::from_script_in(src, Some(&dir)).unwrap();
    let deps = doc.dependencies(&dir);
    let abs = |p: &Path| figs_core::deps::absolutize(&dir, p);
    assert!(deps.contains(&abs(Path::new("d.csv"))), "{deps:?}");
    assert!(deps.contains(&abs(Path::new("sub/e.csv"))), "{deps:?}");
    // A missing file is still a dependency of the (failing) evaluation, so
    // a watcher can rebuild once it appears.
    let (res, recorded) = figs_core::deps::capture(|| {
        eval_script_in(
            r#"let d = csv("later.csv"); page(1, 1, rect())"#,
            Some(&dir),
        )
    });
    assert!(res.is_err());
    assert_eq!(recorded, vec![PathBuf::from("later.csv")]);
}

#[test]
fn csv_errors_are_positioned() {
    let dir = scratch("csv-err");
    std::fs::write(dir.join("bad.csv"), "a,b\n1,2\n3\n").unwrap();
    let e = eval_script_in(
        "let x = 1;\nlet d = csv(\"nope.csv\");\npage(1, 1, rect())",
        Some(&dir),
    )
    .unwrap_err();
    assert_eq!(e.line, Some(2), "{}", e.message);
    assert!(
        e.message.contains("cannot read \"nope.csv\""),
        "{}",
        e.message
    );
    let e = eval_script_in("let d = csv(\"bad.csv\");", Some(&dir)).unwrap_err();
    assert!(
        e.message.contains("line 3: expected 2 fields, got 1"),
        "{}",
        e.message
    );
}

// ---- render ------------------------------------------------------------------

#[test]
fn plot_example_renders() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let src = std::fs::read_to_string(examples.join("plot.figs")).unwrap();
    let doc = Document::from_script_in(&src, Some(&examples)).unwrap();
    let fonts = FontStore::new();
    let computed = layout(&doc, &fonts);
    let opts = RenderOptions {
        fonts: Some(&fonts),
        base_dir: &examples,
    };
    let png = render_png(&computed, &opts).unwrap();
    let p = tiny_skia::Pixmap::decode_png(&png).unwrap();
    // Palette blue (#0072B2) from the line and the bars, and black axes.
    let (mut blue, mut black) = (0, 0);
    for px in p.pixels() {
        let (r, g, b) = (px.red(), px.green(), px.blue());
        if r < 20 && (100..130).contains(&g) && (160..190).contains(&b) {
            blue += 1;
        }
        if r < 40 && g < 40 && b < 40 {
            black += 1;
        }
    }
    assert!(blue > 1000 && black > 1000, "blue {blue}, black {black}");
    let svg = render_svg(&computed, &opts);
    assert!(svg.contains("#0072b2") || svg.contains("#0072B2"));
}

// ---- theme: colours from hooks -------------------------------------------------

/// Lowercase hex of an optional solid colour or paint ("" when unset).
fn hx<T: Clone + Into<figs_core::geom::Paint>>(c: &Option<T>) -> String {
    c.clone()
        .and_then(|c| c.into().as_solid())
        .map(|c| c.to_hex())
        .unwrap_or_default()
}

/// A small labelled plot with every kind of non-data mark: tick and axis
/// labels, a legend, a title, error bars without a colour, markers, a line.
const THEMED: &str = r#"plot(#{ xlabel: "Time", ylabel: "Signal", title: "T" }, [
        line_plot([0, 1, 2], [0.2, 0.5, 0.9], "a"),
        scatter([0, 1, 2], [0.4, 0.3, 0.6], "b"),
        errorbars([0, 1, 2], [0.4, 0.3, 0.6], [0.05, 0.05, 0.05]),
    ])"#;

fn themed(prelude: &str) -> RawDocument {
    eval_script(&format!("{prelude}\npage(\"single\", {THEMED})"))
        .unwrap_or_else(|e| panic!("script failed: {e}"))
}

/// Colours of the plot's non-data parts: text colours, and stroke colours of
/// lines (ticks, error-bar caps — not the legend key of the data line).
fn chrome_colors(raw: &RawDocument) -> (Vec<String>, Vec<String>) {
    let data = themed_line(raw);
    let texts = raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Text)
        .map(|n| hx(&n.color))
        .collect();
    let lines = raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Line)
        .map(|n| hx(&n.stroke))
        .filter(|c| *c != data)
        .collect();
    (texts, lines)
}

/// The data line of `THEMED`.
fn themed_line(raw: &RawDocument) -> String {
    let id = find_raw(raw, |n| {
        n.kind == NodeKind::Path && n.points.len() == 3 && n.points[0] == [0.0, 0.2]
    });
    hx(&raw.nodes[id].stroke)
}

#[test]
fn light_default_plot_colours_are_unchanged() {
    // The default theme is the journal one: black text, axes and ticks,
    // Okabe-Ito series, white marker rings, black error bars, no panel fill.
    let raw = themed("");
    let (texts, lines) = chrome_colors(&raw);
    assert!(
        !texts.is_empty() && texts.iter().all(|c| c == "#000000"),
        "{texts:?}"
    );
    assert!(
        !lines.is_empty() && lines.iter().all(|c| c == "#000000"),
        "{lines:?}"
    );
    let ellipses: Vec<_> = raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Ellipse)
        .collect();
    assert!(!ellipses.is_empty());
    for e in ellipses {
        assert_eq!(hx(&e.fill), "#e69f00"); // the scatter's palette colour 1
        assert_eq!(hx(&e.stroke), "#ffffff");
    }
    assert_eq!(themed_line(&raw), "#0072b2");
    assert!(raw
        .nodes
        .values()
        .all(|n| n.kind != NodeKind::Rect || n.fill.is_none()));
    assert_eq!(raw.page.background, None);
}

#[test]
fn plot_colours_follow_the_theme_hooks() {
    // `style("dark")`: everything but the data in `fg()` white, series from
    // `dark_palette()`, marker rings and page in navy.
    let raw = themed(r#"style("dark");"#);
    let (texts, lines) = chrome_colors(&raw);
    assert!(texts.iter().all(|c| c == "#ffffff"), "{texts:?}");
    assert!(lines.iter().all(|c| c == "#ffffff"), "{lines:?}");
    // The open frame: (x0, y1) -> (x0, y0) -> (x1, y0).
    let frame = find_raw(&raw, |n| {
        n.kind == NodeKind::Path && n.points.len() == 3 && n.points[0][0] == n.points[1][0]
    });
    assert_eq!(hx(&raw.nodes[frame].stroke), "#ffffff");
    assert_eq!(themed_line(&raw), "#ff9f40");
    let marker = find_raw(&raw, |n| n.kind == NodeKind::Ellipse);
    assert_eq!(hx(&raw.nodes[marker].fill), "#5cc8ff");
    assert_eq!(hx(&raw.nodes[marker].stroke), "#0f1a2e");
    assert_eq!(hx(&raw.page.background), "#0f1a2e");

    // `plot_fg` alone (a key or a hook) recolours the plot; `plot_bg` fills
    // the whole panel.
    for prelude in [
        r##"style(#{ plot_fg: "#123456", plot_bg: "#eeeeee" });"##,
        r##"fn plot_fg() { "#123456" } fn plot_bg() { "#eeeeee" }"##,
    ] {
        let raw = themed(prelude);
        let (texts, lines) = chrome_colors(&raw);
        assert!(texts.iter().all(|c| c == "#123456"), "{prelude}: {texts:?}");
        assert!(lines.iter().all(|c| c == "#123456"), "{prelude}: {lines:?}");
        // The fill is the plot node's own (container) fill, under all of it.
        let root = &raw.nodes["root"];
        assert_eq!(root.kind, NodeKind::Column);
        assert_eq!(hx(&root.fill), "#eeeeee");
        assert!(root.aspect_ratio.is_some());
    }

    // `fg()` also colours semantic text; unset, text stays colourless.
    let raw = eval_script(r#"style("dark"); page("single", vbox(["x", caption("y")]))"#).unwrap();
    assert!(raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Text)
        .all(|n| hx(&n.color) == "#ffffff"));
    let raw = eval_script(r#"page("single", vbox(["x", caption("y")]))"#).unwrap();
    assert!(raw.nodes.values().all(|n| n.color.is_none()));
}

#[test]
fn dark_palette_has_contrast_on_navy() {
    // WCAG contrast of every dark-palette colour on the navy page is >= 6:1
    // (above the 4.5:1 text threshold, as a text legend needs).
    fn lum(hex: &str) -> f64 {
        let c = figs_core::geom::Color::parse_hex(hex).unwrap();
        let lin = |v: f32| {
            let v = v as f64;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }
    let contrast = |a: &str, b: &str| {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    };
    let pal = rhai("dark_palette()");
    let colors: Vec<&str> = pal
        .trim_matches(|c| c == '[' || c == ']')
        .split(", ")
        .map(|s| s.trim_matches('"'))
        .collect();
    assert_eq!(colors.len(), 7, "{pal}");
    for c in colors {
        let k = contrast(c, "#0f1a2e");
        assert!(k >= 6.0, "{c}: {k:.2}:1 on #0f1a2e");
    }
}

// ---- markers -------------------------------------------------------------------

#[test]
fn hollow_markers_are_outlined_in_the_series_colour() {
    let raw = eval_script(
        r##"page("single", plot([scatter([0, 1], [0, 1], #{ open: true, color: "#ff0000", label: "a" })]))"##,
    )
    .unwrap();
    let w: f32 = rhai("marker_stroke_width()").parse().unwrap();
    let markers: Vec<_> = raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Ellipse)
        .collect();
    // Two data markers and the legend key, all hollow.
    assert_eq!(markers.len(), 3);
    for m in markers {
        assert_eq!(m.fill, None, "hollow marker is filled");
        assert_eq!(hx(&m.stroke), "#ff0000");
        approx(m.stroke_width.unwrap(), w);
    }
    // The resolved shape is unfilled, with that outline.
    let doc = figs_core::schema::resolve::resolve(raw).unwrap();
    let e = doc
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            figs_core::schema::NodeType::Ellipse(e) => Some(e),
            _ => None,
        })
        .unwrap();
    assert!(e.fill.is_none() && e.stroke.is_some());

    // Every shape takes `open`; `marker_open()` makes it the default, and
    // `marker_ring()` "" drops the ring of filled markers.
    let raw = eval_script(
        r#"style(#{ marker_open: true });
        page("single", plot([scatter([0], [0], #{ marker: "s" }), scatter([1], [1], #{ open: false })]))"#,
    )
    .unwrap();
    let sq = find_raw(&raw, |n| {
        n.kind == NodeKind::Path && n.closed && n.viewbox.is_none()
    });
    assert!(raw.nodes[sq].fill.is_none() && raw.nodes[sq].stroke.is_some());
    let dot = find_raw(&raw, |n| n.kind == NodeKind::Ellipse);
    assert!(raw.nodes[dot].fill.is_some());
    let raw = eval_script(r#"fn marker_ring() { "" } page("single", plot([scatter([0], [0])]))"#)
        .unwrap();
    let dot = find_raw(&raw, |n| n.kind == NodeKind::Ellipse);
    assert!(raw.nodes[dot].stroke.is_none() && raw.nodes[dot].fill.is_some());

    let e = eval_script(r#"page(1, 1, plot([scatter([1], [1], #{ open: "yes" })]))"#).unwrap_err();
    assert!(
        e.message.contains("open must be true or false"),
        "{}",
        e.message
    );
}

#[test]
fn palette_index_colours_are_shared_between_series() {
    // Markers and their fit line in one colour: `color: i` or `palette_color(i)`.
    let raw = eval_script(
        r#"page("single", plot([
            scatter([0, 1], [0, 1], #{ color: 1 }),
            line_plot([0, 1], [0, 1], #{ color: palette_color(1) }),
        ]))"#,
    )
    .unwrap();
    let marker = find_raw(&raw, |n| n.kind == NodeKind::Ellipse);
    let line = find_raw(&raw, |n| {
        n.kind == NodeKind::Path && n.points == vec![[0.0, 0.0], [1.0, 1.0]]
    });
    assert_eq!(hx(&raw.nodes[marker].fill), "#e69f00");
    assert_eq!(hx(&raw.nodes[line].stroke), "#e69f00");
    // Indices wrap (both ways) and follow the palette hook / style key.
    assert_eq!(rhai("palette_color(8)"), "#0072B2");
    assert_eq!(rhai("palette_color(-1)"), "#000000");
    let raw = eval_script(r#"style("dark"); page(1, 1, text(palette_color(0)))"#).unwrap();
    assert_eq!(raw.nodes["root"].content.as_deref(), Some("#FF9F40"));
    let e = eval_script(r#"page(1, 1, text(palette_color(0.5)))"#).unwrap_err();
    assert!(e.message.contains("integer index"), "{}", e.message);
}

// ---- legend styles -------------------------------------------------------------

#[test]
fn text_legend_stacks_coloured_labels_in_a_corner() {
    let p = Plotted::new(
        r##"fn legend_corner() { "top-left" }
        page(12, 9, plot(#{ x: [0, 10], y: [0, 1], legend: "text" }, [
            scatter([2, 8], [0.2, 0.8], #{ color: "#ff0000", label: "10 μM" }),
            line_plot([0, 10], [0, 1], #{ color: "#00ff00", label: "100 μM" }),
        ]).flex(1)).unit("cm")"##,
    );
    let area = p.area();
    let inset = 8.0 * 0.03 * 72.0 / 2.54;
    let a = p.text("10 μM");
    let b = p.text("100 μM");
    approx(a.x, area.x + inset);
    approx(a.y, area.y + inset);
    approx(b.x, a.x); // stacked, left-aligned
    approx(b.y, a.y + LINE_H + 8.0 * 0.008 * 72.0 / 2.54);
    // Each label is its series' colour; no keys are drawn (the only
    // ellipses are the data markers).
    let color_of = |s: &str| {
        hx(&p
            .raw
            .nodes
            .values()
            .find(|n| n.content.as_deref() == Some(s))
            .unwrap()
            .color)
    };
    assert_eq!(color_of("10 μM"), "#ff0000");
    assert_eq!(color_of("100 μM"), "#00ff00");
    let ellipses = |raw: &RawDocument| {
        raw.nodes
            .values()
            .filter(|n| n.kind == NodeKind::Ellipse)
            .count()
    };
    assert_eq!(ellipses(&p.raw), 2);

    // At an explicit data point: its top-left corner there by default.
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [0, 10], y: [0, 1], legend: #{ style: "text", at: [2, 0.8] } },
            [line_plot([0, 10], [0, 1], "a")]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let a = p.text("a");
    approx(a.x, area.x + area.w * 0.2);
    approx(a.y, area.y + area.h * 0.2);
    // The swatch legend stays the default: a marker key before the label.
    let raw = eval_script(r#"page("single", plot([scatter([0, 1], [0, 1], "a")]))"#).unwrap();
    assert_eq!(ellipses(&raw), 3);
    // `legend_style()` switches the default; bad styles / keys are errors.
    let raw = eval_script(
        r#"style(#{ legend_style: "text" }); page("single", plot([scatter([0, 1], [0, 1], "a")]))"#,
    )
    .unwrap();
    assert_eq!(ellipses(&raw), 2);
    let e = eval_script(
        r#"page(1, 1, plot(#{ legend: #{ style: "box" } }, [scatter([1], [1], "a")]))"#,
    )
    .unwrap_err();
    assert!(e.message.contains("unknown style \"box\""), "{}", e.message);
    let e =
        eval_script(r#"page(1, 1, plot(#{ legend: #{ where: 1 } }, [scatter([1], [1], "a")]))"#)
            .unwrap_err();
    assert!(
        e.message.contains("unknown legend option \"where\""),
        "{}",
        e.message
    );
}

// ---- annotations in data coordinates ---------------------------------------------

#[test]
fn plot_text_and_arrow_sit_at_data_points() {
    // In `marks` or among the series alike.
    for (marks, series) in [
        (
            r#"[plot_text(5, 0.5, "mid"), plot_arrow(0, 0, 10, 1)]"#,
            "[line_plot([0, 10], [1, 0])]",
        ),
        (
            "[]",
            r#"[line_plot([0, 10], [1, 0]), plot_text(5, 0.5, "mid"), plot_arrow(0, 0, 10, 1)]"#,
        ),
    ] {
        let p = Plotted::new(&format!(
            r#"page(12, 9, plot(#{{ x: [0, 10], y: [0, 1], marks: {marks} }}, {series}).flex(1)).unit("cm")"#
        ));
        let area = p.area();
        let t = p.text("mid");
        approx(t.x + t.w / 2.0, area.x + area.w / 2.0);
        approx(t.y + t.h / 2.0, area.y + area.h / 2.0);
        let arrow = find_raw(&p.raw, |n| n.kind == NodeKind::Arrow);
        assert_eq!(hx(&p.raw.nodes[arrow].stroke), "#000000"); // plot_fg()
        let s = segs_of(&p.l, arrow);
        assert_eq!(
            s[0],
            PathSegment::MoveTo(Point::new(area.x, area.y + area.h))
        );
        let PathSegment::LineTo(end) = s[1] else {
            panic!()
        };
        approx(end.x, area.x + area.w);
        approx(end.y, area.y);
    }

    // Options, and data values on a log axis (the plot takes log10).
    let p = Plotted::new(
        r##"page(12, 9, plot(#{ x: [1, 100], y: [0, 1], xscale: "log" }, [
            scatter([1, 100], [0, 1]),
            plot_text(10, 0, "dec", #{ anchor: [0.5, 1], color: 1, bold: true, size: 12 }),
            plot_arrow(10, 0.2, 10, 0.8, #{ color: "#ff0000", width: 0.1, head: "both" }),
        ]).flex(1)).unit("cm")"##,
    );
    let area = p.area();
    let t = p.text("dec");
    approx(t.x + t.w / 2.0, area.x + area.w / 2.0);
    approx(t.y + t.h, area.y + area.h);
    let n = p
        .raw
        .nodes
        .values()
        .find(|n| n.content.as_deref() == Some("dec"))
        .unwrap();
    assert_eq!(hx(&n.color), "#e69f00");
    assert_eq!((n.font_weight, n.font_size), (Some(700), Some(12.0)));
    let arrow = find_raw(&p.raw, |n| n.kind == NodeKind::Arrow);
    let a = &p.raw.nodes[arrow];
    assert_eq!(hx(&a.stroke), "#ff0000");
    approx(a.stroke_width.unwrap(), 0.1);
    approx(a.points[0][0], 1.0); // log10(10)

    let e = eval_script(r#"page(1, 1, plot([plot_text(0, 0, "x", #{ colour: 1 })]))"#).unwrap_err();
    assert!(
        e.message.contains("unknown option \"colour\""),
        "{}",
        e.message
    );
}

// ---- title -------------------------------------------------------------------

#[test]
fn title_is_centred_above_the_area() {
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [0, 10], y: [0, 1], title: "加熱曲線" },
            [line_plot([0, 10], [0, 1])]).flex(1)).unit("cm")"#,
    );
    let panel = rect_of(&p.l, "root");
    let area = p.area();
    let t = p.text("加熱曲線");
    approx(t.x + t.w / 2.0, area.x + area.w / 2.0);
    // Its bottom half its own height above the axis (clear of the top tick
    // label's half-line overhang), at the top of the box.
    approx(area.y - (t.y + t.h), LINE_H / 2.0);
    approx(t.y, panel.y + hook_pt("plot_pad"));
    let n = p
        .raw
        .nodes
        .values()
        .find(|n| n.content.as_deref() == Some("加熱曲線"))
        .unwrap();
    assert_eq!(n.font_size, Some(8.0)); // plot_title_size() = base_size()
    assert_eq!(n.font_family, None); // the page font: CJK fallback applies

    // With a horizontal y label the two share one band: title, then label.
    let p = Plotted::new(
        r#"fn ylabel_rotated() { false }
        page(12, 9, plot(#{ x: [0, 10], y: [0, 1], title: "T", ylabel: "Signal" },
            [line_plot([0, 10], [0, 1])]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let (t, yl) = (p.text("T"), p.text("Signal"));
    approx(t.x + t.w / 2.0, area.x + area.w / 2.0);
    approx(yl.x, area.x);
    assert!(yl.y >= t.y + t.h - 1e-3, "{t:?} over {yl:?}");
    assert!(area.y - (yl.y + yl.h) >= LINE_H / 2.0 - 1e-3);
}

// ---- ticks ---------------------------------------------------------------------

#[test]
fn inward_and_minor_ticks() {
    let src = |hooks: &str| {
        format!(
            r#"{hooks}
            page(12, 9, plot(#{{ x: [0, 10], y: [0, 1], xticks: [0, 5, 10], yticks: [0, 1] }},
                [line_plot([0, 10], [0, 1])]).flex(1)).unit("cm")"#
        )
    };
    let ticks = |p: &Plotted| -> Vec<Rect> {
        p.l.nodes
            .iter()
            .filter(|n| p.raw.nodes[&n.id].kind == NodeKind::Line)
            .map(|n| n.rect)
            .collect()
    };
    let gap = 8.0 * 0.012 * 72.0 / 2.54;
    let out = Plotted::new(&src(""));
    let area = out.area();
    assert_eq!(ticks(&out).len(), 5);
    for r in ticks(&out) {
        // Outward: below the x axis or left of the y axis.
        assert!(
            r.y >= area.y + area.h - 1e-3 || r.x + r.w <= area.x + 1e-3,
            "{r:?}"
        );
    }
    approx(
        out.text("5").y,
        area.y + area.h + hook_pt("tick_length") + gap,
    );

    let inn = Plotted::new(&src(r#"style(#{ tick_direction: "in", minor_ticks: 5 });"#));
    let area = inn.area();
    // 3 + 2 labelled ticks, plus 4 minor ones per x step (8) and y step (4).
    let t = ticks(&inn);
    assert_eq!(t.len(), 5 + 8 + 4);
    for r in &t {
        assert!(
            r.x >= area.x - 1e-3
                && r.x + r.w <= area.x + area.w + 1e-3
                && r.y >= area.y - 1e-3
                && r.y + r.h <= area.y + area.h + 1e-3,
            "{r:?} outside {area:?}"
        );
    }
    // The tick labels move up to the axis: no tick room outside.
    approx(inn.text("5").y, area.y + area.h + gap);
    let e = eval_script(&src(r#"fn tick_direction() { "up" }"#)).unwrap_err();
    assert!(e.message.contains("tick_direction"), "{}", e.message);
}

#[test]
fn plot_dark_example_renders() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let src = std::fs::read_to_string(examples.join("plot_dark.figs")).unwrap();
    let doc = Document::from_script_in(&src, Some(&examples)).unwrap();
    let fonts = FontStore::new();
    let computed = layout(&doc, &fonts);
    let opts = RenderOptions {
        fonts: Some(&fonts),
        base_dir: &examples,
    };
    let png = render_png(&computed, &opts).unwrap();
    let p = tiny_skia::Pixmap::decode_png(&png).unwrap();
    let (mut navy, mut white, mut orange, mut sky) = (0, 0, 0, 0);
    for px in p.pixels() {
        match (px.red(), px.green(), px.blue()) {
            (10..=20, 22..=32, 40..=52) => navy += 1,
            (240.., 240.., 240..) => white += 1,
            (240.., 145..=170, 50..=80) => orange += 1,
            (80..=100, 190..=210, 240..) => sky += 1,
            _ => {}
        }
    }
    let total = p.pixels().len();
    assert!(navy * 2 > total, "navy {navy} of {total}");
    assert!(
        white > 5000 && orange > 1000 && sky > 1000,
        "white {white}, orange {orange}, sky {sky}"
    );
}

// ---- frame-anchored plots ------------------------------------------------------

#[test]
fn frame_box_plot_is_its_data_frame() {
    // `frame_box: true`: the node placed with `.at(..).width(..).height(..)`
    // *is* the data frame; tick labels, axis labels and title hang outside.
    let src = |opt: &str, pre: &str| {
        format!(
            r#"{pre}
            page(12, 9, stack([plot(#{{ x: [0, 10], y: [0, 1], xlabel: "Time", ylabel: "Signal",
                    title: "T"{opt} }}, [line_plot([0, 10], [0, 1])]).at(2, 1.5).width(8).height(6)]))
                .unit("cm")"#
        )
    };
    let cm = 72.0 / 2.54;
    for (opt, pre) in [
        (", frame_box: true", ""),
        ("", "style(#{ plot_frame_box: true });"),
    ] {
        let p = Plotted::new(&src(opt, pre));
        assert!(p.raw.nodes.values().all(|n| !n.contain), "no contain stack");
        // Every stack with the data viewbox (the plot node and its area)
        // is exactly the box the plot was given.
        let frames: Vec<Rect> =
            p.l.nodes
                .iter()
                .filter(|n| {
                    let r = &p.raw.nodes[&n.id];
                    r.kind == NodeKind::Stack && r.viewbox == Some([0.0, 1.0, 10.0, 0.0])
                })
                .map(|n| n.rect)
                .collect();
        assert_eq!(frames.len(), 2, "{frames:?}");
        for f in &frames {
            assert_rect(*f, 2.0 * cm, 1.5 * cm, 8.0 * cm, 6.0 * cm);
        }
        let area = frames[0];
        // The data line runs corner to corner of the given box.
        let line_id = find_raw(&p.raw, |n| {
            n.kind == NodeKind::Path && n.points == vec![[0.0, 0.0], [10.0, 1.0]]
        });
        let s = segs_of(&p.l, line_id);
        assert_eq!(
            s[0],
            PathSegment::MoveTo(Point::new(area.x, area.y + area.h))
        );
        // Labels hang outside the frame, at the usual distances.
        let (tl, gap) = (hook_pt("tick_length"), 8.0 * 0.012 * cm);
        let y0 = p.text("0.0");
        approx(y0.x + y0.w, area.x - tl - gap);
        let x5 = p.text("4");
        approx(x5.y, area.y + area.h + tl + gap);
        let xl = p.text("Time");
        approx(xl.y, area.y + area.h + tl + gap + LINE_H + gap);
        let t = p.text("T");
        approx(t.y + t.h, area.y - LINE_H / 2.0);
        let yl =
            p.l.nodes
                .iter()
                .find(|n| p.raw.nodes[&n.id].content.as_deref() == Some("Signal"))
                .unwrap()
                .bounds();
        assert!(yl.x + yl.w < y0.x, "{yl:?} vs {y0:?}");
    }

    // A labelled frame-box plot gets no band: the frame stays where it is put.
    let p = Plotted::new(
        r#"page(12, 9, stack([labeled(plot(#{ x: [0, 2], y: [0, 1], frame_box: true },
                [line_plot([0, 1], [0, 1])]).at(2, 1.5).width(8).height(6), "(a)")])).unit("cm")"#,
    );
    let frames: Vec<Rect> =
        p.l.nodes
            .iter()
            .filter(|n| {
                let r = &p.raw.nodes[&n.id];
                r.kind == NodeKind::Stack && r.viewbox == Some([0.0, 1.0, 2.0, 0.0])
            })
            .map(|n| n.rect)
            .collect();
    assert_eq!(frames.len(), 2, "{frames:?}");
    for f in frames {
        assert_rect(f, 2.0 * cm, 1.5 * cm, 8.0 * cm, 6.0 * cm);
    }
    let e = eval_script(r#"page(1, 1, plot(#{ frame_box: 1 }, [line_plot([0, 1], [0, 1])]))"#)
        .unwrap_err();
    assert!(e.message.contains("frame_box must be"), "{}", e.message);
}

#[test]
fn label_room_only_when_labelled() {
    let plot = r#"plot(#{ x: [0, 10], y: [0, 1] }, [line_plot([0, 10], [0, 1])])"#;
    let room = hook_pt("plot_label_room");
    assert!(room > 10.0, "{room}");
    let pp = hook_pt("plot_pad");
    // Standalone: the area starts right under the top tick label's overhang.
    let alone = Plotted::new(&format!(r#"page(12, 9, {plot}.flex(1)).unit("cm")"#));
    approx(alone.area().y, pp + LINE_H / 2.0);
    let band = |raw: &RawDocument| {
        raw.nodes
            .values()
            .filter(|n| n.kind == NodeKind::Rect && n.height.is_some_and(|h| h > 0.5))
            .count()
    };
    assert_eq!(band(&alone.raw), 0);
    // `panels` / `labeled` prepend the band inside the plot, under its
    // aspect: the box keeps its size, the area moves down by the room.
    for wrap in [
        format!("panels([{plot}])[0]"),
        format!(r#"labeled({plot}, "(a)")"#),
    ] {
        let p = Plotted::new(&format!(
            r#"page(12, "auto", col([{wrap}]).stretch()).unit("cm")"#
        ));
        approx(p.area().y, room + pp + LINE_H / 2.0);
        approx(rect_of(&p.l, "root").h, 9.0 * 72.0 / 2.54); // still 4:3
        assert_eq!(band(&p.raw), 1);
    }
    // Labelling twice adds one band; images get none; room 0 adds none.
    let raw = eval_script(&format!(
        r#"page(12, 9, labeled(labeled({plot}, "a"), "b")).unit("cm")"#
    ))
    .unwrap();
    assert_eq!(band(&raw), 1);
    let raw = eval_script(&format!(
        r#"page(12, 9, col(panels([{plot}, image("a.png")]))).unit("cm")"#
    ))
    .unwrap();
    assert_eq!(band(&raw), 1);
    let raw = eval_script(&format!(
        r#"style(#{{ plot_label_room: 0 }}); page(12, 9, col(panels([{plot}]))).unit("cm")"#
    ))
    .unwrap();
    assert_eq!(band(&raw), 0);
}

// ---- typography and per-axis ticks ----------------------------------------------

#[test]
fn plot_typography_hooks() {
    let raw = eval_script(
        r#"style(#{ plot_font: "Plot Sans", tick_label_size: 6, tick_label_font: "Tick Mono",
                   axis_label_size: 11, legend_size: 7, legend_font: ["Leg A", "Leg B"],
                   plot_title_size: 12 });
        page("single", plot(#{ x: [0, 10], y: [0, 1], xticks: [0, 10], yticks: [0, 1],
                xlabel: "Time", ylabel: "Signal", title: "T" },
            [line_plot([0, 10], [0, 1], "data"), plot_text(5, 0.5, "note")]))"#,
    )
    .unwrap();
    let node = |s: &str| {
        raw.nodes
            .values()
            .find(|n| n.content.as_deref() == Some(s))
            .unwrap_or_else(|| panic!("no text {s:?}"))
    };
    let font = |s: &str| node(s).font_family.clone().unwrap_or_default();
    for t in ["0", "10", "1"] {
        assert_eq!(node(t).font_size, Some(6.0), "{t}");
        assert_eq!(font(t), "Tick Mono");
    }
    for t in ["Time", "Signal"] {
        assert_eq!(node(t).font_size, Some(11.0), "{t}");
        assert_eq!(font(t), "Plot Sans"); // axis_label_font() follows plot_font()
    }
    assert_eq!(node("data").font_size, Some(7.0));
    assert!(font("data").contains("Leg A"), "{}", font("data"));
    assert_eq!(node("T").font_size, Some(12.0));
    assert_eq!(font("T"), "Plot Sans");
    assert_eq!(font("note"), "Plot Sans");
    assert_eq!(node("note").font_size, Some(8.0));

    // The default sets no font: plot text uses the page font.
    let raw = themed("");
    assert!(raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Text)
        .all(|n| n.font_family.is_none()));

    // `tick_label_pad()` is the tick -> label gap; `axis_label_pad()` the
    // tick label -> axis label gap.
    let p = Plotted::new(
        r#"style(#{ tick_label_pad: 0.5, axis_label_pad: 0.3 });
        page(12, 9, plot(#{ x: [0, 10], y: [0, 1], xticks: [0, 5, 10], xlabel: "Time" },
            [line_plot([0, 10], [0, 1])]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let cm = 72.0 / 2.54;
    let tl = hook_pt("tick_length");
    let five = p.text("5");
    approx(five.y, area.y + area.h + tl + 0.5 * cm);
    approx(p.text("Time").y, five.y + LINE_H + 0.3 * cm);
    approx(p.text("0.0").x + 3.0 * CHAR_W, area.x - tl - 0.5 * cm);
}

/// A plot of x 0..10 (ticks 0, 5, 10), y 0..1, with `pre` before the page
/// and `opt` appended to the plot options.
fn pad_plot(pre: &str, opt: &str) -> String {
    format!(
        r#"{pre}
        page(12, 9, plot(#{{ x: [0, 10], y: [0, 1], xticks: [0, 5, 10], xlabel: "Time",
                ylabel: "Signal"{opt} }}, [line_plot([0, 10], [0, 1])]).flex(1)).unit("cm")"#
    )
}

/// The first baseline of an 8 pt label without fonts (the measurer's
/// fallback): half-leading of the 1.2 line plus 0.8 em.
const BASE8: f32 = 8.0 * 0.1 + 8.0 * 0.8;

#[test]
fn tick_label_pads_per_axis() {
    let cm = 72.0 / 2.54;
    let gap = 8.0 * 0.012 * cm; // the default pads
    for (pre, opt) in [
        (
            "style(#{ xtick_label_pad: 0.5, ytick_label_pad: 0.2 });",
            "",
        ),
        ("", ", xtick_label_pad: 0.5, ytick_label_pad: 0.2"),
    ] {
        let p = Plotted::new(&pad_plot(pre, opt));
        let area = p.area();
        let tl = hook_pt("tick_length");
        let five = p.text("5");
        approx(five.y, area.y + area.h + tl + 0.5 * cm);
        approx(p.text("0.0").x + 3.0 * CHAR_W, area.x - tl - 0.2 * cm);
        // The axis labels keep their default gaps.
        approx(p.text("Time").y, five.y + LINE_H + gap);
    }
    // Each defaults to `tick_label_pad()`: spelling the defaults out changes
    // nothing.
    let a = eval_script(&pad_plot("", "")).unwrap();
    let b = eval_script(&pad_plot(
        "",
        r#", xtick_label_pad: tick_label_pad(), ytick_label_pad: tick_label_pad(),
           tick_label_pad_mode: "box", xlabel_pad: axis_label_pad(),
           ylabel_pad: axis_label_pad() * 2, xlabel_offset: (), ylabel_offset: ()"#,
    ))
    .unwrap();
    let sorted = |d: &RawDocument| -> std::collections::BTreeMap<String, String> {
        d.nodes
            .iter()
            .map(|(k, v)| (k.clone(), format!("{v:?}")))
            .collect()
    };
    assert_eq!(sorted(&a), sorted(&b));
    // A negative pad pulls the label box onto the tick, but not past the axis.
    let p = Plotted::new(&pad_plot("", ", xtick_label_pad: -0.05"));
    let area = p.area();
    approx(
        p.text("5").y,
        area.y + area.h + hook_pt("tick_length") - 0.05 * cm,
    );
    for (opt, want) in [
        (
            ", xtick_label_pad: -1",
            "xtick_label_pad -1.0 would put the x tick labels across the axis",
        ),
        (
            ", ytick_label_pad: \"a\"",
            "ytick_label_pad must be a length",
        ),
        (
            ", tick_label_pad_mode: \"ink\"",
            "tick_label_pad_mode must be \"box\" or \"baseline\"",
        ),
        (
            ", xtick_direction: \"in\", xtick_label_pad: -0.01",
            "across the axis",
        ),
    ] {
        let e = eval_script(&pad_plot("", opt)).unwrap_err();
        assert!(e.message.contains(want), "{opt}: {}", e.message);
    }
    let e = eval_script(&pad_plot("style(#{ ytick_label_pad: -1 });", "")).unwrap_err();
    assert!(e.message.contains("ytick_label_pad: -1"), "{}", e.message);
}

#[test]
fn baseline_tick_label_pads() {
    // "baseline": the x tick labels' first baseline `xtick_label_pad` below
    // the tick end; the x label still clears their box bottom.
    let cm = 72.0 / 2.54;
    let gap = 8.0 * 0.012 * cm;
    let tl = hook_pt("tick_length");
    for pre in [
        "style(#{ tick_label_pad_mode: \"baseline\", xtick_label_pad: 0.4 });",
        "",
    ] {
        let opt = if pre.is_empty() {
            ", tick_label_pad_mode: \"baseline\", xtick_label_pad: 0.4"
        } else {
            ""
        };
        let p = Plotted::new(&pad_plot(pre, opt));
        let area = p.area();
        for s in ["0", "5", "10"] {
            approx(p.text(s).y + BASE8, area.y + area.h + tl + 0.4 * cm);
        }
        let five = p.text("5");
        approx(five.x + five.w / 2.0, area.x + area.w / 2.0);
        approx(p.text("Time").y, five.y + LINE_H + gap);
        // The y tick labels are unchanged (box to box).
        approx(p.text("0.0").x + 3.0 * CHAR_W, area.x - tl - gap);
    }
    // Inward ticks: from the axis itself.
    let p = Plotted::new(&pad_plot(
        "",
        ", tick_label_pad_mode: \"baseline\", xtick_label_pad: 0.3, xtick_direction: \"in\"",
    ));
    let area = p.area();
    approx(p.text("5").y + BASE8, area.y + area.h + 0.3 * cm);
}

#[test]
fn axis_label_pads_and_offsets() {
    let cm = 72.0 / 2.54;
    let tl = hook_pt("tick_length");
    let gap = 8.0 * 0.012 * cm;
    let signal = |p: &Plotted| {
        p.l.nodes
            .iter()
            .find(|n| p.raw.nodes[&n.id].content.as_deref() == Some("Signal"))
            .unwrap()
            .bounds()
    };
    // Pads per axis, box to box.
    let p = Plotted::new(&pad_plot("", ", xlabel_pad: 0.5, ylabel_pad: 0.3"));
    approx(p.text("Time").y, p.text("5").y + LINE_H + 0.5 * cm);
    let yl = signal(&p);
    approx(p.text("0.0").x - (yl.x + yl.w), 0.3 * cm);
    let p = Plotted::new(&pad_plot(
        "style(#{ xlabel_pad: 0.5, ylabel_pad: 0.3 });",
        "",
    ));
    approx(p.text("Time").y, p.text("5").y + LINE_H + 0.5 * cm);

    // Offsets: the baselines exactly that far from the axes, whatever the
    // tick labels measure.
    let p = Plotted::new(&pad_plot("", ", xlabel_offset: 1.2, ylabel_offset: 1.5"));
    let area = p.area();
    approx(p.text("Time").y + BASE8, area.y + area.h + 1.2 * cm);
    let t = p.text("Time");
    approx(t.x + t.w / 2.0, area.x + area.w / 2.0);
    let yl = signal(&p);
    // Turned -90°: its baseline runs down the footprint, BASE8 from its left.
    approx(yl.x + BASE8, area.x - 1.5 * cm);
    approx(yl.w, LINE_H);
    approx(yl.y + yl.h / 2.0, area.y + area.h / 2.0);
    // The tick labels didn't move.
    approx(p.text("5").y, area.y + area.h + tl + gap);
    // Contained: everything still fits the panel.
    let panel = rect_of(&p.l, "root");
    assert!(yl.x >= panel.x - 1e-3, "{yl:?}");
    // An unrotated y label: its baseline that far above the axis top.
    let p = Plotted::new(&pad_plot(
        "fn ylabel_rotated() { false }",
        ", ylabel_offset: 0.6",
    ));
    let area = p.area();
    let yl = p.text("Signal");
    approx(yl.y + BASE8, area.y - 0.6 * cm);
    approx(yl.x, area.x);

    // Negative pads are an error that points at the offsets; so are bad
    // offsets.
    for (pre, opt, want) in [
        ("", ", xlabel_pad: -0.075", "xlabel_pad must be >= 0 cm (got -0.075); to bring the label closer, place its baseline with xlabel_offset"),
        ("style(#{ ylabel_pad: -0.1 });", "", "ylabel_pad: must be >= 0 cm"),
        ("style(#{ axis_label_pad: -0.1 });", "", "must be a length >= 0 cm"),
        ("", ", xlabel_offset: 0", "xlabel_offset must be a length > 0 cm"),
        ("", ", ylabel_offset: -1", "ylabel_offset must be a length > 0 cm"),
        ("", ", ylabel_offset: \"1cm\"", "ylabel_offset must be"),
    ] {
        let e = eval_script(&pad_plot(pre, opt)).unwrap_err();
        assert!(e.message.contains(want), "{pre}{opt}: {}", e.message);
    }
}

#[test]
fn per_axis_minor_ticks_and_directions() {
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [0, 10], y: [0, 1], xticks: [0, 5, 10], yticks: [0, 1],
                xminor: 5, yminor: 2, xtick_direction: "in" },
            [line_plot([0, 10], [0, 1])]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let ticks: Vec<Rect> =
        p.l.nodes
            .iter()
            .filter(|n| p.raw.nodes[&n.id].kind == NodeKind::Line)
            .map(|n| n.rect)
            .collect();
    // 3 + 2 labelled, 4 minor per x step (8), 1 per y step (1).
    assert_eq!(ticks.len(), 5 + 8 + 1);
    let (xs, ys): (Vec<Rect>, Vec<Rect>) = ticks.iter().partition(|r| r.w < r.h);
    assert_eq!((xs.len(), ys.len()), (11, 3));
    for r in xs {
        // x ticks point in (up from the axis) ...
        assert!(
            r.y + r.h <= area.y + area.h + 1e-3 && r.y >= area.y,
            "{r:?}"
        );
    }
    for r in ys {
        // ... y ticks still out (left of it).
        assert!(r.x + r.w <= area.x + 1e-3, "{r:?}");
    }
    // The x labels move up to the axis; the y labels keep the tick room.
    let gap = 8.0 * 0.012 * 72.0 / 2.54;
    approx(p.text("5").y, area.y + area.h + gap);
    approx(
        p.text("1").x + CHAR_W,
        area.x - hook_pt("tick_length") - gap,
    );
    // The per-axis option overrides `minor_ticks()`; bad values are errors.
    let raw = eval_script(
        r#"style(#{ minor_ticks: 2 });
        page(1, 1, plot(#{ x: [0, 10], y: [0, 1], xticks: [0, 10], yticks: [0, 1], xminor: 0 },
            [line_plot([0, 10], [0, 1])]))"#,
    )
    .unwrap();
    let lines = raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Line)
        .count();
    assert_eq!(lines, 2 + 2 + 1);
    let e = eval_script(r#"page(1, 1, plot(#{ yminor: -1 }, [line_plot([0, 1], [0, 1])]))"#)
        .unwrap_err();
    assert!(e.message.contains("yminor must be"), "{}", e.message);
    let e =
        eval_script(r#"page(1, 1, plot(#{ ytick_direction: "up" }, [line_plot([0, 1], [0, 1])]))"#)
            .unwrap_err();
    assert!(e.message.contains("ytick_direction"), "{}", e.message);
}

#[test]
fn tick_format_sets_decimals_or_calls_a_function() {
    let texts = |src: &str| -> Vec<String> {
        let raw = eval_script(src).unwrap_or_else(|e| panic!("{e}"));
        raw.nodes
            .values()
            .filter_map(|n| n.content.clone())
            .collect()
    };
    let t = texts(
        r#"page("single", plot(#{ x: [0, 10], y: [0, 1], xticks: [0, 5, 10], yticks: [0, 0.5, 1],
                ytick_format: 2, xtick_format: |v| `${v.to_int()} s` },
            [line_plot([0, 10], [0, 1])]))"#,
    );
    for s in ["0.00", "0.50", "1.00", "0 s", "5 s", "10 s"] {
        assert!(t.contains(&s.to_string()), "missing {s:?} in {t:?}");
    }
    // Negative values keep the real minus; log axes format the value itself.
    let t = texts(
        r#"page("single", plot(#{ x: [1, 100], y: [-1, 1], yticks: [-1, 1], xscale: "log",
                xtick_format: 0, ytick_format: 1 }, [line_plot([1, 100], [-1, 1])]))"#,
    );
    for s in ["1", "10", "100", "−1.0", "1.0"] {
        assert!(t.contains(&s.to_string()), "missing {s:?} in {t:?}");
    }
    let e =
        eval_script(r#"page(1, 1, plot(#{ xtick_format: "%.2f" }, [line_plot([0, 1], [0, 1])]))"#)
            .unwrap_err();
    assert!(e.message.contains("xtick_format must be"), "{}", e.message);
    let e = eval_script(r#"page(1, 1, plot(#{ xtick_format: 1 }, [bars(["a", "b"], [1, 2])]))"#)
        .unwrap_err();
    assert!(e.message.contains("numeric axis"), "{}", e.message);
}

// ---- per-plot style ------------------------------------------------------------

#[test]
fn per_plot_style_does_not_leak() {
    let raw = eval_script(
        r##"style(#{ base_size: 9 });
        let a = plot(#{ style: #{ base_size: 20, plot_fg: "#ff0000", marker_open: true } },
                     [scatter([0, 1], [0, 1], "first")]);
        let b = plot([scatter([0, 1], [0, 1], "second")]);
        page("double", col([a, b, text(`${base_size()}`)]))"##,
    )
    .unwrap();
    let node = |s: &str| {
        raw.nodes
            .values()
            .find(|n| n.content.as_deref() == Some(s))
            .unwrap_or_else(|| panic!("no text {s:?}"))
    };
    // Plot a: its style (also for defaults of series built before it).
    assert_eq!(node("first").font_size, Some(20.0));
    assert_eq!(hx(&node("first").color), "#ff0000");
    // Plot b and everything after: the script's style again.
    assert_eq!(node("second").font_size, Some(9.0));
    assert_eq!(hx(&node("second").color), "#000000");
    assert!(raw
        .nodes
        .values()
        .any(|n| n.content.as_deref() == Some("9")));
    let (open, filled): (Vec<_>, Vec<_>) = raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Ellipse)
        .partition(|n| n.fill.is_none());
    assert_eq!((open.len(), filled.len()), (3, 3)); // 2 markers + legend key each

    let e =
        eval_script(r#"page(1, 1, plot(#{ style: #{ base: 1 } }, [line_plot([0, 1], [0, 1])]))"#)
            .unwrap_err();
    assert!(
        e.message.contains("unknown style key \"base\""),
        "{}",
        e.message
    );
    let e = eval_script(r#"style_pop(); page(1, 1, rect())"#).unwrap_err();
    assert!(e.message.contains("style_push"), "{}", e.message);
}

// ---- plot_text options ---------------------------------------------------------

#[test]
fn plot_text_takes_font_italic_and_markup() {
    let raw = eval_script(
        r#"page("single", plot(#{ x: [0, 10], y: [0, 1] }, [
            line_plot([0, 10], [0, 1]),
            plot_text(2, 0.5, "plain", #{ font: "Serif X", italic: true, size: 10 }),
            plot_text(8, 0.5, "*T*_{1/2}", #{ markup: true, bold: true }),
        ]))"#,
    )
    .unwrap();
    let plain = raw
        .nodes
        .values()
        .find(|n| n.content.as_deref() == Some("plain"))
        .unwrap();
    assert_eq!(plain.font_family.as_deref(), Some("Serif X"));
    assert_eq!(plain.font_style, Some(figs_core::schema::FontStyle::Italic));
    assert_eq!(plain.font_size, Some(10.0));
    let rich = raw
        .nodes
        .values()
        .find(|n| !n.spans.is_empty())
        .expect("markup text has spans");
    let text: String = rich.spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(text, "T1/2");
    assert_eq!(rich.font_weight, Some(700));
    assert_eq!(hx(&rich.color), "#000000");
    // Rich axis labels: `markup(..)` spans work for xlabel / ylabel / title.
    let raw = eval_script(
        r#"page("single", plot(#{ xlabel: markup("*T* [K]") }, [line_plot([0, 1], [0, 1])]))"#,
    )
    .unwrap();
    assert!(raw.nodes.values().any(|n| n.spans.len() >= 2));
    let e =
        eval_script(r#"page(1, 1, plot([plot_text(0, 0, "x", #{ italic: "yes" })]))"#).unwrap_err();
    assert!(e.message.contains("italic must be"), "{}", e.message);
}

#[test]
fn plot_frame_example_puts_frames_on_the_guides() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let src = std::fs::read_to_string(examples.join("plot_frame.figs")).unwrap();
    let raw = eval_script_in(&src, Some(&examples)).unwrap();
    let doc = figs_core::schema::resolve::resolve(raw.clone()).unwrap();
    let fonts = FontStore::new();
    let l = layout(&doc, &fonts);
    // The dashed guides and the plot nodes (stacks with a viewbox, children
    // of the page stack) share their boxes exactly.
    let root = &raw.nodes["root"];
    let boxes = |f: &dyn Fn(&figs_core::schema::RawNode) -> bool| -> Vec<Rect> {
        root.children
            .iter()
            .filter(|id| f(&raw.nodes[id.as_str()]))
            .map(|id| rect_of(&l, id))
            .collect()
    };
    let guides = boxes(&|n| n.kind == NodeKind::Rect && !n.dash.is_empty());
    let plots = boxes(&|n| n.kind == NodeKind::Stack && n.viewbox.is_some());
    assert_eq!((guides.len(), plots.len()), (2, 2));
    for (g, p) in guides.iter().zip(&plots) {
        assert_rect(*p, g.x, g.y, g.w, g.h);
    }
}

// ---- insets ---------------------------------------------------------------------

/// `inset_pad()` at the default 8 pt, in pt.
const INSET_PAD: f32 = 8.0 * 0.03 * 72.0 / 2.54;
const CM: f32 = 72.0 / 2.54;

#[test]
fn inset_sits_in_a_named_corner_of_the_data_frame() {
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [0, 10], y: [0, 1], legend: "" }, [
            line_plot([0, 10], [0, 1]),
            inset(text("I"), #{ corner: "top-right", width: 2, height: 1 }),
        ]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let r = p.text("I");
    approx(r.w, 2.0 * CM);
    approx(r.h, 1.0 * CM);
    approx(r.x + r.w, area.x + area.w - INSET_PAD);
    approx(r.y, area.y + INSET_PAD);
}

#[test]
fn inset_at_a_data_point_hangs_from_it() {
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [0, 10], y: [0, 1],
            marks: [inset(text("I"), #{ at: [2, 0.8], width: 2, height: 1 })] },
            [line_plot([0, 10], [0, 1])]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let r = p.text("I");
    approx(r.x, area.x + area.w * 0.2);
    approx(r.y, area.y + area.h * 0.2);
    // `anchor` picks the point of the inset that sits there.
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [0, 10], y: [0, 1] }, [line_plot([0, 10], [0, 1]),
            inset(text("I"), #{ at: [5, 0.5], anchor: [0.5, 0.5], width: 2, height: 1 })]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let r = p.text("I");
    approx(r.x + r.w / 2.0, area.x + area.w * 0.5);
    approx(r.y + r.h / 2.0, area.y + area.h * 0.5);
    // Without a width it is `size` of the frame wide, still hanging from the
    // point (and kept inside the frame sideways).
    for (at, want_x) in [("[2, 0.8]", 0.2), ("[9, 0.8]", 0.6)] {
        let p = Plotted::new(&format!(
            r#"page(12, 9, plot(#{{ x: [0, 10], y: [0, 1] }}, [line_plot([0, 10], [0, 1]),
                inset(text("I").aspect(2), #{{ at: {at} }})]).flex(1)).unit("cm")"#
        ));
        let area = p.area();
        let r = p.text("I");
        approx(r.w, area.w * 0.4);
        approx(r.h, r.w / 2.0);
        approx(r.x, area.x + area.w * want_x);
        approx(r.y, area.y + area.h * 0.2);
    }
}

#[test]
fn auto_inset_takes_the_emptiest_corner_the_legend_left() {
    // Data on the diagonal: top-left and bottom-right are empty. The legend
    // takes the first (top-left), the inset the other.
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [0, 10], y: [0, 1] }, [
            line_plot([0, 10], [0, 1], "a"),
            inset(text("I"), #{ width: 2, height: 1 }),
        ]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let legend = p.text("a");
    assert!(legend.x < area.x + area.w / 2.0 && legend.y < area.y + area.h / 2.0);
    let r = p.text("I");
    approx(r.x + r.w, area.x + area.w - INSET_PAD);
    approx(r.y + r.h, area.y + area.h - INSET_PAD);
}

#[test]
fn inset_on_any_panel_pins_to_its_corner() {
    let p = Plotted::new(
        r#"page(10, 8, inset(rect(), text("I"), #{ corner: "bottom-left", width: 2, height: 1 })).unit("cm")"#,
    );
    let page = rect_of(&p.l, "root");
    let r = p.text("I");
    approx(r.x, page.x + INSET_PAD);
    approx(r.y + r.h, page.y + page.h - INSET_PAD);
    // "auto" (the default) means top-right off a plot.
    let p = Plotted::new(
        r#"page(10, 8, inset(rect(), text("I"), #{ width: 2, height: 1 })).unit("cm")"#,
    );
    let page = rect_of(&p.l, "root");
    let r = p.text("I");
    approx(r.x + r.w, page.x + page.w - INSET_PAD);
    approx(r.y, page.y + INSET_PAD);
}

#[test]
fn inset_hooks_and_bad_options() {
    // inset_size() is its share of the frame width (less the pads); the
    // height follows the node's aspect.
    let p = Plotted::new(
        r#"style(#{ inset_size: 0.5, inset_corner: "bottom-left" });
        page(12, 9, plot(#{ x: [0, 10], y: [0, 1], legend: "" }, [
            inset(text("I").aspect(2)),
        ]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let r = p.text("I");
    approx(r.w, (area.w - 2.0 * INSET_PAD) * 0.5);
    approx(r.h, r.w / 2.0);
    approx(r.x, area.x + INSET_PAD);
    approx(r.y + r.h, area.y + area.h - INSET_PAD);
    // inset_plot: a plot whose labels are inset_text_size() (6 pt here).
    let raw = eval_script(
        r#"page("single", plot([line_plot([0, 1], [0, 1]), inset(inset_plot(#{ x: [0, 1], y: [0, 4] },
            [line_plot([0, 1], [0, 4])]))]))"#,
    )
    .unwrap();
    let four: Vec<_> = raw
        .nodes
        .values()
        .filter(|n| n.content.as_deref() == Some("4"))
        .collect();
    assert_eq!(four.len(), 1);
    approx(four[0].font_size.unwrap(), 6.0);
    for (src, want) in [
        (r#"inset(text("x"), #{ big: 2 })"#, "unknown option \"big\""),
        (
            r#"inset(text("x"), #{ corner: "middle" })"#,
            "unknown corner \"middle\"",
        ),
        (r#"inset(text("x"), #{ at: [1] })"#, "`at` must be [x, y]"),
        (
            r#"inset(text("x"), #{ size: 0 })"#,
            "size must be a fraction",
        ),
        (
            r#"inset(rect(), text("x"), #{ at: [1, 2] })"#,
            "data coordinates",
        ),
        (r#"inset_plot(#{ style: 1 }, [])"#, "style must be a map"),
    ] {
        let e = eval_script(&format!("page(\"single\", plot([{src}]))"))
            .err()
            .or_else(|| eval_script(&format!("page(\"single\", {src})")).err())
            .unwrap_or_else(|| panic!("{src} should fail"))
            .to_string();
        assert!(e.contains(want), "{src}: {e}");
    }
}

#[test]
fn an_explicitly_sized_stack_measures_its_children_at_that_size() {
    // A filling base layer with an aspect ratio (an image under an overlay)
    // reports the height it gets at the stack's width, so the stack wraps it.
    let raw = eval_script(
        r#"page(20, 10, row([stack([rect().aspect(2), text("o").at(0, 0)]).width(8)]).cross("start")).unit("cm")"#,
    )
    .unwrap();
    let doc = figs_core::schema::resolve::resolve(raw.clone()).unwrap();
    let l = layout(&doc, &PerChar);
    let id = find_raw(&raw, |n| n.kind == NodeKind::Stack);
    let r = rect_of(&l, id);
    approx(r.w, 8.0 * CM);
    approx(r.h, 4.0 * CM);
}

#[test]
fn inset_example_lays_out() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let src = std::fs::read_to_string(examples.join("inset.figs")).unwrap();
    let doc = Document::from_script_in(&src, Some(&examples)).unwrap();
    let l = layout(&doc, &FontStore::new());
    // Three insets, each inside its panel.
    let page_w = l.page.width_pt;
    assert!(l.nodes.iter().all(|n| n.rect.x + n.rect.w <= page_w + 0.5));
}

// ---- dimension lines ----------------------------------------------------------------

#[test]
fn plot_dimension_labels_a_span_in_data_coordinates() {
    let gap = 8.0 * 0.012 * CM; // tick_label_pad() at 8 pt
    let p = Plotted::new(
        r#"page(12, 9, plot(#{ x: [0, 10], y: [0, 1], legend: "" }, [
            line_plot([0, 10], [0, 1]),
            plot_dimension(2, 6, 0.5, "D"),
            plot_dimension(2, 6, 0.25, "E", #{ below: true }),
            plot_dimension_v(5, 0.2, 0.8, "V"),
            plot_dimension_v(9, 0.2, 0.8, "W", #{ left: true }),
        ]).flex(1)).unit("cm")"#,
    );
    let area = p.area();
    let ax = |u: f32| area.x + area.w * u;
    let ay = |v: f32| area.y + area.h * (1.0 - v);
    let d = p.text("D");
    approx(d.x + d.w / 2.0, ax(0.4));
    approx(d.y + d.h, ay(0.5) - gap);
    let e = p.text("E");
    approx(e.y, ay(0.25) + gap);
    let v = p.text("V");
    approx(v.x, ax(0.5) + gap);
    approx(v.y + v.h / 2.0, ay(0.5));
    let w = p.text("W");
    approx(w.x + w.w, ax(0.9) - gap);
    // Double-headed arrows between the data points.
    let heads: Vec<_> = p
        .raw
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Arrow)
        .map(|n| (format!("{:?}", n.head), n.points.clone()))
        .collect();
    assert_eq!(heads.len(), 4);
    assert!(heads.iter().all(|(h, _)| h.contains("Both")), "{heads:?}");
    assert!(heads
        .iter()
        .any(|(_, pts)| pts[0] == [2.0, 0.5] && pts[1] == [6.0, 0.5]));
    for (src, want) in [
        (
            r#"plot_dimension(1, 2, 0.5, 3)"#,
            "the label must be a string",
        ),
        (
            r#"plot_dimension(1, 2, 0.5, "x", #{ below: 1 })"#,
            "must be true or false",
        ),
        (
            r#"plot_dimension(1, 2, 0.5, "x", #{ left: true })"#,
            "unknown option \"left\"",
        ),
        (
            r#"plot_dimension_v(1, 0, 1, "x", #{ below: true })"#,
            "unknown option \"below\"",
        ),
    ] {
        let e = eval_script(&format!("page(\"single\", plot([{src}]))"))
            .err()
            .unwrap_or_else(|| panic!("{src} should fail"))
            .to_string();
        assert!(e.contains(want), "{src}: {e}");
    }
}
