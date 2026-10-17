//! Framework: one image scale (`image_dpi`), overhanging captions (`step`,
//! `captioned`, `fill_row(.., #{ captions })`), sections that fit / align
//! their content, zoom-link outlines and cones, per-call overlay text, shift
//! arrow options and header logo boxes / number inset.

#![cfg(feature = "script")]

use figs_core::eval_script;
use figs_core::geom::Overflow;
use figs_core::schema::{ConnectorRoute, EndShape, NodeKind, RawDocument, RawNode};

fn raw(src: &str) -> RawDocument {
    eval_script(src).unwrap_or_else(|e| panic!("script failed: {e}"))
}

fn fail(src: &str) -> String {
    eval_script(src)
        .expect_err("script should fail")
        .to_string()
}

/// The text node whose content (or spans' text) is `s`.
fn text<'a>(d: &'a RawDocument, s: &str) -> &'a RawNode {
    d.nodes
        .values()
        .find(|n| {
            let spans: String = n.spans.iter().map(|x| x.text.as_str()).collect();
            n.kind == NodeKind::Text && (n.content.as_deref() == Some(s) || spans == s)
        })
        .unwrap_or_else(|| panic!("no text {s:?}"))
}

/// The text of the page's only text node (a script's computed answer).
fn answer(src: &str) -> String {
    let d = raw(src);
    d.nodes
        .values()
        .find_map(|n| n.content.clone())
        .expect("a text node")
}

