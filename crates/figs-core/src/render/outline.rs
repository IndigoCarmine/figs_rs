//! Backend-neutral glyph outlines (via `ttf-parser`, from the same font bytes
//! cosmic-text shaped with).
//!
//! The SVG backend draws every glyph from these outlines; all three backends
//! stroke text outlines (node and span `stroke`s) and synthetic bold along
//! them ([`OutlineCache::text_strokes`]), so they are the same geometry in
//! PNG, PDF and SVG. Outlines are unhinted, in font units (y up), cached per
//! (face, glyph).

use std::collections::HashMap;

use crate::geom::{Color, LineCap, LineJoin, PathSegment, Point, Rect, StrokeStyle, TextAlign};
use crate::text::{FontStore, ShapedGlyph, ShapedText};

use super::geometry;

type FaceId = cosmic_text::fontdb::ID;

/// One glyph's outline in font units (y up).
#[derive(Debug, Clone)]
pub struct GlyphOutline {
    /// Closed contours: `MoveTo`, `LineTo`/`QuadTo`/`CubicTo`…, `Close`.
    pub segments: Vec<PathSegment>,
    /// Font units per em (font units → points: `font_size / units_per_em`).
    pub units_per_em: f32,
}

/// Raw font file bytes plus face index.
struct FaceData {
    data: Vec<u8>,
    index: u32,
}

/// Per-renderer cache of font faces and glyph outlines.
#[derive(Default)]
pub struct OutlineCache {
    faces: HashMap<FaceId, Option<FaceData>>,
    /// `None` = the glyph has no outline (a space, a bitmap-only emoji, …).
    glyphs: HashMap<(FaceId, u16), Option<GlyphOutline>>,
}

