//! The Core's External Tool Hub (M9.4; REQ-EV-0104 list/call/cancel
//! lifecycle, REQ-EV-0193 workspace-scoped transport pool; docs/16 "MCP /
//! external tools", docs/17 `external.list` / `external.call` /
//! `external.cancel`).
//!
//! The hub owns everything an MCP server touches on this side: the child
//! process, its pipes, the handshake, lazy discovery, the in-flight call
//! table and the pool that decides when a second session gets the *same*
//! server rather than a second one. `crates/mcp` decides what is
//! acceptable; this file makes it real.
//!
//! **Pooling (REQ-EV-0193).** A transport is keyed by
//! `(tenant, workspace root, configuration fingerprint)`. Two sessions of
//! one tenant working in one workspace under one configuration share a
//! single server process — the second session's `external.list` reports the
//! same pid and a `sharers` count above one. A different tenant, a
//! different workspace or one changed byte of configuration is a different
//! key and therefore a different process: nothing is ever shared across a
//! tenant boundary.
//!
//! **Cancellation (docs/16, docs/54 fault 25).** Every call is registered
//! before it is sent and holds a guard: whatever ends the wait — a
//! cancellation, a timeout, a dropped task, a dead server — the guard sends
//! `notifications/cancelled` and reconciles. A read that ends without an
//! answer failed; an effectful call that ends without an answer has an
//! *unknown outcome*, and the hub says so rather than claiming the effect
//! did not happen.
//!
//! **Credentials.** A server's credential is a handle name; the value lives
//! in the credential broker (REQ-PX-130), is obtained from it for the spawn
//! — scoped to the server it is for — placed into the child's environment
//! and appears nowhere else: not in the configuration, not in a log, not in a
//! listing, not in an argument and not in a result.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use modbit_mcp::calls::{CallTable, CancelPlan, Reconciled};
use modbit_mcp::config::{ConfigError, PoolKey, ServerConfig, Trust};
use modbit_mcp::discovery::{Discovery, Limits, validate_arguments};
use modbit_mcp::port::{
    BoxFuture, Cancelled, Correlation, Described, ExternalCall, Health, LifecycleRecord, Listing,
    McpPort, PortError, ServerListing,
};
use modbit_mcp::protocol::{self, Frame, ServerInfo};
use modbit_mcp::result::{CallResult, parse_call_result};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// How long the hub waits for one request before it cancels it.
const DEFAULT_CALL_TIMEOUT_MS: u64 = 120_000;
/// How long the handshake may take.
const HANDSHAKE_TIMEOUT_MS: u64 = 20_000;
/// How long a liveness check on a pooled transport may take.
const PING_TIMEOUT_MS: u64 = 5_000;
/// Most `tools/list` pages followed for one server.
const MAX_LIST_PAGES: usize = 16;
/// How long a pooled server may sit unused before the reaper stops it.
const DEFAULT_IDLE_TTL_MS: u64 = 300_000;
/// How long a server gets to exit after its input is closed before it is
/// killed (the second phase of a two-phase shutdown).
const DEFAULT_SHUTDOWN_GRACE_MS: u64 = 3_000;
/// Most lifecycle records kept per pooled server.
const MAX_LIFECYCLE: usize = 16;
/// The environment variable a server's credential is placed in.
const DEFAULT_CREDENTIAL_ENV: &str = "MCP_CREDENTIAL";

/// How a credential handle is keyed in custody and in the environment:
/// `docs-api` is handed to the Core as `MODBIT_MCP_CREDENTIAL_DOCS_API`.
fn credential_key(handle: &str) -> String {
    handle.to_ascii_uppercase().replace(['-', '.'], "_")
}

type Waiter = oneshot::Sender<Result<Value, PortError>>;

/// What a transport shares between its reader task, its writer task and
/// everyone waiting on it.
#[derive(Default)]
struct Shared {
    waiters: BTreeMap<u64, Waiter>,
    calls: CallTable,
    lost: Option<String>,
    reconciled: Vec<Reconciled>,
    /// The server said its tool list changed (`notifications/tools/list_changed`):
    /// the cached catalog is stale until the next discovery (PX-115).
    catalog_stale: bool,
}

/// How a shutdown ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShutdownOutcome {
    /// The server exited when its input was closed.
    Graceful,
    /// The server ignored the closed input and was killed after the grace.
    Killed,
    /// The process was already gone.
    AlreadyGone,
}

/// A request to the task that owns the child process.
struct ShutdownRequest {
    grace: std::time::Duration,
    done: oneshot::Sender<ShutdownOutcome>,
}

/// One live server process and the two tasks that drive its pipes.
struct Connection {
    pid: Option<u32>,
    out: mpsc::UnboundedSender<String>,
    shared: Arc<std::sync::Mutex<Shared>>,
    next_id: AtomicU64,
    info: ServerInfo,
    limits: Limits,
    timeout: std::time::Duration,
    /// Phase one of a shutdown: closes the child's input.
    close: CancellationToken,
    /// The task that owns the child: phase two (the kill after the grace).
    ctl: mpsc::UnboundedSender<ShutdownRequest>,
}

