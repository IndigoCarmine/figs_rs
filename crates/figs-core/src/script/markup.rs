//! A compact inline markup for rich-text spans — framework sugar over the
//! core `spans` model (it only *produces* [`RawSpan`]s; layout never sees it).
//!
//! ```text
//! A. Tanaka *et al.*, *Nature* **2021**, *12*, 345–352
//! ΔG_{300K} = {#e0635a bold|−27.5} kJ/mol      H_2O   4.2 Debye^†
//! ```
//!
//! | Syntax | Meaning |
//! |---|---|
//! | `*text*` | italic |
//! | `**text**` | bold |
//! | `__text__` | underline |
//! | `~~text~~` | line-through |
//! | `_x`, `_{text}` | subscript (one character, or a braced group) |
//! | `^x`, `^{text}` | superscript |
//! | `{spec\|text}` | style group, `spec` = space-separated items: `#rrggbb` (colour), `bold`/`b`, `italic`/`i`, `upright`, `underline`/`u`, `strike`/`s`, `sup`, `sub`, `size=12` (pt), `weight=600`, `font=Arial` or `font='Times New Roman'`, `tracking=0.1` (em), `shift=0.3` (em), `color=#hex` |
//! | `\c` | the character `c` literally (`\*`, `\_`, `\^`, `\{`, `\}`, `\~`, `\\`) |
//!
//! Groups nest freely (`_{*n*}` is an italic subscript). `*`/`**`/`__`/`~~`
//! follow CommonMark-like flanking: a delimiter opens only when followed by a
//! non-space *and* a matching closer (preceded by a non-space) follows in the
//! same brace group; otherwise it is literal — so `5 * 3`, `A. Tanaka^{1,*}`
//! and `*corresponding author` need no escaping. `_`/`^` followed by a space
//! or the end are literal too. Unbalanced braces, a group without `|`, an
//! unknown style item and a dangling `\` are errors naming the 1-based
//! character column.

use crate::geom::Color;
use crate::schema::span::{FontStyle, RawSpan, Script};

/// A markup syntax error at a 1-based character column.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("markup: {message} (column {column})")]
pub struct MarkupError {
    pub message: String,
    pub column: usize,
}

/// The style in effect at a point of the markup (unset = inherit the node).
#[derive(Debug, Clone, Default, PartialEq)]
struct State {
    italic: Option<bool>,
    bold: bool,
    weight: Option<u16>,
    underline: bool,
    strike: bool,
    script: Option<Script>,
    color: Option<Color>,
    size: Option<f32>,
    font: Option<String>,
    tracking: Option<f32>,
    shift: Option<f32>,
}

impl State {
    fn span(&self, text: String) -> RawSpan {
        RawSpan {
            text,
            font_family: self.font.clone(),
            font_size: self.size,
            font_weight: self.weight.or(self.bold.then_some(700)),
            font_style: self.italic.map(|i| {
                if i {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                }
            }),
            color: self.color,
            underline: self.underline.then_some(true),
            strike: self.strike.then_some(true),
            script: self.script,
            baseline_shift: self.shift,
            letter_spacing: self.tracking,
        }
    }
}

/// What ends the sequence being parsed.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Until {
    End,
    /// A delimiter (`*`, `**`, `__`, `~~`) opened at a column.
    Delim(&'static str, usize),
    /// A `}` closing a group opened at a column.
    Brace(usize),
}

struct Parser {
    chars: Vec<char>,
    i: usize,
    out: Vec<RawSpan>,
}

/// Parse `src` into spans. Adjacent text with the same style is merged.
pub fn parse(src: &str) -> Result<Vec<RawSpan>, MarkupError> {
    let mut p = Parser {
        chars: src.chars().collect(),
        i: 0,
        out: Vec::new(),
    };
    p.seq(&State::default(), Until::End)?;
    Ok(p.out)
}

fn error<T>(message: impl Into<String>, column: usize) -> Result<T, MarkupError> {
    Err(MarkupError {
        message: message.into(),
        column,
    })
}

