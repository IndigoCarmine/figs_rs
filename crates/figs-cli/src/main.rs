//! `figs` — render a layout document (TOML or a Rhai `.figs` script) to a file.
//!
//! `figs build IN -o OUT.{png,pdf,svg}`: the output extension picks the backend.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use figs_core::{layout, render_pdf, render_png, render_svg, Document, FontStore, RenderOptions};

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
        /// Input document: `.toml`, or a Rhai script `.figs` / `.rhai`.
        input: PathBuf,
        /// Output path. Extension selects the format: .png, .pdf or .svg.
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Build { input, output } => build(&input, &output),
    }
}

/// Output formats, chosen by the output file's extension.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Png,
    Pdf,
    Svg,
}

impl Format {
    fn from_path(path: &Path) -> Result<Format> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("png") => Ok(Format::Png),
            Some("pdf") => Ok(Format::Pdf),
            Some("svg") => Ok(Format::Svg),
            other => bail!(
                "unsupported output extension {:?} for `{}`; use .png, .pdf, or .svg",
                other.unwrap_or("(none)"),
                path.display()
            ),
        }
    }
}

fn build(input: &Path, output: &Path) -> Result<()> {
    // Validate the output format before doing any work.
    let format = Format::from_path(output)?;
    let src = std::fs::read_to_string(input)
        .with_context(|| format!("reading input `{}`", input.display()))?;
    let doc = match input.extension().and_then(|e| e.to_str()) {
        Some("toml") => Document::from_toml(&src),
        // Data files a script reads resolve against its directory, like images.
        Some("figs") | Some("rhai") => Document::from_script_in(&src, input.parent()),
        other => bail!(
            "unsupported input extension {:?}; use .toml, .figs, or .rhai",
            other.unwrap_or("(none)")
        ),
    }
    .with_context(|| format!("parsing `{}`", input.display()))?;

    // Build the font store once: it both measures text during layout and
    // rasterizes glyphs during rendering, so geometry is identical in both.
    // Relative image `src` paths resolve against the document's directory.
    let base_dir = input.parent().unwrap_or_else(|| Path::new("."));

    let mut fonts = FontStore::new();
    fonts.set_default_family(doc.page.font_family.clone());
    // Images without an aspect ratio take their file's natural aspect (this
    // also drives `height = "auto"` pages).
    fonts.set_image_base_dir(Some(base_dir.to_path_buf()));
    let computed = layout(&doc, &fonts);
    let opts = RenderOptions {
        fonts: Some(&fonts),
        base_dir,
    };

    let bytes = match format {
        Format::Png => render_png(&computed, &opts).context("rendering PNG")?,
        Format::Pdf => render_pdf(&computed, &opts).context("rendering PDF")?,
        Format::Svg => render_svg(&computed, &opts).into_bytes(),
    };

    std::fs::write(output, &bytes)
        .with_context(|| format!("writing output `{}`", output.display()))?;
    // Only raster output depends on the dpi; vector output is in points.
    let res = if format == Format::Png {
        format!(" at {:.0} dpi", computed.page.dpi)
    } else {
        format!(
            ", {:.2} x {:.2} pt",
            computed.page.width_pt, computed.page.height_pt
        )
    };
    eprintln!(
        "wrote {} ({} bytes){}{}",
        output.display(),
        bytes.len(),
        res,
        if computed.page.auto_height {
            format!(", auto height {:.2} pt", computed.page.height_pt)
        } else {
            String::new()
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_follows_the_extension() {
        assert!(Format::from_path(Path::new("a.png")).unwrap() == Format::Png);
        assert!(Format::from_path(Path::new("a.PDF")).unwrap() == Format::Pdf);
        assert!(Format::from_path(Path::new("dir/a.svg")).unwrap() == Format::Svg);
        let err = Format::from_path(Path::new("a.jpg"))
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("\"jpg\"") && err.contains(".pdf"), "{err}");
        assert!(Format::from_path(Path::new("noext")).is_err());
    }
}
