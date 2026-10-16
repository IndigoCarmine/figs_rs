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
/// Accepts either a scalar (`padding = 1.5`, applied to all four sides) or a table
/// (`padding = { top = 1, left = 2 }`, missing sides default to 0).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
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
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Scalar(f32),
            Sides {
                #[serde(default)]
                top: f32,
                #[serde(default)]
                right: f32,
                #[serde(default)]
                bottom: f32,
                #[serde(default)]
                left: f32,
            },
        }

        Ok(match Raw::deserialize(deserializer)? {
            Raw::Scalar(v) => Edges::all(v),
            Raw::Sides {
                top,
                right,
                bottom,
                left,
            } => Edges {
                top,
                right,
                bottom,
                left,
            },
        })
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
