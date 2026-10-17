//! Pass 1 — measure. Computes each node's wrap-content (intrinsic) border-box
//! size under a set of constraints. Flex children contribute their intrinsic
//! size here; their final size is decided in [`super::arrange`].
//!
//! **Tight constraints.** When a container's own size on an axis is fixed (a
//! *tight* constraint: `min == max`), its children's sizes on that axis are
//! predictable before arrange, so measure hands them down: a stretched cross axis
//! gives each child a tight cross size, and a tight main axis gives each flex
//! child its flex share. A leaf with `aspect_ratio` then derives its other side
//! from the tight one — which is how rows of aspect-ratio images report a real
//! height (needed by auto-height pages). Loose constraints keep the plain
//! wrap-content behaviour — except for an image with neither side known, which
//! has no content to wrap: it takes the largest box of its aspect that fits the
//! loose constraints (full available width, or full height if that is the
//! tighter limit), so it is never silently drawn at 0×0.

use crate::geom::{Axis, CrossAxisAlign, Edges, Rect, Size};
use crate::schema::{Common, ImageProps, NodeType};

use super::arrange::{flex_slots, stack_slots, Flex};
use super::axis::axis_offset;
use super::baseline::{anchor_shift, pitch_gaps, row_baseline, row_line};
use super::{cross_of, main_of, Constraints, Ctx, LeafMeasure, FALLBACK_IMAGE_ASPECT};

/// Measure node `idx` under `c`, returning its border-box size (content +
/// padding, excluding margin) **in the parent's frame**.
///
/// A node turned by an odd quarter turn (`rotate = ±90`, `270`, …) is
/// measured in its own frame with the constraints' axes swapped, and its size
/// is swapped back: the parent sees the rotated footprint (Flutter's
/// `RotatedBox`). Any other rotation is paint-only and doesn't affect size.
pub(crate) fn measure<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, c: Constraints) -> Size {
    if ctx.doc.nodes[idx].common.swaps_axes() {
        // Its own width is the parent's height: not sized to content.
        let s = ctx.content_width(false, || measure_own(ctx, idx, swap_constraints(c)));
        return Size::new(s.h, s.w);
    }
    if c.is_tight_w() && ctx.content_w.get() {
        // A width decided for it: what is inside may fill it.
        return ctx.content_width(false, || measure_own(ctx, idx, c));
    }
    measure_own(ctx, idx, c)
}

/// `c` with its width and height ranges exchanged.
fn swap_constraints(c: Constraints) -> Constraints {
    Constraints {
        min_w: c.min_h,
        max_w: c.max_h,
        min_h: c.min_w,
        max_h: c.max_w,
    }
}

/// [`measure`] in the node's own (unrotated) frame.
fn measure_own<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, c: Constraints) -> Size {
    let node = &ctx.doc.nodes[idx];
    let intrinsic = match &node.kind {
        NodeType::Container {
            axis,
            children,
            spacing,
            baseline_pitch,
            cross_axis,
            equal_height,
            ..
        } => measure_container(
            ctx,
            *axis,
            children,
            (*spacing, baseline_pitch.filter(|_| *axis == Axis::Vertical)),
            (*cross_axis, *equal_height && *axis == Axis::Horizontal),
            &node.common,
            // Its children are measured in the box it will have.
            explicit_tight(&node.common, c),
        ),
        NodeType::Stack {
            children,
            viewbox,
            contain,
            ..
        } => measure_stack(
            ctx,
            children,
            viewbox.is_some(),
            *contain,
            &node.common,
            explicit_tight(&node.common, c),
        ),
        NodeType::Rect(_) | NodeType::Ellipse(_) | NodeType::Connector(_) => Size::default(),
        // Coordinates live in the content box, so the intrinsic size is the
        // outline's extent from its origin (exact curve bounds, not control
        // points; not the tight bounding box) plus padding. Viewbox coordinates
        // are relative to the final box, so such a path has no intrinsic size.
        NodeType::Path(props) if props.viewbox.is_some() => Size::new(
            node.common.padding.horizontal(),
            node.common.padding.vertical(),
        ),
        NodeType::Path(props) => {
            let (w, h) = crate::path::bounds(&props.segments)
                .map_or((0.0, 0.0), |(_, hi)| (hi.x.max(0.0), hi.y.max(0.0)));
            Size::new(
                w + node.common.padding.horizontal(),
                h + node.common.padding.vertical(),
            )
        }
        NodeType::Image(props) => {
            if let Some(nat) = natural_size(ctx, props) {
                return measure_natural(&node.common, nat, c);
            }
            let intrinsic = ctx.measurer.measure_image(props);
            if intrinsic.w > 0.0 || intrinsic.h > 0.0 {
                intrinsic
            } else {
                // No intrinsic size: the image's shape is its `aspect_ratio`,
                // else its natural aspect (if the measurer can read it), else
                // the documented 4:3 fallback.
                let ar = node
                    .common
                    .aspect_ratio
                    .or_else(|| ctx.measurer.image_aspect(props))
                    .filter(|a| a.is_finite() && *a > 0.0)
                    .unwrap_or(FALLBACK_IMAGE_ASPECT);
                let common = Common {
                    aspect_ratio: Some(ar),
                    ..node.common.clone()
                };
                let known_side = common.width.is_some()
                    || common.height.is_some()
                    || c.is_tight_w()
                    || c.is_tight_h();
                if known_side {
                    return resolve_box_size(&common, intrinsic, c);
                }
                // Neither side known (loose constraints, e.g. a centred
                // column): fill the space like `img { max-width: 100% }`.
                return c.clamp(contain(ar, c.max_w, c.max_h));
            }
        }
        NodeType::Text(props) => {
            // Text wraps at its box width: an explicit `width` (clamped to
            // `c`, as the box will be), else the most `c` allows. Unwrapped
            // text takes its natural width (then clamped to `c`). A
            // `fit_width` text is shaped to its box width when it has one
            // (explicit, or tight from the parent); otherwise it is natural.
            let pad = node.common.padding.horizontal();
            let explicit_w = node.common.width.map(|w| c.clamp_w(w));
            let inner = |w: f32| (w - pad).max(0.0);
            let avail = if props.fit_width.is_some() {
                explicit_w
                    .or(c.is_tight_w().then_some(c.max_w))
                    .map_or(f32::INFINITY, inner)
            } else if props.wrap {
                inner(explicit_w.unwrap_or(c.max_w))
            } else {
                f32::INFINITY
            };
            let inner = ctx.measurer.measure_text(props, avail);
            Size::new(
                inner.w + node.common.padding.horizontal(),
                inner.h + node.common.padding.vertical(),
            )
        }
    };

    // A container with an aspect ratio and no side known (loose constraints
    // — a plot in a centred column, or wrapped in a plain `col`) would wrap
    // its content, which says little about its size (a plot's labels):
    // like an image, it takes the largest box of its aspect that fits.
    if let Some(ar) = loose_aspect(&node.common, c) {
        if matches!(
            node.kind,
            NodeType::Container { .. } | NodeType::Stack { .. }
        ) {
            let fit = contain(ar, c.max_w, c.max_h);
            if fit.w > 0.0 && fit.h > 0.0 {
                return c.clamp(fit);
            }
        }
    }
    resolve_box_size(&node.common, intrinsic, c)
}

