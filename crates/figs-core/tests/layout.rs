//! M2 layout-engine integration tests: containers, flex, alignment, nesting and
//! overflow, using rect leaves with explicit/flex sizing (no fonts or images).

use figs_core::geom::Rect;
use figs_core::layout::ComputedLayout;
use figs_core::{layout, Document, NullMeasurer};

fn lay(src: &str) -> ComputedLayout {
    let doc = Document::from_toml(src).expect("document should resolve");
    layout(&doc, &NullMeasurer)
}

fn rect_of(l: &ComputedLayout, id: &str) -> Rect {
    l.nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("node `{id}` not found in computed layout"))
        .rect
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

#[test]
fn four_panel_grid() {
    // 100x100 page, column of two equal-flex rows, each a row of two equal-flex
    // rects -> four 50x50 quadrants.
    let src = r#"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "column"
        cross_axis_alignment = "stretch"
        children = ["top", "bottom"]

        [nodes.top]
        type = "row"
        flex = 1
        cross_axis_alignment = "stretch"
        children = ["a", "b"]

        [nodes.bottom]
        type = "row"
        flex = 1
        cross_axis_alignment = "stretch"
        children = ["c", "d"]

        [nodes.a]
        type = "rect"
        flex = 1
        [nodes.b]
        type = "rect"
        flex = 1
        [nodes.c]
        type = "rect"
        flex = 1
        [nodes.d]
        type = "rect"
        flex = 1
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "a"), 0.0, 0.0, 50.0, 50.0);
    assert_rect(rect_of(&l, "b"), 50.0, 0.0, 50.0, 50.0);
    assert_rect(rect_of(&l, "c"), 0.0, 50.0, 50.0, 50.0);
    assert_rect(rect_of(&l, "d"), 50.0, 50.0, 50.0, 50.0);
}

#[test]
fn uneven_flex_1_2_1() {
    let src = r#"
        [page]
        width = 120
        height = 10
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "row"
        cross_axis_alignment = "stretch"
        children = ["x", "y", "z"]

        [nodes.x]
        type = "rect"
        flex = 1
        [nodes.y]
        type = "rect"
        flex = 2
        [nodes.z]
        type = "rect"
        flex = 1
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "x"), 0.0, 0.0, 30.0, 10.0);
    assert_rect(rect_of(&l, "y"), 30.0, 0.0, 60.0, 10.0);
    assert_rect(rect_of(&l, "z"), 90.0, 0.0, 30.0, 10.0);
}

#[test]
fn mixed_fixed_and_flex() {
    // row: flex 1, fixed 20, flex 1 over width 100 -> 40 | 20 | 40
    let src = r#"
        [page]
        width = 100
        height = 10
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "row"
        cross_axis_alignment = "stretch"
        children = ["l", "m", "r"]

        [nodes.l]
        type = "rect"
        flex = 1
        [nodes.m]
        type = "rect"
        width = 20
        [nodes.r]
        type = "rect"
        flex = 1
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "l"), 0.0, 0.0, 40.0, 10.0);
    assert_rect(rect_of(&l, "m"), 40.0, 0.0, 20.0, 10.0);
    assert_rect(rect_of(&l, "r"), 60.0, 0.0, 40.0, 10.0);
}

#[test]
fn nested_padding_and_spacing_accumulate() {
    // root column padding 10 -> content (10,10,80,80)
    //   inner row flex 1 padding 5 -> fills content, then inset 5 -> (15,15,70,70)
    //     leaf rect flex 1 stretch -> fills (15,15,70,70)
    let src = r#"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "column"
        padding = 10
        cross_axis_alignment = "stretch"
        children = ["inner"]

        [nodes.inner]
        type = "row"
        flex = 1
        padding = 5
        cross_axis_alignment = "stretch"
        children = ["leaf"]

        [nodes.leaf]
        type = "rect"
        flex = 1
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "inner"), 10.0, 10.0, 80.0, 80.0);
    assert_rect(rect_of(&l, "leaf"), 15.0, 15.0, 70.0, 70.0);
}