impl Connection {
    /// Two-phase shutdown (PX-115): close the server's input, which is how
    /// MCP tells a stdio server to exit, wait the grace period, then kill
    /// what is still running. The caller learns which of the two ended it.
    async fn shutdown(&self, grace: std::time::Duration) -> ShutdownOutcome {
        {
            let mut s = self.lock();
            if s.lost.is_none() {
                s.lost = Some("the host shut the server down".into());
            }
        }
        // The request is queued before the input is closed: a server that
        // exits the instant its input ends must still find the shutdown it
        // was given, or the exit would be mistaken for an unexpected death.
        let (done, rx) = oneshot::channel();
        if self.ctl.send(ShutdownRequest { grace, done }).is_err() {
            self.close.cancel();
            return ShutdownOutcome::AlreadyGone;
        }
        self.close.cancel();
        tokio::time::timeout(grace + std::time::Duration::from_secs(10), rx)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(ShutdownOutcome::AlreadyGone)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn send(&self, frame: &Frame) -> Result<(), PortError> {
        self.out.send(frame.encode()).map_err(|_| {
            PortError::clean("EXTERNAL_TRANSPORT_LOST", "the server is no longer running")
        })
    }

    /// Issue a request and wait for its answer. `call` names the host's tool
    /// call when this request is one (so it can be cancelled by id and
    /// reconciled if it is interrupted).
    async fn request(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        call: Option<(&str, &str, &str, bool)>,
    ) -> Result<Value, PortError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut shared = self.lock();
            if let Some(lost) = &shared.lost {
                return Err(PortError::clean("EXTERNAL_TRANSPORT_LOST", lost.clone()));
            }
            shared.waiters.insert(id, tx);
            if let Some((call_id, server, tool, effectful)) = call {
                shared.calls.begin(call_id, server, tool, id, effectful);
            }
        }
        let guard = CallGuard {
            conn: Arc::clone(self),
            request_id: id,
            armed: true,
        };
        self.send(&Frame::request(id, method, params))?;
        let answer = tokio::time::timeout(self.timeout, rx).await;
        match answer {
            Ok(Ok(result)) => {
                let mut guard = guard;
                guard.armed = false;
                self.lock().calls.complete(id);
                result
            }
            // The waiter was dropped: the reader resolved it another way
            // (transport lost) or the connection is going down.
            Ok(Err(_)) => {
                let mut guard = guard;
                guard.armed = false;
                Err(self.reconcile(id, "the server closed before answering"))
            }
            Err(_) => {
                // Timed out: the guard's drop sends the cancellation.
                drop(guard);
                Err(self.reconcile(id, "the server did not answer in time"))
            }
        }
    }

    /// How a call that never got an answer is reported.
    fn reconcile(&self, request_id: u64, detail: &str) -> PortError {
        let mut shared = self.lock();
        shared.waiters.remove(&request_id);
        match shared.calls.complete(request_id) {
            Some((call, _)) if call.effectful => PortError::unknown(
                "EXTERNAL_OUTCOME_UNKNOWN",
                format!(
                    "`{}.{}` was in flight when it ended ({detail}); the effect may have happened",
                    call.server, call.tool
                ),
            ),
            Some((call, _)) => PortError::clean(
                "EXTERNAL_CALL_CANCELLED",
                format!(
                    "`{}.{}` ended before it answered ({detail})",
                    call.server, call.tool
                ),
            ),
            None => PortError::clean("EXTERNAL_CALL_CANCELLED", detail.to_owned()),
        }
    }
}

/// Holds an in-flight request open. Whatever ends the wait — a timeout, a
/// cancelled task, a dropped future — this sends `notifications/cancelled`
/// so the server stops working on something nobody will read.
struct CallGuard {
    conn: Arc<Connection>,
    request_id: u64,
    armed: bool,
}

impl Drop for CallGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let _ = self.conn.send(&protocol::cancelled_notification(
            self.request_id,
            "the host stopped waiting",
        ));
        let mut shared = self.conn.lock();
        shared.waiters.remove(&self.request_id);
        if let Some((call, _)) = shared.calls.complete(self.request_id) {
            let outcome = CallTable::cancelled_outcome(&call);
            shared.reconciled.push(outcome);
        }
    }
}

/// One pooled server: its configuration, its transport when it is running,
/// and every session it serves.
struct Pooled {
    key: PoolKey,
    cfg: ServerConfig,
    conn: Mutex<Option<Arc<Connection>>>,
    state: std::sync::Mutex<PooledState>,
}

#[derive(Default)]
struct PooledState {
    sessions: BTreeSet<String>,
    discovery: Option<Discovery>,
    failure: Option<(String, String)>,
    /// How many times the catalog has been read from the server.
    catalog_generation: u64,
    /// What happened to the process, newest last.
    lifecycle: Vec<LifecycleRecord>,
    /// When a session last used this server (reads, calls, discovery).
    last_used: Option<std::time::Instant>,
}

impl Pooled {
    fn state(&self) -> std::sync::MutexGuard<'_, PooledState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A session used the server now: the idle clock restarts.
    fn touch(&self) {
        self.state().last_used = Some(std::time::Instant::now());
    }

    fn record(&self, event: &str, pid: Option<u32>) {
        let mut st = self.state();
        st.lifecycle.push(LifecycleRecord {
            at_ms: modbit_domain::Timestamp::now().0,
            event: event.to_owned(),
            pid,
        });
        let excess = st.lifecycle.len().saturating_sub(MAX_LIFECYCLE);
        st.lifecycle.drain(..excess);
    }
}

/// The hub: every pooled transport and the credentials in this Core's
/// custody. Which servers a task may see is not kept here — that is the
/// task's resolved configuration (`crate::config`), so a project layer and
/// a user layer answer per task while the transports they name are still
/// shared across tasks by fingerprint.
pub struct McpHub {
    limits: Limits,
    timeout: std::time::Duration,
    reads: Arc<modbit_mcp::ReadDeclarations>,
    /// The credential broker the servers' credentials live in.
    broker: Arc<modbit_secrets::CredentialBroker>,
    pool: Mutex<BTreeMap<PoolKey, Arc<Pooled>>>,
    /// Unused longer than this, a server is stopped (PX-115).
    idle_ttl: std::time::Duration,
    /// How long a server gets to exit before it is killed.
    grace: std::time::Duration,
    reaper_started: std::sync::atomic::AtomicBool,
}

/// What one task brings to the hub: the servers its configuration resolved
/// to, the ones the configuration refused, the capabilities its lease
/// grants (what a server may need before it is reachable at all) and the
/// secrets in the Core's custody that must never come back out of a
/// server's answer.
pub struct TaskScope {
    /// Servers the task's configuration resolved to.
    pub servers: Vec<ConfiguredServer>,
    /// Servers the configuration refused, with the reason.
    pub refused_servers: Vec<String>,
    /// What the task's capability lease grants.
    pub lease_ops: Vec<String>,
    /// Secret values in the Core's custody.
    pub secrets: Vec<String>,
}

/// One server a task's configuration resolved to, with the record of how
/// the layers decided it.
#[derive(Clone, Debug)]
pub struct ConfiguredServer {
    /// The validated configuration.
    pub config: ServerConfig,
    /// Which layer decided, whose contrary definition was overridden, and
    /// any widening the resolver refused for this name.
    pub provenance: Vec<String>,
}

/// The MCP servers a resolved configuration refused: a layer tried to add
/// one a higher authority denied, or to remove one a higher authority
/// added. The resolver keeps every such attempt; this is the MCP subset.
#[must_use]
pub fn refused_servers(resolved: &modbit_policy::config::ResolvedConfig) -> Vec<String> {
    resolved
        .rejected_widenings
        .iter()
        .filter(|w| w.contains("MCP server"))
        .cloned()
        .collect()
}

