//! The render pipeline shared by `build` and `watch`: TOML in, PNG/PDF out.

use std::path::Path;

use anyhow::{bail, Context, Result};
use figs_core::{layout, render_pdf, render_png, Assets, Document};

/// Build the asset store for a document: images resolve relative to the
/// document's directory. Fonts are loaded once and can be reused across reloads.
pub fn assets_for(input: &Path) -> Assets {
    let base = input.parent().filter(|p| !p.as_os_str().is_empty());
    Assets::new(base.unwrap_or_else(|| Path::new(".")))
}

/// Parse, lay out and render `input` to `output`, choosing the format from the
/// output extension. Returns the number of bytes written.
pub fn render_once(input: &Path, output: &Path, assets: &Assets) -> Result<usize> {
    let src = std::fs::read_to_string(input)
        .with_context(|| format!("reading input `{}`", input.display()))?;
    let doc =
        Document::from_toml(&src).with_context(|| format!("parsing `{}`", input.display()))?;
    let computed = layout(&doc, assets);

    let bytes = match output.extension().and_then(|e| e.to_str()) {
        Some("png") => render_png(&computed, assets).context("rendering PNG")?,
        Some("pdf") => render_pdf(&computed, assets).context("rendering PDF")?,
        other => bail!(
            "unsupported output extension {:?}; use .png or .pdf",
            other.unwrap_or("(none)")
        ),
    };

    std::fs::write(output, &bytes)
        .with_context(|| format!("writing output `{}`", output.display()))?;
    Ok(bytes.len())
}
