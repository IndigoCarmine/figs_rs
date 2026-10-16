//! Files a document depends on, for watch mode and live reload.
//!
//! A document's output depends on more than its own source: image `src` files,
//! and (for scripts) any data a built-in function reads while the script runs.
//! [`Document::dependencies`] lists them all as absolute paths so a front-end can
//! watch them and rebuild when one changes.
//!
//! There are two ways a new kind of dependency registers:
//!
//! - **A node field that names a file** (like an image's `src`): add a match arm
//!   in [`Document::dependencies`].
//! - **A script built-in that reads a file** (e.g. a `csv("data.csv")` loader):
//!   call [`record`] with the path it read. Evaluation through
//!   [`Document::from_script`] (or [`capture`] around [`crate::eval_script`])
//!   collects every recorded path into [`Document::data_files`]; that is a
//!   one-line addition in the built-in.

use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};

use crate::schema::{Document, NodeType};

thread_local! {
    /// One frame per active [`capture`] (innermost last). Script evaluation runs
    /// on the calling thread, so built-ins record into the caller's frame.
    static FRAMES: RefCell<Vec<Vec<PathBuf>>> = const { RefCell::new(Vec::new()) };
}

/// Record that the code running inside the current [`capture`] read `path`.
/// Relative paths are resolved later against the document's base directory.
/// Outside any capture this is a no-op, so built-ins can call it
/// unconditionally.
pub fn record(path: impl Into<PathBuf>) {
    let path = path.into();
    FRAMES.with(|f| {
        if let Some(top) = f.borrow_mut().last_mut() {
            top.push(path);
        }
    });
}

/// Run `f`, returning its result together with every path [`record`]ed while
/// it ran (in order, possibly with duplicates). Captures nest: an inner capture
/// sees only its own recordings.
pub fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<PathBuf>) {
    /// Pops the frame even if `f` panics, so later captures stay balanced.
    struct Frame;
    impl Drop for Frame {
        fn drop(&mut self) {
            FRAMES.with(|f| {
                f.borrow_mut().pop();
            });
        }
    }
    FRAMES.with(|f| f.borrow_mut().push(Vec::new()));
    let guard = Frame;
    let out = f();
    let recorded = FRAMES.with(|f| f.borrow_mut().last_mut().map(std::mem::take));
    drop(guard);
    (out, recorded.unwrap_or_default())
}

/// `path` as an absolute, lexically normalized path: relative paths are joined
/// onto `base_dir` (itself made absolute against the working directory), and
/// `.`/`..` components are folded away. Symlinks are not resolved, so the
/// result matches the paths a file watcher reports for the same file.
pub fn absolutize(base_dir: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };
    let abs = std::path::absolute(&joined).unwrap_or(joined);
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never pop past the root/prefix.
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

impl Document {
    /// Every file this document reads besides its own source — image `src`s and
    /// recorded [`Document::data_files`] — as absolute paths (relative ones are
    /// resolved against `base_dir`, the document's directory), sorted and
    /// deduplicated. Missing files are included: they may appear later.
    pub fn dependencies(&self, base_dir: &Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeType::Image(img) => Some(Path::new(&img.src)),
                _ => None,
            })
            .chain(self.data_files.iter().map(PathBuf::as_path))
            .map(|p| absolutize(base_dir, p))
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> PathBuf {
        std::env::temp_dir().join("figs-deps-test")
    }

    #[test]
    fn images_are_collected_absolute_sorted_and_deduped() {
        let doc = Document::from_toml(
            r#"
            [page]
            width = 10
            height = 10
            root = "root"
            [nodes.root]
            type = "column"
            children = ["b", "a", "a2", "t"]
            [nodes.a]
            type = "image"
            src = "img/a.png"
            [nodes.a2]
            type = "image"
            src = "./img/../img/a.png"
            [nodes.b]
            type = "image"
            src = "b.jpg"
            [nodes.t]
            type = "text"
            content = "no deps"
            "#,
        )
        .unwrap();
        let deps = doc.dependencies(&base());
        assert_eq!(
            deps,
            vec![base().join("b.jpg"), base().join("img").join("a.png")]
        );
        assert!(deps.iter().all(|p| p.is_absolute()));
    }

    #[test]
    fn absolute_srcs_ignore_the_base_dir() {
        let abs = base().join("elsewhere").join("x.png");
        let toml = format!(
            "[page]\nwidth = 1\nheight = 1\nroot = \"i\"\n[nodes.i]\ntype = \"image\"\nsrc = {:?}\n",
            abs.to_string_lossy()
        );
        let doc = Document::from_toml(&toml).unwrap();
        assert_eq!(doc.dependencies(Path::new("unrelated")), vec![abs]);
    }

    #[test]
    fn relative_base_dir_is_made_absolute() {
        let p = absolutize(Path::new("some/dir"), Path::new("../a.png"));
        assert!(p.is_absolute());
        assert!(p.ends_with(Path::new("some").join("a.png")), "{p:?}");
    }

    #[test]
    fn recorded_data_files_join_the_dependencies() {
        let ((), recorded) = capture(|| {
            record("data.csv");
            // Nested captures keep their own recordings.
            let ((), inner) = capture(|| record("inner.csv"));
            assert_eq!(inner, vec![PathBuf::from("inner.csv")]);
            record("data.csv");
        });
        assert_eq!(recorded, vec![PathBuf::from("data.csv"); 2]);
        // Outside a capture, recording is a harmless no-op.
        record("ignored.csv");

        let mut doc = Document::from_toml(
            "[page]\nwidth = 1\nheight = 1\nroot = \"r\"\n[nodes.r]\ntype = \"rect\"\n",
        )
        .unwrap();
        assert!(doc.dependencies(&base()).is_empty());
        doc.data_files = recorded;
        assert_eq!(doc.dependencies(&base()), vec![base().join("data.csv")]);
    }

    #[cfg(feature = "script")]
    #[test]
    fn script_documents_list_their_images() {
        let doc = Document::from_script(r#"page(10, 10, image("fig.png"))"#).unwrap();
        assert_eq!(doc.dependencies(&base()), vec![base().join("fig.png")]);
    }
}
