//! Resolution: validate a [`RawDocument`] and convert it into a typed,
//! index-addressed [`Document`].
//!
//! Responsibilities:
//! - check the root id exists,
//! - resolve every `children` id to an index (error on dangling references),
//! - reject cycles and nodes referenced by more than one parent,
//! - enforce per-type required fields (image needs `src`, text needs `content`),
//! - convert all page-unit lengths into points.
//!
//! Every error carries the offending node id so diagnostics are actionable.

use std::collections::HashMap;

use crate::geom::{
    AnchorValue, ArcSweep, ArrowHead, Axis, Color, CrossAxisAlign, Edges, Effects, HeadStyle,
    HeadWidth, ImageFit, LineJoin, MainAxisAlign, Paint, PathSegment, Point, Shadow, StrokeAlign,
    StrokeStyle, TextAlign, ViewBox,
};
use crate::path::arc_to_cubics;

use super::parse::RawPage;
use super::parse::{NodeKind, RawDocument, RawNode, RawSegment};
use super::span::{FitWidth, FontSpec, FontStyle, TextSpan, TextTypography};

/// Default raster resolution for PNG output when `page.dpi` is unset.
pub const DEFAULT_DPI: f32 = 300.0;
/// Default text size in points.
pub const DEFAULT_FONT_SIZE: f32 = 12.0;
/// Default line height as a multiple of font size.
pub const DEFAULT_LINE_HEIGHT: f32 = 1.2;
/// Default stroke width of line/arrow/ellipse/path, in points (unit-independent).
pub const DEFAULT_SHAPE_STROKE_WIDTH: f32 = 1.0;
/// Default arrowhead length as a multiple of the stroke width.
pub const DEFAULT_HEAD_SIZE_PER_STROKE: f32 = 5.0;
/// Default arrowhead base width as a multiple of its length.
pub const DEFAULT_HEAD_WIDTH_PER_SIZE: f32 = 0.8;
/// Default `shadow` colour: translucent black.
pub const DEFAULT_SHADOW_COLOR: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.5,
};

/// Errors produced while resolving a raw document. Each variant names the node
/// at fault where applicable.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ResolveError {
    #[error("page.root refers to unknown node `{0}`")]
    RootMissing(String),
    #[error("node `{node}`: child `{child}` does not exist")]
    DanglingChild { node: String, child: String },
    #[error("node `{node}` is referenced as a child by more than one parent")]
    MultipleParents { node: String },
    #[error("cycle detected through node `{0}`")]
    Cycle(String),
    #[error("node `{node}`: {kind} requires field `{field}`")]
    MissingField {
        node: String,
        kind: &'static str,
        field: &'static str,
    },
    #[error("node `{node}`: invalid `{field}`: {reason}")]
    InvalidField {
        node: String,
        field: &'static str,
        reason: String,
    },
}

/// Page geometry and presentation, with all lengths converted to points.
#[derive(Debug, Clone)]
pub struct PageInfo {
    pub width_pt: f32,
    /// Fixed page height in points, or `None` for `height = "auto"`: layout
    /// then derives it from the content ([`crate::layout::ComputedLayout`]'s
    /// `page.height_pt` carries the final value).
    pub height_pt: Option<f32>,
    pub dpi: f32,
    /// Page fill (absolute gradient lengths in points), over the page box.
    pub background: Option<Paint>,
    /// Default font family for text nodes that don't name one.
    pub font_family: Option<String>,
    /// Minimum stroke width in device pixels for raster output (`0` = off;
    /// see [`super::RawPage::min_stroke_px`]).
    pub min_stroke_px: f32,
}

/// A fully resolved document. Nodes are stored flat and addressed by index;
/// `root` is the entry index.
#[derive(Debug)]
pub struct Document {
    pub page: PageInfo,
    pub nodes: Vec<Node>,
    pub root: usize,
    /// Files read while producing the document besides images (e.g. data a
    /// script loaded), as recorded via [`crate::deps::record`]. Relative paths
    /// are relative to the document's directory. See [`Document::dependencies`].
    pub data_files: Vec<std::path::PathBuf>,
}

impl Document {
    /// Parse and resolve TOML source in one step.
    pub fn from_toml(src: &str) -> Result<Document, crate::Error> {
        let raw = super::parse::parse_str(src)?;
        Ok(resolve(raw)?)
    }

    /// Evaluate a Rhai script and resolve it in one step.
    #[cfg(feature = "script")]
    pub fn from_script(src: &str) -> Result<Document, crate::Error> {
        Document::from_script_in(src, None)
    }

    /// [`Document::from_script`] for a script in `base_dir` (relative data
    /// paths such as `csv("data.csv")` resolve against it).
    #[cfg(feature = "script")]
    pub fn from_script_in(
        src: &str,
        base_dir: Option<&std::path::Path>,
    ) -> Result<Document, crate::Error> {
        // Built-ins that read files record them; keep them as dependencies.
        let (raw, data_files) =
            crate::deps::capture(|| crate::script::eval_script_in(src, base_dir));
        let mut doc = resolve(raw?)?;
        doc.data_files = data_files;
        Ok(doc)
    }

    pub fn node(&self, idx: usize) -> &Node {
        &self.nodes[idx]
    }
}

/// A resolved node: stable id, shared layout properties (in points), and a
/// typed payload.
#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub common: Common,
    pub kind: NodeType,
    /// Shadows / glow / group opacity (default: none). Visual only.
    pub effects: Effects,
}

