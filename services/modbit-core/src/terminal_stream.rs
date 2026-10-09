//! The terminal stream and the background-terminal registry for every client
//! (PX-043, PX-099; docs/21 "Durable modbit-execd", docs/65 AFW-I03/I04).
//!
//! The broker (`modbit-execd`) is the one terminal runtime: it owns the PTY,
//! the bounded replay log and the person's input lease. This module is the
//! Core's relay between it and a client of the SurfaceProtocol, and owns no
//! terminal state of its own:
//!
//! * `ListTerminals` is the broker's session list, with each session's owner
//!   task, state, start, elapsed timer and replay window.
//! * `AttachTerminal` opens one broker connection per attachment, attaches
//!   from the client's cursor with an acknowledgement window, and relays the
//!   broker's output to the client as `SurfaceFrame.terminal_frame`s. The
//!   client acknowledges what it consumed (`AckTerminal`); the Core forwards
//!   the acknowledgement, so the bytes in flight between the broker and the
//!   client never exceed the window. A client that stops acknowledging is
//!   dropped to cursor-pull (`TerminalEnded` SLOW_CONSUMER) and resumes from
//!   its own cursor; a cursor older than the replay window is
//!   `CURSOR_EXPIRED`, one beyond the head `CURSOR_BEYOND_HEAD`.
//! * A person attaching may take the terminal's input lease: the broker then
//!   refuses the agent's `shell.input` (INPUT_LEASED) and accepts keystrokes
//!   only from that attachment's connection. Taking and giving back the
//!   lease, resizing and typing are journaled on the owning task; the typed
//!   bytes themselves never are.
//! * A client names the task it believes owns the terminal; a session of
//!   another task is `SESSION_NOT_OWNED`, never served.
//!
//! A lost client or a killed Core never ends the process: the broker keeps
//! it, releases the lease with the connection, and a client reattaches from
//! its cursor.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use modbit_domain::event::AggregateType;
use modbit_domain::task::TaskEvent;
use modbit_domain::{SessionId, TaskId};
use modbit_event_store::{AppendRequest, CommandRecord};
use modbit_protocol::v1::terminal_frame::Body as FrameBody;
use modbit_protocol::v1::{self as wire, CommandAck, CommandEnvelope};
use modbit_terminal::{
    AttachOptions, Error as ExecError, Event, ExecClient, ExecReader, ExecWriter,
};
use prost::Message;
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::server::{Core, accept, error_code, id16, reject, require_lease, typed, wire_id};

/// The acknowledgement window an attach gets when it names none.
pub(crate) const DEFAULT_WINDOW_BYTES: u64 = 256 * 1024;
const MIN_WINDOW_BYTES: u64 = 4 * 1024;
const MAX_WINDOW_BYTES: u64 = 4 * 1024 * 1024;
/// Terminals one connection may watch at once.
const MAX_ATTACHMENTS: usize = 16;
/// The most keystroke bytes one `WriteTerminal` may carry.
const MAX_WRITE_BYTES: usize = 64 * 1024;

/// How long a full window waits for an acknowledgement before the
/// attachment is dropped to cursor-pull (`MODBIT_TERMINAL_STALL_MS`).
pub(crate) fn stall_ms() -> u64 {
    std::env::var("MODBIT_TERMINAL_STALL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|ms| *ms > 0)
        .unwrap_or(30_000)
}

/// What the broker answered a request that has a reply.
enum Reply {
    Lease(wire::TerminalLeaseChanged),
    Written(u64, u64),
    Resized(u32, u32),
    Refused { code: String, message: String },
}

/// What kind of answer a waiting request expects, so an unsolicited frame
/// (the lease taken from this attachment) is never mistaken for one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Lease,
    Written,
    Resized,
}

type Pending = Arc<std::sync::Mutex<VecDeque<(Kind, oneshot::Sender<Reply>)>>>;

