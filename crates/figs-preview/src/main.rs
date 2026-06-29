//! `figs-preview IN.toml` — a live preview window. Renders the document and
//! re-renders whenever the file changes; export buttons write PNG/PDF.
//!
//! This binary needs a desktop environment (it opens a window) and is excluded
//! from the workspace default members, so headless CI never builds it.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, SystemTime};

use eframe::egui;
use egui::load::SizedTexture;
use figs_core::{layout, render_rgba, Assets, Document};
use figs_cli::pipeline;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};

fn main() -> eframe::Result<()> {
    let input = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("usage: figs-preview <document.toml>");
            std::process::exit(2);
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "figs preview",
        options,
        Box::new(move |cc| {
            let (tx, rx) = channel::<()>();
            spawn_watcher(input.clone(), cc.egui_ctx.clone(), tx);
            let mut app = PreviewApp::new(input, rx);
            app.rerender(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

struct PreviewApp {
    input: PathBuf,
    assets: Assets,
    texture: Option<egui::TextureHandle>,
    size: [usize; 2],
    status: String,
    rx: Receiver<()>,
}

impl PreviewApp {
    fn new(input: PathBuf, rx: Receiver<()>) -> Self {
        let assets = pipeline::assets_for(&input);
        PreviewApp {
            input,
            assets,
            texture: None,
            size: [0, 0],
            status: String::new(),
            rx,
        }
    }

    /// Re-read the document and refresh the preview texture.
    fn rerender(&mut self, ctx: &egui::Context) {
        match render(&self.input, &self.assets) {
            Ok((w, h, rgba)) => {
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                self.texture =
                    Some(ctx.load_texture("preview", image, egui::TextureOptions::LINEAR));
                self.size = [w as usize, h as usize];
                self.status = format!("{w}x{h} px");
            }
            Err(e) => self.status = format!("error: {e:#}"),
        }
    }

    fn export(&mut self, ext: &str) {
        let out = self.input.with_extension(ext);
        match pipeline::render_once(&self.input, &out, &self.assets) {
            Ok(n) => self.status = format!("wrote {} ({n} bytes)", out.display()),
            Err(e) => self.status = format!("export failed: {e:#}"),
        }
    }
}

impl eframe::App for PreviewApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Coalesce any pending file-change signals into one rerender.
        let mut dirty = false;
        while self.rx.try_recv().is_ok() {
            dirty = true;
        }
        if dirty {
            self.rerender(ctx);
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.monospace(self.input.display().to_string());
                ui.separator();
                if ui.button("Export PNG").clicked() {
                    self.export("png");
                }
                if ui.button("Export PDF").clicked() {
                    self.export("pdf");
                }
                ui.separator();
                ui.label(&self.status);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(texture) = &self.texture {
                let avail = ui.available_size();
                let [w, h] = self.size;
                if w > 0 && h > 0 {
                    let ar = w as f32 / h as f32;
                    let mut size = avail;
                    if avail.x / avail.y > ar {
                        size.x = avail.y * ar;
                    } else {
                        size.y = avail.x / ar;
                    }
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Image::new(SizedTexture::new(texture.id(), size)));
                    });
                }
            } else {
                ui.centered_and_justified(|ui| ui.label("rendering…"));
            }
        });
    }
}

fn render(input: &Path, assets: &Assets) -> anyhow::Result<(u32, u32, Vec<u8>)> {
    let src = std::fs::read_to_string(input)?;
    let doc = Document::from_toml(&src)?;
    let computed = layout(&doc, assets);
    let (w, h, rgba) = render_rgba(&computed, assets)?;
    Ok((w, h, rgba))
}

/// Background thread: watch the input's directory and signal the UI (and force a
/// repaint) when the input's mtime changes. Mirrors the CLI watcher's mtime
/// keying to avoid reacting to unrelated writes.
fn spawn_watcher(input: PathBuf, ctx: egui::Context, tx: Sender<()>) {
    std::thread::spawn(move || {
        let dir = input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let (etx, erx) = channel::<DebounceEventResult>();
        let mut debouncer = match new_debouncer(Duration::from_millis(150), etx) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("watcher init failed: {e}");
                return;
            }
        };
        if let Err(e) = debouncer.watcher().watch(&dir, RecursiveMode::NonRecursive) {
            eprintln!("watch failed: {e}");
            return;
        }

        let mut last = mtime(&input);
        for res in erx {
            if res.is_ok() {
                let now = mtime(&input);
                if now != last {
                    last = now;
                    let _ = tx.send(());
                    ctx.request_repaint();
                }
            }
        }
    });
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}
