//! Core layout for coordinate-free figures: `cross_axis_alignment = "axis"`
//! (children lined up by their declared / propagated axis, `.axis()`),
//! `equal_height` rows (children that scale with their width share one
//! height and fill the row) and `overflow` (`visible` / `clip` / `shrink`,
//! every overflow reported by name in the page unit).

#![cfg(feature = "script")]

use figs_core::geom::{Axis, AxisMark, Overflow, Rect, Size};
use figs_core::layout::{ComputedLayout, OverflowAction};
use figs_core::schema::{ImageProps, NodeType, TextProps};
use figs_core::units::Unit;
use figs_core::{eval_script, layout, Document, LeafMeasure, NullMeasurer};

/// Every text block measures `TEXT_W` x `TEXT_H` pt.
struct FixedText;
const TEXT_W: f32 = 20.0;
const TEXT_H: f32 = 10.0;
impl LeafMeasure for FixedText {
    fn measure_text(&self, _t: &TextProps, _max_width: f32) -> Size {
        Size::new(TEXT_W, TEXT_H)
    }
    fn measure_image(&self, _i: &ImageProps) -> Size {
        Size::default()
    }
}

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &FixedText)
}

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-2, "expected {b}, got {a}");
}

/// The rect of the `n`-th node (pre-order) whose box is `w` wide and `h`
/// tall — the sized rects of a test page are unique.
fn sized(l: &ComputedLayout, w: f32, h: f32) -> Rect {
    l.nodes
        .iter()
        .map(|n| n.rect)
        .find(|r| (r.w - w).abs() < 1e-3 && (r.h - h).abs() < 1e-3)
        .unwrap_or_else(|| panic!("no {w} x {h} box"))
}

fn text_rects(l: &ComputedLayout) -> Vec<Rect> {
    l.nodes
        .iter()
        .filter(|n| matches!(n.content, figs_core::layout::PaintContent::Text(_)))
        .map(|n| n.rect)
        .collect()
}

// ---- axis alignment ---------------------------------------------------------------

#[test]
fn row_lines_up_marked_axes_not_box_centres() {
    // A 20-tall icon with a caption above (10 + 2) next to a 40-tall block:
    // the icon's middle and the block's middle share one line, and the
    // group (20 above the line, 20 below it) is centred in the 100 pt row.
    let l = lay(r#"page(200, 100, row([
            col([text("cap"), rect().width(10).height(20).axis()]).spacing(2),
            rect().width(30).height(40),
        ]).cross("axis")).unit("pt")"#);
    let icon = sized(&l, 10.0, 20.0);
    let block = sized(&l, 30.0, 40.0);
    approx(icon.y + icon.h / 2.0, block.y + block.h / 2.0);
    // Above the line: max(12 + 10, 20) = 22; below: max(10, 20) = 20.
    approx(block.y + 20.0, (100.0 - 42.0) / 2.0 + 22.0);
}

#[test]
fn unmarked_children_use_their_box_middle() {
    // Without any mark the axis is the box middle: "axis" then centres.
    let a = lay(
        r#"page(100, 100, row([rect().width(10).height(20), rect().width(10).height(60)])
            .cross("axis")).unit("pt")"#,
    );
    let b = lay(
        r#"page(100, 100, row([rect().width(10).height(20), rect().width(10).height(60)])
            .cross("center")).unit("pt")"#,
    );
    assert_eq!(sized(&a, 10.0, 20.0), sized(&b, 10.0, 20.0));
    assert_eq!(sized(&a, 10.0, 60.0), sized(&b, 10.0, 60.0));
}

#[test]
fn axis_propagates_through_nesting_and_fractions() {
    // The mark sits two containers down; `.axis(fx, fy)` puts it at a
    // fraction of the box (here a quarter of the way down a 40-tall rect).
    let l = lay(r#"page(200, 100, row([
            row([col([text("t"), rect().width(10).height(40).axis(0.5, 0.25)]).spacing(5)]),
            rect().width(30).height(6),
        ]).cross("axis")).unit("pt")"#);
    let tall = sized(&l, 10.0, 40.0);
    let flat = sized(&l, 30.0, 6.0);
    approx(tall.y + 10.0, flat.y + 3.0);
}