/// The servers a resolved configuration names, validated and normalized.
/// A definition that does not parse or does not validate is dropped with a
/// line on stderr: one bad server never hides the rest.
#[must_use]
pub fn servers_from(resolved: &modbit_policy::config::ResolvedConfig) -> Vec<ConfiguredServer> {
    let mut out = Vec::new();
    for (name, entry) in &resolved.mcp_servers {
        let mut cfg = match serde_json::from_str::<ServerConfig>(&entry.value) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "modbit-core: external server `{name}` is not a server definition ({e}); ignored"
                );
                continue;
            }
        };
        // The configuration key is the authority on the name, and the layer
        // that decided is the authority on the layer.
        cfg.name = name.clone();
        cfg.layer = format!("{:?}", entry.provenance.decided_by).to_ascii_lowercase();
        if let Err(e) = cfg.validate() {
            eprintln!("modbit-core: external server `{name}` refused: {e}");
            continue;
        }
        out.push(ConfiguredServer {
            config: cfg,
            provenance: crate::config::server_provenance(resolved, name),
        });
    }
    out
}

impl McpHub {
    /// A hub with the host's bounds, the call timeout from
    /// `MODBIT_MCP_CALL_TIMEOUT_MS`, and the credentials handed to the Core
    /// at boot as `MODBIT_MCP_CREDENTIAL_<HANDLE>` — taken into memory here
    /// and never written anywhere else.
    pub fn from_env(
        reads: Arc<modbit_mcp::ReadDeclarations>,
        broker: Arc<modbit_secrets::CredentialBroker>,
    ) -> Self {
        let env_ms = |key: &str, default: u64| -> std::time::Duration {
            std::time::Duration::from_millis(
                std::env::var(key)
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default),
            )
        };
        let mut limits = Limits::default();
        if let Some(n) = std::env::var("MODBIT_MCP_MAX_TOOLS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| *n > 0)
        {
            limits.max_tools = n;
        }
        let hub = Self {
            idle_ttl: env_ms("MODBIT_MCP_IDLE_TTL_MS", DEFAULT_IDLE_TTL_MS),
            grace: env_ms("MODBIT_MCP_SHUTDOWN_GRACE_MS", DEFAULT_SHUTDOWN_GRACE_MS),
            reaper_started: std::sync::atomic::AtomicBool::new(false),
            limits,
            timeout: std::time::Duration::from_millis(
                std::env::var("MODBIT_MCP_CALL_TIMEOUT_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_CALL_TIMEOUT_MS),
            ),
            reads,
            broker,
            pool: Mutex::new(BTreeMap::new()),
        };
        for (key, value) in std::env::vars() {
            if let Some(handle) = key.strip_prefix("MODBIT_MCP_CREDENTIAL_")
                && !handle.is_empty()
                && !value.is_empty()
            {
                hub.set_credential(handle, value);
            }
        }
        hub
    }

    /// Publish what a task's servers declare as reads, so the registered
    /// `external.call` presents the host's judgement to the kernel.
    ///
    /// # Errors
    /// Whatever [`ServerConfig::validate`] refuses.
    pub fn configure(&self, mut cfg: ServerConfig) -> Result<(), ConfigError> {
        cfg.validate()?;
        self.reads.publish(&cfg);
        Ok(())
    }

    /// The broker's identity of a server credential handle.
    pub(crate) fn credential_id(handle: &str) -> modbit_secrets::CredentialId {
        modbit_secrets::CredentialId::new(modbit_secrets::Kind::Mcp, &credential_key(handle))
    }

    /// Put a credential value in the broker under `handle` (a rotation when
    /// it is already there). Held in memory only: never journaled, logged,
    /// written to any file, echoed in a view or returned to a client.
    pub fn set_credential(&self, handle: &str, value: String) {
        self.broker.register(modbit_secrets::Registration {
            id: Self::credential_id(handle),
            kind: modbit_secrets::Kind::Mcp,
            source: modbit_secrets::SecretHandle::Inline(value),
            audience: "mcp:*".into(),
        });
    }

    /// Whether the broker holds a usable credential for `handle`.
    pub fn has_credential(&self, handle: &str) -> bool {
        self.broker.configured(&Self::credential_id(handle))
    }

    /// Forget a credential. A server that needs it stops being reachable at
    /// its next start; a transport already running keeps what it was given
    /// until it is replaced.
    pub fn clear_credential(&self, handle: &str) {
        self.broker.forget(&Self::credential_id(handle));
    }

    /// Stop every pooled transport of the server called `name`. A person
    /// taking trust away means the program stops, not merely that the next
    /// task cannot reach it.
    pub async fn stop_named(&self, name: &str) {
        let doomed: Vec<PoolKey> = {
            let pool = self.pool.lock().await;
            pool.iter()
                .filter(|(_, e)| e.cfg.name == name)
                .map(|(k, _)| k.clone())
                .collect()
        };
        for key in doomed {
            let entry = self.pool.lock().await.remove(&key);
            if let Some(entry) = entry {
                // A person taking trust away means the program stops: its
                // input is closed and, if it does not exit, it is killed.
                let conn = entry.conn.lock().await.take();
                if let Some(c) = conn {
                    let pid = c.pid;
                    let outcome = c.shutdown(self.grace).await;
                    entry.record(outcome_label(outcome), pid);
                }
            }
        }
    }

    /// Stop every pooled server (Core shutdown).
    pub async fn shutdown(&self) {
        let entries: Vec<Arc<Pooled>> = self.pool.lock().await.values().cloned().collect();
        let mut stops = Vec::new();
        for entry in entries {
            let conn = entry.conn.lock().await.take();
            if let Some(c) = conn {
                let grace = self.grace.min(std::time::Duration::from_millis(400));
                stops.push(tokio::spawn(async move { c.shutdown(grace).await }));
            }
        }
        for s in stops {
            let _ = s.await;
        }
        self.pool.lock().await.clear();
    }

    /// Stop every pooled server that has been unused for the idle bound and
    /// has no call in flight: close its input, wait the grace period, kill
    /// what is left (PX-115). The next use starts a fresh process. Returns
    /// how many were stopped.
    pub(crate) async fn reap_idle(&self) -> usize {
        let entries: Vec<Arc<Pooled>> = self.pool.lock().await.values().cloned().collect();
        let mut stopped = 0;
        for entry in entries {
            let conn = {
                let mut slot = entry.conn.lock().await;
                let Some(c) = slot.as_ref() else { continue };
                let idle = entry
                    .state()
                    .last_used
                    .is_none_or(|t| t.elapsed() >= self.idle_ttl);
                let busy = !c.lock().calls.in_flight().is_empty();
                if !idle || busy {
                    continue;
                }
                entry.state().discovery = None;
                slot.take()
            };
            if let Some(c) = conn {
                let pid = c.pid;
                let outcome = c.shutdown(self.grace).await;
                entry.record(outcome_label(outcome), pid);
                stopped += 1;
            }
        }
        stopped
    }

    /// Start the idle reaper once, on the runtime that serves the Core.
    fn ensure_reaper(self: &Arc<Self>) {
        if self.idle_ttl.is_zero()
            || tokio::runtime::Handle::try_current().is_err()
            || self
                .reaper_started
                .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let weak = Arc::downgrade(self);
        let interval = (self.idle_ttl / 4).clamp(
            std::time::Duration::from_millis(20),
            std::time::Duration::from_secs(30),
        );
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let Some(hub) = weak.upgrade() else { break };
                hub.reap_idle().await;
            }
        });
    }

    /// The hub as one task sees it.
    pub fn for_task(
        self: &Arc<Self>,
        tenant: &str,
        workspace: Option<&str>,
        correlation: Correlation,
        scope: TaskScope,
    ) -> TaskHub {
        for s in &scope.servers {
            let _ = self.configure(s.config.clone());
        }
        self.ensure_reaper();
        TaskHub {
            hub: Arc::clone(self),
            tenant: tenant.to_owned(),
            workspace: workspace.map(str::to_owned),
            correlation,
            servers: scope.servers,
            refused_servers: scope.refused_servers,
            lease_ops: scope.lease_ops,
            secrets: scope.secrets,
        }
    }

    /// The pool entry for `cfg` under this tenant and workspace, created on
    /// first use. This is where two sessions meet: same key, same entry.
    async fn entry(
        &self,
        tenant: &str,
        workspace: Option<&str>,
        cfg: &ServerConfig,
    ) -> Arc<Pooled> {
        let key = PoolKey::new(tenant, workspace, cfg);
        let mut pool = self.pool.lock().await;
        Arc::clone(pool.entry(key.clone()).or_insert_with(|| {
            Arc::new(Pooled {
                key,
                cfg: cfg.clone(),
                conn: Mutex::new(None),
                state: std::sync::Mutex::new(PooledState::default()),
            })
        }))
    }

    /// The live transport for `entry`, starting the server if this is the
    /// first session to need it (lazy discovery, REQ-EV-0128).
    async fn connection(
        &self,
        entry: &Arc<Pooled>,
        session: &str,
        cfg: &ServerConfig,
    ) -> Result<Arc<Connection>, PortError> {
        // Trust first, and before the pool: a transport another task started
        // is not a way to reach a server this task's configuration does not
        // trust, and trust can be taken away while an entry is alive.
        if !cfg.startable() {
            return Err(PortError::clean(
                "EXTERNAL_SERVER_UNTRUSTED",
                format!(
                    "`{}` is proposed but not trusted; a person or an admin layer must trust it before it can run",
                    cfg.name
                ),
            ));
        }
        entry.state().sessions.insert(session.to_owned());
        let mut slot = entry.conn.lock().await;
        if let Some(c) = slot.as_ref() {
            // Health (REQ-EV-0128): a pooled transport is handed out only
            // while it still answers. A server that has stopped answering
            // is not "ready" because it once was.
            if c.lock().lost.is_none() && alive(c).await {
                entry.touch();
                return Ok(Arc::clone(c));
            }
            // A dead transport is never handed out again: the entry starts
            // a fresh process for the next caller.
            entry.record("LOST", c.pid);
            *slot = None;
            entry.state().discovery = None;
        }
        match self.start(cfg).await {
            Ok(c) => {
                entry.state().failure = None;
                entry.record("STARTED", c.pid);
                entry.touch();
                *slot = Some(Arc::clone(&c));
                Ok(c)
            }
            Err(e) => {
                entry.state().failure = Some((e.code.clone(), e.message.clone()));
                Err(e)
            }
        }
    }

    /// Start a server and complete the handshake.
    async fn start(&self, cfg: &ServerConfig) -> Result<Arc<Connection>, PortError> {
        let modbit_mcp::Transport::Stdio { command, args } = &cfg.transport;
        let mut command = tokio::process::Command::new(command);
        command
            .args(args)
            .envs(&cfg.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The server's own diagnostics stay with the Core's; nothing a
            // server writes to stderr is protocol.
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(handle) = &cfg.credential {
            // The credential for this server, from the broker: scoped to the
            // server it is started for, counted, and refused if revoked.
            let secret = self
                .broker
                .acquire(
                    &Self::credential_id(handle),
                    &modbit_secrets::Use {
                        principal: "core".into(),
                        audience: format!("mcp:{}", cfg.name),
                        purpose: "mcp.spawn".into(),
                        nonce: None,
                    },
                )
                .map_err(|refusal| {
                    PortError::clean(
                        "EXTERNAL_CREDENTIAL_UNAVAILABLE",
                        format!(
                            "`{}` needs the credential `{handle}`, which the credential broker does not give it ({})",
                            cfg.name,
                            refusal.code()
                        ),
                    )
                })?;
            command.env(DEFAULT_CREDENTIAL_ENV, secret.expose());
        }
        let mut child = command.spawn().map_err(|e| {
            PortError::clean(
                "EXTERNAL_SERVER_UNAVAILABLE",
                format!("`{}` could not be started: {e}", cfg.name),
            )
        })?;
        let pid = child.id();
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let shared = Arc::new(std::sync::Mutex::new(Shared::default()));
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let close = CancellationToken::new();
        let (ctl, ctl_rx) = mpsc::unbounded_channel::<ShutdownRequest>();
        spawn_writer(stdin, rx, close.clone());
        spawn_reader(stdout, Arc::clone(&shared), self.limits, cfg.name.clone());
        // The child is owned by a task that reaps it when it exits, so a
        // finished server never becomes a zombie, and that carries out the
        // second phase of a shutdown (the kill after the grace period).
        spawn_child_owner(child, ctl_rx, self.grace);
        let conn = Arc::new(Connection {
            pid,
            out: tx,
            shared,
            next_id: AtomicU64::new(1),
            info: ServerInfo::default(),
            limits: self.limits,
            timeout: std::time::Duration::from_millis(HANDSHAKE_TIMEOUT_MS),
            close: close.clone(),
            ctl: ctl.clone(),
        });
        let id = conn.next_id.fetch_add(1, Ordering::SeqCst);
        let (waiter, rx) = oneshot::channel();
        conn.lock().waiters.insert(id, waiter);
        conn.send(&Frame {
            id: Some(id),
            ..protocol::initialize_request(0)
        })?;
        let result = match tokio::time::timeout(conn.timeout, rx).await {
            Ok(Ok(r)) => r?,
            _ => {
                return Err(PortError::clean(
                    "EXTERNAL_HANDSHAKE_FAILED",
                    format!("`{}` did not complete the MCP handshake", cfg.name),
                ));
            }
        };
        let info = protocol::accept_initialize_result(&result, 256)
            .map_err(|e| PortError::clean(e.code, format!("`{}`: {}", cfg.name, e.message)))?;
        conn.send(&Frame::notification(
            protocol::METHOD_INITIALIZED,
            json!({}),
        ))?;
        Ok(Arc::new(Connection {
            pid,
            out: conn.out.clone(),
            shared: Arc::clone(&conn.shared),
            next_id: AtomicU64::new(conn.next_id.load(Ordering::SeqCst)),
            info,
            limits: self.limits,
            timeout: self.timeout,
            close,
            ctl,
        }))
    }

    /// Discover a server's tools, once per transport (lazy).
    async fn discover(
        &self,
        entry: &Arc<Pooled>,
        conn: &Arc<Connection>,
    ) -> Result<Discovery, PortError> {
        // `notifications/tools/list_changed` invalidates the cached catalog:
        // the next read asks the server again (PX-115).
        let stale = std::mem::take(&mut conn.lock().catalog_stale);
        if stale {
            entry.state().discovery = None;
            entry.record("CATALOG_CHANGED", conn.pid);
        }
        if let Some(d) = entry.state().discovery.clone() {
            return Ok(d);
        }
        let mut all = Discovery::default();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_LIST_PAGES {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let result = conn
                .request(protocol::METHOD_TOOLS_LIST, params, None)
                .await?;
            let page = modbit_mcp::parse_tools_list(&entry.cfg, &result, &conn.limits)
                .map_err(|e| PortError::clean(e.code, e.message))?;
            cursor = page.next_cursor.clone();
            all.dropped_over_limit += page.dropped_over_limit;
            all.rejected.extend(page.rejected);
            for t in page.tools {
                if all.tools.len() >= conn.limits.max_tools {
                    all.dropped_over_limit += 1;
                } else if !all.tools.iter().any(|k| k.name == t.name) {
                    all.tools.push(t);
                }
            }
            if cursor.is_none() {
                break;
            }
        }
        {
            let mut st = entry.state();
            st.discovery = Some(all.clone());
            st.catalog_generation += 1;
        }
        Ok(all)
    }
}

