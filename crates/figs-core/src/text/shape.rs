//! Shaping a [`TextProps`] into positioned glyph runs via cosmic-text.
//!
//! The result, [`ShapedText`], carries both the block size (used by the layout
//! engine) and per-glyph geometry (used by every backend), so shaping happens
//! exactly once per (content, style, width) and PNG/PDF/SVG draw identical
//! glyphs.
//!
//! Pipeline per paragraph (`\n`-separated):
//!
//! 1. **Styled runs** — the node style and its spans flattened into
//!    [`StyledRun`]s with every inherited value resolved ([`styled_runs`]).
//! 2. **Font runs** — each character goes to the first family of its run's
//!    ordered list that covers it (see [`super::fonts`]); the result is a
//!    cosmic-text `AttrsList` (family/weight/style per range, the run index
//!    as `metadata`, the run's size as per-span `Metrics`).
//! 3. **Shaping** — cosmic-text `ShapeLine` (rustybuzz, bidi, UAX #14 word
//!    boundaries).
//! 4. **Tracking** — `letter_spacing` is added to the advance of the last
//!    glyph of every cluster *before* line breaking, so wrapping sees it.
//! 5. **Line breaking** — cosmic-text's `Wrap::WordOrGlyph`.
//! 6. **Line metrics & decorations** — computed here (see [`shape`]).

use cosmic_text::{
    Attrs, AttrsList, CacheKeyFlags, Family, FontSystem, Metrics, ShapeBuffer, ShapeLine, Shaping,
    Weight, Wrap,
};

use crate::geom::{Color, Size};
use crate::schema::span::{parse_family_list, FontStyle, SCRIPT_SCALE};
use crate::schema::TextProps;

use super::fonts::{attaches_to_previous, FaceChoice, FontCache};

/// A fully shaped text block, positioned relative to its own top-left origin
/// (points).
#[derive(Debug, Clone, Default)]
pub struct ShapedText {
    /// Overall block size: max line width × the stacked line boxes, grown at
    /// the top/bottom when glyphs overhang the first/last line box (see
    /// [`shape`]).
    pub size: Size,
    pub lines: Vec<ShapedLine>,
    /// Underline and line-through bars, in paint-independent block
    /// coordinates (see [`Decoration`]).
    pub decorations: Vec<Decoration>,
}

#[derive(Debug, Clone)]
pub struct ShapedLine {
    /// Baseline y, relative to the block top (points).
    pub baseline: f32,
    /// Line advance width (points), without the trailing tracking of its
    /// last character — what alignment centres/right-aligns.
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
    /// Advance width (points), including tracking.
    pub advance: f32,
    /// Offsets from the pen position (points; `y_offset` positive is up, as
    /// in OpenType): the shaper's offsets (marks/accents) **plus** the run's
    /// baseline shift (super/subscript), so a backend drawing at
    /// `baseline - y_offset` places scripts correctly with no extra logic.
    pub x_offset: f32,
    pub y_offset: f32,
    /// Byte range of the glyph's cluster in the source `content`, used by
    /// vector backends to map glyphs back to text (PDF copy/paste).
    pub text_range: std::ops::Range<usize>,
    /// Fill colour of the glyph's run.
    pub color: Color,
    /// Draw slanted by [`crate::schema::span::SYNTHETIC_ITALIC_DEG`] (italic
    /// requested, family has no italic face). The shear is about the pen
    /// position: `x += tan(angle) · height above the baseline`.
    pub synthetic_italic: bool,
    /// Index of the styled run (see [`styled_runs`]) the glyph belongs to.
    /// Consecutive glyphs of one run share colour, size and shift.
    pub run: usize,
}

/// What a [`Decoration`] bar is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    Underline,
    Strike,
}

/// A horizontal bar (underline / line-through) belonging to one line.
#[derive(Debug, Clone)]
pub struct Decoration {
    pub kind: DecorationKind,
    /// Index into [`ShapedText::lines`] (the line's alignment offset applies).
    pub line: usize,
    /// Left edge, relative to the line start (points).
    pub x: f32,
    pub width: f32,
    /// Centre of the bar, relative to the block top (points, y down).
    pub y: f32,
    pub thickness: f32,
    pub color: Color,
}