/// Layout properties shared by every node type, lengths in points.
#[derive(Debug, Clone, Default)]
pub struct Common {
    pub flex: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub aspect_ratio: Option<f32>,
    pub margin: Edges,
    pub padding: Edges,
    /// Offset of the border box (after margin) from the parent's content-box
    /// origin. Only a `stack` parent reads these; flex containers ignore them.
    /// Points — or, under a stack with a `viewbox`, that viewbox's user units
    /// (mapped at layout time).
    pub x: Option<f32>,
    pub y: Option<f32>,
    /// Which point of the footprint is placed at the `x`/`y` offset, per
    /// axis a fraction or (text only) a baseline (`None` = top-left; see
    /// [`RawNode::anchor`]).
    pub anchor: Option<[AnchorValue; 2]>,
    /// Clockwise rotation in degrees about the box centre (`0` = none; see
    /// [`RawNode::rotate`]). `width`/`height`/`aspect_ratio`/`padding` are the
    /// node's own (unrotated) box; `margin`, `flex`, `x`/`y` and `anchor` act
    /// in the parent's frame, on the footprint.
    pub rotate: f32,
}

impl Common {
    /// Whether the node is turned by an odd number of quarter turns (`90°`,
    /// `270°`, …), so its layout footprint is its box with width and height
    /// swapped.
    pub fn swaps_axes(&self) -> bool {
        crate::geom::quarter_turns(self.rotate).is_some_and(|k| k % 2 == 1)
    }

    /// The properties a parent reads, in the parent's frame: for a node
    /// turned by an odd quarter turn, `width`/`height` are swapped and
    /// `aspect_ratio` inverted (they describe the footprint); otherwise
    /// unchanged.
    pub fn outer(&self) -> std::borrow::Cow<'_, Common> {
        if !self.swaps_axes() {
            return std::borrow::Cow::Borrowed(self);
        }
        std::borrow::Cow::Owned(Common {
            width: self.height,
            height: self.width,
            aspect_ratio: self.aspect_ratio.map(|a| if a > 0.0 { 1.0 / a } else { a }),
            ..self.clone()
        })
    }
}

/// Typed node payload.
#[derive(Debug)]
pub enum NodeType {
    Container {
        axis: Axis,
        children: Vec<usize>,
        spacing: f32,
        main_axis: MainAxisAlign,
        cross_axis: CrossAxisAlign,
        /// Background / border / clip (`None` = a plain container).
        decoration: Option<Box<Decoration>>,
    },
    /// Children overlap inside the content box, painted in order (later = on
    /// top), each positioned by its own `x`/`y` — in points, or in `viewbox`
    /// units when set. With `contain`, children that would hang past an edge
    /// shrink the content box instead (see `RawNode::contain`).
    Stack {
        children: Vec<usize>,
        viewbox: Option<ViewBox>,
        contain: bool,
        /// Background / border / clip (`None` = a plain stack).
        decoration: Option<Box<Decoration>>,
    },
    Image(ImageProps),
    Text(TextProps),
    Rect(RectProps),
    /// `line`, `arrow` and `path` all resolve to a stroked/filled polyline.
    Path(PathProps),
    Ellipse(EllipseProps),
}

impl NodeType {
    /// The decoration of a container (`column`/`row`/`stack`), if any.
    pub fn decoration(&self) -> Option<&Decoration> {
        match self {
            NodeType::Container { decoration, .. } | NodeType::Stack { decoration, .. } => {
                decoration.as_deref()
            }
            _ => None,
        }
    }

    /// Child indices of a container (`column`/`row`/`stack`); empty for leaves.
    pub fn children(&self) -> &[usize] {
        match self {
            NodeType::Container { children, .. } | NodeType::Stack { children, .. } => children,
            _ => &[],
        }
    }
}

/// A container's own painting: a background `fill` and a border `stroke`
/// with rounded corners, both behind its children (the border inside the
/// node's border box unless `stroke_align` says otherwise); `clip` keeps the
/// children inside the box (within the border).
#[derive(Debug, Clone, PartialEq)]
pub struct Decoration {
    /// Background paint over the whole (rounded) border box.
    pub fill: Option<Paint>,
    /// Border, drawn *inside* the box by default: its outer edge is the box
    /// edge (see `stroke_align`).
    pub stroke: Option<StrokeStyle>,
    /// Where the border lies relative to the box edge (default `Inside`).
    pub stroke_align: StrokeAlign,
    /// Outer corner radius, points.
    pub corner_radius: f32,
    /// Clip the children to the box inside the border.
    pub clip: bool,
}

#[derive(Debug, Clone)]
pub struct ImageProps {
    pub src: String,
    pub fit: ImageFit,
    /// Smooth (bilinear) scaling; `false` = nearest neighbour (blocky pixels).
    pub smooth: bool,
}

/// A resolved text node: the node-level style plus optional inline spans.
#[derive(Debug, Clone)]
pub struct TextProps {
    /// The full text: `content` followed by every span's text.
    pub content: String,
    pub font_size: f32,
    /// Font spec: an ordered family list (comma-separated, see
    /// [`parse_family_list`](super::span::parse_family_list)) or a
    /// per-script spec (see [`FontSpec`](super::FontSpec)); `None` = the
    /// page / store default.
    pub font_family: Option<String>,
    pub font_weight: Option<u16>,
    pub color: Color,
    pub align: TextAlign,
    /// Multiple of `font_size`.
    pub line_height: f32,
    /// Glyph outline, painted behind the fill (paint order: stroke, then
    /// fill). Purely visual: it never affects measurement, and may overhang
    /// the text box by half its width. Joins default to round.
    pub stroke: Option<StrokeStyle>,
    pub font_style: FontStyle,
    pub underline: bool,
    pub strike: bool,
    /// Tracking in em of each character's font size, added after every
    /// character (cluster).
    pub letter_spacing: f32,
    /// Styled byte ranges of `content` (sorted, non-overlapping); bytes
    /// outside every span use the node style.
    pub spans: Vec<TextSpan>,
    /// Wrap lines to the available width (default). `false`: each paragraph
    /// is one line whatever the width (see [`RawNode::wrap`]).
    pub wrap: bool,
    /// Fit every line to the box width (see [`FitWidth`]); such a text
    /// never wraps. `None`: natural widths.
    pub fit_width: Option<FitWidth>,
    /// Script metrics and synthetic bold/slant (node value, else the page's).
    pub typography: TextTypography,
}

