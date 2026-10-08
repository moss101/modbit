//! Broker: sessions, process/PTY lifecycle, durable bounded output log,
//! replay, ownership.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use modbit_protocol::client::BoxedStream;
use modbit_protocol::framing::{FrameError, read_message, write_message};
use modbit_protocol::local::{Endpoint, ReadyLine, encode_hex};
use modbit_protocol::v1::exec_frame::Body;
use modbit_protocol::v1::{
    AcquireTerminalLease, Attach, Cancel, ExecError, ExecFrame, ExecRequest, ExecStarted, HelloAck,
    ListSessions, OutputChunk, ProcessExited, ReleaseTerminalLease, SessionInfo, SessionList,
    StdinWritten, TerminalAck, TerminalLeaseState, TerminalResize, TerminalResized, WriteStdin,
};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, mpsc, watch};

use crate::seglog::{self, ReadOutcome, SegLog, TAG_PTY, TAG_STDERR, TAG_STDOUT};

/// Largest output frame sent to a client (REQ-EV-0108).
const CHUNK: usize = 64 * 1024;

/// The most stdin bytes one write may carry (PX-099): a bigger one is
/// refused INPUT_TOO_LARGE and writes nothing.
const MAX_STDIN_BYTES: usize = 64 * 1024;

/// How long a full acknowledgement window waits for an `Ack` before the
/// attachment is dropped to cursor-pull (PX-099), when the attach names none.
const DEFAULT_STALL_MS: u64 = 30_000;

/// PTY bounds a `Resize` may ask for.
const MAX_PTY_ROWS: u32 = 500;
const MAX_PTY_COLS: u32 = 1000;

/// The terminal's size when nobody asked for one.
const DEFAULT_PTY_ROWS: u32 = 40;
const DEFAULT_PTY_COLS: u32 = 120;

/// Connection numbers: the input lease is held by a connection.
static CONNECTIONS: AtomicU64 = AtomicU64::new(1);

/// The terminal owner lease (PX-099): the connection a person attached
/// through holds the right to type into the terminal; the agent's writes are
/// refused while it does.
struct InputLease {
    /// The connection that holds it (it ends with the connection).
    conn: u64,
    /// Who the Core says holds it, for the record.
    holder: String,
    /// Where to tell the holder that it was taken from them.
    notify: mpsc::Sender<ExecFrame>,
}

fn tag_name(t: u8) -> &'static str {
    match t {
        TAG_STDOUT => "stdout",
        TAG_STDERR => "stderr",
        _ => "pty",
    }
}

/// What can write to a running process.
enum Stdin {
    Pipe(tokio::process::ChildStdin),
    Pty(Box<dyn Write + Send>),
    Closed,
}

/// How to kill a running process.
enum Killer {
    Child(Arc<Mutex<Option<tokio::process::Child>>>),
    Pty(Arc<std::sync::Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>>),
}

struct Session {
    id: String,
    request_id: String,
    argv: Vec<String>,
    /// Working directory (EPR-018: a review environment's processes are
    /// found and ended by it).
    cwd: String,
    /// The principal that owns the session (`task:<id>`), or empty for a
    /// session the host itself started. Only the owner and the host (the
    /// empty requester) may list, read, write to or cancel it.
    owner: String,
    /// Bytes written to the log (cursor high-water mark); watchers wake on change.
    written: watch::Sender<u64>,
    running: AtomicBool,
    exited: Mutex<Option<ProcessExited>>,
    stdin: Mutex<Stdin>,
    killer: Mutex<Option<Killer>>,
    cancelled: AtomicBool,
    data_bytes: AtomicU64,
    /// The segmented, indexed output log (bounded by the replay window).
    log: std::sync::Mutex<SegLog>,
    /// Where the log's segments live (readers open them outside the lock).
    log_dir: PathBuf,
    /// Durable metadata beside the log (M4.5): what a restarted broker
    /// needs to serve the session's replay and report its state.
    meta_path: PathBuf,
    started_at_ms: i64,
    /// Terminal replay generation (docs/13 "Fencing and epochs"): the newest
    /// attach generation; an older attach is refused, a newer one ends the
    /// older attachments.
    generation: AtomicU64,
    /// The process died with a previous broker: the log is durable and
    /// replayable, the exit unknown.
    lost: AtomicBool,
    /// The PTY master, kept so `Resize` can change the terminal's size
    /// (PX-099); taken (dropped) when the process ends.
    pty_master: std::sync::Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    /// Whether the session runs on a PTY, and the size it has now.
    pty: AtomicBool,
    rows: AtomicU32,
    cols: AtomicU32,
    /// The person's input lease, if one is held (PX-099).
    lease: std::sync::Mutex<Option<InputLease>>,
    /// The broker's replay window per session, for the registry.
    replay_window_bytes: u64,
}

