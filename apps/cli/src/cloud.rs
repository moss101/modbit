//! `modbit-cli cloud ...` (PX-128, PX-129; docs/24, docs/30): the headless
//! client of the Cloud API — sign in, hand a local task to the cloud, watch
//! it, answer its approvals, and publish or check the organisation's signed
//! policy bundle. The CLI runs nothing of the task and holds no policy: a
//! handoff is the Core's own export (`ExportHandoff`, so the Core's policy and
//! its secret scan apply before a byte is written), uploaded as the
//! `EnvironmentHandoffBundle` the API admits; every other verb is one HTTP
//! request. The API's token pair is kept in `<data-dir>/cloud/session.json`
//! (readable by its owner only), is never printed and is never logged.
//!
//! ```text
//! cloud login   --api <url> [--oidc [--no-browser] | --secret-stdin]   (the secret also from MODBIT_CLOUD_SECRET)
//! cloud logout
//! cloud status
//! cloud handoff --session <id> --task <id> [--confirm <manifest-hash-prefix>]
//! cloud list
//! cloud watch   --session <id> [--after N] [--follow] [--until <EventType>[,..]] [--timeout-secs N] [--json]
//! cloud approvals --session <id>
//! cloud approve --approval <id> --intent <hash> [--deny] [reason]
//! cloud policy keygen --out <file>
//! cloud policy sign   --key <file> --key-id <id> --tenant <id> --generation N --config <file> [--ttl-secs N] --out <file>
//! cloud policy publish --file <signed.json>
//! cloud policy fetch  --trust <key-id>=<public-key-hex>... [--write-admin-config <file>]
//! ```
//!
//! A handoff is two steps by design: the first invocation exports the bundle
//! to `<data-dir>/cloud/bundles/<task>/`, prints exactly what would leave the
//! machine and exits (code 2) without sending a byte; the second, naming the
//! manifest hash it printed (`--confirm`), uploads that bundle. A policy that
//! forbids handoff, or a secret in the workspace, is refused by the Core
//! before anything is written.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine;
use modbit_domain::policy_bundle::{self, BundleDocument, Expect, SignedBundle};
use modbit_protocol::client::Client;
use modbit_protocol::local::decode_hex;
use modbit_protocol::v1::{ClientKind, ExportHandoff, HandoffExported};
use prost::Message;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{attach_or_spawn, envelope_fenced, join_lease, parse_id};

/// The usage of the cloud verbs.
pub(crate) const CLOUD_USAGE: &str = "usage: modbit-cli --data-dir <dir> cloud (login --api <url> [--oidc [--no-browser] | --secret-stdin] | logout | status | handoff --session <id> --task <id> [--confirm <manifest-hash-prefix>] | list | watch --session <id> [--after N] [--follow] [--until <EventType>[,..]] [--timeout-secs N] [--json] | approvals --session <id> | approve --approval <id> --intent <hash> [--deny] [reason] | policy (keygen --out <file> | sign --key <file> --key-id <id> --tenant <id> --generation N --config <file> [--ttl-secs N] --out <file> | publish --file <signed.json> | fetch --trust <key-id>=<public-key-hex>... [--write-admin-config <file>]))";

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn cloud_dir(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("cloud")
}

/// Write a file only its owner can read (a token or a signing key).
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        f.write_all(bytes)
            .map_err(|e| format!("{}: {e}", path.display()))
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// A UUID's text from its 16 canonical bytes (how the API names an aggregate).
fn uuid_text(bytes: &[u8]) -> String {
    let h = hex::encode(bytes);
    if h.len() != 32 {
        return h;
    }
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

fn id_array_text(v: &Value) -> String {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_u64().and_then(|n| u8::try_from(n).ok()))
                .collect::<Vec<u8>>()
        })
        .map(|b| uuid_text(&b))
        .unwrap_or_default()
}

/// A request's outcome as one line: `CODE: message (HTTP status)`.
fn refusal(status: u16, body: &Value) -> String {
    format!(
        "{}: {} (HTTP {status})",
        body["code"].as_str().unwrap_or("ERROR"),
        body["message"]
            .as_str()
            .unwrap_or("the Cloud API refused the request")
    )
}

/// The API the CLI is signed in to.
struct Api {
    http: reqwest::Client,
    base: String,
    file: PathBuf,
    saved: Value,
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}

/// Tokens go to `https`, or to a loopback host (a development API).
fn check_base(base: &str) -> Result<String, String> {
    let base = base.trim().trim_end_matches('/').to_owned();
    let url = reqwest::Url::parse(&base).map_err(|e| format!("--api is not a URL: {e}"))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));
    match url.scheme() {
        "https" => Ok(base),
        "http" if loopback || std::env::var("MODBIT_CLOUD_ALLOW_HTTP").as_deref() == Ok("1") => {
            Ok(base)
        }
        "http" => Err(
            "the Cloud API is reached over https (a plain http URL only on this machine)".into(),
        ),
        other => Err(format!("unsupported scheme `{other}`")),
    }
}

