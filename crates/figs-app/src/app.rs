//! The editor application: state, the live render bridge, and the top-level
//! egui layout (menu bar, left rail, TOML source pane, canvas, status bar).

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use eframe::egui;
use figs_core::deps::absolutize;
use figs_core::watch::{same_path, FileWatcher, DEFAULT_DEBOUNCE};
use figs_core::{layout, render_rgba, ComputedLayout, FontStore, RenderOptions};

use crate::model;
use crate::theme::Theme;

/// What is currently selected: the page itself, or a node by id.
#[derive(Clone, PartialEq, Eq)]
pub enum Selection {
    Page,
    Node(String),
}

impl Selection {
    pub fn node_id(&self) -> Option<&str> {
        match self {
            Selection::Node(id) => Some(id),
            Selection::Page => None,
        }
    }
}

/// A cached preview: the uploaded texture plus the layout it came from (used for
/// canvas hit-testing in page-point space).
pub struct Preview {
    pub texture: egui::TextureHandle,
    pub computed: ComputedLayout,
}

pub struct FigsApp {
    pub model: figs_core::schema::RawDocument,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub base_dir: PathBuf,
    pub selected: Selection,
    pub fonts: FontStore,
    pub preview: Option<Preview>,
    pub error: Option<String>,
    pub theme: Theme,
    pub tab: RailTab,
    pub collapsed: std::collections::HashSet<String>,
    /// Set after any model edit; consumed once per frame to rebuild the preview.
    pub needs_rebuild: bool,
    /// Editable buffer backing the TOML pane.
    pub toml_buffer: String,
    /// Last TOML parse error from direct editing (separate from resolve/render).
    pub toml_error: Option<String>,
    /// Set when the model changed via structured edits, so the TOML buffer is
    /// re-synced from the model (but never while the user types in the pane).
    pub regen_toml: bool,
    /// Installed font families, for the inspector font pickers.
    pub font_families: Vec<String>,
    /// Highlighted suggestion in the TOML completion popup.
    pub completion_index: usize,
    /// A token the user dismissed (Esc); completion stays hidden until it changes.
    pub completion_suppress: Option<String>,
    /// Editable buffer backing the Rhai script pane (the `.figs` source).
    pub script_buffer: String,
    /// Last error from evaluating the script.
    pub script_error: Option<String>,
    /// Which tab the source pane shows (Script or TOML).
    pub source_tab: SourceTab,
    /// The open file's contents as last loaded/saved: a disk event whose
    /// contents equal this (e.g. our own save) is not an external change.
    pub disk_text: Option<String>,
    /// Newer on-disk contents of the open file that arrived while the editor
    /// had unsaved edits; drives the "changed on disk" banner.
    pub disk_conflict: Option<String>,
    /// Files the last script evaluation read (recorded by data built-ins).
    pub data_files: Vec<PathBuf>,
    /// Absolute dependencies (images, data) of the last resolved document.
    pub deps: Vec<PathBuf>,
    /// Watches the open file and `deps`; `None` if the OS watcher failed.
    watcher: Option<FileWatcher>,
    /// The set last handed to `watcher` (to skip redundant updates).
    watched: Vec<PathBuf>,
    /// Changed files reported by `watcher`.
    disk_events: mpsc::Receiver<Vec<PathBuf>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RailTab {
    Outline,
    Inspector,
}

/// Which source representation the source pane edits. Rhai is the source of
/// truth; "Evaluate" lowers it to the (concrete) TOML/GUI document.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SourceTab {
    Script,
    Toml,
}

