//! Rich text: the inline **spans** of a `text` node and their per-span style
//! overrides, font specs (ordered family lists, optionally per script), and
//! the page/node typography switches (script metrics, synthetic bold/slant).
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

use std::fmt;
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
    /// the upright face slanted by the `synthetic_slant` (default
    /// [`SYNTHETIC_ITALIC_DEG`]).
    #[serde(alias = "oblique")]
    Italic,
}

/// Default slant of a synthesized italic, in degrees (the tops of glyphs lean
/// right). The value cosmic-text/swash use for their fake italic.
pub const SYNTHETIC_ITALIC_DEG: f32 = 14.0;

/// PowerPoint's synthetic-italic slant: a shear of exactly 1/3 (≈ 18.43°).
pub const OFFICE_SLANT_DEG: f32 = 18.434_948;

/// Superscript / subscript: a smaller glyph size on a shifted baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Script {
    #[serde(alias = "sup", alias = "superscript")]
    Super,
    #[serde(alias = "subscript")]
    Sub,
}

/// Size of a script span relative to its base size (`script_metrics =
/// "figs"`, the default).
pub const SCRIPT_SCALE: f32 = 0.65;
/// Baseline raise of a superscript, in em of the base size (`"figs"`).
pub const SUPER_SHIFT: f32 = 0.35;
/// Baseline shift of a subscript, in em of the base size (negative = down;
/// `"figs"`).
pub const SUB_SHIFT: f32 = -0.15;

/// Script size under `script_metrics = "office"`: PowerPoint and Word draw a
/// super- or subscript at 2/3 of the run's size.
pub const OFFICE_SCRIPT_SCALE: f32 = 2.0 / 3.0;
/// Office superscript raise: `baseline="30000"`, 30 % of the base size.
pub const OFFICE_SUPER_SHIFT: f32 = 0.30;
/// Office subscript drop: `baseline="-25000"`, 25 % of the base size.
pub const OFFICE_SUB_SHIFT: f32 = -0.25;

impl Script {
    /// Default baseline shift under the `"figs"` metrics, em of the base size
    /// (positive = up). See [`ScriptMetrics::shift`].
    pub fn shift(self) -> f32 {
        ScriptMetrics::Figs.shift(self)
    }
}

/// How `script = "super" | "sub"` spans are sized and shifted (page- or
/// node-level `script_metrics`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptMetrics {
    /// × [`SCRIPT_SCALE`], [`SUPER_SHIFT`] / [`SUB_SHIFT`] em: figs' own.
    #[default]
    Figs,
    /// PowerPoint / Word: × 2/3, superscript +30 %, subscript −25 % of the
    /// base size.
    Office,
}

impl ScriptMetrics {
    /// Glyph size of a script relative to its base size.
    pub fn scale(self) -> f32 {
        match self {
            ScriptMetrics::Figs => SCRIPT_SCALE,
            ScriptMetrics::Office => OFFICE_SCRIPT_SCALE,
        }
    }

    /// Baseline shift of `script`, em of the base size (positive = up).
    pub fn shift(self, script: Script) -> f32 {
        match (self, script) {
            (ScriptMetrics::Figs, Script::Super) => SUPER_SHIFT,
            (ScriptMetrics::Figs, Script::Sub) => SUB_SHIFT,
            (ScriptMetrics::Office, Script::Super) => OFFICE_SUPER_SHIFT,
            (ScriptMetrics::Office, Script::Sub) => OFFICE_SUB_SHIFT,
        }
    }
}

/// When bold is drawn by thickening glyphs with a same-colour outline
/// (`font_synthesis`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FontSynthesis {
    /// Never: bold uses the nearest installed weight, so a family without a
    /// bold face stays regular.
    #[default]
    None,
    /// Runs asking for weight ≥ 600 that land on a face lighter than 600 (the
    /// family has no bold: MS Mincho, Arial Unicode MS, …) are emboldened —
    /// what PowerPoint and browsers do.
    Auto,
    /// Every glyph is emboldened, whatever its weight (on top of a real bold
    /// face if one was chosen).
    Bold,
}