/// Fully resolved style of a run of text.
#[derive(Debug, Clone, PartialEq)]
pub struct RunStyle {
    /// Ordered family list (never empty: the default is `sans-serif`).
    pub families: Vec<String>,
    /// Glyph size (points), after the script scale.
    pub font_size: f32,
    /// The size line metrics are computed with: `font_size` for ordinary
    /// text, the pre-scale size for scripts (so a superscript never changes
    /// line spacing).
    pub base_size: f32,
    pub weight: u16,
    pub italic: bool,
    pub color: Color,
    pub underline: bool,
    pub strike: bool,
    /// Baseline shift, points, positive = up.
    pub shift: f32,
    /// Tracking, points, added after every cluster.
    pub tracking: f32,
}

/// A byte range of the content with one resolved style.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledRun {
    pub range: std::ops::Range<usize>,
    pub style: RunStyle,
}

/// The node style and its spans as contiguous runs covering all of
/// `text.content` (in order). `default_family` is the store/page default
/// used when the node names no family.
pub fn styled_runs(text: &TextProps, default_family: Option<&str>) -> Vec<StyledRun> {
    let mut node_families = text
        .font_family
        .as_deref()
        .or(default_family)
        .map(parse_family_list)
        .unwrap_or_default();
    if node_families.is_empty() {
        node_families.push("sans-serif".to_string());
    }
    let node = RunStyle {
        families: node_families,
        font_size: text.font_size,
        base_size: text.font_size,
        weight: text.font_weight.unwrap_or(400),
        italic: text.font_style == FontStyle::Italic,
        color: text.color,
        underline: text.underline,
        strike: text.strike,
        shift: 0.0,
        tracking: text.letter_spacing * text.font_size,
    };
    let len = text.content.len();
    let mut runs = Vec::with_capacity(text.spans.len() * 2 + 1);
    let mut at = 0usize;
    let mut push = |range: std::ops::Range<usize>, style: RunStyle| {
        if range.start < range.end {
            runs.push(StyledRun { range, style });
        }
    };
    for span in &text.spans {
        let (start, end) = (span.range.start.min(len), span.range.end.min(len));
        if start < at || start >= end {
            continue; // overlapping or empty: ignore (resolve never makes these)
        }
        push(at..start, node.clone());
        let s = &span.style;
        let base_size = s.font_size.unwrap_or(text.font_size);
        let font_size = if s.script.is_some() {
            base_size * SCRIPT_SCALE
        } else {
            base_size
        };
        let shift_em = s
            .baseline_shift
            .or(s.script.map(|sc| sc.shift()))
            .unwrap_or(0.0);
        let families = s
            .font_family
            .as_deref()
            .map(parse_family_list)
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| node.families.clone());
        push(
            start..end,
            RunStyle {
                families,
                font_size,
                base_size,
                weight: s.font_weight.unwrap_or(node.weight),
                italic: s
                    .font_style
                    .map_or(node.italic, |st| st == FontStyle::Italic),
                color: s.color.unwrap_or(node.color),
                underline: s.underline.unwrap_or(node.underline),
                strike: s.strike.unwrap_or(node.strike),
                shift: shift_em * base_size,
                tracking: s.letter_spacing.unwrap_or(text.letter_spacing) * font_size,
            },
        );
        at = end;
    }
    push(at..len, node.clone());
    if runs.is_empty() {
        // Empty content: one (empty) run so lines still get the node metrics.
        runs.push(StyledRun {
            range: 0..0,
            style: node,
        });
    }
    runs
}

/// `content` split into paragraphs at `\n` (a preceding `\r` is dropped),
/// each with its byte offset.
fn paragraphs(content: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut base = 0usize;
    for piece in content.split('\n') {
        out.push((base, piece.strip_suffix('\r').unwrap_or(piece)));
        base += piece.len() + 1;
    }
    out
}

/// The cosmic-text attributes of one font run.
fn run_attrs<'a>(
    choice: Option<&'a FaceChoice>,
    style: &RunStyle,
    run: usize,
    line_height: f32,
) -> Attrs<'a> {
    let attrs = Attrs::new().metadata(run).metrics(Metrics::new(
        style.font_size,
        (style.base_size * line_height).max(1.0),
    ));
    match choice {
        Some(c) => attrs
            .family(Family::Name(&c.family))
            .weight(Weight(c.weight))
            .style(c.style)
            .stretch(c.stretch)
            .cache_key_flags(if c.synthetic_italic {
                CacheKeyFlags::FAKE_ITALIC
            } else {
                CacheKeyFlags::empty()
            }),
        // No listed family is installed: cosmic-text's own fallback.
        None => attrs.family(Family::SansSerif).weight(Weight(style.weight)),
    }
}