#[test]
fn main_axis_alignment_end_and_center() {
    // column height 100, single fixed-height (20) full-width rect.
    // end -> y = 80; center -> y = 40.
    let make = |align: &str| {
        format!(
            r#"
            [page]
            width = 50
            height = 100
            unit = "pt"
            root = "root"

            [nodes.root]
            type = "column"
            main_axis_alignment = "{align}"
            cross_axis_alignment = "stretch"
            children = ["box"]

            [nodes.box]
            type = "rect"
            height = 20
        "#
        )
    };
    let end = lay(&make("end"));
    assert_rect(rect_of(&end, "box"), 0.0, 80.0, 50.0, 20.0);
    let center = lay(&make("center"));
    assert_rect(rect_of(&center, "box"), 0.0, 40.0, 50.0, 20.0);
}

#[test]
fn cross_axis_alignment_center_and_end() {
    // row width 100, single fixed-width (20) rect; cross axis is vertical (height 40).
    // We instead test cross on a column: column width 100, fixed-width rect 20.
    let make = |align: &str| {
        format!(
            r#"
            [page]
            width = 100
            height = 40
            unit = "pt"
            root = "root"

            [nodes.root]
            type = "column"
            cross_axis_alignment = "{align}"
            children = ["box"]

            [nodes.box]
            type = "rect"
            width = 20
            height = 40
        "#
        )
    };
    let center = lay(&make("center"));
    assert_rect(rect_of(&center, "box"), 40.0, 0.0, 20.0, 40.0);
    let end = lay(&make("end"));
    assert_rect(rect_of(&end, "box"), 80.0, 0.0, 20.0, 40.0);
    let start = lay(&make("start"));
    assert_rect(rect_of(&start, "box"), 0.0, 0.0, 20.0, 40.0);
}

#[test]
fn space_between_distributes_gaps() {
    // column height 100, two fixed rects height 20 each -> leftover 60 split as
    // one gap of 60 between them. y = 0 and y = 20 + 60 = 80.
    let src = r#"
        [page]
        width = 50
        height = 100
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "column"
        main_axis_alignment = "space_between"
        cross_axis_alignment = "stretch"
        children = ["p", "q"]

        [nodes.p]
        type = "rect"
        height = 20
        [nodes.q]
        type = "rect"
        height = 20
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "p"), 0.0, 0.0, 50.0, 20.0);
    assert_rect(rect_of(&l, "q"), 0.0, 80.0, 50.0, 20.0);
}

#[test]
fn space_evenly_distributes_gaps() {
    // column height 100, two fixed rects height 20 each -> leftover 60 split into
    // 3 equal gaps of 20 (before, between, after). y = 20 and y = 20 + 20 + 20 = 60.
    let src = r#"
        [page]
        width = 50
        height = 100
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "column"
        main_axis_alignment = "space_evenly"
        cross_axis_alignment = "stretch"
        children = ["p", "q"]

        [nodes.p]
        type = "rect"
        height = 20
        [nodes.q]
        type = "rect"
        height = 20
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "p"), 0.0, 20.0, 50.0, 20.0);
    assert_rect(rect_of(&l, "q"), 0.0, 60.0, 50.0, 20.0);
}

#[test]
fn space_around_distributes_gaps() {
    // column height 100, two fixed rects height 20 each -> leftover 60, gap = 60/2
    // = 30 around each child (half-gap 15 at the ends). y = 15 and y = 15 + 20 + 30
    // = 65.
    let src = r#"
        [page]
        width = 50
        height = 100
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "column"
        main_axis_alignment = "space_around"
        cross_axis_alignment = "stretch"
        children = ["p", "q"]

        [nodes.p]
        type = "rect"
        height = 20
        [nodes.q]
        type = "rect"
        height = 20
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "p"), 0.0, 15.0, 50.0, 20.0);
    assert_rect(rect_of(&l, "q"), 0.0, 65.0, 50.0, 20.0);
}