/// The aspect ratio of a node that has one but no side decided: no explicit
/// `width` / `height` and no tight constraint.
fn loose_aspect(common: &Common, c: Constraints) -> Option<f32> {
    let ar = common.aspect_ratio.filter(|a| a.is_finite() && *a > 0.0)?;
    let known =
        common.width.is_some() || common.height.is_some() || c.is_tight_w() || c.is_tight_h();
    (!known).then_some(ar)
}

/// Wrap-content size of a container: sum along the main axis, max along the
/// cross axis, plus spacing and padding.
///
/// With a tight cross axis and `stretch`, children are measured at the tight
/// cross size; with a tight main axis, flex children are measured at their flex
/// share (as arrange will size them). Otherwise flex children contribute their
/// intrinsic size — in particular under an unbounded main axis. An
/// `equal_height` row with a bounded width fills it (see
/// [`equal_height_widths`]).
///
/// A row whose first (last) child has an overhanging caption reaching past
/// its left (right) edge keeps that much room at that end (see
/// [`overhang_reach`]): its items are measured in what is left.
pub(super) fn measure_container<M: LeafMeasure>(
    ctx: &Ctx<M>,
    axis: Axis,
    children: &[usize],
    (spacing, pitch): (f32, Option<f32>),
    (cross_align, equal_height): (CrossAxisAlign, bool),
    common: &Common,
    c: Constraints,
) -> Size {
    let args = ((spacing, pitch), (cross_align, equal_height));
    ctx.scaled(equal_height, || {
        measure_container_edges(ctx, axis, children, args, common, c)
    })
}

/// [`measure_container`] keeping room for edge captions.
#[allow(clippy::type_complexity)]
fn measure_container_edges<M: LeafMeasure>(
    ctx: &Ctx<M>,
    axis: Axis,
    children: &[usize],
    args: ((f32, Option<f32>), (CrossAxisAlign, bool)),
    common: &Common,
    c: Constraints,
) -> Size {
    let (s, edge) = measure_container_in(ctx, axis, children, args, common, c);
    if edge <= 0.01 {
        return s;
    }
    let c2 = Constraints {
        min_w: (c.min_w - edge).max(0.0),
        max_w: (c.max_w - edge).max(0.0),
        ..c
    };
    let (s2, _) = measure_container_in(ctx, axis, children, args, common, c2);
    Size::new(s2.w + edge, s2.h)
}

