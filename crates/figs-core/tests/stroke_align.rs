//! `stroke_align = "inside" | "center" | "outside"` on rect, ellipse, closed
//! path and container borders: shared geometry, resolve rules and defaults,
//! clip regions, and the same stroke band in PNG (pixels), SVG (element
//! geometry) and PDF (pixels via `pdftoppm`, skipped when it isn't
//! installed).

use figs_core::geom::{PathSegment, Point, Rect, StrokeAlign};
use figs_core::layout::PaintContent;
use figs_core::render::geometry::{aligned_rect, offset_outline};
use figs_core::{layout, render_png, render_svg, Document, NullMeasurer, RenderOptions};
use tiny_skia::Pixmap;

/// The three shape kinds and the container border, each as a 40 x 40 pt
/// outline whose left edge is at x = 20 (vertical centre y = 40), stroked
/// 8 pt black with no fill, on a 100 x 80 pt page at 72 dpi (1 pt = 1 px).
#[derive(Clone, Copy, Debug)]
enum Kind {
    Rect,
    Ellipse,
    Path,
    Frame,
}

const KINDS: [Kind; 4] = [Kind::Rect, Kind::Ellipse, Kind::Path, Kind::Frame];
const ALIGNS: [Option<&str>; 4] = [None, Some("inside"), Some("center"), Some("outside")];

fn doc_src(kind: Kind, align: Option<&str>) -> String {
    let align = align.map_or(String::new(), |a| format!("stroke_align = \"{a}\""));
    let body = match kind {
        Kind::Rect => "type = \"rect\"\nwidth = 40\nheight = 40",
        Kind::Ellipse => "type = \"ellipse\"\nwidth = 40\nheight = 40",
        Kind::Path => {
            "type = \"path\"\npoints = [[0, 0], [40, 0], [40, 40], [0, 40]]\nclosed = true"
        }
        Kind::Frame => "type = \"stack\"\nwidth = 40\nheight = 40",
    };
    format!(
        r##"
        [page]
        width = 100
        height = 80
        unit = "pt"
        dpi = 72
        root = "page"
        [nodes.page]
        type = "stack"
        children = ["s"]
        [nodes.s]
        {body}
        x = 20
        y = 20
        stroke = "#000000"
        stroke_width = 8
        {align}
        "##
    )
}

/// The effective alignment: containers default to inside, shapes to center.
fn effective(kind: Kind, align: Option<&str>) -> StrokeAlign {
    match align {
        Some(a) => StrokeAlign::parse(a).unwrap(),
        None if matches!(kind, Kind::Frame) => StrokeAlign::Inside,
        None => StrokeAlign::Center,
    }
}

/// Left and right x of the stroke band across the left edge (x = 20).
fn band(align: StrokeAlign) -> (u32, u32) {
    match align {
        StrokeAlign::Inside => (20, 28),
        StrokeAlign::Center => (16, 24),
        StrokeAlign::Outside => (12, 20),
    }
}

fn layout_of(kind: Kind, align: Option<&str>) -> figs_core::ComputedLayout {
    let doc = Document::from_toml(&doc_src(kind, align)).expect("resolve");
    layout(&doc, &NullMeasurer)
}

fn dark(p: &Pixmap, x: u32, y: u32) -> i32 {
    let c = p.pixel(x, y).expect("in bounds").demultiply();
    255 - (c.red() as i32 + c.green() as i32 + c.blue() as i32) / 3
}

#[track_caller]
fn assert_band(p: &Pixmap, kind: Kind, align: StrokeAlign, what: &str) {
    let (a, b) = band(align);
    let y = 40;
    for x in [a, b - 1] {
        assert!(
            dark(p, x, y) > 200,
            "{what} {kind:?} {align:?}: x = {x} should be inside the stroke"
        );
    }
    for x in [a - 2, b + 1] {
        assert!(
            dark(p, x, y) < 50,
            "{what} {kind:?} {align:?}: x = {x} should be outside the stroke"
        );
    }
}

#[test]
fn png_draws_the_band_where_the_alignment_puts_it() {
    for kind in KINDS {
        for align in ALIGNS {
            let l = layout_of(kind, align);
            let png = render_png(&l, &RenderOptions::default()).unwrap();
            let p = Pixmap::decode_png(&png).unwrap();
            assert_band(&p, kind, effective(kind, align), "png");
        }
    }
}

/// The stroked element's centre-line left edge in an SVG, and its width.
fn svg_stroke_left(svg: &str) -> (f32, f32) {
    let doc = roxmltree::Document::parse(svg).expect("well-formed");
    let el = doc
        .descendants()
        .find(|n| n.is_element() && n.attribute("stroke-width").is_some())
        .expect("a stroked element");
    let num = |k: &str| el.attribute(k).unwrap().parse::<f32>().unwrap();
    let width = num("stroke-width");
    let left = match el.tag_name().name() {
        "rect" => num("x"),
        "ellipse" => num("cx") - num("rx"),
        "path" => {
            let d = el.attribute("d").unwrap();
            let nums: Vec<f32> = d
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().unwrap())
                .collect();
            nums.iter()
                .step_by(2)
                .copied()
                .fold(f32::INFINITY, f32::min)
        }
        other => panic!("unexpected stroked element <{other}>"),
    };
    (left, width)
}