impl OutlineCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The outline of glyph `gid` of `face`, extracted on first use. `None`
    /// when the glyph draws nothing or the face can't be read.
    pub fn glyph(&mut self, fonts: &FontStore, face: FaceId, gid: u16) -> Option<&GlyphOutline> {
        if !self.glyphs.contains_key(&(face, gid)) {
            let data = self.faces.entry(face).or_insert_with(|| {
                fonts.with_system(|sys| {
                    sys.db().with_face_data(face, |d, index| FaceData {
                        data: d.to_vec(),
                        index,
                    })
                })
            });
            let outline = data.as_ref().and_then(|fd| {
                let parsed = ttf_parser::Face::parse(&fd.data, fd.index).ok()?;
                let upem = parsed.units_per_em() as f32;
                let mut b = SegmentBuilder(Vec::new());
                parsed.outline_glyph(ttf_parser::GlyphId(gid), &mut b)?;
                (!b.0.is_empty() && upem > 0.0).then_some(GlyphOutline {
                    segments: b.0,
                    units_per_em: upem,
                })
            });
            self.glyphs.insert((face, gid), outline);
        }
        self.glyphs.get(&(face, gid)).and_then(Option::as_ref)
    }

    /// The outlines of every glyph of `shaped`, placed exactly where the text
    /// backends draw them inside `rect` (page points, y down) — one multi-
    /// contour path, ready to stroke for a text `stroke`. Stroking it as a
    /// single path (rather than glyph by glyph) keeps a translucent outline
    /// uniform where neighbouring glyphs' outlines overlap.
    pub fn text_outline(
        &mut self,
        fonts: &FontStore,
        shaped: &ShapedText,
        rect: Rect,
        align: TextAlign,
    ) -> Vec<PathSegment> {
        self.outline_where(fonts, shaped, rect, align, |_| true)
    }

    /// Everything a text box strokes, in paint order, each as one path: the
    /// glyph outlines (`stroke`s of the node and its spans, one path per
    /// distinct [`ShapedText::strokes`] entry), then the synthetic-bold
    /// outlines (one path per colour × width, round joins, in the glyphs'
    /// own colour). Backends stroke these before filling the glyphs, so an
    /// outline never covers a glyph and synthetic bold thickens it evenly.
    pub fn text_strokes(
        &mut self,
        fonts: &FontStore,
        shaped: &ShapedText,
        rect: Rect,
        align: TextAlign,
    ) -> Vec<(StrokeStyle, Vec<PathSegment>)> {
        let mut out = Vec::new();
        for (k, style) in shaped.strokes.iter().enumerate() {
            let segs =
                self.outline_where(fonts, shaped, rect, align, |g| g.stroke == Some(k as u16));
            if !segs.is_empty() {
                out.push((style.clone(), segs));
            }
        }
        let mut bolds: Vec<(Color, f32)> = Vec::new();
        for g in shaped.lines.iter().flat_map(|l| &l.glyphs) {
            if g.embolden > 0.0 && !bolds.contains(&(g.color, g.embolden)) {
                bolds.push((g.color, g.embolden));
            }
        }
        for (color, width) in bolds {
            let segs = self.outline_where(fonts, shaped, rect, align, |g| {
                g.color == color && g.embolden == width
            });
            if !segs.is_empty() {
                let style = StrokeStyle {
                    color,
                    width,
                    dash: Vec::new(),
                    cap: LineCap::Butt,
                    join: LineJoin::Round,
                };
                out.push((style, segs));
            }
        }
        out
    }

    /// The outlines of the glyphs of `shaped` for which `keep` holds, placed
    /// as in [`Self::text_outline`].
    pub fn outline_where(
        &mut self,
        fonts: &FontStore,
        shaped: &ShapedText,
        rect: Rect,
        align: TextAlign,
        keep: impl Fn(&ShapedGlyph) -> bool,
    ) -> Vec<PathSegment> {
        let mut out = Vec::new();
        for line in &shaped.lines {
            let align_off = geometry::align_offset(align, line.width, rect.w);
            for g in line.glyphs.iter().filter(|g| keep(g)) {
                let origin = glyph_origin(rect, align_off, line.baseline, g);
                let Some(outline) = self.glyph(fonts, g.font_id, g.glyph_id) else {
                    continue;
                };
                let k = g.font_size / outline.units_per_em;
                let shear = g.skew;
                let map =
                    |p: Point| Point::new(origin.x + (p.x + shear * p.y) * k, origin.y - p.y * k);
                out.extend(outline.segments.iter().map(|s| match *s {
                    PathSegment::MoveTo(p) => PathSegment::MoveTo(map(p)),
                    PathSegment::LineTo(p) => PathSegment::LineTo(map(p)),
                    PathSegment::QuadTo(c, p) => PathSegment::QuadTo(map(c), map(p)),
                    PathSegment::CubicTo(c1, c2, p) => {
                        PathSegment::CubicTo(map(c1), map(c2), map(p))
                    }
                    PathSegment::Close => PathSegment::Close,
                }));
            }
        }
        out
    }
}

/// Pen position (page points, y down) of a shaped glyph in a text box at
/// `rect`, on a line `align_off` in from the left with its baseline at
/// `baseline` (relative to the box top) — the origin every backend draws the
/// glyph at.
pub fn glyph_origin(rect: Rect, align_off: f32, baseline: f32, g: &ShapedGlyph) -> Point {
    Point::new(
        rect.x + align_off + g.x + g.x_offset,
        rect.y + baseline - g.y_offset,
    )
}

/// Collects a glyph outline as [`PathSegment`]s in font units.
struct SegmentBuilder(Vec<PathSegment>);

impl ttf_parser::OutlineBuilder for SegmentBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(PathSegment::MoveTo(Point::new(x, y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(PathSegment::LineTo(Point::new(x, y)));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0
            .push(PathSegment::QuadTo(Point::new(x1, y1), Point::new(x, y)));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.push(PathSegment::CubicTo(
            Point::new(x1, y1),
            Point::new(x2, y2),
            Point::new(x, y),
        ));
    }
    fn close(&mut self) {
        self.0.push(PathSegment::Close);
    }
}