/// What survives a broker restart beside the output log (M4.5).
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SessionMeta {
    id: String,
    request_id: String,
    argv: Vec<String>,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    owner: String,
    started_at_ms: i64,
    /// Exit, once known.
    exited: Option<ExitMeta>,
    /// Newest attach generation seen.
    #[serde(default)]
    generation: u64,
    /// PX-099: whether it runs on a PTY and the size it was last given.
    #[serde(default)]
    pty: bool,
    #[serde(default)]
    rows: u32,
    #[serde(default)]
    cols: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ExitMeta {
    exit_code: Option<i32>,
    signal: Option<i32>,
    duration_ms: u64,
    output_ref: String,
    total_bytes: u64,
    timed_out: bool,
    cancelled: bool,
    #[serde(default)]
    lost: bool,
    /// The cursor of the first byte `output_ref` holds (0 = all of it).
    #[serde(default)]
    retained_from: u64,
}

/// Whether an environment variable is one that carries a credential (M7.7,
/// docs/22 "Prompt-injection isolation", docs/23): a child process never
/// inherits it, whatever a command asks — the broker holds no secret a
/// shell could print. Names are matched by their shape, so a new provider's
/// key is covered before anyone lists it.
fn secret_bearing(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.ends_with("_API_KEY")
        || n.ends_with("_TOKEN")
        || n.ends_with("_SECRET")
        || n.ends_with("_SECRET_KEY")
        || n.ends_with("_ACCESS_KEY")
        || n.ends_with("_PRIVATE_KEY")
        || n.contains("PASSWORD")
        || n.contains("PASSPHRASE")
        || n == "OPENAI_API_KEY"
        || n == "ANTHROPIC_API_KEY"
        || n == "MODBIT_GITHUB_TOKEN"
        || n == "GITHUB_TOKEN"
        || n == "GH_TOKEN"
}

/// The value of a secret-bearing variable this broker's own environment
/// holds: a command whose requested environment carries such a value is
/// refused before it runs (the model never legitimately knows one).
fn leaks_own_secret(env: &std::collections::HashMap<String, String>) -> Option<String> {
    let own: Vec<(String, String)> = std::env::vars()
        .filter(|(k, v)| secret_bearing(k) && v.len() >= 8)
        .collect();
    for (k, v) in env {
        if let Some((name, _)) = own.iter().find(|(_, secret)| v.contains(secret.as_str())) {
            return Some(format!("{k} carries the value of {name}"));
        }
    }
    None
}

fn nonzero(v: u32, default: u32) -> u32 {
    if v == 0 { default } else { v }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Session {
    /// Persist the metadata (written whole, then renamed into place).
    async fn write_meta(&self) {
        let exited = self.exited.lock().await.clone().map(|e| ExitMeta {
            exit_code: e.exit_code,
            signal: e.signal,
            duration_ms: e.duration_ms,
            output_ref: e.output_ref,
            total_bytes: e.total_bytes,
            timed_out: e.timed_out,
            cancelled: e.cancelled,
            lost: self.lost.load(Ordering::SeqCst),
            retained_from: e.retained_from,
        });
        let meta = SessionMeta {
            id: self.id.clone(),
            request_id: self.request_id.clone(),
            argv: self.argv.clone(),
            cwd: self.cwd.clone(),
            owner: self.owner.clone(),
            started_at_ms: self.started_at_ms,
            exited,
            generation: self.generation.load(Ordering::SeqCst),
            pty: self.pty.load(Ordering::SeqCst),
            rows: self.rows.load(Ordering::SeqCst),
            cols: self.cols.load(Ordering::SeqCst),
        };
        let tmp = self.meta_path.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec(&meta)
            && std::fs::write(&tmp, bytes).is_ok()
        {
            let _ = std::fs::rename(&tmp, &self.meta_path);
        }
    }

    fn status(&self) -> &'static str {
        if self.running.load(Ordering::SeqCst) {
            "RUNNING"
        } else if self.lost.load(Ordering::SeqCst) {
            "LOST"
        } else {
            "EXITED"
        }
    }

    /// Append one record; returns the new data high-water mark.
    fn append(&self, tag: u8, data: &[u8]) -> u64 {
        let mut log = self.log.lock().expect("log");
        match log.append(tag, data) {
            Ok(new) => {
                self.data_bytes.store(new, Ordering::SeqCst);
                self.written.send_replace(new);
                new
            }
            Err(e) => {
                eprintln!("modbit-execd: session {}: output not stored: {e}", self.id);
                log.written()
            }
        }
    }

    /// The next piece of output at `cursor` (at most one record, at most
    /// `CHUNK` bytes) or why there is none. The segment table is read under
    /// the writer's lock; the bytes are read outside it, through the index,
    /// so a reader never re-reads the log and never blocks the writer.
    fn read_chunk(&self, cursor: u64, high: u64) -> std::io::Result<ReadOutcome> {
        let mut last = None;
        for _ in 0..4 {
            let (segments, _) = self.log.lock().expect("log").snapshot();
            match seglog::read_at(&self.log_dir, &self.id, &segments, high, cursor, CHUNK) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => last = Some(e),
                other => return other,
            }
        }
        Err(last.expect("loop ran"))
    }

    fn oldest_cursor(&self) -> u64 {
        self.log.lock().expect("log").oldest()
    }

    /// The session as the registry lists it.
    async fn info(&self) -> SessionInfo {
        let exited = self.exited.lock().await.clone();
        let holder = self
            .lease
            .lock()
            .expect("lease")
            .as_ref()
            .map(|l| l.holder.clone())
            .unwrap_or_default();
        SessionInfo {
            session_id: self.id.clone(),
            request_id: self.request_id.clone(),
            argv: self.argv.clone(),
            running: self.running.load(Ordering::SeqCst),
            bytes_so_far: self.data_bytes.load(Ordering::SeqCst),
            exit_code: exited.as_ref().and_then(|e| e.exit_code),
            status: self.status().into(),
            replay_generation: self.generation.load(Ordering::SeqCst),
            started_at_ms: self.started_at_ms,
            cwd: self.cwd.clone(),
            owner: self.owner.clone(),
            oldest_cursor: self.oldest_cursor(),
            pty: self.pty.load(Ordering::SeqCst),
            pty_rows: self.rows.load(Ordering::SeqCst),
            pty_cols: self.cols.load(Ordering::SeqCst),
            input_lease_holder: holder,
            duration_ms: exited.as_ref().map_or(0, |e| e.duration_ms),
            cancelled: exited.as_ref().is_some_and(|e| e.cancelled),
            timed_out: exited.as_ref().is_some_and(|e| e.timed_out),
            output_ref: exited
                .as_ref()
                .map(|e| e.output_ref.clone())
                .unwrap_or_default(),
            replay_window_bytes: self.replay_window_bytes,
            lost: self.lost.load(Ordering::SeqCst),
        }
    }

    /// Drop the input lease (the session ended, or its holder left).
    fn clear_lease(&self, only_conn: Option<u64>) {
        let mut lease = self.lease.lock().expect("lease");
        if only_conn.is_none_or(|c| lease.as_ref().is_some_and(|l| l.conn == c)) {
            *lease = None;
        }
    }

    /// Whether `requester` may act on this session: the host (empty) always,
    /// a task only on a session it owns.
    fn permits(&self, requester: &str) -> bool {
        requester.is_empty() || requester == self.owner
    }
}

fn not_owned(session_id: &str, requester: &str) -> ExecFrame {
    err_frame(
        session_id,
        "SESSION_NOT_OWNED",
        format!("session {session_id} belongs to another owner than `{requester}`"),
    )
}

/// The bounds a broker keeps (docs/21 "sliding replay window"). Disk per
/// session is at most `replay_window_bytes + segment_bytes`; finished
/// sessions are dropped by count and by age.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Output kept replayable per session.
    pub replay_window_bytes: u64,
    /// Rotation size of the on-disk log.
    pub segment_bytes: u64,
    /// Finished sessions kept (oldest dropped first).
    pub retain_sessions: usize,
    /// Finished sessions older than this are dropped (`None` = never).
    pub retain_age: Option<Duration>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            replay_window_bytes: 64 * 1024 * 1024,
            segment_bytes: 4 * 1024 * 1024,
            retain_sessions: 256,
            retain_age: Some(Duration::from_secs(7 * 24 * 3600)),
        }
    }
}

/// Everything `run` needs.
pub struct Config {
    pub data_dir: PathBuf,
    pub orphan_grace: Option<Duration>,
    /// The content-addressed object store `output_ref`s are sealed into
    /// (the Core's own store when the Core starts the broker).
    pub object_dir: PathBuf,
    pub limits: Limits,
}

struct Broker {
    data_dir: PathBuf,
    object_dir: PathBuf,
    limits: Limits,
    boot_secret: Vec<u8>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    by_request: Mutex<HashMap<String, String>>,
    /// Authenticated connections right now (the orphan grace starts at 0).
    clients: AtomicUsize,
}

/// Decrements the client count when a connection ends.
struct ClientGuard(Arc<Broker>);

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.clients.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Rebuild the sessions a previous broker left on disk (M4.5, docs/54 fault
/// 12): every session with a log and metadata comes back — replayable from
/// its durable log by cursor; one that was still running when the previous
/// broker died is LOST (its process is gone, its exit unknown).
fn load_sessions(data_dir: &Path, object_dir: &Path, limits: &Limits) -> Vec<Arc<Session>> {
    let dir = data_dir.join("sessions");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<SessionMeta>(&bytes) else {
            continue;
        };
        let log = match SegLog::open(
            &dir,
            &meta.id,
            limits.replay_window_bytes,
            limits.segment_bytes,
        ) {
            Ok(Some(l)) => l,
            _ => continue,
        };
        let size = log.written();
        let (written, _) = watch::channel(size);
        let lost = meta.exited.is_none();
        let exited = match &meta.exited {
            Some(x) => ProcessExited {
                session_id: meta.id.clone(),
                exit_code: x.exit_code,
                signal: x.signal,
                duration_ms: x.duration_ms,
                output_ref: x.output_ref.clone(),
                total_bytes: x.total_bytes,
                timed_out: x.timed_out,
                cancelled: x.cancelled,
                retained_from: x.retained_from,
            },
            None => {
                // The process is gone with the previous broker: seal what the
                // log holds so the OutputRef and the replay still work.
                let (segments, _) = log.snapshot();
                let sealed = seglog::seal(&dir, &meta.id, &segments, object_dir);
                if let Err(e) = &sealed {
                    eprintln!(
                        "modbit-execd: session {}: sealing its output failed: {e}",
                        meta.id
                    );
                }
                let sealed = sealed.ok();
                ProcessExited {
                    session_id: meta.id.clone(),
                    exit_code: None,
                    signal: None,
                    duration_ms: 0,
                    output_ref: sealed.as_ref().map(|s| s.hash.clone()).unwrap_or_default(),
                    total_bytes: size,
                    timed_out: false,
                    cancelled: false,
                    retained_from: sealed.map_or(0, |s| s.retained_from),
                }
            }
        };
        let session = Arc::new(Session {
            id: meta.id.clone(),
            request_id: meta.request_id.clone(),
            argv: meta.argv.clone(),
            cwd: meta.cwd.clone(),
            owner: meta.owner.clone(),
            written,
            running: AtomicBool::new(false),
            exited: Mutex::new(Some(exited)),
            stdin: Mutex::new(Stdin::Closed),
            killer: Mutex::new(None),
            cancelled: AtomicBool::new(meta.exited.as_ref().is_some_and(|x| x.cancelled)),
            data_bytes: AtomicU64::new(size),
            log: std::sync::Mutex::new(log),
            log_dir: dir.clone(),
            meta_path: path,
            started_at_ms: meta.started_at_ms,
            generation: AtomicU64::new(meta.generation),
            lost: AtomicBool::new(lost || meta.exited.as_ref().is_some_and(|x| x.lost)),
            pty_master: std::sync::Mutex::new(None),
            pty: AtomicBool::new(meta.pty),
            rows: AtomicU32::new(meta.rows),
            cols: AtomicU32::new(meta.cols),
            lease: std::sync::Mutex::new(None),
            replay_window_bytes: limits.replay_window_bytes,
        });
        out.push(session);
    }
    out
}

