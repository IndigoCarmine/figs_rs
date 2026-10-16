//! `fit_width = "tracking" | "scale"`: each line of a text is stretched or
//! squeezed to exactly its box width, in shaping (so layout and every
//! backend agree).

use figs_core::layout::{ComputedLayout, ComputedNode, PaintContent};
use figs_core::render::geometry::text_props;
use figs_core::schema::{FitWidth, TextProps};
use figs_core::{eval_script, layout, render_png, Document, FontStore, RenderOptions};
use tiny_skia::Pixmap;

const FAMILIES: [&str; 6] = [
    "Liberation Sans",
    "DejaVu Sans",
    "Arial",
    "Times New Roman",
    "Segoe UI",
    "Yu Gothic",
];

fn family(fs: &FontStore) -> Option<&'static str> {
    let have = fs.families();
    FAMILIES
        .iter()
        .copied()
        .find(|f| have.iter().any(|h| h == f))
}

fn props(fam: &str, content: &str, fit: Option<FitWidth>) -> TextProps {
    TextProps {
        content: content.into(),
        font_family: Some(fam.into()),
        font_size: 20.0,
        fit_width: fit,
        ..Default::default()
    }
}

#[test]
fn tracking_fits_each_line_exactly_and_keeps_the_size() {
    let fs = FontStore::new();
    let Some(fam) = family(&fs) else {
        eprintln!("no candidate family installed; skipping");
        return;
    };
    let natural = fs.shape(&props(fam, "Fitted title", None), f32::INFINITY);
    let w0 = natural.lines[0].width;
    // Wider (spread) and narrower (squeezed: no wrapping) than natural.
    for target in [w0 * 1.2, w0 * 0.9] {
        let s = fs.shape(
            &props(fam, "Fitted title", Some(FitWidth::Tracking)),
            target,
        );
        assert_eq!(s.lines.len(), 1, "a fitted text never wraps");
        let line = &s.lines[0];
        assert!(
            (line.width - target).abs() < 1e-3,
            "{} vs {target}",
            line.width
        );
        assert!((s.size.w - target).abs() < 1e-3);
        let first = &line.glyphs[0];
        let last = line.glyphs.last().unwrap();
        assert_eq!(first.x, natural.lines[0].glyphs[0].x);
        assert!((last.x + last.advance - target).abs() < 1e-3);
        // Same glyphs, same sizes, same box height; evenly spaced extra.
        assert!(line.glyphs.iter().all(|g| g.font_size == 20.0));
        assert!((s.size.h - natural.size.h).abs() < 1e-3);
        let n = line.glyphs.len();
        let extra = (target - w0) / (n - 1) as f32;
        for (k, (g, g0)) in line.glyphs.iter().zip(&natural.lines[0].glyphs).enumerate() {
            assert!((g.x - g0.x - k as f32 * extra).abs() < 1e-3);
        }
    }
}

#[test]
fn tracking_counts_the_trailing_letter_spacing_out() {
    let fs = FontStore::new();
    let Some(fam) = family(&fs) else {
        return;
    };
    let mut p = props(fam, "Tracked", Some(FitWidth::Tracking));
    p.letter_spacing = 0.1;
    let s = fs.shape(&p, 150.0);
    assert!((s.lines[0].width - 150.0).abs() < 1e-3);
}

#[test]
fn scale_fits_each_line_by_scaling_the_size() {
    let fs = FontStore::new();
    let Some(fam) = family(&fs) else {
        return;
    };
    let natural = fs.shape(&props(fam, "Scaled", None), f32::INFINITY);
    let w0 = natural.lines[0].width;
    let target = w0 * 1.5;
    let s = fs.shape(&props(fam, "Scaled", Some(FitWidth::Scale)), target);
    let line = &s.lines[0];
    assert!((line.width - target).abs() < 1e-3);
    assert!(line
        .glyphs
        .iter()
        .all(|g| (g.font_size - 30.0).abs() < 1e-3));
    // Line box and baseline scale with the size.
    assert!(
        (s.size.h - natural.size.h * 1.5).abs() < 1e-2,
        "{:?}",
        s.size
    );
    assert!((line.baseline - natural.lines[0].baseline * 1.5).abs() < 1e-2);
}

#[test]
fn every_paragraph_is_fitted_on_its_own() {
    let fs = FontStore::new();
    let Some(fam) = family(&fs) else {
        return;
    };
    for fit in [FitWidth::Tracking, FitWidth::Scale] {
        let s = fs.shape(&props(fam, "Short\nA longer second line", Some(fit)), 180.0);
        assert_eq!(s.lines.len(), 2);
        for l in &s.lines {
            assert!((l.width - 180.0).abs() < 1e-3, "{fit:?}: {}", l.width);
        }
    }
}

fn text_node(l: &ComputedLayout) -> &ComputedNode {
    l.nodes
        .iter()
        .find(|n| matches!(n.content, PaintContent::Text(_)))
        .unwrap()
}