/// Whether a pooled transport still answers, bounded so a wedged server
/// holds its pool entry for a moment rather than a call timeout.
async fn alive(conn: &Arc<Connection>) -> bool {
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_millis(PING_TIMEOUT_MS),
            conn.request(protocol::METHOD_PING, json!({}), None),
        )
        .await,
        Ok(Ok(_))
    )
}

fn outcome_label(o: ShutdownOutcome) -> &'static str {
    match o {
        ShutdownOutcome::Graceful => "REAPED_GRACEFUL",
        ShutdownOutcome::Killed => "REAPED_KILLED",
        ShutdownOutcome::AlreadyGone => "LOST",
    }
}

/// Owns the child process. It reaps the child when it exits on its own and
/// carries out a shutdown: the input is already closed by the caller, so it
/// waits the grace period and kills the process if it is still there. When
/// every handle to the connection is dropped it does the same with the
/// default grace, so a server that ignores a closed input cannot outlive its
/// connection.
fn spawn_child_owner(
    mut child: tokio::process::Child,
    mut ctl: mpsc::UnboundedReceiver<ShutdownRequest>,
    default_grace: std::time::Duration,
) {
    tokio::spawn(async move {
        let request = tokio::select! {
            _ = child.wait() => {
                // Exited: if a host shutdown was waiting, this is its
                // graceful end; otherwise it died on its own.
                while let Ok(req) = ctl.try_recv() {
                    let _ = req.done.send(ShutdownOutcome::Graceful);
                }
                return;
            }
            r = ctl.recv() => r,
        };
        let (grace, done) = match request {
            Some(r) => (r.grace, Some(r.done)),
            None => (default_grace, None),
        };
        let outcome = match tokio::time::timeout(grace, child.wait()).await {
            Ok(_) => ShutdownOutcome::Graceful,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                ShutdownOutcome::Killed
            }
        };
        if let Some(done) = done {
            let _ = done.send(outcome);
        }
    });
}

