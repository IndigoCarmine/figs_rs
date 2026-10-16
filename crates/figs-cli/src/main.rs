//! `figs` — render a layout document (TOML or a Rhai `.figs` script) to a file.
//!
//! - `figs build IN -o OUT.{png,pdf,svg}`: render once; the output extension
//!   picks the backend.
//! - `figs watch IN -o OUT`: render, then re-render whenever the input or any
//!   file it depends on (images, script data) changes, until Ctrl+C.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use figs_core::watch::{FileWatcher, DEFAULT_DEBOUNCE};
use figs_core::{layout, render_pdf, render_png, render_svg, Document, FontStore, RenderOptions};

#[derive(Parser)]
#[command(
    name = "figs",
    version,
    about = "Compose figures/posters from TOML or Rhai scripts"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug, PartialEq)]
enum Command {
    /// Render a document once to an output file (format inferred from extension).
    Build {
        /// Input document: `.toml`, or a Rhai script `.figs` / `.rhai`.
        input: PathBuf,
        /// Output path. Extension selects the format: .png, .pdf or .svg.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Render, then re-render whenever the input or a file it uses changes.
    ///
    /// Watches the input plus every dependency of the last successful build
    /// (image `src`s, data a script loaded). Errors are printed and watching
    /// continues; Ctrl+C stops.
    Watch {
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
        Command::Build { input, output } => {
            let built = build(&input, &output, &mut FontStore::new())?;
            eprintln!("{}", built.summary);
            Ok(())
        }
        Command::Watch { input, output } => watch(&input, &output),
    }
}

/// Output formats, chosen by the output file's extension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

/// The result of one successful build.
struct Built {
    /// One line describing what was written (path, size, resolution).
    summary: String,
    /// Absolute paths of the files the document read besides its source.
    deps: Vec<PathBuf>,
}

/// Relative image `src` (and data) paths resolve against the document's
/// directory.
fn base_dir(input: &Path) -> &Path {
    match input.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    }
}

