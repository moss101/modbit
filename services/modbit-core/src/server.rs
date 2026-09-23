//! Authenticated local SurfaceProtocol server (docs/30 "Local SurfaceProtocol").
//!
//! Security model: the only credential is the boot-scoped random secret the
//! Core prints once on stdout to the process that spawned it; a client that
//! cannot present it is closed after the first frame (docs/29, REQ-EV-0103).
//! Every frame is bounded (REQ-EV-0108); an oversized or malformed frame ends
//! the connection with a `ProtocolError`. Multiple clients may attach to one
//! session and resume from an event offset (REQ-EV-0010, REQ-EV-0192).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use modbit_domain::approval::ApprovalEvent;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::lease::CapabilityLeaseEvent;
use modbit_domain::session::SessionEvent;
use modbit_domain::state::StateMachine;
use modbit_domain::task::{InputMode, TaskEvent, TaskOrigin};
use modbit_domain::toolcall::ToolCallEvent;
use modbit_domain::{
    EventId, SessionId, SpaceId, TaskId, TenantId, Timestamp, ToolCallId, UserId, WorkspaceId,
};
use modbit_event_store::{
    AppendRequest, CommandOutcome, CommandRecord, EventStore, NewEvent, RecoveryOutcome,
    StoredEvent,
};
use modbit_protocol::client::BoxedStream;
use modbit_protocol::framing::{FrameError, read_frame, write_frame};
use modbit_protocol::local::{Endpoint, ReadyLine, encode_hex};
use modbit_protocol::v1::surface_frame::Body;
use modbit_protocol::v1::{
    self as wire, CommandAck, CommandEnvelope, CommandStatus, HelloAck, InputQueued,
    ObjectRangeChunk, ProtocolError, SessionCreated, SessionLeaseAcquired, SessionSnapshot,
    StoredEventFrame, SubscribeEvents, SurfaceFrame, TaskCreated, TaskView,
};
use prost::Message;
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, watch};

/// Shared Core state for M1.3.
pub struct Core {
    pub(crate) store: Arc<Mutex<EventStore>>,
    /// Latest committed store offset; subscribers wake on change.
    pub(crate) last_offset: watch::Sender<u64>,
    boot_secret: Vec<u8>,
    /// Local single-user identity for M1 (accounts arrive with the cloud plane).
    pub(crate) tenant_id: TenantId,
    pub(crate) user_id: UserId,
    /// What startup recovery did (docs/19), served to clients.
    recovery: RecoveryOutcome,
    started_at: Timestamp,
    /// Tool registry, kernel port, broker (M2.4).
    pub(crate) tools: crate::tools::ToolHost,
    /// Provider Gateway (M2.6): endpoints from the Core's environment only.
    pub(crate) gateway: modbit_providers::ProviderGateway,
    /// One-agent runtime (M2.7).
    pub(crate) runtime: crate::runtime::Runtime,
    /// The profile directory (fork worktrees live under `worktrees/`).
    pub(crate) data_dir: PathBuf,
    /// The assurance policy this Core runs under (REQ-EPR-008): the base
    /// strengthened by the organization layer, never weakened.
    pub(crate) assurance_policy: modbit_policy::AssurancePolicy,
    /// Capacity tickets (M6.2, REQ-EV-0272): the host's resource vector
    /// and the tickets alive against it.
    pub(crate) capacity: crate::capacity::Capacity,
    /// Browser sessions and their hosts (M7.1, docs/22).
    pub(crate) browser: Arc<crate::browser::BrowserSessions>,
}

/// Bounded number of events per subscription batch (REQ-EV-0108).
const BATCH: usize = 256;

/// Largest object range served in one frame (REQ-EV-0108).
const MAX_OBJECT_CHUNK: u64 = 1024 * 1024;
/// An attached engineering document is context, not a corpus (REQ-EV-0161).
const MAX_CONTEXT_DOCUMENT_BYTES: usize = 256 * 1024;

/// Fencing (docs/13, docs/33): a mutating command must present the session's
/// current lease generation in `expected_generation`.
async fn require_lease(
    core: &Core,
    cid: &Option<wire::Id>,
    env: &CommandEnvelope,
    session_id: &SessionId,
) -> Result<(), CommandAck> {
    let store = core.store.lock().await;
    let session = match store.session(session_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err(reject(
                cid.clone(),
                "UNKNOWN_SESSION",
                session_id.to_string(),
            ));
        }
        Err(e) => return Err(reject(cid.clone(), error_code(&e), e.to_string())),
    };
    match env.expected_generation {
        Some(g) if g == session.lease_generation && g > 0 => Ok(()),
        Some(g) => Err(reject(
            cid.clone(),
            "STALE_LEASE",
            format!(
                "presented lease generation {g}, current is {} (owner {:?}); re-acquire the session lease",
                session.lease_generation, session.lease_owner
            ),
        )),
        None => Err(reject(
            cid.clone(),
            "LEASE_REQUIRED",
            "mutating commands must present expected_generation from AcquireSessionLease",
        )),
    }
}

/// Run the daemon until the listener fails or the process is signalled, as
/// `tenant` (M8.2: a Cloud Core Worker's Core serves the cloud tenant whose
/// log it materializes); `None` is the local profile's tenant.
pub async fn run_as(
    data_dir: PathBuf,
    idle_exit_secs: Option<u64>,
    tenant: Option<TenantId>,
) -> Result<()> {
    let tenant_id = tenant.unwrap_or_else(|| TenantId::from_bytes([0xA1; 16]));
    std::fs::create_dir_all(&data_dir)?;
    acquire_singleton_lock(&data_dir)?;
    let mut store = EventStore::open(&data_dir.join("core")).context("opening core store")?;
    // docs/33: recovery completes before the endpoint is bound and CoreReady is announced.
    let recovery = store.recover_on_start().context("startup recovery")?;
    eprintln!(
        "modbit-core: recovery complete: boot_generation={} events_verified={} aggregates={} projections_rebuilt={} sessions={} tasks={} in {}ms{}",
        recovery.boot_generation,
        recovery.events_verified,
        recovery.aggregates_verified,
        recovery.projections_rebuilt,
        recovery.sessions,
        recovery.tasks,
        recovery.recovery_ms,
        if recovery.notes.is_empty() {
            String::new()
        } else {
            format!("; notes: {}", recovery.notes.join(" | "))
        }
    );
    // Interrupted agent loops suspend at a turn boundary (docs/14); nothing re-executes.
    let suspended =
        crate::runtime::reconcile_after_restart(&mut store, tenant_id, recovery.boot_generation);
    if !suspended.is_empty() {
        eprintln!(
            "modbit-core: suspended {} running task(s) after restart",
            suspended.len()
        );
    }
    let start = store.last_offset()?;
    let boot_secret: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
    let nonce = encode_hex(&(0..6).map(|_| rand::random::<u8>()).collect::<Vec<_>>());
    let endpoint = Endpoint::for_dir(&data_dir, &nonce).context("choosing local endpoint")?;
    let (tx, _) = watch::channel(start);
    let boot_generation = recovery.boot_generation;
    let browser = Arc::new(crate::browser::BrowserSessions::default());
    let gateway = modbit_providers::ProviderGateway::new(modbit_providers::endpoints_from_env())
        .with_policy(modbit_providers::OrgModelPolicy::from_env());
    let core = Arc::new(Core {
        store: Arc::new(Mutex::new(store)),
        last_offset: tx,
        boot_secret: boot_secret.clone(),
        tenant_id,
        user_id: UserId::from_bytes([0xB1; 16]),
        recovery,
        started_at: Timestamp::now(),
        tools: crate::tools::ToolHost::new(
            &data_dir,
            boot_generation,
            crate::browser::port(&browser),
            gateway.clone(),
        )
        .context("tool host")?,
        gateway,
        runtime: crate::runtime::Runtime::default(),
        data_dir: data_dir.clone(),
        assurance_policy: {
            let (policy, ignored) = crate::assurance::org_policy().context("assurance policy")?;
            for i in &ignored {
                eprintln!(
                    "modbit-core: MODBIT_POLICY_FILE cannot weaken the base policy; ignored: {i}"
                );
            }
            policy
        },
        capacity: crate::capacity::from_env()
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("capacity")?,
        browser,
    });
    let _ = std::fs::remove_file(data_dir.join("core.ready"));
    let listener = Listener::bind(&endpoint)
        .await
        .context("binding local endpoint")?;
    // Ready line: to the supervising parent on stdout, and to `core.ready`
    // (owner-only, 0600) in the profile so a headless client can attach to the
    // running Core instead of racing it for the profile lock (REQ-PX-000).
    let ready_line = ReadyLine {
        endpoint: endpoint.clone(),
        boot_secret_hex: encode_hex(&boot_secret),
        protocol: (
            modbit_protocol::PROTOCOL_VERSION.major,
            modbit_protocol::PROTOCOL_VERSION.minor,
        ),
    }
    .render();
    println!("{ready_line}");
    use std::io::Write;
    std::io::stdout().flush().ok();
    let ready_path = data_dir.join("core.ready");
    write_owner_only(&ready_path, format!("{ready_line}\n").as_bytes());
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    // Idle exit (headless clients): a Core spawned by a CLI stays up for later
    // invocations to attach to, and leaves on its own once no client has been
    // connected for `idle_exit_secs`.
    let connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let last_activity = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));
    let mut idle_tick = tokio::time::interval(std::time::Duration::from_secs(1));
    // One accept future lives across ticks: the Windows named-pipe accept is
    // not cancel-safe (its instance is consumed before the connect completes).
    let mut accept = Box::pin(listener.accept());
    loop {
        tokio::select! {
            accepted = &mut accept => {
                accept = Box::pin(listener.accept());
                let stream = accepted.context("accept")?;
                let core = Arc::clone(&core);
                let connections = Arc::clone(&connections);
                let last_activity = Arc::clone(&last_activity);
                connections.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::spawn(async move {
                    if let Err(e) = serve_connection(core, stream).await {
                        eprintln!("modbit-core: connection ended: {e}");
                    }
                    connections.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    *last_activity.lock().expect("activity") = std::time::Instant::now();
                });
            }
            _ = idle_tick.tick() => {
                if let Some(secs) = idle_exit_secs
                    && connections.load(std::sync::atomic::Ordering::SeqCst) == 0
                    && last_activity.lock().expect("activity").elapsed().as_secs() >= secs
                {
                    eprintln!("modbit-core: idle for {secs}s with no client; exiting");
                    break;
                }
            }
            _ = &mut shutdown => break,
        }
    }
    listener.cleanup();
    // M9.4: every pooled external server is stopped with the Core, so a
    // stopping Core leaves no MCP child process behind.
    core.tools.mcp.shutdown().await;
    let _ = std::fs::remove_file(&ready_path);
    Ok(())
}

/// Write `bytes` to `path` readable by the owner only (the boot secret lives there).
fn write_owner_only(path: &Path, bytes: &[u8]) {
    let _ = std::fs::remove_file(path);
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            let _ = f.write_all(bytes);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::write(path, bytes);
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn acquire_singleton_lock(data_dir: &Path) -> Result<()> {
    // docs/33 step 4: one Core per user profile. The lock is an OS file lock
    // held for the process lifetime, so two Cores racing for one profile can
    // never both proceed (a loser would otherwise rebind the shared socket
    // path); a crashed owner's lock is released by the OS and reclaimed.
    let lock = data_dir.join("core.lock");
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock)
        .with_context(|| format!("opening {}", lock.display()))?;
    let mut attempts = 0;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => {
                let pid = std::fs::read_to_string(&lock)
                    .ok()
                    .and_then(|t| t.trim().parse::<u32>().ok())
                    .unwrap_or(0);
                let owner = process_command(pid);
                // A tethered Core whose supervising parent is gone is an orphan
                // by contract (docs/33): reclaim the profile from it once.
                if attempts == 0 && orphaned_tethered_core(pid, &owner) {
                    eprintln!(
                        "modbit-core: reclaiming the profile from orphaned tethered Core pid {pid}"
                    );
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .status();
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    attempts += 1;
                    continue;
                }
                anyhow::bail!(
                    "another modbit-core (pid {pid}: {owner}) owns {}",
                    data_dir.display()
                );
            }
            Err(std::fs::TryLockError::Error(e)) => {
                return Err(anyhow::Error::from(e).context("locking core.lock"));
            }
        }
    }
    use std::io::Write;
    file.set_len(0)?;
    file.write_all(std::process::id().to_string().as_bytes())?;
    file.flush()?;
    // Held until exit: dropping the handle would release the lock.
    std::mem::forget(file);
    Ok(())
}

/// The owner's command line plus parent pid, state, age and the parent's
/// command, for the refusal message (diagnosability).
#[cfg(unix)]
fn process_command(pid: u32) -> String {
    let ps = |args: &[&str]| {
        std::process::Command::new("ps")
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default()
    };
    let own = ps(&["-o", "command=", "-p", &pid.to_string()]);
    let meta = ps(&["-o", "ppid=,stat=,etime=", "-p", &pid.to_string()]);
    let ppid = meta.split_whitespace().next().unwrap_or("?").to_owned();
    let parent = ps(&["-o", "command=", "-p", &ppid]);
    format!("{own} [ppid={ppid} stat/etime={meta} parent={parent}]")
}

/// A `--tether-stdin` Core re-parented to pid 1: its supervisor is gone.
#[cfg(unix)]
fn orphaned_tethered_core(_pid: u32, owner: &str) -> bool {
    owner.contains("--tether-stdin") && owner.contains("[ppid=1 ")
}

#[cfg(not(unix))]
fn orphaned_tethered_core(_pid: u32, _owner: &str) -> bool {
    false
}

/// The owner's command line, for the refusal message (diagnosability).
#[cfg(not(unix))]
fn process_command(pid: u32) -> String {
    format!("pid {pid}")
}

/// Platform listener.
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
            // Only the owning user may connect.
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
            // Create the next instance before handing this one out.
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

fn error_frame(code: &str, message: impl Into<String>) -> SurfaceFrame {
    SurfaceFrame {
        body: Some(Body::Error(ProtocolError {
            code: code.into(),
            message: message.into(),
        })),
    }
}

async fn serve_connection(core: Arc<Core>, mut stream: BoxedStream) -> Result<()> {
    // 1. Handshake: first frame must be ClientHello with the boot secret.
    let first = match read_frame(&mut stream).await {
        Ok(Some(f)) => f,
        Ok(None) => return Ok(()),
        Err(e) => {
            let _ = write_frame(&mut stream, &error_frame("MALFORMED_FRAME", e.to_string())).await;
            return Ok(());
        }
    };
    let Some(Body::ClientHello(hello)) = first.body else {
        let _ = write_frame(
            &mut stream,
            &error_frame("HANDSHAKE_REQUIRED", "first frame must be ClientHello"),
        )
        .await;
        return Ok(());
    };
    let presented = hello
        .auth
        .as_ref()
        .map(|a| a.boot_secret.as_slice())
        .unwrap_or(&[]);
    let ok =
        presented.len() == core.boot_secret.len() && bool::from(presented.ct_eq(&core.boot_secret));
    if !ok {
        // Deliberately no detail; the connection simply ends (docs/29: "gets nothing").
        let _ = write_frame(
            &mut stream,
            &error_frame("UNAUTHENTICATED", "boot secret rejected"),
        )
        .await;
        return Ok(());
    }
    let client_version = hello.hello.as_ref().and_then(|h| h.protocol_version);
    let ours = modbit_protocol::PROTOCOL_VERSION;
    let compatible = client_version.is_some_and(|v| v.major == ours.major);
    // REQ-EV-0043: the ProtocolCapabilitySet for this client kind, fixed for
    // the connection. What a client may ask for is not what a task may do:
    // execution authority lives in the task's leases and policy.
    let client_kind = hello.hello.as_ref().map(|h| h.client_kind).unwrap_or(0);
    let capabilities = client_capabilities(client_kind);
    write_frame(
        &mut stream,
        &SurfaceFrame {
            body: Some(Body::HelloAck(HelloAck {
                protocol_version: Some(ours),
                compatible,
                upgrade_required: !compatible,
                reason: if compatible {
                    String::new()
                } else {
                    format!("protocol major mismatch: client {client_version:?}, core {ours:?}")
                },
                supported_command_types: [
                    "CreateSession",
                    "CreateTask",
                    "GetSessionSnapshot",
                    "SubscribeEvents",
                    "GetRecoveryReport",
                    "AcquireSessionLease",
                    "QueueInput",
                    "ReadObjectRange",
                    "InvokeTool",
                    "ListTools",
                    "ListApprovals",
                    "ResolveApproval",
                    "EmergencyStop",
                    "GetEffectReceipts",
                    "GetCapabilityLeases",
                    "ListModels",
                    "ListLanguages",
                    "GetContextInspector",
                    "GetRoutingPlan",
                    "GetWorkGraph",
                    "GetAgentGraph",
                    "GetCapacity",
                    "GetAttention",
                    "GetPlan",
                    "RevisePlan",
                    "AdmitReviewEnvironment",
                    "DisposeReviewEnvironment",
                    "AdmitRoutingPlan",
                    "CompileRoutingPlan",
                    "ConfigureProvider",
                    "ConfigureForge",
                    "ProposeExternalServer",
                    "TrustExternalServer",
                    "ConfigureExternalCredential",
                    "ConfigureSandboxGateway",
                    "ExportHandoff",
                    "RebindTaskWorkspace",
                    "ImportObjects",
                    "TrustRepository",
                    "ListStarterTasks",
                    "ActivateModelRegistry",
                    "GetModelRegistry",
                    "MaterializeOutcomeStatistics",
                    "GetOutcomeStatistics",
                    "GetTaskEconomics",
                    "GetRequestOutcome",
                    "ReconcileUsage",
                    "SetTaskSelection",
                    "AttachContextDocument",
                    "AllowUnsupportedLanguage",
                    "PublishOutcomeBaseline",
                    "ProbeModel",
                    "StartTask",
                    "CancelTask",
                    "GetTaskStatus",
                    "GetReviewBundle",
                    "GetCodeView",
                    "DecideReview",
                    "ApplyUserPatch",
                    "SubmitExternalDiagnostics",
                    "OpenPullRequest",
                    "UpdatePullRequest",
                    "IngestCiResults",
                    "UndoToolCall",
                    "AskSideQuestion",
                    "ListQuestions",
                    "RespondToQuestion",
                    "IngestAttachment",
                ]
                .map(String::from)
                .to_vec(),
                client_capabilities: capabilities.iter().map(|c| (*c).to_owned()).collect(),
            })),
        },
    )
    .await?;
    if !compatible {
        return Ok(());
    }

    // 2. Command / subscription loop. A subscription streams events between commands.
    let mut subscription: Option<(SessionId, u64)> = None;
    let mut rx = core.last_offset.subscribe();
    // M7.1: this connection's number and, once it attaches as a browser
    // host, the queue of requests the Core wants written to it.
    let connection = CONNECTIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let mut host_rx: Option<tokio::sync::mpsc::Receiver<wire::BrowserHostRequest>> = None;
    // M8.8: the browser views this connection watches — frames arrive here.
    let mut views = Views::default();
    let outcome = serve_frames(
        &core,
        &mut stream,
        &capabilities,
        client_kind,
        &mut subscription,
        &mut rx,
        connection,
        &mut host_rx,
        &mut views,
    )
    .await;
    core.browser.connection_closed(connection).await;
    views.unwatch_all(&core).await;
    outcome
}

/// The browser views one connection watches (M8.8): one frame queue for
/// all of them, and the watcher id each host knows this connection by.
#[derive(Default)]
struct Views {
    rx: Option<
        tokio::sync::mpsc::Receiver<(
            modbit_browser::BrowserSessionId,
            crate::browser_cloud::ViewFrame,
        )>,
    >,
    tx: Option<
        tokio::sync::mpsc::Sender<(
            modbit_browser::BrowserSessionId,
            crate::browser_cloud::ViewFrame,
        )>,
    >,
    watching: std::collections::HashMap<modbit_browser::BrowserSessionId, u64>,
}

static WATCHERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Views {
    fn queue(
        &mut self,
    ) -> tokio::sync::mpsc::Sender<(
        modbit_browser::BrowserSessionId,
        crate::browser_cloud::ViewFrame,
    )> {
        if let Some(tx) = &self.tx {
            return tx.clone();
        }
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        self.rx = Some(rx);
        self.tx = Some(tx.clone());
        tx
    }

    async fn unwatch_all(&mut self, core: &Arc<Core>) {
        for (bsid, id) in self.watching.drain() {
            if let Some((_, ctl)) = core.browser.view_control(bsid).await {
                let _ = ctl
                    .send(crate::browser_cloud::ViewControl::Unwatch(id))
                    .await;
            }
        }
    }
}

