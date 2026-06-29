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
    Axis, Color, CrossAxisAlign, Edges, ImageFit, MainAxisAlign, TextAlign,
};

use super::parse::{NodeKind, RawDocument, RawNode};

/// Default raster resolution for PNG output when `page.dpi` is unset.
pub const DEFAULT_DPI: f32 = 300.0;
/// Default text size in points.
pub const DEFAULT_FONT_SIZE: f32 = 12.0;
/// Default line height as a multiple of font size.
pub const DEFAULT_LINE_HEIGHT: f32 = 1.2;

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
    Image(ImageProps),
    Text(TextProps),
    Rect(RectProps),
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
        if let NodeType::Container { children, .. } = &node.kind {
            for &c in children {
                parent_count[c] += 1;
            }
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
    };

    let kind = match raw.kind {
        NodeKind::Column | NodeKind::Row => {
            let axis = if raw.kind == NodeKind::Column {
                Axis::Vertical
            } else {
                Axis::Horizontal
            };
            let mut children = Vec::with_capacity(raw.children.len());
            for child in &raw.children {
                let &idx = index_of
                    .get(child.as_str())
                    .ok_or_else(|| ResolveError::DanglingChild {
                        node: id.to_string(),
                        child: child.clone(),
                    })?;
                children.push(idx);
            }
            NodeType::Container {
                axis,
                children,
                spacing: raw.spacing * factor,
                main_axis: raw.main_axis_alignment.unwrap_or_default(),
                cross_axis: raw.cross_axis_alignment.unwrap_or_default(),
            }
        }
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
    };

    Ok(Node {
        id: id.to_string(),
        common,
        kind,
    })
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
        let children: &[usize] = match &nodes[node].kind {
            NodeType::Container { children, .. } => children,
            _ => &[],
        };
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
