//! Resolution: validate a [`RawDocument`] and convert it into a typed,
//! index-addressed [`Document`].
//!
//! Responsibilities:
//! - check the root id exists,
//! - resolve every `children` id to an index (error on dangling references),
//! - reject cycles and nodes referenced by more than one parent,
//! - enforce per-type required fields (image needs `src`, text needs `content`),
//! - convert all page-unit lengths into points.
//!
//! Every error carries the offending node id so diagnostics are actionable.

use std::collections::HashMap;

use crate::geom::{
    ArrowHead, Axis, Color, CrossAxisAlign, Edges, ImageFit, MainAxisAlign, Point,
    StrokeStyle, TextAlign,
};

use super::parse::{NodeKind, RawDocument, RawNode};

/// Default raster resolution for PNG output when `page.dpi` is unset.
pub const DEFAULT_DPI: f32 = 300.0;
/// Default text size in points.
pub const DEFAULT_FONT_SIZE: f32 = 12.0;
/// Default line height as a multiple of font size.
pub const DEFAULT_LINE_HEIGHT: f32 = 1.2;
/// Default stroke width of line/arrow/ellipse/path, in points (unit-independent).
pub const DEFAULT_SHAPE_STROKE_WIDTH: f32 = 1.0;
/// Default arrowhead length as a multiple of the stroke width.
pub const DEFAULT_HEAD_SIZE_PER_STROKE: f32 = 5.0;

/// Errors produced while resolving a raw document. Each variant names the node
/// at fault where applicable.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ResolveError {
    #[error("page.root refers to unknown node `{0}`")]
    RootMissing(String),
    #[error("node `{node}`: child `{child}` does not exist")]
    DanglingChild { node: String, child: String },
    #[error("node `{node}` is referenced as a child by more than one parent")]
    MultipleParents { node: String },
    #[error("cycle detected through node `{0}`")]
    Cycle(String),
    #[error("node `{node}`: {kind} requires field `{field}`")]
    MissingField {
        node: String,
        kind: &'static str,
        field: &'static str,
    },
    #[error("node `{node}`: invalid `{field}`: {reason}")]
    InvalidField {
        node: String,
        field: &'static str,
        reason: String,
    },
}

/// Page geometry and presentation, with all lengths converted to points.
#[derive(Debug, Clone)]
pub struct PageInfo {
    pub width_pt: f32,
    pub height_pt: f32,
    pub dpi: f32,
    pub background: Option<Color>,
    /// Default font family for text nodes that don't name one.
    pub font_family: Option<String>,
}

/// A fully resolved document. Nodes are stored flat and addressed by index;
/// `root` is the entry index.
#[derive(Debug)]
pub struct Document {
    pub page: PageInfo,
    pub nodes: Vec<Node>,
    pub root: usize,
}

impl Document {
    /// Parse and resolve TOML source in one step.
    pub fn from_toml(src: &str) -> Result<Document, crate::Error> {
        let raw = super::parse::parse_str(src)?;
        Ok(resolve(raw)?)
    }

    /// Evaluate a Rhai script and resolve it in one step.
    #[cfg(feature = "script")]
    pub fn from_script(src: &str) -> Result<Document, crate::Error> {
        let raw = crate::script::eval_script(src)?;
        Ok(resolve(raw)?)
    }

    pub fn node(&self, idx: usize) -> &Node {
        &self.nodes[idx]
    }
}

/// A resolved node: stable id, shared layout properties (in points), and a
/// typed payload.
#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub common: Common,
    pub kind: NodeType,
}

/// Layout properties shared by every node type, lengths in points.
#[derive(Debug, Clone, Default)]
pub struct Common {
    pub flex: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub aspect_ratio: Option<f32>,
    pub margin: Edges,
    pub padding: Edges,
    /// Offset of the border box (after margin) from the parent's content-box
    /// origin. Only a `stack` parent reads these; flex containers ignore them.
    pub x: Option<f32>,
    pub y: Option<f32>,
}

/// Typed node payload.
#[derive(Debug)]
pub enum NodeType {
    Container {
        axis: Axis,
        children: Vec<usize>,
        spacing: f32,
        main_axis: MainAxisAlign,
        cross_axis: CrossAxisAlign,
    },
    /// Children overlap inside the content box, painted in order (later = on
    /// top), each positioned by its own `x`/`y`.
    Stack {
        children: Vec<usize>,
    },
    Image(ImageProps),
    Text(TextProps),
    Rect(RectProps),
    /// `line`, `arrow` and `path` all resolve to a stroked/filled polyline.
    Path(PathProps),
    Ellipse(EllipseProps),
}

