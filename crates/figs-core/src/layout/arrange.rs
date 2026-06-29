//! Pass 2 — arrange. Given the border-box rect a parent assigns to a node,
//! place it, resolve flex among its children, apply alignment, and recurse.
//! Produces [`ComputedNode`]s in paint order (parent before children).

use crate::geom::{Axis, CrossAxisAlign, Edges, MainAxisAlign, Rect, Size};
use crate::schema::{NodeType, RectProps, TextProps};

use super::computed::{ComputedNode, PaintContent, TextBox};
use super::measure::measure;
use super::{cross_of, main_of, Constraints, Ctx, LeafMeasure};

/// Place node `idx` into `rect` (its border box) and recurse into children.
pub(crate) fn arrange<M: LeafMeasure>(
    ctx: &Ctx<M>,
    idx: usize,
    rect: Rect,
    out: &mut Vec<ComputedNode>,
) {
    let node = &ctx.doc.nodes[idx];
    out.push(ComputedNode {
        id: node.id.clone(),
        rect,
        content: paint_content(&node.kind),
    });

    if let NodeType::Container {
        axis,
        children,
        spacing,
        main_axis,
        cross_axis,
    } = &node.kind
    {
        let content = rect.inset(node.common.padding);
        layout_children(
            ctx, *axis, children, *spacing, *main_axis, *cross_axis, content, out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_children<M: LeafMeasure>(
    ctx: &Ctx<M>,
    axis: Axis,
    children: &[usize],
    spacing: f32,
    main_align: MainAxisAlign,
    cross_align: CrossAxisAlign,
    content: Rect,
    out: &mut Vec<ComputedNode>,
) {
    let n = children.len();
    if n == 0 {
        return;
    }

    let content_main = main_of(axis, content.size());
    let content_cross = cross_of(axis, content.size());
    let child_c = Constraints::loose(content.w, content.h);

    // Measure each child's border box and record its flex weight + margins.
    let mut measured: Vec<Size> = Vec::with_capacity(n);
    let mut margins: Vec<Edges> = Vec::with_capacity(n);
    let mut flex: Vec<Option<f32>> = Vec::with_capacity(n);
    for &child in children {
        measured.push(measure(ctx, child, child_c));
        let common = &ctx.doc.nodes[child].common;
        margins.push(common.margin);
        flex.push(common.flex.filter(|w| *w > 0.0));
    }

    let gaps = spacing * (n as f32 - 1.0);
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

    if content_main - fixed_main - gaps < -0.01 {
        tracing::warn!(
            overflow = fixed_main + gaps - content_main,
            "children overflow container main axis; clamping"
        );
    }

    // Final border-box main size for each child.
    let mut main_size: Vec<f32> = Vec::with_capacity(n);
    for i in 0..n {
        let m = match flex[i] {
            Some(w) if total_flex > 0.0 => (free * w / total_flex - margin_main(i)).max(0.0),
            _ => main_of(axis, measured[i]),
        };
        main_size.push(m);
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

    let mut cursor = main_start + start_offset;
    for i in 0..n {
        let child = children[i];
        let bmain = main_size[i];

        // Cross-axis size: stretch fills the content cross extent; otherwise the
        // measured cross size. aspect_ratio, when present, derives cross from the
        // (possibly flex-imposed) main size and takes precedence over stretch.
        let mut bcross = match cross_align {
            CrossAxisAlign::Stretch => (content_cross - margin_cross(i)).max(0.0),
            _ => cross_of(axis, measured[i]),
        };
        if let Some(ar) = ctx.doc.nodes[child].common.aspect_ratio {
            if ar > 0.0 {
                bcross = match axis {
                    // column: main = height, cross = width = height * ar
                    Axis::Vertical => bmain * ar,
                    // row: main = width, cross = height = width / ar
                    Axis::Horizontal => bmain / ar,
                };
            }
        }
        bcross = bcross.min((content_cross - margin_cross(i)).max(0.0));

        let cross_off = cross_axis_offset(cross_align, content_cross, bcross + margin_cross(i));

        let main_pos = cursor + margin_leading_main(axis, margins[i]);
        let cross_pos = cross_start + cross_off + margin_leading_cross(axis, margins[i]);

        let child_rect = match axis {
            Axis::Vertical => Rect::new(cross_pos, main_pos, bcross, bmain),
            Axis::Horizontal => Rect::new(main_pos, cross_pos, bmain, bcross),
        };

        arrange(ctx, child, child_rect, out);

        cursor += margin_main(i) + bmain + spacing + extra_gap;
    }
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
        CrossAxisAlign::Start | CrossAxisAlign::Stretch => 0.0,
        CrossAxisAlign::Center => slack / 2.0,
        CrossAxisAlign::End => slack,
    }
}

fn paint_content(kind: &NodeType) -> PaintContent {
    match kind {
        NodeType::Container { .. } => PaintContent::Container,
        NodeType::Rect(RectProps {
            fill,
            stroke,
            stroke_width,
            corner_radius,
        }) => PaintContent::Rect {
            fill: *fill,
            stroke: *stroke,
            stroke_width: *stroke_width,
            corner_radius: *corner_radius,
        },
        NodeType::Image(props) => PaintContent::Image {
            src: props.src.clone().into(),
            fit: props.fit,
        },
        NodeType::Text(TextProps {
            content,
            font_size,
            font_family,
            font_weight,
            color,
            align,
            line_height,
        }) => PaintContent::Text(TextBox {
            content: content.clone(),
            font_size: *font_size,
            font_family: font_family.clone(),
            font_weight: *font_weight,
            color: *color,
            align: *align,
            line_height: *line_height,
        }),
    }
}
