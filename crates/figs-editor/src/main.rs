//! `figs-editor IN.toml` — a native egui WYSIWYG editor.
//!
//! Renders the document to a canvas, lets you click a node to select it, edit
//! its properties in a side panel, and saves changes back to the TOML with
//! comments/formatting preserved (via `EditableDocument`). This is the
//! bidirectional-sync loop: visual edit -> toml_edit -> file.
//!
//! Needs a desktop (opens a window); excluded from workspace default members so
//! headless CI never builds it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, SystemTime};

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use figs_core::geom::Rect as FigRect;
use figs_core::layout::ComputedLayout;
use figs_core::{layout, render_rgba, Assets, EditableDocument};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};

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
    /// Receives a signal when the file changes on disk.
    reload_rx: Receiver<()>,
    /// Last input mtime we are in sync with; gates self-saves vs external edits.
    last_mtime: Option<SystemTime>,
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
        let (tx, reload_rx) = channel::<()>();
        spawn_watcher(path.clone(), ctx.clone(), tx);
        let last_mtime = mtime(&path);

        let mut app = EditorApp {
            path,
            assets,
            editable,
            computed: None,
            texture: None,
            img_size: [0, 0],
            selected: None,
            status: String::new(),
            reload_rx,
            last_mtime,
        };
        app.rerender(ctx);
        app
    }

    /// Reload the document from disk (external edit), preserving the selection
    /// if its node still exists.
    fn reload(&mut self, ctx: &egui::Context) {
        match std::fs::read_to_string(&self.path)
            .map_err(|e| e.to_string())
            .and_then(|s| EditableDocument::parse(&s).map_err(|e| e.to_string()))
        {
            Ok(doc) => {
                if let Some(sel) = &self.selected {
                    if !doc.node_ids().iter().any(|n| n == sel) {
                        self.selected = None;
                    }
                }
                self.editable = Some(doc);
                self.rerender(ctx);
                self.status = "reloaded (external change)".into();
            }
            Err(e) => self.status = format!("reload failed: {e}"),
        }
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
                Ok(()) => {
                    // Record our own write so the watcher doesn't reload it.
                    self.last_mtime = mtime(&self.path);
                    self.status = format!("saved {}", self.path.display());
                }
                Err(e) => self.status = format!("save failed: {e}"),
            }
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut dirty = false;

        // Reflect external edits: reload only when the file's mtime differs from
        // what we last wrote/loaded (so our own Save doesn't trigger a reload).
        let mut signalled = false;
        while self.reload_rx.try_recv().is_ok() {
            signalled = true;
        }
        if signalled {
            let now = mtime(&self.path);
            if now != self.last_mtime {
                self.last_mtime = now;
                self.reload(ctx);
            }
        }

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

        egui::SidePanel::left("tree")
            .default_width(220.0)
            .show(ctx, |ui| {
                dirty |= self.tree_panel(ui);
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
    /// The node tree with structural-edit buttons. Returns true if an edit was
    /// made (so the caller re-renders).
    fn tree_panel(&mut self, ui: &mut egui::Ui) -> bool {
        ui.heading("Tree");
        let has_sel = self.selected.is_some();
        let mut action: Option<TreeAction> = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(has_sel, egui::Button::new("+ child"))
                .clicked()
            {
                action = Some(TreeAction::AddChild);
            }
            if ui.add_enabled(has_sel, egui::Button::new("Delete")).clicked() {
                action = Some(TreeAction::Delete);
            }
            if ui.add_enabled(has_sel, egui::Button::new("↑")).clicked() {
                action = Some(TreeAction::MoveUp);
            }
            if ui.add_enabled(has_sel, egui::Button::new("↓")).clicked() {
                action = Some(TreeAction::MoveDown);
            }
        });
        ui.separator();

        let rows = self.tree_rows();
        let mut clicked: Option<String> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (id, depth) in &rows {
                ui.horizontal(|ui| {
                    ui.add_space(*depth as f32 * 14.0);
                    let kind = self
                        .editable
                        .as_ref()
                        .and_then(|e| e.get_string(id, "type"))
                        .unwrap_or_default();
                    let selected = self.selected.as_deref() == Some(id.as_str());
                    if ui
                        .selectable_label(selected, format!("{id}  ({kind})"))
                        .clicked()
                    {
                        clicked = Some(id.clone());
                    }
                });
            }
        });
        if let Some(id) = clicked {
            self.selected = Some(id);
        }

        match action {
            Some(a) => self.apply_tree_action(a),
            None => false,
        }
    }

    /// Flatten the node tree into `(id, depth)` rows via preorder DFS, guarding
    /// against cycles.
    fn tree_rows(&self) -> Vec<(String, usize)> {
        let mut rows = Vec::new();
        if let Some(editable) = &self.editable {
            if let Some(root) = editable.root_id() {
                let mut visited = HashSet::new();
                collect_tree(editable, root, 0, &mut visited, &mut rows);
            }
        }
        rows
    }

    /// Apply a structural edit to the selected node. Returns true on change.
    fn apply_tree_action(&mut self, action: TreeAction) -> bool {
        let (Some(editable), Some(sel)) = (&mut self.editable, self.selected.clone()) else {
            return false;
        };
        match action {
            TreeAction::AddChild => {
                let id = unique_id(editable);
                let index = editable.children(&sel).map(|c| c.len()).unwrap_or(0);
                if editable.add_node(&id, "rect", &sel, index).is_ok() {
                    self.selected = Some(id);
                    return true;
                }
            }
            TreeAction::Delete => {
                if editable.root_id().as_deref() == Some(sel.as_str()) {
                    self.status = "cannot delete the root node".into();
                    return false;
                }
                let parent = editable.parent_of(&sel);
                if editable.remove_node(&sel).is_ok() {
                    self.selected = parent;
                    return true;
                }
            }
            TreeAction::MoveUp => return self.move_sibling(&sel, -1),
            TreeAction::MoveDown => return self.move_sibling(&sel, 1),
        }
        false
    }

    /// Swap the selected node with its previous/next sibling.
    fn move_sibling(&mut self, id: &str, delta: i32) -> bool {
        let Some(editable) = &mut self.editable else {
            return false;
        };
        let Some(parent) = editable.parent_of(id) else {
            return false;
        };
        let Ok(mut kids) = editable.children(&parent) else {
            return false;
        };
        let Some(pos) = kids.iter().position(|c| c == id) else {
            return false;
        };
        let target = pos as i32 + delta;
        if target < 0 || target >= kids.len() as i32 {
            return false;
        }
        kids.swap(pos, target as usize);
        editable.set_children(&parent, &kids).is_ok()
    }

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

