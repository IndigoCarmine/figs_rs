//! Raw deserialization layer: TOML text -> [`RawDocument`].
//!
//! Each node is deserialized into a single [`RawNode`] carrying `type` plus
//! every possible field as an `Option`. Type-specific validation (e.g. "an
//! image needs `src`") happens later in [`super::resolve`], so that errors can
//! be reported with the offending node id.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::geom::{
    AnchorValue, ArcSweep, ArrowHead, AxisMark, Color, CrossAxisAlign, Edges, HeadStyle, HeadWidth,
    ImageFit, LineCap, LineJoin, MainAxisAlign, Overflow, Paint, StrokeAlign, TextAlign,
};
use crate::units::Unit;

use super::span::{FontStyle, FontSynthesis, RawSpan, ScriptMetrics};

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
    /// Page fill: a colour or a gradient over the whole page (see
    /// [`crate::paint`]). White when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Paint>,
    /// Default font family list for text nodes that don't name one: a string
    /// (`"Arial, Yu Gothic"`) or an array of names (see [`RawNode::font_family`]).
    #[serde(
        default,
        deserialize_with = "super::span::family_list_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub font_family: Option<String>,
    /// Super/subscript geometry for every text node (`"figs"`, the default,
    /// or `"office"`: × 2/3, +30 % / −25 %); a node may override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_metrics: Option<ScriptMetrics>,
    /// Synthetic bold for every text node (`"none"`, the default, `"auto"`,
    /// `"bold"`); a node may override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_synthesis: Option<FontSynthesis>,
    /// Synthetic-bold stroke width, em of the glyph size (default 1/35).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthetic_bold_width: Option<f32>,
    /// Slant of a synthetic italic, degrees (default 14; PowerPoint 18.43).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthetic_slant: Option<f32>,
    /// PNG only: draw every shape stroke (rect, ellipse, line/arrow/path,
    /// container border) at least this many device pixels wide at the page
    /// `dpi`, as PDF viewers do with hairlines, so a 0.25 pt border stays a
    /// crisp line in a 100 dpi render. Default `0` (off: exact widths). PDF
    /// and SVG are unaffected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_stroke_px: Option<f32>,
    /// Default pixel density of images that set no `dpi` of their own: an
    /// image's natural size is its pixels / dpi inches (see
    /// [`RawNode::dpi`]). Unset: images have no natural size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_dpi: Option<f32>,
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
    /// A line (or a wide band) between two other nodes, `from` and `to`,
    /// drawn after layout between points of their final boxes. It takes no
    /// room in its parent; where it sits in the tree only decides its paint
    /// order (and the clips / effects it is under).
    Connector,
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
    /// A human-readable name for diagnostics (an overflow warning names the
    /// container by it: `section "Results"`); no effect on layout or paint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

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
    ///
    /// On a `text` node a component may instead be `"baseline"` (the first
    /// line's baseline) or `"last_baseline"`: `anchor = [0, "baseline"]` puts
    /// the left end of the first baseline at the offset. Like fractions it
    /// acts in the parent's frame, so it goes in the slot *across* the
    /// baseline: `y` for upright (and `180`) text, `x` for a quarter-turned
    /// one (`rotate = -90, anchor = ["baseline", 1]` is a label reading
    /// upwards whose baseline is at `x` and which starts at `y`).
    /// `"cap_middle"` is the line half the first line's cap height above its
    /// first baseline — the optical centre of a line of text (`anchor = [0.5,
    /// "cap_middle"]` centres a label on a point by its capitals, not by its
    /// line box). It goes in the same slot; on a node with no text inside it
    /// is the middle of the box.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<[AnchorValue; 2]>,
    /// Rotation in degrees, **clockwise** on the page, about the centre of the
    /// node's box; it turns the node and its whole subtree. A multiple of 90°
    /// is a *quarter turn*: an odd one (`90`, `-90`, `270`) swaps the node's
    /// footprint for layout, so the parent sees the rotated box (a vertical
    /// axis label in a row is as wide as the text is tall). Any other angle is
    /// paint-only: layout uses the unrotated box and the drawing may overhang.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotate: Option<f32>,
    /// The node's axis for a parent with `cross_axis_alignment = "axis"`:
    /// `true` marks it as its container's axis provider (its box middle, or
    /// for a container its own axis), `[fx, fy]` puts the axis point at those
    /// fractions of its box (see [`AxisMark`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axis: Option<AxisMark>,

    /// User coordinates `[x0, y0, x1, y1]` laid over the content box (`x0` ->
    /// left, `x1` -> right, `y0` -> top, `y1` -> bottom; reversing a pair flips
    /// that axis). On a `stack` its children's `x`/`y` offsets are in these
    /// units; on a `line`/`arrow`/`path` its `points`/`segments` are. Only
    /// positions are mapped: sizes, strokes, dashes and heads stay in the page
    /// unit. See [`crate::geom::ViewBox`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewbox: Option<[f32; 4]>,
    /// `stack` only: keep everything the children draw inside the stack's
    /// (padding-inset) box. Children that would hang past an edge — an anchored
    /// label centred on the edge, a tick label beside a plot's axis — shrink
    /// the content box (where offsets and the `viewbox` map) by exactly their
    /// overhang instead. The stack's own size is unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contain: bool,
    /// `column` / `row` / `stack` only: clip the children to the node's
    /// (rounded) box — inside its border when it has a `stroke`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clip: bool,

    // ---- effects (all node types; purely visual, never affect layout) ----
    /// A drop shadow behind everything the node and its subtree paint (see
    /// [`RawShadow`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow: Option<RawShadow>,
    /// An outer glow: a soft halo hugging the silhouette (see [`RawGlow`]),
    /// painted behind the node, over its `shadow`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glow: Option<RawGlow>,
    /// Group opacity `0..=1`: the node and its subtree are composited as one
    /// layer (overlaps inside don't add up).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,

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
    /// Column only: the distance (page unit) between consecutive children's
    /// first baselines, which then replaces `spacing` between them (it may
    /// overlap them); a pair where either child has no baseline (no text
    /// inside) keeps `spacing`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_pitch: Option<f32>,
    /// Row only: the children whose height follows their width (images,
    /// aspect-ratio nodes, panels built on them) share **one height**, and
    /// together with the other children (at their natural width) and the
    /// spacing fill the row's width — widths in proportion to the aspect
    /// ratios. `flex` is ignored in such a row.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub equal_height: bool,
    /// Column / row: what happens when the children overflow the box along
    /// the main axis — `visible` (default), `clip` or `shrink` (see
    /// [`Overflow`]). Every overflow is reported either way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overflow: Option<Overflow>,
    /// Take no room across the parent column / row: the node is laid out at
    /// its natural cross size (a caption in one line) and aligned as the
    /// parent's cross alignment says — centred unless `start` / `end` —
    /// overhanging the parent's box instead of widening it (or wrapping to
    /// it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overhang: bool,

    // ---- text ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Font size in points (never affected by the page unit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    /// Ordered font-family list, authoritative per character: each character
    /// uses the first listed family that has a glyph for it (`"Arial, Yu
    /// Gothic"` sets Latin in Arial and Japanese in Yu Gothic). A string with
    /// comma-separated names or an array of names; stored comma-joined. Or
    /// per script, `{ latin = "Yu Gothic", cjk = "MS Mincho" }` (stored as
    /// `"latin: Yu Gothic; cjk: MS Mincho"`, see [`super::FontSpec`]).
    #[serde(
        default,
        deserialize_with = "super::span::family_list_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    /// `normal` / `italic` (a real italic face when the family has one, else
    /// a synthesized slant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_style: Option<FontStyle>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    /// Line-through.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strike: bool,
    /// Tracking: extra space after every character, in em of the font size
    /// (`0.1` = a tenth of the size). Spans may override it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f32>,
    /// Inline rich-text runs appended after `content` (see [`RawSpan`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<RawSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
    /// Line height as a multiple of `font_size` (unitless). Defaults to 1.2.
    /// (`stroke` / `stroke_width` / `join` below also apply to text: an
    /// outline painted behind the glyph fill.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f32>,
    /// `false`: never wrap — each paragraph (`\n`) is one line, laid out at
    /// its natural width (clamped to the space the parent gives, with a
    /// warning when the text then overflows its box). Default `true`: wrap
    /// to the box width. Meant for labels (a quarter-turned axis label
    /// longer than its axis stays one line instead of breaking).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    /// Make every line exactly as wide as the box (`"tracking"`: adjust the
    /// letter spacing, `"scale"`: scale the size). Each paragraph is one
    /// line, as with `wrap = false`. The box width is an explicit `width`,
    /// or one the parent imposes (stretch, a stack fill); a text sized by
    /// its content is left as is. See [`super::FitWidth`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit_width: Option<super::FitWidth>,
    /// Overrides `[page] script_metrics` for this text node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_metrics: Option<ScriptMetrics>,
    /// Overrides `[page] font_synthesis` (`"none"` / `"auto"` / `"bold"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_synthesis: Option<FontSynthesis>,
    /// Overrides `[page] synthetic_bold_width` (em of the glyph size).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthetic_bold_width: Option<f32>,
    /// Overrides `[page] synthetic_slant` (degrees).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthetic_slant: Option<f32>,

    // ---- image ----
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<ImageFit>,
    /// `false`: scale the image with nearest-neighbour sampling, so a
    /// low-resolution image keeps its blocky pixels (default `true`: smooth).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smooth: Option<bool>,
    /// Pixel density: the image's natural size is its pixels / `dpi` inches
    /// (overrides `[page] image_dpi`; `0` = none). An image with a natural
    /// size keeps it — it isn't stretched across its parent — unless it has
    /// an explicit `width` / `height` or a parent decides a side (a flex
    /// share, an equal-height row); a natural size bigger than the room
    /// shrinks to fit, keeping the aspect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<f32>,

    // ---- rect (`stroke` / `stroke_width` are shared with shapes and text) ----
    /// Fill paint: a colour (`"#rrggbb"`) or a gradient table (see
    /// [`crate::paint`]). On `rect` / `ellipse` / `path` it fills the shape;
    /// on `column` / `row` / `stack` it is the container's background,
    /// painted behind its children (with `stroke` / `stroke_width` /
    /// `corner_radius` / `dash` as its border).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<Paint>,
    /// Outline colour. On a `text` node: the glyph outline, painted *behind*
    /// the fill (SVG `paint-order="stroke"`), so a halo never thins the glyphs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Color>,
    /// Stroke width in the page unit (on text the outline reaches half of it
    /// past the glyph edges; it does not change the laid-out box).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f32>,
    /// Where the stroke of a closed outline lies (`rect`, `ellipse`, closed
    /// `path`, container border): `inside` (outer edge on the outline — the
    /// container default), `center` (the shape default) or `outside` (inner
    /// edge on the outline). A container's `clip` stays inside the border.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke_align: Option<StrokeAlign>,
    /// Corner radius in the page unit (`rect` and containers; `radius` is
    /// accepted as an alias).
    #[serde(alias = "radius", skip_serializing_if = "Option::is_none")]
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
    /// Line join at outline corners (`miter` / `round` / `bevel`): shapes
    /// default to `miter`, text outlines to `round`. Rect borders are always
    /// mitered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<LineJoin>,
    /// Arrowhead placement (`none` / `start` / `end` / `both`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<ArrowHead>,
    /// Arrowhead length in the page unit (default: proportional to the stroke width).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_size: Option<f32>,
    /// Arrowhead base width: a length in the page unit, or a multiple of the
    /// stroke width as `"3x"` (default `0.8 × head_size`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_width: Option<HeadWidth>,
    /// Arrowhead shape: `triangle` (default), `stealth`, `open`, `barb_left`,
    /// `barb_right`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_style: Option<HeadStyle>,

    // ---- connector (`stroke` / `stroke_width` / `dash` / `cap` / `join` /
    //      `head*` / `fill` / `corner_radius` above are shared) ----
    /// Connector: the id of the node it starts at.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Connector: the id of the node it ends at.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Connector: where on `from`'s box it starts — a side (`"left"`,
    /// `"right"`, `"top"`, `"bottom"`, a corner like `"top-left"`,
    /// `"center"`), `[fx, fy]` fractions of the box, or `"auto"` (default:
    /// the side facing the other node).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_at: Option<RawAttach>,
    /// Connector: where on `to`'s box it ends (as `from_at`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_at: Option<RawAttach>,
    /// Connector: attach to this part of `from`'s box, `[fx0, fy0, fx1,
    /// fy1]` fractions (a region of an image): `from_at` is then relative
    /// to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_box: Option<[f32; 4]>,
    /// Connector: the part of `to`'s box `to_at` refers to (as `from_box`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_box: Option<[f32; 4]>,
    /// Connector: `straight` (default), `elbow` (horizontal / vertical runs,
    /// corners rounded by `corner_radius`) or `curve` (a cubic leaving and
    /// entering along the sides' normals).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<ConnectorRoute>,
    /// Connector: keep the ends this far (page unit) off the attachment
    /// points — one number for both ends or `[start, end]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<RawGap>,
    /// Connector: draw a filled band this wide (page unit) along the route
    /// instead of a stroke — a block arrow (`head` puts triangular heads on
    /// it, `head_size` long, `head_width` across; default 1 × and 2 × the
    /// band). Filled with `fill` (default: the `stroke` colour, else black).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band: Option<f32>,
    /// Connector: the shape of `from`'s box (or `from_box`) — `box`
    /// (default) or `ellipse`: the region is the inscribed ellipse, so the
    /// `facing-1` / `facing-2` points lie on it (its extreme points across
    /// the facing direction) and the `outline` route draws it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_shape: Option<EndShape>,
}

