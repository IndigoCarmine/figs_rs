//! Baselines in layout: `anchor = [ax, "baseline"]` / `"last_baseline"` in
//! stacks (any font, size, spans, rotation), `cross_axis_alignment =
//! "baseline"` in rows, the measurer-less fallback, and the script side
//! (`.at_baseline`, `.anchor(.., "baseline")`, `.cross("baseline")`).
//!
//! Font-dependent checks run for every candidate family that is installed
//! and are skipped (with a note) when none is.

use figs_core::geom::{AnchorValue, BaselineKind, Point, Rect};
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

/// The candidate families installed here (may be empty).
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

/// First and last baseline of a laid-out text node, from its box top, shaped
/// exactly as the backends shape it.
fn baselines(fs: &FontStore, n: &ComputedNode) -> (f32, f32) {
    let PaintContent::Text(t) = &n.content else {
        panic!("not text")
    };
    let s = fs.shape(&text_props(t), n.rect.w);
    (s.lines[0].baseline, s.lines.last().unwrap().baseline)
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

/// The lowest row (in points) with ink in columns `x0..x1` (points) of a
/// render at 4 px / pt.
fn ink_bottom(p: &Pixmap, x0: f32, x1: f32) -> Option<f32> {
    let mut low = None;
    for y in 0..p.height() {
        for x in (x0 * 4.0) as u32..(x1 * 4.0) as u32 {
            let c = p.pixel(x, y).unwrap().demultiply();
            if (c.red() as i32 + c.green() as i32 + c.blue() as i32) / 3 < 128 {
                low = Some((y + 1) as f32 / 4.0);
            }
        }
    }
    low
}

#[test]
fn baseline_anchor_puts_the_first_baseline_on_the_offset() {
    let fs = FontStore::new();
    let fams = installed(&fs);
    if fams.is_empty() {
        eprintln!("none of {FAMILIES:?} installed; skipping");
        return;
    }
    for fam in fams {
        for size in [8.0f32, 17.5, 40.0] {
            let src = page(&format!(
                r#"content = "Hxg"
                font_family = "{fam}"
                font_size = {size}
                x = 20
                y = 100
                anchor = [0, "baseline"]"#
            ));
            let l = layout(&Document::from_toml(&src).unwrap(), &fs);
            let t = node(&l, "t");
            let (first, _) = baselines(&fs, t);
            assert!(
                (t.rect.y + first - 100.0).abs() < 1e-3,
                "{fam} {size}: baseline at {}",
                t.rect.y + first
            );
            assert_eq!(t.rect.x, 20.0);
            // A real baseline: somewhere in the first line's box.
            assert!(
                first > 0.5 * size && first < 1.5 * size,
                "{fam} {size}: {first}"
            );
        }
    }
}

#[test]
fn the_ink_of_a_capital_sits_on_the_offset() {
    // Independent of the shaper's numbers: render and find where "H" ends.
    let fs = FontStore::new();
    let fams = installed(&fs);
    if fams.is_empty() {
        eprintln!("none of {FAMILIES:?} installed; skipping");
        return;
    }
    for fam in fams {
        // A plain run, and the same with a larger span on the line.
        for spans in ["", r#"spans = [{ text = "H", font_size = 60 }]"#] {
            let src = page(&format!(
                r#"content = "H"
                font_family = "{fam}"
                font_size = 30
                x = 20
                y = 120
                anchor = [0, "baseline"]
                {spans}"#
            ));
            let l = layout(&Document::from_toml(&src).unwrap(), &fs);
            let png = render_png(
                &l,
                &RenderOptions {
                    fonts: Some(&fs),
                    base_dir: std::path::Path::new("."),
                },
            )
            .unwrap();
            let p = Pixmap::decode_png(&png).unwrap();
            let t = node(&l, "t");
            let bottom = ink_bottom(&p, t.rect.x, t.rect.x + t.rect.w).expect("ink");
            assert!(
                (bottom - 120.0).abs() <= 0.5,
                "{fam} spans={spans:?}: H ends at {bottom} pt, want 120"
            );
        }
    }
}

#[test]
fn last_baseline_anchor() {
    let fs = FontStore::new();
    let Some(fam) = installed(&fs).first().copied() else {
        eprintln!("no candidate family installed; skipping");
        return;
    };
    let src = page(&format!(
        r#"content = "First line\nsecond, larger"
        font_family = "{fam}"
        font_size = 14
        line_height = 1.5
        x = 150
        y = 180
        anchor = [0.5, "last_baseline"]"#
    ));
    let l = layout(&Document::from_toml(&src).unwrap(), &fs);
    let t = node(&l, "t");
    let (first, last) = baselines(&fs, t);
    assert!((t.rect.y + last - 180.0).abs() < 1e-3);
    assert!(
        (last - first - 21.0).abs() < 0.5,
        "two lines 1.5 x 14 apart"
    );
    assert!((t.rect.x + t.rect.w / 2.0 - 150.0).abs() < 1e-3);
}

/// Where the pen origin of the first line (left end of its baseline, in the
/// text's own frame) lands on the page.
fn pen_origin(fs: &FontStore, t: &ComputedNode) -> Point {
    let (first, _) = baselines(fs, t);
    let p = Point::new(t.rect.x, t.rect.y + first);
    t.transform.map_or(p, |xf| xf.apply(p))
}

#[test]
fn rotated_text_anchors_its_baseline_in_the_parents_frame() {
    let fs = FontStore::new();
    let Some(fam) = installed(&fs).first().copied() else {
        eprintln!("no candidate family installed; skipping");
        return;
    };
    // (rotate, anchor) that put the pen origin at the offset.
    for (rot, anchor) in [
        (0, r#"[0, "baseline"]"#),
        (-90, r#"["baseline", 1]"#),
        (270, r#"["baseline", 1]"#),
        (90, r#"["baseline", 0]"#),
        (180, r#"[1, "baseline"]"#),
    ] {
        let src = page(&format!(
            r#"content = "Label"
            font_family = "{fam}"
            font_size = 20
            rotate = {rot}
            x = 120
            y = 90
            anchor = {anchor}"#
        ));
        let l = layout(&Document::from_toml(&src).unwrap(), &fs);
        let o = pen_origin(&fs, node(&l, "t"));
        assert!(
            (o.x - 120.0).abs() < 1e-3 && (o.y - 90.0).abs() < 1e-3,
            "rotate {rot}: pen origin at {o:?}"
        );
    }
}

#[test]
fn script_at_baseline_places_the_pen_origin_for_any_quarter_turn() {
    let fs = FontStore::new();
    let Some(fam) = installed(&fs).first().copied() else {
        eprintln!("no candidate family installed; skipping");
        return;
    };
    for rot in [0, 90, 180, -90] {
        // `.rotate` after `.at_baseline`: the anchor follows the final angle.
        let src = format!(
            r#"page(300, 200, stack([
                text("Label").font("{fam}").size(20).at_baseline(120, 90).rotate({rot}),
            ])).unit("pt")"#
        );
        let doc = Document::from_script(&src).unwrap();
        let l = layout(&doc, &fs);
        let t = l
            .nodes
            .iter()
            .find(|n| matches!(n.content, PaintContent::Text(_)))
            .unwrap();
        let o = pen_origin(&fs, t);
        assert!(
            (o.x - 120.0).abs() < 1e-3 && (o.y - 90.0).abs() < 1e-3,
            "rotate {rot}: pen origin at {o:?}"
        );
    }
}

#[test]
fn baseline_slot_and_node_kind_are_checked() {
    let err = |extra: &str| Document::from_toml(&page(extra)).unwrap_err().to_string();
    let e = err(r#"content = "a"
        anchor = ["baseline", 0]"#);
    assert!(e.contains("y slot"), "{e}");
    let e = err(r#"content = "a"
        rotate = -90
        anchor = [0, "baseline"]"#);
    assert!(e.contains("x slot"), "{e}");
    let e = err(r#"content = "a"
        anchor = ["baseline", "last_baseline"]"#);
    assert!(e.contains("one slot"), "{e}");
    let e = Document::from_toml(&page(
        r#"content = "a"
        anchor = [0, "middle"]"#,
    ))
    .unwrap_err();
    assert!(format!("{e:?}").contains("last_baseline"), "{e:?}");
    let rect = r#"
        [page]
        width = 10
        height = 10
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["r"]
        [nodes.r]
        type = "rect"
        anchor = [0, "baseline"]
    "#;
    let e = Document::from_toml(rect).unwrap_err().to_string();
    assert!(e.contains("only a text node has a baseline"), "{e}");
}

#[test]
fn without_a_shaper_baselines_are_synthesized() {
    // NullMeasurer: 10 pt text, line height 1.2: half-leading 1 + 0.8 em.
    let src = page(
        r#"content = "a"
        font_size = 10
        x = 5
        y = 50
        anchor = [0, "baseline"]"#,
    );
    let l = layout(&Document::from_toml(&src).unwrap(), &NullMeasurer);
    assert_eq!(node(&l, "t").rect.y, 41.0);
}

/// A row of texts of different fonts/sizes and a 10 pt square, baseline-
/// aligned, in a column (so the row is as tall as it measures).
fn baseline_row(fams: &[&str]) -> String {
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
        cross_axis_alignment = "baseline"
        children = [{ids}]
        [nodes.sq]
        type = "rect"
        width = 10
        height = 10
        fill = "#000000"
        {nodes}
        "##,
        ids = ids.join(", ")
    )
}

#[test]
fn baseline_row_shares_one_baseline() {
    let fs = FontStore::new();
    let fams = installed(&fs);
    if fams.is_empty() {
        eprintln!("none of {FAMILIES:?} installed; skipping");
        return;
    }
    let l = layout(&Document::from_toml(&baseline_row(&fams)).unwrap(), &fs);
    let row = node(&l, "r");
    let lines: Vec<f32> = (0..3)
        .map(|i| {
            let t = node(&l, &format!("t{i}"));
            t.rect.y + baselines(&fs, t).0
        })
        .collect();
    for y in &lines[1..] {
        assert!((y - lines[0]).abs() < 1e-3, "baselines {lines:?}");
    }
    // A box without a baseline sits on it with its bottom edge.
    let sq = node(&l, "sq").rect;
    assert!((sq.y + sq.h - lines[0]).abs() < 1e-3, "{sq:?} vs {lines:?}");
    // The child reaching furthest above the baseline touches the row top,
    // and the row is exactly as tall as above + below.
    let tops: Vec<f32> = (0..3)
        .map(|i| node(&l, &format!("t{i}")).rect.y - i as f32)
        .chain([sq.y])
        .collect();
    let top = tops.iter().copied().fold(f32::INFINITY, f32::min);
    assert!((top - row.rect.y).abs() < 1e-3, "{top} vs {:?}", row.rect);
    let bottom = (0..3)
        .map(|i| {
            let r = node(&l, &format!("t{i}")).rect;
            r.y + r.h
        })
        .chain([sq.y + sq.h])
        .fold(0.0f32, f32::max);
    assert!(
        (row.rect.y + row.rect.h - bottom).abs() < 1e-3,
        "row {:?}, lowest child bottom {bottom}",
        row.rect
    );
}

#[test]
fn baseline_in_a_column_is_start() {
    let src = r##"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "c"
        [nodes.c]
        type = "column"
        cross_axis_alignment = "baseline"
        children = ["a"]
        [nodes.a]
        type = "rect"
        width = 10
        height = 10
    "##;
    let l = layout(&Document::from_toml(src).unwrap(), &NullMeasurer);
    assert_eq!(node(&l, "a").rect, Rect::new(0.0, 0.0, 10.0, 10.0));
}

#[test]
fn script_baseline_setters_round_trip() {
    let raw = eval_script(
        r#"page(10, 10, stack([
            text("a").at_baseline(1, 2),
            text("b").at(3, 4).anchor(0.5, "last_baseline"),
            row([text("c"), text("d").size(30)]).cross("baseline"),
        ]))"#,
    )
    .unwrap();
    let toml = toml::to_string(&raw).unwrap();
    assert!(toml.contains(r#"anchor = [0.0, "baseline"]"#), "{toml}");
    assert!(
        toml.contains(r#"anchor = [0.5, "last_baseline"]"#),
        "{toml}"
    );
    assert!(
        toml.contains(r#"cross_axis_alignment = "baseline""#),
        "{toml}"
    );
    let back = figs_core::schema::parse::parse_str(&toml).unwrap();
    let a = back
        .nodes
        .values()
        .find_map(|n| n.anchor.filter(|a| a[0] == AnchorValue::Frac(0.5)))
        .unwrap();
    assert_eq!(a[1], AnchorValue::Baseline(BaselineKind::Last));
    // A later `.anchor` replaces what `.at_baseline` set.
    let raw =
        eval_script(r#"page(10, 10, stack([text("a").at_baseline(1, 2).anchor(1, 1)]))"#).unwrap();
    let n = raw.nodes.values().find(|n| n.anchor.is_some()).unwrap();
    assert_eq!(n.anchor, Some([1.0.into(), 1.0.into()]));
    for bad in [
        r#"page(10, 10, stack([rect().at_baseline(1, 2)]))"#,
        r#"page(10, 10, stack([text("a").anchor(0, "middle")]))"#,
    ] {
        assert!(eval_script(bad).is_err(), "{bad}");
    }
}
