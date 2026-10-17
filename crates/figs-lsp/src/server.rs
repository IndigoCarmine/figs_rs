//! The LSP loop: documents are synced whole, every request is answered from a
//! fresh [`Analysis`] of the text, and diagnostics come from evaluating the
//! script on a worker thread shortly after the last edit.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, Request as _,
    SemanticTokensFullRequest, SignatureHelpRequest,
};
use lsp_types::*;
use serde_json::Value;

use crate::analysis::{Analysis, Context, Role, KEYWORDS};
use crate::catalog::{Catalog, Entry, Origin};
use crate::lexer::{lex, Kind};

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::COMMENT,
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFAULT_LIBRARY,
    SemanticTokenModifier::READONLY,
];

pub fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".into()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".into(), ",".into()]),
            retrigger_characters: None,
            work_done_progress_options: Default::default(),
        }),
        definition_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                work_done_progress_options: Default::default(),
                legend: SemanticTokensLegend {
                    token_types: TOKEN_TYPES.to_vec(),
                    token_modifiers: TOKEN_MODIFIERS.to_vec(),
                },
                range: None,
                full: Some(SemanticTokensFullOptions::Bool(true)),
            },
        )),
        ..Default::default()
    }
}

/// An evaluation for the diagnostics worker; `text: None` clears the file's
/// diagnostics (it was closed).
struct Job {
    uri: Url,
    version: Option<i32>,
    text: Option<String>,
}

pub struct Server {
    catalog: Arc<Catalog>,
    docs: HashMap<Url, String>,
    jobs: mpsc::Sender<Job>,
}

pub fn run(connection: Connection) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let caps = serde_json::to_value(capabilities())?;
    connection.initialize(caps)?;

    let (jobs, rx) = mpsc::channel();
    let sender = connection.sender.clone();
    std::thread::spawn(move || {
        for (uri, version, diagnostics) in diagnostics_worker(rx) {
            let params = PublishDiagnosticsParams {
                uri,
                diagnostics,
                version,
            };
            let note = Notification::new(PublishDiagnostics::METHOD.into(), params);
            if sender.send(Message::Notification(note)).is_err() {
                break;
            }
        }
    });

    let mut server = Server::new(jobs);
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                let id = req.id.clone();
                let resp = match server.request(req) {
                    Ok(v) => Response::new_ok(id, v),
                    Err(e) => Response::new_err(id, lsp_server::ErrorCode::InvalidParams as i32, e),
                };
                connection.sender.send(Message::Response(resp))?;
            }
            Message::Notification(note) => server.notification(note),
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn params<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| e.to_string())
}

fn markdown(value: String) -> MarkupContent {
    MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    }
}

impl Server {
    fn new(jobs: mpsc::Sender<Job>) -> Server {
        Server {
            catalog: Arc::new(Catalog::load()),
            docs: HashMap::new(),
            jobs,
        }
    }

    fn notification(&mut self, note: Notification) {
        match note.method.as_str() {
            DidOpenTextDocument::METHOD => {
                if let Ok(p) = params::<DidOpenTextDocumentParams>(note.params) {
                    let d = p.text_document;
                    self.set(d.uri, Some(d.version), d.text);
                }
            }
            DidChangeTextDocument::METHOD => {
                if let Ok(mut p) = params::<DidChangeTextDocumentParams>(note.params) {
                    // Full sync: the last change is the whole text.
                    if let Some(c) = p.content_changes.pop() {
                        let d = p.text_document;
                        self.set(d.uri, Some(d.version), c.text);
                    }
                }
            }
            DidCloseTextDocument::METHOD => {
                if let Ok(p) = params::<DidCloseTextDocumentParams>(note.params) {
                    let uri = p.text_document.uri;
                    self.docs.remove(&uri);
                    let _ = self.jobs.send(Job {
                        uri,
                        version: None,
                        text: None,
                    });
                }
            }
            _ => {}
        }
    }

    fn set(&mut self, uri: Url, version: Option<i32>, text: String) {
        let _ = self.jobs.send(Job {
            uri: uri.clone(),
            version,
            text: Some(text.clone()),
        });
        self.docs.insert(uri, text);
    }

    fn analysis(&self, uri: &Url) -> Result<Analysis, String> {
        let text = self.docs.get(uri).ok_or("unknown document")?;
        Ok(Analysis::new(text, &self.catalog))
    }

