//! Data plots: the core `viewbox` (user coordinates on stacks and shapes) and
//! `anchor` (which point of a stack child sits at its offset) that plots are
//! built on, the prelude's tick generation and `plot` layout, `csv()` loading,
//! and rendering the bundled `examples/plot.figs`.

#![cfg(feature = "script")]

use std::path::{Path, PathBuf};

use figs_core::geom::{PathSegment, Point, Rect};
use figs_core::layout::{ComputedLayout, PaintContent};
use figs_core::schema::{NodeKind, RawDocument};
use figs_core::{
    eval_script, eval_script_in, layout, render_png, render_svg, Document, FontStore, NullMeasurer,
    RenderOptions,
};

fn lay_toml(src: &str) -> ComputedLayout {
    let d = Document::from_toml(src).unwrap_or_else(|e| panic!("resolve failed: {e}"));
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
    assert_eq!(rect.anchor, Some([0.5, 1.0]));

    let toml = toml::to_string(&raw).unwrap();
    assert!(toml.contains("viewbox = [0.0, 1.0, 10.0, 0.0]"), "{toml}");
    assert!(toml.contains("anchor = [0.5, 1.0]"), "{toml}");
    let back: RawDocument = toml::from_str(&toml).unwrap();
    assert_eq!(back.nodes["root"].viewbox, raw.nodes["root"].viewbox);
    // Nodes without them serialize without the keys.
    let plain = toml::to_string(&eval_script("page(1, 1, rect())").unwrap()).unwrap();
    assert!(!plain.contains("viewbox") && !plain.contains("anchor"));
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

    // The data area: the stack whose viewbox is [x0, y1, x1, y0].
    let area_id = find_raw(&raw, |n| {
        n.kind == NodeKind::Stack && n.viewbox == Some([0.0, 1.0, 10.0, 0.0])
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
