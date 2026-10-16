//! Raw deserialization layer: TOML text -> [`RawDocument`].
//!
//! Each node is deserialized into a single [`RawNode`] carrying `type` plus
//! every possible field as an `Option`. Type-specific validation (e.g. "an
//! image needs `src`") happens later in [`super::resolve`], so that errors can
//! be reported with the offending node id.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::geom::{
    ArcSweep, ArrowHead, Color, CrossAxisAlign, Edges, ImageFit, LineCap, MainAxisAlign, TextAlign,
};
use crate::units::Unit;

/// `skip_serializing_if` helper for `f32` fields whose clean default is zero.
fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

/// The whole document as written in TOML, before validation.
///
/// Derives `Clone` + `Serialize` (in addition to `Deserialize`) so the GUI
/// editor can mutate a document in memory and write it back out as TOML. All
/// serialization attributes are additive — parsing is unchanged.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawDocument {
    pub page: RawPage,
    /// Flat map of node id -> node. Tree structure lives in each node's
    /// `children` field.
    #[serde(default)]
    pub nodes: HashMap<String, RawNode>,
}

/// The page height: a fixed length, or `"auto"` — derived from the content.
///
/// In TOML it is either a number (`height = 20`) or the string `"auto"`
/// (`height = "auto"`). The field stays required: omitting `height` is still an
/// error, so auto sizing is always an explicit opt-in.
///
/// With `Auto`, layout measures the root with the page width fixed and an
/// unbounded height, and the measured height becomes the page height (see
/// [`crate::layout::layout`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageHeight {
    /// A fixed height in the page unit.
    Fixed(f32),
    /// Fit the content.
    Auto,
}

impl PageHeight {
    /// The fixed height (page units), or `None` for `Auto`.
    pub fn fixed(self) -> Option<f32> {
        match self {
            PageHeight::Fixed(h) => Some(h),
            PageHeight::Auto => None,
        }
    }

    pub fn is_auto(self) -> bool {
        matches!(self, PageHeight::Auto)
    }
}

impl From<f32> for PageHeight {
    fn from(h: f32) -> Self {
        PageHeight::Fixed(h)
    }
}

impl Serialize for PageHeight {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            PageHeight::Fixed(h) => s.serialize_f32(*h),
            PageHeight::Auto => s.serialize_str("auto"),
        }
    }
}

impl<'de> Deserialize<'de> for PageHeight {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = PageHeight;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#"a number or "auto""#)
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<PageHeight, E> {
                Ok(PageHeight::Fixed(v as f32))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<PageHeight, E> {
                Ok(PageHeight::Fixed(v as f32))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<PageHeight, E> {
                Ok(PageHeight::Fixed(v as f32))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<PageHeight, E> {
                if v == "auto" {
                    Ok(PageHeight::Auto)
                } else {
                    Err(E::invalid_value(serde::de::Unexpected::Str(v), &self))
                }
            }
        }
        d.deserialize_any(V)
    }
}

/// The `[page]` table.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawPage {
    pub width: f32,
    /// A number (page units) or `"auto"` to fit the content.
    pub height: PageHeight,
    #[serde(default)]
    pub unit: Unit,
    /// Id of the root node within `nodes`.
    pub root: String,
    /// Raster resolution for PNG output. Defaults to 300 in resolve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Color>,
    /// Default font family for text nodes that don't name one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
}

/// The discriminant of a node, from its `type` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Column,
    Row,
    /// Overlapping children painted in array order (later = on top), each
    /// placed at its own `x`/`y` offset inside the stack's content box.
    Stack,
    Image,
    Text,
    Rect,
    /// A straight stroke between two `points` (in the node's own box).
    Line,
    /// A [`NodeKind::Line`] with an arrowhead (at the end by default).
    Arrow,
    /// Fills/strokes the ellipse inscribed in its box (a circle when square).
    Ellipse,
    /// A polyline (or polygon when `closed`) through `points`.
    Path,
}