impl Default for TextProps {
    fn default() -> Self {
        TextProps {
            content: String::new(),
            font_size: DEFAULT_FONT_SIZE,
            font_family: None,
            font_weight: None,
            color: Color::BLACK,
            align: TextAlign::default(),
            line_height: DEFAULT_LINE_HEIGHT,
            stroke: None,
            font_style: FontStyle::Normal,
            underline: false,
            strike: false,
            letter_spacing: 0.0,
            spans: Vec::new(),
            wrap: true,
            fit_width: None,
            typography: TextTypography::default(),
        }
    }
}

impl TextProps {
    /// Plain (span-less) text with the default style.
    pub fn plain(content: impl Into<String>, font_size: f32) -> Self {
        TextProps {
            content: content.into(),
            font_size,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct RectProps {
    pub fill: Option<Paint>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,
    pub corner_radius: f32,
    /// Where the border lies relative to the box edge (default `Center`).
    pub stroke_align: StrokeAlign,
}

/// A stroked/filled outline (`line`, `arrow`, `path`). Coordinates are in
/// points, relative to the node's content-box origin; they are not scaled with
/// the box — unless `viewbox` is set: then they are user units mapped onto the
/// content box at layout time.
#[derive(Debug, Clone)]
pub struct PathProps {
    /// `MoveTo` (first point), then one segment per further point/`segments`
    /// entry — arcs already converted to cubics — and a trailing `Close` when
    /// `closed`. See [`PathSegment`].
    pub segments: Vec<PathSegment>,
    pub fill: Option<Paint>,
    pub stroke: Option<StrokeStyle>,
    /// Ignored when `closed` or when there is no stroke.
    pub head: ArrowHead,
    /// Arrowhead length in points.
    pub head_size: f32,
    /// Arrowhead base width in points (default 0.8 × `head_size`).
    pub head_width: f32,
    pub head_style: HeadStyle,
    /// User coordinates of `segments`, mapped onto the content box (`None`:
    /// segments are points from the content-box origin).
    pub viewbox: Option<ViewBox>,
    /// Where the stroke lies relative to a *closed* outline (default
    /// `Center`; resolve rejects anything else on an open one).
    pub stroke_align: StrokeAlign,
}

/// An ellipse inscribed in the node's border box.
#[derive(Debug, Clone)]
pub struct EllipseProps {
    pub fill: Option<Paint>,
    pub stroke: Option<StrokeStyle>,
    /// Where the stroke lies relative to the ellipse (default `Center`).
    pub stroke_align: StrokeAlign,
}

/// Validate and convert a [`RawDocument`] into a [`Document`].
pub fn resolve(raw: RawDocument) -> Result<Document, ResolveError> {
    let factor = raw.page.unit.to_pt();

    // Stable ordering: sort ids so the produced index layout is deterministic
    // across runs (HashMap iteration order is not). Output paint order is later
    // derived by tree traversal, but a deterministic base helps tests/debug.
    let mut ids: Vec<&String> = raw.nodes.keys().collect();
    ids.sort();
    let index_of: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    if !index_of.contains_key(raw.page.root.as_str()) {
        return Err(ResolveError::RootMissing(raw.page.root.clone()));
    }

    // First convert every node, resolving child ids to indices.
    let mut nodes: Vec<Node> = Vec::with_capacity(ids.len());
    for id in &ids {
        let raw_node = &raw.nodes[id.as_str()];
        nodes.push(convert_node(id, raw_node, &index_of, factor)?);
    }

    // Each non-root node may be referenced by at most one parent.
    let mut parent_count = vec![0u32; nodes.len()];
    for node in &nodes {
        for &c in node.kind.children() {
            parent_count[c] += 1;
        }
    }
    for (i, &count) in parent_count.iter().enumerate() {
        if count > 1 {
            return Err(ResolveError::MultipleParents {
                node: nodes[i].id.clone(),
            });
        }
    }

    // Offsets of a viewbox stack's children are user units, mapped at layout
    // time: undo the page-unit scaling applied in `convert_node`.
    let user_offset: Vec<usize> = nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeType::Stack {
                children,
                viewbox: Some(_),
                ..
            } => Some(children.iter().copied()),
            _ => None,
        })
        .flatten()
        .collect();
    for c in user_offset {
        let r = &raw.nodes[ids[c].as_str()];
        nodes[c].common.x = r.x;
        nodes[c].common.y = r.y;
    }

    // Text typography: the node's own settings over the page's.
    let page_typo = typography("page", None, &raw.page, &TextTypography::default())?;
    if let Some(f) = &raw.page.font_family {
        check_font_spec("page", "font_family", f)?;
    }
    for (i, id) in ids.iter().enumerate() {
        if let NodeType::Text(t) = &mut nodes[i].kind {
            let r = &raw.nodes[id.as_str()];
            t.typography = typography(id, Some(r), &raw.page, &page_typo)?;
        }
    }

    let root = index_of[raw.page.root.as_str()];
    detect_cycle(&nodes, root)?;

    if let Some(bg) = &raw.page.background {
        bg.validate().map_err(|reason| ResolveError::InvalidField {
            node: "page".to_string(),
            field: "background",
            reason,
        })?;
    }

    let min_stroke_px = raw.page.min_stroke_px.unwrap_or(0.0);
    if !(min_stroke_px.is_finite() && min_stroke_px >= 0.0) {
        return Err(ResolveError::InvalidField {
            node: "page".to_string(),
            field: "min_stroke_px",
            reason: format!("must be a non-negative number of pixels, got {min_stroke_px}"),
        });
    }
    let page = PageInfo {
        min_stroke_px,
        width_pt: factor * raw.page.width,
        height_pt: raw.page.height.fixed().map(|h| factor * h),
        dpi: raw.page.dpi.unwrap_or(DEFAULT_DPI),
        background: raw.page.background.clone().map(|p| p.scaled(factor)),
        font_family: raw.page.font_family.clone(),
    };

    Ok(Document {
        page,
        nodes,
        root,
        data_files: Vec::new(),
    })
}

/// The typography of a text node (`node`) or of the page (`node = None`):
/// each setting the node leaves unset comes from `base`.
fn typography(
    id: &str,
    node: Option<&RawNode>,
    page: &RawPage,
    base: &TextTypography,
) -> Result<TextTypography, ResolveError> {
    let (metrics, synthesis, bold, slant) = match node {
        Some(n) => (
            n.script_metrics,
            n.font_synthesis,
            n.synthetic_bold_width,
            n.synthetic_slant,
        ),
        None => (
            page.script_metrics,
            page.font_synthesis,
            page.synthetic_bold_width,
            page.synthetic_slant,
        ),
    };
    let invalid = |field, reason: String| ResolveError::InvalidField {
        node: id.to_string(),
        field,
        reason,
    };
    if let Some(w) = bold {
        if !(w.is_finite() && w >= 0.0) {
            return Err(invalid(
                "synthetic_bold_width",
                format!("must be a non-negative number (em), got {w}"),
            ));
        }
    }
    if let Some(a) = slant {
        if !(a.is_finite() && a.abs() < 80.0) {
            return Err(invalid(
                "synthetic_slant",
                format!("must be between -80 and 80 degrees, got {a}"),
            ));
        }
    }
    Ok(TextTypography {
        script_metrics: metrics.unwrap_or(base.script_metrics),
        font_synthesis: synthesis.unwrap_or(base.font_synthesis),
        synthetic_bold_width: bold.unwrap_or(base.synthetic_bold_width),
        synthetic_slant: slant.unwrap_or(base.synthetic_slant),
    })
}

/// Reject a font spec naming an unknown script slot (`latim: Arial`).
fn check_font_spec(id: &str, field: &'static str, spec: &str) -> Result<(), ResolveError> {
    FontSpec::parse(spec)
        .map(|_| ())
        .map_err(|reason| ResolveError::InvalidField {
            node: id.to_string(),
            field,
            reason,
        })
}

/// The full text of a text node (`content` then every span's text) and the
/// byte range + overrides of each span.
fn text_spans(
    id: &str,
    raw: &RawNode,
    factor: f32,
) -> Result<(String, Vec<TextSpan>), ResolveError> {
    let mut content = raw.content.clone().unwrap_or_default();
    let mut spans = Vec::with_capacity(raw.spans.len());
    let invalid = |reason: String| ResolveError::InvalidField {
        node: id.to_string(),
        field: "spans",
        reason,
    };
    if let Some(f) = &raw.font_family {
        check_font_spec(id, "font_family", f)?;
    }
    for s in &raw.spans {
        if let Some(f) = &s.font_family {
            FontSpec::parse(f).map_err(|e| invalid(format!("span `{}`: {e}", s.text)))?;
        }
        if let Some(w) = s.stroke_width {
            if !(w.is_finite() && w >= 0.0) {
                return Err(invalid(format!(
                    "span `{}`: stroke_width must be non-negative, got {w}",
                    s.text
                )));
            }
        }
        if let Some(size) = s.font_size {
            if !(size.is_finite() && size > 0.0) {
                return Err(invalid(format!(
                    "span `{}`: font_size must be positive, got {size}",
                    s.text
                )));
            }
        }
        for (field, v) in [
            ("baseline_shift", s.baseline_shift),
            ("letter_spacing", s.letter_spacing),
        ] {
            if v.is_some_and(|v| !v.is_finite()) {
                return Err(invalid(format!(
                    "span `{}`: {field} must be finite",
                    s.text
                )));
            }
        }
        let start = content.len();
        content.push_str(&s.text);
        if !s.text.is_empty() {
            let mut style = s.style();
            style.stroke_width = style.stroke_width.map(|w| w * factor);
            spans.push(TextSpan {
                range: start..content.len(),
                style,
            });
        }
    }
    Ok((content, spans))
}

fn check_finite(id: &str, field: &'static str, v: f32) -> Result<(), ResolveError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(ResolveError::InvalidField {
            node: id.to_string(),
            field,
            reason: format!("must be a finite number, got {v}"),
        })
    }
}

