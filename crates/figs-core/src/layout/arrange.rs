//! Pass 2 — arrange. Given the border-box rect a parent assigns to a node,
//! place it, resolve flex among its children (or position them, for a stack),
//! apply alignment, and recurse. Produces [`ComputedNode`]s in paint order
//! (parent before children, siblings in array order — so later stack children
//! paint on top).

use crate::geom::{
    quarter_turns, Affine, Axis, CrossAxisAlign, Edges, MainAxisAlign, Overflow, Rect, Size,
    ViewBox,
};
use crate::schema::{Common, NodeType, RectProps, TextProps};

use super::baseline::{anchor_shift, pitch_gaps, row_baseline, row_line};
use super::computed::{
    ClipRegion, ComputedNode, FramePaint, OverflowAction, OverflowReport, PaintContent, PathPaint,
    TextBox,
};
use super::measure::{
    axis_line, cross_axis, cross_margin, equal_height_widths, keeps_cross_size, measure,
    measure_container, stack_child_avail, stack_child_constraints, stack_child_size, tighten,
};
use super::{cross_of, main_of, Constraints, Ctx, LeafMeasure};

/// Place node `idx` into `slot` (its border box in the parent's frame, as the
/// parent laid it out) and recurse into children. `parent_xf` is the parent's
/// accumulated local → page transform (`None` = identity).
///
/// A rotated node is laid out unrotated in its own box — the slot itself, or
/// for an odd quarter turn the slot with width and height swapped about its
/// centre — and gets the transform `rotate about the box centre`, then the
/// parent's. Its children are arranged in that local frame and inherit it.
pub(crate) fn arrange<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    slot: Rect,
    parent_xf: Option<Affine>,
    out: &mut Vec<ComputedNode>,
) {
    let node = &ctx.doc.nodes[idx];
    let (rect, xf) = rotated_frame(
        node.common.rotate,
        node.common.swaps_axes(),
        slot,
        parent_xf,
    );
    let mut content = paint_content(&node.kind, &node.common, rect);
    if collapsed_path(&node.kind, &content) {
        // A shape drawn in a viewbox over a box that came out empty (an
        // arrow with length "fill" in a parent that doesn't stretch it):
        // nothing to draw — say so instead of handing the renderer a
        // degenerate path.
        tracing::warn!(
            node = %node.id,
            within = ctx.named_ancestor(idx).as_deref().unwrap_or("-"),
            width = rect.w,
            height = rect.h,
            "a line / arrow / path drawn in a viewbox got an empty box (zero length);              nothing drawn — give it a size, or a parent that stretches it"
        );
        content = PaintContent::Container;
    }
    out.push(ComputedNode {
        id: node.id.clone(),
        rect,
        transform: xf,
        content,
        clips: Vec::new(),
        effects: (!node.effects.is_none()).then(|| Box::new(node.effects.clone())),
        descendants: 0,
    });
    // Everything pushed after this node, until it returns, is its subtree
    // (pre-order): a clipping container hands its region to all of it.
    let subtree_start = out.len();
    let self_index = subtree_start - 1;

    let mut overflow_clip = false;
    match &node.kind {
        NodeType::Container { overflow, .. } => {
            let content = rect.inset(node.common.padding);
            // A column whose height is content-derived (auto-height page) gives
            // every child its measured height; flex is ignored on that axis.
            if let Some(flex) = Flex::of(ctx, idx) {
                overflow_clip = layout_children(ctx, idx, &flex, content, *overflow, xf, out);
            }
        }
        NodeType::Stack {
            children,
            viewbox,
            contain,
            overlays,
            ..
        } => {
            let content = rect.inset(node.common.padding);
            if *contain {
                layout_stack_contained(ctx, children, overlays, *viewbox, content, xf, out);
            } else {
                layout_stack(ctx, children, overlays, *viewbox, content, xf, out);
            }
        }
        NodeType::Text(_) => warn_unwrapped_overflow(ctx, idx, rect),
        _ => {}
    }

    if let Some(region) = node
        .kind
        .decoration()
        .and_then(|d| clip_region(d, rect, xf))
    {
        for n in &mut out[subtree_start..] {
            // Ancestors finish later, so inserting at the front keeps the
            // list outermost-first.
            n.clips.insert(0, region.clone());
        }
    }
    if overflow_clip {
        // `overflow = "clip"` and the children overflowed: keep them in the box.
        let region = ClipRegion {
            rect,
            radius: node.kind.decoration().map_or(0.0, |d| d.corner_radius),
            transform: xf,
        };
        for n in &mut out[subtree_start..] {
            n.clips.insert(0, region.clone());
        }
    }
    out[self_index].descendants = out.len() - subtree_start;
}