    fn request(&mut self, req: Request) -> Result<Value, String> {
        let v = match req.method.as_str() {
            Completion::METHOD => {
                let p: CompletionParams = params(req.params)?;
                let pos = p.text_document_position;
                let a = self.analysis(&pos.text_document.uri)?;
                serde_json::to_value(self.completion(&a, pos.position))
            }
            HoverRequest::METHOD => {
                let p: HoverParams = params(req.params)?;
                let pos = p.text_document_position_params;
                let a = self.analysis(&pos.text_document.uri)?;
                serde_json::to_value(self.hover(&a, pos.position))
            }
            SignatureHelpRequest::METHOD => {
                let p: SignatureHelpParams = params(req.params)?;
                let pos = p.text_document_position_params;
                let a = self.analysis(&pos.text_document.uri)?;
                serde_json::to_value(self.signature_help(&a, pos.position))
            }
            GotoDefinition::METHOD => {
                let p: GotoDefinitionParams = params(req.params)?;
                let pos = p.text_document_position_params;
                let a = self.analysis(&pos.text_document.uri)?;
                let loc = definition(&a, pos.position)
                    .map(|range| Location::new(pos.text_document.uri.clone(), range));
                serde_json::to_value(loc.map(GotoDefinitionResponse::Scalar))
            }
            DocumentSymbolRequest::METHOD => {
                let p: DocumentSymbolParams = params(req.params)?;
                let a = self.analysis(&p.text_document.uri)?;
                serde_json::to_value(DocumentSymbolResponse::Nested(symbols(&a)))
            }
            SemanticTokensFullRequest::METHOD => {
                let p: SemanticTokensParams = params(req.params)?;
                let a = self.analysis(&p.text_document.uri)?;
                serde_json::to_value(SemanticTokensResult::Tokens(semantic_tokens(&a)))
            }
            _ => return Ok(Value::Null),
        };
        v.map_err(|e| e.to_string())
    }

    fn completion(&self, a: &Analysis, pos: Position) -> Option<CompletionResponse> {
        let items = match a.completion_context(pos.line, pos.character) {
            Context::None => return None,
            Context::Global { .. } => self.global_items(a),
            Context::Method { .. } => self.method_items(a),
            Context::FnName { .. } => self.hook_items(),
        };
        Some(CompletionResponse::Array(items))
    }

