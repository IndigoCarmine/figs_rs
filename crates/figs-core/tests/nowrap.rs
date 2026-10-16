//! `wrap = false` (`.nowrap()`): unwrapped text measures and paints as one
//! line per paragraph, overflowing its box (centred text on both sides)
//! instead of breaking — the fix for a quarter-turned axis label longer than
//! its axis, which used to wrap mid-word and inflate its footprint.

use figs_core::geom::TextAlign;
use figs_core::layout::{ComputedLayout, ComputedNode, PaintContent};
use figs_core::render::geometry::{align_offset, text_props};
use figs_core::{eval_script, layout, Document, FontStore};

fn store() -> FontStore {
    FontStore::new().with_default_family("Liberation Sans, Arial, DejaVu Sans")
}

fn node<'a>(l: &'a ComputedLayout, id: &str) -> &'a ComputedNode {
    l.nodes.iter().find(|n| n.id == id).expect("node")
}

fn lines(fs: &FontStore, n: &ComputedNode) -> usize {
    let PaintContent::Text(t) = &n.content else {
        panic!("not text")
    };
    fs.shape(&text_props(t), n.rect.w).lines.len()
}

/// A 40 pt wide column holding a 12 pt text that needs ~3 lines to wrap.
fn narrow(wrap: &str) -> String {
    format!(
        r##"
        [page]
        width = 40
        height = 200
        unit = "pt"
        root = "c"
        [nodes.c]
        type = "column"
        cross_axis_alignment = "stretch"
        children = ["t"]
        [nodes.t]
        type = "text"
        content = "a fairly long label"
        font_size = 12
        {wrap}
        "##
    )
}

#[test]
fn unwrapped_text_is_one_line_and_overflows() {
    let fs = store();
    let wrapped = layout(&Document::from_toml(&narrow("")).unwrap(), &fs);
    let one = layout(&Document::from_toml(&narrow("wrap = false")).unwrap(), &fs);
    let (w, o) = (node(&wrapped, "t"), node(&one, "t"));
    assert!(lines(&fs, w) >= 3, "wraps by default");
    assert_eq!(lines(&fs, o), 1, "wrap = false: one line");
    // Same box width (the column's), but one line tall.
    assert_eq!(o.rect.w, 40.0);
    assert!(o.rect.h < 12.0 * 1.6, "{:?}", o.rect);
    assert!(w.rect.h > 2.0 * o.rect.h);
    // `\n` still breaks.
    let two = narrow("wrap = false").replace("a fairly long label", "first part\\nsecond");
    let two = layout(&Document::from_toml(&two).unwrap(), &fs);
    assert_eq!(lines(&fs, node(&two, "t")), 2);
}

#[test]
fn rotated_label_longer_than_its_axis_stays_one_line() {
    // A y-axis label beside a 60 pt tall plot area: its text is longer than
    // the axis. Wrapped, it breaks inside the word and its footprint is
    // several lines wide; unwrapped it is one line wide and centred on the
    // axis, overflowing both ends equally.
    let fs = store();
    let src = |nowrap: &str| {
        format!(
            r##"page(300, 60, row([
                text("height [nm]").size(14).center().rotate(-90){nowrap},
                rect().fill("#dddddd").flex(1),
            ]).cross("stretch")).unit("pt")"##
        )
    };
    let footprint = |s: &str| {
        let doc = Document::from_script(&src(s)).unwrap();
        let l = layout(&doc, &fs);
        let t = l
            .nodes
            .iter()
            .find(|n| matches!(n.content, PaintContent::Text(_)))
            .unwrap()
            .clone();
        let PaintContent::Text(tb) = &t.content else {
            unreachable!()
        };
        let shaped = fs.shape(&text_props(tb), t.rect.w);
        (t.bounds(), shaped, t.rect)
    };
    let (wrapped, wrapped_text, _) = footprint("");
    let (one, one_text, own) = footprint(".nowrap()");
    assert!(wrapped_text.lines.len() >= 2, "the bug: it wraps");
    assert_eq!(one_text.lines.len(), 1);
    assert!(one.w < 14.0 * 1.6, "one line wide: {one:?}");
    assert!(wrapped.w > 1.5 * one.w, "{wrapped:?} vs {one:?}");
    assert_eq!(one.h, 60.0, "fills the axis");
    // Centred: the overflowing line sticks out equally at both ends.
    let line_w = one_text.lines[0].width;
    assert!(line_w > own.w, "the label is longer than the axis");
    let off = align_offset(TextAlign::Center, line_w, own.w);
    assert!((off - (own.w - line_w) / 2.0).abs() < 1e-4 && off < 0.0);
}

#[test]
fn wrap_is_text_only_and_scriptable() {
    let e = Document::from_toml(
        "[page]\nwidth = 1\nheight = 1\nroot = \"r\"\n[nodes.r]\ntype = \"rect\"\nwrap = false",
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("wrap"), "{e}");
    let raw =
        eval_script(r#"page(10, 10, col([text("a").nowrap(), text("b").wrap(true)]))"#).unwrap();
    let toml = toml::to_string(&raw).unwrap();
    assert!(toml.contains("wrap = false"), "{toml}");
    assert!(toml.contains("wrap = true"), "{toml}");
    for bad in [
        r#"page(10, 10, rect().nowrap())"#,
        r#"page(10, 10, col([]).wrap(false))"#,
    ] {
        assert!(eval_script(bad).is_err(), "{bad}");
    }
}

#[test]
fn alignment_offsets_for_overflowing_lines() {
    assert_eq!(align_offset(TextAlign::Left, 50.0, 40.0), 0.0);
    assert_eq!(align_offset(TextAlign::Center, 50.0, 40.0), -5.0);
    assert_eq!(align_offset(TextAlign::Right, 50.0, 40.0), -10.0);
    assert_eq!(align_offset(TextAlign::Center, 30.0, 40.0), 5.0);
}
