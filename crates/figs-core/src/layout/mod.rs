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
//!
//! ## Auto page height
//!
//! With `page.height = "auto"` the root is first measured with the page width
//! **tight** (fixed) and the height **unbounded**; the measured height becomes
//! the page height, then arrange runs as usual. Under the unbounded vertical axis:
//!
//! - a column's children — flex or not — take their measured (intrinsic) height,
//!   like Flutter flex children in an unbounded main axis. `flex` is ignored
//!   along that axis (there is no free space to share), so a `grid` of equal-flex
//!   rows gets rows sized by their content instead of equal shares.
//! - widths are decided as on a fixed page: a stretched cross axis or a flex
//!   share hands the child a tight width, and `aspect_ratio` derives the height
//!   from it.
//! - an image with no `aspect_ratio` and no explicit height uses its natural
//!   aspect ([`LeafMeasure::image_aspect`]) or, when that is unknown (missing
//!   file, or a measurer without image access), [`FALLBACK_IMAGE_ASPECT`] (4:3),
//!   so its height is never zero once its width is known.
//! - if the measured height is still zero or non-finite, the page falls back to
//!   a 4:3 (width × 3/4) height with a warning; layout never panics.

pub mod arrange;
pub mod computed;
pub mod measure;

pub use computed::{ComputedLayout, ComputedNode, PageGeom, PaintContent, PathPaint, TextBox};

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

    /// Force the width to exactly `w` (a tight width), keeping the height range.
    pub fn with_tight_w(self, w: f32) -> Self {
        let w = w.max(0.0);
        Constraints {
            min_w: w,
            max_w: w,
            ..self
        }
    }

    /// Force the height to exactly `h` (a tight height), keeping the width range.
    pub fn with_tight_h(self, h: f32) -> Self {
        let h = h.max(0.0);
        Constraints {
            min_h: h,
            max_h: h,
            ..self
        }
    }

    /// True when the width is fixed to a single finite value.
    pub fn is_tight_w(&self) -> bool {
        self.max_w.is_finite() && self.min_w >= self.max_w
    }

    /// True when the height is fixed to a single finite value.
    pub fn is_tight_h(&self) -> bool {
        self.max_h.is_finite() && self.min_h >= self.max_h
    }
}

/// Aspect ratio (width / height) assumed for an image whose natural size is
/// unknown (e.g. a missing file drawn as a placeholder) when it has no
/// `aspect_ratio` and no explicit height. 4:3 matches the classic figure panel.
pub const FALLBACK_IMAGE_ASPECT: f32 = 4.0 / 3.0;

/// Provides intrinsic sizes for leaf content the schema can't size on its own.
///
/// Implemented by the text backend (cosmic-text) and the image module. The
/// engine never reads fonts or image files directly.
pub trait LeafMeasure {
    /// Wrap-content size of a text block given the available width (points).
    fn measure_text(&self, text: &TextProps, max_width: f32) -> Size;
    /// Intrinsic size of an image (points), before aspect/flex constraints.
    fn measure_image(&self, image: &ImageProps) -> Size;
    /// Natural aspect ratio (width / height) of an image, if known (e.g. read
    /// from the file header). Used to derive an image's height from its width
    /// when it has no `aspect_ratio`; `None` falls back to
    /// [`FALLBACK_IMAGE_ASPECT`].
    fn image_aspect(&self, _image: &ImageProps) -> Option<f32> {
        None
    }
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
    /// Per node index: is this node's height content-derived (an auto-height
    /// root, or a child of a content-sized column)? Empty on fixed-height pages.
    pub content_h: Vec<bool>,
}

impl<M: LeafMeasure> Ctx<'_, M> {
    /// Whether node `idx` takes its measured height (unbounded vertical axis).
    pub fn is_content_h(&self, idx: usize) -> bool {
        self.content_h.get(idx).copied().unwrap_or(false)
    }
}

/// Mark the nodes whose height is content-derived on an auto-height page: the
/// root, and recursively every child of a content-sized column (a column in an
/// unbounded vertical axis gives each child its measured height). Rows and
/// stacks hand their children a known height, so the marking stops there.
fn content_sized_heights(doc: &Document) -> Vec<bool> {
    let mut marks = vec![false; doc.nodes.len()];
    let mut stack = vec![doc.root];
    while let Some(i) = stack.pop() {
        // A quarter-turned node's own vertical axis is its parent's
        // horizontal one, which is bounded: neither it nor its subtree is
        // content-sized.
        if doc.nodes[i].common.swaps_axes() {
            continue;
        }
        if std::mem::replace(&mut marks[i], true) {
            continue; // resolve rejects cycles; guard anyway
        }
        if let crate::schema::NodeType::Container {
            axis: Axis::Vertical,
            children,
            ..
        } = &doc.nodes[i].kind
        {
            stack.extend(children.iter().copied());
        }
    }
    marks
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
///
/// For an auto-height page (`doc.page.height_pt == None`) the root is measured
/// with the page width fixed and the height unbounded, and the result is the
/// page height recorded in [`PageGeom::height_pt`] (with
/// [`PageGeom::auto_height`] set). See the module docs for the semantics.
pub fn layout<M: LeafMeasure>(doc: &Document, measurer: &M) -> ComputedLayout {
    let root = &doc.nodes[doc.root];
    let auto = doc.page.height_pt.is_none();

    let mut ctx = Ctx {
        doc,
        measurer,
        content_h: Vec::new(),
    };
    let height_pt = match doc.page.height_pt {
        Some(h) => h,
        None => {
            ctx.content_h = content_sized_heights(doc);
            auto_page_height(&ctx, doc.page.width_pt)
        }
    };

    let page = PageGeom {
        width_pt: doc.page.width_pt,
        height_pt,
        dpi: doc.page.dpi,
        background: doc.page.background,
        auto_height: auto,
    };

    // The root fills the whole page, inset by its own margin.
    let page_rect = crate::geom::Rect::new(0.0, 0.0, page.width_pt, page.height_pt);
    let root_box = page_rect.inset(root.common.margin);

    let mut nodes = Vec::with_capacity(doc.nodes.len());
    arrange::arrange(&ctx, doc.root, root_box, None, &mut nodes);

    ComputedLayout { page, nodes }
}

/// Page height (points) that fits the root: measure it with the page width
/// (minus its margin) tight and the height unbounded, then add the vertical
/// margin back. Zero/non-finite results fall back to a 4:3 page with a warning.
fn auto_page_height<M: LeafMeasure>(ctx: &Ctx<M>, width_pt: f32) -> f32 {
    let margin = ctx.doc.nodes[ctx.doc.root].common.margin;
    let avail_w = (width_pt - margin.horizontal()).max(0.0);
    let c = Constraints::loose(avail_w, f32::INFINITY).with_tight_w(avail_w);
    let measured = measure::measure(ctx, ctx.doc.root, c);
    let h = measured.h + margin.vertical();
    if h.is_finite() && h > 0.0 {
        h
    } else {
        let fallback = (width_pt / FALLBACK_IMAGE_ASPECT).max(1.0);
        tracing::warn!(
            measured = h,
            fallback,
            "auto page height: content has no height; using a 4:3 page"
        );
        fallback
    }
}