/// The region a `clip` decoration keeps its children in: the box inside
/// the border (inset by however far the border reaches inside the box —
/// its full width when `inside`, half when `center`, nothing when
/// `outside` — radius reduced to match), in the container's local frame.
fn clip_region(
    d: &crate::schema::Decoration,
    rect: Rect,
    xf: Option<Affine>,
) -> Option<ClipRegion> {
    if !d.clip {
        return None;
    }
    let inner = d
        .stroke
        .as_ref()
        .map_or(0.0, |s| d.stroke_align.inner_extent(s.width));
    Some(ClipRegion {
        rect: rect.inset(Edges::all(inner)),
        radius: (d.corner_radius - inner).max(0.0),
        transform: xf,
    })
}

/// A `wrap = false` text node whose single-line width doesn't fit its box
/// overflows it: say so (the drawing is not clipped).
fn warn_unwrapped_overflow<M: LeafMeasure>(ctx: &Ctx<M>, idx: usize, rect: Rect) {
    let node = &ctx.doc.nodes[idx];
    let NodeType::Text(props) = &node.kind else {
        return;
    };
    if props.wrap || props.fit_width.is_some() {
        return;
    }
    let natural =
        ctx.measurer.measure_text(props, f32::INFINITY).w + node.common.padding.horizontal();
    if natural > rect.w + 0.01 {
        tracing::warn!(
            node = %node.id,
            width = natural,
            room = rect.w,
            "unwrapped text (wrap = false) is wider than its box; it overflows"
        );
    }
}

/// The local box and accumulated transform of a node rotated by `degrees`
/// placed into `slot`: an odd quarter turn (`swaps`) lays the node out in the
/// slot with its sides exchanged about the same centre; every rotation turns
/// about that centre. A rotation that is a whole number of turns is no
/// rotation at all.
fn rotated_frame(
    degrees: f32,
    swaps: bool,
    slot: Rect,
    parent_xf: Option<Affine>,
) -> (Rect, Option<Affine>) {
    if degrees == 0.0 || quarter_turns(degrees) == Some(0) {
        return (slot, parent_xf);
    }
    let (cx, cy) = (slot.x + slot.w / 2.0, slot.y + slot.h / 2.0);
    let rect = if swaps {
        Rect::new(cx - slot.h / 2.0, cy - slot.w / 2.0, slot.h, slot.w)
    } else {
        slot
    };
    let local = Affine::rotation_about(degrees, cx, cy);
    let xf = match parent_xf {
        Some(p) => local.then(p),
        None => local,
    };
    (rect, Some(xf))
}

/// Place each stack child at `content origin + (x, y) - anchor * size +
/// leading margin`, sized by [`stack_child_size`]; with a `viewbox` the offsets
/// are user units mapped onto the content box first. Children overlap; array
/// order is paint order.
fn layout_stack<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    overlays: &[(usize, usize)],
    viewbox: Option<ViewBox>,
    content: Rect,
    xf: Option<Affine>,
    out: &mut Vec<ComputedNode>,
) {
    let slots = stack_slots(ctx, children, viewbox, content);
    arrange_in_order(ctx, children, overlays, slots, content, xf, out);
}

/// Arrange in-flow `children` in their `slots`, and each connector of
/// `overlays` (out of flow; see `NodeType::overlays`) in its place among
/// them, with an empty box at the `content` origin — it is drawn once every
/// box is known ([`super::connect::place_connectors`]).
fn arrange_in_order<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    overlays: &[(usize, usize)],
    slots: Vec<Rect>,
    content: Rect,
    xf: Option<Affine>,
    out: &mut Vec<ComputedNode>,
) {
    let empty = Rect::new(content.x, content.y, 0.0, 0.0);
    let mut pending = overlays.iter().peekable();
    for (k, (&child, slot)) in children.iter().zip(slots).enumerate() {
        while let Some(&(_, c)) = pending.next_if(|(at, _)| *at <= k) {
            arrange(ctx, c, empty, xf, out);
        }
        arrange(ctx, child, slot, xf, out);
    }
    for &(_, c) in pending {
        arrange(ctx, c, empty, xf, out);
    }
}

