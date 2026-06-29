//! Shaping a [`TextProps`] into positioned glyph runs via cosmic-text.
//!
//! The result, [`ShapedText`], carries both the block size (used by the layout
//! engine) and per-glyph geometry (used by the PNG/PDF backends in later
//! milestones), so shaping happens exactly once per (content, font, width).

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

use crate::geom::Size;
use crate::schema::TextProps;

/// A fully shaped text block, positioned relative to its own top-left origin
/// (points).
#[derive(Debug, Clone, Default)]
pub struct ShapedText {
    /// Overall block size: max line width × (line count × line height).
    pub size: Size,
    pub lines: Vec<ShapedLine>,
}

#[derive(Debug, Clone)]
pub struct ShapedLine {
    /// Baseline y, relative to the block top (points).
    pub baseline: f32,
    /// Line advance width (points).
    pub width: f32,
    pub glyphs: Vec<ShapedGlyph>,
}

#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    pub font_id: cosmic_text::fontdb::ID,
    pub glyph_id: u16,
    pub font_size: f32,
    /// Glyph origin x, relative to the block left (points).
    pub x: f32,
    /// First source character of this glyph's cluster, for PDF ToUnicode
    /// mapping (so the text stays copyable). Space if unknown.
    pub ch: char,
}

/// Shape `text` under `max_width` (points). A non-finite or non-positive width
/// means unconstrained (single line, no wrapping).
pub fn shape(
    system: &mut FontSystem,
    text: &TextProps,
    max_width: f32,
    default_family: Option<&str>,
) -> ShapedText {
    let line_height = (text.font_size * text.line_height).max(1.0);
    let metrics = Metrics::new(text.font_size, line_height);
    let mut buffer = Buffer::new(system, metrics);
    let mut buffer = buffer.borrow_with(system);

    let width = if max_width.is_finite() && max_width > 0.0 {
        Some(max_width)
    } else {
        None
    };
    buffer.set_size(width, None);

    let family = match (&text.font_family, default_family) {
        (Some(f), _) => Family::Name(f),
        (None, Some(d)) => Family::Name(d),
        (None, None) => Family::SansSerif,
    };
    let attrs = Attrs::new()
        .family(family)
        .weight(Weight(text.font_weight.unwrap_or(400)));
    buffer.set_text(&text.content, attrs, Shaping::Advanced);
    buffer.shape_until_scroll(false);

    let mut lines = Vec::new();
    let mut max_w = 0.0f32;
    for run in buffer.layout_runs() {
        max_w = max_w.max(run.line_w);
        let glyphs = run
            .glyphs
            .iter()
            .map(|g| ShapedGlyph {
                font_id: g.font_id,
                glyph_id: g.glyph_id,
                font_size: g.font_size,
                x: g.x,
                ch: run.text[g.start..g.end].chars().next().unwrap_or(' '),
            })
            .collect();
        lines.push(ShapedLine {
            baseline: run.line_y,
            width: run.line_w,
            glyphs,
        });
    }

    let height = lines.len() as f32 * line_height;
    ShapedText {
        size: Size::new(max_w, height),
        lines,
    }
}
