//! Pass 1 — measure. Computes each node's wrap-content (intrinsic) border-box
//! size under a set of constraints. Flex children contribute their intrinsic
//! size here; their final size is decided in [`super::arrange`].

use crate::geom::{Axis, Size};
use crate::schema::{Common, NodeType};

use super::{cross_of, main_of, Constraints, Ctx, LeafMeasure};

/// Measure node `idx` under `c`, returning its border-box size (content +
/// padding, excluding margin).
pub(crate) fn measure<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, c: Constraints) -> Size {
    let node = &ctx.doc.nodes[idx];
    let intrinsic = match &node.kind {
        NodeType::Container {
            axis,
            children,
            spacing,
            ..
        } => measure_container(ctx, *axis, children, *spacing, &node.common, c),
        NodeType::Stack { children } => measure_stack(ctx, children, &node.common, c),
        NodeType::Rect(_) | NodeType::Ellipse(_) => Size::default(),
        // Points live in the content box, so the intrinsic size is their extent
        // from its origin (not their tight bounding box) plus padding.
        NodeType::Path(props) => {
            let (w, h) = props
                .points
                .iter()
                .fold((0.0f32, 0.0f32), |(w, h), p| (w.max(p.x), h.max(p.y)));
            Size::new(
                w + node.common.padding.horizontal(),
                h + node.common.padding.vertical(),
            )
        }
        NodeType::Image(props) => ctx.measurer.measure_image(props),
        NodeType::Text(props) => {
            let avail = (c.max_w - node.common.padding.horizontal()).max(0.0);
            let inner = ctx.measurer.measure_text(props, avail);
            Size::new(
                inner.w + node.common.padding.horizontal(),
                inner.h + node.common.padding.vertical(),
            )
        }
    };

    resolve_box_size(&node.common, intrinsic, c)
}

/// Wrap-content size of a container: sum along the main axis, max along the
/// cross axis, plus spacing and padding.
fn measure_container<M: LeafMeasure>(
    ctx: &Ctx<M>,
    axis: Axis,
    children: &[usize],
    spacing: f32,
    common: &Common,
    c: Constraints,
) -> Size {
    let inner_max_w = (c.max_w - common.padding.horizontal()).max(0.0);
    let inner_max_h = (c.max_h - common.padding.vertical()).max(0.0);
    let child_c = Constraints::loose(inner_max_w, inner_max_h);

    let mut main_sum = 0.0f32;
    let mut cross_max = 0.0f32;
    for &child in children {
        let sz = measure(ctx, child, child_c);
        let margin = ctx.doc.nodes[child].common.margin;
        let outer = Size::new(sz.w + margin.horizontal(), sz.h + margin.vertical());
        main_sum += main_of(axis, outer);
        cross_max = cross_max.max(cross_of(axis, outer));
    }
    if !children.is_empty() {
        main_sum += spacing * (children.len() as f32 - 1.0);
    }

    let (w, h) = match axis {
        Axis::Vertical => (cross_max, main_sum),
        Axis::Horizontal => (main_sum, cross_max),
    };
    Size::new(w + common.padding.horizontal(), h + common.padding.vertical())
}

/// Wrap-content size of a stack: the furthest extent of any child
/// (`offset + margins + border box`) on each axis, plus padding. Children that
/// fill the stack in arrange contribute their intrinsic size here, as flex
/// children do in a row/column.
fn measure_stack<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    common: &Common,
    c: Constraints,
) -> Size {
    let inner = Size::new(
        (c.max_w - common.padding.horizontal()).max(0.0),
        (c.max_h - common.padding.vertical()).max(0.0),
    );

    let mut w = 0.0f32;
    let mut h = 0.0f32;
    for &child in children {
        let cc = &ctx.doc.nodes[child].common;
        let avail = stack_child_avail(cc, inner);
        let sz = measure(ctx, child, Constraints::loose(avail.w, avail.h));
        w = w.max(cc.x.unwrap_or(0.0) + cc.margin.horizontal() + sz.w);
        h = h.max(cc.y.unwrap_or(0.0) + cc.margin.vertical() + sz.h);
    }
    Size::new(w + common.padding.horizontal(), h + common.padding.vertical())
}

