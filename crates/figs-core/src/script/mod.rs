//! Rhai scripting front-end (`.figs` / `.rhai`).
//!
//! A script *evaluates to* the existing [`RawDocument`] IR, so everything
//! downstream — [`crate::schema::resolve`], [`crate::layout`], [`crate::render`],
//! and the editor's TOML round-trip — is reused unchanged. Rhai natively provides
//! variables (`let`), arithmetic, and reusable components (`fn`).
//!
//! ## Authoring
//!
//! Build nodes with `text` / `rect` / `image` / `col` / `row` / `stack` (and the
//! vector shapes `line` / `arrow` / `ellipse` / `path`, whose outline continues
//! with the curve builders `.to` / `.quad` / `.cubic` / `.arc`), set properties by
//! **chaining methods** (Rhai has no keyword arguments, so chaining is how you get
//! concise, named-ish properties — no map literals), and finish with a single
//! `page(width, height, root)` call (`height` may be `"auto"` to fit the content):
//!
//! ```text
//! page(20, 12,
//!     col([
//!         text("Title").size(48).bold().center(),
//!         rect().fill("#e0635a").height(0.2),
//!     ]).spacing(0.5).pad(1.0).cross("center"),
//! ).unit("cm").background("#ffffff")
//! ```
//!
//! Fills take a colour string or a gradient built with `linear(angle, stops)`
//! / `linear([x1, y1], [x2, y2], stops)` / `radial(stops)` /
//! `radial([cx, cy], radius, stops)` (box-relative; `.focal(fx, fy)` and
//! `.absolute()` refine them), where `stops` is `[[0, "#…"], [1, "#…"]]` or
//! just `["#…", "#…"]`. Containers take `.fill` / `.stroke` /
//! `.stroke_width` / `.radius` / `.dash` / `.clip()` as their background and
//! border:
//!
//! ```text
//! col([...]).fill(linear(90, ["#0b1a3a", "#1d3b6e"])).stroke("#ffffff").radius(0.4).clip()
//! ```
//!
//! [`eval_script`] then lowers the tree into a flat, id-keyed [`RawDocument`].

use std::collections::HashMap;
use std::path::Path;

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map, NativeCallContext, Position};

use crate::geom::{
    AnchorValue, ArcSweep, ArrowHead, BaselineKind, Color, CrossAxisAlign, EdgeSides, Edges,
    HeadStyle, HeadWidth, ImageFit, LineCap, LineJoin, LinearDir, MainAxisAlign, Paint,
    StrokeAlign, TextAlign,
};
use crate::schema::{
    ArcRadius, NodeKind, PageHeight, RawAttach, RawDocument, RawGlow, RawNode, RawPage, RawSegment,
    RawShadow,
};
use crate::units::Unit;

// ---------------------------------------------------------------------------
// Script-side value types (passed around inside Rhai)
// ---------------------------------------------------------------------------

/// A node under construction: a `RawNode` (its `children` ids filled at lowering)
/// plus the child subtrees.
#[derive(Clone)]
pub struct ScriptNode {
    node: RawNode,
    children: Vec<ScriptNode>,
    /// Where `x`/`y` were last set (editor metadata; see [`sites`]).
    sites: sites::RawSites,
    /// Script-only key -> value notes (`.meta(key, v)` / `.meta(key)`), never
    /// lowered: how framework components tell each other about a node (a plot
    /// asks `labeled` for panel-label room). See `prelude::register_meta`.
    meta: Map,
    /// Set by `.at_baseline(x, y)`: the offset is the pen origin of the
    /// first line (left end of its baseline). The `anchor` that says so
    /// depends on the final `rotate`, so it is filled in at lowering.
    baseline_origin: bool,
    /// Identity of the node as built (copies made by setters keep it), so a
    /// connector can name a node value: `connect(a, b)`.
    uid: u64,
    /// `.id(s)`: the node's id in the lowered document (else synthesised).
    explicit_id: Option<String>,
    /// A connector's two ends, resolved to ids at lowering.
    link: Option<Box<[LinkRef; 2]>>,
}

/// One end of a connector as the script named it.
#[derive(Clone, Debug)]
enum LinkRef {
    /// A node value (its [`ScriptNode::uid`]).
    Node(u64),
    /// An id given with `.id(s)`.
    Id(String),
}