/// Default synthetic-bold stroke width, em of the glyph size. PowerPoint's
/// faux bold is a same-colour outline `size / 35` wide: its PDF export writes
/// such text in fill + stroke mode with a line width of 0.0286 × size, at
/// every size.
pub const SYNTHETIC_BOLD_WIDTH: f32 = 1.0 / 35.0;

/// Text rendering choices set on the page (the default for every text node)
/// or per text node: script metrics, synthetic bold and synthetic slant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextTypography {
    pub script_metrics: ScriptMetrics,
    pub font_synthesis: FontSynthesis,
    /// Synthetic-bold stroke width, em of each glyph's size.
    pub synthetic_bold_width: f32,
    /// Slant of a synthetic italic, degrees.
    pub synthetic_slant: f32,
}

impl Default for TextTypography {
    fn default() -> Self {
        TextTypography {
            script_metrics: ScriptMetrics::Figs,
            font_synthesis: FontSynthesis::None,
            synthetic_bold_width: SYNTHETIC_BOLD_WIDTH,
            synthetic_slant: SYNTHETIC_ITALIC_DEG,
        }
    }
}

impl TextTypography {
    /// `tan` of the synthetic-italic slant: a glyph point `h` above the
    /// baseline moves `h × shear()` to the right.
    pub fn shear(&self) -> f32 {
        self.synthetic_slant.to_radians().tan()
    }
}

/// One inline span as written in TOML (`spans = [{ text = "…", … }]`).
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawSpan {
    pub text: String,
    /// A font spec for this span (see [`FontSpec`]): a plain list replaces
    /// the node's; a per-script table overrides only the slots it names.
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
    /// Outline colour of this span's glyphs, painted behind the fill like a
    /// node's text `stroke` (and replacing it for the span).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Color>,
    /// Outline width in the page unit; `0` switches an inherited outline off.
    /// Default: the node's stroke width, else 1 pt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f32>,
}

impl RawSpan {
    /// A span of plain text inheriting everything.
    pub fn new(text: impl Into<String>) -> Self {
        RawSpan {
            text: text.into(),
            ..Default::default()
        }
    }

    /// The span's overrides, without its text (`stroke_width` still in the
    /// page unit; resolve converts it to points).
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
            stroke: self.stroke,
            stroke_width: self.stroke_width,
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
    /// Outline colour (see [`RawSpan::stroke`]).
    pub stroke: Option<Color>,
    /// Outline width, **points** once resolved into a [`TextSpan`].
    pub stroke_width: Option<f32>,
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

/// The script slots of a [`FontSpec`] (PowerPoint's Latin / East Asian /
/// symbol fonts).
pub const FONT_SLOTS: [&str; 4] = ["latin", "cjk", "symbol", "fallback"];

/// A font spec: an ordered family list, optionally split by script.
///
/// - **Plain list** — `"Arial, Yu Gothic"`: every character uses the first
///   listed family that has a glyph for it.
/// - **Per script** — `"latin: Arial; cjk: Yu Gothic, MS Mincho; fallback:
///   Segoe UI Symbol"` (TOML `{ latin = "Arial", cjk = "Yu Gothic" }`, Rhai
///   `#{ latin: "Arial", cjk: "Yu Gothic" }`): a character's *script class*
///   (see [`crate::text::char_class`]) picks its slot's list first, then
///   `fallback`, then the other slots. `symbol` takes the ambiguous symbols
///   and punctuation (`× — ○ ■ → ①`), which otherwise follow the
///   neighbouring text.
///
/// Inheritance (page → node → span): a plain list **replaces** the inherited
/// spec; a per-script spec **overrides only the slots it names**.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontSpec {
    pub latin: Option<Vec<String>>,
    pub cjk: Option<Vec<String>>,
    pub symbol: Option<Vec<String>>,
    /// The plain list, or the `fallback` slot.
    pub fallback: Vec<String>,
    /// Written per script (overlays slot by slot when inherited).
    pub scoped: bool,
}

