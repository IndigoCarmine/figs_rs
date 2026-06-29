//! `figs watch`: re-render on every change to the input document.
//!
//! The font store is built once and reused across reloads; the image cache only
//! re-decodes files whose mtime changed, so reloads stay fast. A build error
//! (e.g. a half-saved TOML) is logged and the previous output is left intact.

use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};

use crate::pipeline;

/// Watch `input` and re-render to `output` until interrupted.
///
/// We watch the input's directory (editors save via atomic rename, so watching
/// the file directly misses updates) and rebuild only when the input's mtime
/// actually changes. Keying on the input mtime — rather than the event path —
/// avoids a feedback loop when `output` is written into the same directory.
pub fn run(input: &Path, output: &Path) -> Result<()> {
    let assets = pipeline::assets_for(input);
    let dir = watch_dir(input);

    build_and_report(input, output, &assets);
    let mut last_mtime = mtime(input);

    let (tx, rx) = channel::<DebounceEventResult>();
    let mut debouncer =
        new_debouncer(Duration::from_millis(150), tx).context("initializing file watcher")?;
    debouncer
        .watcher()
        .watch(&dir, RecursiveMode::NonRecursive)
        .with_context(|| format!("watching `{}`", dir.display()))?;
    eprintln!(
        "watching {} -> {} (press Ctrl-C to stop)",
        input.display(),
        output.display()
    );

    for res in rx {
        match res {
            Ok(_) => {
                let now = mtime(input);
                if now != last_mtime {
                    last_mtime = now;
                    build_and_report(input, output, &assets);
                }
            }
            Err(e) => tracing::warn!(error = %e, "file watch error"),
        }
    }
    Ok(())
}

fn watch_dir(input: &Path) -> PathBuf {
    input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn build_and_report(input: &Path, output: &Path, assets: &figs_core::Assets) {
    match pipeline::render_once(input, output, assets) {
        Ok(n) => eprintln!("rebuilt {} ({} bytes)", output.display(), n),
        Err(e) => tracing::error!("build failed (keeping previous output): {e:#}"),
    }
}
