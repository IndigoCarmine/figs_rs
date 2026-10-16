//! Effects: shadows, glows, group opacity, pixelated images and arrowhead
//! styles — pixels (PNG at 72 dpi, so 1 pt = 1 px), PDF/SVG structure, TOML
//! round trip and the script setters. Also the plot-as-page-root regression.

use std::path::{Path, PathBuf};

use figs_core::geom::{HeadStyle, HeadWidth};
use figs_core::layout::PaintContent;
use figs_core::schema::{parse::parse_str, NodeType};
use figs_core::{layout, render_png, render_svg, Document, FontStore, NullMeasurer, RenderOptions};
use tiny_skia::Pixmap;

fn render_with(doc: &Document, fonts: Option<&FontStore>, base_dir: &Path) -> Pixmap {
    let computed = match fonts {
        Some(f) => layout(doc, f),
        None => layout(doc, &NullMeasurer),
    };
    let bytes = render_png(&computed, &RenderOptions { fonts, base_dir }).expect("png");
    Pixmap::decode_png(&bytes).expect("decode png")
}

fn render(src: &str) -> Pixmap {
    render_with(
        &Document::from_toml(src).expect("resolve"),
        None,
        Path::new("."),
    )
}

fn rgb(p: &Pixmap, x: u32, y: u32) -> (u8, u8, u8) {
    let c = p.pixel(x, y).expect("in bounds").demultiply();
    (c.red(), c.green(), c.blue())
}

/// Darkness `0..=255` (255 - mean channel) of a pixel on a white page.
fn dark(p: &Pixmap, x: u32, y: u32) -> i32 {
    let (r, g, b) = rgb(p, x, y);
    255 - (r as i32 + g as i32 + b as i32) / 3
}