#[test]
fn column_lines_up_vertical_axes() {
    // A column by "axis": the icon of a row [label, icon] sits under the
    // middle of the plain rect below it, the label hanging to its left.
    let l = lay(r#"page(200, 100, col([
            row([text("left"), rect().width(10).height(10).axis()]).spacing(4),
            rect().width(30).height(6),
        ]).cross("axis")).unit("pt")"#);
    let icon = sized(&l, 10.0, 10.0);
    let bar = sized(&l, 30.0, 6.0);
    approx(icon.x + 5.0, bar.x + 15.0);
    // Left of the line: max(20 + 4 + 5, 15) = 29; right: max(5, 15) = 15.
    approx(icon.x + 5.0, (200.0 - 44.0) / 2.0 + 29.0);
    let t = text_rects(&l)[0];
    approx(t.x + t.w + 4.0, icon.x);
}

#[test]
fn axis_round_trips_through_toml() {
    let raw = eval_script(
        r#"page(10, 10, row([rect().axis(), rect().axis(0.5, 0.25)]).cross("axis").name("r"))"#,
    )
    .unwrap();
    let toml = toml::to_string(&raw).unwrap();
    assert!(toml.contains("axis = true"), "{toml}");
    assert!(toml.contains("axis = [0.5, 0.25]"), "{toml}");
    assert!(toml.contains(r#"cross_axis_alignment = "axis""#), "{toml}");
    assert!(toml.contains(r#"name = "r""#), "{toml}");
    let back = Document::from_toml(&toml).unwrap();
    let marks: Vec<_> = back.nodes.iter().filter_map(|n| n.common.axis).collect();
    assert!(marks.contains(&AxisMark::Auto));
    assert!(marks.contains(&AxisMark::At([0.5, 0.25])));
    assert!(back.nodes.iter().any(|n| n.name.as_deref() == Some("r")));
    // `axis = false` is not a thing.
    let bad = toml.replace("axis = true", "axis = false");
    assert!(Document::from_toml(&bad).is_err());
}

// ---- equal-height rows --------------------------------------------------------------

#[test]
fn equal_height_row_shares_one_height_and_fills() {
    // Aspects 2 and 1 in 300 pt with a 10 pt gap: H = 290 / 3.
    let l = lay(
        r#"page(300, 400, col([row([rect().aspect(2), rect().aspect(1)]).spacing(10)
            .equal_height()]).stretch()).unit("pt")"#,
    );
    let h = 290.0 / 3.0;
    let a = sized(&l, 2.0 * h, h);
    let b = sized(&l, h, h);
    approx(a.x, 0.0);
    approx(b.x + b.w, 300.0);
}

#[test]
fn equal_height_keeps_text_natural_and_solves_captions() {
    // A text item keeps its 20 pt; a captioned panel (aspect 1 over a 10 pt
    // caption) and a plain aspect-1 rect end up the same total height.
    let l = lay(r#"page(220, 400, col([row([
            text("t"),
            col([rect().aspect(1), text("c")]).stretch(),
            rect().aspect(1),
        ]).equal_height()]).stretch()).unit("pt")"#);
    // 20 + (H - 10) + H = 220  ->  H = 105.
    let panel = sized(&l, 95.0, 95.0);
    let plain = sized(&l, 105.0, 105.0);
    approx(panel.x, 20.0);
    approx(plain.x + plain.w, 220.0);
}

#[test]
fn equal_height_measures_on_auto_pages_and_errors_elsewhere() {
    let d = Document::from_script(
        r#"page(300, "auto", row([rect().aspect(1.5), rect().aspect(1.5)]).equal_height())
            .unit("pt")"#,
    )
    .unwrap();
    let l = layout(&d, &NullMeasurer);
    approx(l.page.height_pt, 100.0);
    let e = eval_script(r#"page(1, 1, col([]).equal_height())"#).unwrap_err();
    assert!(
        e.message.contains("`equal_height` applies to a row"),
        "{}",
        e.message
    );
    let bad = r#"
        [page]
        width = 10
        height = 10
        root = "c"
        [nodes.c]
        type = "column"
        equal_height = true
    "#;
    assert!(Document::from_toml(bad).is_err());
}

// ---- overflow -------------------------------------------------------------------------

const TALL: &str = r#"page(100, 100, col([
        col([rect().width(10).height(30).name("a"), rect().width(10).height(50).name("b")])
            .spacing(10).name("box")
            .height(60)OVERFLOW,
        rect().height(10),
    ]).stretch()).unit("cm")"#;

fn tall(overflow: &str) -> ComputedLayout {
    lay(&TALL.replace("OVERFLOW", overflow))
}