/// The owner-only file a restarted Core reads to reattach to a broker that
/// survived it (docs/33: durable terminal resources are detached, not
/// killed).
pub fn ready_file(data_dir: &Path) -> PathBuf {
    data_dir.join("execd.ready")
}

fn write_owner_only(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("ready.tmp");
    std::fs::write(&tmp, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

/// Run until the listener fails, Ctrl-C, or — with an orphan grace — until
/// no Core has been connected for that long (docs/54 fault 11: a process
/// outlives a client disconnect, but not forever without an owner).
pub async fn run(config: Config) -> Result<()> {
    let Config {
        data_dir,
        orphan_grace,
        object_dir,
        mut limits,
    } = config;
    limits.replay_window_bytes = limits.replay_window_bytes.max(limits.segment_bytes);
    std::fs::create_dir_all(data_dir.join("sessions"))?;
    std::fs::create_dir_all(&object_dir)?;
    let boot_secret: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
    let nonce = encode_hex(&(0..6).map(|_| rand::random::<u8>()).collect::<Vec<_>>());
    let endpoint =
        Endpoint::for_dir(&data_dir.join("execd"), &nonce).context("choosing endpoint")?;
    let mut sessions = HashMap::new();
    let mut by_request = HashMap::new();
    for s in load_sessions(&data_dir, &object_dir, &limits) {
        by_request.insert(s.request_id.clone(), s.id.clone());
        sessions.insert(s.id.clone(), s);
    }
    let recovered = sessions.len();
    let broker = Arc::new(Broker {
        data_dir: data_dir.clone(),
        object_dir,
        limits,
        boot_secret: boot_secret.clone(),
        sessions: Mutex::new(sessions),
        by_request: Mutex::new(by_request),
        clients: AtomicUsize::new(0),
    });
    for s in broker.sessions.lock().await.values() {
        s.write_meta().await;
    }
    broker.prune_finished().await;
    let listener = Listener::bind(&endpoint)
        .await
        .context("binding endpoint")?;
    let ready = ReadyLine {
        endpoint: endpoint.clone(),
        boot_secret_hex: encode_hex(&boot_secret),
        protocol: (
            modbit_protocol::PROTOCOL_VERSION.major,
            modbit_protocol::PROTOCOL_VERSION.minor,
        ),
    }
    .render();
    write_owner_only(&ready_file(&data_dir), &ready).context("writing execd.ready")?;
    println!("{ready}");
    std::io::stdout().flush().ok();
    if recovered > 0 {
        eprintln!("modbit-execd: recovered {recovered} durable session(s) from disk");
    }
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let grace = async {
        let Some(grace) = orphan_grace else {
            std::future::pending::<()>().await;
            return;
        };
        let mut idle_since: Option<Instant> = None;
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if broker.clients.load(Ordering::SeqCst) > 0 {
                idle_since = None;
                continue;
            }
            let since = *idle_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= grace {
                return;
            }
        }
    };
    tokio::pin!(grace);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let stream = accepted.context("accept")?;
                let b = Arc::clone(&broker);
                tokio::spawn(async move {
                    if let Err(e) = serve(b, stream).await {
                        eprintln!("modbit-execd: connection ended: {e}");
                    }
                });
            }
            _ = &mut shutdown => break,
            () = &mut grace => {
                eprintln!("modbit-execd: no Core connected for the orphan grace; stopping");
                break;
            }
        }
    }
    // Nobody to report to: running processes are stopped and recorded as
    // cancelled, so a later broker serves their logs as exited, not lost.
    let running: Vec<Arc<Session>> = broker
        .sessions
        .lock()
        .await
        .values()
        .filter(|s| s.running.load(Ordering::SeqCst))
        .cloned()
        .collect();
    for s in &running {
        s.cancelled.store(true, Ordering::SeqCst);
        kill(s).await;
    }
    for s in &running {
        let deadline = Instant::now() + Duration::from_secs(3);
        while s.running.load(Ordering::SeqCst) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    let _ = std::fs::remove_file(ready_file(&data_dir));
    listener.cleanup();
    Ok(())
}

enum Listener {
    #[cfg(unix)]
    Unix(tokio::net::UnixListener, PathBuf),
    #[cfg(windows)]
    Pipe {
        name: String,
        next: std::sync::Mutex<Option<tokio::net::windows::named_pipe::NamedPipeServer>>,
    },
}

impl Listener {
    async fn bind(endpoint: &Endpoint) -> Result<Self> {
        #[cfg(unix)]
        {
            let path = endpoint.path();
            let _ = std::fs::remove_file(&path);
            let l = tokio::net::UnixListener::bind(&path)?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Listener::Unix(l, path))
        }
        #[cfg(windows)]
        {
            let first = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(&endpoint.0)?;
            Ok(Listener::Pipe {
                name: endpoint.0.clone(),
                next: std::sync::Mutex::new(Some(first)),
            })
        }
    }

    async fn accept(&self) -> Result<BoxedStream> {
        #[cfg(unix)]
        {
            let Listener::Unix(l, _) = self;
            let (s, _) = l.accept().await?;
            Ok(Box::new(s))
        }
        #[cfg(windows)]
        {
            let Listener::Pipe { name, next } = self;
            let server = next.lock().unwrap().take().expect("pipe instance");
            server.connect().await?;
            let replacement = tokio::net::windows::named_pipe::ServerOptions::new().create(name)?;
            *next.lock().unwrap() = Some(replacement);
            Ok(Box::new(server))
        }
    }

    fn cleanup(&self) {
        #[cfg(unix)]
        {
            let Listener::Unix(_, path) = self;
            let _ = std::fs::remove_file(path);
        }
    }
}

fn err_frame(request_id: &str, code: &str, message: impl Into<String>) -> ExecFrame {
    ExecFrame {
        body: Some(Body::Error(ExecError {
            request_id: request_id.into(),
            code: code.into(),
            message: message.into(),
        })),
    }
}

