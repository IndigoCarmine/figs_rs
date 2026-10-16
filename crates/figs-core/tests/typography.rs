//! Script-scoped font families, Office script metrics, synthetic bold and
//! slant, span outlines, and keeping super/subscripts on their line.
//!
//! Tests that need particular fonts (Arial, Yu Gothic, MS Mincho, Times New
//! Roman) check the font database first and return early when they are
//! missing, so the suite stays green on machines without them.

use std::path::Path;

use figs_core::schema::span::{
    FontSynthesis, ScriptMetrics, OFFICE_SLANT_DEG, SYNTHETIC_BOLD_WIDTH,
};
use figs_core::schema::{NodeType, Script, SpanStyle, TextProps, TextSpan};
use figs_core::text::{char_class, CharClass, ShapedGlyph, ShapedText};
use figs_core::{layout, render_png, render_svg, Document, FontStore, RenderOptions};

fn installed(fs: &FontStore, families: &[&str]) -> bool {
    let all = fs.families();
    let ok = families
        .iter()
        .all(|f| all.iter().any(|a| a.eq_ignore_ascii_case(f)));
    if !ok {
        eprintln!("skipping: needs {families:?}");
    }
    ok
}

/// The family name of the face a glyph was shaped with.
fn face_family(fs: &FontStore, g: &ShapedGlyph) -> String {
    fs.with_system(|s| {
        s.db()
            .face(g.font_id)
            .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
            .unwrap_or_default()
    })
}

/// (character, family) of every glyph of the first line.
fn families_of(fs: &FontStore, t: &TextProps) -> Vec<(char, String)> {
    let s = fs.shape(t, f32::INFINITY);
    s.lines[0]
        .glyphs
        .iter()
        .map(|g| {
            let ch = t.content[g.text_range.clone()].chars().next().unwrap();
            (ch, face_family(fs, g))
        })
        .collect()
}

fn text(content: &str, family: &str) -> TextProps {
    let mut t = TextProps::plain(content, 12.0);
    t.font_family = Some(family.into());
    t
}

/// Text made of spans `(text, style)`.
fn spans(parts: &[(&str, SpanStyle)], size: f32) -> TextProps {
    let mut t = TextProps::plain("", size);
    for (s, st) in parts {
        let start = t.content.len();
        t.content.push_str(s);
        t.spans.push(TextSpan {
            range: start..t.content.len(),
            style: st.clone(),
        });
    }
    t
}

fn text_props(doc: &Document) -> TextProps {
    doc.nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeType::Text(t) => Some(t.clone()),
            _ => None,
        })
        .expect("a text node")
}

fn opts(fs: &FontStore) -> RenderOptions<'_> {
    RenderOptions {
        fonts: Some(fs),
        base_dir: Path::new("."),
    }
}

// ---------------------------------------------------------------------------
// Script classes and per-script families
// ---------------------------------------------------------------------------

#[test]
fn characters_are_classified_like_office() {
    for c in ['a', 'Z', '7', ' ', '.', 'Δ', 'ж', 'é'] {
        assert_eq!(char_class(c), CharClass::Latin, "{c:?}");
    }
    for c in ['か', 'カ', '・', '漢', '。', '（', '０', '\u{3000}'] {
        assert_eq!(char_class(c), CharClass::Cjk, "{c:?}");
    }
    for c in ['×', '—', '○', '■', '−', '°', '①', '→'] {
        assert_eq!(char_class(c), CharClass::Ambiguous, "{c:?}");
    }
    assert_eq!(char_class('😀'), CharClass::Other);
}

#[test]
fn latin_and_cjk_slots_pick_their_own_family() {
    let fs = FontStore::new();
    if !installed(&fs, &["Yu Gothic", "MS Mincho"]) {
        return;
    }
    // Both faces cover Latin, so a plain list can't do this: Latin (and
    // spaces, digits) in Yu Gothic, Japanese in MS Mincho.
    let t = text("○山田 花子 12・Ab", "latin: Yu Gothic; cjk: MS Mincho");
    let fams = families_of(&fs, &t);
    for (ch, fam) in &fams {
        let want = match ch {
            '山' | '田' | '花' | '子' | '・' => "MS Mincho",
            // ○ opens a Japanese name: it follows the CJK text after it.
            '○' => "MS Mincho",
            _ => "Yu Gothic",
        };
        assert_eq!(fam, want, "{ch:?} in {fams:?}");
    }
    // The plain list sets everything in its first family.
    let fams = families_of(&fs, &text("山 A", "MS Mincho, Yu Gothic"));
    assert!(fams.iter().all(|(_, f)| f == "MS Mincho"), "{fams:?}");
}

