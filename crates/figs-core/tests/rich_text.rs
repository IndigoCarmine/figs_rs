//! Rich text: inline spans (sizes, scripts, colours, decorations, tracking),
//! ordered font-family lists, synthetic italic, and Japanese line breaking —
//! shaping/layout properties plus PNG / PDF / SVG output.
//!
//! Tests that need particular fonts (Arial, Yu Gothic, a CJK face) check the
//! font database first and return early when they are missing, so the suite
//! stays green on machines without them.

use std::path::Path;

use figs_core::geom::Color;
use figs_core::schema::{FontStyle, Script, SpanStyle, TextProps, TextSpan};
use figs_core::text::{DecorationKind, ShapedText};
use figs_core::{layout, render_png, render_svg, Document, FontStore, RenderOptions};

fn store() -> FontStore {
    FontStore::new().with_default_family("Liberation Sans, Arial")
}

fn installed(fs: &FontStore, family: &str) -> bool {
    fs.families().iter().any(|f| f.eq_ignore_ascii_case(family))
}

/// The family name of the face a glyph was shaped with.
fn face_family(fs: &FontStore, g: &figs_core::text::ShapedGlyph) -> String {
    fs.with_system(|s| {
        s.db()
            .face(g.font_id)
            .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
            .unwrap_or_default()
    })
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

fn plain() -> SpanStyle {
    SpanStyle::default()
}

fn glyph_char<'a>(t: &'a TextProps, g: &figs_core::text::ShapedGlyph) -> &'a str {
    &t.content[g.text_range.clone()]
}

// ---------------------------------------------------------------------------
// Line metrics, scripts, wrapping
// ---------------------------------------------------------------------------

#[test]
fn larger_span_grows_its_line_only() {
    let fs = store();
    let base = fs.shape(&TextProps::plain("Hello world", 10.0), f32::INFINITY);
    let big = spans(
        &[
            ("Hello ", plain()),
            (
                "world",
                SpanStyle {
                    font_size: Some(20.0),
                    ..plain()
                },
            ),
        ],
        10.0,
    );
    let shaped = fs.shape(&big, f32::INFINITY);
    // The line box follows the largest run: 20 pt × 1.2.
    assert!(
        shaped.size.h > base.size.h * 1.8,
        "{} vs {}",
        shaped.size.h,
        base.size.h
    );
    assert!(
        (shaped.size.h - 24.0).abs() < 20.0 * 0.25,
        "{}",
        shaped.size.h
    );
    // Both runs share one baseline.
    assert_eq!(shaped.lines.len(), 1);
    let sizes: Vec<f32> = shaped.lines[0].glyphs.iter().map(|g| g.font_size).collect();
    assert!(sizes.contains(&10.0) && sizes.contains(&20.0), "{sizes:?}");
}

#[test]
fn scripts_are_scaled_and_shifted_without_changing_line_spacing() {
    let fs = store();
    let mk = |script| {
        let mut t = spans(
            &[
                ("ΔG", plain()),
                ("300K", SpanStyle { script, ..plain() }),
                (" = −27.5", plain()),
            ],
            20.0,
        );
        t.line_height = 1.5;
        t
    };
    let flat = fs.shape(&mk(None), f32::INFINITY);
    for (script, sign) in [(Script::Sub, -1.0), (Script::Super, 1.0)] {
        let t = mk(Some(script));
        let shaped = fs.shape(&t, f32::INFINITY);
        // Same block height and baseline as without the script.
        assert!((shaped.size.h - flat.size.h).abs() < 0.01, "{script:?}");
        assert!((shaped.lines[0].baseline - flat.lines[0].baseline).abs() < 0.01);
        for g in &shaped.lines[0].glyphs {
            let ch = glyph_char(&t, g);
            if (3..7).contains(&g.text_range.start) {
                assert!((g.font_size - 20.0 * figs_core::schema::span::SCRIPT_SCALE).abs() < 1e-4);
                let shift = script.shift() * 20.0;
                assert!((g.y_offset - shift).abs() < 1e-3, "{ch}: {}", g.y_offset);
                assert_eq!(g.y_offset.signum(), sign);
            } else {
                assert_eq!(g.font_size, 20.0);
                assert!(g.y_offset.abs() < 1e-3);
            }
        }
    }
}