/// Watch the input's directory and signal the UI (and force a repaint) on any
/// filesystem event; the app gates actual reloads on the input's mtime.
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
        if debouncer
            .watcher()
            .watch(&dir, RecursiveMode::NonRecursive)
            .is_err()
        {
            return;
        }
        for res in erx {
            if res.is_ok() {
                let _ = tx.send(());
                ctx.request_repaint();
            }
        }
    });
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// A structural edit requested from the tree panel.
#[derive(Clone, Copy)]
enum TreeAction {
    AddChild,
    Delete,
    MoveUp,
    MoveDown,
}

/// Preorder DFS collecting `(id, depth)` rows, guarding against cycles.
fn collect_tree(
    editable: &EditableDocument,
    id: String,
    depth: usize,
    visited: &mut HashSet<String>,
    rows: &mut Vec<(String, usize)>,
) {
    if !visited.insert(id.clone()) {
        return;
    }
    rows.push((id.clone(), depth));
    if let Ok(kids) = editable.children(&id) {
        for k in kids {
            collect_tree(editable, k, depth + 1, visited, rows);
        }
    }
}

/// Pick a node id not already used in the document (`node_1`, `node_2`, …).
fn unique_id(editable: &EditableDocument) -> String {
    let existing: HashSet<String> = editable.node_ids().into_iter().collect();
    (1..)
        .map(|i| format!("node_{i}"))
        .find(|id| !existing.contains(id))
        .expect("infinite range yields a free id")
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
