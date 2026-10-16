//! Source sites of stack offsets: which `.at(x, y)` / `.x(n)` / `.y(n)` call
//! last set each lowered node's `x` / `y`.
//!
//! This is editor metadata only: it never reaches the [`RawDocument`] (and so
//! is never serialized). The editor uses it to write a canvas drag back into
//! the script by rewriting the call's number literals.
//!
//! A native function only learns its call *position* (line, column); a
//! position carries no file, and the prelude is merged into the same AST as
//! the user's script (see `prelude.rs`). So each site is classified by
//! checking which source actually holds a call of that name at that position:
//! the user's script, the prelude, or — when both or neither do — neither.
//!
//! [`RawDocument`]: crate::schema::RawDocument

use std::collections::HashMap;

use rhai::{NativeCallContext, Position};

/// Which source a call site lies in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SiteOrigin {
    /// The user's script: the site's line/column refer to it.
    Script,
    /// The framework prelude (e.g. a component placed the node itself).
    Prelude,
    /// Could not be told apart (both sources, or neither, hold such a call
    /// at that position): never safe to rewrite.
    Unknown,
}

/// One setter call: `call` is `"at"`, `"x"` or `"y"`; `line`/`column` are
/// 1-based (columns count characters) and point at the call's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OffsetSite {
    pub call: &'static str,
    pub line: usize,
    pub column: usize,
    pub origin: SiteOrigin,
}

/// The calls that last set a node's `x` and `y` (`None`: never set).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OffsetSites {
    pub x: Option<OffsetSite>,
    pub y: Option<OffsetSite>,
}

/// Lowered node id -> where its offsets were set.
pub type SourceMap = HashMap<String, OffsetSites>;

/// A site as recorded during evaluation (classified at lowering).
#[derive(Clone, Copy, Debug)]
pub(super) struct RawSite {
    call: &'static str,
    pos: Position,
    /// Script-function nesting depth: 0 is the user's top-level code.
    level: usize,
}

/// Recorded sites of a node under construction.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RawSites {
    pub x: Option<RawSite>,
    pub y: Option<RawSite>,
}

impl RawSite {
    pub(super) fn here(ctx: &NativeCallContext, call: &'static str) -> RawSite {
        RawSite {
            call,
            pos: ctx.call_position(),
            level: ctx.call_level(),
        }
    }

    pub(super) fn classify(self, user_src: &str, prelude_src: &str) -> Option<OffsetSite> {
        let (line, column) = (self.pos.line()?, self.pos.position()?);
        let in_user = call_name_at(user_src, line, column) == Some(self.call);
        let in_prelude = call_name_at(prelude_src, line, column) == Some(self.call);
        let origin = match (in_user, in_prelude) {
            // Top-level code is always the user's (the prelude only defines
            // functions), so a match there is unambiguous.
            (true, _) if self.level == 0 => SiteOrigin::Script,
            (true, false) => SiteOrigin::Script,
            (false, true) => SiteOrigin::Prelude,
            _ => SiteOrigin::Unknown,
        };
        Some(OffsetSite {
            call: self.call,
            line,
            column,
            origin,
        })
    }
}

/// The identifier a call at 1-based (`line`, `column`) names, if the text
/// there is `[.] name (` (whitespace allowed around the dot and before `(`).
pub fn call_name_at(src: &str, line: usize, column: usize) -> Option<&str> {
    let text = src.lines().nth(line.checked_sub(1)?)?;
    let start = text.char_indices().nth(column.checked_sub(1)?)?.0;
    let rest = text[start..].trim_start();
    let rest = rest.strip_prefix('.').unwrap_or(rest).trim_start();
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let (name, after) = rest.split_at(end);
    (!name.is_empty() && after.trim_start().starts_with('(')).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_call_names() {
        let src = "let a = 1;\ntext(\"x\").at(1, 2)\n  .x (3)";
        assert_eq!(call_name_at(src, 2, 11), Some("at"));
        assert_eq!(call_name_at(src, 2, 10), Some("at")); // at the dot
        assert_eq!(call_name_at(src, 3, 3), Some("x"));
        assert_eq!(call_name_at(src, 1, 1), None); // `let a` is no call
        assert_eq!(call_name_at(src, 9, 1), None);
    }

    fn sites(src: &str) -> SourceMap {
        crate::script::eval_script_mapped(src, None).unwrap().1
    }

    #[test]
    fn records_user_call_sites() {
        // node1 = the text (pre-order ids); columns count characters (μ is one).
        let src = "page(10, 10, stack([\n  text(\"μm\").at(1, 2),\n  rect().width(1)\n    .x(3).y(4),\n]))";
        let m = sites(src);
        let t = m["node1"];
        let at = t.x.unwrap();
        assert_eq!((at.call, at.line, at.origin), ("at", 2, SiteOrigin::Script));
        assert_eq!(call_name_at(src, at.line, at.column), Some("at"));
        assert_eq!(t.y, t.x);
        let r = m["node2"];
        let (x, y) = (r.x.unwrap(), r.y.unwrap());
        assert_eq!((x.call, x.line, y.call, y.line), ("x", 4, "y", 4));
        assert_eq!(call_name_at(src, y.line, y.column), Some("y"));
        assert!(!m.contains_key("root"));
    }

    #[test]
    fn last_setter_wins_and_user_fns_are_script() {
        let src = "fn lab(s) { text(s).at(1, 1) }\npage(10, 10, stack([lab(\"a\").x(5)]))";
        let m = sites(src);
        let n = m["node1"];
        assert_eq!(n.x.unwrap().call, "x");
        assert_eq!(n.y.unwrap().call, "at");
        assert_eq!(n.y.unwrap().line, 1);
        assert_eq!(n.y.unwrap().origin, SiteOrigin::Script);
    }

    #[test]
    fn prelude_placed_nodes_are_not_script() {
        // `circle(cx, cy, r)` places itself with `.at` inside the prelude, and
        // `panels` places its labels there too.
        let src = "page(10, 10, stack([circle(5, 5, 1), grid(2, panels([rect()]))]))";
        let m = sites(src);
        assert_eq!(m["node1"].x.unwrap().origin, SiteOrigin::Prelude);
        assert!(m.len() > 1);
        assert!(m
            .values()
            .flat_map(|s| [s.x, s.y])
            .flatten()
            .all(|s| s.origin == SiteOrigin::Prelude));
    }
}
