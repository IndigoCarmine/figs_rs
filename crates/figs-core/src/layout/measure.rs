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
        NodeType::Rect(_) => Size::default(),
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
