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
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::session::SessionEvent;
use modbit_domain::task::{InputMode, TaskEvent, TaskOrigin};
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
    store: Mutex<EventStore>,
    /// Latest committed store offset; subscribers wake on change.
    last_offset: watch::Sender<u64>,
    boot_secret: Vec<u8>,
    /// Local single-user identity for M1 (accounts arrive with the cloud plane).
    tenant_id: TenantId,
    user_id: UserId,
    /// What startup recovery did (docs/19), served to clients.
    recovery: RecoveryOutcome,
    started_at: Timestamp,
    /// Tool registry, kernel port, broker (M2.4).
    tools: crate::tools::ToolHost,
}

/// Bounded number of events per subscription batch (REQ-EV-0108).
const BATCH: usize = 256;

/// Largest object range served in one frame (REQ-EV-0108).
const MAX_OBJECT_CHUNK: u64 = 1024 * 1024;

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
pub async fn run(data_dir: PathBuf) -> Result<()> {
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
    let start = store.last_offset()?;
    let boot_secret: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
    let nonce = encode_hex(&(0..6).map(|_| rand::random::<u8>()).collect::<Vec<_>>());
    let endpoint = Endpoint::for_dir(&data_dir, &nonce).context("choosing local endpoint")?;
    let (tx, _) = watch::channel(start);
    let core = Arc::new(Core {
        store: Mutex::new(store),
        last_offset: tx,
        boot_secret: boot_secret.clone(),
        tenant_id: TenantId::from_bytes([0xA1; 16]),
        user_id: UserId::from_bytes([0xB1; 16]),
        recovery,
        started_at: Timestamp::now(),
        tools: crate::tools::ToolHost::new(&data_dir).context("tool host")?,
    });
    let listener = Listener::bind(&endpoint)
        .await
        .context("binding local endpoint")?;
    // Ready line: the only place the secret leaves the process (docs/30).
    println!(
        "{}",
        ReadyLine {
            endpoint: endpoint.clone(),
            boot_secret_hex: encode_hex(&boot_secret),
            protocol: (
                modbit_protocol::PROTOCOL_VERSION.major,
                modbit_protocol::PROTOCOL_VERSION.minor
            )
        }
        .render()
    );
    use std::io::Write;
    std::io::stdout().flush().ok();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let stream = accepted.context("accept")?;
                let core = Arc::clone(&core);
                tokio::spawn(async move {
                    if let Err(e) = serve_connection(core, stream).await {
                        eprintln!("modbit-core: connection ended: {e}");
                    }
                });
            }
            _ = &mut shutdown => break,
        }
    }
    listener.cleanup();
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn acquire_singleton_lock(data_dir: &Path) -> Result<()> {
    // docs/33 step 4: one Core per user profile. A stale lock from a dead
    // process is reclaimed; a live one refuses startup.
    let lock = data_dir.join("core.lock");
    if let Ok(text) = std::fs::read_to_string(&lock)
        && let Ok(pid) = text.trim().parse::<u32>()
        && pid != std::process::id()
        && process_alive(pid)
    {
        anyhow::bail!(
            "another modbit-core (pid {pid}) owns {}",
            data_dir.display()
        );
    }
    std::fs::write(&lock, std::process::id().to_string())?;
    Ok(())
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
        || std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
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

async fn handle_command(core: &Core, env: CommandEnvelope) -> CommandAck {
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
                            goal_text: p.goal_text,
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
                                p.execution_profile
                            },
                            policy_profile_id: None,
                            origin,
                        },
                        actor.clone(),
                    ),
                    typed("TaskQueued", &TaskEvent::TaskQueued, actor),
                ],
            };
            match store.execute_command(record("CreateTask"), req) {
                Ok(outcome) => {
                    let (events, replayed) = split(outcome);
                    let offset = events.last().map(|e| e.offset).unwrap_or(0);
                    if !replayed {
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
        "ListTools" => accept(
            cid,
            false,
            wire::ToolList {
                tools: core
                    .tools
                    .runtime
                    .registry()
                    .specs()
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
        ),
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
            // Idempotent by tool_call_id: an existing call replays its recorded result.
            let prior = {
                let store = core.store.lock().await;
                match store.tool_call(&tool_call_id) {
                    Ok(Some(existing)) => existing.result_ref.and_then(|r| {
                        let bytes = store.objects().get(&r).ok()?;
                        let prior: modbit_tools::ToolCallResult =
                            serde_json::from_slice(&bytes).ok()?;
                        Some((prior, r))
                    }),
                    _ => None,
                }
            };
            if let Some((prior, r)) = prior {
                return accept(cid, true, tool_invoked(&prior, &r).encode_to_vec());
            }
            match core
                .tools
                .invoke(
                    &core.store,
                    core.tenant_id,
                    task.session_id,
                    task_id,
                    task.workspace_root.clone(),
                    &task.execution_profile,
                    tool_call_id,
                    &p.tool_name,
                    &p.arguments_json,
                    p.output_budget_bytes,
                    actor,
                )
                .await
            {
                Ok((result, result_ref)) => {
                    let offset = core.store.lock().await.last_offset().unwrap_or(0);
                    core.last_offset.send_replace(offset);
                    accept(
                        cid,
                        false,
                        tool_invoked(&result, &result_ref).encode_to_vec(),
                    )
                }
                Err(e) => reject(cid, "TOOL_HOST", e.to_string()),
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

fn tool_invoked(r: &modbit_tools::ToolCallResult, result_ref: &str) -> wire::ToolInvoked {
    wire::ToolInvoked {
        tool_call_id: Some(wire_id(r.tool_call_id.as_bytes())),
        status: format!("{:?}", r.status)
            .to_uppercase()
            .replace("APPLICATIONFAILURE", "APPLICATION_FAILURE")
            .replace("INFRAFAILURE", "INFRA_FAILURE")
            .replace("UNKNOWNOUTCOME", "UNKNOWN_OUTCOME")
            .replace("POLICYDENIED", "POLICY_DENIED")
            .replace("INVALIDARGUMENTS", "INVALID_ARGUMENTS")
            .replace("UNKNOWNTOOL", "UNKNOWN_TOOL"),
        structured_output_json: r.structured_output.to_string(),
        stdout_ref: r.stdout_ref.clone().unwrap_or_default(),
        error_code: r.error_code.clone().unwrap_or_default(),
        error_message: r.error_message.clone().unwrap_or_default(),
        workspace_revision_after: r.workspace_revision_after.unwrap_or(0),
        result_ref: result_ref.to_owned(),
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
