//! Optical centring and baseline pitch: the `"cap_middle"` anchor component
//! (half the first line's cap height above its first baseline), the rows'
//! `cross_axis_alignment = "cap_middle"`, and a column's `baseline_pitch`
//! (consecutive first baselines exactly that far apart), in TOML and script.
//!
//! Font-dependent checks run for every candidate family that is installed
//! and are skipped (with a note) when none is.

use figs_core::geom::{AnchorValue, BaselineKind, CrossAxisAlign};
use figs_core::layout::{ComputedLayout, ComputedNode, PaintContent};
use figs_core::render::geometry::text_props;
use figs_core::{
    eval_script, layout, render_png, Document, FontStore, NullMeasurer, RenderOptions,
};
use tiny_skia::Pixmap;

const FAMILIES: [&str; 8] = [
    "Liberation Sans",
    "Liberation Serif",
    "DejaVu Sans",
    "DejaVu Serif",
    "Arial",
    "Times New Roman",
    "Yu Gothic",
    "Segoe UI",
];

fn installed(fs: &FontStore) -> Vec<&'static str> {
    let have = fs.families();
    FAMILIES
        .iter()
        .copied()
        .filter(|f| have.iter().any(|h| h == f))
        .collect()
}

fn node<'a>(l: &'a ComputedLayout, id: &str) -> &'a ComputedNode {
    l.nodes.iter().find(|n| n.id == id).expect("node")
}

/// First baseline and first-line cap height of a laid-out text node.
fn first_line(fs: &FontStore, n: &ComputedNode) -> (f32, f32) {
    let PaintContent::Text(t) = &n.content else {
        panic!("not text")
    };
    let s = fs.shape(&text_props(t), n.rect.w);
    (s.lines[0].baseline, s.lines[0].cap_height)
}

/// A 300 x 200 pt stack page holding text `t` with `extra` keys.
fn page(extra: &str) -> String {
    format!(
        r##"
        [page]
        width = 300
        height = 200
        unit = "pt"
        dpi = 288
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["t"]
        [nodes.t]
        type = "text"
        {extra}
        "##
    )
}

/// The highest and lowest rows (points) with ink in columns `x0..x1`
/// (points) of a render at 4 px / pt.
fn ink_rows(p: &Pixmap, x0: f32, x1: f32) -> Option<(f32, f32)> {
    let (mut top, mut bottom) = (None, None);
    for y in 0..p.height() {
        for x in (x0 * 4.0) as u32..(x1 * 4.0) as u32 {
            let c = p.pixel(x, y).unwrap().demultiply();
            if (c.red() as i32 + c.green() as i32 + c.blue() as i32) / 3 < 128 {
                top.get_or_insert(y as f32 / 4.0);
                bottom = Some((y + 1) as f32 / 4.0);
            }
        }
    }
    Some((top?, bottom?))
}

#[test]
fn cap_middle_parses_and_serializes() {
    assert_eq!(
        BaselineKind::parse("cap_middle"),
        Some(BaselineKind::CapMiddle)
    );
    assert_eq!(BaselineKind::CapMiddle.name(), "cap_middle");
    let d = Document::from_toml(&page(
        r#"content = "H"
        x = 10
        y = 10
        anchor = [0.5, "cap_middle"]"#,
    ))
    .unwrap();
    let t = d.nodes.iter().find(|n| n.id == "t").unwrap();
    assert_eq!(
        t.common.anchor,
        Some([
            AnchorValue::Frac(0.5),
            AnchorValue::Baseline(BaselineKind::CapMiddle)
        ])
    );
    let raw = eval_script(r#"page(10, 10, row([text("a"), rect()]).cross("cap_middle"))"#).unwrap();
    let toml = toml::to_string(&raw).unwrap();
    assert!(
        toml.contains(r#"cross_axis_alignment = "cap_middle""#),
        "{toml}"
    );
    let back = Document::from_toml(&toml).unwrap();
    let r = back
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            figs_core::schema::NodeType::Container { cross_axis, .. } => Some(*cross_axis),
            _ => None,
        })
        .unwrap();
    assert_eq!(r, CrossAxisAlign::CapMiddle);
}

