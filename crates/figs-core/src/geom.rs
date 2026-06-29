//! Shared geometry and style primitives used across schema, layout and render.

use serde::{Deserialize, Deserializer};

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

/// Distribution of children along a container's main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// How an image is fitted into its computed box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
    Fill,
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