/// `WatchBrowserView` on this connection: frames of the session's view
/// are written to it until it unwatches or disconnects.
async fn watch_browser_view(
    core: &Arc<Core>,
    env: CommandEnvelope,
    views: &mut Views,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::WatchBrowserView::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "WatchBrowserView");
    };
    let Some(bsid) = p
        .browser_session_id
        .as_ref()
        .and_then(id16)
        .map(modbit_browser::BrowserSessionId::from_bytes)
    else {
        return reject(cid, "BAD_PAYLOAD", "browser_session_id required");
    };
    let Some(rec) = core.browser.get(bsid).await else {
        return reject(cid, "NO_SUCH_SESSION", bsid.to_string());
    };
    if rec.closed {
        return reject(cid, "SESSION_CLOSED", bsid.to_string());
    }
    let Some(host) = &rec.host else {
        return reject(cid, "NO_BROWSER_HOST", "no host is attached to the session");
    };
    let Some(ctl) = host.view.clone() else {
        return reject(
            cid,
            "HOST_NOT_STREAMABLE",
            format!(
                "the session's {} host shows its view on the desktop itself; nothing to stream",
                host.kind
            ),
        );
    };
    let id = *views
        .watching
        .entry(bsid)
        .or_insert_with(|| WATCHERS.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    let queue = views.queue();
    let (ftx, mut frx) = tokio::sync::mpsc::channel::<crate::browser_cloud::ViewFrame>(4);
    // Frames of this session into the connection's one queue (tagged).
    tokio::spawn(async move {
        while let Some(f) = frx.recv().await {
            if queue.send((bsid, f)).await.is_err() {
                break;
            }
        }
    });
    if ctl
        .send(crate::browser_cloud::ViewControl::Watch {
            id,
            tx: ftx,
            max_width: p.max_width,
            max_height: p.max_height,
            quality: p.quality,
        })
        .await
        .is_err()
    {
        views.watching.remove(&bsid);
        return reject(cid, "HOST_GONE", "the session's host is gone");
    }
    accept(
        cid,
        false,
        wire::BrowserViewWatched {
            browser_session_id: Some(wire_id(bsid.as_bytes())),
            host_kind: host.kind.clone(),
            controller: match rec.lease.controller {
                modbit_browser::Controller::Agent => "AGENT".into(),
                modbit_browser::Controller::User => "USER".into(),
            },
            lease_generation: rec.lease.generation,
        }
        .encode_to_vec(),
    )
}

async fn unwatch_browser_view(
    core: &Arc<Core>,
    env: CommandEnvelope,
    views: &mut Views,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::UnwatchBrowserView::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "UnwatchBrowserView");
    };
    let Some(bsid) = p
        .browser_session_id
        .as_ref()
        .and_then(id16)
        .map(modbit_browser::BrowserSessionId::from_bytes)
    else {
        return reject(cid, "BAD_PAYLOAD", "browser_session_id required");
    };
    let was = views.watching.remove(&bsid);
    if let Some(id) = was
        && let Some((_, ctl)) = core.browser.view_control(bsid).await
    {
        let _ = ctl
            .send(crate::browser_cloud::ViewControl::Unwatch(id))
            .await;
    }
    accept(
        cid,
        false,
        wire::BrowserViewUnwatched {
            was_watching: was.is_some(),
        }
        .encode_to_vec(),
    )
}

/// Connection counter (a host is tied to the connection it attached on).
static CONNECTIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[allow(clippy::too_many_arguments)]
async fn serve_frames(
    core: &Arc<Core>,
    stream: &mut BoxedStream,
    capabilities: &[&str],
    client_kind: i32,
    subscription: &mut Option<(SessionId, u64)>,
    rx: &mut watch::Receiver<u64>,
    connection: u64,
    host_rx: &mut Option<tokio::sync::mpsc::Receiver<wire::BrowserHostRequest>>,
    views: &mut Views,
) -> Result<()> {
    loop {
        // Drain any events the subscriber has not seen yet, in bounded batches.
        if let Some((session, after)) = subscription.as_mut() {
            loop {
                let batch = core
                    .store
                    .lock()
                    .await
                    .read_session(session, *after, BATCH)?;
                if batch.is_empty() {
                    break;
                }
                for ev in &batch {
                    write_frame(
                        stream,
                        &SurfaceFrame {
                            body: Some(Body::Event(to_wire(ev))),
                        },
                    )
                    .await?;
                    *after = ev.offset;
                }
            }
        }
        let frame = tokio::select! {
            f = read_frame(stream) => match f {
                Ok(Some(f)) => Some(f),
                Ok(None) => return Ok(()),
                Err(e @ (FrameError::TooLarge { .. } | FrameError::Malformed(_) | FrameError::EmptyBody)) => {
                    let code = if matches!(e, FrameError::TooLarge { .. }) { "FRAME_TOO_LARGE" } else { "MALFORMED_FRAME" };
                    let _ = write_frame(stream, &error_frame(code, e.to_string())).await;
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            },
            changed = rx.changed(), if subscription.is_some() => {
                if changed.is_err() { return Ok(()); }
                None
            }
            // M7.1: a request for the browser host attached on this connection.
            req = async { host_rx.as_mut().expect("guarded").recv().await }, if host_rx.is_some() => {
                match req {
                    Some(r) => {
                        write_frame(stream, &SurfaceFrame { body: Some(Body::BrowserRequest(r)) }).await?;
                    }
                    None => { *host_rx = None; }
                }
                None
            }
            // M8.8: a frame of a browser view this connection watches.
            f = async { views.rx.as_mut().expect("guarded").recv().await }, if views.rx.is_some() => {
                match f {
                    Some((bsid, frame)) => {
                        write_frame(stream, &SurfaceFrame { body: Some(Body::BrowserFrame(wire::BrowserViewFrame {
                            browser_session_id: Some(wire_id(bsid.as_bytes())),
                            jpeg: frame.jpeg,
                            width: frame.width,
                            height: frame.height,
                            page_width: frame.page_width,
                            page_height: frame.page_height,
                            seq: frame.seq,
                            url: frame.url,
                            title: frame.title,
                        })) }).await?;
                    }
                    None => { views.rx = None; }
                }
                None
            }
        };
        let Some(frame) = frame else { continue };
        match frame.body {
            Some(Body::Command(env)) => {
                // REQ-EV-0043: a command that needs a capability this client
                // kind does not hold is refused at the transport, before the
                // Core looks at the task; the task itself stays valid.
                let ack = match required_client_capability(&env) {
                    Some(need) if !capabilities.contains(&need) => reject(
                        env.command_id.clone(),
                        "CLIENT_CAPABILITY",
                        format!(
                            "`{}` needs the client capability `{need}`, which a {} client does not hold (it holds: {})",
                            env.command_type,
                            client_kind_label(client_kind),
                            capabilities.join(", ")
                        ),
                    ),
                    // M7.1: attaching a host binds this connection's writer to
                    // the session, so it is handled here, not in handle_command.
                    _ if env.command_type == "AttachBrowserHost" => {
                        let (tx, new_rx) = tokio::sync::mpsc::channel(32);
                        let ack = attach_browser_host(core, env, tx, connection).await;
                        if ack.status == wire::CommandStatus::Accepted as i32 {
                            *host_rx = Some(new_rx);
                        }
                        ack
                    }
                    // M8.8: watching binds this connection's writer to the view's frames.
                    _ if env.command_type == "WatchBrowserView" => {
                        watch_browser_view(core, env, views).await
                    }
                    _ if env.command_type == "UnwatchBrowserView" => {
                        unwatch_browser_view(core, env, views).await
                    }
                    _ => handle_command(core, env).await,
                };
                write_frame(
                    stream,
                    &SurfaceFrame {
                        body: Some(Body::CommandAck(ack)),
                    },
                )
                .await?;
            }
            Some(Body::Subscribe(SubscribeEvents {
                session_id,
                after_offset,
            })) => {
                let Some(sid) = session_id.and_then(|id| id16(&id)) else {
                    write_frame(
                        stream,
                        &error_frame("BAD_SUBSCRIBE", "session_id must be 16 bytes"),
                    )
                    .await?;
                    return Ok(());
                };
                // REQ-EV-0010: a cursor beyond the log cannot be resumed from; the
                // client must rehydrate from a snapshot rather than silently skip.
                let last = core.store.lock().await.last_offset()?;
                if after_offset > last {
                    write_frame(
                        stream,
                        &error_frame("INVALID_CURSOR", format!("after_offset {after_offset} is beyond the log ({last}); rehydrate from a snapshot")),
                    )
                    .await?;
                    return Ok(());
                }
                *subscription = Some((SessionId::from_bytes(sid), after_offset));
                rx.mark_changed();
            }
            Some(Body::ClientHello(_)) => {
                write_frame(
                    stream,
                    &error_frame("DUPLICATE_HELLO", "handshake already completed"),
                )
                .await?;
                return Ok(());
            }
            other => {
                write_frame(
                    stream,
                    &error_frame("UNEXPECTED_FRAME", format!("{other:?}")),
                )
                .await?;
                return Ok(());
            }
        }
    }
}

fn id16(id: &wire::Id) -> Option<[u8; 16]> {
    id.value.as_slice().try_into().ok()
}

/// REQ-EV-0043 (docs/30, docs/23): the ProtocolCapabilitySet by client kind.
/// `task.author` and `events.subscribe` are the headless floor; the UI
/// surfaces (`ui.selection`, `ui.code_view`) belong to clients that have
/// them; human decisions (`approval.resolve`, `question.answer`,
/// `review.decide`) to clients a person drives; a worker or a sandbox guest
/// holds neither.
fn client_capabilities(kind: i32) -> Vec<&'static str> {
    use modbit_protocol::v1::ClientKind;
    let kind = ClientKind::try_from(kind).unwrap_or(ClientKind::Unspecified);
    match kind {
        ClientKind::Desktop => vec![
            "task.author",
            "events.subscribe",
            "session.control",
            "approval.resolve",
            "question.answer",
            "review.decide",
            "attachments.ingest",
            "provider.configure",
            "repository.trust",
            "ui.selection",
            "ui.code_view",
            // M7.1: only the desktop hosts a browser (a sandboxed
            // WebContentsView in Electron main); a headless client cannot.
            "browser.host",
        ],
        ClientKind::IdeAdapter => vec![
            "task.author",
            "events.subscribe",
            "session.control",
            "approval.resolve",
            "question.answer",
            "review.decide",
            "attachments.ingest",
            "repository.trust",
            "ui.selection",
            "ui.code_view",
        ],
        ClientKind::Cli => vec![
            "task.author",
            "events.subscribe",
            "session.control",
            "approval.resolve",
            "question.answer",
            "review.decide",
            "attachments.ingest",
            "provider.configure",
            "repository.trust",
        ],
        // M8.2: the Cloud Core Worker acts for the cloud's principals on its
        // local Core — it authors and controls tasks, decides approvals and
        // answers questions as the API relays them, trusts the workspace a
        // principal named for a task, and mirrors the cloud log in
        // (`session.mirror`); it hosts no browser and no UI.
        ClientKind::CloudWorker => vec![
            "task.author",
            "events.subscribe",
            "attachments.ingest",
            "session.control",
            "approval.resolve",
            "question.answer",
            "review.decide",
            "provider.configure",
            "repository.trust",
            "session.mirror",
            "sandbox.configure",
        ],
        ClientKind::SandboxGuest | ClientKind::Unspecified => vec!["events.subscribe"],
    }
}

fn client_kind_label(kind: i32) -> &'static str {
    use modbit_protocol::v1::ClientKind;
    match ClientKind::try_from(kind).unwrap_or(ClientKind::Unspecified) {
        ClientKind::Desktop => "DESKTOP",
        ClientKind::Cli => "CLI",
        ClientKind::IdeAdapter => "IDE_ADAPTER",
        ClientKind::CloudWorker => "CLOUD_WORKER",
        ClientKind::SandboxGuest => "SANDBOX_GUEST",
        ClientKind::Unspecified => "UNSPECIFIED",
    }
}

/// The client capability a command needs, when it needs one beyond the
/// headless floor. Read-only queries need none.
fn required_client_capability(env: &CommandEnvelope) -> Option<&'static str> {
    Some(match env.command_type.as_str() {
        "SetTaskSelection" => {
            let source = wire::SetTaskSelection::decode(env.payload.as_slice())
                .map(|p| p.source)
                .unwrap_or_default();
            if source == "cli" {
                "task.author"
            } else {
                "ui.selection"
            }
        }
        "GetCodeView" => "ui.code_view",
        "DecideReview" => "review.decide",
        "ApplyUserPatch" | "SubmitExternalDiagnostics" => "task.author",
        "OpenPullRequest" | "UpdatePullRequest" | "IngestCiResults" => "review.decide",
        "ResolveApproval" => "approval.resolve",
        "RespondToQuestion" | "AskSideQuestion" => "question.answer",
        // A late invoice changes what a request is said to have cost: the
        // same class of decision as configuring the provider (REQ-EPR-010).
        "ConfigureProvider"
        | "ActivateModelRegistry"
        | "ProbeModel"
        | "ConfigureForge"
        | "ReconcileUsage" => "provider.configure",
        // Proposing costs nothing and grants nothing, so any client that can
        // author a task may do it; trusting a program the host did not write
        // to run against this workspace is the same class of decision as
        // trusting the repository, and is held to the same capability.
        "ProposeExternalServer" => "task.author",
        "TrustExternalServer" | "ConfigureExternalCredential" => "repository.trust",
        "ConfigureSandboxGateway" => "sandbox.configure",
        "ExportHandoff" => "task.author",
        "GetEnvironment" | "RebuildEnvironment" => "task.author",
        "ListMemory" => "task.author",
        "PromoteMemory" | "ForgetMemory" => "task.author",
        "RebindTaskWorkspace" | "ImportObjects" => "session.mirror",
        "TrustRepository" => "repository.trust",
        "EmergencyStop" => "session.control",
        "AttachBrowserHost"
        | "BrowserHostResponse"
        | "RegisterBrowserCredential"
        | "ForgetBrowserCredential" => "browser.host",
        "OpenBrowserSession" | "CloseBrowserSession" => "task.author",
        "SetBrowserControl" => "session.control",
        "WatchBrowserView" | "UnwatchBrowserView" => "events.subscribe",
        "BrowserViewInput" => "session.control",
        "ImportMirroredEvents" | "ReadMirrorEvents" => "session.mirror",
        "IngestAttachment" | "AttachContextDocument" => "attachments.ingest",
        "CreateSession"
        | "CreateTask"
        | "StartTask"
        | "CancelTask"
        | "QueueInput"
        | "AcquireSessionLease"
        | "InvokeTool"
        | "UndoToolCall"
        | "ForkTask"
        | "RewindTask"
        | "AdmitRoutingPlan"
        | "CompileRoutingPlan"
        | "PublishOutcomeBaseline"
        | "AllowUnsupportedLanguage"
        | "MaterializeOutcomeStatistics"
        | "ReconcileToolCall"
        | "RevisePlan"
        | "AdmitReviewEnvironment"
        | "DisposeReviewEnvironment" => "task.author",
        _ => return None,
    })
}

pub(crate) fn wire_id(b: &[u8; 16]) -> wire::Id {
    wire::Id { value: b.to_vec() }
}

/// A memory item view (from `crate::memory`) to the wire message (M9.1).
fn memory_item_view(v: &serde_json::Value) -> wire::MemoryItemView {
    let s = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let strs = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let record_type = v
        .get("record_type")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_owned();
    let source = v
        .get("source")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_owned();
    let sensitivity = v
        .get("sensitivity")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_owned();
    let status = v
        .get("status")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_owned();
    wire::MemoryItemView {
        id: s("id"),
        scope: s("scope"),
        record_type,
        topic: s("topic"),
        content: s("content"),
        source,
        author: s("author"),
        confidence: v
            .get("confidence")
            .and_then(serde_json::Value::as_f64)
            .map(|f| f as f32)
            .unwrap_or(0.0),
        created_at_ms: v
            .get("created_at_ms")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        expires_at_ms: v
            .get("expires_at_ms")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        sensitivity,
        status,
        supersedes: strs("supersedes"),
        conflicts: strs("conflicts"),
        last_validation_revision: s("last_validation_revision"),
        validated: v
            .get("validated")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }
}

/// The task's workspace git HEAD, if it has a workspace root and a HEAD — a
/// repository memory fact binds to it (docs/19). `None` when unknown.
async fn current_revision_of(
    _core: &Arc<Core>,
    task: &modbit_domain::task::Task,
) -> Option<String> {
    let root = task.workspace_root.clone()?;
    tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let rev = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (!rev.is_empty()).then_some(rev)
    })
    .await
    .ok()
    .flatten()
}

fn to_wire(ev: &StoredEvent) -> StoredEventFrame {
    let e = &ev.envelope;
    let payload = serde_json::to_vec(&e.payload).unwrap_or_default();
    StoredEventFrame {
        offset: ev.offset,
        event: Some(wire::EventEnvelope {
            event_id: Some(wire_id(e.event_id.as_bytes())),
            tenant_id: Some(wire_id(e.tenant_id.as_bytes())),
            session_id: Some(wire_id(e.session_id.as_bytes())),
            run_id: e.run_id.map(|r| wire_id(r.as_bytes())),
            sequence: e.sequence,
            causation_id: e.causation_id.map(|c| wire_id(c.as_bytes())),
            event_type: e.event_type.clone(),
            schema_version: e.schema_version,
            payload,
            aggregate_type: e.aggregate_type.as_str().to_owned(),
            aggregate_id: Some(wire_id(&e.aggregate_id)),
            task_id: e.task_id.map(|t| wire_id(t.as_bytes())),
            recorded_at: Some(prost_types::Timestamp {
                seconds: e.occurred_at.millis().div_euclid(1000),
                nanos: (e.occurred_at.millis().rem_euclid(1000) * 1_000_000) as i32,
            }),
        }),
    }
}

/// The session record, rebuilt from the task's events when this Core has
/// not seen it since its start (M7.1).
async fn browser_session_record(
    core: &Arc<Core>,
    bsid: modbit_browser::BrowserSessionId,
    task_id: Option<&wire::Id>,
) -> Result<crate::browser::SessionRecord, Box<dyn FnOnce(Option<wire::Id>) -> CommandAck + Send>> {
    if let Some(rec) = core.browser.get(bsid).await {
        return Ok(rec);
    }
    let Some(task_id) = task_id.and_then(id16).map(TaskId::from_bytes) else {
        return Err(Box::new(move |cid| {
            reject(
                cid,
                "NO_SUCH_SESSION",
                format!(
                    "browser session {bsid} is not open in this Core (name task_id to rebuild it)"
                ),
            )
        }));
    };
    let (task, events) = {
        let store = core.store.lock().await;
        let task = match store.task(&task_id) {
            Ok(Some(t)) => t,
            Ok(None) => {
                return Err(Box::new(move |cid| {
                    reject(cid, "UNKNOWN_TASK", task_id.to_string())
                }));
            }
            Err(e) => {
                let (code, msg) = (error_code(&e).to_owned(), e.to_string());
                return Err(Box::new(move |cid| reject(cid, &code, msg)));
            }
        };
        (task, crate::browser::task_events(&store, task_id))
    };
    match crate::browser::from_events(bsid, task_id, task.session_id, &events) {
        Some(rec) => {
            core.browser.restore(bsid, rec.clone()).await;
            Ok(rec)
        }
        None => Err(Box::new(move |cid| {
            reject(
                cid,
                "NO_SUCH_SESSION",
                format!("task {task_id} has no browser session {bsid}"),
            )
        })),
    }
}

/// M7.1: a `browser.host` client binds this connection to a session.
async fn attach_browser_host(
    core: &Arc<Core>,
    env: CommandEnvelope,
    tx: tokio::sync::mpsc::Sender<wire::BrowserHostRequest>,
    connection: u64,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::AttachBrowserHost::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "AttachBrowserHost");
    };
    let Some(bsid) = p
        .browser_session_id
        .as_ref()
        .and_then(id16)
        .map(modbit_browser::BrowserSessionId::from_bytes)
    else {
        return reject(cid, "BAD_PAYLOAD", "browser_session_id required");
    };
    // The host states how the view is isolated (docs/22: Node disabled,
    // strict context isolation, the renderer sandbox); a view without
    // them never becomes the agent's browser.
    if p.node_integration || !p.context_isolated || !p.sandboxed {
        return reject(
            cid,
            "HOST_NOT_ISOLATED",
            format!(
                "the view must run sandboxed with context isolation and without Node (sandboxed={}, context_isolated={}, node_integration={})",
                p.sandboxed, p.context_isolated, p.node_integration
            ),
        );
    }
    let rec = match browser_session_record(core, bsid, p.task_id.as_ref()).await {
        Ok(r) => r,
        Err(ack) => return ack(cid),
    };
    if rec.closed {
        return reject(cid, "SESSION_CLOSED", bsid.to_string());
    }
    if p.partition != rec.partition {
        return reject(
            cid,
            "PARTITION_MISMATCH",
            format!(
                "the view is in `{}`; the session's partition is `{}`",
                p.partition, rec.partition
            ),
        );
    }
    let host_kind = if p.host_kind.is_empty() {
        "unknown".to_owned()
    } else {
        p.host_kind.clone()
    };
    let lease = match core
        .browser
        .attach(
            bsid,
            crate::browser::HostLink {
                kind: host_kind.clone(),
                tx,
                connection,
                view: None,
            },
        )
        .await
    {
        Ok(l) => l,
        Err(crate::browser::AttachRefusal::NoSuchSession) => {
            return reject(cid, "NO_SUCH_SESSION", bsid.to_string());
        }
        Err(crate::browser::AttachRefusal::HostConflict { kind }) => {
            // IMP-EV-0084 / IMP-EV-0110: one controller per session — a
            // second host, on another connection, does not take it over.
            return reject(
                cid,
                "HOST_CONFLICT",
                format!("a {kind} host already holds browser session {bsid} on a live connection"),
            );
        }
    };
    let offset = match crate::runtime::append_batch(
        &mut *core.store.lock().await,
        core,
        crate::runtime::Lineage::task(core.tenant_id, rec.session_id, rec.task_id),
        vec![(
            AggregateType::Task,
            *rec.task_id.as_bytes(),
            vec![crate::runtime::typed(
                "BrowserHostAttached",
                &TaskEvent::BrowserHostAttached {
                    browser_session_id: bsid.to_string(),
                    host_kind,
                    partition: p.partition,
                    sandboxed: p.sandboxed,
                    context_isolated: p.context_isolated,
                    node_integration: p.node_integration,
                },
                Actor::User(core.user_id),
            )],
        )],
    ) {
        Ok(o) => o,
        Err(e) => return reject(cid, "STORE", e),
    };
    accept(
        cid,
        false,
        wire::BrowserHostAttached {
            browser_session_id: p.browser_session_id,
            lease_generation: lease.generation,
            offset,
        }
        .encode_to_vec(),
    )
}

