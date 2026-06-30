//! Operations over the in-memory [`RawDocument`] the editor mutates: building a
//! starter document, creating nodes, tree edits (add/delete/move/indent), and
//! serializing back to deterministic TOML.

use std::collections::BTreeMap;

use figs_core::geom::{Color, Edges};
use figs_core::schema::{NodeKind, RawDocument, RawNode, RawPage};
use figs_core::units::Unit;
use serde::Serialize;

/// A fresh node with everything cleared; callers layer kind-specific defaults.
fn blank(kind: NodeKind) -> RawNode {
    RawNode {
        kind,
        flex: None,
        width: None,
        height: None,
        aspect_ratio: None,
        margin: Edges::default(),
        padding: Edges::default(),
        children: Vec::new(),
        spacing: 0.0,
        main_axis_alignment: None,
        cross_axis_alignment: None,
        content: None,
        font_size: None,
        font_family: None,
        font_weight: None,
        color: None,
        align: None,
        line_height: None,
        src: None,
        fit: None,
        fill: None,
        stroke: None,
        stroke_width: None,
        corner_radius: None,
    }
}

/// A new node of `kind` with sensible per-kind defaults for editing.
pub fn new_node(kind: NodeKind) -> RawNode {
    let mut n = blank(kind);
    match kind {
        NodeKind::Text => {
            n.content = Some("Text".to_string());
            n.font_size = Some(16.0);
        }
        NodeKind::Rect => {
            n.fill = Some(Color::parse_hex("#6ea8fe").unwrap());
            n.corner_radius = Some(8.0);
        }
        NodeKind::Image => {
            n.src = Some("image.png".to_string());
            n.fit = Some(figs_core::geom::ImageFit::Contain);
        }
        NodeKind::Column | NodeKind::Row => {
            n.spacing = 12.0;
        }
    }
    n
}

/// A minimal starter document used by `New` and on first launch.
pub fn default_document() -> RawDocument {
    let mut nodes = std::collections::HashMap::new();

    let mut root = new_node(NodeKind::Column);
    root.children = vec!["title".into(), "subtitle".into()];
    // No default outer padding: content reaches the page border (edge-to-edge).
    // Add Margin/Padding in the inspector when insets are wanted.
    root.spacing = 16.0;
    root.cross_axis_alignment = Some(figs_core::geom::CrossAxisAlign::Stretch);

    let mut title = new_node(NodeKind::Text);
    title.content = Some("New figure".to_string());
    title.font_size = Some(40.0);
    title.font_weight = Some(700);
    title.align = Some(figs_core::geom::TextAlign::Center);

    let mut subtitle = new_node(NodeKind::Text);
    subtitle.content = Some("Edit me in the inspector".to_string());
    subtitle.font_size = Some(18.0);
    subtitle.color = Some(Color::parse_hex("#6b7280").unwrap());
    subtitle.align = Some(figs_core::geom::TextAlign::Center);

    nodes.insert("root".to_string(), root);
    nodes.insert("title".to_string(), title);
    nodes.insert("subtitle".to_string(), subtitle);

    RawDocument {
        page: RawPage {
            width: 720.0,
            height: 960.0,
            unit: Unit::Pt,
            root: "root".to_string(),
            dpi: Some(96.0),
            background: Some(Color::parse_hex("#faf7f0").unwrap()),
            font_family: None,
        },
        nodes,
    }
}

/// The starter Rhai script used by `New` and on first launch. Mirrors
/// [`default_document`], but expressed as an editable `.figs` script.
pub fn default_script() -> String {
    r##"// figs script — edit, then press Evaluate (Ctrl+Enter).
// `let` defines variables, `fn` defines reusable components, arithmetic just works.
let muted = "#6b7280";

page(720, 960,
    col([
        text("New figure").size(40).bold().center(),
        text("Edit me, then Evaluate").size(18).color(muted).center(),
    ]).spacing(16).cross("stretch"),
).unit("pt").dpi(96).background("#faf7f0")
"##
    .to_string()
}

/// True when a node distributes children (column or row).
pub fn is_container(doc: &RawDocument, id: &str) -> bool {
    matches!(
        doc.nodes.get(id).map(|n| n.kind),
        Some(NodeKind::Column | NodeKind::Row)
    )
}

/// Find the id of the node whose `children` contains `id`, if any.
pub fn parent_of(doc: &RawDocument, id: &str) -> Option<String> {
    doc.nodes
        .iter()
        .find(|(_, n)| n.children.iter().any(|c| c == id))
        .map(|(k, _)| k.clone())
}

/// Generate an unused `nodeN` id.
pub fn unique_id(doc: &RawDocument) -> String {
    for i in 1.. {
        let id = format!("node{i}");
        if !doc.nodes.contains_key(&id) {
            return id;
        }
    }
    unreachable!()
}

/// Add a new node of `kind` under `target` (or its parent if `target` is a
/// leaf). Returns the new node's id.
pub fn add_child(doc: &mut RawDocument, target: &str, kind: NodeKind) -> String {
    let id = unique_id(doc);
    doc.nodes.insert(id.clone(), new_node(kind));

    if is_container(doc, target) {
        if let Some(n) = doc.nodes.get_mut(target) {
            n.children.push(id.clone());
        }
    } else if let Some(parent) = parent_of(doc, target) {
        // Insert right after the target among its siblings.
        if let Some(p) = doc.nodes.get_mut(&parent) {
            let at = p.children.iter().position(|c| c == target).map_or(p.children.len(), |i| i + 1);
            p.children.insert(at, id.clone());
        }
    } else if let Some(r) = doc.nodes.get_mut(&doc.page.root.clone()) {
        r.children.push(id.clone());
    }
    id
}

