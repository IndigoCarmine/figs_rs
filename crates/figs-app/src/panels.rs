//! The editor panels: the unified tabbed left rail (Outline / Inspector), the
//! live TOML source pane, and the canvas/viewer. This is the "1b" layout from
//! the design mock — a single rail beside the work area to maximise the canvas.

use std::collections::HashSet;
use std::path::PathBuf;

use eframe::egui::{self, Color32, Rect, RichText, Sense, Stroke};
use figs_core::geom::{Color, CrossAxisAlign, Edges, ImageFit, MainAxisAlign, TextAlign};
use figs_core::schema::{NodeKind, RawDocument};
use figs_core::units::Unit;
use figs_core::ComputedLayout;

use crate::app::{kind_label, FigsApp, RailTab, Selection, SourceTab};
use crate::model;
use crate::theme::{kind_badge, page_badge, Palette};

// ---------------------------------------------------------------------------
// Left rail (tabbed Outline / Inspector)
// ---------------------------------------------------------------------------

pub fn left_rail(app: &mut FigsApp, ctx: &egui::Context) {
    let p = app.theme.palette();
    egui::SidePanel::left("rail")
        .resizable(true)
        .default_width(300.0)
        .width_range(220.0..=460.0)
        .frame(egui::Frame::none().fill(p.panel))
        .show(ctx, |ui| {
            // Tab strip.
            egui::Frame::none()
                .fill(p.panel2)
                .inner_margin(egui::Margin::symmetric(6.0, 5.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        tab(ui, app, RailTab::Outline, "Outline");
                        tab(ui, app, RailTab::Inspector, "Inspector");
                    });
                });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    match app.tab {
                        RailTab::Outline => outline(app, ui, &p),
                        RailTab::Inspector => inspector(app, ui, &p),
                    }
                });
        });
}

fn tab(ui: &mut egui::Ui, app: &mut FigsApp, which: RailTab, label: &str) {
    if ui.selectable_label(app.tab == which, label).clicked() {
        app.tab = which;
    }
}

// ---------------------------------------------------------------------------
// Outline tree
// ---------------------------------------------------------------------------