fn reject(command_id: Option<wire::Id>, code: &str, message: impl Into<String>) -> CommandAck {
    CommandAck {
        command_id,
        status: CommandStatus::Rejected as i32,
        error_code: code.into(),
        error_message: message.into(),
        result: vec![],
    }
}

fn accept(command_id: Option<wire::Id>, replayed: bool, result: Vec<u8>) -> CommandAck {
    CommandAck {
        command_id,
        status: if replayed {
            CommandStatus::Replayed
        } else {
            CommandStatus::Accepted
        } as i32,
        error_code: String::new(),
        error_message: String::new(),
        result,
    }
}

async fn handle_command(core: &Arc<Core>, env: CommandEnvelope) -> CommandAck {
    let cid = env.command_id.clone();
    let Some(command_id) = env.command_id.as_ref().and_then(id16) else {
        return reject(cid, "BAD_COMMAND_ID", "command_id must be 16 bytes");
    };
    if env.schema_version != modbit_domain::SCHEMA_VERSION {
        return reject(
            cid,
            "UNSUPPORTED_SCHEMA_VERSION",
            format!("schema_version {} not supported", env.schema_version),
        );
    }
    let request_hash = modbit_event_store::objects::sha256_hex(
        &[env.command_type.as_bytes(), b"\0", &env.payload].concat(),
    );
    let record = |command_type: &str| CommandRecord {
        command_id: EventId::from_bytes(command_id),
        tenant_id: core.tenant_id,
        command_type: command_type.to_owned(),
        request_hash: request_hash.clone(),
    };
    let actor = Actor::User(core.user_id);
    match env.command_type.as_str() {
        "CreateSession" => {
            let Ok(p) = wire::CreateSession::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "CreateSession");
            };
            let space_id = p
                .space_id
                .as_ref()
                .and_then(id16)
                .map(SpaceId::from_bytes)
                .unwrap_or_else(SpaceId::new);
            // The session id is derived from the command id so a replayed command names the same session.
            let session_id = SessionId::from_bytes(command_id);
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id,
                task_id: None,
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Session,
                aggregate_id: *session_id.as_bytes(),
                expected_sequence: Some(0),
                events: vec![typed(
                    "SessionCreated",
                    &SessionEvent::SessionCreated {
                        tenant_id: core.tenant_id,
                        user_id: core.user_id,
                        space_id,
                    },
                    actor,
                )],
            };
            let mut store = core.store.lock().await;
            match store.execute_command(record("CreateSession"), req) {
                Ok(outcome) => {
                    let (events, replayed) = split(outcome);
                    let offset = events.last().map(|e| e.offset).unwrap_or(0);
                    if !replayed {
                        core.last_offset.send_replace(offset);
                    }
                    accept(
                        cid,
                        replayed,
                        SessionCreated {
                            session_id: Some(wire_id(session_id.as_bytes())),
                            offset,
                        }
                        .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "CreateTask" => {
            let Ok(p) = wire::CreateTask::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "CreateTask");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            let origin = match p.origin.as_str() {
                "desktop" => TaskOrigin::Desktop,
                "cli" => TaskOrigin::Cli,
                "ide_adapter" => TaskOrigin::IdeAdapter,
                "forge_issue" => TaskOrigin::ForgeIssue,
                "forge_webhook" => TaskOrigin::ForgeWebhook,
                other => return reject(cid, "BAD_PAYLOAD", format!("unknown origin `{other}`")),
            };
            if p.goal_text.trim().is_empty()
                && !matches!(origin, TaskOrigin::ForgeIssue | TaskOrigin::ForgeWebhook)
            {
                return reject(cid, "BAD_PAYLOAD", "goal_text required");
            }
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            // PX-010: a task from an issue reads the issue first — through the
            // forge adapter, on the Core's token and egress pin — so an
            // unreadable issue is a clear refusal and no task. The text is
            // data: attached as an untrusted context document, never policy.
            let issue = if origin == TaskOrigin::ForgeIssue {
                if p.issue_url.trim().is_empty() {
                    return reject(
                        cid,
                        "BAD_PAYLOAD",
                        "issue_url required with origin forge_issue",
                    );
                }
                let Some(cfg) = core.tools.forge.get() else {
                    return reject(
                        cid,
                        "NO_FORGE",
                        "no forge is configured for this Core (ConfigureForge)",
                    );
                };
                match modbit_tools::forge::read_issue(&cfg, p.issue_url.trim()).await {
                    Ok(v) => Some((
                        modbit_domain::task::ForgeIssueIntake {
                            url: p.issue_url.trim().to_owned(),
                            number: v["number"].as_u64().unwrap_or(0),
                            title: v["title"].as_str().unwrap_or_default().to_owned(),
                            author: v["author"].as_str().unwrap_or_default().to_owned(),
                            state: v["state"].as_str().unwrap_or_default().to_owned(),
                            labels: v["labels"]
                                .as_array()
                                .map(|l| {
                                    l.iter()
                                        .filter_map(|x| x.as_str().map(str::to_owned))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            body: v["body"].as_str().unwrap_or_default().to_owned(),
                        },
                        "forge_issue",
                    )),
                    Err((code, msg)) => {
                        return reject(
                            cid,
                            "FORGE_ISSUE_UNREADABLE",
                            format!("{code}: {msg} (no task was created)"),
                        );
                    }
                }
            } else if origin == TaskOrigin::ForgeWebhook {
                // PX-011: the forge delivered the issue to the Cloud API,
                // which relayed it here with the command; nothing is read.
                match serde_json::from_str::<modbit_domain::task::ForgeIssueIntake>(&p.issue_json) {
                    Ok(v) if !v.url.trim().is_empty() => Some((v, "forge_webhook")),
                    _ => {
                        return reject(
                            cid,
                            "BAD_PAYLOAD",
                            "issue_json (url, number, title, …) required with origin forge_webhook",
                        );
                    }
                }
            } else {
                None
            };
            let goal_text = if p.goal_text.trim().is_empty() {
                let (v, _) = issue.as_ref().expect("an issue when the goal is empty");
                v.goal()
            } else {
                p.goal_text.clone()
            };
            let workspace_id = p
                .workspace_id
                .as_ref()
                .and_then(id16)
                .map(WorkspaceId::from_bytes)
                .unwrap_or_else(WorkspaceId::new);
            let task_id = TaskId::from_bytes(command_id);
            let mut store = core.store.lock().await;
            match store.session(&session_id) {
                Ok(Some(_)) => {}
                Ok(None) => return reject(cid, "UNKNOWN_SESSION", session_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            }
            // The issue as an attached document (REQ-EV-0161) and the record
            // of where the task came from, in the same batch as its creation.
            let mut intake_events = Vec::new();
            if let Some((v, provenance)) = &issue {
                use sha2::Digest;
                let text = v.document_text();
                let document_id = hex::encode(sha2::Sha256::digest(text.as_bytes()));
                let content_ref = match store.objects().put(text.as_bytes()) {
                    Ok(r) => r,
                    Err(e) => return reject(cid, "OBJECT_STORE", e.to_string()),
                };
                intake_events.push(typed(
                    "ContextDocumentAttached",
                    &TaskEvent::ContextDocumentAttached {
                        document_id: document_id.clone(),
                        source: format!("{provenance}:{}", v.url),
                        title: v.title.clone(),
                        content_ref,
                        byte_length: text.len() as u64,
                        trust: "UNTRUSTED_EXTERNAL_CONTENT".into(),
                    },
                    actor.clone(),
                ));
                intake_events.push(typed(
                    "TaskCreatedFromIssue",
                    &TaskEvent::TaskCreatedFromIssue {
                        url: v.url.clone(),
                        number: v.number,
                        title: v.title.clone(),
                        provenance: (*provenance).to_owned(),
                        document_id,
                    },
                    actor.clone(),
                ));
            }
            let mut events = vec![
                typed(
                    "TaskCreated",
                    &TaskEvent::TaskCreated {
                        session_id,
                        goal_text: goal_text.clone(),
                        workspace_id,
                        workspace_root: if p.workspace_root.is_empty() {
                            None
                        } else {
                            Some(p.workspace_root.clone())
                        },
                        base_revision: None,
                        execution_profile: if p.execution_profile.is_empty() {
                            "local_trusted".into()
                        } else {
                            p.execution_profile.clone()
                        },
                        policy_profile_id: None,
                        origin,
                    },
                    actor.clone(),
                ),
                typed("TaskQueued", &TaskEvent::TaskQueued, actor.clone()),
            ];
            events.extend(intake_events);
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id,
                task_id: Some(task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task_id.as_bytes(),
                expected_sequence: Some(0),
                events,
            };
            let profile = if p.execution_profile.is_empty() {
                "local_trusted".to_owned()
            } else {
                p.execution_profile.clone()
            };
            let root = if p.workspace_root.is_empty() {
                None
            } else {
                Some(p.workspace_root.clone())
            };
            match store.execute_command(record("CreateTask"), req) {
                Ok(outcome) => {
                    let (events, replayed) = split(outcome);
                    let mut offset = events.last().map(|e| e.offset).unwrap_or(0);
                    if !replayed {
                        // Capability Kernel (docs/23): the task's default lease. Tools
                        // present it; the tool name alone is never authority.
                        let (resources, operations, effect_ceiling) =
                            modbit_policy::default_lease_for_profile(&profile, root.as_deref());
                        let lease_id = modbit_domain::CapabilityLeaseId::new();
                        let grant = AppendRequest {
                            tenant_id: core.tenant_id,
                            session_id,
                            task_id: Some(task_id),
                            run_id: None,
                            turn_id: None,
                            step_id: None,
                            aggregate_type: AggregateType::CapabilityLease,
                            aggregate_id: *lease_id.as_bytes(),
                            expected_sequence: Some(0),
                            events: vec![typed(
                                "CapabilityLeaseGranted",
                                &CapabilityLeaseEvent::CapabilityLeaseGranted {
                                    tenant_id: core.tenant_id,
                                    task_id,
                                    agent_id: None,
                                    resources,
                                    operations,
                                    effect_ceiling,
                                    execution_profile: profile,
                                    generation: 1,
                                    expires_at: None,
                                },
                                actor.clone(),
                            )],
                        };
                        match store.append(grant) {
                            Ok(stored) => {
                                if let Some(last) = stored.last() {
                                    offset = last.offset;
                                }
                            }
                            Err(e) => return reject(cid, error_code(&e), e.to_string()),
                        }
                        core.last_offset.send_replace(offset);
                    }
                    accept(
                        cid,
                        replayed,
                        TaskCreated {
                            task_id: Some(wire_id(task_id.as_bytes())),
                            offset,
                            goal_text,
                        }
                        .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "ReadMirrorEvents" => {
            let Ok(p) = wire::ReadMirrorEvents::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ReadMirrorEvents");
            };
            let Some(sid) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            let limit = if p.limit == 0 { 500 } else { p.limit.min(2000) } as usize;
            let store = core.store.lock().await;
            let events = match store.read_session(&sid, p.after_offset, limit) {
                Ok(e) => e,
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let last_offset = store.last_offset().unwrap_or(0);
            let mut out = Vec::with_capacity(events.len());
            let mut offsets = Vec::with_capacity(events.len());
            for e in events {
                let payload = match store.payload(&e.envelope) {
                    Ok(v) => v,
                    Err(err) => return reject(cid, error_code(&err), err.to_string()),
                };
                out.push(wire::MirroredEvent {
                    envelope_json: serde_json::to_string(&e.envelope).unwrap_or_default(),
                    payload_json: payload.to_string(),
                });
                offsets.push(e.offset);
            }
            drop(store);
            accept(
                cid,
                false,
                wire::MirrorEvents {
                    events: out,
                    offsets,
                    last_offset,
                }
                .encode_to_vec(),
            )
        }
        "ImportMirroredEvents" => {
            // M8.2: the cloud log, verbatim, into this Core — every envelope
            // must continue its aggregate's chain; one refusal writes nothing.
            let Ok(p) = wire::ImportMirroredEvents::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ImportMirroredEvents");
            };
            let mut events = Vec::with_capacity(p.events.len());
            for e in &p.events {
                let envelope: modbit_domain::event::EventEnvelope =
                    match serde_json::from_str(&e.envelope_json) {
                        Ok(v) => v,
                        Err(err) => {
                            return reject(cid, "BAD_PAYLOAD", format!("envelope_json: {err}"));
                        }
                    };
                if envelope.tenant_id != core.tenant_id && !p.admitted_handoff {
                    // This Core serves one tenant; another tenant's log is not its
                    // business — unless the cloud admitted it as a handoff (M8.7),
                    // in which case the envelopes keep their origin tenant as
                    // provenance and the cloud has scoped them to this tenant.
                    return reject(
                        cid,
                        "TENANT_MISMATCH",
                        format!(
                            "event {} belongs to tenant {}",
                            envelope.event_id, envelope.tenant_id
                        ),
                    );
                }
                let payload: serde_json::Value = match serde_json::from_str(&e.payload_json) {
                    Ok(v) => v,
                    Err(err) => return reject(cid, "BAD_PAYLOAD", format!("payload_json: {err}")),
                };
                events.push((envelope, payload));
            }
            let outcome = core.store.lock().await.import_envelopes(events);
            match outcome {
                Ok((imported, already_present, last_offset)) => {
                    if imported > 0 {
                        core.last_offset.send_replace(last_offset);
                    }
                    accept(
                        cid,
                        imported == 0,
                        wire::MirroredEventsImported {
                            imported,
                            already_present,
                            last_offset,
                        }
                        .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "GetSessionSnapshot" => {
            let Ok(p) = wire::GetSessionSnapshot::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetSessionSnapshot");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            let store = core.store.lock().await;
            let session = match store.session(&session_id) {
                Ok(Some(s)) => s,
                Ok(None) => return reject(cid, "UNKNOWN_SESSION", session_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let events = match store.read_session(&session_id, 0, usize::MAX) {
                Ok(e) => e,
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let last_offset = events.last().map(|e| e.offset).unwrap_or(0);
            // M6.6: a subagent's task names its parent, from the parent's
            // admission record, so a client rebuilt from the snapshot nests
            // it where it belongs.
            let mut parents: std::collections::HashMap<[u8; 16], TaskId> =
                std::collections::HashMap::new();
            for ev in &events {
                if ev.envelope.event_type == "SubagentAdmitted"
                    && let Some(parent) = ev.envelope.task_id
                    && let Ok(p) = store.payload(&ev.envelope)
                    && let Some(child) = p["child_task_id"]
                        .as_str()
                        .and_then(|c| TaskId::parse(c).ok())
                {
                    parents.insert(*child.as_bytes(), parent);
                }
            }
            let mut tasks = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for ev in &events {
                if ev.envelope.aggregate_type == AggregateType::Task
                    && seen.insert(ev.envelope.aggregate_id)
                    && let Ok(Some(t)) = store.task(&TaskId::from_bytes(ev.envelope.aggregate_id))
                {
                    {
                        tasks.push(TaskView {
                            task_id: Some(wire_id(t.task_id.as_bytes())),
                            goal_text: t.goal_text.clone(),
                            state: format!("{:?}", t.state),
                            generation: t.generation,
                            created_at: Some(prost_types::Timestamp {
                                seconds: t.created_at.millis().div_euclid(1000),
                                nanos: (t.created_at.millis().rem_euclid(1000) * 1_000_000) as i32,
                            }),
                            origin: serde_json::to_string(&t.origin)
                                .unwrap_or_default()
                                .trim_matches('"')
                                .to_owned(),
                            parent_task_id: parents
                                .get(t.task_id.as_bytes())
                                .map(|p| wire_id(p.as_bytes())),
                            workspace_root: t.workspace_root.clone().unwrap_or_default(),
                        });
                    }
                }
            }
            accept(
                cid,
                false,
                SessionSnapshot {
                    session_id: Some(wire_id(session_id.as_bytes())),
                    state: format!("{:?}", session.state),
                    generation: session.generation,
                    tasks,
                    last_offset,
                    lease_generation: session.lease_generation,
                    lease_owner: session.lease_owner.clone().unwrap_or_default(),
                }
                .encode_to_vec(),
            )
        }
        "AcquireSessionLease" => {
            let Ok(p) = wire::AcquireSessionLease::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "AcquireSessionLease");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            let mut store = core.store.lock().await;
            let session = match store.session(&session_id) {
                Ok(Some(s)) => s,
                Ok(None) => return reject(cid, "UNKNOWN_SESSION", session_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let next_generation = session.lease_generation + 1;
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id,
                task_id: None,
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Session,
                aggregate_id: *session_id.as_bytes(),
                expected_sequence: None,
                events: vec![typed(
                    "SessionLeaseAcquired",
                    &SessionEvent::SessionLeaseAcquired {
                        lease_generation: next_generation,
                        owner: p.owner.clone(),
                    },
                    actor,
                )],
            };
            match store.execute_command(record("AcquireSessionLease"), req) {
                Ok(outcome) => {
                    let (events, replayed) = split(outcome);
                    let offset = events.last().map(|e| e.offset).unwrap_or(0);
                    if !replayed {
                        core.last_offset.send_replace(offset);
                    }
                    // A replay returns the generation this command minted, not the current one.
                    let generation = if replayed {
                        store
                            .session(&session_id)
                            .ok()
                            .flatten()
                            .map(|s| s.lease_generation)
                            .unwrap_or(next_generation)
                    } else {
                        next_generation
                    };
                    accept(
                        cid,
                        replayed,
                        SessionLeaseAcquired {
                            session_id: Some(wire_id(session_id.as_bytes())),
                            lease_generation: generation,
                            offset,
                        }
                        .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "QueueInput" => {
            let Ok(p) = wire::QueueInput::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "QueueInput");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let mode = match p.mode.as_str() {
                "STEER" => InputMode::Steer,
                "COLLECT" => InputMode::Collect,
                "FOLLOW_UP" => InputMode::FollowUp,
                other => {
                    return reject(cid, "BAD_PAYLOAD", format!("unknown input mode `{other}`"));
                }
            };
            if p.input_id.is_empty() || p.text.trim().is_empty() {
                return reject(cid, "BAD_PAYLOAD", "input_id and text required");
            }
            let session_id = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t.session_id,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id,
                task_id: Some(task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task_id.as_bytes(),
                expected_sequence: None,
                events: vec![typed(
                    "TaskInputQueued",
                    &TaskEvent::TaskInputQueued {
                        input_id: p.input_id.clone(),
                        mode,
                        text: p.text.clone(),
                    },
                    actor,
                )],
            };
            let mut store = core.store.lock().await;
            match store.execute_command(record("QueueInput"), req) {
                Ok(outcome) => {
                    let (events, replayed) = split(outcome);
                    let last = events.last();
                    let (offset, sequence) = last
                        .map(|e| (e.offset, e.envelope.sequence))
                        .unwrap_or((0, 0));
                    if !replayed {
                        core.last_offset.send_replace(offset);
                    }
                    accept(
                        cid,
                        replayed,
                        InputQueued {
                            task_id: Some(wire_id(task_id.as_bytes())),
                            sequence,
                            offset,
                        }
                        .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "PublishOutcomeBaseline" => {
            let Ok(p) = wire::PublishOutcomeBaseline::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "PublishOutcomeBaseline");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            {
                let store = core.store.lock().await;
                match store.session(&session_id) {
                    Ok(Some(_)) => {}
                    Ok(None) => return reject(cid, "UNKNOWN_SESSION", session_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            }
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let bundle =
                crate::baseline::assemble(core, session_id, p.repository_revision.trim()).await;
            let bundle_ref = {
                let store = core.store.lock().await;
                match store
                    .objects()
                    .put(serde_json::to_vec(&bundle).unwrap_or_default().as_slice())
                {
                    Ok(r) => r,
                    Err(e) => return reject(cid, "OBJECT_STORE", e.to_string()),
                }
            };
            // The baseline is itself an event: what was published, when, and
            // over which revision (docs/27 §22 — a baseline is a record, not a
            // report someone kept).
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id,
                task_id: None,
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Session,
                aggregate_id: *session_id.as_bytes(),
                expected_sequence: None,
                events: vec![typed(
                    "OutcomeBaselinePublished",
                    &modbit_domain::session::SessionEvent::OutcomeBaselinePublished {
                        bundle_digest: bundle.bundle_digest.clone(),
                        bundle_ref: bundle_ref.clone(),
                        repository_revision: bundle.repository_revision.clone(),
                        build_digest: bundle.build_digest.clone(),
                        environment_digest: bundle.environment_digest.clone(),
                        tasks: u32::try_from(bundle.tasks.len()).unwrap_or(u32::MAX),
                        verified_tasks: u32::try_from(bundle.verified_tasks).unwrap_or(u32::MAX),
                        tasks_with_unknown_usage: u32::try_from(bundle.tasks_with_unknown_usage)
                            .unwrap_or(u32::MAX),
                    },
                    actor,
                )],
            };
            let mut store = core.store.lock().await;
            match store.execute_command(record("PublishOutcomeBaseline"), req) {
                Ok(outcome) => {
                    let (events, replayed) = split(outcome);
                    let offset = events.last().map_or(0, |e| e.offset);
                    if !replayed {
                        core.last_offset.send_replace(offset);
                    }
                    accept(
                        cid,
                        replayed,
                        crate::baseline::published(&bundle, bundle_ref, offset).encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "AllowUnsupportedLanguage" => {
            let Ok(p) = wire::AllowUnsupportedLanguage::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "AllowUnsupportedLanguage");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let languages: Vec<String> = p
                .languages
                .iter()
                .map(|l| l.trim().to_ascii_lowercase())
                .filter(|l| !l.is_empty() && l.len() <= 64)
                .collect();
            if languages.is_empty() {
                return reject(cid, "BAD_PAYLOAD", "at least one language is required");
            }
            let session_id = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t.session_id,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id,
                task_id: Some(task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task_id.as_bytes(),
                expected_sequence: None,
                events: vec![typed(
                    "UnsupportedLanguageOptInRecorded",
                    &TaskEvent::UnsupportedLanguageOptInRecorded {
                        languages: languages.clone(),
                        reason: p.reason.clone(),
                    },
                    actor,
                )],
            };
            let mut store = core.store.lock().await;
            match store.execute_command(record("AllowUnsupportedLanguage"), req) {
                Ok(outcome) => {
                    let (events, replayed) = split(outcome);
                    let offset = events.last().map_or(0, |e| e.offset);
                    if !replayed {
                        core.last_offset.send_replace(offset);
                    }
                    accept(
                        cid,
                        replayed,
                        wire::UnsupportedLanguageAllowed { offset, languages }.encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "AttachContextDocument" => {
            let Ok(p) = wire::AttachContextDocument::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "AttachContextDocument");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if p.source.trim().is_empty() || p.text.trim().is_empty() {
                return reject(cid, "BAD_PAYLOAD", "source and text required");
            }
            if p.text.len() > MAX_CONTEXT_DOCUMENT_BYTES {
                return reject(
                    cid,
                    "DOCUMENT_TOO_LARGE",
                    format!("at most {MAX_CONTEXT_DOCUMENT_BYTES} bytes"),
                );
            }
            let session_id = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t.session_id,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let (document_id, content_ref) = {
                use sha2::Digest;
                let store = core.store.lock().await;
                let hash = hex::encode(sha2::Sha256::digest(p.text.as_bytes()));
                let stored = match store.objects().put(p.text.as_bytes()) {
                    Ok(r) => r,
                    Err(e) => return reject(cid, "OBJECT_STORE", e.to_string()),
                };
                (hash, stored)
            };
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id,
                task_id: Some(task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task_id.as_bytes(),
                expected_sequence: None,
                events: vec![typed(
                    "ContextDocumentAttached",
                    &TaskEvent::ContextDocumentAttached {
                        document_id: document_id.clone(),
                        source: p.source.clone(),
                        title: p.title.clone(),
                        content_ref: content_ref.clone(),
                        byte_length: p.text.len() as u64,
                        trust: "UNTRUSTED_EXTERNAL_CONTENT".into(),
                    },
                    actor,
                )],
            };
            let mut store = core.store.lock().await;
            match store.execute_command(record("AttachContextDocument"), req) {
                Ok(outcome) => {
                    let (events, replayed) = split(outcome);
                    let offset = events.last().map_or(0, |e| e.offset);
                    if !replayed {
                        core.last_offset.send_replace(offset);
                    }
                    accept(
                        cid,
                        replayed,
                        wire::ContextDocumentAttached {
                            document_id,
                            content_ref,
                            offset,
                            trust: "UNTRUSTED_EXTERNAL_CONTENT".into(),
                            replayed,
                        }
                        .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "SetTaskSelection" => {
            let Ok(p) = wire::SetTaskSelection::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "SetTaskSelection");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if p.paths.is_empty() && p.symbol.is_empty() && p.review_hunks.is_empty() {
                return reject(
                    cid,
                    "BAD_PAYLOAD",
                    "a selection needs a path, a symbol or a review hunk",
                );
            }
            if p.paths.iter().any(|x| x.trim().is_empty()) {
                return reject(cid, "BAD_PAYLOAD", "empty path in selection");
            }
            if p.line_end != 0 && p.line_start > p.line_end {
                return reject(cid, "BAD_PAYLOAD", "line_start must not exceed line_end");
            }
            let source = match p.source.as_str() {
                "" => "cli".to_owned(),
                "review" | "editor" | "cli" | "desktop" => p.source.clone(),
                other => {
                    return reject(
                        cid,
                        "BAD_PAYLOAD",
                        format!("unknown selection source `{other}`"),
                    );
                }
            };
            let session_id = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t.session_id,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id,
                task_id: Some(task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task_id.as_bytes(),
                expected_sequence: None,
                events: vec![typed(
                    "SelectionRecorded",
                    &TaskEvent::SelectionRecorded {
                        paths: p.paths.clone(),
                        symbol: (!p.symbol.is_empty()).then(|| p.symbol.clone()),
                        lines: (p.line_end != 0).then_some((p.line_start.max(1), p.line_end)),
                        review_hunks: p.review_hunks.clone(),
                        source,
                    },
                    actor,
                )],
            };
            let mut store = core.store.lock().await;
            match store.execute_command(record("SetTaskSelection"), req) {
                Ok(outcome) => {
                    let (events, replayed) = split(outcome);
                    let offset = events.last().map_or(0, |e| e.offset);
                    if !replayed {
                        core.last_offset.send_replace(offset);
                    }
                    accept(
                        cid,
                        replayed,
                        wire::TaskSelectionRecorded { offset }.encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "ReadObjectRange" => {
            let Ok(p) = wire::ReadObjectRange::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ReadObjectRange");
            };
            if p.object_hash.len() != 64 || !p.object_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return reject(cid, "BAD_PAYLOAD", "object_hash must be 64 hex chars");
            }
            if p.length == 0 || p.length > MAX_OBJECT_CHUNK {
                return reject(
                    cid,
                    "RANGE_TOO_LARGE",
                    format!("length must be 1..={MAX_OBJECT_CHUNK}"),
                );
            }
            let store = core.store.lock().await;
            match store
                .objects()
                .read_range(&p.object_hash, p.offset, p.length)
            {
                Ok((data, total, checksum)) => accept(
                    cid,
                    false,
                    ObjectRangeChunk {
                        object_hash: p.object_hash,
                        total_bytes: total,
                        offset: p.offset,
                        data,
                        checksum_sha256: checksum,
                    }
                    .encode_to_vec(),
                ),
                Err(e) => reject(cid, "UNKNOWN_OBJECT", e.to_string()),
            }
        }
        "ListTools" => {
            let task_id = wire::ListTools::decode(env.payload.as_slice())
                .ok()
                .and_then(|p| p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes));
            let (profile, lease) = match task_id {
                Some(id) => {
                    let store = core.store.lock().await;
                    let task = match store.task(&id) {
                        Ok(Some(t)) => t,
                        Ok(None) => return reject(cid, "UNKNOWN_TASK", id.to_string()),
                        Err(e) => return reject(cid, error_code(&e), e.to_string()),
                    };
                    let lease = store
                        .leases_for_task(&id)
                        .ok()
                        .and_then(|l| l.into_iter().next());
                    (Some(task.execution_profile.clone()), lease)
                }
                None => (None, None),
            };
            accept(
                cid,
                false,
                wire::ToolList {
                    tools: core
                        .tools
                        .visible_specs(profile.as_deref(), lease.as_ref())
                        .into_iter()
                        .map(|t| wire::ToolSpecView {
                            name: t.name,
                            version: t.version,
                            effect_class: format!("{:?}", t.effect_class),
                            input_schema_json: t.input_schema.to_string(),
                            description: t.description,
                        })
                        .collect(),
                }
                .encode_to_vec(),
            )
        }
        "InvokeTool" => {
            let Ok(p) = wire::InvokeTool::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "InvokeTool");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let Some(tool_call_id) = p
                .tool_call_id
                .as_ref()
                .and_then(id16)
                .map(ToolCallId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "tool_call_id required (16 bytes)");
            };
            let (task, session) = {
                let store = core.store.lock().await;
                let task = match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                let session = match store.session(&task.session_id) {
                    Ok(Some(s)) => s,
                    Ok(None) => return reject(cid, "UNKNOWN_SESSION", task.session_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                (task, session)
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            // Idempotent by tool_call_id: a finished call replays its recorded
            // result; a call waiting on an approval re-enters the pipeline with
            // the same intent; a different intent under the same id is refused.
            let (existing, lease, approval) = {
                let store = core.store.lock().await;
                let existing = match store.tool_call(&tool_call_id) {
                    Ok(e) => e,
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                if let Some(c) = &existing {
                    if let Some(h) = modbit_tools::arguments_hash(&p.arguments_json)
                        && h != c.arguments_hash
                    {
                        return reject(
                            cid,
                            "TOOL_CALL_ID_REUSED",
                            "tool_call_id already bound to different arguments (intent hash mismatch)",
                        );
                    }
                    if let Some(r) = &c.result_ref
                        && let Ok(bytes) = store.objects().get(r)
                        && let Ok(prior) =
                            serde_json::from_slice::<modbit_tools::ToolCallResult>(&bytes)
                    {
                        return accept(
                            cid,
                            true,
                            tool_invoked(&prior, r, c.approval_id).encode_to_vec(),
                        );
                    }
                    if c.state.is_terminal() {
                        let mut view = wire::ToolInvoked {
                            tool_call_id: Some(wire_id(c.tool_call_id.as_bytes())),
                            status: "POLICY_DENIED".into(),
                            error_code: "APPROVAL_DENIED".into(),
                            error_message: c.policy_decision.clone().unwrap_or_default(),
                            ..Default::default()
                        };
                        view.approval_id = c
                            .approval_id
                            .map(|a| encode_hex(a.as_bytes()))
                            .unwrap_or_default();
                        return accept(cid, true, view.encode_to_vec());
                    }
                    if c.state != modbit_domain::toolcall::ToolCallState::ApprovalPending {
                        return reject(cid, "TOOL_CALL_IN_FLIGHT", format!("{:?}", c.state));
                    }
                }
                let lease = match store.leases_for_task(&task_id) {
                    Ok(l) => l.into_iter().next(),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                let approval = match store.approval_for_call(&tool_call_id) {
                    Ok(a) => a,
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                (existing, lease, approval)
            };
            let req = crate::tools::InvokeRequest {
                tenant_id: core.tenant_id,
                session_id: task.session_id,
                task_id,
                workspace_root: task.workspace_root.clone(),
                execution_profile: &task.execution_profile,
                tool_call_id,
                tool_name: &p.tool_name,
                arguments_json: &p.arguments_json,
                output_budget_bytes: p.output_budget_bytes,
                actor,
                lease,
                approval,
                emergency_stopped: session.emergency_stopped_at.is_some(),
                existing,
                run_id: None,
                turn_id: None,
                call_id: None,
                lease_generation: env.expected_generation,
                projection: None,
                cancel: None,
            };
            match core.tools.invoke(&core.store, req).await {
                Ok(done) => {
                    let offset = core.store.lock().await.last_offset().unwrap_or(0);
                    core.last_offset.send_replace(offset);
                    accept(
                        cid,
                        false,
                        tool_invoked(&done.result, &done.result_ref, done.approval_id)
                            .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, "TOOL_HOST", e.to_string()),
            }
        }
        "ListApprovals" => {
            let Ok(p) = wire::ListApprovals::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ListApprovals");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            let store = core.store.lock().await;
            match store.approvals_for_session(&session_id) {
                Ok(list) => accept(
                    cid,
                    false,
                    wire::ApprovalList {
                        approvals: list.iter().map(approval_view).collect(),
                    }
                    .encode_to_vec(),
                ),
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "ResolveApproval" => {
            let Ok(p) = wire::ResolveApproval::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ResolveApproval");
            };
            let Some(approval_id) = p
                .approval_id
                .as_ref()
                .and_then(id16)
                .map(modbit_domain::ApprovalId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "approval_id required");
            };
            let (approval, task) = {
                let store = core.store.lock().await;
                let approval = match store.approval(&approval_id) {
                    Ok(Some(a)) => a,
                    Ok(None) => return reject(cid, "UNKNOWN_APPROVAL", approval_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                let task = match store.task(&approval.task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", approval.task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                (approval, task)
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            // PX-001 (docs/29): the decision binds to the intent the person
            // saw; a client that presents another one decides nothing.
            if !p.intent_hash.is_empty() && p.intent_hash != approval.intent_hash {
                return reject(
                    cid,
                    "INTENT_MISMATCH",
                    format!(
                        "the approval binds intent {} and the decision names {}; reload the approval before deciding",
                        approval.intent_hash, p.intent_hash
                    ),
                );
            }
            if approval.state != modbit_domain::approval::ApprovalState::Requested {
                return accept(
                    cid,
                    true,
                    wire::ApprovalResolvedAck {
                        approval_id: Some(wire_id(approval_id.as_bytes())),
                        status: format!("{:?}", approval.state).to_uppercase(),
                        offset: 0,
                    }
                    .encode_to_vec(),
                );
            }
            let mut store = core.store.lock().await;
            let resolved = AppendRequest {
                tenant_id: core.tenant_id,
                session_id: task.session_id,
                task_id: Some(task.task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Approval,
                aggregate_id: *approval_id.as_bytes(),
                expected_sequence: Some(approval.generation),
                events: vec![typed(
                    "ApprovalResolved",
                    &ApprovalEvent::ApprovalResolved {
                        approved: p.approve,
                        resolver: format!("user:{}", core.user_id),
                        reason: p.reason.clone(),
                    },
                    actor.clone(),
                )],
            };
            let mut offset = match store.append(resolved) {
                Ok(ev) => ev.last().map(|e| e.offset).unwrap_or(0),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if !p.approve
                && let Ok(Some(call)) = store.tool_call(&approval.tool_call_id)
                && call.state == modbit_domain::toolcall::ToolCallState::ApprovalPending
            {
                let failed = AppendRequest {
                    tenant_id: core.tenant_id,
                    session_id: task.session_id,
                    task_id: Some(task.task_id),
                    run_id: None,
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::ToolCall,
                    aggregate_id: *call.tool_call_id.as_bytes(),
                    expected_sequence: Some(call.generation),
                    events: vec![typed(
                        "ToolCallFailed",
                        &ToolCallEvent::ToolCallFailed {
                            failure_code: "APPROVAL_DENIED".into(),
                            result_ref: None,
                        },
                        actor.clone(),
                    )],
                };
                match store.append(failed) {
                    Ok(ev) => offset = ev.last().map(|e| e.offset).unwrap_or(offset),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            }
            core.last_offset.send_replace(offset);
            accept(
                cid,
                false,
                wire::ApprovalResolvedAck {
                    approval_id: Some(wire_id(approval_id.as_bytes())),
                    status: if p.approve { "APPROVED" } else { "DENIED" }.into(),
                    offset,
                }
                .encode_to_vec(),
            )
        }
        "EmergencyStop" => {
            let Ok(p) = wire::EmergencyStop::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "EmergencyStop");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let mut store = core.store.lock().await;
            let session = match store.session(&session_id) {
                Ok(Some(s)) => s,
                Ok(None) => return reject(cid, "UNKNOWN_SESSION", session_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let reason = if p.reason.is_empty() {
                "operator emergency stop".to_owned()
            } else {
                p.reason.clone()
            };
            let mut offset = 0;
            if session.emergency_stopped_at.is_none() {
                let stop = AppendRequest {
                    tenant_id: core.tenant_id,
                    session_id,
                    task_id: None,
                    run_id: None,
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::Session,
                    aggregate_id: *session_id.as_bytes(),
                    expected_sequence: Some(session.generation),
                    events: vec![typed(
                        "EmergencyStopActivated",
                        &SessionEvent::EmergencyStopActivated {
                            reason: reason.clone(),
                        },
                        actor.clone(),
                    )],
                };
                match store.append(stop) {
                    Ok(ev) => offset = ev.last().map(|e| e.offset).unwrap_or(0),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            }
            let leases = match store.active_leases_for_session(&session_id) {
                Ok(l) => l,
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let mut revoked = 0u32;
            for l in leases {
                let req = AppendRequest {
                    tenant_id: core.tenant_id,
                    session_id,
                    task_id: Some(l.task_id),
                    run_id: None,
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::CapabilityLease,
                    aggregate_id: *l.lease_id.as_bytes(),
                    expected_sequence: None,
                    events: vec![typed(
                        "CapabilityLeaseRevoked",
                        &CapabilityLeaseEvent::CapabilityLeaseRevoked {
                            reason: format!("EMERGENCY_STOP: {reason}"),
                        },
                        actor.clone(),
                    )],
                };
                match store.append(req) {
                    Ok(ev) => {
                        revoked += 1;
                        offset = ev.last().map(|e| e.offset).unwrap_or(offset);
                    }
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            }
            if offset > 0 {
                core.last_offset.send_replace(offset);
            }
            // docs/23: safe tool calls in flight are cancelled, not waited
            // for — every live loop of the session is cancelled, which ends
            // the call it is inside through the pipeline's own accounting (a
            // process is killed with its group and recorded Cancelled; an
            // effect already sent is recorded UnknownOutcome for
            // reconciliation) and ends the run without another model call.
            // The loop's own lease fencing does not do this: the stop revokes
            // capability leases, not the session's lease generation.
            let live: Vec<TaskId> = store
                .live_tasks()
                .unwrap_or_default()
                .into_iter()
                .filter(|t| t.session_id == session_id)
                .map(|t| t.task_id)
                .collect();
            drop(store);
            let mut cancelled = 0u32;
            for task_id in live {
                if core.runtime.cancel(&task_id).await {
                    cancelled += 1;
                }
            }
            eprintln!(
                "modbit-core: emergency stop on session {session_id}: {revoked} lease(s) revoked, {cancelled} live run(s) cancelled ({reason})"
            );
            accept(
                cid,
                false,
                wire::EmergencyStopped {
                    leases_revoked: revoked,
                    offset,
                }
                .encode_to_vec(),
            )
        }
        "IngestAttachment" => {
            let Ok(p) = wire::IngestAttachment::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "IngestAttachment");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if p.data.is_empty() {
                return reject(cid, "BAD_PAYLOAD", "data required");
            }
            let task = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let channel = match p.channel.as_str() {
                "desktop" | "cli" | "api" => p.channel.clone(),
                _ => "api".to_owned(),
            };
            let filename: String = p
                .filename
                .chars()
                .filter(|c| !c.is_control())
                .take(255)
                .collect();
            let attachment_id = {
                use sha2::Digest;
                hex::encode(sha2::Sha256::digest(&p.data))
            };
            let mut store = core.store.lock().await;
            // Same bytes for the same task: replay the recorded ingestion.
            let prior = store
                .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
                .unwrap_or_default()
                .into_iter()
                .find_map(|e| {
                    (e.envelope.event_type == "AttachmentIngested")
                        .then(|| store.payload(&e.envelope).ok())
                        .flatten()
                        .filter(|v| v["attachment_id"] == attachment_id.as_str())
                        .map(|v| (v, e.offset))
                });
            if let Some((v, offset)) = prior {
                let env_v = v["envelope"].clone();
                return accept(
                    cid,
                    true,
                    wire::AttachmentIngested {
                        attachment_id,
                        envelope_json: env_v.to_string(),
                        kind: env_v["kind"].as_str().unwrap_or_default().to_owned(),
                        mime: env_v["mime"].as_str().unwrap_or_default().to_owned(),
                        content_ref: env_v["content_ref"].as_str().unwrap_or_default().to_owned(),
                        offset,
                        replayed: true,
                    }
                    .encode_to_vec(),
                );
            }
            let sink = crate::tools::StoreSink(store.objects().clone());
            let req = modbit_tools::media::ReadRequest {
                bytes: &p.data,
                source: &format!("attachment:{channel}:{filename}"),
                workspace_revision: None,
                task_id: Some(task_id),
                pages: None,
                region: None,
                budget: modbit_tools::media::default_budget(),
            };
            let read = match modbit_tools::media::read(&req, &sink) {
                Ok(m) => m,
                Err(e) => return reject(cid, e.code, e.message),
            };
            let envelope = read.envelope;
            let actor = Actor::User(core.user_id);
            let offset = match crate::runtime::append(
                &mut store,
                core,
                crate::runtime::Lineage::task(core.tenant_id, task.session_id, task_id),
                AggregateType::Task,
                *task_id.as_bytes(),
                vec![crate::runtime::typed(
                    "AttachmentIngested",
                    &modbit_domain::task::TaskEvent::AttachmentIngested {
                        attachment_id: attachment_id.clone(),
                        filename,
                        channel,
                        envelope: Box::new(envelope.clone()),
                    },
                    actor,
                )],
            ) {
                Ok(o) => o,
                Err(e) => return reject(cid, "STORE", e.to_string()),
            };
            accept(
                cid,
                false,
                wire::AttachmentIngested {
                    attachment_id,
                    envelope_json: serde_json::to_string(&envelope).unwrap_or_default(),
                    kind: format!("{:?}", envelope.kind).to_uppercase(),
                    mime: envelope.mime.clone(),
                    content_ref: envelope.content_ref.clone(),
                    offset,
                    replayed: false,
                }
                .encode_to_vec(),
            )
        }
        "ListQuestions" => {
            let Ok(p) = wire::ListQuestions::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ListQuestions");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let store = core.store.lock().await;
            accept(
                cid,
                false,
                wire::QuestionList {
                    questions: questions_of(&store, &task_id),
                }
                .encode_to_vec(),
            )
        }
        "RespondToQuestion" => {
            let Ok(p) = wire::RespondToQuestion::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "RespondToQuestion");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let task = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let mut store = core.store.lock().await;
            let questions = questions_of(&store, &task_id);
            let Some(q) = questions.iter().find(|q| q.question_id == p.question_id) else {
                return reject(cid, "UNKNOWN_QUESTION", p.question_id);
            };
            if q.answered {
                return accept(
                    cid,
                    true,
                    wire::QuestionResponded {
                        question_id: q.question_id.clone(),
                        already_answered: true,
                    }
                    .encode_to_vec(),
                );
            }
            let option_id = (!p.option_id.is_empty()).then(|| p.option_id.clone());
            let text = (!p.text.trim().is_empty()).then(|| p.text.trim().to_owned());
            if let Some(o) = &option_id
                && !q.options.iter().any(|x| &x.id == o)
            {
                return reject(
                    cid,
                    "BAD_ANSWER",
                    format!(
                        "option `{o}` is not one of {:?}",
                        q.options.iter().map(|x| &x.id).collect::<Vec<_>>()
                    ),
                );
            }
            if option_id.is_none() && (text.is_none() || !q.allow_free_text) {
                return reject(
                    cid,
                    "BAD_ANSWER",
                    "answer with one of the options (free text is not accepted here)",
                );
            }
            let actor = Actor::User(core.user_id);
            if let Err(e) = crate::runtime::append(
                &mut store,
                core,
                crate::runtime::Lineage::task(core.tenant_id, task.session_id, task_id),
                AggregateType::Task,
                *task_id.as_bytes(),
                vec![crate::runtime::typed(
                    "UserQuestionAnswered",
                    &modbit_domain::task::TaskEvent::UserQuestionAnswered {
                        question_id: p.question_id.clone(),
                        option_id,
                        text,
                    },
                    actor,
                )],
            ) {
                return reject(cid, "STORE", e.to_string());
            }
            accept(
                cid,
                false,
                wire::QuestionResponded {
                    question_id: p.question_id,
                    already_answered: false,
                }
                .encode_to_vec(),
            )
        }
        "AskSideQuestion" => {
            let Ok(p) = wire::AskSideQuestion::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "AskSideQuestion");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if p.text.trim().is_empty() {
                return reject(cid, "BAD_PAYLOAD", "text required");
            }
            let task = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            match crate::side::ask(core, &task, &p.text, &p.endpoint, &p.model).await {
                Ok(a) => accept(cid, false, a.encode_to_vec()),
                Err((code, message)) => reject(cid, &code, message),
            }
        }
        "ReconcileToolCall" => {
            let Ok(p) = wire::ReconcileToolCall::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ReconcileToolCall");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let Some(call) = p
                .tool_call_id
                .as_ref()
                .and_then(id16)
                .map(ToolCallId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "tool_call_id required");
            };
            let resolution = match p.resolution.as_str() {
                "EFFECT_CONFIRMED" => "USER_CONFIRMED",
                "EFFECT_ABSENT" => "USER_ABSENT",
                other => {
                    return reject(
                        cid,
                        "BAD_PAYLOAD",
                        format!(
                            "resolution must be EFFECT_CONFIRMED or EFFECT_ABSENT, not `{other}`"
                        ),
                    );
                }
            };
            let (task, tc) = {
                let store = core.store.lock().await;
                let task = match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                let tc = match store.tool_call(&call) {
                    Ok(Some(c)) if c.task_id == task_id => c,
                    Ok(_) => return reject(cid, "UNKNOWN_TOOL_CALL", call.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                (task, tc)
            };
            if tc.state != modbit_domain::toolcall::ToolCallState::UnknownOutcome {
                return reject(
                    cid,
                    "NOT_UNKNOWN",
                    format!(
                        "the call is {:?}; only an unknown outcome is reconciled",
                        tc.state
                    ),
                );
            }
            if core.runtime.is_running(&task_id).await {
                return reject(
                    cid,
                    "TASK_RUNNING",
                    "the agent loop is executing; it reconciles in flight, or cancel it first",
                );
            }
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let state = crate::protocol::reconstruct(&*core.store.lock().await, &task_id);
            if state.call(&call).is_none() {
                return reject(cid, "ALREADY_RECONCILED", call.to_string());
            }
            let lt = crate::runtime::Lineage::task(core.tenant_id, task.session_id, task_id);
            let mut store = core.store.lock().await;
            match crate::runtime::append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *task_id.as_bytes(),
                vec![crate::runtime::typed(
                    "ToolCallReconciled",
                    &modbit_domain::task::TaskEvent::ToolCallReconciled {
                        tool_call_id: call.to_string(),
                        tool_name: tc.tool_name.clone(),
                        effect_class: format!("{:?}", tc.effect_class),
                        resolution: resolution.to_owned(),
                        observed: if p.note.is_empty() {
                            "reconciled by the user".to_owned()
                        } else {
                            p.note.clone()
                        },
                    },
                    actor,
                )],
            ) {
                Ok(offset) => accept(
                    cid,
                    false,
                    wire::ToolCallReconciledAck {
                        tool_call_id: Some(wire_id(call.as_bytes())),
                        resolution: resolution.to_owned(),
                        offset,
                    }
                    .encode_to_vec(),
                ),
                Err(e) => reject(cid, "STORE", e),
            }
        }
        "CreateCheckpoint" => {
            let Ok(p) = wire::CreateCheckpoint::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "CreateCheckpoint");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let kind = match crate::checkpoint::kind_of(&p.kind) {
                Ok(k) => k,
                Err(e) => return reject(cid, "BAD_PAYLOAD", e),
            };
            let task = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if task.workspace_root.is_none() {
                return reject(cid, "NO_WORKSPACE", "the task has no workspace root");
            }
            // A checkpoint claims an epoch on the task: fenced like any write.
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let reason = if p.reason.is_empty() {
                "requested".to_owned()
            } else {
                p.reason.clone()
            };
            let lt = crate::runtime::Lineage::task(core.tenant_id, task.session_id, task_id);
            match crate::checkpoint::capture(core, &task, lt, &actor, kind, &reason).await {
                Ok(c) => {
                    let offset = core.store.lock().await.last_offset().unwrap_or(0);
                    core.last_offset.send_replace(offset);
                    let row = core
                        .store
                        .lock()
                        .await
                        .checkpoints(&task_id)
                        .unwrap_or_default()
                        .into_iter()
                        .find(|r| r.checkpoint_id == c.manifest.checkpoint_id.to_string());
                    let (committed, refusal) = match &c.committed {
                        Ok(()) => (true, String::new()),
                        Err(modbit_checkpoint::StaleCheckpoint::NotNewer { .. }) => {
                            (false, "NOT_NEWER".to_owned())
                        }
                        Err(modbit_checkpoint::StaleCheckpoint::IntegrityMismatch { .. }) => {
                            (false, "INTEGRITY_MISMATCH".to_owned())
                        }
                    };
                    accept(
                        cid,
                        false,
                        wire::CheckpointCreated {
                            checkpoint: row.as_ref().map(crate::checkpoint::view),
                            committed,
                            refusal,
                        }
                        .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, "CHECKPOINT", e.to_string()),
            }
        }
        "ListCheckpoints" => {
            let Ok(p) = wire::ListCheckpoints::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ListCheckpoints");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let store = core.store.lock().await;
            let task = match store.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            accept(
                cid,
                false,
                crate::checkpoint::list(&store, &task).encode_to_vec(),
            )
        }
        "RestoreCheckpoint" => {
            let Ok(p) = wire::RestoreCheckpoint::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "RestoreCheckpoint");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let target = if p.checkpoint_id.is_empty() {
                None
            } else {
                match modbit_domain::CheckpointId::parse(&p.checkpoint_id) {
                    Ok(id) => Some(id),
                    Err(_) => return reject(cid, "BAD_PAYLOAD", "checkpoint_id is not an id"),
                }
            };
            let task = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if task.workspace_root.is_none() {
                return reject(cid, "NO_WORKSPACE", "the task has no workspace root");
            }
            if core.runtime.is_running(&task_id).await {
                return reject(
                    cid,
                    "TASK_RUNNING",
                    "the agent loop is executing; cancel or wait before restoring",
                );
            }
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let lt = crate::runtime::Lineage::task(core.tenant_id, task.session_id, task_id);
            let expected: Vec<(String, String)> = p
                .expected
                .iter()
                .map(|f| (f.path.clone(), f.content_hash.clone()))
                .collect();
            match crate::checkpoint::restore(core, &task, lt, &actor, target, &expected).await {
                Ok(Ok(r)) => {
                    let offset = core.store.lock().await.last_offset().unwrap_or(0);
                    core.last_offset.send_replace(offset);
                    accept(
                        cid,
                        false,
                        wire::CheckpointRestoreResult {
                            restored: true,
                            checkpoint_id: r.checkpoint_id.to_string(),
                            epoch: r.epoch,
                            chain: r.chain.iter().map(ToString::to_string).collect(),
                            files_written: r.files_written,
                            files_reverted: r.files_reverted,
                            workspace_revision_after: r.workspace_revision_after,
                            event_offset: r.event_offset,
                            refusal: String::new(),
                            detail: String::new(),
                            preconditions_checked: r.preconditions_checked,
                        }
                        .encode_to_vec(),
                    )
                }
                Ok(Err(refused)) => accept(
                    cid,
                    false,
                    wire::CheckpointRestoreResult {
                        restored: false,
                        refusal: refused.code.to_owned(),
                        detail: refused.detail,
                        ..Default::default()
                    }
                    .encode_to_vec(),
                ),
                Err(e) => reject(cid, "CHECKPOINT", e.to_string()),
            }
        }
        "GetRoutingSessionState" => {
            let Ok(p) = wire::GetRoutingSessionState::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetRoutingSessionState");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            let store = core.store.lock().await;
            match crate::routing::session_state(&store, session_id) {
                Some(v) => accept(cid, false, v.encode_to_vec()),
                None => reject(cid, "UNKNOWN_SESSION", session_id.to_string()),
            }
        }
        "GetTaskAssurance" => {
            let Ok(p) = wire::GetTaskAssurance::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetTaskAssurance");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let store = core.store.lock().await;
            let task = match store.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let (policy, notes) = crate::assurance::policy_for(
                &core.assurance_policy,
                Path::new(task.workspace_root.as_deref().unwrap_or(".")),
            );
            let latest = crate::assurance::latest(&store, &task);
            let gate = crate::gate::latest(&store, &task);
            accept(
                cid,
                false,
                wire::TaskAssuranceView {
                    task_id: Some(wire_id(task_id.as_bytes())),
                    derived: latest.is_some(),
                    realized_risk: latest
                        .as_ref()
                        .map(|(r, rref, _)| crate::assurance::view(r, rref)),
                    derived_at_offset: latest.as_ref().map(|(_, _, o)| *o).unwrap_or(0),
                    policy_version: policy.version(),
                    policy_notes: notes,
                    acceptance: gate
                        .as_ref()
                        .map(|(g, gref, off, trig)| crate::gate::view(g, gref, *off, trig)),
                }
                .encode_to_vec(),
            )
        }
        "PreviewRewind" => {
            let Ok(p) = wire::PreviewRewind::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "PreviewRewind");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let target = if p.checkpoint_id.is_empty() {
                None
            } else {
                match modbit_domain::CheckpointId::parse(&p.checkpoint_id) {
                    Ok(id) => Some(id),
                    Err(_) => return reject(cid, "BAD_PAYLOAD", "checkpoint_id is not an id"),
                }
            };
            let task = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if task.workspace_root.is_none() {
                return reject(cid, "NO_WORKSPACE", "the task has no workspace root");
            }
            // A preview is a read: no lease, no event, no write.
            match crate::branch::preview_rewind(core, &task, target).await {
                Ok(Ok(pv)) => {
                    let files_written = pv
                        .entries
                        .iter()
                        .filter(|e| matches!(e.action.as_str(), "WRITE" | "DELETE"))
                        .count() as u32;
                    let files_reverted = pv
                        .entries
                        .iter()
                        .filter(|e| {
                            matches!(e.action.as_str(), "REVERT_TO_HEAD" | "REMOVE_UNTRACKED")
                        })
                        .count() as u32;
                    accept(
                        cid,
                        false,
                        wire::RewindPreview {
                            checkpoint_id: pv.checkpoint_id.to_string(),
                            epoch: pv.epoch,
                            event_offset: pv.event_offset,
                            entries: pv
                                .entries
                                .into_iter()
                                .map(|e| wire::RewindEntryView {
                                    path: e.path,
                                    action: e.action,
                                    current_hash: e.current_hash.unwrap_or_default(),
                                    target_hash: e.target_hash.unwrap_or_default(),
                                })
                                .collect(),
                            files_written,
                            files_reverted,
                            workspace_revision: pv.workspace_revision,
                            refusal: String::new(),
                            detail: String::new(),
                        }
                        .encode_to_vec(),
                    )
                }
                Ok(Err(refused)) => accept(
                    cid,
                    false,
                    wire::RewindPreview {
                        refusal: refused.code.to_owned(),
                        detail: refused.detail,
                        ..Default::default()
                    }
                    .encode_to_vec(),
                ),
                Err(e) => reject(cid, "CHECKPOINT", e.to_string()),
            }
        }
        "ForkTask" => {
            let Ok(p) = wire::ForkTask::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ForkTask");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let checkpoint = if p.checkpoint_id.is_empty() {
                None
            } else {
                match modbit_domain::CheckpointId::parse(&p.checkpoint_id) {
                    Ok(id) => Some(id),
                    Err(_) => return reject(cid, "BAD_PAYLOAD", "checkpoint_id is not an id"),
                }
            };
            let mut carry = Vec::new();
            for c in &p.carry {
                match modbit_checkpoint::Carry::parse(c) {
                    Some(k) => {
                        if !carry.contains(&k) {
                            carry.push(k);
                        }
                    }
                    None => {
                        return reject(
                            cid,
                            "BAD_PAYLOAD",
                            format!("unknown carry `{c}` (PLAN | DECISIONS | EVIDENCE | CONTEXT)"),
                        );
                    }
                }
            }
            let source = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if source.workspace_root.is_none() {
                return reject(cid, "NO_WORKSPACE", "the source task has no workspace root");
            }
            if core.runtime.is_running(&task_id).await {
                return reject(
                    cid,
                    "TASK_RUNNING",
                    "the source's agent loop is executing; fork from a checkpoint once it stops",
                );
            }
            if let Err(ack) = require_lease(core, &cid, &env, &source.session_id).await {
                return ack;
            }
            let new_task_id = TaskId::from_bytes(command_id);
            {
                let store = core.store.lock().await;
                if let Ok(Some(existing)) = store.task(&new_task_id) {
                    // The command id is the task id: a replay returns the fork.
                    return accept(
                        cid,
                        true,
                        wire::TaskForked {
                            task_id: Some(wire_id(new_task_id.as_bytes())),
                            source_task_id: Some(wire_id(task_id.as_bytes())),
                            worktree: existing.workspace_root.unwrap_or_default(),
                            ..Default::default()
                        }
                        .encode_to_vec(),
                    );
                }
            }
            let req = crate::branch::ForkRequest {
                source,
                checkpoint,
                goal_text: if p.goal_text.trim().is_empty() {
                    None
                } else {
                    Some(p.goal_text.clone())
                },
                carry,
                worktree_dir: if p.worktree_dir.trim().is_empty() {
                    None
                } else {
                    Some(PathBuf::from(p.worktree_dir.trim()))
                },
                new_task_id,
                subagent: None,
            };
            match crate::branch::fork(core, req, &actor).await {
                Ok(Ok(f)) => accept(
                    cid,
                    false,
                    wire::TaskForked {
                        task_id: Some(wire_id(f.task_id.as_bytes())),
                        source_task_id: Some(wire_id(task_id.as_bytes())),
                        checkpoint_id: f.checkpoint_id.to_string(),
                        epoch: f.epoch,
                        capsule_ref: f.capsule_ref,
                        worktree: f.worktree,
                        branch: f.branch,
                        branch_generation: f.branch_generation,
                        carried: f.carried.iter().map(|c| c.label().to_owned()).collect(),
                        decisions_carried: f.decisions_carried,
                        evidence_carried: f.evidence_carried,
                        approvals_dropped: f.approvals_dropped,
                        calls_dropped: f.calls_dropped,
                        files_materialized: f.files_materialized,
                        offset: f.offset,
                    }
                    .encode_to_vec(),
                ),
                Ok(Err(refused)) => reject(cid, refused.code, refused.detail),
                Err(e) => reject(cid, "FORK", e.to_string()),
            }
        }
        "GetSessionTree" => {
            let Ok(p) = wire::GetSessionTree::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetSessionTree");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            let store = core.store.lock().await;
            match crate::branch::session_tree(&store, session_id) {
                Ok(v) => accept(cid, false, v.encode_to_vec()),
                Err(e) => reject(cid, "UNKNOWN_SESSION", e.to_string()),
            }
        }
        "UndoToolCall" => {
            let Ok(p) = wire::UndoToolCall::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "UndoToolCall");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let Some(call) = p
                .tool_call_id
                .as_ref()
                .and_then(id16)
                .map(ToolCallId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "tool_call_id required");
            };
            let task = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            let Some(root) = task.workspace_root.clone() else {
                return reject(cid, "NO_WORKSPACE", "the task has no workspace root");
            };
            let plan = match crate::undo::plan(core, &root, call).await {
                Ok(p) => p,
                Err(e) => return reject(cid, "UNDO_PLAN", e.to_string()),
            };
            if plan.steps.is_empty() {
                return reject(cid, "NOTHING_TO_UNDO", "the call changed no files");
            }
            if !p.apply {
                let rev = match core.tools.workspace(&root).await {
                    Ok((ws, _)) => ws.lock().await.revision().number,
                    Err(_) => 0,
                };
                return accept(
                    cid,
                    false,
                    crate::undo::view(&plan, false, vec![], rev).encode_to_vec(),
                );
            }
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            match crate::undo::apply(core, core.tenant_id, task.session_id, task_id, &root, &plan)
                .await
            {
                Ok((applied, refusals, rev)) => accept(
                    cid,
                    false,
                    crate::undo::view(&plan, applied, refusals, rev).encode_to_vec(),
                ),
                Err(e) => reject(cid, "UNDO_FAILED", e.to_string()),
            }
        }
        "GetEffectReceipts" => {
            let Ok(p) = wire::GetEffectReceipts::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetEffectReceipts");
            };
            let task = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes);
            let store = core.store.lock().await;
            // Verify the whole chain (links cross tasks), then project the requested slice.
            let all = match store.receipts(None) {
                Ok(r) => r,
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let verified = modbit_policy::ledger::verify_chain(&all);
            let receipts: Vec<_> = all
                .iter()
                .filter(|r| task.is_none_or(|t| r.task_id == t))
                .map(receipt_view)
                .collect();
            accept(
                cid,
                false,
                wire::EffectReceiptList {
                    receipts,
                    chain_valid: verified.is_ok(),
                    detail: verified.err().unwrap_or_default(),
                }
                .encode_to_vec(),
            )
        }
        "GetCapabilityLeases" => {
            let Ok(p) = wire::GetCapabilityLeases::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetCapabilityLeases");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let store = core.store.lock().await;
            match store.leases_for_task(&task_id) {
                Ok(list) => accept(
                    cid,
                    false,
                    wire::CapabilityLeaseList {
                        leases: list
                            .iter()
                            .map(|l| wire::CapabilityLeaseView {
                                lease_id: Some(wire_id(l.lease_id.as_bytes())),
                                task_id: Some(wire_id(l.task_id.as_bytes())),
                                resources: l.resources.clone(),
                                operations: l.operations.clone(),
                                effect_ceiling: format!("{:?}", l.effect_ceiling),
                                execution_profile: l.execution_profile.clone(),
                                generation: l.generation,
                                status: format!("{:?}", l.state).to_uppercase(),
                                revoke_reason: l.revoke_reason.clone().unwrap_or_default(),
                            })
                            .collect(),
                    }
                    .encode_to_vec(),
                ),
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "GetTaskEconomics" => {
            let Ok(p) = wire::GetTaskEconomics::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetTaskEconomics");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let view = crate::economics::view(core, task_id).await;
            accept(cid, false, view.encode_to_vec())
        }
        "GetContextInspector" => {
            let Ok(p) = wire::GetContextInspector::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetContextInspector");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let view = crate::inspector::view(core, task_id).await;
            accept(cid, false, view.encode_to_vec())
        }
        "GetRoutingPlan" => {
            let Ok(p) = wire::GetRoutingPlan::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetRoutingPlan");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let view = crate::routing::view(core, task_id).await;
            accept(cid, false, view.encode_to_vec())
        }
        // REQ-EPR-010: the request's accounting and outcome record.
        "GetRequestOutcome" => {
            let Ok(p) = wire::GetRequestOutcome::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetRequestOutcome");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let view = crate::accounting::view(core, task_id).await;
            if !view.found {
                return reject(cid, "UNKNOWN_TASK", task_id.to_string());
            }
            accept(cid, false, view.encode_to_vec())
        }
        // REQ-EPR-010 / EPR-FI-010: a late invoice for an attempt of
        // unknown usage, settled once.
        "ReconcileUsage" => {
            let Ok(p) = wire::ReconcileUsage::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ReconcileUsage");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let Some(run_id) = p
                .run_id
                .as_ref()
                .and_then(id16)
                .map(modbit_domain::RunId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "run_id required");
            };
            let actor = Actor::User(core.user_id);
            match crate::accounting::reconcile(core, &p, task_id, run_id, &actor).await {
                Ok(v) => accept(cid, false, v.encode_to_vec()),
                Err((code, detail)) => reject(cid, &code, detail),
            }
        }
        "GetWorkGraph" => {
            let Ok(p) = wire::GetWorkGraph::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetWorkGraph");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let store = core.store.lock().await;
            match store.task(&task_id) {
                Ok(Some(_)) => {}
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            }
            let nodes = store.work_nodes(&task_id).unwrap_or_default();
            let graph = modbit_domain::agent::WorkGraph {
                nodes: nodes.clone(),
            };
            let view = wire::WorkGraphView {
                task_id: Some(wire_id(task_id.as_bytes())),
                ready: graph.ready().iter().map(|n| n.id.clone()).collect(),
                plan_version: nodes.iter().map(|n| n.plan_version).max().unwrap_or(0),
                nodes: nodes
                    .into_iter()
                    .map(|n| wire::WorkNodeView {
                        id: n.id,
                        title: n.title,
                        depends_on: n.depends_on,
                        owner_agent_id: n.owner.map(|o| wire_id(o.as_bytes())),
                        status: serde_json::to_string(&n.status)
                            .unwrap_or_default()
                            .trim_matches('"')
                            .to_owned(),
                        expected_artifacts: n.expected_artifacts,
                        verification: n.verification,
                        evidence_refs: n.evidence_refs,
                        blockers: n.blockers,
                        attempts: n.attempts,
                        plan_version: n.plan_version,
                    })
                    .collect(),
            };
            accept(cid, false, view.encode_to_vec())
        }
        "GetAgentGraph" => {
            let Ok(p) = wire::GetAgentGraph::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetAgentGraph");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let store = core.store.lock().await;
            match store.task(&task_id) {
                Ok(Some(_)) => {}
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            }
            let view = wire::AgentGraphView {
                task_id: Some(wire_id(task_id.as_bytes())),
                nodes: store
                    .agent_nodes(&task_id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|n| wire::AgentNodeView {
                        agent_id: Some(wire_id(n.agent_id.as_bytes())),
                        parent_agent_id: n.parent_agent_id.map(|p| wire_id(p.as_bytes())),
                        root_agent_id: Some(wire_id(n.root_agent_id.as_bytes())),
                        depth: n.depth,
                        kind: n.kind,
                        status: n.status,
                        run_id: n.run_id.map(|r| wire_id(r.as_bytes())),
                        capsule_ref: n.capsule_ref.unwrap_or_default(),
                        endpoint: n.endpoint,
                        model: n.model,
                        idempotency_key: n.idempotency_key,
                        owns: n.owns,
                        created_at_ms: n.created_at.0,
                        updated_at_ms: n.updated_at.0,
                        last_offset: n.last_offset,
                    })
                    .collect(),
            };
            accept(cid, false, view.encode_to_vec())
        }
        "GetCapacity" => accept(cid, false, core.capacity.view().encode_to_vec()),
        "AdmitReviewEnvironment" => {
            let Ok(p) = wire::AdmitReviewEnvironment::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "AdmitReviewEnvironment");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let task = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let actor = Actor::User(core.user_id);
            match crate::review_env::admit(core, &task, &p.revision, &actor).await {
                Ok(e) => accept(
                    cid,
                    false,
                    wire::ReviewEnvironmentView {
                        env_id: e.env_id,
                        candidate_task_id: Some(wire_id(e.candidate_task_id.as_bytes())),
                        review_task_id: Some(wire_id(e.review_task_id.as_bytes())),
                        worktree: e.worktree,
                        branch: e.branch,
                        revision: e.revision,
                        lease_id: Some(wire_id(e.lease_id.as_bytes())),
                        sandbox: e.sandbox,
                    }
                    .encode_to_vec(),
                ),
                Err((code, detail)) => reject(cid, &code, detail),
            }
        }
        "DisposeReviewEnvironment" => {
            let Ok(p) = wire::DisposeReviewEnvironment::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "DisposeReviewEnvironment");
            };
            let found = {
                let store = core.store.lock().await;
                crate::review_env::find(&store, &p.env_id)
            };
            let Some(e) = found else {
                return reject(cid, "UNKNOWN_ENVIRONMENT", p.env_id);
            };
            let session_id = {
                let store = core.store.lock().await;
                store
                    .task(&e.candidate_task_id)
                    .ok()
                    .flatten()
                    .map(|t| t.session_id)
            };
            if let Some(sid) = session_id
                && let Err(ack) = require_lease(core, &cid, &env, &sid).await
            {
                return ack;
            }
            let actor = Actor::User(core.user_id);
            let reason = if p.reason.trim().is_empty() {
                "disposed by the client".to_owned()
            } else {
                p.reason.trim().to_owned()
            };
            let d = crate::review_env::dispose(core, &e, &reason, &actor).await;
            accept(
                cid,
                false,
                wire::ReviewEnvironmentDisposedAck {
                    env_id: e.env_id,
                    killed: d.killed,
                    worktree_removed: d.worktree_removed,
                }
                .encode_to_vec(),
            )
        }
        "GetPlan" => {
            let Ok(p) = wire::GetPlan::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetPlan");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let store = core.store.lock().await;
            match store.task(&task_id) {
                Ok(Some(_)) => {}
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            }
            accept(
                cid,
                false,
                crate::plans::view(&store, task_id).encode_to_vec(),
            )
        }
        "RevisePlan" => {
            let Ok(p) = wire::RevisePlan::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "RevisePlan");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if p.note.trim().is_empty() && p.plan_json.trim().is_empty() {
                return reject(cid, "BAD_PAYLOAD", "a note or an edited plan is required");
            }
            let task = {
                let store = core.store.lock().await;
                match store.task(&task_id) {
                    Ok(Some(t)) => t,
                    Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            // A plan is edited between runs, never under a live loop.
            if task.state == modbit_domain::task::TaskState::Running
                || core.runtime.is_running(&task_id).await
            {
                return reject(
                    cid,
                    "TASK_RUNNING",
                    "the task is running; steer it (QueueInput) or wait for its boundary",
                );
            }
            let provenance = if p.provenance.trim().is_empty() {
                "user_review".to_owned()
            } else {
                p.provenance.trim().to_owned()
            };
            let mut store = core.store.lock().await;
            match crate::plans::revise(
                &mut store,
                core,
                &task,
                p.note.trim(),
                p.plan_json.trim(),
                &provenance,
            ) {
                Ok((version, plan_ref, offset)) => accept(
                    cid,
                    false,
                    wire::PlanRevisedAck {
                        version,
                        plan_ref,
                        offset,
                    }
                    .encode_to_vec(),
                ),
                Err((code, detail)) => reject(cid, &code, detail),
            }
        }
        "GetAttention" => {
            let Ok(p) = wire::GetAttention::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetAttention");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            let store = core.store.lock().await;
            match store.session(&session_id) {
                Ok(Some(_)) => {}
                Ok(None) => return reject(cid, "UNKNOWN_SESSION", session_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            }
            let items = crate::attention::attention(&store, session_id);
            let view = wire::AttentionView {
                items: items
                    .into_iter()
                    .map(|i| wire::AttentionItemView {
                        kind: i.kind.to_owned(),
                        task_id: Some(wire_id(i.task_id.as_bytes())),
                        reference: i.reference,
                        reason: i.reason,
                        action: i.action,
                        since_offset: i.since_offset,
                    })
                    .collect(),
                last_offset: store.last_offset().unwrap_or(0),
            };
            accept(cid, false, view.encode_to_vec())
        }
        "GetProtocolState" => {
            let Ok(p) = wire::GetProtocolState::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetProtocolState");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let store = core.store.lock().await;
            match store.task(&task_id) {
                Ok(Some(_)) => {}
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            }
            let state = crate::protocol::reconstruct(&store, &task_id);
            accept(cid, false, protocol_state_view(&state).encode_to_vec())
        }
        "AdmitRoutingPlan" => {
            let Ok(p) = wire::AdmitRoutingPlan::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "AdmitRoutingPlan");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let session_id = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t.session_id,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            // Admission is a write against the run, so it is fenced by the
            // session lease like any other: a stale generation never installs
            // a plan.
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let view = crate::routing::admit(core, task_id, &p.plan_json).await;
            accept(cid, false, view.encode_to_vec())
        }
        "ActivateModelRegistry" => {
            let Ok(p) = wire::ActivateModelRegistry::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ActivateModelRegistry");
            };
            let view = crate::model_registry::activate(core, &p.signed_json);
            accept(cid, false, view.encode_to_vec())
        }
        "GetModelRegistry" => {
            let view = crate::model_registry::current(core);
            accept(cid, false, view.encode_to_vec())
        }
        "MaterializeOutcomeStatistics" => {
            let Ok(p) = wire::MaterializeOutcomeStatistics::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "MaterializeOutcomeStatistics");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            {
                let store = core.store.lock().await;
                match store.session(&session_id) {
                    Ok(Some(_)) => {}
                    Ok(None) => return reject(cid, "UNKNOWN_SESSION", session_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            }
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let view = crate::statistics::materialize(core, session_id, &p.stats_version).await;
            accept(cid, false, view.encode_to_vec())
        }
        "GetOutcomeStatistics" => {
            let Ok(p) = wire::GetOutcomeStatistics::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetOutcomeStatistics");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            let view = crate::statistics::get(core, session_id, &p.stats_version).await;
            accept(cid, false, view.encode_to_vec())
        }
        "CompileRoutingPlan" => {
            let Ok(p) = wire::CompileRoutingPlan::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "CompileRoutingPlan");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let session_id = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t.session_id,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            // Compiling installs a plan on the run, so it is fenced like any
            // other write.
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let pin = (!p.pin_endpoint.is_empty() && !p.pin_model.is_empty())
                .then(|| (p.pin_endpoint.clone(), p.pin_model.clone()));
            let view = crate::routing::compile(core, task_id, pin, p.request_cap_minor).await;
            accept(cid, false, view.encode_to_vec())
        }
        "ConfigureProvider" => {
            // Not journaled: the request carries a credential, and a command
            // record would keep a hash of it. The Core holds the key in
            // memory and nothing else.
            let Ok(p) = wire::ConfigureProvider::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ConfigureProvider");
            };
            match crate::onboarding::configure_provider(core, &p) {
                Ok(v) => accept(cid, false, v.encode_to_vec()),
                Err((code, msg)) => reject(cid, &code, msg),
            }
        }
        "ConfigureForge" => {
            // Not journaled, for the same reason as ConfigureProvider: the
            // request carries a credential. The Core holds it in memory and
            // answers with everything but the token.
            let Ok(p) = wire::ConfigureForge::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ConfigureForge");
            };
            if p.forge != "github" {
                return reject(
                    cid,
                    "UNSUPPORTED_FORGE",
                    format!("`{}` is not a forge this build reaches (github)", p.forge),
                );
            }
            let api_base = if p.api_base_url.trim().is_empty() {
                "https://api.github.com".to_owned()
            } else {
                p.api_base_url.trim().trim_end_matches('/').to_owned()
            };
            if !(api_base.starts_with("https://")
                || api_base.starts_with("http://127.0.0.1")
                || api_base.starts_with("http://localhost"))
            {
                return reject(
                    cid,
                    "BAD_PAYLOAD",
                    "api_base_url must be https (or a loopback test host)",
                );
            }
            let cfg = modbit_tools::forge::ForgeConfig {
                kind: "github".into(),
                api_base,
                web_host: if p.web_host.trim().is_empty() {
                    "github.com".to_owned()
                } else {
                    p.web_host.trim().to_owned()
                },
                token: (!p.token.trim().is_empty()).then(|| p.token.trim().to_owned()),
            };
            let egress = cfg.egress_target();
            let view = wire::ForgeConfigured {
                forge: cfg.kind.clone(),
                api_base_url: cfg.api_base.clone(),
                web_host: cfg.web_host.clone(),
                token_held: cfg.token.is_some(),
                egress,
            };
            core.tools.forge.set(cfg);
            accept(cid, false, view.encode_to_vec())
        }
        // M9.4 (REQ-EV-0224): a client proposes an external tool server on
        // its own behalf or relaying what an agent suggested. The host
        // validates it and writes it into the user configuration layer as
        // PROPOSED — inert until a person trusts it. Proposing is not
        // installing, and nothing here starts a process.
        "ProposeExternalServer" => {
            let Ok(p) = wire::ProposeExternalServer::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ProposeExternalServer");
            };
            match crate::mcp::propose(core, &p.name, &p.definition_json, &p.reason) {
                Ok(view) => accept(cid, false, view.encode_to_vec()),
                Err((code, message)) => reject(cid, code, message),
            }
        }
        // M9.4 (REQ-EV-0224): a person trusts a proposal, or takes trust
        // away. Every gate is answered here rather than at the moment the
        // server would have run: the definition must validate, no higher
        // layer may have denied the name, and a named credential must be in
        // the Core's custody.
        "TrustExternalServer" => {
            let Ok(p) = wire::TrustExternalServer::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "TrustExternalServer");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(modbit_domain::SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            match crate::mcp::set_trust(core, &p.name, p.trust) {
                Ok(view) => {
                    if !p.trust {
                        // Taking trust away stops the program, not merely
                        // the next task's access to it.
                        core.tools.mcp.stop_named(&view.name).await;
                    }
                    accept(cid, false, view.encode_to_vec())
                }
                Err((code, message)) => reject(cid, code, message),
            }
        }
        // M9.4 (REQ-EV-0224): a credential for an external server, by
        // handle. Not journaled, for the same reason as ConfigureProvider
        // and ConfigureForge: the request carries a secret. The Core holds
        // it in memory and answers with everything but the value.
        "ConfigureExternalCredential" => {
            let Ok(p) = wire::ConfigureExternalCredential::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ConfigureExternalCredential");
            };
            let handle = p.handle.trim().to_owned();
            if handle.is_empty() {
                return reject(cid, "BAD_PAYLOAD", "handle required");
            }
            if p.value.is_empty() {
                core.tools.mcp.clear_credential(&handle);
            } else {
                core.tools.mcp.set_credential(&handle, p.value.clone());
            }
            let view = wire::ExternalCredentialConfigured {
                held: core.tools.mcp.has_credential(&handle),
                handle,
            };
            accept(cid, false, view.encode_to_vec())
        }
        "ExportHandoff" => {
            let Ok(p) = wire::ExportHandoff::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ExportHandoff");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if p.out_dir.trim().is_empty() {
                return reject(cid, "BAD_PAYLOAD", "out_dir required");
            }
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let out = std::path::PathBuf::from(p.out_dir.trim());
            let exported = crate::handoff::export(core, task_id, &actor, &out).await;
            match exported {
                Ok(x) => accept(
                    cid,
                    false,
                    wire::HandoffExported {
                        task_id: Some(wire_id(task_id.as_bytes())),
                        bundle_dir: out.to_string_lossy().into_owned(),
                        manifest_json: x.manifest.to_string(),
                        manifest_hash: x.manifest_hash,
                        parts: x.parts,
                    }
                    .encode_to_vec(),
                ),
                Err((code, why)) => reject(cid, code.as_str(), why),
            }
        }
        "RebindTaskWorkspace" => {
            let Ok(p) = wire::RebindTaskWorkspace::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "RebindTaskWorkspace");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if !std::path::Path::new(&p.workspace_root).is_dir() {
                return reject(
                    cid,
                    "REPOSITORY_MISSING",
                    format!("`{}` is not a directory on this machine", p.workspace_root),
                );
            }
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let ev = typed(
                "TaskWorkspaceRebound",
                &TaskEvent::TaskWorkspaceRebound {
                    workspace_root: p.workspace_root.clone(),
                    reason: if p.reason.is_empty() {
                        "handoff".into()
                    } else {
                        p.reason.clone()
                    },
                    execution_profile: p.execution_profile.clone(),
                },
                actor.clone(),
            );
            let mut store = core.store.lock().await;
            // A profile change retires the leases granted under the old one
            // (M8.7): a lease is valid under one profile, and the kernel
            // denies every tool under another. StartTask grants the new
            // profile's default lease before the continuation runs.
            if !p.execution_profile.is_empty() && p.execution_profile != task.execution_profile {
                let leases = match store.leases_for_task(&task_id) {
                    Ok(l) => l,
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                };
                for l in leases.into_iter().filter(|l| l.is_valid(Timestamp::now())) {
                    if let Err(e) = store.append(AppendRequest {
                        tenant_id: core.tenant_id,
                        session_id: task.session_id,
                        task_id: Some(task_id),
                        run_id: None,
                        turn_id: None,
                        step_id: None,
                        aggregate_type: AggregateType::CapabilityLease,
                        aggregate_id: *l.lease_id.as_bytes(),
                        expected_sequence: None,
                        events: vec![typed(
                            "CapabilityLeaseRevoked",
                            &CapabilityLeaseEvent::CapabilityLeaseRevoked {
                                reason: format!(
                                    "handoff: the task continues under `{}`, not `{}`",
                                    p.execution_profile, l.execution_profile
                                ),
                            },
                            actor.clone(),
                        )],
                    }) {
                        return reject(cid, error_code(&e), e.to_string());
                    }
                }
            }
            match store.append(AppendRequest {
                tenant_id: core.tenant_id,
                session_id: task.session_id,
                task_id: Some(task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task_id.as_bytes(),
                expected_sequence: None,
                events: vec![ev],
            }) {
                Ok(stored) => {
                    let offset = stored.last().map(|e| e.offset).unwrap_or(0);
                    core.last_offset.send_replace(offset);
                    accept(
                        cid,
                        false,
                        wire::TaskWorkspaceRebound {
                            task_id: Some(wire_id(task_id.as_bytes())),
                            offset,
                        }
                        .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, error_code(&e), e.to_string()),
            }
        }
        "ImportObjects" => {
            let Ok(p) = wire::ImportObjects::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ImportObjects");
            };
            let store = core.store.lock().await;
            let mut hashes = Vec::with_capacity(p.objects.len());
            for bytes in &p.objects {
                match store.objects().put(bytes) {
                    Ok(h) => hashes.push(h),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            }
            accept(cid, false, wire::ObjectsImported { hashes }.encode_to_vec())
        }
        "ConfigureSandboxGateway" => {
            // Not journaled: the request carries the worker's credential.
            let Ok(p) = wire::ConfigureSandboxGateway::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ConfigureSandboxGateway");
            };
            if p.worker_token.trim().is_empty() || p.base_url.trim().is_empty() {
                *core.tools.sandbox_gateway.lock().await = None;
                return accept(
                    cid,
                    false,
                    wire::SandboxGatewayConfigured {
                        base_url: String::new(),
                        worker_id: String::new(),
                        lease_generation: 0,
                        token_held: false,
                    }
                    .encode_to_vec(),
                );
            }
            let base = p.base_url.trim().trim_end_matches('/').to_owned();
            let loopback = base.starts_with("http://127.0.0.1")
                || base.starts_with("http://localhost")
                || base.starts_with("http://[::1]");
            if !base.starts_with("https://") && !loopback {
                return reject(
                    cid,
                    "BAD_PAYLOAD",
                    "base_url must be https (or a loopback test host)",
                );
            }
            let custody = crate::tools::SandboxGatewayCustody {
                client: modbit_sandbox::client::GatewayClient::new(&base, p.worker_token.trim()),
                tenant_id: p.tenant_id.clone(),
                lease_generation: p.lease_generation,
                worker_id: p.worker_id.clone(),
                features: p.features.clone(),
            };
            *core.tools.sandbox_gateway.lock().await = Some(custody);
            accept(
                cid,
                false,
                wire::SandboxGatewayConfigured {
                    base_url: base,
                    worker_id: p.worker_id,
                    lease_generation: p.lease_generation,
                    token_held: true,
                }
                .encode_to_vec(),
            )
        }
        "TrustRepository" => {
            let Ok(p) = wire::TrustRepository::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "TrustRepository");
            };
            let Some(session_id) = p
                .session_id
                .as_ref()
                .and_then(id16)
                .map(SessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "session_id required");
            };
            if p.workspace_root.trim().is_empty() {
                return reject(cid, "BAD_PAYLOAD", "workspace_root required");
            }
            if !std::path::Path::new(&p.workspace_root).is_dir() {
                return reject(
                    cid,
                    "REPOSITORY_MISSING",
                    format!("`{}` is not a directory on this machine", p.workspace_root),
                );
            }
            {
                let store = core.store.lock().await;
                match store.session(&session_id) {
                    Ok(Some(_)) => {}
                    Ok(None) => return reject(cid, "UNKNOWN_SESSION", session_id.to_string()),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            }
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id,
                task_id: None,
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Session,
                aggregate_id: *session_id.as_bytes(),
                expected_sequence: None,
                events: vec![typed(
                    "RepositoryTrusted",
                    &modbit_domain::session::SessionEvent::RepositoryTrusted {
                        workspace_root: p.workspace_root.clone(),
                        scope: if p.scope.is_empty() {
                            "repository".into()
                        } else {
                            p.scope.clone()
                        },
                    },
                    actor.clone(),
                )],
            };
            let offset = {
                let mut store = core.store.lock().await;
                match store.append(req) {
                    Ok(stored) => stored.last().map(|e| e.offset).unwrap_or(0),
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            };
            if offset > 0 {
                core.last_offset.send_replace(offset);
            }
            accept(
                cid,
                false,
                wire::RepositoryTrusted {
                    workspace_root: p.workspace_root,
                    offset,
                }
                .encode_to_vec(),
            )
        }
        "ListStarterTasks" => {
            let Ok(p) = wire::ListStarterTasks::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ListStarterTasks");
            };
            let (stacks, tasks) = crate::onboarding::starter_tasks(&p.workspace_root);
            accept(
                cid,
                false,
                wire::StarterTaskList {
                    stacks,
                    tasks,
                    trusted: false,
                }
                .encode_to_vec(),
            )
        }
        "ListLanguages" => {
            let languages = crate::languages::catalog();
            accept(cid, false, wire::LanguageList { languages }.encode_to_vec())
        }
        "ListModels" => {
            let gw = &core.gateway;
            let mut models = Vec::new();
            let mut health = Vec::new();
            for ep in gw.endpoints() {
                let credential_available =
                    matches!(ep.credential, modbit_providers::SecretHandle::None)
                        || ep.credential.resolve().is_some();
                for m in &ep.models {
                    models.push(wire::ModelCapabilityView {
                        endpoint: ep.name.clone(),
                        provider: format!("{:?}", ep.kind).to_lowercase(),
                        model: m.model.clone(),
                        context_tokens: m.context_tokens,
                        max_output_tokens: m.max_output_tokens,
                        tools: m.tools,
                        vision: m.vision,
                        reasoning: m.reasoning,
                        structured_output: m.structured_output,
                        input_modalities: m.input_modalities.clone(),
                        credential_available,
                        blocked_by_policy: gw
                            .policy()
                            .blocking_rule(&ep.name, ep.kind, &m.model)
                            .unwrap_or_default(),
                    });
                }
                let h = gw.health(&ep.name);
                health.push(wire::EndpointHealthView {
                    endpoint: ep.name.clone(),
                    requests: h.requests,
                    successes: h.successes,
                    failures: h.failures,
                    interruptions: h.interruptions,
                    cancellations: h.cancellations,
                    rate_limited: h.rate_limited,
                    last_first_token_ms: h.last_first_token_ms.unwrap_or(0),
                });
            }
            accept(
                cid,
                false,
                wire::ModelList { models, health }.encode_to_vec(),
            )
        }
        "ProbeModel" => {
            let Ok(p) = wire::ProbeModel::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ProbeModel");
            };
            accept(
                cid,
                false,
                crate::probe::probe(&core.gateway, &p).await.encode_to_vec(),
            )
        }
        "StartTask" => {
            let Ok(p) = wire::StartTask::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "StartTask");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            // docs/23 "Emergency stop": a stopped session starts nothing; the
            // stop is on the log and lasts for the session.
            if let Ok(Some(s)) = core.store.lock().await.session(&task.session_id)
                && s.emergency_stopped_at.is_some()
            {
                return reject(
                    cid,
                    "EMERGENCY_STOP",
                    "the session is under an emergency stop; no run starts in it",
                );
            }
            // REQ-PX-022: a desktop task runs only on a repository the user
            // trusted in this session, explicitly and scoped to that root.
            // The headless CLI and the adapters run where the operator points
            // them, which is the trust act itself.
            if task.origin == modbit_domain::task::TaskOrigin::Desktop
                && let Some(root) = task.workspace_root.as_deref()
                && !crate::onboarding::is_trusted(&*core.store.lock().await, task.session_id, root)
            {
                return reject(
                    cid,
                    "REPOSITORY_UNTRUSTED",
                    format!(
                        "`{root}` has not been trusted in this session; trust it (TrustRepository) before starting a task there"
                    ),
                );
            }
            let lease_generation = env.expected_generation.unwrap_or(0);
            // A task materialized from the cloud log (M8.2: created by the
            // Cloud API, not by this Core's CreateTask) has no capability
            // lease yet; it gets its profile's default one here, exactly as
            // CreateTask grants it, before anything runs under it. So does a
            // task handed off into another profile (M8.7): its laptop lease
            // was retired at the rebind.
            {
                let mut store = core.store.lock().await;
                let has_lease = store
                    .leases_for_task(&task_id)
                    .map(|l| {
                        l.iter().any(|l| {
                            l.execution_profile == task.execution_profile
                                && l.is_valid(Timestamp::now())
                        })
                    })
                    .unwrap_or(false);
                if !has_lease {
                    let (resources, operations, effect_ceiling) =
                        modbit_policy::default_lease_for_profile(
                            &task.execution_profile,
                            task.workspace_root.as_deref(),
                        );
                    let lease_id = modbit_domain::CapabilityLeaseId::new();
                    let grant = AppendRequest {
                        tenant_id: core.tenant_id,
                        session_id: task.session_id,
                        task_id: Some(task_id),
                        run_id: None,
                        turn_id: None,
                        step_id: None,
                        aggregate_type: AggregateType::CapabilityLease,
                        aggregate_id: *lease_id.as_bytes(),
                        expected_sequence: Some(0),
                        events: vec![typed(
                            "CapabilityLeaseGranted",
                            &CapabilityLeaseEvent::CapabilityLeaseGranted {
                                tenant_id: core.tenant_id,
                                task_id,
                                agent_id: None,
                                resources,
                                operations,
                                effect_ceiling,
                                execution_profile: task.execution_profile.clone(),
                                generation: 1,
                                expires_at: None,
                            },
                            actor.clone(),
                        )],
                    };
                    match store.append(grant) {
                        Ok(stored) => {
                            if let Some(last) = stored.last() {
                                core.last_offset.send_replace(last.offset);
                            }
                        }
                        Err(e) => return reject(cid, error_code(&e), e.to_string()),
                    }
                }
            }
            // M8.5: a `cloud_isolated` task runs inside a sandbox the
            // gateway issues for it; without one the run does not start.
            if task.execution_profile == modbit_policy::kernel::PROFILE_CLOUD_ISOLATED {
                let sandbox = match crate::sandboxes::ensure_for_task(core, &task, &actor).await {
                    Ok(h) => h,
                    Err((code, why)) => return reject(cid, &code, why),
                };
                // M8.8: the task's browser is the Chromium inside that
                // sandbox, hosted by this Core over the gateway's relay,
                // when the sandbox's policy grants one; the session exists
                // from the start, the browser runs on first use.
                if modbit_sandbox::port::SandboxPort::identity(&*sandbox).browser {
                    // A session without a live host (a restart) is re-hosted.
                    let hosted =
                        match core.browser.session_for_task(task_id).await {
                            Some(b) => core.browser.get(b).await.is_some_and(|r| {
                                r.host.as_ref().is_some_and(|h| !h.tx.is_closed())
                            }),
                            None => false,
                        };
                    if !hosted
                        && let Err(why) =
                            crate::browser_cloud::CloudHost::attach(core, &task, sandbox, &actor)
                                .await
                    {
                        return reject(cid, "BROWSER_HOST", why);
                    }
                }
            }
            // Model policy: request → environment defaults → first registered.
            let endpoints = core.gateway.endpoints();
            // A named endpoint must be registered in this Core: a provider
            // registration is live control, gone with the process (docs/33,
            // REQ-EV-0242), so a run is refused here rather than started to
            // fail at its first model call.
            if !p.endpoint.is_empty() && !endpoints.iter().any(|e| e.name == p.endpoint) {
                return reject(
                    cid,
                    "NO_PROVIDER",
                    format!(
                        "provider endpoint `{}` is not registered in this Core (configure it with ConfigureProvider, or set OPENAI_API_KEY / ANTHROPIC_API_KEY)",
                        p.endpoint
                    ),
                );
            }
            let endpoint = if !p.endpoint.is_empty() {
                p.endpoint.clone()
            } else if let Ok(e) = std::env::var("MODBIT_DEFAULT_ENDPOINT") {
                e
            } else if let Some(e) = endpoints.first() {
                e.name.clone()
            } else {
                return reject(
                    cid,
                    "NO_PROVIDER",
                    "no provider endpoint is registered (set OPENAI_API_KEY / ANTHROPIC_API_KEY)",
                );
            };
            let model = if !p.model.is_empty() {
                p.model.clone()
            } else if let Ok(m) = std::env::var("MODBIT_DEFAULT_MODEL") {
                m
            } else if let Some(m) = endpoints
                .iter()
                .find(|e| e.name == endpoint)
                .and_then(|e| e.models.iter().find(|m| m.tools))
            {
                m.model.clone()
            } else {
                return reject(
                    cid,
                    "NO_MODEL",
                    format!("endpoint `{endpoint}` serves no tool-capable model"),
                );
            };
            let mut budgets = modbit_core_runtime::Budgets::default();
            if p.max_turns > 0 {
                budgets.max_turns = p.max_turns;
            }
            if p.max_tool_calls > 0 {
                budgets.max_tool_calls = p.max_tool_calls;
            }
            if p.max_no_progress_turns > 0 {
                budgets.max_consecutive_no_progress_turns = p.max_no_progress_turns;
            }
            let cfg = crate::runtime::StartConfig {
                endpoint: endpoint.clone(),
                model: model.clone(),
                budgets,
                pinned: !p.model.is_empty(),
                plan_id: String::new(),
                skills: p.skills.clone(),
                slot_id: String::new(),
                lease_generation: 0,
                ticket_id: String::new(),
            };
            match core
                .runtime
                .start(core, task, cfg, lease_generation, actor)
                .await
            {
                Ok((run_id, resumed)) => accept(
                    cid,
                    false,
                    wire::TaskRunStarted {
                        run_id: Some(wire_id(run_id.as_bytes())),
                        resumed,
                        endpoint,
                        model,
                    }
                    .encode_to_vec(),
                ),
                Err((code, msg)) => reject(cid, &code, msg),
            }
        }
        // ---- M7.1 browser sessions (docs/22) ----
        "OpenBrowserSession" => {
            let Ok(p) = wire::OpenBrowserSession::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "OpenBrowserSession");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            // One open session per task: opening again answers the one there is.
            if let Some(existing) = core.browser.session_for_task(task_id).await
                && let Some(rec) = core.browser.get(existing).await
            {
                return accept(
                    cid,
                    true,
                    wire::BrowserSessionOpened {
                        browser_session_id: Some(wire::Id {
                            value: existing.as_bytes().to_vec(),
                        }),
                        partition: rec.partition,
                        controller: "AGENT".into(),
                        lease_generation: rec.lease.generation,
                        offset: 0,
                    }
                    .encode_to_vec(),
                );
            }
            // The session id is the command id: a replayed command names the same session.
            let bsid = modbit_browser::BrowserSessionId::from_bytes(command_id);
            let rec = core.browser.open(bsid, task_id, task.session_id).await;
            let offset = match crate::runtime::append_batch(
                &mut *core.store.lock().await,
                core,
                crate::runtime::Lineage::task(core.tenant_id, task.session_id, task_id),
                vec![(
                    AggregateType::Task,
                    *task_id.as_bytes(),
                    vec![crate::runtime::typed(
                        "BrowserSessionOpened",
                        &TaskEvent::BrowserSessionOpened {
                            browser_session_id: bsid.to_string(),
                            partition: rec.partition.clone(),
                        },
                        actor.clone(),
                    )],
                )],
            ) {
                Ok(o) => o,
                Err(e) => return reject(cid, "STORE", e),
            };
            accept(
                cid,
                false,
                wire::BrowserSessionOpened {
                    browser_session_id: Some(wire::Id {
                        value: bsid.as_bytes().to_vec(),
                    }),
                    partition: rec.partition,
                    controller: "AGENT".into(),
                    lease_generation: rec.lease.generation,
                    offset,
                }
                .encode_to_vec(),
            )
        }
        "BrowserHostResponse" => {
            let Ok(p) = wire::BrowserHostResponse::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "BrowserHostResponse");
            };
            let response: modbit_browser::HostResponse =
                match serde_json::from_str(&p.response_json) {
                    Ok(r) => r,
                    Err(e) => {
                        return reject(cid, "BAD_PAYLOAD", format!("response_json: {e}"));
                    }
                };
            let delivered = core.browser.deliver(&p.request_id, response).await;
            accept(
                cid,
                false,
                wire::BrowserHostResponded { delivered }.encode_to_vec(),
            )
        }
        "RegisterBrowserCredential" => {
            // M7.8: handle metadata only — a value in any field is refused.
            let Ok(p) = wire::RegisterBrowserCredential::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "RegisterBrowserCredential");
            };
            if !p.handle.starts_with("cred_") || p.handle.len() < 9 || p.handle.len() > 64 {
                return reject(cid, "BAD_PAYLOAD", "handle must be `cred_<id>`");
            }
            let Some(origin) = modbit_browser::origin_of(&p.origin) else {
                return reject(
                    cid,
                    "BAD_ORIGIN",
                    format!("`{}` is not an http(s) origin", p.origin),
                );
            };
            if origin != p.origin.trim() {
                return reject(
                    cid,
                    "BAD_ORIGIN",
                    format!("origin must be exactly `{origin}` (scheme://host[:port])"),
                );
            }
            if p.label.len() > 200 || p.username.len() > 200 {
                return reject(
                    cid,
                    "BAD_PAYLOAD",
                    "label and username are at most 200 bytes",
                );
            }
            core.browser
                .register_credential(modbit_browser::CredentialHandle {
                    handle: p.handle.clone(),
                    label: p.label,
                    origin: origin.clone(),
                    username: p.username,
                })
                .await;
            accept(
                cid,
                false,
                wire::BrowserCredentialRegistered {
                    handle: p.handle,
                    origin,
                }
                .encode_to_vec(),
            )
        }
        "ForgetBrowserCredential" => {
            let Ok(p) = wire::ForgetBrowserCredential::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ForgetBrowserCredential");
            };
            let existed = core.browser.forget_credential(&p.handle).await;
            accept(
                cid,
                !existed,
                wire::BrowserCredentialForgotten {
                    handle: p.handle,
                    existed,
                }
                .encode_to_vec(),
            )
        }
        "GetBrowserSession" => {
            let Ok(p) = wire::GetBrowserSession::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetBrowserSession");
            };
            let Some(bsid) = p
                .browser_session_id
                .as_ref()
                .and_then(id16)
                .map(modbit_browser::BrowserSessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "browser_session_id required");
            };
            let rec = match browser_session_record(core, bsid, p.task_id.as_ref()).await {
                Ok(r) => r,
                Err(ack) => return ack(cid),
            };
            accept(cid, false, crate::browser::view(bsid, &rec).encode_to_vec())
        }
        "SetBrowserControl" => {
            let Ok(p) = wire::SetBrowserControl::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "SetBrowserControl");
            };
            let Some(bsid) = p
                .browser_session_id
                .as_ref()
                .and_then(id16)
                .map(modbit_browser::BrowserSessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "browser_session_id required");
            };
            let to = match p.controller.as_str() {
                "AGENT" => modbit_browser::Controller::Agent,
                "USER" => modbit_browser::Controller::User,
                other => {
                    return reject(
                        cid,
                        "BAD_PAYLOAD",
                        format!("controller must be AGENT or USER, not `{other}`"),
                    );
                }
            };
            let rec = match browser_session_record(core, bsid, p.task_id.as_ref()).await {
                Ok(r) => r,
                Err(ack) => return ack(cid),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &rec.session_id).await {
                return ack;
            }
            if rec.closed {
                return reject(cid, "SESSION_CLOSED", bsid.to_string());
            }
            let Some((lease, changed)) = core.browser.hand_control(bsid, to).await else {
                return reject(cid, "NO_SUCH_SESSION", bsid.to_string());
            };
            let controller = match lease.controller {
                modbit_browser::Controller::Agent => "AGENT",
                modbit_browser::Controller::User => "USER",
            };
            let offset = if changed {
                match crate::runtime::append_batch(
                    &mut *core.store.lock().await,
                    core,
                    crate::runtime::Lineage::task(core.tenant_id, rec.session_id, rec.task_id),
                    vec![(
                        AggregateType::Task,
                        *rec.task_id.as_bytes(),
                        vec![crate::runtime::typed(
                            "BrowserControlChanged",
                            &TaskEvent::BrowserControlChanged {
                                browser_session_id: bsid.to_string(),
                                controller: controller.into(),
                                lease_generation: lease.generation,
                            },
                            actor.clone(),
                        )],
                    )],
                ) {
                    Ok(o) => o,
                    Err(e) => return reject(cid, "STORE", e),
                }
            } else {
                0
            };
            accept(
                cid,
                !changed,
                wire::BrowserControlChanged {
                    browser_session_id: p.browser_session_id,
                    controller: controller.into(),
                    lease_generation: lease.generation,
                    changed,
                    offset,
                }
                .encode_to_vec(),
            )
        }
        // REQ-EV-0021/0062/0146: the environment revision a task is pinned
        // to, against what is there now.
        "GetEnvironment" => {
            let Ok(p) = wire::GetEnvironment::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetEnvironment");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let pinned = crate::environment::pinned(core, &task).await;
            let now = crate::environment::current(core, &task).await;
            let (shown, stale, changes, revision_ref, pinned_digest) = match &pinned {
                Some(p) => {
                    let stale = p.revision.digest != now.digest;
                    let changes = if stale {
                        modbit_workspace::environment::changes(&p.revision, &now)
                    } else {
                        vec![]
                    };
                    (
                        p.revision.clone(),
                        stale,
                        changes,
                        p.revision_ref.clone(),
                        p.revision.digest.clone(),
                    )
                }
                None => (now.clone(), false, vec![], String::new(), String::new()),
            };
            accept(
                cid,
                false,
                wire::EnvironmentView {
                    task_id: Some(wire_id(task_id.as_bytes())),
                    pinned_digest,
                    current_digest: now.digest,
                    stale,
                    changes,
                    sources: shown
                        .sources
                        .iter()
                        .map(|s| wire::EnvironmentSource {
                            kind: s.kind.clone(),
                            path: s.path.clone(),
                            sha256: s.sha256.clone(),
                        })
                        .collect(),
                    toolchain: shown
                        .toolchain
                        .iter()
                        .map(|t| wire::EnvironmentTool {
                            name: t.name.clone(),
                            version: t.version.clone().unwrap_or_default(),
                            optional: t.optional,
                        })
                        .collect(),
                    path: shown.path.clone(),
                    env_names: shown.env_names.clone(),
                    problems: shown.problems.clone(),
                    revision_ref,
                }
                .encode_to_vec(),
            )
        }
        "RebuildEnvironment" => {
            let Ok(p) = wire::RebuildEnvironment::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "RebuildEnvironment");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            // A running run keeps its revision; the rebuild is for a run at
            // rest (waiting on the stale environment, queued, or reviewed).
            if core.runtime.is_running(&task_id).await {
                return reject(
                    cid,
                    "TASK_RUNNING",
                    "the run is in progress on its pinned environment; rebuild once it waits",
                );
            }
            match crate::environment::rebuild(core, &task, &actor).await {
                Ok((from, to, changes, offset)) => {
                    core.last_offset.send_replace(offset);
                    accept(
                        cid,
                        false,
                        wire::EnvironmentRebuilt {
                            task_id: Some(wire_id(task_id.as_bytes())),
                            from_digest: from,
                            to_digest: to,
                            changes,
                            offset,
                        }
                        .encode_to_vec(),
                    )
                }
                Err(why) => reject(cid, "ENVIRONMENT_UNAVAILABLE", why),
            }
        }
        // M9.1 (REQ-EV-0162, docs/19): governed engineering memory —
        // inspect the task's scope chain (proposals included, conflicts
        // surfaced), promote a proposal (a governed step, refused with a
        // typed reason when the rules do not allow it), or forget an item.
        "ListMemory" => {
            let Ok(p) = wire::ListMemory::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ListMemory");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let chain = crate::memory::scope_chain_for(core.tenant_id, core.user_id, &task);
            match crate::memory::list_scoped(&core.store, &chain).await {
                Ok((items, scopes, conflicts)) => accept(
                    cid,
                    false,
                    wire::MemoryList {
                        task_id: Some(wire_id(task_id.as_bytes())),
                        items: items.iter().map(memory_item_view).collect(),
                        scopes,
                        conflicts: conflicts
                            .into_iter()
                            .map(|item_ids| wire::MemoryConflict { item_ids })
                            .collect(),
                    }
                    .encode_to_vec(),
                ),
                Err(e) => reject(cid, "MEMORY_STORE", e),
            }
        }
        "PromoteMemory" => {
            let Ok(p) = wire::PromoteMemory::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "PromoteMemory");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if p.memory_id.is_empty() {
                return reject(cid, "BAD_PAYLOAD", "memory_id required");
            }
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            // The promotion context: the workspace's current revision (a
            // repository fact must bind to it) and whether the scope permits
            // sensitive memory (conservative default: no; a later slice wires
            // policy). Now, to reject an expired proposal.
            let current_repository_revision = current_revision_of(core, &task).await;
            let pctx = modbit_memory::PromotionContext {
                scope_permits_sensitive: false,
                current_repository_revision,
                now_ms: modbit_domain::Timestamp::now().0,
            };
            match crate::memory::promote(&core.store, &p.memory_id, &pctx).await {
                Ok(outcome) => {
                    use crate::memory::Promoted;
                    let (out, code, detail, superseded) = match outcome {
                        Promoted::Curated { superseded, .. } => {
                            ("curated", String::new(), String::new(), superseded)
                        }
                        Promoted::Unknown => (
                            "unknown",
                            "UNKNOWN_MEMORY".to_owned(),
                            "no such proposal".to_owned(),
                            vec![],
                        ),
                        Promoted::Refused(refusal) => {
                            let v = serde_json::to_value(&refusal).unwrap_or_default();
                            let code = v
                                .get("code")
                                .and_then(|c| c.as_str())
                                .unwrap_or("REFUSED")
                                .to_owned();
                            ("refused", code, v.to_string(), vec![])
                        }
                    };
                    accept(
                        cid,
                        false,
                        wire::MemoryPromoted {
                            memory_id: p.memory_id.clone(),
                            outcome: out.to_owned(),
                            refusal_code: code,
                            refusal_detail: detail,
                            superseded,
                            offset: core.last_offset.borrow().to_owned(),
                        }
                        .encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, "MEMORY_STORE", e),
            }
        }
        "ForgetMemory" => {
            let Ok(p) = wire::ForgetMemory::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ForgetMemory");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if p.memory_id.is_empty() {
                return reject(cid, "BAD_PAYLOAD", "memory_id required");
            }
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let status = if p.supersede {
                modbit_memory::Status::Superseded
            } else {
                modbit_memory::Status::Deleted
            };
            match crate::memory::set_status(&core.store, &p.memory_id, status).await {
                Ok(changed) => accept(
                    cid,
                    false,
                    wire::MemoryForgotten {
                        memory_id: p.memory_id.clone(),
                        changed,
                        status: serde_json::to_value(status)
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_owned))
                            .unwrap_or_default(),
                    }
                    .encode_to_vec(),
                ),
                Err(e) => reject(cid, "MEMORY_STORE", e),
            }
        }
        // M8.8: the person's input into a view they watch, under the
        // control lease — theirs, or it is refused before the host.
        "BrowserViewInput" => {
            let Ok(p) = wire::BrowserViewInput::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "BrowserViewInput");
            };
            let Some(bsid) = p
                .browser_session_id
                .as_ref()
                .and_then(id16)
                .map(modbit_browser::BrowserSessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "browser_session_id required");
            };
            let Some(rec) = core.browser.get(bsid).await else {
                return reject(cid, "NO_SUCH_SESSION", bsid.to_string());
            };
            if rec.lease.controller != modbit_browser::Controller::User {
                return reject(
                    cid,
                    "AGENT_ACTIVE",
                    format!(
                        "the agent holds control of the session (lease generation {}); take control first (SetBrowserControl USER)",
                        rec.lease.generation
                    ),
                );
            }
            let Some((_, ctl)) = core.browser.view_control(bsid).await else {
                return reject(
                    cid,
                    "HOST_NOT_STREAMABLE",
                    "the session's host takes no remote input",
                );
            };
            let (tx, rx) = tokio::sync::oneshot::channel();
            let input = crate::browser_cloud::ViewInput {
                kind: p.kind.clone(),
                x: p.x,
                y: p.y,
                button: p.button.clone(),
                text: p.text.clone(),
                key: p.key.clone(),
                delta_x: p.delta_x,
                delta_y: p.delta_y,
                modifiers: p.modifiers,
            };
            if ctl
                .send(crate::browser_cloud::ViewControl::Input(input, tx))
                .await
                .is_err()
            {
                return reject(cid, "HOST_GONE", "the session's host is gone");
            }
            match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
                Ok(Ok(Ok(detail))) => accept(
                    cid,
                    false,
                    wire::BrowserViewInputDelivered {
                        delivered: true,
                        detail,
                    }
                    .encode_to_vec(),
                ),
                Ok(Ok(Err((code, why)))) => reject(cid, &code, why),
                _ => reject(
                    cid,
                    "BROWSER_TIMEOUT",
                    "the host did not apply the input in time",
                ),
            }
        }
        "CloseBrowserSession" => {
            let Ok(p) = wire::CloseBrowserSession::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "CloseBrowserSession");
            };
            let Some(bsid) = p
                .browser_session_id
                .as_ref()
                .and_then(id16)
                .map(modbit_browser::BrowserSessionId::from_bytes)
            else {
                return reject(cid, "BAD_PAYLOAD", "browser_session_id required");
            };
            let Some(rec) = core.browser.get(bsid).await else {
                return reject(cid, "NO_SUCH_SESSION", bsid.to_string());
            };
            if let Err(ack) = require_lease(core, &cid, &env, &rec.session_id).await {
                return ack;
            }
            if rec.closed {
                return accept(
                    cid,
                    true,
                    wire::BrowserSessionClosed {
                        browser_session_id: p.browser_session_id,
                        offset: 0,
                    }
                    .encode_to_vec(),
                );
            }
            // The host releases its view; whatever it answers, the session is closed here.
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                modbit_browser::BrowserPort::request(
                    core.browser.as_ref(),
                    bsid,
                    modbit_browser::HostRequest::Close,
                ),
            )
            .await;
            core.browser.close(bsid).await;
            let offset = match crate::runtime::append_batch(
                &mut *core.store.lock().await,
                core,
                crate::runtime::Lineage::task(core.tenant_id, rec.session_id, rec.task_id),
                vec![(
                    AggregateType::Task,
                    *rec.task_id.as_bytes(),
                    vec![crate::runtime::typed(
                        "BrowserSessionClosed",
                        &TaskEvent::BrowserSessionClosed {
                            browser_session_id: bsid.to_string(),
                        },
                        actor.clone(),
                    )],
                )],
            ) {
                Ok(o) => o,
                Err(e) => return reject(cid, "STORE", e),
            };
            accept(
                cid,
                false,
                wire::BrowserSessionClosed {
                    browser_session_id: p.browser_session_id,
                    offset,
                }
                .encode_to_vec(),
            )
        }
        "CancelTask" => {
            let Ok(p) = wire::CancelTask::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "CancelTask");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            let was_running = core.runtime.cancel(&task_id).await;
            if !was_running && !task.state.is_terminal() {
                // No loop alive: cancel durably here.
                let mut store = core.store.lock().await;
                if let Ok(runs) = store.runs_for_task(&task_id) {
                    for r in runs.into_iter().filter(|r| !r.state.is_terminal()) {
                        let _ = store.append(AppendRequest {
                            tenant_id: core.tenant_id,
                            session_id: task.session_id,
                            task_id: Some(task_id),
                            run_id: Some(r.run_id),
                            turn_id: None,
                            step_id: None,
                            aggregate_type: AggregateType::Run,
                            aggregate_id: *r.run_id.as_bytes(),
                            expected_sequence: None,
                            events: vec![typed(
                                "RunCancelled",
                                &modbit_domain::run::RunEvent::RunCancelled,
                                actor.clone(),
                            )],
                        });
                    }
                }
                match store.append(AppendRequest {
                    tenant_id: core.tenant_id,
                    session_id: task.session_id,
                    task_id: Some(task_id),
                    run_id: None,
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::Task,
                    aggregate_id: *task_id.as_bytes(),
                    expected_sequence: None,
                    events: vec![typed(
                        "TaskCancelled",
                        &TaskEvent::TaskCancelled,
                        actor.clone(),
                    )],
                }) {
                    Ok(ev) => {
                        if let Some(last) = ev.last() {
                            core.last_offset.send_replace(last.offset);
                        }
                    }
                    Err(e) => return reject(cid, error_code(&e), e.to_string()),
                }
            }
            if !was_running {
                crate::sandboxes::release_if_ended(core, task_id, &actor).await;
            }
            accept(
                cid,
                false,
                wire::TaskCancelRequested { was_running }.encode_to_vec(),
            )
        }
        "GetTaskStatus" => {
            let Ok(p) = wire::GetTaskStatus::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetTaskStatus");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let loop_alive = core.runtime.is_running(&task_id).await;
            let store = core.store.lock().await;
            let task = match store.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            let (state, wait_reason) = match task.state {
                modbit_domain::task::TaskState::Waiting(r) => {
                    ("Waiting".to_owned(), format!("{r:?}"))
                }
                other => (format!("{other:?}"), String::new()),
            };
            let run_state = store
                .runs_for_task(&task_id)
                .ok()
                .and_then(|r| r.into_iter().next())
                .map(|r| format!("{:?}", r.state))
                .unwrap_or_default();
            let attention = store.latest_attention(&task_id).ok().flatten();
            let attention_reason = attention
                .as_ref()
                .and_then(|a| a["reason"].as_str())
                .unwrap_or_default()
                .to_owned();
            let diagnostic = attention.as_ref().and_then(|a| {
                serde_json::from_value::<modbit_domain::failure::FailureDiagnostic>(
                    a["diagnostic"].clone(),
                )
                .ok()
            });
            let (
                failure_class,
                failure_code,
                retryable,
                user_action,
                recovery_path,
                evidence_refs,
                diagnostic_features,
            ) = match diagnostic {
                Some(d) => (
                    d.class.label().to_owned(),
                    d.code,
                    d.retryable,
                    d.user_action,
                    d.recovery_path,
                    d.evidence_refs,
                    d.features,
                ),
                None => Default::default(),
            };
            accept(
                cid,
                false,
                wire::TaskStatus {
                    state,
                    wait_reason,
                    run_state,
                    loop_alive,
                    last_offset: store.last_offset().unwrap_or(0),
                    attention_reason,
                    failure_class,
                    failure_code,
                    retryable,
                    user_action,
                    recovery_path,
                    evidence_refs,
                    diagnostic_features,
                }
                .encode_to_vec(),
            )
        }
        "GetReviewBundle" => {
            let Ok(p) = wire::GetReviewBundle::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetReviewBundle");
            };
            match crate::review::bundle(core, &p).await {
                Ok(b) => accept(cid, false, b.encode_to_vec()),
                Err((code, msg)) => reject(cid, &code, msg),
            }
        }
        "GetCodeView" => {
            let Ok(p) = wire::GetCodeView::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "GetCodeView");
            };
            match crate::review::code_view(core, &p).await {
                Ok(v) => accept(cid, false, v.encode_to_vec()),
                Err((code, msg)) => reject(cid, &code, msg),
            }
        }
        "DecideReview" => {
            let Ok(p) = wire::DecideReview::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "DecideReview");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &task.session_id).await {
                return ack;
            }
            match crate::review::decide(core, &p, actor).await {
                Ok(v) => accept(cid, false, v.encode_to_vec()),
                Err((code, msg)) => reject(cid, &code, msg),
            }
        }
        "ApplyUserPatch" => {
            let Ok(p) = wire::ApplyUserPatch::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ApplyUserPatch");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let session_id = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t.session_id,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            match crate::user_patch::apply(core, &p, command_id, record("ApplyUserPatch"), actor)
                .await
            {
                Ok(v) => {
                    let replayed = v.replayed;
                    accept(cid, replayed, v.encode_to_vec())
                }
                Err((code, msg)) => reject(cid, &code, msg),
            }
        }
        "SubmitExternalDiagnostics" => {
            let Ok(p) = wire::SubmitExternalDiagnostics::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "SubmitExternalDiagnostics");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let session_id = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t.session_id,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            match crate::external_diagnostics::submit(
                core,
                &p,
                record("SubmitExternalDiagnostics"),
                actor,
            )
            .await
            {
                Ok(v) => {
                    let replayed = v.replayed;
                    accept(cid, replayed, v.encode_to_vec())
                }
                Err((code, msg)) => reject(cid, &code, msg),
            }
        }
        "OpenPullRequest" | "UpdatePullRequest" => {
            let update = env.command_type == "UpdatePullRequest";
            let (task_id, expected, base, title, remote) = if update {
                let Ok(p) = wire::UpdatePullRequest::decode(env.payload.as_slice()) else {
                    return reject(cid, "BAD_PAYLOAD", "UpdatePullRequest");
                };
                (
                    p.task_id,
                    p.expected_candidate_revision,
                    String::new(),
                    String::new(),
                    p.remote,
                )
            } else {
                let Ok(p) = wire::OpenPullRequest::decode(env.payload.as_slice()) else {
                    return reject(cid, "BAD_PAYLOAD", "OpenPullRequest");
                };
                (
                    p.task_id,
                    p.expected_candidate_revision,
                    p.base,
                    p.title,
                    p.remote,
                )
            };
            let Some(task_id) = task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            if expected == 0 {
                return reject(
                    cid,
                    "BAD_PAYLOAD",
                    "expected_candidate_revision required: the pull request is bound to the revision the review accepted",
                );
            }
            let session_id = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t.session_id,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            match crate::pull_request::run(
                core,
                crate::pull_request::Request {
                    task_id,
                    expected_candidate_revision: expected,
                    base: &base,
                    title: &title,
                    remote: &remote,
                    actor,
                    lease_generation: env.expected_generation,
                },
                update,
            )
            .await
            {
                Ok(v) => {
                    let replayed = v.replayed;
                    accept(cid, replayed, v.encode_to_vec())
                }
                Err((code, msg)) => reject(cid, &code, msg),
            }
        }
        // PX-009: the forge's check runs for the pushed commit, as evidence
        // with provenance ci — never a verification result.
        "IngestCiResults" => {
            let Ok(p) = wire::IngestCiResults::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "IngestCiResults");
            };
            let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
                return reject(cid, "BAD_PAYLOAD", "task_id required");
            };
            let session_id = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t.session_id,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
            match crate::ci_evidence::ingest(core, task_id, actor, env.expected_generation).await {
                Ok(v) => accept(cid, false, v.encode_to_vec()),
                Err((code, msg)) => reject(cid, &code, msg),
            }
        }
        "GetRecoveryReport" => {
            let r = &core.recovery;
            accept(
                cid,
                false,
                wire::RecoveryReport {
                    boot_generation: r.boot_generation,
                    started_at: Some(prost_types::Timestamp {
                        seconds: core.started_at.millis().div_euclid(1000),
                        nanos: (core.started_at.millis().rem_euclid(1000) * 1_000_000) as i32,
                    }),
                    last_offset: r.last_offset,
                    events_verified: r.events_verified,
                    aggregates_verified: r.aggregates_verified,
                    projections_rebuilt: r.projections_rebuilt,
                    sessions: r.sessions,
                    tasks: r.tasks,
                    notes: r.notes.clone(),
                    recovery_ms: r.recovery_ms,
                }
                .encode_to_vec(),
            )
        }
        other => reject(
            cid,
            "UNSUPPORTED_COMMAND",
            format!("`{other}` is not served by this build"),
        ),
    }
}

