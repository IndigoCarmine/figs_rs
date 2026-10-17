//! Framework (prelude) pieces for coordinate-free posters and figures:
//! theme-following rich text, step captions of any kind, sections that name
//! themselves in overflow reports and protect their footer, and the fitting
//! / overlay helpers built on the core `axis`, `equal_height` and `overflow`.

#![cfg(feature = "script")]

use figs_core::geom::{Color, Overflow, Rect, Size};
use figs_core::layout::{ComputedLayout, OverflowAction, PaintContent};
use figs_core::schema::{ImageProps, NodeKind, RawDocument, RawNode, TextProps};
use figs_core::units::Unit;
use figs_core::{eval_script, layout, Document, LeafMeasure};

/// Every text block measures `TEXT_W` x `TEXT_H` pt.
struct FixedText;
const TEXT_W: f32 = 20.0;
const TEXT_H: f32 = 10.0;
impl LeafMeasure for FixedText {
    fn measure_text(&self, _t: &TextProps, _max_width: f32) -> Size {
        Size::new(TEXT_W, TEXT_H)
    }
    fn measure_image(&self, _i: &ImageProps) -> Size {
        Size::default()
    }
}

fn raw(src: &str) -> RawDocument {
    eval_script(src).unwrap_or_else(|e| panic!("script failed: {e}"))
}

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &FixedText)
}

fn fail(src: &str) -> String {
    eval_script(src).expect_err("script should fail").message
}

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-2, "expected {b}, got {a}");
}

/// (The last-painted text with this content: the foreground.)
fn text_rect(l: &ComputedLayout, content: &str) -> Rect {
    l.nodes
        .iter()
        .rev()
        .find(|n| matches!(&n.content, PaintContent::Text(t) if t.content == content))
        .unwrap_or_else(|| panic!("no text {content:?} in layout"))
        .rect
}

/// The raw text node whose plain text (content or spans) is `s`.
fn text_node<'a>(d: &'a RawDocument, s: &str) -> &'a RawNode {
    d.nodes
        .values()
        .find(|n| {
            n.kind == NodeKind::Text
                && (n.content.as_deref() == Some(s)
                    || n.spans.iter().map(|x| x.text.as_str()).collect::<String>() == s)
        })
        .unwrap_or_else(|| panic!("no text {s:?}"))
}

fn named<'a>(d: &'a RawDocument, name: &str) -> &'a RawNode {
    d.nodes
        .values()
        .find(|n| n.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named {name:?}"))
}

// ---- bugs ------------------------------------------------------------------------

#[test]
fn rich_follows_the_theme() {
    let d = raw(r#"style("dark"); page(10, 10, rich("H_2O *x*"))"#);
    let t = text_node(&d, "H2O x");
    assert_eq!(t.color, Color::parse_hex("#ffffff").ok());
    assert_eq!(t.font_size, Some(8.0));
    let d = raw(r#"style(#{ base_size: 11 }); page(10, 10, rich("a"))"#);
    assert_eq!(text_node(&d, "a").font_size, Some(11.0));
    assert_eq!(
        text_node(&d, "a").color,
        None,
        "no colour without a theme fg"
    );
}

#[test]
fn step_takes_a_node_or_markup_as_its_caption() {
    for cap in [r#"text("cap")"#, r#"markup("cap")"#, r#""cap""#] {
        let l = lay(&format!(
            r#"fn scheme_label_gap() {{ 2 }}
            page(200, 100, col([step(rect().width(10).height(20), {cap})])).unit("pt")"#
        ));
        let icon = l
            .nodes
            .iter()
            .find(|n| n.rect.w == 10.0 && n.rect.h == 20.0)
            .unwrap();
        approx(text_rect(&l, "cap").y, icon.rect.y + 22.0);
    }
    let e = fail(r#"page(1, 1, step(rect(), 3))"#);
    assert!(
        e.contains("step: expected a string, spans or a node"),
        "{e}"
    );
}

/// A 100 x 100 pt page holding one section with `content` and `opts`.
fn section_page(content: &str, opts: &str) -> String {
    format!(
        r#"style(#{{ section_bar_height: 10, section_pad: 0, section_gap: 2 }});
        page(100, 100, section("Results", {content}, {opts})).unit("pt")"#
    )
}

#[test]
fn sections_name_themselves_for_diagnostics() {
    let d = raw(&section_page("[]", "#{}"));
    assert_eq!(named(&d, r#"section "Results""#).kind, NodeKind::Column);
    assert_eq!(
        named(&d, r#"section "Results" (body)"#).overflow,
        Some(Overflow::Visible)
    );
    let d = raw(&section_page("[]", r#"#{ footer: "f" }"#));
    assert_eq!(
        named(&d, r#"section "Results" (content)"#).overflow,
        Some(Overflow::Clip)
    );
    named(&d, r#"section "Results" (footer)"#);
    // A numbered title that isn't a string is named by its number.
    let d = raw(r#"page(10, 10, section(markup("*A*"), []))"#);
    named(&d, "section 1.");
}

#[test]
fn an_overfull_section_never_covers_its_footer() {
    let l = lay(&section_page(
        "[rect().height(50), rect().height(50)]",
        r#"#{ footer: "f" }"#,
    ));
    // Bar 10, footer 10 + gap 2: the content has 78 pt and needs 102.
    let foot = text_rect(&l, "f");
    approx(foot.y, 90.0);
    let o = &l.overflows[0];
    assert_eq!(o.name.as_deref(), Some(r#"section "Results" (content)"#));
    assert_eq!(o.action, OverflowAction::Clipped);
    approx(o.room_pt, 78.0);
    approx(o.need_pt, 102.0);
    let m = o.message(Unit::Pt);
    assert!(
        m.contains(r#"section "Results" (content)"#) && m.contains("24.00 pt too tall"),
        "{m}"
    );
    // Everything in the content is clipped at the footer's gap.
    let second = l
        .nodes
        .iter()
        .find(|n| (n.rect.y - 62.0).abs() < 1e-2 && (n.rect.h - 50.0).abs() < 1e-2)
        .expect("second rect");
    approx(second.clips[0].rect.y + second.clips[0].rect.h, 88.0);
}

#[test]
fn section_overflow_option_and_hook() {
    let l = lay(&section_page(
        "[rect().height(50), rect().height(50)]",
        r#"#{ footer: "f", overflow: "shrink" }"#,
    ));
    assert!(matches!(l.overflows[0].action, OverflowAction::Shrunk(_)));
    // Without a footer the body is the container; the hook sets the mode.
    let l = lay(
        &section_page("[rect().height(50), rect().height(50)]", "#{}").replace(
            "section_gap: 2 }",
            r#"section_gap: 2, section_overflow: "clip" }"#,
        ),
    );
    assert_eq!(
        l.overflows[0].name.as_deref(),
        Some(r#"section "Results" (body)"#)
    );
    assert_eq!(l.overflows[0].action, OverflowAction::Clipped);
    let e = fail(&section_page("[]", r#"#{ overflow: "hide" }"#));
    assert!(
        e.contains(r#"section: unknown overflow "hide" (visible, clip, shrink)"#),
        "{e}"
    );
}
