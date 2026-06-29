//! Text shaping & measurement tests. Uses a fixed default family so results are
//! stable on this machine; assertions are tolerant of exact font metrics.

use figs_core::schema::TextProps;
use figs_core::{layout, Document, FontStore};

fn store() -> FontStore {
    // Liberation Sans ships on the CI image; pin it for determinism.
    FontStore::new().with_default_family("Liberation Sans")
}

fn text(content: &str, font_size: f32) -> TextProps {
    TextProps {
        content: content.to_string(),
        font_size,
        font_family: None,
        font_weight: None,
        color: figs_core::geom::Color::BLACK,
        align: figs_core::geom::TextAlign::Left,
        line_height: 1.2,
    }
}

#[test]
fn single_line_has_width_and_one_line_height() {
    let fs = store();
    let shaped = fs.shape(&text("Hello", 20.0), f32::INFINITY);
    assert_eq!(shaped.lines.len(), 1, "single line expected");
    assert!(shaped.size.w > 0.0, "width should be positive");
    // line height = font_size * 1.2 = 24
    assert!(
        (shaped.size.h - 24.0).abs() < 0.5,
        "height {} != ~24",
        shaped.size.h
    );
    // glyphs were produced (one per visible char, roughly)
    let glyphs: usize = shaped.lines.iter().map(|l| l.glyphs.len()).sum();
    assert!(glyphs >= 4, "expected glyphs for 'Hello', got {glyphs}");
}

#[test]
fn wrapping_increases_line_count() {
    let fs = store();
    let long = "the quick brown fox jumps over the lazy dog";
    let wide = fs.shape(&text(long, 20.0), f32::INFINITY);
    assert_eq!(wide.lines.len(), 1);
    // constrain width hard so it must wrap to several lines
    let narrow = fs.shape(&text(long, 20.0), 80.0);
    assert!(
        narrow.lines.len() >= 3,
        "expected wrapping, got {} lines",
        narrow.lines.len()
    );
    // height scales with line count
    assert!(
        (narrow.size.h - narrow.lines.len() as f32 * 24.0).abs() < 0.5,
        "size.h={} lines={}",
        narrow.size.h,
        narrow.lines.len()
    );
    // wrapped block is no wider than the constraint
    assert!(narrow.size.w <= 80.0 + 0.5);
}

#[test]
fn cjk_shapes_non_zero() {
    let fs = store();
    // IPAGothic provides CJK coverage on the image.
    let shaped = fs.shape(&text("日本語テキスト", 24.0), f32::INFINITY);
    assert_eq!(shaped.lines.len(), 1);
    assert!(shaped.size.w > 0.0, "CJK should have positive advance");
    let glyphs: usize = shaped.lines.iter().map(|l| l.glyphs.len()).sum();
    assert!(glyphs >= 6, "expected a glyph per CJK char, got {glyphs}");
}

#[test]
fn layout_sizes_text_node_from_shaping() {
    // A text node with no explicit size gets its height from shaping when laid
    // out with a FontStore measurer.
    let src = r#"
        [page]
        width = 400
        height = 200
        unit = "pt"
        root = "root"

        [nodes.root]
        type = "column"
        children = ["t"]

        [nodes.t]
        type = "text"
        content = "Hello"
        font_size = 20
    "#;
    let doc = Document::from_toml(src).unwrap();
    let computed = layout(&doc, &store());
    let t = computed.nodes.iter().find(|n| n.id == "t").unwrap();
    // height ~= one line of 20pt * 1.2 = 24pt
    assert!(
        (t.rect.h - 24.0).abs() < 1.0,
        "text node height {} != ~24",
        t.rect.h
    );
    assert!(t.rect.w > 0.0);
}