/// Remove `id` and its whole subtree; detach from its parent. The root cannot
/// be deleted.
pub fn delete_node(doc: &mut RawDocument, id: &str) {
    if id == doc.page.root {
        return;
    }
    if let Some(parent) = parent_of(doc, id) {
        if let Some(p) = doc.nodes.get_mut(&parent) {
            p.children.retain(|c| c != id);
        }
    }
    // Collect the subtree, then remove all of it.
    let mut to_remove = Vec::new();
    let mut stack = vec![id.to_string()];
    while let Some(cur) = stack.pop() {
        if let Some(n) = doc.nodes.get(&cur) {
            stack.extend(n.children.iter().cloned());
        }
        to_remove.push(cur);
    }
    for r in to_remove {
        doc.nodes.remove(&r);
    }
}

/// Move `id` up (-1) or down (+1) among its siblings.
pub fn move_sibling(doc: &mut RawDocument, id: &str, delta: i32) {
    let Some(parent) = parent_of(doc, id) else {
        return;
    };
    if let Some(p) = doc.nodes.get_mut(&parent) {
        if let Some(i) = p.children.iter().position(|c| c == id) {
            let j = i as i32 + delta;
            if j >= 0 && (j as usize) < p.children.len() {
                p.children.swap(i, j as usize);
            }
        }
    }
}

/// Reparent `id` to its grandparent, just after its current parent.
pub fn outdent(doc: &mut RawDocument, id: &str) {
    let Some(parent) = parent_of(doc, id) else {
        return;
    };
    let Some(grand) = parent_of(doc, &parent) else {
        return; // parent is root
    };
    if let Some(p) = doc.nodes.get_mut(&parent) {
        p.children.retain(|c| c != id);
    }
    if let Some(g) = doc.nodes.get_mut(&grand) {
        let at = g.children.iter().position(|c| c == &parent).map_or(g.children.len(), |i| i + 1);
        g.children.insert(at, id.to_string());
    }
}

/// Reparent `id` into its preceding sibling (which must be a container).
pub fn indent(doc: &mut RawDocument, id: &str) {
    let Some(parent) = parent_of(doc, id) else {
        return;
    };
    let prev = {
        let p = &doc.nodes[&parent];
        let i = p.children.iter().position(|c| c == id);
        match i {
            Some(i) if i > 0 => Some(p.children[i - 1].clone()),
            _ => None,
        }
    };
    let Some(prev) = prev else { return };
    if !is_container(doc, &prev) {
        return;
    }
    if let Some(p) = doc.nodes.get_mut(&parent) {
        p.children.retain(|c| c != id);
    }
    if let Some(s) = doc.nodes.get_mut(&prev) {
        s.children.push(id.to_string());
    }
}

/// Copy `src_file` into `base_dir` and insert an image node under `target`,
/// referencing the file by its (relative) name and sizing it from the image's
/// pixel dimensions so it's visible (images have no intrinsic layout size).
pub fn insert_image(
    doc: &mut RawDocument,
    target: &str,
    base_dir: &std::path::Path,
    src_file: &std::path::Path,
) -> std::io::Result<String> {
    let file_name = src_file.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "image has no file name")
    })?;
    let dest = base_dir.join(file_name);

    // Skip the copy when the picked file already is the destination.
    let same = matches!(
        (src_file.canonicalize(), dest.canonicalize()),
        (Ok(a), Ok(b)) if a == b
    );
    if !same {
        std::fs::copy(src_file, &dest)?;
    }

    let rel = file_name.to_string_lossy().to_string();
    let id = add_child(doc, target, NodeKind::Image);
    let unit_pt = doc.page.unit.to_pt().max(1e-6);
    let fallback_w = doc.page.width * 0.3;
    if let Some(node) = doc.nodes.get_mut(&id) {
        node.src = Some(rel);
        node.fit = Some(figs_core::geom::ImageFit::Contain);
        match figs_core::image_size(src_file) {
            Some((w, h)) if w > 0 && h > 0 => {
                // px -> pt (CSS 96dpi) -> page unit.
                node.width = Some((w as f32 * 72.0 / 96.0) / unit_pt);
                node.aspect_ratio = Some(w as f32 / h as f32);
            }
            _ => {
                node.width = Some(fallback_w);
                node.aspect_ratio = Some(1.0);
            }
        }
    }
    Ok(id)
}

/// A view over the document that serializes nodes in a stable (sorted) order,
/// so saved files don't churn with `HashMap`'s random iteration order.
#[derive(Serialize)]
struct DocView<'a> {
    page: &'a RawPage,
    nodes: BTreeMap<&'a String, &'a RawNode>,
}

/// Serialize the document to TOML with deterministic node ordering.
pub fn to_toml(doc: &RawDocument) -> Result<String, toml::ser::Error> {
    let view = DocView {
        page: &doc.page,
        nodes: doc.nodes.iter().collect(),
    };
    toml::to_string_pretty(&view)
}