fn spawn_writer(
    mut stdin: tokio::process::ChildStdin,
    mut rx: mpsc::UnboundedReceiver<String>,
    close: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            let line = tokio::select! {
                l = rx.recv() => l,
                () = close.cancelled() => None,
            };
            let Some(line) = line else { break };
            if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                break;
            }
        }
        // Closing the child's input is how an MCP server is told to exit.
        let _ = stdin.shutdown().await;
    });
}

fn spawn_reader(
    stdout: tokio::process::ChildStdout,
    shared: Arc<std::sync::Mutex<Shared>>,
    limits: Limits,
    server: String,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            // A line the host cannot read is not a reason to tear the
            // server down: it is dropped, and a call waiting on an answer
            // times out and reconciles.
            let Ok(frame) = protocol::decode(&line, limits.max_frame_bytes) else {
                continue;
            };
            let Some(id) = frame.id else {
                // A notification. The only one the host acts on is a changed
                // tool list, and only by marking its cache stale: the server
                // can cost one more `tools/list`, nothing else.
                if frame.method.as_deref() == Some(protocol::METHOD_TOOLS_LIST_CHANGED) {
                    shared
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .catalog_stale = true;
                }
                continue;
            };
            if frame.method.is_some() {
                // The host declared no client capabilities, so it serves no
                // server-initiated request.
                continue;
            }
            let waiter = {
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                s.waiters.remove(&id)
            };
            if let Some(waiter) = waiter {
                let answer = match (frame.result, frame.error) {
                    (Some(r), _) => Ok(r),
                    (None, Some(e)) => Err(PortError::clean(
                        "EXTERNAL_SERVER_ERROR",
                        format!("{server} refused the request: {} ({})", e.message, e.code),
                    )),
                    (None, None) => Err(PortError::clean(
                        "EXTERNAL_SERVER_ERROR",
                        format!("{server} answered with neither a result nor an error"),
                    )),
                };
                let _ = waiter.send(answer);
            }
        }
        // End of stream: the server is gone. Everything waiting ends now,
        // and an effect that was in flight is reported unknown rather than
        // assumed away (docs/54 fault 25).
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.lost = Some(format!("`{server}` closed its output"));
        let in_flight = s.calls.in_flight();
        let reconciled = s.calls.transport_lost("the server exited");
        s.reconciled.extend(reconciled);
        for call in in_flight {
            if let Some(w) = s.waiters.remove(&call.request_id) {
                let _ = w.send(Err(if call.effectful {
                    PortError::unknown(
                        "EXTERNAL_OUTCOME_UNKNOWN",
                        format!(
                            "`{}.{}` was in flight when `{server}` exited; the effect may have happened",
                            call.server, call.tool
                        ),
                    )
                } else {
                    PortError::clean(
                        "EXTERNAL_TRANSPORT_LOST",
                        format!(
                            "`{}.{}` was reading when `{server}` exited; nothing was changed",
                            call.server, call.tool
                        ),
                    )
                }));
            }
        }
        // Whatever else was waiting (a handshake, a discovery) simply failed.
        for id in s.waiters.keys().copied().collect::<Vec<_>>() {
            if let Some(w) = s.waiters.remove(&id) {
                let _ = w.send(Err(PortError::clean(
                    "EXTERNAL_TRANSPORT_LOST",
                    format!("`{server}` exited before answering"),
                )));
            }
        }
    });
}