#[test]
fn explicit_baseline_shift_overrides_script() {
    let fs = store();
    let t = spans(
        &[
            ("x", plain()),
            (
                "2",
                SpanStyle {
                    script: Some(Script::Super),
                    baseline_shift: Some(0.5),
                    font_size: Some(10.0),
                    ..plain()
                },
            ),
        ],
        12.0,
    );
    let shaped = fs.shape(&t, f32::INFINITY);
    let two = shaped.lines[0].glyphs.last().unwrap();
    // Base = the span's font_size (10): glyph 6.5 pt, raised 5 pt.
    assert!((two.font_size - 6.5).abs() < 1e-4);
    assert!((two.y_offset - 5.0).abs() < 1e-3);
}

#[test]
fn wrapping_crosses_span_boundaries() {
    let fs = store();
    let text = "the quick brown fox jumps over the lazy dog";
    let plain_t = TextProps::plain(text, 16.0);
    // Same text cut into coloured spans at arbitrary points (mid-word too).
    let red = SpanStyle {
        color: Some(Color::parse_hex("#ff0000").unwrap()),
        ..plain()
    };
    let cut = spans(
        &[
            ("the qu", plain()),
            ("ick brown f", red.clone()),
            ("ox jumps over the l", plain()),
            ("azy dog", red),
        ],
        16.0,
    );
    for w in [60.0, 90.0, 140.0] {
        let a = fs.shape(&plain_t, w);
        let b = fs.shape(&cut, w);
        assert_eq!(a.lines.len(), b.lines.len(), "width {w}");
        for (la, lb) in a.lines.iter().zip(&b.lines) {
            assert!((la.width - lb.width).abs() < 0.5, "width {w}");
        }
    }
    // Colour follows the glyph's source text.
    let b = fs.shape(&cut, f32::INFINITY);
    for g in b.lines.iter().flat_map(|l| &l.glyphs) {
        let red = (6..17).contains(&g.text_range.start) || g.text_range.start >= 36;
        assert_eq!(g.color.r > 0.5, red, "glyph at {:?}", g.text_range);
    }
}

#[test]
fn letter_spacing_widens_and_wraps_earlier() {
    let fs = store();
    let t = TextProps::plain("SPACED", 20.0);
    let tracked = TextProps {
        letter_spacing: 0.25,
        ..t.clone()
    };
    let a = fs.shape(&t, f32::INFINITY);
    let b = fs.shape(&tracked, f32::INFINITY);
    // Five gaps of 0.25 em (the trailing tracking is not part of the width).
    let expected = a.lines[0].width + 5.0 * 0.25 * 20.0;
    assert!(
        (b.lines[0].width - expected).abs() < 0.05,
        "{} vs {expected}",
        b.lines[0].width
    );
    // Glyph pitch grows by exactly the tracking.
    let (ga, gb) = (&a.lines[0].glyphs, &b.lines[0].glyphs);
    for i in 1..ga.len() {
        let d = (gb[i].x - gb[i - 1].x) - (ga[i].x - ga[i - 1].x);
        assert!((d - 5.0).abs() < 1e-3, "pitch delta {d}");
    }
    // Wrapping sees the tracking: a width that fits the plain text breaks
    // the tracked text.
    let words = TextProps::plain("ab cd ef gh", 20.0);
    let w = fs.shape(&words, f32::INFINITY).size.w + 1.0;
    let words_tracked = TextProps {
        letter_spacing: 0.3,
        ..words.clone()
    };
    assert_eq!(fs.shape(&words, w).lines.len(), 1);
    assert!(fs.shape(&words_tracked, w).lines.len() > 1);
    // Per-span tracking applies to that span only.
    let mixed = spans(
        &[
            ("AB", plain()),
            (
                "CD",
                SpanStyle {
                    letter_spacing: Some(0.5),
                    ..plain()
                },
            ),
        ],
        20.0,
    );
    let plain_w = fs
        .shape(&TextProps::plain("ABCD", 20.0), f32::INFINITY)
        .size
        .w;
    let m = fs.shape(&mixed, f32::INFINITY);
    // One 10 pt gap after C; D's is trailing.
    assert!(
        (m.size.w - plain_w - 10.0).abs() < 0.05,
        "{} vs {plain_w}",
        m.size.w
    );
}