async fn serve(broker: Arc<Broker>, mut stream: BoxedStream) -> Result<()> {
    // Handshake: boot secret, constant-time compare.
    let first = match read_message::<_, ExecFrame>(&mut stream).await {
        Ok(Some(f)) => f,
        Ok(None) => return Ok(()),
        Err(e) => {
            let _ = write_message(
                &mut stream,
                &err_frame("", "MALFORMED_FRAME", e.to_string()),
            )
            .await;
            return Ok(());
        }
    };
    let Some(Body::ClientHello(hello)) = first.body else {
        let _ = write_message(
            &mut stream,
            &err_frame("", "HANDSHAKE_REQUIRED", "first frame must be ClientHello"),
        )
        .await;
        return Ok(());
    };
    let presented = hello
        .auth
        .as_ref()
        .map(|a| a.boot_secret.as_slice())
        .unwrap_or(&[]);
    if presented.len() != broker.boot_secret.len()
        || !bool::from(presented.ct_eq(&broker.boot_secret))
    {
        let _ = write_message(
            &mut stream,
            &err_frame("", "UNAUTHENTICATED", "boot secret rejected"),
        )
        .await;
        return Ok(());
    }
    let ours = modbit_protocol::PROTOCOL_VERSION;
    let compatible = hello
        .hello
        .as_ref()
        .and_then(|h| h.protocol_version)
        .is_some_and(|v| v.major == ours.major);
    write_message(
        &mut stream,
        &ExecFrame {
            body: Some(Body::HelloAck(HelloAck {
                protocol_version: Some(ours),
                compatible,
                upgrade_required: !compatible,
                reason: if compatible {
                    String::new()
                } else {
                    "protocol major mismatch".into()
                },
                supported_command_types: vec![],
                // The broker's client is the Core; it negotiates no
                // ProtocolCapabilitySet (REQ-EV-0043 is the surface's).
                client_capabilities: vec![],
            })),
        },
    )
    .await?;
    if !compatible {
        return Ok(());
    }
    broker.clients.fetch_add(1, Ordering::SeqCst);
    let _guard = ClientGuard(Arc::clone(&broker));

    // One bounded outbound queue per connection; forwarders push, one task writes.
    let (tx, mut rx) = mpsc::channel::<ExecFrame>(256);
    let (mut reader, mut writer) = tokio::io::split(stream);
    let writer_task = tokio::spawn(async move {
        while let Some(f) = rx.recv().await {
            if write_message(&mut writer, &f).await.is_err() {
                break;
            }
        }
    });
    // PX-099: this connection's number (the input lease is held by a
    // connection), the acknowledgement channel of each windowed attachment it
    // made, and the sessions whose lease it holds (released when it ends).
    let conn_id = CONNECTIONS.fetch_add(1, Ordering::SeqCst);
    let mut acks: HashMap<String, (Arc<Session>, watch::Sender<u64>)> = HashMap::new();
    let mut leased: Vec<Arc<Session>> = Vec::new();
    loop {
        let frame = match read_message::<_, ExecFrame>(&mut reader).await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(FrameError::Io(_)) => break,
            Err(e) => {
                let _ = tx
                    .send(err_frame("", "MALFORMED_FRAME", e.to_string()))
                    .await;
                break;
            }
        };
        match frame.body {
            Some(Body::Exec(req)) => {
                let rid = req.request_id.clone();
                match broker.start(req).await {
                    Ok((session, replayed)) => {
                        let _ = tx
                            .send(ExecFrame {
                                body: Some(Body::Started(ExecStarted {
                                    request_id: rid,
                                    session_id: session.id.clone(),
                                    process_id: 0,
                                    replayed,
                                })),
                            })
                            .await;
                        let g = session.generation.load(Ordering::SeqCst);
                        spawn_forwarder(Arc::clone(&session), 0, tx.clone(), g, None);
                    }
                    Err(e) => {
                        let text = e.to_string();
                        let code = if text.starts_with("SANDBOX_UNAVAILABLE") {
                            "SANDBOX_UNAVAILABLE"
                        } else if text.starts_with("SESSION_NOT_OWNED") {
                            "SESSION_NOT_OWNED"
                        } else {
                            "EXEC_FAILED"
                        };
                        let _ = tx.send(err_frame(&rid, code, text)).await;
                    }
                }
            }
            Some(Body::Attach(Attach {
                session_id,
                after_cursor,
                generation,
                requester,
                window_bytes,
                stall_ms,
                strict_cursor,
            })) => {
                match broker.sessions.lock().await.get(&session_id).cloned() {
                    Some(s) if !s.permits(&requester) => {
                        let _ = tx.send(not_owned(&session_id, &requester)).await;
                    }
                    Some(s) => {
                        // Terminal replay generation (docs/13): an older reader is
                        // refused; a newer one takes over and older attachments end.
                        let current = s.generation.load(Ordering::SeqCst);
                        let head = s.data_bytes.load(Ordering::SeqCst);
                        if generation != 0 && generation < current {
                            let _ = tx
                            .send(err_frame(
                                &session_id,
                                "STALE_GENERATION",
                                format!("attach generation {generation} is older than the session's {current}"),
                            ))
                            .await;
                        } else if strict_cursor && after_cursor > head {
                            // A cursor past the head names output that does not
                            // exist yet: the client's cursor is wrong, and waiting
                            // for it would hide that (PX-043).
                            let _ = tx
                            .send(err_frame(
                                &session_id,
                                "CURSOR_BEYOND_HEAD",
                                format!("cursor {after_cursor} is beyond the output head; head={head}"),
                            ))
                            .await;
                        } else {
                            if generation > current {
                                s.generation.store(generation, Ordering::SeqCst);
                                s.write_meta().await;
                                // Wake older forwarders so they notice and end.
                                s.written.send_modify(|_| {});
                            }
                            let mine = s.generation.load(Ordering::SeqCst);
                            let flow = if window_bytes > 0 {
                                let (ack_tx, ack_rx) = watch::channel(after_cursor);
                                // A second attach to the same session on this
                                // connection replaces the first's channel; its
                                // forwarder ends when the sender is dropped.
                                acks.insert(session_id.clone(), (Arc::clone(&s), ack_tx));
                                Some(Flow {
                                    window: window_bytes,
                                    stall: Duration::from_millis(if stall_ms == 0 {
                                        DEFAULT_STALL_MS
                                    } else {
                                        stall_ms
                                    }),
                                    acked: ack_rx,
                                })
                            } else {
                                None
                            };
                            spawn_forwarder(s, after_cursor, tx.clone(), mine, flow);
                        }
                    }
                    None => {
                        let _ = tx.send(err_frame("", "UNKNOWN_SESSION", session_id)).await;
                    }
                }
            }
            Some(Body::Ack(TerminalAck { session_id, cursor })) => {
                // The client consumed output up to `cursor`: the window
                // opens by that much. An acknowledgement beyond what exists
                // is clamped to the head, so a client cannot widen its own
                // window past the bytes there are.
                if let Some((s, ack_tx)) = acks.get(&session_id) {
                    let cursor = cursor.min(s.data_bytes.load(Ordering::SeqCst));
                    ack_tx.send_if_modified(|c| {
                        if cursor > *c {
                            *c = cursor;
                            true
                        } else {
                            false
                        }
                    });
                }
            }
            Some(Body::Stdin(WriteStdin {
                session_id,
                data,
                requester,
                want_ack,
                as_user,
            })) => {
                let found = broker.sessions.lock().await.get(&session_id).cloned();
                let Some(s) = found else {
                    let _ = tx.send(err_frame("", "UNKNOWN_SESSION", session_id)).await;
                    continue;
                };
                if !s.permits(&requester) {
                    let _ = tx.send(not_owned(&s.id, &requester)).await;
                    continue;
                }
                if data.len() > MAX_STDIN_BYTES {
                    let _ = tx
                        .send(err_frame(
                            &session_id,
                            "INPUT_TOO_LARGE",
                            format!(
                                "{} bytes exceed the {MAX_STDIN_BYTES}-byte bound of one write; nothing was written",
                                data.len()
                            ),
                        ))
                        .await;
                    continue;
                }
                // The owner lease (PX-099): a person's keystrokes only from
                // the connection that holds it; the agent's never while it
                // is held.
                let holder = s
                    .lease
                    .lock()
                    .expect("lease")
                    .as_ref()
                    .map(|l| (l.conn, l.holder.clone()));
                if as_user {
                    if !requester.is_empty() || holder.as_ref().is_none_or(|(c, _)| *c != conn_id) {
                        let _ = tx
                            .send(err_frame(
                                &session_id,
                                "LEASE_REQUIRED",
                                "user input is accepted only from the connection that holds the terminal's input lease",
                            ))
                            .await;
                        continue;
                    }
                } else if let Some((_, who)) = holder {
                    let _ = tx
                        .send(err_frame(
                            &session_id,
                            "INPUT_LEASED",
                            format!(
                                "the terminal's input lease is held by {who}; nothing was written"
                            ),
                        ))
                        .await;
                    continue;
                }
                if !s.running.load(Ordering::SeqCst) {
                    let _ = tx
                        .send(err_frame(
                            &session_id,
                            "SESSION_FINISHED",
                            "the session's process has ended; nothing was written",
                        ))
                        .await;
                    continue;
                }
                let mut guard = s.stdin.lock().await;
                let result = match &mut *guard {
                    Stdin::Pipe(w) => {
                        use tokio::io::AsyncWriteExt;
                        w.write_all(&data)
                            .await
                            .and(w.flush().await)
                            .map_err(|e| e.to_string())
                    }
                    Stdin::Pty(w) => w
                        .write_all(&data)
                        .and_then(|()| w.flush())
                        .map_err(|e| e.to_string()),
                    Stdin::Closed => Err("stdin is closed".into()),
                };
                drop(guard);
                match result {
                    Err(e) => {
                        let _ = tx.send(err_frame(&session_id, "STDIN_FAILED", e)).await;
                    }
                    Ok(()) if want_ack => {
                        let _ = tx
                            .send(ExecFrame {
                                body: Some(Body::StdinWritten(StdinWritten {
                                    session_id: session_id.clone(),
                                    bytes: data.len() as u64,
                                    cursor: s.data_bytes.load(Ordering::SeqCst),
                                })),
                            })
                            .await;
                    }
                    Ok(()) => {}
                }
            }
            Some(Body::Resize(TerminalResize {
                session_id,
                rows,
                cols,
                requester,
            })) => {
                let found = broker.sessions.lock().await.get(&session_id).cloned();
                let Some(s) = found else {
                    let _ = tx.send(err_frame("", "UNKNOWN_SESSION", session_id)).await;
                    continue;
                };
                let reply = if !s.permits(&requester) {
                    not_owned(&s.id, &requester)
                } else if !(1..=MAX_PTY_ROWS).contains(&rows) || !(1..=MAX_PTY_COLS).contains(&cols)
                {
                    err_frame(
                        &session_id,
                        "BAD_SIZE",
                        format!(
                            "{rows}x{cols} is outside 1..={MAX_PTY_ROWS} rows by 1..={MAX_PTY_COLS} columns"
                        ),
                    )
                } else if !s.pty.load(Ordering::SeqCst) {
                    err_frame(
                        &session_id,
                        "NOT_A_PTY",
                        "the session runs on pipes, not a PTY",
                    )
                } else {
                    match s.resize(rows, cols) {
                        Ok(()) => ExecFrame {
                            body: Some(Body::Resized(TerminalResized {
                                session_id: session_id.clone(),
                                rows,
                                cols,
                            })),
                        },
                        Err(why) => err_frame(&session_id, "RESIZE_FAILED", why),
                    }
                };
                let _ = tx.send(reply).await;
            }
            Some(Body::AcquireLease(AcquireTerminalLease {
                session_id,
                requester,
                holder,
                steal,
            })) => {
                let found = broker.sessions.lock().await.get(&session_id).cloned();
                let Some(s) = found else {
                    let _ = tx.send(err_frame("", "UNKNOWN_SESSION", session_id)).await;
                    continue;
                };
                let reply = if !s.permits(&requester) {
                    not_owned(&s.id, &requester)
                } else if !requester.is_empty() {
                    // The lease is a person's; only the host speaks for one.
                    err_frame(
                        &session_id,
                        "LEASE_NOT_PERMITTED",
                        "a task cannot take the person's input lease",
                    )
                } else if !s.running.load(Ordering::SeqCst) {
                    err_frame(
                        &session_id,
                        "SESSION_FINISHED",
                        "the session's process has ended",
                    )
                } else {
                    let who = if holder.is_empty() {
                        format!("user:conn-{conn_id}")
                    } else {
                        holder
                    };
                    let mut lease = s.lease.lock().expect("lease");
                    match lease.as_ref() {
                        Some(l) if l.conn != conn_id && !steal => err_frame(
                            &session_id,
                            "LEASE_HELD",
                            format!("the terminal's input lease is held by {}", l.holder),
                        ),
                        held => {
                            if let Some(old) = held.filter(|l| l.conn != conn_id) {
                                // Taken from another person: tell them.
                                let _ = old.notify.try_send(ExecFrame {
                                    body: Some(Body::LeaseState(TerminalLeaseState {
                                        session_id: session_id.clone(),
                                        held: false,
                                        holder: who.clone(),
                                    })),
                                });
                            }
                            *lease = Some(InputLease {
                                conn: conn_id,
                                holder: who.clone(),
                                notify: tx.clone(),
                            });
                            drop(lease);
                            if !leased.iter().any(|l| l.id == s.id) {
                                leased.push(Arc::clone(&s));
                            }
                            ExecFrame {
                                body: Some(Body::LeaseState(TerminalLeaseState {
                                    session_id: session_id.clone(),
                                    held: true,
                                    holder: who,
                                })),
                            }
                        }
                    }
                };
                let _ = tx.send(reply).await;
            }
            Some(Body::ReleaseLease(ReleaseTerminalLease {
                session_id,
                requester,
            })) => {
                let found = broker.sessions.lock().await.get(&session_id).cloned();
                let Some(s) = found else {
                    let _ = tx.send(err_frame("", "UNKNOWN_SESSION", session_id)).await;
                    continue;
                };
                if !s.permits(&requester) {
                    let _ = tx.send(not_owned(&s.id, &requester)).await;
                    continue;
                }
                s.clear_lease(Some(conn_id));
                let holder = s
                    .lease
                    .lock()
                    .expect("lease")
                    .as_ref()
                    .map(|l| l.holder.clone())
                    .unwrap_or_default();
                let _ = tx
                    .send(ExecFrame {
                        body: Some(Body::LeaseState(TerminalLeaseState {
                            session_id,
                            held: false,
                            holder,
                        })),
                    })
                    .await;
            }
            Some(Body::Cancel(Cancel {
                session_id,
                requester,
            })) => {
                let found = broker.sessions.lock().await.get(&session_id).cloned();
                if let Some(s) = found.as_ref().filter(|s| !s.permits(&requester)) {
                    let _ = tx.send(not_owned(&s.id, &requester)).await;
                } else if let Some(s) = found {
                    s.cancelled.store(true, Ordering::SeqCst);
                    kill(&s).await;
                } else {
                    let _ = tx.send(err_frame("", "UNKNOWN_SESSION", session_id)).await;
                }
            }
            Some(Body::ProbeSandbox(_)) => {
                let kind = review_sandbox::available();
                let _ = tx
                    .send(ExecFrame {
                        body: Some(Body::SandboxProbed(modbit_protocol::v1::SandboxProbed {
                            available: kind.is_some(),
                            kind: kind.unwrap_or_default().to_owned(),
                            detail: match kind {
                                Some("seatbelt") => "macOS sandbox-exec: no network; writes only under the worktree and the temp dirs".into(),
                                Some("seccomp-net") => "Linux seccomp filter: no network; writes are the lease's to confine".into(),
                                _ => "no review sandbox on this host (macOS sandbox-exec or Linux seccomp required)".into(),
                            },
                        })),
                    })
                    .await;
            }
            Some(Body::List(ListSessions { requester })) => {
                let sessions = broker.sessions.lock().await;
                let mut list: Vec<SessionInfo> = Vec::new();
                // A task sees the sessions it owns; the host sees them all.
                for s in sessions.values().filter(|s| s.permits(&requester)) {
                    list.push(s.info().await);
                }
                list.sort_by(|a, b| a.session_id.cmp(&b.session_id));
                let _ = tx
                    .send(ExecFrame {
                        body: Some(Body::Sessions(SessionList { sessions: list })),
                    })
                    .await;
            }
            other => {
                let _ = tx
                    .send(err_frame("", "UNEXPECTED_FRAME", format!("{other:?}")))
                    .await;
                break;
            }
        }
    }
    // The connection is gone: so is every lease it held. The processes it
    // watched keep running (docs/33: a lost transport never ends one).
    for s in &leased {
        s.clear_lease(Some(conn_id));
    }
    drop(acks);
    drop(tx);
    let _ = writer_task.await;
    Ok(())
}