#[test]
fn layout_fits_to_the_explicit_width_and_leaves_content_sized_text_alone() {
    let fs = FontStore::new();
    let Some(fam) = family(&fs) else {
        return;
    };
    for fit in ["tracking", "scale"] {
        // An explicit width (in cm): the box is that wide, one line.
        let src = format!(
            r#"page(10, 4, stack([text("Wide header line").font("{fam}").size(20)
                .fit_width("{fit}").width(8).at(1, 1)])).unit("cm")"#
        );
        let l = layout(&Document::from_script(&src).unwrap(), &fs);
        let t = text_node(&l);
        let PaintContent::Text(tb) = &t.content else {
            unreachable!()
        };
        let s = fs.shape(&text_props(tb), t.rect.w);
        assert!((t.rect.w - 8.0 * 72.0 / 2.54).abs() < 1e-2);
        assert_eq!(s.lines.len(), 1);
        assert!((s.lines[0].width - t.rect.w).abs() < 1e-2, "{fit}");
        // The box is as tall as what is drawn (scale changes the height).
        assert!(
            (s.size.h - t.rect.h).abs() < 1e-2,
            "{fit}: {} vs {}",
            s.size.h,
            t.rect.h
        );

        // Content-sized (no width, loose): exactly the natural text.
        let src = format!(
            r#"page(10, 4, stack([text("Natural").font("{fam}").size(20)
                .fit_width("{fit}").at(1, 1)])).unit("cm")"#
        );
        let l = layout(&Document::from_script(&src).unwrap(), &fs);
        let plain = fs.shape(&props(fam, "Natural", None), f32::INFINITY);
        assert!((text_node(&l).rect.w - plain.size.w).abs() < 1e-3);
    }
}

#[test]
fn a_fitted_line_spans_its_box_in_the_png() {
    let fs = FontStore::new();
    let Some(fam) = family(&fs) else {
        return;
    };
    // 72 dpi on a pt page: 1 px = 1 pt. "H…H" has small side bearings.
    let src = format!(
        r##"page(400, 60, stack([text("HHH HHH").font("{fam}").size(30)
            .fit_width().width(300).at(50, 10)])).unit("pt").dpi(72).background("#ffffff")"##
    );
    let l = layout(&Document::from_script(&src).unwrap(), &fs);
    let png = render_png(
        &l,
        &RenderOptions {
            fonts: Some(&fs),
            base_dir: std::path::Path::new("."),
        },
    )
    .unwrap();
    let p = Pixmap::decode_png(&png).unwrap();
    let inked: Vec<u32> = (0..p.width())
        .filter(|&x| (0..p.height()).any(|y| p.pixel(x, y).unwrap().demultiply().red() < 128))
        .collect();
    let (x0, x1) = (inked[0] as f32, *inked.last().unwrap() as f32 + 1.0);
    assert!((x0 - 50.0).abs() < 4.0, "ink starts at {x0}");
    assert!((x1 - 350.0).abs() < 4.0, "ink ends at {x1}");
}

/// `examples/fine_layout.figs` (fit_width, stretch with sized children, a
/// list placed by its baseline, hairlines) evaluates and lays out.
#[test]
fn fine_layout_example() {
    let fs = FontStore::new();
    let doc = Document::from_script(include_str!("../../../examples/fine_layout.figs")).unwrap();
    assert_eq!(doc.page.min_stroke_px, 1.0);
    let l = layout(&doc, &fs);
    let title = l
        .nodes
        .iter()
        .find(|n| matches!(&n.content, PaintContent::Text(t) if t.fit_width.is_some()))
        .unwrap();
    assert!((title.rect.w - 14.0 * 72.0 / 2.54).abs() < 1e-2);
}

#[test]
fn script_and_toml_setters() {
    let raw = eval_script(
        r#"page(10, 10, stack([text("a").fit_width(), text("b").fit_width("scale")]))"#,
    )
    .unwrap();
    let mut fits: Vec<_> = raw.nodes.values().filter_map(|n| n.fit_width).collect();
    fits.sort_by_key(|f| *f == FitWidth::Scale);
    assert_eq!(fits, vec![FitWidth::Tracking, FitWidth::Scale]);
    let toml = toml::to_string(&raw).unwrap();
    assert!(toml.contains(r#"fit_width = "scale""#), "{toml}");
    for bad in [
        r#"page(10, 10, stack([text("a").fit_width("squash")]))"#,
        r#"page(10, 10, stack([rect().fit_width()]))"#,
    ] {
        assert!(eval_script(bad).is_err(), "{bad}");
    }
    let e = Document::from_toml(
        r#"
        [page]
        width = 10
        height = 10
        root = "r"
        [nodes.r]
        type = "rect"
        fit_width = "tracking"
        "#,
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("fit_width"), "{e}");
}