/// Space available to a stack child's border box inside a content box of size
/// `content`: from the child's `x`/`y` offset (plus leading margin) to the far
/// edge (minus trailing margin). As in rows/columns, sizes are clamped to it, so
/// a child never spills past the right/bottom edge; a negative offset may hang
/// off the top/left.
pub(super) fn stack_child_avail(common: &Common, content: Size) -> Size {
    Size::new(
        (content.w - common.x.unwrap_or(0.0) - common.margin.horizontal()).max(0.0),
        (content.h - common.y.unwrap_or(0.0) - common.margin.vertical()).max(0.0),
    )
}

/// Final border-box size of a stack child, given its `measured` size (explicit
/// `width`/`height` + `aspect_ratio` over intrinsic) and its available space.
///
/// Per axis: an explicit size wins; otherwise an axis with **no offset** (`x`
/// unset for width, `y` unset for height) **fills** `avail`, and an axis with an
/// offset keeps its measured (intrinsic) size. So an unpositioned, unsized child
/// is a full-size base layer, while `.at(x, y)` labels wrap their content.
///
/// `aspect_ratio` treats a filled side like a known one: it derives a
/// non-explicit, non-filling side from a filling one, an explicit side pins the
/// other (which then does not fill), and when both sides would fill the child is
/// the largest box of that ratio that fits (`contain`, anchored top-left).
pub(super) fn stack_child_size(common: &Common, measured: Size, avail: Size) -> Size {
    let ar = common.aspect_ratio.filter(|a| *a > 0.0);
    let pinned_w = common.width.is_some() || (ar.is_some() && common.height.is_some());
    let pinned_h = common.height.is_some() || (ar.is_some() && common.width.is_some());
    let fill_w = !pinned_w && common.x.is_none();
    let fill_h = !pinned_h && common.y.is_none();

    let mut w = if fill_w { avail.w } else { measured.w };
    let mut h = if fill_h { avail.h } else { measured.h };
    if let Some(ar) = ar {
        match (fill_w, fill_h) {
            (true, true) => {
                if avail.w / ar <= avail.h {
                    h = avail.w / ar;
                } else {
                    w = avail.h * ar;
                }
            }
            (true, false) => h = w / ar,
            (false, true) => w = h * ar,
            // measured already applied the ratio to explicit/intrinsic sides.
            (false, false) => {}
        }
    }
    Size::new(w.min(avail.w), h.min(avail.h))
}

/// Apply explicit `width`/`height` and `aspect_ratio` over an intrinsic size,
/// then clamp to constraints.
///
/// `aspect_ratio` is width / height. An explicit dimension wins; the other is
/// derived from it via the ratio when only one is fixed.
pub(super) fn resolve_box_size(common: &Common, intrinsic: Size, c: Constraints) -> Size {
    let mut w = common.width.unwrap_or(intrinsic.w);
    let mut h = common.height.unwrap_or(intrinsic.h);

    if let Some(ar) = common.aspect_ratio {
        if ar > 0.0 {
            match (common.width.is_some(), common.height.is_some()) {
                // both explicit: leave as-is (explicit wins over ratio)
                (true, true) => {}
                // width known -> derive height
                (true, false) => h = w / ar,
                // height known -> derive width
                (false, true) => w = h * ar,
                // neither explicit: prefer the larger intrinsic dimension as the
                // driver so a known intrinsic side is preserved.
                (false, false) => {
                    if intrinsic.w >= intrinsic.h {
                        h = w / ar;
                    } else {
                        w = h * ar;
                    }
                }
            }
        }
    }

    c.clamp(Size::new(w, h))
}
