//! Live-qualification plumbing shared by the provider suites (REQ-EV-0211,
//! QUAL-EV-0211: "staging integration uses real test credential and
//! recorded safe fixture; mock-only cannot pass").
//!
//! Three pieces, all real sockets:
//!
//! * the result record every live test leaves in `MODBIT_LIVE_RESULTS_DIR`
//!   — `passed` with what it proved, or `skipped` with why — so the
//!   integration gate (`tools/integration_gate.py --live`) can refuse a run
//!   whose live half skipped, which would otherwise exit 0 like a pass;
//! * a recording proxy: the adapter talks to it over plain HTTP on
//!   127.0.0.1, it forwards over TLS to the real endpoint with the
//!   adapter's own headers, streams the answer back as it arrives and keeps
//!   the exchange — the request body and the response chunks, never a
//!   credential header;
//! * a replay server that answers only the recorded requests, byte for
//!   byte, with the recorded chunks, and refuses any other body: a fixture
//!   pins what the real provider accepted, not what a fake would.

#![allow(dead_code)] // each test crate uses part of this module

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use modbit_providers::{Endpoint, ProviderKind};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Where live tests leave their result records.
pub const RESULTS_DIR: &str = "MODBIT_LIVE_RESULTS_DIR";
/// Where the recording test writes the exchanges it captured.
pub const RECORD_DIR: &str = "MODBIT_LIVE_RECORD_DIR";
/// Schema of a recorded fixture.
pub const SCHEMA: &str = "modbit.recorded-exchange/1";

/// Request headers a recording keeps: what shapes the request, never who
/// sent it. Everything else — `authorization`, `x-api-key`, cookies, our
/// own per-request id — stays out of the file.
const KEPT_REQUEST_HEADERS: &[&str] = &["accept", "anthropic-version", "content-type"];
/// Response headers a recording keeps: the type and the provider's own
/// request ids, which the gateway reads into its route record.
const KEPT_RESPONSE_HEADERS: &[&str] = &["content-type", "x-request-id", "request-id", "cf-ray"];
/// Header names a safe fixture must never carry.
pub const CREDENTIAL_HEADERS: &[&str] = &[
    "authorization",
    "x-api-key",
    "api-key",
    "cookie",
    "set-cookie",
    "proxy-authorization",
];

/// The integration id a provider family qualifies (`tools/integrations.json`).
#[must_use]
pub fn integration_of(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAi => "provider.openai-wire",
        ProviderKind::Anthropic => "provider.anthropic-wire",
    }
}

/// The fixture file name a provider family's recording goes to.
#[must_use]
pub fn fixture_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAi => "openai-wire.json",
        ProviderKind::Anthropic => "anthropic-wire.json",
    }
}

/// The committed recordings.
#[must_use]
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recorded")
}

/// `(host, path)` of an endpoint base URL.
#[must_use]
pub fn host_and_path(base_url: &str) -> (String, String) {
    let url = reqwest::Url::parse(base_url).expect("a base URL");
    (
        url.host_str().unwrap_or_default().to_owned(),
        url.path().trim_end_matches('/').to_owned(),
    )
}

