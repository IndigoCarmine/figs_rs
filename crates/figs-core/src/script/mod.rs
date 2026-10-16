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
    ArcSweep, ArrowHead, Color, CrossAxisAlign, Edges, HeadStyle, HeadWidth, ImageFit, LineCap,
    LineJoin, LinearDir, MainAxisAlign, Paint, TextAlign,
};
use crate::schema::{
    ArcRadius, NodeKind, PageHeight, RawDocument, RawGlow, RawNode, RawPage, RawSegment, RawShadow,
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
        _ => return None,
    })
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
        src: None,
        fit: None,
        smooth: None,
        fill: None,
        stroke: None,
        stroke_width: None,
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
    }
}

fn leaf(kind: NodeKind) -> ScriptNode {
    ScriptNode {
        node: blank_node(kind),
        children: Vec::new(),
        sites: Default::default(),
        meta: Map::new(),
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

fn cast_children(children: Array) -> Result<Vec<ScriptNode>, Box<EvalAltResult>> {
    let mut out = Vec::with_capacity(children.len());
    for child in children {
        let node = child
            .try_cast::<ScriptNode>()
            .ok_or_else(|| {
                err("container children must be nodes (col/row/stack/text/rect/image/line/arrow/ellipse/path)")
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
/// `node1`, `node2`, … in pre-order) and filling each node's `children`. The
/// offset call sites of each node are classified against `src` (the user's
/// script) into the returned [`SourceMap`].
fn lower(doc: ScriptDoc, src: &str) -> (RawDocument, SourceMap) {
    let mut nodes: HashMap<String, RawNode> = HashMap::new();
    let mut map = SourceMap::new();
    let mut counter = 0usize;
    let mut out = Lowered {
        nodes: &mut nodes,
        map: &mut map,
        src,
    };
    assign(doc.root, "root".to_string(), &mut counter, &mut out);
    let mut page = doc.page;
    page.root = "root".to_string();
    (RawDocument { page, nodes }, map)
}

struct Lowered<'a> {
    nodes: &'a mut HashMap<String, RawNode>,
    map: &'a mut SourceMap,
    src: &'a str,
}

fn assign(mut sn: ScriptNode, id: String, counter: &mut usize, out: &mut Lowered<'_>) {
    let children = std::mem::take(&mut sn.children);
    let mut child_ids = Vec::with_capacity(children.len());
    for child in children {
        *counter += 1;
        let cid = format!("node{}", *counter);
        child_ids.push(cid.clone());
        assign(child, cid, counter, out);
    }
    sn.node.children = child_ids;
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
    reg_num(&mut engine, "stroke_width", |r, x| r.stroke_width = Some(x));
    reg_num(&mut engine, "radius", |r, x| r.corner_radius = Some(x));
    reg_num(&mut engine, "head_size", |r, x| r.head_size = Some(x));
    reg_num(&mut engine, "margin", |r, x| r.margin = Edges::all(x));
    reg_num(&mut engine, "pad", |r, x| r.padding = Edges::all(x));
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
    // Which point of the node sits at its offset, as fractions of its size.
    engine.register_fn(
        "anchor",
        |n: ScriptNode, ax: Dynamic, ay: Dynamic| -> NodeResult {
            let [ax, ay] = nums("anchor", "ax, ay", [ax, ay])?;
            Ok(set_node(n, |r| r.anchor = Some([ax, ay])))
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
    reg_enum(&mut engine, "align", parse_text_align, |r, v| {
        r.align = Some(v)
    });
    reg_enum(&mut engine, "main", parse_main_axis, |r, v| {
        r.main_axis_alignment = Some(v)
    });
    reg_enum(&mut engine, "cross", parse_cross_axis, |r, v| {
        r.cross_axis_alignment = Some(v)
    });
    reg_enum(&mut engine, "fit", parse_fit, |r, v| r.fit = Some(v));
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
    Ok(lower(doc, src))
}
