//! Dark / light palettes mirroring the `Figs Editor` design mock, plus the
//! per-node-kind accent colors used by the outline and inspector.

use eframe::egui::{self, Color32};
use figs_core::schema::NodeKind;

/// Parse `#rrggbb` into a [`Color32`]. Falls back to magenta on bad input so
/// mistakes are visible rather than silent.
pub fn hex(s: &str) -> Color32 {
    let h = s.strip_prefix('#').unwrap_or(s);
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    if h.len() == 6 {
        Color32::from_rgb(b(0), b(2), b(4))
    } else {
        Color32::from_rgb(255, 0, 255)
    }
}

fn hexa(s: &str, alpha: u8) -> Color32 {
    let c = hex(s);
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

/// Which color scheme the editor is using.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

/// A resolved set of colors for one theme.
pub struct Palette {
    pub bar: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub line: Color32,
    pub tx: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub acc: Color32,
    pub acc_soft: Color32,
    pub canvas: Color32,
    pub input: Color32,
    pub input_line: Color32,
    pub hover: Color32,
    pub dark_mode: bool,
}

impl Theme {
    pub fn toggle(self) -> Theme {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "\u{2600} Dark",
            Theme::Light => "\u{263e} Light",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Theme::Dark => Palette {
                bar: hex("#1b1e25"),
                panel: hex("#1e2229"),
                panel2: hex("#23272f"),
                line: hex("#2c313b"),
                tx: hex("#d6dae1"),
                dim: hex("#8a929e"),
                faint: hex("#5b626d"),
                acc: hex("#6ea8fe"),
                acc_soft: hexa("#6ea8fe", 41),
                canvas: hex("#0e1014"),
                input: hex("#171a20"),
                input_line: hex("#343b46"),
                hover: Color32::from_white_alpha(13),
                dark_mode: true,
            },
            Theme::Light => Palette {
                bar: hex("#f5f6f9"),
                panel: hex("#fbfbfd"),
                panel2: hex("#f0f1f5"),
                line: hex("#dfe2e8"),
                tx: hex("#2b2f36"),
                dim: hex("#737b86"),
                faint: hex("#a4aab4"),
                acc: hex("#2f6fed"),
                acc_soft: hexa("#2f6fed", 31),
                canvas: hex("#cdd1d8"),
                input: hex("#ffffff"),
                input_line: hex("#d5d9e0"),
                hover: Color32::from_black_alpha(12),
                dark_mode: false,
            },
        }
    }

    /// Push this palette into the egui visuals so panels, inputs and selection
    /// colors all match the design.
    pub fn apply(self, ctx: &egui::Context) {
        let p = self.palette();
        let mut v = if p.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.override_text_color = Some(p.tx);
        v.panel_fill = p.panel;
        v.window_fill = p.panel;
        v.extreme_bg_color = p.input;
        v.faint_bg_color = p.panel2;
        v.selection.bg_fill = p.acc_soft;
        v.selection.stroke = egui::Stroke::new(1.0, p.acc);
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, p.line);
        v.widgets.inactive.bg_fill = p.input;
        v.widgets.inactive.weak_bg_fill = p.input;
        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, p.input_line);
        v.widgets.hovered.weak_bg_fill = p.hover;
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, p.acc);
        v.widgets.active.bg_stroke = egui::Stroke::new(1.0, p.acc);
        v.hyperlink_color = p.acc;
        ctx.set_visuals(v);
    }
}

/// Accent color + single-letter badge for a node kind (matches the mock).
pub fn kind_badge(kind: NodeKind) -> (Color32, &'static str) {
    match kind {
        NodeKind::Column | NodeKind::Row => (hex("#8a929e"), "C"),
        NodeKind::Text => (hex("#6ea8fe"), "T"),
        NodeKind::Rect => (hex("#3ddc97"), "R"),
        NodeKind::Image => (hex("#ffb454"), "I"),
    }
}

/// Accent color + badge for the page pseudo-node.
pub fn page_badge() -> (Color32, &'static str) {
    (hex("#b98cff"), "P")
}
