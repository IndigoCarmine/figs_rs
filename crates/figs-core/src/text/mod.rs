//! Text shaping and measurement, built on cosmic-text (rustybuzz shaping +
//! fontdb discovery + swash rasterization). The same shaped output drives both
//! measurement (now) and the raster/vector backends (later milestones), so text
//! geometry is computed exactly once and shared.
//!
//! Rich text (inline spans with their own size/weight/style/colour/script/
//! tracking/decorations/outline), ordered font-family lists resolved per
//! character — optionally per script class ([`char_class`]) — and synthetic
//! bold/slant are handled in [`shape`] and `fonts`; see the README's "Rich
//! text" section.

mod classify;
mod fonts;
pub mod shape;

pub use classify::{char_class, CharClass};
pub use fonts::FaceMetrics;
pub use shape::{
    styled_runs, Decoration, DecorationKind, RunStyle, ShapedGlyph, ShapedLine, ShapedText,
    StyledRun,
};

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cosmic_text::FontSystem;

use crate::geom::Size;
use crate::layout::{LeafMeasure, TextBaselines};
use crate::schema::{ImageProps, TextProps};

/// Owns the font database and shaping engine. Load once and share across layout
/// and rendering. Uses interior mutability because cosmic-text needs `&mut` for
/// shaping while the layout engine measures through a shared reference.
pub struct FontStore {
    system: RefCell<FontSystem>,
    /// Face choices, coverage and metrics (see [`fonts`]).
    cache: RefCell<fonts::FontCache>,
    /// Family list used when a text node doesn't name one.
    default_family: Option<String>,
    /// Directory relative image `src` paths resolve against when reading an
    /// image's natural aspect ratio. `None` disables image lookups.
    image_base_dir: Option<PathBuf>,
    /// Natural aspect (w / h) per resolved image path; `None` = unreadable.
    image_aspects: RefCell<HashMap<PathBuf, Option<f32>>>,
}

impl FontStore {
    /// Build a store from system fonts.
    pub fn new() -> Self {
        FontStore {
            system: RefCell::new(FontSystem::new()),
            cache: RefCell::new(fonts::FontCache::default()),
            default_family: None,
            image_base_dir: None,
            image_aspects: RefCell::new(HashMap::new()),
        }
    }

    /// Enable reading images' natural aspect ratios during layout, resolving
    /// relative `src` paths against `dir` (the document's directory — the same
    /// base the renderer uses). Without it, images with no `aspect_ratio` use
    /// [`crate::layout::FALLBACK_IMAGE_ASPECT`] wherever an aspect is needed.
    pub fn set_image_base_dir(&mut self, dir: Option<PathBuf>) {
        self.image_base_dir = dir;
        self.image_aspects.borrow_mut().clear();
    }

    /// Set the family (or comma-separated family list, e.g. `"Arial, Yu
    /// Gothic"`) used for text nodes without an explicit `font_family`. Pass
    /// concrete family names for deterministic output across machines.
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
        let mut cache = self.cache.borrow_mut();
        shape::shape(
            &mut system,
            &mut cache,
            text,
            max_width,
            self.default_family.as_deref(),
        )
    }

    /// Vertical metrics (ascent, descent, underline/strike placement) of a
    /// face, in em.
    pub fn face_metrics(&self, id: cosmic_text::fontdb::ID) -> FaceMetrics {
        let mut system = self.system.borrow_mut();
        self.cache.borrow_mut().metrics(&mut system, id)
    }

    /// Run a closure with mutable access to the underlying `FontSystem`.
    ///
    /// The renderer needs this to rasterize glyphs through a
    /// [`cosmic_text::SwashCache`], which requires `&mut FontSystem`.
    pub fn with_system<R>(&self, f: impl FnOnce(&mut FontSystem) -> R) -> R {
        let mut system = self.system.borrow_mut();
        f(&mut system)
    }

    /// The default family list used for text nodes without an explicit
    /// `font_family`, if one was configured.
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

    /// The first and last line's baseline from the shaped block (the same
    /// shaping the backends draw).
    fn text_baselines(&self, text: &TextProps, max_width: f32) -> Option<TextBaselines> {
        let shaped = self.shape(text, max_width);
        Some(TextBaselines {
            first: shaped.lines.first()?.baseline,
            last: shaped.lines.last()?.baseline,
        })
    }

    /// Images contribute no intrinsic size; their shape comes from explicit
    /// size / `aspect_ratio`, else their natural aspect ([`Self::image_aspect`]).
    fn measure_image(&self, _image: &ImageProps) -> Size {
        Size::default()
    }

    /// Natural aspect from the image file header (cached per path), when an
    /// image base dir is set. Missing/unreadable files yield `None`.
    fn image_aspect(&self, image: &ImageProps) -> Option<f32> {
        let base = self.image_base_dir.as_deref()?;
        let src = Path::new(&image.src);
        let path = if src.is_absolute() {
            src.to_path_buf()
        } else {
            base.join(src)
        };
        if let Some(cached) = self.image_aspects.borrow().get(&path) {
            return *cached;
        }
        let aspect = crate::render::image_size(&path)
            .filter(|(w, h)| *w > 0 && *h > 0)
            .map(|(w, h)| w as f32 / h as f32);
        self.image_aspects.borrow_mut().insert(path, aspect);
        aspect
    }
}
