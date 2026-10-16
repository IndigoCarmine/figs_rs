//! Gradient paints and container decoration: resolve, TOML round trip, the
//! Rhai builders, PNG pixels (gradient endpoints, backgrounds behind
//! children, rounded corners, clipping, rotation) and SVG / PDF structure.

use figs_core::geom::{Brush, Color, LinearDir, Paint};
use figs_core::layout::{ComputedLayout, PaintContent};
use figs_core::schema::{NodeType, RawDocument};
use figs_core::{
    eval_script, layout, render_png, render_svg, Document, NullMeasurer, RenderOptions,
};
use tiny_skia::Pixmap;

fn lay(src: &str) -> ComputedLayout {
    layout(&Document::from_toml(src).expect("resolve"), &NullMeasurer)
}

fn png(src: &str) -> Pixmap {
    let bytes = render_png(&lay(src), &RenderOptions::default()).expect("render png");
    Pixmap::decode_png(&bytes).expect("decode png")
}

/// Straight RGB of a pixel (the tests use opaque paints).
fn rgb(p: &Pixmap, x: u32, y: u32) -> (u8, u8, u8) {
    let c = p.pixel(x, y).expect("in bounds").demultiply();
    (c.red(), c.green(), c.blue())
}

#[track_caller]
fn assert_near(p: &Pixmap, x: u32, y: u32, want: (u8, u8, u8), tol: i32) {
    let got = rgb(p, x, y);
    let ok = |a: u8, b: u8| (a as i32 - b as i32).abs() <= tol;
    assert!(
        ok(got.0, want.0) && ok(got.1, want.1) && ok(got.2, want.2),
        "pixel ({x},{y}) = {got:?}, expected {want:?} ±{tol}"
    );
}

/// A 100×60 pt page (72 dpi: 1 pt = 1 px) with a white background.
fn page(nodes: &str) -> String {
    format!(
        "[page]\nwidth = 100\nheight = 60\nunit = \"pt\"\ndpi = 72\nroot = \"root\"\nbackground = \"#ffffff\"\n{nodes}"
    )
}

const BW: &str = r##"stops = [[0, "#000000"], [1, "#ffffff"]]"##;

// ---------------------------------------------------------------------------
// resolve / round trip
// ---------------------------------------------------------------------------

#[test]
fn solid_strings_and_gradients_resolve() {
    let d = Document::from_toml(&page(&format!(
        r##"
        [nodes.root]
        type = "column"
        children = ["a", "b"]
        fill = {{ linear = 90, {BW} }}
        stroke = "#ffffff"
        stroke_width = 2
        radius = 6
        dash = [3, 2]
        clip = true
        [nodes.a]
        type = "rect"
        fill = "#ff0000"
        [nodes.b]
        type = "ellipse"
        fill = {{ radial = [0.5, 0.5], radius = 0.4, {BW} }}
        "##
    )))
    .unwrap();
    let root = &d.nodes[d.root];
    let deco = root.kind.decoration().expect("decorated column");
    assert!(matches!(
        deco.fill,
        Some(Paint::Linear {
            dir: LinearDir::Angle(a),
            ..
        }) if a == 90.0
    ));
    let stroke = deco.stroke.as_ref().unwrap();
    assert_eq!((stroke.width, stroke.dash.clone()), (2.0, vec![3.0, 2.0]));
    assert_eq!(deco.corner_radius, 6.0);
    assert!(deco.clip);
    let rect = d.nodes.iter().find(|n| n.id == "a").unwrap();
    let NodeType::Rect(r) = &rect.kind else {
        panic!()
    };
    assert_eq!(
        r.fill,
        Some(Paint::Solid(Color::parse_hex("#ff0000").unwrap()))
    );
}

#[test]
fn absolute_gradient_lengths_follow_the_page_unit() {
    let d = Document::from_toml(
        r##"
        [page]
        width = 10
        height = 10
        unit = "cm"
        root = "r"
        [nodes.r]
        type = "rect"
        fill = { radial = [1, 2], radius = 3, absolute = true, stops = ["#000", "#fff"] }
        "##,
    )
    .unwrap();
    let NodeType::Rect(r) = &d.nodes[d.root].kind else {
        panic!()
    };
    let Some(Paint::Radial { center, radius, .. }) = &r.fill else {
        panic!()
    };
    let cm = 72.0 / 2.54;
    assert!((center[0] - cm).abs() < 1e-3 && (radius.unwrap() - 3.0 * cm).abs() < 1e-3);
}

