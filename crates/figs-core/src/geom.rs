//! Shared geometry and style primitives used across schema, layout and render.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use crate::paint::{Brush, GradientStop, LinearDir, Paint};

/// A width/height pair, in points.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

impl Size {
    pub fn new(w: f32, h: f32) -> Self {
        Size { w, h }
    }
}

/// An axis-aligned rectangle with a top-left origin, in points.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }

    pub fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }

    /// Shrink this rect inwards by the given edge insets, clamping to non-negative size.
    pub fn inset(&self, e: Edges) -> Rect {
        Rect {
            x: self.x + e.left,
            y: self.y + e.top,
            w: (self.w - e.left - e.right).max(0.0),
            h: (self.h - e.top - e.bottom).max(0.0),
        }
    }
}

/// Per-side insets (margin/padding), in the page unit at parse time, points after conversion.
///
/// Accepts a scalar (`padding = 1.5`, all four sides), a table
/// (`padding = { top = 1, left = 2 }`, missing sides default to 0; `x` / `y`
/// set left+right / top+bottom, and a named side overrides them), or an array
/// in CSS order: `[all]`, `[vertical, horizontal]`, `[top, horizontal,
/// bottom]` or `[top, right, bottom, left]`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// The named sides of an [`Edges`] table / map, before defaults (see
/// [`Edges::from_sides`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeSides {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
    /// Shorthand for `left` and `right`.
    pub x: Option<f32>,
    /// Shorthand for `top` and `bottom`.
    pub y: Option<f32>,
}

