//! PDF backend tests: structural checks on the written file — header/trailer,
//! a consistent cross-reference table, the exact page MediaBox, and the number
//! of embedded fonts and images.
#![cfg(feature = "pdf")]

use std::path::{Path, PathBuf};

use figs_core::{layout, render_pdf, Document, FontStore, NullMeasurer, RenderOptions};

fn pdf_with(doc: &Document, fonts: Option<&FontStore>, base_dir: &Path) -> Vec<u8> {
    let computed = match fonts {
        Some(f) => layout(doc, f),
        None => layout(doc, &NullMeasurer),
    };
    let opts = RenderOptions { fonts, base_dir };
    let bytes = render_pdf(&computed, &opts).expect("render pdf");
    check_structure(&bytes);
    bytes
}

fn pdf_of(src: &str) -> Vec<u8> {
    pdf_with(&Document::from_toml(src).expect("resolve"), None, Path::new("."))
}

fn count(hay: &[u8], needle: &str) -> usize {
    hay.windows(needle.len())
        .filter(|w| *w == needle.as_bytes())
        .count()
}

fn find(hay: &[u8], needle: &str) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle.as_bytes())
}

/// Header, `%%EOF`, and a classic xref table whose every in-use entry points at
/// the matching `N 0 obj`.
#[track_caller]
fn check_structure(pdf: &[u8]) {
    assert!(pdf.starts_with(b"%PDF-1."), "missing %PDF header");
    // Work on byte offsets: the trailer and xref are ASCII, streams aren't.
    let tail = String::from_utf8_lossy(&pdf[pdf.len().saturating_sub(64)..]).into_owned();
    assert!(tail.trim_end().ends_with("%%EOF"), "missing %%EOF");
    let sx = tail.rfind("startxref").expect("startxref");
    let xref_at: usize = tail[sx + "startxref".len()..]
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .expect("startxref offset");
    assert!(pdf[xref_at..].starts_with(b"xref"), "startxref doesn't point at xref");
    let xref = String::from_utf8_lossy(&pdf[xref_at..]).into_owned();
    let mut lines = xref.lines().skip(1);
    let header = lines.next().unwrap();
    let mut it = header.split_whitespace();
    let first: usize = it.next().unwrap().parse().unwrap();
    let n: usize = it.next().unwrap().parse().unwrap();
    assert!(n > 1, "empty xref");
    for (i, line) in lines.take(n).enumerate() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.get(2) != Some(&"n") {
            continue;
        }
        let off: usize = f[0].parse().unwrap();
        let obj = format!("{} 0 obj", first + i);
        assert!(
            pdf[off..].starts_with(obj.as_bytes()),
            "xref entry {} doesn't point at `{obj}`",
            first + i
        );
    }
}

