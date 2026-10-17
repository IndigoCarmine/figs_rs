//! What a script can call: the core builders the engine registers (from their
//! signatures) and the functions of the framework prelude (from its source,
//! with the `//` comment block above each `fn` as its documentation).

use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    /// A builder or method registered in Rust (`text`, `.size`).
    Core,
    /// A function of the framework prelude (`grid`, `title`).
    Prelude,
}

/// One callable name and all of its overloads.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub origin: Origin,
    /// One line per overload: `grid(ncols, items)`, `size(Node, any) -> Node`.
    pub signatures: Vec<String>,
    /// Parameter names (prelude) or types (core) of each overload, in the
    /// order of `signatures`.
    pub params: Vec<Vec<String>>,
    /// Markdown documentation, if the prelude comments the function.
    pub doc: Option<String>,
    /// Called as a method (`x.name()`): a core function taking a value of a
    /// figs type first, or a prelude function that uses `this`.
    pub method: bool,
    /// Called as a plain function (`name()`).
    pub function: bool,
    /// A prelude hook the user may override with their own `fn` (it reads the
    /// style registry: `fn base_size() { style_get("base_size", 8) }`).
    pub hook: bool,
}

impl Entry {
    /// The documentation shown on hover and in completion details.
    pub fn markdown(&self) -> String {
        let mut s = String::from("```rhai\n");
        for sig in &self.signatures {
            s.push_str(sig);
            s.push('\n');
        }
        s.push_str("```");
        if let Some(doc) = &self.doc {
            s.push_str("\n\n");
            s.push_str(doc);
        }
        s
    }

    /// Does any overload take arguments (counting the receiver of a method)?
    pub fn takes_args(&self, as_method: bool) -> bool {
        let skip = usize::from(as_method && self.origin == Origin::Core);
        self.params.iter().any(|p| p.len() > skip)
    }
}

pub struct Catalog {
    pub entries: BTreeMap<String, Entry>,
}

/// The figs types a core method can be called on.
const RECEIVERS: &[&str] = &["Node", "Doc", "Paint", "Span"];

impl Catalog {
    pub fn load() -> Catalog {
        let mut entries = BTreeMap::new();
        for sig in figs_core::script::native_signatures() {
            if let Some((name, params, ret)) = parse_native(&sig) {
                let e = entries.entry(name.clone()).or_insert_with(|| Entry {
                    name: name.clone(),
                    origin: Origin::Core,
                    signatures: Vec::new(),
                    params: Vec::new(),
                    doc: None,
                    method: false,
                    function: false,
                    hook: false,
                });
                let receiver = params
                    .first()
                    .is_some_and(|p| RECEIVERS.contains(&p.as_str()));
                e.method |= receiver;
                e.function |= !receiver;
                let line = match ret {
                    Some(r) => format!("{name}({}) -> {r}", params.join(", ")),
                    None => format!("{name}({})", params.join(", ")),
                };
                if !e.signatures.contains(&line) {
                    e.signatures.push(line);
                    e.params.push(params);
                }
            }
        }
        // Rhai lists overloads in no fixed order; show them sorted.
        for e in entries.values_mut() {
            let mut both: Vec<_> = e.signatures.drain(..).zip(e.params.drain(..)).collect();
            both.sort();
            (e.signatures, e.params) = both.into_iter().unzip();
        }
        for (file, src) in figs_core::script::PRELUDE_FILES {
            for f in prelude_fns(src) {
                let e = entries.entry(f.name.clone()).or_insert_with(|| Entry {
                    name: f.name.clone(),
                    origin: Origin::Prelude,
                    signatures: Vec::new(),
                    params: Vec::new(),
                    doc: None,
                    method: false,
                    function: false,
                    hook: false,
                });
                e.origin = Origin::Prelude;
                e.signatures
                    .push(format!("{}({})", f.name, f.params.join(", ")));
                e.params.push(f.params);
                e.method |= f.uses_this;
                e.function |= !f.uses_this;
                e.hook |= f.reads_style || file.contains("hooks");
                if e.doc.is_none() {
                    e.doc = f.doc;
                }
            }
        }
        Catalog { entries }
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }
}

/// `name(_: Node, _: types::dynamic::Dynamic) -> core::result::Result<…>`
/// as `(name, ["Node", "any"], Some("Node"))`. Names rhai uses internally
/// (`get$x`, operators) are skipped.
fn parse_native(sig: &str) -> Option<(String, Vec<String>, Option<String>)> {
    let open = sig.find('(')?;
    let name = &sig[..open];
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let close = matching_paren(sig, open)?;
    let params = split_top(&sig[open + 1..close])
        .into_iter()
        .map(|p| short_type(p.split_once(':').map_or(p, |(_, t)| t).trim()))
        .collect();
    let ret = sig[close + 1..]
        .trim()
        .strip_prefix("->")
        .map(|r| short_type(r.trim()))
        .filter(|r| r != "()");
    Some((name.to_string(), params, ret))
}

fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s[open..].char_indices() {
        match c {
            '(' | '<' => depth += 1,
            ')' | '>' => {
                depth -= 1;
                if depth == 0 && c == ')' {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `s` split at the commas outside `<…>` / `(…)`.
fn split_top(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if !s[start..].trim().is_empty() {
        out.push(s[start..].trim());
    }
    out
}

/// A Rust type path as a script author knows it: `Result<T, E>` is `T`,
/// `ScriptNode` is `Node`, `Dynamic` is `any`, `BTreeMap<…>` is `map`.
fn short_type(t: &str) -> String {
    let t = t.trim().trim_start_matches('&');
    if let Some(inner) = t
        .strip_prefix("core::result::Result<")
        .or_else(|| t.strip_prefix("Result<"))
    {
        return short_type(
            split_top(inner.trim_end_matches('>'))
                .first()
                .copied()
                .unwrap_or(""),
        );
    }
    let base = t.split('<').next().unwrap_or(t);
    let last = base.rsplit("::").next().unwrap_or(base);
    match last {
        "ScriptNode" => "Node",
        "ScriptDoc" => "Doc",
        "RawSpan" => "Span",
        "Dynamic" => "any",
        "BTreeMap" | "Map" => "map",
        "Vec" | "Array" => "array",
        "str" | "String" | "ImmutableString" | "SmartString" => "string",
        "FnPtr" => "Fn",
        other => other,
    }
    .to_string()
}

/// A function definition found in prelude source.
pub struct PreludeFn {
    pub name: String,
    pub params: Vec<String>,
    pub doc: Option<String>,
    pub uses_this: bool,
    pub reads_style: bool,
}

/// The public top-level functions of a prelude file. A function's doc is the
/// block of `//` lines right above it, without ruler lines (`// ----`).
pub fn prelude_fns(src: &str) -> Vec<PreludeFn> {
    let lines: Vec<&str> = src.lines().collect();
    let starts: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].starts_with("fn ") || lines[i].starts_with("private fn "))
        .collect();
    let mut out = Vec::new();
    for (k, &i) in starts.iter().enumerate() {
        let line = lines[i];
        let Some(rest) = line.strip_prefix("fn ") else {
            continue; // private
        };
        let Some(open) = rest.find('(') else { continue };
        let Some(close) = rest[open..].find(')').map(|c| open + c) else {
            continue;
        };
        let name = rest[..open].trim().to_string();
        let params = rest[open + 1..close]
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect();
        let end = starts.get(k + 1).copied().unwrap_or(lines.len());
        let body = lines[i..end].join("\n");
        out.push(PreludeFn {
            name,
            params,
            doc: doc_above(&lines, i),
            uses_this: has_word(&body, "this"),
            reads_style: body.contains("style_get("),
        });
    }
    out
}

/// The `//` comment block directly above line `i`, as Markdown text.
pub fn doc_above(lines: &[&str], i: usize) -> Option<String> {
    let mut j = i;
    while j > 0 && lines[j - 1].trim_start().starts_with("//") {
        j -= 1;
    }
    let text: Vec<&str> = lines[j..i]
        .iter()
        .map(|l| {
            let l = l.trim_start().trim_start_matches('/');
            l.strip_prefix(' ').unwrap_or(l).trim_end()
        })
        .filter(|l| !l.chars().all(|c| c == '-' || c == '=') || l.is_empty())
        .collect();
    let text = text.join("\n").trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn has_word(s: &str, w: &str) -> bool {
    s.match_indices(w).any(|(i, _)| {
        let before = s[..i].chars().next_back();
        let after = s[i + w.len()..].chars().next();
        let ident = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        !ident(before) && !ident(after)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_signatures_read_as_script_types() {
        let (name, params, ret) = parse_native(
            "anchor(_: Node, _: types::dynamic::Dynamic, _: types::dynamic::Dynamic) -> core::result::Result<figs_core::script::ScriptNode,alloc::boxed::Box<rhai::types::error::EvalAltResult>>",
        )
        .unwrap();
        assert_eq!(name, "anchor");
        assert_eq!(params, ["Node", "any", "any"]);
        assert_eq!(ret.as_deref(), Some("Node"));
        assert!(parse_native("get$x(_: Node) -> i64").is_none());
    }

    #[test]
    fn prelude_docs_and_overloads() {
        let src = "// -----\n// A grid of `ncols`\n// cells.\nfn grid(ncols, items) { grid(ncols, 1, items) }\n\nfn grid(ncols, g, items) {\n    col(items)\n}\n\nprivate fn hidden() {}\nfn sz() { this.size(3) }\n";
        let fs = prelude_fns(src);
        assert_eq!(fs.len(), 3);
        assert_eq!(fs[0].doc.as_deref(), Some("A grid of `ncols`\ncells."));
        assert_eq!(fs[1].params, ["ncols", "g", "items"]);
        assert!(fs[1].doc.is_none());
        assert!(fs[2].uses_this && !fs[0].uses_this);
    }

    #[test]
    fn catalog_knows_core_and_prelude() {
        let c = Catalog::load();
        let text = c.get("text").unwrap();
        assert_eq!(text.origin, Origin::Core);
        assert!(text.function);
        let size = c.get("size").unwrap();
        assert!(size.method);
        let grid = c.get("grid").unwrap();
        assert_eq!(grid.origin, Origin::Prelude);
        assert!(grid.doc.is_some());
        assert!(c.get("base_size").unwrap().hook);
    }
}