struct Attachment {
    task_id: TaskId,
    /// The Surface session the task belongs to (the session lease's scope).
    surface_session: SessionId,
    terminal: String,
    writer: Arc<Mutex<ExecWriter>>,
    pending: Pending,
    acked: Arc<AtomicU64>,
    holds_lease: Arc<AtomicBool>,
    holder: String,
    relay: tokio::task::JoinHandle<()>,
}

/// The terminals one connection attached to, and the one bounded queue their
/// frames reach the connection through.
pub(crate) struct Terminals {
    attachments: HashMap<String, Attachment>,
    tx: mpsc::Sender<wire::TerminalFrame>,
    pub(crate) rx: mpsc::Receiver<wire::TerminalFrame>,
    /// This connection's number, to name the holder of a lease.
    connection: u64,
}

impl Terminals {
    pub(crate) fn new(connection: u64) -> Self {
        // Bounded, small: with the broker's window this is what keeps a slow
        // client from growing the Core.
        let (tx, rx) = mpsc::channel(8);
        Self {
            attachments: HashMap::new(),
            tx,
            rx,
            connection,
        }
    }

    /// The connection ended: every attachment ends with it (their broker
    /// connections close, the leases they held are released; the processes
    /// keep running).
    pub(crate) fn detach_all(&mut self) {
        for (_, a) in self.attachments.drain() {
            a.relay.abort();
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn random_id() -> String {
    modbit_protocol::local::encode_hex(&(0..8).map(|_| rand::random::<u8>()).collect::<Vec<_>>())
}

/// The task a session's owner string names, when it is a task's.
pub(crate) fn owner_task(owner: &str) -> Option<TaskId> {
    owner
        .strip_prefix("task:")
        .and_then(|t| TaskId::parse(t).ok())
}

/// One broker session as the registry lists it.
fn view_of(core: &Core, info: &modbit_terminal::SessionInfo, now: i64) -> wire::TerminalView {
    let redactor = core.tools.redactor();
    let state = if info.running {
        "RUNNING"
    } else if info.lost {
        "LOST"
    } else if info.cancelled {
        "KILLED"
    } else {
        "EXITED"
    };
    let title: String = redactor
        .error_text(&info.argv.join(" "))
        .chars()
        .take(200)
        .collect();
    let task = owner_task(&info.owner);
    wire::TerminalView {
        session_id: info.session_id.clone(),
        task_id: task.map(|t| wire_id(t.as_bytes())),
        owner: if task.is_some() { "agent" } else { "host" }.into(),
        argv: info.argv.iter().map(|a| redactor.error_text(a)).collect(),
        title,
        state: state.into(),
        exit_code: info.exit_code,
        started_at_ms: info.started_at_ms,
        elapsed_ms: if info.running {
            u64::try_from((now - info.started_at_ms).max(0)).unwrap_or(0)
        } else {
            info.duration_ms
        },
        bytes_so_far: info.bytes_so_far,
        oldest_cursor: info.oldest_cursor,
        replay_window_bytes: info.replay_window_bytes,
        pty: info.pty,
        rows: info.pty_rows,
        cols: info.pty_cols,
        input_lease_holder: info.input_lease_holder.clone(),
        output_ref: info.output_ref.clone(),
        cwd: redactor.error_text(&info.cwd),
        timed_out: info.timed_out,
    }
}

/// The broker, as a connection speaking for the host (the Core speaks for
/// the person; ownership is checked here against what the broker reports).
pub(crate) async fn broker(core: &Core) -> Result<(ExecClient, u64), (&'static str, String)> {
    let Some(execd) = &core.tools.execd else {
        return Err((
            "NO_BROKER",
            "no terminal broker is attached to this Core".into(),
        ));
    };
    let target = &execd.target;
    ExecClient::connect(&target.endpoint, &target.boot_secret)
        .await
        .map(|c| (c, target.replay_generation))
        .map_err(|e| ("BROKER_UNAVAILABLE", e.to_string()))
}

/// The broker's sessions.
pub(crate) async fn sessions_of(
    client: &mut ExecClient,
) -> Result<Vec<modbit_terminal::SessionInfo>, (&'static str, String)> {
    client
        .list()
        .await
        .map_err(|e| ("BROKER_UNAVAILABLE", e.to_string()))?;
    loop {
        match tokio::time::timeout(std::time::Duration::from_secs(10), client.next()).await {
            Ok(Ok(Some(Event::Sessions(l)))) => return Ok(l),
            Ok(Ok(Some(_))) => {}
            Ok(Ok(None)) => return Err(("BROKER_UNAVAILABLE", "the broker closed".into())),
            Ok(Err(e)) => return Err(("BROKER_UNAVAILABLE", e.to_string())),
            Err(_) => {
                return Err((
                    "BROKER_UNAVAILABLE",
                    "the broker did not answer the listing in 10s".into(),
                ));
            }
        }
    }
}

/// `ListTerminals`: the background terminals of every task (or one), the
/// same for every client.
pub(crate) async fn list_terminals(core: &Core, env: CommandEnvelope) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::ListTerminals::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "ListTerminals");
    };
    let filter = match p.task_id.as_ref() {
        None => None,
        Some(id) => match id16(id) {
            Some(b) => Some(TaskId::from_bytes(b)),
            None => return reject(cid, "BAD_PAYLOAD", "task_id must be 16 bytes"),
        },
    };
    let (mut client, _) = match broker(core).await {
        Ok(c) => c,
        Err((code, msg)) => return reject(cid, code, msg),
    };
    let sessions = match sessions_of(&mut client).await {
        Ok(s) => s,
        Err((code, msg)) => return reject(cid, code, msg),
    };
    let now = now_ms();
    let mut terminals: Vec<wire::TerminalView> = sessions
        .iter()
        .filter(|s| filter.is_none_or(|t| owner_task(&s.owner) == Some(t)))
        .map(|s| view_of(core, s, now))
        .collect();
    // Running first, then newest first: the order a tray shows them in.
    terminals.sort_by(|a, b| {
        (b.state == "RUNNING")
            .cmp(&(a.state == "RUNNING"))
            .then(b.started_at_ms.cmp(&a.started_at_ms))
    });
    accept(
        cid,
        false,
        wire::TerminalList {
            terminals,
            now_ms: now,
            default_window_bytes: DEFAULT_WINDOW_BYTES,
            stall_ms: stall_ms(),
        }
        .encode_to_vec(),
    )
}

