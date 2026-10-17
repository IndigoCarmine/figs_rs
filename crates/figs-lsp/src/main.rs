//! `figs-lsp`: a language server for figs scripts (`.figs`), spoken over
//! stdin / stdout.
//!
//! - **Completion** of the core builders and methods (`text`, `.size`), the
//!   framework prelude (`grid`, `title`), and the script's own functions and
//!   variables; after `fn`, the prelude hooks to override.
//! - **Hover** and **signature help** with every overload, and the prelude's
//!   comments as documentation.
//! - **Diagnostics**: the script is evaluated as `figs build` does, a moment
//!   after the last edit, and its error is shown where it arose.
//! - **Semantic highlighting**, outline (the script's `fn`s) and go to
//!   definition within the script.

mod analysis;
mod catalog;
mod lexer;
mod server;

fn main() {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some("--version" | "-V") => {
            println!("figs-lsp {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        // Editors commonly pass `--stdio`; it is the only transport.
        None | Some("--stdio") => {}
        Some(other) => {
            eprintln!(
                "figs-lsp: unknown argument `{other}` (usage: figs-lsp [--stdio | --version])"
            );
            std::process::exit(2);
        }
    }
    let (connection, io_threads) = lsp_server::Connection::stdio();
    if let Err(e) = server::run(connection) {
        eprintln!("figs-lsp: {e}");
        std::process::exit(1);
    }
    if let Err(e) = io_threads.join() {
        eprintln!("figs-lsp: {e}");
    }
}