/// [`measure_container`] in `c`, and the room a row's edge children's
/// overhanging captions reach out past its ends (0 for a column).
#[allow(clippy::type_complexity)]
fn measure_container_in<M: LeafMeasure>(
    ctx: &Ctx<M>,
    axis: Axis,
    children: &[usize],
    ((spacing, pitch), (cross_align, equal_height)): ((f32, Option<f32>), (CrossAxisAlign, bool)),
    common: &Common,
    c: Constraints,
) -> (Size, f32) {
    let inner_max_w = (c.max_w - common.padding.horizontal()).max(0.0);
    let inner_max_h = (c.max_h - common.padding.vertical()).max(0.0);
    let base = Constraints::loose(inner_max_w, inner_max_h);
    let inner = Size::new(inner_max_w, inner_max_h);
    let (main_tight, cross_tight) = match axis {
        Axis::Vertical => (c.is_tight_h(), c.is_tight_w()),
        Axis::Horizontal => (c.is_tight_w(), c.is_tight_h()),
    };
    // How wide the column is, for its "auto" overhanging children: the
    // room it has when that is decided, else as wide as the rest.
    let auto_cross = std::cell::Cell::new(cross_of(axis, inner));
    let child_c = |child: usize, main: Option<f32>| {
        let common = &ctx.doc.nodes[child].common;
        let mut cc = base;
        if common.overhang {
            // Laid out at its natural cross size, whatever the room (an
            // "auto" one: the room its row found beside the parent).
            let room = overhang_limit(ctx, child, axis, auto_cross.get());
            cc = match axis {
                Axis::Vertical => Constraints { max_w: room, ..cc },
                Axis::Horizontal => Constraints { max_h: room, ..cc },
            };
        } else if cross_tight && stretches(ctx, child, axis, cross_align) {
            cc = tighten(
                cc,
                cross_axis(axis),
                cross_of(axis, inner) - cross_margin(axis, common.margin),
            );
        }
        if let Some(m) = main {
            cc = tighten(cc, axis, m);
        }
        cc
    };

    let gaps = if children.is_empty() {
        0.0
    } else {
        spacing * (children.len() as f32 - 1.0)
    };
    // An equal-height row fills its width: the widths of the children that
    // scale with it (no bounded width: measured as usual).
    let shared = (equal_height && inner_max_w.is_finite())
        .then(|| equal_height_widths(ctx, children, inner_max_w, gaps, base));
    // Flex children share the main axis when it is tight, exactly as
    // arrange will; in a row also when its width is only bounded: a row with
    // flex children takes all the width it may (Flutter's
    // `MainAxisSize.max`), so they are measured at the width they get, not
    // at their intrinsic one (an aspect-ratio panel would claim it all).
    //
    // Not when the row is itself sized to its content (an "auto" cell, a
    // child of a row without flex): there its flex children are measured
    // at their own size, at most their share (`capped`), so the row is as
    // wide as what it holds.
    let row_bounded = axis == Axis::Horizontal && inner_max_w.is_finite();
    let capped = row_bounded && !main_tight && ctx.content_w.get();
    let main_bounded = main_tight || (row_bounded && !capped);
    let flex_of = |child: usize| {
        ctx.doc.nodes[child]
            .common
            .flex
            .filter(|w| *w > 0.0 && (main_bounded || capped) && shared.is_none())
    };
    // A row's children without flex are sized to their content.
    let in_row = |f: &dyn Fn() -> Size| {
        if axis == Axis::Horizontal {
            ctx.content_width(true, f)
        } else {
            f()
        }
    };

    let measure_all = || {
        // Non-flex children (all of them when the main axis isn't tight) first.
        let mut sizes: Vec<Option<Size>> = children
            .iter()
            .enumerate()
            .map(|(i, &child)| match flex_of(child) {
                Some(_) => None,
                None => {
                    let w = shared.as_ref().and_then(|ws| ws[i]);
                    Some(in_row(&|| measure(ctx, child, child_c(child, w))))
                }
            })
            .collect();

        // Flex children take their share of what's left, exactly as in arrange.
        let total_flex: f32 = children.iter().filter_map(|&ch| flex_of(ch)).sum();
        if total_flex > 0.0 {
            let fixed: f32 = children
                .iter()
                .zip(&sizes)
                .filter_map(|(&ch, s)| {
                    s.map(|s| main_of(axis, s) + main_margin(axis, ctx.doc.nodes[ch].common.margin))
                })
                .sum();
            let free = (main_of(axis, inner) - fixed - gaps).max(0.0);
            for (i, &child) in children.iter().enumerate() {
                if let Some(w) = flex_of(child) {
                    let common = &ctx.doc.nodes[child].common;
                    let share = (free * w / total_flex - main_margin(axis, common.margin)).max(0.0);
                    let share = share.max(min_main(common, axis));
                    sizes[i] = Some(if capped {
                        // At its own width, at most its share.
                        let cc = child_c(child, None);
                        let cc = Constraints {
                            max_w: share.max(cc.min_w),
                            ..cc
                        };
                        measure(ctx, child, cc)
                    } else {
                        measure(ctx, child, child_c(child, Some(share)))
                    });
                }
            }
        }
        // "auto" overhanging children wrap to the column's own width.
        let auto = |c: usize| ctx.doc.nodes[c].common.overhang_auto;
        if axis == Axis::Vertical && children.iter().any(|&c| auto(c)) {
            if !cross_tight {
                let w = children
                    .iter()
                    .zip(&sizes)
                    .filter(|(&c, _)| !ctx.doc.nodes[c].common.overhang)
                    .map(|(&c, s)| {
                        s.map_or(0.0, |s| s.w) + ctx.doc.nodes[c].common.margin.horizontal()
                    })
                    .fold(0.0, f32::max);
                auto_cross.set(w.min(cross_of(axis, inner)));
            }
            for (i, &c) in children.iter().enumerate() {
                if auto(c) && flex_of(c).is_none() {
                    sizes[i] = Some(measure(ctx, c, child_c(c, None)));
                }
            }
        }
        sizes
    };
    let mut sizes = measure_all();
    // "auto" overhanging captions take the room their neighbours leave:
    // settle it, then measure again with it (they may wrap).
    if axis == Axis::Horizontal {
        for _ in 0..2 {
            let szs: Vec<Size> = sizes.iter().map(|s| s.unwrap_or_default()).collect();
            let w = szs
                .iter()
                .zip(children)
                .map(|(s, &c)| s.w + ctx.doc.nodes[c].common.margin.horizontal())
                .sum::<f32>()
                + gaps;
            let h = szs.iter().map(|s| s.h).fold(0.0, f32::max);
            let content = Rect::new(0.0, 0.0, w, h);
            if !settle_auto_overhangs(ctx, children, &szs, spacing, cross_align, content) {
                break;
            }
            sizes = measure_all();
        }
    }

    // A pitched column: the gaps that put the first baselines `pitch` apart.
    let gaps = match pitch {
        Some(p) if children.len() > 1 => {
            let szs: Vec<Size> = sizes.iter().map(|s| s.unwrap_or_default()).collect();
            pitch_gaps(ctx, children, &szs, spacing, p).iter().sum()
        }
        _ => gaps,
    };

    let mut main_sum = 0.0f32;
    let mut cross_max = 0.0f32;
    // Baseline rows: the tallest part above the shared baseline plus the
    // tallest part below it (likewise the shared cap middle).
    // Axis alignment likewise, across either kind of container.
    let row_kind = row_line(axis, cross_align);
    let by_axis = cross_align == CrossAxisAlign::Axis;
    let (mut above, mut below) = (0.0f32, 0.0f32);
    for (&child, sz) in children.iter().zip(&sizes) {
        let sz = sz.unwrap_or_default();
        let margin = ctx.doc.nodes[child].common.margin;
        let outer = Size::new(sz.w + margin.horizontal(), sz.h + margin.vertical());
        main_sum += main_of(axis, outer);
        if ctx.doc.nodes[child].common.overhang {
            continue; // takes no room across
        }
        cross_max = cross_max.max(cross_of(axis, outer));
        if ctx.doc.nodes[child].common.align_self.is_some() {
            continue; // aligned on its own, off the shared line
        }
        if let Some(kind) = row_kind {
            let b = row_baseline(ctx, child, sz, margin, kind);
            above = above.max(b);
            below = below.max(outer.h - b);
        } else if by_axis {
            let b = axis_line(ctx, axis, child, sz, margin);
            above = above.max(b);
            below = below.max(cross_of(axis, outer) - b);
        }
    }
    if row_kind.is_some() || by_axis {
        cross_max = cross_max.max(above + below);
    }
    main_sum += gaps;
    // Sized to its content, a row with flex children is wide enough that
    // each gets its own width from its share (a table's columns then don't
    // wrap), within the room it has.
    let total_flex: f32 = children.iter().filter_map(|&c| flex_of(c)).sum();
    if capped && total_flex > 0.0 {
        let outer_main = |c: usize, s: &Option<Size>| {
            s.map_or(0.0, |s| s.w) + ctx.doc.nodes[c].common.margin.horizontal()
        };
        let fixed: f32 = children
            .iter()
            .zip(&sizes)
            .filter(|(&c, _)| flex_of(c).is_none())
            .map(|(&c, s)| outer_main(c, s))
            .sum();
        let flexed = children
            .iter()
            .zip(&sizes)
            .filter_map(|(&c, s)| flex_of(c).map(|w| outer_main(c, s) * total_flex / w))
            .fold(0.0, f32::max);
        main_sum = main_sum.max((fixed + flexed + gaps).min(inner_max_w));
    }

    let (w, h) = match axis {
        Axis::Vertical => (cross_max, main_sum),
        Axis::Horizontal => (main_sum, cross_max),
    };
    let edge = match (axis, children.first(), children.last()) {
        (Axis::Horizontal, Some(&first), Some(&last)) => {
            let w_of = |i: usize| sizes[i].map_or(0.0, |s| s.w);
            overhang_reach(ctx, first, w_of(0)).0
                + overhang_reach(ctx, last, w_of(children.len() - 1)).1
        }
        _ => 0.0,
    };
    (
        Size::new(
            w + common.padding.horizontal(),
            h + common.padding.vertical(),
        ),
        edge,
    )
}