    fn global_items(&self, a: &Analysis) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for f in &a.fns {
            items.push(CompletionItem {
                label: f.name.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(f.signature()),
                documentation: f
                    .doc
                    .clone()
                    .map(|d| Documentation::MarkupContent(markdown(d))),
                insert_text: Some(call_snippet(&f.name, !f.params.is_empty())),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                sort_text: Some(format!("0{}", f.name)),
                ..Default::default()
            });
        }
        let mut seen = std::collections::HashSet::new();
        for (name, _) in a.vars.iter().rev() {
            if seen.insert(name.clone()) {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    sort_text: Some(format!("0{name}")),
                    ..Default::default()
                });
            }
        }
        for e in self.catalog.entries.values() {
            if e.function && a.user_fn(&e.name).is_none() {
                items.push(entry_item(e, false));
            }
        }
        for k in KEYWORDS {
            items.push(CompletionItem {
                label: (*k).into(),
                kind: Some(CompletionItemKind::KEYWORD),
                sort_text: Some(format!("3{k}")),
                ..Default::default()
            });
        }
        items
    }

    fn method_items(&self, a: &Analysis) -> Vec<CompletionItem> {
        let mut items: Vec<_> = self
            .catalog
            .entries
            .values()
            .filter(|e| e.method)
            .map(|e| entry_item(e, true))
            .collect();
        // The script's own functions that use `this` are methods too.
        for f in &a.fns {
            let uses_this =
                (f.body.0..=f.body.1).any(|i| a.tokens.get(i).is_some_and(|t| t.text == "this"));
            if uses_this {
                items.push(CompletionItem {
                    label: f.name.clone(),
                    kind: Some(CompletionItemKind::METHOD),
                    detail: Some(f.signature()),
                    insert_text: Some(call_snippet(&f.name, !f.params.is_empty())),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    sort_text: Some(format!("0{}", f.name)),
                    ..Default::default()
                });
            }
        }
        items
    }

    /// After `fn`: the prelude hooks, completed to a whole definition.
    fn hook_items(&self) -> Vec<CompletionItem> {
        self.catalog
            .entries
            .values()
            .filter(|e| e.hook)
            .map(|e| {
                let params = e.params.first().map(|p| p.join(", ")).unwrap_or_default();
                CompletionItem {
                    label: e.name.clone(),
                    kind: Some(CompletionItemKind::FUNCTION),
                    detail: Some("override the prelude hook".into()),
                    documentation: Some(Documentation::MarkupContent(markdown(e.markdown()))),
                    insert_text: Some(format!("{}({params}) {{\n\t$0\n}}", e.name)),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    ..Default::default()
                }
            })
            .collect()
    }

    fn hover(&self, a: &Analysis, pos: Position) -> Option<Hover> {
        let i = a.token_at(pos.line, pos.character)?;
        let t = &a.tokens[i];
        if t.kind != Kind::Ident {
            return None;
        }
        let value = if let Some(f) = a.user_fn(&t.text) {
            let mut s = format!("```rhai\nfn {}\n```", f.signature());
            if let Some(d) = &f.doc {
                s.push_str("\n\n");
                s.push_str(d);
            }
            s
        } else if matches!(a.roles[i], Role::Function { .. } | Role::Method { .. }) {
            self.catalog.get(&t.text)?.markdown()
        } else {
            return None;
        };
        Some(Hover {
            contents: HoverContents::Markup(markdown(value)),
            range: Some(token_range(a, i)),
        })
    }

    fn signature_help(&self, a: &Analysis, pos: Position) -> Option<SignatureHelp> {
        let (name, method, arg) = a.call_at(pos.line, pos.character)?;
        let (sigs, skip): (Vec<Overload>, usize) = if let Some(f) = a.user_fn(&name) {
            let only = Overload {
                params: f.params.clone(),
                tail: String::new(),
                doc: f.doc.clone(),
            };
            (vec![only], 0)
        } else {
            let e = self.catalog.get(&name)?;
            let skip = usize::from(method && e.origin == Origin::Core);
            let sigs = e
                .signatures
                .iter()
                .zip(&e.params)
                .map(|(sig, p)| {
                    let tail = sig.rfind(')').map_or("", |c| &sig[c + 1..]);
                    Overload {
                        params: p.clone(),
                        tail: tail.to_string(),
                        doc: e.doc.clone(),
                    }
                })
                .collect();
            (sigs, skip)
        };
        let index = arg + skip;
        let signatures: Vec<SignatureInformation> = sigs
            .into_iter()
            .map(|Overload { params, tail, doc }| {
                let mut label = format!("{name}(");
                let mut infos = Vec::new();
                for (k, p) in params.iter().enumerate() {
                    if k > 0 {
                        label.push_str(", ");
                    }
                    let start = label.encode_utf16().count() as u32;
                    label.push_str(p);
                    let end = label.encode_utf16().count() as u32;
                    infos.push(ParameterInformation {
                        label: ParameterLabel::LabelOffsets([start, end]),
                        documentation: None,
                    });
                }
                label.push(')');
                label.push_str(&tail);
                SignatureInformation {
                    label,
                    documentation: doc.map(|d| Documentation::MarkupContent(markdown(d))),
                    parameters: Some(infos),
                    active_parameter: None,
                }
            })
            .collect();
        let active = signatures
            .iter()
            .position(|s| s.parameters.as_ref().is_some_and(|p| p.len() > index))
            .unwrap_or(0);
        Some(SignatureHelp {
            signatures,
            active_signature: Some(active as u32),
            active_parameter: Some(index as u32),
        })
    }
}

/// One overload in signature help.
struct Overload {
    params: Vec<String>,
    /// What follows the `)`: ` -> Node`, or nothing.
    tail: String,
    doc: Option<String>,
}

/// `name($0)` (or `name()` for a function without arguments).
fn call_snippet(name: &str, args: bool) -> String {
    if args {
        format!("{name}($0)")
    } else {
        format!("{name}()")
    }
}