fn visible_rows(doc: &RawDocument, collapsed: &HashSet<String>) -> Vec<(String, usize, bool)> {
    fn rec(
        doc: &RawDocument,
        collapsed: &HashSet<String>,
        id: &str,
        depth: usize,
        out: &mut Vec<(String, usize, bool)>,
    ) {
        let Some(n) = doc.nodes.get(id) else { return };
        let has = !n.children.is_empty();
        out.push((id.to_string(), depth, has));
        if has && !collapsed.contains(id) {
            for c in &n.children {
                rec(doc, collapsed, c, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    rec(doc, collapsed, &doc.page.root, 0, &mut out);
    out
}

fn outline(app: &mut FigsApp, ui: &mut egui::Ui, p: &Palette) {
    let target = app
        .selected
        .node_id()
        .map(str::to_string)
        .unwrap_or_else(|| app.model.page.root.clone());
    let sel_id = app.selected.node_id().map(str::to_string);

    // Toolbar.
    ui.horizontal(|ui| {
        ui.menu_button("\u{ff0b}", |ui| {
            for (k, lbl) in [
                (NodeKind::Column, "Column"),
                (NodeKind::Row, "Row"),
                (NodeKind::Text, "Text"),
                (NodeKind::Rect, "Rect"),
                (NodeKind::Image, "Image"),
            ] {
                if ui.button(lbl).clicked() {
                    let id = model::add_child(&mut app.model, &target, k);
                    app.selected = Selection::Node(id);
                    app.touch();
                    ui.close_menu();
                }
            }
        })
        .response
        .on_hover_text("Add child");

        let has_sel = sel_id.is_some();
        if ui.add_enabled(has_sel, egui::Button::new("\u{2715}")).on_hover_text("Delete").clicked() {
            if let Some(id) = &sel_id {
                model::delete_node(&mut app.model, id);
                app.selected = Selection::Page;
                app.touch();
            }
        }
        if ui.add_enabled(has_sel, egui::Button::new("\u{2191}")).on_hover_text("Move up").clicked() {
            if let Some(id) = &sel_id {
                model::move_sibling(&mut app.model, id, -1);
                app.touch();
            }
        }
        if ui.add_enabled(has_sel, egui::Button::new("\u{2193}")).on_hover_text("Move down").clicked() {
            if let Some(id) = &sel_id {
                model::move_sibling(&mut app.model, id, 1);
                app.touch();
            }
        }
        if ui.add_enabled(has_sel, egui::Button::new("\u{21e4}")).on_hover_text("Outdent").clicked() {
            if let Some(id) = &sel_id {
                model::outdent(&mut app.model, id);
                app.touch();
            }
        }
        if ui.add_enabled(has_sel, egui::Button::new("\u{21e5}")).on_hover_text("Indent").clicked() {
            if let Some(id) = &sel_id {
                model::indent(&mut app.model, id);
                app.touch();
            }
        }
    });
    ui.separator();

    let rows = visible_rows(&app.model, &app.collapsed);
    let mut new_sel: Option<Selection> = None;
    let mut toggle: Option<String> = None;

    for (id, depth, has) in &rows {
        let selected = app.selected.node_id() == Some(id.as_str());
        let kind = app.model.nodes.get(id).map(|n| n.kind);
        let fill = if selected { p.acc_soft } else { Color32::TRANSPARENT };
        let resp = egui::Frame::none()
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(4.0, 3.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(*depth as f32 * 14.0);
                    if *has {
                        let arrow = if app.collapsed.contains(id) {
                            "\u{25b8}"
                        } else {
                            "\u{25be}"
                        };
                        if ui
                            .add(egui::Label::new(RichText::new(arrow).color(p.dim)).sense(Sense::click()))
                            .clicked()
                        {
                            toggle = Some(id.clone());
                        }
                    } else {
                        ui.add_space(14.0);
                    }
                    if let Some(k) = kind {
                        let (c, l) = kind_badge(k);
                        ui.label(RichText::new(l).color(c).monospace().strong());
                    }
                    let name = RichText::new(id);
                    ui.label(if selected { name.strong() } else { name });
                    if let Some(k) = kind {
                        ui.label(RichText::new(kind_label(k)).weak().small());
                    }
                });
            })
            .response;
        if resp.interact(Sense::click()).clicked() {
            new_sel = Some(Selection::Node(id.clone()));
        }
    }

    if let Some(id) = toggle {
        if !app.collapsed.remove(&id) {
            app.collapsed.insert(id);
        }
    }
    if let Some(s) = new_sel {
        app.selected = s;
    }
}

// ---------------------------------------------------------------------------
// Inspector
// ---------------------------------------------------------------------------

fn inspector(app: &mut FigsApp, ui: &mut egui::Ui, _p: &Palette) {
    let families = app.font_families.clone();
    match app.selected.clone() {
        Selection::Page => {
            header(ui, page_badge(), "Page");
            let mut changed = false;
            section(ui, "Geometry");
            changed |= num(ui, "Width", &mut app.model.page.width, 1.0);
            changed |= num(ui, "Height", &mut app.model.page.height, 1.0);
            changed |= unit_combo(ui, "Unit", &mut app.model.page.unit);
            changed |= opt_num(ui, "DPI", &mut app.model.page.dpi, 1.0);
            section(ui, "Appearance");
            changed |= opt_color(ui, "Background", &mut app.model.page.background);
            section(ui, "Typography");
            changed |= font_combo(ui, "Default font", &families, &mut app.model.page.font_family);
            if changed {
                app.touch();
            }
        }
        Selection::Node(id) => {
            let Some(kind) = app.model.nodes.get(&id).map(|n| n.kind) else {
                return;
            };
            header(ui, kind_badge(kind), &id);

            let mut changed = false;
            {
                let node = app.model.nodes.get_mut(&id).unwrap();
                match kind {
                    NodeKind::Text => {
                        section(ui, "Text");
                        changed |= text_multi(ui, "Content", &mut node.content);
                        changed |= font_combo(ui, "Font family", &families, &mut node.font_family);
                        changed |= opt_num(ui, "Font size", &mut node.font_size, 0.5);
                        changed |= weight_combo(ui, "Weight", &mut node.font_weight);
                        changed |= opt_color(ui, "Color", &mut node.color);
                        changed |= align_seg(ui, "Align", &mut node.align);
                        changed |= opt_num(ui, "Line height", &mut node.line_height, 0.05);
                    }
                    NodeKind::Rect => {
                        section(ui, "Rectangle");
                        changed |= opt_color(ui, "Fill", &mut node.fill);
                        changed |= opt_color(ui, "Stroke", &mut node.stroke);
                        changed |= opt_num(ui, "Stroke width", &mut node.stroke_width, 0.25);
                        changed |= opt_num(ui, "Corner radius", &mut node.corner_radius, 0.5);
                    }
                    NodeKind::Image => {
                        section(ui, "Image");
                        changed |= text_single(ui, "Source", &mut node.src);
                        changed |= fit_seg(ui, "Fit", &mut node.fit);
                    }
                    NodeKind::Column | NodeKind::Row => {
                        section(ui, "Container");
                        // Axis is encoded as the node kind; switching rewrites it.
                        let mut axis_row = node.kind == NodeKind::Row;
                        ui.horizontal(|ui| {
                            ui.label("Axis");
                            if ui.selectable_label(!axis_row, "column").clicked() {
                                axis_row = false;
                            }
                            if ui.selectable_label(axis_row, "row").clicked() {
                                axis_row = true;
                            }
                        });
                        let new_kind = if axis_row {
                            NodeKind::Row
                        } else {
                            NodeKind::Column
                        };
                        if new_kind != node.kind {
                            node.kind = new_kind;
                            changed = true;
                        }
                        changed |= num(ui, "Spacing", &mut node.spacing, 0.5);
                        changed |= main_align_combo(ui, "Main align", &mut node.main_axis_alignment);
                        changed |= cross_align_combo(ui, "Cross align", &mut node.cross_axis_alignment);
                    }
                }

                section(ui, "Layout");
                changed |= opt_num(ui, "Flex", &mut node.flex, 0.1);
                changed |= opt_num(ui, "Width", &mut node.width, 1.0);
                changed |= opt_num(ui, "Height", &mut node.height, 1.0);
                if kind == NodeKind::Image {
                    changed |= opt_num(ui, "Aspect ratio", &mut node.aspect_ratio, 0.05);
                }
                changed |= edges(ui, "Margin", &mut node.margin);
                if matches!(kind, NodeKind::Column | NodeKind::Row) {
                    changed |= edges(ui, "Padding", &mut node.padding);
                }
            }
            if changed {
                app.touch();
            }
        }
    }
}

// ---- inspector widgets ----------------------------------------------------

fn header(ui: &mut egui::Ui, badge: (Color32, &str), title: &str) {
    ui.horizontal(|ui| {
        let (c, l) = badge;
        ui.label(RichText::new(l).color(c).monospace().strong());
        ui.label(RichText::new(title).heading().size(15.0));
    });
}

fn section(ui: &mut egui::Ui, txt: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(txt.to_uppercase()).small().strong());
    ui.separator();
}