/// Assign every character of `para` (at byte offset `base` in the content) a
/// face: the first family of its run's list covering it.
fn attrs_list(
    system: &mut FontSystem,
    cache: &mut FontCache,
    para: &str,
    base: usize,
    runs: &[StyledRun],
    line_height: f32,
) -> AttrsList {
    let end = base + para.len();
    let first = runs
        .iter()
        .position(|r| r.range.end > base)
        .unwrap_or(0)
        .min(runs.len().saturating_sub(1));
    let default_choice = runs.get(first).and_then(|r| {
        let st = &r.style;
        st.families
            .iter()
            .map(String::as_str)
            .chain(["sans-serif"])
            .find_map(|f| cache.choose(system, f, st.weight, st.italic))
    });
    let mut list = AttrsList::new(match runs.get(first) {
        Some(r) => run_attrs(default_choice.as_deref(), &r.style, first, line_height),
        None => Attrs::new(),
    });
    for (ri, run) in runs.iter().enumerate() {
        let (s, e) = (run.range.start.max(base), run.range.end.min(end));
        if s >= e {
            continue;
        }
        let st = &run.style;
        let choices: Vec<_> = st
            .families
            .iter()
            .filter_map(|f| cache.choose(system, f, st.weight, st.italic))
            .collect();
        // No listed family installed: the generic sans-serif face, so weight
        // and italic still resolve to a real (or synthetic) face.
        let fallback = choices
            .first()
            .cloned()
            .or_else(|| cache.choose(system, "sans-serif", st.weight, st.italic));
        // (byte start relative to `para`, chosen face index) per font run.
        let mut segs: Vec<(usize, Option<usize>)> = Vec::new();
        let mut prev: Option<Option<usize>> = None;
        for (i, ch) in para[s - base..e - base].char_indices() {
            let pick = match prev {
                Some(p) if attaches_to_previous(ch) => p,
                _ => choices.iter().position(|c| cache.covers(system, c.id, ch)),
            };
            if prev != Some(pick) {
                segs.push((s - base + i, pick));
                prev = Some(pick);
            }
        }
        for (k, &(start, pick)) in segs.iter().enumerate() {
            let stop = segs.get(k + 1).map_or(e - base, |n| n.0);
            // Uncovered characters stay in the first family; cosmic-text's
            // fallback then finds them a face.
            let choice = pick.map(|i| choices[i].clone()).or(fallback.clone());
            list.add_span(
                start..stop,
                run_attrs(choice.as_deref(), st, ri, line_height),
            );
        }
    }
    list
}

/// Add each run's tracking to the last glyph of every cluster.
fn apply_tracking(line: &mut ShapeLine, runs: &[StyledRun], default_size: f32) {
    for span in &mut line.spans {
        for word in &mut span.words {
            let n = word.glyphs.len();
            for i in 0..n {
                let cluster_end = i + 1 == n || word.glyphs[i + 1].start != word.glyphs[i].start;
                let g = &mut word.glyphs[i];
                let Some(run) = runs.get(g.metadata) else {
                    continue;
                };
                let size = g.metrics_opt.map_or(default_size, |m| m.font_size);
                if cluster_end && run.style.tracking != 0.0 && size > 0.0 {
                    // Advances are in em of the glyph's size.
                    g.x_advance += run.style.tracking / size;
                }
            }
        }
    }
}