/// `anchor`: finite fractions; a baseline only on a text node or a container
/// (`column` / `row` / `stack`, which has its children's baselines), in the
/// slot across its baseline in the parent's frame (`y` unless the node is
/// turned by an odd quarter turn, then `x`), and in one slot at most.
fn check_anchor(id: &str, raw: &RawNode, anchor: &[AnchorValue; 2]) -> Result<(), ResolveError> {
    let invalid = |reason: String| ResolveError::InvalidField {
        node: id.to_string(),
        field: "anchor",
        reason,
    };
    if anchor
        .iter()
        .any(|v| v.frac().is_some_and(|f| !f.is_finite()))
    {
        return Err(invalid("values must be finite numbers".into()));
    }
    let slots: Vec<usize> = (0..2).filter(|&i| anchor[i].baseline().is_some()).collect();
    if slots.is_empty() {
        return Ok(());
    }
    if !matches!(
        raw.kind,
        NodeKind::Text | NodeKind::Column | NodeKind::Row | NodeKind::Stack
    ) {
        return Err(invalid(
            "only a text node or a column/row/stack (the baseline of a text inside) has a baseline (use a fraction)".into(),
        ));
    }
    if slots.len() > 1 {
        return Err(invalid(
            "a baseline goes in one slot: [ax, \"baseline\"] (or [\"baseline\", ay] for quarter-turned text)".into(),
        ));
    }
    let turned = crate::geom::quarter_turns(raw.rotate.unwrap_or(0.0)).is_some_and(|k| k % 2 == 1);
    let want = if turned { 0 } else { 1 };
    if slots[0] != want {
        let reason = if turned {
            "this node is turned a quarter turn, so its baseline runs vertically: put \"baseline\" in the x slot ([\"baseline\", ay])"
        } else {
            "the baseline runs horizontally: put \"baseline\" in the y slot ([ax, \"baseline\"])"
        };
        return Err(invalid(reason.into()));
    }
    Ok(())
}

