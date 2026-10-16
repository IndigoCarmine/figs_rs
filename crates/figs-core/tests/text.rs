//! Text shaping & measurement tests. Uses a fixed default family so results are
//! stable on this machine; assertions are tolerant of exact font metrics.

use figs_core::schema::TextProps;
use figs_core::{layout, Document, FontStore};

fn store() -> FontStore {
    // Liberation Sans ships on the CI image; pin it for determinism.
    FontStore::new().with_default_family("Liberation Sans")
}

#[test]
fn families_are_listed_and_sorted() {
    let families = FontStore::new().families();
    assert!(!families.is_empty(), "expected installed font families");
    // Sorted case-insensitively and deduplicated.
    let mut sorted = families.clone();
    sorted.sort_by_key(|s| s.to_lowercase());
    assert_eq!(families, sorted, "families must be sorted");
    let mut deduped = families.clone();
    deduped.dedup();
    assert_eq!(families.len(), deduped.len(), "families must be deduplicated");
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

/// A block is `lines` line boxes of `lh`, grown by at most the font's
/// ascent+descent overhang (well under a quarter em for real fonts).
fn assert_block_height(h: f32, lines: usize, lh: f32, font_size: f32) {
    let stacked = lines as f32 * lh;
    assert!(
        h >= stacked - 0.5 && h <= stacked + 0.25 * font_size,
        "height {h} for {lines} line(s) of {lh}"
    );
}

#[test]
fn single_line_has_width_and_one_line_height() {
    let fs = store();
    let shaped = fs.shape(&text("Hello", 20.0), f32::INFINITY);
    assert_eq!(shaped.lines.len(), 1, "single line expected");
    assert!(shaped.size.w > 0.0, "width should be positive");
    // line height = font_size * 1.2 = 24 (grown if the font's ascent+descent
    // is taller — e.g. a Segoe UI fallback when Liberation Sans is missing)
    assert_block_height(shaped.size.h, 1, 24.0, 20.0);
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
    assert_block_height(narrow.size.h, narrow.lines.len(), 24.0, 20.0);
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

fn with_line_height(content: &str, font_size: f32, line_height: f32) -> TextProps {
    TextProps {
        line_height,
        ..text(content, font_size)
    }
}

#[test]
fn tight_line_height_box_contains_ascent_and_descent() {
    // At line_height 0.8 every real font's ascent + descent overhangs the line
    // box; the block grows so the first line's ascent starts at y = 0 and the
    // last line's descent ends at the bottom (nothing clips at a page edge).
    let fs = store();
    let tight = fs.shape(&with_line_height("Hgp", 20.0, 0.8), f32::INFINITY);
    assert_eq!(tight.lines.len(), 1);
    assert!(tight.size.h > 16.0, "block {} not grown past its line box", tight.size.h);
    let ascent = tight.lines[0].baseline;
    let descent = tight.size.h - ascent;
    assert!(ascent > 0.0 && descent > 0.0, "ascent {ascent}, descent {descent}");

    // With a generous line height the line box already contains the glyphs:
    // the height is exactly the line box and the ascent+descent box is centred
    // in it (half-leading) — consistent with the metrics recovered above.
    let tall = fs.shape(&with_line_height("Hgp", 20.0, 3.0), f32::INFINITY);
    assert!((tall.size.h - 60.0).abs() < 0.01, "{}", tall.size.h);
    let expected_baseline = (60.0 - (ascent + descent)) / 2.0 + ascent;
    assert!(
        (tall.lines[0].baseline - expected_baseline).abs() < 0.01,
        "baseline {} != {expected_baseline}",
        tall.lines[0].baseline
    );
}

#[test]
fn tight_line_height_keeps_inner_line_spacing() {
    // Only the block's outer edges grow; baselines stay one line height apart.
    let fs = store();
    let single = fs.shape(&with_line_height("Hg", 20.0, 0.8), f32::INFINITY);
    let multi = fs.shape(&with_line_height("Hg\nHg\nHg", 20.0, 0.8), f32::INFINITY);
    assert_eq!(multi.lines.len(), 3);
    for pair in multi.lines.windows(2) {
        assert!((pair[1].baseline - pair[0].baseline - 16.0).abs() < 0.01);
    }
    assert!((multi.lines[0].baseline - single.lines[0].baseline).abs() < 0.01);
    assert!((multi.size.h - (single.size.h + 32.0)).abs() < 0.01, "{}", multi.size.h);
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
    assert_block_height(t.rect.h, 1, 24.0, 20.0);
    assert!(t.rect.w > 0.0);
}