/// The page MediaBox as numbers.
fn media_box(pdf: &[u8]) -> [f32; 4] {
    let at = find(pdf, "/MediaBox[").expect("MediaBox") + "/MediaBox[".len();
    let end = at + pdf[at..].iter().position(|b| *b == b']').unwrap();
    let nums: Vec<f32> = String::from_utf8_lossy(&pdf[at..end])
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    nums.try_into().expect("4 numbers")
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("figs-pdf-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn media_box_is_the_page_size_in_points() {
    let pdf = pdf_of(
        r##"
        [page]
        width = 100
        height = 50
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "rect"
        fill = "#000000"
        "##,
    );
    assert_eq!(media_box(&pdf), [0.0, 0.0, 100.0, 50.0]);
    assert_eq!(count(&pdf, "/Type/Page/"), 1, "single page");
    // No text, no images -> nothing embedded.
    assert_eq!(count(&pdf, "/FontFile"), 0);
    assert_eq!(count(&pdf, "/Subtype/Image"), 0);
}

#[test]
fn fractional_page_sizes_are_exact() {
    let src = r##"
        [page]
        width = 17.5
        height = 10
        unit = "cm"
        root = "root"
        [nodes.root]
        type = "column"
    "##;
    let doc = Document::from_toml(src).unwrap();
    let page = layout(&doc, &NullMeasurer).page;
    let mb = media_box(&pdf_of(src));
    assert!((mb[2] - page.width_pt).abs() < 1e-3, "{mb:?} vs {}", page.width_pt);
    assert!((mb[3] - page.height_pt).abs() < 1e-3, "{mb:?} vs {}", page.height_pt);
}

#[test]
fn vector_shapes_render() {
    // Dashes, caps, arrowheads, ellipses and translucent fills all serialize.
    let pdf = pdf_of(
        r##"
        [page]
        width = 200
        height = 100
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "stack"
        children = ["r", "l", "a", "e"]
        [nodes.r]
        type = "rect"
        fill = "#ff000080"
        stroke = "#000000"
        corner_radius = 8
        x = 5
        y = 5
        width = 50
        height = 30
        [nodes.l]
        type = "line"
        points = [[10, 60], [60, 60]]
        dash = [4, 2]
        cap = "round"
        [nodes.a]
        type = "arrow"
        points = [[0, 80], [100, 80]]
        head = "both"
        [nodes.e]
        type = "ellipse"
        x = 120
        y = 10
        width = 60
        height = 40
        stroke = "#0000ff"
        "##,
    );
    assert_eq!(media_box(&pdf), [0.0, 0.0, 200.0, 100.0]);
    // The translucent fill needs an ExtGState with a fill alpha.
    assert!(count(&pdf, "/ExtGState") >= 1, "expected an alpha graphics state");
}

#[test]
fn text_embeds_one_subset_font() {
    let doc = Document::from_toml(
        r##"
        [page]
        width = 200
        height = 100
        unit = "pt"
        root = "root"
        [nodes.root]
        type = "column"
        children = ["a", "b"]
        [nodes.a]
        type = "text"
        content = "Hello"
        font_size = 20
        [nodes.b]
        type = "text"
        content = "World, again"
        font_size = 12
        "##,
    )
    .unwrap();
    let fonts = FontStore::new();
    let pdf = pdf_with(&doc, Some(&fonts), Path::new("."));
    // One family, one weight -> one embedded Type0/CID font, subset (tagged
    // `ABCDEF+Name`) with a ToUnicode map for copy/paste.
    assert_eq!(count(&pdf, "/Subtype/Type0"), 1);
    assert_eq!(
        count(&pdf, "/FontFile2") + count(&pdf, "/FontFile3"),
        1,
        "exactly one embedded font program"
    );
    assert_eq!(count(&pdf, "/ToUnicode"), 1);
    let at = find(&pdf, "/Subtype/Type0/BaseFont/").unwrap() + "/Subtype/Type0/BaseFont/".len();
    assert_eq!(pdf[at + 6], b'+', "font name should carry a subset tag");
}

#[test]
fn images_are_embedded_once_and_missing_ones_become_placeholders() {
    let dir = scratch_dir("img");
    image::RgbImage::from_pixel(7, 5, image::Rgb([200, 20, 20]))
        .save(dir.join("a.png"))
        .unwrap();
    image::RgbImage::from_pixel(9, 6, image::Rgb([20, 200, 20]))
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
        type = "row"
        children = ["a1", "a2", "b", "missing"]
        [nodes.a1]
        type = "image"
        src = "a.png"
        flex = 1
        [nodes.a2]
        type = "image"
        src = "a.png"
        fit = "cover"
        flex = 1
        [nodes.b]
        type = "image"
        src = "b.jpg"
        fit = "fill"
        flex = 1
        [nodes.missing]
        type = "image"
        src = "does-not-exist.png"
        flex = 1
        "##,
    )
    .unwrap();
    let fonts = FontStore::new();
    let pdf = pdf_with(&doc, Some(&fonts), &dir);
    // The PNG drawn twice is one XObject; the JPEG keeps its DCT stream.
    assert_eq!(count(&pdf, "/Width 7/Height 5"), 1, "PNG embedded once");
    assert_eq!(count(&pdf, "/DCTDecode/Width 9/Height 6"), 1, "JPEG passed through");
    assert_eq!(count(&pdf, "/Subtype/Image"), 2, "two images, no extra for the missing one");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn all_examples_render() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let fonts = FontStore::new();
    let mut n = 0;
    for entry in std::fs::read_dir(&examples).unwrap() {
        let path = entry.unwrap().path();
        let src = std::fs::read_to_string(&path).unwrap();
        let doc = match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => Document::from_toml(&src),
            Some("figs") => Document::from_script(&src),
            _ => continue,
        }
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let page = layout(&doc, &fonts).page;
        let pdf = pdf_with(&doc, Some(&fonts), &examples);
        let mb = media_box(&pdf);
        assert!(
            (mb[2] - page.width_pt).abs() < 1e-3 && (mb[3] - page.height_pt).abs() < 1e-3,
            "{}: MediaBox {mb:?} != page {} x {}",
            path.display(),
            page.width_pt,
            page.height_pt
        );
        n += 1;
    }
    assert!(n >= 5, "expected the bundled examples, found {n}");
}