/// The slot (border box in the stack's frame) of each child of a plain
/// (non-`contain`) stack whose content box is `content` (see
/// [`layout_stack`]).
pub(super) fn stack_slots<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    viewbox: Option<ViewBox>,
    content: Rect,
) -> Vec<Rect> {
    let mut slots = Vec::with_capacity(children.len());
    for &child in children {
        // Size/aspect in the stack's frame (swapped for a quarter-turned child).
        let common = ctx.doc.nodes[child].common.outer();
        let common = &*common;
        let avail = stack_child_avail(common, content.size(), viewbox.is_some());
        // The content box is known here, so filling axes measure tight.
        let measured = measure(
            ctx,
            child,
            stack_child_constraints(common, avail, true, true),
        );
        let size = stack_child_size(common, measured, avail);
        // Offset from the content-box origin, in points.
        let (ox, oy) = match viewbox {
            Some(vb) => (
                common.x.map_or(0.0, |x| vb.map_x(x, content) - content.x),
                common.y.map_or(0.0, |y| vb.map_y(y, content) - content.y),
            ),
            None => (common.x.unwrap_or(0.0), common.y.unwrap_or(0.0)),
        };
        let (dx, dy) = anchor_shift(ctx, child, common.anchor, size);
        let x = content.x + ox - dx + common.margin.left;
        let y = content.y + oy - dy + common.margin.top;
        slots.push(Rect::new(x, y, size.w, size.h));
    }
    slots
}

/// Passes of [`layout_stack_contained`]: overhangs that don't depend on the
/// content size (a label centred on an edge) settle in one pass; the rest
/// converge geometrically, so a few passes leave sub-point residue at most.
const CONTAIN_PASSES: usize = 6;

/// Overhang (points) below which a contained stack counts as settled.
const CONTAIN_EPS: f32 = 0.01;

/// A `contain` stack: lay the children out as [`layout_stack`] does, measure
/// how far the boxes of everything they drew (all descendants) reach past
/// `bounds`, inset the content box by that much on each side, and repeat until
/// nothing overhangs. Children placed relative to the content box (offsets,
/// viewbox mapping, filling children) move inwards; the stack itself keeps its
/// size. Insets only grow, and never past the box (a collapsed content box
/// stops shrinking). Returns the content box the children were finally laid
/// out in.
pub(super) fn layout_stack_contained<M: LeafMeasure>(
    ctx: &Ctx<M>,
    children: &[usize],
    overlays: &[(usize, usize)],
    viewbox: Option<ViewBox>,
    bounds: Rect,
    xf: Option<Affine>,
    out: &mut Vec<ComputedNode>,
) -> Rect {
    // Descendants report page-space boxes; measure them in the stack's frame.
    let to_local = xf.and_then(|t| t.invert());
    let mut inset = Edges::default();
    let mut placed = Vec::new();
    let mut used = bounds;
    for _ in 0..CONTAIN_PASSES {
        placed.clear();
        used = bounds.inset(inset);
        layout_stack(ctx, children, overlays, viewbox, used, xf, &mut placed);
        let Some(ink) = union_of(&placed, to_local) else {
            break;
        };
        let over = Edges {
            left: (bounds.x - ink.x).max(0.0),
            top: (bounds.y - ink.y).max(0.0),
            right: (ink.x + ink.w - (bounds.x + bounds.w)).max(0.0),
            bottom: (ink.y + ink.h - (bounds.y + bounds.h)).max(0.0),
        };
        let room_w = bounds.w - inset.left - inset.right;
        let room_h = bounds.h - inset.top - inset.bottom;
        let settled = over.left.max(over.right) < CONTAIN_EPS || room_w <= 0.0;
        let settled_v = over.top.max(over.bottom) < CONTAIN_EPS || room_h <= 0.0;
        if settled && settled_v {
            break;
        }
        if !settled {
            inset.left += over.left;
            inset.right += over.right;
        }
        if !settled_v {
            inset.top += over.top;
            inset.bottom += over.bottom;
        }
    }
    out.append(&mut placed);
    used
}

