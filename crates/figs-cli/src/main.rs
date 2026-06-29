//! `figs` — render a TOML layout document to an image.
//!
//! Phase 1 entry point. This milestone supports `figs build IN.toml -o OUT.png`.
//! PDF output and a `watch` mode land in later milestones.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use figs_core::{layout, render_png, Document, FontStore};

#[derive(Parser)]
#[command(name = "figs", version, about = "Compose figures/posters from TOML")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a document once to an output file (format inferred from extension).
    Build {
        /// Input TOML document.
        input: PathBuf,
        /// Output path. Extension selects the format (.png).
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Build { input, output } => build(&input, &output),
    }
}

fn build(input: &Path, output: &Path) -> Result<()> {
    let src = std::fs::read_to_string(input)
        .with_context(|| format!("reading input `{}`", input.display()))?;
    let doc = Document::from_toml(&src)
        .with_context(|| format!("parsing `{}`", input.display()))?;
    let fonts = FontStore::new();
    let computed = layout(&doc, &fonts);

    let bytes = match output.extension().and_then(|e| e.to_str()) {
        Some("png") => render_png(&computed, &fonts).context("rendering PNG")?,
        Some("pdf") => bail!("PDF output is not implemented yet"),
        other => bail!(
            "unsupported output extension {:?}; use .png",
            other.unwrap_or("(none)")
        ),
    };

    std::fs::write(output, &bytes)
        .with_context(|| format!("writing output `{}`", output.display()))?;
    eprintln!(
        "wrote {} ({} bytes) at {:.0} dpi",
        output.display(),
        bytes.len(),
        computed.page.dpi
    );
    Ok(())
}