impl Parser {
    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.i + k).copied()
    }

    fn run_len(&self, c: char) -> usize {
        self.chars[self.i..].iter().take_while(|&&x| x == c).count()
    }

    fn flush(&mut self, buf: &mut String, st: &State) {
        if buf.is_empty() {
            return;
        }
        let text = std::mem::take(buf);
        let span = st.span(text);
        if let Some(last) = self.out.last_mut() {
            let same_style = RawSpan {
                text: String::new(),
                ..last.clone()
            } == RawSpan {
                text: String::new(),
                ..span.clone()
            };
            if same_style {
                last.text.push_str(&span.text);
                return;
            }
        }
        self.out.push(span);
    }

    fn seq(&mut self, st: &State, until: Until) -> Result<(), MarkupError> {
        let mut buf = String::new();
        loop {
            let Some(c) = self.peek(0) else {
                self.flush(&mut buf, st);
                return match until {
                    Until::End => Ok(()),
                    Until::Delim(d, col) => error(format!("unclosed `{d}`"), col),
                    Until::Brace(col) => error("unclosed `{`", col),
                };
            };
            let col = self.i + 1;
            match c {
                '\\' => {
                    let Some(n) = self.peek(1) else {
                        return error("dangling `\\` at the end", col);
                    };
                    buf.push(n);
                    self.i += 2;
                }
                '*' | '_' | '~' if self.closes(until, c) => {
                    let Until::Delim(u, _) = until else {
                        unreachable!("closes() only matches a delimiter")
                    };
                    self.i += u.len();
                    self.flush(&mut buf, st);
                    return Ok(());
                }
                '*' | '_' | '~' if self.delimiter(c).is_some() => {
                    let d = self.delimiter(c).expect("checked");
                    let next = self.peek(d.len());
                    let can_open = next.is_some_and(|n| !n.is_whitespace()) && self.has_closer(d);
                    if can_open {
                        self.flush(&mut buf, st);
                        self.i += d.len();
                        let mut inner = st.clone();
                        match d {
                            "*" => inner.italic = Some(true),
                            "**" => inner.bold = true,
                            "__" => inner.underline = true,
                            _ => inner.strike = true,
                        }
                        self.seq(&inner, Until::Delim(d, col))?;
                    } else {
                        buf.push_str(d);
                        self.i += d.len();
                    }
                }
                '_' | '^' => {
                    let script = if c == '_' { Script::Sub } else { Script::Super };
                    let mut inner = st.clone();
                    inner.script = Some(script);
                    match self.peek(1) {
                        Some('{') => {
                            self.flush(&mut buf, st);
                            self.i += 2;
                            self.seq(&inner, Until::Brace(col))?;
                        }
                        Some('\\') if self.peek(2).is_some() => {
                            self.flush(&mut buf, st);
                            let ch = self.peek(2).expect("checked");
                            self.i += 3;
                            self.flush(&mut ch.to_string(), &inner);
                        }
                        Some(n) if !n.is_whitespace() && n != '}' => {
                            self.flush(&mut buf, st);
                            self.i += 2;
                            self.flush(&mut n.to_string(), &inner);
                        }
                        _ => {
                            buf.push(c);
                            self.i += 1;
                        }
                    }
                }
                '{' => {
                    self.flush(&mut buf, st);
                    let inner = self.group_spec(st, col)?;
                    self.seq(&inner, Until::Brace(col))?;
                }
                '}' => {
                    if let Until::Brace(_) = until {
                        self.i += 1;
                        self.flush(&mut buf, st);
                        return Ok(());
                    }
                    return error("unmatched `}` (write `\\}` for a literal brace)", col);
                }
                _ => {
                    buf.push(c);
                    self.i += 1;
                }
            }
        }
    }

    /// Whether the `c` run at the cursor closes the innermost open delimiter:
    /// it starts with that delimiter and follows a non-space.
    fn closes(&self, until: Until, c: char) -> bool {
        let Until::Delim(u, _) = until else {
            return false;
        };
        let prev = (self.i > 0).then(|| self.chars[self.i - 1]);
        u.starts_with(c) && self.run_len(c) >= u.len() && prev.is_some_and(|p| !p.is_whitespace())
    }

    /// Whether a closing `d` (preceded by a non-space) follows the opener at
    /// the cursor within the same brace group. Without one the opener is
    /// literal text (`A. Tanaka^{1,*}`, `*corresponding author`).
    fn has_closer(&self, d: &str) -> bool {
        let pat: Vec<char> = d.chars().collect();
        let mut depth = 0i32;
        let mut j = self.i + pat.len();
        while j < self.chars.len() {
            match self.chars[j] {
                '\\' => {
                    j += 2;
                    continue;
                }
                '{' => depth += 1,
                '}' if depth == 0 => return false,
                '}' => depth -= 1,
                _ => {}
            }
            if depth == 0
                && self.chars[j..].starts_with(&pat)
                && !self.chars[j - 1].is_whitespace()
                && j > self.i + pat.len()
            {
                return true;
            }
            j += 1;
        }
        false
    }

    /// The paired delimiter starting at the cursor, if `c` begins one:
    /// `**`/`*` for `*`, `__` for `_`, `~~` for `~`.
    fn delimiter(&self, c: char) -> Option<&'static str> {
        match (c, self.run_len(c)) {
            ('*', n) if n >= 2 => Some("**"),
            ('*', _) => Some("*"),
            ('_', n) if n >= 2 => Some("__"),
            ('~', n) if n >= 2 => Some("~~"),
            _ => None,
        }
    }

    /// Parse `{spec|` (cursor on `{`), leaving the cursor after `|`.
    fn group_spec(&mut self, st: &State, col: usize) -> Result<State, MarkupError> {
        let start = self.i + 1;
        let mut j = start;
        let mut quote: Option<char> = None;
        loop {
            match self.chars.get(j) {
                None => return error("style group `{` has no `|`", col),
                Some(&q) if quote == Some(q) => quote = None,
                Some(_) if quote.is_some() => {}
                Some(&q @ ('\'' | '"')) => quote = Some(q),
                Some('|') => break,
                Some('}') => {
                    return error(
                        "style group `{` has no `|` (write `\\{` for a literal brace)",
                        col,
                    )
                }
                Some(_) => {}
            }
            j += 1;
        }
        let spec: String = self.chars[start..j].iter().collect();
        self.i = j + 1;
        let mut inner = st.clone();
        for item in split_spec(&spec) {
            apply_item(&mut inner, &item).map_err(|m| MarkupError {
                message: format!("in style group: {m}"),
                column: col,
            })?;
        }
        Ok(inner)
    }
}

