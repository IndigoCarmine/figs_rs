//! Natural sizes and room: an image's size from its pixel density (`dpi`,
//! `[page] image_dpi`), a node that overhangs its parent across instead of
//! widening it (`overhang`), and `overflow = "fit"` scaling content up as
//! well as down.

#![cfg(feature = "script")]

use figs_core::geom::{Point, Rect, Size};
use figs_core::layout::{ComputedLayout, LeafMeasure};
use figs_core::schema::{ImageProps, TextProps};
use figs_core::{eval_script, layout, Document};

/// Text 10 pt per character wide, 10 pt per line; every image is 300 x 150
/// pixels.
struct Pixels;

impl LeafMeasure for Pixels {
    fn measure_text(&self, t: &TextProps, max_width: f32) -> Size {
        let w = 10.0 * t.content.chars().count() as f32;
        if w <= max_width {
            Size::new(w, 10.0)
        } else {
            let lines = (w / max_width.max(10.0)).ceil();
            Size::new(max_width, 10.0 * lines)
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
}

fn lay(src: &str) -> ComputedLayout {
    let d = Document::from_script(src).unwrap_or_else(|e| panic!("script failed: {e}"));
    layout(&d, &Pixels)
}

fn rect_of(l: &ComputedLayout, id: &str) -> Rect {
    l.nodes.iter().find(|n| n.id == id).unwrap().rect
}

fn approx(r: Rect, x: f32, y: f32, w: f32, h: f32) {
    let ok = (r.x - x).abs() < 1e-2
        && (r.y - y).abs() < 1e-2
        && (r.w - w).abs() < 1e-2
        && (r.h - h).abs() < 1e-2;
    assert!(ok, "expected ({x}, {y}, {w}, {h}), got {r:?}");
}

#[test]
fn dpi_gives_an_image_its_natural_size() {
    // 300 x 150 px at 150 dpi = 2 x 1 in = 144 x 72 pt, even in a stretched
    // column (it isn't stretched across).
    let l =
        lay(r#"page(400, "auto", col([image("a.png").dpi(150).id("a")]).stretch()).unit("pt")"#);
    approx(rect_of(&l, "a"), 0.0, 0.0, 144.0, 72.0);
    // The page default; an image's own dpi wins; 0 opts out (it fills).
    let l = lay(
        r#"page(400, "auto", col([image("a.png").id("a"), image("b.png").dpi(300).id("b"),
            image("c.png").dpi(0).id("c")]).stretch()).unit("pt").image_dpi(150)"#,
    );
    approx(rect_of(&l, "a"), 0.0, 0.0, 144.0, 72.0);
    approx(rect_of(&l, "b"), 0.0, 72.0, 72.0, 36.0);
    approx(rect_of(&l, "c"), 0.0, 108.0, 400.0, 200.0);
    // An explicit side wins (the other follows the aspect).
    let l =
        lay(r#"page(400, "auto", col([image("a.png").dpi(150).height(36).id("a")])).unit("pt")"#);
    approx(rect_of(&l, "a"), 0.0, 0.0, 72.0, 36.0);
    // Bigger than the room: shrunk to fit, keeping the aspect.
    let l = lay(r#"page(100, "auto", col([image("a.png").dpi(150).id("a")])).unit("pt")"#);
    approx(rect_of(&l, "a"), 0.0, 0.0, 100.0, 50.0);
    // An overlay over it keeps it natural too (the stack's base layer).
    let l = lay(
        r#"page(400, "auto", col([stack([image("a.png").dpi(150), text("x").at(0, 0)]).id("s")])
            .stretch()).unit("pt")"#,
    );
    approx(rect_of(&l, "s"), 0.0, 0.0, 144.0, 72.0);
    // An equal-height row still scales them to share one height.
    let l = lay(
        r#"page(300, "auto", row([image("a.png").dpi(150).id("a"), image("b.png").dpi(75).id("b")])
            .equal_height()).unit("pt")"#,
    );
    approx(rect_of(&l, "a"), 0.0, 0.0, 150.0, 75.0);
    approx(rect_of(&l, "b"), 150.0, 0.0, 150.0, 75.0);
    // Errors.
    let e = eval_script(r#"page(1, 1, rect().dpi(150))"#)
        .unwrap_err()
        .to_string();
    assert!(e.contains("applies to an image"), "{e}");
    let e = eval_script(r#"page(1, 1, image("a.png").dpi(-1))"#)
        .unwrap_err()
        .to_string();
    assert!(e.contains("pixels per inch"), "{e}");
    // TOML round trip.
    let raw = eval_script(r#"page(1, 1, image("a.png").dpi(150)).image_dpi(72)"#).unwrap();
    let toml = toml::to_string(&raw).unwrap();
    assert!(
        toml.contains("dpi = 150") && toml.contains("image_dpi = 72"),
        "{toml}"
    );
}

#[test]
fn an_overhanging_caption_takes_no_room_across() {
    // A 40 pt wide box with a 100 pt caption: the column stays 40 wide and
    // the caption is centred on it, in one line.
    let l = lay(
        r#"page(400, "auto", row([col([rect().width(40).height(20), text("0123456789").overhang().id("c")])
            .cross("center").id("k")]).main("center")).unit("pt")"#,
    );
    let k = rect_of(&l, "k");
    assert!((k.w - 40.0).abs() < 1e-2, "{k:?}");
    approx(rect_of(&l, "c"), k.x - 30.0, 20.0, 100.0, 10.0);
    // In a stretched column it isn't stretched (or wrapped) either (and the
    // row keeps room for it at its left end: the column starts at 30).
    let l = lay(
        r#"page(400, "auto", row([col([rect().width(40).height(20), text("0123456789").overhang().id("c")])
            .stretch().width(40).id("k")])).unit("pt")"#,
    );
    approx(rect_of(&l, "c"), 0.0, 20.0, 100.0, 10.0);
    // By the axis: centred on the marked node.
    let l = lay(
        r#"page(400, "auto", row([col([rect().width(40).height(20).axis(), text("0123456789").overhang().id("c")])
            .cross("axis").id("k")])).unit("pt")"#,
    );
    let k = rect_of(&l, "k");
    assert!((k.w - 40.0).abs() < 1e-2, "{k:?}");
    approx(rect_of(&l, "c"), 0.0, 20.0, 100.0, 10.0);
}