#[test]
fn svg_strokes_the_offset_centre_line() {
    for kind in KINDS {
        for align in ALIGNS {
            let l = layout_of(kind, align);
            let svg = render_svg(&l, &RenderOptions::default());
            let (left, width) = svg_stroke_left(&svg);
            assert_eq!(width, 8.0);
            let want = 20.0 - effective(kind, align).centre_offset(8.0);
            assert!(
                (left - want).abs() < 0.01,
                "svg {kind:?} {align:?}: centre line at {left}, want {want}\n{svg}"
            );
        }
    }
}

#[cfg(feature = "pdf")]
#[test]
fn pdf_draws_the_same_band() {
    let probe = std::process::Command::new("pdftoppm").arg("-v").output();
    if probe.is_err() {
        eprintln!("pdftoppm not installed; skipping the PDF raster check");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    for kind in KINDS {
        for align in ALIGNS {
            let l = layout_of(kind, align);
            let pdf = figs_core::render_pdf(&l, &RenderOptions::default()).unwrap();
            let path = dir.path().join("s.pdf");
            std::fs::write(&path, pdf).unwrap();
            let out = dir.path().join("s");
            let status = std::process::Command::new("pdftoppm")
                .args(["-r", "72", "-png", "-singlefile"])
                .arg(&path)
                .arg(&out)
                .status()
                .unwrap();
            assert!(status.success());
            let p = Pixmap::load_png(out.with_extension("png")).unwrap();
            assert_band(&p, kind, effective(kind, align), "pdf");
        }
    }
}

#[test]
fn defaults_keep_the_old_behaviour() {
    for kind in KINDS {
        let l = layout_of(kind, None);
        let n = l.nodes.iter().find(|n| n.id == "s").unwrap();
        let got = match &n.content {
            PaintContent::Rect { stroke_align, .. }
            | PaintContent::Ellipse { stroke_align, .. } => *stroke_align,
            PaintContent::Path(p) => p.stroke_align,
            PaintContent::Frame(f) => f.stroke_align,
            other => panic!("{other:?}"),
        };
        assert_eq!(got, effective(kind, None), "{kind:?}");
    }
}

#[test]
fn only_closed_outlines_take_stroke_align() {
    let bad = |body: &str| {
        let src = format!(
            r##"
            [page]
            width = 10
            height = 10
            root = "n"
            [nodes.n]
            {body}
            stroke_align = "inside"
            "##
        );
        Document::from_toml(&src).unwrap_err().to_string()
    };
    for body in [
        "type = \"line\"\npoints = [[0, 0], [1, 1]]",
        "type = \"arrow\"\npoints = [[0, 0], [1, 1]]",
        "type = \"path\"\npoints = [[0, 0], [1, 1], [2, 0]]",
        "type = \"text\"\ncontent = \"a\"",
        "type = \"image\"\nsrc = \"a.png\"",
    ] {
        let e = bad(body);
        assert!(e.contains("stroke_align"), "{e}");
    }
    let e = Document::from_toml(
        "[page]\nwidth = 1\nheight = 1\nroot = \"r\"\n[nodes.r]\ntype = \"rect\"\nstroke_align = \"middle\"",
    )
    .unwrap_err();
    assert!(format!("{e:?}").contains("middle"), "{e:?}");
}

#[test]
fn clip_stays_inside_the_border() {
    for (align, inset) in [("inside", 8.0), ("center", 4.0), ("outside", 0.0)] {
        let src = format!(
            r##"
            [page]
            width = 100
            height = 80
            unit = "pt"
            root = "page"
            [nodes.page]
            type = "stack"
            children = ["s"]
            [nodes.s]
            type = "stack"
            x = 20
            y = 20
            width = 40
            height = 40
            radius = 10
            stroke = "#000000"
            stroke_width = 8
            stroke_align = "{align}"
            clip = true
            children = ["c"]
            [nodes.c]
            type = "rect"
            fill = "#ff0000"
            "##
        );
        let l = layout(&Document::from_toml(&src).unwrap(), &NullMeasurer);
        let c = l.nodes.iter().find(|n| n.id == "c").unwrap();
        let clip = &c.clips[0];
        assert_eq!(
            clip.rect,
            Rect::new(
                20.0 + inset,
                20.0 + inset,
                40.0 - 2.0 * inset,
                40.0 - 2.0 * inset
            ),
            "{align}"
        );
        assert_eq!(clip.radius, (10.0f32 - inset).max(0.0), "{align}");
    }
}

#[test]
fn aligned_rect_offsets_the_centre_line() {
    let r = Rect::new(10.0, 10.0, 40.0, 20.0);
    let (line, radius) = aligned_rect(r, 5.0, StrokeAlign::Inside, 4.0).unwrap();
    assert_eq!((line, radius), (Rect::new(12.0, 12.0, 36.0, 16.0), 3.0));
    let (line, radius) = aligned_rect(r, 5.0, StrokeAlign::Outside, 4.0).unwrap();
    assert_eq!((line, radius), (Rect::new(8.0, 8.0, 44.0, 24.0), 7.0));
    // A square corner stays square (the miter of its offset).
    assert_eq!(
        aligned_rect(r, 0.0, StrokeAlign::Outside, 4.0).unwrap().1,
        0.0
    );
    assert_eq!(
        aligned_rect(r, 5.0, StrokeAlign::Center, 4.0),
        Some((r, 5.0))
    );
    // An inside stroke wider than the box has no centre line.
    assert_eq!(aligned_rect(r, 0.0, StrokeAlign::Inside, 50.0), None);
}

fn square(clockwise: bool) -> Vec<PathSegment> {
    let mut pts = [
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ];
    if !clockwise {
        pts.reverse();
    }
    let mut segs = vec![PathSegment::MoveTo(pts[0])];
    segs.extend(pts[1..].iter().map(|p| PathSegment::LineTo(*p)));
    segs.push(PathSegment::Close);
    segs
}

fn points(segs: &[PathSegment]) -> Vec<Point> {
    segs.iter()
        .flat_map(|s| match *s {
            PathSegment::MoveTo(p) | PathSegment::LineTo(p) => vec![p],
            PathSegment::QuadTo(c, p) => vec![c, p],
            PathSegment::CubicTo(a, b, p) => vec![a, b, p],
            PathSegment::Close => vec![],
        })
        .collect()
}

#[test]
fn offset_outline_is_exact_for_polygons_in_either_winding() {
    for cw in [true, false] {
        let out = offset_outline(&square(cw), 2.0);
        let mut xs: Vec<(f32, f32)> = points(&out).iter().map(|p| (p.x, p.y)).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            xs,
            vec![(-2.0, -2.0), (-2.0, 12.0), (12.0, -2.0), (12.0, 12.0)],
            "outwards, clockwise = {cw}"
        );
        let inner = offset_outline(&square(cw), -3.0);
        let lo = points(&inner)
            .iter()
            .map(|p| p.x)
            .fold(f32::INFINITY, f32::min);
        assert_eq!(lo, 3.0, "inwards, clockwise = {cw}");
        assert!(matches!(inner.last(), Some(PathSegment::Close)));
    }
    // Open outlines are left alone.
    let open = &square(true)[..4];
    assert_eq!(offset_outline(open, 2.0), open.to_vec());
}