/// Record what a person did to a terminal on its task (idempotent by the
/// command id). `Ok(offset, replayed)` or the rejection.
async fn journal(
    core: &Core,
    env: &CommandEnvelope,
    task_id: TaskId,
    surface_session: SessionId,
    event: TaskEvent,
) -> Result<(u64, bool), CommandAck> {
    let cid = env.command_id.clone();
    let Some(command_id) = env.command_id.as_ref().and_then(id16) else {
        return Err(reject(cid, "BAD_COMMAND_ID", "command_id must be 16 bytes"));
    };
    let request_hash = modbit_event_store::objects::sha256_hex(
        &[env.command_type.as_bytes(), b"\0", &env.payload].concat(),
    );
    let record = CommandRecord {
        command_id: modbit_domain::EventId::from_bytes(command_id),
        tenant_id: core.tenant_id,
        command_type: env.command_type.clone(),
        request_hash,
    };
    let req = AppendRequest {
        tenant_id: core.tenant_id,
        session_id: surface_session,
        task_id: Some(task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *task_id.as_bytes(),
        expected_sequence: None,
        events: vec![typed(
            "TerminalControlRecorded",
            &event,
            modbit_domain::event::Actor::User(core.user_id),
        )],
    };
    let mut store = core.store.lock().await;
    match store.execute_command(record, req) {
        Ok(outcome) => {
            let (events, replayed) = match outcome {
                modbit_event_store::CommandOutcome::Applied(e) => (e, false),
                modbit_event_store::CommandOutcome::Replayed(e) => (e, true),
            };
            let offset = events.last().map_or(0, |e| e.offset);
            if !replayed {
                core.last_offset.send_replace(offset);
            }
            Ok((offset, replayed))
        }
        Err(e) => Err(reject(cid, error_code(&e), e.to_string())),
    }
}

fn control(handle: &str, kind: &str, holder: &str, rows: u32, cols: u32, bytes: u64) -> TaskEvent {
    TaskEvent::TerminalControlRecorded {
        handle_id: handle.to_owned(),
        kind: kind.to_owned(),
        holder: holder.to_owned(),
        rows,
        cols,
        bytes,
    }
}

/// `AttachTerminal`.
pub(crate) async fn attach_terminal(
    core: &Core,
    env: CommandEnvelope,
    terms: &mut Terminals,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::AttachTerminal::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "AttachTerminal");
    };
    let Some(task_id) = p.task_id.as_ref().and_then(id16).map(TaskId::from_bytes) else {
        return reject(cid, "BAD_PAYLOAD", "task_id required");
    };
    if p.session_id.is_empty() {
        return reject(cid, "BAD_PAYLOAD", "session_id required");
    }
    if terms.attachments.len() >= MAX_ATTACHMENTS {
        return reject(
            cid,
            "TOO_MANY_ATTACHMENTS",
            format!("a connection may watch at most {MAX_ATTACHMENTS} terminals"),
        );
    }
    let surface_session = {
        let store = core.store.lock().await;
        match store.task(&task_id) {
            Ok(Some(t)) => t.session_id,
            Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
            Err(e) => return reject(cid, error_code(&e), e.to_string()),
        }
    };
    // Taking the input lease changes what the terminal does: it needs the
    // session lease like any mutating command.
    if p.take_input_lease
        && let Err(ack) = require_lease(core, &cid, &env, &surface_session).await
    {
        return ack;
    }
    let (mut client, generation) = match broker(core).await {
        Ok(c) => c,
        Err((code, msg)) => return reject(cid, code, msg),
    };
    let sessions = match sessions_of(&mut client).await {
        Ok(s) => s,
        Err((code, msg)) => return reject(cid, code, msg),
    };
    let Some(info) = sessions.into_iter().find(|s| s.session_id == p.session_id) else {
        return reject(cid, "UNKNOWN_SESSION", p.session_id);
    };
    // Ownership: the task the client names must own the terminal.
    if owner_task(&info.owner) != Some(task_id) {
        return reject(
            cid,
            "SESSION_NOT_OWNED",
            format!(
                "terminal {} does not belong to task {task_id}",
                info.session_id
            ),
        );
    }
    // The typed range errors, answered here so the client learns it from the
    // command's own answer, never by a silent truncation.
    if p.after_cursor > info.bytes_so_far {
        return reject(
            cid,
            "CURSOR_BEYOND_HEAD",
            format!(
                "cursor {} is beyond the output head; head={}",
                p.after_cursor, info.bytes_so_far
            ),
        );
    }
    if p.after_cursor < info.oldest_cursor {
        return reject(
            cid,
            "CURSOR_EXPIRED",
            format!(
                "cursor {} is older than the replay window; oldest_cursor={}",
                p.after_cursor, info.oldest_cursor
            ),
        );
    }
    let window = if p.window_bytes == 0 {
        DEFAULT_WINDOW_BYTES
    } else {
        p.window_bytes.clamp(MIN_WINDOW_BYTES, MAX_WINDOW_BYTES)
    };
    let holder = format!("user:{}-{}", terms.connection, &random_id()[..6]);
    let mut holds = false;
    let mut input_journaled = None;
    if p.take_input_lease {
        if !info.running {
            return reject(cid, "SESSION_FINISHED", "the terminal's process has ended");
        }
        if let Err(e) = client
            .acquire_lease(&info.session_id, &holder, p.steal_input_lease)
            .await
        {
            return reject(cid, "BROKER_UNAVAILABLE", e.to_string());
        }
        loop {
            match client.next().await {
                Ok(Some(Event::Lease(l))) if l.held => {
                    holds = true;
                    break;
                }
                Ok(Some(_)) => {}
                Ok(None) => return reject(cid, "BROKER_UNAVAILABLE", "the broker closed"),
                Err(ExecError::Exec { code, message }) => return reject(cid, &code, message),
                Err(e) => return reject(cid, "BROKER_UNAVAILABLE", e.to_string()),
            }
        }
        match journal(
            core,
            &env,
            task_id,
            surface_session,
            control(&info.session_id, "INPUT_LEASE_TAKEN", &holder, 0, 0, 0),
        )
        .await
        {
            // The connection (and with it the lease) ends here.
            Err(ack) => return ack,
            Ok((offset, _)) => input_journaled = Some(offset),
        }
    }
    let stall = stall_ms();
    if let Err(e) = client
        .attach_with(
            &info.session_id,
            p.after_cursor,
            AttachOptions {
                generation,
                window_bytes: window,
                stall_ms: stall,
                strict_cursor: true,
            },
        )
        .await
    {
        return reject(cid, "BROKER_UNAVAILABLE", e.to_string());
    }
    let (reader, writer) = client.into_split();
    let attach_id = random_id();
    let pending: Pending = Arc::default();
    let acked = Arc::new(AtomicU64::new(p.after_cursor));
    let holds_lease = Arc::new(AtomicBool::new(holds));
    let relay = tokio::spawn(relay_output(
        attach_id.clone(),
        info.session_id.clone(),
        reader,
        terms.tx.clone(),
        Arc::clone(&pending),
        Arc::clone(&holds_lease),
    ));
    terms.attachments.insert(
        attach_id.clone(),
        Attachment {
            task_id,
            surface_session,
            terminal: info.session_id.clone(),
            writer: Arc::new(Mutex::new(writer)),
            pending,
            acked,
            holds_lease,
            holder,
            relay,
        },
    );
    let mut terminal = view_of(core, &info, now_ms());
    if holds {
        terminal.input_lease_holder = terms.attachments[&attach_id].holder.clone();
    }
    let _ = input_journaled;
    accept(
        cid,
        false,
        wire::TerminalAttached {
            attach_id,
            terminal: Some(terminal),
            after_cursor: p.after_cursor,
            window_bytes: window,
            stall_ms: stall,
            input_lease_held: holds,
        }
        .encode_to_vec(),
    )
}