/// Shape `text` under `max_width` (points). A non-finite or non-positive width
/// means unconstrained (no wrapping).
///
/// **Line metrics.** Each line box is `base_size × line_height` tall for the
/// largest run on the line, and its baseline sits where the largest ascent +
/// descent box is centred in it (CSS half-leading). Super/subscripts count
/// with their *base* size and unshifted, so they never push lines apart; the
/// block only grows at its top/bottom edge when a glyph (shifted or not)
/// overhangs the first/last line box, so nothing is clipped at a page edge.
pub(crate) fn shape(
    system: &mut FontSystem,
    cache: &mut FontCache,
    text: &TextProps,
    max_width: f32,
    default_family: Option<&str>,
) -> ShapedText {
    let runs = styled_runs(text, default_family);
    let width = (max_width.is_finite() && max_width > 0.0).then_some(max_width);
    let node_line = (text.font_size * text.line_height).max(1.0);
    let mut scratch = ShapeBuffer::default();

    let mut lines: Vec<ShapedLine> = Vec::new();
    let mut decorations = Vec::new();
    let mut max_w = 0.0f32;
    let mut line_top = 0.0f32;
    let mut ink_top = 0.0f32;
    let mut ink_bottom = 0.0f32;
    let mut layout = Vec::new();

    for (base, para) in paragraphs(&text.content) {
        let attrs = attrs_list(system, cache, para, base, &runs, text.line_height);
        let mut shaped =
            ShapeLine::new_in_buffer(&mut scratch, system, para, &attrs, Shaping::Advanced, 8);
        apply_tracking(&mut shaped, &runs, text.font_size);
        layout.clear();
        shaped.layout_to_buffer(
            &mut scratch,
            text.font_size,
            width,
            Wrap::WordOrGlyph,
            None,
            &mut layout,
            None,
        );
        kinsoku(&mut layout, para);

        for ll in &layout {
            let style_of =
                |g: &cosmic_text::LayoutGlyph| &runs[g.metadata.min(runs.len() - 1)].style;
            let (mut lh, mut ascent, mut descent) = (0.0f32, 0.0f32, 0.0f32);
            let (mut ink_up, mut ink_down) = (0.0f32, 0.0f32);
            for g in &ll.glyphs {
                let st = style_of(g);
                let m = cache.metrics(system, g.font_id);
                lh = lh.max(st.base_size * text.line_height);
                ascent = ascent.max(m.ascent * st.base_size);
                descent = descent.max(m.descent * st.base_size);
                ink_up = ink_up.max(m.ascent * g.font_size + st.shift);
                ink_down = ink_down.max(m.descent * g.font_size - st.shift);
            }
            if ll.glyphs.is_empty() {
                lh = node_line;
            }
            let lh = lh.max(1.0);
            let baseline = line_top + (lh - (ascent + descent)) / 2.0 + ascent;
            if !ll.glyphs.is_empty() && ascent + descent > 0.0 {
                ink_top = ink_top.min(baseline - ascent.max(ink_up));
                ink_bottom = ink_bottom.max(baseline + descent.max(ink_down));
            }
            line_top += lh;

            let trailing = ll.glyphs.last().map_or(0.0, |g| style_of(g).tracking);
            let line_w = (ll.w - trailing).max(0.0);
            max_w = max_w.max(line_w);

            let line_index = lines.len();
            push_decorations(
                &mut decorations,
                system,
                cache,
                &runs,
                ll,
                line_index,
                baseline,
            );

            let glyphs = ll
                .glyphs
                .iter()
                .map(|g| {
                    let st = style_of(g);
                    ShapedGlyph {
                        font_id: g.font_id,
                        glyph_id: g.glyph_id,
                        font_size: g.font_size,
                        x: g.x,
                        advance: g.w,
                        x_offset: g.x_offset * g.font_size,
                        y_offset: g.y_offset * g.font_size + st.shift,
                        text_range: base + g.start..base + g.end,
                        color: st.color,
                        synthetic_italic: g.cache_key_flags.contains(CacheKeyFlags::FAKE_ITALIC),
                        run: g.metadata,
                    }
                })
                .collect();
            lines.push(ShapedLine {
                baseline,
                width: line_w,
                glyphs,
            });
        }
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
    for d in &mut decorations {
        d.y += shift;
    }
    let height = line_top.max(ink_bottom) + shift;
    ShapedText {
        size: Size::new(max_w, height),
        lines,
        decorations,
    }
}

/// Closing punctuation that must not start a line (Japanese 行頭禁則).
const NO_LINE_START: &str = "、。，．・：；！？）」』】〕〉》｝］〙〗〟’”,.;:!?)]}";
/// Opening brackets that must not end a line (行末禁則).
const NO_LINE_END: &str = "（「『【〔〈《｛［〘〖〝‘“([{";

/// Repair line starts/ends that word-level breaking can't prevent.
///
/// cosmic-text breaks at UAX #14 opportunities, which already forbid a break
/// before `。、）」…` or after `（「…` — but a "word" wider than the line
/// (a long Latin word, or a run of small kana / `ー`, which UAX #14 glues
/// together) is then broken between *any* two glyphs. Here an opening bracket
/// left at a line end moves to the next line, and closing punctuation left
/// at a line start hangs at the end of the previous line (ぶら下げ; it may
/// overhang the box by that one glyph).
fn kinsoku(lines: &mut Vec<cosmic_text::LayoutLine>, para: &str) {
    let ch = |g: &cosmic_text::LayoutGlyph| para.get(g.start..).and_then(|s| s.chars().next());
    let ltr = |l: &cosmic_text::LayoutLine| l.glyphs.iter().all(|g| !g.level.is_rtl());
    if lines.len() < 2 || !lines.iter().all(ltr) {
        return;
    }
    for i in 0..lines.len() - 1 {
        while lines[i].glyphs.len() > 1
            && lines[i]
                .glyphs
                .last()
                .and_then(ch)
                .is_some_and(|c| NO_LINE_END.contains(c))
        {
            let mut g = lines[i].glyphs.pop().expect("non-empty");
            lines[i].w -= g.w;
            let next = &mut lines[i + 1];
            for n in &mut next.glyphs {
                n.x += g.w;
            }
            g.x = 0.0;
            next.w += g.w;
            next.glyphs.insert(0, g);
        }
    }
    for i in 1..lines.len() {
        while lines[i]
            .glyphs
            .first()
            .and_then(ch)
            .is_some_and(|c| NO_LINE_START.contains(c))
        {
            let mut g = lines[i].glyphs.remove(0);
            for n in &mut lines[i].glyphs {
                n.x -= g.w;
            }
            lines[i].w -= g.w;
            let prev = &mut lines[i - 1];
            g.x = prev.w;
            prev.w += g.w;
            prev.glyphs.push(g);
        }
    }
    lines.retain(|l| !l.glyphs.is_empty());
}

/// Underline / line-through bars of one laid-out line: one bar per maximal
/// group of consecutive glyphs of the same decorated run. Underlines sit at
/// the unshifted baseline with the face's underline metrics at the run's base
/// size (a subscript's underline lines up with its neighbours'); a
/// line-through follows the glyphs (shifted, at their size).
fn push_decorations(
    out: &mut Vec<Decoration>,
    system: &mut FontSystem,
    cache: &mut FontCache,
    runs: &[StyledRun],
    ll: &cosmic_text::LayoutLine,
    line: usize,
    baseline: f32,
) {
    let glyphs = &ll.glyphs;
    let n = glyphs.len();
    let run_of = |i: usize| glyphs[i].metadata.min(runs.len() - 1);
    for kind in [DecorationKind::Underline, DecorationKind::Strike] {
        let mut i = 0;
        while i < n {
            let st = &runs[run_of(i)].style;
            let on = match kind {
                DecorationKind::Underline => st.underline,
                DecorationKind::Strike => st.strike,
            };
            let mut j = i;
            while j + 1 < n && run_of(j + 1) == run_of(i) {
                j += 1;
            }
            if on {
                let first = &glyphs[i];
                let last = &glyphs[j];
                let mut x1 = last.x + last.w;
                if j + 1 == n {
                    x1 -= st.tracking; // no bar under the line's trailing tracking
                }
                let m = cache.metrics(system, first.font_id);
                let (y, thickness) = match kind {
                    DecorationKind::Underline => (
                        baseline - m.underline_pos * st.base_size,
                        m.underline_thickness * st.base_size,
                    ),
                    DecorationKind::Strike => (
                        baseline - st.shift - m.strike_pos * st.font_size,
                        m.strike_thickness * st.font_size,
                    ),
                };
                if x1 > first.x && thickness > 0.0 {
                    out.push(Decoration {
                        kind,
                        line,
                        x: first.x,
                        width: x1 - first.x,
                        y,
                        thickness,
                        color: st.color,
                    });
                }
            }
            i = j + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::span::{Script, SpanStyle, TextSpan, SUPER_SHIFT};

    #[test]
    fn runs_cover_content_and_inherit() {
        let text = TextProps {
            content: "ΔG300K = x".into(),
            font_family: Some("Arial, Yu Gothic".into()),
            font_size: 10.0,
            spans: vec![TextSpan {
                range: 3..7,
                style: SpanStyle {
                    script: Some(Script::Super),
                    color: Some(Color::WHITE),
                    ..Default::default()
                },
            }],
            ..Default::default()
        };
        let runs = styled_runs(&text, None);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].range, 0..3);
        assert_eq!(runs[2].range, 7..text.content.len());
        let sup = &runs[1].style;
        assert_eq!(sup.families, vec!["Arial", "Yu Gothic"]);
        assert!((sup.font_size - 6.5).abs() < 1e-5);
        assert_eq!(sup.base_size, 10.0);
        assert!((sup.shift - SUPER_SHIFT * 10.0).abs() < 1e-5);
        assert_eq!(sup.color, Color::WHITE);
        assert_eq!(runs[2].style.color, Color::BLACK);
    }

    #[test]
    fn paragraphs_split_at_newlines() {
        assert_eq!(paragraphs("a\r\nbc\n"), vec![(0, "a"), (3, "bc"), (6, "")]);
    }
}