impl EdgeSides {
    /// The accepted keys.
    pub const KEYS: [&'static str; 6] = ["top", "right", "bottom", "left", "x", "y"];

    /// The slot for key `k` (one of [`EdgeSides::KEYS`]).
    pub fn slot(&mut self, k: &str) -> Option<&mut Option<f32>> {
        Some(match k {
            "top" => &mut self.top,
            "right" => &mut self.right,
            "bottom" => &mut self.bottom,
            "left" => &mut self.left,
            "x" => &mut self.x,
            "y" => &mut self.y,
            _ => return None,
        })
    }
}

impl Edges {
    pub fn all(v: f32) -> Self {
        Edges {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// Edges from 1–4 values in CSS order: `[all]`, `[vertical,
    /// horizontal]`, `[top, horizontal, bottom]`, `[top, right, bottom,
    /// left]`. `None` for any other length.
    pub fn from_css(v: &[f32]) -> Option<Self> {
        let (top, right, bottom, left) = match *v {
            [a] => (a, a, a, a),
            [y, x] => (y, x, y, x),
            [t, x, b] => (t, x, b, x),
            [t, r, b, l] => (t, r, b, l),
            _ => return None,
        };
        Some(Edges {
            top,
            right,
            bottom,
            left,
        })
    }

    /// Edges from named sides: `x` / `y` set both horizontal / vertical
    /// sides, and an explicitly named side overrides them; unset sides are 0.
    pub fn from_sides(s: EdgeSides) -> Self {
        Edges {
            top: s.top.or(s.y).unwrap_or(0.0),
            right: s.right.or(s.x).unwrap_or(0.0),
            bottom: s.bottom.or(s.y).unwrap_or(0.0),
            left: s.left.or(s.x).unwrap_or(0.0),
        }
    }

    /// Scale every side (used for unit conversion).
    pub fn scaled(self, factor: f32) -> Self {
        Edges {
            top: self.top * factor,
            right: self.right * factor,
            bottom: self.bottom * factor,
            left: self.left * factor,
        }
    }

    /// True when every side is zero (the default). Used to skip empty
    /// margin/padding when serializing back to TOML.
    pub fn is_zero(&self) -> bool {
        self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0 && self.left == 0.0
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

impl<'de> Deserialize<'de> for Edges {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Edges;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(
                    "a number, a table of top/right/bottom/left/x/y, or an array of 1 to 4 numbers",
                )
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Edges, E> {
                Ok(Edges::all(v as f32))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Edges, E> {
                Ok(Edges::all(v as f32))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Edges, E> {
                Ok(Edges::all(v as f32))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Edges, A::Error> {
                let mut v = Vec::new();
                while let Some(x) = seq.next_element::<f32>()? {
                    v.push(x);
                }
                Edges::from_css(&v).ok_or_else(|| {
                    serde::de::Error::invalid_length(v.len(), &"1 to 4 numbers (CSS order)")
                })
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Edges, A::Error> {
                let mut sides = EdgeSides::default();
                while let Some(k) = map.next_key::<String>()? {
                    let slot = sides
                        .slot(&k)
                        .ok_or_else(|| serde::de::Error::unknown_field(&k, &EdgeSides::KEYS))?;
                    *slot = Some(map.next_value::<f32>()?);
                }
                Ok(Edges::from_sides(sides))
            }
        }
        deserializer.deserialize_any(V)
    }
}

impl Serialize for Edges {
    /// Emit a scalar when all four sides are equal, otherwise a table holding
    /// only the non-zero sides. Round-trips with the `Deserialize` impl above.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        if self.top == self.right && self.right == self.bottom && self.bottom == self.left {
            return serializer.serialize_f32(self.top);
        }
        let sides = [
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
            ("left", self.left),
        ];
        let len = sides.iter().filter(|(_, v)| *v != 0.0).count();
        let mut map = serializer.serialize_map(Some(len))?;
        for (k, v) in sides {
            if v != 0.0 {
                map.serialize_entry(k, &v)?;
            }
        }
        map.end()
    }
}

/// An sRGB color with alpha, components in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    /// Parse `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (leading `#` optional).
    pub fn parse_hex(s: &str) -> Result<Color, ColorParseError> {
        let h = s.strip_prefix('#').unwrap_or(s);
        let expand = |c: u8| (c << 4) | c; // 0xA -> 0xAA
        let hex = |c: char| c.to_digit(16).map(|d| d as u8);

        let chars: Vec<char> = h.chars().collect();
        let bytes = |a: char, b: char| -> Option<u8> { Some((hex(a)? << 4) | hex(b)?) };

        let (r, g, b, a) = match chars.len() {
            3 => {
                let r = expand(hex(chars[0]).ok_or(ColorParseError)?);
                let g = expand(hex(chars[1]).ok_or(ColorParseError)?);
                let b = expand(hex(chars[2]).ok_or(ColorParseError)?);
                (r, g, b, 255)
            }
            4 => {
                let r = expand(hex(chars[0]).ok_or(ColorParseError)?);
                let g = expand(hex(chars[1]).ok_or(ColorParseError)?);
                let b = expand(hex(chars[2]).ok_or(ColorParseError)?);
                let a = expand(hex(chars[3]).ok_or(ColorParseError)?);
                (r, g, b, a)
            }
            6 => {
                let r = bytes(chars[0], chars[1]).ok_or(ColorParseError)?;
                let g = bytes(chars[2], chars[3]).ok_or(ColorParseError)?;
                let b = bytes(chars[4], chars[5]).ok_or(ColorParseError)?;
                (r, g, b, 255)
            }
            8 => {
                let r = bytes(chars[0], chars[1]).ok_or(ColorParseError)?;
                let g = bytes(chars[2], chars[3]).ok_or(ColorParseError)?;
                let b = bytes(chars[4], chars[5]).ok_or(ColorParseError)?;
                let a = bytes(chars[6], chars[7]).ok_or(ColorParseError)?;
                (r, g, b, a)
            }
            _ => return Err(ColorParseError),
        };

        Ok(Color {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        })
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid color: expected hex like #rrggbb")]
pub struct ColorParseError;

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Color::parse_hex(&s).map_err(serde::de::Error::custom)
    }
}

impl Color {
    /// Format as `#rrggbb`, or `#rrggbbaa` when not fully opaque. Round-trips
    /// with [`Color::parse_hex`].
    pub fn to_hex(&self) -> String {
        let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let (r, g, b, a) = (to_u8(self.r), to_u8(self.g), to_u8(self.b), to_u8(self.a));
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

/// Distribution of children along a container's main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MainAxisAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Alignment of children along a container's cross axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
    Stretch,
    /// Rows: line up the children's first text baselines; a child with no
    /// baseline (anything but a text node, or a quarter-turned text) sits on
    /// it with its bottom edge. In a column it acts as `start`.
    Baseline,
}

/// How a node distributes its children. Determines the main/cross axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Column: main axis is vertical.
    Vertical,
    /// Row: main axis is horizontal.
    Horizontal,
}

/// Horizontal alignment of text within its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// Which line's baseline an anchor refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineKind {
    /// The first line's baseline (`"baseline"`).
    First,
    /// The last line's baseline (`"last_baseline"`).
    Last,
}

impl BaselineKind {
    /// The schema spelling: `"baseline"` / `"last_baseline"`.
    pub fn name(self) -> &'static str {
        match self {
            BaselineKind::First => "baseline",
            BaselineKind::Last => "last_baseline",
        }
    }

    /// Parse `"baseline"` / `"last_baseline"`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "baseline" => Some(BaselineKind::First),
            "last_baseline" => Some(BaselineKind::Last),
            _ => None,
        }
    }
}