/// The broker codes that end an attachment (as opposed to answering a
/// request made on it).
fn ends_attachment(code: &str) -> bool {
    matches!(
        code,
        "CURSOR_EXPIRED"
            | "ATTACH_STALLED"
            | "STALE_GENERATION"
            | "CURSOR_BEYOND_HEAD"
            | "UNKNOWN_SESSION"
            | "SESSION_NOT_OWNED"
    )
}

/// `key=<n>` from a broker message.
fn number_after(message: &str, key: &str) -> u64 {
    message
        .split(key)
        .nth(1)
        .and_then(|rest| {
            rest.trim_start_matches('=')
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or(0)
}

/// Hand an answer to the request waiting for it (`kind`; any kind for a
/// refusal); the answer comes back when nothing was waiting.
fn deliver(pending: &Pending, kind: Option<Kind>, reply: Reply) -> Result<(), Reply> {
    let waiter = {
        let mut q = pending.lock().expect("pending");
        match q.front() {
            Some((k, _)) if kind.is_none_or(|want| want == *k) => q.pop_front(),
            _ => None,
        }
    };
    match waiter {
        Some((_, w)) => w.send(reply),
        None => Err(reply),
    }
}

/// Relay one attachment's broker frames to the connection's queue until the
/// process exits, the attachment is ended, or the connection goes away. Each
/// send waits for room: with the broker's window, that is the whole bound.
async fn relay_output(
    attach_id: String,
    session_id: String,
    mut reader: ExecReader,
    tx: mpsc::Sender<wire::TerminalFrame>,
    pending: Pending,
    holds_lease: Arc<AtomicBool>,
) {
    let frame = |body: FrameBody| wire::TerminalFrame {
        attach_id: attach_id.clone(),
        session_id: session_id.clone(),
        body: Some(body),
    };
    let ended = |reason: &str, message: String, oldest: u64, resume: u64| {
        frame(FrameBody::Ended(wire::TerminalEnded {
            reason: reason.into(),
            message,
            oldest_cursor: oldest,
            resume_cursor: resume,
        }))
    };
    loop {
        let out = match reader.next().await {
            Ok(Some(Event::Output(o))) => Some(frame(FrameBody::Output(wire::TerminalOutput {
                cursor: o.cursor,
                stream: o.stream,
                data: o.data,
            }))),
            Ok(Some(Event::Exited(x))) => {
                let _ = tx
                    .send(frame(FrameBody::Exited(wire::TerminalExit {
                        exit_code: x.exit_code,
                        signal: x.signal,
                        duration_ms: x.duration_ms,
                        output_ref: x.output_ref,
                        total_bytes: x.total_bytes,
                        timed_out: x.timed_out,
                        cancelled: x.cancelled,
                        retained_from: x.retained_from,
                    })))
                    .await;
                return;
            }
            Ok(Some(Event::Lease(l))) => {
                let changed = wire::TerminalLeaseChanged {
                    held: l.held,
                    holder: l.holder,
                };
                holds_lease.store(l.held, Ordering::SeqCst);
                // A reply to our own request, or the lease taken from us.
                match deliver(&pending, Some(Kind::Lease), Reply::Lease(changed)) {
                    Ok(()) => None,
                    Err(Reply::Lease(c)) => Some(frame(FrameBody::Lease(c))),
                    Err(_) => None,
                }
            }
            Ok(Some(Event::StdinWritten(w))) => {
                let _ = deliver(
                    &pending,
                    Some(Kind::Written),
                    Reply::Written(w.bytes, w.cursor),
                );
                None
            }
            Ok(Some(Event::Resized(r))) => {
                let _ = deliver(
                    &pending,
                    Some(Kind::Resized),
                    Reply::Resized(r.rows, r.cols),
                );
                None
            }
            Ok(Some(_)) => None,
            Ok(None) => {
                let _ = tx
                    .send(ended(
                        "BROKER_LOST",
                        "the terminal broker closed the attachment".into(),
                        0,
                        0,
                    ))
                    .await;
                return;
            }
            Err(ExecError::Exec { code, message }) if ends_attachment(&code) => {
                let resume = number_after(&message, "resume_cursor");
                let oldest = number_after(&message, "oldest_cursor");
                let reason = match code.as_str() {
                    "ATTACH_STALLED" => "SLOW_CONSUMER",
                    other => other,
                };
                let _ = tx.send(ended(reason, message, oldest, resume)).await;
                return;
            }
            Err(ExecError::Exec { code, message }) => {
                // Refusal of a request made on this attachment.
                match deliver(
                    &pending,
                    None,
                    Reply::Refused {
                        code: code.clone(),
                        message: message.clone(),
                    },
                ) {
                    Ok(()) => None,
                    Err(_) => {
                        let _ = tx
                            .send(ended("BROKER_ERROR", format!("{code}: {message}"), 0, 0))
                            .await;
                        return;
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(ended("BROKER_LOST", e.to_string(), 0, 0)).await;
                return;
            }
        };
        if let Some(f) = out
            && tx.send(f).await.is_err()
        {
            return;
        }
    }
}

fn attachment<'a>(
    terms: &'a Terminals,
    cid: &Option<wire::Id>,
    attach_id: &str,
) -> Result<&'a Attachment, CommandAck> {
    terms.attachments.get(attach_id).ok_or_else(|| {
        reject(
            cid.clone(),
            "NO_SUCH_ATTACHMENT",
            "this connection has no such terminal attachment (it ended, or was never made here)",
        )
    })
}

/// Ask the broker something on an attachment and wait for its answer.
async fn ask(
    a: &Attachment,
    kind: Kind,
    send: impl std::future::Future<Output = Result<(), ExecError>>,
) -> Result<Reply, (String, String)> {
    let (tx, rx) = oneshot::channel();
    a.pending.lock().expect("pending").push_back((kind, tx));
    if let Err(e) = send.await {
        a.pending.lock().expect("pending").pop_back();
        return Err(("BROKER_UNAVAILABLE".into(), e.to_string()));
    }
    match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
        Ok(Ok(r)) => Ok(r),
        Ok(Err(_)) => Err((
            "BROKER_UNAVAILABLE".into(),
            "the attachment ended before the broker answered".into(),
        )),
        Err(_) => Err((
            "BROKER_UNAVAILABLE".into(),
            "the broker did not answer within 10s".into(),
        )),
    }
}

/// `AckTerminal`: the client consumed output up to a cursor.
pub(crate) async fn ack_terminal(env: CommandEnvelope, terms: &Terminals) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::AckTerminal::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "AckTerminal");
    };
    let a = match attachment(terms, &cid, &p.attach_id) {
        Ok(a) => a,
        Err(ack) => return ack,
    };
    a.acked.fetch_max(p.cursor, Ordering::SeqCst);
    if let Err(e) = a.writer.lock().await.ack(&a.terminal, p.cursor).await {
        return reject(cid, "BROKER_UNAVAILABLE", e.to_string());
    }
    accept(
        cid,
        false,
        wire::TerminalAcked {
            acked_cursor: a.acked.load(Ordering::SeqCst),
        }
        .encode_to_vec(),
    )
}

