//! Image decoding and caching. Images are decoded once into straight RGBA8 and
//! cached by absolute path with their modification time, so a live-reload only
//! re-decodes files that actually changed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use crate::geom::Size;
use crate::schema::ImageProps;
use crate::units::PT_PER_INCH;

/// Nominal resolution used to give an image an intrinsic point size when it is
/// otherwise unconstrained (CSS pixel convention: 96 px per inch).
pub const NOMINAL_DPI: f32 = 96.0;

/// A decoded image: straight (non-premultiplied) RGBA8, row-major.
#[derive(Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl DecodedImage {
    /// Intrinsic size in points, assuming [`NOMINAL_DPI`].
    pub fn intrinsic_pt(&self) -> Size {
        Size::new(
            self.width as f32 * PT_PER_INCH / NOMINAL_DPI,
            self.height as f32 * PT_PER_INCH / NOMINAL_DPI,
        )
    }
}

struct CacheEntry {
    mtime: Option<SystemTime>,
    image: Rc<DecodedImage>,
}

/// Decodes and caches images relative to a base directory (the document's
/// folder), so `src` paths in the TOML resolve the way a user expects.
pub struct ImageStore {
    base: PathBuf,
    cache: RefCell<HashMap<PathBuf, CacheEntry>>,
}

impl ImageStore {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        ImageStore {
            base: base.into(),
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn resolve(&self, src: &Path) -> PathBuf {
        if src.is_absolute() {
            src.to_path_buf()
        } else {
            self.base.join(src)
        }
    }

    /// Get a decoded image for `src`, decoding (and caching) on first use and
    /// re-decoding when the file's mtime changes. Returns `None` (and logs) if
    /// the file is missing or cannot be decoded.
    pub fn get(&self, src: &Path) -> Option<Rc<DecodedImage>> {
        let path = self.resolve(src);
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();

        if let Some(entry) = self.cache.borrow().get(&path) {
            if entry.mtime == mtime {
                return Some(Rc::clone(&entry.image));
            }
        }

        let image = match decode(&path) {
            Ok(img) => Rc::new(img),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "failed to load image");
                return None;
            }
        };
        self.cache.borrow_mut().insert(
            path,
            CacheEntry {
                mtime,
                image: Rc::clone(&image),
            },
        );
        Some(image)
    }

    /// Intrinsic point size of an image, or zero if it can't be loaded.
    pub fn measure(&self, props: &ImageProps) -> Size {
        match self.get(Path::new(&props.src)) {
            Some(img) => img.intrinsic_pt(),
            None => Size::default(),
        }
    }
}

fn decode(path: &Path) -> Result<DecodedImage, image::ImageError> {
    let dynimg = image::open(path)?;
    let rgba = dynimg.to_rgba8();
    Ok(DecodedImage {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}