/// Node `id`'s box corners on the page, through its transform.
fn page_corners(l: &ComputedLayout, id: &str) -> (f32, Point, Point) {
    let n = l.nodes.iter().find(|n| n.id == id).unwrap();
    let t = n.transform.expect("scaled");
    let p = t.apply(Point::new(n.rect.x, n.rect.y));
    let q = t.apply(Point::new(n.rect.x + n.rect.w, n.rect.y + n.rect.h));
    (t.a, p, q)
}

#[test]
fn overflow_fit_scales_up_and_down() {
    // A 50 x 25 box in a 200 x 200 column: scaled 4x to 200 x 100 (the main
    // axis keeps its alignment: top).
    let l = lay(
        r#"page(200, 200, col([rect().width(50).height(25).id("r")]).overflow("fit")).unit("pt")"#,
    );
    let (s, p, q) = page_corners(&l, "r");
    assert!((s - 4.0).abs() < 1e-3, "{s}");
    assert!(p.x.abs() < 1e-2 && p.y.abs() < 1e-2, "{p:?}");
    assert!(
        (q.x - 200.0).abs() < 1e-2 && (q.y - 100.0).abs() < 1e-2,
        "{q:?}"
    );
    // Height-limited: 2x, centred across.
    let l = lay(
        r#"page(200, 100, col([rect().width(50).height(50).id("r")]).overflow("fit")).unit("pt")"#,
    );
    let (s, p, _) = page_corners(&l, "r");
    assert!((s - 2.0).abs() < 1e-3, "{s}");
    assert!((p.x - 50.0).abs() < 1e-2 && p.y.abs() < 1e-2, "{p:?}");
    // Too tall: shrunk, as "shrink".
    let l = lay(
        r#"page(100, 100, col([rect().width(50).height(200).id("r")]).overflow("fit")).unit("pt")"#,
    );
    let (s, _, _) = page_corners(&l, "r");
    assert!((s - 0.5).abs() < 1e-3, "{s}");
    let e = eval_script(r#"page(1, 1, col([]).overflow("grow"))"#)
        .unwrap_err()
        .to_string();
    assert!(e.contains("visible, clip, shrink, fit"), "{e}");
}

#[test]
fn a_row_keeps_room_for_its_edge_captions() {
    // The first item's caption reaches 30 pt past its left edge: the row
    // starts it 30 pt in (and is that much wider), so the caption stays
    // inside the row's box.
    let l = lay(
        r#"page(400, "auto", col([row([col([rect().width(40).height(20).id("r"), text("0123456789").overhang().id("c")])
            .cross("center"), rect().width(20).height(20)]).id("row")])).unit("pt")"#,
    );
    approx(rect_of(&l, "r"), 30.0, 0.0, 40.0, 20.0);
    approx(rect_of(&l, "c"), 0.0, 20.0, 100.0, 10.0);
    let row = rect_of(&l, "row");
    assert!((row.w - 90.0).abs() < 1e-2, "{row:?}");
}

#[test]
fn overflow_fit_shrinks_what_is_too_wide() {
    // A row of fixed 300 pt in a 100 pt column: scaled to a third.
    let l = lay(
        r#"page(100, 100, col([row([rect().width(300).height(30).id("r")])]).overflow("fit")).unit("pt")"#,
    );
    let (s, p, q) = page_corners(&l, "r");
    assert!((s - 1.0 / 3.0).abs() < 1e-3, "{s}");
    assert!(
        p.x.abs() < 1e-2 && (q.x - 100.0).abs() < 1e-2,
        "{p:?} {q:?}"
    );
}

#[test]
fn an_equal_height_row_scales_wrapped_natural_images_too() {
    // Captioned natural-size images (a column each) still share one height
    // that fills the row; outside such a row they keep their size.
    let l = lay(r#"page(300, "auto", col([
            row([col([image("a.png").dpi(150).id("a"), text("x")]).stretch(),
                 col([image("b.png").dpi(75).id("b"), text("y")]).stretch()]).equal_height(),
            col([image("c.png").dpi(150).id("c"), text("z")]).stretch(),
        ]).stretch()).unit("pt")"#);
    let (a, b) = (rect_of(&l, "a"), rect_of(&l, "b"));
    assert!(
        (a.w + b.w - 300.0).abs() < 0.5 && (a.h - b.h).abs() < 0.5,
        "{a:?} {b:?}"
    );
    approx(rect_of(&l, "c"), 0.0, a.h + 10.0, 144.0, 72.0);
}