fn tool_invoked(
    r: &modbit_tools::ToolCallResult,
    result_ref: &str,
    approval_id: Option<modbit_domain::ApprovalId>,
) -> wire::ToolInvoked {
    wire::ToolInvoked {
        tool_call_id: Some(wire_id(r.tool_call_id.as_bytes())),
        status: format!("{:?}", r.status)
            .to_uppercase()
            .replace("APPLICATIONFAILURE", "APPLICATION_FAILURE")
            .replace("INFRAFAILURE", "INFRA_FAILURE")
            .replace("UNKNOWNOUTCOME", "UNKNOWN_OUTCOME")
            .replace("POLICYDENIED", "POLICY_DENIED")
            .replace("APPROVALPENDING", "APPROVAL_PENDING")
            .replace("INVALIDARGUMENTS", "INVALID_ARGUMENTS")
            .replace("UNKNOWNTOOL", "UNKNOWN_TOOL"),
        structured_output_json: r.structured_output.to_string(),
        stdout_ref: r.stdout_ref.clone().unwrap_or_default(),
        error_code: r.error_code.clone().unwrap_or_default(),
        error_message: r.error_message.clone().unwrap_or_default(),
        workspace_revision_after: r.workspace_revision_after.unwrap_or(0),
        result_ref: result_ref.to_owned(),
        approval_id: approval_id
            .map(|a| encode_hex(a.as_bytes()))
            .unwrap_or_default(),
        effect_receipt_ids: r.effect_receipt_ids.clone(),
    }
}

