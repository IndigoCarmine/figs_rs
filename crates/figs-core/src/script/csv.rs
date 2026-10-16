//! `csv(path)`: load a small table into a script as a map of columns.
//!
//! The first non-comment row is the header; every later row is data. A column
//! whose non-empty cells all parse as numbers becomes an array of floats (an
//! empty cell is `NaN`, which the plot framework treats as a gap); any other
//! column stays an array of strings. Fields are comma-separated — or
//! tab-separated when the header has tabs and no commas — with RFC 4180
//! quoting (`"a, b"`, `""` for a literal quote). Blank lines and lines starting
//! with `#` are skipped.
//!
//! Relative paths resolve against the script's directory when the caller
//! knows it ([`super::eval_script_in`]), else the current directory — the same
//! rule the renderers use for image `src`.

use std::path::{Path, PathBuf};

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map};

use super::err;

/// Largest file `csv()` reads (bytes); the framework is for *small* data.
const MAX_CSV_BYTES: u64 = 16 * 1024 * 1024;

/// Register `csv(path)` on `engine`, resolving relative paths against `base`.
pub(super) fn register(engine: &mut Engine, base: Option<PathBuf>) {
    engine.register_fn(
        "csv",
        move |path: &str| -> Result<Map, Box<EvalAltResult>> {
            let full = resolve(base.as_deref(), Path::new(path));
            let text =
                read(&full).map_err(|e| err(format!("csv(): cannot read \"{path}\": {e}")))?;
            let table = parse_csv(&text).map_err(|e| err(format!("csv(): \"{path}\": {e}")))?;
            Ok(to_map(table))
        },
    );
}

fn resolve(base: Option<&Path>, p: &Path) -> PathBuf {
    match base {
        Some(b) if p.is_relative() => b.join(p),
        _ => p.to_path_buf(),
    }
}

fn read(p: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > MAX_CSV_BYTES {
        return Err(format!(
            "file is {} bytes; csv() reads at most {MAX_CSV_BYTES}",
            meta.len()
        ));
    }
    std::fs::read_to_string(p).map_err(|e| e.to_string())
}

/// One parsed column.
#[derive(Debug, Clone, PartialEq)]
pub enum Column {
    Numbers(Vec<f64>),
    Strings(Vec<String>),
}

/// A parsed table: `(header, column)` pairs in file order.
pub type Table = Vec<(String, Column)>;

/// Parse CSV/TSV text into columns (see the module docs for the format).
/// `Err` names the 1-based line at fault.
pub fn parse_csv(text: &str) -> Result<Table, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
    let mut delim = ',';
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if rows.is_empty() && line.contains('\t') && !line.contains(',') {
            delim = '\t';
        }
        let fields = split_line(line, delim).map_err(|e| format!("line {}: {e}", i + 1))?;
        rows.push((i + 1, fields));
    }
    let Some(((_, header), data)) = rows.split_first() else {
        return Err("no header row".into());
    };
    let names: Vec<String> = header.iter().map(|h| h.trim().to_string()).collect();
    for (k, n) in names.iter().enumerate() {
        if n.is_empty() {
            return Err(format!("column {} has an empty header", k + 1));
        }
        if names[..k].contains(n) {
            return Err(format!("duplicate column \"{n}\""));
        }
    }
    let mut cells: Vec<Vec<String>> = vec![Vec::with_capacity(data.len()); names.len()];
    for (line, fields) in data {
        if fields.len() != names.len() {
            return Err(format!(
                "line {line}: expected {} fields, got {}",
                names.len(),
                fields.len()
            ));
        }
        for (col, f) in cells.iter_mut().zip(fields) {
            col.push(f.trim().to_string());
        }
    }
    Ok(names
        .into_iter()
        .zip(cells)
        .map(|(name, col)| (name, typed(col)))
        .collect())
}

/// Numbers when every non-empty cell parses (empty -> NaN), else strings.
fn typed(col: Vec<String>) -> Column {
    let nums: Option<Vec<f64>> = col
        .iter()
        .map(|c| {
            if c.is_empty() {
                Some(f64::NAN)
            } else {
                c.parse::<f64>().ok()
            }
        })
        .collect();
    match nums {
        Some(v) if col.iter().any(|c| !c.is_empty()) => Column::Numbers(v),
        _ => Column::Strings(col),
    }
}

/// Split one line into fields, honouring double-quoted fields.
fn split_line(line: &str, delim: char) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    let mut at_start = true;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' && at_start {
            quoted = true;
            at_start = false;
        } else if c == delim {
            out.push(std::mem::take(&mut cur));
            at_start = true;
        } else {
            if !c.is_whitespace() {
                at_start = false;
            }
            cur.push(c);
        }
    }
    if quoted {
        return Err("unterminated quoted field".into());
    }
    out.push(cur);
    Ok(out)
}

fn to_map(table: Table) -> Map {
    let mut m = Map::new();
    for (name, col) in table {
        let arr: Array = match col {
            Column::Numbers(v) => v.into_iter().map(Dynamic::from_float).collect(),
            Column::Strings(v) => v.into_iter().map(Dynamic::from).collect(),
        };
        m.insert(name.into(), arr.into());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_strings_and_gaps() {
        let t = parse_csv("x,y,name\n0,1.5,a\n1,,b\n2,-3e-1,\"c, d\"\n").unwrap();
        assert_eq!(t[0], ("x".into(), Column::Numbers(vec![0.0, 1.0, 2.0])));
        match &t[1].1 {
            Column::Numbers(v) => {
                assert_eq!(v[0], 1.5);
                assert!(v[1].is_nan());
                assert_eq!(v[2], -0.3);
            }
            c => panic!("{c:?}"),
        }
        assert_eq!(
            t[2].1,
            Column::Strings(vec!["a".into(), "b".into(), "c, d".into()])
        );
    }

    #[test]
    fn tsv_comments_bom_and_quotes() {
        let t = parse_csv("\u{feff}# comment\na\tb\n\n1\t\"say \"\"hi\"\"\"\n").unwrap();
        assert_eq!(t[0].1, Column::Numbers(vec![1.0]));
        assert_eq!(t[1].1, Column::Strings(vec!["say \"hi\"".into()]));
    }

    #[test]
    fn errors_name_the_line() {
        assert_eq!(
            parse_csv("a,b\n1,2\n3\n").unwrap_err(),
            "line 3: expected 2 fields, got 1"
        );
        assert_eq!(parse_csv("").unwrap_err(), "no header row");
        assert!(parse_csv("a,a\n1,2").unwrap_err().contains("duplicate"));
        assert!(parse_csv("a\n\"open\n").unwrap_err().contains("line 2"));
    }
}