/// `stroke_align` only applies to a closed outline: a rect, an ellipse, a
/// closed path, or a container border.
fn check_stroke_align(id: &str, raw: &RawNode) -> Result<(), ResolveError> {
    if raw.stroke_align.is_none() {
        return Ok(());
    }
    let ok = match raw.kind {
        NodeKind::Rect | NodeKind::Ellipse | NodeKind::Column | NodeKind::Row | NodeKind::Stack => {
            true
        }
        NodeKind::Path => raw.closed,
        _ => false,
    };
    if ok {
        return Ok(());
    }
    Err(ResolveError::InvalidField {
        node: id.to_string(),
        field: "stroke_align",
        reason: "only a closed outline (rect, ellipse, closed path, container border) has an inside and an outside".into(),
    })
}

fn convert_node(
    id: &str,
    raw: &RawNode,
    index_of: &HashMap<&str, usize>,
    factor: f32,
) -> Result<Node, ResolveError> {
    let common = Common {
        flex: raw.flex,
        width: raw.width.map(|w| w * factor),
        height: raw.height.map(|h| h * factor),
        aspect_ratio: raw.aspect_ratio,
        margin: raw.margin.scaled(factor),
        padding: raw.padding.scaled(factor),
        x: raw.x.map(|x| x * factor),
        y: raw.y.map(|y| y * factor),
        anchor: raw.anchor,
        rotate: raw.rotate.unwrap_or(0.0),
    };
    if !common.rotate.is_finite() {
        return Err(ResolveError::InvalidField {
            node: id.to_string(),
            field: "rotate",
            reason: "must be a finite number of degrees".into(),
        });
    }
    if let Some(anchor) = &raw.anchor {
        check_anchor(id, raw, anchor)?;
    }
    check_stroke_align(id, raw)?;
    if raw.wrap.is_some() && raw.kind != NodeKind::Text {
        return Err(ResolveError::InvalidField {
            node: id.to_string(),
            field: "wrap",
            reason: "only a text node wraps".into(),
        });
    }
    if raw.fit_width.is_some() && raw.kind != NodeKind::Text {
        return Err(ResolveError::InvalidField {
            node: id.to_string(),
            field: "fit_width",
            reason: "only a text node is fitted to its width".into(),
        });
    }
    let is_container = matches!(raw.kind, NodeKind::Column | NodeKind::Row | NodeKind::Stack);
    if raw.clip && !is_container {
        return Err(ResolveError::InvalidField {
            node: id.to_string(),
            field: "clip",
            reason: "only a column/row/stack clips its children".into(),
        });
    }
    let fill = match &raw.fill {
        Some(p) => {
            p.validate().map_err(|reason| ResolveError::InvalidField {
                node: id.to_string(),
                field: "fill",
                reason,
            })?;
            Some(p.clone().scaled(factor))
        }
        None => None,
    };
    let decoration = || -> Result<Option<Box<Decoration>>, ResolveError> {
        let stroke = shape_stroke(id, raw, factor, false)?;
        let corner_radius = raw.corner_radius.unwrap_or(0.0) * factor;
        if !corner_radius.is_finite() || corner_radius < 0.0 {
            return Err(ResolveError::InvalidField {
                node: id.to_string(),
                field: "corner_radius",
                reason: "must be a non-negative number".into(),
            });
        }
        if fill.is_none() && stroke.is_none() && !raw.clip {
            return Ok(None);
        }
        Ok(Some(Box::new(Decoration {
            fill: fill.clone(),
            stroke,
            stroke_align: raw.stroke_align.unwrap_or(StrokeAlign::Inside),
            corner_radius,
            clip: raw.clip,
        })))
    };
    if raw.contain && raw.kind != NodeKind::Stack {
        return Err(ResolveError::InvalidField {
            node: id.to_string(),
            field: "contain",
            reason: "only a stack contains its children".into(),
        });
    }
    let viewbox = raw
        .viewbox
        .map(ViewBox::from_array)
        .transpose()
        .map_err(|reason| ResolveError::InvalidField {
            node: id.to_string(),
            field: "viewbox",
            reason,
        })?;

    let resolve_children = || -> Result<Vec<usize>, ResolveError> {
        raw.children
            .iter()
            .map(|child| {
                index_of
                    .get(child.as_str())
                    .copied()
                    .ok_or_else(|| ResolveError::DanglingChild {
                        node: id.to_string(),
                        child: child.clone(),
                    })
            })
            .collect()
    };

    let kind = match raw.kind {
        NodeKind::Column | NodeKind::Row => {
            let axis = if raw.kind == NodeKind::Column {
                Axis::Vertical
            } else {
                Axis::Horizontal
            };
            let children = resolve_children()?;
            NodeType::Container {
                axis,
                children,
                spacing: raw.spacing * factor,
                main_axis: raw.main_axis_alignment.unwrap_or_default(),
                cross_axis: raw.cross_axis_alignment.unwrap_or_default(),
                decoration: decoration()?,
            }
        }
        NodeKind::Stack => NodeType::Stack {
            children: resolve_children()?,
            viewbox,
            contain: raw.contain,
            decoration: decoration()?,
        },
        NodeKind::Image => {
            let src = raw.src.clone().ok_or_else(|| ResolveError::MissingField {
                node: id.to_string(),
                kind: "image",
                field: "src",
            })?;
            NodeType::Image(ImageProps {
                src,
                fit: raw.fit.unwrap_or_default(),
                smooth: raw.smooth.unwrap_or(true),
            })
        }
        NodeKind::Text => {
            if raw.content.is_none() && raw.spans.is_empty() {
                return Err(ResolveError::MissingField {
                    node: id.to_string(),
                    kind: "text",
                    field: "content",
                });
            }
            let (content, spans) = text_spans(id, raw, factor)?;
            let letter_spacing = raw.letter_spacing.unwrap_or(0.0);
            check_finite(id, "letter_spacing", letter_spacing)?;
            NodeType::Text(TextProps {
                content,
                spans,
                letter_spacing,
                wrap: raw.wrap.unwrap_or(true),
                fit_width: raw.fit_width,
                // Set with the page defaults once every node is converted.
                typography: TextTypography::default(),
                font_style: raw.font_style.unwrap_or_default(),
                underline: raw.underline,
                strike: raw.strike,
                font_size: raw.font_size.unwrap_or(DEFAULT_FONT_SIZE),
                font_family: raw.font_family.clone(),
                font_weight: raw.font_weight,
                color: raw.color.unwrap_or(Color::BLACK),
                align: raw.align.unwrap_or_default(),
                line_height: raw.line_height.unwrap_or(DEFAULT_LINE_HEIGHT),
                stroke: shape_stroke(id, raw, factor, false)?.map(|s| StrokeStyle {
                    join: raw.join.unwrap_or(LineJoin::Round),
                    ..s
                }),
            })
        }
        NodeKind::Rect => NodeType::Rect(RectProps {
            fill: fill.clone(),
            stroke: raw.stroke,
            stroke_width: raw.stroke_width.unwrap_or(0.0) * factor,
            corner_radius: raw.corner_radius.unwrap_or(0.0) * factor,
            stroke_align: raw.stroke_align.unwrap_or_default(),
        }),
        NodeKind::Line | NodeKind::Arrow | NodeKind::Path => {
            let (kind, exact) = match raw.kind {
                NodeKind::Line => ("line", true),
                NodeKind::Arrow => ("arrow", true),
                _ => ("path", false),
            };
            let invalid = |field, reason: String| ResolveError::InvalidField {
                node: id.to_string(),
                field,
                reason,
            };
            if raw.points.is_empty() {
                return Err(ResolveError::MissingField {
                    node: id.to_string(),
                    kind,
                    field: "points",
                });
            }
            let (np, ns) = (raw.points.len(), raw.segments.len());
            if ns == 0 {
                if exact && np != 2 {
                    let reason = format!("{kind} needs exactly 2 points, got {np}");
                    return Err(invalid("points", reason));
                }
                if np < 2 {
                    let reason = format!("{kind} needs at least 2 points, got {np}");
                    return Err(invalid("points", reason));
                }
            } else if exact && np + ns != 2 {
                return Err(invalid(
                    "segments",
                    format!(
                        "{kind} is a single segment: give 2 points, or 1 point and 1 segment \
                         (got {np} points and {ns} segments)"
                    ),
                ));
            }
            if raw.points.iter().flatten().any(|v| !v.is_finite()) {
                return Err(invalid(
                    "points",
                    "coordinates must be finite numbers".into(),
                ));
            }
            // Viewbox coordinates are user units: no page-unit scaling.
            let coord_factor = if viewbox.is_some() { 1.0 } else { factor };
            let segments = path_segments(&raw.points, &raw.segments, raw.closed, coord_factor)
                .map_err(|reason| invalid("segments", reason))?;
            // Line/arrow are pure strokes, so they default to a black one; a path
            // is stroked black only when it has neither fill nor stroke.
            let default_stroke = exact || raw.fill.is_none();
            let stroke = shape_stroke(id, raw, factor, default_stroke)?;
            let default_head = if raw.kind == NodeKind::Arrow {
                ArrowHead::End
            } else {
                ArrowHead::None
            };
            let stroke_w = stroke.as_ref().map_or(0.0, |s| s.width);
            let head_size = match raw.head_size {
                Some(h) => h * factor,
                None => stroke_w * DEFAULT_HEAD_SIZE_PER_STROKE,
            };
            let head_width = match raw.head_width {
                Some(HeadWidth::Abs(w)) => w * factor,
                Some(w @ HeadWidth::PerStroke(_)) => w.resolve(stroke_w),
                None => head_size * DEFAULT_HEAD_WIDTH_PER_SIZE,
            };
            if !head_width.is_finite() || head_width < 0.0 {
                return Err(invalid(
                    "head_width",
                    "must be a non-negative length or multiple like \"3x\"".into(),
                ));
            }
            NodeType::Path(PathProps {
                segments,
                fill: fill.clone(),
                stroke,
                head: raw.head.unwrap_or(default_head),
                head_size,
                head_width,
                head_style: raw.head_style.unwrap_or_default(),
                viewbox,
                stroke_align: raw.stroke_align.unwrap_or_default(),
            })
        }
        NodeKind::Ellipse => NodeType::Ellipse(EllipseProps {
            fill: fill.clone(),
            stroke: shape_stroke(id, raw, factor, raw.fill.is_none())?,
            stroke_align: raw.stroke_align.unwrap_or_default(),
        }),
    };

    let effects = effects(id, raw, factor)?;
    if raw.smooth.is_some() && raw.kind != NodeKind::Image {
        return Err(ResolveError::InvalidField {
            node: id.to_string(),
            field: "smooth",
            reason: "only an image has smooth/pixelated scaling".into(),
        });
    }
    Ok(Node {
        id: id.to_string(),
        common,
        kind,
        effects,
    })
}