/// How far node `idx`, `w` wide, has overhanging children (see
/// `Common::overhang`) reaching past its left and right edges: a column's
/// direct children laid out at their natural width (centred unless the
/// column aligns to an edge), or those of a stack's base layer (a labelled
/// captioned panel).
pub(super) fn overhang_reach<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, w: f32) -> (f32, f32) {
    let node = &ctx.doc.nodes[idx];
    match &node.kind {
        NodeType::Container {
            axis: Axis::Vertical,
            children,
            cross_axis,
            ..
        } => {
            let inner = w - node.common.padding.horizontal();
            let (mut l, mut r) = (0.0f32, 0.0f32);
            for &ch in children {
                let cc = &ctx.doc.nodes[ch].common;
                // An "auto" one stays inside the row by construction.
                if !cc.overhang || cc.overhang_auto {
                    continue;
                }
                let nat = measure(ctx, ch, Constraints::loose(f32::INFINITY, f32::INFINITY)).w
                    + cc.margin.horizontal();
                let extra = nat - inner;
                if extra <= 0.0 {
                    continue;
                }
                let (a, b) = match cross_axis {
                    CrossAxisAlign::Start => (0.0, extra),
                    CrossAxisAlign::End => (extra, 0.0),
                    _ => (extra / 2.0, extra / 2.0),
                };
                l = l.max(a);
                r = r.max(b);
            }
            (l, r)
        }
        NodeType::Stack { children, .. } => children
            .first()
            .map_or((0.0, 0.0), |&c| overhang_reach(ctx, c, w)),
        _ => (0.0, 0.0),
    }
}

/// Distance from the leading cross edge of a child's margin box (top in a
/// row, left in a column) to its axis line, its border box being `fp`.
pub(super) fn axis_line<M: LeafMeasure>(
    ctx: &Ctx<M>,
    axis: Axis,
    child: usize,
    fp: Size,
    margin: Edges,
) -> f32 {
    let lead = match axis {
        Axis::Horizontal => margin.top,
        Axis::Vertical => margin.left,
    };
    lead + axis_offset(ctx, child, fp, cross_axis(axis))
}

/// The border-box width of each child of an `equal_height` row whose
/// content box is `avail` wide (`gaps`: the total spacing): `Some(w)` for a
/// child whose height follows its width — all of them then end up one
/// height, and with the rest at their natural width the row is exactly
/// full — `None` for a child that keeps its natural width (text, explicit
/// sizes: anything whose height doesn't grow with its width).
///
/// Each child's height is taken as an affine function of its width, `h =
/// s·w + c` (an image: `c = 0`, `s = 1/aspect`; a captioned panel: `c` = the
/// caption), measured at two widths and refined once at the solution, so
/// the shared height `H` solves `Σ (H - c_i) / s_i = room`.
pub(super) fn equal_height_widths<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    avail: f32,
    gaps: f32,
    base: Constraints,
) -> Vec<Option<f32>> {
    let n = children.len();
    let mut out = vec![None; n];
    if n == 0 || !avail.is_finite() {
        return out;
    }
    let height_at = |child: usize, w: f32| {
        let c = Constraints::loose(w, f32::INFINITY).with_tight_w(w);
        measure(ctx, child, c).h
    };
    // (slope, intercept) of h(w) between widths a and b; None when flat.
    let line = |child: usize, a: f32, b: f32| {
        let (ha, hb) = (height_at(child, a), height_at(child, b));
        let s = (ha - hb) / (a - b);
        (s.is_finite() && s > 1e-3).then_some((s, ha - s * a))
    };
    let margins: Vec<f32> = children
        .iter()
        .map(|&c| ctx.doc.nodes[c].common.margin.horizontal())
        .collect();
    let probe = ((avail - gaps) / n as f32).max(1.0);
    let mut lines: Vec<Option<(f32, f32)>> = children
        .iter()
        .map(|&c| line(c, probe, probe * 0.5))
        .collect();
    if lines.iter().all(Option::is_none) {
        return out;
    }
    // The fixed children's natural widths, and every margin, come off the top.
    let fixed: f32 = (0..n)
        .map(|i| match lines[i] {
            Some(_) => margins[i],
            None => measure(ctx, children[i], base).w + margins[i],
        })
        .sum();
    let room = avail - gaps - fixed;
    if room <= 0.0 {
        return out;
    }
    let solve = |lines: &[Option<(f32, f32)>]| {
        let (mut num, mut den) = (room, 0.0f32);
        for (s, c) in lines.iter().flatten() {
            num += c / s;
            den += 1.0 / s;
        }
        let h = num / den;
        lines
            .iter()
            .map(|l| l.map(|(s, c)| ((h - c) / s).max(0.0)))
            .collect::<Vec<_>>()
    };
    let first = solve(&lines);
    // Refine each line around its solution (wrapping captions, clamps).
    for i in 0..n {
        if let (Some(w), Some(_)) = (first[i], lines[i]) {
            if w > 1.0 {
                if let Some(l) = line(children[i], w, w * 0.8) {
                    lines[i] = Some(l);
                }
            }
        }
    }
    for (o, w) in out.iter_mut().zip(solve(&lines)) {
        *o = w;
    }
    out
}

