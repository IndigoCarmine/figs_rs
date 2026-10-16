//! Paints: what a fill is made of — a solid [`Color`] or a linear / radial
//! gradient.
//!
//! Two stages, like the rest of the pipeline:
//!
//! - [`Paint`] is the *specification* as written (TOML `fill = …`, Rhai
//!   `.fill(…)`): gradient geometry is **relative to the node's border box**
//!   by default (`0..1` fractions of its width/height, so it scales with
//!   layout), or in the page unit from the box's top-left corner with
//!   `absolute = true`.
//! - [`Brush`] is that paint *placed* on a concrete box (layout does it, see
//!   [`Paint::place`]): every coordinate is in the node's local frame (points,
//!   the frame of [`ComputedNode::rect`]), so a backend draws it under the
//!   node's rotation exactly like the shape it fills.
//!
//! ## TOML
//!
//! ```toml
//! fill = "#ff0000"                                              # solid
//! fill = { linear = 90, stops = [[0, "#0b1a3a"], [1, "#1d3b6e"]] }  # angle
//! fill = { linear = [[0, 0], [1, 1]], stops = ["#fff", "#000"] }    # two points
//! fill = { radial = [0.5, 0.5], radius = 0.5, focal = [0.3, 0.3],
//!          stops = [[0, "#ffffffcc"], [1, "#ffffff00"]] }
//! ```
//!
//! - `linear = <degrees>`: the gradient runs **clockwise from left → right**
//!   (`0` = left → right, `90` = top → bottom, `180` = right → left; the
//!   PowerPoint convention). The gradient line passes through the box centre
//!   and is just long enough that the two extreme corners get the first and
//!   last stop (as CSS `linear-gradient`).
//! - `linear = [[x1, y1], [x2, y2]]`: from the first point to the second;
//!   colour bands are perpendicular to that line on the page.
//! - `radial = [cx, cy]`: centre; `radius` (default `0.5`) and an optional
//!   `focal` point (where offset 0 sits; default the centre). Box-relative,
//!   the circle is scaled with the box — in a non-square box it is an
//!   ellipse touching all four edges at `radius = 0.5` (SVG's
//!   `objectBoundingBox`). With `absolute = true` it is a circle of `radius`
//!   page units (then `radius` is required).
//! - `stops`: `[offset, color]` pairs, offsets `0..=1` and non-decreasing;
//!   colours may carry alpha (`#rrggbbaa`). A bare colour (`"#fff"`) takes
//!   an automatic offset: the first defaults to 0, the last to 1, and any run
//!   in between is spread evenly between its neighbours (as in CSS).
//!
//! [`ComputedNode::rect`]: crate::layout::ComputedNode::rect

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeTuple};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::geom::{Affine, Color, Point, Rect};

/// One colour stop of a gradient: where it sits along the gradient
/// (`0..=1`) and its colour (alpha honoured).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Color,
}

impl GradientStop {
    pub fn new(offset: f32, color: Color) -> Self {
        GradientStop { offset, color }
    }
}

/// Direction of a linear gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinearDir {
    /// Degrees clockwise from left → right (`90` = top → bottom), spanning
    /// the box corner to corner through its centre.
    Angle(f32),
    /// From the first point to the second (box fractions, or page units with
    /// `absolute`).
    Points([f32; 2], [f32; 2]),
}

/// A fill as written: a solid colour or a gradient. See the [module
/// docs](self) for the TOML form and the coordinate conventions.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Solid(Color),
    Linear {
        dir: LinearDir,
        stops: Vec<GradientStop>,
        /// Points in page units from the box's top-left corner instead of
        /// box fractions (no effect on an angle).
        absolute: bool,
    },
    Radial {
        /// Centre (box fractions, or page units with `absolute`).
        center: [f32; 2],
        /// End-circle radius; `None` = `0.5` (box-relative only).
        radius: Option<f32>,
        /// Where offset 0 sits; `None` = the centre.
        focal: Option<[f32; 2]>,
        stops: Vec<GradientStop>,
        absolute: bool,
    },
}

impl From<Color> for Paint {
    fn from(c: Color) -> Self {
        Paint::Solid(c)
    }
}

/// Default radial radius (box-relative): the ellipse touching all four edges.
pub const DEFAULT_RADIAL_RADIUS: f32 = 0.5;