/// The hub bound to one task: the servers its configuration resolved to,
/// the tenant and workspace that decide which pool entries it may touch,
/// the capabilities its lease grants, the secrets in the Core's custody,
/// and the identity every call carries.
pub struct TaskHub {
    hub: Arc<McpHub>,
    tenant: String,
    workspace: Option<String>,
    correlation: Correlation,
    servers: Vec<ConfiguredServer>,
    refused_servers: Vec<String>,
    lease_ops: Vec<String>,
    secrets: Vec<String>,
}

impl TaskHub {
    fn server(&self, name: &str) -> Option<&ConfiguredServer> {
        self.servers.iter().find(|s| s.config.name == name)
    }

    /// Whether this task may reach `cfg` at all: a server is not a way
    /// around the task's own lease (REQ-EV-0128). Returns what is missing.
    fn unleased(&self, cfg: &ServerConfig) -> Vec<String> {
        cfg.missing_capabilities(&self.lease_ops)
    }

    /// Replace every secret in the Core's custody that a server put in its
    /// answer. A server is handed a credential to use, not to repeat: an
    /// answer that carries one back would put it in the model's context,
    /// which is the exfiltration this forbids. Returns the number of
    /// replacements so the host can record that it happened.
    fn redact(&self, result: &mut CallResult) -> usize {
        let redactor = self.redactor();
        let mut hits = 0;
        for part in &mut result.parts {
            match part {
                modbit_mcp::Part::Text { text, .. }
                | modbit_mcp::Part::Resource {
                    text: Some(text), ..
                } => {
                    let r = redactor.data(text);
                    if r.changed() {
                        hits += r.held;
                        *text = r.text;
                    }
                }
                _ => {}
            }
        }
        if let Some(structured) = &mut result.structured {
            hits += redactor.data_json(structured);
        }
        hits
    }

    /// The secrets in this Core's custody, as the one redactor (REQ-EV-0017).
    fn redactor(&self) -> modbit_secrets::Redactor {
        modbit_secrets::Redactor::new(self.secrets.iter().cloned())
    }

    /// What a server said when it failed is error text: it loses every held
    /// value and every credential shape before the host reads it
    /// (REQ-EV-0017).
    fn redact_error(&self, mut e: PortError) -> PortError {
        e.message = self.redactor().error_text(&e.message);
        e
    }

    async fn listing_for(&self, server: &ConfiguredServer) -> ServerListing {
        let cfg = &server.config;
        let entry = self
            .hub
            .entry(&self.tenant, self.workspace.as_deref(), cfg)
            .await;
        let mut listing = ServerListing {
            name: cfg.name.clone(),
            trust: cfg.trust,
            health: Health::NotStarted,
            scopes: cfg.scopes.iter().cloned().collect(),
            requires: cfg.needed_capabilities().into_iter().collect(),
            layer: cfg.layer.clone(),
            provenance: server.provenance.clone(),
            pool_key: entry.key.to_string(),
            tools: vec![],
            rejected: vec![],
            dropped_over_limit: 0,
            catalog_generation: 0,
            lifecycle: vec![],
        };
        if cfg.trust != Trust::Trusted {
            listing.health = Health::Untrusted;
            return listing;
        }
        let missing = self.unleased(cfg);
        if !missing.is_empty() {
            listing.health = Health::Unleased { missing };
            return listing;
        }
        match self
            .hub
            .connection(&entry, &self.correlation.session_id, cfg)
            .await
        {
            Ok(conn) => match self.hub.discover(&entry, &conn).await {
                Ok(d) => {
                    listing.health = Health::Ready {
                        server_info: conn.info.clone(),
                        sharers: entry.state().sessions.len(),
                        pid: conn.pid,
                    };
                    listing.tools = d.tools;
                    listing.rejected = d.rejected;
                    listing.dropped_over_limit = d.dropped_over_limit;
                    entry.touch();
                }
                Err(e) => {
                    let e = self.redact_error(e);
                    listing.health = Health::Failed {
                        code: e.code,
                        message: e.message,
                    };
                }
            },
            Err(e) => {
                let e = self.redact_error(e);
                listing.health = Health::Failed {
                    code: e.code,
                    message: e.message,
                };
            }
        }
        {
            let st = entry.state();
            listing.catalog_generation = st.catalog_generation;
            listing.lifecycle = st.lifecycle.clone();
        }
        listing
    }
}

impl McpPort for TaskHub {
    fn list_server<'a>(&'a self, server: &'a str) -> BoxFuture<'a, Result<Listing, PortError>> {
        Box::pin(async move {
            let mut servers = Vec::new();
            for s in self.servers.iter().filter(|s| s.config.name == server) {
                servers.push(self.listing_for(s).await);
            }
            Ok(Listing {
                servers,
                refused_servers: self.refused_servers.clone(),
            })
        })
    }