/// The live model for an endpoint: `MODBIT_<P>_LIVE_MODEL`, else
/// `MODBIT_LIVE_MODEL`, else the catalog's small model. A configured model
/// the catalog does not list is refused, never guessed around.
#[must_use]
pub fn live_model(ep: &Endpoint) -> String {
    // CI passes every variable, so an unset per-family one arrives empty:
    // empty means absent and falls back to the shared one.
    let var = |name: String| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let configured = var(format!(
        "MODBIT_{}_LIVE_MODEL",
        ep.name.to_ascii_uppercase()
    ))
    .or_else(|| var("MODBIT_LIVE_MODEL".into()));
    match configured {
        Some(m) => {
            assert!(
                ep.models.iter().any(|c| c.model == m.trim()),
                "{}: live model {m:?} is not in its catalog {:?} (set MODBIT_{}_MODELS)",
                ep.name,
                ep.models
                    .iter()
                    .map(|c| c.model.as_str())
                    .collect::<Vec<_>>(),
                ep.name.to_ascii_uppercase()
            );
            m.trim().to_owned()
        }
        None => ep
            .models
            .iter()
            .find(|m| m.model.contains("mini") || m.model.contains("haiku"))
            .map(|m| m.model.clone())
            .unwrap_or_else(|| {
                panic!(
                    "{}: no small model in the catalog and no MODBIT_LIVE_MODEL",
                    ep.name
                )
            }),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// The CI run this process belongs to, or `local`.
#[must_use]
pub fn run_id() -> String {
    std::env::var("GITHUB_RUN_ID")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "local".into())
}

/// Leave this live test's result record, when a results directory is named.
/// `passed` is written only after every assertion held, so a panic leaves
/// no record at all — which the gate reads as "did not run".
pub fn write_result(test: &str, outcome: &str, detail: Value) {
    let Some(dir) = std::env::var_os(RESULTS_DIR).filter(|d| !d.is_empty()) else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("results dir");
    let mut record = json!({
        "schema": "modbit.live-result/1",
        "test": test,
        "outcome": outcome,
        "run": run_id(),
        "finished_unix": unix_now(),
    });
    if let (Some(r), Some(d)) = (record.as_object_mut(), detail.as_object()) {
        for (k, v) in d {
            r.insert(k.clone(), v.clone());
        }
    }
    std::fs::write(
        dir.join(format!("{test}.json")),
        serde_json::to_vec_pretty(&record).expect("json"),
    )
    .expect("write result record");
}

/// Hex sha256.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// One request read off a socket.
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Read one HTTP/1.1 request (head, then a `content-length` body).
pub async fn read_request(sock: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let (head_end, body_len) = loop {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..i]).to_string();
            let len = head
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            break (i + 4, len);
        }
    };
    while buf.len() < head_end + body_len {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next().unwrap_or_default().split(' ');
    let method = first.next().unwrap_or_default().to_owned();
    let path = first.next().unwrap_or_default().to_owned();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let end = (head_end + body_len).min(buf.len());
    Some(Request {
        method,
        path,
        headers,
        body: buf[head_end..end].to_vec(),
    })
}

async fn write_chunk(sock: &mut TcpStream, bytes: &[u8]) -> bool {
    let mut frame = format!("{:x}\r\n", bytes.len()).into_bytes();
    frame.extend_from_slice(bytes);
    frame.extend_from_slice(b"\r\n");
    sock.write_all(&frame).await.is_ok()
}

async fn write_head(sock: &mut TcpStream, status: u16, headers: &[(String, String)]) -> bool {
    let mut head = format!("HTTP/1.1 {status} X\r\n");
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("transfer-encoding: chunked\r\nconnection: close\r\n\r\n");
    sock.write_all(head.as_bytes()).await.is_ok()
}

/// Splits received bytes into UTF-8 text chunks: a chunk that ends inside a
/// character keeps that character's first bytes back for the next one, so
/// every recorded chunk is readable text and the boundaries stay where the
/// network put them, give or take one character.
#[derive(Default)]
struct Utf8Chunker {
    pending: Vec<u8>,
}

impl Utf8Chunker {
    fn push(&mut self, bytes: &[u8]) -> Option<String> {
        self.pending.extend_from_slice(bytes);
        let valid = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => self.pending.len(),
        };
        if valid == 0 {
            return None;
        }
        let rest = self.pending.split_off(valid);
        let text =
            String::from_utf8_lossy(&std::mem::replace(&mut self.pending, rest)).into_owned();
        Some(text)
    }

    fn finish(&mut self) -> Option<String> {
        (!self.pending.is_empty())
            .then(|| String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned())
    }
}

/// A proxy that records every exchange it forwards.
pub struct RecordingProxy {
    /// Base URL to give the adapter: this proxy plus the upstream base path.
    pub base_url: String,
    /// Exchanges in the order they completed.
    pub exchanges: Arc<Mutex<Vec<Value>>>,
}

