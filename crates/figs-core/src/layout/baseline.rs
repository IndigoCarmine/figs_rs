//! Text baselines in layout: `anchor` components `"baseline"` /
//! `"last_baseline"` in stacks, and `cross_axis_alignment = "baseline"` in
//! rows.
//!
//! A text node's baseline is a horizontal line of its own (unrotated) box,
//! `TextBaselines` below the box top (where the backends draw the block,
//! shaped at the box width). Anchors and alignment act in the **parent's
//! frame**, on the node's footprint, so the baseline is mapped through the
//! node's rotation first: upright (and `180°`) text has a baseline across
//! the parent's vertical axis, a quarter-turned one across the horizontal
//! axis. Any other angle is paint-only, so the unrotated box is used.

use crate::geom::{quarter_turns, AnchorValue, Axis, BaselineKind, Edges, Size};
use crate::schema::NodeType;

use super::{Ctx, LeafMeasure, TextBaselines};

/// Where node `idx`'s `kind` baseline crosses `axis` of the parent's frame,
/// as a distance from the leading (left/top) edge of its footprint `fp` (its
/// border box in the parent's frame). `None` for anything but a text node,
/// and when its baseline runs along `axis` instead of across it.
pub(super) fn baseline_offset<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    fp: Size,
    axis: Axis,
    kind: BaselineKind,
) -> Option<f32> {
    let node = &ctx.doc.nodes[idx];
    let NodeType::Text(props) = &node.kind else {
        return None;
    };
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
    let b = ctx
        .measurer
        .text_baselines(props, own.w)
        .unwrap_or_else(|| TextBaselines::synthetic(props, own.h))
        .get(kind);
    // Turning clockwise by k quarters carries the box top to the top (0),
    // right (1), bottom (2) or left (3) side of the footprint.
    Some(match k {
        1 | 2 => own.h - b,
        _ => b,
    })
}

/// The offset `(dx, dy)` from a stack child's footprint top-left corner to
/// its anchor point: per axis the fraction × the footprint size, or where
/// the named baseline crosses that axis (resolve makes sure there is one;
/// the far edge otherwise).
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
/// none across the row (not a text node, or quarter-turned text) — the
/// bottom of its border box.
pub(super) fn row_baseline<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    fp: Size,
    margin: Edges,
) -> f32 {
    margin.top + baseline_offset(ctx, idx, fp, Axis::Vertical, BaselineKind::First).unwrap_or(fp.h)
}