impl Api {
    fn load(data_dir: &str) -> Result<Self, String> {
        let file = cloud_dir(data_dir).join("session.json");
        let saved = read_json(&file).ok_or("not signed in: run `cloud login` first")?;
        let base = check_base(saved["api"].as_str().unwrap_or_default())?;
        Ok(Self {
            http: http_client()?,
            base,
            file,
            saved,
        })
    }

    fn save(&self) -> Result<(), String> {
        write_private(&self.file, self.saved.to_string().as_bytes())
    }

    /// One request, with the access token; a token the API no longer accepts
    /// is renewed once with the refresh token (which rotates).
    async fn send(
        &mut self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<(u16, Value), String> {
        for attempt in 0..2 {
            let mut r = self
                .http
                .request(method.clone(), format!("{}{path}", self.base))
                .bearer_auth(self.saved["access_token"].as_str().unwrap_or_default());
            if let Some(b) = &body {
                r = r.json(b);
            }
            let resp = r.send().await.map_err(|e| format!("network: {e}"))?;
            let status = resp.status().as_u16();
            let value: Value = resp.json().await.unwrap_or(Value::Null);
            if status == 401
                && attempt == 0
                && value["code"] == "UNAUTHENTICATED"
                && self.refresh().await?
            {
                continue;
            }
            return Ok((status, value));
        }
        Err("the Cloud API did not accept the renewed token".into())
    }

    async fn refresh(&mut self) -> Result<bool, String> {
        let Some(refresh) = self.saved["refresh_token"].as_str().map(str::to_owned) else {
            return Ok(false);
        };
        let r = self
            .http
            .post(format!("{}/v1/auth/refresh", self.base))
            .json(&json!({"refresh_token": refresh}))
            .send()
            .await
            .map_err(|e| format!("network: {e}"))?;
        if !r.status().is_success() {
            return Ok(false);
        }
        let pair: Value = r.json().await.map_err(|e| e.to_string())?;
        self.keep(&pair)?;
        Ok(true)
    }

    /// Remember a token pair (never printed).
    fn keep(&mut self, pair: &Value) -> Result<(), String> {
        for k in [
            "access_token",
            "refresh_token",
            "tenant_id",
            "principal_id",
            "kind",
            "label",
        ] {
            self.saved[k] = pair[k].clone();
        }
        self.saved["expires_at_ms"] = json!(now_ms() + pair["expires_in_ms"].as_i64().unwrap_or(0));
        self.save()
    }
}

/// `cloud <verb> ...`. `data_dir` is `--data-dir` (verbs that touch no local
/// state take none).
pub(crate) async fn run(data_dir: Option<&str>, words: &[&str]) -> Result<(), String> {
    let dd = || data_dir.ok_or_else(|| CLOUD_USAGE.to_owned());
    let opt = |flag: &str| {
        words
            .iter()
            .position(|w| *w == flag)
            .and_then(|i| words.get(i + 1).copied())
    };
    match words {
        ["cloud", "login", ..] => login(dd()?, words).await,
        ["cloud", "logout", ..] => {
            let file = cloud_dir(dd()?).join("session.json");
            let _ = std::fs::remove_file(&file);
            println!("signed out (the token pair is deleted from this machine)");
            Ok(())
        }
        ["cloud", "status", ..] => {
            let mut api = Api::load(dd()?)?;
            let (s, health) = api.send(reqwest::Method::GET, "/v1/health", None).await?;
            println!(
                "cloud api={} health={} tenant={} principal={} kind={} label={}",
                api.base,
                if s == 200 { "ok" } else { "unhealthy" },
                api.saved["tenant_id"].as_str().unwrap_or("-"),
                api.saved["principal_id"].as_str().unwrap_or("-"),
                api.saved["kind"].as_str().unwrap_or("-"),
                api.saved["label"].as_str().unwrap_or("-"),
            );
            let _ = health;
            Ok(())
        }
        ["cloud", "handoff", ..] => {
            let sid = opt("--session").ok_or(CLOUD_USAGE)?;
            let tid = opt("--task").ok_or(CLOUD_USAGE)?;
            handoff(dd()?, sid, tid, opt("--confirm")).await
        }
        ["cloud", "list", ..] => list(dd()?).await,
        ["cloud", "watch", ..] => watch(dd()?, words).await,
        ["cloud", "approvals", ..] => {
            let sid = opt("--session").ok_or(CLOUD_USAGE)?;
            let mut api = Api::load(dd()?)?;
            let events = all_events(&mut api, sid).await?;
            for a in approvals_of(&events) {
                println!("{a}");
            }
            Ok(())
        }
        ["cloud", "approve", ..] => {
            let approval = opt("--approval").ok_or(CLOUD_USAGE)?;
            let intent = opt("--intent").ok_or(CLOUD_USAGE)?;
            let deny = words.contains(&"--deny");
            // Words that are neither a flag nor its value are the reason.
            let mut reason_words = Vec::new();
            let mut i = 2;
            while i < words.len() {
                match words[i] {
                    "--approval" | "--intent" => i += 1,
                    "--deny" => {}
                    w => reason_words.push(w),
                }
                i += 1;
            }
            let reason = reason_words.join(" ");
            approve(dd()?, approval, intent, deny, &reason).await
        }
        ["cloud", "policy", rest @ ..] => policy(data_dir, rest).await,
        _ => Err(CLOUD_USAGE.into()),
    }
}

// ---- sign in --------------------------------------------------------------

async fn login(data_dir: &str, words: &[&str]) -> Result<(), String> {
    let opt = |flag: &str| {
        words
            .iter()
            .position(|w| *w == flag)
            .and_then(|i| words.get(i + 1).copied())
    };
    let base = check_base(
        &opt("--api")
            .map(str::to_owned)
            .or_else(|| std::env::var("MODBIT_CLOUD_API").ok())
            .ok_or("--api <url> (or MODBIT_CLOUD_API) names the Cloud API")?,
    )?;
    let http = http_client()?;
    let pair: Value = if words.contains(&"--oidc") {
        oidc_login(&http, &base, words.contains(&"--no-browser")).await?
    } else {
        // A principal's secret, from the environment or stdin — never an argument.
        let secret = if words.contains(&"--secret-stdin") {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
                .map_err(|e| e.to_string())?;
            s.trim().to_owned()
        } else {
            std::env::var("MODBIT_CLOUD_SECRET").map_err(|_| {
                "give the principal's secret on stdin (--secret-stdin) or in MODBIT_CLOUD_SECRET, or sign in with --oidc".to_owned()
            })?
        };
        let r = http
            .post(format!("{base}/v1/auth/token"))
            .json(&json!({"secret": secret}))
            .send()
            .await
            .map_err(|e| format!("network: {e}"))?;
        let status = r.status().as_u16();
        let body: Value = r.json().await.unwrap_or(Value::Null);
        if status != 200 {
            return Err(refusal(status, &body));
        }
        body
    };
    let mut api = Api {
        http,
        base: base.clone(),
        file: cloud_dir(data_dir).join("session.json"),
        saved: json!({"api": base}),
    };
    api.keep(&pair)?;
    println!(
        "signed in api={} tenant={} principal={} label={}",
        base,
        pair["tenant_id"].as_str().unwrap_or("-"),
        pair["principal_id"].as_str().unwrap_or("-"),
        pair["label"].as_str().unwrap_or("-")
    );
    Ok(())
}

/// Authorization code with PKCE through the system browser and a loopback
/// redirect (RFC 8252): the verifier never leaves this process.
async fn oidc_login(http: &reqwest::Client, base: &str, no_browser: bool) -> Result<Value, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let verifier = b64(&rand::random::<[u8; 32]>());
    let challenge = b64(&Sha256::digest(verifier.as_bytes()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("loopback listener: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let r = http
        .post(format!("{base}/v1/auth/oidc/start"))
        .json(&json!({"redirect_uri": redirect, "code_challenge": challenge}))
        .send()
        .await
        .map_err(|e| format!("network: {e}"))?;
    let status = r.status().as_u16();
    let started: Value = r.json().await.unwrap_or(Value::Null);
    if status != 200 {
        return Err(refusal(status, &started));
    }
    let url = started["authorization_url"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let expected_state = started["state"].as_str().unwrap_or_default().to_owned();
    println!("authorize {url}");
    if !no_browser && std::env::var("MODBIT_CLOUD_NO_BROWSER").is_err() {
        open_browser(&url);
    }
    // Wait for the provider to redirect the browser here.
    let deadline = Instant::now() + Duration::from_secs(300);
    let (code, state) = loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .ok_or("the sign-in was not completed within five minutes")?;
        let (mut sock, _) = tokio::time::timeout(left, listener.accept())
            .await
            .map_err(|_| "the sign-in was not completed within five minutes".to_owned())?
            .map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 8192];
        let n = tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf))
            .await
            .unwrap_or(Ok(0))
            .unwrap_or(0);
        let head = String::from_utf8_lossy(&buf[..n]).to_string();
        let target = head
            .lines()
            .next()
            .and_then(|l| l.split(' ').nth(1))
            .unwrap_or_default()
            .to_owned();
        let parsed = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).ok();
        let q: std::collections::HashMap<String, String> = parsed
            .as_ref()
            .map(|u| {
                u.query_pairs()
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect()
            })
            .unwrap_or_default();
        let ok = parsed.as_ref().is_some_and(|u| u.path() == "/callback") && q.contains_key("code");
        let page = if ok {
            "Signed in. You can close this tab and return to the terminal."
        } else {
            "Nothing to do here."
        };
        let _ = sock
            .write_all(
                format!(
                    "HTTP/1.1 {} X\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{page}",
                    if ok { 200 } else { 404 },
                    page.len()
                )
                .as_bytes(),
            )
            .await;
        if ok {
            break (
                q["code"].clone(),
                q.get("state").cloned().unwrap_or_default(),
            );
        }
    };
    if state != expected_state {
        return Err("the redirect carries another login's state; nothing was signed in".into());
    }
    let r = http
        .post(format!("{base}/v1/auth/oidc/callback"))
        .json(&json!({"code": code, "state": state, "code_verifier": verifier}))
        .send()
        .await
        .map_err(|e| format!("network: {e}"))?;
    let status = r.status().as_u16();
    let body: Value = r.json().await.unwrap_or(Value::Null);
    if status != 200 {
        return Err(refusal(status, &body));
    }
    Ok(body)
}