impl FigsApp {
    /// Create the editor, opening `file` if given (else the default script).
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>) -> Self {
        install_fonts(&cc.egui_ctx);
        let script_buffer = model::default_script();
        let model =
            figs_core::eval_script(&script_buffer).unwrap_or_else(|_| model::default_document());
        let toml_buffer = model::to_toml(&model).unwrap_or_default();
        let fonts = FontStore::new();
        let font_families = fonts.families();

        // Disk events arrive on the watcher's thread; wake the UI to handle them.
        let (tx, disk_events) = mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        let watcher = FileWatcher::new(DEFAULT_DEBOUNCE, move |paths| {
            let _ = tx.send(paths);
            ctx.request_repaint();
        })
        .map_err(|e| tracing::warn!("live reload disabled: {e}"))
        .ok();

        let mut app = FigsApp {
            model,
            path: None,
            dirty: false,
            base_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            selected: Selection::Page,
            fonts,
            preview: None,
            error: None,
            theme: Theme::Light,
            tab: RailTab::Inspector,
            collapsed: Default::default(),
            needs_rebuild: true,
            toml_buffer,
            toml_error: None,
            regen_toml: false,
            font_families,
            completion_index: 0,
            completion_suppress: None,
            script_buffer,
            script_error: None,
            source_tab: SourceTab::Script,
            disk_text: None,
            disk_conflict: None,
            data_files: Vec::new(),
            deps: Vec::new(),
            watcher,
            watched: Vec::new(),
            disk_events,
        };
        if let Some(path) = file {
            app.open_path(&path);
        }
        app
    }

    /// Mark the model dirty and schedule a preview rebuild + TOML re-sync.
    pub fn touch(&mut self) {
        self.dirty = true;
        self.needs_rebuild = true;
        self.regen_toml = true;
    }

    /// Evaluate the Rhai script buffer into the model, refreshing the preview,
    /// the TOML pane and the inspector. On error the current model is kept.
    pub fn evaluate_script(&mut self) {
        // Files the script reads (data built-ins) become watched dependencies.
        let (result, data_files) =
            figs_core::deps::capture(|| figs_core::eval_script(&self.script_buffer));
        match result {
            Ok(raw) => {
                self.model = raw;
                self.data_files = data_files;
                self.script_error = None;
                self.error = None;
                self.dirty = true;
                self.needs_rebuild = true;
                self.regen_toml = true;
            }
            Err(e) => self.script_error = Some(e.to_string()),
        }
    }

    /// Re-resolve, lay out and rasterize the current model into a texture. On
    /// any error the previous preview is kept and `error` is set.
    fn rebuild(&mut self, ctx: &egui::Context) {
        self.fonts
            .set_default_family(self.model.page.font_family.clone());
        // Images without an aspect ratio take their file's natural aspect.
        self.fonts.set_image_base_dir(Some(self.base_dir.clone()));
        let raw = self.model.clone();
        let mut doc = match figs_core::schema::resolve::resolve(raw) {
            Ok(d) => d,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        // Watch what this document reads, so edits to its images refresh it.
        doc.data_files = self.data_files.clone();
        self.deps = doc.dependencies(&self.base_dir);
        self.sync_watch();
        let mut computed = layout(&doc, &self.fonts);

        // Cap the preview resolution so big 300-dpi posters stay a sane texture
        // size; hit-testing uses points, so this doesn't affect selection.
        let long_pt = computed.page.width_pt.max(computed.page.height_pt).max(1.0);
        let screen_dpi = (1600.0 * 72.0 / long_pt).min(computed.page.dpi).max(24.0);
        computed.page.dpi = screen_dpi;

        let opts = RenderOptions {
            fonts: Some(&self.fonts),
            base_dir: &self.base_dir,
        };
        match render_rgba(&computed, &opts) {
            Ok(img) => {
                let size = [img.width as usize, img.height as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba);
                let texture = ctx.load_texture("figs-preview", color, egui::TextureOptions::LINEAR);
                self.preview = Some(Preview { texture, computed });
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    // ---- file actions ----------------------------------------------------

    pub fn new_document(&mut self) {
        self.script_buffer = model::default_script();
        self.source_tab = SourceTab::Script;
        self.path = None;
        self.disk_text = None;
        self.disk_conflict = None;
        self.selected = Selection::Page;
        self.error = None;
        self.toml_error = None;
        self.script_error = None;
        self.evaluate_script();
        self.dirty = false;
    }

    pub fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("figs script", &["figs", "rhai"])
            .add_filter("TOML", &["toml"])
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    /// Open `path` (from the dialog or the command line). Relative image paths
    /// then resolve against the file's directory, and the file is watched.
    pub fn open_path(&mut self, path: &Path) {
        let path = absolutize(Path::new(""), path);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(format!("open failed: `{}`: {e}", path.display()));
                return;
            }
        };
        if !is_script_path(&path) {
            // Validate before replacing the current document.
            if let Err(e) = figs_core::schema::parse::parse_str(&text) {
                self.error = Some(format!("open failed: {e}"));
                return;
            }
        }
        self.base_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        self.selected = Selection::Page;
        self.data_files.clear();
        self.path = Some(path);
        self.load_text(text);
        self.sync_watch();
    }

    /// Load `text` as the open file's contents (fresh open or disk reload):
    /// scripts go to the Script tab and are evaluated, TOML to the TOML tab.
    /// The selection is kept when the node still exists.
    fn load_text(&mut self, text: String) {
        self.disk_text = Some(text.clone());
        self.disk_conflict = None;
        self.toml_error = None;
        self.script_error = None;
        self.error = None;
        if self.path.as_deref().is_some_and(is_script_path) {
            self.script_buffer = text;
            self.source_tab = SourceTab::Script;
            self.evaluate_script();
        } else {
            match figs_core::schema::parse::parse_str(&text) {
                Ok(doc) => {
                    self.model = doc;
                    self.source_tab = SourceTab::Toml;
                    self.needs_rebuild = true;
                    self.regen_toml = true;
                }
                // Keep the last good model; show the broken text and its error.
                Err(e) => {
                    self.toml_buffer = text;
                    self.toml_error = Some(e.to_string());
                    self.source_tab = SourceTab::Toml;
                }
            }
        }
        if let Selection::Node(id) = &self.selected {
            if !self.model.nodes.contains_key(id) {
                self.selected = Selection::Page;
            }
        }
        self.dirty = false; // matches the disk
    }

    // ---- live reload -----------------------------------------------------

    /// Point the watcher at the open file plus the current dependencies.
    fn sync_watch(&mut self) {
        let want: Vec<PathBuf> = self.path.iter().cloned().chain(self.deps.clone()).collect();
        if want != self.watched {
            if let Some(w) = self.watcher.as_mut() {
                w.set_files(want.clone());
            }
            self.watched = want;
        }
    }

    /// Handle files the watcher reported as changed: reload the open file (or
    /// raise the conflict banner if there are unsaved edits), re-evaluate the
    /// script when its data changed, and re-render when an image changed.
    fn poll_disk(&mut self) {
        let changed: Vec<PathBuf> = self.disk_events.try_iter().flatten().collect();
        if changed.is_empty() {
            return;
        }
        let is = |list: &[PathBuf], p: &Path| list.iter().any(|q| same_path(q, p));
        let source_changed = self
            .path
            .as_deref()
            .is_some_and(|src| changed.iter().any(|p| same_path(p, src)));
        let data_changed = changed.iter().any(|p| is(&self.data_files_abs(), p));

        if source_changed {
            // A read can fail mid-save (rename in progress): the next event retries.
            if let Some(text) = self
                .path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
            {
                if self.disk_text.as_ref() == Some(&text) {
                    // Our own save, a touch, or a revert: nothing to reload.
                    self.disk_conflict = None;
                } else if self.disk_conflict.as_ref() != Some(&text) {
                    if self.dirty {
                        self.disk_conflict = Some(text);
                    } else {
                        self.load_text(text);
                        return;
                    }
                }
            }
        }
        if data_changed && self.path.as_deref().is_some_and(is_script_path) {
            // Data is read at evaluation time; re-run without marking edits.
            let dirty = self.dirty;
            self.evaluate_script();
            self.dirty = dirty;
        }
        // Images are read at render time (rebuild also drops cached aspects).
        self.needs_rebuild = true;
    }

    fn data_files_abs(&self) -> Vec<PathBuf> {
        self.data_files
            .iter()
            .map(|p| absolutize(&self.base_dir, p))
            .collect()
    }

    /// "Reload": discard unsaved edits for the newer on-disk contents.
    pub fn accept_disk_version(&mut self) {
        if let Some(text) = self.disk_conflict.take() {
            self.load_text(text);
        }
    }

    /// "Keep mine": keep the edits (saving overwrites the disk version). Only a
    /// further external change raises the banner again.
    pub fn keep_my_version(&mut self) {
        if let Some(text) = self.disk_conflict.take() {
            self.disk_text = Some(text);
        }
    }

    pub fn save(&mut self) {
        match self.path.clone() {
            Some(path) => self.write_to(&path),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) {
        let default_name = match self.source_tab {
            SourceTab::Script => "figure.figs",
            SourceTab::Toml => "figure.toml",
        };
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("figs script", &["figs", "rhai"])
            .add_filter("TOML", &["toml"])
            .set_file_name(default_name)
            .save_file()
        {
            self.write_to(&path);
            self.base_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            self.path = Some(path);
            // Relative image paths now resolve against the new folder.
            self.needs_rebuild = true;
            self.sync_watch();
        }
    }

    /// Write the document. `.figs`/`.rhai` saves the script source; anything else
    /// saves the lowered TOML document.
    fn write_to(&mut self, path: &std::path::Path) {
        let text = if is_script_path(path) {
            Ok(self.script_buffer.clone())
        } else {
            model::to_toml(&self.model).map_err(|e| e.to_string())
        };
        let result = text.and_then(|text| {
            std::fs::write(path, &text)
                .map(|()| text)
                .map_err(|e| e.to_string())
        });
        match result {
            Ok(text) => {
                self.dirty = false;
                // Our own write must not look like an external change; saving
                // also resolves a pending conflict in favor of the editor.
                self.disk_text = Some(text);
                self.disk_conflict = None;
            }
            Err(e) => self.error = Some(format!("save failed: {e}")),
        }
    }

    /// Export the current document as PNG, PDF or SVG (`ext`), after a save
    /// dialog.
    pub fn export(&mut self, ext: &str) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(ext.to_ascii_uppercase(), &[ext])
            .set_file_name(format!("figure.{ext}"))
            .save_file()
        else {
            return;
        };
        self.fonts
            .set_default_family(self.model.page.font_family.clone());
        // Images without an aspect ratio take their file's natural aspect.
        self.fonts.set_image_base_dir(Some(self.base_dir.clone()));
        let raw = self.model.clone();
        let result = figs_core::schema::resolve::resolve(raw)
            .map_err(|e| e.to_string())
            .and_then(|doc| {
                let computed = layout(&doc, &self.fonts);
                let opts = RenderOptions {
                    fonts: Some(&self.fonts),
                    base_dir: &self.base_dir,
                };
                match ext {
                    "pdf" => figs_core::render_pdf(&computed, &opts).map_err(|e| e.to_string()),
                    "svg" => Ok(figs_core::render_svg(&computed, &opts).into_bytes()),
                    _ => figs_core::render_png(&computed, &opts).map_err(|e| e.to_string()),
                }
            })
            .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
        if let Err(e) = result {
            self.error = Some(format!("export failed: {e}"));
        }
    }

    /// Write the lowered document to a `.toml` file (the concrete form of the
    /// current script/model), leaving the in-memory document untouched.
    pub fn export_toml(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("TOML", &["toml"])
            .set_file_name("figure.toml")
            .save_file()
        else {
            return;
        };
        match model::to_toml(&self.model) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&path, text) {
                    self.error = Some(format!("export failed: {e}"));
                }
            }
            Err(e) => self.error = Some(format!("serialize failed: {e}")),
        }
    }

    /// Copy `picked` into the document folder and insert an image node under
    /// `target`, sized from the image's pixel dimensions.
    pub fn insert_image(&mut self, picked: &std::path::Path, target: &str) {
        match model::insert_image(&mut self.model, target, &self.base_dir, picked) {
            Ok(id) => {
                self.selected = Selection::Node(id);
                self.touch();
            }
            Err(e) => self.error = Some(format!("insert image failed: {e}")),
        }
    }

    fn insert_image_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg"])
            .pick_file()
        {
            let target = self
                .selected
                .node_id()
                .map(str::to_string)
                .unwrap_or_else(|| self.model.page.root.clone());
            self.insert_image(&path, &target);
        }
    }

    // ---- chrome ----------------------------------------------------------

    fn menu_bar(&mut self, ctx: &egui::Context) {
        let p = self.theme.palette();
        egui::TopBottomPanel::top("menu")
            .frame(
                egui::Frame::none()
                    .fill(p.bar)
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("New").clicked() {
                        self.new_document();
                    }
                    if ui.button("Open").clicked() {
                        self.open_dialog();
                    }
                    if ui.button("Save").clicked() {
                        self.save();
                    }
                    if ui.button("Save As").clicked() {
                        self.save_as();
                    }
                    for (label, ext) in [
                        ("Export PNG", "png"),
                        ("Export PDF", "pdf"),
                        ("Export SVG", "svg"),
                    ] {
                        if ui.button(label).clicked() {
                            self.export(ext);
                        }
                    }
                    if ui.button("Export TOML").clicked() {
                        self.export_toml();
                    }
                    ui.separator();
                    if ui.button("Insert Image").clicked() {
                        self.insert_image_dialog();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.theme.label()).clicked() {
                            self.theme = self.theme.toggle();
                        }
                        let name = self
                            .path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "untitled.toml".to_string());
                        let dirty = if self.dirty { " \u{25cf}" } else { "" };
                        ui.colored_label(p.dim, format!("{name}{dirty}"));
                    });
                });
            });
    }

    /// Non-modal notice under the menu bar when the open file changed on disk
    /// while the editor has unsaved edits.
    fn disk_banner(&mut self, ctx: &egui::Context) {
        if self.disk_conflict.is_none() {
            return;
        }
        let p = self.theme.palette();
        // The accent tint, composited over the bar color (panels need an opaque
        // fill: the translucent tint alone would show the clear color).
        let a = 1.0 - p.acc_soft.a() as f32 / 255.0;
        let ch = |top: u8, base: u8| (top as f32 + base as f32 * a).round().min(255.0) as u8;
        let fill = egui::Color32::from_rgb(
            ch(p.acc_soft.r(), p.bar.r()),
            ch(p.acc_soft.g(), p.bar.g()),
            ch(p.acc_soft.b(), p.bar.b()),
        );
        egui::TopBottomPanel::top("disk_banner")
            .frame(
                egui::Frame::none()
                    .fill(fill)
                    .inner_margin(egui::Margin::symmetric(12.0, 5.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(p.acc, "\u{21bb}");
                    ui.colored_label(p.tx, "File changed on disk");
                    ui.colored_label(p.dim, "\u{2014} you have unsaved edits.");
                    if ui
                        .button("Reload")
                        .on_hover_text("Discard your edits and load the file from disk")
                        .clicked()
                    {
                        self.accept_disk_version();
                    }
                    if ui
                        .button("Keep mine")
                        .on_hover_text("Keep your edits; saving overwrites the file")
                        .clicked()
                    {
                        self.keep_my_version();
                    }
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let p = self.theme.palette();
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::none()
                    .fill(p.bar)
                    .inner_margin(egui::Margin::symmetric(12.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let n = self.model.nodes.len();
                    let height = match self.model.page.height.fixed() {
                        Some(h) => format!("{h:.0}"),
                        None => match &self.preview {
                            Some(prev) => format!(
                                "auto ({:.1})",
                                prev.computed.page.height_pt / self.model.page.unit.to_pt()
                            ),
                            None => "auto".to_string(),
                        },
                    };
                    ui.colored_label(
                        p.dim,
                        format!(
                            "{:.0} \u{00d7} {} {} \u{00b7} {} nodes",
                            self.model.page.width,
                            height,
                            unit_label(self.model.page.unit),
                            n
                        ),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(err) = &self.error {
                            ui.colored_label(
                                egui::Color32::from_rgb(0xff, 0x6b, 0x6b),
                                format!("\u{26a0} {err}"),
                            );
                        } else {
                            let sel = match &self.selected {
                                Selection::Page => "page".to_string(),
                                Selection::Node(id) => match self.model.nodes.get(id) {
                                    Some(n) => format!("{id} ({})", kind_label(n.kind)),
                                    None => "none".to_string(),
                                },
                            };
                            ui.colored_label(p.dim, format!("sel: {sel}"));
                        }
                    });
                });
            });
    }
}