/// One component of an `anchor`: a fraction of the footprint along that
/// axis, or a text baseline (`"baseline"` / `"last_baseline"`): where that
/// baseline crosses the axis, in the parent's frame (see
/// [`crate::schema::RawNode::anchor`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnchorValue {
    Frac(f32),
    Baseline(BaselineKind),
}

impl Default for AnchorValue {
    fn default() -> Self {
        AnchorValue::Frac(0.0)
    }
}

impl From<f32> for AnchorValue {
    fn from(v: f32) -> Self {
        AnchorValue::Frac(v)
    }
}

impl AnchorValue {
    /// The fraction, when this is one.
    pub fn frac(self) -> Option<f32> {
        match self {
            AnchorValue::Frac(f) => Some(f),
            AnchorValue::Baseline(_) => None,
        }
    }

    /// The baseline, when this is one.
    pub fn baseline(self) -> Option<BaselineKind> {
        match self {
            AnchorValue::Baseline(k) => Some(k),
            AnchorValue::Frac(_) => None,
        }
    }
}

impl Serialize for AnchorValue {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            AnchorValue::Frac(f) => s.serialize_f32(*f),
            AnchorValue::Baseline(k) => s.serialize_str(k.name()),
        }
    }
}

impl<'de> Deserialize<'de> for AnchorValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = AnchorValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#"a fraction, "baseline" or "last_baseline""#)
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<AnchorValue, E> {
                Ok(AnchorValue::Frac(v as f32))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<AnchorValue, E> {
                Ok(AnchorValue::Frac(v as f32))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<AnchorValue, E> {
                Ok(AnchorValue::Frac(v as f32))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<AnchorValue, E> {
                BaselineKind::parse(v)
                    .map(AnchorValue::Baseline)
                    .ok_or_else(|| E::invalid_value(serde::de::Unexpected::Str(v), &self))
            }
        }
        d.deserialize_any(V)
    }
}

/// Where a closed outline's stroke lies relative to the outline (rect,
/// ellipse, closed path, container border): `inside` (its outer edge on the
/// outline), `center` (centred on it, as in SVG) or `outside` (its inner edge
/// on it). Containers default to `inside`, shapes to `center`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StrokeAlign {
    Inside,
    #[default]
    Center,
    Outside,
}

