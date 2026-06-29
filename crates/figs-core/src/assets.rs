//! [`Assets`] bundles the font and image stores and implements [`LeafMeasure`],
//! so a single value drives both layout (measuring/shaping) and rendering
//! (glyph rasterization + image decoding).

use std::path::PathBuf;

use crate::geom::Size;
use crate::image::ImageStore;
use crate::layout::LeafMeasure;
use crate::schema::{ImageProps, TextProps};
use crate::text::{FontStore, ShapedText};

/// Fonts + images for a document. `base_dir` is the folder image `src` paths
/// resolve against (normally the document's directory).
pub struct Assets {
    pub fonts: FontStore,
    pub images: ImageStore,
}

impl Assets {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Assets {
            fonts: FontStore::new(),
            images: ImageStore::new(base_dir),
        }
    }

    /// Set the default font family for text without an explicit `font_family`.
    pub fn with_default_family(mut self, family: impl Into<String>) -> Self {
        self.fonts = self.fonts.with_default_family(family);
        self
    }
}

impl LeafMeasure for Assets {
    fn shape_text(&self, text: &TextProps, max_width: f32) -> ShapedText {
        self.fonts.shape(text, max_width)
    }

    fn measure_image(&self, image: &ImageProps) -> Size {
        self.images.measure(image)
    }
}
