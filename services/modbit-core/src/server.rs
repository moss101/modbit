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

/// Run the daemon until the listener fails or the process is signalled.
pub async fn run(data_dir: PathBuf, idle_exit_secs: Option<u64>) -> Result<()> {
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
        crate::runtime::reconcile_after_restart(&mut store, TenantId::from_bytes([0xA1; 16]));
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
    let core = Arc::new(Core {
        store: Arc::new(Mutex::new(store)),
        last_offset: tx,
        boot_secret: boot_secret.clone(),
        tenant_id: TenantId::from_bytes([0xA1; 16]),
        user_id: UserId::from_bytes([0xB1; 16]),
        recovery,
        started_at: Timestamp::now(),
        tools: crate::tools::ToolHost::new(&data_dir).context("tool host")?,
        gateway: modbit_providers::ProviderGateway::new(modbit_providers::endpoints_from_env())
            .with_policy(modbit_providers::OrgModelPolicy::from_env()),
        runtime: crate::runtime::Runtime::default(),
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
                    "GetTaskEconomics",
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
                    "UndoToolCall",
                    "AskSideQuestion",
                    "ListQuestions",
                    "RespondToQuestion",
                    "IngestAttachment",
                ]
                .map(String::from)
                .to_vec(),
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
    loop {
        // Drain any events the subscriber has not seen yet, in bounded batches.
        if let Some((session, ref mut after)) = subscription {
            loop {
                let batch = core
                    .store
                    .lock()
                    .await
                    .read_session(&session, *after, BATCH)?;
                if batch.is_empty() {
                    break;
                }
                for ev in &batch {
                    write_frame(
                        &mut stream,
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
            f = read_frame(&mut stream) => match f {
                Ok(Some(f)) => Some(f),
                Ok(None) => return Ok(()),
                Err(e @ (FrameError::TooLarge { .. } | FrameError::Malformed(_) | FrameError::EmptyBody)) => {
                    let code = if matches!(e, FrameError::TooLarge { .. }) { "FRAME_TOO_LARGE" } else { "MALFORMED_FRAME" };
                    let _ = write_frame(&mut stream, &error_frame(code, e.to_string())).await;
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            },
            changed = rx.changed(), if subscription.is_some() => {
                if changed.is_err() { return Ok(()); }
                None
            }
        };
        let Some(frame) = frame else { continue };
        match frame.body {
            Some(Body::Command(env)) => {
                let ack = handle_command(&core, env).await;
                write_frame(
                    &mut stream,
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
                        &mut stream,
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
                        &mut stream,
                        &error_frame("INVALID_CURSOR", format!("after_offset {after_offset} is beyond the log ({last}); rehydrate from a snapshot")),
                    )
                    .await?;
                    return Ok(());
                }
                subscription = Some((SessionId::from_bytes(sid), after_offset));
                rx.mark_changed();
            }
            Some(Body::ClientHello(_)) => {
                write_frame(
                    &mut stream,
                    &error_frame("DUPLICATE_HELLO", "handshake already completed"),
                )
                .await?;
                return Ok(());
            }
            other => {
                write_frame(
                    &mut stream,
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

fn wire_id(b: &[u8; 16]) -> wire::Id {
    wire::Id { value: b.to_vec() }
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
            if p.goal_text.trim().is_empty() {
                return reject(cid, "BAD_PAYLOAD", "goal_text required");
            }
            let origin = match p.origin.as_str() {
                "desktop" => TaskOrigin::Desktop,
                "cli" => TaskOrigin::Cli,
                "ide_adapter" => TaskOrigin::IdeAdapter,
                "forge_issue" => TaskOrigin::ForgeIssue,
                "forge_webhook" => TaskOrigin::ForgeWebhook,
                other => return reject(cid, "BAD_PAYLOAD", format!("unknown origin `{other}`")),
            };
            if let Err(ack) = require_lease(core, &cid, &env, &session_id).await {
                return ack;
            }
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
                events: vec![
                    typed(
                        "TaskCreated",
                        &TaskEvent::TaskCreated {
                            session_id,
                            goal_text: p.goal_text.clone(),
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
                ],
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
            let lease_generation = env.expected_generation.unwrap_or(0);
            // Model policy: request → environment defaults → first registered.
            let endpoints = core.gateway.endpoints();
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
            accept(
                cid,
                false,
                wire::TaskStatus {
                    state,
                    wait_reason,
                    run_state,
                    loop_alive,
                    last_offset: store.last_offset().unwrap_or(0),
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

fn error_code(e: &modbit_event_store::Error) -> &'static str {
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