#[test]
fn aspect_ratio_derives_cross_from_flex_main() {
    // column width 100; child flex 1 with aspect 2.0 (w/h). Main is height.
    // cross (width) = height * 2, but clamped to content cross (100).
    // height = 100 (fills), so width would be 200 -> clamped to 100.
    let src = r#"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "column"
        children = ["img"]

        [nodes.img]
        type = "rect"
        flex = 1
        aspect_ratio = 0.5
    "#;
    // aspect 0.5 => width = height * 0.5 = 50 when height = 100.
    let l = lay(src);
    let r = rect_of(&l, "img");
    assert_rect(r, 0.0, 0.0, 50.0, 100.0);
}

#[test]
fn overflow_does_not_panic_and_clamps() {
    // column height 50 with two fixed rects of height 40 (sum 80 > 50).
    // free clamps to 0; children keep their sizes and overflow downward.
    let src = r#"
        [page]
        width = 50
        height = 50
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "column"
        cross_axis_alignment = "stretch"
        children = ["p", "q"]

        [nodes.p]
        type = "rect"
        height = 40
        [nodes.q]
        type = "rect"
        height = 40
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "p"), 0.0, 0.0, 50.0, 40.0);
    assert_rect(rect_of(&l, "q"), 0.0, 40.0, 50.0, 40.0);
}

// ---- stack ----------------------------------------------------------------

/// Index of a node in paint order.
fn paint_index(l: &ComputedLayout, id: &str) -> usize {
    l.nodes
        .iter()
        .position(|n| n.id == id)
        .unwrap_or_else(|| panic!("node `{id}` not found in computed layout"))
}

#[test]
fn stack_overlaps_children_in_paint_order() {
    // Both children fill the padded content box (10,10,80,80) and overlap;
    // array order is paint order, so `top` paints after `base`.
    let src = r#"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "stack"
        padding = 10
        children = ["base", "top"]

        [nodes.base]
        type = "rect"
        [nodes.top]
        type = "rect"
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "base"), 10.0, 10.0, 80.0, 80.0);
    assert_rect(rect_of(&l, "top"), 10.0, 10.0, 80.0, 80.0);
    assert!(paint_index(&l, "root") < paint_index(&l, "base"));
    assert!(paint_index(&l, "base") < paint_index(&l, "top"));
}

#[test]
fn stack_offsets_place_border_box_after_margin() {
    // content box (10,10,80,80); label at x=5,y=7 with margin 2 and explicit
    // 20x10 size -> border box at (10+5+2, 10+7+2).
    let src = r#"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "stack"
        padding = 10
        children = ["label"]

        [nodes.label]
        type = "rect"
        x = 5
        y = 7
        margin = 2
        width = 20
        height = 10
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "label"), 17.0, 19.0, 20.0, 10.0);
}

#[test]
fn stack_offsets_convert_page_units() {
    // x/y are page lengths: 1 cm offset -> 28.3465 pt.
    let src = r#"
        [page]
        width = 10
        height = 10
        unit = "cm"
        root = "root"

        [nodes.root]
        type = "stack"
        children = ["dot"]

        [nodes.dot]
        type = "rect"
        x = 1
        y = 2
        width = 1
        height = 1
    "#;
    let l = lay(src);
    let cm = 72.0 / 2.54;
    assert_rect(rect_of(&l, "dot"), cm, 2.0 * cm, cm, cm);
}

