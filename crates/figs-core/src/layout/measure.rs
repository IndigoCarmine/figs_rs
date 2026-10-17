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

use crate::geom::{Axis, CrossAxisAlign, Edges, Size};
use crate::schema::{Common, NodeType};

use super::baseline::{anchor_shift, row_baseline};
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
        let s = measure_own(ctx, idx, swap_constraints(c));
        return Size::new(s.h, s.w);
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
            cross_axis,
            ..
        } => measure_container(ctx, *axis, children, *spacing, *cross_axis, &node.common, c),
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
        NodeType::Rect(_) | NodeType::Ellipse(_) => Size::default(),
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

    resolve_box_size(&node.common, intrinsic, c)
}

/// Wrap-content size of a container: sum along the main axis, max along the
/// cross axis, plus spacing and padding.
///
/// With a tight cross axis and `stretch`, children are measured at the tight
/// cross size; with a tight main axis, flex children are measured at their flex
/// share (as arrange will size them). Otherwise flex children contribute their
/// intrinsic size — in particular under an unbounded main axis.
fn measure_container<M: LeafMeasure>(
    ctx: &Ctx<M>,
    axis: Axis,
    children: &[usize],
    spacing: f32,
    cross_align: CrossAxisAlign,
    common: &Common,
    c: Constraints,
) -> Size {
    let inner_max_w = (c.max_w - common.padding.horizontal()).max(0.0);
    let inner_max_h = (c.max_h - common.padding.vertical()).max(0.0);
    let base = Constraints::loose(inner_max_w, inner_max_h);
    let inner = Size::new(inner_max_w, inner_max_h);
    let (main_tight, cross_tight) = match axis {
        Axis::Vertical => (c.is_tight_h(), c.is_tight_w()),
        Axis::Horizontal => (c.is_tight_w(), c.is_tight_h()),
    };
    let stretch = cross_tight && cross_align == CrossAxisAlign::Stretch;
    let child_c = |child: usize, main: Option<f32>| {
        let common = &ctx.doc.nodes[child].common;
        let mut cc = base;
        if stretch && !has_cross_size(common, axis) {
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

    let flex_of = |child: usize| {
        ctx.doc.nodes[child]
            .common
            .flex
            .filter(|w| *w > 0.0 && main_tight)
    };
    let gaps = if children.is_empty() {
        0.0
    } else {
        spacing * (children.len() as f32 - 1.0)
    };

    // Non-flex children (all of them when the main axis isn't tight) first.
    let mut sizes: Vec<Option<Size>> = children
        .iter()
        .map(|&child| match flex_of(child) {
            Some(_) => None,
            None => Some(measure(ctx, child, child_c(child, None))),
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
                let margin = ctx.doc.nodes[child].common.margin;
                let share = (free * w / total_flex - main_margin(axis, margin)).max(0.0);
                sizes[i] = Some(measure(ctx, child, child_c(child, Some(share))));
            }
        }
    }

    let mut main_sum = 0.0f32;
    let mut cross_max = 0.0f32;
    // Baseline rows: the tallest part above the shared baseline plus the
    // tallest part below it.
    let baseline_row = axis == Axis::Horizontal && cross_align == CrossAxisAlign::Baseline;
    let (mut above, mut below) = (0.0f32, 0.0f32);
    for (&child, sz) in children.iter().zip(&sizes) {
        let sz = sz.unwrap_or_default();
        let margin = ctx.doc.nodes[child].common.margin;
        let outer = Size::new(sz.w + margin.horizontal(), sz.h + margin.vertical());
        main_sum += main_of(axis, outer);
        cross_max = cross_max.max(cross_of(axis, outer));
        if baseline_row {
            let b = row_baseline(ctx, child, sz, margin);
            above = above.max(b);
            below = below.max(outer.h - b);
        }
    }
    if baseline_row {
        cross_max = cross_max.max(above + below);
    }
    main_sum += gaps;

    let (w, h) = match axis {
        Axis::Vertical => (cross_max, main_sum),
        Axis::Horizontal => (main_sum, cross_max),
    };
    Size::new(
        w + common.padding.horizontal(),
        h + common.padding.vertical(),
    )
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
