//! The figs extension for Zed: starts `figs-lsp` for `.figs` files.
//!
//! The executable is `lsp.figs-lsp.binary.path` from the settings when set,
//! else `figs-lsp` on `PATH` (`cargo install --path crates/figs-lsp`).

use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

struct FigsExtension;

impl zed::Extension for FigsExtension {
    fn new() -> Self {
        FigsExtension
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.binary);
        let args = binary
            .as_ref()
            .and_then(|b| b.arguments.clone())
            .unwrap_or_default();
        let command = binary
            .and_then(|b| b.path)
            .or_else(|| worktree.which("figs-lsp"))
            .ok_or_else(|| {
                "figs-lsp was not found on PATH: install it with \
                 `cargo install --path crates/figs-lsp`, or set lsp.figs-lsp.binary.path"
                    .to_string()
            })?;
        Ok(zed::Command {
            command,
            args,
            env: Default::default(),
        })
    }
}

zed::register_extension!(FigsExtension);