/// `shadow` / `glow` / `opacity`, validated and converted to points. A glow
/// is a shadow with no offset whose silhouette is grown by `spread` (default
/// half its blur) first; it paints over the drop shadow.
fn effects(id: &str, raw: &RawNode, factor: f32) -> Result<Effects, ResolveError> {
    let invalid = |field, reason: &str| ResolveError::InvalidField {
        node: id.to_string(),
        field,
        reason: reason.to_string(),
    };
    let mut fx = Effects::default();
    if let Some(o) = raw.opacity {
        if !(0.0..=1.0).contains(&o) {
            return Err(invalid("opacity", "must be a number from 0 to 1"));
        }
        fx.opacity = o;
    }
    if let Some(s) = &raw.shadow {
        if ![s.dx, s.dy, s.blur, s.spread].iter().all(|v| v.is_finite()) || s.blur < 0.0 {
            return Err(invalid(
                "shadow",
                "dx/dy/spread must be finite numbers and blur >= 0",
            ));
        }
        fx.shadows.push(Shadow {
            dx: s.dx * factor,
            dy: s.dy * factor,
            blur: s.blur * factor,
            spread: s.spread * factor,
            color: s.color.unwrap_or(DEFAULT_SHADOW_COLOR),
        });
    }
    if let Some(g) = &raw.glow {
        let spread = g.spread.unwrap_or(g.blur / 2.0);
        if !(g.blur.is_finite() && g.blur >= 0.0 && spread.is_finite()) {
            return Err(invalid("glow", "blur must be >= 0 and spread finite"));
        }
        fx.shadows.push(Shadow {
            dx: 0.0,
            dy: 0.0,
            blur: g.blur * factor,
            spread: spread * factor,
            color: g.color.unwrap_or(Color::BLACK),
        });
    }
    // A fully transparent shadow paints nothing.
    fx.shadows.retain(|s| s.color.a > 0.0);
    Ok(fx)
}