/// The smallest rect holding every node's (possibly rotated) box in `nodes`,
/// in the frame that `to_local` maps page points into (`None`: the page);
/// `None` if there are no nodes.
fn union_of(nodes: &[ComputedNode], to_local: Option<Affine>) -> Option<Rect> {
    let mut points = nodes
        .iter()
        .flat_map(|n| n.corners())
        .map(|p| match &to_local {
            Some(t) => t.apply(p),
            None => p,
        });
    let first = points.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for p in points {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// A column's or row's own layout parameters.
#[derive(Clone, Copy)]
pub(super) struct Flex<'a> {
    pub axis: Axis,
    pub children: &'a [usize],
    /// Connector children, out of flow (see `NodeType::overlays`).
    pub overlays: &'a [(usize, usize)],
    pub spacing: f32,
    /// Column: first baselines this far apart (see `pitch_gaps`).
    pub pitch: Option<f32>,
    pub main_align: MainAxisAlign,
    pub cross_align: CrossAxisAlign,
    /// The container is a column in an unbounded (content-sized) vertical
    /// axis — children are measured with unbounded height and keep their
    /// measured height, flex or not (auto-height pages).
    pub content_h: bool,
    /// Row: the children that scale with their width share one height and
    /// fill the row (see [`equal_height_widths`]).
    pub equal_height: bool,
}

impl<'a> Flex<'a> {
    /// The parameters of node `idx`, if it is a column or row.
    pub(super) fn of<M: LeafMeasure>(ctx: &Ctx<'a, M>, idx: usize) -> Option<Flex<'a>> {
        let NodeType::Container {
            axis,
            children,
            spacing,
            baseline_pitch,
            main_axis,
            cross_axis,
            equal_height,
            overlays,
            ..
        } = &ctx.doc.nodes[idx].kind
        else {
            return None;
        };
        Some(Flex {
            axis: *axis,
            children,
            overlays,
            spacing: *spacing,
            pitch: baseline_pitch.filter(|_| *axis == Axis::Vertical),
            main_align: *main_axis,
            cross_align: *cross_axis,
            content_h: *axis == Axis::Vertical && ctx.is_content_h(idx),
            equal_height: *equal_height && *axis == Axis::Horizontal,
        })
    }
}

/// Lay a column's/row's (node `idx`) children out in `content` and arrange
/// each in its slot. An overflow along the main axis is reported; under
/// `overflow = "shrink"` the children are laid out in an enlarged box and
/// scaled into `content` instead. Returns whether the subtree must be
/// clipped to the node's box (`overflow = "clip"` and an overflow).
fn layout_children<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    flex: &Flex,
    content: Rect,
    overflow: Overflow,
    xf: Option<Affine>,
    out: &mut Vec<ComputedNode>,
) -> bool {
    let have = main_of(flex.axis, content.size());
    let scales = matches!(overflow, Overflow::Shrink | Overflow::Fit) && !flex.content_h;
    if scales {
        if let Some((s, need)) = shrink_scale(ctx, flex, content) {
            // Laid out as usual in a box as wide (tall, for a row) as the
            // content box and long enough along the main axis, then scaled
            // by `s` into the content box, centred across.
            let (virt, dx, dy) = match flex.axis {
                Axis::Vertical => (
                    Rect::new(content.x, content.y, content.w, content.h / s),
                    content.w * (1.0 - s) / 2.0,
                    0.0,
                ),
                Axis::Horizontal => (
                    Rect::new(content.x, content.y, content.w / s, content.h),
                    0.0,
                    content.h * (1.0 - s) / 2.0,
                ),
            };
            let scale = Affine {
                a: s,
                b: 0.0,
                c: 0.0,
                d: s,
                e: content.x * (1.0 - s) + dx,
                f: content.y * (1.0 - s) + dy,
            };
            let cxf = Some(match xf {
                Some(p) => scale.then(p),
                None => scale,
            });
            let slots = flex_slots(ctx, flex, virt);
            report_overflow(
                ctx,
                idx,
                flex,
                &slots,
                need,
                have,
                OverflowAction::Shrunk(s),
            );
            arrange_in_order(ctx, flex.children, flex.overlays, slots, virt, cxf, out);
            return false;
        }
    }
    if overflow == Overflow::Fit && !flex.content_h {
        if let Some((s, virt, shift)) = grow_scale(ctx, flex, content) {
            // Laid out in a box `1 / s` of the content box (as wide across
            // as the children are), then scaled up by `s` about the content
            // box's top-left and centred across.
            let scale = Affine {
                a: s,
                b: 0.0,
                c: 0.0,
                d: s,
                e: content.x * (1.0 - s) + shift.0,
                f: content.y * (1.0 - s) + shift.1,
            };
            let cxf = Some(match xf {
                Some(p) => scale.then(p),
                None => scale,
            });
            let slots = flex_slots(ctx, flex, virt);
            arrange_in_order(ctx, flex.children, flex.overlays, slots, virt, cxf, out);
            return false;
        }
    }
    let slots = flex_slots(ctx, flex, content);
    // How far the last margin box runs past the content box.
    let end = match flex.axis {
        Axis::Vertical => content.y + content.h,
        Axis::Horizontal => content.x + content.w,
    };
    let reach = flex
        .children
        .iter()
        .zip(&slots)
        .map(|(&c, s)| {
            let m = ctx.doc.nodes[c].common.margin;
            match flex.axis {
                Axis::Vertical => s.y + s.h + m.bottom,
                Axis::Horizontal => s.x + s.w + m.right,
            }
        })
        .fold(f32::NEG_INFINITY, f32::max);
    let over = reach - end;
    let clip = over > 0.01;
    if clip {
        let action = match overflow {
            Overflow::Clip => OverflowAction::Clipped,
            _ => OverflowAction::Visible,
        };
        report_overflow(ctx, idx, flex, &slots, have + over, have, action);
    }
    arrange_in_order(ctx, flex.children, flex.overlays, slots, content, xf, out);
    clip && overflow == Overflow::Clip
}

/// The scale `s` (< 1) that fits a column's/row's children, laid out at
/// the content box's cross size, into `content` along the main axis — a
/// uniform scale of the layout, so images and text shrink alike — with the
/// unscaled extent they need; `None` when they fit as they are.
fn shrink_scale<M: LeafMeasure>(ctx: &Ctx<M>, flex: &Flex, content: Rect) -> Option<(f32, f32)> {
    let axis = flex.axis;
    let have = main_of(axis, content.size());
    let cross = cross_of(axis, content.size());
    let c = match axis {
        Axis::Vertical => Constraints::loose(cross, f32::INFINITY).with_tight_w(cross),
        Axis::Horizontal => Constraints::loose(f32::INFINITY, cross).with_tight_h(cross),
    };
    let need = main_of(
        axis,
        measure_container(
            ctx,
            axis,
            flex.children,
            (flex.spacing, flex.pitch),
            (flex.cross_align, flex.equal_height),
            &Common::default(),
            c,
        ),
    );
    if need <= have + 0.01 || need.is_nan() || have <= 0.0 {
        return None;
    }
    Some(((have / need).max(0.01), need))
}

/// `overflow = "fit"` with room to spare: the scale `s` (> 1) at which the
/// children's natural extent (measured loose in `content`) fills `content`
/// on one axis, the box `1 / s` of it to lay them out in (as wide across as
/// they are, so they don't stretch into the spare room), and the shift
/// that centres them across; `None` when they fill an axis already.
fn grow_scale<M: LeafMeasure>(
    ctx: &Ctx<M>,
    flex: &Flex,
    content: Rect,
) -> Option<(f32, Rect, (f32, f32))> {
    let nat = measure_container(
        ctx,
        flex.axis,
        flex.children,
        (flex.spacing, flex.pitch),
        (flex.cross_align, flex.equal_height),
        &Common::default(),
        Constraints::loose(content.w, content.h),
    );
    if !(nat.w > 0.01 && nat.h > 0.01 && nat.w.is_finite() && nat.h.is_finite()) {
        return None;
    }
    let s = (content.w / nat.w).min(content.h / nat.h);
    if !(s > 1.001 && s.is_finite()) {
        return None;
    }
    // Along the main axis the box keeps the content's length (alignment
    // places the children in it); across, the children's own extent.
    Some(match flex.axis {
        Axis::Vertical => (
            s,
            Rect::new(content.x, content.y, nat.w, content.h / s),
            ((content.w - nat.w * s) / 2.0, 0.0),
        ),
        Axis::Horizontal => (
            s,
            Rect::new(content.x, content.y, content.w / s, nat.h),
            (0.0, (content.h - nat.h * s) / 2.0),
        ),
    })
}

/// Record (once per node) that node `idx`'s children need `need` points
/// along its main axis where it has `have`, naming it and its named
/// children (with their extent in `slots`) for the message.
fn report_overflow<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    flex: &Flex,
    slots: &[Rect],
    need: f32,
    have: f32,
    action: OverflowAction,
) {
    let node = &ctx.doc.nodes[idx];
    let mut reports = ctx.overflows.borrow_mut();
    if reports.iter().any(|r| r.node == node.id) {
        return;
    }
    let children = flex
        .children
        .iter()
        .zip(slots)
        .filter_map(|(&c, s)| {
            let name = ctx.doc.nodes[c].name.clone()?;
            Some((name, main_of(flex.axis, s.size())))
        })
        .collect();
    reports.push(OverflowReport {
        node: node.id.clone(),
        name: node.name.clone(),
        within: ctx.named_ancestor(idx),
        axis: flex.axis,
        need_pt: need,
        room_pt: have,
        action,
        children,
    });
}

