//! SVG backend tests: the output is well-formed XML (parsed with roxmltree) and
//! carries the expected elements at the expected point coordinates.

use std::path::{Path, PathBuf};

use figs_core::{layout, render_svg, Document, FontStore, NullMeasurer, RenderOptions};

fn svg_of(src: &str) -> String {
    let doc = Document::from_toml(src).expect("resolve");
    let computed = layout(&doc, &NullMeasurer);
    render_svg(&computed, &RenderOptions::default())
}

fn svg_with(doc: &Document, fonts: &FontStore, base_dir: &Path) -> String {
    let computed = layout(doc, fonts);
    let opts = RenderOptions {
        fonts: Some(fonts),
        base_dir,
    };
    render_svg(&computed, &opts)
}

/// Elements with the given tag name, in document order.
fn elements<'a>(doc: &'a roxmltree::Document<'a>, tag: &str) -> Vec<roxmltree::Node<'a, 'a>> {
    doc.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == tag)
        .collect()
}

/// A fresh scratch directory under the system temp dir.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("figs-svg-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn root_is_sized_in_points() {
    let svg = svg_of(
        r##"
        [page]
        width = 100
        height = 50
        unit = "pt"
        root = "root"
        background = "#00ff00"
        [nodes.root]
        type = "column"
        "##,
    );
    let doc = roxmltree::Document::parse(&svg).expect("well-formed SVG");
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "svg");
    assert_eq!(root.attribute("width"), Some("100pt"));
    assert_eq!(root.attribute("height"), Some("50pt"));
    assert_eq!(root.attribute("viewBox"), Some("0 0 100 50"));
    // The background is the first painted element.
    let bg = elements(&doc, "rect")[0];
    assert_eq!(bg.attribute("fill"), Some("#00ff00"));
    assert_eq!(bg.attribute("width"), Some("100"));
}

#[test]
fn rect_has_exact_geometry_and_paint() {
    let svg = svg_of(
        r##"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "stack"
        children = ["r"]
        [nodes.r]
        type = "rect"
        x = 10
        y = 20
        width = 30
        height = 40
        fill = "#ff000080"
        stroke = "#0000ff"
        stroke_width = 2
        corner_radius = 50
        "##,
    );
    let doc = roxmltree::Document::parse(&svg).unwrap();
    let r = elements(&doc, "rect")
        .into_iter()
        .find(|n| n.attribute("fill") == Some("#ff0000"))
        .expect("the red rect");
    assert_eq!(r.attribute("x"), Some("10"));
    assert_eq!(r.attribute("y"), Some("20"));
    assert_eq!(r.attribute("width"), Some("30"));
    assert_eq!(r.attribute("height"), Some("40"));
    assert_eq!(r.attribute("fill-opacity"), Some("0.502"));
    assert_eq!(r.attribute("stroke"), Some("#0000ff"));
    assert_eq!(r.attribute("stroke-width"), Some("2"));
    // Radius clamped to half the shorter side, circular corners.
    assert_eq!(r.attribute("rx"), Some("15"));
    assert_eq!(r.attribute("ry"), Some("15"));
}

#[test]
fn dashed_line_and_arrow() {
    let svg = svg_of(
        r##"
        [page]
        width = 200
        height = 100
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "stack"
        children = ["l", "a"]
        [nodes.l]
        type = "line"
        points = [[10, 60], [60, 60]]
        dash = [4, 2]
        cap = "round"
        [nodes.a]
        type = "arrow"
        points = [[0, 10], [100, 10]]
        stroke = "#e0635a"
        stroke_width = 2
        "##,
    );
    let doc = roxmltree::Document::parse(&svg).unwrap();
    let paths = elements(&doc, "path");

    let line = paths
        .iter()
        .find(|p| p.attribute("stroke-dasharray").is_some())
        .expect("dashed line");
    assert_eq!(line.attribute("d"), Some("M10 60 L60 60"));
    assert_eq!(line.attribute("stroke-dasharray"), Some("4 2"));
    assert_eq!(line.attribute("stroke-linecap"), Some("round"));
    assert_eq!(line.attribute("fill"), Some("none"));
    assert_eq!(line.attribute("stroke"), Some("#000000"));

    // head_size defaults to 5x the stroke width (10 pt): the shaft stops half
    // a head short of the tip and the head is a filled triangle 8 pt wide.
    let shaft = paths
        .iter()
        .find(|p| p.attribute("stroke") == Some("#e0635a"))
        .expect("arrow shaft");
    assert_eq!(shaft.attribute("d"), Some("M0 10 L95 10"));
    let head = paths
        .iter()
        .find(|p| p.attribute("fill") == Some("#e0635a"))
        .expect("arrowhead");
    assert_eq!(head.attribute("d"), Some("M100 10 L90 14 L90 6 Z"));
    assert_eq!(head.attribute("stroke"), None);
}

#[test]
fn ellipse_is_native() {
    let svg = svg_of(
        r##"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "stack"
        children = ["e"]
        [nodes.e]
        type = "ellipse"
        x = 10
        y = 10
        width = 40
        height = 20
        fill = "#ffffff"
        "##,
    );
    let doc = roxmltree::Document::parse(&svg).unwrap();
    let e = elements(&doc, "ellipse")[0];
    assert_eq!(e.attribute("cx"), Some("30"));
    assert_eq!(e.attribute("cy"), Some("20"));
    assert_eq!(e.attribute("rx"), Some("20"));
    assert_eq!(e.attribute("ry"), Some("10"));
}

