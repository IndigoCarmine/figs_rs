//! `figs-editor` — a desktop Editor + Viewer for figs documents (egui/eframe).
//!
//! Edit a `RawDocument` through the outline / inspector / TOML panes and see it
//! rendered live on the canvas; save back to TOML or export a PNG. The "1b"
//! design layout: a unified tabbed left rail beside a TOML-source + canvas
//! work area.
//!
//! `figs-editor [FILE]` opens a `.figs`/`.rhai` script (Script tab, evaluated
//! immediately) or a `.toml` document (TOML tab). The open file and the images
//! it uses are watched: external edits reload live.

mod app;
mod model;
mod panels;
mod theme;

use std::path::PathBuf;

use app::FigsApp;

const USAGE: &str = "usage: figs-editor [FILE.figs|FILE.rhai|FILE.toml]";

/// The optional file to open: the single positional argument.
fn parse_args(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<Option<PathBuf>, String> {
    let mut file = None;
    for arg in args {
        match arg.to_str() {
            Some("-h" | "--help") => return Err(USAGE.to_string()),
            Some(s) if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option `{s}`\n{USAGE}"))
            }
            _ if file.is_some() => return Err(format!("expected at most one file\n{USAGE}")),
            _ => file = Some(PathBuf::from(arg)),
        }
    }
    Ok(file)
}

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let file = match parse_args(std::env::args_os().skip(1)) {
        Ok(file) => file,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 840.0])
            .with_min_inner_size([900.0, 560.0])
            .with_drag_and_drop(true)
            .with_title("figs editor"),
        ..Default::default()
    };

    eframe::run_native(
        "figs editor",
        options,
        Box::new(move |cc| Ok(Box::new(FigsApp::new(cc, file)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<PathBuf>, String> {
        parse_args(args.iter().map(std::ffi::OsString::from))
    }

    #[test]
    fn takes_an_optional_file() {
        assert_eq!(parse(&[]), Ok(None));
        assert_eq!(parse(&["fig.figs"]), Ok(Some(PathBuf::from("fig.figs"))));
        assert_eq!(parse(&["-"]), Ok(Some(PathBuf::from("-"))));
        assert!(parse(&["a.figs", "b.figs"]).is_err());
        assert!(parse(&["--bogus"]).unwrap_err().contains("--bogus"));
        assert!(parse(&["--help"]).unwrap_err().starts_with("usage"));
    }
}
