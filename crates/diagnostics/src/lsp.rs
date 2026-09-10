//! Minimal LSP client over stdio: Content-Length framing, request/response
//! matching, notification capture (`publishDiagnostics`), and the handful of
//! methods the bridge needs. Synchronous by design: one server process per
//! workspace and language, driven from a blocking context.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Errors of the bridge.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    /// No server for the language on this host.
    #[error("no language server for `{language}`: {detail}")]
    Unavailable {
        /// Language label.
        language: String,
        /// Why.
        detail: String,
    },
    /// Spawn or I/O failure.
    #[error("language server io: {0}")]
    Io(#[from] std::io::Error),
    /// The server did not answer in time.
    #[error("language server timeout after {0:?}")]
    Timeout(Duration),
    /// The server returned an error or malformed data.
    #[error("language server protocol: {0}")]
    Protocol(String),
    /// The server exited.
    #[error("language server exited")]
    Exited,
}

/// Zero-based position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// Line.
    pub line: u32,
    /// UTF-16 character offset (LSP native).
    pub character: u32,
}

/// Range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    /// Start.
    pub start: Position,
    /// End (exclusive).
    pub end: Position,
}

/// Normalized diagnostic (docs/18 "diagnostics"; docs/76 Tier A parity).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Root-relative path.
    pub path: String,
    /// Range.
    pub range: Range,
    /// `error` | `warning` | `information` | `hint`.
    pub severity: String,
    /// Server code, if any.
    pub code: Option<String>,
    /// Message.
    pub message: String,
    /// Server name.
    pub source: String,
}

/// Normalized symbol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspSymbol {
    /// Name.
    pub name: String,
    /// LSP SymbolKind label.
    pub kind: String,
    /// Root-relative path.
    pub path: String,
    /// Full range.
    pub range: Range,
    /// The name's range (where references/definition requests should point).
    pub selection: Range,
    /// Enclosing symbol, if any.
    pub container: Option<String>,
}

/// Normalized location.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// Root-relative path (or an absolute path outside the root).
    pub path: String,
    /// Range.
    pub range: Range,
}

type DiagnosticStore = Arc<Mutex<HashMap<String, Vec<Value>>>>;

/// One running server.
pub struct LanguageServer {
    child: Child,
    stdin: std::process::ChildStdin,
    incoming: Receiver<Value>,
    next_id: u64,
    root: PathBuf,
    name: String,
    versions: HashMap<String, i32>,
    diagnostics: DiagnosticStore,
    capabilities: Value,
}

impl std::fmt::Debug for LanguageServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageServer")
            .field("name", &self.name)
            .field("root", &self.root)
            .finish()
    }
}

