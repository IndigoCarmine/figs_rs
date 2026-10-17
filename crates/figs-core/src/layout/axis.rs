//! Axes in layout: `cross_axis_alignment = "axis"` lines a row's children up
//! by their horizontal axis lines (a column's by their vertical ones), the
//! way a reaction scheme keeps every molecule and arrow on one centre line
//! however long the captions above and below them are.
//!
//! A node's **axis point** (in its own box) is
//! - `axis = [fx, fy]`: those fractions of its box;
//! - otherwise, for a column / row / stack, the axis point of its first
//!   child (in `children` order) that is *marked* — has an `axis` of its own
//!   or a marked node somewhere inside — offset by where layout puts that
//!   child (a caption column's icon, through any nesting);
//! - otherwise the middle of its box (`axis = true` on a leaf, or any node
//!   without a mark inside).
//!
//! Like baselines (see `baseline.rs`), the point is found in the node's own
//! (unrotated) frame and then turned with it: a quarter-turned node maps its
//! point onto its footprint; a paint-only angle uses the unrotated box.

use crate::geom::{quarter_turns, Axis, AxisMark, Rect, Size};
use crate::schema::NodeType;

use super::arrange::{flex_slots, layout_stack_contained, stack_slots, Flex};
use super::{Ctx, LeafMeasure};

/// Whether node `idx` is marked as an axis provider or has one inside.
pub(super) fn has_axis_mark<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize) -> bool {
    let node = &ctx.doc.nodes[idx];
    node.common.axis.is_some() || node.kind.children().iter().any(|&c| has_axis_mark(ctx, c))
}

/// Node `idx`'s axis point `(x, y)` from the top-left corner of its
/// footprint `fp` (its border box in the parent's frame).
pub(super) fn axis_point<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, fp: Size) -> (f32, f32) {
    let node = &ctx.doc.nodes[idx];
    let k = quarter_turns(node.common.rotate).unwrap_or(0);
    let own = if k % 2 == 1 {
        Size::new(fp.h, fp.w)
    } else {
        fp
    };
    let (px, py) = own_axis(ctx, idx, own);
    // Turning clockwise by k quarters about the centre.
    match k {
        1 => (own.h - py, px),
        2 => (own.w - px, own.h - py),
        3 => (py, own.w - px),
        _ => (px, py),
    }
}

/// Where node `idx`'s axis crosses `axis` of the parent's frame, from the
/// leading edge of its footprint: the axis point's x for a horizontal
/// measure, its y for a vertical one.
pub(super) fn axis_offset<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, fp: Size, axis: Axis) -> f32 {
    let (x, y) = axis_point(ctx, idx, fp);
    match axis {
        Axis::Horizontal => x,
        Axis::Vertical => y,
    }
}

/// The axis point in the node's own (unrotated) box `own`.
fn own_axis<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, own: Size) -> (f32, f32) {
    let node = &ctx.doc.nodes[idx];
    let middle = (own.w / 2.0, own.h / 2.0);
    if let Some(AxisMark::At([fx, fy])) = node.common.axis {
        return (fx * own.w, fy * own.h);
    }
    let content = Rect::new(0.0, 0.0, own.w, own.h).inset(node.common.padding);
    let (children, slots) = match &node.kind {
        NodeType::Container { children, .. } => {
            if !children.iter().any(|&c| has_axis_mark(ctx, c)) {
                return middle;
            }
            let Some(flex) = Flex::of(ctx, idx) else {
                return middle;
            };
            (children, flex_slots(ctx, &flex, content))
        }
        NodeType::Stack {
            children,
            viewbox,
            contain,
            ..
        } => {
            if !children.iter().any(|&c| has_axis_mark(ctx, c)) {
                return middle;
            }
            let content = if *contain {
                layout_stack_contained(ctx, children, &[], *viewbox, content, None, &mut Vec::new())
            } else {
                content
            };
            (children, stack_slots(ctx, children, *viewbox, content))
        }
        _ => return middle,
    };
    children
        .iter()
        .zip(&slots)
        .find(|(&c, _)| has_axis_mark(ctx, c))
        .map_or(middle, |(&c, s)| {
            let (x, y) = axis_point(ctx, c, s.size());
            (s.x + x, s.y + y)
        })
}
