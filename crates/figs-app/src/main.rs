//! `figs-editor` — a desktop Editor + Viewer for figs documents (egui/eframe).
//!
//! Edit a `RawDocument` through the outline / inspector / TOML panes and see it
//! rendered live on the canvas; save back to TOML or export a PNG. The "1b"
//! design layout: a unified tabbed left rail beside a TOML-source + canvas
//! work area.

mod app;
mod model;
mod panels;
mod theme;

use app::FigsApp;

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

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
        Box::new(|cc| Ok(Box::new(FigsApp::new(cc)))),
    )
}