#[track_caller]
fn assert_near(got: (u8, u8, u8), want: (u8, u8, u8), tol: i32) {
    let ok = |a: u8, b: u8| (a as i32 - b as i32).abs() <= tol;
    assert!(
        ok(got.0, want.0) && ok(got.1, want.1) && ok(got.2, want.2),
        "got {got:?}, want {want:?} (±{tol})"
    );
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("figs-fx-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A 100 x 80 pt page holding `rect` at (20, 20), 30 x 20, with `extra` keys.
fn shadowed_rect(extra: &str) -> String {
    format!(
        r##"
        [page]
        width = 100
        height = 80
        unit = "pt"
        dpi = 72
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["r"]
        [nodes.r]
        type = "rect"
        x = 20
        y = 20
        width = 30
        height = 20
        fill = "#ff0000"
        {extra}
        "##
    )
}

#[test]
fn hard_shadow_is_the_silhouette_offset() {
    let p = render(&shadowed_rect(
        r##"shadow = { dx = 10, dy = 8, color = "#000000" }"##,
    ));
    // The rect itself, untouched.
    assert_near(rgb(&p, 35, 30), (255, 0, 0), 2);
    // Its shadow: the same box moved by (10, 8), black where not covered.
    assert_near(rgb(&p, 55, 35), (0, 0, 0), 2);
    assert_near(rgb(&p, 40, 45), (0, 0, 0), 2);
    // Outside the shadow: white.
    assert_near(rgb(&p, 25, 45), (255, 255, 255), 2);
    assert_near(rgb(&p, 62, 35), (255, 255, 255), 2);
    assert_near(rgb(&p, 55, 25), (255, 255, 255), 2);
}

#[test]
fn soft_shadow_falls_off_smoothly_and_does_not_move_layout() {
    let src = shadowed_rect(r##"shadow = { dx = 10, dy = 0, blur = 8, color = "#000000" }"##);
    let p = render(&src);
    // Along y = 30 to the right of the shadow edge at x = 60 (sigma = 4 pt):
    // darkest inside, about half at the edge, fading to nothing by 3 sigma.
    let row: Vec<i32> = (50..76).map(|x| dark(&p, x, 30)).collect();
    for w in row.windows(2) {
        assert!(w[0] >= w[1], "darkness must not increase outwards: {row:?}");
    }
    let at = |x: u32| dark(&p, x, 30);
    assert!(at(52) > 200, "deep inside: {}", at(52));
    assert!((100..160).contains(&at(60)), "half at the edge: {}", at(60));
    assert!(at(64) < 60 && at(64) > 5, "a sigma out: {}", at(64));
    assert!(at(74) <= 2, "3.5 sigma out: {}", at(74));
    // Layout is exactly as without the shadow.
    let with = layout(&Document::from_toml(&src).unwrap(), &NullMeasurer);
    let without = layout(
        &Document::from_toml(&shadowed_rect("")).unwrap(),
        &NullMeasurer,
    );
    let rects = |l: &figs_core::ComputedLayout| l.nodes.iter().map(|n| n.rect).collect::<Vec<_>>();
    assert_eq!(rects(&with), rects(&without));
}

#[test]
fn shadow_spread_grows_the_silhouette() {
    let p = render(&shadowed_rect(
        r##"shadow = { dx = 0, dy = 0, spread = 4, color = "#000000" }"##,
    ));
    assert_near(rgb(&p, 52, 30), (0, 0, 0), 2); // 2 pt right of the rect
    assert_near(rgb(&p, 56, 30), (255, 255, 255), 2); // 6 pt right
                                                      // Rounded corner: the diagonal 3.5 pt out is past a radius of 4.
    assert!(dark(&p, 53, 43) < 128, "corner {}", dark(&p, 53, 43));
}

#[test]
fn shadow_follows_rotation_and_clips() {
    // Turned 90 degrees: the stack lays out the turned footprint, 20 x 30 at
    // (20, 20); the shadow (a page-space offset) is that box moved right by
    // 10, showing at x 40..50.
    let p = render(&shadowed_rect(
        r##"rotate = 90
        shadow = { dx = 10, color = "#000000" }"##,
    ));
    assert_near(rgb(&p, 30, 35), (255, 0, 0), 2);
    assert_near(rgb(&p, 45, 22), (0, 0, 0), 2);
    assert_near(rgb(&p, 45, 47), (0, 0, 0), 2);
    assert_near(rgb(&p, 52, 35), (255, 255, 255), 2);
    assert_near(rgb(&p, 45, 17), (255, 255, 255), 2);
    // Inside a clipping parent, the shadow is cut at the clip.
    let src = r##"
        [page]
        width = 100
        height = 80
        unit = "pt"
        dpi = 72
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["frame"]
        [nodes.frame]
        type = "stack"
        x = 10
        y = 10
        width = 45
        height = 40
        clip = true
        children = ["r"]
        [nodes.r]
        type = "rect"
        x = 10
        y = 10
        width = 30
        height = 20
        fill = "#ff0000"
        shadow = { dx = 10, dy = 8, color = "#000000" }
    "##;
    let p = render(src);
    assert_near(rgb(&p, 53, 40), (0, 0, 0), 2); // inside the frame
    assert_near(rgb(&p, 58, 40), (255, 255, 255), 2); // clipped away
}

#[test]
fn glow_hugs_text() {
    let fonts = FontStore::new();
    let src = r##"
        [page]
        width = 200
        height = 90
        unit = "pt"
        dpi = 72
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["t"]
        [nodes.t]
        type = "text"
        content = "HHH"
        font_size = 30
        color = "#ffffff"
        x = 20
        y = 10
        glow = { blur = 6, color = "#000000" }
    "##;
    let doc = Document::from_toml(src).unwrap();
    let p = render_with(&doc, Some(&fonts), Path::new("."));
    let l = layout(&doc, &fonts);
    let t = l.nodes.iter().find(|n| n.id == "t").unwrap().rect;
    // White glyphs on a white page: only the glow shows. It is dark right
    // around the glyphs, inside the text box…
    let (x0, y0) = (t.x as u32, t.y as u32);
    let (x1, y1) = ((t.x + t.w) as u32, (t.y + t.h) as u32);
    let darkest = (x0..x1)
        .flat_map(|x| (y0..y1).map(move |y| (x, y)))
        .map(|(x, y)| dark(&p, x, y))
        .max()
        .unwrap();
    assert!(darkest > 180, "a dark halo around the glyphs: {darkest}");
    // …and gone well clear of them (blur 6: it reaches about 6 pt).
    for (x, y) in [(5, 30), (x1 + 15, 30), (80, y1 + 12)] {
        assert!(dark(&p, x, y) < 3, "({x}, {y}) is {}", dark(&p, x, y));
    }
}

/// Two overlapping red rects (50 % opaque) on white: as one group, or each.
fn overlap(group: bool) -> Pixmap {
    let (g, each) = if group {
        ("opacity = 0.5", "")
    } else {
        ("", "opacity = 0.5")
    };
    render(&format!(
        r##"
        [page]
        width = 80
        height = 40
        unit = "pt"
        dpi = 72
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["a", "b"]
        {g}
        [nodes.a]
        type = "rect"
        x = 10
        y = 10
        width = 30
        height = 20
        fill = "#ff0000"
        {each}
        [nodes.b]
        type = "rect"
        x = 30
        y = 10
        width = 30
        height = 20
        fill = "#ff0000"
        {each}
        "##
    ))
}

#[test]
fn opacity_composites_the_group_once() {
    let half = (255, 128, 128);
    let p = overlap(true);
    assert_near(rgb(&p, 20, 20), half, 2);
    assert_near(rgb(&p, 35, 20), half, 2); // overlap: same as a single layer
    assert_near(rgb(&p, 50, 20), half, 2);
    let p = overlap(false);
    assert_near(rgb(&p, 20, 20), half, 2);
    assert_near(rgb(&p, 35, 20), (255, 64, 64), 3); // per node: 75 % red
}

#[test]
fn zero_opacity_hides_the_subtree_and_its_shadow() {
    let p = render(&shadowed_rect(
        r##"opacity = 0
        shadow = { dx = 10, dy = 8, color = "#000000" }"##,
    ));
    assert_near(rgb(&p, 35, 30), (255, 255, 255), 0);
    assert_near(rgb(&p, 55, 35), (255, 255, 255), 0);
}

/// A 2 x 1 px image (black | white) drawn 40 x 20 pt, smooth or not.
fn two_pixels(smooth: bool) -> (Document, PathBuf) {
    let dir = scratch_dir(if smooth { "smooth" } else { "nearest" });
    let mut img = image::RgbImage::new(2, 1);
    img.put_pixel(0, 0, image::Rgb([0, 0, 0]));
    img.put_pixel(1, 0, image::Rgb([255, 255, 255]));
    img.save(dir.join("px.png")).unwrap();
    let src = format!(
        r##"
        [page]
        width = 40
        height = 20
        unit = "pt"
        dpi = 72
        background = "#ff0000"
        root = "i"
        [nodes.i]
        type = "image"
        src = "px.png"
        fit = "fill"
        smooth = {smooth}
        "##
    );
    (Document::from_toml(&src).unwrap(), dir)
}

#[test]
fn smooth_false_scales_nearest_neighbour() {
    let (doc, dir) = two_pixels(false);
    let p = render_with(&doc, None, &dir);
    assert_near(rgb(&p, 18, 10), (0, 0, 0), 1);
    assert_near(rgb(&p, 21, 10), (255, 255, 255), 1);
    let (doc, dir2) = two_pixels(true);
    let p = render_with(&doc, None, &dir2);
    // Bilinear: grey in between.
    let (r, _, _) = rgb(&p, 20, 10);
    assert!((60..200).contains(&r), "smooth middle {r}");
    // SVG marks the pixelated image and upscales its pixels.
    let svg = render_svg(
        &layout(&doc, &NullMeasurer),
        &RenderOptions {
            fonts: None,
            base_dir: &dir2,
        },
    );
    assert!(!svg.contains("image-rendering"));
    let (doc, _) = two_pixels(false);
    let svg = render_svg(
        &layout(&doc, &NullMeasurer),
        &RenderOptions {
            fonts: None,
            base_dir: &dir,
        },
    );
    assert!(svg.contains("style=\"image-rendering:pixelated\""), "{svg}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn smooth_is_only_for_images() {
    let e = Document::from_toml(&shadowed_rect("smooth = false"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("smooth"), "{e}");
}

fn arrow_doc(extra: &str) -> String {
    format!(
        r##"
        [page]
        width = 100
        height = 60
        unit = "pt"
        dpi = 72
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["a"]
        [nodes.a]
        type = "arrow"
        points = [[10, 30], [90, 30]]
        stroke = "#000000"
        stroke_width = 2
        head_size = 12
        {extra}
        "##
    )
}

#[test]
fn head_width_is_absolute_or_per_stroke() {
    let width_of = |extra: &str| {
        let d = Document::from_toml(&arrow_doc(extra)).unwrap();
        let n = d.nodes.iter().find(|n| n.id == "a").unwrap();
        match &n.kind {
            NodeType::Path(p) => p.head_width,
            _ => unreachable!(),
        }
    };
    assert!((width_of("") - 9.6).abs() < 1e-4); // default 0.8 x head_size
    assert!((width_of("head_width = 20") - 20.0).abs() < 1e-4);
    assert!((width_of(r#"head_width = "4x""#) - 8.0).abs() < 1e-4);
    // Pixels: a 20 pt wide head covers 9 pt above the shaft at its base.
    let p = render(&arrow_doc("head_width = 20"));
    assert_near(rgb(&p, 79, 23), (0, 0, 0), 2);
    assert_near(rgb(&p, 79, 18), (255, 255, 255), 2);
    let p = render(&arrow_doc(""));
    assert_near(rgb(&p, 79, 23), (255, 255, 255), 2); // 9.6 wide: 4.8 up
}

#[test]
fn barb_heads_sit_on_one_side() {
    let l = layout(
        &Document::from_toml(&arrow_doc(r#"head_style = "barb_left""#)).unwrap(),
        &NullMeasurer,
    );
    let PaintContent::Path(path) = &l.nodes[1].content else {
        panic!("a path");
    };
    let geo = path.stroke_geometry();
    assert_eq!(geo.heads.len(), 1);
    let barb = &geo.heads[0];
    assert_eq!(barb.len(), 3);
    // Tip, the outer corner on the left (up, for a rightward arrow), and the
    // inner base on the shaft's far (lower) edge.
    assert!((barb[0].x - 90.0).abs() < 1e-3 && (barb[0].y - 30.0).abs() < 1e-3);
    assert!((barb[1].x - 78.0).abs() < 1e-3 && (barb[1].y - 25.2).abs() < 1e-3);
    assert!((barb[2].x - 78.0).abs() < 1e-3 && (barb[2].y - 31.0).abs() < 1e-3);
    // The shaft stops at the head's base.
    let end = geo.shaft.last().unwrap().end().unwrap();
    assert!((end.x - 78.0).abs() < 1e-3, "{end:?}");
    // Pixels: ink above the shaft near the tip, none below.
    // (Pixel rows 27 and 32 mirror each other about the axis y = 30.)
    let p = render(&arrow_doc(r#"head_style = "barb_left""#));
    assert!(dark(&p, 82, 27) > 200);
    assert!(dark(&p, 82, 32) < 5);
    let p = render(&arrow_doc(r#"head_style = "barb_right""#));
    assert!(dark(&p, 82, 27) < 5);
    assert!(dark(&p, 82, 32) > 200);
}

#[test]
fn stealth_and_open_heads() {
    let geo = |style: &str| {
        let l = layout(
            &Document::from_toml(&arrow_doc(&format!(r#"head_style = "{style}""#))).unwrap(),
            &NullMeasurer,
        );
        let PaintContent::Path(path) = &l.nodes[1].content else {
            panic!("a path");
        };
        path.stroke_geometry()
    };
    let s = geo("stealth");
    assert_eq!(s.heads[0].len(), 4);
    // The notch lies on the axis, 0.6 x head_size back from the tip.
    assert!((s.heads[0][2].x - (90.0 - 7.2)).abs() < 1e-3);
    let o = geo("open");
    assert!(o.heads.is_empty());
    assert_eq!(o.open_heads.len(), 1);
    // The chevron's mitered apex lands on the tip: its geometric apex is
    // pulled back by (w / 2) / sin(half angle).
    let [_, apex, _] = o.open_heads[0];
    let half = 4.8f32;
    let sin = half / (12.0f32 * 12.0 + half * half).sqrt();
    assert!((apex.x - (90.0 - 1.0 / sin)).abs() < 1e-3, "{apex:?}");
    let p = render(&arrow_doc(r#"head_style = "open""#));
    assert!(dark(&p, 87, 30) > 200, "the apex is inked");
    assert!(dark(&p, 92, 30) < 5, "nothing past the tip");
}

#[test]
fn effects_round_trip_through_toml() {
    let src = r##"
        [page]
        width = 10
        height = 10
        root = "s"
        [nodes.s]
        type = "stack"
        children = ["i", "a"]
        opacity = 0.6
        shadow = { dx = 0.1, dy = 0.2, blur = 0.3, spread = 0.05, color = "#00000066" }
        glow = { blur = 0.2, color = "#000000c0" }
        [nodes.i]
        type = "image"
        src = "x.png"
        smooth = false
        [nodes.a]
        type = "arrow"
        points = [[0, 0], [5, 0]]
        head_width = "3x"
        head_style = "barb_right"
    "##;
    let raw = parse_str(src).unwrap();
    let out = toml::to_string(&raw).unwrap();
    let back = parse_str(&out).unwrap();
    let s = &back.nodes["s"];
    assert_eq!(s.opacity, Some(0.6));
    assert_eq!(s.shadow, raw.nodes["s"].shadow);
    assert_eq!(s.glow, raw.nodes["s"].glow);
    assert_eq!(back.nodes["i"].smooth, Some(false));
    assert_eq!(back.nodes["a"].head_width, Some(HeadWidth::PerStroke(3.0)));
    assert_eq!(back.nodes["a"].head_style, Some(HeadStyle::BarbRight));
    assert!(out.contains("head_width = \"3x\""), "{out}");
    // And it resolves: glow becomes a shadow without offset, spread blur/2.
    let d = Document::from_toml(&out).unwrap();
    let fx = &d.nodes.iter().find(|n| n.id == "s").unwrap().effects;
    assert_eq!(fx.shadows.len(), 2);
    let g = fx.shadows[1];
    assert_eq!((g.dx, g.dy), (0.0, 0.0));
    assert!((g.spread - g.blur / 2.0).abs() < 1e-4);
}

#[test]
fn invalid_effects_are_rejected() {
    for bad in [
        "opacity = 1.5",
        "shadow = { blur = -1 }",
        "glow = { blur = -2 }",
        r#"head_width = "wide""#,
    ] {
        let src = format!(
            r##"
            [page]
            width = 10
            height = 10
            root = "a"
            [nodes.a]
            type = "arrow"
            points = [[0, 0], [5, 0]]
            {bad}
            "##
        );
        assert!(Document::from_toml(&src).is_err(), "{bad} accepted");
    }
}

#[test]
fn script_setters() {
    let d = figs_core::script::eval_script(
        r##"
        page(10, 10, stack([
            rect().shadow(0.1, 0.2, 0.3, "#00000066").glow(0.2, "#000"),
            image("a.png").smooth(false).opacity(0.5),
            arrow(0, 0, 5, 0).head_width("3x").head_style("barb_left"),
            arrow(0, 1, 5, 1).head_width(0.4).shadow(0.1, 0.1, 0.2, 0.05, "#000000"),
        ])).unit("cm")
        "##,
    )
    .unwrap();
    let by = |pred: &dyn Fn(&figs_core::schema::RawNode) -> bool| {
        d.nodes.values().find(|n| pred(n)).unwrap().clone()
    };
    let r = by(&|n| n.kind == figs_core::schema::NodeKind::Rect);
    let s = r.shadow.unwrap();
    assert_eq!((s.dx, s.dy, s.blur), (0.1, 0.2, 0.3));
    assert_eq!(r.glow.unwrap().blur, 0.2);
    let i = by(&|n| n.kind == figs_core::schema::NodeKind::Image);
    assert_eq!((i.smooth, i.opacity), (Some(false), Some(0.5)));
    let a = by(&|n| n.head_style.is_some());
    assert_eq!(a.head_width, Some(HeadWidth::PerStroke(3.0)));
    assert_eq!(a.head_style, Some(HeadStyle::BarbLeft));
    let b = by(&|n| n.head_width == Some(HeadWidth::Abs(0.4)));
    assert_eq!(b.shadow.unwrap().spread, 0.05);

    for bad in [
        r#"page(1, 1, rect().opacity(2))"#,
        r#"page(1, 1, rect().smooth(false))"#,
        r#"page(1, 1, arrow(0, 0, 1, 0).head_style("hook"))"#,
        r#"page(1, 1, arrow(0, 0, 1, 0).head_width("x3"))"#,
        r#"page(1, 1, rect().shadow(0, 0, -1))"#,
    ] {
        assert!(
            figs_core::script::eval_script(bad).is_err(),
            "{bad} accepted"
        );
    }
}

#[test]
fn prelude_block_arrow_and_equilibrium() {
    let d = figs_core::script::eval_script(
        r##"
        page(10, 4, stack([
            block_arrow(0, 1, 6, 1, 0.6, 1.4, 1.0),
            equilibrium(0, 3, 4, 3),
        ])).unit("cm")
        "##,
    )
    .unwrap();
    let block = d
        .nodes
        .values()
        .find(|n| n.kind == figs_core::schema::NodeKind::Path)
        .unwrap();
    assert!(block.closed);
    assert_eq!(block.points.len(), 7);
    // Tip at (6, 1), head base 1.0 back and 1.4 across, shaft 0.6 wide.
    assert!(block.points.contains(&[6.0, 1.0]));
    assert!(block.points.contains(&[5.0, 0.3]));
    assert!(block.points.contains(&[0.0, 0.7]));
    let barbs: Vec<_> = d
        .nodes
        .values()
        .filter(|n| n.head_style == Some(HeadStyle::BarbLeft))
        .collect();
    assert_eq!(barbs.len(), 2);
    // One each way, either side of the chord.
    let ys: Vec<f32> = barbs.iter().map(|n| n.points[0][1]).collect();
    assert!(
        ys.iter().any(|y| *y < 3.0) && ys.iter().any(|y| *y > 3.0),
        "{ys:?}"
    );
    let dirs: Vec<f32> = barbs
        .iter()
        .map(|n| n.points[1][0] - n.points[0][0])
        .collect();
    assert!(dirs.iter().any(|d| *d > 0.0) && dirs.iter().any(|d| *d < 0.0));
}

#[test]
fn svg_and_pdf_carry_effects() {
    let src = shadowed_rect(
        r##"opacity = 0.5
        shadow = { dx = 4, dy = 4, blur = 6 }"##,
    );
    let doc = Document::from_toml(&src).unwrap();
    let l = layout(&doc, &NullMeasurer);
    let svg = render_svg(&l, &RenderOptions::default());
    assert!(svg.contains("<g opacity=\"0.5\">"), "{svg}");
    let shadow_at = svg.find("class=\"figs-shadow\"").expect("shadow image");
    let rect_at = svg.find("fill=\"#ff0000\"").expect("rect");
    assert!(shadow_at < rect_at, "the shadow is painted first");
    roxmltree::Document::parse(&svg).expect("well-formed");
    #[cfg(feature = "pdf")]
    {
        let pdf = figs_core::render_pdf(&l, &RenderOptions::default()).unwrap();
        let n = |s: &str| pdf.windows(s.len()).filter(|w| *w == s.as_bytes()).count();
        // The shadow raster is an image with a soft mask, interpolated.
        assert_eq!(n("/Subtype/Image"), 2, "colour + soft mask");
        assert!(n("/Interpolate true") >= 1);
        // The opacity group is a transparency group with a constant alpha.
        assert!(n("/ca 0.5") >= 1 || n("/ca .5") >= 1);
    }
}

#[test]
fn plot_as_auto_page_root_fills_its_aspect() {
    // A column with an aspect ratio as the root of an auto-height page: its
    // height comes from the width, and a flex child shares it.
    let src = r##"
        [page]
        width = 400
        height = "auto"
        unit = "pt"
        root = "c"
        [nodes.c]
        type = "column"
        aspect_ratio = 2
        cross_axis_alignment = "stretch"
        children = ["body"]
        [nodes.body]
        type = "stack"
        flex = 1
    "##;
    let l = layout(&Document::from_toml(src).unwrap(), &NullMeasurer);
    assert!((l.page.height_pt - 200.0).abs() < 1e-3);
    let body = l.nodes.iter().find(|n| n.id == "body").unwrap();
    assert!((body.rect.h - 200.0).abs() < 1e-3, "{:?}", body.rect);
    // The framework's plot, directly as the root of a journal page.
    let fonts = FontStore::new();
    let doc = Document::from_script(
        r#"page("double", plot(#{ xlabel: "x", ylabel: "y" }, [line_plot([0, 1, 2], [0, 1, 4])]))"#,
    )
    .unwrap();
    let l = layout(&doc, &fonts);
    let w = l.page.width_pt;
    assert!(
        (l.page.height_pt - w * 0.75).abs() < 1.0,
        "{}",
        l.page.height_pt
    );
    // The data area (the line's viewbox box) is most of the page, not 0.
    let line = l
        .nodes
        .iter()
        .find(|n| matches!(&n.content, PaintContent::Path(p) if p.stroke.is_some() && p.segments.len() == 3))
        .expect("the data line");
    assert!(line.rect.h > 0.5 * l.page.height_pt, "{:?}", line.rect);
}
