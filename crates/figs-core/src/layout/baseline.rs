//! Baselines in layout: `anchor` components `"baseline"` /
//! `"last_baseline"` in stacks, and `cross_axis_alignment = "baseline"` in
//! rows.
//!
//! A text node's baseline is a horizontal line of its own (unrotated) box,
//! `TextBaselines` below the box top (where the backends draw the block,
//! shaped at the box width). A container (`column` / `row` / `stack`) has the
//! baseline of a child: its **first** baseline is the first baseline of its
//! first child (in `children` order) that has one, its **last** baseline the
//! last baseline of its last child that has one, offset by where that child
//! is laid out in the container (CSS inline-block, Flutter
//! `RenderFlex.computeDistanceToActualBaseline`). In a baseline-aligned row
//! every text child shares that line. A container with no text anywhere
//! inside has none.
//!
//! Anchors and alignment act in the **parent's frame**, on the node's
//! footprint, so the baseline is mapped through the node's rotation first:
//! upright (and `180°`) nodes have a baseline across the parent's vertical
//! axis, quarter-turned ones across the horizontal axis. Any other angle is
//! paint-only, so the unrotated box is used. A container's baseline is found
//! in its own (unrotated) frame — a child turned inside it counts with its
//! own baseline only when that runs horizontally in the container — and then
//! turns with the container like a text's.

use crate::geom::{quarter_turns, AnchorValue, Axis, BaselineKind, Edges, Rect, Size};
use crate::schema::NodeType;

use super::arrange::{flex_slots, layout_stack_contained, stack_slots, Flex};
use super::{Ctx, LeafMeasure, TextBaselines};

/// Where node `idx`'s `kind` baseline crosses `axis` of the parent's frame,
/// as a distance from the leading (left/top) edge of its footprint `fp` (its
/// border box in the parent's frame). `None` for a node without a baseline
/// (not text, nor a container with a text inside), and when its baseline
/// runs along `axis` instead of across it.
pub(super) fn baseline_offset<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    fp: Size,
    axis: Axis,
    kind: BaselineKind,
) -> Option<f32> {
    if !has_text(ctx, idx) {
        return None;
    }
    let node = &ctx.doc.nodes[idx];
    let k = quarter_turns(node.common.rotate).unwrap_or(0);
    let odd = k % 2 == 1;
    let across = if odd {
        Axis::Horizontal
    } else {
        Axis::Vertical
    };
    if axis != across {
        return None;
    }
    // The node's own box: the footprint, with its sides swapped back for a
    // quarter turn.
    let own = if odd { Size::new(fp.h, fp.w) } else { fp };
    let b = own_baseline(ctx, idx, own, kind)?;
    // Turning clockwise by k quarters carries the box top to the top (0),
    // right (1), bottom (2) or left (3) side of the footprint.
    Some(match k {
        1 | 2 => own.h - b,
        _ => b,
    })
}

/// Whether node `idx` is a text node or has one somewhere inside: only
/// those can have a baseline.
fn has_text<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize) -> bool {
    match &ctx.doc.nodes[idx].kind {
        NodeType::Text(_) => true,
        kind => kind.children().iter().any(|&c| has_text(ctx, c)),
    }
}

/// Node `idx`'s `kind` baseline in its own (unrotated) frame: the distance
/// from the top of its box, `own` large.
fn own_baseline<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    own: Size,
    kind: BaselineKind,
) -> Option<f32> {
    let node = &ctx.doc.nodes[idx];
    let content = Rect::new(0.0, 0.0, own.w, own.h).inset(node.common.padding);
    let (children, slots) = match &node.kind {
        NodeType::Text(props) => {
            return Some(
                ctx.measurer
                    .text_baselines(props, own.w)
                    .unwrap_or_else(|| TextBaselines::synthetic(props, own.h))
                    .get(kind),
            )
        }
        NodeType::Container { children, .. } => {
            let flex = Flex::of(ctx, idx)?;
            (children, flex_slots(ctx, &flex, content, false))
        }
        NodeType::Stack {
            children,
            viewbox,
            contain,
            ..
        } => {
            // A contained stack lays its children out in a shrunken box.
            let content = if *contain {
                layout_stack_contained(ctx, children, *viewbox, content, None, &mut Vec::new())
            } else {
                content
            };
            (children, stack_slots(ctx, children, *viewbox, content))
        }
        _ => return None,
    };
    let mut placed = children.iter().zip(&slots);
    let at = |(&c, s): (&usize, &Rect)| {
        baseline_offset(ctx, c, s.size(), Axis::Vertical, kind).map(|b| s.y + b)
    };
    match kind {
        BaselineKind::First => placed.find_map(at),
        BaselineKind::Last => placed.rev().find_map(at),
    }
}

/// The offset `(dx, dy)` from a stack child's footprint top-left corner to
/// its anchor point: per axis the fraction × the footprint size, or where
/// the named baseline crosses that axis (the far edge when there is none:
/// a container without any text inside).
pub(super) fn anchor_shift<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    anchor: Option<[AnchorValue; 2]>,
    fp: Size,
) -> (f32, f32) {
    let [ax, ay] = anchor.unwrap_or_default();
    let one = |v: AnchorValue, axis: Axis, len: f32| match v {
        AnchorValue::Frac(f) => f * len,
        AnchorValue::Baseline(k) => baseline_offset(ctx, idx, fp, axis, k).unwrap_or(len),
    };
    (
        one(ax, Axis::Horizontal, fp.w),
        one(ay, Axis::Vertical, fp.h),
    )
}

/// Distance from the top of a row child's margin box to the line it sits on
/// under `cross_axis_alignment = "baseline"`: its first baseline, or — with
/// none across the row (no text in it, or quarter-turned) — the bottom of
/// its border box.
pub(super) fn row_baseline<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    fp: Size,
    margin: Edges,
) -> f32 {
    margin.top + baseline_offset(ctx, idx, fp, Axis::Vertical, BaselineKind::First).unwrap_or(fp.h)
}