/// Wrap-content size of a stack: the furthest extent of any child
/// (`offset - anchor shift + margins + border box`) on each axis, plus
/// padding. Children that fill the stack in arrange contribute their intrinsic
/// size here, as flex children do in a row/column. In a stack with a viewbox
/// (`user` offsets) the offsets only make sense against the final box, so they
/// are ignored: such a stack wraps its largest child. A plain `contain` stack
/// also counts what hangs off its top/left (negative offsets, anchors), since
/// it will make room for it.
fn measure_stack<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    user: bool,
    contain: bool,
    common: &Common,
    c: Constraints,
) -> Size {
    let inner = Size::new(
        (c.max_w - common.padding.horizontal()).max(0.0),
        (c.max_h - common.padding.vertical()).max(0.0),
    );

    let mut w = 0.0f32;
    let mut h = 0.0f32;
    let (mut left, mut top) = (0.0f32, 0.0f32);
    for &child in children {
        // Size/aspect in the stack's frame (swapped for a quarter-turned child).
        let cc = ctx.doc.nodes[child].common.outer();
        let cc = &*cc;
        let avail = stack_child_avail(cc, inner, user);
        // A filling axis is tight when the stack's own size on it is.
        let sz = measure(
            ctx,
            child,
            stack_child_constraints(cc, avail, c.is_tight_w(), c.is_tight_h()),
        );
        let (x, y) = if user {
            (0.0, 0.0)
        } else {
            let (dx, dy) = anchor_shift(ctx, child, cc.anchor, sz);
            (cc.x.unwrap_or(0.0) - dx, cc.y.unwrap_or(0.0) - dy)
        };
        w = w.max(x + cc.margin.horizontal() + sz.w);
        h = h.max(y + cc.margin.vertical() + sz.h);
        if contain {
            left = left.min(x);
            top = top.min(y);
        }
    }
    Size::new(
        w - left + common.padding.horizontal(),
        h - top + common.padding.vertical(),
    )
}

/// `c` made tight on each axis the node sizes explicitly (`width` / `height`,
/// clamped to `c`), so its children are measured against the box it will have:
/// a filling base layer with an aspect ratio (an image under an overlay) then
/// reports the height it gets at that width, and the stack wraps it exactly.
fn explicit_tight(common: &Common, c: Constraints) -> Constraints {
    let mut c = c;
    if let Some(w) = common.width {
        c = c.with_tight_w(c.clamp_w(w));
    }
    if let Some(h) = common.height {
        c = c.with_tight_h(c.clamp_h(h));
    }
    c
}

/// Space available to a stack child's border box inside a content box of size
/// `content`: from the child's `x`/`y` offset (plus leading margin) to the far
/// edge (minus trailing margin). As in rows/columns, sizes are clamped to it, so
/// a child never spills past the right/bottom edge; a negative offset may hang
/// off the top/left.
///
/// An anchored child, or any child of a viewbox stack (`user` offsets), sits
/// *around* its point rather than hanging from it, so the offset doesn't
/// shorten its space: it is only clamped to the content size (minus margins)
/// and may extend past any edge.
pub(super) fn stack_child_avail(common: &Common, content: Size, user: bool) -> Size {
    let (x, y) = if user || common.anchor.is_some() {
        (0.0, 0.0)
    } else {
        (common.x.unwrap_or(0.0), common.y.unwrap_or(0.0))
    };
    Size::new(
        (content.w - x - common.margin.horizontal()).max(0.0),
        (content.h - y - common.margin.vertical()).max(0.0),
    )
}

/// Which axes of a stack child fill the stack's content box: an axis with no
/// explicit size (direct, or pinned through `aspect_ratio` by the other side)
/// and no offset. Returns `(fill_w, fill_h)`.
pub(super) fn stack_child_fills(common: &Common) -> (bool, bool) {
    let ar = common.aspect_ratio.filter(|a| *a > 0.0);
    let pinned_w = common.width.is_some() || (ar.is_some() && common.height.is_some());
    let pinned_h = common.height.is_some() || (ar.is_some() && common.width.is_some());
    (
        !pinned_w && common.x.is_none(),
        !pinned_h && common.y.is_none(),
    )
}