impl FontSpec {
    /// A plain ordered list.
    pub fn list(names: Vec<String>) -> Self {
        FontSpec {
            fallback: names,
            ..Default::default()
        }
    }

    /// Parse a spec string: a plain comma list, or `slot: list` segments
    /// separated by `;` (slots `latin`, `cjk`, `symbol`, `fallback`; a bare
    /// segment is the fallback). Errors name an unknown slot.
    pub fn parse(s: &str) -> Result<FontSpec, String> {
        let mut spec = FontSpec::default();
        for seg in s.split(';') {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            let slot = seg.split_once(':').and_then(|(k, v)| {
                let k = k.trim();
                let ident = !k.is_empty()
                    && k.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
                ident.then_some((k, v))
            });
            let Some((key, value)) = slot else {
                spec.fallback.extend(parse_family_list(seg));
                continue;
            };
            let names = parse_family_list(value);
            spec.scoped = true;
            match key.to_ascii_lowercase().as_str() {
                "latin" => spec.latin = Some(names),
                "cjk" | "ea" | "east_asian" => spec.cjk = Some(names),
                "symbol" | "sym" => spec.symbol = Some(names),
                "fallback" | "other" => spec.fallback = names,
                _ => {
                    return Err(format!(
                        "unknown font slot `{key}` (expected latin, cjk, symbol or fallback)"
                    ))
                }
            }
        }
        Ok(spec)
    }

    /// [`FontSpec::parse`], reading an invalid spec as a plain list.
    pub fn parse_lenient(s: &str) -> FontSpec {
        FontSpec::parse(s).unwrap_or_else(|_| FontSpec::list(parse_family_list(s)))
    }

    /// `top` inherited over `self`: a plain `top` replaces it, a per-script
    /// `top` overrides the slots it names (and the fallback if non-empty).
    pub fn overlay(&self, top: &FontSpec) -> FontSpec {
        if !top.scoped {
            return top.clone();
        }
        FontSpec {
            latin: top.latin.clone().or_else(|| self.latin.clone()),
            cjk: top.cjk.clone().or_else(|| self.cjk.clone()),
            symbol: top.symbol.clone().or_else(|| self.symbol.clone()),
            fallback: if top.fallback.is_empty() {
                self.fallback.clone()
            } else {
                top.fallback.clone()
            },
            scoped: true,
        }
    }

    /// No family named anywhere.
    pub fn is_empty(&self) -> bool {
        self.fallback.is_empty()
            && [&self.latin, &self.cjk, &self.symbol]
                .iter()
                .all(|s| s.as_ref().is_none_or(Vec::is_empty))
    }

    /// Every family named, in slot order (latin, cjk, symbol, fallback),
    /// without duplicates.
    pub fn all(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        let slots = [&self.latin, &self.cjk, &self.symbol];
        for name in slots.into_iter().flatten().flatten().chain(&self.fallback) {
            if !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                out.push(name);
            }
        }
        out
    }
}

impl fmt::Display for FontSpec {
    /// The canonical string: `"A, B"` for a plain list, else `"latin: A;
    /// cjk: B; fallback: C"` (slots in that order, unset ones omitted).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.scoped {
            return f.write_str(&self.fallback.join(", "));
        }
        let slots = [
            ("latin", self.latin.as_ref()),
            ("cjk", self.cjk.as_ref()),
            ("symbol", self.symbol.as_ref()),
            (
                "fallback",
                (!self.fallback.is_empty()).then_some(&self.fallback),
            ),
        ];
        let parts: Vec<String> = slots
            .into_iter()
            .filter_map(|(k, v)| v.map(|v| format!("{k}: {}", v.join(", "))))
            .collect();
        f.write_str(&parts.join("; "))
    }
}