impl Paint {
    /// The colour when this is a solid paint.
    pub fn as_solid(&self) -> Option<Color> {
        match self {
            Paint::Solid(c) => Some(*c),
            _ => None,
        }
    }

    /// The gradient stops (`None` for a solid paint).
    pub fn stops(&self) -> Option<&[GradientStop]> {
        match self {
            Paint::Solid(_) => None,
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => Some(stops),
        }
    }

    /// Mutable gradient stops (`None` for a solid paint) — the editor's
    /// stop colour buttons.
    pub fn stops_mut(&mut self) -> Option<&mut Vec<GradientStop>> {
        match self {
            Paint::Solid(_) => None,
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => Some(stops),
        }
    }

    /// A representative colour: the solid colour, or a gradient's first stop
    /// (used where only a flat colour makes sense).
    pub fn primary_color(&self) -> Color {
        match self {
            Paint::Solid(c) => *c,
            _ => self
                .stops()
                .and_then(|s| s.first())
                .map_or(Color::BLACK, |s| s.color),
        }
    }

    /// Check the paint is well formed: at least one stop, finite offsets in
    /// `0..=1` that never decrease, finite geometry, a positive radius, and a
    /// `radius` for an absolute radial gradient. `Err` explains what is wrong.
    pub fn validate(&self) -> Result<(), String> {
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite());
        let (stops, geometry_ok) = match self {
            Paint::Solid(_) => return Ok(()),
            Paint::Linear { dir, stops, .. } => {
                let ok = match dir {
                    LinearDir::Angle(a) => a.is_finite(),
                    LinearDir::Points(a, b) => finite(a) && finite(b),
                };
                (stops, ok)
            }
            Paint::Radial {
                center,
                radius,
                focal,
                stops,
                absolute,
            } => {
                if let Some(r) = radius {
                    if !(r.is_finite() && *r > 0.0) {
                        return Err("gradient `radius` must be a positive number".into());
                    }
                } else if *absolute {
                    return Err("an absolute radial gradient needs a `radius`".into());
                }
                (stops, finite(center) && focal.is_none_or(|f| finite(&f)))
            }
        };
        if !geometry_ok {
            return Err("gradient coordinates must be finite numbers".into());
        }
        if stops.is_empty() {
            return Err("a gradient needs at least one stop".into());
        }
        let mut prev = 0.0f32;
        for s in stops {
            if !(s.offset.is_finite() && (0.0..=1.0).contains(&s.offset)) {
                return Err(format!(
                    "stop offset {} is outside 0..1 (offsets are fractions along the gradient)",
                    s.offset
                ));
            }
            if s.offset < prev {
                return Err(format!(
                    "stop offsets must not decrease ({} after {prev})",
                    s.offset
                ));
            }
            prev = s.offset;
        }
        Ok(())
    }

    /// Convert absolute lengths from page units to points (`factor` = points
    /// per page unit). Box-relative paints are unchanged.
    pub fn scaled(mut self, factor: f32) -> Paint {
        let sc = |p: &mut [f32; 2]| {
            p[0] *= factor;
            p[1] *= factor;
        };
        match &mut self {
            Paint::Linear {
                dir: LinearDir::Points(a, b),
                absolute: true,
                ..
            } => {
                sc(a);
                sc(b);
            }
            Paint::Radial {
                center,
                radius,
                focal,
                absolute: true,
                ..
            } => {
                sc(center);
                if let Some(r) = radius {
                    *r *= factor;
                }
                if let Some(f) = focal {
                    sc(f);
                }
            }
            _ => {}
        }
        self
    }

    /// Place the paint on `rect` (the node's border box, in its local
    /// frame; lengths already in points — see [`Paint::scaled`]).
    pub fn place(&self, rect: Rect) -> Brush {
        let at = |p: [f32; 2], absolute: bool| {
            if absolute {
                Point::new(rect.x + p[0], rect.y + p[1])
            } else {
                Point::new(rect.x + p[0] * rect.w, rect.y + p[1] * rect.h)
            }
        };
        match self {
            Paint::Solid(c) => Brush::Solid(*c),
            Paint::Linear {
                dir,
                stops,
                absolute,
            } => {
                let (start, end) = match *dir {
                    LinearDir::Angle(deg) => angle_line(rect, deg),
                    LinearDir::Points(a, b) => (at(a, *absolute), at(b, *absolute)),
                };
                Brush::Linear {
                    start,
                    end,
                    stops: stops.clone(),
                }
            }
            Paint::Radial {
                center,
                radius,
                focal,
                stops,
                absolute,
            } => {
                let r = radius.unwrap_or(DEFAULT_RADIAL_RADIUS);
                let f = focal.unwrap_or(*center);
                if *absolute {
                    Brush::Radial {
                        center: at(*center, true),
                        focal: at(f, true),
                        radius: r,
                        transform: Affine::IDENTITY,
                        stops: stops.clone(),
                    }
                } else {
                    // Defined in the unit box, mapped onto the node's box.
                    Brush::Radial {
                        center: Point::new(center[0], center[1]),
                        focal: Point::new(f[0], f[1]),
                        radius: r,
                        transform: Affine {
                            a: rect.w,
                            b: 0.0,
                            c: 0.0,
                            d: rect.h,
                            e: rect.x,
                            f: rect.y,
                        },
                        stops: stops.clone(),
                    }
                }
            }
        }
    }
}

