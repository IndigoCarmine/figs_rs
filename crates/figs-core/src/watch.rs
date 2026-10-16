//! Debounced watching of a document and its [dependencies](crate::deps), for
//! `figs watch` and the editor's live reload (feature `watch`).
//!
//! Editors often save by writing a temp file and renaming it over the original,
//! which replaces the file (and breaks a watch on the file itself). So
//! [`FileWatcher`] watches each file's *parent directory* and filters the
//! directory's events down to the files of interest.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify_debouncer_mini::notify::{self, RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

use crate::deps::absolutize;

/// How long a burst of file events is coalesced before reporting it. Short
/// enough to feel instant, long enough to merge an editor's write+rename.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(120);

/// Comparison key for a path: absolute and normalized, and case-folded on
/// Windows (whose file system is case-insensitive), so a dependency written as
/// `Fig.PNG` still matches the event the OS reports for `fig.png`.
fn key(path: &Path) -> String {
    let abs = absolutize(Path::new(""), path);
    let s = abs.to_string_lossy();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.into_owned()
    }
}

/// Whether `a` and `b` name the same file, by the same normalization the
/// watcher uses to match events (absolute, `.`/`..` folded, case-insensitive on
/// Windows). Symlinks are not resolved.
pub fn same_path(a: &Path, b: &Path) -> bool {
    key(a) == key(b)
}

/// The directories to watch for `files`: each file's parent, deduplicated.
fn parent_dirs<'a>(files: impl IntoIterator<Item = &'a PathBuf>) -> BTreeSet<PathBuf> {
    files
        .into_iter()
        .filter_map(|f| f.parent().map(Path::to_path_buf))
        .collect()
}

/// The subset of `events` (paths reported by the OS) that are among `files`
/// (keys from [`key`]), deduplicated, in first-seen order.
fn matching(events: impl IntoIterator<Item = PathBuf>, files: &HashSet<String>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    events
        .into_iter()
        .filter(|p| {
            let k = key(p);
            files.contains(&k) && seen.insert(k)
        })
        .collect()
}

/// Watches a set of files and calls a handler (on a background thread) with the
/// ones that changed, after debouncing. Update the set with
/// [`FileWatcher::set_files`] whenever the document's dependencies change.
pub struct FileWatcher {
    debouncer: Debouncer<RecommendedWatcher>,
    /// Keys of the files whose changes are reported.
    files: Arc<Mutex<HashSet<String>>>,
    dirs: BTreeSet<PathBuf>,
}

impl FileWatcher {
    /// Start a watcher (initially watching nothing). `on_change` receives the
    /// changed files — always a non-empty subset of the current set.
    pub fn new(
        debounce: Duration,
        mut on_change: impl FnMut(Vec<PathBuf>) + Send + 'static,
    ) -> notify::Result<FileWatcher> {
        let files: Arc<Mutex<HashSet<String>>> = Arc::default();
        let filter = Arc::clone(&files);
        let debouncer = new_debouncer(debounce, move |res: DebounceEventResult| match res {
            Ok(events) => {
                let changed = {
                    let files = filter.lock().unwrap_or_else(|e| e.into_inner());
                    matching(events.into_iter().map(|e| e.path), &files)
                };
                if !changed.is_empty() {
                    on_change(changed);
                }
            }
            Err(e) => tracing::warn!("file watcher error: {e}"),
        })?;
        Ok(FileWatcher {
            debouncer,
            files,
            dirs: BTreeSet::new(),
        })
    }

    /// Replace the watched set with `files` (made absolute against the working
    /// directory). Parent directories that don't exist are skipped: a file
    /// there is not noticed until a later `set_files` after it appears.
    pub fn set_files(&mut self, files: impl IntoIterator<Item = PathBuf>) {
        let files: Vec<PathBuf> = files
            .into_iter()
            .map(|f| absolutize(Path::new(""), &f))
            .collect();
        let dirs = parent_dirs(&files);
        for gone in self.dirs.difference(&dirs) {
            let _ = self.debouncer.watcher().unwatch(gone);
        }
        let mut watched = BTreeSet::new();
        for dir in &dirs {
            if self.dirs.contains(dir) {
                watched.insert(dir.clone());
                continue;
            }
            match self
                .debouncer
                .watcher()
                .watch(dir, RecursiveMode::NonRecursive)
            {
                Ok(()) => {
                    watched.insert(dir.clone());
                }
                Err(e) => tracing::debug!("not watching `{}`: {e}", dir.display()),
            }
        }
        self.dirs = watched;
        *self.files.lock().unwrap_or_else(|e| e.into_inner()) =
            files.iter().map(|f| key(f)).collect();
    }

    /// The directories currently being watched.
    pub fn watched_dirs(&self) -> impl Iterator<Item = &Path> {
        self.dirs.iter().map(PathBuf::as_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_filtered_to_the_watched_files() {
        let dir = std::env::temp_dir().join("figs-watch-filter");
        let fig = dir.join("fig.figs");
        let img = dir.join("img").join("a.png");
        let files: HashSet<String> = [&fig, &img].iter().map(|p| key(p)).collect();
        let events = vec![
            dir.join("out.png"),     // the build output: ignored
            dir.join("fig.figs~"),   // an editor's temp file: ignored
            fig.clone(),             // the source
            dir.join("img/./a.png"), // an image, un-normalized
            fig.clone(),             // duplicate
        ];
        assert_eq!(matching(events, &files), vec![fig, dir.join("img/./a.png")]);
    }

    #[test]
    fn each_parent_directory_is_watched_once() {
        let d = std::env::temp_dir();
        let files = [
            d.join("a.figs"),
            d.join("b.png"),
            d.join("sub").join("c.png"),
        ];
        let dirs: Vec<PathBuf> = parent_dirs(&files).into_iter().collect();
        assert_eq!(dirs, vec![d.clone(), d.join("sub")]);
    }

    #[cfg(windows)]
    #[test]
    fn matching_is_case_insensitive_on_windows() {
        assert!(same_path(
            Path::new(r"C:\X\Fig.PNG"),
            Path::new(r"c:\x\fig.png")
        ));
    }

    #[test]
    fn missing_directories_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let mut w = FileWatcher::new(DEFAULT_DEBOUNCE, |_| {}).unwrap();
        w.set_files([
            tmp.path().join("a.figs"),
            tmp.path().join("nope").join("b.png"),
        ]);
        let dirs: Vec<&Path> = w.watched_dirs().collect();
        assert_eq!(dirs.len(), 1);
        // Shrinking the set unwatches the directory.
        w.set_files([]);
        assert_eq!(w.watched_dirs().count(), 0);
    }

    /// End-to-end through the OS watcher, including a rename-over save.
    #[test]
    #[ignore = "timing-dependent: relies on OS file-event delivery latency"]
    fn reports_writes_and_rename_replacements() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("fig.figs");
        std::fs::write(&file, "a").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut w = FileWatcher::new(Duration::from_millis(50), move |p| {
            let _ = tx.send(p);
        })
        .unwrap();
        w.set_files([file.clone()]);

        std::fs::write(tmp.path().join("other.txt"), "x").unwrap();
        std::fs::write(&file, "b").unwrap();
        let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(key(&got[0]), key(&file));

        let staged = tmp.path().join("fig.figs.tmp");
        std::fs::write(&staged, "c").unwrap();
        std::fs::rename(&staged, &file).unwrap();
        let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(got.iter().any(|p| key(p) == key(&file)), "{got:?}");
    }
}
