//! Rich text: the inline **spans** of a `text` node and their per-span style
//! overrides, plus font-family lists.
//!
//! ```toml
//! [nodes.dg]
//! type = "text"
//! font_family = "Arial, Yu Gothic"      # ordered fallback list (or an array)
//! spans = [
//!   { text = "ΔG" },
//!   { text = "300K", script = "sub" },
//!   { text = " = " },
//!   { text = "−27.5", color = "#e0635a", font_weight = 700 },
//!   { text = " kJ/mol" },
//! ]
//! ```
//!
//! Every override is optional; an unset one inherits the node's text style.
//! The node's `content` (if any) is an unstyled leading run before the spans.

use std::ops::Range;

use serde::{Deserialize, Deserializer, Serialize};

use crate::geom::Color;

/// Font style of a text node or span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    #[default]
    Normal,
    /// A real italic (or oblique) face of the family when it has one, else
    /// the upright face slanted by [`SYNTHETIC_ITALIC_DEG`].
    #[serde(alias = "oblique")]
    Italic,
}

/// Slant of a synthesized italic, in degrees (the tops of glyphs lean right).
/// The same value cosmic-text/swash use for their fake italic, so every
/// backend draws the identical shear.
pub const SYNTHETIC_ITALIC_DEG: f32 = 14.0;

/// Superscript / subscript: a smaller glyph size on a shifted baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Script {
    #[serde(alias = "sup", alias = "superscript")]
    Super,
    #[serde(alias = "subscript")]
    Sub,
}

/// Size of a script span relative to its base size.
pub const SCRIPT_SCALE: f32 = 0.65;
/// Baseline raise of a superscript, in em of the base size.
pub const SUPER_SHIFT: f32 = 0.35;
/// Baseline shift of a subscript, in em of the base size (negative = down).
pub const SUB_SHIFT: f32 = -0.15;

impl Script {
    /// Default baseline shift, em of the base size (positive = up).
    pub fn shift(self) -> f32 {
        match self {
            Script::Super => SUPER_SHIFT,
            Script::Sub => SUB_SHIFT,
        }
    }
}

/// One inline span as written in TOML (`spans = [{ text = "…", … }]`).
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawSpan {
    pub text: String,
    /// Replaces (does not extend) the node's family list for this span.
    #[serde(
        default,
        deserialize_with = "family_list_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub font_family: Option<String>,
    /// Points. With `script`, the *base* size the script is derived from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(default, alias = "style", skip_serializing_if = "Option::is_none")]
    pub font_style: Option<FontStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<Script>,
    /// Exact baseline shift in em of the base size (positive = up); overrides
    /// the `script` default shift.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_shift: Option<f32>,
    /// Tracking in em of the span's font size (see [`TextProps::letter_spacing`](super::TextProps::letter_spacing)).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f32>,
}

impl RawSpan {
    /// A span of plain text inheriting everything.
    pub fn new(text: impl Into<String>) -> Self {
        RawSpan {
            text: text.into(),
            ..Default::default()
        }
    }

    /// The span's overrides, without its text.
    pub fn style(&self) -> SpanStyle {
        SpanStyle {
            font_family: self.font_family.clone(),
            font_size: self.font_size,
            font_weight: self.font_weight,
            font_style: self.font_style,
            color: self.color,
            underline: self.underline,
            strike: self.strike,
            script: self.script,
            baseline_shift: self.baseline_shift,
            letter_spacing: self.letter_spacing,
        }
    }
}

/// Per-span style overrides (`None` = inherit the node's).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpanStyle {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
    pub color: Option<Color>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub script: Option<Script>,
    pub baseline_shift: Option<f32>,
    pub letter_spacing: Option<f32>,
}

/// A resolved span: a byte range of the node's full text plus its overrides.
/// Bytes not covered by any span use the node style.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    pub range: Range<usize>,
    pub style: SpanStyle,
}

/// Split a font-family list (`"Arial, 'Yu Gothic'"`) into names: comma
/// separated, trimmed, surrounding quotes removed, empties dropped.
pub fn parse_family_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|f| {
            let f = f.trim();
            let unquoted = f
                .strip_prefix('"')
                .and_then(|f| f.strip_suffix('"'))
                .or_else(|| f.strip_prefix('\'').and_then(|f| f.strip_suffix('\'')))
                .unwrap_or(f);
            unquoted.trim().to_string()
        })
        .filter(|f| !f.is_empty())
        .collect()
}

/// A family list in TOML: a string (`"Arial, Yu Gothic"`) or an array of
/// names (`["Arial", "Yu Gothic"]`), normalised to the comma-joined string.
#[derive(Deserialize)]
#[serde(untagged)]
enum FamilyListRepr {
    One(String),
    Many(Vec<String>),
}

/// `deserialize_with` for `Option<String>` family-list fields.
pub fn family_list_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<FamilyListRepr>::deserialize(d)? {
        None => None,
        Some(FamilyListRepr::One(s)) => Some(s),
        Some(FamilyListRepr::Many(v)) => Some(v.join(", ")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_lists_split_and_unquote() {
        assert_eq!(
            parse_family_list(" Arial , 'Yu Gothic',\"MS Mincho\",, "),
            vec!["Arial", "Yu Gothic", "MS Mincho"]
        );
        assert!(parse_family_list("").is_empty());
    }

    #[test]
    fn spans_parse_from_toml() {
        #[derive(Deserialize)]
        struct T {
            spans: Vec<RawSpan>,
        }
        let t: T = toml::from_str(
            r##"spans = [
                { text = "ΔG" },
                { text = "300K", script = "sub" },
                { text = "x", style = "italic", font_family = ["Times New Roman", "MS Mincho"] },
                { text = "y", script = "sup", baseline_shift = 0.5, letter_spacing = 0.1 },
            ]"##,
        )
        .unwrap();
        assert_eq!(t.spans[1].script, Some(Script::Sub));
        assert_eq!(t.spans[2].font_style, Some(FontStyle::Italic));
        assert_eq!(
            t.spans[2].font_family.as_deref(),
            Some("Times New Roman, MS Mincho")
        );
        assert_eq!(t.spans[3].baseline_shift, Some(0.5));
    }
}
