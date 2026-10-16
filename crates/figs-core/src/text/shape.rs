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
//!    font spec — for its script slot when the spec is per script — that
//!    covers it (see [`super::fonts`]); a word joiner keeps attached
//!    super/subscripts from being broken off (removed after breaking); the
//!    result is a
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

use std::sync::Arc;

use crate::geom::{Color, LineCap, LineJoin, Size, StrokeStyle};
use crate::schema::span::{FitWidth, FontSpec, FontStyle, FontSynthesis};
use crate::schema::TextProps;

use super::classify::{self, Slot};
use super::fonts::{attaches_to_previous, chain, FaceChoice, FontCache};

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
    /// The distinct glyph outlines (node `stroke`, span `stroke`s) in order
    /// of first use; [`ShapedGlyph::stroke`] indexes it. Backends stroke each
    /// one as a single path behind every fill.
    pub strokes: Vec<StrokeStyle>,
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
    /// Italic was requested but the family has no italic face: the glyph is
    /// drawn slanted by [`ShapedGlyph::skew`].
    pub synthetic_italic: bool,
    /// Horizontal shear of a synthetic italic (`tan` of the node's
    /// `synthetic_slant`; 0 for upright glyphs), about the pen position:
    /// `x += skew · height above the baseline`.
    pub skew: f32,
    /// Synthetic bold: width (points) of an outline in the glyph's own
    /// colour, stroked along its contours under the fill (0 = none).
    pub embolden: f32,
    /// Index into [`ShapedText::strokes`] of this glyph's outline (`None` =
    /// no outline).
    pub stroke: Option<u16>,
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
    /// Font spec: page, node and span specs inherited (see
    /// [`FontSpec::overlay`]). Empty = the generic `sans-serif`.
    pub fonts: FontSpec,
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
    /// A super/subscript or shifted run: it stays on the line of the
    /// character it follows (no line break right before it).
    pub attached: bool,
    /// Glyph outline (node `stroke`, or the span's own), points.
    pub stroke: Option<StrokeStyle>,
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
    let page_fonts = default_family
        .map(FontSpec::parse_lenient)
        .unwrap_or_default();
    let node_fonts = match text.font_family.as_deref() {
        Some(f) => page_fonts.overlay(&FontSpec::parse_lenient(f)),
        None => page_fonts,
    };
    let metrics = text.typography.script_metrics;
    let node = RunStyle {
        fonts: node_fonts,
        font_size: text.font_size,
        base_size: text.font_size,
        weight: text.font_weight.unwrap_or(400),
        italic: text.font_style == FontStyle::Italic,
        color: text.color,
        underline: text.underline,
        strike: text.strike,
        shift: 0.0,
        tracking: text.letter_spacing * text.font_size,
        attached: false,
        stroke: text.stroke.clone(),
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
            base_size * metrics.scale()
        } else {
            base_size
        };
        let shift_em = s
            .baseline_shift
            .or(s.script.map(|sc| metrics.shift(sc)))
            .unwrap_or(0.0);
        let fonts = s
            .font_family
            .as_deref()
            .map(FontSpec::parse_lenient)
            .filter(|f| !f.is_empty())
            .map_or_else(|| node.fonts.clone(), |f| node.fonts.overlay(&f));
        let color = s.color.unwrap_or(node.color);
        push(
            start..end,
            RunStyle {
                fonts,
                font_size,
                base_size,
                weight: s.font_weight.unwrap_or(node.weight),
                italic: s
                    .font_style
                    .map_or(node.italic, |st| st == FontStyle::Italic),
                color,
                underline: s.underline.unwrap_or(node.underline),
                strike: s.strike.unwrap_or(node.strike),
                shift: shift_em * base_size,
                tracking: s.letter_spacing.unwrap_or(text.letter_spacing) * font_size,
                attached: s.script.is_some() || shift_em != 0.0,
                stroke: span_stroke(s, node.stroke.as_ref(), color),
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

/// The outline of a span's glyphs: the node's unless the span sets `stroke`
/// (colour) or `stroke_width` (points; `0` = none). A span outline keeps the
/// node outline's dash/cap/join; its colour defaults to the node outline's,
/// else the span's text colour, its width to the node outline's, else 1 pt.
fn span_stroke(
    s: &crate::schema::SpanStyle,
    node: Option<&StrokeStyle>,
    text_color: Color,
) -> Option<StrokeStyle> {
    match (s.stroke, s.stroke_width) {
        (None, None) => node.cloned(),
        (_, Some(w)) if w <= 0.0 => None,
        (color, width) => Some(StrokeStyle {
            color: color.or(node.map(|n| n.color)).unwrap_or(text_color),
            width: width.or(node.map(|n| n.width)).unwrap_or(1.0),
            dash: node.map(|n| n.dash.clone()).unwrap_or_default(),
            cap: node.map_or(LineCap::default(), |n| n.cap),
            join: node.map_or(LineJoin::Round, |n| n.join),
        }),
    }
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

/// The installed faces a run's characters may use, per slot: indices into
/// `faces` in chain order, plus the face an uncovered character keeps (the
/// first installed one of its chain, else the generic sans-serif).
struct SlotFaces {
    faces: Vec<Arc<FaceChoice>>,
    chains: Vec<(Slot, Vec<usize>, Option<usize>)>,
}

impl SlotFaces {
    fn chain(
        &mut self,
        system: &FontSystem,
        cache: &mut FontCache,
        st: &RunStyle,
        slot: Slot,
    ) -> usize {
        if let Some(k) = self.chains.iter().position(|c| c.0 == slot) {
            return k;
        }
        let mut idx = Vec::new();
        for name in chain(&st.fonts, slot) {
            if let Some(c) = cache.choose(system, name, st.weight, st.italic) {
                idx.push(self.face(c));
            }
        }
        idx.dedup();
        // No listed family installed: the generic sans-serif face, so weight
        // and italic still resolve to a real (or synthetic) face.
        let keep = idx.first().copied().or_else(|| {
            cache
                .choose(system, "sans-serif", st.weight, st.italic)
                .map(|c| self.face(c))
        });
        self.chains.push((slot, idx, keep));
        self.chains.len() - 1
    }

    fn face(&mut self, c: Arc<FaceChoice>) -> usize {
        match self.faces.iter().position(|f| f.id == c.id) {
            Some(i) => i,
            None => {
                self.faces.push(c);
                self.faces.len() - 1
            }
        }
    }
}

/// Assign every character of `para` (at byte offset `base` in the content) a
/// face: the first family of its run's font spec, for the character's script
/// slot ([`classify::slots`]), that covers it.
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
        chain(&st.fonts, Slot::Latin)
            .into_iter()
            .chain(["sans-serif"])
            .find_map(|f| cache.choose(system, f, st.weight, st.italic))
    });
    let mut list = AttrsList::new(match runs.get(first) {
        Some(r) => run_attrs(default_choice.as_deref(), &r.style, first, line_height),
        None => Attrs::new(),
    });
    // Byte offset (in `para`) and script slot of every character.
    let slots: Vec<(usize, Slot)> = para
        .char_indices()
        .map(|(i, _)| i)
        .zip(classify::slots(para))
        .collect();
    for (ri, run) in runs.iter().enumerate() {
        let (s, e) = (run.range.start.max(base), run.range.end.min(end));
        if s >= e {
            continue;
        }
        let st = &run.style;
        let mut sf = SlotFaces {
            faces: Vec::new(),
            chains: Vec::new(),
        };
        // (byte start relative to `para`, face index) per font run.
        let mut segs: Vec<(usize, Option<usize>)> = Vec::new();
        let mut prev: Option<Option<usize>> = None;
        let from = slots.partition_point(|(i, _)| *i < s - base);
        for &(i, slot) in slots[from..].iter().take_while(|(i, _)| *i < e - base) {
            let ch = para[i..].chars().next().expect("char boundary");
            let pick = match prev {
                Some(p) if attaches_to_previous(ch) => p,
                _ => {
                    let k = sf.chain(system, cache, st, slot);
                    let (_, idx, keep) = &sf.chains[k];
                    // Uncovered characters keep their chain's first face;
                    // cosmic-text's fallback then finds them a glyph.
                    idx.iter()
                        .copied()
                        .find(|&f| cache.covers(system, sf.faces[f].id, ch))
                        .or(*keep)
                }
            };
            if prev != Some(pick) {
                segs.push((i, pick));
                prev = Some(pick);
            }
        }
        for (k, &(start, pick)) in segs.iter().enumerate() {
            let stop = segs.get(k + 1).map_or(e - base, |n| n.0);
            let choice = pick.map(|i| sf.faces[i].clone());
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
/// means unconstrained (no wrapping), as does `text.wrap == false`.
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
    let bounded = max_width.is_finite() && max_width > 0.0;
    // A fitted text never wraps; each of its lines is fitted to `max_width`.
    let fit = text.fit_width.filter(|_| bounded);
    let width = (text.wrap && text.fit_width.is_none() && bounded).then_some(max_width);
    let node_line = (text.font_size * text.line_height).max(1.0);
    let mut scratch = ShapeBuffer::default();
    let typo = text.typography;
    let skew = typo.shear();
    // The distinct outlines, and each run's index into them.
    let mut strokes: Vec<StrokeStyle> = Vec::new();
    let run_stroke: Vec<Option<u16>> = runs
        .iter()
        .map(|r| {
            let s = r.style.stroke.as_ref()?;
            let k = strokes.iter().position(|x| x == s).unwrap_or_else(|| {
                strokes.push(s.clone());
                strokes.len() - 1
            });
            Some(k as u16)
        })
        .collect();

    let mut lines: Vec<ShapedLine> = Vec::new();
    let mut decorations = Vec::new();
    let mut max_w = 0.0f32;
    let mut line_top = 0.0f32;
    let mut ink_top = 0.0f32;
    let mut ink_bottom = 0.0f32;
    let mut layout = Vec::new();

    for (base, para) in paragraphs(&text.content) {
        // A word joiner before every super/subscript glued to the previous
        // character, so UAX #14 offers no break there (removed again below).
        let joins = join_points(para, base, &runs);
        let (joined, joined_runs);
        let (shape_para, shape_runs): (&str, &[StyledRun]) = if joins.is_empty() {
            (para, &runs)
        } else {
            (joined, joined_runs) = with_joiners(para, base, &runs, &joins);
            (&joined, &joined_runs)
        };
        let attrs = attrs_list(
            system,
            cache,
            shape_para,
            base,
            shape_runs,
            text.line_height,
        );
        let mut shaped = ShapeLine::new_in_buffer(
            &mut scratch,
            system,
            shape_para,
            &attrs,
            Shaping::Advanced,
            8,
        );
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
        if !joins.is_empty() {
            strip_joiners(&mut layout, &joins);
        }
        keep_scripts_attached(&mut layout, para, &runs);
        kinsoku(&mut layout, para);

        for ll in &mut layout {
            // `fit_width`: stretch/squeeze the line to the box width; a
            // scaled line has its runs' sizes scaled to match.
            let scaled;
            let runs: &[StyledRun] = match fit {
                Some(FitWidth::Tracking) => {
                    fit_tracking(ll, &runs, max_width);
                    &runs
                }
                Some(FitWidth::Scale) => match fit_scale(ll, &runs, max_width) {
                    Some(k) => {
                        scaled = scaled_runs(&runs, k);
                        &scaled
                    }
                    None => &runs,
                },
                None => &runs,
            };
            let ll = &*ll;
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
                runs,
                ll,
                line_index,
                baseline,
            );

            let mut glyphs = Vec::with_capacity(ll.glyphs.len());
            for g in &ll.glyphs {
                let st = style_of(g);
                let synthetic_italic = g.cache_key_flags.contains(CacheKeyFlags::FAKE_ITALIC);
                let bold = match typo.font_synthesis {
                    FontSynthesis::None => false,
                    FontSynthesis::Bold => true,
                    FontSynthesis::Auto => {
                        st.weight >= 600 && cache.face_weight(system, g.font_id) < 600
                    }
                };
                glyphs.push(ShapedGlyph {
                    font_id: g.font_id,
                    glyph_id: g.glyph_id,
                    font_size: g.font_size,
                    x: g.x,
                    advance: g.w,
                    x_offset: g.x_offset * g.font_size,
                    y_offset: g.y_offset * g.font_size + st.shift,
                    text_range: base + g.start..base + g.end,
                    color: st.color,
                    synthetic_italic,
                    skew: if synthetic_italic { skew } else { 0.0 },
                    embolden: if bold {
                        typo.synthetic_bold_width * g.font_size
                    } else {
                        0.0
                    },
                    stroke: run_stroke[g.metadata.min(runs.len() - 1)],
                    run: g.metadata,
                });
            }
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
        strokes,
    }
}

/// The width a laid-out line is aligned by: its advance without the
/// trailing tracking of its last character.
fn line_width(ll: &cosmic_text::LayoutLine, runs: &[StyledRun]) -> f32 {
    let trailing = ll
        .glyphs
        .last()
        .map_or(0.0, |g| runs[g.metadata.min(runs.len() - 1)].style.tracking);
    ll.w - trailing
}

/// `fit_width = "tracking"`: spread `target - width` evenly over the gaps
/// between the line's clusters (after every cluster but the last, like
/// `letter_spacing`), so the line is exactly `target` wide. A line of one
/// cluster is left alone.
fn fit_tracking(ll: &mut cosmic_text::LayoutLine, runs: &[StyledRun], target: f32) {
    let n = ll.glyphs.len();
    let starts = |i: usize, g: &[cosmic_text::LayoutGlyph]| i == 0 || g[i].start != g[i - 1].start;
    let clusters = (0..n).filter(|&i| starts(i, &ll.glyphs)).count();
    if clusters < 2 {
        return;
    }
    let extra = (target - line_width(ll, runs)) / (clusters - 1) as f32;
    let mut k = 0usize;
    for i in 0..n {
        if i > 0 && starts(i, &ll.glyphs) {
            k += 1;
        }
        let ends = i + 1 == n || ll.glyphs[i + 1].start != ll.glyphs[i].start;
        let g = &mut ll.glyphs[i];
        g.x += k as f32 * extra;
        if ends && k + 1 < clusters {
            g.w += extra;
        }
    }
    ll.w += (clusters - 1) as f32 * extra;
}

/// `fit_width = "scale"`: scale the line's glyphs (positions, advances,
/// sizes) by `target / width` about its start; returns the factor (`None`
/// for an empty line), which the caller applies to the runs' sizes so line
/// metrics, shifts and decorations scale with it.
fn fit_scale(ll: &mut cosmic_text::LayoutLine, runs: &[StyledRun], target: f32) -> Option<f32> {
    let w = line_width(ll, runs);
    if ll.glyphs.is_empty() || w <= 0.0 {
        return None;
    }
    let k = target / w;
    for g in &mut ll.glyphs {
        g.x *= k;
        g.w *= k;
        // Offsets are in em of the glyph size, so they scale with it.
        g.font_size *= k;
    }
    ll.w *= k;
    Some(k)
}

/// `runs` with every size scaled by `k` (see [`fit_scale`]); outlines keep
/// their width.
fn scaled_runs(runs: &[StyledRun], k: f32) -> Vec<StyledRun> {
    runs.iter()
        .map(|r| {
            let mut r = r.clone();
            r.style.font_size *= k;
            r.style.base_size *= k;
            r.style.shift *= k;
            r.style.tracking *= k;
            r
        })
        .collect()
}

/// U+2060 WORD JOINER: UAX #14 never breaks on either side of it.
const JOINER: &str = "\u{2060}";

/// Byte offsets (in `para`, at content offset `base`) where a super/subscript
/// (or shifted) run starts right after a non-space character: no line break
/// may fall there.
fn join_points(para: &str, base: usize, runs: &[StyledRun]) -> Vec<usize> {
    let end = base + para.len();
    let mut out: Vec<usize> = runs
        .iter()
        .filter(|r| r.style.attached && r.range.start > base && r.range.start < end)
        .map(|r| r.range.start - base)
        .filter(|&p| {
            para.is_char_boundary(p)
                && para[..p]
                    .chars()
                    .next_back()
                    .is_some_and(|c| !c.is_whitespace())
        })
        .collect();
    out.dedup();
    out
}

/// `para` with a [`JOINER`] inserted at each of `joins`, and `runs` with
/// their ranges moved to match (a joiner belongs to the run before it).
fn with_joiners(
    para: &str,
    base: usize,
    runs: &[StyledRun],
    joins: &[usize],
) -> (String, Vec<StyledRun>) {
    let mut s = String::with_capacity(para.len() + joins.len() * JOINER.len());
    let mut at = 0;
    for &p in joins {
        s.push_str(&para[at..p]);
        s.push_str(JOINER);
        at = p;
    }
    s.push_str(&para[at..]);
    let map = |x: usize| x + JOINER.len() * joins.iter().filter(|&&p| base + p <= x).count();
    let runs = runs
        .iter()
        .map(|r| StyledRun {
            // A run ending at a join point takes the joiner; one starting
            // there starts after it.
            range: map(r.range.start)..map(r.range.end),
            style: r.style.clone(),
        })
        .collect();
    (s, runs)
}

/// Undo [`with_joiners`] on laid-out lines: drop the joiner glyphs (closing
/// any advance they had) and map glyph byte ranges back to the paragraph
/// without joiners.
fn strip_joiners(lines: &mut Vec<cosmic_text::LayoutLine>, joins: &[usize]) {
    let n = JOINER.len();
    // Joiner k sits at [q_k, q_k + n) in the joined paragraph.
    let q: Vec<usize> = joins.iter().enumerate().map(|(k, p)| p + k * n).collect();
    let back = |x: usize| x - n * q.iter().filter(|&&qk| qk < x).count();
    for line in lines.iter_mut() {
        let mut shift = 0.0;
        line.glyphs.retain_mut(|g| {
            let is_joiner = q.iter().any(|&qk| g.start == qk && g.end == qk + n);
            if is_joiner {
                shift += g.w;
                return false;
            }
            g.x -= shift;
            g.start = back(g.start);
            g.end = back(g.end);
            true
        });
        line.w -= shift;
    }
    lines.retain(|l| !l.glyphs.is_empty());
}

/// Keep a super/subscript (or any shifted run) on the line of the character
/// it is attached to. Word joiners already keep UAX #14 from breaking there;
/// this repairs the remaining case, a "word" wider than the box broken
/// between arbitrary glyphs: when a line starts with an attached run right
/// after a non-space character that ended the previous line, the attached
/// glyphs move up to the end of that line (they may overhang the box, as a
/// hanging punctuation mark does).
fn keep_scripts_attached(lines: &mut Vec<cosmic_text::LayoutLine>, para: &str, runs: &[StyledRun]) {
    let attached =
        |g: &cosmic_text::LayoutGlyph| runs.get(g.metadata).is_some_and(|r| r.style.attached);
    let ltr = |l: &cosmic_text::LayoutLine| l.glyphs.iter().all(|g| !g.level.is_rtl());
    if lines.len() < 2 || !lines.iter().all(ltr) {
        return;
    }
    for i in 0..lines.len() - 1 {
        let (Some(next), Some(last)) = (lines[i + 1].glyphs.first(), lines[i].glyphs.last()) else {
            continue;
        };
        let glued = attached(next)
            && last.end == next.start
            && para
                .get(last.start..last.end)
                .and_then(|s| s.chars().next())
                .is_some_and(|c| !c.is_whitespace());
        if !glued {
            continue;
        }
        let last_end = last.x + last.w;
        let count = lines[i + 1]
            .glyphs
            .iter()
            .take_while(|g| attached(g))
            .count();
        let moved: Vec<_> = lines[i + 1].glyphs.drain(..count).collect();
        let w: f32 = moved.iter().map(|g| g.w).sum();
        let next = &mut lines[i + 1];
        for g in &mut next.glyphs {
            g.x -= w;
        }
        next.w -= w;
        let prev = &mut lines[i];
        let x0 = last_end - moved[0].x;
        prev.glyphs.extend(moved.into_iter().map(|mut g| {
            g.x += x0;
            g
        }));
        prev.w += w;
    }
    lines.retain(|l| !l.glyphs.is_empty());
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
        assert_eq!(sup.fonts.fallback, vec!["Arial", "Yu Gothic"]);
        assert!(sup.attached && !runs[0].style.attached);
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