/// `DetachTerminal`: end the attachment (the process is untouched; a person's
/// input lease goes with it).
pub(crate) async fn detach_terminal(
    core: &Core,
    env: CommandEnvelope,
    terms: &mut Terminals,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::DetachTerminal::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "DetachTerminal");
    };
    let Some(a) = terms.attachments.remove(&p.attach_id) else {
        return accept(
            cid,
            false,
            wire::TerminalDetached {
                was_attached: false,
                acked_cursor: 0,
            }
            .encode_to_vec(),
        );
    };
    let held = a.holds_lease.load(Ordering::SeqCst);
    a.relay.abort();
    // Ending the broker connection releases the lease; the task's log says so.
    if held {
        let _ = journal(
            core,
            &env,
            a.task_id,
            a.surface_session,
            control(&a.terminal, "INPUT_LEASE_RELEASED", &a.holder, 0, 0, 0),
        )
        .await;
    }
    accept(
        cid,
        false,
        wire::TerminalDetached {
            was_attached: true,
            acked_cursor: a.acked.load(Ordering::SeqCst),
        }
        .encode_to_vec(),
    )
}

/// `SetTerminalInput`: take or give back the input lease of an attachment.
pub(crate) async fn set_terminal_input(
    core: &Core,
    env: CommandEnvelope,
    terms: &Terminals,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::SetTerminalInput::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "SetTerminalInput");
    };
    let a = match attachment(terms, &cid, &p.attach_id) {
        Ok(a) => a,
        Err(ack) => return ack,
    };
    if let Err(ack) = require_lease(core, &cid, &env, &a.surface_session).await {
        return ack;
    }
    let reply = if p.hold {
        ask(a, Kind::Lease, async {
            a.writer
                .lock()
                .await
                .acquire_lease(&a.terminal, &a.holder, p.steal)
                .await
        })
        .await
    } else {
        ask(a, Kind::Lease, async {
            a.writer.lock().await.release_lease(&a.terminal).await
        })
        .await
    };
    let lease = match reply {
        Ok(Reply::Lease(l)) => l,
        Ok(Reply::Refused { code, message }) => return reject(cid, &code, message),
        Ok(_) => return reject(cid, "BROKER_ERROR", "unexpected answer"),
        Err((code, message)) => return reject(cid, &code, message),
    };
    a.holds_lease.store(lease.held, Ordering::SeqCst);
    let kind = if p.hold {
        "INPUT_LEASE_TAKEN"
    } else {
        "INPUT_LEASE_RELEASED"
    };
    let offset = match journal(
        core,
        &env,
        a.task_id,
        a.surface_session,
        control(&a.terminal, kind, &a.holder, 0, 0, 0),
    )
    .await
    {
        Ok((offset, _)) => offset,
        Err(ack) => return ack,
    };
    accept(
        cid,
        false,
        wire::TerminalInputSet {
            held: lease.held,
            holder: lease.holder,
            offset,
        }
        .encode_to_vec(),
    )
}