#[test]
fn text_is_glyph_outlines() {
    let doc = Document::from_toml(
        r##"
        [page]
        width = 200
        height = 80
        unit = "pt"
        root = "t"
        [nodes.t]
        type = "text"
        content = "Hi & <b>"
        font_size = 40
        color = "#123456"
        "##,
    )
    .unwrap();
    let fonts = FontStore::new();
    let svg = svg_with(&doc, &fonts, Path::new("."));
    let xml = roxmltree::Document::parse(&svg).expect("well-formed with escaped text");
    let group = elements(&xml, "g")
        .into_iter()
        .find(|g| g.attribute("aria-label").is_some())
        .expect("text group");
    assert_eq!(group.attribute("aria-label"), Some("Hi & <b>"));
    assert_eq!(group.attribute("fill"), Some("#123456"));
    let uses = group.children().filter(|n| n.is_element()).count();
    assert!(uses >= 5, "expected one <use> per inked glyph, got {uses}");
    // Every <use> refers to a defined glyph outline.
    let defs: Vec<_> = elements(&xml, "path")
        .into_iter()
        .filter_map(|p| p.attribute("id"))
        .collect();
    for u in elements(&xml, "use") {
        let href = u
            .attribute(("http://www.w3.org/1999/xlink", "href"))
            .expect("xlink:href");
        assert!(defs.contains(&href.trim_start_matches('#')), "undefined glyph {href}");
    }
}

#[test]
fn images_embed_and_cover_is_clipped() {
    let dir = scratch_dir("img");
    image::RgbImage::from_pixel(8, 4, image::Rgb([200, 20, 20]))
        .save(dir.join("a.png"))
        .unwrap();
    image::RgbImage::from_pixel(4, 8, image::Rgb([20, 200, 20]))
        .save(dir.join("b.jpg"))
        .unwrap();
    let doc = Document::from_toml(
        r##"
        [page]
        width = 300
        height = 100
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "stack"
        children = ["contain", "cover", "missing"]
        [nodes.contain]
        type = "image"
        src = "a.png"
        fit = "contain"
        x = 0
        y = 0
        width = 80
        height = 80
        [nodes.cover]
        type = "image"
        src = "b.jpg"
        fit = "cover"
        x = 100
        y = 0
        width = 80
        height = 40
        [nodes.missing]
        type = "image"
        src = "nope.png"
        x = 200
        y = 0
        width = 50
        height = 50
        "##,
    )
    .unwrap();
    let fonts = FontStore::new();
    let svg = svg_with(&doc, &fonts, &dir);
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let images = elements(&xml, "image");
    assert_eq!(images.len(), 2);
    let href = |n: &roxmltree::Node| {
        n.attribute(("http://www.w3.org/1999/xlink", "href"))
            .unwrap()
            .to_string()
    };

    // contain: 8x4 into 80x80 -> 80x40 centred vertically.
    let a = images[0];
    assert!(href(&a).starts_with("data:image/png;base64,iVBOR"));
    assert_eq!(
        [a.attribute("x"), a.attribute("y"), a.attribute("width"), a.attribute("height")],
        [Some("0"), Some("20"), Some("80"), Some("40")]
    );
    assert_eq!(a.attribute("clip-path"), None);

    // cover: 4x8 into 80x40 -> 80x160 centred, clipped to the node rect.
    let b = images[1];
    assert!(href(&b).starts_with("data:image/jpeg;base64,/9j/"));
    assert_eq!(
        [b.attribute("x"), b.attribute("y"), b.attribute("width"), b.attribute("height")],
        [Some("100"), Some("-60"), Some("80"), Some("160")]
    );
    let clip_ref = b.attribute("clip-path").expect("cover is clipped");
    let clip_id = clip_ref.trim_start_matches("url(#").trim_end_matches(')');
    let clip = elements(&xml, "clipPath")
        .into_iter()
        .find(|c| c.attribute("id") == Some(clip_id))
        .expect("clipPath defined");
    let clip_rect = clip.first_element_child().unwrap();
    assert_eq!(clip_rect.attribute("x"), Some("100"));
    assert_eq!(clip_rect.attribute("height"), Some("40"));

    // The missing image becomes the placeholder box.
    let placeholder = elements(&xml, "rect")
        .into_iter()
        .find(|r| r.attribute("fill") == Some("#e7e4dd"))
        .expect("placeholder");
    assert_eq!(placeholder.attribute("x"), Some("200"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn all_examples_render_well_formed() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let fonts = FontStore::new();
    let mut count = 0;
    for entry in std::fs::read_dir(&examples).unwrap() {
        let path = entry.unwrap().path();
        let src = std::fs::read_to_string(&path).unwrap();
        let doc = match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => Document::from_toml(&src),
            Some("figs") => Document::from_script(&src),
            _ => continue,
        }
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let svg = svg_with(&doc, &fonts, &examples);
        roxmltree::Document::parse(&svg)
            .unwrap_or_else(|e| panic!("{}: malformed SVG: {e}", path.display()));
        count += 1;
    }
    assert!(count >= 5, "expected the bundled examples, found {count}");
}
