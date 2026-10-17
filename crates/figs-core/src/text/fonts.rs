//! Font selection for shaping: ordered family lists resolved per character,
//! CSS-style weight matching, real-vs-synthetic italic, and cached face
//! metrics (ascent/descent, underline and line-through placement).
//!
//! cosmic-text has its own fallback (the requested family, then a per-script
//! platform list, then a few common fonts, then any face), but it only takes a
//! single family per run and, being script-driven, would e.g. set Japanese
//! kanji in a Chinese face on a non-`ja` locale. figs makes the **author's
//! ordered list authoritative**: each character goes to the first listed
//! family that has a glyph for it, and only characters no listed family
//! covers are left to cosmic-text's fallback (see the README, "Fonts").
//!
//! A per-script spec (`latin: Arial; cjk: Yu Gothic`, see
//! [`crate::schema::FontSpec`]) turns that one list into one per script
//! class ([`super::classify`]): each character walks [`chain`] for its slot.

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::fontdb::{self, Family, Stretch, Style};
use cosmic_text::FontSystem;

use crate::schema::FontSpec;

use super::classify::Slot;

/// The face chosen for one family name at a requested weight/style.
#[derive(Debug, Clone)]
pub(crate) struct FaceChoice {
    pub id: fontdb::ID,
    /// The family name exactly as the face lists it (what cosmic-text's
    /// matcher compares against).
    pub family: String,
    pub weight: u16,
    pub style: Style,
    pub stretch: Stretch,
    /// Italic was requested but the family has no italic/oblique face: the
    /// upright face is slanted by [`crate::schema::span::SYNTHETIC_ITALIC_DEG`].
    pub synthetic_italic: bool,
}

/// Vertical metrics of a face, in em (y up; `descent` is positive below the
/// baseline, `underline_pos` negative below it).
#[derive(Debug, Clone, Copy)]
pub struct FaceMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub underline_pos: f32,
    pub underline_thickness: f32,
    pub strike_pos: f32,
    pub strike_thickness: f32,
    /// Height of the capitals above the baseline (OS/2 `sCapHeight`), em.
    pub cap_height: f32,
}

/// The cap height (em) assumed for a face that doesn't record one (no OS/2
/// v2 table, or a zero value): about that of Latin sans/serif faces.
pub const FALLBACK_CAP_HEIGHT: f32 = 0.7;

impl Default for FaceMetrics {
    /// Size-based fallbacks for a face that can't be read.
    fn default() -> Self {
        FaceMetrics {
            ascent: 0.8,
            descent: 0.2,
            underline_pos: -0.1,
            underline_thickness: 0.05,
            strike_pos: 0.25,
            strike_thickness: 0.05,
            cap_height: FALLBACK_CAP_HEIGHT,
        }
    }
}

/// Caches shared by every shaping call of a [`super::FontStore`].
#[derive(Default)]
pub(crate) struct FontCache {
    choices: HashMap<(String, u16, bool), Option<Arc<FaceChoice>>>,
    metrics: HashMap<fontdb::ID, FaceMetrics>,
    coverage: HashMap<(fontdb::ID, char), bool>,
    weights: HashMap<fontdb::ID, u16>,
}

/// The name a generic CSS family stands for in `db`, or `name` itself.
fn concrete_family(db: &fontdb::Database, name: &str) -> String {
    let generic = match name.to_ascii_lowercase().as_str() {
        "sans-serif" | "sans" => Family::SansSerif,
        "serif" => Family::Serif,
        "monospace" => Family::Monospace,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        _ => return name.to_string(),
    };
    db.family_name(&generic).to_string()
}

/// CSS font-matching order for weights: lower key = better.
fn weight_key(desired: u16, w: u16) -> (u8, u16) {
    let d = desired;
    if w == d {
        return (0, 0);
    }
    if (400..=500).contains(&d) {
        if w > d && w <= 500 {
            (1, w - d)
        } else if w < d {
            (2, d - w)
        } else {
            (3, w - d)
        }
    } else if d < 400 {
        if w < d {
            (1, d - w)
        } else {
            (2, w - d)
        }
    } else if w > d {
        (1, w - d)
    } else {
        (2, d - w)
    }
}