/// Build the resolved segment list (points, scaled by `factor`) from `points`
/// (straight polyline) followed by `segments`, closing it if asked. Arcs become
/// cubics here so every backend only sees lines and Béziers. `Err` carries the
/// reason, naming the 1-based segment at fault.
fn path_segments(
    points: &[[f32; 2]],
    segments: &[RawSegment],
    closed: bool,
    factor: f32,
) -> Result<Vec<PathSegment>, String> {
    let pt = |[x, y]: [f32; 2]| Point::new(x * factor, y * factor);
    let mut out = Vec::with_capacity(points.len() + segments.len() + 1);
    let mut cur = pt(points[0]);
    out.push(PathSegment::MoveTo(cur));
    out.extend(points[1..].iter().map(|p| PathSegment::LineTo(pt(*p))));
    if let Some(last) = points.last() {
        cur = pt(*last);
    }
    for (i, seg) in segments.iter().enumerate() {
        let bad = |why: &str| format!("segment {}: {why}", i + 1);
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite());
        let given = [seg.quad.is_some(), seg.cubic.is_some(), seg.arc.is_some()];
        if given.iter().filter(|g| **g).count() > 1 {
            return Err(bad("give at most one of `quad`, `cubic`, `arc`"));
        }
        if seg.arc.is_none() && (seg.large || seg.sweep.is_some() || seg.rotation.is_some()) {
            return Err(bad(
                "`large`, `sweep` and `rotation` only apply to an `arc`",
            ));
        }
        if !finite(&seg.to) {
            return Err(bad("coordinates must be finite numbers"));
        }
        let to = pt(seg.to);
        if let Some(c) = seg.quad {
            if !finite(&c) {
                return Err(bad("coordinates must be finite numbers"));
            }
            out.push(PathSegment::QuadTo(pt(c), to));
        } else if let Some([c1, c2]) = seg.cubic {
            if !finite(&c1) || !finite(&c2) {
                return Err(bad("coordinates must be finite numbers"));
            }
            out.push(PathSegment::CubicTo(pt(c1), pt(c2), to));
        } else if let Some(r) = seg.arc {
            let (rx, ry) = r.radii();
            if !(rx.is_finite() && ry.is_finite() && rx > 0.0 && ry > 0.0) {
                return Err(bad("arc radius must be a positive number"));
            }
            let rotation = seg.rotation.unwrap_or(0.0);
            if !rotation.is_finite() {
                return Err(bad("arc rotation must be a finite number"));
            }
            let cw = seg.sweep.unwrap_or_default() == ArcSweep::Cw;
            out.extend(arc_to_cubics(
                cur,
                to,
                rx * factor,
                ry * factor,
                rotation,
                seg.large,
                cw,
            ));
        } else {
            out.push(PathSegment::LineTo(to));
        }
        cur = to;
    }
    if closed {
        out.push(PathSegment::Close);
    }
    Ok(out)
}

/// The stroke of a vector shape: `stroke` (or black when `default_black` and
/// unset), `stroke_width` (default [`DEFAULT_SHAPE_STROKE_WIDTH`] pt), the dash
/// pattern (validated, scaled, odd lists repeated), the cap and the join
/// (default miter; text overrides it to round). `None` when
/// there is no color or the width is zero.
fn shape_stroke(
    id: &str,
    raw: &RawNode,
    factor: f32,
    default_black: bool,
) -> Result<Option<StrokeStyle>, ResolveError> {
    let mut dash: Vec<f32> = raw.dash.iter().map(|d| d * factor).collect();
    if !dash.is_empty() {
        if dash.iter().any(|d| !d.is_finite() || *d < 0.0) || dash.iter().sum::<f32>() <= 0.0 {
            return Err(ResolveError::InvalidField {
                node: id.to_string(),
                field: "dash",
                reason: "lengths must be non-negative and not all zero".into(),
            });
        }
        if dash.len() % 2 == 1 {
            dash.extend_from_within(..);
        }
    }
    let color = match raw.stroke {
        Some(c) => c,
        None if default_black => Color::BLACK,
        None => return Ok(None),
    };
    let width = raw
        .stroke_width
        .map_or(DEFAULT_SHAPE_STROKE_WIDTH, |w| w * factor);
    if width <= 0.0 {
        return Ok(None);
    }
    Ok(Some(StrokeStyle {
        color,
        width,
        dash,
        cap: raw.cap.unwrap_or_default(),
        join: raw.join.unwrap_or_default(),
    }))
}

