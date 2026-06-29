//! `figs` — render a TOML layout document to PNG/PDF, once or on every change.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use figs_cli::{pipeline, watch};

#[derive(Parser)]
#[command(name = "figs", version, about = "Compose figures/posters from TOML")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a document once (format inferred from the output extension).
    Build {
        /// Input TOML document.
        input: PathBuf,
        /// Output path (.png or .pdf).
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Watch the document and re-render on every change until interrupted.
    Watch {
        /// Input TOML document.
        input: PathBuf,
        /// Output path (.png or .pdf).
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
        Command::Build { input, output } => {
            let assets = pipeline::assets_for(&input);
            let n = pipeline::render_once(&input, &output, &assets)?;
            eprintln!("wrote {} ({n} bytes)", output.display());
            Ok(())
        }
        Command::Watch { input, output } => watch::run(&input, &output),
    }
}
