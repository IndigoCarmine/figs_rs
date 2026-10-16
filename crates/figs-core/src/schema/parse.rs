//! Raw deserialization layer: TOML text -> [`RawDocument`].
//!
//! Each node is deserialized into a single [`RawNode`] carrying `type` plus
//! every possible field as an `Option`. Type-specific validation (e.g. "an
//! image needs `src`") happens later in [`super::resolve`], so that errors can
//! be reported with the offending node id.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::geom::{Color, CrossAxisAlign, Edges, ImageFit, MainAxisAlign, TextAlign};
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
}

/// Error from the raw TOML parsing stage.
#[derive(Debug, thiserror::Error)]
#[error("failed to parse TOML document")]
pub struct ParseError(#[from] pub toml::de::Error);

/// Parse TOML source text into a [`RawDocument`].
pub fn parse_str(src: &str) -> Result<RawDocument, ParseError> {
    Ok(toml::from_str(src)?)
}
