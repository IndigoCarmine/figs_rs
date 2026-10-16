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
    assert!(e.contains("only a text node or a column/row/stack"), "{e}");
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

// ---- container baselines ----

/// The first / last baseline of laid-out text node `id`, on the page.
fn page_baselines(fs: &FontStore, l: &ComputedLayout, id: &str) -> (f32, f32) {
    let t = node(l, id);
    let (a, b) = baselines(fs, t);
    (t.rect.y + a, t.rect.y + b)
}

/// A 300 x 300 pt stack holding container `c` (with `extra` keys) of: a
/// 12 pt rect (no baseline), text `a` (two lines) and text `b`.
fn container_page(fam: &str, kind: &str, extra: &str) -> String {
    format!(
        r##"
        [page]
        width = 300
        height = 300
        unit = "pt"
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["c"]
        [nodes.c]
        type = "{kind}"
        children = ["r", "a", "b"]
        {extra}
        [nodes.r]
        type = "rect"
        width = 12
        height = 12
        [nodes.a]
        type = "text"
        content = "Alpha\nbeta"
        font_family = "{fam}"
        font_size = 14
        [nodes.b]
        type = "text"
        content = "Gamma"
        font_family = "{fam}"
        font_size = 24
        "##
    )
}

#[test]
fn a_column_has_the_baselines_of_its_first_and_last_text() {
    let fs = FontStore::new();
    let Some(fam) = installed(&fs).first().copied() else {
        eprintln!("no candidate family installed; skipping");
        return;
    };
    // The rect before the texts has no baseline: `a` gives the first.
    let first = container_page(
        fam,
        "column",
        r#"padding = 7
        spacing = 3
        x = 40
        y = 100
        anchor = [0, "baseline"]"#,
    );
    let l = layout(&Document::from_toml(&first).unwrap(), &fs);
    assert!((page_baselines(&fs, &l, "a").0 - 100.0).abs() < 1e-3);
    assert!((node(&l, "c").rect.x - 40.0).abs() < 1e-3);
    // The last baseline is the last text's (`b`), centred columns too.
    let last = container_page(
        fam,
        "column",
        r#"spacing = 3
        main_axis_alignment = "center"
        height = 200
        x = 40
        y = 250
        anchor = [0, "last_baseline"]"#,
    );
    let l = layout(&Document::from_toml(&last).unwrap(), &fs);
    assert!((page_baselines(&fs, &l, "b").1 - 250.0).abs() < 1e-3);
}

#[test]
fn a_row_and_a_stack_have_the_baseline_of_their_first_text() {
    let fs = FontStore::new();
    let Some(fam) = installed(&fs).first().copied() else {
        eprintln!("no candidate family installed; skipping");
        return;
    };
    for (kind, extra) in [
        ("row", r#"cross_axis_alignment = "center""#),
        ("row", r#"cross_axis_alignment = "baseline""#),
        ("stack", ""),
    ] {
        let src = container_page(
            fam,
            kind,
            &format!(
                r#"{extra}
                padding = 5
                x = 20
                y = 150
                anchor = [0, "baseline"]"#
            ),
        );
        let l = layout(&Document::from_toml(&src).unwrap(), &fs);
        let a = page_baselines(&fs, &l, "a").0;
        assert!((a - 150.0).abs() < 1e-3, "{kind} {extra}: {a}");
        if extra.contains("baseline") {
            // A baseline row: every text on that line.
            assert!((page_baselines(&fs, &l, "b").0 - 150.0).abs() < 1e-3);
        }
    }
}

#[test]
fn containers_take_part_in_baseline_rows() {
    // A row of a label and a column of two lines (a list item): the
    // column's first line shares the label's baseline.
    let fs = FontStore::new();
    let Some(fam) = installed(&fs).first().copied() else {
        eprintln!("no candidate family installed; skipping");
        return;
    };
    let src = format!(
        r##"
        [page]
        width = 300
        height = 200
        unit = "pt"
        root = "row"
        [nodes.row]
        type = "row"
        spacing = 4
        cross_axis_alignment = "baseline"
        children = ["mark", "item"]
        [nodes.mark]
        type = "text"
        content = "•"
        font_family = "{fam}"
        font_size = 30
        [nodes.item]
        type = "column"
        padding = {{ top = 6 }}
        children = ["l1", "l2"]
        [nodes.l1]
        type = "text"
        content = "first line"
        font_family = "{fam}"
        font_size = 12
        [nodes.l2]
        type = "text"
        content = "second line"
        font_family = "{fam}"
        font_size = 12
        "##
    );
    let l = layout(&Document::from_toml(&src).unwrap(), &fs);
    let mark = page_baselines(&fs, &l, "mark").0;
    let line = page_baselines(&fs, &l, "l1").0;
    assert!((mark - line).abs() < 1e-3, "{mark} vs {line}");
}

#[test]
fn a_quarter_turned_container_anchors_its_baseline_on_x() {
    let fs = FontStore::new();
    let Some(fam) = installed(&fs).first().copied() else {
        eprintln!("no candidate family installed; skipping");
        return;
    };
    let src = container_page(
        fam,
        "column",
        r#"rotate = -90
        x = 60
        y = 200
        anchor = ["baseline", 1]"#,
    );
    let l = layout(&Document::from_toml(&src).unwrap(), &fs);
    // Reading upwards: the first baseline of `a` (in the column's frame) is
    // the vertical line x = 60 on the page.
    let a = node(&l, "a");
    let (first, _) = baselines(&fs, a);
    let p = a
        .transform
        .unwrap()
        .apply(Point::new(a.rect.x, a.rect.y + first));
    assert!((p.x - 60.0).abs() < 1e-3, "{p:?}");
    let c = node(&l, "c").bounds();
    assert!((c.y + c.h - 200.0).abs() < 1e-3, "{c:?}");
}

#[test]
fn a_container_without_text_sits_on_its_bottom_edge() {
    let src = r##"
        [page]
        width = 100
        height = 100
        unit = "pt"
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["c"]
        [nodes.c]
        type = "column"
        x = 10
        y = 50
        anchor = [0, "baseline"]
        children = ["r"]
        [nodes.r]
        type = "rect"
        width = 20
        height = 30
    "##;
    let l = layout(&Document::from_toml(src).unwrap(), &NullMeasurer);
    assert_eq!(node(&l, "c").rect, Rect::new(10.0, 20.0, 20.0, 30.0));
}

#[test]
fn script_at_baseline_places_a_container() {
    let fs = FontStore::new();
    let Some(fam) = installed(&fs).first().copied() else {
        eprintln!("no candidate family installed; skipping");
        return;
    };
    let src = format!(
        r#"page(300, 200, stack([
            col([text("one").font("{fam}").size(18), text("two").font("{fam}").size(18)])
                .pad(4).at_baseline(30, 120),
        ])).unit("pt")"#
    );
    let l = layout(&Document::from_script(&src).unwrap(), &fs);
    let t = l
        .nodes
        .iter()
        .find(|n| matches!(&n.content, PaintContent::Text(t) if t.content == "one"))
        .unwrap();
    let (first, _) = baselines(&fs, t);
    assert!((t.rect.y + first - 120.0).abs() < 1e-3);
    assert!((t.rect.x - 34.0).abs() < 1e-3, "pad 4 from x = 30");
}