/// A fresh [`ScriptNode::uid`].
fn next_uid() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The whole document a script returns: a page plus its root node.
#[derive(Clone)]
pub struct ScriptDoc {
    page: RawPage,
    root: ScriptNode,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A Rhai evaluation error, carrying source position when available.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ScriptError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl ScriptError {
    fn from_eval(e: Box<EvalAltResult>) -> Self {
        let pos = e.position();
        let line = pos.line();
        let column = pos.position();
        let base = e.to_string();
        // rhai's `Display` for some variants already embeds a position; only add
        // our own prefix when it doesn't, to avoid duplication.
        let message = if base.contains("line ") {
            base
        } else {
            match (line, column) {
                (Some(l), Some(c)) => format!("line {l}, col {c}: {base}"),
                (Some(l), None) => format!("line {l}: {base}"),
                _ => base,
            }
        };
        ScriptError {
            message,
            line,
            column,
        }
    }

    fn not_a_document() -> Self {
        ScriptError {
            message: "the script must end with a `page(width, height, root)` call".to_string(),
            line: None,
            column: None,
        }
    }
}

/// Build a Rhai runtime error with no position; Rhai fills in the call site.
fn err(msg: impl Into<String>) -> Box<EvalAltResult> {
    let s: String = msg.into();
    Box::new(EvalAltResult::ErrorRuntime(s.into(), Position::NONE))
}

// ---------------------------------------------------------------------------
// Value coercion
// ---------------------------------------------------------------------------

/// Accept either an integer (`16`) or a float (`16.0`) as `f32`.
fn dyn_to_f32(d: &Dynamic) -> Option<f32> {
    if let Ok(f) = d.as_float() {
        Some(f as f32)
    } else if let Ok(i) = d.as_int() {
        Some(i as f32)
    } else {
        None
    }
}

// String -> enum tables (kept in sync with the `rename_all` serde rules in geom.rs/units.rs).
fn parse_main_axis(s: &str) -> Option<MainAxisAlign> {
    Some(match s {
        "start" => MainAxisAlign::Start,
        "center" => MainAxisAlign::Center,
        "end" => MainAxisAlign::End,
        "space_between" => MainAxisAlign::SpaceBetween,
        "space_around" => MainAxisAlign::SpaceAround,
        "space_evenly" => MainAxisAlign::SpaceEvenly,
        _ => return None,
    })
}
fn parse_cross_axis(s: &str) -> Option<CrossAxisAlign> {
    Some(match s {
        "start" => CrossAxisAlign::Start,
        "center" => CrossAxisAlign::Center,
        "end" => CrossAxisAlign::End,
        "stretch" => CrossAxisAlign::Stretch,
        "baseline" => CrossAxisAlign::Baseline,
        "cap_middle" => CrossAxisAlign::CapMiddle,
        "axis" => CrossAxisAlign::Axis,
        _ => return None,
    })
}
fn parse_self_align(s: &str) -> Option<CrossAxisAlign> {
    Some(match s {
        "start" => CrossAxisAlign::Start,
        "center" => CrossAxisAlign::Center,
        "end" => CrossAxisAlign::End,
        "stretch" => CrossAxisAlign::Stretch,
        _ => return None,
    })
}
/// A pixel density: a number >= 0 or `"file"`.
fn dyn_to_dpi(v: &Dynamic) -> Option<crate::geom::Dpi> {
    if v.clone().into_string().is_ok_and(|s| s == "file") {
        return Some(crate::geom::Dpi::File);
    }
    dyn_to_f32(v)
        .filter(|x| x.is_finite() && *x >= 0.0)
        .map(crate::geom::Dpi::Value)
}
fn parse_text_align(s: &str) -> Option<TextAlign> {
    Some(match s {
        "left" => TextAlign::Left,
        "center" => TextAlign::Center,
        "right" => TextAlign::Right,
        "justify" => TextAlign::Justify,
        _ => return None,
    })
}
fn parse_fit(s: &str) -> Option<ImageFit> {
    Some(match s {
        "contain" => ImageFit::Contain,
        "cover" => ImageFit::Cover,
        "fill" => ImageFit::Fill,
        _ => return None,
    })
}
fn parse_cap(s: &str) -> Option<LineCap> {
    Some(match s {
        "butt" => LineCap::Butt,
        "round" => LineCap::Round,
        "square" => LineCap::Square,
        _ => return None,
    })
}
fn parse_join(s: &str) -> Option<LineJoin> {
    Some(match s {
        "miter" => LineJoin::Miter,
        "round" => LineJoin::Round,
        "bevel" => LineJoin::Bevel,
        _ => return None,
    })
}
fn parse_head(s: &str) -> Option<ArrowHead> {
    Some(match s {
        "none" => ArrowHead::None,
        "start" => ArrowHead::Start,
        "end" => ArrowHead::End,
        "both" => ArrowHead::Both,
        _ => return None,
    })
}
fn parse_head_style(s: &str) -> Option<HeadStyle> {
    Some(match s {
        "triangle" => HeadStyle::Triangle,
        "stealth" => HeadStyle::Stealth,
        "open" => HeadStyle::Open,
        "barb_left" => HeadStyle::BarbLeft,
        "barb_right" => HeadStyle::BarbRight,
        _ => return None,
    })
}
fn parse_unit(s: &str) -> Option<Unit> {
    Some(match s {
        "cm" => Unit::Cm,
        "mm" => Unit::Mm,
        "in" => Unit::In,
        "pt" => Unit::Pt,
        "px" => Unit::Px,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Builders & chainable setters
// ---------------------------------------------------------------------------

fn blank_node(kind: NodeKind) -> RawNode {
    RawNode {
        kind,
        name: None,
        flex: None,
        width: None,
        height: None,
        aspect_ratio: None,
        margin: Edges::default(),
        padding: Edges::default(),
        x: None,
        y: None,
        anchor: None,
        rotate: None,
        axis: None,
        viewbox: None,
        contain: false,
        clip: false,
        shadow: None,
        glow: None,
        opacity: None,
        children: Vec::new(),
        spacing: 0.0,
        main_axis_alignment: None,
        cross_axis_alignment: None,
        baseline_pitch: None,
        equal_height: false,
        overflow: None,
        content: None,
        font_size: None,
        font_family: None,
        font_weight: None,
        font_style: None,
        underline: false,
        strike: false,
        letter_spacing: None,
        spans: Vec::new(),
        color: None,
        align: None,
        line_height: None,
        wrap: None,
        fit_width: None,
        script_metrics: None,
        font_synthesis: None,
        synthetic_bold_width: None,
        synthetic_slant: None,
        src: None,
        fit: None,
        smooth: None,
        fill: None,
        stroke: None,
        stroke_width: None,
        stroke_align: None,
        corner_radius: None,
        points: Vec::new(),
        segments: Vec::new(),
        closed: false,
        dash: Vec::new(),
        cap: None,
        join: None,
        head: None,
        head_size: None,
        head_width: None,
        head_style: None,
        from: None,
        to: None,
        from_at: None,
        to_at: None,
        from_box: None,
        to_box: None,
        route: None,
        gap: None,
        band: None,
        from_shape: None,
        dpi: None,
        overhang: Default::default(),
        align_self: None,
        min_width: None,
        min_height: None,
    }
}

fn leaf(kind: NodeKind) -> ScriptNode {
    ScriptNode {
        node: blank_node(kind),
        children: Vec::new(),
        sites: Default::default(),
        meta: Map::new(),
        baseline_origin: false,
        uid: next_uid(),
        explicit_id: None,
        link: None,
    }
}

/// Apply a mutation to a node and return it (enables `.a().b()` chaining).
fn set_node(mut n: ScriptNode, f: impl FnOnce(&mut RawNode)) -> ScriptNode {
    f(&mut n.node);
    n
}

fn set_doc(mut d: ScriptDoc, f: impl FnOnce(&mut RawPage)) -> ScriptDoc {
    f(&mut d.page);
    d
}

/// Container children: nodes; an array among them is spliced in (a helper
/// may return several nodes, e.g. connectors).
fn cast_children(children: Array) -> Result<Vec<ScriptNode>, Box<EvalAltResult>> {
    let mut out = Vec::with_capacity(children.len());
    for child in children {
        if child.is_array() {
            out.extend(cast_children(child.cast::<Array>())?);
            continue;
        }
        let node = child
            .try_cast::<ScriptNode>()
            .ok_or_else(|| {
                err("container children must be nodes (col/row/stack/text/rect/image/line/arrow/ellipse/path/connect) or arrays of nodes")
            })?;
        out.push(node);
    }
    Ok(out)
}

fn make_container(kind: NodeKind, children: Array) -> Result<ScriptNode, Box<EvalAltResult>> {
    Ok(ScriptNode {
        node: blank_node(kind),
        children: cast_children(children)?,
        sites: Default::default(),
        meta: Map::new(),
        baseline_origin: false,
        uid: next_uid(),
        explicit_id: None,
        link: None,
    })
}

/// A two-point `line`/`arrow` from `(x1, y1)` to `(x2, y2)` (page units, in the
/// node's own box).
fn segment(
    kind: NodeKind,
    name: &str,
    coords: [Dynamic; 4],
) -> Result<ScriptNode, Box<EvalAltResult>> {
    let mut v = [0.0f32; 4];
    for (o, d) in v.iter_mut().zip(&coords) {
        *o = dyn_to_f32(d)
            .ok_or_else(|| err(format!("`{name}` expects numbers (x1, y1, x2, y2)")))?;
    }
    let mut sn = leaf(kind);
    sn.node.points = vec![[v[0], v[1]], [v[2], v[3]]];
    Ok(sn)
}

/// `[[x, y], …]` -> points, for `path`.
fn cast_points(points: Array) -> Result<Vec<[f32; 2]>, Box<EvalAltResult>> {
    let bad = || err("`path` expects an array of [x, y] number pairs");
    points
        .into_iter()
        .map(|p| {
            let xy = p.try_cast::<Array>().ok_or_else(bad)?;
            match xy.as_slice() {
                [x, y] => Ok([
                    dyn_to_f32(x).ok_or_else(bad)?,
                    dyn_to_f32(y).ok_or_else(bad)?,
                ]),
                _ => Err(bad()),
            }
        })
        .collect()
}

/// Result of a chainable node setter.
type NodeResult = Result<ScriptNode, Box<EvalAltResult>>;

/// `N` numbers for the curve builder `name` (signature in `sig`).
fn nums<const N: usize>(
    name: &str,
    sig: &str,
    args: [Dynamic; N],
) -> Result<[f32; N], Box<EvalAltResult>> {
    let mut v = [0.0f32; N];
    for (o, d) in v.iter_mut().zip(&args) {
        *o = dyn_to_f32(d).ok_or_else(|| err(format!("`{name}` expects numbers ({sig})")))?;
    }
    Ok(v)
}

/// Append a curve-builder segment to a `path`/`line`/`arrow` node.
fn push_segment(mut n: ScriptNode, name: &str, seg: RawSegment) -> NodeResult {
    if !matches!(
        n.node.kind,
        NodeKind::Path | NodeKind::Line | NodeKind::Arrow
    ) {
        return Err(err(format!("`{name}` continues a path/line/arrow outline")));
    }
    n.node.segments.push(seg);
    Ok(n)
}

/// `.arc(r, x, y[, opts])`: `r` is a number or `[rx, ry]`; `opts` a map with
/// optional `sweep` ("cw"/"ccw"), `large` (bool) and `rotation` (degrees).
fn arc_segment(
    r: Dynamic,
    x: Dynamic,
    y: Dynamic,
    opts: Map,
) -> Result<RawSegment, Box<EvalAltResult>> {
    let bad_r = || err("`arc` radius must be a number or [rx, ry]");
    let radius = match dyn_to_f32(&r) {
        Some(r) => ArcRadius::Circle(r),
        None => {
            let a = r.try_cast::<Array>().ok_or_else(bad_r)?;
            match a.as_slice() {
                [rx, ry] => ArcRadius::Ellipse([
                    dyn_to_f32(rx).ok_or_else(bad_r)?,
                    dyn_to_f32(ry).ok_or_else(bad_r)?,
                ]),
                _ => return Err(bad_r()),
            }
        }
    };
    let [x, y] = nums("arc", "r, x, y[, opts]", [x, y])?;
    let mut seg = RawSegment::line([x, y]);
    seg.arc = Some(radius);
    for (k, v) in opts {
        match k.as_str() {
            "sweep" => {
                let s = v
                    .into_string()
                    .map_err(|_| err(r#"`arc`: `sweep` must be "cw" or "ccw""#))?;
                seg.sweep = Some(match s.as_str() {
                    "cw" => ArcSweep::Cw,
                    "ccw" => ArcSweep::Ccw,
                    _ => return Err(err(format!("`arc`: invalid sweep `{s}` (cw, ccw)"))),
                });
            }
            "large" => {
                seg.large = v
                    .as_bool()
                    .map_err(|_| err("`arc`: `large` must be true or false"))?;
            }
            "rotation" => {
                seg.rotation =
                    Some(dyn_to_f32(&v).ok_or_else(|| err("`arc`: `rotation` must be a number"))?);
            }
            other => {
                return Err(err(format!(
                    "`arc`: unknown option `{other}` (sweep, large, rotation)"
                )))
            }
        }
    }
    Ok(seg)
}

/// One end of `connect(a, b)`: a node value or an id string.
fn link_ref(which: &str, v: Dynamic) -> Result<LinkRef, Box<EvalAltResult>> {
    if v.is_string() {
        return Ok(LinkRef::Id(v.into_string().unwrap_or_default()));
    }
    match v.try_cast::<ScriptNode>() {
        Some(n) => Ok(LinkRef::Node(n.uid)),
        None => Err(err(format!(
            "connect(): `{which}` must be a node or a node id (a string given with .id(..))"
        ))),
    }
}

/// An attachment option of `connect`: a named point or `[fx, fy]`.
fn cast_attach(key: &str, v: Dynamic) -> Result<RawAttach, Box<EvalAltResult>> {
    let names = crate::schema::Attach::NAMES;
    if v.is_string() {
        let s = v.into_string().unwrap_or_default();
        crate::schema::Attach::parse(&s).ok_or_else(|| {
            err(format!(
                "connect(): unknown `{key}` point \"{s}\" ({names})"
            ))
        })?;
        return Ok(RawAttach::Named(s));
    }
    let bad = || err(format!("connect(): `{key}` must be one of {names}"));
    let a = v.try_cast::<Array>().ok_or_else(bad)?;
    match a.as_slice() {
        [x, y] => Ok(RawAttach::Frac([
            dyn_to_f32(x).ok_or_else(bad)?,
            dyn_to_f32(y).ok_or_else(bad)?,
        ])),
        _ => Err(bad()),
    }
}

/// A sub-box option of `connect`: `[fx0, fy0, fx1, fy1]`.
fn cast_box(key: &str, v: Dynamic) -> Result<[f32; 4], Box<EvalAltResult>> {
    let bad = || err(format!("connect(): `{key}` must be [fx0, fy0, fx1, fy1]"));
    let a = v.try_cast::<Array>().ok_or_else(bad)?;
    let mut out = [0.0f32; 4];
    if a.len() != 4 {
        return Err(bad());
    }
    for (o, d) in out.iter_mut().zip(&a) {
        *o = dyn_to_f32(d).ok_or_else(bad)?;
    }
    if out[2] < out[0] || out[3] < out[1] {
        return Err(bad());
    }
    Ok(out)
}

/// The options `connect(a, b, opts)` accepts.
const CONNECT_KEYS: &str = "from, to, from_box, to_box, route, head, head_style, head_size, head_width, stroke, color, width, dash, cap, band, fill, radius, gap, opacity, from_shape";

/// `connect(a, b[, opts])`: a connector node from `a` to `b`.
fn connect_fn(a: Dynamic, b: Dynamic, opts: Map) -> NodeResult {
    let mut sn = leaf(NodeKind::Connector);
    sn.link = Some(Box::new([link_ref("a", a)?, link_ref("b", b)?]));
    let r = &mut sn.node;
    let num = |k: &str, v: &Dynamic| {
        dyn_to_f32(v).ok_or_else(|| err(format!("connect(): `{k}` must be a number")))
    };
    let string = |k: &str, v: Dynamic| {
        v.into_string()
            .map_err(|_| err(format!("connect(): `{k}` must be a string")))
    };
    for (k, v) in opts {
        let k = k.as_str();
        match k {
            "from" => r.from_at = Some(cast_attach(k, v)?),
            "from_box" => r.from_box = Some(cast_box(k, v)?),
            "to_box" => r.to_box = Some(cast_box(k, v)?),
            "to" => r.to_at = Some(cast_attach(k, v)?),
            "route" => {
                let s = string(k, v)?;
                r.route = Some(crate::schema::ConnectorRoute::parse(&s).ok_or_else(|| {
                    err(format!(
                        "connect(): unknown route \"{s}\" (straight, elbow, curve, cone, outline)"
                    ))
                })?);
            }
            "head" => {
                let s = string(k, v)?;
                r.head = Some(parse_head(&s).ok_or_else(|| {
                    err(format!(
                        "connect(): unknown head \"{s}\" (none, start, end, both)"
                    ))
                })?);
            }
            "head_style" => {
                let s = string(k, v)?;
                r.head_style = Some(parse_head_style(&s).ok_or_else(|| {
                    err(format!("connect(): unknown head_style \"{s}\" (triangle, stealth, open, barb_left, barb_right)"))
                })?);
            }
            "head_size" => r.head_size = Some(num(k, &v)?),
            "head_width" => r.head_width = Some(match dyn_to_f32(&v) {
                Some(w) => HeadWidth::Abs(w),
                None => v
                    .into_string()
                    .ok()
                    .and_then(|s| HeadWidth::parse_ratio(&s))
                    .ok_or_else(|| {
                        err(r#"connect(): `head_width` must be a length or a multiple like "3x""#)
                    })?,
            }),
            "stroke" | "color" => r.stroke = Some(parse_color("connect", &string(k, v)?)?),
            "width" => r.stroke_width = Some(num(k, &v)?),
            "dash" => {
                let a = v
                    .try_cast::<Array>()
                    .ok_or_else(|| err("connect(): `dash` must be an array of numbers"))?;
                r.dash = a.iter().map(|d| num(k, d)).collect::<Result<_, _>>()?;
            }
            "cap" => {
                let s = string(k, v)?;
                r.cap = Some(parse_cap(&s).ok_or_else(|| {
                    err(format!(
                        "connect(): unknown cap \"{s}\" (butt, round, square)"
                    ))
                })?);
            }
            "band" => r.band = Some(num(k, &v)?),
            "from_shape" => {
                let s = string(k, v)?;
                r.from_shape = Some(crate::schema::EndShape::parse(&s).ok_or_else(|| {
                    err(format!(
                        "connect(): unknown from_shape \"{s}\" (box, ellipse)"
                    ))
                })?);
            }
            "fill" => {
                r.fill = Some(if v.is_string() {
                    parse_color("connect", &string(k, v)?)?.into()
                } else {
                    v.try_cast::<Paint>()
                        .ok_or_else(|| err("connect(): `fill` must be a colour or a gradient"))?
                })
            }
            "radius" => r.corner_radius = Some(num(k, &v)?),
            "gap" => {
                r.gap = Some(match dyn_to_f32(&v) {
                    Some(g) => crate::schema::RawGap::Both(g),
                    None => {
                        let a = v.try_cast::<Array>().unwrap_or_default();
                        match a.as_slice() {
                            [g0, g1] => crate::schema::RawGap::Ends([num(k, g0)?, num(k, g1)?]),
                            _ => {
                                return Err(err(
                                    "connect(): `gap` must be a number or [start, end]",
                                ))
                            }
                        }
                    }
                })
            }
            "opacity" => {
                let o = num(k, &v)?;
                if !(0.0..=1.0).contains(&o) {
                    return Err(err("connect(): `opacity` must be from 0 to 1"));
                }
                r.opacity = Some(o);
            }
            _ => {
                return Err(err(format!(
                    "connect(): unknown option \"{k}\" ({CONNECT_KEYS})"
                )))
            }
        }
    }
    Ok(sn)
}

/// `.id(s)`: the node's id in the document (`connect("a", "b")` and TOML
/// name it); `""` clears it.
fn id_setter(mut n: ScriptNode, id: &str) -> NodeResult {
    if id.is_empty() {
        n.explicit_id = None;
        return Ok(n);
    }
    let synthetic = id == "root"
        || id
            .strip_prefix("node")
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
    if synthetic {
        return Err(err(format!(
            "`id`: \"{id}\" is reserved for generated ids (root, node1, node2, …); pick another"
        )));
    }
    n.explicit_id = Some(id.to_string());
    Ok(n)
}

/// `page(width, height, root)`; `height` may be a number or `"auto"` (fit the
/// content — see [`PageHeight::Auto`]).
fn page_fn(
    width: Dynamic,
    height: Dynamic,
    root: Dynamic,
) -> Result<ScriptDoc, Box<EvalAltResult>> {
    let width = dyn_to_f32(&width).ok_or_else(|| err("page(): width must be a number"))?;
    let height = match dyn_to_f32(&height) {
        Some(h) => PageHeight::Fixed(h),
        None if height.clone().into_string().is_ok_and(|s| s == "auto") => PageHeight::Auto,
        None => return Err(err(r#"page(): height must be a number or "auto""#)),
    };
    let root = root
        .try_cast::<ScriptNode>()
        .ok_or_else(|| err("page(): the third argument (root) must be a node"))?;
    Ok(ScriptDoc {
        page: RawPage {
            width,
            height,
            unit: Unit::default(),
            root: String::new(),
            dpi: None,
            background: None,
            font_family: None,
            script_metrics: None,
            font_synthesis: None,
            synthetic_bold_width: None,
            synthetic_slant: None,
            min_stroke_px: None,
            image_dpi: None,
        },
        root,
    })
}

/// Parse a colour string for the setter `name`.
fn parse_color(name: &str, hex: &str) -> Result<Color, Box<EvalAltResult>> {
    Color::parse_hex(hex).map_err(|_| {
        err(format!(
            "`{name}`: invalid color `{hex}` (expected hex like #rrggbb)"
        ))
    })
}

/// Gradient stops from `[[offset, "#…"], …]` or bare colours `["#…", …]`
/// (automatic offsets, see [`crate::paint::auto_offsets`]).
fn cast_stops(
    name: &str,
    stops: Array,
) -> Result<Vec<crate::geom::GradientStop>, Box<EvalAltResult>> {
    let bad = || {
        err(format!(
            r##"`{name}`: stops are [[offset, "#rrggbb"], …] or ["#rrggbb", …]"##
        ))
    };
    let mut raw = Vec::with_capacity(stops.len());
    for s in stops {
        if s.is_string() {
            let hex = s.into_string().map_err(|_| bad())?;
            raw.push((None, parse_color(name, &hex)?));
            continue;
        }
        let pair = s.try_cast::<Array>().ok_or_else(bad)?;
        match pair.as_slice() {
            [o, c] => {
                let o = dyn_to_f32(o).ok_or_else(bad)?;
                let hex = c.clone().into_string().map_err(|_| bad())?;
                raw.push((Some(o), parse_color(name, &hex)?));
            }
            _ => return Err(bad()),
        }
    }
    Ok(crate::paint::auto_offsets(&raw))
}

/// `[x, y]` -> a point, for the gradient builder `name`.
fn cast_xy(name: &str, v: Array) -> Result<[f32; 2], Box<EvalAltResult>> {
    let bad = || err(format!("`{name}` expects [x, y] number pairs"));
    match v.as_slice() {
        [x, y] => Ok([
            dyn_to_f32(x).ok_or_else(bad)?,
            dyn_to_f32(y).ok_or_else(bad)?,
        ]),
        _ => Err(bad()),
    }
}

/// Insets for `.pad` / `.margin`: a number (all sides), an array of 1–4
/// numbers in CSS order (`[v, h]`, `[t, r, b, l]`, …), or a map with any of
/// `top` / `right` / `bottom` / `left` and the shorthands `x` (left + right)
/// and `y` (top + bottom); a named side overrides a shorthand. Same rules as
/// the TOML `padding` / `margin` (see [`Edges`]).
fn cast_edges(name: &str, v: Dynamic) -> Result<Edges, Box<EvalAltResult>> {
    if let Some(x) = dyn_to_f32(&v) {
        return Ok(Edges::all(x));
    }
    let bad = || {
        err(format!(
            "`{name}` expects a number, [v, h] / [t, r, b, l], or #{{ top, right, bottom, left, x, y }}"
        ))
    };
    if v.is_array() {
        let a = v.cast::<Array>();
        let nums = a
            .iter()
            .map(|d| dyn_to_f32(d).ok_or_else(bad))
            .collect::<Result<Vec<_>, _>>()?;
        return Edges::from_css(&nums).ok_or_else(bad);
    }
    if v.is_map() {
        let mut sides = EdgeSides::default();
        for (k, val) in v.cast::<Map>() {
            let slot = sides.slot(k.as_str()).ok_or_else(|| {
                err(format!(
                    "`{name}`: unknown side `{k}` (top, right, bottom, left, x, y)"
                ))
            })?;
            *slot = Some(
                dyn_to_f32(&val).ok_or_else(|| err(format!("`{name}`: `{k}` must be a number")))?,
            );
        }
        return Ok(Edges::from_sides(sides));
    }
    Err(bad())
}

/// One `anchor` component: a fraction, or `"baseline"` / `"last_baseline"` /
/// `"cap_middle"`.
fn cast_anchor(v: Dynamic) -> Result<AnchorValue, Box<EvalAltResult>> {
    if let Some(f) = dyn_to_f32(&v) {
        return Ok(AnchorValue::Frac(f));
    }
    v.into_string()
        .ok()
        .and_then(|s| BaselineKind::parse(&s))
        .map(AnchorValue::Baseline)
        .ok_or_else(|| {
            err(r#"`anchor` expects numbers (ax, ay); "cap_middle" too, and on text "baseline" / "last_baseline""#)
        })
}

/// The anchor that puts the pen origin of a text's first line (the left end
/// of its baseline, in its own frame) at its offset, for a text turned by
/// `rotate` degrees. Anchors act on the footprint in the parent's frame, so
/// the baseline goes in the slot across it and the text's start edge in
/// the other.
fn baseline_origin_anchor(rotate: f32) -> [AnchorValue; 2] {
    let b = AnchorValue::Baseline(BaselineKind::First);
    let f = AnchorValue::Frac;
    match crate::geom::quarter_turns(rotate) {
        // Reading downwards: the start is at the top.
        Some(1) => [b, f(0.0)],
        // Upside down: the start is at the right.
        Some(2) => [f(1.0), b],
        // Reading upwards: the start is at the bottom.
        Some(3) => [b, f(1.0)],
        // Upright, or a paint-only angle (the unrotated box counts).
        _ => [f(0.0), b],
    }
}

/// `.fit_width(mode)` on a text node.
fn fit_width_setter(n: ScriptNode, mode: &str) -> NodeResult {
    if n.node.kind != NodeKind::Text {
        return Err(err("`fit_width` applies to a text node"));
    }
    let fit = crate::schema::FitWidth::parse(mode).ok_or_else(|| {
        err(format!(
            r#"`fit_width`: unknown mode `{mode}` ("tracking", "scale")"#
        ))
    })?;
    Ok(set_node(n, |r| r.fit_width = Some(fit)))
}

/// Validate a built gradient, reporting under the builder's name.
fn checked_paint(name: &str, p: Paint) -> Result<Paint, Box<EvalAltResult>> {
    p.validate().map_err(|e| err(format!("`{name}`: {e}")))?;
    Ok(p)
}

/// `.shadow(dx, dy, blur[, spread], color)`.
fn shadow_setter(
    n: ScriptNode,
    [dx, dy, blur]: [Dynamic; 3],
    spread: Option<Dynamic>,
    color: Option<&str>,
) -> NodeResult {
    let sig = "dx, dy, blur[, spread], color";
    let [dx, dy, blur] = nums("shadow", sig, [dx, dy, blur])?;
    let spread = match spread {
        Some(v) => nums("shadow", sig, [v])?[0],
        None => 0.0,
    };
    if blur < 0.0 {
        return Err(err("`shadow`: blur must be >= 0"));
    }
    let color = color.map(|c| parse_color("shadow", c)).transpose()?;
    Ok(set_node(n, |r| {
        r.shadow = Some(RawShadow {
            dx,
            dy,
            blur,
            spread,
            color,
        })
    }))
}

/// `.glow(blur[, color])`.
fn glow_setter(n: ScriptNode, blur: Dynamic, color: Option<&str>) -> NodeResult {
    let [blur] = nums("glow", "blur[, color]", [blur])?;
    if blur < 0.0 {
        return Err(err("`glow`: blur must be >= 0"));
    }
    let color = color.map(|c| parse_color("glow", c)).transpose()?;
    Ok(set_node(n, |r| {
        r.glow = Some(RawGlow {
            blur,
            spread: None,
            color,
        })
    }))
}

/// Register a numeric chainable setter that accepts integer or float.
fn reg_num(engine: &mut Engine, name: &'static str, set: fn(&mut RawNode, f32)) {
    engine.register_fn(
        name,
        move |n: ScriptNode, v: Dynamic| -> Result<ScriptNode, Box<EvalAltResult>> {
            let x = dyn_to_f32(&v).ok_or_else(|| err(format!("`{name}` expects a number")))?;
            Ok(set_node(n, |r| set(r, x)))
        },
    );
}

/// Register a color chainable setter that parses a hex string.
fn reg_color(engine: &mut Engine, name: &'static str, set: fn(&mut RawNode, Color)) {
    engine.register_fn(
        name,
        move |n: ScriptNode, hex: &str| -> Result<ScriptNode, Box<EvalAltResult>> {
            let c = Color::parse_hex(hex).map_err(|_| {
                err(format!(
                    "`{name}`: invalid color `{hex}` (expected hex like #rrggbb)"
                ))
            })?;
            Ok(set_node(n, |r| set(r, c)))
        },
    );
}

/// Register an enum chainable setter (string value -> typed enum).
fn reg_enum<T: Clone + Send + Sync + 'static>(
    engine: &mut Engine,
    name: &'static str,
    parse: fn(&str) -> Option<T>,
    set: fn(&mut RawNode, T),
) {
    engine.register_fn(
        name,
        move |n: ScriptNode, s: &str| -> Result<ScriptNode, Box<EvalAltResult>> {
            let v = parse(s).ok_or_else(|| err(format!("`{name}`: invalid value `{s}`")))?;
            Ok(set_node(n, move |r| set(r, v)))
        },
    );
}

// ---------------------------------------------------------------------------
// Lowering: ScriptDoc -> RawDocument
// ---------------------------------------------------------------------------

/// Flatten the node tree into a `RawDocument`, synthesising ids (`root`, then
/// `node1`, `node2`, … in pre-order; a node given `.id(s)` gets `s`) and
/// filling each node's `children`. A connector's ends become ids: a node
/// value names the last copy of it placed (pre-order), an id string the node
/// with that id. The offset call sites of each node are classified against
/// `src` (the user's script) into the returned [`SourceMap`].
fn lower(doc: ScriptDoc, src: &str) -> Result<(RawDocument, SourceMap), String> {
    // Which copy (pre-order position) of a node with an explicit id gets
    // it: the last one placed.
    let mut chosen: HashMap<String, (usize, u64)> = HashMap::new();
    let mut counter = 0usize;
    claim_ids(&doc.root, 0, &mut counter, &mut chosen)?;
    let mut nodes: HashMap<String, RawNode> = HashMap::new();
    let mut map = SourceMap::new();
    let mut counter = 0usize;
    let mut out = Lowered {
        nodes: &mut nodes,
        map: &mut map,
        src,
        chosen: &chosen,
        by_uid: HashMap::new(),
        links: Vec::new(),
    };
    let root_id = node_id(&doc.root, 0, &chosen);
    assign(doc.root, root_id.clone(), &mut counter, &mut out);
    let (by_uid, links) = (
        std::mem::take(&mut out.by_uid),
        std::mem::take(&mut out.links),
    );
    for (id, ends) in links {
        let mut resolved = Vec::with_capacity(2);
        for (end, which) in ends.iter().zip(["from", "to"]) {
            resolved.push(match end {
                LinkRef::Node(uid) => by_uid.get(uid).cloned().ok_or_else(|| {
                    format!("connect(): the `{which}` node is not on the page (put it in the tree under page(..))")
                })?,
                LinkRef::Id(s) if chosen.contains_key(s) => s.clone(),
                LinkRef::Id(s) => {
                    let mut known: Vec<&str> = chosen.keys().map(String::as_str).collect();
                    known.sort();
                    let known = if known.is_empty() {
                        "none; give a node one with .id(..)".to_string()
                    } else {
                        known.join(", ")
                    };
                    return Err(format!(
                        "connect(): no node has the id \"{s}\" (ids on the page: {known})"
                    ));
                }
            });
        }
        let n = nodes.get_mut(&id).expect("connector lowered");
        n.to = resolved.pop();
        n.from = resolved.pop();
    }
    let mut page = doc.page;
    page.root = root_id;
    Ok((RawDocument { page, nodes }, map))
}

/// Pre-order walk (same numbering as [`assign`]) recording, per explicit id,
/// the position of its last copy; two different nodes with one id are an
/// error.
fn claim_ids(
    sn: &ScriptNode,
    pos: usize,
    counter: &mut usize,
    chosen: &mut HashMap<String, (usize, u64)>,
) -> Result<(), String> {
    if let Some(id) = &sn.explicit_id {
        if let Some(&(_, uid)) = chosen.get(id) {
            if uid != sn.uid {
                return Err(format!(
                    "`id`: \"{id}\" is given to two different nodes (ids must be unique)"
                ));
            }
        }
        chosen.insert(id.clone(), (pos, sn.uid));
    }
    for child in &sn.children {
        *counter += 1;
        claim_ids(child, *counter, counter, chosen)?;
    }
    Ok(())
}

/// The id of the node at pre-order position `pos`.
fn node_id(sn: &ScriptNode, pos: usize, chosen: &HashMap<String, (usize, u64)>) -> String {
    match &sn.explicit_id {
        Some(id) if chosen.get(id).is_some_and(|&(p, _)| p == pos) => id.clone(),
        _ if pos == 0 => "root".to_string(),
        _ => format!("node{pos}"),
    }
}

struct Lowered<'a> {
    nodes: &'a mut HashMap<String, RawNode>,
    map: &'a mut SourceMap,
    src: &'a str,
    chosen: &'a HashMap<String, (usize, u64)>,
    /// Node uid -> id of its last copy placed so far.
    by_uid: HashMap<u64, String>,
    /// Connectors to fill in: (id, ends).
    links: Vec<(String, [LinkRef; 2])>,
}

fn assign(mut sn: ScriptNode, id: String, counter: &mut usize, out: &mut Lowered<'_>) {
    out.by_uid.insert(sn.uid, id.clone());
    if let Some(link) = sn.link.take() {
        out.links.push((id.clone(), *link));
    }
    let children = std::mem::take(&mut sn.children);
    let mut child_ids = Vec::with_capacity(children.len());
    for child in children {
        *counter += 1;
        let cid = node_id(&child, *counter, out.chosen);
        child_ids.push(cid.clone());
        assign(child, cid, counter, out);
    }
    sn.node.children = child_ids;
    if sn.baseline_origin {
        sn.node.anchor = Some(baseline_origin_anchor(sn.node.rotate.unwrap_or(0.0)));
    }
    let classify =
        |s: Option<sites::RawSite>| s.and_then(|s| s.classify(out.src, prelude::source()));
    let found = OffsetSites {
        x: classify(sn.sites.x),
        y: classify(sn.sites.y),
    };
    if found != OffsetSites::default() {
        out.map.insert(id.clone(), found);
    }
    out.nodes.insert(id, sn.node);
}

// ---------------------------------------------------------------------------
// Engine + entry point
// ---------------------------------------------------------------------------

fn build_engine(base_dir: Option<&Path>) -> Engine {
    let mut engine = Engine::new();

    // Sandbox: bound CPU/recursion/allocations and forbid file `import`/`eval`.
    engine.set_max_operations(5_000_000);
    engine.set_max_call_levels(64);
    engine.set_max_expr_depths(128, 64);
    engine.set_max_string_size(64 * 1024);
    engine.set_max_array_size(10_000);
    engine.set_max_map_size(2_000);
    engine.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
    engine.disable_symbol("eval");

    engine
        .register_type_with_name::<ScriptNode>("Node")
        .register_type_with_name::<ScriptDoc>("Doc")
        .register_type_with_name::<Paint>("Paint");

    // Constructors.
    engine.register_fn("text", |content: &str| {
        let mut sn = leaf(NodeKind::Text);
        sn.node.content = Some(content.to_string());
        sn
    });
    engine.register_fn("image", |src: &str| {
        let mut sn = leaf(NodeKind::Image);
        sn.node.src = Some(src.to_string());
        sn
    });
    engine.register_fn("rect", || leaf(NodeKind::Rect));
    engine.register_fn("col", |children: Array| {
        make_container(NodeKind::Column, children)
    });
    engine.register_fn("row", |children: Array| {
        make_container(NodeKind::Row, children)
    });
    engine.register_fn("stack", |children: Array| {
        make_container(NodeKind::Stack, children)
    });
    // Vector shapes: coordinates in page units, relative to the node's own box.
    engine.register_fn(
        "line",
        |x1: Dynamic, y1: Dynamic, x2: Dynamic, y2: Dynamic| {
            segment(NodeKind::Line, "line", [x1, y1, x2, y2])
        },
    );
    engine.register_fn(
        "arrow",
        |x1: Dynamic, y1: Dynamic, x2: Dynamic, y2: Dynamic| {
            segment(NodeKind::Arrow, "arrow", [x1, y1, x2, y2])
        },
    );
    engine.register_fn("ellipse", || leaf(NodeKind::Ellipse));
    engine.register_fn(
        "path",
        |points: Array| -> Result<ScriptNode, Box<EvalAltResult>> {
            let mut sn = leaf(NodeKind::Path);
            sn.node.points = cast_points(points)?;
            Ok(sn)
        },
    );

    // Numeric chainable setters (accept int or float).
    reg_num(&mut engine, "flex", |r, x| r.flex = Some(x));
    reg_num(&mut engine, "width", |r, x| r.width = Some(x));
    reg_num(&mut engine, "height", |r, x| r.height = Some(x));
    reg_num(&mut engine, "aspect", |r, x| r.aspect_ratio = Some(x));
    reg_num(&mut engine, "size", |r, x| r.font_size = Some(x));
    reg_num(&mut engine, "line_height", |r, x| r.line_height = Some(x));
    reg_num(&mut engine, "spacing", |r, x| r.spacing = x);
    // Column: consecutive children's first baselines exactly this far apart.
    reg_num(&mut engine, "baseline_pitch", |r, x| {
        r.baseline_pitch = Some(x)
    });
    reg_num(&mut engine, "stroke_width", |r, x| r.stroke_width = Some(x));
    reg_num(&mut engine, "radius", |r, x| r.corner_radius = Some(x));
    reg_num(&mut engine, "head_size", |r, x| r.head_size = Some(x));
    // Insets: a number, `[v, h]` / `[t, r, b, l]` (CSS order), or a map
    // `#{ top, right, bottom, left, x, y }`.
    engine.register_fn("margin", |n: ScriptNode, v: Dynamic| -> NodeResult {
        let e = cast_edges("margin", v)?;
        Ok(set_node(n, |r| r.margin = e))
    });
    engine.register_fn("pad", |n: ScriptNode, v: Dynamic| -> NodeResult {
        let e = cast_edges("pad", v)?;
        Ok(set_node(n, |r| r.padding = e))
    });
    // Absolute offset inside a `stack` parent (ignored under col/row). Each
    // records its call site, so the editor can write a drag back.
    engine.register_fn(
        "x",
        |ctx: NativeCallContext, mut n: ScriptNode, v: Dynamic| -> NodeResult {
            let x = dyn_to_f32(&v).ok_or_else(|| err("`x` expects a number"))?;
            n.sites.x = Some(sites::RawSite::here(&ctx, "x"));
            Ok(set_node(n, |r| r.x = Some(x)))
        },
    );
    engine.register_fn(
        "y",
        |ctx: NativeCallContext, mut n: ScriptNode, v: Dynamic| -> NodeResult {
            let y = dyn_to_f32(&v).ok_or_else(|| err("`y` expects a number"))?;
            n.sites.y = Some(sites::RawSite::here(&ctx, "y"));
            Ok(set_node(n, |r| r.y = Some(y)))
        },
    );
    // Clockwise degrees about the box centre; odd quarter turns swap the
    // layout footprint (`text("y").rotate(-90)` in a row is a vertical label).
    reg_num(&mut engine, "rotate", |r, d| r.rotate = Some(d));
    engine.register_fn(
        "at",
        |ctx: NativeCallContext, mut n: ScriptNode, x: Dynamic, y: Dynamic| -> NodeResult {
            let x = dyn_to_f32(&x).ok_or_else(|| err("`at` expects numbers (x, y)"))?;
            let y = dyn_to_f32(&y).ok_or_else(|| err("`at` expects numbers (x, y)"))?;
            let site = sites::RawSite::here(&ctx, "at");
            n.sites.x = Some(site);
            n.sites.y = Some(site);
            Ok(set_node(n, |r| {
                r.x = Some(x);
                r.y = Some(y);
            }))
        },
    );
    // Which point of the node sits at its offset, as fractions of its size
    // (or, on text, "baseline" / "last_baseline" across the baseline;
    // "cap_middle": the first line's optical middle, the box middle without text).
    engine.register_fn(
        "anchor",
        |mut n: ScriptNode, ax: Dynamic, ay: Dynamic| -> NodeResult {
            let a = [cast_anchor(ax)?, cast_anchor(ay)?];
            n.baseline_origin = false;
            Ok(set_node(n, |r| r.anchor = Some(a)))
        },
    );
    // Place a text (or a container, by the first baseline of a text inside)
    // by the left end of its first baseline (its pen origin), whatever its
    // rotation.
    engine.register_fn(
        "at_baseline",
        |ctx: NativeCallContext, mut n: ScriptNode, x: Dynamic, y: Dynamic| -> NodeResult {
            if !matches!(
                n.node.kind,
                NodeKind::Text | NodeKind::Column | NodeKind::Row | NodeKind::Stack
            ) {
                return Err(err(
                    "`at_baseline` applies to a text node or a col/row/stack",
                ));
            }
            let [x, y] = nums("at_baseline", "x, y", [x, y])?;
            let site = sites::RawSite::here(&ctx, "at_baseline");
            n.sites.x = Some(site);
            n.sites.y = Some(site);
            n.baseline_origin = true;
            Ok(set_node(n, |r| {
                r.x = Some(x);
                r.y = Some(y);
            }))
        },
    );
    // User coordinates over the content box: a stack's child offsets, or a
    // shape's points, are given in them.
    engine.register_fn(
        "viewbox",
        |n: ScriptNode, x0: Dynamic, y0: Dynamic, x1: Dynamic, y1: Dynamic| -> NodeResult {
            let v = nums("viewbox", "x0, y0, x1, y1", [x0, y0, x1, y1])?;
            if !matches!(
                n.node.kind,
                NodeKind::Stack | NodeKind::Path | NodeKind::Line | NodeKind::Arrow
            ) {
                return Err(err("`viewbox` applies to a stack/path/line/arrow"));
            }
            crate::geom::ViewBox::from_array(v).map_err(|e| err(format!("`viewbox`: {e}")))?;
            Ok(set_node(n, |r| r.viewbox = Some(v)))
        },
    );
    // Keep what the children draw inside the stack: overhanging children
    // shrink the content box (and its viewbox mapping) instead.
    engine.register_fn("contain", |n: ScriptNode| -> NodeResult {
        if n.node.kind != NodeKind::Stack {
            return Err(err("`contain` applies to a stack"));
        }
        Ok(set_node(n, |r| r.contain = true))
    });

    // Color chainable setters.
    reg_color(&mut engine, "color", |r, c| r.color = Some(c));
    reg_color(&mut engine, "fill", |r, c| r.fill = Some(c.into()));
    reg_color(&mut engine, "stroke", |r, c| r.stroke = Some(c));
    // `.fill(linear(…))` / `.fill(radial(…))`.
    engine.register_fn("fill", |n: ScriptNode, p: Paint| {
        set_node(n, |r| r.fill = Some(p))
    });

    // Gradient builders (box-relative geometry; see `crate::paint`).
    engine.register_fn(
        "linear",
        |angle: Dynamic, stops: Array| -> Result<Paint, Box<EvalAltResult>> {
            let a = dyn_to_f32(&angle)
                .ok_or_else(|| err("`linear` expects an angle in degrees, or two [x, y] points"))?;
            let stops = cast_stops("linear", stops)?;
            checked_paint(
                "linear",
                Paint::Linear {
                    dir: LinearDir::Angle(a),
                    stops,
                    absolute: false,
                },
            )
        },
    );
    engine.register_fn(
        "linear",
        |from: Array, to: Array, stops: Array| -> Result<Paint, Box<EvalAltResult>> {
            let dir = LinearDir::Points(cast_xy("linear", from)?, cast_xy("linear", to)?);
            let stops = cast_stops("linear", stops)?;
            checked_paint(
                "linear",
                Paint::Linear {
                    dir,
                    stops,
                    absolute: false,
                },
            )
        },
    );
    engine.register_fn(
        "radial",
        |stops: Array| -> Result<Paint, Box<EvalAltResult>> {
            let stops = cast_stops("radial", stops)?;
            checked_paint(
                "radial",
                Paint::Radial {
                    center: [0.5, 0.5],
                    radius: None,
                    focal: None,
                    stops,
                    absolute: false,
                },
            )
        },
    );
    engine.register_fn(
        "radial",
        |center: Array, radius: Dynamic, stops: Array| -> Result<Paint, Box<EvalAltResult>> {
            let center = cast_xy("radial", center)?;
            let r = dyn_to_f32(&radius).ok_or_else(|| err("`radial`: radius must be a number"))?;
            let stops = cast_stops("radial", stops)?;
            checked_paint(
                "radial",
                Paint::Radial {
                    center,
                    radius: Some(r),
                    focal: None,
                    stops,
                    absolute: false,
                },
            )
        },
    );
    // Where offset 0 of a radial gradient sits (default: its centre).
    engine.register_fn(
        "focal",
        |mut p: Paint, fx: Dynamic, fy: Dynamic| -> Result<Paint, Box<EvalAltResult>> {
            let [fx, fy] = nums("focal", "fx, fy", [fx, fy])?;
            match &mut p {
                Paint::Radial { focal, .. } => *focal = Some([fx, fy]),
                _ => return Err(err("`focal` applies to a radial gradient")),
            }
            checked_paint("focal", p)
        },
    );
    // Gradient points/radius in page units from the box's top-left corner.
    engine.register_fn(
        "absolute",
        |mut p: Paint| -> Result<Paint, Box<EvalAltResult>> {
            match &mut p {
                Paint::Linear { absolute, .. } | Paint::Radial { absolute, .. } => *absolute = true,
                Paint::Solid(_) => return Err(err("`absolute` applies to a gradient")),
            }
            checked_paint("absolute", p)
        },
    );
    // Connectors: a line (or band) between two nodes, drawn after layout.
    engine.register_fn("connect", |a: Dynamic, b: Dynamic| {
        connect_fn(a, b, Map::new())
    });
    engine.register_fn("connect", connect_fn);
    // The node's id in the document (connectors and TOML refer to it).
    engine.register_fn("id", id_setter);
    // The id a node will get, if set with `.id(s)` (else "").
    engine.register_fn("id", |n: ScriptNode| {
        n.explicit_id.clone().unwrap_or_default()
    });
    // Diagnostics: a human-readable name (an overflow warning uses it).
    engine.register_fn("name", |n: ScriptNode, s: &str| {
        set_node(n, |r| r.name = Some(s.to_string()))
    });
    // The node's axis for a parent with `.cross("axis")`: `.axis()` marks it
    // as the axis provider, `.axis(fx, fy)` puts the axis at those fractions.
    engine.register_fn("axis", |n: ScriptNode| {
        set_node(n, |r| r.axis = Some(crate::geom::AxisMark::Auto))
    });
    engine.register_fn(
        "axis",
        |n: ScriptNode, fx: Dynamic, fy: Dynamic| -> NodeResult {
            let [fx, fy] = nums("axis", "fx, fy", [fx, fy])?;
            Ok(set_node(n, |r| {
                r.axis = Some(crate::geom::AxisMark::At([fx, fy]))
            }))
        },
    );
    // Row: children whose height follows their width share one height and
    // fill the row's width.
    engine.register_fn("equal_height", |n: ScriptNode| -> NodeResult {
        if n.node.kind != NodeKind::Row {
            return Err(err("`equal_height` applies to a row"));
        }
        Ok(set_node(n, |r| r.equal_height = true))
    });
    // Column / row: "visible" (default), "clip" or "shrink" on overflow.
    engine.register_fn("overflow", |n: ScriptNode, mode: &str| -> NodeResult {
        if !matches!(n.node.kind, NodeKind::Column | NodeKind::Row) {
            return Err(err("`overflow` applies to a col/row"));
        }
        let v = match mode {
            "visible" => crate::geom::Overflow::Visible,
            "clip" => crate::geom::Overflow::Clip,
            "shrink" => crate::geom::Overflow::Shrink,
            "fit" => crate::geom::Overflow::Fit,
            _ => {
                return Err(err(format!(
                    "`overflow`: unknown mode `{mode}` (visible, clip, shrink, fit)"
                )))
            }
        };
        Ok(set_node(n, |r| r.overflow = Some(v)))
    });
    // Take no room across the parent column / row (overhang it, centred).
    engine.register_fn("overhang", |n: ScriptNode| {
        set_node(n, |r| r.overhang = crate::geom::Overhang::On)
    });
    // `.overhang(true | false | "auto")`: "auto" overhangs only into free
    // room beside its parent in a row, wrapping otherwise.
    engine.register_fn("overhang", |n: ScriptNode, v: Dynamic| -> NodeResult {
        use crate::geom::Overhang;
        let o = if let Ok(b) = v.as_bool() {
            if b {
                Overhang::On
            } else {
                Overhang::Off
            }
        } else if v.clone().into_string().is_ok_and(|s| s == "auto") {
            Overhang::Auto
        } else {
            return Err(err("`overhang` expects true, false or \"auto\""));
        };
        Ok(set_node(n, |r| r.overhang = o))
    });
    // Image: natural size = pixels / dpi inches (0: none, over the page's).
    engine.register_fn("dpi", |n: ScriptNode, v: Dynamic| -> NodeResult {
        if n.node.kind != NodeKind::Image {
            return Err(err("`dpi` applies to an image (or a page)"));
        }
        let x = dyn_to_dpi(&v).ok_or_else(|| {
            err("`dpi` expects pixels per inch (0 for no natural size) or \"file\"")
        })?;
        Ok(set_node(n, |r| r.dpi = Some(x)))
    });
    // At least this wide / tall (a flex share or wrap-content size grows).
    engine.register_fn("min_width", |n: ScriptNode, v: Dynamic| -> NodeResult {
        let x = dyn_to_f32(&v)
            .filter(|x| x.is_finite() && *x >= 0.0)
            .ok_or_else(|| err("`min_width` expects a length >= 0"))?;
        Ok(set_node(n, |r| r.min_width = Some(x)))
    });
    engine.register_fn("min_height", |n: ScriptNode, v: Dynamic| -> NodeResult {
        let x = dyn_to_f32(&v)
            .filter(|x| x.is_finite() && *x >= 0.0)
            .ok_or_else(|| err("`min_height` expects a length >= 0"))?;
        Ok(set_node(n, |r| r.min_height = Some(x)))
    });
    // Its own cross alignment in a column / row (CSS `align-self`).
    engine.register_fn("align_self", |n: ScriptNode, s: &str| -> NodeResult {
        let v = parse_self_align(s).ok_or_else(|| {
            err(format!(
                "`align_self`: unknown value `{s}` (start, center, end, stretch)"
            ))
        })?;
        Ok(set_node(n, |r| r.align_self = Some(v)))
    });
    // Containers: clip the children to the (rounded) box inside the border.
    engine.register_fn("clip", |n: ScriptNode| -> NodeResult {
        if !matches!(
            n.node.kind,
            NodeKind::Column | NodeKind::Row | NodeKind::Stack
        ) {
            return Err(err("`clip` applies to a col/row/stack"));
        }
        Ok(set_node(n, |r| r.clip = true))
    });

    // Enum chainable setters.
    // `align`: a text's line alignment (left, center, right, justify); on
    // any other node — and "start" / "end" / "stretch" on a text — the
    // node's own cross alignment in its column / row (`align_self`).
    engine.register_fn("align", |n: ScriptNode, s: &str| -> NodeResult {
        if n.node.kind == NodeKind::Text {
            if let Some(v) = parse_text_align(s) {
                return Ok(set_node(n, move |r| r.align = Some(v)));
            }
        }
        let v = match s {
            "left" => Some(CrossAxisAlign::Start),
            "right" => Some(CrossAxisAlign::End),
            _ => parse_self_align(s),
        }
        .ok_or_else(|| {
            err(format!(
                "`align`: invalid value `{s}` (text: left, center, right, justify;                  any node: start, center, end, stretch)"
            ))
        })?;
        Ok(set_node(n, move |r| r.align_self = Some(v)))
    });
    reg_enum(&mut engine, "main", parse_main_axis, |r, v| {
        r.main_axis_alignment = Some(v)
    });
    reg_enum(&mut engine, "cross", parse_cross_axis, |r, v| {
        r.cross_axis_alignment = Some(v)
    });
    reg_enum(&mut engine, "fit", parse_fit, |r, v| r.fit = Some(v));
    // Where a closed outline's stroke lies: "inside" / "center" / "outside".
    reg_enum(&mut engine, "stroke_align", StrokeAlign::parse, |r, v| {
        r.stroke_align = Some(v)
    });
    // Text wrapping: `.nowrap()` keeps each paragraph on one line.
    engine.register_fn("nowrap", |n: ScriptNode| -> NodeResult {
        if n.node.kind != NodeKind::Text {
            return Err(err("`nowrap` applies to a text node"));
        }
        Ok(set_node(n, |r| r.wrap = Some(false)))
    });
    engine.register_fn("wrap", |n: ScriptNode, on: bool| -> NodeResult {
        if n.node.kind != NodeKind::Text {
            return Err(err("`wrap` applies to a text node"));
        }
        Ok(set_node(n, |r| r.wrap = Some(on)))
    });
    // Fit each line to the box width: `.fit_width()` adjusts the letter
    // spacing, `.fit_width("scale")` the size.
    engine.register_fn("fit_width", |n: ScriptNode| -> NodeResult {
        fit_width_setter(n, "tracking")
    });
    engine.register_fn("fit_width", |n: ScriptNode, mode: &str| -> NodeResult {
        fit_width_setter(n, mode)
    });
    reg_enum(&mut engine, "cap", parse_cap, |r, v| r.cap = Some(v));
    // Outline corner join (shapes and text strokes).
    reg_enum(&mut engine, "join", parse_join, |r, v| r.join = Some(v));
    reg_enum(&mut engine, "head", parse_head, |r, v| r.head = Some(v));
    reg_enum(&mut engine, "head_style", parse_head_style, |r, v| {
        r.head_style = Some(v)
    });
    // Arrowhead base width: a length, or "3x" (a multiple of the stroke width).
    engine.register_fn("head_width", |n: ScriptNode, v: Dynamic| -> NodeResult {
        let w = match dyn_to_f32(&v) {
            Some(w) => HeadWidth::Abs(w),
            None => v
                .into_string()
                .ok()
                .and_then(|s| HeadWidth::parse_ratio(&s))
                .ok_or_else(|| err(r#"`head_width` expects a length or a multiple like "3x""#))?,
        };
        Ok(set_node(n, |r| r.head_width = Some(w)))
    });

    // Effects (any node; visual only). `shadow(dx, dy, blur[, [spread,] color])`.
    engine.register_fn(
        "shadow",
        |n: ScriptNode, dx: Dynamic, dy: Dynamic, blur: Dynamic| -> NodeResult {
            shadow_setter(n, [dx, dy, blur], None, None)
        },
    );
    engine.register_fn(
        "shadow",
        |n: ScriptNode, dx: Dynamic, dy: Dynamic, blur: Dynamic, color: &str| -> NodeResult {
            shadow_setter(n, [dx, dy, blur], None, Some(color))
        },
    );
    engine.register_fn(
        "shadow",
        |n: ScriptNode,
         dx: Dynamic,
         dy: Dynamic,
         blur: Dynamic,
         spread: Dynamic,
         color: &str|
         -> NodeResult { shadow_setter(n, [dx, dy, blur], Some(spread), Some(color)) },
    );
    engine.register_fn("glow", |n: ScriptNode, blur: Dynamic| -> NodeResult {
        glow_setter(n, blur, None)
    });
    engine.register_fn(
        "glow",
        |n: ScriptNode, blur: Dynamic, color: &str| -> NodeResult {
            glow_setter(n, blur, Some(color))
        },
    );
    engine.register_fn("opacity", |n: ScriptNode, v: Dynamic| -> NodeResult {
        let a = dyn_to_f32(&v)
            .filter(|a| (0.0..=1.0).contains(a))
            .ok_or_else(|| err("`opacity` expects a number from 0 to 1"))?;
        Ok(set_node(n, |r| r.opacity = Some(a)))
    });
    engine.register_fn("smooth", |n: ScriptNode, on: bool| -> NodeResult {
        if n.node.kind != NodeKind::Image {
            return Err(err("`smooth` applies to an image"));
        }
        Ok(set_node(n, |r| r.smooth = Some(on)))
    });

    // Shape stroke dash pattern (page units) and polygon closing.
    engine.register_fn(
        "dash",
        |n: ScriptNode, pattern: Array| -> Result<ScriptNode, Box<EvalAltResult>> {
            let dash = pattern
                .iter()
                .map(|d| dyn_to_f32(d).ok_or_else(|| err("`dash` expects an array of numbers")))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(set_node(n, |r| r.dash = dash))
        },
    );
    engine.register_fn("closed", |n: ScriptNode| set_node(n, |r| r.closed = true));

    // Curve builders: each appends a segment from the outline's current end
    // (the last point so far) to (x, y). Page units, node-box frame.
    engine.register_fn(
        "to",
        |n: ScriptNode, x: Dynamic, y: Dynamic| -> NodeResult {
            let [x, y] = nums("to", "x, y", [x, y])?;
            push_segment(n, "to", RawSegment::line([x, y]))
        },
    );
    engine.register_fn(
        "quad",
        |n: ScriptNode, cx: Dynamic, cy: Dynamic, x: Dynamic, y: Dynamic| -> NodeResult {
            let [cx, cy, x, y] = nums("quad", "cx, cy, x, y", [cx, cy, x, y])?;
            let mut seg = RawSegment::line([x, y]);
            seg.quad = Some([cx, cy]);
            push_segment(n, "quad", seg)
        },
    );
    engine.register_fn(
        "cubic",
        |n: ScriptNode,
         c1x: Dynamic,
         c1y: Dynamic,
         c2x: Dynamic,
         c2y: Dynamic,
         x: Dynamic,
         y: Dynamic|
         -> NodeResult {
            let [c1x, c1y, c2x, c2y, x, y] = nums(
                "cubic",
                "c1x, c1y, c2x, c2y, x, y",
                [c1x, c1y, c2x, c2y, x, y],
            )?;
            let mut seg = RawSegment::line([x, y]);
            seg.cubic = Some([[c1x, c1y], [c2x, c2y]]);
            push_segment(n, "cubic", seg)
        },
    );
    engine.register_fn(
        "arc",
        |n: ScriptNode, r: Dynamic, x: Dynamic, y: Dynamic| -> NodeResult {
            push_segment(n, "arc", arc_segment(r, x, y, Map::new())?)
        },
    );
    engine.register_fn(
        "arc",
        |n: ScriptNode, r: Dynamic, x: Dynamic, y: Dynamic, opts: Map| -> NodeResult {
            push_segment(n, "arc", arc_segment(r, x, y, opts)?)
        },
    );

    // font weight (u16, rounded + range-checked).
    engine.register_fn(
        "weight",
        |n: ScriptNode, v: Dynamic| -> Result<ScriptNode, Box<EvalAltResult>> {
            let x = dyn_to_f32(&v).ok_or_else(|| err("`weight` expects a number"))?;
            if !(0.0..=u16::MAX as f32).contains(&x) {
                return Err(err(format!("`weight` out of range: {x}")));
            }
            Ok(set_node(n, |r| r.font_weight = Some(x.round() as u16)))
        },
    );

    // String setter + no-arg shorthands.
    engine.register_fn("font", |n: ScriptNode, s: &str| {
        set_node(n, |r| r.font_family = Some(s.to_string()))
    });
    engine.register_fn("bold", |n: ScriptNode| {
        set_node(n, |r| r.font_weight = Some(700))
    });
    engine.register_fn("center", |n: ScriptNode| {
        set_node(n, |r| r.align = Some(TextAlign::Center))
    });
    engine.register_fn("stretch", |n: ScriptNode| {
        set_node(n, |r| {
            r.cross_axis_alignment = Some(CrossAxisAlign::Stretch)
        })
    });

    // Document: page(width, height, root) + chainable page settings.
    engine.register_fn("page", page_fn);
    engine.register_fn(
        "unit",
        |d: ScriptDoc, s: &str| -> Result<ScriptDoc, Box<EvalAltResult>> {
            let u = parse_unit(s).ok_or_else(|| err(format!("`unit`: invalid value `{s}`")))?;
            Ok(set_doc(d, |pg| pg.unit = u))
        },
    );
    engine.register_fn(
        "background",
        |d: ScriptDoc, hex: &str| -> Result<ScriptDoc, Box<EvalAltResult>> {
            let c = Color::parse_hex(hex)
                .map_err(|_| err(format!("`background`: invalid color `{hex}`")))?;
            Ok(set_doc(d, |pg| pg.background = Some(c.into())))
        },
    );
    engine.register_fn("background", |d: ScriptDoc, p: Paint| {
        set_doc(d, |pg| pg.background = Some(p))
    });
    engine.register_fn(
        "dpi",
        |d: ScriptDoc, v: Dynamic| -> Result<ScriptDoc, Box<EvalAltResult>> {
            let x = dyn_to_f32(&v).ok_or_else(|| err("`dpi` expects a number"))?;
            Ok(set_doc(d, |pg| pg.dpi = Some(x)))
        },
    );
    engine.register_fn("font", |d: ScriptDoc, s: &str| {
        set_doc(d, |pg| pg.font_family = Some(s.to_string()))
    });
    // Default pixel density of images without their own `dpi`.
    engine.register_fn(
        "image_dpi",
        |d: ScriptDoc, v: Dynamic| -> Result<ScriptDoc, Box<EvalAltResult>> {
            let x = dyn_to_dpi(&v).ok_or_else(|| {
                err("`image_dpi` expects pixels per inch (0 for none) or \"file\"")
            })?;
            Ok(set_doc(d, |pg| pg.image_dpi = Some(x)))
        },
    );
    // PNG: draw shape strokes at least this many device pixels wide.
    engine.register_fn(
        "min_stroke_px",
        |d: ScriptDoc, v: Dynamic| -> Result<ScriptDoc, Box<EvalAltResult>> {
            let x = dyn_to_f32(&v)
                .filter(|x| x.is_finite() && *x >= 0.0)
                .ok_or_else(|| err("`min_stroke_px` expects a non-negative number (pixels)"))?;
            Ok(set_doc(d, |pg| pg.min_stroke_px = Some(x)))
        },
    );

    // Rich text: `span(..)`, `text([..])`, span setters, `markup(..)`.
    rich::register(&mut engine);

    // Data loading (the only file access scripts get).
    csv::register(&mut engine, base_dir.map(Path::to_path_buf));

    engine
}

pub mod csv;
pub mod markup;
mod prelude;
mod rich;
pub mod sites;

pub use prelude::location as prelude_location;
pub use sites::{OffsetSite, OffsetSites, SiteOrigin, SourceMap};

/// Evaluate a Rhai script into a [`RawDocument`]. The script's last expression
/// must be a `page(...)` call. The framework prelude is available implicitly.
///
/// Error positions always refer to `src`: an error raised inside the prelude is
/// reported at the user's call into the framework, named after that function.
pub fn eval_script(src: &str) -> Result<RawDocument, ScriptError> {
    eval_script_in(src, None)
}

/// [`eval_script`] for a script living in `base_dir`: relative paths the
/// script reads (`csv("data.csv")`) resolve against it — pass the script
/// file's directory, as the CLI and the editor do. `None` resolves them
/// against the current directory.
pub fn eval_script_in(src: &str, base_dir: Option<&Path>) -> Result<RawDocument, ScriptError> {
    eval_script_mapped(src, base_dir).map(|(doc, _)| doc)
}

/// [`eval_script_in`] that also returns where each node's stack offsets were
/// set (the `.at` / `.x` / `.y` call sites, see [`sites`]) — editor metadata
/// for writing a canvas drag back into the script.
pub fn eval_script_mapped(
    src: &str,
    base_dir: Option<&Path>,
) -> Result<(RawDocument, SourceMap), ScriptError> {
    let mut engine = build_engine(base_dir);
    prelude::register_style(&mut engine);
    prelude::register_meta(&mut engine);
    let (ast, origins) =
        prelude::compile_with_prelude(&engine, src).map_err(ScriptError::from_eval)?;
    let value = engine
        .eval_ast::<Dynamic>(&ast)
        .map_err(|e| origins.script_error(e))?;
    let doc = value
        .try_cast::<ScriptDoc>()
        .ok_or_else(ScriptError::not_a_document)?;
    lower(doc, src).map_err(|message| ScriptError {
        message,
        line: None,
        column: None,
    })
}
