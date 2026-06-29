//! Bidirectional-sync editing core for the flat id-keyed schema.
//!
//! Wraps a [`toml_edit::DocumentMut`] so the WYSIWYG editor can mutate the
//! document structurally — set properties, reorder/reparent children, add and
//! remove nodes — and serialize back with **minimal diffs**, preserving the
//! user's comments, key order and formatting on untouched nodes.
//!
//! Editing the flat `[nodes.<id>]` map (rather than a nested tree) is what makes
//! this tractable: a reorder is one array rewrite, a reparent touches exactly
//! two `children` arrays, and every node keeps a stable id across edits.

use toml_edit::{value, Array, DocumentMut, Item, Table, Value};

/// Errors from structural edits.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EditError {
    #[error("invalid TOML: {0}")]
    Parse(String),
    #[error("`nodes` table is missing")]
    NodesMissing,
    #[error("node `{0}` not found")]
    NodeNotFound(String),
    #[error("node `{0}` already exists")]
    NodeExists(String),
    #[error("`{0}` is not a child of `{1}`")]
    NotAChild(String, String),
}

/// An editable document over the flat id-keyed schema.
pub struct EditableDocument {
    doc: DocumentMut,
}

impl EditableDocument {
    /// Parse TOML source into an editable document, preserving formatting.
    pub fn parse(src: &str) -> Result<Self, EditError> {
        let doc = src
            .parse::<DocumentMut>()
            .map_err(|e| EditError::Parse(e.to_string()))?;
        Ok(EditableDocument { doc })
    }

    /// Serialize back to TOML (minimal diff vs the parsed input).
    pub fn to_toml_string(&self) -> String {
        self.doc.to_string()
    }

    /// Resolve the current state into a laid-out-ready [`crate::Document`].
    pub fn document(&self) -> Result<crate::Document, crate::Error> {
        crate::Document::from_toml(&self.to_toml_string())
    }

    // ---- property edits ----

    /// Set a string property on a node.
    pub fn set_string(&mut self, id: &str, key: &str, v: &str) -> Result<(), EditError> {
        set_preserving_decor(self.node_mut(id)?, key, Value::from(v));
        Ok(())
    }

    /// Set a floating-point property on a node.
    pub fn set_f64(&mut self, id: &str, key: &str, v: f64) -> Result<(), EditError> {
        set_preserving_decor(self.node_mut(id)?, key, Value::from(v));
        Ok(())
    }

    /// Set an integer property on a node.
    pub fn set_i64(&mut self, id: &str, key: &str, v: i64) -> Result<(), EditError> {
        set_preserving_decor(self.node_mut(id)?, key, Value::from(v));
        Ok(())
    }

    /// Set a boolean property on a node.
    pub fn set_bool(&mut self, id: &str, key: &str, v: bool) -> Result<(), EditError> {
        set_preserving_decor(self.node_mut(id)?, key, Value::from(v));
        Ok(())
    }

    /// Remove a property from a node (no-op if absent).
    pub fn remove_property(&mut self, id: &str, key: &str) -> Result<(), EditError> {
        self.node_mut(id)?.remove(key);
        Ok(())
    }

    // ---- reads (raw authored values, in the page unit) ----

    /// All node ids, in document order.
    pub fn node_ids(&self) -> Vec<String> {
        self.doc
            .get("nodes")
            .and_then(Item::as_table)
            .map(|t| t.iter().map(|(k, _)| k.to_string()).collect())
            .unwrap_or_default()
    }

    /// The parent id of `id`, if any.
    pub fn parent_of(&self, id: &str) -> Option<String> {
        let nodes = self.doc.get("nodes").and_then(Item::as_table)?;
        nodes.iter().find_map(|(parent, item)| {
            read_children(item)
                .iter()
                .any(|c| c == id)
                .then(|| parent.to_string())
        })
    }

    /// Read a numeric property (float or integer) in its authored units.
    pub fn get_f64(&self, id: &str, key: &str) -> Option<f64> {
        let v = self.node(id)?.get(key)?.as_value()?;
        v.as_float().or_else(|| v.as_integer().map(|i| i as f64))
    }

    /// Read a string property.
    pub fn get_string(&self, id: &str, key: &str) -> Option<String> {
        self.node(id)?.get(key)?.as_str().map(String::from)
    }

    /// Read a boolean property.
    pub fn get_bool(&self, id: &str, key: &str) -> Option<bool> {
        self.node(id)?.get(key)?.as_bool()
    }