/// Best effort: the URL is printed either way.
fn open_browser(url: &str) {
    let spawned = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if spawned.is_err() {
        eprintln!("modbit-cli: open the address above in a browser to continue");
    }
}

// ---- handoff ---------------------------------------------------------------

fn dir_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| {
                    let p = e.path();
                    if p.is_dir() {
                        dir_bytes(&p)
                    } else {
                        e.metadata().map_or(0, |m| m.len())
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// What a bundle on disk would send, in words a person can check.
fn print_plan(bundle: &Path, manifest: &Value, hash: &str) {
    let size = |name: &str| std::fs::metadata(bundle.join(name)).map_or(0, |m| m.len());
    let objects = bundle.join("objects");
    let object_count = std::fs::read_dir(&objects).map_or(0, |r| r.flatten().count());
    println!(
        "handoff-plan task={} session={} profile={} goal={:?}",
        manifest["task_id"].as_str().unwrap_or("-"),
        manifest["session_id"].as_str().unwrap_or("-"),
        manifest["execution_profile"].as_str().unwrap_or("-"),
        manifest["goal_text"].as_str().unwrap_or("")
    );
    println!(
        "leaves-the-machine repository history: {} bytes (git bundle of every ref, head {} on {})",
        size("repo.bundle"),
        manifest["git"]["head"].as_str().unwrap_or("-"),
        manifest["git"]["branch"].as_str().unwrap_or("-")
    );
    println!(
        "leaves-the-machine session log: {} events, {} bytes",
        manifest["events"].as_u64().unwrap_or(0),
        size("events.jsonl")
    );
    println!(
        "leaves-the-machine stored objects: {object_count} ({} bytes: transcripts, outputs, the checkpoint's files)",
        dir_bytes(&objects)
    );
    let checkpoint_ref = manifest["checkpoint"]["manifest_ref"]
        .as_str()
        .unwrap_or_default();
    if let Some(cm) = read_json(&objects.join(checkpoint_ref)) {
        let files: Vec<&String> = cm["files"]
            .as_object()
            .map(|m| m.keys().collect())
            .unwrap_or_default();
        println!(
            "leaves-the-machine worktree files: {} carried in the checkpoint ({})",
            files.len(),
            files
                .iter()
                .take(20)
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let list = |k: &str| {
        manifest[k]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default()
    };
    println!("requires capabilities={}", list("capabilities"));
    println!(
        "secret-handles {} (names only: no secret value leaves this machine)",
        if list("secret_handles").is_empty() {
            "-".to_owned()
        } else {
            list("secret_handles")
        }
    );
    println!("manifest {hash}");
}

fn read_registry(dir: &Path) -> Vec<Value> {
    read_json(&dir.join("handoffs.json"))
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

async fn handoff(
    data_dir: &str,
    session: &str,
    task: &str,
    confirm: Option<&str>,
) -> Result<(), String> {
    let mut api = Api::load(data_dir)?;
    let dir = cloud_dir(data_dir);
    let bundle = dir.join("bundles").join(task);
    let manifest_path = bundle.join("manifest.json");
    let confirm_prefix = confirm.map(str::to_lowercase);
    if confirm_prefix.is_none() {
        // Step one: the Core exports (and applies its own policy and secret
        // scan before a byte is written); nothing is sent.
        let (child, ready) = attach_or_spawn(data_dir).await?;
        let secret = decode_hex(&ready.boot_secret_hex).ok_or("bad secret in ready line")?;
        let mut client = Client::connect(
            &ready.endpoint,
            &secret,
            ClientKind::Cli,
            env!("CARGO_PKG_VERSION"),
        )
        .await
        .map_err(|e| e.to_string())?;
        let sid = parse_id(session)?;
        let lease = join_lease(&mut client, &sid).await?;
        let _ = std::fs::remove_dir_all(&bundle);
        std::fs::create_dir_all(&bundle).map_err(|e| e.to_string())?;
        let ack = client
            .command(envelope_fenced(
                "ExportHandoff",
                ExportHandoff {
                    task_id: Some(parse_id(task)?),
                    out_dir: bundle.to_string_lossy().into_owned(),
                }
                .encode_to_vec(),
                Some(lease),
            ))
            .await;
        drop(child);
        let ack = match ack {
            Ok(a) => a,
            Err(e) => {
                // Refused: nothing leaves, and nothing is left half-written.
                let _ = std::fs::remove_dir_all(&bundle);
                return Err(format!("{e}; nothing left this machine"));
            }
        };
        let exported: HandoffExported = Client::result(&ack).map_err(|e| e.to_string())?;
        let manifest: Value =
            serde_json::from_str(&exported.manifest_json).map_err(|e| e.to_string())?;
        print_plan(&bundle, &manifest, &exported.manifest_hash);
        println!(
            "confirm-required to send this to {}, run the same command with --confirm {}",
            api.base,
            &exported.manifest_hash[..12.min(exported.manifest_hash.len())]
        );
        crate::EXIT_CODE.store(2, std::sync::atomic::Ordering::SeqCst);
        return Ok(());
    }
    // Step two: the confirmation names the manifest the person saw.
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .map_err(|_| "no exported bundle for this task: run the handoff without --confirm first")?;
    let manifest_hash = hex::encode(Sha256::digest(manifest_text.as_bytes()));
    let prefix = confirm_prefix.unwrap_or_default();
    if prefix.len() < 8 || !manifest_hash.starts_with(&prefix) {
        return Err(format!(
            "--confirm does not match the exported bundle (manifest {}): nothing was sent",
            &manifest_hash[..12]
        ));
    }
    let manifest: Value = serde_json::from_str(&manifest_text).map_err(|e| e.to_string())?;
    // Upload every part to the tenant's object store; the API names each by
    // its content hash, which must be the one we computed.
    let mut parts = serde_json::Map::new();
    let mut named: Vec<(String, PathBuf)> = vec![
        ("events.jsonl".into(), bundle.join("events.jsonl")),
        ("manifest.json".into(), manifest_path.clone()),
    ];
    if bundle.join("repo.bundle").is_file() {
        named.push(("repo.bundle".into(), bundle.join("repo.bundle")));
    }
    let mut object_files: Vec<PathBuf> = std::fs::read_dir(bundle.join("objects"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();
    object_files.sort();
    let mut uploaded = 0usize;
    let mut upload_bytes = 0u64;
    for (name, path) in named
        .iter()
        .map(|(n, p)| (Some(n.clone()), p.clone()))
        .chain(object_files.iter().map(|p| (None, p.clone())))
    {
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let want = hex::encode(Sha256::digest(&bytes));
        let r = api
            .http
            .put(format!("{}/v1/objects", api.base))
            .bearer_auth(api.saved["access_token"].as_str().unwrap_or_default())
            .header("content-type", "application/octet-stream")
            .body(bytes.clone())
            .send()
            .await
            .map_err(|e| {
                format!("network: {e}; the handoff was not admitted, the same --confirm resumes it")
            })?;
        let status = r.status().as_u16();
        let body: Value = r.json().await.unwrap_or(Value::Null);
        if status == 401 {
            api.refresh().await?;
            return Err(
                "the Cloud API's token had expired and was renewed: run the same command again"
                    .into(),
            );
        }
        if status != 201 || body["hash"].as_str() != Some(want.as_str()) {
            return Err(format!(
                "upload of {} refused: {}",
                path.display(),
                refusal(status, &body)
            ));
        }
        uploaded += 1;
        upload_bytes += bytes.len() as u64;
        if let Some(n) = name {
            parts.insert(n, json!(want));
        }
    }
    // The command id is named by the manifest: a retry of the same bundle is
    // the same admission.
    let digest = Sha256::digest(format!("modbit-handoff-1:{manifest_hash}").as_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    let command_id = uuid_text(&id);
    let (status, admitted) = api
        .send(
            reqwest::Method::POST,
            "/v1/handoffs",
            Some(json!({"command_id": command_id, "manifest": manifest, "parts": Value::Object(parts)})),
        )
        .await?;
    if status != 200 && status != 201 {
        return Err(refusal(status, &admitted));
    }
    // Remember it: `cloud list` shows it and `cloud watch` knows the session.
    let mut registry = read_registry(&dir);
    registry.retain(|e| e["task_id"] != manifest["task_id"]);
    registry.push(json!({
        "api": api.base, "session_id": manifest["session_id"], "task_id": manifest["task_id"],
        "manifest_hash": manifest_hash, "bundle_hash": admitted["bundle_hash"], "handed_off_at_ms": now_ms(),
        "imported_through": admitted["imported_through"],
    }));
    write_private(
        &dir.join("handoffs.json"),
        serde_json::to_string_pretty(&registry)
            .unwrap_or_default()
            .as_bytes(),
    )?;
    println!(
        "handoff-admitted session={} task={} bundle={} imported_through={} uploaded_objects={uploaded} uploaded_bytes={upload_bytes}",
        manifest["session_id"].as_str().unwrap_or("-"),
        manifest["task_id"].as_str().unwrap_or("-"),
        admitted["bundle_hash"].as_str().unwrap_or("-"),
        admitted["imported_through"]
    );
    println!(
        "watch it with: cloud watch --session {} --follow",
        manifest["session_id"].as_str().unwrap_or("-")
    );
    Ok(())
}

async fn list(data_dir: &str) -> Result<(), String> {
    let mut api = Api::load(data_dir)?;
    let registry = read_registry(&cloud_dir(data_dir));
    if registry.is_empty() {
        println!("no task has been handed to the cloud from this profile");
    }
    for e in registry {
        let sid = e["session_id"].as_str().unwrap_or_default();
        let state = match api
            .send(reqwest::Method::GET, &format!("/v1/sessions/{sid}"), None)
            .await
        {
            Ok((200, v)) => v["tasks"]
                .as_array()
                .and_then(|t| t.iter().find(|t| t["task_id"] == e["task_id"]))
                .map_or("-".to_owned(), |t| {
                    t["state"].as_str().unwrap_or("-").to_owned()
                }),
            Ok((s, v)) => refusal(s, &v),
            Err(err) => err,
        };
        println!(
            "cloud-task location=cloud session={sid} task={} state={state} handed_off_at_ms={}",
            e["task_id"].as_str().unwrap_or("-"),
            e["handed_off_at_ms"]
        );
    }
    Ok(())
}

// ---- watch and approve -----------------------------------------------------

async fn events_after(
    api: &mut Api,
    session: &str,
    after: u64,
) -> Result<(Vec<Value>, u64), String> {
    let (s, v) = api
        .send(
            reqwest::Method::GET,
            &format!("/v1/events?session_id={session}&after={after}&limit=200"),
            None,
        )
        .await?;
    if s != 200 {
        return Err(refusal(s, &v));
    }
    Ok((
        v["events"].as_array().cloned().unwrap_or_default(),
        v["next_after"].as_u64().unwrap_or(after),
    ))
}

async fn all_events(api: &mut Api, session: &str) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut after = 0;
    loop {
        let (batch, next) = events_after(api, session, after).await?;
        if batch.is_empty() {
            return Ok(out);
        }
        out.extend(batch);
        after = next;
    }
}

/// The approvals on a log: one line each with the intent a decision names.
fn approvals_of(events: &[Value]) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut info: std::collections::HashMap<String, (String, String, String, String, String)> =
        Default::default();
    for e in events {
        let kind = e["envelope"]["event_type"].as_str().unwrap_or_default();
        let id = id_array_text(&e["envelope"]["aggregate_id"]);
        match kind {
            "ApprovalRequested" => {
                order.push(id.clone());
                info.insert(
                    id,
                    (
                        "REQUESTED".into(),
                        e["payload"]["tool_name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                        e["payload"]["effect_class"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                        e["payload"]["intent_hash"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                        e["envelope"]["task_id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    ),
                );
            }
            "ApprovalResolved" => {
                if let Some(i) = info.get_mut(&id) {
                    i.0 = if e["payload"]["approved"] == true {
                        "APPROVED"
                    } else {
                        "DENIED"
                    }
                    .into();
                }
            }
            "ApprovalExpired" => {
                if let Some(i) = info.get_mut(&id) {
                    i.0 = "EXPIRED".into();
                }
            }
            _ => {}
        }
    }
    order
        .into_iter()
        .filter_map(|id| {
            info.get(&id).map(|(status, tool, effect, intent, task)| {
                format!("approval {id} status={status} tool={tool} effect={effect} task={task} intent={intent}")
            })
        })
        .collect()
}

fn describe(e: &Value) -> String {
    let env = &e["envelope"];
    let kind = env["event_type"].as_str().unwrap_or("?");
    let mut line = format!(
        "event {} {} task={}",
        e["session_offset"].as_u64().unwrap_or(0),
        kind,
        env["task_id"].as_str().unwrap_or("-")
    );
    match kind {
        "ApprovalRequested" => line.push_str(&format!(
            " approval={} tool={} effect={} intent={}",
            id_array_text(&env["aggregate_id"]),
            e["payload"]["tool_name"].as_str().unwrap_or_default(),
            e["payload"]["effect_class"].as_str().unwrap_or_default(),
            e["payload"]["intent_hash"].as_str().unwrap_or_default()
        )),
        "ApprovalResolved" => line.push_str(&format!(
            " approval={} approved={} resolver={}",
            id_array_text(&env["aggregate_id"]),
            e["payload"]["approved"],
            e["payload"]["resolver"].as_str().unwrap_or_default()
        )),
        "TaskFailed" => line.push_str(&format!(
            " failure={}",
            e["payload"]["failure_code"].as_str().unwrap_or_default()
        )),
        _ => {}
    }
    line
}

async fn watch(data_dir: &str, words: &[&str]) -> Result<(), String> {
    let opt = |flag: &str| {
        words
            .iter()
            .position(|w| *w == flag)
            .and_then(|i| words.get(i + 1).copied())
    };
    let session = opt("--session").ok_or(CLOUD_USAGE)?.to_owned();
    let follow = words.contains(&"--follow");
    let json_out = words.contains(&"--json");
    let until: Vec<String> = opt("--until")
        .map(|u| u.split(',').map(str::to_owned).collect())
        .unwrap_or_default();
    let timeout = Duration::from_secs(
        opt("--timeout-secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    );
    let cursor_file = cloud_dir(data_dir).join("cursors").join(&session);
    // Resume where the last watch stopped, unless told where to start.
    let mut after: u64 = match opt("--after").and_then(|v| v.parse().ok()) {
        Some(n) => n,
        None => std::fs::read_to_string(&cursor_file)
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .unwrap_or(0),
    };
    let mut api = Api::load(data_dir)?;
    let started = Instant::now();
    let mut failures = 0u32;
    loop {
        match events_after(&mut api, &session, after).await {
            Ok((batch, next)) => {
                failures = 0;
                let mut stop = false;
                for e in &batch {
                    if json_out {
                        println!("{e}");
                    } else {
                        println!("{}", describe(e));
                    }
                    if until
                        .iter()
                        .any(|u| Some(u.as_str()) == e["envelope"]["event_type"].as_str())
                    {
                        stop = true;
                    }
                }
                if !batch.is_empty() {
                    after = next;
                    let _ = write_private(&cursor_file, after.to_string().as_bytes());
                }
                if stop {
                    return Ok(());
                }
                if batch.is_empty() {
                    if !follow {
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(400)).await;
                }
            }
            // A lost connection resumes from the cursor, not from the start.
            Err(e) if follow && e.starts_with("network:") => {
                failures += 1;
                if failures > 120 {
                    return Err(format!(
                        "{e} (gave up after {failures} attempts; resume with `cloud watch --session {session} --follow`, the cursor is {after})"
                    ));
                }
                eprintln!("modbit-cli: connection lost ({e}); retrying from cursor {after}");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => return Err(e),
        }
        if !timeout.is_zero() && started.elapsed() > timeout {
            return Err(format!(
                "watch timed out after {}s (cursor {after})",
                timeout.as_secs()
            ));
        }
    }
}

async fn approve(
    data_dir: &str,
    approval: &str,
    intent: &str,
    deny: bool,
    reason: &str,
) -> Result<(), String> {
    let mut api = Api::load(data_dir)?;
    let verb = if deny { "deny" } else { "approve" };
    let command_id = uuid_text(&rand::random::<[u8; 16]>());
    let (status, body) = api
        .send(
            reqwest::Method::POST,
            &format!("/v1/approvals/{approval}:{verb}"),
            Some(json!({"command_id": command_id, "intent_hash": intent, "reason": reason})),
        )
        .await?;
    match status {
        200 => {
            println!(
                "approval {approval} status={} intent={intent}",
                body["status"].as_str().unwrap_or("-")
            );
            Ok(())
        }
        // Relayed to the session's owner: its outcome is the command's.
        202 => {
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                let (s, c) = api
                    .send(
                        reqwest::Method::GET,
                        &format!(
                            "/v1/commands/{}",
                            body["command_id"].as_str().unwrap_or(&command_id)
                        ),
                        None,
                    )
                    .await?;
                if s == 200 && c["status"] != "PENDING" {
                    if c["status"] == "ACCEPTED" {
                        println!(
                            "approval {approval} status={} intent={intent}",
                            c["result"]["status"].as_str().unwrap_or("-")
                        );
                        return Ok(());
                    }
                    return Err(format!(
                        "{}: {}",
                        c["code"].as_str().unwrap_or("REJECTED"),
                        c["result"]["message"]
                            .as_str()
                            .unwrap_or("the session's owner refused the decision")
                    ));
                }
                if Instant::now() > deadline {
                    return Err("the session's owner did not answer within a minute; the decision stays queued".into());
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
        409 if body["code"] == "APPROVAL_NOT_OPEN" => {
            Err(format!("already decided: {}", refusal(status, &body)))
        }
        _ => Err(refusal(status, &body)),
    }
}

// ---- policy bundle ----------------------------------------------------------

fn signing_key(path: &str) -> Result<ed25519_dalek::SigningKey, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let bytes = hex::decode(text.trim())
        .map_err(|_| format!("{path} is not a hex-encoded 32-byte seed"))?;
    let seed: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| format!("{path} is not a 32-byte seed"))?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&seed))
}

async fn policy(data_dir: Option<&str>, words: &[&str]) -> Result<(), String> {
    let opt = |flag: &str| {
        words
            .iter()
            .position(|w| *w == flag)
            .and_then(|i| words.get(i + 1).copied())
    };
    match words {
        ["keygen", ..] => {
            let out = opt("--out").ok_or(CLOUD_USAGE)?;
            let seed: [u8; 32] = rand::random();
            write_private(Path::new(out), hex::encode(seed).as_bytes())?;
            let key = ed25519_dalek::SigningKey::from_bytes(&seed);
            println!(
                "policy-key written={out} public_key={} (register the public key with the platform administrator; keep the file private)",
                hex::encode(key.verifying_key().to_bytes())
            );
            Ok(())
        }
        ["sign", ..] => {
            let key = signing_key(opt("--key").ok_or(CLOUD_USAGE)?)?;
            let key_id = opt("--key-id").ok_or(CLOUD_USAGE)?;
            let tenant = opt("--tenant").ok_or(CLOUD_USAGE)?;
            let generation: u64 = opt("--generation")
                .and_then(|v| v.parse().ok())
                .ok_or(CLOUD_USAGE)?;
            let ttl: i64 = opt("--ttl-secs")
                .and_then(|v| v.parse().ok())
                .unwrap_or(86_400);
            let config_path = opt("--config").ok_or(CLOUD_USAGE)?;
            let admin_config: Value = serde_json::from_slice(
                &std::fs::read(config_path).map_err(|e| format!("{config_path}: {e}"))?,
            )
            .map_err(|e| format!("{config_path} is not JSON: {e}"))?;
            let now = now_ms();
            let doc = BundleDocument {
                kind: policy_bundle::KIND.into(),
                schema_version: policy_bundle::SCHEMA_VERSION,
                tenant_id: tenant.into(),
                generation,
                issued_at_ms: now,
                expires_at_ms: now + ttl * 1000,
                min_protocol_major: 1,
                admin_config,
            };
            let signed = policy_bundle::sign(&doc, key_id, &key).map_err(|e| e.to_string())?;
            let out = opt("--out").ok_or(CLOUD_USAGE)?;
            std::fs::write(
                out,
                serde_json::to_string_pretty(&signed).unwrap_or_default(),
            )
            .map_err(|e| format!("{out}: {e}"))?;
            println!(
                "policy-signed generation={generation} key_id={key_id} expires_at_ms={} out={out}",
                doc.expires_at_ms
            );
            Ok(())
        }
        ["publish", ..] => {
            let file = opt("--file").ok_or(CLOUD_USAGE)?;
            let signed: Value =
                serde_json::from_slice(&std::fs::read(file).map_err(|e| format!("{file}: {e}"))?)
                    .map_err(|e| format!("{file} is not JSON: {e}"))?;
            let mut api = Api::load(data_dir.ok_or(CLOUD_USAGE)?)?;
            let (s, v) = api
                .send(reqwest::Method::PUT, "/v1/policy/bundle", Some(signed))
                .await?;
            if s != 201 {
                return Err(refusal(s, &v));
            }
            println!(
                "policy-published generation={} expires_at_ms={}",
                v["generation"], v["expires_at_ms"]
            );
            Ok(())
        }
        ["fetch", ..] => {
            let dd = data_dir.ok_or(CLOUD_USAGE)?;
            let mut keys: Vec<(String, [u8; 32])> = Vec::new();
            for (i, w) in words.iter().enumerate() {
                if *w == "--trust" {
                    let (id, hex_key) = words
                        .get(i + 1)
                        .and_then(|t| t.split_once('='))
                        .ok_or("--trust takes <key-id>=<public-key-hex>")?;
                    let bytes = hex::decode(hex_key)
                        .ok()
                        .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok());
                    keys.push((
                        id.to_owned(),
                        bytes.ok_or("--trust: the public key is not 32 bytes of hex")?,
                    ));
                }
            }
            if keys.is_empty() {
                return Err("name at least one trusted key with --trust <key-id>=<public-key-hex>: a bundle is never trusted for its own say-so".into());
            }
            let mut api = Api::load(dd)?;
            let (s, v) = api
                .send(reqwest::Method::GET, "/v1/policy/bundle", None)
                .await?;
            if s != 200 {
                return Err(refusal(s, &v));
            }
            let signed: SignedBundle = serde_json::from_value(v["bundle"].clone())
                .map_err(|e| format!("REFUSED BUNDLE_MALFORMED: {e}; nothing was applied"))?;
            let floor_file = cloud_dir(dd).join("policy-floor.json");
            let floor = read_json(&floor_file)
                .and_then(|f| f["generation"].as_u64())
                .unwrap_or(0);
            let tenant = api.saved["tenant_id"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            // Fail closed: anything but a bundle that verifies applies nothing.
            let doc = policy_bundle::verify(
                &signed,
                &Expect {
                    keys: &keys,
                    tenant_id: &tenant,
                    now_ms: now_ms(),
                    min_generation: floor,
                    protocol_major: modbit_protocol::PROTOCOL_VERSION.major,
                },
            )
            .map_err(|r| format!("REFUSED {}: {r}; nothing was applied", r.code()))?;
            if let Some(out) = opt("--write-admin-config") {
                std::fs::write(
                    out,
                    serde_json::to_string_pretty(&doc.admin_config).unwrap_or_default(),
                )
                .map_err(|e| format!("{out}: {e}"))?;
            }
            write_private(
                &floor_file,
                json!({"generation": doc.generation}).to_string().as_bytes(),
            )?;
            println!(
                "policy generation={} key_id={} expires_at_ms={} verified=true",
                doc.generation, signed.key_id, doc.expires_at_ms
            );
            Ok(())
        }
        _ => Err(CLOUD_USAGE.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_go_to_https_or_this_machine_only() {
        assert!(check_base("https://cloud.example/").is_ok());
        assert!(check_base("http://127.0.0.1:8787").is_ok());
        assert!(check_base("http://localhost:8787/").is_ok());
        assert!(check_base("http://cloud.example").is_err());
        assert!(check_base("ftp://cloud.example").is_err());
        assert!(check_base("not a url").is_err());
    }

    #[test]
    fn an_aggregate_id_is_a_uuid_text() {
        let bytes: Vec<u8> = (0..16).collect();
        assert_eq!(uuid_text(&bytes), "00010203-0405-0607-0809-0a0b0c0d0e0f");
        let v = json!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        assert_eq!(id_array_text(&v), "00010203-0405-0607-0809-0a0b0c0d0e0f");
    }

    #[test]
    fn approvals_are_listed_with_the_intent_a_decision_names() {
        let id: Vec<u8> = (0..16).collect();
        let events = vec![
            json!({"session_offset": 5, "envelope": {"event_type": "ApprovalRequested", "aggregate_id": id, "task_id": "t-1"}, "payload": {"tool_name": "shell.exec", "effect_class": "Destructive", "intent_hash": "abc"}}),
            json!({"session_offset": 6, "envelope": {"event_type": "ApprovalResolved", "aggregate_id": id}, "payload": {"approved": true, "resolver": "user:ada"}}),
        ];
        let lines = approvals_of(&events);
        assert_eq!(
            lines,
            [
                "approval 00010203-0405-0607-0809-0a0b0c0d0e0f status=APPROVED tool=shell.exec effect=Destructive task=t-1 intent=abc"
            ]
        );
    }
}