#[test]
fn tracked_text_fits_its_own_natural_width() {
    // The natural width leaves the last character's trailing tracking out;
    // shaping at that width must not wrap the last word because of it.
    let fs = store();
    for (content, ls) in [("Take-home message", 0.1), ("ab cd", 0.3), ("SPACED", 0.05)] {
        let t = TextProps {
            letter_spacing: ls,
            ..TextProps::plain(content, 20.0)
        };
        let natural = fs.shape(&t, f32::INFINITY);
        assert_eq!(natural.lines.len(), 1);
        let at = fs.shape(&t, natural.size.w);
        assert_eq!(
            at.lines.len(),
            1,
            "{content:?} wrapped at its natural width"
        );
        assert!((at.size.w - natural.size.w).abs() < 1e-3);
        // One tracking step narrower still breaks (wrapping sees the
        // tracking between the words).
        if content.contains(' ') {
            let tight = fs.shape(&t, natural.size.w - ls * 20.0 - 0.5);
            assert!(tight.lines.len() > 1, "{content:?}");
        }
    }
    // Through layout: a tracked text at its natural width in a row is drawn
    // (shaped at its laid-out width, as the backends do) on one line.
    let src = r#"
        [page]
        width = 400
        height = 200
        unit = "pt"
        root = "r"

        [nodes.r]
        type = "row"
        children = ["t"]

        [nodes.t]
        type = "text"
        content = "Take-home message here"
        font_size = 20
        letter_spacing = 0.12
    "#;
    let computed = layout(&Document::from_toml(src).unwrap(), &fs);
    let t = computed.nodes.iter().find(|n| n.id == "t").unwrap();
    let figs_core::layout::PaintContent::Text(tb) = &t.content else {
        panic!("text")
    };
    let drawn = fs.shape(&figs_core::render::geometry::text_props(tb), t.rect.w);
    assert_eq!(drawn.lines.len(), 1, "drawn text wrapped at {}", t.rect.w);
}

#[test]
fn long_latin_word_breaks_inside_a_narrow_box() {
    let fs = store();
    let t = TextProps::plain("Pneumonoultramicroscopic", 14.0);
    let shaped = fs.shape(&t, 50.0);
    assert!(shaped.lines.len() >= 3);
    for l in &shaped.lines {
        assert!(l.width <= 50.0 + 0.5, "{}", l.width);
    }
}

// ---------------------------------------------------------------------------
// Decorations
// ---------------------------------------------------------------------------

fn decorations(s: &ShapedText, kind: DecorationKind) -> Vec<&figs_core::text::Decoration> {
    s.decorations.iter().filter(|d| d.kind == kind).collect()
}