/// The wire view of a task's protocol state (docs/19 layer 2, M4.1).
fn protocol_state_view(state: &modbit_protocol_state::ProtocolState) -> wire::ProtocolStateView {
    use modbit_protocol_state::CallPhase;
    wire::ProtocolStateView {
        task_id: Some(wire_id(state.task_id.as_bytes())),
        version: state.version.clone(),
        boundary: state.boundary(None).label().to_owned(),
        calls: state
            .calls
            .iter()
            .map(|c| {
                let (phase, approval_id, reason) = match &c.phase {
                    CallPhase::Proposed => ("PROPOSED", String::new(), String::new()),
                    CallPhase::AwaitingApproval { approval_id } => (
                        "AWAITING_APPROVAL",
                        encode_hex(approval_id.as_bytes()),
                        String::new(),
                    ),
                    CallPhase::InFlight => ("IN_FLIGHT", String::new(), String::new()),
                    CallPhase::UnknownOutcome { reason } => {
                        ("UNKNOWN_OUTCOME", String::new(), reason.clone())
                    }
                };
                wire::PendingCallView {
                    tool_call_id: Some(wire_id(c.tool_call_id.as_bytes())),
                    tool_name: c.tool_name.clone(),
                    effect_class: format!("{:?}", c.effect_class),
                    phase: phase.to_owned(),
                    arguments_hash: c.arguments_hash.clone(),
                    call_id: c.call_id.clone().unwrap_or_default(),
                    run_id: c.run_id.map(|r| wire_id(r.as_bytes())),
                    approval_id,
                    reason,
                }
            })
            .collect(),
        approvals: state
            .approvals
            .iter()
            .map(|a| wire::PendingApprovalView {
                approval_id: Some(wire_id(a.approval_id.as_bytes())),
                tool_call_id: Some(wire_id(a.tool_call_id.as_bytes())),
                tool_name: a.tool_name.clone(),
                effect_class: format!("{:?}", a.effect_class),
                intent_hash: a.intent_hash.clone(),
                expires_at: a.expires_at.map(|t| t.0).unwrap_or(0),
                expired: a.expired,
            })
            .collect(),
        question_id: state
            .question
            .as_ref()
            .map(|q| q.question_id.clone())
            .unwrap_or_default(),
        active_leases: u32::try_from(state.leases.len()).unwrap_or(u32::MAX),
        digest: state.digest(),
        terminals: state
            .terminals
            .iter()
            .map(|t| wire::TerminalCursorView {
                handle_id: t.handle_id.clone(),
                request_id: t.request_id.clone(),
                argv: t.argv.clone(),
                replay_generation: t.replay_generation,
                last_acknowledged_cursor: t.last_acknowledged_cursor,
                running: t.running,
                output_ref: t.output_ref.clone().unwrap_or_default(),
                exit_code: t.exit_code.unwrap_or(0),
                exit_known: t.exit_code.is_some(),
                tool_call_id: t.tool_call_id.clone(),
            })
            .collect(),
    }
}