impl StrokeAlign {
    /// How far the stroke's centre line lies *outside* the outline, for a
    /// stroke `width` wide: `-width / 2`, `0` or `width / 2`.
    pub fn centre_offset(self, width: f32) -> f32 {
        match self {
            StrokeAlign::Inside => -width / 2.0,
            StrokeAlign::Center => 0.0,
            StrokeAlign::Outside => width / 2.0,
        }
    }

    /// How far the stroke reaches past the outline, outwards.
    pub fn outer_extent(self, width: f32) -> f32 {
        self.centre_offset(width) + width / 2.0
    }

    /// How far the stroke reaches inside the outline.
    pub fn inner_extent(self, width: f32) -> f32 {
        width / 2.0 - self.centre_offset(width)
    }

    /// Parse `"inside"` / `"center"` / `"outside"`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "inside" => Some(StrokeAlign::Inside),
            "center" => Some(StrokeAlign::Center),
            "outside" => Some(StrokeAlign::Outside),
            _ => None,
        }
    }
}

/// How an image is fitted into its computed box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
    Fill,
}

/// A 2-D point, in points (top-left origin) once resolved.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// A 2-D affine transform in page space (points, top-left origin, y down),
/// in SVG's `matrix(a b c d e f)` convention:
///
/// ```text
/// x' = a·x + c·y + e
/// y' = b·x + d·y + f
/// ```
///
/// The layout IR carries one per rotated node ([`ComputedNode::transform`]);
/// backends hand the six numbers straight to tiny-skia / krilla
/// (`Transform::from_row(a, b, c, d, e, f)`) or SVG.
///
/// [`ComputedNode::transform`]: crate::layout::ComputedNode::transform
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// A rotation by `degrees` **clockwise on the page** (y points down, so
    /// this is the usual rotation matrix) about the point (`cx`, `cy`).
    /// Multiples of 90° use exact 0/±1 coefficients, so quarter turns carry no
    /// floating-point noise.
    pub fn rotation_about(degrees: f32, cx: f32, cy: f32) -> Affine {
        let (sin, cos) = match quarter_turns(degrees) {
            Some(0) => (0.0, 1.0),
            Some(1) => (1.0, 0.0),
            Some(2) => (0.0, -1.0),
            Some(_) => (-1.0, 0.0),
            None => {
                let (s, c) = (degrees as f64).to_radians().sin_cos();
                (s as f32, c as f32)
            }
        };
        // Translate to the origin, rotate, translate back.
        Affine {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: cx - cos * cx + sin * cy,
            f: cy - sin * cx - cos * cy,
        }
    }

    /// `self` followed by `outer`: the transform that applies `self` first,
    /// then `outer` (i.e. `outer ∘ self`).
    pub fn then(self, outer: Affine) -> Affine {
        let (m, n) = (outer, self);
        Affine {
            a: m.a * n.a + m.c * n.b,
            b: m.b * n.a + m.d * n.b,
            c: m.a * n.c + m.c * n.d,
            d: m.b * n.c + m.d * n.d,
            e: m.a * n.e + m.c * n.f + m.e,
            f: m.b * n.e + m.d * n.f + m.f,
        }
    }

    /// Map a point.
    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    /// The inverse transform, or `None` when singular.
    pub fn invert(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Affine {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }

    /// True when the transform moves nothing (within `1e-6`).
    pub fn is_identity(&self) -> bool {
        let close = |x: f32, y: f32| (x - y).abs() <= 1e-6;
        close(self.a, 1.0)
            && close(self.b, 0.0)
            && close(self.c, 0.0)
            && close(self.d, 1.0)
            && close(self.e, 0.0)
            && close(self.f, 0.0)
    }
}

/// How many clockwise quarter turns (`0..=3`) `degrees` is, when it is a
/// multiple of 90° (within 0.001°); `None` for any other angle. `-90` is 3.
pub fn quarter_turns(degrees: f32) -> Option<u8> {
    if !degrees.is_finite() {
        return None;
    }
    let r = degrees.rem_euclid(360.0);
    let k = (r / 90.0).round();
    ((r - k * 90.0).abs() < 1e-3).then_some((k as i64).rem_euclid(4) as u8)
}