#[test]
fn ambiguous_symbols_follow_their_neighbours_or_the_symbol_slot() {
    let fs = FontStore::new();
    if !installed(&fs, &["Arial", "MS Mincho", "Times New Roman"]) {
        return;
    }
    let spec = "latin: Arial; cjk: MS Mincho";
    let fams = families_of(&fs, &text("5 × 2、結果 × 2", spec));
    let times: Vec<&String> = fams
        .iter()
        .filter(|(c, _)| *c == '×')
        .map(|(_, f)| f)
        .collect();
    assert_eq!(times, ["Arial", "MS Mincho"], "{fams:?}");
    // A `symbol` slot takes them all.
    let fams = families_of(
        &fs,
        &text(
            "5 × 2、結果 × 2",
            "latin: Arial; cjk: MS Mincho; symbol: Times New Roman",
        ),
    );
    assert!(fams
        .iter()
        .filter(|(c, _)| *c == '×')
        .all(|(_, f)| f == "Times New Roman"));
}

#[test]
fn slots_inherit_from_page_to_node_to_span() {
    // Page: a plain list; node: only the CJK slot; span: only the Latin slot.
    let fs = FontStore::new().with_default_family("Arial, Yu Gothic");
    if !installed(&fs, &["Arial", "Yu Gothic", "MS Mincho", "Times New Roman"]) {
        return;
    }
    let mut t = spans(
        &[
            ("a山", SpanStyle::default()),
            (
                "b川",
                SpanStyle {
                    font_family: Some("latin: Times New Roman".into()),
                    ..Default::default()
                },
            ),
        ],
        12.0,
    );
    t.font_family = Some("cjk: MS Mincho".into());
    let fams: Vec<String> = families_of(&fs, &t).into_iter().map(|(_, f)| f).collect();
    assert_eq!(fams, ["Arial", "MS Mincho", "Times New Roman", "MS Mincho"]);
    // A plain span list still replaces everything.
    t.spans[1].style.font_family = Some("Yu Gothic".into());
    let fams: Vec<String> = families_of(&fs, &t).into_iter().map(|(_, f)| f).collect();
    assert_eq!(fams, ["Arial", "MS Mincho", "Yu Gothic", "Yu Gothic"]);
}