    fn node(&self, id: &str) -> Option<&Table> {
        self.doc
            .get("nodes")
            .and_then(Item::as_table)?
            .get(id)
            .and_then(Item::as_table)
    }

    // ---- structural edits ----

    /// The ordered children ids of a container node (empty if none).
    pub fn children(&self, id: &str) -> Result<Vec<String>, EditError> {
        let node = self
            .doc
            .get("nodes")
            .and_then(Item::as_table)
            .ok_or(EditError::NodesMissing)?
            .get(id)
            .ok_or_else(|| EditError::NodeNotFound(id.to_string()))?;
        Ok(read_children(node))
    }

    /// Replace a container's child order (e.g. after a drag reorder).
    pub fn set_children(&mut self, id: &str, ids: &[String]) -> Result<(), EditError> {
        let arr: Array = ids.iter().map(|s| s.as_str()).collect();
        set_preserving_decor(self.node_mut(id)?, "children", Value::Array(arr));
        Ok(())
    }

    /// Move `child` out of `from_parent` and into `to_parent` at `index`
    /// (clamped). Reparenting touches exactly the two children arrays.
    pub fn reparent(
        &mut self,
        child: &str,
        from_parent: &str,
        to_parent: &str,
        index: usize,
    ) -> Result<(), EditError> {
        let mut from = self.children(from_parent)?;
        let pos = from
            .iter()
            .position(|c| c == child)
            .ok_or_else(|| EditError::NotAChild(child.to_string(), from_parent.to_string()))?;
        from.remove(pos);

        let mut to = if from_parent == to_parent {
            from.clone()
        } else {
            self.children(to_parent)?
        };
        let index = index.min(to.len());
        to.insert(index, child.to_string());

        if from_parent == to_parent {
            // Single array: `to` already started from the post-removal vector.
            self.set_children(to_parent, &to)?;
        } else {
            self.set_children(from_parent, &from)?;
            self.set_children(to_parent, &to)?;
        }
        Ok(())
    }

    /// Add a new node table `[nodes.<id>] type = <kind>` and link it as a child
    /// of `parent` at `index` (clamped).
    pub fn add_node(
        &mut self,
        id: &str,
        kind: &str,
        parent: &str,
        index: usize,
    ) -> Result<(), EditError> {
        if self.nodes_mut()?.contains_key(id) {
            return Err(EditError::NodeExists(id.to_string()));
        }
        let mut table = Table::new();
        table.insert("type", value(kind));
        self.nodes_mut()?.insert(id, Item::Table(table));

        let mut children = self.children(parent)?;
        let index = index.min(children.len());
        children.insert(index, id.to_string());
        self.set_children(parent, &children)
    }

    /// Remove a node and its whole subtree, and unlink it from any parent.
    pub fn remove_node(&mut self, id: &str) -> Result<(), EditError> {
        if !self.nodes_mut()?.contains_key(id) {
            return Err(EditError::NodeNotFound(id.to_string()));
        }
        // Collect the subtree rooted at `id` via children arrays.
        let mut to_remove = Vec::new();
        let mut stack = vec![id.to_string()];
        while let Some(cur) = stack.pop() {
            if let Ok(kids) = self.children(&cur) {
                stack.extend(kids);
            }
            to_remove.push(cur);
        }

        for dead in &to_remove {
            self.nodes_mut()?.remove(dead);
        }
        // Unlink any reference to the removed root from remaining children arrays.
        let remaining: Vec<String> = self
            .nodes_mut()?
            .iter()
            .map(|(k, _)| k.to_string())
            .collect();
        for node_id in remaining {
            let kids = self.children(&node_id).unwrap_or_default();
            if kids.iter().any(|c| c == id) {
                let filtered: Vec<String> = kids.into_iter().filter(|c| c != id).collect();
                self.set_children(&node_id, &filtered)?;
            }
        }
        Ok(())
    }

    // ---- helpers ----

    fn nodes_mut(&mut self) -> Result<&mut Table, EditError> {
        self.doc
            .get_mut("nodes")
            .and_then(Item::as_table_mut)
            .ok_or(EditError::NodesMissing)
    }

    fn node_mut(&mut self, id: &str) -> Result<&mut Table, EditError> {
        self.nodes_mut()?
            .get_mut(id)
            .and_then(Item::as_table_mut)
            .ok_or_else(|| EditError::NodeNotFound(id.to_string()))
    }
}

/// Set `key = v` on a table, preserving the existing value's decor (whitespace
/// and inline comments) when the key already exists, so untouched formatting and
/// comments survive a round-trip.
fn set_preserving_decor(table: &mut Table, key: &str, v: Value) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(existing) => {
            let decor = existing.decor().clone();
            *existing = v;
            *existing.decor_mut() = decor;
        }
        None => {
            table.insert(key, Item::Value(v));
        }
    }
}