/// The slot (border box in the container's frame) of each child of a
/// column/row whose content box is `content`.
pub(super) fn flex_slots<M: LeafMeasure>(ctx: &Ctx<M>, flex: &Flex, content: Rect) -> Vec<Rect> {
    let Flex {
        axis,
        children,
        spacing,
        pitch,
        main_align,
        cross_align,
        content_h,
        equal_height,
        ..
    } = *flex;
    let n = children.len();
    if n == 0 {
        return Vec::new();
    }

    let content_main = main_of(axis, content.size());
    let content_cross = cross_of(axis, content.size());
    let base_c = Constraints::loose(content.w, if content_h { f32::INFINITY } else { content.h });
    // Stretched children get a tight cross size, so whatever depends on it (an
    // aspect-ratio leaf's main size, a nested row's flex shares) is measured as
    // it will be arranged — matching the measure pass. A child with its own
    // cross size is not stretched.
    // An overhanging child, or one with a natural size (an image with a
    // `dpi`), isn't stretched either.
    let stretched = |child: usize| {
        cross_align == CrossAxisAlign::Stretch && !keeps_cross_size(ctx, child, axis)
    };
    let overhang = |i: usize| ctx.doc.nodes[children[i]].common.overhang;
    let child_c = |child: usize, main: Option<f32>| {
        let mut cc = base_c;
        if ctx.doc.nodes[child].common.overhang {
            // Its natural cross size, whatever the room.
            cc = match axis {
                Axis::Vertical => Constraints {
                    max_w: f32::INFINITY,
                    ..cc
                },
                Axis::Horizontal => Constraints {
                    max_h: f32::INFINITY,
                    ..cc
                },
            };
        }
        if stretched(child) {
            let m = ctx.doc.nodes[child].common.margin;
            cc = tighten(cc, cross_axis(axis), content_cross - cross_margin(axis, m));
        }
        if let Some(v) = main {
            cc = tighten(cc, axis, v);
        }
        cc
    };

    // An equal-height row: the widths of the children that scale with it.
    let shared: Option<Vec<Option<f32>>> = equal_height.then(|| {
        let gaps = spacing * (n as f32 - 1.0);
        equal_height_widths(ctx, children, content_main, gaps, base_c)
    });

    // Measure each child's border box and record its flex weight + margins.
    let mut measured: Vec<Size> = Vec::with_capacity(n);
    let mut margins: Vec<Edges> = Vec::with_capacity(n);
    let mut flex: Vec<Option<f32>> = Vec::with_capacity(n);
    for (i, &child) in children.iter().enumerate() {
        let common = &ctx.doc.nodes[child].common;
        let w = shared.as_ref().and_then(|ws| ws[i]);
        measured.push(measure(ctx, child, child_c(child, w)));
        margins.push(common.margin);
        // No free space to share along an unbounded axis (or in an
        // equal-height row): flex is ignored.
        flex.push(
            common
                .flex
                .filter(|w| *w > 0.0 && !content_h && shared.is_none()),
        );
    }

    // The gap after each child: `spacing`, or what puts the first baselines
    // `baseline_pitch` apart.
    let gap_after: Vec<f32> = match pitch {
        Some(p) => pitch_gaps(ctx, children, &measured, spacing, p),
        None => vec![spacing; n - 1],
    };
    let gaps: f32 = gap_after.iter().sum();
    let margin_main = |i: usize| match axis {
        Axis::Vertical => margins[i].vertical(),
        Axis::Horizontal => margins[i].horizontal(),
    };
    let margin_cross = |i: usize| match axis {
        Axis::Vertical => margins[i].horizontal(),
        Axis::Horizontal => margins[i].vertical(),
    };

    // Fixed children consume their measured main size; the remainder is shared
    // among flex children by weight.
    let fixed_main: f32 = (0..n)
        .filter(|&i| flex[i].is_none())
        .map(|i| main_of(axis, measured[i]) + margin_main(i))
        .sum();
    let total_flex: f32 = flex.iter().filter_map(|f| *f).sum();
    let free = (content_main - fixed_main - gaps).max(0.0);

    // Final border-box main size for each child.
    let mut main_size: Vec<f32> = Vec::with_capacity(n);
    for i in 0..n {
        let m = match flex[i] {
            Some(w) if total_flex > 0.0 => (free * w / total_flex - margin_main(i)).max(0.0),
            _ => main_of(axis, measured[i]),
        };
        main_size.push(m);
    }

    // Re-measure flex children at their final main size so a cross size that
    // depends on it (aspect ratio, wrapped content) is right when not stretched.
    for i in 0..n {
        if flex[i].is_some() && total_flex > 0.0 {
            measured[i] = measure(ctx, children[i], child_c(children[i], Some(main_size[i])));
        }
    }

    // Leftover space (after fixed + flex + gaps) drives main-axis alignment.
    let used: f32 = (0..n).map(|i| main_size[i] + margin_main(i)).sum::<f32>() + gaps;
    let leftover = (content_main - used).max(0.0);
    let (start_offset, extra_gap) = main_axis_distribution(main_align, leftover, n);

    let main_start = match axis {
        Axis::Vertical => content.y,
        Axis::Horizontal => content.x,
    };
    let cross_start = match axis {
        Axis::Vertical => content.x,
        Axis::Horizontal => content.y,
    };

    // Cross-axis size: stretch fills the content cross extent; otherwise the
    // measured cross size. aspect_ratio, when present, derives cross from the
    // (possibly flex-imposed) main size and takes precedence over stretch.
    let cross_size: Vec<f32> = (0..n)
        .map(|i| {
            let bmain = main_size[i];
            let mut bcross = if stretched(children[i]) {
                (content_cross - margin_cross(i)).max(0.0)
            } else {
                cross_of(axis, measured[i])
            };
            if let Some(ar) = ctx.doc.nodes[children[i]].common.outer().aspect_ratio {
                if ar > 0.0 {
                    bcross = match axis {
                        // column: main = height, cross = width = height * ar
                        Axis::Vertical => bmain * ar,
                        // row: main = width, cross = height = width / ar
                        Axis::Horizontal => bmain / ar,
                    };
                }
            }
            if overhang(i) {
                bcross
            } else {
                bcross.min((content_cross - margin_cross(i)).max(0.0))
            }
        })
        .collect();

    // A baseline row puts every child's first baseline (or bottom edge) on
    // one line, as low as the child reaching furthest above it needs.
    // (`cap_middle`: every child's first-line cap middle, or box middle.)
    let baselines: Option<Vec<f32>> = row_line(axis, cross_align).map(|kind| {
        (0..n)
            .map(|i| {
                let fp = Size::new(main_size[i], cross_size[i]);
                row_baseline(ctx, children[i], fp, margins[i], kind)
            })
            .collect()
    });
    let shared_baseline = baselines
        .as_ref()
        .map_or(0.0, |b| b.iter().copied().fold(0.0, f32::max));
    // An axis-aligned container puts every child's axis on one line, and
    // centres the group (tallest part before the line + after it) across.
    let axes: Option<(Vec<f32>, f32)> = (cross_align == CrossAxisAlign::Axis).then(|| {
        let b: Vec<f32> = (0..n)
            .map(|i| {
                let fp = match axis {
                    Axis::Horizontal => Size::new(main_size[i], cross_size[i]),
                    Axis::Vertical => Size::new(cross_size[i], main_size[i]),
                };
                axis_line(ctx, axis, children[i], fp, margins[i])
            })
            .collect();
        // Overhanging children sit on the line without widening the group.
        let before = (0..n)
            .filter(|&i| !overhang(i))
            .map(|i| b[i])
            .fold(0.0, f32::max);
        let after = (0..n)
            .filter(|&i| !overhang(i))
            .map(|i| cross_size[i] + margin_cross(i) - b[i])
            .fold(0.0, f32::max);
        let line = ((content_cross - before - after) / 2.0).max(0.0) + before;
        (b, line)
    });

    let mut cursor = main_start + start_offset;
    let mut slots = Vec::with_capacity(n);
    for i in 0..n {
        let bmain = main_size[i];
        let bcross = cross_size[i];

        let cross_off = match (&baselines, &axes) {
            (Some(b), _) => shared_baseline - b[i],
            (None, Some((b, line))) => line - b[i],
            (None, None) if overhang(i) => {
                // Centred (unless aligned to an edge), past both edges.
                let free = content_cross - bcross - margin_cross(i);
                match cross_align {
                    CrossAxisAlign::Start => 0.0,
                    CrossAxisAlign::End => free,
                    _ => free / 2.0,
                }
            }
            (None, None) => cross_axis_offset(cross_align, content_cross, bcross + margin_cross(i)),
        };

        let main_pos = cursor + margin_leading_main(axis, margins[i]);
        let cross_pos = cross_start + cross_off + margin_leading_cross(axis, margins[i]);

        slots.push(match axis {
            Axis::Vertical => Rect::new(cross_pos, main_pos, bcross, bmain),
            Axis::Horizontal => Rect::new(main_pos, cross_pos, bmain, bcross),
        });

        cursor += margin_main(i) + bmain + gap_after.get(i).copied().unwrap_or(0.0) + extra_gap;
    }
    slots
}

