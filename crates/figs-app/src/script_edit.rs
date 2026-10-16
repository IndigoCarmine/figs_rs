//! Writing a canvas edit back into the Rhai script: rewrite the number
//! literals of the `.at(x, y)` / `.x(n)` / `.y(n)` call that set a node's stack
//! offset (its call site comes from [`figs_core::script::SourceMap`]).
//!
//! Only plain number literals written in the user's script are rewritten. An
//! argument that is an expression or a variable, a call made inside the
//! framework prelude, or a site that can't be matched to the script text is
//! refused with a reason — the script stays the source of truth and is never
//! guessed at.

use std::ops::Range;

use figs_core::script::{OffsetSite, SiteOrigin};

/// Why an offset can't be written back into the script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// No `.at` / `.x` / `.y` call sets this offset.
    NoSite,
    /// The call is inside the framework prelude (a component placed the node).
    Prelude,
    /// The argument isn't a plain number literal (or the call couldn't be
    /// matched to the script text): the value is computed at `line`.
    Computed { line: usize },
}

impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Blocked::NoSite => f.write_str("no `.at(x, y)` / `.x` / `.y` sets it in the script"),
            Blocked::Prelude => {
                f.write_str("position is set by a framework component (prelude), not your script")
            }
            Blocked::Computed { line } => write!(f, "position is computed in script (line {line})"),
        }
    }
}

/// A text replacement in the script (`range` in bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteralEdit {
    pub range: Range<usize>,
    pub text: String,
}

/// Byte offset of 1-based (`line`, `column`) — columns count characters.
fn byte_offset(src: &str, line: usize, column: usize) -> Option<usize> {
    let mut start = 0;
    for (i, l) in src.split_inclusive('\n').enumerate() {
        if i + 1 == line {
            let col = l.char_indices().nth(column.checked_sub(1)?)?.0;
            return Some(start + col);
        }
        start += l.len();
    }
    None
}

/// The trimmed byte ranges of the top-level arguments of the call whose name
/// starts at (or just after a `.` at) `line`/`column`, named `name`.
pub fn call_args(src: &str, line: usize, column: usize, name: &str) -> Option<Vec<Range<usize>>> {
    let mut i = byte_offset(src, line, column)?;
    let b = src.as_bytes();
    let skip_ws = |mut i: usize| {
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        i
    };
    i = skip_ws(i);
    if b.get(i) == Some(&b'.') {
        i = skip_ws(i + 1);
    }
    if !src[i..].starts_with(name) {
        return None;
    }
    i = skip_ws(i + name.len());
    if b.get(i) != Some(&b'(') {
        return None;
    }
    i += 1;

    // Scan to the matching `)`, splitting at top-level commas; strings, chars
    // and comments are skipped so their commas/brackets don't count.
    let mut args = Vec::new();
    let mut start = i;
    let mut depth = 0usize;
    while i < b.len() {
        match b[i] {
            b'"' | b'`' | b'\'' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' if depth == 0 => {
                push_arg(src, start..i, &mut args);
                return Some(args);
            }
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                push_arg(src, start..i, &mut args);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Record a trimmed, non-empty argument (a trailing comma adds nothing).
fn push_arg(src: &str, r: Range<usize>, out: &mut Vec<Range<usize>>) {
    let s = &src[r.clone()];
    let lead = s.len() - s.trim_start().len();
    let t = s.trim();
    if !t.is_empty() {
        out.push(r.start + lead..r.start + lead + t.len());
    }
}

/// A plain Rhai number literal: `-?digits[.digits][e[+-]digits]` (`_`
/// separators allowed).
pub fn parse_number_literal(s: &str) -> Option<f64> {
    let body = s.strip_prefix('-').unwrap_or(s);
    let mut chars = body.chars().peekable();
    let digits = |c: &mut std::iter::Peekable<std::str::Chars>| {
        let mut n = 0;
        while c.peek().is_some_and(|ch| ch.is_ascii_digit() || *ch == '_') {
            c.next();
            n += 1;
        }
        n
    };
    if !body.starts_with(|c: char| c.is_ascii_digit()) || digits(&mut chars) == 0 {
        return None;
    }
    if chars.peek() == Some(&'.') {
        chars.next();
        if !chars.peek().is_some_and(char::is_ascii_digit) || digits(&mut chars) == 0 {
            return None;
        }
    }
    if chars.peek().is_some_and(|c| *c == 'e' || *c == 'E') {
        chars.next();
        if chars.peek().is_some_and(|c| *c == '+' || *c == '-') {
            chars.next();
        }
        if digits(&mut chars) == 0 {
            return None;
        }
    }
    if chars.next().is_some() {
        return None;
    }
    s.replace('_', "").parse().ok()
}

/// The literal argument of `site` that holds the `axis` (0 = x, 1 = y)
/// offset: its byte range and value. `current` (the node's lowered value) must
/// match the literal, so a stale or unrelated site is never rewritten.
pub fn offset_literal(
    src: &str,
    site: &OffsetSite,
    axis: usize,
    current: f32,
) -> Result<(Range<usize>, f64), Blocked> {
    match site.origin {
        SiteOrigin::Script => {}
        SiteOrigin::Prelude => return Err(Blocked::Prelude),
        SiteOrigin::Unknown => return Err(Blocked::Computed { line: site.line }),
    }
    let computed = Blocked::Computed { line: site.line };
    let args = call_args(src, site.line, site.column, site.call).ok_or(computed.clone())?;
    // Method form `n.at(x, y)`, or function form `at(n, x, y)`.
    let (arity, index) = match site.call {
        "at" | "at_baseline" => (2, axis),
        _ => (1, 0),
    };
    let index = match args.len() {
        n if n == arity => index,
        n if n == arity + 1 => index + 1,
        _ => return Err(computed),
    };
    let range = args[index].clone();
    let value = parse_number_literal(&src[range.clone()]).ok_or(computed.clone())?;
    let tol = 1e-4 * (current.abs() as f64).max(1.0);
    if (value - current as f64).abs() > tol {
        return Err(computed);
    }
    Ok((range, value))
}

/// Format an offset for the script: at most 4 decimals, trailing zeros
/// dropped (`2.5`, `3`, `-0.25`) — minimal notation.
pub fn format_num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        &s
    };
    match s {
        "-0" => "0".to_string(),
        s => s.to_string(),
    }
}

