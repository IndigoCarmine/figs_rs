//! A coordinate-free poster rewrite, second round:
//! - content-sized ("auto") cells: a row measuring a child without `flex`
//!   sizes it to its content — rows with flex items inside (`labeled(..,
//!   outside-left)`, tables, "fill" scheme arrows) no longer claim the
//!   whole width;
//! - an outer `step` captions its own node, not an axis inside it;
//! - `cycle_arrows` beside an item shrink the item to stay in the box;
//! - `overflow = "fit"` counts a rigid item's flex share;
//! - `overhang = "auto"`, `align_self`, `min_width` / `min_height`,
//!   `dpi = "file"`, hull connectors; `styled`, `plot_aspect: "fill"`,
//!   `section(.., #{ fit: "flex" })` and the small hooks.

#![cfg(feature = "script")]

use figs_core::geom::{Rect, Size};
use figs_core::layout::{ComputedLayout, LeafMeasure, PaintContent, PathPaint};
use figs_core::schema::{ImageProps, RawDocument, RawNode, TextProps};
use figs_core::{eval_script, layout, Document};

/// Text 10 pt per character wide, 10 pt per line (wrapping at whole
/// characters); every image is 300 x 150 pixels and records 150 dpi when
/// its file name contains "dense".
struct Pixels;

impl LeafMeasure for Pixels {
    fn measure_text(&self, t: &TextProps, max_width: f32) -> Size {
        let n = t.content.chars().count() as f32;
        let w = 10.0 * n;
        if w <= max_width + 1e-3 {
            Size::new(w, 10.0)
        } else {
            let per = (max_width / 10.0).floor().max(1.0);
            Size::new(per * 10.0, 10.0 * (n / per).ceil())
        }
    }
    fn measure_image(&self, _i: &ImageProps) -> Size {
        Size::default()
    }
    fn image_aspect(&self, _i: &ImageProps) -> Option<f32> {
        Some(2.0)
    }
    fn image_pixels(&self, _i: &ImageProps) -> Option<(u32, u32)> {
        Some((300, 150))
    }
    fn image_density(&self, i: &ImageProps) -> Option<f32> {
        i.src.contains("dense").then_some(150.0)
    }
}

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &Pixels)
}

fn raw(src: &str) -> RawDocument {
    eval_script(src).unwrap_or_else(|e| panic!("script failed: {e}"))
}

fn fail(src: &str) -> String {
    match eval_script(src) {
        Ok(_) => panic!("expected an error from {src}"),
        Err(e) => e.to_string(),
    }
}

fn rect_of(l: &ComputedLayout, id: &str) -> Rect {
    l.nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("no node {id}"))
        .rect
}

/// The box of the text node reading `s`.
fn text_rect(l: &ComputedLayout, s: &str) -> Rect {
    l.nodes
        .iter()
        .find(|n| matches!(&n.content, PaintContent::Text(t) if t.content == s))
        .unwrap_or_else(|| panic!("no text {s}"))
        .rect
}

fn texts<'a>(d: &'a RawDocument, s: &str) -> Vec<&'a RawNode> {
    d.nodes
        .values()
        .filter(|n| n.content.as_deref() == Some(s))
        .collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

fn paths(l: &ComputedLayout) -> Vec<&PathPaint> {
    l.nodes
        .iter()
        .filter_map(|n| match &n.content {
            PaintContent::Path(p) => Some(p),
            _ => None,
        })
        .collect()
}