#[test]
fn shaped_lines_carry_the_fonts_cap_height() {
    let fs = FontStore::new();
    let fams = installed(&fs);
    if fams.is_empty() {
        eprintln!("none of {FAMILIES:?} installed; skipping");
        return;
    }
    for fam in fams {
        let src = page(&format!(
            r#"content = "Hxg"
            font_family = "{fam}"
            font_size = 20"#
        ));
        let l = layout(&Document::from_toml(&src).unwrap(), &fs);
        let (_, cap) = first_line(&fs, node(&l, "t"));
        assert!(
            cap > 0.55 * 20.0 && cap < 0.85 * 20.0,
            "{fam}: cap height {cap}"
        );
    }
}

#[test]
fn cap_middle_anchor_centres_the_capitals_on_the_offset() {
    // Independent of the font's numbers: render "H" and find its ink.
    let fs = FontStore::new();
    let fams = installed(&fs);
    if fams.is_empty() {
        eprintln!("none of {FAMILIES:?} installed; skipping");
        return;
    }
    for fam in fams {
        for size in [12.0f32, 40.0] {
            let src = page(&format!(
                r#"content = "H"
                font_family = "{fam}"
                font_size = {size}
                x = 40
                y = 100
                anchor = [0, "cap_middle"]"#
            ));
            let l = layout(&Document::from_toml(&src).unwrap(), &fs);
            let t = node(&l, "t");
            let (base, cap) = first_line(&fs, t);
            assert!(
                (t.rect.y + base - cap / 2.0 - 100.0).abs() < 1e-3,
                "{fam} {size}"
            );
            let png = render_png(
                &l,
                &RenderOptions {
                    fonts: Some(&fs),
                    base_dir: std::path::Path::new("."),
                },
            )
            .unwrap();
            let p = Pixmap::decode_png(&png).unwrap();
            let (top, bottom) = ink_rows(&p, t.rect.x, t.rect.x + t.rect.w).expect("ink");
            let mid = (top + bottom) / 2.0;
            assert!(
                (mid - 100.0).abs() <= 0.5 + size * 0.02,
                "{fam} {size}: H ink {top}..{bottom}, middle {mid}, want 100"
            );
        }
    }
}

#[test]
fn cap_middle_without_a_shaper_is_synthesized() {
    // 10 pt at line height 1.2: first baseline 1 + 8 = 9, cap middle 9 - 3.5.
    let src = page(
        r#"content = "a"
        font_size = 10
        x = 5
        y = 50
        anchor = [0, "cap_middle"]"#,
    );
    let l = layout(&Document::from_toml(&src).unwrap(), &NullMeasurer);
    assert!((node(&l, "t").rect.y - (50.0 - 5.5)).abs() < 1e-4);
}