/// Constraints for measuring a stack child within `avail`: loose, except that a
/// filling axis is tight (`avail`) when the stack's content size on that axis is
/// known (`tight_w` / `tight_h`).
pub(super) fn stack_child_constraints(
    common: &Common,
    avail: Size,
    tight_w: bool,
    tight_h: bool,
) -> Constraints {
    let (fill_w, fill_h) = stack_child_fills(common);
    let mut c = Constraints::loose(avail.w, avail.h);
    if fill_w && tight_w && avail.w.is_finite() {
        c = c.with_tight_w(avail.w);
    }
    if fill_h && tight_h && avail.h.is_finite() {
        c = c.with_tight_h(avail.h);
    }
    c
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
    let (fill_w, fill_h) = stack_child_fills(common);

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
/// derived from it via the ratio when only one is fixed. A tight constraint on
/// a non-explicit side counts as known too (the parent decides that side).
pub(super) fn resolve_box_size(common: &Common, intrinsic: Size, c: Constraints) -> Size {
    let s = resolve_box_size_in(common, intrinsic, c);
    match (common.min_width, common.min_height) {
        (None, None) => s,
        (mw, mh) => c.clamp(Size::new(
            s.w.max(mw.unwrap_or(0.0)),
            s.h.max(mh.unwrap_or(0.0)),
        )),
    }
}

/// The smallest size of a node along `axis` (`min_width` / `min_height`).
pub(super) fn min_main(common: &Common, axis: Axis) -> f32 {
    match axis {
        Axis::Horizontal => common.min_width,
        Axis::Vertical => common.min_height,
    }
    .unwrap_or(0.0)
}

/// [`resolve_box_size`] before the minimum sizes.
fn resolve_box_size_in(common: &Common, intrinsic: Size, c: Constraints) -> Size {
    let tight_w = common.width.is_none() && c.is_tight_w();
    let tight_h = common.height.is_none() && c.is_tight_h();
    let mut w = if tight_w {
        c.max_w
    } else {
        common.width.unwrap_or(intrinsic.w)
    };
    let mut h = if tight_h {
        c.max_h
    } else {
        common.height.unwrap_or(intrinsic.h)
    };

    if let Some(ar) = common.aspect_ratio {
        if ar > 0.0 {
            match (
                common.width.is_some() || tight_w,
                common.height.is_some() || tight_h,
            ) {
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

/// The largest box of aspect `ar` (w / h, > 0) within `max_w` × `max_h`: the
/// full width when it fits, else the full height. An unbounded axis doesn't
/// limit (the other one decides); with both unbounded there is nothing to fill
/// and the result is zero — never infinite.
fn contain(ar: f32, max_w: f32, max_h: f32) -> Size {
    let by_width = max_w.is_finite().then(|| Size::new(max_w, max_w / ar));
    let by_height = max_h.is_finite().then(|| Size::new(max_h * ar, max_h));
    match (by_width, by_height) {
        (Some(w), Some(h)) => {
            if w.h <= max_h {
                w
            } else {
                h
            }
        }
        (Some(s), None) | (None, Some(s)) => s,
        (None, None) => Size::default(),
    }
}

/// An image's natural size (points): its pixels / `dpi` inches, when it has
/// a `dpi` and the measurer knows its pixel size.
pub(super) fn natural_size<M: LeafMeasure>(ctx: &Ctx<M>, props: &ImageProps) -> Option<Size> {
    let file = props
        .dpi_file
        .then(|| ctx.measurer.image_density(props))
        .flatten();
    let dpi = file.or(props.dpi).filter(|d| d.is_finite() && *d > 0.0)?;
    let (w, h) = ctx.measurer.image_pixels(props)?;
    Some(Size::new(w as f32 / dpi * 72.0, h as f32 / dpi * 72.0))
}

/// The size of an image with natural size `nat`: an explicit side or a side
/// its parent decides (tight) wins, the other following the aspect;
/// otherwise the natural size, shrunk (keeping the aspect) to fit `c`.
fn measure_natural(common: &Common, nat: Size, c: Constraints) -> Size {
    let ar = common
        .aspect_ratio
        .filter(|a| a.is_finite() && *a > 0.0)
        .unwrap_or(nat.w / nat.h.max(1e-6));
    let nat = Size::new(nat.w, nat.w / ar);
    let common = Common {
        aspect_ratio: Some(ar),
        ..common.clone()
    };
    let known =
        common.width.is_some() || common.height.is_some() || c.is_tight_w() || c.is_tight_h();
    if known {
        return resolve_box_size(&common, nat, c);
    }
    let fit = contain(ar, c.max_w, c.max_h);
    let s = if fit.w > 0.0 && fit.w < nat.w {
        fit
    } else {
        nat
    };
    c.clamp(s)
}

/// Whether node `idx` keeps a natural size of its own — an image with one
/// (see [`natural_size`]), or a stack without an explicit size whose base
/// layer (first child) does (an overlay, a label over such an image) — so
/// a stretching parent leaves it at that size.
///
/// Not inside an equal-height row, which scales its items (and what they
/// hold) to share one height.
pub(super) fn keeps_natural<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize) -> bool {
    if ctx.scaling.get() > 0 {
        return false;
    }
    let node = &ctx.doc.nodes[idx];
    match &node.kind {
        NodeType::Image(props) => {
            node.common.width.is_none()
                && node.common.height.is_none()
                && natural_size(ctx, props).is_some()
        }
        NodeType::Stack { children, .. } => {
            node.common.width.is_none()
                && node.common.height.is_none()
                && node.common.aspect_ratio.is_none()
                && children.first().is_some_and(|&c| {
                    let cc = &ctx.doc.nodes[c].common;
                    cc.x.is_none() && cc.y.is_none() && keeps_natural(ctx, c)
                })
        }
        _ => false,
    }
}

/// The width node `idx`'s border box can't go below by wrapping or
/// shrinking (a min-content width, approximated): explicit widths, natural
/// sizes of images, unwrapped text and path extents; rows add their
/// children up, columns and stacks take the widest. Wrapping text, images
/// without a natural size, equal-height rows and overhanging nodes count 0.
pub(super) fn min_content_width<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize) -> f32 {
    let node = &ctx.doc.nodes[idx];
    let common = &node.common;
    if common.overhang {
        return 0.0;
    }
    if common.swaps_axes() {
        return common.height.unwrap_or(0.0);
    }
    if let Some(w) = common.width {
        return w;
    }
    let pad = common.padding.horizontal();
    let of = |c: usize| min_content_width(ctx, c) + ctx.doc.nodes[c].common.margin.horizontal();
    match &node.kind {
        NodeType::Image(props) => {
            match (common.height, common.aspect_ratio, natural_size(ctx, props)) {
                (Some(h), Some(ar), _) if ar > 0.0 => h * ar,
                (Some(h), None, Some(n)) => h * n.w / n.h.max(1e-6),
                // A height and the file's aspect (a scheme's structures).
                (Some(h), None, None) => h * ctx.measurer.image_aspect(props).unwrap_or(0.0),
                (None, _, Some(n)) => n.w,
                _ => 0.0,
            }
        }
        NodeType::Text(props) if !props.wrap => {
            ctx.measurer.measure_text(props, f32::INFINITY).w + pad
        }
        NodeType::Path(props) if props.viewbox.is_none() => {
            crate::path::bounds(&props.segments).map_or(0.0, |(_, hi)| hi.x.max(0.0)) + pad
        }
        // An equal-height row scales its items to whatever width it gets.
        NodeType::Container {
            equal_height: true, ..
        } => pad,
        // A row: its fixed children side by side, and room enough for the
        // flex ones that every one gets its own minimum from its share.
        NodeType::Container {
            axis: Axis::Horizontal,
            children,
            spacing,
            ..
        } => {
            let flex = |c: usize| ctx.doc.nodes[c].common.flex.filter(|w| *w > 0.0);
            let total: f32 = children.iter().filter_map(|&c| flex(c)).sum();
            let fixed: f32 = children
                .iter()
                .filter(|&&c| flex(c).is_none())
                .map(|&c| of(c))
                .sum();
            let flexed = children
                .iter()
                .filter_map(|&c| flex(c).map(|w| of(c) * total / w))
                .fold(0.0, f32::max);
            fixed + flexed + spacing * children.len().saturating_sub(1) as f32 + pad
        }
        NodeType::Container { children, .. } | NodeType::Stack { children, .. } => {
            children.iter().map(|&c| of(c)).fold(0.0, f32::max) + pad
        }
        _ => 0.0,
    }
}

/// Whether a column / row laid out along `axis` with cross alignment
/// `align` stretches `child` across (when its cross size is known): by the
/// child's own `align_self` — "stretch" stretches anything without an
/// explicit cross size, even a natural-size image — else when `align` is
/// `stretch` and the child doesn't keep a size of its own.
pub(super) fn stretches<M: LeafMeasure>(
    ctx: &Ctx<M>,
    child: usize,
    axis: Axis,
    align: CrossAxisAlign,
) -> bool {
    let common = &ctx.doc.nodes[child].common;
    match common.align_self {
        Some(CrossAxisAlign::Stretch) => !common.overhang && !has_cross_size(common, axis),
        Some(_) => false,
        None => align == CrossAxisAlign::Stretch && !keeps_cross_size(ctx, child, axis),
    }
}

/// The cross size an overhanging child of a column / row laid out along
/// `axis` may take, the parent's content being `inner` across: unbounded,
/// or for an "auto" one the parent's width plus the room its row found
/// free beside it (none known: the parent's width — it wraps).
pub(super) fn overhang_limit<M: LeafMeasure>(
    ctx: &Ctx<M>,
    child: usize,
    axis: Axis,
    inner: f32,
) -> f32 {
    let common = &ctx.doc.nodes[child].common;
    if !common.overhang_auto || axis != Axis::Vertical {
        return f32::INFINITY;
    }
    let (l, r) = auto_room(ctx, child);
    (inner + l + r).max(0.0)
}

/// The room (left, right) an "auto" overhanging node's row found free
/// beside its parent; none known: `(0, 0)`.
pub(super) fn auto_room<M: LeafMeasure>(ctx: &Ctx<M>, child: usize) -> (f32, f32) {
    ctx.overhang_room
        .borrow()
        .get(&child)
        .copied()
        .unwrap_or((0.0, 0.0))
}

/// The "auto" overhanging captions of row child `idx` laid out in `rect`
/// (its border box, in the row's frame): each caption's node index, its
/// box, and the content box of the column it sits in. A column's direct
/// children, or those of a stack's base layer (a labelled panel).
fn auto_captions<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, rect: Rect) -> Vec<(usize, Rect, Rect)> {
    let node = &ctx.doc.nodes[idx];
    let content = rect.inset(node.common.padding);
    match &node.kind {
        NodeType::Container {
            axis: Axis::Vertical,
            children,
            ..
        } => {
            if !children
                .iter()
                .any(|&c| ctx.doc.nodes[c].common.overhang_auto)
            {
                return Vec::new();
            }
            let Some(flex) = Flex::of(ctx, idx) else {
                return Vec::new();
            };
            let slots = flex_slots(ctx, &flex, content);
            children
                .iter()
                .zip(slots)
                .filter(|(&c, _)| ctx.doc.nodes[c].common.overhang_auto)
                .map(|(&c, s)| (c, s, content))
                .collect()
        }
        NodeType::Stack {
            children, viewbox, ..
        } => match children.first() {
            Some(&base) if has_auto_captions(ctx, base) => {
                let slot = stack_slots(ctx, &children[..1], *viewbox, content)[0];
                auto_captions(ctx, base, slot)
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Whether row child `idx` holds "auto" overhanging captions (see
/// [`auto_captions`]).
fn has_auto_captions<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize) -> bool {
    match &ctx.doc.nodes[idx].kind {
        NodeType::Container {
            axis: Axis::Vertical,
            children,
            ..
        } => children
            .iter()
            .any(|&c| ctx.doc.nodes[c].common.overhang_auto),
        NodeType::Stack { children, .. } => {
            children.first().is_some_and(|&b| has_auto_captions(ctx, b))
        }
        _ => false,
    }
}

/// What row child `idx`, laid out in `rect`, draws: one level down for a
/// column (its children's boxes; an "auto" caption as its column's width,
/// flagged, so two captions reaching into one gap split it) or a stack's
/// layers (its base layer likewise), else its box.
fn ink_boxes<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, rect: Rect, out: &mut Vec<(Rect, bool)>) {
    let node = &ctx.doc.nodes[idx];
    let content = rect.inset(node.common.padding);
    match &node.kind {
        NodeType::Container {
            axis: Axis::Vertical,
            children,
            ..
        } if !children.is_empty() => {
            let Some(flex) = Flex::of(ctx, idx) else {
                out.push((rect, false));
                return;
            };
            for (&c, s) in children.iter().zip(flex_slots(ctx, &flex, content)) {
                if ctx.doc.nodes[c].common.overhang_auto {
                    out.push((Rect::new(content.x, s.y, content.w, s.h), true));
                } else {
                    out.push((s, false));
                }
            }
        }
        NodeType::Stack {
            children, viewbox, ..
        } if !children.is_empty() => {
            let slots = stack_slots(ctx, children, *viewbox, content);
            for (k, (&c, s)) in children.iter().zip(slots).enumerate() {
                if k == 0 {
                    ink_boxes(ctx, c, s, out);
                } else {
                    out.push((s, false));
                }
            }
        }
        _ => out.push((rect, false)),
    }
}

/// Settle the room of the "auto" overhanging captions among a row's
/// `children` (border-box `sizes`, `spacing` apart, aligned across by
/// `align`, in a content box `content`): each may reach left and right of
/// its column as far as nothing else the row draws at its height — the
/// row's other children, one level down (see [`ink_boxes`]) — half way to
/// another such caption, and never past the row's box. Recorded in
/// [`Ctx::overhang_room`]; returns whether anything changed (the caller
/// measures again).
pub(super) fn settle_auto_overhangs<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    sizes: &[Size],
    spacing: f32,
    align: CrossAxisAlign,
    content: Rect,
) -> bool {
    if !children.iter().any(|&c| has_auto_captions(ctx, c)) {
        return false;
    }
    let slots = row_slots_of(ctx, children, sizes, spacing, align, content);
    settle_auto_overhangs_at(ctx, children, &slots, content)
}

/// [`settle_auto_overhangs`] with the children's slots known.
pub(super) fn settle_auto_overhangs_at<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    slots: &[Rect],
    content: Rect,
) -> bool {
    if !children.iter().any(|&c| has_auto_captions(ctx, c)) {
        return false;
    }
    let mut inks: Vec<Vec<(Rect, bool)>> = Vec::with_capacity(children.len());
    for (&c, &s) in children.iter().zip(slots) {
        let mut v = Vec::new();
        ink_boxes(ctx, c, s, &mut v);
        inks.push(v);
    }
    let mut changed = false;
    for (i, (&c, &s)) in children.iter().zip(slots).enumerate() {
        for (cap, cap_box, col) in auto_captions(ctx, c, s) {
            let (lo, hi) = (cap_box.y, cap_box.y + cap_box.h);
            let mut left = col.x - content.x;
            let mut right = content.x + content.w - (col.x + col.w);
            for (j, boxes) in inks.iter().enumerate() {
                if j == i {
                    continue;
                }
                for &(b, shared) in boxes {
                    // Only what lies at the caption's height.
                    if b.y >= hi - 0.01 || b.y + b.h <= lo + 0.01 || b.w <= 0.0 {
                        continue;
                    }
                    let k = if shared { 0.5 } else { 1.0 };
                    if b.x >= col.x + col.w - 0.01 {
                        right = right.min((b.x - (col.x + col.w)) * k);
                    } else if b.x + b.w <= col.x + 0.01 {
                        left = left.min((col.x - (b.x + b.w)) * k);
                    }
                }
            }
            let room = (left.max(0.0), right.max(0.0));
            let old = ctx.overhang_room.borrow_mut().insert(cap, room);
            if old.is_none_or(|(l, r)| (l - room.0).abs() > 0.01 || (r - room.1).abs() > 0.01) {
                changed = true;
            }
        }
    }
    changed
}

/// Where a row lays out `children` of border-box `sizes` (with their
/// margins), `spacing` apart from the start of `content`, aligned across
/// by `align` (an axis line, centre, end, else the top) — a quick
/// arrangement for [`settle_auto_overhangs`] while measuring.
fn row_slots_of<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    sizes: &[Size],
    spacing: f32,
    align: CrossAxisAlign,
    content: Rect,
) -> Vec<Rect> {
    let margins: Vec<Edges> = children
        .iter()
        .map(|&c| ctx.doc.nodes[c].common.margin)
        .collect();
    let axes: Vec<f32> = children
        .iter()
        .zip(sizes)
        .zip(&margins)
        .map(|((&c, &s), &m)| axis_line(ctx, Axis::Horizontal, c, s, m))
        .collect();
    let line = axes.iter().copied().fold(0.0, f32::max);
    let mut x = content.x;
    let mut out = Vec::with_capacity(children.len());
    for (i, &s) in sizes.iter().enumerate() {
        let m = margins[i];
        let outer_h = s.h + m.vertical();
        let dy = match align {
            CrossAxisAlign::Axis => line - axes[i],
            CrossAxisAlign::Center => (content.h - outer_h) / 2.0,
            CrossAxisAlign::End => content.h - outer_h,
            _ => 0.0,
        };
        out.push(Rect::new(x + m.left, content.y + dy + m.top, s.w, s.h));
        x += s.w + m.horizontal() + spacing;
    }
    out
}

/// Whether a stretching parent laid out along `axis` leaves `child` at its
/// own cross size: an explicit one ([`has_cross_size`]), a natural one
/// ([`keeps_natural`]), or an overhanging child.
pub(super) fn keeps_cross_size<M: LeafMeasure>(ctx: &Ctx<M>, child: usize, axis: Axis) -> bool {
    let common = &ctx.doc.nodes[child].common;
    common.overhang || has_cross_size(common, axis) || keeps_natural(ctx, child)
}

/// Whether a child of a container laid out along `axis` has an explicit size
/// across that axis (`width` in a column, `height` in a row; in the parent's
/// frame, so swapped for a quarter-turned child). `stretch` only stretches a
/// child whose cross size is unconstrained (CSS `align-items: stretch`,
/// Flutter `CrossAxisAlignment.stretch` over a loose child): an explicit size
/// wins and the child is aligned at the start.
pub(super) fn has_cross_size(common: &Common, axis: Axis) -> bool {
    let outer = common.outer();
    match axis {
        Axis::Vertical => outer.width.is_some(),
        Axis::Horizontal => outer.height.is_some(),
    }
}

/// The cross axis of a container laid out along `axis`.
pub(super) fn cross_axis(axis: Axis) -> Axis {
    match axis {
        Axis::Vertical => Axis::Horizontal,
        Axis::Horizontal => Axis::Vertical,
    }
}

/// Make `c` tight to `v` along `axis` (width for horizontal, height for vertical).
pub(super) fn tighten(c: Constraints, axis: Axis, v: f32) -> Constraints {
    match axis {
        Axis::Horizontal => c.with_tight_w(v),
        Axis::Vertical => c.with_tight_h(v),
    }
}

/// A child's margin along the main axis of a container laid out along `axis`.
pub(super) fn main_margin(axis: Axis, m: Edges) -> f32 {
    match axis {
        Axis::Vertical => m.vertical(),
        Axis::Horizontal => m.horizontal(),
    }
}

/// A child's margin along the cross axis of a container laid out along `axis`.
pub(super) fn cross_margin(axis: Axis, m: Edges) -> f32 {
    match axis {
        Axis::Vertical => m.horizontal(),
        Axis::Horizontal => m.vertical(),
    }
}
