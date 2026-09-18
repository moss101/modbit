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
//! in this hub's memory only, is placed into the child's environment at
//! spawn and appears nowhere else — not in the configuration, not in a log,
//! not in a listing, not in an argument and not in a result.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use modbit_mcp::calls::{CallTable, CancelPlan, Reconciled};
use modbit_mcp::config::{ConfigError, PoolKey, ServerConfig, Trust};
use modbit_mcp::discovery::{Discovery, Limits, validate_arguments};
use modbit_mcp::port::{
    BoxFuture, Cancelled, Correlation, ExternalCall, Health, Listing, McpPort, PortError,
    ServerListing,
};
use modbit_mcp::protocol::{self, Frame, ServerInfo};
use modbit_mcp::result::{CallResult, parse_call_result};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc, oneshot};

/// How long the hub waits for one request before it cancels it.
const DEFAULT_CALL_TIMEOUT_MS: u64 = 120_000;
/// How long the handshake may take.
const HANDSHAKE_TIMEOUT_MS: u64 = 20_000;
/// Most `tools/list` pages followed for one server.
const MAX_LIST_PAGES: usize = 16;
/// The environment variable a server's credential is placed in, unless the
/// configuration names another.
const DEFAULT_CREDENTIAL_ENV: &str = "MCP_CREDENTIAL";

type Waiter = oneshot::Sender<Result<Value, PortError>>;

/// What a transport shares between its reader task, its writer task and
/// everyone waiting on it.
#[derive(Default)]
struct Shared {
    waiters: BTreeMap<u64, Waiter>,
    calls: CallTable,
    lost: Option<String>,
    reconciled: Vec<Reconciled>,
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
}

impl Connection {
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
}

