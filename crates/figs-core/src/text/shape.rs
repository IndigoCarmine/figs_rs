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
    /// Overall block size: max line width × (line count × line height), grown
    /// at the top/bottom when the font's ascent/descent overhangs the first/last
    /// line box (see [`shape`]).
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
    /// Advance width (points).
    pub advance: f32,
    /// Shaping offsets from the pen position (points; `y_offset` positive is
    /// up, as in OpenType). Non-zero mainly for marks/accents.
    pub x_offset: f32,
    pub y_offset: f32,
    /// Byte range of the glyph's cluster in the source `content`, used by
    /// vector backends to map glyphs back to text (PDF copy/paste).
    pub text_range: std::ops::Range<usize>,
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

    // Walk the layout lines directly (rather than `layout_runs`, which skips a
    // line whose baseline lands above the block top) and place baselines the
    // way cosmic-text does: the font's ascent + descent box is centred in each
    // line box (CSS half-leading).
    let mut lines = Vec::new();
    let mut max_w = 0.0f32;
    let mut line_top = 0.0f32;
    let mut ink_top = 0.0f32;
    let mut ink_bottom = 0.0f32;
    // Glyph `start`/`end` are relative to their buffer line (paragraph); `base`
    // turns them into offsets into the whole `content`.
    let mut base = 0usize;
    let mut placed = Vec::new();
    for l in buffer.lines.iter() {
        if let Some(layout) = l.layout_opt().as_ref() {
            placed.extend(layout.iter().map(|ll| (base, ll)));
        }
        base += l.text().len() + l.ending().as_str().len();
    }
    for (base, layout_line) in placed {
        let lh = layout_line.line_height_opt.unwrap_or(line_height);
        let (ascent, descent) = (layout_line.max_ascent, layout_line.max_descent);
        let baseline = line_top + (lh - (ascent + descent)) / 2.0 + ascent;
        if ascent + descent > 0.0 {
            ink_top = ink_top.min(baseline - ascent);
            ink_bottom = ink_bottom.max(baseline + descent);
        }
        line_top += lh;

        max_w = max_w.max(layout_line.w);
        let glyphs = layout_line
            .glyphs
            .iter()
            .map(|g| ShapedGlyph {
                font_id: g.font_id,
                glyph_id: g.glyph_id,
                font_size: g.font_size,
                x: g.x,
                advance: g.w,
                x_offset: g.x_offset * g.font_size,
                y_offset: g.y_offset * g.font_size,
                text_range: base + g.start..base + g.end,
            })
            .collect();
        lines.push(ShapedLine {
            baseline,
            width: layout_line.w,
            glyphs,
        });
    }

    // The block is its stacked line boxes, grown where needed so the first
    // line's ascent and the last line's descent stay inside it: a font whose
    // ascent + descent exceeds the line height (e.g. Segoe UI at the default
    // 1.2) would otherwise poke its accents/descenders out of the box and get
    // clipped at a page edge. Inner line spacing is unchanged.
    let shift = -ink_top; // >= 0
    for line in &mut lines {
        line.baseline += shift;
    }
    let height = line_top.max(ink_bottom) + shift;
    ShapedText {
        size: Size::new(max_w, height),
        lines,
    }
}