#[test]
fn underline_and_strike_follow_their_spans() {
    let fs = store();
    let t = spans(
        &[
            ("Y. ", plain()),
            (
                "Sato",
                SpanStyle {
                    underline: Some(true),
                    ..plain()
                },
            ),
            (" and ", plain()),
            (
                "old",
                SpanStyle {
                    strike: Some(true),
                    color: Some(Color::parse_hex("#0000ff").unwrap()),
                    ..plain()
                },
            ),
        ],
        20.0,
    );
    let s = fs.shape(&t, f32::INFINITY);
    let line = &s.lines[0];
    let u = decorations(&s, DecorationKind::Underline);
    let st = decorations(&s, DecorationKind::Strike);
    assert_eq!((u.len(), st.len()), (1, 1));
    // Underline spans exactly "Sato", just below the baseline.
    let sato: Vec<_> = line
        .glyphs
        .iter()
        .filter(|g| (3..7).contains(&g.text_range.start))
        .collect();
    let x0 = sato[0].x;
    let x1 = sato.last().map(|g| g.x + g.advance).unwrap();
    assert!((u[0].x - x0).abs() < 1e-3 && (u[0].x + u[0].width - x1).abs() < 1e-3);
    assert!(u[0].y > line.baseline && u[0].y < line.baseline + 0.3 * 20.0);
    assert!(u[0].thickness > 0.2 && u[0].thickness < 0.15 * 20.0);
    // Line-through above the baseline, in the span's colour.
    assert!(st[0].y < line.baseline - 0.1 * 20.0);
    assert_eq!(st[0].color, Color::parse_hex("#0000ff").unwrap());
    // Node-level underline covers everything.
    let all = TextProps {
        underline: true,
        ..TextProps::plain("all of it", 20.0)
    };
    let s = fs.shape(&all, f32::INFINITY);
    let u = decorations(&s, DecorationKind::Underline);
    assert_eq!(u.len(), 1);
    assert!((u[0].width - s.lines[0].width).abs() < 1e-3);
}

// ---------------------------------------------------------------------------
// Fonts: ordered lists, italic
// ---------------------------------------------------------------------------

#[test]
fn family_list_is_authoritative_per_character() {
    let fs = FontStore::new();
    if !(installed(&fs, "Arial") && installed(&fs, "Yu Gothic")) {
        eprintln!("skipping: Arial / Yu Gothic not installed");
        return;
    }
    let mut t = TextProps::plain("Sato 佐藤 (10 μM)。", 12.0);
    t.font_family = Some("Arial, Yu Gothic".into());
    let s = fs.shape(&t, f32::INFINITY);
    for g in &s.lines[0].glyphs {
        let ch = glyph_char(&t, g).chars().next().unwrap();
        let fam = face_family(&fs, g);
        let want = if ch == '佐' || ch == '藤' || ch == '。' {
            "Yu Gothic"
        } else {
            "Arial"
        };
        assert_eq!(fam, want, "{ch:?}");
    }
    // Reversed: Yu Gothic covers Latin too, so it takes everything.
    t.font_family = Some("Yu Gothic, Arial".into());
    let s = fs.shape(&t, f32::INFINITY);
    for g in &s.lines[0].glyphs {
        assert_eq!(face_family(&fs, g), "Yu Gothic");
    }
    // A span's family list replaces the node's.
    let mut sp = spans(
        &[
            ("A", plain()),
            (
                "B山",
                SpanStyle {
                    font_family: Some("Times New Roman, MS Mincho".into()),
                    ..plain()
                },
            ),
        ],
        12.0,
    );
    sp.font_family = Some("Arial, Yu Gothic".into());
    if installed(&fs, "Times New Roman") && installed(&fs, "MS Mincho") {
        let s = fs.shape(&sp, f32::INFINITY);
        let fams: Vec<String> = s.lines[0]
            .glyphs
            .iter()
            .map(|g| face_family(&fs, g))
            .collect();
        assert_eq!(fams, ["Arial", "Times New Roman", "MS Mincho"]);
    }
}

#[test]
fn page_default_family_list_applies() {
    let fs = FontStore::new().with_default_family("Arial, Yu Gothic");
    if !(installed(&fs, "Arial") && installed(&fs, "Yu Gothic")) {
        return;
    }
    let s = fs.shape(&TextProps::plain("a山", 12.0), f32::INFINITY);
    let fams: Vec<String> = s.lines[0]
        .glyphs
        .iter()
        .map(|g| face_family(&fs, g))
        .collect();
    assert_eq!(fams, ["Arial", "Yu Gothic"]);
}