#[test]
fn overflow_is_reported_by_name_in_the_page_unit() {
    let l = tall("");
    assert_eq!(l.overflows.len(), 1, "{:?}", l.overflows);
    let o = &l.overflows[0];
    assert_eq!(o.name.as_deref(), Some("box"));
    assert_eq!(o.axis, Axis::Vertical);
    assert_eq!(o.action, OverflowAction::Visible);
    let cm = Unit::Cm.to_pt();
    approx(o.need_pt / cm, 90.0);
    approx(o.room_pt / cm, 60.0);
    let m = o.message(Unit::Cm);
    assert!(
        m.starts_with("column box (") && m.contains("30.00 cm too tall") && m.contains("runs past"),
        "{m}"
    );
    assert!(m.contains("[a 30.00 cm, b 50.00 cm]"), "{m}");
    // Nothing is clipped by default.
    assert!(l.nodes.iter().all(|n| n.clips.is_empty()));
}

#[test]
fn unnamed_overflow_names_its_named_ancestor() {
    let l = lay(
        r#"page(100, 100, col([col([col([rect().height(40), rect().height(40)]).height(50)])])
            .name("outer")).unit("pt")"#,
    );
    let m = l.overflows[0].message(Unit::Pt);
    assert!(m.contains(" in outer:"), "{m}");
}

#[test]
fn overflow_clip_clips_only_when_overflowing() {
    let l = tall(r#".overflow("clip")"#);
    assert_eq!(l.overflows[0].action, OverflowAction::Clipped);
    let cm = Unit::Cm.to_pt();
    let b = sized(&l, 10.0 * cm, 50.0 * cm);
    let clipped = l.nodes.iter().find(|n| n.rect == b).unwrap();
    assert_eq!(clipped.clips.len(), 1);
    approx(clipped.clips[0].rect.h / cm, 60.0);
    // A container that fits paints exactly as a visible one: no clip.
    let l = lay(&TALL
        .replace("OVERFLOW", r#".overflow("clip")"#)
        .replace("height(50)", "height(10)"));
    assert!(l.overflows.is_empty());
    assert!(l.nodes.iter().all(|n| n.clips.is_empty()));
}

#[test]
fn overflow_shrink_scales_the_children_into_the_box() {
    let l = tall(r#".overflow("shrink")"#);
    let o = &l.overflows[0];
    let s = 60.0 / 90.0;
    let OverflowAction::Shrunk(got) = o.action else {
        panic!("{:?}", o.action)
    };
    approx(got, s);
    let m = o.message(Unit::Cm);
    assert!(m.contains("scaled to 67% to fit"), "{m}");
    // The children are laid out unscaled (full width, natural heights) under
    // a uniform scale about the box's top-left, centred across.
    let cm = Unit::Cm.to_pt();
    let b = l
        .nodes
        .iter()
        .find(|n| (n.rect.h - 50.0 * cm).abs() < 1e-2 && n.transform.is_some())
        .expect("scaled child");
    let t = b.transform.unwrap();
    approx(t.a, s);
    approx(t.d, s);
    let bottom = t.apply(figs_core::geom::Point::new(b.rect.x, b.rect.y + b.rect.h));
    approx(bottom.y / cm, 60.0);
    let left = t.apply(figs_core::geom::Point::new(b.rect.x, b.rect.y));
    approx(left.x / cm, 100.0 * (1.0 - s) / 2.0);
    approx(b.rect.w / cm, 10.0);
}

#[test]
fn overflow_setter_and_resolve_errors() {
    let e = eval_script(r#"page(1, 1, col([]).overflow("hide"))"#).unwrap_err();
    assert!(
        e.message
            .contains("unknown mode `hide` (visible, clip, shrink, fit)"),
        "{}",
        e.message
    );
    let e = eval_script(r#"page(1, 1, stack([]).overflow("clip"))"#).unwrap_err();
    assert!(
        e.message.contains("`overflow` applies to a col/row"),
        "{}",
        e.message
    );
    let raw = eval_script(r#"page(1, 1, col([]).overflow("shrink"))"#).unwrap();
    let toml = toml::to_string(&raw).unwrap();
    assert!(toml.contains(r#"overflow = "shrink""#), "{toml}");
    let back = Document::from_toml(&toml).unwrap();
    assert!(back.nodes.iter().any(|n| matches!(
        n.kind,
        NodeType::Container {
            overflow: Overflow::Shrink,
            ..
        }
    )));
}
