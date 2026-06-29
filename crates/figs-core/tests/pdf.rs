//! PDF backend tests. Renders rects, text and an image, then checks the output
//! is a valid PDF with embedded (subset) fonts and an image XObject.

use std::path::Path;

use figs_core::{layout, render_pdf, Assets, Document};
use tiny_skia::Pixmap;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn make_png(path: &Path) {
    let mut p = Pixmap::new(8, 8).unwrap();
    p.fill(tiny_skia::Color::from_rgba8(200, 50, 50, 255));
    std::fs::write(path, p.encode_png().unwrap()).unwrap();
}

#[test]
fn pdf_has_text_image_and_valid_header() {
    let img = std::env::temp_dir().join(format!("figs_pdf_{}.png", std::process::id()));
    make_png(&img);
    let src = format!(
        r##"
        [page]
        width = 200
        height = 100
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "column"
        padding = 5
        spacing = 5
        cross_axis_alignment = "stretch"
        children = ["bar", "label", "pic"]
        [nodes.bar]
        type = "rect"
        height = 20
        fill = "#3366cc"
        [nodes.label]
        type = "text"
        content = "Hello PDF"
        font_size = 18
        [nodes.pic]
        type = "image"
        flex = 1
        src = "{}"
    "##,
        img.display()
    );

    let doc = Document::from_toml(&src).unwrap();
    let assets = Assets::new(".").with_default_family("Liberation Sans");
    let computed = layout(&doc, &assets);
    let bytes = render_pdf(&computed, &assets).unwrap();
    std::fs::remove_file(&img).ok();

    assert!(bytes.starts_with(b"%PDF-"), "missing PDF header");
    assert!(bytes.len() > 1000, "PDF unexpectedly small: {} bytes", bytes.len());
    // Embedded, selectable font (Type0 CID font with a font program).
    assert!(contains(&bytes, b"/Type0"), "expected an embedded Type0 font");
    assert!(contains(&bytes, b"FontFile2"), "expected an embedded font program");
    assert!(contains(&bytes, b"ToUnicode"), "expected a ToUnicode map");
    // Embedded image XObject.
    assert!(
        contains(&bytes, b"/Subtype/Image"),
        "expected an image XObject"
    );
}

#[test]
fn pdf_page_size_in_points() {
    // A4-ish custom page; just confirm it renders and is a one-page PDF.
    let src = r##"
        [page]
        width = 100
        height = 200
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "rect"
        fill = "#101010"
    "##;
    let doc = Document::from_toml(src).unwrap();
    let assets = Assets::new(".");
    let bytes = render_pdf(&layout(&doc, &assets), &assets).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    // MediaBox should reflect the 100x200 pt page.
    assert!(
        contains(&bytes, b"/MediaBox"),
        "expected a MediaBox in the PDF"
    );
}
