//! The layout engine: a pure function from a resolved [`Document`] to a
//! [`ComputedLayout`]. Modeled on Flutter's `BoxConstraints` / `RenderFlex`.
//!
//! Two passes:
//! - [`measure`] (bottom-up) computes each node's wrap-content (intrinsic) size,
//! - [`arrange`] (top-down) distributes space, resolves flex, and produces the
//!   absolute rects in the IR.
//!
//! Leaf content whose size can't be known from the schema alone (text, images)
//! is sized through the [`LeafMeasure`] trait, so the engine itself stays free
//! of font and image I/O.

pub mod arrange;
pub mod computed;
pub mod measure;

pub use computed::{ComputedLayout, ComputedNode, PageGeom, PaintContent, TextBox};

use crate::geom::{Axis, Size};
use crate::schema::{Document, ImageProps, TextProps};

/// Box constraints handed down during measurement: a min/max range per axis.
#[derive(Debug, Clone, Copy)]
pub struct Constraints {
    pub min_w: f32,
    pub max_w: f32,
    pub min_h: f32,
    pub max_h: f32,
}

impl Constraints {
    /// Loose constraints: anything from zero up to the given maxima.
    pub fn loose(max_w: f32, max_h: f32) -> Self {
        Constraints {
            min_w: 0.0,
            max_w,
            min_h: 0.0,
            max_h,
        }
    }

    pub fn clamp_w(&self, w: f32) -> f32 {
        w.clamp(self.min_w, self.max_w)
    }

    pub fn clamp_h(&self, h: f32) -> f32 {
        h.clamp(self.min_h, self.max_h)
    }

    pub fn clamp(&self, s: Size) -> Size {
        Size::new(self.clamp_w(s.w), self.clamp_h(s.h))
    }
}

/// Provides intrinsic sizes for leaf content the schema can't size on its own.
///
/// Implemented by the text backend (cosmic-text) and the image module. The
/// engine never reads fonts or image files directly.
pub trait LeafMeasure {
    /// Wrap-content size of a text block given the available width (points).
    fn measure_text(&self, text: &TextProps, max_width: f32) -> Size;
    /// Intrinsic size of an image (points), before aspect/flex constraints.
    fn measure_image(&self, image: &ImageProps) -> Size;
}

/// A measurer that reports zero size for all leaf content. Useful for
/// container/rect-only layouts and tests where explicit sizes are given.
pub struct NullMeasurer;

impl LeafMeasure for NullMeasurer {
    fn measure_text(&self, _text: &TextProps, _max_width: f32) -> Size {
        Size::default()
    }
    fn measure_image(&self, _image: &ImageProps) -> Size {
        Size::default()
    }
}

/// Shared context threaded through both passes.
pub(crate) struct Ctx<'a, M: LeafMeasure> {
    pub doc: &'a Document,
    pub measurer: &'a M,
}

/// Project a [`Size`] onto the main/cross axes of a container.
pub(crate) fn main_of(axis: Axis, s: Size) -> f32 {
    match axis {
        Axis::Vertical => s.h,
        Axis::Horizontal => s.w,
    }
}

pub(crate) fn cross_of(axis: Axis, s: Size) -> f32 {
    match axis {
        Axis::Vertical => s.w,
        Axis::Horizontal => s.h,
    }
}

/// Run the full layout for a document.
pub fn layout<M: LeafMeasure>(doc: &Document, measurer: &M) -> ComputedLayout {
    let ctx = Ctx { doc, measurer };
    let page = PageGeom {
        width_pt: doc.page.width_pt,
        height_pt: doc.page.height_pt,
        dpi: doc.page.dpi,
        background: doc.page.background,
    };

    // The root fills the whole page, inset by its own margin.
    let root = &doc.nodes[doc.root];
    let page_rect = crate::geom::Rect::new(0.0, 0.0, page.width_pt, page.height_pt);
    let root_box = page_rect.inset(root.common.margin);

    let mut nodes = Vec::with_capacity(doc.nodes.len());
    arrange::arrange(&ctx, doc.root, root_box, &mut nodes);

    ComputedLayout { page, nodes }
}