/// Read, lay out and render `input` to `output`. `fonts` is reused across
/// builds (loading system fonts is the slow part of a cold start).
fn build(input: &Path, output: &Path, fonts: &mut FontStore) -> Result<Built> {
    // Validate the output format before doing any work.
    let format = Format::from_path(output)?;
    let src = std::fs::read_to_string(input)
        .with_context(|| format!("reading input `{}`", input.display()))?;
    let doc = match input.extension().and_then(|e| e.to_str()) {
        Some("toml") => Document::from_toml(&src),
        Some("figs") | Some("rhai") => Document::from_script(&src),
        other => bail!(
            "unsupported input extension {:?}; use .toml, .figs, or .rhai",
            other.unwrap_or("(none)")
        ),
    }
    .with_context(|| format!("parsing `{}`", input.display()))?;

    // The font store both measures text during layout and rasterizes glyphs
    // during rendering, so geometry is identical in both.
    let base_dir = base_dir(input);
    fonts.set_default_family(doc.page.font_family.clone());
    // Images without an aspect ratio take their file's natural aspect (this
    // also drives `height = "auto"` pages). Setting it also drops cached image
    // aspects, so a rebuild sees edited images.
    fonts.set_image_base_dir(Some(base_dir.to_path_buf()));
    let computed = layout(&doc, fonts);
    let opts = RenderOptions {
        fonts: Some(fonts),
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
    let auto = if computed.page.auto_height {
        format!(", auto height {:.2} pt", computed.page.height_pt)
    } else {
        String::new()
    };
    Ok(Built {
        summary: format!(
            "wrote {} ({} bytes){res}{auto}",
            output.display(),
            bytes.len()
        ),
        deps: doc.dependencies(base_dir),
    })
}

/// What the watch loop reacts to.
enum Event {
    Changed(Vec<PathBuf>),
    Stop,
}

/// One watch-mode rebuild: build, print a one-line result, and return the files
/// to watch next. On error the previous dependency list is kept (the broken
/// document can't say what it needs), so fixing an image still triggers a build.
fn rebuild(input: &Path, output: &Path, fonts: &mut FontStore, deps: &mut Vec<PathBuf>) -> bool {
    let start = Instant::now();
    match build(input, output, fonts) {
        Ok(built) => {
            eprintln!(
                "ok  {} in {} ms",
                built.summary,
                start.elapsed().as_millis()
            );
            *deps = built.deps;
            true
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            false
        }
    }
}

/// The files to watch: the source plus its dependencies, never the output
/// (writing it must not trigger another build).
fn watch_set(input: &Path, output: &Path, deps: &[PathBuf]) -> Vec<PathBuf> {
    let out = figs_core::deps::absolutize(Path::new(""), output);
    std::iter::once(figs_core::deps::absolutize(Path::new(""), input))
        .chain(deps.iter().cloned())
        .filter(|p| *p != out)
        .collect()
}

fn watch(input: &Path, output: &Path) -> Result<()> {
    // A bad output extension never fixes itself: fail fast.
    Format::from_path(output)?;

    let (tx, rx) = mpsc::channel();
    let stop = tx.clone();
    ctrlc::set_handler(move || {
        let _ = stop.send(Event::Stop);
    })
    .context("installing the Ctrl+C handler")?;
    let mut watcher = FileWatcher::new(DEFAULT_DEBOUNCE, move |paths| {
        let _ = tx.send(Event::Changed(paths));
    })
    .context("starting the file watcher")?;

    let mut fonts = FontStore::new();
    let mut deps = Vec::new();
    rebuild(input, output, &mut fonts, &mut deps);
    watcher.set_files(watch_set(input, output, &deps));
    eprintln!(
        "watching {} and {} dependenc{} (Ctrl+C to stop)",
        input.display(),
        deps.len(),
        if deps.len() == 1 { "y" } else { "ies" }
    );

    while let Ok(event) = rx.recv() {
        let Event::Changed(mut changed) = event else {
            break;
        };
        // Coalesce anything else that arrived meanwhile into this build.
        let mut stop = false;
        while let Ok(more) = rx.recv_timeout(Duration::from_millis(20)) {
            match more {
                Event::Changed(paths) => changed.extend(paths),
                Event::Stop => stop = true,
            }
        }
        if stop {
            break;
        }
        let mut names: Vec<String> = changed
            .iter()
            .map(|p| {
                p.file_name()
                    .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into())
            })
            .collect();
        names.dedup();
        eprintln!("changed: {}", names.join(", "));
        rebuild(input, output, &mut fonts, &mut deps);
        watcher.set_files(watch_set(input, output, &deps));
    }
    eprintln!("stopped watching");
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

    fn parse(args: &[&str]) -> Result<Command, clap::Error> {
        Cli::try_parse_from(std::iter::once("figs").chain(args.iter().copied())).map(|c| c.command)
    }

    #[test]
    fn parses_build_and_watch() {
        assert_eq!(
            parse(&["build", "fig.toml", "-o", "out.png"]).unwrap(),
            Command::Build {
                input: "fig.toml".into(),
                output: "out.png".into()
            }
        );
        assert_eq!(
            parse(&["watch", "fig.figs", "--output", "out.pdf"]).unwrap(),
            Command::Watch {
                input: "fig.figs".into(),
                output: "out.pdf".into()
            }
        );
        // The output is required; so is the input.
        assert!(parse(&["watch", "fig.figs"]).is_err());
        assert!(parse(&["watch", "-o", "out.png"]).is_err());
    }

    #[test]
    fn base_dir_of_a_bare_file_name_is_the_working_directory() {
        assert_eq!(base_dir(Path::new("fig.figs")), Path::new("."));
        assert_eq!(base_dir(Path::new("a/fig.figs")), Path::new("a"));
    }

    #[test]
    fn the_output_is_never_watched() {
        let set = watch_set(
            Path::new("fig.figs"),
            Path::new("out.png"),
            &[figs_core::deps::absolutize(
                Path::new(""),
                Path::new("out.png"),
            )],
        );
        assert_eq!(set.len(), 1);
        assert!(set[0].is_absolute() && set[0].ends_with("fig.figs"));
    }

    fn write_png(path: &Path, w: f32, h: f32) {
        let doc = Document::from_toml(&format!(
            "[page]\nwidth = {w}\nheight = {h}\nunit = \"pt\"\ndpi = 72\nroot = \"r\"\n\
             [nodes.r]\ntype = \"rect\"\nfill = \"#ff0000\"\n"
        ))
        .unwrap();
        let computed = layout(&doc, &figs_core::NullMeasurer);
        std::fs::write(
            path,
            render_png(&computed, &RenderOptions::default()).unwrap(),
        )
        .unwrap();
    }

    /// The watch loop's rebuild step, driven directly (no OS file events):
    /// dependencies are found, edits show up in the output, and a broken edit
    /// keeps the last dependency list.
    #[test]
    fn rebuild_tracks_dependencies_and_survives_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_png(&dir.join("a.png"), 20.0, 10.0);
        let input = dir.join("fig.toml");
        let output = dir.join("out.svg");
        let doc = |src: &str| {
            format!(
                "[page]\nwidth = 100\nheight = \"auto\"\nunit = \"pt\"\nroot = \"root\"\n\
                 [nodes.root]\ntype = \"column\"\nchildren = [\"img\"]\n\
                 [nodes.img]\ntype = \"image\"\nsrc = \"{src}\"\n"
            )
        };
        std::fs::write(&input, doc("a.png")).unwrap();

        let mut fonts = FontStore::new();
        let mut deps = Vec::new();
        assert!(rebuild(&input, &output, &mut fonts, &mut deps));
        let abs = |name: &str| figs_core::deps::absolutize(dir, Path::new(name));
        assert_eq!(deps, vec![abs("a.png")]);
        let first = std::fs::read_to_string(&output).unwrap();
        // auto height follows the image's natural 2:1 aspect: 100 x 50 pt.
        assert!(first.contains("height=\"50"), "{first}");

        // Editing the image is picked up (the aspect cache is dropped).
        write_png(&dir.join("a.png"), 10.0, 10.0);
        assert!(rebuild(&input, &output, &mut fonts, &mut deps));
        let second = std::fs::read_to_string(&output).unwrap();
        assert!(second.contains("height=\"100"), "{second}");

        // A broken edit reports an error and keeps the previous dependencies.
        std::fs::write(&input, "[page\n").unwrap();
        assert!(!rebuild(&input, &output, &mut fonts, &mut deps));
        assert_eq!(deps, vec![abs("a.png")]);
        assert_eq!(std::fs::read_to_string(&output).unwrap(), second);

        // Fixing it with a different image switches the dependency.
        std::fs::write(&input, doc("sub/b.png")).unwrap();
        assert!(rebuild(&input, &output, &mut fonts, &mut deps));
        assert_eq!(deps, vec![abs("sub/b.png")]);
        let set = watch_set(&input, &output, &deps);
        assert_eq!(set, vec![abs("fig.toml"), abs("sub/b.png")]);
    }
}
