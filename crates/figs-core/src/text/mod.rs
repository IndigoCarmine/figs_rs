//! Text shaping and measurement, built on cosmic-text (rustybuzz shaping +
//! fontdb discovery + swash rasterization). The same shaped output drives both
//! measurement (now) and the raster/vector backends (later milestones), so text
//! geometry is computed exactly once and shared.

pub mod shape;

pub use shape::{ShapedGlyph, ShapedLine, ShapedText};

use std::cell::RefCell;

use cosmic_text::{CacheKey, CacheKeyFlags, FontSystem, SwashCache, SwashContent};

use crate::geom::Size;
use crate::layout::LeafMeasure;
use crate::schema::{ImageProps, TextProps};

/// A rasterized glyph: an 8-bit coverage mask plus its offset from the pen
/// (baseline) origin. `left`/`top` follow swash conventions (top is positive
/// upward from the baseline).
#[derive(Debug, Clone)]
pub struct GlyphBitmap {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    /// `width * height` coverage values (0..=255).
    pub alpha: Vec<u8>,
}

/// Owns the font database and shaping engine. Load once and share across layout
/// and rendering. Uses interior mutability because cosmic-text needs `&mut` for
/// shaping/rasterizing while the layout engine measures through a shared
/// reference.
pub struct FontStore {
    system: RefCell<FontSystem>,
    swash: RefCell<SwashCache>,
    /// Family used when a text node doesn't name one.
    default_family: Option<String>,
}

impl FontStore {
    /// Build a store from system fonts.
    pub fn new() -> Self {
        FontStore {
            system: RefCell::new(FontSystem::new()),
            swash: RefCell::new(SwashCache::new()),
            default_family: None,
        }
    }

    /// Rasterize a single glyph at a physical pixel size, returning its coverage
    /// mask. Returns `None` for color/bitmap glyphs (not supported yet) or empty
    /// glyphs (e.g. spaces).
    pub fn glyph_alpha(&self, font_id: cosmic_text::fontdb::ID, glyph_id: u16, px_size: f32) -> Option<GlyphBitmap> {
        let (key, _, _) =
            CacheKey::new(font_id, glyph_id, px_size, (0.0, 0.0), CacheKeyFlags::empty());
        let mut system = self.system.borrow_mut();
        let mut swash = self.swash.borrow_mut();
        let image = swash.get_image(&mut system, key).as_ref()?;
        if image.content != SwashContent::Mask || image.data.is_empty() {
            return None;
        }
        Some(GlyphBitmap {
            left: image.placement.left,
            top: image.placement.top,
            width: image.placement.width,
            height: image.placement.height,
            alpha: image.data.clone(),
        })
    }

    /// Set the family used for text nodes without an explicit `font_family`.
    /// Pass a concrete family name (e.g. "Liberation Sans") for deterministic
    /// output across machines.
    pub fn with_default_family(mut self, family: impl Into<String>) -> Self {
        self.default_family = Some(family.into());
        self
    }

    /// Shape a text block under a width constraint (points; non-positive or
    /// non-finite means unconstrained).
    pub fn shape(&self, text: &TextProps, max_width: f32) -> ShapedText {
        let mut system = self.system.borrow_mut();
        shape::shape(&mut system, text, max_width, self.default_family.as_deref())
    }
}

impl Default for FontStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LeafMeasure for FontStore {
    fn shape_text(&self, text: &TextProps, max_width: f32) -> ShapedText {
        self.shape(text, max_width)
    }

    /// Image intrinsic sizing arrives with the image backend; until then images
    /// contribute no intrinsic size (they rely on explicit size / aspect_ratio).
    fn measure_image(&self, _image: &ImageProps) -> Size {
        Size::default()
    }
}