#[test]
fn per_script_tables_parse_from_toml_and_scripts() {
    let doc = Document::from_toml(
        r##"
        [page]
        width = 100
        height = 40
        unit = "pt"
        root = "t"
        font_family = { latin = "Arial", cjk = ["Yu Gothic", "MS Mincho"] }
        [nodes.t]
        type = "text"
        content = "x"
        font_family = { cjk = "MS Mincho" }
        "##,
    )
    .unwrap();
    assert_eq!(
        doc.page.font_family.as_deref(),
        Some("latin: Arial; cjk: Yu Gothic, MS Mincho")
    );
    assert_eq!(
        text_props(&doc).font_family.as_deref(),
        Some("cjk: MS Mincho")
    );
    // An unknown slot is an error naming it.
    let e = Document::from_toml(
        r##"
        [page]
        width = 10
        height = 10
        root = "t"
        [nodes.t]
        type = "text"
        content = "x"
        font_family = "latim: Arial"
        "##,
    )
    .unwrap_err();
    assert!(e.to_string().contains("latim") || format!("{e:?}").contains("latim"));

    let doc = Document::from_script(
        r#"page(10, 4, text([span("A"), span("字").font(#{ cjk: "MS Mincho" })])
                .font(#{ latin: "Arial", cjk: ["Yu Gothic"] }))
            .font(#{ latin: "Times New Roman" })"#,
    )
    .unwrap();
    assert_eq!(
        doc.page.font_family.as_deref(),
        Some("latin: Times New Roman")
    );
    let t = text_props(&doc);
    assert_eq!(
        t.font_family.as_deref(),
        Some("latin: Arial; cjk: Yu Gothic")
    );
    assert_eq!(
        t.spans[1].style.font_family.as_deref(),
        Some("cjk: MS Mincho")
    );
    assert!(Document::from_script(r#"page(1, 1, text("x").font(#{ latn: "A" }))"#).is_err());
}

// ---------------------------------------------------------------------------
// Script metrics
// ---------------------------------------------------------------------------

fn script_glyphs(metrics: Option<&str>) -> (f32, f32, f32, f32) {
    let fs = FontStore::new();
    let src = format!(
        r##"
        [page]
        width = 300
        height = 60
        unit = "pt"
        root = "t"
        {}
        [nodes.t]
        type = "text"
        font_size = 30
        spans = [{{ text = "x" }}, {{ text = "2", script = "super" }}, {{ text = "y" }}, {{ text = "3", script = "sub" }}]
        "##,
        metrics.map_or(String::new(), |m| format!("script_metrics = \"{m}\""))
    );
    let doc = Document::from_toml(&src).unwrap();
    let s = fs.shape(&text_props(&doc), f32::INFINITY);
    let g = &s.lines[0].glyphs;
    (g[1].font_size, g[1].y_offset, g[3].font_size, g[3].y_offset)
}

#[test]
fn office_script_metrics_match_powerpoint() {
    let (sup_size, sup_y, sub_size, sub_y) = script_glyphs(Some("office"));
    assert!((sup_size - 20.0).abs() < 1e-3, "{sup_size}"); // 30 × 2/3
    assert!((sup_y - 9.0).abs() < 1e-3, "{sup_y}"); // +30 % of 30 pt
    assert!((sub_size - 20.0).abs() < 1e-3, "{sub_size}");
    assert!((sub_y + 7.5).abs() < 1e-3, "{sub_y}"); // −25 %
                                                    // The default stays figs' own.
    let (sup_size, sup_y, _, sub_y) = script_glyphs(None);
    assert!((sup_size - 19.5).abs() < 1e-3);
    assert!((sup_y - 10.5).abs() < 1e-3);
    assert!((sub_y + 4.5).abs() < 1e-3);
    // A node overrides the page; Rhai `page(..).scripts("office")`.
    let doc = Document::from_script(
        r#"page(10, 4, col([text([span("a"), span("1").sup()]),
                          text([span("b"), span("2").sup()]).scripts("figs")]))
            .scripts("office")"#,
    )
    .unwrap();
    let metrics: Vec<ScriptMetrics> = doc
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeType::Text(t) => Some(t.typography.script_metrics),
            _ => None,
        })
        .collect();
    assert!(metrics.contains(&ScriptMetrics::Office) && metrics.contains(&ScriptMetrics::Figs));
    assert!(Document::from_script(r#"page(1, 1, text("x")).scripts("word")"#).is_err());
}

// ---------------------------------------------------------------------------
// Line breaking next to scripts
// ---------------------------------------------------------------------------

#[test]
fn no_line_break_right_before_an_attached_superscript() {
    let fs = FontStore::new();
    let sup = SpanStyle {
        script: Some(Script::Super),
        ..Default::default()
    };
    let mut t = spans(
        &[
            ("山田花子", SpanStyle::default()),
            ("1", sup.clone()),
            ("、川村一郎", SpanStyle::default()),
            ("2,*", sup.clone()),
            ("、Taro Kimura", SpanStyle::default()),
            ("3", sup),
        ],
        12.0,
    );
    t.font_family = Some("Yu Gothic, MS Gothic, sans-serif".into());
    let natural = fs.shape(&t, f32::INFINITY).size.w;
    let mut breaks_checked = 0;
    let mut w = 20.0;
    while w < natural + 5.0 {
        let s: ShapedText = fs.shape(&t, w);
        for pair in s.lines.windows(2) {
            let (Some(last), Some(first)) = (pair[0].glyphs.last(), pair[1].glyphs.first()) else {
                continue;
            };
            breaks_checked += 1;
            let first_is_script = first.y_offset > 0.5;
            let glued = last.text_range.end == first.text_range.start
                && !t.content[last.text_range.clone()]
                    .chars()
                    .all(char::is_whitespace);
            assert!(
                !(first_is_script && glued),
                "width {w}: a line starts with the superscript {:?} right after {:?}",
                &t.content[first.text_range.clone()],
                &t.content[last.text_range.clone()],
            );
        }
        // Nothing lost or duplicated (a space at a break may be dropped).
        let ink = |gs: &mut dyn Iterator<Item = &ShapedGlyph>| {
            gs.filter(|g| !t.content[g.text_range.clone()].trim().is_empty())
                .count()
        };
        let one = fs.shape(&t, f32::INFINITY);
        assert_eq!(
            ink(&mut s.lines.iter().flat_map(|l| &l.glyphs)),
            ink(&mut one.lines[0].glyphs.iter())
        );
        w += 1.5;
    }
    assert!(breaks_checked > 20);
}

// ---------------------------------------------------------------------------
// Synthetic bold, span outlines, slant
// ---------------------------------------------------------------------------

#[test]
fn synthetic_bold_where_the_family_has_no_bold_face() {
    let fs = FontStore::new();
    if !installed(&fs, &["MS Mincho", "Yu Gothic"]) {
        return;
    }
    let mut t = text("明朝体", "MS Mincho");
    t.font_size = 35.0;
    t.font_weight = Some(700);
    let bold_of = |t: &TextProps| -> Vec<f32> {
        fs.shape(t, f32::INFINITY).lines[0]
            .glyphs
            .iter()
            .map(|g| g.embolden)
            .collect()
    };
    // Default: no synthesis.
    assert!(bold_of(&t).iter().all(|&b| b == 0.0));
    // Auto: MS Mincho has no bold face -> a size/35 outline (1 pt at 35 pt).
    t.typography.font_synthesis = FontSynthesis::Auto;
    for b in bold_of(&t) {
        assert!((b - 35.0 * SYNTHETIC_BOLD_WIDTH).abs() < 1e-4 && (b - 1.0).abs() < 1e-4);
    }
    // ... but not where a real bold face exists, nor for regular weight.
    let mut yu = t.clone();
    yu.font_family = Some("Yu Gothic".into());
    assert!(bold_of(&yu).iter().all(|&b| b == 0.0));
    t.font_weight = None;
    assert!(bold_of(&t).iter().all(|&b| b == 0.0));
    // "bold" emboldens everything, with a tunable width.
    t.typography.font_synthesis = FontSynthesis::Bold;
    t.typography.synthetic_bold_width = 0.05;
    assert!(bold_of(&t).iter().all(|&b| (b - 1.75).abs() < 1e-4));
}

const BOLD_TOML: &str = r##"
[page]
width = 220
height = 60
unit = "pt"
root = "t"
font_synthesis = "bold"
[nodes.t]
type = "text"
content = "Bold 太"
font_size = 30
color = "#203040"
"##;

fn dark_pixels(png: &[u8]) -> usize {
    let pm = tiny_skia::Pixmap::decode_png(png).unwrap();
    pm.pixels()
        .iter()
        .filter(|p| p.demultiply().red() < 128)
        .count()
}

#[test]
fn synthetic_bold_thickens_png_svg_and_pdf() {
    let fs = FontStore::new();
    let bold = Document::from_toml(BOLD_TOML).unwrap();
    let plain = Document::from_toml(&BOLD_TOML.replace("font_synthesis = \"bold\"", "")).unwrap();
    let (lb, lp) = (layout(&bold, &fs), layout(&plain, &fs));
    // Same layout: synthesis is paint-only.
    assert_eq!(lb.nodes[0].rect, lp.nodes[0].rect);

    // PNG: more ink.
    let (ib, ip) = (
        dark_pixels(&render_png(&lb, &opts(&fs)).unwrap()),
        dark_pixels(&render_png(&lp, &opts(&fs)).unwrap()),
    );
    assert!(ib as f32 > ip as f32 * 1.1, "{ib} vs {ip}");

    // SVG: one stroked path in the text colour before the glyph group.
    let svg = render_svg(&lb, &opts(&fs));
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let stroked: Vec<_> = xml
        .descendants()
        .filter(|n| n.tag_name().name() == "path" && n.attribute("stroke") == Some("#203040"))
        .collect();
    assert_eq!(stroked.len(), 1, "{svg}");
    let w: f32 = stroked[0]
        .attribute("stroke-width")
        .unwrap()
        .parse()
        .unwrap();
    assert!((w - 30.0 / 35.0).abs() < 1e-3, "{w}");
    assert_eq!(stroked[0].attribute("stroke-linejoin"), Some("round"));
    assert!(svg.find("stroke=\"#203040\"").unwrap() < svg.find("aria-label").unwrap());
    let plain_svg = render_svg(&lp, &opts(&fs));
    assert!(!plain_svg.contains("stroke=\"#203040\""));

    #[cfg(feature = "pdf")]
    {
        let tokens = pdf_tokens(&figs_core::render_pdf(&lb, &opts(&fs)).unwrap());
        let plain = pdf_tokens(&figs_core::render_pdf(&lp, &opts(&fs)).unwrap());
        let count = |t: &[String], op: &str| t.iter().filter(|x| *x == op).count();
        let shows = |t: &[String]| t.iter().filter(|x| x.ends_with("TJ")).count();
        // The text is still real text, shown exactly as often as without
        // synthesis; the bold is one stroked path under it.
        assert_eq!(count(&tokens, "BT"), count(&plain, "BT"));
        assert_eq!(shows(&tokens), shows(&plain));
        assert_eq!(count(&plain, "S"), 0);
        assert_eq!(count(&tokens, "S"), 1);
        let pos = |op: &str| tokens.iter().position(|x| x == op).unwrap();
        assert!(pos("S") < pos("BT"));
    }
}

/// Whitespace-split tokens of the inflated page content stream(s).
#[cfg(feature = "pdf")]
fn pdf_tokens(pdf: &[u8]) -> Vec<String> {
    let find = |hay: &[u8], needle: &str| {
        hay.windows(needle.len())
            .position(|w| w == needle.as_bytes())
    };
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = find(&pdf[at..], "stream\n") {
        let start = at + i + "stream\n".len();
        let Some(len) = find(&pdf[start..], "endstream") else {
            break;
        };
        if let Ok(data) = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..start + len]) {
            let s = String::from_utf8_lossy(&data).into_owned();
            if s.split_whitespace().any(|t| t == "BT") {
                out.extend(s.split_whitespace().map(str::to_owned));
            }
        }
        at = start + len + "endstream".len();
    }
    out
}