fn num(ui: &mut egui::Ui, label: &str, v: &mut f32, speed: f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed = ui.add(egui::DragValue::new(v).speed(speed)).changed();
    });
    changed
}

fn opt_num(ui: &mut egui::Ui, label: &str, v: &mut Option<f32>, speed: f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut on = v.is_some();
        if ui.checkbox(&mut on, "").changed() {
            *v = if on { Some(0.0) } else { None };
            changed = true;
        }
        if let Some(x) = v {
            if ui.add(egui::DragValue::new(x).speed(speed)).changed() {
                changed = true;
            }
        }
    });
    changed
}

fn edges(ui: &mut egui::Ui, label: &str, e: &mut Edges) -> bool {
    let mut changed = false;
    let mut v = e.top; // editor treats margin/padding as a single scalar
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(egui::DragValue::new(&mut v).speed(0.5)).changed() {
            *e = Edges::all(v);
            changed = true;
        }
    });
    changed
}

fn col_to_c32(c: Color) -> Color32 {
    let u = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(u(c.r), u(c.g), u(c.b), u(c.a))
}

fn c32_to_col(c: Color32) -> Color {
    Color {
        r: c.r() as f32 / 255.0,
        g: c.g() as f32 / 255.0,
        b: c.b() as f32 / 255.0,
        a: c.a() as f32 / 255.0,
    }
}

fn opt_color(ui: &mut egui::Ui, label: &str, v: &mut Option<Color>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut on = v.is_some();
        if ui.checkbox(&mut on, "").changed() {
            *v = if on { Some(Color::BLACK) } else { None };
            changed = true;
        }
        if let Some(c) = v {
            let mut c32 = col_to_c32(*c);
            if ui.color_edit_button_srgba(&mut c32).changed() {
                *c = c32_to_col(c32);
                changed = true;
            }
            ui.label(RichText::new(c.to_hex()).monospace().small());
        }
    });
    changed
}

fn text_single(ui: &mut egui::Ui, label: &str, v: &mut Option<String>) -> bool {
    let mut s = v.clone().unwrap_or_default();
    let mut changed = false;
    ui.label(label);
    if ui.text_edit_singleline(&mut s).changed() {
        *v = Some(s);
        changed = true;
    }
    changed
}

fn text_multi(ui: &mut egui::Ui, label: &str, v: &mut Option<String>) -> bool {
    let mut s = v.clone().unwrap_or_default();
    let mut changed = false;
    ui.label(label);
    if ui
        .add(egui::TextEdit::multiline(&mut s).desired_rows(2).desired_width(f32::INFINITY))
        .changed()
    {
        *v = Some(s);
        changed = true;
    }
    changed
}

fn weight_combo(ui: &mut egui::Ui, label: &str, v: &mut Option<u16>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let cur = v.unwrap_or(400);
        egui::ComboBox::from_id_salt(label)
            .selected_text(cur.to_string())
            .show_ui(ui, |ui| {
                for w in [400u16, 500, 600, 700] {
                    if ui.selectable_label(cur == w, w.to_string()).clicked() {
                        *v = Some(w);
                        changed = true;
                    }
                }
            });
    });
    changed
}

/// Font family picker: `(default)` (None) followed by the installed families.
fn font_combo(ui: &mut egui::Ui, label: &str, families: &[String], v: &mut Option<String>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let cur = v.as_deref().unwrap_or("(default)").to_string();
        egui::ComboBox::from_id_salt(label)
            .selected_text(cur)
            .show_ui(ui, |ui| {
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    if ui.selectable_label(v.is_none(), "(default)").clicked() {
                        *v = None;
                        changed = true;
                    }
                    for f in families {
                        if ui.selectable_label(v.as_deref() == Some(f.as_str()), f).clicked() {
                            *v = Some(f.clone());
                            changed = true;
                        }
                    }
                });
            });
    });
    changed
}

