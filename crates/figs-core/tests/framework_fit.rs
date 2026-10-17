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
        e.contains(r#"section: unknown overflow "hide" (visible, clip, shrink, fit)"#),
        "{e}"
    );
}

// ---- fill_row -------------------------------------------------------------------------

#[test]
fn fill_row_shares_one_height_across_the_width() {
    // Aspects 2 and 1 in 200 pt, fill_row_gap() 8 apart: H = 192 / 3 = 64.
    let l = lay(r#"style(#{ fill_row_gap: 8 });
        page(200, 300, col([fill_row([rect().aspect(2), labeled(rect().aspect(1), "(b)")])]).stretch())
            .unit("pt")"#);
    let a = l
        .nodes
        .iter()
        .find(|n| (n.rect.w - 128.0).abs() < 1e-2)
        .unwrap()
        .rect;
    approx(a.h, 64.0);
    approx(a.x, 0.0);
    let b = l
        .nodes
        .iter()
        .find(|n| (n.rect.w - 64.0).abs() < 1e-2)
        .unwrap()
        .rect;
    approx(b.h, 64.0);
    approx(b.x, 136.0);
    let d = raw(r#"page(10, 10, fill_row([rect()], #{ gap: 0.3, align: "center" }))"#);
    let r = &d.nodes["root"];
    assert!(r.equal_height);
    approx(r.spacing, 0.3);
    let e = fail(r#"page(1, 1, fill_row([rect()], #{ align: "baseline" }))"#);
    assert!(
        e.contains(r#"fill_row: unknown align "baseline" (start, center, end)"#),
        "{e}"
    );
    let e = fail(r#"page(1, 1, fill_row([rect()], #{ size: 1 }))"#);
    assert!(e.contains(r#"unknown option "size""#), "{e}");
}

// ---- overlay text, insets ------------------------------------------------------------------

#[test]
fn overlay_text_follows_its_hooks_and_reads_markup() {
    let d = raw(
        r##"style(#{ overlay_text_size: 9, overlay_text_color: "#ffffff",
                    overlay_text_halo: "#000000", overlay_text_halo_width: 0.05 });
        page(10, 10, overlay(rect(), #{ "top-left": "50 *μ*m", bottom: body("plain") }))"##,
    );
    let t = text_node(&d, "50 μm");
    assert_eq!(t.font_size, Some(9.0));
    assert_eq!(t.color, Color::parse_hex("#ffffff").ok());
    assert_eq!(t.stroke, Color::parse_hex("#000000").ok());
    assert_eq!(t.stroke_width, Some(0.1));
    assert!(t.spans.iter().any(|s| s.text == "μ"), "markup became spans");
    // A node passes through untouched.
    assert_eq!(text_node(&d, "plain").stroke, None);
    // Defaults: base size, fg(), no halo.
    let d = raw(r#"page(10, 10, overlay(rect(), #{ center: "x" }))"#);
    let t = text_node(&d, "x");
    assert_eq!((t.font_size, t.color, t.stroke), (Some(8.0), None, None));
}

#[test]
fn overlay_inset_per_position() {
    let l = lay(
        r#"page(100, 100, overlay(rect(), #{ "top-left": "a", "bottom-right": "b" },
            #{ inset: #{ "top-left": 0, "bottom-right": #{ bottom: 15, right: 5 } } })).unit("pt")"#,
    );
    let a = text_rect(&l, "a");
    approx(a.x, 0.0);
    approx(a.y, 0.0);
    let b = text_rect(&l, "b");
    approx(b.x + b.w, 95.0);
    approx(b.y + b.h, 85.0);
    let e = fail(r#"page(1, 1, overlay(rect(), #{}, #{ inset: #{ middle: 1 } }))"#);
    assert!(
        e.contains(r#"overlay: inset: unknown position "middle""#),
        "{e}"
    );
}

#[test]
fn inset_avoid_keeps_off_an_edge_band() {
    let src = |avoid: &str| {
        format!(
            r#"style(#{{ inset_avoid_band: 20 }});
            page(100, 100, inset(rect(), rect().width(10).height(10),
                #{{ corner: "bottom-right", width: 10, pad: 2{avoid} }})).unit("pt")"#
        )
    };
    let find = |l: &ComputedLayout| l.nodes.iter().find(|n| n.rect.w == 10.0).unwrap().rect;
    let plain = find(&lay(&src("")));
    approx(plain.y + plain.h, 98.0);
    let banded = find(&lay(&src(r#", avoid: "bottom""#)));
    approx(banded.y + banded.h, 78.0);
    approx(banded.x + banded.w, 98.0);
    let mapped = find(&lay(&src(r#", avoid: #{ right: 7 }"#)));
    approx(mapped.x + mapped.w, 91.0);
    let e = fail(r#"page(1, 1, inset(rect(), rect(), #{ avoid: "middle" }))"#);
    assert!(e.contains(r#"inset: avoid: unknown edge "middle""#), "{e}");
}

// ---- scale bar on a box ------------------------------------------------------------------------

#[test]
fn scalebar_on_a_backing_box() {
    let d = raw(
        r##"page(10, 10, annotate(rect(), scalebar(2, "5 μm", #{ box: "#0d0d0d", inset: 0, pad: 0.2 })))"##,
    );
    let dark = Color::parse_hex("#0d0d0d").unwrap();
    let boxed = d
        .nodes
        .values()
        .find(|n| matches!(&n.fill, Some(figs_core::geom::Paint::Solid(c)) if *c == dark))
        .expect("the box");
    assert_eq!(boxed.padding, figs_core::geom::Edges::all(0.2));
    // On a box the label and bar need no halo.
    assert_eq!(text_node(&d, "5 μm").stroke, None);
    let l = lay(
        r#"page(100, 100, annotate(rect(), scalebar(20, "s", #{ box: true, inset: 0, pad: 3 }))).unit("pt")"#,
    );
    let b = l
        .nodes
        .iter()
        .find(|n| matches!(&n.content, PaintContent::Frame(_)))
        .expect("box")
        .rect;
    approx(b.x + b.w, 100.0);
    approx(b.y + b.h, 100.0);
    // Without a box: the old look (haloed label).
    let d = raw(r#"page(10, 10, annotate(rect(), scalebar(2, "x")))"#);
    assert!(text_node(&d, "x").stroke.is_some());
    let e = fail(r#"page(1, 1, scalebar(2, "x", #{ boxed: 1 }))"#);
    assert!(e.contains(r#"unknown option "boxed""#), "{e}");
}

// ---- poster header ---------------------------------------------------------------------------------

#[test]
fn poster_number_above_the_title_and_author_font() {
    let src = |extra: &str| {
        format!(
            r#"style(#{{ author_font: "Georgia" }});
            page(400, 200, poster_header(#{{ title: "T", number: "P-1", authors: ["A"]{extra} }})).unit("pt")"#
        )
    };
    let l = lay(&src(r#", number_position: "above""#));
    let num = text_rect(&l, "P-1");
    let title = text_rect(&l, "T");
    assert!(num.y + num.h <= title.y + 1e-3, "{num:?} above {title:?}");
    let l = lay(&src(""));
    approx(text_rect(&l, "P-1").y, text_rect(&l, "T").y);
    let d = raw(&src(""));
    assert_eq!(text_node(&d, "A").font_family.as_deref(), Some("Georgia"));
    let e = fail(&src(r#", number_position: "below""#));
    assert!(
        e.contains(r#"unknown number_position "below" (title, above)"#),
        "{e}"
    );
}

// ---- variants --------------------------------------------------------------------------------------

#[test]
fn component_variants_from_the_style() {
    let d = raw(
        r##"style(#{ frame_variants: #{ box: #{ frame_color: "#7f7f7f", frame_radius: 0 } },
                    note_variants: #{ quiet: #{ note_color: "#123456" } },
                    ref_font: "Georgia" });
        page(10, 10, col([frame("in", "box"), note("n", "quiet"),
                          frame("plain"), note("m", #{ note_size: 5 }), ref("r")]))"##,
    );
    let frames: Vec<&RawNode> = d
        .nodes
        .values()
        .filter(|n| n.corner_radius.is_some())
        .collect();
    let grey = Color::parse_hex("#7f7f7f").ok();
    assert!(frames
        .iter()
        .any(|n| n.corner_radius == Some(0.0) && n.stroke == grey));
    assert!(
        frames.iter().any(|n| n.corner_radius != Some(0.0)),
        "the variant doesn't leak"
    );
    assert_eq!(text_node(&d, "n").color, Color::parse_hex("#123456").ok());
    assert_eq!(text_node(&d, "m").font_size, Some(5.0));
    assert_eq!(text_node(&d, "r").font_family.as_deref(), Some("Georgia"));
    // table and section take a `variant` option.
    let d = raw(r##"style(#{ table_variants: #{ big: #{ table_size: 20 } },
                    section_variants: #{ red: #{ section_title_color: "#ff0000" } } });
        page(10, 10, col([table([["c"]], #{ header: false, variant: "big" }),
                          section("S", [], #{ variant: "red" })]))"##);
    assert_eq!(text_node(&d, "c").font_size, Some(20.0));
    assert_eq!(
        text_node(&d, "1. S").color,
        Color::parse_hex("#ff0000").ok()
    );
    let e = fail(r#"page(1, 1, frame("x", "box"))"#);
    assert!(
        e.contains(r#"frame: unknown variant "box" (none defined"#),
        "{e}"
    );
    let e = fail(r#"page(1, 1, note("x", #{ colour: 1 }))"#);
    assert!(
        e.contains(r#"note: variant key "colour" is not a style key"#),
        "{e}"
    );
}
