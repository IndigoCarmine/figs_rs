//! A small Rhai tokenizer for editor features.
//!
//! It only has to tell identifiers, keywords, literals, comments and
//! punctuation apart — enough to colour a script and to know what the cursor
//! is on — and it never fails: anything it does not recognise becomes an
//! [`Kind::Op`]. Positions are LSP positions (0-based line, UTF-16 column), and
//! a token never spans lines: a multi-line string or block comment comes out as
//! one piece per line, as semantic tokens need.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Ident,
    Number,
    /// A string or character literal (or one line of it). The literal parts of
    /// a back-tick string are strings; its `${…}` interpolations are lexed as
    /// code.
    Str,
    Comment,
    /// `.` and `?.`
    Dot,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    /// Any other operator or punctuation.
    Op,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    pub line: u32,
    /// Start column in UTF-16 code units.
    pub col: u32,
    /// Length in UTF-16 code units.
    pub len: u32,
}

impl Token {
    pub fn end_col(&self) -> u32 {
        self.col + self.len
    }

    /// Does this token contain (or end right at) `line:col`?
    pub fn touches(&self, line: u32, col: u32) -> bool {
        self.line == line && self.col <= col && col <= self.end_col()
    }
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    line: u32,
    col: u32,
    out: Vec<Token>,
}

pub fn lex(src: &str) -> Vec<Token> {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        i: 0,
        line: 0,
        col: 0,
        out: Vec::new(),
    };
    lx.code(false);
    lx.out
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