/// The shape of a connector end's region (see [`RawNode::from_shape`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EndShape {
    #[default]
    Box,
    Ellipse,
}

impl EndShape {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "box" => EndShape::Box,
            "ellipse" => EndShape::Ellipse,
            _ => return None,
        })
    }
}

/// Where a connector attaches to a node's box: a named point or fractions.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RawAttach {
    /// `"auto"`, `"facing-1"` / `"facing-2"`, `"center"`, a side (`"left"`,
    /// …) or a corner (`"top-left"`, …).
    Named(String),
    /// `[fx, fy]`: fractions of the box (`[0, 0]` top-left, `[1, 0.5]` right middle).
    Frac([f32; 2]),
}

/// A connector's path between its two ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectorRoute {
    #[default]
    Straight,
    Elbow,
    Curve,
    /// A filled quadrilateral from the `facing-1` / `facing-2` points of
    /// `from` to those of `to` — a light / projection cone joining a region
    /// to its magnified copy (`fill`, optional `stroke` outline).
    Cone,
    /// No line between the ends: the outline of `from`'s box (or
    /// `from_box`) — its inscribed ellipse with `from_shape = "ellipse"`,
    /// else the rectangle (`fill` inside, `stroke` round it).
    Outline,
}

impl ConnectorRoute {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "straight" => ConnectorRoute::Straight,
            "elbow" => ConnectorRoute::Elbow,
            "curve" => ConnectorRoute::Curve,
            "cone" => ConnectorRoute::Cone,
            "outline" => ConnectorRoute::Outline,
            _ => return None,
        })
    }
}

/// A connector's end gaps: one length for both ends, or `[start, end]`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RawGap {
    Both(f32),
    Ends([f32; 2]),
}

impl RawGap {
    /// `[start, end]`.
    pub fn ends(self) -> [f32; 2] {
        match self {
            RawGap::Both(g) => [g, g],
            RawGap::Ends(e) => e,
        }
    }
}

/// `shadow = { dx, dy, blur, spread, color }` (lengths in the page unit; all
/// optional): offset on the page, blur radius (CSS semantics, σ = blur / 2),
/// growth of the silhouette before blurring, and colour with alpha (default
/// `#00000080`).
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawShadow {
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub dx: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub dy: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub blur: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub spread: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

/// `glow = { blur, color, spread }`: a shadow with no offset whose silhouette
/// is first grown by `spread` (default `blur / 2`), so the halo is nearly
/// opaque at the edges and fades out about `blur` past them. `color` defaults
/// to `#000000`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawGlow {
    pub blur: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spread: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
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