fn margin_leading_main(axis: Axis, m: Edges) -> f32 {
    match axis {
        Axis::Vertical => m.top,
        Axis::Horizontal => m.left,
    }
}

fn margin_leading_cross(axis: Axis, m: Edges) -> f32 {
    match axis {
        Axis::Vertical => m.left,
        Axis::Horizontal => m.top,
    }
}

/// Returns `(start_offset, extra_gap_between_children)` for a main-axis
/// alignment, given the leftover free space and child count.
///
/// `space_*` modes are only meaningful when there is leftover space (i.e. no
/// flex child ate it); with flex present `leftover` is ~0 and these reduce to
/// `start`.
fn main_axis_distribution(align: MainAxisAlign, leftover: f32, n: usize) -> (f32, f32) {
    let n = n as f32;
    match align {
        MainAxisAlign::Start => (0.0, 0.0),
        MainAxisAlign::Center => (leftover / 2.0, 0.0),
        MainAxisAlign::End => (leftover, 0.0),
        MainAxisAlign::SpaceBetween => {
            if n > 1.0 {
                (0.0, leftover / (n - 1.0))
            } else {
                (leftover / 2.0, 0.0)
            }
        }
        MainAxisAlign::SpaceAround => {
            let gap = leftover / n;
            (gap / 2.0, gap)
        }
        MainAxisAlign::SpaceEvenly => {
            let gap = leftover / (n + 1.0);
            (gap, gap)
        }
    }
}