#[test]
fn invalid_paints_and_clip_are_reported_with_the_node() {
    let e = Document::from_toml(&page(
        r##"
        [nodes.root]
        type = "rect"
        clip = true
        "##,
    ))
    .unwrap_err()
    .to_string();
    assert!(e.contains("`root`") && e.contains("clip"), "{e}");

    let e = Document::from_toml(&page(
        r##"
        [nodes.root]
        type = "rect"
        fill = { linear = 0, stops = [[0.8, "#000"], [0.2, "#fff"]] }
        "##,
    ))
    .unwrap_err();
    assert!(format!("{e:?}").contains("must not decrease"), "{e:?}");
}

#[test]
fn paints_and_decoration_round_trip_through_toml() {
    let src = page(&format!(
        r##"
        [nodes.root]
        type = "stack"
        children = ["a"]
        fill = {{ linear = [[0, 0], [1, 1]], stops = [[0, "#0b1a3a"], [0.5, "#ff000080"], [1, "#1d3b6e"]] }}
        stroke = "#ffffff"
        corner_radius = 4
        dash = [2, 1]
        clip = true
        [nodes.a]
        type = "path"
        points = [[0, 0], [10, 0], [5, 8]]
        closed = true
        fill = {{ radial = [0.5, 0.5], focal = [0.3, 0.2], {BW} }}
        "##
    ));
    let raw: RawDocument = toml::from_str(&src).unwrap();
    let out = toml::to_string(&raw).unwrap();
    let back: RawDocument = toml::from_str(&out).unwrap_or_else(|e| panic!("{out}\n{e}"));
    for id in ["root", "a"] {
        let (x, y) = (&raw.nodes[id], &back.nodes[id]);
        assert_eq!(x.fill, y.fill, "{id}");
        assert_eq!(
            (x.clip, x.corner_radius, &x.dash),
            (y.clip, y.corner_radius, &y.dash)
        );
    }
    assert_eq!(raw.page.background, back.page.background);
}

// ---------------------------------------------------------------------------
// layout IR
// ---------------------------------------------------------------------------

#[test]
fn decorated_containers_are_frames_and_children_carry_clips() {
    let l = lay(&page(
        r##"
        [nodes.root]
        type = "stack"
        children = ["inner"]
        fill = "#ff0000"
        stroke = "#000000"
        stroke_width = 2
        corner_radius = 10
        clip = true
        [nodes.inner]
        type = "column"
        children = ["leaf"]
        clip = true
        [nodes.leaf]
        type = "rect"
        fill = "#00ff00"
        "##,
    ));
    let by = |id: &str| l.nodes.iter().find(|n| n.id == id).unwrap();
    let root = by("root");
    let PaintContent::Frame(f) = &root.content else {
        panic!("{:?}", root.content)
    };
    assert_eq!(f.corner_radius, 10.0);
    assert!(root.clips.is_empty(), "a clip affects the children only");
    // Clip-only container paints nothing itself.
    assert!(matches!(by("inner").content, PaintContent::Container));
    let inner = by("inner");
    assert_eq!(inner.clips.len(), 1);
    // Inside the 2 pt border: inset by the stroke width, radius reduced.
    assert_eq!(
        inner.clips[0].rect,
        figs_core::geom::Rect::new(2.0, 2.0, 96.0, 56.0)
    );
    assert_eq!(inner.clips[0].radius, 8.0);
    // Nested clips accumulate outermost first.
    assert_eq!(by("leaf").clips.len(), 2);
    assert_eq!(by("leaf").clips[0], inner.clips[0]);
}

#[test]
fn page_background_gradient_is_placed_on_the_page() {
    let l = lay(&page("[nodes.root]\ntype = \"column\"\n").replace(
        "background = \"#ffffff\"",
        &format!("background = {{ linear = 0, {BW} }}"),
    ));
    let Some(Brush::Linear { start, end, .. }) = &l.page.background else {
        panic!("{:?}", l.page.background)
    };
    assert_eq!((start.x, start.y, end.x, end.y), (0.0, 30.0, 100.0, 30.0));
}