/// How a vector shape's (or a text node's) outline is stroked. Lengths in
/// points once resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    pub color: Color,
    pub width: f32,
    /// Alternating on/off lengths; always even-length (or empty = solid).
    pub dash: Vec<f32>,
    pub cap: LineCap,
    pub join: LineJoin,
}

/// How a stroke turns the corners of an outline. Shapes default to `miter`,
/// text outlines to `round` (a halo without spikes at sharp glyph corners).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineJoin {
    /// Sharp corner (bevelled past a miter limit of 4, as in SVG).
    #[default]
    Miter,
    /// Circular arc around the corner.
    Round,
    /// Corner cut off flat.
    Bevel,
}

/// How the open ends of a stroked line/arrow/path are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineCap {
    /// Flat, ending exactly at the endpoint.
    #[default]
    Butt,
    /// Semicircle centered on the endpoint.
    Round,
    /// Square extending half the stroke width past the endpoint.
    Square,
}

/// Which ends of an open line/path carry an arrowhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArrowHead {
    #[default]
    None,
    Start,
    End,
    Both,
}

impl ArrowHead {
    pub fn at_start(self) -> bool {
        matches!(self, ArrowHead::Start | ArrowHead::Both)
    }
    pub fn at_end(self) -> bool {
        matches!(self, ArrowHead::End | ArrowHead::Both)
    }
}

/// The shape of an arrowhead (`head_style`). Every style is `head_size` long
/// (tip to base) and `head_width` across its base; "left"/"right" are as
/// seen travelling towards the tip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadStyle {
    /// A filled triangle (the default).
    #[default]
    Triangle,
    /// A filled, swept-back head with a notch in its base (PowerPoint's
    /// "stealth").
    Stealth,
    /// An open chevron: two strokes from the base corners to the tip, in the
    /// stroke's width (no fill).
    Open,
    /// A filled half head on the left of the shaft only — a harpoon, as in
    /// the two arrows of an equilibrium `⇌`.
    BarbLeft,
    /// [`HeadStyle::BarbLeft`] on the right side.
    BarbRight,
}

/// An arrowhead's base width (`head_width`): an absolute length (a number,
/// page units) or a multiple of the stroke width (a string `"3x"`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeadWidth {
    /// Page units (points once resolved).
    Abs(f32),
    /// Multiple of the stroke width.
    PerStroke(f32),
}

impl HeadWidth {
    /// The width in points for a stroke `stroke_width` points wide.
    pub fn resolve(self, stroke_width: f32) -> f32 {
        match self {
            HeadWidth::Abs(w) => w,
            HeadWidth::PerStroke(k) => k * stroke_width,
        }
    }

    /// Parse `"3x"` / `"2.5x"` (a multiple of the stroke width).
    pub fn parse_ratio(s: &str) -> Option<HeadWidth> {
        let k: f32 = s.trim().strip_suffix('x')?.trim().parse().ok()?;
        k.is_finite().then_some(HeadWidth::PerStroke(k))
    }
}

impl Serialize for HeadWidth {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            HeadWidth::Abs(w) => s.serialize_f32(*w),
            HeadWidth::PerStroke(k) => s.serialize_str(&format!("{k}x")),
        }
    }
}