/// Offset of a child within the cross extent, given the child's full cross
/// footprint (border box + cross margins).
fn cross_axis_offset(align: CrossAxisAlign, content_cross: f32, child_cross: f32) -> f32 {
    let slack = (content_cross - child_cross).max(0.0);
    match align {
        // Baseline is resolved by the caller in rows; in a column it is start.
        CrossAxisAlign::Start
        | CrossAxisAlign::Stretch
        | CrossAxisAlign::Baseline
        | CrossAxisAlign::CapMiddle
        | CrossAxisAlign::Axis => 0.0,
        CrossAxisAlign::Center => slack / 2.0,
        CrossAxisAlign::End => slack,
    }
}

/// Whether a path's points, not all coincident in its own (viewbox)
/// coordinates, collapsed onto one point when mapped onto its box.
fn collapsed_path(kind: &NodeType, content: &PaintContent) -> bool {
    let (NodeType::Path(props), PaintContent::Path(paint)) = (kind, content) else {
        return false;
    };
    let extent = |segs: &[crate::geom::PathSegment]| {
        crate::path::bounds(segs).map_or(0.0, |(lo, hi)| (hi.x - lo.x).max(hi.y - lo.y))
    };
    props.viewbox.is_some() && extent(&props.segments) > 0.0 && extent(&paint.segments) < 1e-3
}