/// Space-separated spec items; `'…'`/`"…"` quote a value containing spaces.
fn split_spec(spec: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in spec.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    items.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        items.push(cur);
    }
    items
}

fn apply_item(st: &mut State, item: &str) -> Result<(), String> {
    let num = |v: &str| {
        v.parse::<f32>()
            .ok()
            .filter(|x| x.is_finite())
            .ok_or_else(|| format!("`{item}`: expected a number"))
    };
    let color =
        |v: &str| Color::parse_hex(v).map_err(|_| format!("`{item}`: invalid colour `{v}`"));
    if item.starts_with('#') {
        st.color = Some(color(item)?);
        return Ok(());
    }
    if let Some((k, v)) = item.split_once('=') {
        match k {
            "color" | "colour" => st.color = Some(color(v)?),
            "size" => {
                let s = num(v.trim_end_matches("pt"))?;
                if s <= 0.0 {
                    return Err(format!("`{item}`: size must be positive"));
                }
                st.size = Some(s);
            }
            "weight" => {
                let w = num(v)?;
                if !(1.0..=1000.0).contains(&w) {
                    return Err(format!("`{item}`: weight must be 1–1000"));
                }
                st.weight = Some(w.round() as u16);
            }
            "font" => st.font = Some(v.to_string()),
            "tracking" => st.tracking = Some(num(v)?),
            "shift" => st.shift = Some(num(v)?),
            _ => return Err(format!("unknown key `{k}`")),
        }
        return Ok(());
    }
    match item {
        "b" | "bold" => st.bold = true,
        "i" | "italic" => st.italic = Some(true),
        "upright" => st.italic = Some(false),
        "u" | "underline" => st.underline = true,
        "s" | "strike" => st.strike = true,
        "sup" => st.script = Some(Script::Super),
        "sub" => st.script = Some(Script::Sub),
        _ => return Err(format!("unknown item `{item}`")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(spans: &[RawSpan]) -> Vec<&str> {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn plain_text_is_one_span() {
        let s = parse("hello world").unwrap();
        assert_eq!(s, vec![RawSpan::new("hello world")]);
        assert!(parse("").unwrap().is_empty());
    }

    #[test]
    fn reference_line() {
        let s = parse("A. Tanaka *et al.*, *Nature* **2021**, *12*, 345–352").unwrap();
        assert_eq!(
            texts(&s),
            vec![
                "A. Tanaka ",
                "et al.",
                ", ",
                "Nature",
                " ",
                "2021",
                ", ",
                "12",
                ", 345–352"
            ]
        );
        assert_eq!(s[1].font_style, Some(FontStyle::Italic));
        assert_eq!(s[5].font_weight, Some(700));
        assert_eq!(s[5].font_style, None);
        assert_eq!(s[0].font_style, None);
    }

    #[test]
    fn scripts() {
        let s = parse("ΔG_{300K} H_2O Debye^† x^{*n*}").unwrap();
        assert_eq!(
            texts(&s),
            vec!["ΔG", "300K", " H", "2", "O Debye", "†", " x", "n"]
        );
        assert_eq!(s[1].script, Some(Script::Sub));
        assert_eq!(s[3].script, Some(Script::Sub));
        assert_eq!(s[5].script, Some(Script::Super));
        assert_eq!(s[7].script, Some(Script::Super));
        assert_eq!(s[7].font_style, Some(FontStyle::Italic));
    }

    #[test]
    fn nesting_and_triple_stars() {
        let s = parse("**bold *both* bold** ***x***").unwrap();
        assert_eq!(texts(&s), vec!["bold ", "both", " bold", " ", "x"]);
        assert_eq!(s[1].font_weight, Some(700));
        assert_eq!(s[1].font_style, Some(FontStyle::Italic));
        assert_eq!(s[4].font_weight, Some(700));
        assert_eq!(s[4].font_style, Some(FontStyle::Italic));
    }

    #[test]
    fn style_groups() {
        let s = parse(
            "in {#f0a030|10 μM} and {#ffd700 bold size=14 font='Times New Roman'|B} = {u|Y. Sato}",
        )
        .unwrap();
        assert_eq!(
            texts(&s),
            vec!["in ", "10 μM", " and ", "B", " = ", "Y. Sato"]
        );
        assert_eq!(s[1].color, Color::parse_hex("#f0a030").ok());
        assert_eq!(s[3].font_weight, Some(700));
        assert_eq!(s[3].font_size, Some(14.0));
        assert_eq!(s[3].font_family.as_deref(), Some("Times New Roman"));
        assert_eq!(s[5].underline, Some(true));
    }

    #[test]
    fn underline_strike_and_literals() {
        let s = parse("__u__ ~~s~~ 5 * 3 a_ b ^ c").unwrap();
        assert_eq!(texts(&s), vec!["u", " ", "s", " 5 * 3 a_ b ^ c"]);
        assert_eq!(s[0].underline, Some(true));
        assert_eq!(s[2].strike, Some(true));
    }

    #[test]
    fn escapes() {
        let s = parse(r"\*not\* \_ \{x\} \\ a\^b").unwrap();
        assert_eq!(texts(&s), vec![r"*not* _ {x} \ a^b"]);
        let s = parse(r"x_\*").unwrap();
        assert_eq!(texts(&s), vec!["x", "*"]);
    }

    #[test]
    fn adjacent_equal_styles_merge() {
        let s = parse("*a**b*").unwrap();
        // `*a*` `*b*`? No: `**` inside an italic run opens bold only when
        // flanked; here it closes nothing and opens bold "b*"... check it
        // parses deterministically and keeps all text.
        let all: String = s.iter().map(|x| x.text.as_str()).collect();
        assert!(all.contains('a') && all.contains('b'));
        let s = parse("{b|x}{b|y}").unwrap();
        assert_eq!(texts(&s), vec!["xy"]);
    }

    #[test]
    fn errors_have_columns() {
        // A delimiter without a closer is literal...
        assert_eq!(texts(&parse("ab **cd").unwrap()), vec!["ab **cd"]);
        assert_eq!(
            texts(&parse("Tanaka^{1,*}").unwrap()),
            vec!["Tanaka", "1,*"]
        );
        // ...but one whose closer was taken by an inner group is unclosed.
        let e = parse("*a **b**").unwrap_err();
        assert_eq!(e.column, 1);
        assert!(e.message.contains("unclosed `*`"), "{e}");
        let e = parse("x}").unwrap_err();
        assert_eq!(e.column, 2);
        let e = parse("a {#zz|b}").unwrap_err();
        assert_eq!(e.column, 3);
        assert!(e.to_string().contains("invalid colour"), "{e}");
        let e = parse("a {bold b}").unwrap_err();
        assert!(e.message.contains("no `|`"), "{e}");
        let e = parse("H_{2").unwrap_err();
        assert_eq!(e.column, 2);
        let e = parse("{wat|x}").unwrap_err();
        assert!(e.message.contains("unknown item `wat`"), "{e}");
        let e = parse("end\\").unwrap_err();
        assert_eq!(e.column, 4);
    }
}
