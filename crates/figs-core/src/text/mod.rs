//! Text shaping and measurement, built on cosmic-text (rustybuzz shaping +
//! fontdb discovery + swash rasterization). The same shaped output drives both
//! measurement (now) and the raster/vector backends (later milestones), so text
//! geometry is computed exactly once and shared.

pub mod shape;

pub use shape::{ShapedGlyph, ShapedLine, ShapedText};

use std::cell::RefCell;

use cosmic_text::FontSystem;

use crate::geom::Size;
use crate::layout::LeafMeasure;
use crate::schema::{ImageProps, TextProps};

/// Owns the font database and shaping engine. Load once and share across layout
/// and rendering. Uses interior mutability because cosmic-text needs `&mut` for
/// shaping while the layout engine measures through a shared reference.
pub struct FontStore {
    system: RefCell<FontSystem>,
    /// Family used when a text node doesn't name one.
    default_family: Option<String>,
}

impl FontStore {
    /// Build a store from system fonts.
    pub fn new() -> Self {
        FontStore {
            system: RefCell::new(FontSystem::new()),
            default_family: None,
        }
    }

    /// Set the family used for text nodes without an explicit `font_family`.
    /// Pass a concrete family name (e.g. "Liberation Sans") for deterministic
    /// output across machines.
    pub fn with_default_family(mut self, family: impl Into<String>) -> Self {
        self.default_family = Some(family.into());
        self
    }

    /// Set (or clear) the default family after construction. `None` falls back
    /// to a generic sans-serif during shaping.
    pub fn set_default_family(&mut self, family: Option<String>) {
        self.default_family = family;
    }

    /// All installed font families, deduplicated and sorted case-insensitively.
    /// Used by the editor to populate font pickers.
    pub fn families(&self) -> Vec<String> {
        let system = self.system.borrow();
        let mut names: Vec<String> = system
            .db()
            .faces()
            .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
            .collect();
        names.sort_by_key(|s| s.to_lowercase());
        names.dedup();
        names
    }

    /// Shape a text block under a width constraint (points; non-positive or
    /// non-finite means unconstrained).
    pub fn shape(&self, text: &TextProps, max_width: f32) -> ShapedText {
        let mut system = self.system.borrow_mut();
        shape::shape(&mut system, text, max_width, self.default_family.as_deref())
    }

    /// Run a closure with mutable access to the underlying `FontSystem`.
    ///
    /// The renderer needs this to rasterize glyphs through a
    /// [`cosmic_text::SwashCache`], which requires `&mut FontSystem`.
    pub fn with_system<R>(&self, f: impl FnOnce(&mut FontSystem) -> R) -> R {
        let mut system = self.system.borrow_mut();
        f(&mut system)
    }

    /// The default family used for text nodes without an explicit `font_family`,
    /// if one was configured.
    pub fn default_family(&self) -> Option<&str> {
        self.default_family.as_deref()
    }
}

impl Default for FontStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LeafMeasure for FontStore {
    fn measure_text(&self, text: &TextProps, max_width: f32) -> Size {
        self.shape(text, max_width).size
    }

    /// Image intrinsic sizing arrives with the image backend; until then images
    /// contribute no intrinsic size (they rely on explicit size / aspect_ratio).
    fn measure_image(&self, _image: &ImageProps) -> Size {
        Size::default()
    }
}
