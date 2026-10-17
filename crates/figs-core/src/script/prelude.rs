//! The framework prelude: opinionated components (`grid`, `panels`, `title`,
//! `page("a4", root)`, …) written in Rhai on top of the core builders.
//!
//! The core builders registered in [`super`] stay explicit (1px-level freedom);
//! the prelude is the concise, paternalistic layer above them. Its source —
//! the files of `prelude/`, concatenated in file-name order ([`PARTS`]) — is
//! compiled once and *merged in front of* every user script,
//! so its functions are callable without an `import`, and a user `fn` with the
//! same name and arity replaces the prelude's — which is how the default hooks
//! (`base_size()`, `gap()`, `label_style()`) are customised.
//!
//! ## Errors point at the user's code
//!
//! A framework is only pleasant if its errors do. Rhai reports an error raised
//! inside a script function as nested `ErrorInFunctionCall(fn, _, inner, pos)`
//! frames, where `pos` is the call site *in the caller's code* and `inner` comes
//! from the callee's body. `Position` carries no source, and every frame shares
//! the one evaluation source (the merged AST), so the frames themselves can't
//! say which file a line belongs to. Instead [`FnOrigins`] classifies each
//! callee by name — defined by the user script (including overriding hooks) or
//! only by the prelude — and [`FnOrigins::script_error`] walks the chain to the
//! innermost position that lies in the user's script. An error raised in the
//! prelude is then reported at the user's call into the framework, prefixed
//! with that framework function's name (`grid: ncols must be …`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use rhai::{Dynamic, Engine, EvalAltResult, Map, Position, AST};

use super::ScriptError;

/// The framework source files, embedded at build time, in load order (their
/// file-name order), as `(file name, source)`. Rhai hoists script functions, so the order only decides
/// which line of the concatenated [`source`] a function lands on.
pub const PARTS: &[(&str, &str)] = &[
    ("00_base.rhai", include_str!("prelude/00_base.rhai")),
    ("10_text.rhai", include_str!("prelude/10_text.rhai")),
    ("15_layout.rhai", include_str!("prelude/15_layout.rhai")),
    ("20_panels.rhai", include_str!("prelude/20_panels.rhai")),
    (
        "30_annotations.rhai",
        include_str!("prelude/30_annotations.rhai"),
    ),
    ("35_shapes.rhai", include_str!("prelude/35_shapes.rhai")),
    ("40_schemes.rhai", include_str!("prelude/40_schemes.rhai")),
    (
        "45_figure_kit.rhai",
        include_str!("prelude/45_figure_kit.rhai"),
    ),
    ("50_links.rhai", include_str!("prelude/50_links.rhai")),
    (
        "60_plot_hooks.rhai",
        include_str!("prelude/60_plot_hooks.rhai"),
    ),
    (
        "61_plot_axes.rhai",
        include_str!("prelude/61_plot_axes.rhai"),
    ),
    (
        "62_plot_series.rhai",
        include_str!("prelude/62_plot_series.rhai"),
    ),
    (
        "63_plot_annotations.rhai",
        include_str!("prelude/63_plot_annotations.rhai"),
    ),
    (
        "64_plot_insets.rhai",
        include_str!("prelude/64_plot_insets.rhai"),
    ),
    (
        "65_plot_legend.rhai",
        include_str!("prelude/65_plot_legend.rhai"),
    ),
    (
        "66_plot_build.rhai",
        include_str!("prelude/66_plot_build.rhai"),
    ),
    (
        "70_poster_hooks.rhai",
        include_str!("prelude/70_poster_hooks.rhai"),
    ),
    (
        "71_poster_grid.rhai",
        include_str!("prelude/71_poster_grid.rhai"),
    ),
    (
        "72_poster_sections.rhai",
        include_str!("prelude/72_poster_sections.rhai"),
    ),
    (
        "73_poster_content.rhai",
        include_str!("prelude/73_poster_content.rhai"),
    ),
    (
        "74_poster_header.rhai",
        include_str!("prelude/74_poster_header.rhai"),
    ),
    (
        "75_poster_page.rhai",
        include_str!("prelude/75_poster_page.rhai"),
    ),
];

/// The concatenated prelude and the first line (1-based) of each part in it.
struct Source {
    text: String,
    starts: Vec<usize>,
}

static SOURCE: OnceLock<Source> = OnceLock::new();

fn combined() -> &'static Source {
    SOURCE.get_or_init(|| {
        let mut text = String::new();
        let mut starts = Vec::with_capacity(PARTS.len());
        let mut line = 1;
        for (_, part) in PARTS {
            starts.push(line);
            text.push_str(part);
            if !part.ends_with('\n') {
                text.push('\n');
            }
            line = text.lines().count() + 1;
        }
        Source { text, starts }
    })
}