// ---------------------------------------------------------------------------
// PNG pixels
// ---------------------------------------------------------------------------

#[test]
fn linear_gradient_endpoints() {
    // 0° runs left -> right.
    let p = png(&page(&format!(
        "[nodes.root]\ntype = \"rect\"\nfill = {{ linear = 0, {BW} }}\n"
    )));
    assert_near(&p, 0, 30, (0, 0, 0), 6);
    assert_near(&p, 99, 30, (255, 255, 255), 6);
    assert_near(&p, 50, 5, (128, 128, 128), 8);
    // 90° runs top -> bottom.
    let p = png(&page(&format!(
        "[nodes.root]\ntype = \"rect\"\nfill = {{ linear = 90, {BW} }}\n"
    )));
    assert_near(&p, 50, 0, (0, 0, 0), 8);
    assert_near(&p, 50, 59, (255, 255, 255), 8);
    // Two points, mid-box: padded beyond both ends.
    let p = png(&page(&format!(
        "[nodes.root]\ntype = \"rect\"\nfill = {{ linear = [[0.25, 0], [0.75, 0]], {BW} }}\n"
    )));
    assert_near(&p, 10, 30, (0, 0, 0), 2);
    assert_near(&p, 90, 30, (255, 255, 255), 2);
}

#[test]
fn radial_gradient_centre_and_rim() {
    let p = png(&page(&format!(
        "[nodes.root]\ntype = \"rect\"\nfill = {{ radial = [0.5, 0.5], {BW} }}\n"
    )));
    assert_near(&p, 50, 30, (0, 0, 0), 8);
    // Beyond the (elliptical) rim: the last stop.
    assert_near(&p, 1, 1, (255, 255, 255), 2);
    // Box-relative radius 0.5 reaches the edge midpoints.
    assert_near(&p, 1, 30, (250, 250, 250), 8);
}

#[test]
fn gradient_alpha_composites_over_the_background() {
    let p = png(&page(
        r##"
        [nodes.root]
        type = "rect"
        fill = { linear = 0, stops = [[0, "#ff0000ff"], [1, "#ff000000"]] }
        "##,
    ));
    assert_near(&p, 0, 30, (255, 0, 0), 6);
    assert_near(&p, 99, 30, (255, 255, 255), 6);
}

#[test]
fn container_background_is_behind_children() {
    let p = png(&page(
        r##"
        [nodes.root]
        type = "column"
        children = ["child"]
        fill = "#ff0000"
        padding = 20
        cross_axis_alignment = "stretch"
        [nodes.child]
        type = "rect"
        flex = 1
        fill = "#0000ff"
        "##,
    ));
    assert_near(&p, 5, 5, (255, 0, 0), 2);
    assert_near(&p, 50, 30, (0, 0, 255), 2);
}

#[test]
fn rounded_container_corners_stay_transparent() {
    let p = png(&page(
        r##"
        [nodes.root]
        type = "row"
        fill = "#ff0000"
        stroke = "#000000"
        stroke_width = 2
        radius = 20
        "##,
    ));
    // Corner outside the rounding: page background.
    assert_near(&p, 1, 1, (255, 255, 255), 2);
    // The border lies inside the box (outer edge = box edge).
    assert_near(&p, 50, 0, (0, 0, 0), 2);
    assert_near(&p, 50, 1, (0, 0, 0), 2);
    assert_near(&p, 50, 3, (255, 0, 0), 2);
}

#[test]
fn clip_cuts_children_at_the_rounded_inner_edge() {
    let nodes = |clip: bool| {
        page(&format!(
            r##"
            [nodes.root]
            type = "stack"
            children = ["frame"]
            [nodes.frame]
            type = "stack"
            children = ["big"]
            x = 20
            y = 10
            width = 60
            height = 40
            radius = 12
            stroke = "#000000"
            stroke_width = 2
            clip = {clip}
            [nodes.big]
            type = "rect"
            fill = "#0000ff"
            x = -15
            y = -8
            width = 90
            height = 56
            "##
        ))
    };
    let p = png(&nodes(true));
    // Outside the frame: untouched page.
    assert_near(&p, 10, 5, (255, 255, 255), 2);
    assert_near(&p, 90, 55, (255, 255, 255), 2);
    // In the rounded corner region of the frame (outside the inner curve).
    assert_near(&p, 21, 11, (255, 255, 255), 2);
    // Inside the frame: the child.
    assert_near(&p, 50, 30, (0, 0, 255), 2);
    // Without clip the child overhangs.
    let p = png(&nodes(false));
    assert_near(&p, 10, 5, (0, 0, 255), 2);
}