/// The CSS-style gradient line for an angle (degrees clockwise from
/// left → right): through the centre of `r`, long enough that the corners
/// farthest along the direction land on offsets 0 and 1.
fn angle_line(r: Rect, deg: f32) -> (Point, Point) {
    let (sin, cos) = (deg as f64).to_radians().sin_cos();
    let (sin, cos) = (sin as f32, cos as f32);
    // Snap the noise off exact axis directions.
    let snap = |v: f32| if v.abs() < 1e-6 { 0.0 } else { v };
    let (dx, dy) = (snap(cos), snap(sin));
    let half = (r.w * dx.abs() + r.h * dy.abs()) / 2.0;
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    (
        Point::new(cx - dx * half, cy - dy * half),
        Point::new(cx + dx * half, cy + dy * half),
    )
}

/// A [`Paint`] placed in a node's local frame (points) — what the IR carries
/// and backends draw.
#[derive(Debug, Clone, PartialEq)]
pub enum Brush {
    Solid(Color),
    /// Offset 0 at `start`, 1 at `end`; bands perpendicular to the line;
    /// padded beyond both ends.
    Linear {
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
    },
    /// In *gradient space*: offset 0 at `focal`, 1 on the circle of `radius`
    /// about `center`; `transform` maps gradient space onto the local frame
    /// (identity for an absolute gradient; the unit box → node box for a
    /// box-relative one, which makes it an ellipse in a non-square box).
    Radial {
        center: Point,
        focal: Point,
        radius: f32,
        transform: Affine,
        stops: Vec<GradientStop>,
    },
}

impl Brush {
    /// The colour of a solid brush.
    pub fn as_solid(&self) -> Option<Color> {
        match self {
            Brush::Solid(c) => Some(*c),
            _ => None,
        }
    }

    pub fn stops(&self) -> &[GradientStop] {
        match self {
            Brush::Solid(_) => &[],
            Brush::Linear { stops, .. } | Brush::Radial { stops, .. } => stops,
        }
    }

    /// The flat colour a backend falls back to when a gradient degenerates
    /// (zero-length line, empty box): its last stop, as a padded gradient
    /// would show.
    pub fn fallback_color(&self) -> Color {
        match self {
            Brush::Solid(c) => *c,
            _ => self.stops().last().map_or(Color::BLACK, |s| s.color),
        }
    }

    /// The colour at gradient offset `t` (clamped), interpolating straight
    /// RGBA between stops. A solid brush is its colour everywhere. Used by
    /// tests and previews; renderers use their native gradients.
    pub fn color_at(&self, t: f32) -> Color {
        let stops = self.stops();
        let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
            return self.fallback_color();
        };
        let t = t.clamp(0.0, 1.0);
        if t <= first.offset {
            return first.color;
        }
        for w in stops.windows(2) {
            let (a, b) = (w[0], w[1]);
            if t <= b.offset {
                let span = b.offset - a.offset;
                let k = if span > 0.0 {
                    (t - a.offset) / span
                } else {
                    1.0
                };
                let mix = |x: f32, y: f32| x + (y - x) * k;
                return Color {
                    r: mix(a.color.r, b.color.r),
                    g: mix(a.color.g, b.color.g),
                    b: mix(a.color.b, b.color.b),
                    a: mix(a.color.a, b.color.a),
                };
            }
        }
        last.color
    }
}