/// Script-function name -> the arities defined under that name.
type FnTable = HashMap<String, Vec<usize>>;

struct Prelude {
    ast: AST,
    fns: FnTable,
}

static PRELUDE: OnceLock<Prelude> = OnceLock::new();

/// The prelude's source text: all [`PARTS`] concatenated (call sites inside
/// it refer to its lines; see [`location`] for the file a line belongs to).
pub(crate) fn source() -> &'static str {
    &combined().text
}

/// The prelude file and the line in it (both 1-based) of `line` of the
/// concatenated [`source`]; `None` past its end.
pub fn location(line: usize) -> Option<(&'static str, usize)> {
    let src = combined();
    if line == 0 || line > src.text.lines().count() {
        return None;
    }
    let i = src.starts.iter().rposition(|&s| s <= line)?;
    Some((PARTS[i].0, line - src.starts[i] + 1))
}

fn fn_table(ast: &AST) -> FnTable {
    let mut t = FnTable::new();
    for f in ast.iter_functions() {
        t.entry(f.name.to_string())
            .or_default()
            .push(f.params.len());
    }
    t
}

/// Which code a script function's body lives in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Origin {
    User,
    Prelude,
}

/// Where the script functions of one evaluation were defined, used to map
/// runtime errors back to the user's script (see the module docs).
pub(super) struct FnOrigins {
    prelude: &'static FnTable,
    user: FnTable,
}

/// Compile `src` and merge it after the (cached) prelude. User functions
/// override prelude functions of the same name and arity; the user script's
/// statements (and so its return value) are unchanged.
pub(super) fn compile_with_prelude(
    engine: &Engine,
    src: &str,
) -> Result<(AST, FnOrigins), Box<EvalAltResult>> {
    let prelude = PRELUDE.get_or_init(|| {
        let ast = engine
            .compile(source())
            .expect("the bundled prelude must compile");
        let fns = fn_table(&ast);
        Prelude { ast, fns }
    });
    let user = engine.compile(src)?;
    let origins = FnOrigins {
        prelude: &prelude.fns,
        user: fn_table(&user),
    };
    Ok((prelude.ast.merge(&user), origins))
}

/// Register the style registry behind `style(..)` presets on `engine`.
///
/// Hooks are functions, and Rhai functions can't see script globals, so a
/// preset can't simply assign variables the hooks read. Instead each
/// evaluation gets one shared key -> value map, captured by three native
/// functions that the prelude builds `style(..)` and every default hook on:
///
/// - `style_set(map)` merges `map` into the registry (later keys win);
/// - `style_reset()` empties it (back to the hooks' built-in defaults);
/// - `style_get(key, default)` reads `key`, or returns `default` when unset.
///
/// A hook reads its own key — `fn base_size() { style_get("base_size", 8) }` —
/// so a user `fn base_size()` replaces the whole hook and still wins over any
/// preset. The engine is built per [`super::eval_script`] call, so the registry
/// starts empty for every evaluation and never leaks between scripts.
pub(super) fn register_style(engine: &mut Engine) {
    // A poisoned lock only means an earlier call panicked mid-update; the map
    // itself is still a valid map, so keep using it.
    fn lock(s: &Mutex<Map>) -> MutexGuard<'_, Map> {
        s.lock().unwrap_or_else(PoisonError::into_inner)
    }

    let store: Arc<Mutex<Map>> = Arc::default();
    let s = Arc::clone(&store);
    engine.register_fn("style_set", move |m: Map| {
        lock(&s).extend(m);
    });
    let s = Arc::clone(&store);
    engine.register_fn("style_reset", move || lock(&s).clear());
    let s = Arc::clone(&store);
    engine.register_fn("style_get", move |key: &str, default: Dynamic| {
        lock(&s).get(key).cloned().unwrap_or(default)
    });

    // Scoped styles (a plot's `style:` option): `style_push(map)` saves the
    // whole registry and merges `map` over it; `style_pop()` restores the
    // saved registry exactly, so nothing set in between leaks out.
    let saved: Arc<Mutex<Vec<Map>>> = Arc::default();
    let (s, v) = (Arc::clone(&store), Arc::clone(&saved));
    engine.register_fn("style_push", move |m: Map| {
        let mut cur = lock(&s);
        v.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(cur.clone());
        cur.extend(m);
    });
    let (s, v) = (store, saved);
    engine.register_fn(
        "style_pop",
        move || -> Result<(), Box<rhai::EvalAltResult>> {
            let prev = v.lock().unwrap_or_else(PoisonError::into_inner).pop();
            match prev {
                Some(m) => {
                    *lock(&s) = m;
                    Ok(())
                }
                None => Err("style_pop() without a matching style_push()".into()),
            }
        },
    );
}