fn entry_item(e: &Entry, as_method: bool) -> CompletionItem {
    let detail = match e.signatures.len() {
        0 => None,
        1 => Some(e.signatures[0].clone()),
        n => Some(format!("{} (+{} overloads)", e.signatures[0], n - 1)),
    };
    let (kind, rank) = match (as_method, e.origin) {
        (true, _) => (CompletionItemKind::METHOD, 1),
        (false, Origin::Prelude) => (CompletionItemKind::FUNCTION, 1),
        (false, Origin::Core) => (CompletionItemKind::CONSTRUCTOR, 2),
    };
    CompletionItem {
        label: e.name.clone(),
        kind: Some(kind),
        detail,
        documentation: Some(Documentation::MarkupContent(markdown(e.markdown()))),
        insert_text: Some(call_snippet(&e.name, e.takes_args(as_method))),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        sort_text: Some(format!("{rank}{}", e.name)),
        ..Default::default()
    }
}

fn token_range(a: &Analysis, i: usize) -> Range {
    let t = &a.tokens[i];
    Range::new(
        Position::new(t.line, t.col),
        Position::new(t.line, t.end_col()),
    )
}

/// Where the function or variable under the cursor is defined in the script.
fn definition(a: &Analysis, pos: Position) -> Option<Range> {
    let i = a.token_at(pos.line, pos.character)?;
    let name = &a.tokens[i].text;
    let at = match a.roles[i] {
        Role::Function { .. } | Role::Method { .. } => a.user_fn(name)?.token,
        Role::Variable { .. } => a.vars.iter().rev().find(|(n, at)| n == name && *at <= i)?.1,
        Role::Parameter { .. } => {
            // The parameter in the innermost enclosing definition.
            let f = a
                .fns
                .iter()
                .filter(|f| f.token < i && i <= f.body.1 && f.params.contains(name))
                .min_by_key(|f| f.body.1 - f.token)?;
            (f.token..f.body.0).find(|&j| &a.tokens[j].text == name)?
        }
        _ => return None,
    };
    Some(token_range(a, at))
}

fn symbols(a: &Analysis) -> Vec<DocumentSymbol> {
    a.fns
        .iter()
        .map(|f| {
            let start = a.prev(f.token).unwrap_or(f.token);
            let range = Range::new(token_range(a, start).start, token_range(a, f.body.1).end);
            #[allow(deprecated)]
            DocumentSymbol {
                name: f.name.clone(),
                detail: Some(f.signature()),
                kind: SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                range,
                selection_range: token_range(a, f.token),
                children: None,
            }
        })
        .collect()
}

pub fn semantic_tokens(a: &Analysis) -> SemanticTokens {
    let mut data = Vec::new();
    let (mut line, mut col) = (0, 0);
    for (t, role) in a.tokens.iter().zip(&a.roles) {
        let (ty, mods) = match *role {
            Role::Keyword => (0, 0),
            Role::Function { decl, lib } => (1, u32::from(decl) | u32::from(lib) << 1),
            Role::Method { lib } => (2, u32::from(lib) << 1),
            Role::Property => (3, 0),
            Role::Variable { decl, readonly } => (4, u32::from(decl) | u32::from(readonly) << 2),
            Role::Parameter { decl } => (5, u32::from(decl)),
            Role::Str => (6, 0),
            Role::Number => (7, 0),
            Role::Comment => (8, 0),
            Role::Other => continue,
        };
        let delta_line = t.line - line;
        let delta_start = if delta_line == 0 { t.col - col } else { t.col };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: t.len,
            token_type: ty,
            token_modifiers_bitset: mods,
        });
        (line, col) = (t.line, t.col);
    }
    SemanticTokens {
        result_id: None,
        data,
    }
}

/// Evaluates the latest text of each document a moment after its last edit,
/// yielding the diagnostics to publish.
fn diagnostics_worker(
    rx: mpsc::Receiver<Job>,
) -> impl Iterator<Item = (Url, Option<i32>, Vec<Diagnostic>)> {
    let mut pending: Vec<(Url, Option<i32>, Vec<Diagnostic>)> = Vec::new();
    std::iter::from_fn(move || loop {
        if let Some(out) = pending.pop() {
            return Some(out);
        }
        let first = rx.recv().ok()?;
        let mut latest: HashMap<Url, Job> = HashMap::new();
        latest.insert(first.uri.clone(), first);
        // Debounce: wait until edits pause.
        loop {
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(job) => {
                    latest.insert(job.uri.clone(), job);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        for (uri, job) in latest {
            let diags = match &job.text {
                Some(text) => check(&uri, text),
                None => Vec::new(),
            };
            pending.push((uri, job.version, diags));
        }
    })
}

/// Evaluate a script as the CLI does, as diagnostics.
pub fn check(uri: &Url, text: &str) -> Vec<Diagnostic> {
    let base: Option<PathBuf> = uri
        .to_file_path()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        figs_core::Document::from_script_in(text, base.as_deref()).map(|_| ())
    }));
    let (message, line, column) = match result {
        Ok(Ok(())) => return Vec::new(),
        Ok(Err(figs_core::Error::Script(e))) => (e.message, e.line, e.column),
        Ok(Err(e)) => (e.to_string(), None, None),
        Err(_) => (
            "internal error while evaluating the script".to_string(),
            None,
            None,
        ),
    };
    vec![Diagnostic {
        range: error_range(text, line, column),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("figs".into()),
        message: clean_message(&message),
        ..Default::default()
    }]
}