/// `ResizeTerminal`: the viewer's size becomes the terminal's.
pub(crate) async fn resize_terminal(
    core: &Core,
    env: CommandEnvelope,
    terms: &Terminals,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::ResizeTerminal::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "ResizeTerminal");
    };
    let a = match attachment(terms, &cid, &p.attach_id) {
        Ok(a) => a,
        Err(ack) => return ack,
    };
    if let Err(ack) = require_lease(core, &cid, &env, &a.surface_session).await {
        return ack;
    }
    let reply = ask(a, Kind::Resized, async {
        a.writer
            .lock()
            .await
            .resize(&a.terminal, p.rows, p.cols)
            .await
    })
    .await;
    let (rows, cols) = match reply {
        Ok(Reply::Resized(r, c)) => (r, c),
        Ok(Reply::Refused { code, message }) => return reject(cid, &code, message),
        Ok(_) => return reject(cid, "BROKER_ERROR", "unexpected answer"),
        Err((code, message)) => return reject(cid, &code, message),
    };
    let offset = match journal(
        core,
        &env,
        a.task_id,
        a.surface_session,
        control(&a.terminal, "RESIZED", "", rows, cols, 0),
    )
    .await
    {
        Ok((offset, _)) => offset,
        Err(ack) => return ack,
    };
    accept(
        cid,
        false,
        wire::TerminalResizeDone { rows, cols, offset }.encode_to_vec(),
    )
}