/// A family list in TOML: a string (`"Arial, Yu Gothic"`), an array of
/// names (`["Arial", "Yu Gothic"]`), or a per-script table (`{ latin =
/// "Arial", cjk = ["Yu Gothic", "MS Mincho"] }`), normalised to the spec
/// string (see [`FontSpec`]).
#[derive(Deserialize)]
#[serde(untagged)]
enum FamilyListRepr {
    One(String),
    Many(Vec<String>),
    Slots(FamilySlots),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FamilySlots {
    latin: Option<NamesRepr>,
    cjk: Option<NamesRepr>,
    symbol: Option<NamesRepr>,
    fallback: Option<NamesRepr>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NamesRepr {
    One(String),
    Many(Vec<String>),
}

impl NamesRepr {
    fn names(self) -> Vec<String> {
        match self {
            NamesRepr::One(s) => parse_family_list(&s),
            NamesRepr::Many(v) => v,
        }
    }
}

/// `deserialize_with` for `Option<String>` family-list fields.
pub fn family_list_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<FamilyListRepr>::deserialize(d)? {
        None => None,
        Some(FamilyListRepr::One(s)) => Some(s),
        Some(FamilyListRepr::Many(v)) => Some(v.join(", ")),
        Some(FamilyListRepr::Slots(t)) => Some(
            FontSpec {
                latin: t.latin.map(NamesRepr::names),
                cjk: t.cjk.map(NamesRepr::names),
                symbol: t.symbol.map(NamesRepr::names),
                fallback: t.fallback.map(NamesRepr::names).unwrap_or_default(),
                scoped: true,
            }
            .to_string(),
        ),
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
    fn font_specs_parse_overlay_and_print() {
        let plain = FontSpec::parse("Arial, Yu Gothic").unwrap();
        assert!(!plain.scoped);
        assert_eq!(plain.fallback, vec!["Arial", "Yu Gothic"]);
        assert_eq!(plain.to_string(), "Arial, Yu Gothic");

        let s = FontSpec::parse("latin: Arial; cjk: 'MS Mincho', Yu Gothic").unwrap();
        assert!(s.scoped);
        assert_eq!(s.latin.as_deref(), Some(&["Arial".to_string()][..]));
        assert_eq!(s.cjk.as_ref().unwrap().len(), 2);
        assert_eq!(s.to_string(), "latin: Arial; cjk: MS Mincho, Yu Gothic");
        assert_eq!(FontSpec::parse(&s.to_string()).unwrap(), s);

        // Slot-wise overlay; a plain list replaces.
        let node = plain.overlay(&FontSpec::parse("cjk: MS Mincho").unwrap());
        assert_eq!(node.fallback, vec!["Arial", "Yu Gothic"]);
        assert_eq!(node.cjk.as_ref().unwrap(), &vec!["MS Mincho".to_string()]);
        assert_eq!(node.overlay(&FontSpec::parse("Times").unwrap()).latin, None);

        assert!(FontSpec::parse("latim: Arial")
            .unwrap_err()
            .contains("latim"));
        assert!(FontSpec::parse("").unwrap().is_empty());
        assert_eq!(s.all(), vec!["Arial", "MS Mincho", "Yu Gothic"]);
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
                { text = "z", font_family = { latin = "Arial", cjk = ["Yu Gothic", "MS Mincho"] }, stroke = "#ffffff", stroke_width = 0.1 },
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
        assert_eq!(
            t.spans[4].font_family.as_deref(),
            Some("latin: Arial; cjk: Yu Gothic, MS Mincho")
        );
        assert_eq!(t.spans[4].stroke_width, Some(0.1));
        assert!(
            toml::from_str::<T>(r#"spans = [{ text = "a", font_family = { latn = "A" } }]"#)
                .is_err()
        );
    }
}