#[test]
fn outside_left_labels_side_by_side_take_their_own_width() {
    let l = lay(r#"
        fn label_outside_gap() { 0 }
        page(400, "auto", row([
            labeled(image("a.png").dpi(300).id("a"), "a", #{ position: "outside-left" }),
            labeled(image("b.png").dpi(300).id("b"), "b", #{ position: "outside-left" }),
        ])).unit("pt")"#);
    assert!(l.overflows.is_empty(), "{:?}", l.overflows);
    // 300 x 150 px at 300 dpi = 72 x 36 pt, each after a 10 pt label.
    let (a, b) = (rect_of(&l, "a"), rect_of(&l, "b"));
    assert!(near(a.x, 10.0) && near(a.w, 72.0), "{a:?}");
    assert!(near(b.x, 92.0) && near(b.w, 72.0), "{b:?}");
}

#[test]
fn an_outer_step_captions_its_own_node() {
    let l = lay(r#"
        page(400, "auto", col([
            step(row([rect().width(40).height(20),
                      step(rect().width(20).height(20), "in")]).id("r"), "outer")
        ]).cross("start")).unit("pt")"#);
    assert!(l.overflows.is_empty(), "{:?}", l.overflows);
    let r = rect_of(&l, "r");
    let c = text_rect(&l, "outer");
    assert!(near(c.x + c.w / 2.0, r.x + r.w / 2.0), "{c:?} under {r:?}");
}

#[test]
fn auto_cells_hold_tables_and_fill_schemes_at_their_own_width() {
    // A table in an "auto" block cell: as wide as its text, not the row.
    let l = lay(r#"
        fn table_pad() { 0 }
        fn table_rule_width() { 0 }
        page(400, "auto", block("", [table([["ab", "c"], ["d", "e"]]).id("t"),
                                      rect().height(5).id("x")],
                                 #{ widths: ["auto", 1], gap: 0 })).unit("pt")"#);
    let (t, x) = (rect_of(&l, "t"), rect_of(&l, "x"));
    assert!(t.w < 60.0, "{t:?}");
    assert!(near(x.w, 400.0 - t.w), "{x:?}");
    // A "fill" scheme in an "auto" cell: its arrows at their least length.
    let l = lay(r#"
        fn scheme_fill_min_length() { 12 }
        page(400, "auto", row([
            scheme([rect().width(10).height(10), arrow_step(), rect().width(10).height(10)],
                   #{ arrow_length: "fill", gap: 0 }).id("s"),
            rect().height(5).flex(1).id("x")])).unit("pt")"#);
    let (s, x) = (rect_of(&l, "s"), rect_of(&l, "x"));
    assert!(near(s.w, 32.0), "{s:?}");
    assert!(near(x.w, 368.0), "{x:?}");
    // A filling row elsewhere still takes the width it may (centred column).
    let l = lay(r#"
        page(100, "auto", col([
            scheme([rect().width(10).height(10), arrow_step(#{ length: "fill" }),
                    rect().width(10).height(10)], #{ gap: 0 }).id("s")
        ]).cross("center")).unit("pt")"#);
    assert!(near(rect_of(&l, "s").w, 100.0));
}

#[test]
fn cycle_arrows_beside_an_item_stay_in_its_box() {
    let l = lay(r#"
        fn cycle_arrow_depth() { 10 }
        page(100, 100, row([
            cycle_arrows(image("a.png").id("i"), #{ sides: "left-right" }).width(50).id("c"),
            rect().flex(1)])).unit("pt")"#);
    assert!(l.overflows.is_empty(), "{:?}", l.overflows);
    let (i, c) = (rect_of(&l, "i"), rect_of(&l, "c"));
    // 50 wide, less two 10 pt arcs.
    assert!(near(i.w, 30.0) && near(i.x, c.x + 10.0), "{i:?} in {c:?}");
}

#[test]
fn fit_scales_a_rigid_item_in_a_flex_cell() {
    // A 300 pt rect in one of two equal cells of a 200 pt section: the row
    // needs 600 pt for the rect to keep its width, so all is scaled 1/3.
    let l = lay(r#"
        page(200, 400, col([
            row([col([rect().width(300).height(30).id("big")]).flex(1), rect().flex(1).height(10)])
        ]).stretch().overflow("fit")).unit("pt")"#);
    let big = &l.nodes.iter().find(|n| n.id == "big").unwrap();
    let s = big.transform.map_or(1.0, |t| t.a);
    assert!(near(big.rect.w, 300.0), "{:?}", big.rect);
    assert!(near(s, 1.0 / 3.0), "{s}");
    // An image sized by its height and its file's aspect is rigid too.
    let l = lay(r#"
        page(100, 400, col([row([image("a.png").height(100).id("i")])]).stretch().overflow("fit"))
            .unit("pt")"#);
    let i = l.nodes.iter().find(|n| n.id == "i").unwrap();
    assert!(near(i.rect.w, 200.0), "{:?}", i.rect);
    assert!(near(i.transform.map_or(1.0, |t| t.a), 0.5));
}

#[test]
fn auto_overhang_takes_only_free_room() {
    // The caption below the first structure: the arrow beside it is drawn
    // at the structures' middle, so the caption reaches over it (one line,
    // kept inside the row: it can't reach left of the row).
    let src = |below: &str| {
        format!(
            r#"fn caption_overhang() {{ "auto" }}
            page(600, "auto", col([scheme([
                step(rect().width(20).height(20).id("m"), "longcaption"),
                arrow_step(#{{ length: 100, above: "t"{below} }}),
                rect().width(20).height(20)]).id("s")]).cross("start")).unit("pt")"#
        )
    };
    let l = lay(&src(""));
    assert!(l.overflows.is_empty(), "{:?}", l.overflows);
    let (c, m, s) = (
        text_rect(&l, "longcaption"),
        rect_of(&l, "m"),
        rect_of(&l, "s"),
    );
    assert!(near(c.h, 10.0) && near(c.w, 110.0), "one line: {c:?}");
    assert!(c.x >= s.x - 0.01 && near(c.x, m.x), "{c:?} {m:?}");
    // The arrow with a label under it: the caption stops short of the
    // label and wraps instead.
    let l = lay(&src(r#", below: "uuuuuu""#));
    let (c, u) = (text_rect(&l, "longcaption"), text_rect(&l, "uuuuuu"));
    assert!(c.h > 10.5, "wraps: {c:?}");
    assert!(c.x + c.w <= u.x + 0.01, "{c:?} clear of {u:?}");
    // Two captions reaching into one gap split it.
    let l = lay(r#"fn caption_overhang() { "auto" }
        page(600, "auto", col([scheme([
            step(rect().width(20).height(20), "aaaaaaaaaa"),
            arrow_step(#{ length: 40 }),
            step(rect().width(20).height(20), "bbbbbbbbbb")])]).cross("start")).unit("pt")"#);
    let (a, b) = (text_rect(&l, "aaaaaaaaaa"), text_rect(&l, "bbbbbbbbbb"));
    assert!(a.x + a.w <= b.x + 0.01, "{a:?} {b:?}");
    // `.overhang("auto")` / errors.
    let d = raw(r#"page(10, 10, col([text("x").overhang("auto")]))"#);
    assert_eq!(texts(&d, "x")[0].overhang, figs_core::geom::Overhang::Auto);
    assert!(fail(r#"page(10, 10, text("x").overhang("wide"))"#).contains("\"auto\""));
}

#[test]
fn align_self_places_one_child() {
    let l = lay(r#"
        page(400, "auto", col([
            image("a.png").dpi(300).align("center").id("c"),
            rect().width(50).height(5).align_self("end").id("e"),
            image("b.png").dpi(300).align_self("stretch").id("s"),
            text("t").align("center").id("t"),
        ]).stretch()).unit("pt")"#);
    let c = rect_of(&l, "c");
    assert!(near(c.x, (400.0 - 72.0) / 2.0) && near(c.w, 72.0), "{c:?}");
    assert!(near(rect_of(&l, "e").x, 350.0));
    assert!(near(rect_of(&l, "s").w, 400.0));
    // On a text "center" stays the text's own alignment.
    let d = raw(r#"page(10, 10, col([text("t").align("center")]))"#);
    assert!(texts(&d, "t")[0].align_self.is_none());
    assert!(
        fail(r#"page(10, 10, col([rect().align_self("baseline")]))"#).contains("start, center")
    );
}

#[test]
fn minimum_sizes_hold_against_flex_shares() {
    let l = lay(r#"
        page(100, 100, col([rect().height(90), rect().flex(1).min_height(30).id("f")]).stretch())
            .unit("pt")"#);
    assert!(near(rect_of(&l, "f").h, 30.0));
    let l = lay(
        r#"page(100, "auto", col([rect().min_height(12).min_width(7).id("r")]).cross("start")).unit("pt")"#,
    );
    let r = rect_of(&l, "r");
    assert!(near(r.h, 12.0) && near(r.w, 7.0), "{r:?}");
}

#[test]
fn dpi_from_the_file() {
    // The file records 150 dpi: 300 x 150 px = 144 x 72 pt.
    let l = lay(
        r#"page(400, "auto", col([image("dense.png").dpi("file").id("i")]).stretch()).unit("pt")"#,
    );
    let i = rect_of(&l, "i");
    assert!(near(i.w, 144.0) && near(i.h, 72.0), "{i:?}");
    // The page's "file", and a file without one: no natural size (fills).
    let l = lay(r#"
        page(400, "auto", col([image("dense.png").id("a"), image("plain.png").id("b")]).stretch())
            .unit("pt").image_dpi("file")"#);
    assert!(near(rect_of(&l, "a").w, 144.0));
    assert!(near(rect_of(&l, "b").w, 400.0));
    // A node's "file" falls back to the page's number.
    let l = lay(r#"
        page(400, "auto", col([image("plain.png").dpi("file").id("b")]).stretch()).unit("pt")
            .image_dpi(300)"#);
    assert!(near(rect_of(&l, "b").w, 72.0));
    assert!(fail(r#"page(10, 10, image("a.png").dpi("exif"))"#).contains("\"file\""));
}

#[test]
fn file_density_is_read_from_png_and_jpeg_headers() {
    let dir = std::env::temp_dir().join(format!("figs_density_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let chunk = |kind: &[u8], data: &[u8]| {
        let mut v = (data.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(data);
        v.extend_from_slice(&[0, 0, 0, 0]); // crc (not checked)
        v
    };
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]));
    let mut phys = 11811u32.to_be_bytes().to_vec(); // 300 dpi in px / m
    phys.extend(11811u32.to_be_bytes());
    phys.push(1);
    png.extend(chunk(b"pHYs", &phys));
    png.extend(chunk(b"IEND", &[]));
    let p = dir.join("a.png");
    std::fs::write(&p, &png).unwrap();
    let d = figs_core::render::image_density(&p).unwrap();
    assert!((d - 300.0).abs() < 0.1, "{d}");
    // JFIF: units 1 (dpi), 144 x 144.
    let jpg = [
        0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1, 1, 0, 144, 0, 144, 0, 0,
    ];
    let p = dir.join("a.jpg");
    std::fs::write(&p, jpg).unwrap();
    assert_eq!(figs_core::render::image_density(&p), Some(144.0));
    // No pHYs: none.
    let mut bare = png[..8 + 25].to_vec();
    bare.extend(chunk(b"IEND", &[]));
    let p = dir.join("b.png");
    std::fs::write(&p, &bare).unwrap();
    assert_eq!(figs_core::render::image_density(&p), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hull_links_join_diagonal_boxes_by_their_outer_tangents() {
    let src = |opts: &str| {
        format!(
            r#"let a = rect().width(10).height(10).at(0, 0);
            let b = rect().width(10).height(10).at(50, 50);
            page(100, 100, stack([a, b, connect(a, b, #{{ {opts} }})])).unit("pt")"#
        )
    };
    let ends = |opts: &str| {
        let l = lay(&src(opts));
        let p = paths(&l)[0].segments.clone();
        p
    };
    use figs_core::geom::{PathSegment, Point};
    let pt = |s: &PathSegment| match s {
        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => Some(*p),
        _ => None,
    };
    let s1: Vec<Point> = ends(r#"from: "hull-1", to: "hull-1""#)
        .iter()
        .filter_map(pt)
        .collect();
    let s2: Vec<Point> = ends(r#"from: "hull-2", to: "hull-2""#)
        .iter()
        .filter_map(pt)
        .collect();
    let mut got = vec![
        (s1[0].x, s1[0].y, s1[1].x, s1[1].y),
        (s2[0].x, s2[0].y, s2[1].x, s2[1].y),
    ];
    got.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    // a's bottom-left to b's bottom-left, a's top-right to b's top-right.
    let want = [(0.0, 10.0, 50.0, 60.0), (10.0, 0.0, 60.0, 50.0)];
    for (g, w) in got.iter().zip(want) {
        assert!(
            near(g.0, w.0) && near(g.1, w.1) && near(g.2, w.2) && near(g.3, w.3),
            "{got:?}"
        );
    }
    // The hull cone: the area between them, both near corners included.
    let cone: Vec<Point> =
        ends(r##"route: "cone", from: "hull-1", to: "hull-1", fill: "#ff0000""##)
            .iter()
            .filter_map(pt)
            .collect();
    assert_eq!(cone.len(), 6, "{cone:?}");
    assert!(cone.iter().any(|p| near(p.x, 10.0) && near(p.y, 10.0)));
    assert!(cone.iter().any(|p| near(p.x, 50.0) && near(p.y, 50.0)));
    // zoom_link's attach option.
    let d = raw(r#"let a = rect(); let b = rect();
        page(10, 10, stack([a, b, zoom_link(a, b, #{ attach: "hull" })]))"#);
    let hulls = d
        .nodes
        .values()
        .filter(|n| format!("{:?}", n.from_at).contains("hull"))
        .count();
    assert_eq!(hulls, 2);
    assert!(
        fail(r#"let a = rect(); page(1, 1, stack([a, zoom_link(a, a, #{ attach: "x" })]))"#)
            .contains("unknown attach")
    );
}

#[test]
fn styled_scopes_style_to_what_it_builds() {
    let d = raw(r#"
        let s = styled(#{ scheme_label_size: 18 }, || [
            step(rect(), "big"),
            styled(#{ scheme_label_size: 4 }, || step(rect(), "small")),
            step(rect(), "big2")]);
        try { with_style(#{ base_size: 30 }, || { throw "boom"; }); } catch {}
        page(10, 10, col(s + [step(rect(), "plain"), caption("cap")]))"#);
    let size = |s: &str| texts(&d, s)[0].font_size;
    assert_eq!(size("big"), Some(18.0));
    assert_eq!(size("small"), Some(4.0));
    assert_eq!(size("big2"), Some(18.0));
    assert_eq!(size("plain"), Some(8.0));
    // Restored after the error too.
    assert_eq!(size("cap"), Some(8.0));
    let e = fail(r#"styled(#{ nope: 1 }, || rect())"#);
    assert!(e.contains("unknown key \"nope\""), "{e}");
    assert!(fail(r#"styled(#{}, rect())"#).contains("function"));
}

#[test]
fn fill_plots_take_the_spare_height() {
    let l = lay(r#"
        style(#{ plot_aspect: "fill" });
        page(100, 200, col([text("a"), labeled(plot(#{}, [line_plot([1, 2], [1, 2])]).id("p"), "(a)"),
                            text("b")]).stretch()).unit("pt")"#);
    let p = rect_of(&l, "p");
    assert!(near(p.h, 180.0), "{p:?}");
    // In a section that must shrink, the plot gives way first.
    let l = lay(r#"
        style(#{ plot_aspect: "fill", plot_min_height: 20 });
        fn section_pad() { 0 } fn section_gap() { 0 } fn section_bar_height() { 0 }
        fn section_title_size() { 10 }
        page(100, 100, section("S", [rect().height(70), plot(#{}, [line_plot([1, 2], [1, 2])]).id("p")],
                               #{ fit: "flex" })).unit("pt")"#);
    assert!(l.overflows.is_empty(), "{:?}", l.overflows);
    let p = rect_of(&l, "p");
    assert!(near(p.h, 30.0), "{p:?}");
}

#[test]
fn small_hooks_inset_titles_lists_and_section_bodies() {
    let d = raw(
        r#"poster("a4", poster_header(#{ title: "T", title_inset: [1, 2] }), [section("S", [bullets(["x"])])])"#,
    );
    let t = texts(&d, "T")[0];
    assert_eq!((t.margin.left, t.margin.right), (1.0, 2.0));
    let d = raw(
        r#"style(#{ bullet_inset: 0.5, section_body_inset: 0.25, section_pad: 0.1 });
        page(10, 10, section("S", [bullets(["x"])]))"#,
    );
    let pads: Vec<_> = d.nodes.values().map(|n| n.padding).collect();
    assert!(
        pads.iter().any(|p| p.left == 0.5 && p.right == 0.0),
        "bullets"
    );
    assert!(
        pads.iter()
            .any(|p| (p.left - 0.35).abs() < 1e-6 && (p.top - 0.1).abs() < 1e-6),
        "body"
    );
    assert!(
        fail(r#"page(1, 1, poster_header(#{ title: "T", title_inset: -1 }))"#)
            .contains("title_inset")
    );
}

#[test]
fn the_flex_fit_example_lays_out_without_overflows() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let src = std::fs::read_to_string(dir.join("flex_fit.figs")).unwrap();
    let d = Document::from_script_in(&src, Some(&dir)).unwrap_or_else(|e| panic!("{e}"));
    let l = layout(&d, &Pixels);
    assert!(l.overflows.is_empty(), "{:?}", l.overflows);
}