fn unit_combo(ui: &mut egui::Ui, label: &str, v: &mut Unit) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label)
            .selected_text(crate::app::unit_label(*v))
            .show_ui(ui, |ui| {
                for u in [Unit::Cm, Unit::Mm, Unit::In, Unit::Pt, Unit::Px] {
                    if ui.selectable_label(*v == u, crate::app::unit_label(u)).clicked() {
                        *v = u;
                        changed = true;
                    }
                }
            });
    });
    changed
}

fn align_seg(ui: &mut egui::Ui, label: &str, v: &mut Option<TextAlign>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        for (a, t) in [
            (TextAlign::Left, "left"),
            (TextAlign::Center, "center"),
            (TextAlign::Right, "right"),
            (TextAlign::Justify, "justify"),
        ] {
            if ui.selectable_label(*v == Some(a), t).clicked() {
                *v = Some(a);
                changed = true;
            }
        }
    });
    changed
}

fn fit_seg(ui: &mut egui::Ui, label: &str, v: &mut Option<ImageFit>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        for (a, t) in [
            (ImageFit::Contain, "contain"),
            (ImageFit::Cover, "cover"),
            (ImageFit::Fill, "fill"),
        ] {
            if ui.selectable_label(*v == Some(a), t).clicked() {
                *v = Some(a);
                changed = true;
            }
        }
    });
    changed
}

fn main_align_combo(ui: &mut egui::Ui, label: &str, v: &mut Option<MainAxisAlign>) -> bool {
    let mut changed = false;
    let cur = v.unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label)
            .selected_text(main_label(cur))
            .show_ui(ui, |ui| {
                for a in [
                    MainAxisAlign::Start,
                    MainAxisAlign::Center,
                    MainAxisAlign::End,
                    MainAxisAlign::SpaceBetween,
                    MainAxisAlign::SpaceAround,
                    MainAxisAlign::SpaceEvenly,
                ] {
                    if ui.selectable_label(cur == a, main_label(a)).clicked() {
                        *v = Some(a);
                        changed = true;
                    }
                }
            });
    });
    changed
}

fn cross_align_combo(ui: &mut egui::Ui, label: &str, v: &mut Option<CrossAxisAlign>) -> bool {
    let mut changed = false;
    let cur = v.unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label)
            .selected_text(cross_label(cur))
            .show_ui(ui, |ui| {
                for a in [
                    CrossAxisAlign::Start,
                    CrossAxisAlign::Center,
                    CrossAxisAlign::End,
                    CrossAxisAlign::Stretch,
                ] {
                    if ui.selectable_label(cur == a, cross_label(a)).clicked() {
                        *v = Some(a);
                        changed = true;
                    }
                }
            });
    });
    changed
}

fn main_label(a: MainAxisAlign) -> &'static str {
    match a {
        MainAxisAlign::Start => "start",
        MainAxisAlign::Center => "center",
        MainAxisAlign::End => "end",
        MainAxisAlign::SpaceBetween => "space_between",
        MainAxisAlign::SpaceAround => "space_around",
        MainAxisAlign::SpaceEvenly => "space_evenly",
    }
}

fn cross_label(a: CrossAxisAlign) -> &'static str {
    match a {
        CrossAxisAlign::Start => "start",
        CrossAxisAlign::Center => "center",
        CrossAxisAlign::End => "end",
        CrossAxisAlign::Stretch => "stretch",
    }
}

// ---------------------------------------------------------------------------
// TOML source pane (live two-way editing)
// ---------------------------------------------------------------------------

/// Syntax-highlight colors derived from the active theme.
#[derive(Clone, Copy)]
struct SyntaxColors {
    key: Color32,
    string: Color32,
    num: Color32,
    tbl: Color32,
    punct: Color32,
    tx: Color32,
}

fn syntax_colors(p: &Palette) -> SyntaxColors {
    let d = p.dark_mode;
    SyntaxColors {
        key: if d { crate::theme::hex("#82b0ea") } else { crate::theme::hex("#2f6fed") },
        string: if d { crate::theme::hex("#9ece6a") } else { crate::theme::hex("#3f8f4f") },
        num: if d { crate::theme::hex("#e0a566") } else { crate::theme::hex("#b06a1c") },
        tbl: crate::theme::hex("#b98cff"),
        punct: p.faint,
        tx: p.tx,
    }
}

fn seg(job: &mut egui::text::LayoutJob, s: &str, color: Color32) {
    job.append(
        s,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::monospace(12.5),
            color,
            ..Default::default()
        },
    );
}

/// Build a colored `LayoutJob` for the whole TOML buffer (used as the
/// `TextEdit` layouter so highlighting survives editing).
fn toml_layout_job(text: &str, c: &SyntaxColors) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for line in text.split_inclusive('\n') {
        let (content, nl) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        let trimmed = content.trim_start();
        if trimmed.starts_with('[') {
            seg(&mut job, content, c.tbl);
        } else if let Some(eq) = content.find('=') {
            let (k, rest) = content.split_at(eq);
            seg(&mut job, k, c.key);
            seg(&mut job, "=", c.punct);
            let val = &rest[1..];
            let vt = val.trim_start();
            let vc = if vt.starts_with('"') {
                c.string
            } else if vt.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-' || ch == '[') {
                c.num
            } else {
                c.tx
            };
            seg(&mut job, val, vc);
        } else {
            seg(&mut job, content, c.tx);
        }
        if !nl.is_empty() {
            seg(&mut job, nl, c.tx);
        }
    }
    job
}