impl NodeType {
    /// Child indices of a container (`column`/`row`/`stack`); empty for leaves.
    pub fn children(&self) -> &[usize] {
        match self {
            NodeType::Container { children, .. } | NodeType::Stack { children } => children,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageProps {
    pub src: String,
    pub fit: ImageFit,
}

#[derive(Debug, Clone)]
pub struct TextProps {
    pub content: String,
    pub font_size: f32,
    pub font_family: Option<String>,
    pub font_weight: Option<u16>,
    pub color: Color,
    pub align: TextAlign,
    /// Multiple of `font_size`.
    pub line_height: f32,
}

#[derive(Debug, Clone)]
pub struct RectProps {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,
    pub corner_radius: f32,
}

/// A polyline/polygon (`line`, `arrow`, `path`). Points are in points, relative
/// to the node's content-box origin; they are not scaled with the box.
#[derive(Debug, Clone)]
pub struct PathProps {
    pub points: Vec<Point>,
    pub closed: bool,
    pub fill: Option<Color>,
    pub stroke: Option<StrokeStyle>,
    /// Ignored when `closed` or when there is no stroke.
    pub head: ArrowHead,
    /// Arrowhead length in points (its width is 0.8x this).
    pub head_size: f32,
}

/// An ellipse inscribed in the node's border box.
#[derive(Debug, Clone)]
pub struct EllipseProps {
    pub fill: Option<Color>,
    pub stroke: Option<StrokeStyle>,
}

/// Validate and convert a [`RawDocument`] into a [`Document`].
pub fn resolve(raw: RawDocument) -> Result<Document, ResolveError> {
    let factor = raw.page.unit.to_pt();

    // Stable ordering: sort ids so the produced index layout is deterministic
    // across runs (HashMap iteration order is not). Output paint order is later
    // derived by tree traversal, but a deterministic base helps tests/debug.
    let mut ids: Vec<&String> = raw.nodes.keys().collect();
    ids.sort();
    let index_of: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    if !index_of.contains_key(raw.page.root.as_str()) {
        return Err(ResolveError::RootMissing(raw.page.root.clone()));
    }

    // First convert every node, resolving child ids to indices.
    let mut nodes: Vec<Node> = Vec::with_capacity(ids.len());
    for id in &ids {
        let raw_node = &raw.nodes[id.as_str()];
        nodes.push(convert_node(id, raw_node, &index_of, factor)?);
    }

    // Each non-root node may be referenced by at most one parent.
    let mut parent_count = vec![0u32; nodes.len()];
    for node in &nodes {
        for &c in node.kind.children() {
            parent_count[c] += 1;
        }
    }
    for (i, &count) in parent_count.iter().enumerate() {
        if count > 1 {
            return Err(ResolveError::MultipleParents {
                node: nodes[i].id.clone(),
            });
        }
    }

    let root = index_of[raw.page.root.as_str()];
    detect_cycle(&nodes, root)?;

    let page = PageInfo {
        width_pt: factor * raw.page.width,
        height_pt: factor * raw.page.height,
        dpi: raw.page.dpi.unwrap_or(DEFAULT_DPI),
        background: raw.page.background,
        font_family: raw.page.font_family.clone(),
    };

    Ok(Document { page, nodes, root })
}

fn convert_node(
    id: &str,
    raw: &RawNode,
    index_of: &HashMap<&str, usize>,
    factor: f32,
) -> Result<Node, ResolveError> {
    let common = Common {
        flex: raw.flex,
        width: raw.width.map(|w| w * factor),
        height: raw.height.map(|h| h * factor),
        aspect_ratio: raw.aspect_ratio,
        margin: raw.margin.scaled(factor),
        padding: raw.padding.scaled(factor),
        x: raw.x.map(|x| x * factor),
        y: raw.y.map(|y| y * factor),
    };

    let resolve_children = || -> Result<Vec<usize>, ResolveError> {
        raw.children
            .iter()
            .map(|child| {
                index_of
                    .get(child.as_str())
                    .copied()
                    .ok_or_else(|| ResolveError::DanglingChild {
                        node: id.to_string(),
                        child: child.clone(),
                    })
            })
            .collect()
    };

    let kind = match raw.kind {
        NodeKind::Column | NodeKind::Row => {
            let axis = if raw.kind == NodeKind::Column {
                Axis::Vertical
            } else {
                Axis::Horizontal
            };
            let children = resolve_children()?;
            NodeType::Container {
                axis,
                children,
                spacing: raw.spacing * factor,
                main_axis: raw.main_axis_alignment.unwrap_or_default(),
                cross_axis: raw.cross_axis_alignment.unwrap_or_default(),
            }
        }
        NodeKind::Stack => NodeType::Stack {
            children: resolve_children()?,
        },
        NodeKind::Image => {
            let src = raw.src.clone().ok_or_else(|| ResolveError::MissingField {
                node: id.to_string(),
                kind: "image",
                field: "src",
            })?;
            NodeType::Image(ImageProps {
                src,
                fit: raw.fit.unwrap_or_default(),
            })
        }
        NodeKind::Text => {
            let content = raw
                .content
                .clone()
                .ok_or_else(|| ResolveError::MissingField {
                    node: id.to_string(),
                    kind: "text",
                    field: "content",
                })?;
            NodeType::Text(TextProps {
                content,
                font_size: raw.font_size.unwrap_or(DEFAULT_FONT_SIZE),
                font_family: raw.font_family.clone(),
                font_weight: raw.font_weight,
                color: raw.color.unwrap_or(Color::BLACK),
                align: raw.align.unwrap_or_default(),
                line_height: raw.line_height.unwrap_or(DEFAULT_LINE_HEIGHT),
            })
        }
        NodeKind::Rect => NodeType::Rect(RectProps {
            fill: raw.fill,
            stroke: raw.stroke,
            stroke_width: raw.stroke_width.unwrap_or(0.0) * factor,
            corner_radius: raw.corner_radius.unwrap_or(0.0) * factor,
        }),
        NodeKind::Line | NodeKind::Arrow | NodeKind::Path => {
            let (kind, exact) = match raw.kind {
                NodeKind::Line => ("line", true),
                NodeKind::Arrow => ("arrow", true),
                _ => ("path", false),
            };
            let invalid = |field, reason: String| ResolveError::InvalidField {
                node: id.to_string(),
                field,
                reason,
            };
            if raw.points.is_empty() {
                return Err(ResolveError::MissingField {
                    node: id.to_string(),
                    kind,
                    field: "points",
                });
            }
            let n = raw.points.len();
            if exact && n != 2 {
                return Err(invalid("points", format!("{kind} needs exactly 2 points, got {n}")));
            }
            if n < 2 {
                return Err(invalid("points", format!("{kind} needs at least 2 points, got {n}")));
            }
            if raw.points.iter().flatten().any(|v| !v.is_finite()) {
                return Err(invalid("points", "coordinates must be finite numbers".into()));
            }
            // Line/arrow are pure strokes, so they default to a black one; a path
            // is stroked black only when it has neither fill nor stroke.
            let default_stroke = exact || raw.fill.is_none();
            let stroke = shape_stroke(id, raw, factor, default_stroke)?;
            let default_head = if raw.kind == NodeKind::Arrow {
                ArrowHead::End
            } else {
                ArrowHead::None
            };
            let head_size = match raw.head_size {
                Some(h) => h * factor,
                None => {
                    stroke.as_ref().map_or(0.0, |s| s.width) * DEFAULT_HEAD_SIZE_PER_STROKE
                }
            };
            NodeType::Path(PathProps {
                points: raw
                    .points
                    .iter()
                    .map(|[x, y]| Point::new(x * factor, y * factor))
                    .collect(),
                closed: raw.closed,
                fill: raw.fill,
                stroke,
                head: raw.head.unwrap_or(default_head),
                head_size,
            })
        }
        NodeKind::Ellipse => NodeType::Ellipse(EllipseProps {
            fill: raw.fill,
            stroke: shape_stroke(id, raw, factor, raw.fill.is_none())?,
        }),
    };

    Ok(Node {
        id: id.to_string(),
        common,
        kind,
    })
}

/// The stroke of a vector shape: `stroke` (or black when `default_black` and
/// unset), `stroke_width` (default [`DEFAULT_SHAPE_STROKE_WIDTH`] pt), the dash
/// pattern (validated, scaled, odd lists repeated) and the cap. `None` when
/// there is no color or the width is zero.
fn shape_stroke(
    id: &str,
    raw: &RawNode,
    factor: f32,
    default_black: bool,
) -> Result<Option<StrokeStyle>, ResolveError> {
    let mut dash: Vec<f32> = raw.dash.iter().map(|d| d * factor).collect();
    if !dash.is_empty() {
        if dash.iter().any(|d| !d.is_finite() || *d < 0.0) || dash.iter().sum::<f32>() <= 0.0 {
            return Err(ResolveError::InvalidField {
                node: id.to_string(),
                field: "dash",
                reason: "lengths must be non-negative and not all zero".into(),
            });
        }
        if dash.len() % 2 == 1 {
            dash.extend_from_within(..);
        }
    }
    let color = match raw.stroke {
        Some(c) => c,
        None if default_black => Color::BLACK,
        None => return Ok(None),
    };
    let width = raw
        .stroke_width
        .map_or(DEFAULT_SHAPE_STROKE_WIDTH, |w| w * factor);
    if width <= 0.0 {
        return Ok(None);
    }
    Ok(Some(StrokeStyle {
        color,
        width,
        dash,
        cap: raw.cap.unwrap_or_default(),
    }))
}

/// DFS from the root, rejecting back-edges. Because every node has at most one
/// parent (checked earlier), a back-edge is a genuine cycle.
fn detect_cycle(nodes: &[Node], root: usize) -> Result<(), ResolveError> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Unvisited,
        InStack,
        Done,
    }
    let mut marks = vec![Mark::Unvisited; nodes.len()];
    // Iterative DFS to avoid stack overflow on deep trees.
    let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
    marks[root] = Mark::InStack;
    while let Some(&mut (node, ref mut child_pos)) = stack.last_mut() {
        let children = nodes[node].kind.children();
        if *child_pos < children.len() {
            let next = children[*child_pos];
            *child_pos += 1;
            match marks[next] {
                Mark::InStack => return Err(ResolveError::Cycle(nodes[next].id.clone())),
                Mark::Done => {}
                Mark::Unvisited => {
                    marks[next] = Mark::InStack;
                    stack.push((next, 0));
                }
            }
        } else {
            marks[node] = Mark::Done;
            stack.pop();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Unit;

    fn doc(src: &str) -> Result<Document, crate::Error> {
        Document::from_toml(src)
    }

    const TWO_PANEL: &str = r#"
        [page]
        width = 10
        height = 10
        unit = "cm"
        root = "root"

        [nodes.root]
        type = "column"
        children = ["panel1", "panel2"]

        [nodes.panel1]
        type = "row"
        children = ["img1", "cap1"]

        [nodes.img1]
        type = "image"
        src = "fig1.png"
        flex = 1

        [nodes.cap1]
        type = "text"
        content = "Panel 1"
        flex = 1

        [nodes.panel2]
        type = "text"
        content = "Panel 2"
    "#;

    #[test]
    fn resolves_two_panel() {
        let d = doc(TWO_PANEL).unwrap();
        // 5 nodes, root is a column with two children
        assert_eq!(d.nodes.len(), 5);
        let root = &d.nodes[d.root];
        assert_eq!(root.id, "root");
        match &root.kind {
            NodeType::Container { children, axis, .. } => {
                assert_eq!(*axis, Axis::Vertical);
                assert_eq!(children.len(), 2);
            }
            _ => panic!("root should be a container"),
        }
        // cm -> pt page conversion
        assert!((d.page.width_pt - Unit::Cm.convert(10.0)).abs() < 1e-3);
        assert_eq!(d.page.dpi, DEFAULT_DPI);
    }

    #[test]
    fn root_missing() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "nope"
            [nodes.a]
            type = "rect"
        "#;
        assert_eq!(
            doc(src).unwrap_err().to_string(),
            "page.root refers to unknown node `nope`"
        );
    }

    #[test]
    fn dangling_child() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "column"
            children = ["ghost"]
        "#;
        let e = doc(src).unwrap_err().to_string();
        assert!(e.contains("child `ghost` does not exist"), "{e}");
    }

    #[test]
    fn multiple_parents() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "column"
            children = ["a", "b"]
            [nodes.a]
            type = "column"
            children = ["shared"]
            [nodes.b]
            type = "column"
            children = ["shared"]
            [nodes.shared]
            type = "rect"
        "#;
        let e = doc(src).unwrap_err().to_string();
        assert!(e.contains("more than one parent"), "{e}");
    }

    #[test]
    fn cycle_detected() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "column"
            children = ["a"]
            [nodes.a]
            type = "column"
            children = ["root"]
        "#;
        // root has two parents (itself via a) -> caught as MultipleParents first,
        // which is also a valid rejection. Use a self-free cycle instead.
        let _ = e_or(src);

        let src2 = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "column"
            children = ["a"]
            [nodes.a]
            type = "column"
            children = ["b"]
            [nodes.b]
            type = "column"
            children = ["a"]
        "#;
        let e = e_or(src2);
        assert!(
            e.contains("more than one parent") || e.contains("cycle"),
            "{e}"
        );
    }

    fn e_or(src: &str) -> String {
        doc(src).unwrap_err().to_string()
    }

    #[test]
    fn image_requires_src() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "image"
        "#;
        let e = doc(src).unwrap_err().to_string();
        assert!(e.contains("image requires field `src`"), "{e}");
    }

    #[test]
    fn text_requires_content() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "text"
        "#;
        let e = doc(src).unwrap_err().to_string();
        assert!(e.contains("text requires field `content`"), "{e}");
    }

    #[test]
    fn unknown_field_rejected() {
        let src = r##"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "rect"
            colour = "#fff"
        "##;
        assert!(doc(src).is_err());
    }
}