// ---------------------------------------------------------------------------
// Serde: `"#rrggbb"` or `{ linear | radial = …, stops = […], … }`
// ---------------------------------------------------------------------------

/// A stop as written: `[offset, color]` or a bare colour (automatic offset).
struct RawStop(Option<f32>, Color);

impl<'de> Deserialize<'de> for RawStop {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = RawStop;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r##"a gradient stop: [offset, "#rrggbb"] or "#rrggbb""##)
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<RawStop, E> {
                Color::parse_hex(v)
                    .map(|c| RawStop(None, c))
                    .map_err(E::custom)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<RawStop, A::Error> {
                let offset: f32 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let color: Color = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(3, &self));
                }
                Ok(RawStop(Some(offset), color))
            }
        }
        d.deserialize_any(V)
    }
}

/// Fill in automatic stop offsets: the first defaults to 0, the last to 1,
/// and a run of unset offsets is spread evenly between its set neighbours.
pub fn auto_offsets(stops: &[(Option<f32>, Color)]) -> Vec<GradientStop> {
    let n = stops.len();
    let mut off: Vec<Option<f32>> = stops.iter().map(|s| s.0).collect();
    if n == 0 {
        return Vec::new();
    }
    if off[0].is_none() {
        off[0] = Some(0.0);
    }
    if off[n - 1].is_none() {
        off[n - 1] = Some(if n == 1 { 0.0 } else { 1.0 });
    }
    let mut i = 0;
    while i < n {
        if off[i].is_some() {
            i += 1;
            continue;
        }
        // Run of unset offsets [i, j); neighbours i-1 and j are set.
        let mut j = i;
        while off[j].is_none() {
            j += 1;
        }
        let (a, b) = (off[i - 1].unwrap_or(0.0), off[j].unwrap_or(1.0));
        let steps = (j - i + 1) as f32;
        for (k, slot) in off[i..j].iter_mut().enumerate() {
            *slot = Some(a + (b - a) * (k + 1) as f32 / steps);
        }
        i = j;
    }
    stops
        .iter()
        .zip(off)
        .map(|((_, c), o)| GradientStop::new(o.unwrap_or(0.0), *c))
        .collect()
}

/// `linear = <degrees>` or `linear = [[x1, y1], [x2, y2]]`.
struct RawLinear(LinearDir);

impl<'de> Deserialize<'de> for RawLinear {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = RawLinear;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an angle in degrees or two points [[x1, y1], [x2, y2]]")
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<RawLinear, E> {
                Ok(RawLinear(LinearDir::Angle(v as f32)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<RawLinear, E> {
                Ok(RawLinear(LinearDir::Angle(v as f32)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<RawLinear, E> {
                Ok(RawLinear(LinearDir::Angle(v as f32)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<RawLinear, A::Error> {
                let a: [f32; 2] = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let b: [f32; 2] = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(3, &self));
                }
                Ok(RawLinear(LinearDir::Points(a, b)))
            }
        }
        d.deserialize_any(V)
    }
}

/// The table form of a gradient, before validation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGradient {
    #[serde(default)]
    linear: Option<RawLinear>,
    #[serde(default)]
    radial: Option<[f32; 2]>,
    #[serde(default)]
    radius: Option<f32>,
    #[serde(default)]
    focal: Option<[f32; 2]>,
    stops: Vec<RawStop>,
    #[serde(default)]
    absolute: bool,
}

impl RawGradient {
    fn into_paint(self) -> Result<Paint, String> {
        let stops: Vec<(Option<f32>, Color)> = self.stops.into_iter().map(|s| (s.0, s.1)).collect();
        let stops = auto_offsets(&stops);
        let paint = match (self.linear, self.radial) {
            (Some(RawLinear(dir)), None) => {
                if self.radius.is_some() || self.focal.is_some() {
                    return Err("`radius` and `focal` only apply to a `radial` gradient".into());
                }
                Paint::Linear {
                    dir,
                    stops,
                    absolute: self.absolute,
                }
            }
            (None, Some(center)) => Paint::Radial {
                center,
                radius: self.radius,
                focal: self.focal,
                stops,
                absolute: self.absolute,
            },
            (Some(_), Some(_)) => {
                return Err("a gradient is either `linear` or `radial`, not both".into())
            }
            (None, None) => {
                return Err(
                    "a gradient needs `linear = <degrees | [[x1, y1], [x2, y2]]>` or `radial = [cx, cy]`"
                        .into(),
                )
            }
        };
        paint.validate()?;
        Ok(paint)
    }
}

impl<'de> Deserialize<'de> for Paint {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Paint;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(
                    r##"a color "#rrggbb" or a gradient table { linear | radial = …, stops = […] }"##,
                )
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Paint, E> {
                Color::parse_hex(v).map(Paint::Solid).map_err(E::custom)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Paint, A::Error> {
                let raw = RawGradient::deserialize(de::value::MapAccessDeserializer::new(map))?;
                raw.into_paint().map_err(de::Error::custom)
            }
        }
        d.deserialize_any(V)
    }
}

/// A stop as `[offset, "#hex"]`.
struct StopOut<'a>(&'a GradientStop);

impl Serialize for StopOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut t = s.serialize_tuple(2)?;
        t.serialize_element(&self.0.offset)?;
        t.serialize_element(&self.0.color)?;
        t.end()
    }
}