/// Read a node's `children` array as a vector of ids.
fn read_children(node: &Item) -> Vec<String> {
    node.get("children")
        .and_then(Item::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r##"
# a poster
[page]
width = 10
height = 10
unit = "cm"
root = "root"

[nodes.root]
type = "column"
spacing = 2      # gap between panels
children = ["a", "b"]

[nodes.a]
type = "rect"
fill = "#ff0000"

[nodes.b]
type = "rect"
"##;

    fn doc() -> EditableDocument {
        EditableDocument::parse(SRC).unwrap()
    }

    #[test]
    fn preserves_comments_on_untouched_edit() {
        let mut d = doc();
        d.set_f64("root", "spacing", 4.0).unwrap();
        let out = d.to_toml_string();
        assert!(out.contains("# a poster"), "lost top comment:\n{out}");
        assert!(out.contains("# gap between panels"), "lost inline comment:\n{out}");
        assert!(out.contains("spacing = 4.0"), "value not updated:\n{out}");
        // still resolves
        d.document().unwrap();
    }

    #[test]
    fn reorder_children_is_one_line() {
        let mut d = doc();
        d.set_children("root", &["b".into(), "a".into()]).unwrap();
        assert_eq!(d.children("root").unwrap(), vec!["b", "a"]);
        let out = d.to_toml_string();
        assert!(out.contains(r#"children = ["b", "a"]"#), "{out}");
    }

    #[test]
    fn reparent_moves_between_arrays() {
        // root -> [a, b]; nest a new container `g`, move `a` under it.
        let mut d = doc();
        d.add_node("g", "column", "root", 0).unwrap();
        // root children now [g, a, b]
        assert_eq!(d.children("root").unwrap(), vec!["g", "a", "b"]);
        d.reparent("a", "root", "g", 0).unwrap();
        assert_eq!(d.children("root").unwrap(), vec!["g", "b"]);
        assert_eq!(d.children("g").unwrap(), vec!["a"]);
        // resolves to a valid tree
        d.document().unwrap();
    }

    #[test]
    fn reparent_within_same_parent_reorders() {
        let mut d = doc();
        d.reparent("b", "root", "root", 0).unwrap();
        assert_eq!(d.children("root").unwrap(), vec!["b", "a"]);
    }

    #[test]
    fn add_and_remove_node() {
        let mut d = doc();
        d.add_node("c", "text", "root", 1).unwrap();
        assert_eq!(d.children("root").unwrap(), vec!["a", "c", "b"]);
        assert!(d.to_toml_string().contains("[nodes.c]"));

        d.remove_node("c").unwrap();
        assert_eq!(d.children("root").unwrap(), vec!["a", "b"]);
        assert!(!d.to_toml_string().contains("[nodes.c]"));
        d.document().unwrap();
    }

    #[test]
    fn remove_node_drops_subtree() {
        let mut d = doc();
        d.add_node("g", "column", "root", 0).unwrap();
        d.reparent("a", "root", "g", 0).unwrap();
        // removing g should also drop its child a
        d.remove_node("g").unwrap();
        let out = d.to_toml_string();
        assert!(!out.contains("[nodes.g]"), "{out}");
        assert!(!out.contains("[nodes.a]"), "subtree not removed:\n{out}");
        assert_eq!(d.children("root").unwrap(), vec!["b"]);
    }

    #[test]
    fn reads_raw_authored_values() {
        let d = doc();
        assert_eq!(d.get_string("root", "type").as_deref(), Some("column"));
        assert_eq!(d.get_f64("root", "spacing"), Some(2.0));
        assert_eq!(d.get_string("a", "fill").as_deref(), Some("#ff0000"));
        assert_eq!(d.parent_of("a").as_deref(), Some("root"));
        assert_eq!(d.parent_of("root"), None);
        let mut ids = d.node_ids();
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "root"]);
    }

    #[test]
    fn errors_are_precise() {
        let mut d = doc();
        assert_eq!(
            d.set_string("ghost", "type", "rect").unwrap_err(),
            EditError::NodeNotFound("ghost".into())
        );
        assert_eq!(
            d.add_node("a", "rect", "root", 0).unwrap_err(),
            EditError::NodeExists("a".into())
        );
        assert_eq!(
            d.reparent("b", "a", "root", 0).unwrap_err(),
            EditError::NotAChild("b".into(), "a".into())
        );
    }
}