#[test]
fn italic_uses_a_real_face_or_synthesizes() {
    let fs = FontStore::new();
    if !(installed(&fs, "Arial") && installed(&fs, "Yu Gothic")) {
        return;
    }
    let mut t = TextProps::plain("Ai山", 12.0);
    t.font_family = Some("Arial, Yu Gothic".into());
    t.font_style = FontStyle::Italic;
    let s = fs.shape(&t, f32::INFINITY);
    let g = &s.lines[0].glyphs;
    let style = |g: &figs_core::text::ShapedGlyph| {
        fs.with_system(|s| s.db().face(g.font_id).map(|f| format!("{:?}", f.style)))
    };
    // Arial has a real italic face; Yu Gothic has none -> slanted upright.
    assert_eq!(style(&g[0]).as_deref(), Some("Italic"));
    assert!(!g[0].synthetic_italic && !g[1].synthetic_italic);
    assert!(g[2].synthetic_italic);
    assert_eq!(face_family(&fs, &g[2]), "Yu Gothic");
    // Upright text is never slanted.
    t.font_style = FontStyle::Normal;
    let s = fs.shape(&t, f32::INFINITY);
    assert!(s.lines[0].glyphs.iter().all(|g| !g.synthetic_italic));
}

#[test]
fn weights_match_the_nearest_face() {
    let fs = FontStore::new();
    if !installed(&fs, "Arial") {
        return;
    }
    let mut t = TextProps::plain("A", 12.0);
    t.font_family = Some("Arial".into());
    let weight = |w: u16| {
        let mut t = t.clone();
        t.font_weight = Some(w);
        let s = fs.shape(&t, f32::INFINITY);
        let id = s.lines[0].glyphs[0].font_id;
        fs.with_system(|s| s.db().face(id).map(|f| f.weight.0))
    };
    assert_eq!(weight(400), Some(400));
    assert_eq!(weight(600), Some(700)); // CSS: > 500 looks bolder first
    assert!(weight(900) >= Some(700)); // 900 when "Arial Black" is installed
    assert_eq!(weight(300), Some(400));
    t.font_weight = None;
}

// ---------------------------------------------------------------------------
// Japanese line breaking (UAX #14 via cosmic-text)
// ---------------------------------------------------------------------------

/// Lines of `t` shaped at `width`, as strings.
fn line_texts(fs: &FontStore, t: &TextProps, width: f32) -> Vec<String> {
    fs.shape(t, width)
        .lines
        .iter()
        .map(|l| {
            let (Some(a), Some(b)) = (l.glyphs.first(), l.glyphs.last()) else {
                return String::new();
            };
            t.content[a.text_range.start..b.text_range.end].to_string()
        })
        .collect()
}

#[test]
fn japanese_kinsoku_at_every_width() {
    let fs = FontStore::new();
    let has_cjk = ["Yu Gothic", "MS Gothic", "IPAGothic", "Noto Sans CJK JP"]
        .iter()
        .find(|f| installed(&fs, f));
    let Some(family) = has_cjk else {
        eprintln!("skipping: no CJK font");
        return;
    };
    let mut t = TextProps::plain(
        "結晶成長（板状体）が観察され、「ドメイン」となる。さらに成長して大きな板となった。\
         Sodium chloride結晶は、（溶媒）中で「溶解」する。\
         ショック、ゃゅょっァィ。ーーー（Pneumonoultramicroscopic）。",
        10.0,
    );
    t.font_family = Some(format!("Arial, {family}"));
    let no_start = "。、）」』，．！？：；";
    let no_end = "（「『";
    for w in (35..=200).step_by(3) {
        let lines = line_texts(&fs, &t, w as f32);
        assert!(lines.len() > 1 || w > 150);
        for (i, l) in lines.iter().enumerate() {
            let first = l.chars().next().unwrap();
            let last = l.trim_end().chars().last().unwrap();
            assert!(
                i == 0 || !no_start.contains(first),
                "width {w}: line {i} starts with {first:?}: {lines:?}"
            );
            assert!(
                i + 1 == lines.len() || !no_end.contains(last),
                "width {w}: line {i} ends with {last:?}: {lines:?}"
            );
        }
        // Nothing is lost or duplicated by wrapping.
        let joined: String = lines.concat();
        let squeezed: String = t.content.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(
            joined
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
            squeezed,
            "width {w}"
        );
    }
}

// ---------------------------------------------------------------------------
// Backends
// ---------------------------------------------------------------------------