/// Which `[page]` / `[nodes.<id>]` block a character offset falls under.
fn block_at_char(text: &str, idx: usize) -> Selection {
    let mut cur = Selection::Page;
    let mut pos = 0usize;
    for line in text.split_inclusive('\n') {
        let len = line.chars().count();
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('[') {
            let head = rest.split(']').next().unwrap_or("").trim();
            cur = if head == "page" {
                Selection::Page
            } else if let Some(id) = head.strip_prefix("nodes.") {
                Selection::Node(id.to_string())
            } else {
                cur
            };
        }
        if idx < pos + len {
            return cur;
        }
        pos += len;
    }
    cur
}

// ---------------------------------------------------------------------------
// TOML autocomplete
// ---------------------------------------------------------------------------

/// A completion request: suggestions plus the character span they replace.
struct Completion {
    /// Char index where the replaced token begins.
    token_start: usize,
    /// The partial token under the caret (used for Esc-suppression).
    token: String,
    items: Vec<String>,
}

/// Which block (and node kind) the caret sits in, read from the buffer text.
#[derive(Clone, Copy)]
enum BlockCtx {
    Page,
    Node(Option<NodeKind>),
}

fn parse_type_value(rest: &str) -> Option<NodeKind> {
    match rest.split('"').nth(1)? {
        "column" => Some(NodeKind::Column),
        "row" => Some(NodeKind::Row),
        "text" => Some(NodeKind::Text),
        "rect" => Some(NodeKind::Rect),
        "image" => Some(NodeKind::Image),
        _ => None,
    }
}

fn block_ctx(buffer: &str, caret: usize) -> BlockCtx {
    let mut ctx = BlockCtx::Page;
    let mut pos = 0usize;
    for line in buffer.split_inclusive('\n') {
        let len = line.chars().count();
        let t = line.trim_start();
        if t.starts_with("[nodes.") {
            ctx = BlockCtx::Node(None);
        } else if t.starts_with("[page]") {
            ctx = BlockCtx::Page;
        } else if let BlockCtx::Node(kind) = &mut ctx {
            if kind.is_none() {
                if let Some(rest) = t.strip_prefix("type") {
                    if let Some(k) = parse_type_value(rest) {
                        *kind = Some(k);
                    }
                }
            }
        }
        if caret < pos + len {
            break;
        }
        pos += len;
    }
    ctx
}

/// Valid key names for the block the caret is in.
fn key_candidates(ctx: BlockCtx) -> Vec<&'static str> {
    match ctx {
        BlockCtx::Page => vec![
            "width", "height", "unit", "dpi", "background", "root", "font_family",
        ],
        BlockCtx::Node(kind) => {
            let mut v = vec!["type"];
            match kind {
                Some(NodeKind::Column) | Some(NodeKind::Row) => v.extend([
                    "children",
                    "spacing",
                    "main_axis_alignment",
                    "cross_axis_alignment",
                ]),
                Some(NodeKind::Text) => v.extend([
                    "content",
                    "font_size",
                    "font_family",
                    "font_weight",
                    "color",
                    "align",
                    "line_height",
                ]),
                Some(NodeKind::Image) => v.extend(["src", "fit"]),
                Some(NodeKind::Rect) => {
                    v.extend(["fill", "stroke", "stroke_width", "corner_radius"])
                }
                None => v.extend([
                    "content", "font_size", "font_family", "font_weight", "color", "align",
                    "line_height", "src", "fit", "fill", "stroke", "stroke_width",
                    "corner_radius", "children", "spacing", "main_axis_alignment",
                    "cross_axis_alignment",
                ]),
            }
            v.extend(["flex", "width", "height", "aspect_ratio", "margin", "padding"]);
            v
        }
    }
}

/// Enum values for a string-valued key (empty if the key is free-form).
fn enum_values(key: &str) -> Vec<&'static str> {
    match key {
        "type" => vec!["column", "row", "text", "rect", "image"],
        "fit" => vec!["contain", "cover", "fill"],
        "align" => vec!["left", "center", "right", "justify"],
        "unit" => vec!["cm", "mm", "in", "pt", "px"],
        "main_axis_alignment" => vec![
            "start",
            "center",
            "end",
            "space_between",
            "space_around",
            "space_evenly",
        ],
        "cross_axis_alignment" => vec!["start", "center", "end", "stretch"],
        _ => vec![],
    }
}