fn stretch_distance(s: Stretch) -> u16 {
    s.to_number().abs_diff(Stretch::Normal.to_number())
}

impl FontCache {
    /// The face of family `name` (case-insensitive, any localized name; or a
    /// generic `sans-serif`/`serif`/`monospace`) best matching `weight` and
    /// `italic`, or `None` when no such family is installed.
    pub fn choose(
        &mut self,
        system: &FontSystem,
        name: &str,
        weight: u16,
        italic: bool,
    ) -> Option<Arc<FaceChoice>> {
        let key = (name.to_lowercase(), weight, italic);
        if let Some(hit) = self.choices.get(&key) {
            return hit.clone();
        }
        let choice = choose_face(system.db(), name, weight, italic).map(Arc::new);
        if choice.is_none() {
            tracing::debug!(family = name, "font family not installed");
        }
        self.choices.insert(key, choice.clone());
        choice
    }

    /// Whether face `id` has a glyph for `ch`.
    pub fn covers(&mut self, system: &mut FontSystem, id: fontdb::ID, ch: char) -> bool {
        *self.coverage.entry((id, ch)).or_insert_with(|| {
            system
                .get_font(id)
                .is_some_and(|f| f.as_swash().charmap().map(ch) != 0)
        })
    }

    /// The weight (100–900) face `id` declares; 400 when unknown.
    pub fn face_weight(&mut self, system: &FontSystem, id: fontdb::ID) -> u16 {
        *self
            .weights
            .entry(id)
            .or_insert_with(|| system.db().face(id).map_or(400, |f| f.weight.0))
    }

    /// Vertical metrics of face `id` (cached).
    pub fn metrics(&mut self, system: &mut FontSystem, id: fontdb::ID) -> FaceMetrics {
        *self
            .metrics
            .entry(id)
            .or_insert_with(|| face_metrics(system, id).unwrap_or_default())
    }
}

fn face_metrics(system: &mut FontSystem, id: fontdb::ID) -> Option<FaceMetrics> {
    let font = system.get_font(id)?;
    let face = font.rustybuzz();
    let upem = face.units_per_em() as f32;
    if upem <= 0.0 {
        return None;
    }
    let fallback = FaceMetrics::default();
    let line = |m: Option<cosmic_text::ttf_parser::LineMetrics>, d: (f32, f32)| {
        m.map(|m| (m.position as f32 / upem, m.thickness as f32 / upem))
            .filter(|(_, t)| *t > 0.0)
            .unwrap_or(d)
    };
    let (underline_pos, underline_thickness) = line(
        face.underline_metrics(),
        (fallback.underline_pos, fallback.underline_thickness),
    );
    let (strike_pos, strike_thickness) = line(
        face.strikeout_metrics(),
        (fallback.strike_pos, underline_thickness),
    );
    Some(FaceMetrics {
        // The same values cosmic-text lays lines out with.
        ascent: face.ascender() as f32 / upem,
        descent: -(face.descender() as f32) / upem,
        underline_pos,
        underline_thickness,
        strike_pos,
        strike_thickness,
        cap_height: face
            .capital_height()
            .map(|h| h as f32 / upem)
            .filter(|h| *h > 0.0)
            .unwrap_or(FALLBACK_CAP_HEIGHT),
    })
}