/// Register script-only node notes and the container insert they enable:
///
/// - `n.meta(key, value)` returns `n` with the note set (any value);
/// - `n.meta(key)` reads it back, `()` when unset;
/// - `n.prepend(child)` inserts `child` as the first child of container `n`;
/// - `n.kind()` is the node's kind (its TOML `type`, e.g. "connector").
///
/// Notes are never lowered into the document; they let one framework component
/// leave a hint for another that receives the finished node. A plot notes the
/// panel-label room it wants (`plot_label_room()`), and `labeled` prepends that
/// band inside the plot — under the plot's own aspect and size — only when it
/// actually labels it.
pub(super) fn register_meta(engine: &mut Engine) {
    engine.register_fn("meta", |mut n: super::ScriptNode, key: &str, v: Dynamic| {
        n.meta.insert(key.into(), v);
        n
    });
    engine.register_fn("meta", |n: super::ScriptNode, key: &str| {
        n.meta.get(key).cloned().unwrap_or(Dynamic::UNIT)
    });
    // The node's kind as its TOML `type` ("column", "image", "connector",
    // …): lets a helper tell connectors (which take no room) from items.
    engine.register_fn("kind", |n: super::ScriptNode| -> String {
        use crate::schema::NodeKind as K;
        match n.node.kind {
            K::Column => "column",
            K::Row => "row",
            K::Stack => "stack",
            K::Image => "image",
            K::Text => "text",
            K::Rect => "rect",
            K::Line => "line",
            K::Arrow => "arrow",
            K::Ellipse => "ellipse",
            K::Path => "path",
            K::Connector => "connector",
        }
        .into()
    });
    engine.register_fn(
        "prepend",
        |mut n: super::ScriptNode,
         child: super::ScriptNode|
         -> Result<super::ScriptNode, Box<rhai::EvalAltResult>> {
            use crate::schema::NodeKind;
            if !matches!(
                n.node.kind,
                NodeKind::Column | NodeKind::Row | NodeKind::Stack
            ) {
                return Err("`prepend`: only containers (col/row/stack) have children".into());
            }
            n.children.insert(0, child);
            Ok(n)
        },
    );
}

/// One `ErrorInFunctionCall` frame: `name` was called at `pos`, a position in
/// the caller's code, whose origin is `caller`.
struct Frame<'a> {
    name: &'a str,
    pos: Position,
    caller: Origin,
}

impl FnOrigins {
    /// Where the body of the script function `name`, called from `caller`
    /// code, lives. A user function shadows a prelude function of the same
    /// name and arity; frames don't carry the arity, so a name the user only
    /// partly overloads is treated as the prelude's (the report then falls
    /// back to the outer — still user — call site). Names neither side defines
    /// (synthetic frames such as `eval`) stay in the caller's code.
    fn body_origin(&self, name: &str, caller: Origin) -> Origin {
        match (self.user.get(name), self.prelude.get(name)) {
            (Some(_), None) => Origin::User,
            (None, Some(_)) => Origin::Prelude,
            (Some(u), Some(p)) if p.iter().all(|a| u.contains(a)) => Origin::User,
            (Some(_), Some(_)) => Origin::Prelude,
            (None, None) => caller,
        }
    }

