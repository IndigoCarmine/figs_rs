//! What one script defines and what each of its tokens is, from the token
//! stream alone (no parse tree): the user's `fn`s with their parameters and
//! comments, the `let` / `const` / `for` names, and a [`Role`] per token for
//! semantic highlighting.

use crate::catalog::{doc_above, Catalog};
use crate::lexer::{lex, Kind, Token};

pub const KEYWORDS: &[&str] = &[
    "let", "const", "fn", "private", "if", "else", "switch", "while", "loop", "for", "in", "do",
    "until", "break", "continue", "return", "throw", "try", "catch", "import", "export", "as",
    "this", "true", "false", "global",
];

/// A function the script defines (including one that overrides a hook).
#[derive(Clone, Debug)]
pub struct UserFn {
    pub name: String,
    pub params: Vec<String>,
    pub doc: Option<String>,
    /// The name token.
    pub token: usize,
    /// Token indices of the body's braces.
    pub body: (usize, usize),
}

impl UserFn {
    pub fn signature(&self) -> String {
        format!("{}({})", self.name, self.params.join(", "))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Keyword,
    /// `lib`: a core builder or prelude function (not the script's own).
    Function {
        decl: bool,
        lib: bool,
    },
    Method {
        lib: bool,
    },
    Property,
    Variable {
        decl: bool,
        readonly: bool,
    },
    Parameter {
        decl: bool,
    },
    Str,
    Number,
    Comment,
    Other,
}

pub struct Analysis {
    pub tokens: Vec<Token>,
    pub roles: Vec<Role>,
    pub fns: Vec<UserFn>,
    /// Names bound by `let`, `const` and `for`, with their token index.
    pub vars: Vec<(String, usize)>,
}

fn is_code(t: &Token) -> bool {
    t.kind != Kind::Comment
}

impl Analysis {
    pub fn new(src: &str, catalog: &Catalog) -> Analysis {
        let tokens = lex(src);
        let lines: Vec<&str> = src.lines().collect();
        let mut a = Analysis {
            roles: vec![Role::Other; tokens.len()],
            tokens,
            fns: Vec::new(),
            vars: Vec::new(),
        };
        a.find_definitions(&lines);
        a.assign_roles(catalog);
        a
    }

    /// The code token before / after index `i` (comments skipped).
    pub fn prev(&self, i: usize) -> Option<usize> {
        (0..i).rev().find(|&j| is_code(&self.tokens[j]))
    }

    pub fn next(&self, i: usize) -> Option<usize> {
        (i + 1..self.tokens.len()).find(|&j| is_code(&self.tokens[j]))
    }

    fn text_at(&self, i: Option<usize>) -> Option<&str> {
        i.map(|i| self.tokens[i].text.as_str())
    }

    fn kind_at(&self, i: Option<usize>) -> Option<Kind> {
        i.map(|i| self.tokens[i].kind)
    }

