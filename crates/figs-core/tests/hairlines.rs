//! `min_stroke_px`: the PNG backend draws shape strokes at least that many
//! device pixels wide (as PDF viewers do with hairlines); PDF/SVG keep the
//! exact widths.

use figs_core::{
    eval_script, layout, render_png, render_svg, Document, NullMeasurer, RenderOptions,
};
use tiny_skia::Pixmap;

/// A 100 x 40 pt page at 72 dpi (1 px = 1 pt): a 0.25 pt black line at
/// y = 10.5, and a column (x 20..80, y 20..36) with a 0.1 pt inside border.
fn page(min: &str) -> String {
    format!(
        r##"
        [page]
        width = 100
        height = 40
        unit = "pt"
        dpi = 72
        root = "s"
        {min}
        [nodes.s]
        type = "stack"
        children = ["l", "frame"]
        [nodes.l]
        type = "line"
        points = [[0, 10.5], [100, 10.5]]
        stroke_width = 0.25
        [nodes.frame]
        type = "column"
        x = 20
        y = 20
        width = 60
        height = 16
        stroke = "#000000"
        stroke_width = 0.1
        "##
    )
}

fn png(src: &str) -> Pixmap {
    let l = layout(&Document::from_toml(src).unwrap(), &NullMeasurer);
    Pixmap::decode_png(&render_png(&l, &RenderOptions::default()).unwrap()).unwrap()
}

/// Darkness 0..255 of pixel (x, y) (white page = 0).
fn ink(p: &Pixmap, x: u32, y: u32) -> u8 {
    255 - p.pixel(x, y).unwrap().demultiply().red()
}

#[test]
fn hairlines_are_faint_by_default_and_one_pixel_with_min_stroke() {
    let exact = png(&page(""));
    // 0.25 px of coverage: a faint grey.
    assert!(ink(&exact, 50, 10) < 100, "{}", ink(&exact, 50, 10));
    let crisp = png(&page("min_stroke_px = 1"));
    assert!(ink(&crisp, 50, 10) > 240, "{}", ink(&crisp, 50, 10));
    assert!(ink(&crisp, 50, 9) < 10 && ink(&crisp, 50, 11) < 10);
    // The inside border keeps its outer edge on the box edge: the 1 px line
    // is the pixel column just inside x = 20, nothing outside.
    assert!(ink(&crisp, 20, 28) > 240, "{}", ink(&crisp, 20, 28));
    assert!(ink(&crisp, 19, 28) < 10, "{}", ink(&crisp, 19, 28));
    assert!(ink(&crisp, 79, 28) > 240 && ink(&crisp, 80, 28) < 10);
    assert!(ink(&crisp, 50, 20) > 240 && ink(&crisp, 50, 19) < 10);
    // Thicker strokes are untouched.
    let thick = |min: &str| {
        page(min)
            .replace("stroke_width = 0.25", "stroke_width = 3")
            .replace("stroke_width = 0.1", "stroke_width = 2")
    };
    assert!(png(&thick("min_stroke_px = 1")).data() == png(&thick("")).data());
}

#[test]
fn min_stroke_scales_with_the_dpi_and_leaves_vector_output_alone() {
    // At 288 dpi one pixel is 0.25 pt: a 0.25 pt line is already that wide.
    let hi = |min: &str| {
        page(min)
            .replace("dpi = 72", "dpi = 288")
            .replace("stroke_width = 0.1", "stroke_width = 0.3")
    };
    assert!(png(&hi("min_stroke_px = 1")).data() == png(&hi("")).data());
    // Two pixels at 288 dpi: 0.5 pt.
    let p = png(&hi("min_stroke_px = 2"));
    let rows = (0..p.height()).filter(|&y| ink(&p, 40, y) > 200).count();
    assert_eq!(rows, 2, "a 2 px line");
    let svg = |src: &str| {
        let l = layout(&Document::from_toml(src).unwrap(), &NullMeasurer);
        render_svg(&l, &RenderOptions::default())
    };
    assert_eq!(svg(&page("min_stroke_px = 1")), svg(&page("")));
}

#[test]
fn min_stroke_from_script_and_validation() {
    let raw = eval_script(r#"page(10, 10, rect()).min_stroke_px(1)"#).unwrap();
    assert_eq!(raw.page.min_stroke_px, Some(1.0));
    assert!(eval_script(r#"page(10, 10, rect()).min_stroke_px(-1)"#).is_err());
    let e = Document::from_toml(&page("min_stroke_px = -2"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("min_stroke_px"), "{e}");
    let l = layout(
        &Document::from_toml(&page("min_stroke_px = 1.5")).unwrap(),
        &NullMeasurer,
    );
    assert_eq!(l.page.min_stroke_px, 1.5);
}