fn paint_content(kind: &NodeType, common: &Common, rect: Rect) -> PaintContent {
    match kind {
        NodeType::Container { .. } | NodeType::Stack { .. } => match kind.decoration() {
            Some(d) if d.fill.is_some() || d.stroke.is_some() => PaintContent::Frame(FramePaint {
                fill: d.fill.as_ref().map(|p| p.place(rect)),
                stroke: d.stroke.clone(),
                corner_radius: d.corner_radius,
                stroke_align: d.stroke_align,
            }),
            _ => PaintContent::Container,
        },
        NodeType::Rect(RectProps {
            fill,
            stroke,
            stroke_width,
            corner_radius,
            stroke_align,
        }) => PaintContent::Rect {
            fill: fill.as_ref().map(|p| p.place(rect)),
            stroke: *stroke,
            stroke_width: *stroke_width,
            corner_radius: *corner_radius,
            stroke_align: *stroke_align,
        },
        NodeType::Image(props) => PaintContent::Image {
            src: props.src.clone().into(),
            fit: props.fit,
            smooth: props.smooth,
        },
        NodeType::Text(TextProps {
            content,
            font_size,
            font_family,
            font_weight,
            color,
            align,
            line_height,
            stroke,
            font_style,
            underline,
            strike,
            letter_spacing,
            spans,
            wrap,
            fit_width,
            typography,
        }) => PaintContent::Text(TextBox {
            wrap: *wrap,
            fit_width: *fit_width,
            typography: *typography,
            content: content.clone(),
            spans: spans.clone(),
            font_style: *font_style,
            underline: *underline,
            strike: *strike,
            letter_spacing: *letter_spacing,
            font_size: *font_size,
            font_family: font_family.clone(),
            font_weight: *font_weight,
            color: *color,
            align: *align,
            line_height: *line_height,
            stroke: stroke.clone(),
        }),
        NodeType::Path(props) => {
            // Box-relative segments -> absolute page points (content-box
            // origin), or viewbox units mapped onto the content box.
            let origin = rect.inset(common.padding);
            PaintContent::Path(PathPaint {
                segments: props
                    .segments
                    .iter()
                    .map(|s| match props.viewbox {
                        Some(vb) => s.mapped(|p| vb.map(p, origin)),
                        None => s.translated(origin.x, origin.y),
                    })
                    .collect(),
                fill: props.fill.as_ref().map(|p| p.place(rect)),
                stroke: props.stroke.clone(),
                head: props.head,
                head_size: props.head_size,
                head_width: props.head_width,
                head_style: props.head_style,
                stroke_align: props.stroke_align,
            })
        }
        NodeType::Ellipse(props) => PaintContent::Ellipse {
            fill: props.fill.as_ref().map(|p| p.place(rect)),
            stroke: props.stroke.clone(),
            stroke_align: props.stroke_align,
        },
        // Drawn after layout (`connect::place_connectors`).
        NodeType::Connector(_) => PaintContent::Container,
    }
}
