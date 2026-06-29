//! `figs-editor IN.toml` — a native egui WYSIWYG editor.
//!
//! Renders the document to a canvas, lets you click a node to select it, edit
//! its properties in a side panel, and saves changes back to the TOML with
//! comments/formatting preserved (via `EditableDocument`). This is the
//! bidirectional-sync loop: visual edit -> toml_edit -> file.
//!
//! Needs a desktop (opens a window); excluded from workspace default members so
//! headless CI never builds it.

use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use figs_core::geom::Rect as FigRect;
use figs_core::layout::ComputedLayout;
use figs_core::{layout, render_rgba, Assets, EditableDocument};

fn main() -> eframe::Result<()> {
    let path = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("usage: figs-editor <document.toml>");
            std::process::exit(2);
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0]),
        ..Default::default()
    };
    eframe::run_native(
        "figs editor",
        options,
        Box::new(move |cc| Ok(Box::new(EditorApp::new(path, &cc.egui_ctx)))),
    )
}

struct EditorApp {
    path: PathBuf,
    assets: Assets,
    editable: Option<EditableDocument>,
    computed: Option<ComputedLayout>,
    texture: Option<egui::TextureHandle>,
    img_size: [usize; 2],
    selected: Option<String>,
    status: String,
}

impl EditorApp {
    fn new(path: PathBuf, ctx: &egui::Context) -> Self {
        let assets = Assets::new(path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| std::path::Path::new(".")));
        let editable = match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| EditableDocument::parse(&s).map_err(|e| e.to_string()))
        {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("failed to load {}: {e}", path.display());
                None
            }
        };
        let mut app = EditorApp {
            path,
            assets,
            editable,
            computed: None,
            texture: None,
            img_size: [0, 0],
            selected: None,
            status: String::new(),
        };
        app.rerender(ctx);
        app
    }

    /// Re-resolve the edited document, lay it out and refresh the texture.
    fn rerender(&mut self, ctx: &egui::Context) {
        let Some(editable) = &self.editable else {
            return;
        };
        match editable.document() {
            Ok(doc) => {
                let computed = layout(&doc, &self.assets);
                match render_rgba(&computed, &self.assets) {
                    Ok((w, h, rgba)) => {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [w as usize, h as usize],
                            &rgba,
                        );
                        self.texture =
                            Some(ctx.load_texture("doc", image, egui::TextureOptions::LINEAR));
                        self.img_size = [w as usize, h as usize];
                        self.status = format!("{w}x{h} px");
                    }
                    Err(e) => self.status = format!("render error: {e}"),
                }
                self.computed = Some(computed);
            }
            Err(e) => self.status = format!("parse error: {e}"),
        }
    }

    fn save(&mut self) {
        if let Some(editable) = &self.editable {
            match std::fs::write(&self.path, editable.to_toml_string()) {
                Ok(()) => self.status = format!("saved {}", self.path.display()),
                Err(e) => self.status = format!("save failed: {e}"),
            }
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut dirty = false;

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.monospace(self.path.display().to_string());
                if ui.button("Save").clicked() {
                    self.save();
                }
                ui.separator();
                ui.label(&self.status);
            });
        });

        egui::SidePanel::right("properties")
            .default_width(240.0)
            .show(ctx, |ui| {
                dirty |= self.properties_panel(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.canvas(ui);
        });

        if dirty {
            self.rerender(ctx);
        }
    }
}

impl EditorApp {
    /// The property editor for the selected node. Returns true if an edit was
    /// made (so the caller re-renders).
    fn properties_panel(&mut self, ui: &mut egui::Ui) -> bool {
        ui.heading("Properties");
        let (Some(editable), Some(id)) = (&mut self.editable, self.selected.clone()) else {
            ui.label("Click a node to select it.");
            return false;
        };
        let kind = editable.get_string(&id, "type").unwrap_or_default();
        ui.monospace(format!("{id}  ({kind})"));
        ui.separator();

        let mut changed = false;
        let mut num = |ui: &mut egui::Ui, label: &str, key: &str| {
            let mut v = editable.get_f64(&id, key).unwrap_or(0.0);
            ui.horizontal(|ui| {
                ui.label(label);
                if ui.add(egui::DragValue::new(&mut v).speed(0.1)).changed() {
                    let _ = editable.set_f64(&id, key, v);
                    changed = true;
                }
            });
        };

        match kind.as_str() {
            "column" | "row" => {
                num(ui, "spacing", "spacing");
                num(ui, "padding", "padding");
                num(ui, "flex", "flex");
            }
            "text" => {
                num(ui, "font_size", "font_size");
                num(ui, "flex", "flex");
                let mut content = editable.get_string(&id, "content").unwrap_or_default();
                ui.label("content");
                if ui.text_edit_multiline(&mut content).changed() {
                    let _ = editable.set_string(&id, "content", &content);
                    changed = true;
                }
            }
            _ => {
                num(ui, "width", "width");
                num(ui, "height", "height");
                num(ui, "flex", "flex");
                num(ui, "aspect_ratio", "aspect_ratio");
            }
        }
        changed
    }

    /// The document canvas: draw the rendered image, the selection outline, and
    /// hit-test clicks to select a node.
    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        let canvas = response.rect;
        let (Some(texture), Some(computed)) = (&self.texture, &self.computed) else {
            return;
        };

        let draw = fit_rect(canvas, self.img_size);
        painter.image(
            texture.id(),
            draw,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );

        let page = &computed.page;
        if let Some(sel) = &self.selected {
            if let Some(node) = computed.nodes.iter().find(|n| &n.id == sel) {
                let r = fig_to_screen(node.rect, draw, page.width_pt, page.height_pt);
                painter.rect_stroke(r, 0.0, Stroke::new(2.0, Color32::from_rgb(220, 40, 40)));
            }
        }

        if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                if draw.contains(p) {
                    let px = (p.x - draw.min.x) / draw.width() * page.width_pt;
                    let py = (p.y - draw.min.y) / draw.height() * page.height_pt;
                    self.selected = hit_test(computed, px, py);
                }
            }
        }
    }
}

/// Smallest node whose rect contains the point (the most specific selection).
fn hit_test(computed: &ComputedLayout, px: f32, py: f32) -> Option<String> {
    computed
        .nodes
        .iter()
        .filter(|n| {
            let r = n.rect;
            px >= r.x && px <= r.x + r.w && py >= r.y && py <= r.y + r.h
        })
        .min_by(|a, b| {
            (a.rect.w * a.rect.h)
                .partial_cmp(&(b.rect.w * b.rect.h))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|n| n.id.clone())
}

/// Aspect-fit a `[w, h]` image centered within `canvas`.
fn fit_rect(canvas: Rect, size: [usize; 2]) -> Rect {
    let (iw, ih) = (size[0].max(1) as f32, size[1].max(1) as f32);
    let ar = iw / ih;
    let (cw, ch) = (canvas.width(), canvas.height());
    let (dw, dh) = if cw / ch > ar {
        (ch * ar, ch)
    } else {
        (cw, cw / ar)
    };
    let center = canvas.center();
    Rect::from_center_size(center, Vec2::new(dw, dh))
}

/// Map a point-space rect onto the on-screen draw rect.
fn fig_to_screen(r: FigRect, draw: Rect, page_w: f32, page_h: f32) -> Rect {
    let sx = draw.min.x + r.x / page_w * draw.width();
    let sy = draw.min.y + r.y / page_h * draw.height();
    Rect::from_min_size(
        Pos2::new(sx, sy),
        Vec2::new(r.w / page_w * draw.width(), r.h / page_h * draw.height()),
    )
}
