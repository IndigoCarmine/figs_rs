//! PNG backend tests: render small layouts at 72 dpi (so points == pixels) and
//! sample pixels to confirm geometry and colors.

use figs_core::{layout, render_png, Document, FontStore, NullMeasurer, RenderOptions};
use tiny_skia::Pixmap;

fn render(src: &str) -> Pixmap {
    let doc = Document::from_toml(src).expect("resolve");
    let computed = layout(&doc, &NullMeasurer);
    let bytes = render_png(&computed, &RenderOptions::default()).expect("render png");
    Pixmap::decode_png(&bytes).expect("decode png")
}

#[track_caller]
fn assert_pixel(p: &Pixmap, x: u32, y: u32, rgb: (u8, u8, u8)) {
    let px = p.pixel(x, y).expect("pixel in bounds");
    // Opaque fills: premultiplied == straight. Allow ±2 for AA/rounding.
    let near = |a: u8, b: u8| (a as i32 - b as i32).abs() <= 2;
    assert!(
        near(px.red(), rgb.0) && near(px.green(), rgb.1) && near(px.blue(), rgb.2),
        "pixel ({x},{y}) = ({},{},{}), expected ({},{},{})",
        px.red(),
        px.green(),
        px.blue(),
        rgb.0,
        rgb.1,
        rgb.2
    );
}

#[test]
fn page_dimensions_match_dpi() {
    // 100x50 pt at 72 dpi -> 100x50 px.
    let src = r##"
        [page]
        width = 100
        height = 50
        unit = "pt"
        dpi = 72
        root = "root"
        [nodes.root]
        type = "rect"
        fill = "#000000"
    "##;
    let p = render(src);
    assert_eq!(p.width(), 100);
    assert_eq!(p.height(), 50);
}

#[test]
fn background_is_painted() {
    let src = r##"
        [page]
        width = 10
        height = 10
        unit = "pt"
        dpi = 72
        background = "#00ff00"
        root = "root"
        [nodes.root]
        type = "column"
    "##;
    let p = render(src);
    assert_pixel(&p, 5, 5, (0, 255, 0));
}

#[test]
fn four_color_quadrants() {
    // 100x100 px, four flex rects -> sample each quadrant center.
    let src = r##"
        [page]
        width = 100
        height = 100
        unit = "pt"
        dpi = 72
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
        fill = "#e0635a"
        [nodes.b]
        type = "rect"
        flex = 1
        fill = "#6aa9e0"
        [nodes.c]
        type = "rect"
        flex = 1
        fill = "#7bbf6a"
        [nodes.d]
        type = "rect"
        flex = 1
        fill = "#e0c25a"
    "##;
    let p = render(src);
    assert_pixel(&p, 25, 25, (0xe0, 0x63, 0x5a));
    assert_pixel(&p, 75, 25, (0x6a, 0xa9, 0xe0));
    assert_pixel(&p, 25, 75, (0x7b, 0xbf, 0x6a));
    assert_pixel(&p, 75, 75, (0xe0, 0xc2, 0x5a));
}

#[test]
fn four_panel_example_renders() {
    // The bundled example renders to a non-trivial PNG without panicking.
    let src = include_str!("../../../examples/four_panel.toml");
    let doc = Document::from_toml(src).unwrap();
    let computed = layout(&doc, &NullMeasurer);
    let bytes = render_png(&computed, &RenderOptions::default()).unwrap();
    assert!(bytes.len() > 1000, "expected a real PNG, got {} bytes", bytes.len());
}

#[test]
fn centered_text_rasterizes() {
    // A single centered text node on a white page. With fonts supplied, real
    // glyphs must appear — and roughly centered horizontally.
    let src = r##"
        [page]
        width = 200
        height = 80
        unit = "pt"
        dpi = 72
        background = "#ffffff"
        root = "t"
        [nodes.t]
        type = "text"
        content = "Hello"
        font_size = 40
        color = "#000000"
        align = "center"
    "##;
    let doc = Document::from_toml(src).expect("resolve");
    let fonts = FontStore::new();
    let computed = layout(&doc, &fonts);
    let opts = RenderOptions {
        fonts: Some(&fonts),
        base_dir: std::path::Path::new("."),
    };
    let bytes = render_png(&computed, &opts).expect("render png");
    let p = Pixmap::decode_png(&bytes).expect("decode png");

    // Count dark (text) pixels and track their horizontal extent.
    let (mut dark, mut sum_x, mut min_x, mut max_x) = (0u32, 0u64, u32::MAX, 0u32);
    for y in 0..p.height() {
        for x in 0..p.width() {
            let px = p.pixel(x, y).unwrap();
            if px.red() < 128 && px.green() < 128 && px.blue() < 128 {
                dark += 1;
                sum_x += x as u64;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
        }
    }
    assert!(dark > 30, "expected rasterized glyphs, got {dark} dark pixels");
    let centroid = (sum_x / dark as u64) as u32;
    // Glyph ink should sit around the horizontal centre (width 200), not flush
    // against an edge.
    assert!(
        (40..=160).contains(&centroid),
        "text not centered: centroid x = {centroid} (extent {min_x}..{max_x})"
    );
}