#[test]
fn stack_fill_rule_is_per_axis() {
    // 100x80 stack:
    // - `base`: no offset, no size -> fills both axes (full-size base layer),
    // - `label`: positioned, no size -> intrinsic (0x0 under NullMeasurer),
    // - `band`: only `y` + height -> width still fills, height explicit,
    // - `side`: only `width`, no offsets -> height fills.
    let src = r#"
        [page]
        width = 100
        height = 80
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "stack"
        children = ["base", "label", "band", "side"]

        [nodes.base]
        type = "rect"
        [nodes.label]
        type = "rect"
        x = 4
        y = 4
        [nodes.band]
        type = "rect"
        y = 30
        height = 10
        [nodes.side]
        type = "rect"
        width = 15
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "base"), 0.0, 0.0, 100.0, 80.0);
    assert_rect(rect_of(&l, "label"), 4.0, 4.0, 0.0, 0.0);
    assert_rect(rect_of(&l, "band"), 0.0, 30.0, 100.0, 10.0);
    assert_rect(rect_of(&l, "side"), 0.0, 0.0, 15.0, 80.0);
}

#[test]
fn stack_explicit_size_and_aspect() {
    // - `sized`: explicit width + aspect pins height (= 40 / 2), no fill,
    // - `contain`: aspect only, no offsets -> largest 2:1 box in 100x80 = 100x50,
    // - `tall`: positioned at x = 60, aspect 0.5, no size -> height fills (80)
    //   and derives width = 40, exactly the space right of the offset.
    let src = r#"
        [page]
        width = 100
        height = 80
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "stack"
        children = ["sized", "contain", "tall"]

        [nodes.sized]
        type = "rect"
        width = 40
        aspect_ratio = 2.0
        [nodes.contain]
        type = "rect"
        aspect_ratio = 2.0
        [nodes.tall]
        type = "rect"
        x = 60
        aspect_ratio = 0.5
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "sized"), 0.0, 0.0, 40.0, 20.0);
    assert_rect(rect_of(&l, "contain"), 0.0, 0.0, 100.0, 50.0);
    assert_rect(rect_of(&l, "tall"), 60.0, 0.0, 40.0, 80.0);
}

#[test]
fn stack_wraps_content_when_measured() {
    // A stack inside a start-aligned column measures to its furthest child
    // extent + padding: max(30 + 2*1 + 20, 10) wide, max(5 + 2*1 + 10, 40) tall,
    // plus padding 3 on each side -> 58 x 46.
    let src = r#"
        [page]
        width = 200
        height = 200
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "column"
        children = ["s"]

        [nodes.s]
        type = "stack"
        padding = 3
        children = ["a", "b"]

        [nodes.a]
        type = "rect"
        x = 30
        y = 5
        margin = 1
        width = 20
        height = 10
        [nodes.b]
        type = "rect"
        width = 10
        height = 40
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "s"), 0.0, 0.0, 58.0, 46.0);
    assert_rect(rect_of(&l, "a"), 34.0, 9.0, 20.0, 10.0);
    assert_rect(rect_of(&l, "b"), 3.0, 3.0, 10.0, 40.0);
}

#[test]
fn stack_nested_in_row_and_offsets_ignored_by_row() {
    // row of [fixed 20 | flex stack]; the stack gets 80x50 at x=20. Its base
    // fills it, its label sits at (+10, +5). The fixed rect's x/y are ignored
    // because its parent is a row, not a stack.
    let src = r#"
        [page]
        width = 100
        height = 50
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "row"
        cross_axis_alignment = "stretch"
        children = ["fixed", "s"]

        [nodes.fixed]
        type = "rect"
        width = 20
        x = 99
        y = 99

        [nodes.s]
        type = "stack"
        flex = 1
        children = ["base", "label"]

        [nodes.base]
        type = "rect"
        [nodes.label]
        type = "rect"
        x = 10
        y = 5
        width = 8
        height = 4
    "#;
    let l = lay(src);
    assert_rect(rect_of(&l, "fixed"), 0.0, 0.0, 20.0, 50.0);
    assert_rect(rect_of(&l, "s"), 20.0, 0.0, 80.0, 50.0);
    assert_rect(rect_of(&l, "base"), 20.0, 0.0, 80.0, 50.0);
    assert_rect(rect_of(&l, "label"), 30.0, 5.0, 8.0, 4.0);
}
