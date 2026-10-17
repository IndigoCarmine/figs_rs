# figs for VS Code

Language support for figs scripts (`.figs`): syntax highlighting, and through
the `figs-lsp` language server, completion (core builders, the framework
prelude, the script's own functions and variables, and hooks after `fn`),
hover documentation, signature help, diagnostics from evaluating the script,
semantic highlighting, the outline and go to definition.

## Setup

1. Build the server: `cargo install --path crates/figs-lsp` (puts `figs-lsp`
   on `PATH`), or `cargo build -p figs-lsp --release` when working in the
   figs_rs repository (the extension finds `target/release` / `target/debug`
   of the workspace).
2. Install the extension: `npm install` here, then
   `npx @vscode/vsce package --skip-license` and
   `code --install-extension figs-0.1.0.vsix`.

`figs.server.path` points at another `figs-lsp` executable; the command
**figs: Restart Language Server** picks up a rebuilt one.
