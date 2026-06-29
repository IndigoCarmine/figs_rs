//! Unit handling. Everything in the engine works in **points** (1 pt = 1/72 inch)
//! after parsing. Page dimensions are declared in a user unit and converted once.

use serde::{Deserialize, Serialize};

/// User-facing length unit, used only in the `[page]` table.
/// `font_size` is always points and is never affected by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Cm,
    Mm,
    In,
    Pt,
    Px,
}

/// Points per inch (the definition of a point).
pub const PT_PER_INCH: f32 = 72.0;

impl Unit {
    /// How many points one of this unit is worth.
    ///
    /// `Px` is treated as a CSS pixel: 96 px per inch, i.e. 0.75 pt each. This
    /// keeps `unit = "px"` intuitive for users coming from the web/screen world.
    pub fn to_pt(self) -> f32 {
        match self {
            Unit::Cm => PT_PER_INCH / 2.54,
            Unit::Mm => PT_PER_INCH / 25.4,
            Unit::In => PT_PER_INCH,
            Unit::Pt => 1.0,
            Unit::Px => PT_PER_INCH / 96.0,
        }
    }

    /// Convert a value expressed in this unit into points.
    pub fn convert(self, value: f32) -> f32 {
        value * self.to_pt()
    }
}

/// Convert a length in points into raster pixels at a given DPI.
pub fn pt_to_px(pt: f32, dpi: f32) -> f32 {
    pt * dpi / PT_PER_INCH
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-3, "expected {b}, got {a}");
    }

    #[test]
    fn cm_to_pt() {
        // 2.54 cm == 1 inch == 72 pt
        approx(Unit::Cm.convert(2.54), 72.0);
        // 1 cm
        approx(Unit::Cm.convert(1.0), 28.346_457);
    }

    #[test]
    fn mm_to_pt() {
        approx(Unit::Mm.convert(25.4), 72.0);
    }

    #[test]
    fn inch_and_pt_identity() {
        approx(Unit::In.convert(1.0), 72.0);
        approx(Unit::Pt.convert(123.0), 123.0);
    }

    #[test]
    fn px_css_definition() {
        // 96 px == 1 inch == 72 pt
        approx(Unit::Px.convert(96.0), 72.0);
    }

    #[test]
    fn pt_to_px_roundtrip() {
        // 72 pt at 300 dpi == 300 px
        approx(pt_to_px(72.0, 300.0), 300.0);
        // and back: a cm at 300 dpi
        let one_cm_pt = Unit::Cm.convert(1.0);
        approx(pt_to_px(one_cm_pt, 300.0), 1.0 * 300.0 / 2.54);
    }
}