#[test]
fn offset_outline_follows_curves_closely() {
    // A circle of radius 20 from four cubic quarter arcs, offset by 4: every
    // point of the result stays within 0.5 % of the offset of the true circle.
    let segs = figs_core::render::geometry::ellipse(Rect::new(0.0, 0.0, 40.0, 40.0));
    for d in [4.0f32, -4.0] {
        let out = offset_outline(&segs, d);
        let mut start = Point::default();
        for s in &out {
            if let PathSegment::CubicTo(a, b, p) = *s {
                for i in 0..=16 {
                    let t = i as f32 / 16.0;
                    let mt = 1.0 - t;
                    let x = mt * mt * mt * start.x
                        + 3.0 * mt * mt * t * a.x
                        + 3.0 * mt * t * t * b.x
                        + t * t * t * p.x;
                    let y = mt * mt * mt * start.y
                        + 3.0 * mt * mt * t * a.y
                        + 3.0 * mt * t * t * b.y
                        + t * t * t * p.y;
                    let r = ((x - 20.0).powi(2) + (y - 20.0).powi(2)).sqrt();
                    assert!(
                        (r - (20.0 + d)).abs() < 0.005 * d.abs() + 0.01,
                        "d = {d}: radius {r} at t = {t}"
                    );
                }
                start = p;
            } else if let PathSegment::MoveTo(p) = *s {
                start = p;
            }
        }
    }
}

#[test]
fn script_setter() {
    let d = figs_core::eval_script(
        r##"page(10, 10, stack([rect().stroke("#000000").stroke_align("outside")]))"##,
    )
    .unwrap();
    let r = d.nodes.values().find(|n| n.stroke_align.is_some()).unwrap();
    assert_eq!(r.stroke_align, Some(StrokeAlign::Outside));
    let toml = toml::to_string(&d).unwrap();
    assert!(toml.contains("stroke_align = \"outside\""), "{toml}");
    let e = figs_core::eval_script(r##"page(10, 10, rect().stroke_align("middle"))"##).unwrap_err();
    assert!(e.message.contains("stroke_align"), "{}", e.message);
}