impl Lexer {
    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.i + k).copied()
    }

    /// Consume one character, keeping `line` / `col` current.
    fn bump(&mut self) -> Option<char> {
        let c = self.peek(0)?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 0;
        } else {
            self.col += c.len_utf16() as u32;
        }
        Some(c)
    }

    fn push(&mut self, kind: Kind, text: String, line: u32, col: u32) {
        if text.is_empty() {
            return;
        }
        let len = text.chars().map(|c| c.len_utf16() as u32).sum();
        self.out.push(Token {
            kind,
            text,
            line,
            col,
            len,
        });
    }

    /// Lex code until the end, or (in an interpolation, `nested`) until the
    /// `}` that closes it, which is consumed.
    fn code(&mut self, nested: bool) {
        let mut depth = 0usize;
        while let Some(c) = self.peek(0) {
            let (line, col) = (self.line, self.col);
            if c.is_whitespace() {
                self.bump();
            } else if c == '/' && self.peek(1) == Some('/') {
                let mut s = String::new();
                while let Some(c) = self.peek(0) {
                    if c == '\n' || c == '\r' {
                        break;
                    }
                    s.push(c);
                    self.bump();
                }
                self.push(Kind::Comment, s, line, col);
            } else if c == '/' && self.peek(1) == Some('*') {
                self.block_comment();
            } else if c == '"' || c == '\'' {
                self.quoted(c);
            } else if c == '`' {
                self.backtick();
            } else if c.is_ascii_digit() {
                self.number();
            } else if is_ident_start(c) {
                let mut s = String::new();
                while let Some(c) = self.peek(0).filter(|&c| is_ident_char(c)) {
                    s.push(c);
                    self.bump();
                }
                self.push(Kind::Ident, s, line, col);
            } else {
                self.bump();
                let kind = match c {
                    '.' => Kind::Dot,
                    '?' if self.peek(0) == Some('.') => {
                        self.bump();
                        self.push(Kind::Dot, "?.".into(), line, col);
                        continue;
                    }
                    '(' => Kind::LParen,
                    ')' => Kind::RParen,
                    '{' => {
                        depth += 1;
                        Kind::LBrace
                    }
                    '}' => {
                        if nested && depth == 0 {
                            return;
                        }
                        depth = depth.saturating_sub(1);
                        Kind::RBrace
                    }
                    '[' => Kind::LBracket,
                    ']' => Kind::RBracket,
                    ',' => Kind::Comma,
                    ':' if self.peek(0) == Some(':') => {
                        self.bump();
                        self.push(Kind::Op, "::".into(), line, col);
                        continue;
                    }
                    ':' => Kind::Colon,
                    _ => Kind::Op,
                };
                self.push(kind, c.to_string(), line, col);
            }
        }
    }

    /// `/* … */`, nested as Rhai allows, one token per line.
    fn block_comment(&mut self) {
        let (mut line, mut col) = (self.line, self.col);
        let mut s = String::new();
        let mut depth = 0usize;
        while let Some(c) = self.peek(0) {
            if c == '/' && self.peek(1) == Some('*') {
                depth += 1;
                s.push_str("/*");
                self.bump();
                self.bump();
            } else if c == '*' && self.peek(1) == Some('/') {
                s.push_str("*/");
                self.bump();
                self.bump();
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if c == '\n' {
                self.push(
                    Kind::Comment,
                    std::mem::take(&mut s).trim_end_matches('\r').into(),
                    line,
                    col,
                );
                self.bump();
                (line, col) = (self.line, self.col);
            } else {
                s.push(c);
                self.bump();
            }
        }
        self.push(Kind::Comment, s, line, col);
    }

    /// A `"…"` string or `'…'` character, with `\` escapes.
    fn quoted(&mut self, quote: char) {
        let (mut line, mut col) = (self.line, self.col);
        let mut s = String::new();
        s.push(quote);
        self.bump();
        while let Some(c) = self.peek(0) {
            if c == '\\' {
                s.push(c);
                self.bump();
                if let Some(e) = self.peek(0) {
                    if e == '\n' {
                        // A line continuation: the string goes on below.
                        self.push(Kind::Str, std::mem::take(&mut s), line, col);
                        self.bump();
                        (line, col) = (self.line, self.col);
                    } else {
                        s.push(e);
                        self.bump();
                    }
                }
            } else if c == '\n' {
                // Unterminated: stop at the line's end.
                break;
            } else {
                s.push(c);
                self.bump();
                if c == quote {
                    break;
                }
            }
        }
        self.push(Kind::Str, s.trim_end_matches('\r').into(), line, col);
    }

    /// A back-tick string: literal parts (one token per line) around `${…}`
    /// interpolations, which are lexed as code.
    fn backtick(&mut self) {
        let (mut line, mut col) = (self.line, self.col);
        let mut s = String::from('`');
        self.bump();
        while let Some(c) = self.peek(0) {
            if c == '$' && self.peek(1) == Some('{') {
                s.push_str("${");
                self.bump();
                self.bump();
                self.push(Kind::Str, std::mem::take(&mut s), line, col);
                self.code(true);
                // The closing `}` of the interpolation belongs to the string.
                (line, col) = (self.line, self.col.saturating_sub(1));
                s.push('}');
            } else if c == '\n' {
                self.push(
                    Kind::Str,
                    std::mem::take(&mut s).trim_end_matches('\r').into(),
                    line,
                    col,
                );
                self.bump();
                (line, col) = (self.line, self.col);
            } else {
                s.push(c);
                self.bump();
                if c == '`' {
                    break;
                }
            }
        }
        self.push(Kind::Str, s, line, col);
    }

    /// Decimal (`1_000`, `2.5e-3`) or `0x` / `0o` / `0b` integers. A `.` is
    /// only part of the number when a digit follows (`1..5` is a range).
    fn number(&mut self) {
        let (line, col) = (self.line, self.col);
        let mut s = String::new();
        let radix = self.peek(0) == Some('0') && matches!(self.peek(1), Some('x' | 'o' | 'b'));
        if radix {
            s.push(self.bump().unwrap());
            s.push(self.bump().unwrap());
        }
        while let Some(c) = self.peek(0) {
            let digit = if radix {
                c.is_ascii_hexdigit() || c == '_'
            } else {
                c.is_ascii_digit() || c == '_'
            };
            let point = !radix && c == '.' && self.peek(1).is_some_and(|d| d.is_ascii_digit());
            if digit || point {
                s.push(c);
                self.bump();
            } else if !radix && (c == 'e' || c == 'E') {
                let sign = matches!(self.peek(1), Some('+' | '-'));
                let after = if sign { self.peek(2) } else { self.peek(1) };
                if !after.is_some_and(|d| d.is_ascii_digit()) {
                    break;
                }
                s.push(c);
                self.bump();
                if sign {
                    s.push(self.bump().unwrap());
                }
            } else {
                break;
            }
        }
        self.push(Kind::Number, s, line, col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(Kind, String)> {
        lex(src).into_iter().map(|t| (t.kind, t.text)).collect()
    }

    #[test]
    fn calls_methods_and_literals() {
        let k = kinds(r#"text("Hi").size(12.5) // note"#);
        assert_eq!(
            k,
            vec![
                (Kind::Ident, "text".into()),
                (Kind::LParen, "(".into()),
                (Kind::Str, "\"Hi\"".into()),
                (Kind::RParen, ")".into()),
                (Kind::Dot, ".".into()),
                (Kind::Ident, "size".into()),
                (Kind::LParen, "(".into()),
                (Kind::Number, "12.5".into()),
                (Kind::RParen, ")".into()),
                (Kind::Comment, "// note".into()),
            ]
        );
    }

    #[test]
    fn ranges_are_not_floats() {
        let k = kinds("0..5");
        assert_eq!(k[0], (Kind::Number, "0".into()));
        assert_eq!(k[1].0, Kind::Dot);
        assert_eq!(k[3], (Kind::Number, "5".into()));
    }

    #[test]
    fn interpolations_are_code() {
        let toks = lex("`a${ f(x) }b`");
        let k: Vec<_> = toks.iter().map(|t| (t.kind, t.text.as_str())).collect();
        assert_eq!(
            k,
            vec![
                (Kind::Str, "`a${"),
                (Kind::Ident, "f"),
                (Kind::LParen, "("),
                (Kind::Ident, "x"),
                (Kind::RParen, ")"),
                (Kind::Str, "}b`"),
            ]
        );
        // The closing piece starts at the `}`.
        assert_eq!(toks[5].col, 10);
    }

    #[test]
    fn multiline_pieces_and_utf16_columns() {
        let toks = lex("/* a\n b */ \"日本\" x");
        assert_eq!((toks[0].line, toks[0].text.as_str()), (0, "/* a"));
        assert_eq!(
            (toks[1].line, toks[1].col, toks[1].text.as_str()),
            (1, 0, " b */")
        );
        assert_eq!((toks[2].col, toks[2].len), (6, 4));
        assert_eq!((toks[3].col, toks[3].text.as_str()), (11, "x"));
    }
}