#[test]
fn gradient_turns_with_node_rotation() {
    // A 60x60 square rotated 90° clockwise: its left -> right gradient now
    // runs top -> bottom on the page.
    let p = png(&page(&format!(
        r##"
        [nodes.root]
        type = "stack"
        children = ["sq"]
        [nodes.sq]
        type = "rect"
        x = 20
        width = 60
        height = 60
        rotate = 90
        fill = {{ linear = 0, {BW} }}
        "##
    )));
    assert_near(&p, 50, 1, (0, 0, 0), 8);
    assert_near(&p, 50, 58, (255, 255, 255), 8);
    assert_near(&p, 22, 30, (128, 128, 128), 10);
    assert_near(&p, 77, 30, (128, 128, 128), 10);
}

// ---------------------------------------------------------------------------
// SVG / PDF structure
// ---------------------------------------------------------------------------

const MIXED: &str = r##"
    [nodes.root]
    type = "column"
    children = ["a", "b"]
    cross_axis_alignment = "stretch"
    fill = { linear = 90, stops = ["#0b1a3a", "#1d3b6e"] }
    stroke = "#ffffff"
    radius = 4
    clip = true
    [nodes.a]
    type = "rect"
    flex = 1
    fill = { linear = 0, stops = [[0, "#000000"], [1, "#ffffff80"]] }
    [nodes.b]
    type = "ellipse"
    flex = 1
    fill = { radial = [0.5, 0.5], focal = [0.3, 0.3], stops = ["#ffffff", "#000000"] }
"##;

#[test]
fn svg_defines_unique_gradients_and_clips() {
    let svg = render_svg(&lay(&page(MIXED)), &RenderOptions::default());
    let doc = roxmltree::Document::parse(&svg).expect("well-formed SVG");
    let lin: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("linearGradient"))
        .collect();
    let rad: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("radialGradient"))
        .collect();
    assert_eq!((lin.len(), rad.len()), (2, 1), "{svg}");
    let mut ids: Vec<&str> = lin
        .iter()
        .chain(&rad)
        .filter_map(|n| n.attribute("id"))
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3, "gradient ids are unique");
    for g in lin.iter().chain(&rad) {
        assert_eq!(g.attribute("gradientUnits"), Some("userSpaceOnUse"));
        let id = g.attribute("id").unwrap();
        assert!(svg.contains(&format!("fill=\"url(#{id})\"")), "{id} unused");
    }
    // The radial one is mapped onto the box and has a focal point.
    let r = rad[0];
    assert!(r.attribute("gradientTransform").is_some() && r.attribute("fx").is_some());
    // Translucent stop.
    assert!(svg.contains("stop-opacity=\"0.502\""), "{svg}");
    // The clip: one clipPath and a group referencing it around the children.
    let clip = doc
        .descendants()
        .find(|n| n.has_tag_name("clipPath"))
        .expect("clipPath");
    let cid = clip.attribute("id").unwrap();
    let g = doc
        .descendants()
        .find(|n| n.attribute("clip-path") == Some(&format!("url(#{cid})")))
        .expect("clipped group");
    assert_eq!(
        g.descendants().filter(|n| n.has_tag_name("rect")).count(),
        1
    );
}

#[cfg(feature = "pdf")]
#[test]
fn pdf_uses_shadings_and_a_clip_path() {
    use figs_core::render_pdf;
    let pdf = render_pdf(&lay(&page(MIXED)), &RenderOptions::default()).expect("pdf");
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/ShadingType 2"), "axial shading");
    assert!(text.contains("/ShadingType 3"), "radial shading");
    // Translucent stop -> a soft mask.
    assert!(text.contains("/SMask"), "soft mask for stop alpha");
    let content = inflated(&pdf);
    assert!(
        content.contains("W n") || content.contains("W\nn"),
        "clip path"
    );
}

