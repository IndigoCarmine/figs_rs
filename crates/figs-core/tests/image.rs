//! Image backend tests. Authors a 4-quadrant source PNG with tiny-skia, then
//! renders it under each ImageFit and samples pixels to confirm placement.

use std::path::{Path, PathBuf};

use figs_core::{layout, render_png, Assets, Document, NullMeasurer};
use tiny_skia::Pixmap;

const RED: (u8, u8, u8) = (224, 0, 0);
const GREEN: (u8, u8, u8) = (0, 160, 0);
const BLUE: (u8, u8, u8) = (0, 0, 224);
const YELLOW: (u8, u8, u8) = (224, 200, 0);

/// Write an 80x80 image split into four solid 40x40 quadrants.
fn make_quadrant_png(path: &Path) {
    let mut p = Pixmap::new(80, 80).unwrap();
    let data = p.data_mut();
    for y in 0..80u32 {
        for x in 0..80u32 {
            let c = match (x < 40, y < 40) {
                (true, true) => RED,
                (false, true) => GREEN,
                (true, false) => BLUE,
                (false, false) => YELLOW,
            };
            let i = ((y * 80 + x) * 4) as usize;
            // opaque -> premultiplied == straight
            data[i] = c.0;
            data[i + 1] = c.1;
            data[i + 2] = c.2;
            data[i + 3] = 255;
        }
    }
    std::fs::write(path, p.encode_png().unwrap()).unwrap();
}

fn unique_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("figs_test_{tag}_{}.png", std::process::id()))
}

fn render_doc(src: &str) -> Pixmap {
    let doc = Document::from_toml(src).unwrap();
    // NullMeasurer is fine for layout here (sizes come from page/flex/aspect);
    // images are decoded by Assets at render time.
    let computed = layout(&doc, &NullMeasurer);
    let assets = Assets::new(".");
    let bytes = render_png(&computed, &assets).unwrap();
    Pixmap::decode_png(&bytes).unwrap()
}

#[track_caller]
fn assert_near(p: &Pixmap, x: u32, y: u32, rgb: (u8, u8, u8), tol: i32) {
    let px = p.pixel(x, y).unwrap();
    let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
    assert!(
        d(px.red(), rgb.0) <= tol && d(px.green(), rgb.1) <= tol && d(px.blue(), rgb.2) <= tol,
        "pixel ({x},{y}) = ({},{},{}), expected ~({},{},{})",
        px.red(),
        px.green(),
        px.blue(),
        rgb.0,
        rgb.1,
        rgb.2
    );
}

#[test]
fn fill_maps_image_one_to_one() {
    let img = unique_path("fill");
    make_quadrant_png(&img);
    let src = format!(
        r#"
        [page]
        width = 80
        height = 80
        unit = "pt"
        dpi = 72
        root = "root"
        [nodes.root]
        type = "image"
        src = "{}"
        fit = "fill"
    "#,
        img.display()
    );
    let p = render_doc(&src);
    // 80x80 image into 80x80 box -> 1:1, quadrant centers exact.
    assert_near(&p, 20, 20, RED, 4);
    assert_near(&p, 60, 20, GREEN, 4);
    assert_near(&p, 20, 60, BLUE, 4);
    assert_near(&p, 60, 60, YELLOW, 4);
    std::fs::remove_file(&img).ok();
}

#[test]
fn contain_letterboxes_and_centers() {
    let img = unique_path("contain");
    make_quadrant_png(&img);
    // Wide page; square image -> scaled to 80x80, centered in x at [40,120].
    let src = format!(
        r##"
        [page]
        width = 160
        height = 80
        unit = "pt"
        dpi = 72
        background = "#ffffff"
        root = "root"
        [nodes.root]
        type = "image"
        src = "{}"
        fit = "contain"
    "##,
        img.display()
    );
    let p = render_doc(&src);
    // Letterbox: far left/right are background white.
    assert_near(&p, 10, 40, (255, 255, 255), 2);
    assert_near(&p, 150, 40, (255, 255, 255), 2);
    // Interior: dest (60,20) -> image left/top quadrant (red).
    assert_near(&p, 60, 20, RED, 30);
    std::fs::remove_file(&img).ok();
}

#[test]
fn cover_fills_box_with_no_gaps() {
    let img = unique_path("cover");
    make_quadrant_png(&img);
    let src = format!(
        r##"
        [page]
        width = 160
        height = 80
        unit = "pt"
        dpi = 72
        background = "#ffffff"
        root = "root"
        [nodes.root]
        type = "image"
        src = "{}"
        fit = "cover"
    "##,
        img.display()
    );
    let p = render_doc(&src);
    // Cover must leave no background: scan for pure-white pixels.
    let white = p
        .pixels()
        .iter()
        .filter(|px| px.red() == 255 && px.green() == 255 && px.blue() == 255)
        .count();
    assert_eq!(white, 0, "cover left {white} background pixels");
    // Left and right halves remain distinguishable.
    let left = p.pixel(40, 20).unwrap();
    let right = p.pixel(120, 20).unwrap();
    assert!(
        left.red() > right.red(),
        "expected red-ish left vs green-ish right"
    );
    std::fs::remove_file(&img).ok();
}