    /// Convert a runtime error into a [`ScriptError`] positioned in the user's
    /// script: the innermost position that lies in user code. Errors raised in
    /// the prelude are reported at the user's call into the framework and named
    /// after the framework function called there.
    pub(super) fn script_error(&self, e: Box<EvalAltResult>) -> ScriptError {
        // Unwrap the nested call frames (outermost first) down to the leaf error.
        let mut frames = Vec::new();
        let mut origin = Origin::User; // top-level statements are the user's
        let mut leaf: &EvalAltResult = &e;
        while let EvalAltResult::ErrorInFunctionCall(name, _src, inner, pos) = leaf {
            frames.push(Frame {
                name,
                pos: *pos,
                caller: origin,
            });
            origin = self.body_origin(name, origin);
            leaf = inner;
        }
        let leaf_origin = origin;

        let in_prelude =
            leaf_origin == Origin::Prelude || frames.iter().any(|f| f.caller == Origin::Prelude);

        if !in_prelude {
            // Plain user code (core builders, user fns): keep rhai's message,
            // but point at the innermost position, i.e. the failing line rather
            // than the outermost call of a user function.
            let pos = if leaf.position().is_none() {
                frames.last().map_or_else(|| e.position(), |f| f.pos)
            } else {
                leaf.position()
            };
            let mut err = ScriptError::from_eval(e);
            err.line = pos.line();
            err.column = pos.position();
            return err;
        }

        // Pick the reporting site and the frames outside it (for the trace).
        let (site, head, outer) = if leaf_origin == Origin::User && !leaf.position().is_none() {
            // Raised directly in user code (e.g. inside an overridden hook
            // that the framework called).
            (leaf.position(), None, &frames[..])
        } else {
            // The innermost call made from user code: the user's entry point
            // into the framework.
            let k = frames
                .iter()
                .rposition(|f| f.caller == Origin::User)
                .expect("the top level is user code");
            let callee = self.callee_origin(&frames, k, leaf_origin);
            let head = (callee == Origin::Prelude).then_some(frames[k].name);
            (frames[k].pos, head, &frames[..k])
        };

        let mut text = leaf_text(leaf, leaf_origin);
        if let Some(name) = head {
            text = name_framework_error(name, &text);
        }

        let mut message = match (site.line(), site.position()) {
            (Some(l), Some(c)) => format!("line {l}, col {c}: {text}"),
            (Some(l), None) => format!("line {l}: {text}"),
            _ => text,
        };
        for f in outer.iter().rev() {
            let what = if f.name.starts_with("anon$") {
                "in closure call".to_string()
            } else {
                format!("in call to function '{}'", f.name)
            };
            let at = match (f.caller, f.pos.line(), f.pos.position()) {
                (Origin::Prelude, ..) => " (called by the framework)".to_string(),
                (Origin::User, Some(l), Some(c)) => format!(" (line {l}, col {c})"),
                (Origin::User, Some(l), None) => format!(" (line {l})"),
                (Origin::User, None, _) => String::new(),
            };
            message.push('\n');
            message.push_str(&what);
            message.push_str(&at);
        }

        ScriptError {
            message,
            line: site.line(),
            column: site.position(),
        }
    }

    /// Origin of the code frame `i` called into (its body).
    fn callee_origin(&self, frames: &[Frame<'_>], i: usize, leaf_origin: Origin) -> Origin {
        frames.get(i + 1).map_or(leaf_origin, |next| next.caller)
    }
}

/// Prefix a framework error with the function the user called, `name: …`.
/// A thrown message's own `fn(): ` prefix is normalised to `fn: `, and dropped
/// when it repeats `name` (so `throw "grid(): …"` reads `grid: …`, and an error
/// from a helper reads `panels: panel_label: …`).
fn name_framework_error(name: &str, text: &str) -> String {
    let rest = match text.split_once("(): ") {
        Some((f, rest)) if !f.is_empty() && f.chars().all(|c| c.is_alphanumeric() || c == '_') => {
            if f == name {
                rest.to_string()
            } else {
                format!("{f}: {rest}")
            }
        }
        _ => text
            .strip_prefix(name)
            .and_then(|r| r.strip_prefix(": "))
            .unwrap_or(text)
            .to_string(),
    };
    format!("{name}: {rest}")
}

/// The leaf error's text without rhai's trailing ` (line …, position …)`.
/// A string thrown by the framework is shown as-is (no `Runtime error:`).
fn leaf_text(leaf: &EvalAltResult, origin: Origin) -> String {
    if let EvalAltResult::ErrorRuntime(d, _) = leaf {
        if origin == Origin::Prelude && d.is_string() {
            return d.to_string();
        }
    }
    let s = leaf.to_string();
    let pos = leaf.position();
    if pos.is_none() {
        return s;
    }
    let suffix = format!(" ({pos})");
    s.strip_suffix(&suffix).map_or(s.clone(), str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_list_every_prelude_file_in_order() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/script/prelude");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .expect("prelude dir")
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|f| f.ends_with(".rhai"))
            .collect();
        files.sort();
        let listed: Vec<&str> = PARTS.iter().map(|(f, _)| *f).collect();
        assert_eq!(
            listed, files,
            "add new prelude files to PARTS (in file-name order)"
        );
    }

    #[test]
    fn locations_map_back_to_their_files() {
        let src = source();
        let lines: Vec<&str> = src.lines().collect();
        for (name, text) in PARTS {
            for (i, l) in text.lines().enumerate().step_by(37) {
                // Find this part's lines in the combined text via `location`.
                let hit = (1..=lines.len())
                    .find(|&n| location(n) == Some((name, i + 1)))
                    .expect("line is mapped");
                assert_eq!(lines[hit - 1], l, "{name}:{}", i + 1);
            }
        }
        assert_eq!(location(0), None);
        assert_eq!(location(lines.len() + 1), None);
    }
}