async fn kill(s: &Arc<Session>) {
    let mut k = s.killer.lock().await;
    match k.take() {
        Some(Killer::Child(child)) => {
            if let Some(c) = child.lock().await.as_mut() {
                // Kill the whole tree, or a grandchild keeps the output
                // pipes open and the exit is never observed (REQ-EV-0221
                // cancel): Windows by job-less tree kill, Unix by the
                // process group the child was started in.
                #[cfg(windows)]
                if let Some(pid) = c.id() {
                    let _ = tokio::process::Command::new("taskkill")
                        .args(["/T", "/F", "/PID", &pid.to_string()])
                        .output()
                        .await;
                }
                #[cfg(unix)]
                if let Some(pid) = c.id() {
                    kill_group(pid);
                }
                let _ = c.start_kill();
            }
        }
        Some(Killer::Pty(child)) => {
            if let Some(c) = child.lock().expect("pty child").as_mut() {
                // The PTY made the child a session leader: its group is its pid.
                #[cfg(unix)]
                if let Some(pid) = c.process_id() {
                    kill_group(pid);
                }
                let _ = c.kill();
            }
        }
        None => {}
    }
}

/// SIGKILL every process in the group a child was started in (Unix).
// SAFETY exception (workspace `unsafe_code = "deny"`, noted in Cargo.toml):
// one killpg call on a process group this broker created.
#[cfg(unix)]
#[allow(unsafe_code)]
fn kill_group(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    unsafe {
        libc::killpg(pid, libc::SIGKILL);
    }
}