    /// The index of the brace closing the `{` at `open` (or the last token).
    fn matching(&self, open: usize) -> usize {
        let mut depth = 0usize;
        for j in open..self.tokens.len() {
            match self.tokens[j].kind {
                Kind::LBrace => depth += 1,
                Kind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return j;
                    }
                }
                _ => {}
            }
        }
        self.tokens.len().saturating_sub(1)
    }

    fn find_definitions(&mut self, lines: &[&str]) {
        let n = self.tokens.len();
        let mut i = 0;
        while i < n {
            let t = &self.tokens[i];
            if t.kind == Kind::Ident && t.text == "fn" {
                if let Some(f) = self.fn_at(i, lines) {
                    self.fns.push(f);
                }
            } else if t.kind == Kind::Ident && (t.text == "let" || t.text == "const") {
                if let Some(j) = self.next(i).filter(|&j| self.tokens[j].kind == Kind::Ident) {
                    self.vars.push((self.tokens[j].text.clone(), j));
                }
            } else if t.kind == Kind::Ident && t.text == "for" {
                // `for x in …` or `for (x, i) in …`
                let mut j = self.next(i);
                while let Some(k) = j {
                    let tk = &self.tokens[k];
                    match tk.kind {
                        Kind::Ident if tk.text == "in" => break,
                        Kind::Ident => self.vars.push((tk.text.clone(), k)),
                        Kind::LParen | Kind::Comma | Kind::RParen => {}
                        _ => break,
                    }
                    j = self.next(k);
                }
            }
            i += 1;
        }
    }

    /// The function defined by the `fn` keyword at token `i`.
    fn fn_at(&self, i: usize, lines: &[&str]) -> Option<UserFn> {
        let name = self
            .next(i)
            .filter(|&j| self.tokens[j].kind == Kind::Ident)?;
        let open = self
            .next(name)
            .filter(|&j| self.tokens[j].kind == Kind::LParen)?;
        let mut params = Vec::new();
        let mut j = self.next(open)?;
        while self.tokens[j].kind != Kind::RParen {
            if self.tokens[j].kind == Kind::Ident {
                params.push(self.tokens[j].text.clone());
            }
            j = self.next(j)?;
        }
        let body_open = self
            .next(j)
            .filter(|&k| self.tokens[k].kind == Kind::LBrace)?;
        // The comment block above the `fn` (or the `private` before it).
        let first = match self.prev(i) {
            Some(p)
                if self.tokens[p].text == "private"
                    && self.tokens[p].line == self.tokens[i].line =>
            {
                p
            }
            _ => i,
        };
        Some(UserFn {
            name: self.tokens[name].text.clone(),
            params,
            doc: doc_above(lines, self.tokens[first].line as usize),
            token: name,
            body: (body_open, self.matching(body_open)),
        })
    }

    pub fn user_fn(&self, name: &str) -> Option<&UserFn> {
        self.fns.iter().find(|f| f.name == name)
    }

    /// The innermost user function whose parameter list or body holds token `i`.
    fn enclosing_fn(&self, i: usize) -> Option<&UserFn> {
        self.fns
            .iter()
            .filter(|f| f.token < i && i <= f.body.1)
            .min_by_key(|f| f.body.1 - f.token)
    }

    fn assign_roles(&mut self, catalog: &Catalog) {
        for i in 0..self.tokens.len() {
            let role = match self.tokens[i].kind {
                Kind::Comment => Role::Comment,
                Kind::Str => Role::Str,
                Kind::Number => Role::Number,
                Kind::Ident => self.ident_role(i, catalog),
                _ => Role::Other,
            };
            self.roles[i] = role;
        }
    }

    fn ident_role(&self, i: usize, catalog: &Catalog) -> Role {
        let text = self.tokens[i].text.as_str();
        let (prev, next) = (self.prev(i), self.next(i));
        let called = self.kind_at(next) == Some(Kind::LParen);
        if self.text_at(prev) == Some("fn") {
            return Role::Function {
                decl: true,
                lib: false,
            };
        }
        if self.kind_at(prev) == Some(Kind::Dot) {
            if called {
                let lib = self.user_fn(text).is_none() && catalog.get(text).is_some();
                return Role::Method { lib };
            }
            return Role::Property;
        }
        if KEYWORDS.contains(&text) {
            return Role::Keyword;
        }
        if called {
            let lib = self.user_fn(text).is_none() && catalog.get(text).is_some();
            return Role::Function { decl: false, lib };
        }
        if matches!(self.text_at(prev), Some("let" | "const")) {
            return Role::Variable {
                decl: true,
                readonly: self.text_at(prev) == Some("const"),
            };
        }
        // A map key: `#{ key: value }`.
        if self.kind_at(next) == Some(Kind::Colon)
            && matches!(self.kind_at(prev), Some(Kind::LBrace | Kind::Comma))
        {
            return Role::Property;
        }
        if let Some(f) = self.enclosing_fn(i) {
            if f.params.iter().any(|p| p == text) {
                return Role::Parameter { decl: i < f.body.0 };
            }
        }
        let readonly = self
            .vars
            .iter()
            .rev()
            .find(|(name, at)| name == text && *at <= i)
            .is_some_and(|&(_, at)| self.text_at(self.prev(at)) == Some("const"));
        Role::Variable {
            decl: false,
            readonly,
        }
    }

    /// The token at `line:col` (an identifier wins over the punctuation it
    /// touches).
    pub fn token_at(&self, line: u32, col: u32) -> Option<usize> {
        let mut hit = None;
        for (i, t) in self.tokens.iter().enumerate() {
            if t.line > line {
                break;
            }
            if t.touches(line, col) {
                if t.kind == Kind::Ident {
                    return Some(i);
                }
                hit.get_or_insert(i);
            }
        }
        hit
    }

    /// What is being typed at `line:col`.
    pub fn completion_context(&self, line: u32, col: u32) -> Context {
        // The last token starting before the cursor.
        let Some(last) = self
            .tokens
            .iter()
            .rposition(|t| (t.line, t.col) < (line, col))
        else {
            return Context::Global {
                prefix: String::new(),
            };
        };
        let t = &self.tokens[last];
        let inside = t.line == line && col < t.end_col();
        let at_end = t.line == line && col == t.end_col();
        let open_string = t.kind == Kind::Str && at_end && !string_closed(&t.text);
        if (t.kind == Kind::Str || t.kind == Kind::Comment) && inside
            || t.kind == Kind::Comment && at_end
            || open_string
        {
            return Context::None;
        }
        let (prefix, before) = if t.kind == Kind::Ident && (inside || at_end) {
            let take = (col - t.col) as usize;
            let prefix: String = t.text.chars().take(take).collect();
            (prefix, self.prev(last))
        } else {
            (String::new(), Some(last))
        };
        match before.map(|b| (&self.tokens[b].kind, self.tokens[b].text.as_str())) {
            Some((Kind::Dot, _)) => Context::Method { prefix },
            Some((Kind::Ident, "fn")) => Context::FnName { prefix },
            Some((Kind::Ident, "let" | "const")) => Context::None,
            _ => Context::Global { prefix },
        }
    }

    /// The call whose argument list holds `line:col`: the callee's name, if it
    /// is called as a method, and the index of the argument being typed.
    pub fn call_at(&self, line: u32, col: u32) -> Option<(String, bool, usize)> {
        let end = self
            .tokens
            .iter()
            .rposition(|t| (t.line, t.end_col()) <= (line, col))?;
        let mut depth = 0i32;
        let mut arg = 0;
        for j in (0..=end).rev() {
            let t = &self.tokens[j];
            match t.kind {
                Kind::RParen | Kind::RBracket | Kind::RBrace => depth += 1,
                Kind::LBracket | Kind::LBrace if depth > 0 => depth -= 1,
                Kind::LParen if depth > 0 => depth -= 1,
                Kind::LParen => {
                    let callee = self
                        .prev(j)
                        .filter(|&c| self.tokens[c].kind == Kind::Ident)?;
                    let name = self.tokens[callee].text.clone();
                    if KEYWORDS.contains(&name.as_str()) {
                        return None;
                    }
                    let method = self.kind_at(self.prev(callee)) == Some(Kind::Dot);
                    return Some((name, method, arg));
                }
                Kind::LBracket | Kind::LBrace => return None,
                Kind::Comma if depth == 0 => arg += 1,
                Kind::Op if depth == 0 && t.text == ";" => return None,
                _ => {}
            }
        }
        None
    }
}