/// Context-aware suggestions for the caret position, or `None` when there's
/// nothing useful to offer.
fn compute_completion(buffer: &str, caret: usize) -> Option<Completion> {
    let chars: Vec<char> = buffer.chars().collect();
    let caret = caret.min(chars.len());
    let line_start = chars[..caret]
        .iter()
        .rposition(|&c| c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let prefix: Vec<char> = chars[line_start..caret].to_vec();
    let lead = prefix.iter().take_while(|c| c.is_whitespace()).count();

    // Value context: an '=' precedes the caret on this line.
    if let Some(eq) = prefix.iter().position(|&c| c == '=') {
        let key: String = prefix[lead..eq].iter().collect::<String>().trim().to_string();
        let after = &prefix[eq + 1..];
        // Only complete while inside an open string literal (odd quote count).
        if after.iter().filter(|&&c| c == '"').count() % 2 != 1 {
            return None;
        }
        let q = after.iter().rposition(|&c| c == '"').unwrap();
        let partial: String = after[q + 1..].iter().collect();
        let items: Vec<String> = enum_values(&key)
            .into_iter()
            .filter(|v| v.starts_with(&partial) && *v != partial)
            .map(String::from)
            .collect();
        if items.is_empty() {
            return None;
        }
        return Some(Completion {
            token_start: line_start + eq + 1 + q + 1,
            token: partial,
            items,
        });
    }

    // Header line: nothing to complete.
    if prefix[lead..].first() == Some(&'[') {
        return None;
    }

    // Key context: the partial key word under the caret.
    let partial: String = prefix[lead..].iter().collect();
    if partial.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    let items: Vec<String> = key_candidates(block_ctx(buffer, caret))
        .into_iter()
        .filter(|k| k.starts_with(&partial) && *k != partial)
        .map(String::from)
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(Completion {
        token_start: line_start + lead,
        token: partial,
        items,
    })
}

/// Insert the highlighted suggestion, reparse, and move the caret past it.
fn apply_completion(
    app: &mut FigsApp,
    ctx: &egui::Context,
    id: egui::Id,
    c: &Completion,
    caret: usize,
) {
    let text = c.items[app.completion_index.min(c.items.len() - 1)].clone();
    let mut chars: Vec<char> = app.toml_buffer.chars().collect();
    let end = caret.min(chars.len());
    let start = c.token_start.min(end);
    chars.splice(start..end, text.chars());
    app.toml_buffer = chars.into_iter().collect();
    let new_caret = start + text.chars().count();

    match figs_core::schema::parse::parse_str(&app.toml_buffer) {
        Ok(raw) => {
            app.model = raw;
            app.dirty = true;
            app.needs_rebuild = true;
            app.toml_error = None;
        }
        Err(e) => app.toml_error = Some(e.to_string()),
    }

    let mut st = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(new_caret))));
    st.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
    app.completion_index = 0;
    app.completion_suppress = None;
}