/// Replay the log from `after_cursor` then follow live output until exit,
/// or until a newer attach generation supersedes this one. A cursor older
/// than the replay window ends the attachment with `CURSOR_EXPIRED` (the
/// oldest readable cursor is in the message); output is read through the
/// index one record piece at a time, so memory per attachment is one chunk.
///
/// With a [`Flow`] (PX-099) the attachment is windowed: at most `window`
/// bytes are pushed beyond the last acknowledgement. The rest stays in the
/// session's log on disk — nothing is buffered for a slow client — and the
/// forwarder waits for an `Ack`; with none for `stall` it ends the
/// attachment `ATTACH_STALLED`, naming the cursor to resume from, so the
/// client drops to cursor-pull instead of the broker holding it.
fn spawn_forwarder(
    s: Arc<Session>,
    after_cursor: u64,
    tx: mpsc::Sender<ExecFrame>,
    generation: u64,
    mut flow: Option<Flow>,
) {
    tokio::spawn(async move {
        let mut cursor = after_cursor;
        let mut rx = s.written.subscribe();
        loop {
            if s.generation.load(Ordering::SeqCst) > generation {
                let _ = tx
                    .send(err_frame(
                        &s.id,
                        "STALE_GENERATION",
                        "a newer reader attached; this attachment ended",
                    ))
                    .await;
                return;
            }
            let high = *rx.borrow_and_update();
            while cursor < high {
                if let Some(f) = flow.as_mut() {
                    loop {
                        let acked = *f.acked.borrow_and_update();
                        if cursor.saturating_sub(acked) < f.window {
                            break;
                        }
                        match tokio::time::timeout(f.stall, f.acked.changed()).await {
                            Ok(Ok(())) => {}
                            // The connection (and its acknowledgements) is gone.
                            Ok(Err(_)) => return,
                            Err(_) => {
                                let _ = tx
                                    .send(err_frame(
                                        &s.id,
                                        "ATTACH_STALLED",
                                        format!(
                                            "no acknowledgement for {} ms with {} bytes unacknowledged; resume_cursor={acked}",
                                            f.stall.as_millis(),
                                            cursor.saturating_sub(acked)
                                        ),
                                    ))
                                    .await;
                                return;
                            }
                        }
                        if s.generation.load(Ordering::SeqCst) > generation {
                            break;
                        }
                    }
                    if s.generation.load(Ordering::SeqCst) > generation {
                        break;
                    }
                }
                let read = {
                    let s = Arc::clone(&s);
                    tokio::task::spawn_blocking(move || s.read_chunk(cursor, high)).await
                };
                match read {
                    Ok(Ok(ReadOutcome::Chunk { tag, data })) => {
                        let len = data.len() as u64;
                        let frame = ExecFrame {
                            body: Some(Body::Output(OutputChunk {
                                session_id: s.id.clone(),
                                cursor,
                                stream: tag_name(tag).into(),
                                data,
                            })),
                        };
                        if tx.send(frame).await.is_err() {
                            return;
                        }
                        cursor += len;
                    }
                    Ok(Ok(ReadOutcome::Expired { oldest })) => {
                        let _ = tx
                            .send(err_frame(
                                &s.id,
                                "CURSOR_EXPIRED",
                                format!(
                                    "cursor {cursor} is older than the replay window; oldest_cursor={oldest}"
                                ),
                            ))
                            .await;
                        return;
                    }
                    Ok(Ok(ReadOutcome::End)) | Ok(Err(_)) | Err(_) => {
                        // A segment vanished mid-read (the session is being
                        // pruned, or the window moved on): report where the
                        // window is now rather than ending silently.
                        let oldest = s.oldest_cursor();
                        let _ = tx
                            .send(err_frame(
                                &s.id,
                                "CURSOR_EXPIRED",
                                format!(
                                    "cursor {cursor} is no longer readable; oldest_cursor={oldest}"
                                ),
                            ))
                            .await;
                        return;
                    }
                }
            }
            if !s.running.load(Ordering::SeqCst) && cursor >= *s.written.borrow() {
                if let Some(e) = s.exited.lock().await.clone() {
                    let _ = tx
                        .send(ExecFrame {
                            body: Some(Body::Exited(e)),
                        })
                        .await;
                }
                return;
            }
            if rx.changed().await.is_err() {
                return;
            }
        }
    });
}

/// The flow control of one windowed attachment (PX-099).
struct Flow {
    window: u64,
    stall: Duration,
    /// The cursor the client has acknowledged consuming.
    acked: watch::Receiver<u64>,
}