fn approval_view(a: &modbit_domain::approval::Approval) -> wire::ApprovalView {
    wire::ApprovalView {
        approval_id: Some(wire_id(a.approval_id.as_bytes())),
        task_id: Some(wire_id(a.task_id.as_bytes())),
        tool_call_id: Some(wire_id(a.tool_call_id.as_bytes())),
        tool_name: a.tool_name.clone(),
        effect_class: format!("{:?}", a.effect_class),
        intent_hash: a.intent_hash.clone(),
        scope_json: a.scope_json.clone(),
        status: format!("{:?}", a.state).to_uppercase(),
        requested_at_ms: a.requested_at.millis(),
        expires_at_ms: a.expires_at.map(Timestamp::millis).unwrap_or(0),
        resolver: a.resolver.clone().unwrap_or_default(),
    }
}

fn receipt_view(r: &modbit_domain::toolcall::EffectReceipt) -> wire::EffectReceiptView {
    wire::EffectReceiptView {
        effect_id: Some(wire_id(r.effect_id.as_bytes())),
        previous_receipt_hash: r.previous_receipt_hash.clone().unwrap_or_default(),
        task_id: Some(wire_id(r.task_id.as_bytes())),
        tool_call_id: Some(wire_id(r.tool_call_id.as_bytes())),
        capability_lease_id: r.capability_lease_id.map(|l| wire_id(l.as_bytes())),
        intent_hash: r.intent_hash.clone(),
        policy_decision: r.policy_decision.clone(),
        approval_id: r.approval_id.map(|a| wire_id(a.as_bytes())),
        execution_target: r.execution_target.clone(),
        evidence_ref: r.evidence_ref.clone().unwrap_or_default(),
        status: r.status.clone(),
        occurred_at_ms: r.occurred_at.millis(),
        receipt_hash: r.receipt_hash.clone(),
    }
}