pub fn toml_pane(app: &mut FigsApp, ctx: &egui::Context) {
    let p = app.theme.palette();
    // Re-sync the buffer from the model after structured edits, but never while
    // the user is typing here (direct edits leave `regen_toml` false).
    if app.regen_toml {
        app.toml_buffer = model::to_toml(&app.model).unwrap_or_default();
        app.regen_toml = false;
    }

    egui::SidePanel::left("source")
        .resizable(true)
        .default_width(400.0)
        .width_range(280.0..=720.0)
        .frame(egui::Frame::none().fill(p.panel))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(p.panel2)
                .inner_margin(egui::Margin::symmetric(11.0, 6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let name = app
                            .path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "untitled.toml".to_string());
                        ui.label(RichText::new(name).monospace().strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            source_tabs(app, ui);
                        });
                    });
                });
            ui.separator();

            // Parse-error banner (distinct from the resolve/render error in the
            // status bar); the canvas keeps the last valid preview.
            if let Some(err) = app.toml_error.clone() {
                egui::Frame::none()
                    .fill(Color32::from_rgb(0x3a, 0x1d, 0x1d))
                    .inner_margin(egui::Margin::symmetric(10.0, 5.0))
                    .show(ui, |ui| {
                        ui.colored_label(
                            Color32::from_rgb(0xff, 0x8a, 0x8a),
                            format!("\u{26a0} {err}"),
                        );
                    });
            }

            let editor_id = egui::Id::new("figs_toml_editor");

            // Completion keyboard handling must run BEFORE the editor consumes
            // the keys, using last frame's caret from the stored state.
            let focused = ctx.memory(|m| m.has_focus(editor_id));
            if focused {
                let pre_caret = egui::widgets::text_edit::TextEditState::load(ctx, editor_id)
                    .and_then(|s| s.cursor.char_range())
                    .map(|r| r.primary.index);
                let mut comp = pre_caret.and_then(|c| compute_completion(&app.toml_buffer, c));
                if let Some(c) = &comp {
                    if app.completion_suppress.as_deref() == Some(c.token.as_str()) {
                        comp = None;
                    } else {
                        app.completion_suppress = None;
                    }
                }
                if let (Some(c), Some(caret)) = (&comp, pre_caret) {
                    let n = c.items.len();
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                        app.completion_index = (app.completion_index + 1) % n;
                    }
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                        app.completion_index = (app.completion_index + n - 1) % n;
                    }
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                        app.completion_suppress = Some(c.token.clone());
                    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                        let c = comp.take().unwrap();
                        apply_completion(app, ctx, editor_id, &c, caret);
                    }
                }
            }

            let colors = syntax_colors(&p);
            let mut layouter = |ui: &egui::Ui, text: &str, wrap: f32| {
                let mut job = toml_layout_job(text, &colors);
                job.wrap.max_width = wrap;
                ui.fonts(|f| f.layout_job(job))
            };

            let out = egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::TextEdit::multiline(&mut app.toml_buffer)
                        .id(editor_id)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(30)
                        .layouter(&mut layouter)
                        .show(ui)
                })
                .inner;

            // Live parse: valid TOML drives the model; invalid keeps last good.
            if out.response.changed() {
                match figs_core::schema::parse::parse_str(&app.toml_buffer) {
                    Ok(raw) => {
                        app.model = raw;
                        app.dirty = true;
                        app.needs_rebuild = true;
                        app.toml_error = None;
                    }
                    Err(e) => app.toml_error = Some(e.to_string()),
                }
            }

            if out.response.has_focus() {
                if let Some(cr) = out.cursor_range {
                    let caret = cr.primary.ccursor.index;

                    // Caret follows selection (when the TOML is valid).
                    if app.toml_error.is_none() {
                        let sel = block_at_char(&app.toml_buffer, caret);
                        if sel != app.selected {
                            app.selected = sel;
                        }
                    }

                    // Completion popup, anchored under the caret. Click inserts.
                    let mut comp = compute_completion(&app.toml_buffer, caret);
                    if let Some(c) = &comp {
                        if app.completion_suppress.as_deref() == Some(c.token.as_str()) {
                            comp = None;
                        }
                    }
                    if let Some(c) = comp {
                        app.completion_index = app.completion_index.min(c.items.len() - 1);
                        let caret_rect = out.galley.pos_from_cursor(&cr.primary);
                        let pos = out.galley_pos + caret_rect.left_bottom().to_vec2();
                        let mut chosen: Option<usize> = None;
                        egui::Area::new(editor_id.with("completion"))
                            .order(egui::Order::Foreground)
                            .fixed_pos(pos)
                            .show(ctx, |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.set_max_width(240.0);
                                    for (i, item) in c.items.iter().enumerate().take(8) {
                                        if ui
                                            .selectable_label(
                                                i == app.completion_index,
                                                RichText::new(item).monospace(),
                                            )
                                            .clicked()
                                        {
                                            chosen = Some(i);
                                        }
                                    }
                                    ui.label(
                                        RichText::new("Tab insert \u{00b7} \u{2191}\u{2193} \u{00b7} Esc")
                                            .small()
                                            .color(p.faint),
                                    );
                                });
                            });
                        if let Some(i) = chosen {
                            app.completion_index = i;
                            apply_completion(app, ctx, editor_id, &c, caret);
                        }
                    }
                }
            }
        });
}

fn source_tabs(app: &mut FigsApp, ui: &mut egui::Ui) {
    // right_to_left layout: the first label added sits furthest right.
    if ui
        .selectable_label(app.source_tab == SourceTab::Toml, "TOML")
        .clicked()
    {
        app.source_tab = SourceTab::Toml;
    }
    if ui
        .selectable_label(app.source_tab == SourceTab::Script, "Script")
        .clicked()
    {
        app.source_tab = SourceTab::Script;
    }
}

/// The Rhai script pane (the `.figs` source of truth). "Evaluate" lowers the
/// script to the model, refreshing the TOML pane, inspector and preview.
pub fn script_pane(app: &mut FigsApp, ctx: &egui::Context) {
    let p = app.theme.palette();
    egui::SidePanel::left("source")
        .resizable(true)
        .default_width(400.0)
        .width_range(280.0..=720.0)
        .frame(egui::Frame::none().fill(p.panel))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(p.panel2)
                .inner_margin(egui::Margin::symmetric(11.0, 6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let name = app
                            .path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "untitled.figs".to_string());
                        ui.label(RichText::new(name).monospace().strong());
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| source_tabs(app, ui),
                        );
                    });
                });
            ui.separator();

            ui.horizontal(|ui| {
                let run = ui
                    .button(RichText::new("\u{25b6} Evaluate").strong())
                    .on_hover_text(
                        "Run the script (Ctrl+Enter): refresh preview, TOML and inspector",
                    )
                    .clicked();
                ui.label(
                    RichText::new("Rhai \u{2192} TOML \u{00b7} GUI")
                        .small()
                        .color(p.dim),
                );
                let ctrl_enter =
                    ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
                if run || ctrl_enter {
                    app.evaluate_script();
                }
            });

            if let Some(err) = app.script_error.clone() {
                egui::Frame::none()
                    .fill(Color32::from_rgb(0x3a, 0x1d, 0x1d))
                    .inner_margin(egui::Margin::symmetric(10.0, 5.0))
                    .show(ui, |ui| {
                        ui.colored_label(
                            Color32::from_rgb(0xff, 0x8a, 0x8a),
                            format!("\u{26a0} {err}"),
                        );
                    });
            }

            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut app.script_buffer)
                            .id(egui::Id::new("figs_script_editor"))
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .desired_rows(30),
                    );
                });
        });
}