impl Session {
    /// Apply a new size to the PTY (PX-099): the kernel tells the child's
    /// foreground process group (SIGWINCH on Unix), and what the child reads
    /// afterwards is the new size.
    fn resize(&self, rows: u32, cols: u32) -> std::result::Result<(), String> {
        let master = self.pty_master.lock().expect("pty master");
        let Some(m) = master.as_ref() else {
            return Err("the PTY is closed (its process has ended)".into());
        };
        m.resize(portable_pty::PtySize {
            rows: u16::try_from(rows).unwrap_or(u16::MAX),
            cols: u16::try_from(cols).unwrap_or(u16::MAX),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
        self.rows.store(rows, Ordering::SeqCst);
        self.cols.store(cols, Ordering::SeqCst);
        Ok(())
    }
}

impl Broker {
    /// Start (or replay) a request.
    async fn start(self: &Arc<Self>, req: ExecRequest) -> Result<(Arc<Session>, bool)> {
        if req.request_id.is_empty() {
            anyhow::bail!("request_id is required");
        }
        let existing = self.by_request.lock().await.get(&req.request_id).cloned();
        if let Some(id) = existing
            && let Some(s) = self.sessions.lock().await.get(&id).cloned()
        {
            // A retry replays the session, but only for its owner: a request
            // id is no handle on someone else's process.
            if !s.permits(&req.owner) {
                anyhow::bail!(
                    "SESSION_NOT_OWNED: request `{}` belongs to another owner",
                    req.request_id
                );
            }
            return Ok((s, true));
        }
        if req.argv.is_empty() {
            anyhow::bail!("argv is required");
        }
        let cwd = if req.cwd.is_empty() {
            std::env::current_dir()?
        } else {
            PathBuf::from(&req.cwd)
        };
        if !cwd.is_dir() {
            anyhow::bail!("cwd `{}` is not a directory", cwd.display());
        }
        // EPR-018 (docs/27 §9.5, docs/21 `review_isolated`): a review
        // process runs inside the disposable review environment — no
        // network, no inherited environment, no secret-looking variable,
        // writes confined to its worktree where the host can confine them —
        // or not at all: an unsupported host admits no review process.
        let req = if req.execution_profile == "review_isolated" {
            review_sandbox::wrap(req, &cwd)?
        } else {
            req
        };
        let id = encode_hex(&(0..8).map(|_| rand::random::<u8>()).collect::<Vec<_>>());
        let log_dir = self.data_dir.join("sessions");
        let meta_path = log_dir.join(format!("{id}.json"));
        let log = SegLog::create(
            &log_dir,
            &id,
            self.limits.replay_window_bytes,
            self.limits.segment_bytes,
        )?;
        let (written, _) = watch::channel(0u64);
        let session = Arc::new(Session {
            id: id.clone(),
            request_id: req.request_id.clone(),
            argv: req.argv.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            owner: req.owner.clone(),
            written,
            running: AtomicBool::new(true),
            exited: Mutex::new(None),
            stdin: Mutex::new(Stdin::Closed),
            killer: Mutex::new(None),
            cancelled: AtomicBool::new(false),
            data_bytes: AtomicU64::new(0),
            log: std::sync::Mutex::new(log),
            log_dir,
            meta_path,
            started_at_ms: now_ms(),
            generation: AtomicU64::new(0),
            lost: AtomicBool::new(false),
            pty_master: std::sync::Mutex::new(None),
            pty: AtomicBool::new(req.pty),
            rows: AtomicU32::new(if req.pty {
                nonzero(req.pty_rows, DEFAULT_PTY_ROWS)
            } else {
                0
            }),
            cols: AtomicU32::new(if req.pty {
                nonzero(req.pty_cols, DEFAULT_PTY_COLS)
            } else {
                0
            }),
            lease: std::sync::Mutex::new(None),
            replay_window_bytes: self.limits.replay_window_bytes,
        });
        session.write_meta().await;
        self.sessions
            .lock()
            .await
            .insert(id.clone(), Arc::clone(&session));
        self.by_request
            .lock()
            .await
            .insert(req.request_id.clone(), id.clone());
        let started = Instant::now();
        let stdin_open = req.stdin_mode == "open";
        if req.pty {
            self.spawn_pty(Arc::clone(&session), &req, &cwd, started)
                .await?;
        } else {
            self.spawn_piped(Arc::clone(&session), &req, &cwd, stdin_open, started)
                .await?;
        }
        Ok((session, false))
    }

    async fn spawn_piped(
        self: &Arc<Self>,
        s: Arc<Session>,
        req: &ExecRequest,
        cwd: &Path,
        stdin_open: bool,
        started: Instant,
    ) -> Result<()> {
        if let Some(why) = leaks_own_secret(&req.env) {
            anyhow::bail!(
                "SECRET_IN_ENV: {why}; a command never carries a credential of this broker"
            );
        }
        let mut cmd = tokio::process::Command::new(&req.argv[0]);
        cmd.args(&req.argv[1..])
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        cmd.stdin(if stdin_open {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        });
        if !req.inherit_env {
            cmd.env_clear();
            // A minimal PATH keeps argv resolvable when the caller gave none.
            if !req.env.contains_key("PATH")
                && let Ok(p) = std::env::var("PATH")
            {
                cmd.env("PATH", p);
            }
            #[cfg(windows)]
            for k in ["SYSTEMROOT", "SystemRoot", "COMSPEC", "TEMP", "TMP"] {
                if let Ok(v) = std::env::var(k) {
                    cmd.env(k, v);
                }
            }
        } else {
            // Inherited or not, no credential of this process reaches a child.
            for (k, _) in std::env::vars() {
                if secret_bearing(&k) {
                    cmd.env_remove(&k);
                }
            }
        }
        cmd.envs(req.env.iter().filter(|(k, _)| !secret_bearing(k)));
        cmd.kill_on_drop(true);
        // Its own process group, so a cancel ends what the command started
        // too (a shell's child, a test runner's workers): a grandchild left
        // alive would hold the output pipes and the exit would never be
        // observed (REQ-EV-0221; EPR-018 disposal).
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawning {:?}", req.argv))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        if let Some(stdin) = child.stdin.take() {
            *s.stdin.lock().await = Stdin::Pipe(stdin);
        }
        let child = Arc::new(Mutex::new(Some(child)));
        *s.killer.lock().await = Some(Killer::Child(Arc::clone(&child)));
        let pump = |s: Arc<Session>,
                    tag: u8,
                    mut r: Option<tokio::process::ChildStdout>,
                    mut e: Option<tokio::process::ChildStderr>| async move {
            use tokio::io::AsyncReadExt;
            let mut buf = vec![0u8; CHUNK];
            loop {
                let n = match (r.as_mut(), e.as_mut()) {
                    (Some(r), _) => r.read(&mut buf).await,
                    (_, Some(e)) => e.read(&mut buf).await,
                    _ => Ok(0),
                };
                match n {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        s.append(tag, &buf[..n]);
                    }
                }
            }
        };
        let p1 = tokio::spawn(pump(Arc::clone(&s), TAG_STDOUT, stdout, None));
        let p2 = tokio::spawn(pump(Arc::clone(&s), TAG_STDERR, None, stderr));
        let timeout = req.timeout_ms;
        let broker = Arc::clone(self);
        tokio::spawn(async move {
            let mut timed_out = false;
            // Poll without holding the child lock across the wait, so Cancel
            // can take the lock and kill the process at any time.
            let wait = async {
                loop {
                    let done = child
                        .lock()
                        .await
                        .as_mut()
                        .and_then(|c| c.try_wait().ok().flatten());
                    if let Some(st) = done {
                        return Some(st);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                }
            };
            let status = if timeout > 0 {
                match tokio::time::timeout(std::time::Duration::from_millis(timeout), wait).await {
                    Ok(st) => st,
                    Err(_) => {
                        timed_out = true;
                        if let Some(c) = child.lock().await.as_mut() {
                            let _ = c.kill().await;
                        }
                        None
                    }
                }
            } else {
                wait.await
            };
            // After a kill (cancel or timeout) a surviving grandchild may hold the
            // pipes: bound the reader wait so the exit is still reported.
            let abrupt = timed_out || s.cancelled.load(Ordering::SeqCst);
            if abrupt {
                let grace = std::time::Duration::from_secs(2);
                let _ = tokio::time::timeout(grace, p1).await;
                let _ = tokio::time::timeout(grace, p2).await;
            } else {
                let _ = p1.await;
                let _ = p2.await;
            }
            #[cfg(unix)]
            let signal = {
                use std::os::unix::process::ExitStatusExt;
                status.as_ref().and_then(|st| st.signal())
            };
            #[cfg(not(unix))]
            let signal: Option<i32> = None;
            broker
                .finish(
                    &s,
                    status.and_then(|st| st.code()),
                    signal,
                    timed_out,
                    started,
                )
                .await;
        });
        Ok(())
    }

    async fn spawn_pty(
        self: &Arc<Self>,
        s: Arc<Session>,
        req: &ExecRequest,
        cwd: &Path,
        started: Instant,
    ) -> Result<()> {
        if let Some(why) = leaks_own_secret(&req.env) {
            anyhow::bail!(
                "SECRET_IN_ENV: {why}; a command never carries a credential of this broker"
            );
        }
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: u16::try_from(nonzero(req.pty_rows, DEFAULT_PTY_ROWS)).unwrap_or(u16::MAX),
                cols: u16::try_from(nonzero(req.pty_cols, DEFAULT_PTY_COLS)).unwrap_or(u16::MAX),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow::anyhow!("openpty: {e}"))?;
        let mut cmd = CommandBuilder::new(&req.argv[0]);
        cmd.args(&req.argv[1..]);
        cmd.cwd(cwd);
        if !req.inherit_env {
            cmd.env_clear();
            if !req.env.contains_key("PATH")
                && let Ok(p) = std::env::var("PATH")
            {
                cmd.env("PATH", p);
            }
            #[cfg(windows)]
            for k in ["SYSTEMROOT", "SystemRoot", "COMSPEC", "TEMP", "TMP"] {
                if let Ok(v) = std::env::var(k) {
                    cmd.env(k, v);
                }
            }
        } else {
            // Inherited or not, no credential of this process reaches a child.
            for (k, _) in std::env::vars() {
                if secret_bearing(&k) {
                    cmd.env_remove(&k);
                }
            }
        }
        for (k, v) in req.env.iter().filter(|(k, _)| !secret_bearing(k)) {
            cmd.env(k, v);
        }
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| anyhow::anyhow!("spawn pty: {e}"))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| anyhow::anyhow!("pty reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| anyhow::anyhow!("pty writer: {e}"))?;
        *s.stdin.lock().await = Stdin::Pty(writer);
        let child = Arc::new(std::sync::Mutex::new(Some(child)));
        *s.killer.lock().await = Some(Killer::Pty(Arc::clone(&child)));
        let s_read = Arc::clone(&s);
        let reader_task = tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; CHUNK];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        s_read.append(TAG_PTY, &buf[..n]);
                        // A Windows pseudo-console asks its terminal where the
                        // cursor is (DSR, `ESC [ 6 n`) and holds the child's
                        // input until it hears back. The broker is that
                        // terminal: it answers, top-left (PX-030 found the
                        // hang).
                        #[cfg(windows)]
                        if buf[..n].windows(4).any(|w| w == b"\x1b[6n")
                            && let Stdin::Pty(w) = &mut *s_read.stdin.blocking_lock()
                        {
                            let _ = w.write_all(b"\x1b[1;1R").and_then(|()| w.flush());
                        }
                    }
                }
            }
        });
        // Kept for `Resize`; released when the process ends (below).
        *s.pty_master.lock().expect("pty master") = Some(pair.master);
        let timeout = req.timeout_ms;
        let broker = Arc::clone(self);
        tokio::spawn(async move {
            let child_wait = Arc::clone(&child);
            let wait = tokio::task::spawn_blocking(move || {
                loop {
                    let done = child_wait
                        .lock()
                        .expect("pty child")
                        .as_mut()
                        .and_then(|c| c.try_wait().ok().flatten());
                    if let Some(st) = done {
                        return Some(st);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
            });
            let mut timed_out = false;
            let status = if timeout > 0 {
                match tokio::time::timeout(std::time::Duration::from_millis(timeout), wait).await {
                    Ok(st) => st.ok().flatten(),
                    Err(_) => {
                        timed_out = true;
                        if let Some(c) = child.lock().expect("pty child").as_mut() {
                            let _ = c.kill();
                        }
                        None
                    }
                }
            } else {
                wait.await.ok().flatten()
            };
            // The process is gone: release every handle on the terminal so
            // its output reaches end-of-file. On Windows the pseudo-console
            // stays open while any end of it — the stdin writer included —
            // is held, and the reader would wait forever (PX-030 found it).
            *s.stdin.lock().await = Stdin::Closed;
            drop(s.pty_master.lock().expect("pty master").take());
            // Output written after the exit is drained if it arrives at
            // once; a reader that never sees end-of-file does not hold the
            // exit record back.
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), reader_task).await;
            let code = status.map(|st| st.exit_code() as i32);
            broker.finish(&s, code, None, timed_out, started).await;
        });
        Ok(())
    }

    /// Finalize: seal the retained output into the content-addressed object
    /// store (the Core's, when it started this broker) and record the exit.
    /// The object is the output the log still holds; `retained_from` is the
    /// cursor of its first byte, 0 when the window never dropped any.
    async fn finish(
        &self,
        s: &Arc<Session>,
        exit_code: Option<i32>,
        signal: Option<i32>,
        timed_out: bool,
        started: Instant,
    ) {
        let (segments, total) = s.log.lock().expect("log").snapshot();
        let sealed = {
            let (dir, id, root) = (s.log_dir.clone(), s.id.clone(), self.object_dir.clone());
            tokio::task::spawn_blocking(move || seglog::seal(&dir, &id, &segments, &root))
                .await
                .map_err(|e| std::io::Error::other(e.to_string()))
                .and_then(|r| r)
        };
        if let Err(e) = &sealed {
            eprintln!(
                "modbit-execd: session {}: sealing its output failed: {e}",
                s.id
            );
        }
        let sealed = sealed.ok();
        let exited = ProcessExited {
            session_id: s.id.clone(),
            exit_code,
            signal,
            duration_ms: started.elapsed().as_millis() as u64,
            output_ref: sealed.as_ref().map(|x| x.hash.clone()).unwrap_or_default(),
            total_bytes: total,
            timed_out,
            cancelled: s.cancelled.load(Ordering::SeqCst),
            retained_from: sealed.map_or(0, |x| x.retained_from),
        };
        *s.exited.lock().await = Some(exited);
        *s.stdin.lock().await = Stdin::Closed;
        s.clear_lease(None);
        s.running.store(false, Ordering::SeqCst);
        s.write_meta().await;
        // Wake forwarders so they deliver the exit.
        s.written.send_modify(|_| {});
        self.prune_finished().await;
    }

    /// Drop finished sessions beyond the retention policy: older than
    /// `retain_age`, then oldest first past `retain_sessions`. Their logs and
    /// metadata go; the sealed objects are the object store's to keep.
    async fn prune_finished(&self) {
        let mut finished: Vec<(i64, Arc<Session>)> = Vec::new();
        {
            let sessions = self.sessions.lock().await;
            for s in sessions.values() {
                if s.running.load(Ordering::SeqCst) {
                    continue;
                }
                let ended = match s.exited.lock().await.as_ref() {
                    Some(x) => s.started_at_ms + x.duration_ms as i64,
                    None => continue,
                };
                finished.push((ended, Arc::clone(s)));
            }
        }
        finished.sort_by_key(|(ended, _)| *ended);
        let now = now_ms();
        let age_ms = self.limits.retain_age.map(|a| a.as_millis() as i64);
        let excess = finished.len().saturating_sub(self.limits.retain_sessions);
        let victims: Vec<Arc<Session>> = finished
            .into_iter()
            .enumerate()
            .filter(|(i, (ended, _))| *i < excess || age_ms.is_some_and(|a| now - ended > a))
            .map(|(_, (_, s))| s)
            .collect();
        if victims.is_empty() {
            return;
        }
        {
            let mut sessions = self.sessions.lock().await;
            for v in &victims {
                sessions.remove(&v.id);
            }
        }
        {
            let mut by_request = self.by_request.lock().await;
            by_request.retain(|_, id| !victims.iter().any(|v| &v.id == id));
        }
        for v in victims {
            v.log.lock().expect("log").remove_all();
            let _ = std::fs::remove_file(&v.meta_path);
        }
    }
}