const RICH_TOML: &str = r##"
[page]
width = 300
height = 60
unit = "pt"
root = "t"
background = "#ffffff"
font_family = "Liberation Sans, Arial"

[nodes.t]
type = "text"
font_size = 30
spans = [
  { text = "ΔG" },
  { text = "300K", script = "sub" },
  { text = " = " },
  { text = "−27.5", color = "#ff0000" },
  { text = " kJ", underline = true },
]
"##;

fn render_rich(src: &str) -> (tiny_skia::Pixmap, figs_core::ComputedLayout) {
    let fs = store();
    let doc = Document::from_toml(src).expect("resolve");
    let computed = layout(&doc, &fs);
    let opts = RenderOptions {
        fonts: Some(&fs),
        base_dir: Path::new("."),
    };
    let png = render_png(&computed, &opts).expect("png");
    (tiny_skia::Pixmap::decode_png(&png).unwrap(), computed)
}

#[test]
fn png_paints_span_colours_and_underline() {
    let (pm, computed) = render_rich(RICH_TOML);
    let scale = pm.width() as f32 / computed.page.width_pt;
    let red = |p: tiny_skia::PremultipliedColorU8| {
        let c = p.demultiply();
        c.alpha() > 200 && c.red() > 200 && c.green() < 80 && c.blue() < 80
    };
    let dark = |p: tiny_skia::PremultipliedColorU8| {
        let c = p.demultiply();
        c.red() < 80 && c.green() < 80 && c.blue() < 80
    };
    // Red pixels exist, and only in the "−27.5" part (right of centre-left).
    let fs = store();
    let shaped = fs.shape(
        &match &Document::from_toml(RICH_TOML).unwrap().nodes[0].kind {
            figs_core::schema::NodeType::Text(t) => t.clone(),
            _ => unreachable!(),
        },
        computed.nodes[0].rect.w,
    );
    let line = &shaped.lines[0];
    let red_glyphs: Vec<_> = line.glyphs.iter().filter(|g| g.color.r > 0.5).collect();
    let rx0 = (red_glyphs[0].x * scale) as u32;
    let rx1 = ((red_glyphs.last().unwrap().x + red_glyphs.last().unwrap().advance) * scale) as u32;
    let mut reds_inside = 0;
    let mut reds_outside = 0;
    for y in 0..pm.height() {
        for x in 0..pm.width() {
            if red(pm.pixel(x, y).unwrap()) {
                if x + 1 >= rx0 && x <= rx1 + 1 {
                    reds_inside += 1;
                } else {
                    reds_outside += 1;
                }
            }
        }
    }
    assert!(reds_inside > 50, "{reds_inside}");
    assert_eq!(reds_outside, 0);
    // The underline: a dark horizontal bar under " kJ".
    let u = shaped
        .decorations
        .iter()
        .find(|d| d.kind == DecorationKind::Underline)
        .expect("underline");
    let y = ((computed.nodes[0].rect.y + u.y) * scale) as u32;
    let (ux0, ux1) = (
        (u.x * scale) as u32 + 2,
        ((u.x + u.width) * scale) as u32 - 2,
    );
    let covered = (ux0..ux1)
        .filter(|&x| dark(pm.pixel(x, y).unwrap()))
        .count();
    assert!(
        covered as f32 > 0.95 * (ux1 - ux0) as f32,
        "{covered} of {}",
        ux1 - ux0
    );
}

