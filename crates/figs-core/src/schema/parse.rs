//! Raw deserialization layer: TOML text -> [`RawDocument`].
//!
//! Each node is deserialized into a single [`RawNode`] carrying `type` plus
//! every possible field as an `Option`. Type-specific validation (e.g. "an
//! image needs `src`") happens later in [`super::resolve`], so that errors can
//! be reported with the offending node id.

use std::collections::HashMap;

use serde::Deserialize;

use crate::geom::{Color, CrossAxisAlign, Edges, ImageFit, MainAxisAlign, TextAlign};
use crate::units::Unit;

/// The whole document as written in TOML, before validation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawDocument {
    pub page: RawPage,
    /// Flat map of node id -> node. Tree structure lives in each node's
    /// `children` field.
    #[serde(default)]
    pub nodes: HashMap<String, RawNode>,
}

/// The `[page]` table.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPage {
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub unit: Unit,
    /// Id of the root node within `nodes`.
    pub root: String,
    /// Raster resolution for PNG output. Defaults to 300 in resolve.
    #[serde(default)]
    pub dpi: Option<f32>,
    #[serde(default)]
    pub background: Option<Color>,
}

/// The discriminant of a node, from its `type` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Column,
    Row,
    Image,
    Text,
    Rect,
}

/// A single node as written in TOML. All type-specific fields are optional;
/// which ones are required depends on `kind` and is checked in resolve.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawNode {
    #[serde(rename = "type")]
    pub kind: NodeKind,

    // ---- shared layout properties (all node types) ----
    pub flex: Option<f32>,
    /// Explicit fixed width, in the page unit.
    pub width: Option<f32>,
    /// Explicit fixed height, in the page unit.
    pub height: Option<f32>,
    /// width / height; derives the other dimension when one is known.
    pub aspect_ratio: Option<f32>,
    #[serde(default)]
    pub margin: Edges,
    #[serde(default)]
    pub padding: Edges,

    // ---- container (column / row) ----
    #[serde(default)]
    pub children: Vec<String>,
    /// Gap between children, in the page unit.
    #[serde(default)]
    pub spacing: f32,
    pub main_axis_alignment: Option<MainAxisAlign>,
    pub cross_axis_alignment: Option<CrossAxisAlign>,

    // ---- text ----
    pub content: Option<String>,
    /// Font size in points (never affected by the page unit).
    pub font_size: Option<f32>,
    pub font_family: Option<String>,
    pub font_weight: Option<u16>,
    pub color: Option<Color>,
    pub align: Option<TextAlign>,
    /// Line height as a multiple of `font_size` (unitless). Defaults to 1.2.
    pub line_height: Option<f32>,

    // ---- image ----
    pub src: Option<String>,
    pub fit: Option<ImageFit>,

    // ---- rect ----
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    /// Stroke width in the page unit.
    pub stroke_width: Option<f32>,
    /// Corner radius in the page unit.
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