fn typed<E: serde::Serialize>(event_type: &str, e: &E, actor: Actor) -> NewEvent {
    let mut ev = NewEvent::new(
        event_type,
        serde_json::to_value(e).expect("serializable domain event"),
        actor,
    );
    ev.occurred_at = Some(Timestamp::now());
    ev
}

fn split(outcome: CommandOutcome) -> (Vec<StoredEvent>, bool) {
    match outcome {
        CommandOutcome::Applied(e) => (e, false),
        CommandOutcome::Replayed(e) => (e, true),
    }
}

pub(crate) fn error_code(e: &modbit_event_store::Error) -> &'static str {
    use modbit_event_store::Error::*;
    match e {
        SequenceConflict { .. } => "SEQUENCE_CONFLICT",
        IdempotencyConflict { .. } => "IDEMPOTENCY_CONFLICT",
        Projection { .. } => "INVALID_TRANSITION",
        Integrity { .. } => "INTEGRITY",
        SchemaTooNew { .. } => "SCHEMA_TOO_NEW",
        _ => "STORE_ERROR",
    }
}

/// The task's questions in order, with their answers.
fn questions_of(store: &EventStore, task_id: &TaskId) -> Vec<wire::QuestionView> {
    let events = store
        .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    let mut out: Vec<wire::QuestionView> = Vec::new();
    for e in &events {
        let p = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "UserQuestionAsked" => out.push(wire::QuestionView {
                question_id: p["question_id"].as_str().unwrap_or_default().to_owned(),
                call_id: p["call_id"].as_str().unwrap_or_default().to_owned(),
                question: p["question"].as_str().unwrap_or_default().to_owned(),
                options: p["options"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|o| wire::QuestionOptionView {
                                id: o["id"].as_str().unwrap_or_default().to_owned(),
                                label: o["label"].as_str().unwrap_or_default().to_owned(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                allow_free_text: p["allow_free_text"].as_bool().unwrap_or(false),
                reason: p["reason"].as_str().unwrap_or_default().to_owned(),
                flags: p["flags"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|f| f.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
                answered: false,
                option_id: String::new(),
                text: String::new(),
            }),
            "UserQuestionAnswered" => {
                if let Some(q) = out
                    .iter_mut()
                    .find(|q| Some(q.question_id.as_str()) == p["question_id"].as_str())
                {
                    q.answered = true;
                    q.option_id = p["option_id"].as_str().unwrap_or_default().to_owned();
                    q.text = p["text"].as_str().unwrap_or_default().to_owned();
                }
            }
            _ => {}
        }
    }
    out
}