/// `WriteTerminal`: a person's keystrokes, only while the attachment holds
/// the input lease. Journaled by length, never by content.
pub(crate) async fn write_terminal(
    core: &Core,
    env: CommandEnvelope,
    terms: &Terminals,
) -> CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::WriteTerminal::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "WriteTerminal");
    };
    let a = match attachment(terms, &cid, &p.attach_id) {
        Ok(a) => a,
        Err(ack) => return ack,
    };
    if p.data.is_empty() || p.data.len() > MAX_WRITE_BYTES {
        return reject(
            cid,
            "INPUT_TOO_LARGE",
            format!("a write carries 1..={MAX_WRITE_BYTES} bytes; nothing was written"),
        );
    }
    if let Err(ack) = require_lease(core, &cid, &env, &a.surface_session).await {
        return ack;
    }
    if !a.holds_lease.load(Ordering::SeqCst) {
        return reject(
            cid,
            "LEASE_REQUIRED",
            "this attachment does not hold the terminal's input lease; take it first",
        );
    }
    let reply = ask(a, Kind::Written, async {
        a.writer
            .lock()
            .await
            .write_user_input(&a.terminal, &p.data)
            .await
    })
    .await;
    let (bytes, cursor) = match reply {
        Ok(Reply::Written(b, c)) => (b, c),
        Ok(Reply::Refused { code, message }) => return reject(cid, &code, message),
        Ok(_) => return reject(cid, "BROKER_ERROR", "unexpected answer"),
        Err((code, message)) => return reject(cid, &code, message),
    };
    let offset = match journal(
        core,
        &env,
        a.task_id,
        a.surface_session,
        control(&a.terminal, "INPUT_WRITTEN", &a.holder, 0, 0, bytes),
    )
    .await
    {
        Ok((offset, _)) => offset,
        Err(ack) => return ack,
    };
    accept(
        cid,
        false,
        wire::TerminalWritten {
            bytes,
            cursor,
            offset,
        }
        .encode_to_vec(),
    )
}
