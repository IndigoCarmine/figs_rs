//! Rhai scripting front-end (`.figs` / `.rhai`).
//!
//! A script *evaluates to* the existing [`RawDocument`] IR, so everything
//! downstream — [`crate::schema::resolve`], [`crate::layout`], [`crate::render`],
//! and the editor's TOML round-trip — is reused unchanged. Rhai natively provides
//! variables (`let`), arithmetic, and reusable components (`fn`).
//!
//! ## Authoring
//!
//! Build nodes with `text` / `rect` / `image` / `col` / `row`, set properties by
//! **chaining methods** (Rhai has no keyword arguments, so chaining is how you get
//! concise, named-ish properties — no map literals), and finish with a single
//! `page(width, height, root)` call:
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
//! [`eval_script`] then lowers the tree into a flat, id-keyed [`RawDocument`].

use std::collections::HashMap;

use rhai::{Array, Dynamic, Engine, EvalAltResult, Position};

use crate::geom::{Color, CrossAxisAlign, Edges, ImageFit, MainAxisAlign, TextAlign};
use crate::schema::{NodeKind, RawDocument, RawNode, RawPage};
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
        children: Vec::new(),
        spacing: 0.0,
        main_axis_alignment: None,
        cross_axis_alignment: None,
        content: None,
        font_size: None,
        font_family: None,
        font_weight: None,
        color: None,
        align: None,
        line_height: None,
        src: None,
        fit: None,
        fill: None,
        stroke: None,
        stroke_width: None,
        corner_radius: None,
    }
}

fn leaf(kind: NodeKind) -> ScriptNode {
    ScriptNode {
        node: blank_node(kind),
        children: Vec::new(),
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
            .ok_or_else(|| err("container children must be nodes (col/row/text/rect/image)"))?;
        out.push(node);
    }
    Ok(out)
}

fn make_container(kind: NodeKind, children: Array) -> Result<ScriptNode, Box<EvalAltResult>> {
    Ok(ScriptNode {
        node: blank_node(kind),
        children: cast_children(children)?,
    })
}

fn page_fn(width: Dynamic, height: Dynamic, root: Dynamic) -> Result<ScriptDoc, Box<EvalAltResult>> {
    let width = dyn_to_f32(&width).ok_or_else(|| err("page(): width must be a number"))?;
    let height = dyn_to_f32(&height).ok_or_else(|| err("page(): height must be a number"))?;
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
            let c = Color::parse_hex(hex)
                .map_err(|_| err(format!("`{name}`: invalid color `{hex}` (expected hex like #rrggbb)")))?;
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
/// `node1`, `node2`, … in pre-order) and filling each node's `children`.
fn lower(doc: ScriptDoc) -> RawDocument {
    let mut nodes: HashMap<String, RawNode> = HashMap::new();
    let mut counter = 0usize;
    assign(doc.root, "root".to_string(), &mut counter, &mut nodes);
    let mut page = doc.page;
    page.root = "root".to_string();
    RawDocument { page, nodes }
}

fn assign(mut sn: ScriptNode, id: String, counter: &mut usize, out: &mut HashMap<String, RawNode>) {
    let children = std::mem::take(&mut sn.children);
    let mut child_ids = Vec::with_capacity(children.len());
    for child in children {
        *counter += 1;
        let cid = format!("node{}", *counter);
        child_ids.push(cid.clone());
        assign(child, cid, counter, out);
    }
    sn.node.children = child_ids;
    out.insert(id, sn.node);
}

// ---------------------------------------------------------------------------
// Engine + entry point
// ---------------------------------------------------------------------------

fn build_engine() -> Engine {
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
        .register_type_with_name::<ScriptDoc>("Doc");

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
    engine.register_fn("col", |children: Array| make_container(NodeKind::Column, children));
    engine.register_fn("row", |children: Array| make_container(NodeKind::Row, children));

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
    reg_num(&mut engine, "margin", |r, x| r.margin = Edges::all(x));
    reg_num(&mut engine, "pad", |r, x| r.padding = Edges::all(x));

    // Color chainable setters.
    reg_color(&mut engine, "color", |r, c| r.color = Some(c));
    reg_color(&mut engine, "fill", |r, c| r.fill = Some(c));
    reg_color(&mut engine, "stroke", |r, c| r.stroke = Some(c));

    // Enum chainable setters.
    reg_enum(&mut engine, "align", parse_text_align, |r, v| r.align = Some(v));
    reg_enum(&mut engine, "main", parse_main_axis, |r, v| r.main_axis_alignment = Some(v));
    reg_enum(&mut engine, "cross", parse_cross_axis, |r, v| r.cross_axis_alignment = Some(v));
    reg_enum(&mut engine, "fit", parse_fit, |r, v| r.fit = Some(v));

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
    engine.register_fn("bold", |n: ScriptNode| set_node(n, |r| r.font_weight = Some(700)));
    engine.register_fn("center", |n: ScriptNode| {
        set_node(n, |r| r.align = Some(TextAlign::Center))
    });
    engine.register_fn("stretch", |n: ScriptNode| {
        set_node(n, |r| r.cross_axis_alignment = Some(CrossAxisAlign::Stretch))
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
            Ok(set_doc(d, |pg| pg.background = Some(c)))
        },
    );
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

    engine
}

/// Evaluate a Rhai script into a [`RawDocument`]. The script's last expression
/// must be a `page(width, height, root)` call.
pub fn eval_script(src: &str) -> Result<RawDocument, ScriptError> {
    let engine = build_engine();
    let value = engine.eval::<Dynamic>(src).map_err(ScriptError::from_eval)?;
    let doc = value
        .try_cast::<ScriptDoc>()
        .ok_or_else(ScriptError::not_a_document)?;
    Ok(lower(doc))
}