/// Start a recording proxy in front of `upstream_base` (an endpoint base
/// URL such as `https://api.z.ai/api/paas/v4`).
pub async fn recording_proxy(upstream_base: &str) -> RecordingProxy {
    let url = reqwest::Url::parse(upstream_base).expect("upstream base URL");
    let origin = url.origin().ascii_serialization();
    let base_path = url.path().trim_end_matches('/').to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let exchanges = Arc::new(Mutex::new(Vec::new()));
    let client = reqwest::Client::builder().build().expect("client");
    let kept = Arc::clone(&exchanges);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let (client, origin, kept) = (client.clone(), origin.clone(), Arc::clone(&kept));
            tokio::spawn(async move {
                let Some(req) = read_request(&mut sock).await else {
                    return;
                };
                let started = Instant::now();
                let method = reqwest::Method::from_bytes(req.method.as_bytes())
                    .unwrap_or(reqwest::Method::POST);
                let mut rb = client.request(method, format!("{origin}{}", req.path));
                for (k, v) in &req.headers {
                    if !matches!(
                        k.as_str(),
                        "host"
                            | "content-length"
                            | "connection"
                            | "transfer-encoding"
                            | "accept-encoding"
                    ) {
                        rb = rb.header(k.as_str(), v.as_str());
                    }
                }
                let request = json!({
                    "method": req.method,
                    "path": req.path,
                    "headers": req.headers.iter()
                        .filter(|(k, _)| KEPT_REQUEST_HEADERS.contains(&k.as_str()))
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect::<serde_json::Map<_, _>>(),
                    "body": serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null),
                });
                let resp = match rb.body(req.body).send().await {
                    Ok(r) => r,
                    Err(e) => {
                        // The adapter sees a 502 and reports it; nothing is
                        // recorded, because nothing was answered.
                        let msg = modbit_secrets::error_text(&e.to_string());
                        let _ = write_head(
                            &mut sock,
                            502,
                            &[("content-type".into(), "text/plain".into())],
                        )
                        .await;
                        let _ = write_chunk(&mut sock, msg.as_bytes()).await;
                        let _ = sock.write_all(b"0\r\n\r\n").await;
                        return;
                    }
                };
                let status = resp.status().as_u16();
                let headers: Vec<(String, String)> = resp
                    .headers()
                    .iter()
                    .filter(|(k, _)| KEPT_RESPONSE_HEADERS.contains(&k.as_str()))
                    .filter_map(|(k, v)| Some((k.as_str().to_owned(), v.to_str().ok()?.to_owned())))
                    .collect();
                let mut chunks = Vec::new();
                let mut chunker = Utf8Chunker::default();
                let mut body = resp.bytes_stream();
                let mut client_open = write_head(&mut sock, status, &headers).await;
                while let Some(next) = body.next().await {
                    let Ok(bytes) = next else { break };
                    if client_open {
                        client_open = write_chunk(&mut sock, &bytes).await;
                    }
                    if let Some(text) = chunker.push(&bytes) {
                        chunks.push(
                            json!({"at_ms": started.elapsed().as_millis() as u64, "text": text}),
                        );
                    }
                    if !client_open {
                        // The adapter hung up (a cancellation): the recording
                        // ends where the adapter stopped listening.
                        break;
                    }
                }
                if let Some(text) = chunker.finish() {
                    chunks
                        .push(json!({"at_ms": started.elapsed().as_millis() as u64, "text": text}));
                }
                if client_open {
                    let _ = sock.write_all(b"0\r\n\r\n").await;
                }
                kept.lock().expect("exchanges").push(json!({
                    "request": request,
                    "response": {
                        "status": status,
                        "headers": headers.into_iter()
                            .map(|(k, v)| (k, Value::String(v)))
                            .collect::<serde_json::Map<_, _>>(),
                        "chunks": chunks,
                    },
                }));
            });
        }
    });
    RecordingProxy {
        base_url: format!("http://127.0.0.1:{port}{base_path}"),
        exchanges,
    }
}

/// The first place two JSON values differ, as a JSON pointer with both sides.
#[must_use]
pub fn first_difference(recorded: &Value, sent: &Value, at: &str) -> Option<String> {
    match (recorded, sent) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().find_map(|k| {
                let path = format!("{at}/{k}");
                match (a.get(k), b.get(k)) {
                    (Some(x), Some(y)) => first_difference(x, y, &path),
                    (x, y) => Some(format!("{path}: recorded {x:?}, sent {y:?}")),
                }
            })
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!(
                    "{at}: recorded {} items, sent {}",
                    a.len(),
                    b.len()
                ));
            }
            a.iter()
                .zip(b)
                .enumerate()
                .find_map(|(i, (x, y))| first_difference(x, y, &format!("{at}/{i}")))
        }
        (a, b) if a == b => None,
        (a, b) => Some(format!("{at}: recorded {a}, sent {b}")),
    }
}

/// Answer 409 with the reason as a provider-shaped error body.
async fn refuse(sock: &mut TcpStream, why: &str) {
    let body = json!({"error": {"message": why}}).to_string();
    let _ = write_head(
        sock,
        409,
        &[("content-type".into(), "application/json".into())],
    )
    .await;
    let _ = write_chunk(sock, body.as_bytes()).await;
    let _ = sock.write_all(b"0\r\n\r\n").await;
}

