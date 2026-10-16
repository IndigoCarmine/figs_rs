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
    assert!(
        bytes.len() > 1000,
        "expected a real PNG, got {} bytes",
        bytes.len()
    );
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
    assert!(
        dark > 30,
        "expected rasterized glyphs, got {dark} dark pixels"
    );
    let centroid = (sum_x / dark as u64) as u32;
    // Glyph ink should sit around the horizontal centre (width 200), not flush
    // against an edge.
    assert!(
        (40..=160).contains(&centroid),
        "text not centered: centroid x = {centroid} (extent {min_x}..{max_x})"
    );
}

#[test]
fn auto_height_page_renders_at_content_height() {
    // 100 pt wide at 72 dpi; a 30 pt band + 10 pt gap + 20 pt band -> 100x60 px.
    let src = r##"
        [page]
        width = 100
        height = "auto"
        unit = "pt"
        dpi = 72
        background = "#ffffff"
        root = "root"
        [nodes.root]
        type = "column"
        spacing = 10
        cross_axis_alignment = "stretch"
        children = ["a", "b"]
        [nodes.a]
        type = "rect"
        height = 30
        fill = "#ff0000"
        [nodes.b]
        type = "rect"
        height = 20
        fill = "#0000ff"
    "##;
    let p = render(src);
    assert_eq!((p.width(), p.height()), (100, 60));
    assert_pixel(&p, 50, 15, (255, 0, 0));
    assert_pixel(&p, 50, 35, (255, 255, 255));
    assert_pixel(&p, 50, 50, (0, 0, 255));
}

/// A 72-dpi page with one big black "H" (white background), optionally
/// outlined by the TOML lines in `stroke`. Returns the pixmap and the text
/// node's rect.
fn render_h(stroke: &str) -> (Pixmap, figs_core::geom::Rect) {
    let src = format!(
        r##"
        [page]
        width = 120
        height = 100
        unit = "pt"
        dpi = 72
        background = "#ffffff"
        root = "root"
        [nodes.root]
        type = "stack"
        children = ["t"]
        [nodes.t]
        type = "text"
        content = "H"
        font_size = 60
        font_weight = 700
        color = "#000000"
        x = 30
        y = 15
        {stroke}
    "##
    );
    let doc = Document::from_toml(&src).expect("resolve");
    let fonts = FontStore::new();
    let computed = layout(&doc, &fonts);
    let rect = computed.nodes.iter().find(|n| n.id == "t").unwrap().rect;
    let opts = RenderOptions {
        fonts: Some(&fonts),
        base_dir: std::path::Path::new("."),
    };
    let bytes = render_png(&computed, &opts).expect("render png");
    (Pixmap::decode_png(&bytes).expect("decode png"), rect)
}

#[test]
fn text_stroke_paints_behind_the_fill() {
    let (plain, plain_rect) = render_h("");
    let (outlined, rect) = render_h("stroke = \"#ff0000\"\nstroke_width = 6");
    // Purely visual: the laid-out box is unchanged.
    assert_eq!(rect, plain_rect);

    let rgb = |p: &Pixmap, x: u32, y: u32| {
        let c = p.pixel(x, y).unwrap();
        (c.red(), c.green(), c.blue())
    };
    let is_black = |c: (u8, u8, u8)| c.0 < 10 && c.1 < 10 && c.2 < 10;
    let is_white = |c: (u8, u8, u8)| c.0 > 250 && c.1 > 250 && c.2 > 250;
    let (w, h) = (plain.width(), plain.height());
    let near_ink = |x: u32, y: u32| {
        (x.saturating_sub(2)..=(x + 2).min(w - 1)).any(|nx| {
            (y.saturating_sub(2)..=(y + 2).min(h - 1)).any(|ny| is_black(rgb(&plain, nx, ny)))
        })
    };
    let (mut interior, mut rim) = (0, 0);
    for y in 0..h {
        for x in 0..w {
            let before = rgb(&plain, x, y);
            let after = rgb(&outlined, x, y);
            if is_black(before) {
                // The glyph interior keeps the fill colour (stroke underneath).
                assert!(is_black(after), "interior ({x},{y}) became {after:?}");
                interior += 1;
            } else if is_white(before) && near_ink(x, y) {
                // Just outside the glyph edge (within 2 px, inside the 3 pt
                // reach): the outline colour.
                assert!(
                    after.0 > 200 && after.1 < 60 && after.2 < 60,
                    "rim ({x},{y}) is {after:?}, expected red"
                );
                rim += 1;
            }
        }
    }
    assert!(
        interior > 200 && rim > 100,
        "interior {interior}, rim {rim}"
    );
    // Far from the glyph the page is untouched.
    assert_pixel(&outlined, 2, 2, (255, 255, 255));
}