// ---------------------------------------------------------------------------
// Canvas / viewer
// ---------------------------------------------------------------------------

/// The deepest (last-painted) node whose rect contains the page-point (px,py).
fn node_at_point(computed: &ComputedLayout, px: f32, py: f32) -> Option<String> {
    computed
        .nodes
        .iter()
        .rev()
        .find(|n| {
            let r = n.rect;
            px >= r.x && px <= r.x + r.w && py >= r.y && py <= r.y + r.h
        })
        .map(|n| n.id.clone())
}

fn is_image_path(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("png" | "jpg" | "jpeg")
    )
}

pub fn canvas(app: &mut FigsApp, ctx: &egui::Context) {
    let p = app.theme.palette();
    // Drag-and-drop state for this frame.
    let dropped: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|f| f.path.clone())
            .filter(|p| is_image_path(p))
            .collect()
    });
    let hovering_files = ctx.input(|i| !i.raw.hovered_files.is_empty());
    let pointer = ctx.input(|i| i.pointer.latest_pos());

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(p.canvas))
        .show(ctx, |ui| {
            let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click());
            let area = resp.rect;

            let mut new_sel: Option<Selection> = None;
            // (file path, target container id) pairs to insert after drawing.
            let mut drops: Vec<(PathBuf, String)> = Vec::new();
            let root = app.model.page.root.clone();

            if let Some(prev) = app.preview.as_ref() {
                let pw = prev.computed.page.width_pt.max(1.0);
                let ph = prev.computed.page.height_pt.max(1.0);
                let pad = 24.0;
                let scale = ((area.width() - pad * 2.0) / pw)
                    .min((area.height() - pad * 2.0) / ph)
                    .max(0.01);
                let dw = pw * scale;
                let dh = ph * scale;
                let origin = egui::pos2(area.center().x - dw / 2.0, area.center().y - dh / 2.0);
                let img_rect = Rect::from_min_size(origin, egui::vec2(dw, dh));
                let to_point = |pos: egui::Pos2| ((pos.x - origin.x) / scale, (pos.y - origin.y) / scale);

                // Soft drop shadow + the rendered page.
                painter.rect_filled(
                    img_rect.translate(egui::vec2(0.0, 6.0)).expand(2.0),
                    4.0,
                    Color32::from_black_alpha(60),
                );
                painter.image(
                    prev.texture.id(),
                    img_rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                painter.rect_stroke(img_rect, 0.0, Stroke::new(1.0, p.line));

                // Selection outline.
                if let Some(id) = app.selected.node_id() {
                    if let Some(node) = prev.computed.nodes.iter().find(|n| n.id == id) {
                        let r = node.rect;
                        let sr = Rect::from_min_size(
                            egui::pos2(origin.x + r.x * scale, origin.y + r.y * scale),
                            egui::vec2(r.w * scale, r.h * scale),
                        );
                        painter.rect_stroke(sr, 2.0, Stroke::new(2.0, p.acc));
                    }
                }

                // Click selects the deepest node containing the point.
                if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        new_sel = Some(if img_rect.contains(pos) {
                            let (px, py) = to_point(pos);
                            match node_at_point(&prev.computed, px, py) {
                                Some(id) => Selection::Node(id),
                                None => Selection::Page,
                            }
                        } else {
                            Selection::Page
                        });
                    }
                }

                // Resolve the drop target: the container under the cursor (or the
                // parent of a leaf, else the page root).
                let drop_target = |pos: Option<egui::Pos2>| -> String {
                    let Some(pos) = pos.filter(|pp| img_rect.contains(*pp)) else {
                        return root.clone();
                    };
                    let (px, py) = to_point(pos);
                    match node_at_point(&prev.computed, px, py) {
                        Some(id) if model::is_container(&app.model, &id) => id,
                        Some(id) => model::parent_of(&app.model, &id).unwrap_or_else(|| root.clone()),
                        None => root.clone(),
                    }
                };
                if !dropped.is_empty() {
                    let target = drop_target(pointer);
                    for path in &dropped {
                        drops.push((path.clone(), target.clone()));
                    }
                }

                // Hover hint while dragging files over the canvas.
                if hovering_files && pointer.is_some_and(|pp| area.contains(pp)) {
                    painter.rect_filled(area, 0.0, Color32::from_black_alpha(120));
                    painter.text(
                        area.center(),
                        egui::Align2::CENTER_CENTER,
                        "Drop image to insert",
                        egui::FontId::proportional(20.0),
                        p.acc,
                    );
                }
            } else {
                painter.text(
                    area.center(),
                    egui::Align2::CENTER_CENTER,
                    "no preview",
                    egui::FontId::proportional(16.0),
                    p.dim,
                );
                // Allow dropping even before a preview exists; add under root.
                for path in &dropped {
                    drops.push((path.clone(), root.clone()));
                }
            }

            if let Some(s) = new_sel {
                app.selected = s;
            }
            for (path, target) in drops {
                app.insert_image(&path, &target);
            }
        });
}