struct StopsOut<'a>(&'a [GradientStop]);

impl Serialize for StopsOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(StopOut))
    }
}

impl Serialize for Paint {
    /// A solid paint is its hex string; a gradient the table form (explicit
    /// offsets). Round-trips with the `Deserialize` impl.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Paint::Solid(c) => c.serialize(s),
            Paint::Linear {
                dir,
                stops,
                absolute,
            } => {
                let mut m = s.serialize_map(None)?;
                match dir {
                    LinearDir::Angle(a) => m.serialize_entry("linear", a)?,
                    LinearDir::Points(a, b) => m.serialize_entry("linear", &[a, b])?,
                }
                m.serialize_entry("stops", &StopsOut(stops))?;
                if *absolute {
                    m.serialize_entry("absolute", &true)?;
                }
                m.end()
            }
            Paint::Radial {
                center,
                radius,
                focal,
                stops,
                absolute,
            } => {
                let mut m = s.serialize_map(None)?;
                m.serialize_entry("radial", center)?;
                if let Some(r) = radius {
                    m.serialize_entry("radius", r)?;
                }
                if let Some(f) = focal {
                    m.serialize_entry("focal", f)?;
                }
                m.serialize_entry("stops", &StopsOut(stops))?;
                if *absolute {
                    m.serialize_entry("absolute", &true)?;
                }
                m.end()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Color {
        Color::parse_hex(s).unwrap()
    }

    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    struct Holder {
        fill: Paint,
    }

    fn parse(src: &str) -> Result<Paint, String> {
        toml::from_str::<Holder>(src)
            .map(|h| h.fill)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn solid_string_still_parses() {
        assert_eq!(
            parse(r##"fill = "#ff0000""##).unwrap(),
            Paint::Solid(hex("#ff0000"))
        );
    }

    #[test]
    fn linear_angle_and_points() {
        let p =
            parse(r##"fill = { linear = 90, stops = [[0, "#0b1a3a"], [1, "#1d3b6e"]] }"##).unwrap();
        assert_eq!(
            p,
            Paint::Linear {
                dir: LinearDir::Angle(90.0),
                stops: vec![
                    GradientStop::new(0.0, hex("#0b1a3a")),
                    GradientStop::new(1.0, hex("#1d3b6e"))
                ],
                absolute: false,
            }
        );
        let p =
            parse(r##"fill = { linear = [[0, 0], [1, 0.5]], stops = ["#fff", "#000"] }"##).unwrap();
        assert!(matches!(
            p,
            Paint::Linear {
                dir: LinearDir::Points([0.0, 0.0], [1.0, 0.5]),
                ..
            }
        ));
    }

    #[test]
    fn radial_with_focal_and_alpha() {
        let p = parse(
            r##"fill = { radial = [0.5, 0.5], radius = 0.4, focal = [0.3, 0.3], stops = [[0, "#ffffffcc"], [1, "#ffffff00"]] }"##,
        )
        .unwrap();
        let Paint::Radial {
            radius,
            focal,
            stops,
            ..
        } = &p
        else {
            panic!("{p:?}")
        };
        assert_eq!(*radius, Some(0.4));
        assert_eq!(*focal, Some([0.3, 0.3]));
        assert!(stops[1].color.a.abs() < 1e-6);
    }

    #[test]
    fn bad_gradients_are_rejected() {
        for (src, needle) in [
            (r##"fill = { stops = ["#fff"] }"##, "needs `linear"),
            (
                r##"fill = { linear = 0, stops = [] }"##,
                "at least one stop",
            ),
            (
                r##"fill = { linear = 0, stops = [[0.6, "#fff"], [0.2, "#000"]] }"##,
                "must not decrease",
            ),
            (
                r##"fill = { linear = 0, stops = [[2, "#fff"]] }"##,
                "outside 0..1",
            ),
            (
                r##"fill = { radial = [0, 0], absolute = true, stops = ["#fff"] }"##,
                "needs a `radius`",
            ),
            (
                r##"fill = { linear = 0, radius = 1, stops = ["#fff"] }"##,
                "only apply",
            ),
            (
                r##"fill = { linear = 0, stop = ["#fff"] }"##,
                "unknown field",
            ),
        ] {
            let e = parse(src).unwrap_err();
            assert!(e.contains(needle), "{src}: {e}");
        }
    }

    #[test]
    fn auto_offsets_spread_evenly() {
        let c = Color::BLACK;
        let o: Vec<f32> =
            auto_offsets(&[(None, c), (None, c), (Some(0.5), c), (None, c), (None, c)])
                .iter()
                .map(|s| s.offset)
                .collect();
        assert_eq!(o, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn toml_round_trip() {
        for src in [
            r##"fill = "#123456""##,
            r##"fill = { linear = 45, stops = [[0, "#000000"], [0.3, "#ff000080"], [1, "#ffffff"]] }"##,
            r##"fill = { linear = [[1, 2], [3, 4]], stops = ["#000", "#fff"], absolute = true }"##,
            r##"fill = { radial = [0.5, 0.25], radius = 0.75, focal = [0.4, 0.2], stops = ["#000", "#fff"] }"##,
        ] {
            let a: Holder = toml::from_str(src).unwrap();
            let out = toml::to_string(&a).unwrap();
            let b: Holder = toml::from_str(&out).unwrap_or_else(|e| panic!("{out}: {e}"));
            assert_eq!(a, b, "{out}");
        }
    }

    #[test]
    fn angle_line_spans_the_box() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        let (s, e) = angle_line(r, 0.0);
        assert_eq!((s, e), (Point::new(10.0, 45.0), Point::new(110.0, 45.0)));
        let (s, e) = angle_line(r, 90.0);
        assert_eq!((s, e), (Point::new(60.0, 20.0), Point::new(60.0, 70.0)));
        // 45°: the line's projection reaches both extreme corners.
        let (s, e) = angle_line(r, 45.0);
        let len = ((e.x - s.x).powi(2) + (e.y - s.y).powi(2)).sqrt();
        assert!((len - 150.0 / 2f32.sqrt()).abs() < 1e-3, "{len}");
    }

    #[test]
    fn place_relative_and_absolute() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        let p = Paint::Linear {
            dir: LinearDir::Points([0.0, 0.0], [1.0, 1.0]),
            stops: vec![GradientStop::new(0.0, Color::BLACK)],
            absolute: false,
        };
        let Brush::Linear { start, end, .. } = p.place(r) else {
            panic!()
        };
        assert_eq!(
            (start, end),
            (Point::new(10.0, 20.0), Point::new(110.0, 70.0))
        );
        let p = Paint::Radial {
            center: [5.0, 5.0],
            radius: Some(2.0),
            focal: None,
            stops: vec![GradientStop::new(0.0, Color::BLACK)],
            absolute: true,
        }
        .scaled(2.0);
        let Brush::Radial { center, radius, .. } = p.place(r) else {
            panic!()
        };
        assert_eq!((center, radius), (Point::new(20.0, 30.0), 4.0));
    }

    #[test]
    fn color_at_interpolates() {
        let b = Brush::Linear {
            start: Point::new(0.0, 0.0),
            end: Point::new(1.0, 0.0),
            stops: vec![
                GradientStop::new(0.0, Color::BLACK),
                GradientStop::new(1.0, Color::WHITE),
            ],
        };
        assert!((b.color_at(0.5).r - 0.5).abs() < 1e-6);
        assert_eq!(b.color_at(-1.0), Color::BLACK);
        assert_eq!(b.color_at(2.0), Color::WHITE);
    }
}
