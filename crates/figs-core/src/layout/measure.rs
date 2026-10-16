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

use super::{cross_of, main_of, Constraints, Ctx, LeafMeasure, FALLBACK_IMAGE_ASPECT};

/// Measure node `idx` under `c`, returning its border-box size (content +
/// padding, excluding margin).
pub(crate) fn measure<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, c: Constraints) -> Size {
    let node = &ctx.doc.nodes[idx];
    let intrinsic = match &node.kind {
        NodeType::Container {
            axis,
            children,
            spacing,
            cross_axis,
            ..
        } => measure_container(ctx, *axis, children, *spacing, *cross_axis, &node.common, c),
        NodeType::Stack { children } => measure_stack(ctx, children, &node.common, c),
        NodeType::Rect(_) | NodeType::Ellipse(_) => Size::default(),
        // Coordinates live in the content box, so the intrinsic size is the
        // outline's extent from its origin (exact curve bounds, not control
        // points; not the tight bounding box) plus padding.
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
    let child_c = |margin: Edges, main: Option<f32>| {
        let mut cc = base;
        if stretch {
            cc = tighten(cc, cross_axis(axis), cross_of(axis, inner) - cross_margin(axis, margin));
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
            None => Some(measure(ctx, child, child_c(ctx.doc.nodes[child].common.margin, None))),
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
                sizes[i] = Some(measure(ctx, child, child_c(margin, Some(share))));
            }
        }
    }

    let mut main_sum = 0.0f32;
    let mut cross_max = 0.0f32;
    for (&child, sz) in children.iter().zip(&sizes) {
        let sz = sz.unwrap_or_default();
        let margin = ctx.doc.nodes[child].common.margin;
        let outer = Size::new(sz.w + margin.horizontal(), sz.h + margin.vertical());
        main_sum += main_of(axis, outer);
        cross_max = cross_max.max(cross_of(axis, outer));
    }
    main_sum += gaps;

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
        // A filling axis is tight when the stack's own size on it is.
        let sz = measure(
            ctx,
            child,
            stack_child_constraints(cc, avail, c.is_tight_w(), c.is_tight_h()),
        );
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

/// Which axes of a stack child fill the stack's content box: an axis with no
/// explicit size (direct, or pinned through `aspect_ratio` by the other side)
/// and no offset. Returns `(fill_w, fill_h)`.
pub(super) fn stack_child_fills(common: &Common) -> (bool, bool) {
    let ar = common.aspect_ratio.filter(|a| *a > 0.0);
    let pinned_w = common.width.is_some() || (ar.is_some() && common.height.is_some());
    let pinned_h = common.height.is_some() || (ar.is_some() && common.width.is_some());
    (!pinned_w && common.x.is_none(), !pinned_h && common.y.is_none())
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
    let mut w = if tight_w { c.max_w } else { common.width.unwrap_or(intrinsic.w) };
    let mut h = if tight_h { c.max_h } else { common.height.unwrap_or(intrinsic.h) };

    if let Some(ar) = common.aspect_ratio {
        if ar > 0.0 {
            match (common.width.is_some() || tight_w, common.height.is_some() || tight_h) {
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

/// The cross axis of a container laid out along `axis`.
fn cross_axis(axis: Axis) -> Axis {
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