impl Pooled {
    fn state(&self) -> std::sync::MutexGuard<'_, PooledState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The hub: every configured server, every pooled transport, and the
/// credentials in this Core's custody.
pub struct McpHub {
    limits: Limits,
    timeout: std::time::Duration,
    reads: Arc<modbit_mcp::ReadDeclarations>,
    servers: std::sync::Mutex<BTreeMap<String, ServerConfig>>,
    credentials: std::sync::Mutex<BTreeMap<String, String>>,
    pool: Mutex<BTreeMap<PoolKey, Arc<Pooled>>>,
}

impl McpHub {
    /// A hub with the host's bounds, reading the servers configured at boot
    /// from `MODBIT_MCP_SERVERS` (a JSON array of server configurations) and
    /// the call timeout from `MODBIT_MCP_CALL_TIMEOUT_MS`.
    ///
    /// A configuration that does not validate is dropped with a line on
    /// stderr: a bad external server never stops the Core from starting.
    pub fn from_env(reads: Arc<modbit_mcp::ReadDeclarations>) -> Self {
        let hub = Self {
            limits: Limits::default(),
            timeout: std::time::Duration::from_millis(
                std::env::var("MODBIT_MCP_CALL_TIMEOUT_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_CALL_TIMEOUT_MS),
            ),
            reads,
            servers: std::sync::Mutex::new(BTreeMap::new()),
            credentials: std::sync::Mutex::new(BTreeMap::new()),
            pool: Mutex::new(BTreeMap::new()),
        };
        if let Ok(raw) = std::env::var("MODBIT_MCP_SERVERS")
            && let Ok(list) = serde_json::from_str::<Vec<ServerConfig>>(&raw)
        {
            for cfg in list {
                let name = cfg.name.clone();
                if let Err(e) = hub.configure(cfg) {
                    eprintln!("modbit-core: external server `{name}` refused: {e}");
                }
            }
        }
        // A credential handed to the Core at boot for a configured server:
        // `MODBIT_MCP_CREDENTIAL_<HANDLE>`. The value is taken into memory
        // here and never written anywhere.
        let handles: Vec<String> = hub
            .servers()
            .iter()
            .filter_map(|c| c.credential.clone())
            .collect();
        for handle in handles {
            let var = format!(
                "MODBIT_MCP_CREDENTIAL_{}",
                handle.to_ascii_uppercase().replace(['-', '.'], "_")
            );
            if let Ok(value) = std::env::var(&var)
                && !value.is_empty()
            {
                hub.set_credential(&handle, value);
            }
        }
        hub
    }

    /// Accept a server configuration (validating and normalizing it), and
    /// publish what it declares a read.
    ///
    /// # Errors
    /// Whatever [`ServerConfig::validate`] refuses.
    pub fn configure(&self, mut cfg: ServerConfig) -> Result<(), ConfigError> {
        cfg.validate()?;
        self.reads.publish(&cfg);
        self.servers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(cfg.name.clone(), cfg);
        Ok(())
    }

    /// Put a credential value in this Core's custody under `handle`. Held in
    /// memory only: never journaled, logged, written to any file, echoed in
    /// a view or returned to a client.
    pub fn set_credential(&self, handle: &str, value: String) {
        self.credentials
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(handle.to_owned(), value);
    }

    /// The credential values in custody — what the pipeline refuses to let
    /// through tool arguments (M9.3).
    pub fn secrets_in_custody(&self) -> Vec<String> {
        self.credentials
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    /// Every configured server, by name.
    pub fn servers(&self) -> Vec<ServerConfig> {
        self.servers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    /// Stop every pooled server (Core shutdown).
    pub async fn shutdown(&self) {
        let entries: Vec<Arc<Pooled>> = self.pool.lock().await.values().cloned().collect();
        for entry in entries {
            let mut conn = entry.conn.lock().await;
            if let Some(c) = conn.take() {
                // Dropping the sender ends the writer task, which closes the
                // child's stdin; an MCP server exits when its input ends.
                drop(c);
            }
        }
        self.pool.lock().await.clear();
    }

    /// The hub as one task sees it.
    pub fn for_task(
        self: &Arc<Self>,
        tenant: &str,
        workspace: Option<&str>,
        correlation: Correlation,
    ) -> TaskHub {
        TaskHub {
            hub: Arc::clone(self),
            tenant: tenant.to_owned(),
            workspace: workspace.map(str::to_owned),
            correlation,
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
    ) -> Result<Arc<Connection>, PortError> {
        entry.state().sessions.insert(session.to_owned());
        let mut slot = entry.conn.lock().await;
        if let Some(c) = slot.as_ref() {
            if c.lock().lost.is_none() {
                return Ok(Arc::clone(c));
            }
            // A dead transport is never handed out again: the entry starts
            // a fresh process for the next caller.
            *slot = None;
            entry.state().discovery = None;
        }
        if !entry.cfg.startable() {
            return Err(PortError::clean(
                "EXTERNAL_SERVER_UNTRUSTED",
                format!(
                    "`{}` is proposed but not trusted; a person or an admin layer must trust it before it can run",
                    entry.cfg.name
                ),
            ));
        }
        match self.start(&entry.cfg).await {
            Ok(c) => {
                entry.state().failure = None;
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
            let value = self
                .credentials
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(handle)
                .cloned();
            let Some(value) = value else {
                return Err(PortError::clean(
                    "EXTERNAL_CREDENTIAL_UNAVAILABLE",
                    format!(
                        "`{}` needs the credential `{handle}`, which is not in this Core's custody",
                        cfg.name
                    ),
                ));
            };
            command.env(DEFAULT_CREDENTIAL_ENV, value);
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
        spawn_writer(stdin, rx);
        spawn_reader(stdout, Arc::clone(&shared), self.limits, cfg.name.clone());
        // The child is owned by a task that reaps it when the pipes close,
        // so a finished server never becomes a zombie.
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        let conn = Arc::new(Connection {
            pid,
            out: tx,
            shared,
            next_id: AtomicU64::new(1),
            info: ServerInfo::default(),
            limits: self.limits,
            timeout: std::time::Duration::from_millis(HANDSHAKE_TIMEOUT_MS),
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
        }))
    }

    /// Discover a server's tools, once per transport (lazy).
    async fn discover(
        &self,
        entry: &Arc<Pooled>,
        conn: &Arc<Connection>,
    ) -> Result<Discovery, PortError> {
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
        entry.state().discovery = Some(all.clone());
        Ok(all)
    }
}

fn spawn_writer(mut stdin: tokio::process::ChildStdin, mut rx: mpsc::UnboundedReceiver<String>) {
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
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
            let Some(id) = frame.id else { continue };
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

/// The hub bound to one task: the tenant and workspace that decide which
/// pool entries it may touch, and the identity every call carries.
pub struct TaskHub {
    hub: Arc<McpHub>,
    tenant: String,
    workspace: Option<String>,
    correlation: Correlation,
}

impl TaskHub {
    fn config(&self, server: &str) -> Option<ServerConfig> {
        self.hub
            .servers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(server)
            .cloned()
    }

    async fn listing_for(&self, cfg: &ServerConfig) -> ServerListing {
        let entry = self
            .hub
            .entry(&self.tenant, self.workspace.as_deref(), cfg)
            .await;
        let mut listing = ServerListing {
            name: cfg.name.clone(),
            trust: cfg.trust,
            health: Health::NotStarted,
            scopes: cfg.scopes.iter().cloned().collect(),
            layer: cfg.layer.clone(),
            pool_key: entry.key.to_string(),
            tools: vec![],
            rejected: vec![],
            dropped_over_limit: 0,
        };
        if cfg.trust != Trust::Trusted {
            listing.health = Health::Untrusted;
            return listing;
        }
        match self
            .hub
            .connection(&entry, &self.correlation.session_id)
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
                }
                Err(e) => {
                    listing.health = Health::Failed {
                        code: e.code,
                        message: e.message,
                    };
                }
            },
            Err(e) => {
                listing.health = Health::Failed {
                    code: e.code,
                    message: e.message,
                };
            }
        }
        listing
    }
}

impl McpPort for TaskHub {
    fn list<'a>(&'a self) -> BoxFuture<'a, Result<Listing, PortError>> {
        Box::pin(async move {
            let mut servers = Vec::new();
            for cfg in self.hub.servers() {
                servers.push(self.listing_for(&cfg).await);
            }
            Ok(Listing { servers })
        })
    }

    fn call<'a>(&'a self, call: ExternalCall) -> BoxFuture<'a, Result<CallResult, PortError>> {
        Box::pin(async move {
            let Some(cfg) = self.config(&call.server) else {
                return Err(PortError::clean(
                    "EXTERNAL_SERVER_UNKNOWN",
                    format!("no external server named `{}` is configured", call.server),
                ));
            };
            let entry = self
                .hub
                .entry(&self.tenant, self.workspace.as_deref(), &cfg)
                .await;
            let conn = self
                .hub
                .connection(&entry, &self.correlation.session_id)
                .await?;
            let discovery = self.hub.discover(&entry, &conn).await?;
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
                .await?;
            parse_call_result(&result, &conn.limits)
                .map_err(|e| PortError::clean(e.code, e.message))
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
