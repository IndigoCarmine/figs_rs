//! The editor application: state, the live render bridge, and the top-level
//! egui layout (menu bar, left rail, TOML source pane, canvas, status bar).

use std::path::PathBuf;

use eframe::egui;
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
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RailTab {
    Outline,
    Inspector,
}

impl FigsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        let model = model::default_document();
        let toml_buffer = model::to_toml(&model).unwrap_or_default();
        let fonts = FontStore::new();
        let font_families = fonts.families();
        FigsApp {
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
        }
    }

    /// Mark the model dirty and schedule a preview rebuild + TOML re-sync.
    pub fn touch(&mut self) {
        self.dirty = true;
        self.needs_rebuild = true;
        self.regen_toml = true;
    }

    /// Re-resolve, lay out and rasterize the current model into a texture. On
    /// any error the previous preview is kept and `error` is set.
    fn rebuild(&mut self, ctx: &egui::Context) {
        self.fonts
            .set_default_family(self.model.page.font_family.clone());
        let raw = self.model.clone();
        let doc = match figs_core::schema::resolve::resolve(raw) {
            Ok(d) => d,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
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
        self.model = model::default_document();
        self.path = None;
        self.selected = Selection::Page;
        self.dirty = false;
        self.error = None;
        self.toml_error = None;
        self.needs_rebuild = true;
        self.regen_toml = true;
    }

    pub fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("TOML", &["toml"])
            .pick_file()
        {
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| figs_core::schema::parse::parse_str(&s).map_err(|e| e.to_string()))
            {
                Ok(doc) => {
                    self.base_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                    self.model = doc;
                    self.path = Some(path);
                    self.selected = Selection::Page;
                    self.dirty = false;
                    self.error = None;
                    self.toml_error = None;
                    self.needs_rebuild = true;
                    self.regen_toml = true;
                }
                Err(e) => self.error = Some(format!("open failed: {e}")),
            }
        }
    }

    pub fn save(&mut self) {
        match self.path.clone() {
            Some(path) => self.write_to(&path),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("TOML", &["toml"])
            .set_file_name("figure.toml")
            .save_file()
        {
            self.write_to(&path);
            self.base_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            self.path = Some(path);
        }
    }

    fn write_to(&mut self, path: &std::path::Path) {
        match model::to_toml(&self.model) {
            Ok(text) => match std::fs::write(path, text) {
                Ok(()) => self.dirty = false,
                Err(e) => self.error = Some(format!("save failed: {e}")),
            },
            Err(e) => self.error = Some(format!("serialize failed: {e}")),
        }
    }

    pub fn export_png(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name("figure.png")
            .save_file()
        else {
            return;
        };
        self.fonts
            .set_default_family(self.model.page.font_family.clone());
        let raw = self.model.clone();
        let result = figs_core::schema::resolve::resolve(raw)
            .map_err(|e| e.to_string())
            .and_then(|doc| {
                let computed = layout(&doc, &self.fonts);
                let opts = RenderOptions {
                    fonts: Some(&self.fonts),
                    base_dir: &self.base_dir,
                };
                figs_core::render_png(&computed, &opts).map_err(|e| e.to_string())
            })
            .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
        if let Err(e) = result {
            self.error = Some(format!("export failed: {e}"));
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
            .frame(egui::Frame::none().fill(p.bar).inner_margin(egui::Margin::symmetric(8.0, 6.0)))
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
                    if ui.button("Export PNG").clicked() {
                        self.export_png();
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

    fn status_bar(&mut self, ctx: &egui::Context) {
        let p = self.theme.palette();
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::none().fill(p.bar).inner_margin(egui::Margin::symmetric(12.0, 4.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let n = self.model.nodes.len();
                    ui.colored_label(
                        p.dim,
                        format!(
                            "{:.0} \u{00d7} {:.0} {} \u{00b7} {} nodes",
                            self.model.page.width,
                            self.model.page.height,
                            unit_label(self.model.page.unit),
                            n
                        ),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(err) = &self.error {
                            ui.colored_label(egui::Color32::from_rgb(0xff, 0x6b, 0x6b), format!("\u{26a0} {err}"));
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
        if self.needs_rebuild {
            self.rebuild(ctx);
            self.needs_rebuild = false;
        }

        self.menu_bar(ctx);
        self.status_bar(ctx);
        crate::panels::left_rail(self, ctx);
        crate::panels::toml_pane(self, ctx);
        crate::panels::canvas(self, ctx);
    }
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
            fonts.font_data.insert(name.clone(), FontData::from_owned(bytes));
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
        Image => "image",
        Text => "text",
        Rect => "rect",
    }
}