/// Apply non-overlapping edits (any order; duplicates of one range collapse).
pub fn apply_edits(src: &str, mut edits: Vec<LiteralEdit>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
    edits.dedup_by(|a, b| a.range == b.range);
    let mut out = src.to_string();
    for e in edits {
        out.replace_range(e.range, &e.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(call: &'static str, line: usize, column: usize, origin: SiteOrigin) -> OffsetSite {
        OffsetSite {
            call,
            line,
            column,
            origin,
        }
    }

    #[test]
    fn splits_call_arguments() {
        let src = "x\n  text(\"a, (b\").at( 1.5 , f(2, 3) ) // c";
        let col = src.lines().nth(1).unwrap().find("at").unwrap() + 1;
        let args = call_args(src, 2, col, "at").unwrap();
        let texts: Vec<&str> = args.iter().map(|r| &src[r.clone()]).collect();
        assert_eq!(texts, ["1.5", "f(2, 3)"]);
        // From the dot, with a comment and a trailing comma inside.
        let src = "n\n  .x(/* , */ -2,)";
        let args = call_args(src, 2, 3, "x").unwrap();
        assert_eq!(&src[args[0].clone()], "/* , */ -2");
        assert!(call_args(src, 2, 3, "y").is_none());
    }

    #[test]
    fn recognises_number_literals() {
        assert_eq!(parse_number_literal("2"), Some(2.0));
        assert_eq!(parse_number_literal("-0.25"), Some(-0.25));
        assert_eq!(parse_number_literal("1_000.5"), Some(1000.5));
        assert_eq!(parse_number_literal("1e-2"), Some(0.01));
        for bad in ["x", "1 + 2", "-x", "1.", ".5", "2.0.1", "(1)", "- 1", ""] {
            assert_eq!(parse_number_literal(bad), None, "{bad}");
        }
    }

    #[test]
    fn formats_minimally() {
        assert_eq!(format_num(2.5), "2.5");
        assert_eq!(format_num(3.0), "3");
        assert_eq!(format_num(0.22f32 as f64), "0.22");
        assert_eq!(format_num(-0.00001), "0");
        assert_eq!(format_num(12.34567), "12.3457");
    }

    #[test]
    fn rewrites_at_literals() {
        let src = "page(9, 9, stack([\n  text(\"t\").at(0.2, 1),\n]))";
        let s = site("at", 2, 13, SiteOrigin::Script);
        let (rx, vx) = offset_literal(src, &s, 0, 0.2).unwrap();
        let (ry, vy) = offset_literal(src, &s, 1, 1.0).unwrap();
        assert_eq!((vx, vy), (0.2, 1.0));
        let out = apply_edits(
            src,
            vec![
                LiteralEdit {
                    range: rx,
                    text: format_num(1.25),
                },
                LiteralEdit {
                    range: ry,
                    text: format_num(-3.0),
                },
            ],
        );
        assert_eq!(out, "page(9, 9, stack([\n  text(\"t\").at(1.25, -3),\n]))");
    }

    #[test]
    fn function_form_and_single_axis_setters() {
        let src = "at(text(\"t\"), 1, 2).y(4)";
        let at = site("at", 1, 1, SiteOrigin::Script);
        let (r, _) = offset_literal(src, &at, 1, 2.0).unwrap();
        assert_eq!(&src[r], "2");
        let y = site("y", 1, 21, SiteOrigin::Script);
        let (r, v) = offset_literal(src, &y, 1, 4.0).unwrap();
        assert_eq!((&src[r], v), ("4", 4.0));
    }

    #[test]
    fn refuses_non_literals_prelude_and_mismatches() {
        let src = "let k = 2;\ntext(\"t\").at(k, 1 + 1)";
        let s = site("at", 2, 11, SiteOrigin::Script);
        assert_eq!(
            offset_literal(src, &s, 0, 2.0),
            Err(Blocked::Computed { line: 2 })
        );
        assert_eq!(
            offset_literal(src, &s, 1, 2.0),
            Err(Blocked::Computed { line: 2 })
        );
        let p = site("at", 2, 11, SiteOrigin::Prelude);
        assert_eq!(offset_literal(src, &p, 0, 2.0), Err(Blocked::Prelude));
        let u = site("at", 2, 11, SiteOrigin::Unknown);
        assert!(offset_literal(src, &u, 0, 2.0).is_err());
        // The literal must hold the node's current value.
        let src = "text(\"t\").at(3, 1)";
        let s = site("at", 1, 11, SiteOrigin::Script);
        assert!(offset_literal(src, &s, 0, 3.0).is_ok());
        assert!(offset_literal(src, &s, 0, 5.0).is_err());
        // A site that doesn't point at the call.
        let off = site("at", 1, 2, SiteOrigin::Script);
        assert!(offset_literal(src, &off, 0, 3.0).is_err());
    }

    #[test]
    fn end_to_end_with_recorded_sites() {
        let src = "fn lab(s) { text(s).size(9).at(0.2, 0.2) }\n\
                   fn k() { 3 } page(10, 5, stack([\n    rect().width(2).height(1).x(1.5).y(k()),\n    lab(\"a\"),\n    circle(5, 2, 1),\n]))";
        let (doc, map) = figs_core::eval_script_mapped(src, None).unwrap();
        // node1: rect — x is a literal, y computed by a call.
        let s = map["node1"];
        let n = &doc.nodes["node1"];
        assert!(offset_literal(src, &s.x.unwrap(), 0, n.x.unwrap()).is_ok());
        assert_eq!(
            offset_literal(src, &s.y.unwrap(), 1, n.y.unwrap()),
            Err(Blocked::Computed { line: 3 })
        );
        // node2: text placed inside the user's own fn — literals, line 1.
        let s = map["node2"];
        let (r, _) = offset_literal(src, &s.x.unwrap(), 0, 0.2).unwrap();
        let out = apply_edits(
            src,
            vec![LiteralEdit {
                range: r,
                text: "0.5".into(),
            }],
        );
        assert!(out.starts_with("fn lab(s) { text(s).size(9).at(0.5, 0.2) }"));
        let (doc2, _) = figs_core::eval_script_mapped(&out, None).unwrap();
        assert_eq!(doc2.nodes["node2"].x, Some(0.5));
        // node3: `circle(cx, cy, r)` places itself in the prelude.
        let s = map["node3"];
        let x = doc.nodes["node3"].x.unwrap();
        assert_eq!(
            offset_literal(src, &s.x.unwrap(), 0, x),
            Err(Blocked::Prelude)
        );
    }
}