/// `file://` URI of a root-relative path.
#[must_use]
pub fn uri_of(root: &Path, rel: &str) -> String {
    let abs = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let s = abs.to_string_lossy().replace('\\', "/");
    let s = s.trim_start_matches("//?/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

/// Root-relative path of a `file://` URI (absolute when outside the root).
#[must_use]
pub fn path_of(root: &Path, uri: &str) -> String {
    let raw = uri.strip_prefix("file://").unwrap_or(uri);
    let mut decoded = percent_decode(raw);
    if cfg!(windows) && decoded.starts_with('/') && decoded.as_bytes().get(2) == Some(&b':') {
        decoded.remove(0);
    }
    let root_s = root.to_string_lossy().replace('\\', "/");
    let root_s = root_s.trim_start_matches("//?/").trim_end_matches('/');
    let d = decoded.replace('\\', "/");
    let (a, b) = if cfg!(windows) {
        (d.to_ascii_lowercase(), root_s.to_ascii_lowercase())
    } else {
        (d.clone(), root_s.to_owned())
    };
    match a.strip_prefix(&format!("{b}/")) {
        Some(rel) => d[d.len() - rel.len()..].to_owned(),
        None => d,
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len() + 1
            && i + 3 <= b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn range_of(v: &Value) -> Range {
    let p = |x: &Value| Position {
        line: x["line"].as_u64().unwrap_or(0) as u32,
        character: x["character"].as_u64().unwrap_or(0) as u32,
    };
    Range {
        start: p(&v["start"]),
        end: p(&v["end"]),
    }
}

fn symbol_kind(k: u64) -> &'static str {
    match k {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum_member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type_parameter",
        _ => "unknown",
    }
}

fn reader_loop(
    stdout: std::process::ChildStdout,
    tx: std::sync::mpsc::Sender<Value>,
    diag: DiagnosticStore,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut len: Option<usize> = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let t = line.trim_end();
            if t.is_empty() {
                break;
            }
            if let Some(v) = t.strip_prefix("Content-Length:") {
                len = v.trim().parse().ok();
            }
        }
        let Some(n) = len else { continue };
        let mut buf = vec![0u8; n];
        if reader.read_exact(&mut buf).is_err() {
            return;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        if v["method"] == "textDocument/publishDiagnostics"
            && let Some(uri) = v["params"]["uri"].as_str()
        {
            let list = v["params"]["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            diag.lock()
                .expect("diagnostics")
                .insert(uri.to_owned(), list);
        }
        if tx.send(v).is_err() {
            return;
        }
    }
}

impl LanguageServer {
    /// Spawn `command args` in `root` and run the initialize handshake.
    pub fn spawn(
        name: &str,
        command: &Path,
        args: &[String],
        root: &Path,
        initialization_options: Value,
        timeout: Duration,
    ) -> Result<Self, LspError> {
        let mut child = Command::new(crate::servers::plain(command))
            .args(args)
            .current_dir(crate::servers::plain(root))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = channel::<Value>();
        let diagnostics: DiagnosticStore = Arc::new(Mutex::new(HashMap::new()));
        let sink = Arc::clone(&diagnostics);
        std::thread::spawn(move || reader_loop(stdout, tx, sink));
        let mut server = Self {
            child,
            stdin,
            incoming: rx,
            next_id: 1,
            root: root.to_path_buf(),
            name: name.to_owned(),
            versions: HashMap::new(),
            diagnostics,
            capabilities: Value::Null,
        };
        let root_uri = uri_of(root, "");
        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "workspaceFolders": [{"uri": root_uri, "name": "workspace"}],
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {"relatedInformation": false},
                    "diagnostic": {"dynamicRegistration": false, "relatedDocumentSupport": false},
                    "documentSymbol": {"hierarchicalDocumentSymbolSupport": true},
                    "definition": {"linkSupport": true}
                },
                "window": {"workDoneProgress": true},
                "workspace": {"workspaceFolders": true, "configuration": true}
            },
            "initializationOptions": initialization_options,
        });
        let init = server.request("initialize", params, timeout)?;
        server.capabilities = init["capabilities"].clone();
        server.notify("initialized", json!({}))?;
        // pyright starts analysing only after a configuration change notice.
        server.notify("workspace/didChangeConfiguration", json!({"settings": {}}))?;
        Ok(server)
    }

    /// Server name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Server capabilities from `initialize`.
    #[must_use]
    pub fn capabilities(&self) -> &Value {
        &self.capabilities
    }

    /// Whether the server offers pull diagnostics (`textDocument/diagnostic`).
    #[must_use]
    pub fn supports_pull_diagnostics(&self) -> bool {
        !self.capabilities["diagnosticProvider"].is_null()
    }

    /// Pull diagnostics for `rel` (LSP 3.17); the raw items.
    pub fn pull_diagnostics(
        &mut self,
        rel: &str,
        timeout: Duration,
    ) -> Result<Vec<Value>, LspError> {
        let uri = uri_of(&self.root, rel);
        let r = self.request(
            "textDocument/diagnostic",
            json!({"textDocument": {"uri": uri}}),
            timeout,
        )?;
        Ok(r["items"].as_array().cloned().unwrap_or_default())
    }

    fn send(&mut self, v: &Value) -> Result<(), LspError> {
        let body = serde_json::to_vec(v).map_err(|e| LspError::Protocol(e.to_string()))?;
        self.stdin
            .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())?;
        self.stdin.write_all(&body)?;
        self.stdin.flush()?;
        Ok(())
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), LspError> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    /// Answer server-initiated requests so the server never stalls on us:
    /// configuration requests get one empty settings object per item, the
    /// rest an empty result.
    fn answer_if_request(&mut self, v: &Value) {
        if v.get("id").is_some() && v.get("method").is_some() {
            let result = match v["method"].as_str() {
                Some("workspace/configuration") => {
                    let n = v["params"]["items"].as_array().map_or(1, Vec::len);
                    Value::Array(vec![json!({}); n])
                }
                Some("workspace/workspaceFolders") => {
                    json!([{"uri": uri_of(&self.root, ""), "name": "workspace"}])
                }
                _ => Value::Null,
            };
            let reply = json!({"jsonrpc": "2.0", "id": v["id"], "result": result});
            let _ = self.send(&reply);
        }
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LspError> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(LspError::Timeout(timeout));
            }
            match self.incoming.recv_timeout(deadline - now) {
                Ok(v) => {
                    if v["id"] == json!(id) && v.get("method").is_none() {
                        if let Some(e) = v.get("error") {
                            return Err(LspError::Protocol(e.to_string()));
                        }
                        return Ok(v["result"].clone());
                    }
                    self.answer_if_request(&v);
                }
                Err(RecvTimeoutError::Timeout) => return Err(LspError::Timeout(timeout)),
                Err(RecvTimeoutError::Disconnected) => return Err(LspError::Exited),
            }
        }
    }

    /// Open (or fully re-sync) a document from its current text.
    pub fn open(&mut self, rel: &str, language_id: &str, text: &str) -> Result<(), LspError> {
        let uri = uri_of(&self.root, rel);
        let version = self.versions.entry(rel.to_owned()).or_insert(0);
        *version += 1;
        let version = *version;
        if version == 1 {
            self.notify(
                "textDocument/didOpen",
                json!({"textDocument": {"uri": uri, "languageId": language_id, "version": version, "text": text}}),
            )
        } else {
            self.notify(
                "textDocument/didChange",
                json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]}),
            )
        }
    }

    /// Diagnostics for `rel` after an `open`. Servers publish once their
    /// analysis settles (rust-analyzer first publishes an empty list while
    /// indexing, pyright publishes after its scan), so the wait tracks the
    /// server's `$/progress` work: it ends once a publish for the file has
    /// arrived, no progress token is active and the stream has been quiet.
    pub fn diagnostics(
        &mut self,
        rel: &str,
        timeout: Duration,
    ) -> Result<Vec<Diagnostic>, LspError> {
        let uri = uri_of(&self.root, rel);
        self.diagnostics.lock().expect("d").remove(&uri);
        let started = Instant::now();
        let deadline = started + timeout;
        let quiet = Duration::from_millis(1500);
        let pull = self.supports_pull_diagnostics();
        let mut last_event = Instant::now();
        let mut published = false;
        let mut active_progress = 0usize;
        // Settle: a publish for the file (or a pull-capable server), no active
        // progress token, a quiet stream, and at least two seconds of grace.
        let settled = |published: bool, active: usize, last: Instant| {
            (published || pull)
                && active == 0
                && Instant::now().duration_since(last) >= quiet
                && Instant::now().duration_since(started) >= Duration::from_secs(2)
        };
        loop {
            let now = Instant::now();
            if now >= deadline {
                if published || pull {
                    break;
                }
                return Err(LspError::Timeout(timeout));
            }
            if settled(published, active_progress, last_event) {
                break;
            }
            let wait = if (published || pull) && active_progress == 0 {
                quiet
                    .saturating_sub(now.duration_since(last_event))
                    .max(Duration::from_millis(50))
            } else {
                (deadline - now).min(Duration::from_millis(500))
            };
            match self.incoming.recv_timeout(wait) {
                Ok(v) => {
                    self.answer_if_request(&v);
                    match v["method"].as_str() {
                        Some("textDocument/publishDiagnostics") if v["params"]["uri"] == uri => {
                            published = true;
                            last_event = Instant::now();
                        }
                        Some("$/progress") => {
                            match v["params"]["value"]["kind"].as_str() {
                                Some("begin") => active_progress += 1,
                                Some("end") => active_progress = active_progress.saturating_sub(1),
                                _ => {}
                            }
                            last_event = Instant::now();
                        }
                        _ => {}
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(LspError::Exited),
            }
        }
        // Servers offering pull diagnostics answer with the settled analysis;
        // the pushed list is the fallback.
        let list = if pull {
            let left = deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_secs(5));
            self.pull_diagnostics(rel, left)?
        } else {
            self.diagnostics
                .lock()
                .expect("d")
                .get(&uri)
                .cloned()
                .unwrap_or_default()
        };
        Ok(list
            .iter()
            .map(|d| Diagnostic {
                path: rel.to_owned(),
                range: range_of(&d["range"]),
                severity: match d["severity"].as_u64() {
                    Some(1) => "error",
                    Some(2) => "warning",
                    Some(3) => "information",
                    Some(4) => "hint",
                    _ => "unknown",
                }
                .into(),
                code: d["code"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| d["code"].as_u64().map(|c| c.to_string())),
                message: d["message"].as_str().unwrap_or_default().to_owned(),
                source: d["source"].as_str().unwrap_or(&self.name).to_owned(),
            })
            .collect())
    }

    /// Document symbols (hierarchical or flat).
    pub fn document_symbols(
        &mut self,
        rel: &str,
        timeout: Duration,
    ) -> Result<Vec<LspSymbol>, LspError> {
        let uri = uri_of(&self.root, rel);
        let result = self.request(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": uri}}),
            timeout,
        )?;
        fn walk(
            items: &[Value],
            container: Option<&str>,
            rel: &str,
            root: &Path,
            out: &mut Vec<LspSymbol>,
        ) {
            for it in items {
                let name = it["name"].as_str().unwrap_or_default().to_owned();
                let kind = symbol_kind(it["kind"].as_u64().unwrap_or(0)).to_owned();
                if it.get("location").is_some() {
                    let range = range_of(&it["location"]["range"]);
                    out.push(LspSymbol {
                        name,
                        kind,
                        path: path_of(root, it["location"]["uri"].as_str().unwrap_or_default()),
                        range,
                        selection: range,
                        container: it["containerName"].as_str().map(str::to_owned),
                    });
                } else {
                    out.push(LspSymbol {
                        name: name.clone(),
                        kind,
                        path: rel.to_owned(),
                        range: range_of(&it["range"]),
                        selection: range_of(&it["selectionRange"]),
                        container: container.map(str::to_owned),
                    });
                    if let Some(children) = it["children"].as_array() {
                        walk(children, Some(&name), rel, root, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        if let Some(items) = result.as_array() {
            walk(items, None, rel, &self.root, &mut out);
        }
        Ok(out)
    }

    fn locations(&self, result: &Value) -> Vec<Location> {
        let items: Vec<Value> = match result {
            Value::Array(a) => a.clone(),
            Value::Object(_) => vec![result.clone()],
            _ => vec![],
        };
        items
            .iter()
            .map(|l| {
                if l.get("targetUri").is_some() {
                    Location {
                        path: path_of(&self.root, l["targetUri"].as_str().unwrap_or_default()),
                        range: range_of(&l["targetSelectionRange"]),
                    }
                } else {
                    Location {
                        path: path_of(&self.root, l["uri"].as_str().unwrap_or_default()),
                        range: range_of(&l["range"]),
                    }
                }
            })
            .collect()
    }

    /// References of the symbol at a position (declaration included), sorted.
    pub fn references(
        &mut self,
        rel: &str,
        pos: Position,
        timeout: Duration,
    ) -> Result<Vec<Location>, LspError> {
        let uri = uri_of(&self.root, rel);
        let r = self.request(
            "textDocument/references",
            json!({"textDocument": {"uri": uri}, "position": {"line": pos.line, "character": pos.character}, "context": {"includeDeclaration": true}}),
            timeout,
        )?;
        let mut out = self.locations(&r);
        out.sort_by(|a, b| {
            (a.path.as_str(), a.range.start.line, a.range.start.character).cmp(&(
                b.path.as_str(),
                b.range.start.line,
                b.range.start.character,
            ))
        });
        Ok(out)
    }

    /// Definition of the symbol at a position.
    pub fn definition(
        &mut self,
        rel: &str,
        pos: Position,
        timeout: Duration,
    ) -> Result<Vec<Location>, LspError> {
        let uri = uri_of(&self.root, rel);
        let r = self.request(
            "textDocument/definition",
            json!({"textDocument": {"uri": uri}, "position": {"line": pos.line, "character": pos.character}}),
            timeout,
        )?;
        Ok(self.locations(&r))
    }

    /// Drain incoming messages for `dur` (diagnosability): method names and a
    /// bounded rendering of each; server requests are answered on the way.
    pub fn drain(&mut self, dur: Duration) -> Vec<String> {
        let deadline = Instant::now() + dur;
        let mut out = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now())
            && !left.is_zero()
        {
            match self.incoming.recv_timeout(left) {
                Ok(v) => {
                    self.answer_if_request(&v);
                    let s = v.to_string();
                    out.push(s.chars().take(300).collect());
                }
                Err(_) => break,
            }
        }
        out
    }

    /// Graceful shutdown; the process is killed on drop regardless.
    pub fn shutdown(&mut self) {
        let _ = self.request("shutdown", Value::Null, Duration::from_secs(3));
        let _ = self.notify("exit", Value::Null);
    }
}

impl Drop for LanguageServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