/// Does string piece `s` end the literal (or hand over to a `${` interpolation),
/// so that what follows it is code?
fn string_closed(s: &str) -> bool {
    if s.ends_with("${") {
        return true;
    }
    // The piece after an interpolation (`}…`) is closed by a back-tick.
    let (quote, body) = match s.strip_prefix('}') {
        Some(rest) => ('`', rest),
        None => {
            let mut cs = s.chars();
            match cs.next() {
                Some(q) => (q, cs.as_str()),
                None => return false,
            }
        }
    };
    body.ends_with(quote) && !body.ends_with(&format!("\\{quote}"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Context {
    /// Inside a string or comment, or naming a new variable.
    None,
    /// A function, variable or keyword.
    Global { prefix: String },
    /// After a `.`.
    Method { prefix: String },
    /// After `fn`: a new function, or a hook to override.
    FnName { prefix: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analysis(src: &str) -> Analysis {
        Analysis::new(src, &Catalog::load())
    }

    fn role_of(a: &Analysis, text: &str, nth: usize) -> Role {
        let i = a
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.text == text)
            .nth(nth)
            .unwrap()
            .0;
        a.roles[i]
    }

    const SRC: &str = "// A boxed label.\nfn boxed(s, w) {\n    text(s).width(w)\n}\nconst W = 3;\nlet m = #{ gap: 1 };\npage(10, 10, boxed(\"x\", W))\n";

    #[test]
    fn finds_user_functions_with_docs() {
        let a = analysis(SRC);
        assert_eq!(a.fns.len(), 1);
        assert_eq!(a.fns[0].signature(), "boxed(s, w)");
        assert_eq!(a.fns[0].doc.as_deref(), Some("A boxed label."));
        assert_eq!(
            a.vars.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
            ["W", "m"]
        );
    }

    #[test]
    fn roles() {
        let a = analysis(SRC);
        assert_eq!(
            role_of(&a, "boxed", 0),
            Role::Function {
                decl: true,
                lib: false
            }
        );
        assert_eq!(
            role_of(&a, "boxed", 1),
            Role::Function {
                decl: false,
                lib: false
            }
        );
        assert_eq!(
            role_of(&a, "text", 0),
            Role::Function {
                decl: false,
                lib: true
            }
        );
        assert_eq!(role_of(&a, "width", 0), Role::Method { lib: true });
        assert_eq!(role_of(&a, "s", 0), Role::Parameter { decl: true });
        assert_eq!(role_of(&a, "s", 1), Role::Parameter { decl: false });
        assert_eq!(
            role_of(&a, "W", 1),
            Role::Variable {
                decl: false,
                readonly: true
            }
        );
        assert_eq!(role_of(&a, "gap", 0), Role::Property);
        assert_eq!(role_of(&a, "let", 0), Role::Keyword);
    }

    #[test]
    fn completion_contexts() {
        let a = analysis("text(\"a\").si\nfn ba\nlet x = gr");
        assert_eq!(
            a.completion_context(0, 12),
            Context::Method {
                prefix: "si".into()
            }
        );
        assert_eq!(
            a.completion_context(0, 10),
            Context::Method {
                prefix: String::new()
            }
        );
        assert_eq!(a.completion_context(0, 7), Context::None);
        assert_eq!(
            a.completion_context(1, 5),
            Context::FnName {
                prefix: "ba".into()
            }
        );
        assert_eq!(
            a.completion_context(2, 10),
            Context::Global {
                prefix: "gr".into()
            }
        );
        assert_eq!(a.completion_context(2, 5), Context::None);
    }

    #[test]
    fn calls_under_the_cursor() {
        let a = analysis("grid(2, [text(\"a\"), x], ");
        assert_eq!(a.call_at(0, 25), Some(("grid".into(), false, 2)));
        assert_eq!(a.call_at(0, 14), Some(("text".into(), false, 0)));
        let a = analysis("x.size(1, ");
        assert_eq!(a.call_at(0, 10), Some(("size".into(), true, 1)));
    }
}