impl<'de> Deserialize<'de> for HeadWidth {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = HeadWidth;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#"a length or a multiple of the stroke width like "3x""#)
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<HeadWidth, E> {
                Ok(HeadWidth::Abs(v as f32))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<HeadWidth, E> {
                Ok(HeadWidth::Abs(v as f32))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<HeadWidth, E> {
                Ok(HeadWidth::Abs(v as f32))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<HeadWidth, E> {
                HeadWidth::parse_ratio(v)
                    .ok_or_else(|| E::invalid_value(serde::de::Unexpected::Str(v), &self))
            }
        }
        d.deserialize_any(V)
    }
}

/// A drop shadow (or, with no offset, a glow) behind everything a node and
/// its subtree paint: their combined silhouette (alpha), grown by `spread`,
/// blurred and tinted with `color`, offset by (`dx`, `dy`) on the page.
/// Lengths in points once resolved. Purely visual: it never affects layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    /// Offset on the page (not turned by a rotation, so every shadow on a page
    /// falls the same way).
    pub dx: f32,
    pub dy: f32,
    /// Blur radius, as CSS `box-shadow`: a Gaussian of σ = `blur / 2`.
    pub blur: f32,
    /// Grow (positive) or shrink (negative) the silhouette before blurring.
    pub spread: f32,
    /// Colour and maximum opacity of the shadow.
    pub color: Color,
}

impl Shadow {
    /// Standard deviation of the blur, points.
    pub fn sigma(&self) -> f32 {
        (self.blur / 2.0).max(0.0)
    }

    /// How far (points) the shadow reaches past its offset silhouette.
    pub fn reach(&self) -> f32 {
        3.0 * self.sigma() + self.spread.max(0.0)
    }
}

/// Visual effects of a node, applied to its whole subtree as one group:
/// shadows/glows painted behind it, and a group opacity. `Default` is "no
/// effect".
#[derive(Debug, Clone, PartialEq)]
pub struct Effects {
    /// Group opacity `0..=1` (the subtree is composited as one layer).
    pub opacity: f32,
    /// Painted in order, behind the node (the drop shadow, then the glow).
    pub shadows: Vec<Shadow>,
}

impl Default for Effects {
    fn default() -> Self {
        Effects {
            opacity: 1.0,
            shadows: Vec::new(),
        }
    }
}

impl Effects {
    /// True when there is nothing to do (opaque, no shadows).
    pub fn is_none(&self) -> bool {
        self.opacity >= 1.0 && self.shadows.is_empty()
    }
}

/// Direction an `arc` path segment turns, as seen on the page (y down).
/// `cw` is SVG's `sweep-flag = 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArcSweep {
    /// Clockwise on the page.
    #[default]
    Cw,
    /// Counter-clockwise on the page.
    Ccw,
}

/// One command of a backend-neutral vector path (the IR every renderer
/// consumes). Coordinates are points, top-left origin, y down; inside
/// [`crate::layout::PathPaint`] they are absolute page coordinates.
///
/// A path is a single subpath: exactly one leading `MoveTo`, then any mix of
/// `LineTo` / `QuadTo` / `CubicTo`, then optionally one trailing `Close`.
/// Arcs never reach the IR — they are converted to `CubicTo`s during resolve
/// (see [`crate::path::arc_to_cubics`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSegment {
    /// Start the path at this point.
    MoveTo(Point),
    /// Straight line to this point.
    LineTo(Point),
    /// Quadratic Bézier: `(control, end)`.
    QuadTo(Point, Point),
    /// Cubic Bézier: `(control1, control2, end)`.
    CubicTo(Point, Point, Point),
    /// Straight line back to the `MoveTo` point, closing the outline.
    Close,
}

impl PathSegment {
    /// The point this segment ends on (`None` for `Close`).
    pub fn end(&self) -> Option<Point> {
        match *self {
            PathSegment::MoveTo(p)
            | PathSegment::LineTo(p)
            | PathSegment::QuadTo(_, p)
            | PathSegment::CubicTo(_, _, p) => Some(p),
            PathSegment::Close => None,
        }
    }

    /// This segment shifted by `(dx, dy)`.
    pub fn translated(&self, dx: f32, dy: f32) -> PathSegment {
        self.mapped(|p| Point::new(p.x + dx, p.y + dy))
    }