/// EPR-018: the host's review sandbox — a real OS confinement or none.
pub(crate) mod review_sandbox {
    use std::path::Path;
    use std::sync::OnceLock;

    use modbit_protocol::v1::ExecRequest;

    /// What this host can confine a review process with: `seatbelt`
    /// (macOS `sandbox-exec`: no network, writes only under the worktree
    /// and the temp dirs), `seccomp-net` (Linux seccomp filter installed by
    /// this broker re-executing itself as the launcher: no network; writes
    /// are the lease's to confine), or nothing.
    pub fn available() -> Option<&'static str> {
        static PROBE: OnceLock<Option<&'static str>> = OnceLock::new();
        *PROBE.get_or_init(probe)
    }

    fn probe() -> Option<&'static str> {
        if std::env::var_os("MODBIT_REVIEW_SANDBOX_DISABLE").is_some() {
            return None;
        }
        #[cfg(target_os = "macos")]
        {
            let ok = std::process::Command::new("/usr/bin/sandbox-exec")
                .args([
                    "-p",
                    "(version 1)(allow default)(deny network*)",
                    "/usr/bin/true",
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            return ok.then_some("seatbelt");
        }
        #[cfg(target_os = "linux")]
        {
            // The launcher is this binary; the check is a process it started
            // failing to open an inet socket.
            let Ok(exe) = std::env::current_exe() else {
                return None;
            };
            let ok = std::process::Command::new(&exe)
                .arg("--review-sandbox-exec")
                .arg("--")
                .arg(&exe)
                .arg("--review-sandbox-selfcheck")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            return ok.then_some("seccomp-net");
        }
        #[allow(unreachable_code)]
        None
    }

    /// Whether an environment variable name looks like a credential.
    fn secret_like(key: &str) -> bool {
        let k = key.to_ascii_uppercase();
        [
            "KEY",
            "TOKEN",
            "SECRET",
            "PASSWORD",
            "PASSWD",
            "CREDENTIAL",
            "AUTH",
        ]
        .iter()
        .any(|m| k.contains(m))
            || k.starts_with("MODBIT_")
    }

    /// The request as the sandbox runs it, or why it cannot.
    pub fn wrap(req: ExecRequest, cwd: &Path) -> anyhow::Result<ExecRequest> {
        let Some(kind) = available() else {
            anyhow::bail!(
                "SANDBOX_UNAVAILABLE: this host has no review sandbox (macOS sandbox-exec or Linux seccomp); a review process cannot be admitted here"
            );
        };
        let mut req = req;
        // Deny-default environment: nothing inherited, nothing secret-like.
        req.inherit_env = false;
        req.env.retain(|k, _| !secret_like(k));
        let argv = std::mem::take(&mut req.argv);
        req.argv = match kind {
            "seatbelt" => {
                let cwd_s = cwd.to_string_lossy().replace('"', "");
                let profile = format!(
                    "(version 1)(allow default)(deny network*)(deny file-write*)(allow file-write* (subpath \"{cwd_s}\"))(allow file-write* (subpath \"/private/tmp\"))(allow file-write* (subpath \"/private/var/folders\"))(allow file-write* (subpath \"/tmp\"))(allow file-write* (subpath \"/dev\"))"
                );
                let mut v = vec!["/usr/bin/sandbox-exec".to_owned(), "-p".to_owned(), profile];
                v.extend(argv);
                v
            }
            _ => {
                let exe = std::env::current_exe().map_err(|e| {
                    anyhow::anyhow!("SANDBOX_UNAVAILABLE: locating the launcher: {e}")
                })?;
                let mut v = vec![
                    exe.to_string_lossy().into_owned(),
                    "--review-sandbox-exec".to_owned(),
                    "--".to_owned(),
                ];
                v.extend(argv);
                v
            }
        };
        Ok(req)
    }
}
