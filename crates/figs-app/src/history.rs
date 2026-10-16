//! A minimal undo/redo history for canvas edits (drag / nudge). Each entry is
//! a snapshot of the source texts and the model taken *before* the edit; the
//! text panes keep their own (egui) undo for typing, and inspector edits are
//! not recorded.

use figs_core::schema::RawDocument;

use crate::app::FigsApp;

/// How many canvas edits can be undone.
const LIMIT: usize = 100;

/// The editable state a canvas edit changes.
#[derive(Clone)]
pub struct Snapshot {
    pub script: String,
    pub toml: String,
    /// `toml` was the text of `model` (not stale or broken), so it can be
    /// shown as-is on restore (keeping its comments).
    pub toml_in_sync: bool,
    pub model: RawDocument,
}

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl History {
    /// Record the state before a new edit; a new edit clears the redo list.
    pub fn push(&mut self, before: Snapshot) {
        self.undo.push(before);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

impl FigsApp {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            script: self.script_buffer.clone(),
            toml: self.toml_buffer.clone(),
            toml_in_sync: self.toml_in_sync(),
            model: self.model.clone(),
        }
    }

    /// Undo the last canvas edit (Ctrl+Z outside the text panes).
    pub fn undo(&mut self) {
        if let Some(prev) = self.history.undo.pop() {
            let now = self.snapshot();
            self.history.redo.push(now);
            self.restore(prev);
        }
    }

    /// Redo an undone canvas edit (Ctrl+Shift+Z / Ctrl+Y).
    pub fn redo(&mut self) {
        if let Some(next) = self.history.redo.pop() {
            let now = self.snapshot();
            self.history.undo.push(now);
            self.restore(next);
        }
    }

    fn restore(&mut self, s: Snapshot) {
        self.drag = None;
        self.nudge_group = None;
        self.script_buffer = s.script;
        self.toml_buffer = s.toml;
        if self.script_is_truth() {
            // Re-evaluate so the model and the call-site map match the text.
            self.evaluate_script();
        } else {
            self.model = s.model;
            self.toml_error = None;
            self.regen_toml = !s.toml_in_sync;
            self.needs_rebuild = true;
        }
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(tag: &str) -> Snapshot {
        Snapshot {
            script: tag.to_string(),
            toml: String::new(),
            toml_in_sync: false,
            model: crate::model::default_document(),
        }
    }

    #[test]
    fn push_clears_redo_and_caps() {
        let mut h = History::default();
        h.push(snap("a"));
        h.redo.push(snap("r"));
        h.push(snap("b"));
        assert!(!h.can_redo());
        for i in 0..(LIMIT + 5) {
            h.push(snap(&i.to_string()));
        }
        assert_eq!(h.undo.len(), LIMIT);
        assert!(h.can_undo());
    }
}