#[test]
fn a_node_without_text_has_its_cap_middle_at_its_box_middle() {
    // A rect (any node kind) and a text-free column take "cap_middle".
    let src = r##"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["r", "c"]
        [nodes.r]
        type = "rect"
        width = 10
        height = 20
        x = 50
        y = 50
        anchor = [0.5, "cap_middle"]
        [nodes.c]
        type = "column"
        width = 6
        height = 8
        x = 10
        y = 10
        anchor = [0, "cap_middle"]
    "##;
    let l = layout(&Document::from_toml(src).unwrap(), &NullMeasurer);
    let r = node(&l, "r").rect;
    assert_eq!((r.x, r.y), (45.0, 40.0));
    assert_eq!(node(&l, "c").rect.y, 6.0);
    // "baseline" still needs text.
    let bad = src.replace(r#"[0.5, "cap_middle"]"#, r#"[0.5, "baseline"]"#);
    assert!(Document::from_toml(&bad).is_err());
}

#[test]
fn a_container_takes_its_first_texts_cap_middle() {
    let src = r##"
        [page]
        width = 200
        height = 100
        unit = "pt"
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["c"]
        [nodes.c]
        type = "column"
        padding = 7
        x = 20
        y = 50
        anchor = [0, "cap_middle"]
        children = ["box", "t", "u"]
        [nodes.box]
        type = "rect"
        width = 10
        height = 4
        [nodes.t]
        type = "text"
        content = "First"
        font_size = 10
        [nodes.u]
        type = "text"
        content = "Second"
        font_size = 20
    "##;
    let l = layout(&Document::from_toml(src).unwrap(), &NullMeasurer);
    let t = node(&l, "t").rect;
    // Synthetic cap middle of the 10 pt text: 9 - 3.5 below its top.
    assert!((t.y + 5.5 - 50.0).abs() < 1e-4, "{t:?}");
}

/// A cap-middle row: texts of different sizes/fonts (one with a top margin)
/// and a 10 x 30 rect, in a column so the row is as tall as it measures.
fn cap_row(fams: &[&str]) -> String {
    let sizes = [10.0, 30.0, 18.0];
    let mut nodes = String::new();
    let mut ids = Vec::new();
    for (i, size) in sizes.iter().enumerate() {
        let fam = fams[i % fams.len()];
        ids.push(format!("\"t{i}\""));
        nodes.push_str(&format!(
            "[nodes.t{i}]\ntype = \"text\"\ncontent = \"Hg {size}\"\nfont_family = \"{fam}\"\nfont_size = {size}\nmargin = {{ top = {m} }}\n",
            m = i
        ));
    }
    ids.push("\"sq\"".into());
    format!(
        r##"
        [page]
        width = 400
        height = 200
        unit = "pt"
        root = "c"
        [nodes.c]
        type = "column"
        padding = 10
        children = ["r"]
        [nodes.r]
        type = "row"
        spacing = 6
        cross_axis_alignment = "cap_middle"
        children = [{ids}]
        [nodes.sq]
        type = "rect"
        width = 10
        height = 60
        fill = "#000000"
        {nodes}
        "##,
        ids = ids.join(", ")
    )
}

#[test]
fn cap_middle_row_shares_one_optical_middle() {
    let fs = FontStore::new();
    let fams = installed(&fs);
    if fams.is_empty() {
        eprintln!("none of {FAMILIES:?} installed; skipping");
        return;
    }
    let l = layout(&Document::from_toml(&cap_row(&fams)).unwrap(), &fs);
    let row = node(&l, "r").rect;
    let mids: Vec<f32> = (0..3)
        .map(|i| {
            let t = node(&l, &format!("t{i}"));
            let (b, cap) = first_line(&fs, t);
            t.rect.y + b - cap / 2.0
        })
        .collect();
    for y in &mids[1..] {
        assert!((y - mids[0]).abs() < 1e-3, "cap middles {mids:?}");
    }
    // A box without text is centred on that line.
    let sq = node(&l, "sq").rect;
    assert!(
        (sq.y + sq.h / 2.0 - mids[0]).abs() < 1e-3,
        "{sq:?} vs {mids:?}"
    );
    // The tall rect sets the row's height: it spans the row exactly.
    assert!(
        (sq.y - row.y).abs() < 1e-3 && (sq.h - row.h).abs() < 1e-3,
        "{sq:?} {row:?}"
    );
}

/// A column of `children` (TOML node blocks) with `extra` keys.
fn pitch_page(extra: &str, children: &[&str], nodes: &str) -> String {
    let ids: Vec<String> = children.iter().map(|c| format!("\"{c}\"")).collect();
    format!(
        r##"
        [page]
        width = 300
        height = "auto"
        unit = "pt"
        root = "c"
        [nodes.c]
        type = "column"
        spacing = 3
        {extra}
        children = [{ids}]
        {nodes}
        "##,
        ids = ids.join(", ")
    )
}

const PITCH_NODES: &str = r##"
    [nodes.a]
    type = "text"
    content = "Small first item"
    font_size = 9
    [nodes.b]
    type = "text"
    content = "Much larger second item"
    font_size = 22
    margin = { top = 4 }
    [nodes.r]
    type = "rect"
    width = 20
    height = 7
    [nodes.d]
    type = "text"
    content = "After a rule"
    font_size = 12
"##;

#[test]
fn baseline_pitch_puts_first_baselines_exactly_apart() {
    let fs = FontStore::new();
    let have_font = !installed(&fs).is_empty();
    for pitch in [14.0f32, 30.0, 6.0] {
        let src = pitch_page(
            &format!("baseline_pitch = {pitch}"),
            &["a", "b", "r", "d"],
            PITCH_NODES,
        );
        let doc = Document::from_toml(&src).unwrap();
        for shaped in [false, true] {
            if shaped && !have_font {
                continue;
            }
            let l = if shaped {
                layout(&doc, &fs)
            } else {
                layout(&doc, &NullMeasurer)
            };
            let line = |id: &str| {
                let n = node(&l, id);
                let b = if shaped {
                    first_line(&fs, n).0
                } else {
                    let PaintContent::Text(t) = &n.content else {
                        panic!()
                    };
                    let size = text_props(t).font_size;
                    size * 0.1 + 0.8 * size
                };
                n.rect.y + b
            };
            let (a, b) = (line("a"), line("b"));
            assert!((b - a - pitch).abs() < 1e-3, "pitch {pitch}: {a} -> {b}");
            // The rect has no baseline: `spacing` either side of it.
            let r = node(&l, "r").rect;
            let bn = node(&l, "b").rect;
            assert!(
                (r.y - (bn.y + bn.h + 3.0)).abs() < 1e-3,
                "{r:?} after {bn:?}"
            );
            assert!((node(&l, "d").rect.y - (r.y + r.h + 3.0)).abs() < 1e-3);
            // The column wraps its children exactly (auto height).
            let c = node(&l, "c").rect;
            let d = node(&l, "d").rect;
            assert!((c.y + c.h - (d.y + d.h)).abs() < 1e-3, "{c:?} vs {d:?}");
        }
    }
}

#[test]
fn baseline_pitch_is_scaled_by_the_unit_and_checked() {
    let src = r##"
        [page]
        width = 10
        height = 10
        unit = "cm"
        root = "c"
        [nodes.c]
        type = "column"
        baseline_pitch = 1
        children = ["a", "b"]
        [nodes.a]
        type = "text"
        content = "a"
        [nodes.b]
        type = "text"
        content = "b"
    "##;
    let l = layout(&Document::from_toml(src).unwrap(), &NullMeasurer);
    let dy = node(&l, "b").rect.y - node(&l, "a").rect.y;
    assert!((dy - 72.0 / 2.54).abs() < 1e-3, "{dy}");
    let bad = src.replace(r#"type = "column""#, r#"type = "row""#);
    let e = Document::from_toml(&bad).unwrap_err().to_string();
    assert!(e.contains("baseline_pitch"), "{e}");
}

#[test]
fn script_baseline_pitch_and_cap_middle() {
    let raw = eval_script(
        r#"page(10, 10, stack([
            col([text("a"), text("b")]).baseline_pitch(0.8),
            text("c").at(1, 1).anchor(0, "cap_middle"),
        ]))"#,
    )
    .unwrap();
    let toml = toml::to_string(&raw).unwrap();
    assert!(toml.contains("baseline_pitch = 0.8"), "{toml}");
    assert!(toml.contains(r#"anchor = [0.0, "cap_middle"]"#), "{toml}");
}