const SPAN_STROKE_TOML: &str = r##"
[page]
width = 260
height = 60
unit = "pt"
root = "t"
[nodes.t]
type = "text"
font_size = 30
stroke = "#000000"
stroke_width = 2
spans = [
  { text = "Ab" },
  { text = "Cd", stroke = "#ff0000", stroke_width = 4 },
  { text = "Ef", stroke_width = 0 },
]
"##;

#[test]
fn span_strokes_override_the_node_outline_in_every_backend() {
    let fs = FontStore::new();
    let doc = Document::from_toml(SPAN_STROKE_TOML).unwrap();
    let t = text_props(&doc);
    let s = fs.shape(&t, f32::INFINITY);
    assert_eq!(s.strokes.len(), 2);
    assert_eq!(s.strokes[1].width, 4.0);
    let idx: Vec<Option<u16>> = s.lines[0].glyphs.iter().map(|g| g.stroke).collect();
    assert_eq!(idx, [Some(0), Some(0), Some(1), Some(1), None, None]);

    let computed = layout(&doc, &fs);
    let svg = render_svg(&computed, &opts(&fs));
    assert!(
        svg.contains("stroke=\"#000000\" stroke-width=\"2\""),
        "{svg}"
    );
    assert!(
        svg.contains("stroke=\"#ff0000\" stroke-width=\"4\""),
        "{svg}"
    );

    // PNG: red outline pixels only around "Cd".
    let pm = tiny_skia::Pixmap::decode_png(&render_png(&computed, &opts(&fs)).unwrap()).unwrap();
    let scale = pm.width() as f32 / computed.page.width_pt;
    let g = &s.lines[0].glyphs;
    let (x0, x1) = (
        (g[2].x - 3.0) * scale,
        (g[3].x + g[3].advance + 3.0) * scale,
    );
    let rect = computed.nodes[0].rect;
    let mut inside = 0;
    for y in 0..pm.height() {
        for x in 0..pm.width() {
            let c = pm.pixel(x, y).unwrap().demultiply();
            if c.red() > 200 && c.green() < 60 && c.alpha() > 200 {
                let fx = x as f32 - rect.x * scale;
                assert!(fx >= x0 && fx <= x1, "red at {x}");
                inside += 1;
            }
        }
    }
    assert!(inside > 30, "{inside}");

    #[cfg(feature = "pdf")]
    {
        let tokens = pdf_tokens(&figs_core::render_pdf(&computed, &opts(&fs)).unwrap());
        // One stroked path per outline style; the text is shown once (one
        // glyph run per span).
        assert_eq!(tokens.iter().filter(|x| *x == "S").count(), 2);
        assert_eq!(tokens.iter().filter(|x| x.ends_with("TJ")).count(), 3);
    }

    // Rhai span setters and markup build the same spans.
    let doc = Document::from_script(
        r##"page(10, 4, text([span("a").stroke("#ffffff").stroke_width(0.05), span("b")]))"##,
    )
    .unwrap();
    // Page unit (cm in scripts) -> points.
    let cm = 72.0 / 2.54;
    let st = &text_props(&doc).spans[0].style;
    assert!((st.stroke_width.unwrap() - 0.05 * cm).abs() < 1e-4);
    assert_eq!(st.stroke, figs_core::geom::Color::parse_hex("#ffffff").ok());
    let doc =
        Document::from_script(r##"page(10, 4, rich("{stroke=#ffffff stroke_width=0.1|x}y"))"##)
            .unwrap();
    let w = text_props(&doc).spans[0].style.stroke_width.unwrap();
    assert!((w - 0.1 * cm).abs() < 1e-4);
}

#[test]
fn synthetic_slant_is_configurable() {
    let fs = FontStore::new();
    if !installed(&fs, &["Yu Gothic"]) {
        return;
    }
    let src = |slant: &str| {
        format!(
            r##"
            [page]
            width = 120
            height = 60
            unit = "pt"
            root = "t"
            {slant}
            [nodes.t]
            type = "text"
            content = "K山"
            font_family = "Yu Gothic"
            font_style = "italic"
            font_size = 36
            "##
        )
    };
    let office =
        Document::from_toml(&src(&format!("synthetic_slant = {OFFICE_SLANT_DEG}"))).unwrap();
    let plain = Document::from_toml(&src("")).unwrap();
    let shaped = |d: &Document| fs.shape(&text_props(d), f32::INFINITY);
    let g = &shaped(&office).lines[0].glyphs[0];
    assert!(g.synthetic_italic);
    assert!((g.skew - 1.0 / 3.0).abs() < 1e-4, "{}", g.skew);
    let g = &shaped(&plain).lines[0].glyphs[0];
    assert!((g.skew - 14f32.to_radians().tan()).abs() < 1e-5);

    // SVG: the shear term of the glyph matrix is size/upem × 1/3.
    let svg = render_svg(&layout(&office, &fs), &opts(&fs));
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let m: Vec<f32> = xml
        .descendants()
        .find(|n| n.tag_name().name() == "use")
        .and_then(|n| n.attribute("transform"))
        .and_then(|t| t.strip_prefix("matrix("))
        .unwrap()
        .trim_end_matches(')')
        .split(' ')
        .map(|x| x.parse().unwrap())
        .collect();
    assert!((m[2] / m[0] - 1.0 / 3.0).abs() < 1e-3, "{m:?}");

    // PNG: the steeper slant moves ink (drawn from outlines, not swash).
    let a = render_png(&layout(&office, &fs), &opts(&fs)).unwrap();
    let b = render_png(&layout(&plain, &fs), &opts(&fs)).unwrap();
    assert_ne!(a, b);
    assert!(dark_pixels(&a) > 100);

    // Rhai and validation.
    let doc = Document::from_script(
        r#"page(4, 4, text("x").italic().synthetic_slant(18)).synthetic_slant(10)"#,
    )
    .unwrap();
    assert_eq!(text_props(&doc).typography.synthetic_slant, 18.0);
    assert!(Document::from_toml(&src("synthetic_slant = 90")).is_err());
}

#[test]
fn rich_markup_newline_escape_breaks_the_line() {
    let fs = FontStore::new();
    // A Rhai backtick string keeps `\n` as two characters; markup reads it as
    // a line break.
    let doc = Document::from_script("page(10, 4, rich(`first\\nsecond`))").unwrap();
    let t = text_props(&doc);
    assert_eq!(t.content, "first\nsecond");
    assert_eq!(fs.shape(&t, f32::INFINITY).lines.len(), 2);
    let e = Document::from_script("page(10, 4, rich(`a\\qb`))").unwrap_err();
    assert!(e.to_string().contains("unknown escape"), "{e}");
    assert!(e.to_string().contains("column 2"), "{e}");
}