    fn describe<'a>(
        &'a self,
        server: &'a str,
        tools: &'a [String],
    ) -> BoxFuture<'a, Result<Described, PortError>> {
        Box::pin(async move {
            let Some(configured) = self.server(server) else {
                return Err(PortError::clean(
                    "EXTERNAL_SERVER_UNKNOWN",
                    format!("no external server named `{server}` is configured for this task"),
                ));
            };
            let cfg = &configured.config;
            // Describing starts the server, so it is held to the same gates
            // as listing it: trusted, and leased. Only this server starts.
            if cfg.trust != Trust::Trusted {
                return Err(PortError::clean(
                    "EXTERNAL_SERVER_UNTRUSTED",
                    format!(
                        "`{}` is proposed but not trusted; it has not been started and has no tools to describe",
                        cfg.name
                    ),
                ));
            }
            let missing = self.unleased(cfg);
            if !missing.is_empty() {
                return Err(PortError::clean(
                    "EXTERNAL_CAPABILITY_NOT_LEASED",
                    format!(
                        "`{}` needs {missing:?}, which this task's capability lease does not grant",
                        cfg.name
                    ),
                ));
            }
            let entry = self
                .hub
                .entry(&self.tenant, self.workspace.as_deref(), cfg)
                .await;
            let conn = self
                .hub
                .connection(&entry, &self.correlation.session_id, cfg)
                .await
                .map_err(|e| self.redact_error(e))?;
            let d = self
                .hub
                .discover(&entry, &conn)
                .await
                .map_err(|e| self.redact_error(e))?;
            entry.touch();
            let mut out = Described {
                server: cfg.name.clone(),
                catalog_generation: entry.state().catalog_generation,
                ..Described::default()
            };
            for name in tools {
                // A tool is named by its own name or its qualified one.
                match d
                    .tools
                    .iter()
                    .find(|t| &t.name == name || &t.qualified == name)
                {
                    Some(t) => out.tools.push(t.clone()),
                    None => out.unknown.push(name.clone()),
                }
            }
            Ok(out)
        })
    }

    fn list<'a>(&'a self) -> BoxFuture<'a, Result<Listing, PortError>> {
        Box::pin(async move {
            let mut servers = Vec::new();
            for server in &self.servers {
                servers.push(self.listing_for(server).await);
            }
            Ok(Listing {
                servers,
                refused_servers: self.refused_servers.clone(),
            })
        })
    }

    fn call<'a>(&'a self, call: ExternalCall) -> BoxFuture<'a, Result<CallResult, PortError>> {
        Box::pin(async move {
            let Some(server) = self.server(&call.server) else {
                return Err(PortError::clean(
                    "EXTERNAL_SERVER_UNKNOWN",
                    format!(
                        "no external server named `{}` is configured for this task",
                        call.server
                    ),
                ));
            };
            let cfg = &server.config;
            let missing = self.unleased(cfg);
            if !missing.is_empty() {
                return Err(PortError::clean(
                    "EXTERNAL_CAPABILITY_NOT_LEASED",
                    format!(
                        "`{}` needs {missing:?}, which this task's capability lease does not grant",
                        cfg.name
                    ),
                ));
            }
            let entry = self
                .hub
                .entry(&self.tenant, self.workspace.as_deref(), cfg)
                .await;
            let conn = self
                .hub
                .connection(&entry, &self.correlation.session_id, cfg)
                .await
                .map_err(|e| self.redact_error(e))?;
            let discovery = self
                .hub
                .discover(&entry, &conn)
                .await
                .map_err(|e| self.redact_error(e))?;
            let Some(tool) = discovery.tools.iter().find(|t| t.name == call.tool) else {
                return Err(PortError::clean(
                    "EXTERNAL_TOOL_UNKNOWN",
                    format!("`{}` declares no tool named `{}`", cfg.name, call.tool),
                ));
            };
            validate_arguments(tool, &call.arguments)
                .map_err(|e| PortError::clean(e.code, e.message))?;
            let mut correlation = self.correlation.clone();
            correlation.call_id = call.call_id.clone();
            let params = json!({
                "name": tool.name,
                "arguments": call.arguments,
                // MCP reserves unprefixed `_meta` keys, so the correlation
                // travels under one this client owns.
                "_meta": correlation.meta(),
            });
            let result = conn
                .request(
                    protocol::METHOD_TOOLS_CALL,
                    params,
                    Some((&call.call_id, &cfg.name, &tool.name, call.effectful)),
                )
                .await
                .map_err(|e| self.redact_error(e))?;
            entry.touch();
            let mut parsed = parse_call_result(&result, &conn.limits)
                .map_err(|e| PortError::clean(e.code, e.message))?;
            parsed.redacted = self.redact(&mut parsed);
            Ok(parsed)
        })
    }

    fn for_site<'a>(&'a self, origin: &'a str) -> BoxFuture<'a, modbit_mcp::SiteTools> {
        Box::pin(async move {
            let mut out = modbit_mcp::SiteTools::default();
            for server in self.servers.iter().filter(|s| s.config.serves_site(origin)) {
                let cfg = &server.config;
                let unreachable = |code: &str, reason: String| modbit_mcp::SiteServerUnavailable {
                    server: cfg.name.clone(),
                    code: code.to_owned(),
                    reason,
                };
                if cfg.trust != Trust::Trusted {
                    out.unavailable.push(unreachable(
                        "EXTERNAL_SERVER_UNTRUSTED",
                        format!("`{}` is proposed but not trusted", cfg.name),
                    ));
                    continue;
                }
                let missing = self.unleased(cfg);
                if !missing.is_empty() {
                    out.unavailable.push(unreachable(
                        "EXTERNAL_CAPABILITY_NOT_LEASED",
                        format!(
                            "`{}` needs {missing:?}, which this task's capability lease does not grant",
                            cfg.name
                        ),
                    ));
                    continue;
                }
                let entry = self
                    .hub
                    .entry(&self.tenant, self.workspace.as_deref(), cfg)
                    .await;
                match self
                    .hub
                    .connection(&entry, &self.correlation.session_id, cfg)
                    .await
                {
                    Ok(conn) => match self.hub.discover(&entry, &conn).await {
                        Ok(d) => out.available.extend(d.tools),
                        Err(e) => out.unavailable.push(unreachable(&e.code, e.message)),
                    },
                    Err(e) => out.unavailable.push(unreachable(&e.code, e.message)),
                }
            }
            out
        })
    }

    fn cancel<'a>(
        &'a self,
        call_id: &'a str,
        reason: &'a str,
    ) -> BoxFuture<'a, Result<Cancelled, PortError>> {
        Box::pin(async move {
            let entries: Vec<Arc<Pooled>> = self.hub.pool.lock().await.values().cloned().collect();
            for entry in entries {
                if entry.key.tenant != self.tenant {
                    // A tenant never reaches another tenant's calls.
                    continue;
                }
                let conn = { entry.conn.lock().await.clone() };
                let Some(conn) = conn else { continue };
                let plan = {
                    let mut shared = conn.lock();
                    shared.calls.cancel(call_id, reason)
                };
                match plan {
                    CancelPlan::NotInFlight => continue,
                    CancelPlan::AlreadyCancelling { .. } => {
                        return Ok(Cancelled {
                            call_id: call_id.to_owned(),
                            outcome: "ALREADY_CANCELLING".into(),
                            unknown_outcome: false,
                        });
                    }
                    CancelPlan::Notify {
                        request_id,
                        effectful,
                    } => {
                        conn.send(&protocol::cancelled_notification(request_id, reason))?;
                        // Whoever is waiting on this call is released now:
                        // the server will not answer a cancelled request.
                        let waiter = {
                            let mut shared = conn.lock();
                            shared.waiters.remove(&request_id)
                        };
                        if let Some(w) = waiter {
                            let _ = w.send(Err(if effectful {
                                PortError::unknown(
                                    "EXTERNAL_OUTCOME_UNKNOWN",
                                    "the call was cancelled after it was sent; the effect may have happened",
                                )
                            } else {
                                PortError::clean(
                                    "EXTERNAL_CALL_CANCELLED",
                                    "the call was cancelled",
                                )
                            }));
                        }
                        return Ok(Cancelled {
                            call_id: call_id.to_owned(),
                            outcome: "NOTIFIED".into(),
                            unknown_outcome: effectful,
                        });
                    }
                }
            }
            Ok(Cancelled {
                call_id: call_id.to_owned(),
                outcome: "NOT_IN_FLIGHT".into(),
                unknown_outcome: false,
            })
        })
    }
}