#[test]
fn svg_groups_colours_under_one_label_and_draws_underline() {
    let fs = store();
    let doc = Document::from_toml(RICH_TOML).unwrap();
    let computed = layout(&doc, &fs);
    let svg = render_svg(
        &computed,
        &RenderOptions {
            fonts: Some(&fs),
            base_dir: Path::new("."),
        },
    );
    let xml = roxmltree::Document::parse(&svg).expect("well-formed");
    let labelled: Vec<_> = xml
        .descendants()
        .filter(|n| n.attribute("aria-label").is_some())
        .collect();
    assert_eq!(labelled.len(), 1);
    assert_eq!(
        labelled[0].attribute("aria-label"),
        Some("ΔG300K = −27.5 kJ")
    );
    let fills: Vec<&str> = labelled[0]
        .children()
        .filter(|n| n.is_element())
        .filter_map(|n| n.attribute("fill"))
        .collect();
    assert_eq!(fills, ["#000000", "#ff0000", "#000000"]);
    // Subscript glyphs are scaled: some <use> has a smaller scale factor.
    let scales: Vec<f32> = xml
        .descendants()
        .filter(|n| n.tag_name().name() == "use")
        .filter_map(|n| {
            let t = n.attribute("transform")?;
            t.strip_prefix("matrix(")?.split(' ').next()?.parse().ok()
        })
        .collect();
    let (min, max) = scales
        .iter()
        .fold((f32::MAX, 0.0f32), |(a, b), &s| (a.min(s), b.max(s)));
    assert!((min / max - 0.65).abs() < 0.01, "{min} / {max}");
    // The underline is a filled rect after the page background.
    let rects = xml
        .descendants()
        .filter(|n| n.tag_name().name() == "rect")
        .count();
    assert_eq!(rects, 2);
}

#[cfg(feature = "pdf")]
#[test]
fn pdf_text_extracts_once_with_scripts_inline() {
    use std::process::Command;
    let fs = store();
    let doc = Document::from_toml(RICH_TOML).unwrap();
    let computed = layout(&doc, &fs);
    let pdf = figs_core::render_pdf(
        &computed,
        &RenderOptions {
            fonts: Some(&fs),
            base_dir: Path::new("."),
        },
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("figs-rich-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rich.pdf");
    std::fs::write(&path, &pdf).unwrap();
    let out = match Command::new("pdftotext")
        .args(["-enc", "UTF-8"])
        .arg(&path)
        .arg("-")
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => {
            eprintln!("skipping: pdftotext not available");
            return;
        }
    };
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(text.matches("ΔG300K = −27.5 kJ").count(), 1, "{text:?}");
    assert_eq!(text.matches("−27.5").count(), 1);
}

// ---------------------------------------------------------------------------
// Schema: resolve, round-trip
// ---------------------------------------------------------------------------

#[test]
fn spans_resolve_after_content_and_round_trip() {
    let src = r##"
[page]
width = 10
height = 10
root = "t"
font_family = ["Arial", "Yu Gothic"]

[nodes.t]
type = "text"
content = "x"
font_family = "Times New Roman, MS Mincho"
font_style = "italic"
underline = true
letter_spacing = 0.1
spans = [
  { text = "ΔG" },
  { text = "300K", script = "sub", color = "#ff0000" },
  { text = "†", script = "sup", baseline_shift = 0.4, style = "normal", font_family = ["Arial"] },
]
"##;
    let raw = figs_core::schema::parse::parse_str(src).unwrap();
    assert_eq!(raw.page.font_family.as_deref(), Some("Arial, Yu Gothic"));
    let again = figs_core::schema::parse::parse_str(&toml::to_string(&raw).unwrap()).unwrap();
    assert_eq!(raw.nodes["t"].spans, again.nodes["t"].spans);
    assert_eq!(again.nodes["t"].font_style, Some(FontStyle::Italic));
    assert!(again.nodes["t"].underline);

    let doc = Document::from_toml(src).unwrap();
    let figs_core::schema::NodeType::Text(t) = &doc.nodes[doc.root].kind else {
        panic!("text")
    };
    assert_eq!(t.content, "xΔG300K†");
    let ranges: Vec<_> = t.spans.iter().map(|s| s.range.clone()).collect();
    assert_eq!(ranges, [1..4, 4..8, 8..11]);
    assert_eq!(t.spans[1].style.script, Some(Script::Sub));
    assert_eq!(t.letter_spacing, 0.1);

    // A text node needs `content` or `spans`.
    let bad = src
        .replace("content = \"x\"\n", "")
        .replace("spans = [", "zzz = [");
    assert!(Document::from_toml(&bad).is_err());
    let only_spans = src.replace("content = \"x\"\n", "");
    assert!(Document::from_toml(&only_spans).is_ok());
    let neg = src.replace("script = \"sub\",", "font_size = -1,");
    let e = Document::from_toml(&neg).unwrap_err().to_string();
    assert!(e.contains("font_size must be positive"), "{e}");
}