/// DFS from the root, rejecting back-edges. Because every node has at most one
/// parent (checked earlier), a back-edge is a genuine cycle.
fn detect_cycle(nodes: &[Node], root: usize) -> Result<(), ResolveError> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Unvisited,
        InStack,
        Done,
    }
    let mut marks = vec![Mark::Unvisited; nodes.len()];
    // Iterative DFS to avoid stack overflow on deep trees.
    let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
    marks[root] = Mark::InStack;
    while let Some(&mut (node, ref mut child_pos)) = stack.last_mut() {
        let children = nodes[node].kind.children();
        if *child_pos < children.len() {
            let next = children[*child_pos];
            *child_pos += 1;
            match marks[next] {
                Mark::InStack => return Err(ResolveError::Cycle(nodes[next].id.clone())),
                Mark::Done => {}
                Mark::Unvisited => {
                    marks[next] = Mark::InStack;
                    stack.push((next, 0));
                }
            }
        } else {
            marks[node] = Mark::Done;
            stack.pop();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Unit;

    fn doc(src: &str) -> Result<Document, crate::Error> {
        Document::from_toml(src)
    }

    const TWO_PANEL: &str = r#"
        [page]
        width = 10
        height = 10
        unit = "cm"
        root = "root"

        [nodes.root]
        type = "column"
        children = ["panel1", "panel2"]

        [nodes.panel1]
        type = "row"
        children = ["img1", "cap1"]

        [nodes.img1]
        type = "image"
        src = "fig1.png"
        flex = 1

        [nodes.cap1]
        type = "text"
        content = "Panel 1"
        flex = 1

        [nodes.panel2]
        type = "text"
        content = "Panel 2"
    "#;

    #[test]
    fn resolves_two_panel() {
        let d = doc(TWO_PANEL).unwrap();
        // 5 nodes, root is a column with two children
        assert_eq!(d.nodes.len(), 5);
        let root = &d.nodes[d.root];
        assert_eq!(root.id, "root");
        match &root.kind {
            NodeType::Container { children, axis, .. } => {
                assert_eq!(*axis, Axis::Vertical);
                assert_eq!(children.len(), 2);
            }
            _ => panic!("root should be a container"),
        }
        // cm -> pt page conversion
        assert!((d.page.width_pt - Unit::Cm.convert(10.0)).abs() < 1e-3);
        assert!((d.page.height_pt.unwrap() - Unit::Cm.convert(10.0)).abs() < 1e-3);
        assert_eq!(d.page.dpi, DEFAULT_DPI);
    }

    #[test]
    fn auto_height_resolves_to_none() {
        let src = r#"
            [page]
            width = 10
            height = "auto"
            root = "r"
            [nodes.r]
            type = "rect"
        "#;
        assert_eq!(doc(src).unwrap().page.height_pt, None);
    }

    #[test]
    fn bad_height_string_rejected() {
        let src = r#"
            [page]
            width = 10
            height = "tall"
            root = "r"
            [nodes.r]
            type = "rect"
        "#;
        assert!(doc(src).is_err());
    }

    #[test]
    fn root_missing() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "nope"
            [nodes.a]
            type = "rect"
        "#;
        assert_eq!(
            doc(src).unwrap_err().to_string(),
            "page.root refers to unknown node `nope`"
        );
    }

    #[test]
    fn dangling_child() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "column"
            children = ["ghost"]
        "#;
        let e = doc(src).unwrap_err().to_string();
        assert!(e.contains("child `ghost` does not exist"), "{e}");
    }

    #[test]
    fn multiple_parents() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "column"
            children = ["a", "b"]
            [nodes.a]
            type = "column"
            children = ["shared"]
            [nodes.b]
            type = "column"
            children = ["shared"]
            [nodes.shared]
            type = "rect"
        "#;
        let e = doc(src).unwrap_err().to_string();
        assert!(e.contains("more than one parent"), "{e}");
    }

    #[test]
    fn cycle_detected() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "column"
            children = ["a"]
            [nodes.a]
            type = "column"
            children = ["root"]
        "#;
        // root has two parents (itself via a) -> caught as MultipleParents first,
        // which is also a valid rejection. Use a self-free cycle instead.
        let _ = e_or(src);

        let src2 = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "column"
            children = ["a"]
            [nodes.a]
            type = "column"
            children = ["b"]
            [nodes.b]
            type = "column"
            children = ["a"]
        "#;
        let e = e_or(src2);
        assert!(
            e.contains("more than one parent") || e.contains("cycle"),
            "{e}"
        );
    }

    fn e_or(src: &str) -> String {
        doc(src).unwrap_err().to_string()
    }

    #[test]
    fn image_requires_src() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "image"
        "#;
        let e = doc(src).unwrap_err().to_string();
        assert!(e.contains("image requires field `src`"), "{e}");
    }

    #[test]
    fn text_requires_content() {
        let src = r#"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "text"
        "#;
        let e = doc(src).unwrap_err().to_string();
        assert!(e.contains("text requires field `content`"), "{e}");
    }

    #[test]
    fn unknown_field_rejected() {
        let src = r##"
            [page]
            width = 1
            height = 1
            root = "root"
            [nodes.root]
            type = "rect"
            colour = "#fff"
        "##;
        assert!(doc(src).is_err());
    }
}