/// The range to mark for an error at 1-based `line` / character `column`: the
/// token there, or the final `page(` call when the error has no position.
fn error_range(text: &str, line: Option<usize>, column: Option<usize>) -> Range {
    let tokens = lex(text);
    let Some(line) = line.filter(|&l| l > 0) else {
        let page = tokens
            .iter()
            .rposition(|t| t.kind == Kind::Ident && t.text == "page");
        return match page {
            Some(i) => {
                let t = &tokens[i];
                Range::new(
                    Position::new(t.line, t.col),
                    Position::new(t.line, t.end_col()),
                )
            }
            None => Range::new(Position::new(0, 0), Position::new(0, 0)),
        };
    };
    let l = (line - 1) as u32;
    let src_line = text.lines().nth(line - 1).unwrap_or("");
    let col: u32 = src_line
        .chars()
        .take(column.unwrap_or(1).saturating_sub(1))
        .map(|c| c.len_utf16() as u32)
        .sum();
    match tokens
        .iter()
        .find(|t| t.line == l && t.col <= col && col < t.end_col())
    {
        Some(t) => Range::new(Position::new(l, t.col), Position::new(l, t.end_col())),
        None => {
            let end = src_line.encode_utf16().count() as u32;
            Range::new(
                Position::new(l, col.min(end)),
                Position::new(l, end.max(col)),
            )
        }
    }
}

/// The message without the position the range already shows: rhai's
/// `line 3, col 5: …` prefix and ` (line 3, position 5)` suffix.
fn clean_message(m: &str) -> String {
    let mut m = m.trim();
    if m.starts_with("line ") {
        if let Some((_, rest)) = m.split_once(": ") {
            m = rest;
        }
    }
    if let Some(i) = m.rfind(" (line ") {
        if m.ends_with(')') {
            m = &m[..i];
        }
    }
    m.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> Url {
        Url::parse("untitled:test.figs").unwrap()
    }

    #[test]
    fn good_scripts_have_no_diagnostics() {
        let d = check(&uri(), "page(10, 10, text(\"hi\"))");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn errors_point_at_the_users_code() {
        let src = "let a = 1;\npage(10, 10, text(\"hi\").nosuch(3))";
        let d = check(&uri(), src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].range.start.line, 1, "{d:?}");
        assert!(d[0].message.contains("nosuch"), "{}", d[0].message);
        assert!(!d[0].message.contains("(line"), "{}", d[0].message);
    }

    #[test]
    fn syntax_errors_too() {
        let d = check(&uri(), "let x = ;\npage(1, 1, rect())");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].range.start.line, 0);
    }

    #[test]
    fn semantic_tokens_are_delta_encoded() {
        let a = Analysis::new("let x = 1;\n  text(x)", &Catalog::load());
        let data = semantic_tokens(&a).data;
        // let, x, 1, text, x
        assert_eq!(data.len(), 5);
        assert_eq!((data[1].delta_line, data[1].delta_start), (0, 4));
        assert_eq!(
            (data[3].delta_line, data[3].delta_start, data[3].token_type),
            (1, 2, 1)
        );
    }

    #[test]
    fn signature_help_skips_a_methods_receiver() {
        let (tx, _rx) = mpsc::channel();
        let s = Server::new(tx);
        let a = Analysis::new("text(\"a\").size(", &Catalog::load());
        let h = s.signature_help(&a, Position::new(0, 15)).unwrap();
        assert_eq!(h.active_parameter, Some(1));
        let labels: Vec<_> = h.signatures.iter().map(|s| s.label.as_str()).collect();
        assert!(labels.contains(&"size(Node, any) -> Node"), "{labels:?}");
    }
}