/// A definition as the host stores it, and the view a client is answered
/// with. Never carries a credential value — only the handle a definition
/// named and whether the Core holds one.
fn view_of(
    core: &crate::server::Core,
    cfg: &ServerConfig,
    reason: &str,
) -> modbit_protocol::v1::ExternalServerConfigured {
    modbit_protocol::v1::ExternalServerConfigured {
        name: cfg.name.clone(),
        trust: format!("{:?}", cfg.trust).to_ascii_uppercase(),
        layer: "user".into(),
        requires: cfg.needed_capabilities().into_iter().collect(),
        scopes: cfg.scopes.iter().cloned().collect(),
        credential_available: cfg
            .credential
            .as_ref()
            .is_some_and(|h| core.tools.mcp.has_credential(h)),
        credential_handle: cfg.credential.clone().unwrap_or_default(),
        reason: reason.to_owned(),
    }
}

/// Parse and validate a definition under `name`, as the user layer holds it.
fn definition(name: &str, json: &str) -> Result<ServerConfig, (&'static str, String)> {
    let name =
        modbit_mcp::normalize_server_name(name).map_err(|e| ("BAD_EXTERNAL_SERVER", e.message))?;
    let mut cfg: ServerConfig = serde_json::from_str(json).map_err(|e| {
        (
            "BAD_EXTERNAL_SERVER",
            format!("`{name}` is not a server definition: {e}"),
        )
    })?;
    cfg.name = name;
    cfg.layer = "user".into();
    cfg.validate()
        .map_err(|e| ("BAD_EXTERNAL_SERVER", format!("{}: {}", e.code, e.message)))?;
    Ok(cfg)
}

/// The definition the user layer holds, as a proposal-shaped JSON string
/// with `trust` set. `reason` is the client's own text, kept for the audit.
fn stored(cfg: &ServerConfig, reason: &str) -> Result<String, (&'static str, String)> {
    let mut v = serde_json::to_value(cfg).map_err(|e| ("BAD_EXTERNAL_SERVER", e.to_string()))?;
    if let Some(o) = v.as_object_mut() {
        // Untrusted text a client supplied: bounded, kept beside the
        // definition so "who asked for this server, and why" has an answer.
        o.insert(
            "reason".into(),
            serde_json::Value::String(reason.chars().take(512).collect::<String>()),
        );
    }
    serde_json::to_string(&v).map_err(|e| ("BAD_EXTERNAL_SERVER", e.to_string()))
}

/// REQ-EV-0224: store a proposed external server. It is written PROPOSED
/// whatever the definition says — a client cannot propose something already
/// trusted — and nothing is started.
///
/// # Errors
/// `BAD_EXTERNAL_SERVER` when the definition does not validate,
/// `EXTERNAL_SERVER_DENIED` when a higher configuration layer denied the
/// name (the proposal could never run, so it is refused now rather than
/// silently at resolve time), `CONFIGURATION_UNWRITABLE` when the user
/// layer cannot be written.
pub fn propose(
    core: &crate::server::Core,
    name: &str,
    definition_json: &str,
    reason: &str,
) -> Result<modbit_protocol::v1::ExternalServerConfigured, (&'static str, String)> {
    let mut cfg = definition(name, definition_json)?;
    cfg.trust = Trust::Proposed;
    if let Some(level) = crate::config::denied_above_user(&core.data_dir, None, &cfg.name) {
        return Err((
            "EXTERNAL_SERVER_DENIED",
            format!(
                "`{}` is denied by the {level:?} configuration; a proposal for it could never run",
                cfg.name
            ),
        ));
    }
    crate::config::put_user_server(&core.data_dir, &cfg.name, &stored(&cfg, reason)?)
        .map_err(|e| ("CONFIGURATION_UNWRITABLE", e))?;
    Ok(view_of(core, &cfg, reason))
}

/// REQ-EV-0224: trust a proposed external server, or take trust away. Every
/// gate is answered here rather than when the server would have run.
///
/// # Errors
/// `UNKNOWN_EXTERNAL_SERVER` when the user layer holds no such definition,
/// `BAD_EXTERNAL_SERVER` when what it holds no longer validates,
/// `EXTERNAL_SERVER_DENIED` when a higher layer denies the name,
/// `EXTERNAL_CREDENTIAL_UNAVAILABLE` when the definition names a credential
/// this Core does not hold, `CONFIGURATION_UNWRITABLE` when the user layer
/// cannot be written.
pub fn set_trust(
    core: &crate::server::Core,
    name: &str,
    trust: bool,
) -> Result<modbit_protocol::v1::ExternalServerConfigured, (&'static str, String)> {
    let normalized =
        modbit_mcp::normalize_server_name(name).map_err(|e| ("BAD_EXTERNAL_SERVER", e.message))?;
    let Some(json) = crate::config::user_server(&core.data_dir, &normalized) else {
        return Err((
            "UNKNOWN_EXTERNAL_SERVER",
            format!("the user configuration holds no external server named `{normalized}`"),
        ));
    };
    let reason = serde_json::from_str::<serde_json::Value>(&json)
        .ok()
        .and_then(|v| v.get("reason").and_then(|r| r.as_str()).map(str::to_owned))
        .unwrap_or_default();
    let mut cfg = definition(&normalized, &json)?;
    if trust {
        if let Some(level) = crate::config::denied_above_user(&core.data_dir, None, &cfg.name) {
            return Err((
                "EXTERNAL_SERVER_DENIED",
                format!("`{}` is denied by the {level:?} configuration", cfg.name),
            ));
        }
        if let Some(handle) = &cfg.credential
            && !core.tools.mcp.has_credential(handle)
        {
            return Err((
                "EXTERNAL_CREDENTIAL_UNAVAILABLE",
                format!(
                    "`{}` needs the credential `{handle}`, which this Core does not hold; configure it before trusting the server",
                    cfg.name
                ),
            ));
        }
    }
    cfg.trust = if trust {
        Trust::Trusted
    } else {
        Trust::Proposed
    };
    crate::config::put_user_server(&core.data_dir, &cfg.name, &stored(&cfg, &reason)?)
        .map_err(|e| ("CONFIGURATION_UNWRITABLE", e))?;
    Ok(view_of(core, &cfg, &reason))
}