fn choose_face(db: &fontdb::Database, name: &str, weight: u16, italic: bool) -> Option<FaceChoice> {
    let target = concrete_family(db, name);
    let named = |f: &fontdb::FaceInfo| -> Option<String> {
        f.families
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&target))
            .map(|(n, _)| n.clone())
    };
    let mut cands: Vec<(&fontdb::FaceInfo, String)> = db
        .faces()
        .filter_map(|f| named(f).map(|n| (f, n)))
        .collect();
    if cands.is_empty() {
        return None;
    }
    // Width closest to normal (a "Narrow" family is all condensed).
    let best = cands
        .iter()
        .map(|(f, _)| stretch_distance(f.stretch))
        .min()?;
    cands.retain(|(f, _)| stretch_distance(f.stretch) == best);
    // Style: a real italic, else an oblique, else upright (slanted later).
    let order: [Style; 3] = if italic {
        [Style::Italic, Style::Oblique, Style::Normal]
    } else {
        [Style::Normal, Style::Oblique, Style::Italic]
    };
    let style = order
        .into_iter()
        .find(|s| cands.iter().any(|(f, _)| f.style == *s))?;
    cands.retain(|(f, _)| f.style == style);
    let (face, family) = cands
        .into_iter()
        .min_by_key(|(f, _)| (weight_key(weight, f.weight.0), f.id))?;
    Some(FaceChoice {
        id: face.id,
        family,
        weight: face.weight.0,
        style,
        stretch: face.stretch,
        synthetic_italic: italic && style == Style::Normal,
    })
}

/// The ordered family names a character in `slot` tries under `spec`: its
/// own slot's list, then the fallback list (the generic `sans-serif` when
/// both are empty), then the other slots — so every family the author named
/// is tried before the system's fallback. A plain list is just its names.
pub(crate) fn chain(spec: &FontSpec, slot: Slot) -> Vec<&str> {
    fn add<'a>(out: &mut Vec<&'a str>, names: Option<&'a Vec<String>>) {
        for n in names.into_iter().flatten() {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(n)) {
                out.push(n);
            }
        }
    }
    type Names<'a> = Option<&'a Vec<String>>;
    let (latin, cjk, symbol) = (spec.latin.as_ref(), spec.cjk.as_ref(), spec.symbol.as_ref());
    // (own slots, slots tried after the fallback list)
    let (first, rest): ([Names; 2], [Names; 3]) = match slot {
        Slot::Latin => ([latin, None], [cjk, symbol, None]),
        Slot::Cjk => ([cjk, None], [latin, symbol, None]),
        Slot::Symbol { cjk: true } => ([symbol, cjk], [latin, None, None]),
        Slot::Symbol { cjk: false } => ([symbol, latin], [cjk, None, None]),
        Slot::Other => ([None, None], [latin, cjk, symbol]),
    };
    let mut out: Vec<&str> = Vec::new();
    for s in first {
        add(&mut out, s);
    }
    add(&mut out, Some(&spec.fallback));
    if out.is_empty() && slot != Slot::Other {
        out.push("sans-serif");
    }
    for s in rest {
        add(&mut out, s);
    }
    if out.is_empty() {
        out.push("sans-serif");
    }
    out
}

/// Characters that never start a font run of their own: combining marks,
/// variation selectors and joiners stay with the preceding character's face.
pub(crate) fn attaches_to_previous(ch: char) -> bool {
    matches!(ch as u32,
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF
        | 0xFE20..=0xFE2F | 0xFE00..=0xFE0F | 0xE0100..=0xE01EF
        | 0x200C | 0x200D | 0x3099 | 0x309A)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_weight_order() {
        // 400 wanted: 400, then 500, then lighter, then bolder.
        let mut ws = [700u16, 300, 500, 400, 100];
        ws.sort_by_key(|w| weight_key(400, *w));
        assert_eq!(ws, [400, 500, 300, 100, 700]);
        // 700 wanted: bolder first, then lighter.
        let mut ws = [400u16, 900, 600, 800];
        ws.sort_by_key(|w| weight_key(700, *w));
        assert_eq!(ws, [800, 900, 600, 400]);
        // 300 wanted: lighter first.
        let mut ws = [400u16, 200, 100];
        ws.sort_by_key(|w| weight_key(300, *w));
        assert_eq!(ws, [200, 100, 400]);
    }
}
