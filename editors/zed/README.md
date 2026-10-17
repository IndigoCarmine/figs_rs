# figs for Zed

Language support for figs scripts (`.figs`): highlighting from the Rhai
tree-sitter grammar ([elkowar/tree-sitter-rhai](https://github.com/elkowar/tree-sitter-rhai)),
and through the `figs-lsp` language server, completion, hover documentation,
signature help, diagnostics, the outline and go to definition.

## Setup

1. Put the server on `PATH`: `cargo install --path crates/figs-lsp`.
2. In Zed, run **zed: install dev extension** and pick this directory (Zed
   builds it; it needs Rust from rustup).
3. Optionally let the server refine the colours (core builders and prelude
   functions vs. the script's own), in `settings.json`:

   ```json
   "languages": { "figs": { "semantic_tokens": "combined" } }
   ```

Another `figs-lsp` executable can be set with
`"lsp": { "figs-lsp": { "binary": { "path": "…" } } }`.