// ---------------------------------------------------------------------------
// Scripts: core span API and markup sugar
// ---------------------------------------------------------------------------

#[cfg(feature = "script")]
mod script {
    use super::*;
    use figs_core::eval_script;

    fn text_node(src: &str) -> figs_core::schema::RawNode {
        let raw = eval_script(src).unwrap_or_else(|e| panic!("{e}"));
        raw.nodes["root"].clone()
    }

    #[test]
    fn core_span_api() {
        let n = text_node(
            r##"page(10, 10, text([
                span("ΔG"), span("300K").sub(), " = ",
                [span("−27.5").color("#ff0000").bold().size(14).underline()],
                span("x").italic().strike().tracking(0.1).font(["Arial", "Yu Gothic"]).shift(0.2),
                span("y").sup().weight(600).upright().font("Times New Roman"),
            ]).italic().underline().tracking(0.05).font(["Arial", "Yu Gothic"]))"##,
        );
        assert_eq!(n.content, None);
        let t: Vec<&str> = n.spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(t, ["ΔG", "300K", " = ", "−27.5", "x", "y"]);
        assert_eq!(n.spans[1].script, Some(Script::Sub));
        assert_eq!(n.spans[3].font_weight, Some(700));
        assert_eq!(n.spans[3].font_size, Some(14.0));
        assert_eq!(n.spans[3].underline, Some(true));
        assert_eq!(n.spans[3].color, Color::parse_hex("#ff0000").ok());
        assert_eq!(n.spans[4].font_style, Some(FontStyle::Italic));
        assert_eq!(n.spans[4].strike, Some(true));
        assert_eq!(n.spans[4].letter_spacing, Some(0.1));
        assert_eq!(n.spans[4].baseline_shift, Some(0.2));
        assert_eq!(n.spans[4].font_family.as_deref(), Some("Arial, Yu Gothic"));
        assert_eq!(n.spans[5].script, Some(Script::Super));
        assert_eq!(n.spans[5].font_weight, Some(600));
        assert_eq!(n.spans[5].font_style, Some(FontStyle::Normal));
        assert_eq!(n.font_style, Some(FontStyle::Italic));
        assert!(n.underline);
        assert_eq!(n.letter_spacing, Some(0.05));
        assert_eq!(n.font_family.as_deref(), Some("Arial, Yu Gothic"));
    }

    #[test]
    fn markup_sugar_builds_the_same_spans() {
        let a = text_node(r##"page(10, 10, rich("ΔG_{300K} = {#ff0000|−27.5}"))"##);
        let b = text_node(
            r##"page(10, 10, text([span("ΔG"), span("300K").sub(), span(" = "), span("−27.5").color("#ff0000")]))"##,
        );
        assert_eq!(a.spans, b.spans);
        // Framework helpers take span arrays too.
        let c = text_node(r##"page(10, 10, body(markup("H_2O")))"##);
        assert_eq!(c.spans.len(), 3);
        assert!(c.font_size.is_some());
    }

    #[test]
    fn markup_errors_name_the_column() {
        let e = eval_script(r##"page(10, 10, rich("ok {#zz|bad}"))"##)
            .expect_err("error")
            .to_string();
        assert!(
            e.contains("invalid colour") && e.contains("column 4"),
            "{e}"
        );
        let e = eval_script(r##"page(10, 10, text([1]))"##)
            .expect_err("error")
            .to_string();
        assert!(e.contains("spans and strings"), "{e}");
    }

    #[test]
    fn rich_example_lays_out() {
        let src = include_str!("../../../examples/rich_text.figs");
        let doc = Document::from_script(src).unwrap_or_else(|e| panic!("{e}"));
        let fs = store();
        let l = layout(&doc, &fs);
        assert!(l.page.height_pt > 100.0);
    }
}