    /// The segment with every point (end and control points) mapped by `t`.
    /// Béziers are affine-invariant, so an affine `t` maps the curve exactly.
    pub fn mapped(&self, t: impl Fn(Point) -> Point) -> PathSegment {
        match *self {
            PathSegment::MoveTo(p) => PathSegment::MoveTo(t(p)),
            PathSegment::LineTo(p) => PathSegment::LineTo(t(p)),
            PathSegment::QuadTo(c, p) => PathSegment::QuadTo(t(c), t(p)),
            PathSegment::CubicTo(c1, c2, p) => PathSegment::CubicTo(t(c1), t(c2), t(p)),
            PathSegment::Close => PathSegment::Close,
        }
    }
}

/// A user coordinate system laid over a node's content box (`viewbox =
/// [x0, y0, x1, y1]`): `x0` maps to the left edge, `x1` to the right, `y0` to
/// the top and `y1` to the bottom, linearly. Either pair may be reversed
/// (`y0 > y1` gives a y axis pointing up, as in a data plot); neither may be
/// degenerate. Only positions are mapped — sizes, strokes, dashes and
/// arrowheads stay in page units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl ViewBox {
    /// From `[x0, y0, x1, y1]`; `Err` explains why it is invalid.
    pub fn from_array([x0, y0, x1, y1]: [f32; 4]) -> Result<ViewBox, String> {
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return Err("viewbox values must be finite numbers".into());
        }
        if x0 == x1 || y0 == y1 {
            return Err(format!(
                "viewbox [{x0}, {y0}, {x1}, {y1}] is degenerate: x0 != x1 and y0 != y1 are required"
            ));
        }
        Ok(ViewBox { x0, y0, x1, y1 })
    }

    /// Horizontal position (points, absolute) of user `x` in `rect`.
    pub fn map_x(&self, x: f32, rect: Rect) -> f32 {
        rect.x + (x - self.x0) / (self.x1 - self.x0) * rect.w
    }

    /// Vertical position (points, absolute) of user `y` in `rect`.
    pub fn map_y(&self, y: f32, rect: Rect) -> f32 {
        rect.y + (y - self.y0) / (self.y1 - self.y0) * rect.h
    }

    /// User point `p` -> absolute page point in `rect`.
    pub fn map(&self, p: Point, rect: Rect) -> Point {
        Point::new(self.map_x(p.x, rect), self.map_y(p.y, rect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_hex_forms() {
        assert_eq!(Color::parse_hex("#ffffff").unwrap(), Color::WHITE);
        assert_eq!(Color::parse_hex("000000").unwrap(), Color::BLACK);
        assert_eq!(Color::parse_hex("#fff").unwrap(), Color::WHITE);
        let half = Color::parse_hex("#00000080").unwrap();
        assert!((half.a - 128.0 / 255.0).abs() < 1e-3);
        assert!(Color::parse_hex("#xyz").is_err());
        assert!(Color::parse_hex("#12345").is_err());
    }

    #[test]
    fn head_width_ratio_strings() {
        assert_eq!(
            HeadWidth::parse_ratio("3x"),
            Some(HeadWidth::PerStroke(3.0))
        );
        assert_eq!(
            HeadWidth::parse_ratio(" 2.5x "),
            Some(HeadWidth::PerStroke(2.5))
        );
        assert_eq!(HeadWidth::parse_ratio("3"), None);
        assert_eq!(HeadWidth::PerStroke(3.0).resolve(2.0), 6.0);
        assert_eq!(HeadWidth::Abs(4.0).resolve(2.0), 4.0);
    }

    #[test]
    fn rect_inset_clamps() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let i = r.inset(Edges::all(2.0));
        assert_eq!(i, Rect::new(2.0, 2.0, 6.0, 6.0));
        // over-inset clamps to zero size, not negative
        let z = r.inset(Edges::all(100.0));
        assert_eq!(z.w, 0.0);
        assert_eq!(z.h, 0.0);
    }
}