impl eframe::App for FigsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.theme.apply(ctx);
        self.poll_disk();
        if self.needs_rebuild {
            self.rebuild(ctx);
            self.needs_rebuild = false;
        }

        self.menu_bar(ctx);
        self.disk_banner(ctx);
        self.status_bar(ctx);
        crate::panels::left_rail(self, ctx);
        match self.source_tab {
            SourceTab::Script => crate::panels::script_pane(self, ctx),
            SourceTab::Toml => crate::panels::toml_pane(self, ctx),
        }
        crate::panels::canvas(self, ctx);
    }
}

/// `.figs`/`.rhai` files hold the Rhai script; anything else is TOML.
fn is_script_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("figs") | Some("rhai")
    )
}

pub fn unit_label(u: figs_core::units::Unit) -> &'static str {
    use figs_core::units::Unit::*;
    match u {
        Cm => "cm",
        Mm => "mm",
        In => "in",
        Pt => "pt",
        Px => "px",
    }
}

/// Register CJK/symbol-capable system fonts as egui fallbacks so Japanese text
/// and toolbar glyphs render instead of tofu (□). Latin still uses egui's
/// bundled fonts; missing glyphs fall through to these.
fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontFamily};

    // (key, candidate paths) — first existing path per key is loaded. Ordered so
    // a broad Japanese face comes first, then a symbol face, then a pan-Unicode
    // catch-all; non-macOS fallbacks included for portability.
    let groups: [&[&str]; 3] = [
        &[
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "C:\\Windows\\Fonts\\YuGothM.ttc",
            "C:\\Windows\\Fonts\\meiryo.ttc",
        ],
        &[
            "/System/Library/Fonts/Apple Symbols.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "C:\\Windows\\Fonts\\seguisym.ttf",
        ],
        &[
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/Library/Fonts/Arial Unicode.ttf",
        ],
    ];

    let mut fonts = egui::FontDefinitions::default();
    let mut added: Vec<String> = Vec::new();
    for (gi, paths) in groups.iter().enumerate() {
        if let Some(bytes) = paths.iter().find_map(|p| std::fs::read(p).ok()) {
            let name = format!("fallback{gi}");
            fonts
                .font_data
                .insert(name.clone(), FontData::from_owned(bytes));
            added.push(name);
        }
    }
    if added.is_empty() {
        return;
    }
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        for name in &added {
            list.push(name.clone());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn kind_label(k: figs_core::schema::NodeKind) -> &'static str {
    use figs_core::schema::NodeKind::*;
    match k {
        Column => "column",
        Row => "row",
        Stack => "stack",
        Image => "image",
        Text => "text",
        Rect => "rect",
        Line => "line",
        Arrow => "arrow",
        Ellipse => "ellipse",
        Path => "path",
    }
}