/// A server that answers only the recorded requests.
pub struct ReplayServer {
    /// Base URL to give the adapter (the recorded base path included).
    pub base_url: String,
    /// Requests that matched the recording and were answered from it.
    pub served: Arc<Mutex<usize>>,
    /// Requests refused, each with the first difference from the recording.
    pub refused: Arc<Mutex<Vec<String>>>,
}

/// Serve `fixture`'s exchanges in order. A request whose method, path or
/// body differs from the recorded one is answered 409 and never replayed.
pub async fn replay_server(fixture: &Value) -> ReplayServer {
    let exchanges = fixture["exchanges"].as_array().cloned().unwrap_or_default();
    let base_path = fixture["provenance"]["base_path"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let served = Arc::new(Mutex::new(0usize));
    let refused = Arc::new(Mutex::new(Vec::new()));
    let (s2, r2) = (Arc::clone(&served), Arc::clone(&refused));
    let next = Arc::new(Mutex::new(0usize));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let (exchanges, served, refused, next) = (
                exchanges.clone(),
                Arc::clone(&s2),
                Arc::clone(&r2),
                Arc::clone(&next),
            );
            tokio::spawn(async move {
                let Some(req) = read_request(&mut sock).await else {
                    return;
                };
                let i = *next.lock().expect("next");
                let Some(ex) = exchanges.get(i) else {
                    let why = format!("request {i} was never recorded");
                    refused.lock().expect("refused").push(why.clone());
                    refuse(&mut sock, &why).await;
                    return;
                };
                let sent = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
                let recorded = &ex["request"];
                let diff = if recorded["method"] != req.method.as_str() {
                    Some(format!(
                        "method: recorded {}, sent {}",
                        recorded["method"], req.method
                    ))
                } else if recorded["path"] != req.path.as_str() {
                    Some(format!(
                        "path: recorded {}, sent {}",
                        recorded["path"], req.path
                    ))
                } else {
                    first_difference(&recorded["body"], &sent, "/body")
                };
                if let Some(d) = diff {
                    let why = format!("request {i} differs from the recording at {d}");
                    refused.lock().expect("refused").push(why.clone());
                    refuse(&mut sock, &why).await;
                    return;
                }
                // Counted when it is accepted: the adapter is done at the
                // stream's last event, before this task writes the terminator.
                *next.lock().expect("next") += 1;
                *served.lock().expect("served") += 1;
                let status = ex["response"]["status"].as_u64().unwrap_or(200) as u16;
                let headers: Vec<(String, String)> = ex["response"]["headers"]
                    .as_object()
                    .map(|h| {
                        h.iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                            .collect()
                    })
                    .unwrap_or_default();
                if !write_head(&mut sock, status, &headers).await {
                    return;
                }
                for chunk in ex["response"]["chunks"].as_array().into_iter().flatten() {
                    let text = chunk["text"].as_str().unwrap_or_default();
                    if !write_chunk(&mut sock, text.as_bytes()).await {
                        return;
                    }
                    let _ = sock.flush().await;
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
                let _ = sock.write_all(b"0\r\n\r\n").await;
            });
        }
    });
    ReplayServer {
        base_url: format!("http://127.0.0.1:{port}{base_path}"),
        served,
        refused,
    }
}

/// Whether a recording is safe to commit: no credential header anywhere, no
/// value the Core holds, and no credential-shaped token as the product's
/// one redactor (`modbit_secrets`) knows them. Returns every finding.
#[must_use]
pub fn safety_findings(fixture: &Value, held: &[String]) -> Vec<String> {
    let mut findings = Vec::new();
    for (i, ex) in fixture["exchanges"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        for side in ["request", "response"] {
            for name in ex[side]["headers"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, _)| k)
            {
                if CREDENTIAL_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
                    findings.push(format!(
                        "exchange {i}: {side} header `{name}` is a credential header"
                    ));
                }
            }
        }
    }
    let mut copy = fixture.clone();
    let replaced = modbit_secrets::Redactor::new(held.iter().cloned()).error_json(&mut copy);
    if replaced > 0 {
        findings.push(format!(
            "{replaced} held value(s) or credential-shaped token(s) the redactor would replace"
        ));
    }
    findings
}