/// A single node as written in TOML. All type-specific fields are optional;
/// which ones are required depends on `kind` and is checked in resolve.
///
/// `skip_serializing_if` keeps saved TOML clean: absent options, zero
/// margins/padding/spacing and empty `children` arrays are simply omitted.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawNode {
    #[serde(rename = "type")]
    pub kind: NodeKind,

    // ---- shared layout properties (all node types) ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flex: Option<f32>,
    /// Explicit fixed width, in the page unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Explicit fixed height, in the page unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// width / height; derives the other dimension when one is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<f32>,
    #[serde(default, skip_serializing_if = "Edges::is_zero")]
    pub margin: Edges,
    #[serde(default, skip_serializing_if = "Edges::is_zero")]
    pub padding: Edges,
    /// Horizontal offset of the border box (after margin) from the parent's
    /// content-box left edge, in the page unit. Only honored when the parent is
    /// a `stack`; ignored under `column`/`row` and on the root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    /// Vertical offset, like [`RawNode::x`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    /// Which point of the node's border box sits at its `x`/`y` offset, as
    /// fractions `[ax, ay]` of its width/height: `[0, 0]` (the default) is the
    /// top-left, `[0.5, 0.5]` the centre, `[1, 0.5]` the right-middle. Only a
    /// `stack` parent reads it; an anchored child may extend past any edge of
    /// the stack (it is clamped to the stack's size, not to the space after
    /// its offset).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<[f32; 2]>,

    /// User coordinates `[x0, y0, x1, y1]` laid over the content box (`x0` ->
    /// left, `x1` -> right, `y0` -> top, `y1` -> bottom; reversing a pair flips
    /// that axis). On a `stack` its children's `x`/`y` offsets are in these
    /// units; on a `line`/`arrow`/`path` its `points`/`segments` are. Only
    /// positions are mapped: sizes, strokes, dashes and heads stay in the page
    /// unit. See [`crate::geom::ViewBox`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewbox: Option<[f32; 4]>,

    // ---- container (column / row / stack; spacing & alignment are flex-only) ----
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    /// Gap between children, in the page unit.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub spacing: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_axis_alignment: Option<MainAxisAlign>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_axis_alignment: Option<CrossAxisAlign>,

    // ---- text ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Font size in points (never affected by the page unit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    /// Line height as a multiple of `font_size` (unitless). Defaults to 1.2.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f32>,

    // ---- image ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<ImageFit>,

    // ---- rect ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Color>,
    /// Stroke width in the page unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f32>,
    /// Corner radius in the page unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f32>,

    // ---- vector shapes (line / arrow / ellipse / path; `fill`/`stroke`/
    //      `stroke_width` above are shared) ----
    /// `[x, y]` vertices in the page unit, relative to the node's content-box
    /// origin, joined by straight lines. Without `segments`, `line`/`arrow`
    /// take exactly two and `path` two or more.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f32; 2]>,
    /// Further (possibly curved) segments continuing from the last of
    /// `points`, in order. See [`RawSegment`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<RawSegment>,
    /// Close a `path` into a polygon (last point joins the first).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
    /// Dash pattern: alternating on/off lengths in the page unit (an odd list
    /// is repeated, as in SVG). Empty = solid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f32>,
    /// Line cap for open ends (`butt` / `round` / `square`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap: Option<LineCap>,
    /// Arrowhead placement (`none` / `start` / `end` / `both`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<ArrowHead>,
    /// Arrowhead length in the page unit (default: proportional to the stroke width).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_size: Option<f32>,
}

/// One entry of a shape's `segments`: a straight, Bézier or arc segment from
/// the current point to `to` (page units, same frame as `points`). At most one
/// of `quad` / `cubic` / `arc` may be given; with none it is a straight line.
///
/// ```toml
/// segments = [
///   { to = [4, 0] },                              # straight
///   { quad = [5, -1], to = [6, 0] },              # quadratic: one control point
///   { cubic = [[7, 1], [8, -1]], to = [9, 0] },   # cubic: two control points
///   { arc = 1.5, to = [12, 0] },                  # circular arc, radius 1.5
///   { arc = [2, 1], rotation = 30, large = true, sweep = "ccw", to = [9, 3] },
/// ]
/// ```
///
/// Arcs follow SVG's endpoint parameterization: of the (up to) four arcs of
/// the given ellipse joining the two points, `large` picks the one spanning
/// more than 180° and `sweep` the turning direction; radii too small to span
/// the chord are scaled up uniformly.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawSegment {
    /// Quadratic Bézier control point `[x, y]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quad: Option<[f32; 2]>,
    /// Cubic Bézier control points `[[x1, y1], [x2, y2]]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cubic: Option<[[f32; 2]; 2]>,
    /// Elliptical arc radius: `r` (circular) or `[rx, ry]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arc: Option<ArcRadius>,
    /// Arc only: rotation of the ellipse's x axis, in degrees (clockwise on the page).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f32>,
    /// Arc only: take the arc spanning more than 180°.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub large: bool,
    /// Arc only: turning direction (`cw`, the default, or `ccw`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sweep: Option<ArcSweep>,
    /// End point `[x, y]`.
    pub to: [f32; 2],
}

impl RawSegment {
    /// A straight segment to `to`.
    pub fn line(to: [f32; 2]) -> Self {
        RawSegment {
            quad: None,
            cubic: None,
            arc: None,
            rotation: None,
            large: false,
            sweep: None,
            to,
        }
    }
}

/// An arc segment's radius: one number (circle) or `[rx, ry]` (ellipse).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ArcRadius {
    Circle(f32),
    Ellipse([f32; 2]),
}

impl ArcRadius {
    /// `(rx, ry)`.
    pub fn radii(self) -> (f32, f32) {
        match self {
            ArcRadius::Circle(r) => (r, r),
            ArcRadius::Ellipse([rx, ry]) => (rx, ry),
        }
    }
}

/// Error from the raw TOML parsing stage.
#[derive(Debug, thiserror::Error)]
#[error("failed to parse TOML document")]
pub struct ParseError(#[from] pub toml::de::Error);

/// Parse TOML source text into a [`RawDocument`].
pub fn parse_str(src: &str) -> Result<RawDocument, ParseError> {
    Ok(toml::from_str(src)?)
}