/// Every stream of the PDF, inflated when Flate-compressed.
#[cfg(feature = "pdf")]
fn inflated(pdf: &[u8]) -> String {
    let mut out = String::new();
    let mut rest = pdf;
    while let Some(i) = rest.windows(6).position(|w| w == b"stream") {
        let mut body = &rest[i + 6..];
        if body.starts_with(b"\r\n") {
            body = &body[2..];
        } else if body.starts_with(b"\n") {
            body = &body[1..];
        }
        let end = body
            .windows(9)
            .position(|w| w == b"endstream")
            .unwrap_or(body.len());
        let data = &body[..end];
        match miniz_oxide::inflate::decompress_to_vec_zlib(data) {
            Ok(d) => out.push_str(&String::from_utf8_lossy(&d)),
            Err(_) => out.push_str(&String::from_utf8_lossy(data)),
        }
        out.push('\n');
        rest = &body[end..];
    }
    out
}

// ---------------------------------------------------------------------------
// script
// ---------------------------------------------------------------------------

#[test]
fn script_gradient_builders_and_container_setters() {
    let raw = eval_script(
        r##"
        page(10, 6,
            col([
                rect().fill(linear(45, [[0, "#000000"], [1, "#ffffff"]])),
                rect().fill(linear([0, 0], [1, 1], ["#000", "#888", "#fff"])),
                ellipse().fill(radial(["#fff", "#000"])),
                ellipse().fill(radial([0.5, 0.5], 2, ["#fff", "#000"]).focal(0.4, 0.4).absolute()),
            ]).fill(linear(90, ["#0b1a3a", "#1d3b6e"])).stroke("#ffffff").stroke_width(0.1)
              .radius(0.5).dash([0.3, 0.2]).clip(),
        ).unit("cm").background(radial(["#222", "#000"]))
        "##,
    )
    .unwrap();
    let root = &raw.nodes["root"];
    assert!(root.clip && root.corner_radius == Some(0.5) && root.dash == vec![0.3, 0.2]);
    assert!(matches!(root.fill, Some(Paint::Linear { .. })));
    assert!(matches!(raw.page.background, Some(Paint::Radial { .. })));
    let three = raw
        .nodes
        .values()
        .find_map(|n| match &n.fill {
            Some(Paint::Linear { stops, .. }) if stops.len() == 3 => Some(stops.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(three[1].offset, 0.5, "bare colours spread evenly");
    let abs = raw
        .nodes
        .values()
        .find_map(|n| match &n.fill {
            Some(p @ Paint::Radial { absolute: true, .. }) => Some(p.clone()),
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        abs,
        Paint::Radial {
            focal: Some([0.4, 0.4]),
            radius: Some(2.0),
            ..
        }
    ));
    // And it resolves.
    figs_core::schema::resolve::resolve(raw).unwrap();
}

#[test]
fn poster_frame_example_evaluates_with_frames_and_a_clip() {
    let raw = eval_script(include_str!("../../../examples/poster_frame.figs")).unwrap();
    assert!(matches!(raw.page.background, Some(Paint::Linear { .. })));
    let doc = figs_core::schema::resolve::resolve(raw).unwrap();
    let l = layout(&doc, &NullMeasurer);
    let frames = l
        .nodes
        .iter()
        .filter(|n| matches!(n.content, PaintContent::Frame(_)))
        .count();
    assert!(frames >= 7, "{frames} decorated containers");
    assert!(l.nodes.iter().any(|n| !n.clips.is_empty()));
}

#[test]
fn script_rejects_misplaced_paint_calls() {
    for (src, needle) in [
        ("page(1, 1, rect().clip())", "col/row/stack"),
        (
            r##"page(1, 1, rect().fill(linear(0, [[0.9, "#000"], [0.1, "#fff"]])))"##,
            "decrease",
        ),
        (
            r##"page(1, 1, rect().fill(linear(0, ["#000"]).focal(0, 0)))"##,
            "radial",
        ),
        (
            r##"page(1, 1, rect().fill(linear(0, [[0, "nope"]])))"##,
            "invalid color",
        ),
    ] {
        let e = eval_script(src).unwrap_err().to_string();
        assert!(e.contains(needle), "{src}: {e}");
    }
}