#[test]
fn image_dpi_style_sets_the_page_default() {
    let d = raw(r#"style(#{ image_dpi: 150 }); page("double", col([image("a.png")]))"#);
    assert_eq!(d.page.image_dpi, Some(figs_core::geom::Dpi::Value(150.0)));
    let d = raw(r#"page("double", col([image("a.png")]))"#);
    assert_eq!(d.page.image_dpi, None);
    // Posters go through page(preset, ..) too.
    let d = raw(r#"style(#{ image_dpi: 300 }); poster("a4", (), [image("a.png")])"#);
    assert_eq!(d.page.image_dpi, Some(figs_core::geom::Dpi::Value(300.0)));
}

#[test]
fn captions_overhang_on_request() {
    // step(node, caption, opts)
    let d = raw(
        r#"page(10, 10, step(rect().width(1).height(1), "a long caption", #{ overhang: true }))"#,
    );
    assert!(!text(&d, "a long caption").overhang.is_off());
    let d = raw(r#"page(10, 10, step(rect().width(1).height(1), "plain"))"#);
    assert!(text(&d, "plain").overhang.is_off());
    // The hook turns it on everywhere; captioned takes the option too.
    let d = raw(r#"style(#{ caption_overhang: true });
        page(10, 10, col([step(rect(), #{ above: "up", below: "down" }), captioned(rect(), "cap")]))"#);
    for s in ["up", "down", "cap"] {
        assert!(!text(&d, s).overhang.is_off(), "{s}");
    }
    let d = raw(r#"page(10, 10, captioned(rect(), "cap", #{ overhang: true }))"#);
    assert!(!text(&d, "cap").overhang.is_off());
    // fill_row rebuilds captioned / step / panels items either way.
    let d = raw(
        r#"page(10, 10, fill_row([captioned(image("a.png"), "one"), step(image("b.png"), "two"),
            image("c.png")] + panels([image("d.png")], ["three"]), #{ captions: "overhang" }))"#,
    );
    for s in ["one", "two", "three"] {
        assert!(!text(&d, s).overhang.is_off(), "{s}");
    }
    let d = raw(r#"style(#{ caption_overhang: true });
        page(10, 10, fill_row([captioned(image("a.png"), "one")], #{ captions: "wrap" }))"#);
    assert!(text(&d, "one").overhang.is_off());
    let e = fail(r#"page(10, 10, fill_row([image("a.png")], #{ captions: "float" }))"#);
    assert!(
        e.contains("unknown captions \"float\" (overhang, wrap, auto)"),
        "{e}"
    );
    let e = fail(r#"page(10, 10, step(rect(), "x", #{ overhang: 1 }))"#);
    assert!(e.contains("overhang is true, false or"), "{e}");
}

/// The column named `name`.
fn named<'a>(d: &'a RawDocument, name: &str) -> &'a RawNode {
    d.nodes
        .values()
        .find(|n| n.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named {name:?}"))
}

#[test]
fn sections_fit_and_align_their_content() {
    use figs_core::geom::MainAxisAlign as M;
    let d = raw(r#"page("a4", section("S", [rect()], #{ fit: "both" }))"#);
    let body = named(&d, "section \"S\" (body)");
    assert_eq!(body.overflow, Some(Overflow::Fit));
    assert_eq!(body.main_axis_alignment, Some(M::Center));
    let d = raw(r#"page("a4", section("S", [rect()], #{ fit: "both", valign: "top" }))"#);
    assert_eq!(
        named(&d, "section \"S\" (body)").main_axis_alignment,
        Some(M::Start)
    );
    let d = raw(r#"page("a4", section("S", [rect()], #{ overflow: "fit" }))"#);
    assert_eq!(
        named(&d, "section \"S\" (body)").overflow,
        Some(Overflow::Fit)
    );
    let d = raw(r#"page("a4", section("S", [rect(), rect()], #{ valign: "spread" }))"#);
    assert_eq!(
        named(&d, "section \"S\" (body)").main_axis_alignment,
        Some(M::SpaceBetween)
    );
    // With a footer: the content column does the aligning.
    let d = raw(r#"page("a4", section("S", [rect()], #{ valign: "bottom", footer: "f" }))"#);
    assert_eq!(
        named(&d, "section \"S\" (content)").main_axis_alignment,
        Some(M::End)
    );
    let d = raw(r#"style(#{ section_valign: "center" }); page("a4", section("S", [rect()]))"#);
    assert_eq!(
        named(&d, "section \"S\" (body)").main_axis_alignment,
        Some(M::Center)
    );
    let e = fail(r#"page("a4", section("S", [rect()], #{ fit: "grow" }))"#);
    assert!(
        e.contains("unknown fit \"grow\" (both, shrink, flex, none)"),
        "{e}"
    );
    let e = fail(r#"page("a4", section("S", [rect()], #{ valign: "middle" }))"#);
    assert!(e.contains("(top, center, bottom, spread)"), "{e}");
    let e = fail(r#"page("a4", section("S", [rect()], #{ fit: "both", overflow: "clip" }))"#);
    assert!(e.contains("fit or overflow"), "{e}");
}

fn connectors(d: &RawDocument) -> Vec<&RawNode> {
    d.nodes
        .values()
        .filter(|n| n.kind == NodeKind::Connector)
        .collect()
}

#[test]
fn zoom_link_outlines_and_cones() {
    let pre = r#"let a = rect().width(4).height(3); let b = rect().width(2).height(2);"#;
    let d = raw(&format!(
        r#"{pre} page(10, 10, col([row([a, b]), zoom_link(a, b, #{{ outline: "circle" }})]))"#
    ));
    let cs = connectors(&d);
    // The round outline, the copy's frame (4) and two lines from the ellipse.
    assert_eq!(cs.len(), 7);
    let outline: Vec<_> = cs
        .iter()
        .filter(|c| c.route == Some(ConnectorRoute::Outline))
        .collect();
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].from_shape, Some(EndShape::Ellipse));
    assert_eq!(
        cs.iter()
            .filter(|c| c.from_shape == Some(EndShape::Ellipse))
            .count(),
        3
    );
    let d = raw(&format!(
        r#"{pre} page(10, 10, col([row([a, b]), zoom_link(a, b, #{{ style: "cone", outline: "none", frame: false, opacity: 0.4 }})]))"#
    ));
    let cs = connectors(&d);
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].route, Some(ConnectorRoute::Cone));
    assert_eq!(cs[0].opacity, Some(0.4));
    assert!(cs[0].fill.is_some());
    // Hooks.
    let d = raw(&format!(
        r#"{pre} style(#{{ zoom_style: "cone", zoom_outline: "none", zoom_copy_frame: false }});
        page(10, 10, col([row([a, b]), zoom_link(a, b)]))"#
    ));
    assert_eq!(connectors(&d).len(), 1);
    let e = fail(&format!(
        r#"{pre} page(10, 10, col([zoom_link(a, b, #{{ outline: "star" }})]))"#
    ));
    assert!(
        e.contains("unknown outline \"star\" (rect, circle, none)"),
        "{e}"
    );
    let e = fail(&format!(
        r#"{pre} page(10, 10, col([zoom_link(a, b, #{{ style: "fan" }})]))"#
    ));
    assert!(e.contains("unknown style \"fan\" (lines, cone)"), "{e}");
}

#[test]
fn overlay_takes_its_own_text_style() {
    let d = raw(
        r##"page(10, 10, col([overlay(rect(), #{ top: "big" }, #{ size: 22, color: "#d64541" }),
                             overlay(rect(), #{ top: "plain" })]))"##,
    );
    let big = text(&d, "big");
    assert_eq!(big.font_size, Some(22.0));
    assert_eq!(big.color.map(|c| c.to_hex()), Some("#d64541".to_string()));
    // Nothing leaks into the next overlay.
    assert_ne!(text(&d, "plain").font_size, Some(22.0));
}

#[test]
fn shift_arrows_skip_short_ones_and_anchor_on_the_curve() {
    let pre = r#"let a = line_plot([0, 10], [0, 10]); let b = line_plot([0, 10], [1, 30]);"#;
    // At x = 0 the shift is 1, at 5 it is 10.5: min_length 2 keeps one.
    let n = answer(&format!(
        r#"{pre} let r = plot_shift_arrows(a, b, [0, 5], #{{ min_length: 2 }}); page(1, 1, text(`${{r.len()}}`))"#
    ));
    assert_eq!(n, "1");
    // anchor "curve": starts on the first series (y = 5 at x = 5).
    let s = answer(&format!(
        r#"{pre} let r = plot_shift_arrows(a, b, [5], #{{ anchor: "curve" }}); page(1, 1, text(`${{r[0].pts[1]}}`))"#
    ));
    assert_eq!(s.parse::<f64>().unwrap(), 5.0);
    let s = answer(&format!(
        r#"{pre} let r = plot_shift_arrows(a, b, [5]); page(1, 1, text(`${{r[0].pts[1]}}`))"#
    ));
    assert!(s.parse::<f64>().unwrap() > 5.0);
    let e = fail(&format!(
        r#"{pre} page(1, 1, plot(#{{}}, [a, b, plot_shift_arrows(a, b, [5], #{{ anchor: "mid" }})]))"#
    ));
    assert!(e.contains("unknown anchor \"mid\" (gap, curve)"), "{e}");
}

#[test]
fn header_logo_boxes_and_number_inset() {
    let d = raw(
        r#"page("a4", poster_header(#{ title: "T", number: "P-1", number_inset: 0.5,
            logos: ["a.png", "b.png"], logo_box: [2, 1] }))"#,
    );
    assert_eq!(text(&d, "P-1").margin.right, 0.5);
    let boxes: Vec<_> = d
        .nodes
        .values()
        .filter(|n| n.width == Some(2.0) && n.height == Some(1.0))
        .collect();
    assert_eq!(boxes.len(), 2);
    let logos: Vec<_> = d
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Image)
        .collect();
    assert!(logos
        .iter()
        .all(|n| n.fit == Some(figs_core::geom::ImageFit::Contain) && n.height.is_none()));
    // Hooks.
    let d = raw(
        r#"style(#{ poster_logo_box: [3, 1.5], poster_number_inset: 0.25 });
        page("a4", poster_header(#{ title: "T", number: "P-2", logos: ["a.png"] }))"#,
    );
    assert_eq!(text(&d, "P-2").margin.right, 0.25);
    assert!(d
        .nodes
        .values()
        .any(|n| n.width == Some(3.0) && n.height == Some(1.5)));
    let e = fail(r#"page("a4", poster_header(#{ logos: ["a.png"], logo_box: [2] }))"#);
    assert!(e.contains("logo_box is [w, h]"), "{e}");
}

#[test]
fn the_image_scale_example_evaluates() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let src = std::fs::read_to_string(dir.join("image_scale.figs")).unwrap();
    let d = figs_core::eval_script_in(&src, Some(&dir)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(d.page.image_dpi, Some(figs_core::geom::Dpi::Value(300.0)));
}
