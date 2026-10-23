//! `modbit-terminal` — exec client, streams and OutputRef handling (docs/21):
//! the Core-side client of `modbit-execd`. It sends structured `ExecRequest`s,
//! consumes cursor-ordered output, attaches to durable sessions from a cursor
//! (with an acknowledgement window, PX-099), writes stdin, resizes the PTY,
//! takes the input lease and cancels. It never runs a process itself.
//!
//! Canonical owner: execution (`docs/12`).

#![forbid(unsafe_code)]

use modbit_protocol::client::{BoxedStream, connect_raw};
use modbit_protocol::framing::{FrameError, read_message, write_message};
use modbit_protocol::local::Endpoint;
use modbit_protocol::v1::exec_frame::Body;
use modbit_protocol::v1::{Auth, ClientHello, ClientKind, ExecFrame, ExecRequest, Hello};

pub use modbit_protocol::v1::{
    ExecError, ExecStarted, OutputChunk, ProcessExited, SessionInfo, StdinWritten,
    TerminalLeaseState, TerminalResized,
};

/// Client errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Framing / transport.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// Handshake refused.
    #[error("execd refused: {0}")]
    Refused(String),
    /// The broker reported an error for a request.
    #[error("execd error {code}: {message}")]
    Exec {
        /// Code.
        code: String,
        /// Message.
        message: String,
    },
    /// Unexpected frame.
    #[error("unexpected execd frame: {0}")]
    Unexpected(String),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Something the broker sent after a request.
#[derive(Debug)]
pub enum Event {
    /// Process started (or replayed).
    Started(ExecStarted),
    /// Output slice.
    Output(OutputChunk),
    /// Process ended.
    Exited(ProcessExited),
    /// Session listing.
    Sessions(Vec<SessionInfo>),
    /// EPR-018: the host's review sandbox, or none.
    SandboxProbed(modbit_protocol::v1::SandboxProbed),
    /// PX-099: the PTY was resized.
    Resized(TerminalResized),
    /// PX-099: the state of the terminal's input lease after a request.
    Lease(TerminalLeaseState),
    /// PX-099: a stdin write that asked for an answer was applied.
    StdinWritten(StdinWritten),
    /// PX-132: the listening sockets of the sessions' process trees.
    Listeners(modbit_protocol::v1::SessionListeners),
}

/// How a windowed attachment is set up (PX-099).
#[derive(Clone, Copy, Debug, Default)]
pub struct AttachOptions {
    /// The terminal replay generation (0 = unfenced).
    pub generation: u64,
    /// Bytes the broker may send beyond the last acknowledgement; 0 = no
    /// window (the caller reads at its own pace).
    pub window_bytes: u64,
    /// How long a full window waits for an acknowledgement; 0 = the broker's
    /// default.
    pub stall_ms: u64,
    /// Refuse a cursor beyond the session's head (`CURSOR_BEYOND_HEAD`).
    pub strict_cursor: bool,
}

fn attach_body(principal: &str, session_id: &str, after_cursor: u64, o: AttachOptions) -> Body {
    Body::Attach(modbit_protocol::v1::Attach {
        session_id: session_id.into(),
        after_cursor,
        generation: o.generation,
        requester: principal.into(),
        window_bytes: o.window_bytes,
        stall_ms: o.stall_ms,
        strict_cursor: o.strict_cursor,
    })
}

fn stdin_body(
    principal: &str,
    session_id: &str,
    data: &[u8],
    want_ack: bool,
    as_user: bool,
) -> Body {
    Body::Stdin(modbit_protocol::v1::WriteStdin {
        session_id: session_id.into(),
        data: data.to_vec(),
        requester: principal.into(),
        want_ack,
        as_user,
    })
}

fn ack_body(session_id: &str, cursor: u64) -> Body {
    Body::Ack(modbit_protocol::v1::TerminalAck {
        session_id: session_id.into(),
        cursor,
    })
}

fn resize_body(principal: &str, session_id: &str, rows: u32, cols: u32) -> Body {
    Body::Resize(modbit_protocol::v1::TerminalResize {
        session_id: session_id.into(),
        rows,
        cols,
        requester: principal.into(),
    })
}

fn acquire_body(principal: &str, session_id: &str, holder: &str, steal: bool) -> Body {
    Body::AcquireLease(modbit_protocol::v1::AcquireTerminalLease {
        session_id: session_id.into(),
        requester: principal.into(),
        holder: holder.into(),
        steal,
    })
}

fn release_body(principal: &str, session_id: &str) -> Body {
    Body::ReleaseLease(modbit_protocol::v1::ReleaseTerminalLease {
        session_id: session_id.into(),
        requester: principal.into(),
    })
}

/// Turn one broker frame into what the caller sees; `None` for a frame that
/// is only protocol housekeeping.
fn event_of(frame: ExecFrame) -> Option<Result<Event>> {
    Some(match frame.body {
        None => Err(Error::Unexpected("empty frame".into())),
        Some(Body::HelloAck(_)) => return None,
        Some(Body::Started(s)) => Ok(Event::Started(s)),
        Some(Body::Output(o)) => Ok(Event::Output(o)),
        Some(Body::Exited(e)) => Ok(Event::Exited(e)),
        Some(Body::Sessions(l)) => Ok(Event::Sessions(l.sessions)),
        Some(Body::SandboxProbed(p)) => Ok(Event::SandboxProbed(p)),
        Some(Body::Resized(r)) => Ok(Event::Resized(r)),
        Some(Body::LeaseState(l)) => Ok(Event::Lease(l)),
        Some(Body::StdinWritten(w)) => Ok(Event::StdinWritten(w)),
        Some(Body::ServiceListeners(l)) => Ok(Event::Listeners(l)),
        Some(Body::Error(e)) => Err(Error::Exec {
            code: e.code,
            message: e.message,
        }),
        Some(other) => Err(Error::Unexpected(format!("{other:?}"))),
    })
}

/// An authenticated connection to `modbit-execd`.
pub struct ExecClient {
    stream: BoxedStream,
    /// Who this connection speaks for (FIX-20): empty = the host (the Core
    /// itself, or the user's own terminal lease); `task:<id>` = a task, which
    /// the broker lets reach only the sessions that task owns.
    principal: String,
}

impl std::fmt::Debug for ExecClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExecClient")
    }
}

impl ExecClient {
    /// Connect and authenticate.
    pub async fn connect(endpoint: &Endpoint, boot_secret: &[u8]) -> Result<Self> {
        let mut stream = connect_raw(endpoint).await.map_err(FrameError::Io)?;
        let hello = ExecFrame {
            body: Some(Body::ClientHello(ClientHello {
                hello: Some(Hello {
                    protocol_version: Some(modbit_protocol::PROTOCOL_VERSION),
                    client_kind: ClientKind::CloudWorker as i32,
                    client_build: env!("CARGO_PKG_VERSION").into(),
                    supported_command_types: vec![],
                }),
                auth: Some(Auth {
                    boot_secret: boot_secret.to_vec(),
                }),
            })),
        };
        write_message(&mut stream, &hello).await?;
        match read_message::<_, ExecFrame>(&mut stream).await? {
            Some(ExecFrame {
                body: Some(Body::HelloAck(a)),
            }) if a.compatible => Ok(Self {
                stream,
                principal: String::new(),
            }),
            Some(ExecFrame {
                body: Some(Body::HelloAck(a)),
            }) => Err(Error::Refused(a.reason)),
            Some(ExecFrame {
                body: Some(Body::Error(e)),
            }) => Err(Error::Exec {
                code: e.code,
                message: e.message,
            }),
            other => Err(Error::Unexpected(format!("{other:?}"))),
        }
    }

    /// Speak for `principal` from now on (FIX-20): every session this
    /// connection starts is owned by it, and every attach, stdin write,
    /// cancel and listing carries it, so the broker refuses what is not its.
    /// A connection that never calls this speaks for the host.
    #[must_use]
    pub fn act_as(mut self, principal: impl Into<String>) -> Self {
        self.principal = principal.into();
        self
    }

    /// The principal a task's tool calls speak as.
    #[must_use]
    pub fn task_principal(task_id: impl std::fmt::Display) -> String {
        format!("task:{task_id}")
    }

    async fn send(&mut self, body: Body) -> Result<()> {
        write_message(&mut self.stream, &ExecFrame { body: Some(body) }).await?;
        Ok(())
    }

    /// Submit a structured request.
    pub async fn exec(&mut self, mut req: ExecRequest) -> Result<()> {
        if req.owner.is_empty() {
            req.owner = self.principal.clone();
        }
        self.send(Body::Exec(req)).await
    }

    /// Attach to a session from a cursor.
    pub async fn attach(&mut self, session_id: &str, after_cursor: u64) -> Result<()> {
        self.attach_fenced(session_id, after_cursor, 0).await
    }

    /// Attach under a terminal replay generation (docs/13, M4.5): an older
    /// generation than the session's current one is refused
    /// `STALE_GENERATION`; a newer one ends every older attachment.
    pub async fn attach_fenced(
        &mut self,
        session_id: &str,
        after_cursor: u64,
        generation: u64,
    ) -> Result<()> {
        self.attach_with(
            session_id,
            after_cursor,
            AttachOptions {
                generation,
                ..AttachOptions::default()
            },
        )
        .await
    }

    /// Attach with a flow-control window and a strict cursor (PX-099).
    pub async fn attach_with(
        &mut self,
        session_id: &str,
        after_cursor: u64,
        options: AttachOptions,
    ) -> Result<()> {
        let body = attach_body(&self.principal, session_id, after_cursor, options);
        self.send(body).await
    }

    /// Write stdin bytes.
    pub async fn write_stdin(&mut self, session_id: &str, data: &[u8]) -> Result<()> {
        let body = stdin_body(&self.principal, session_id, data, false, false);
        self.send(body).await
    }

    /// Write stdin bytes and have the broker answer `StdinWritten` (or the
    /// reason it refused): the agent's `shell.input` (PX-099).
    pub async fn write_stdin_acked(&mut self, session_id: &str, data: &[u8]) -> Result<()> {
        let body = stdin_body(&self.principal, session_id, data, true, false);
        self.send(body).await
    }

    /// Acknowledge the output consumed up to `cursor` (PX-099).
    pub async fn ack(&mut self, session_id: &str, cursor: u64) -> Result<()> {
        self.send(ack_body(session_id, cursor)).await
    }

    /// Change the PTY's size (PX-099).
    pub async fn resize(&mut self, session_id: &str, rows: u32, cols: u32) -> Result<()> {
        let body = resize_body(&self.principal, session_id, rows, cols);
        self.send(body).await
    }

    /// Cancel (kill) a session's process.
    pub async fn cancel(&mut self, session_id: &str) -> Result<()> {
        self.send(Body::Cancel(modbit_protocol::v1::Cancel {
            session_id: session_id.into(),
            requester: self.principal.clone(),
        }))
        .await
    }

    /// List sessions.
    pub async fn list(&mut self) -> Result<()> {
        self.send(Body::List(modbit_protocol::v1::ListSessions {
            requester: self.principal.clone(),
        }))
        .await
    }

    /// PX-132: ask for the listening sockets of the process trees of the
    /// running sessions this connection may see.
    pub async fn list_services(&mut self) -> Result<()> {
        self.send(Body::ListServices(modbit_protocol::v1::ListServices {
            requester: self.principal.clone(),
        }))
        .await
    }

    /// EPR-018: ask whether the broker's host confines review processes.
    pub async fn probe_sandbox(&mut self) -> Result<()> {
        self.send(Body::ProbeSandbox(modbit_protocol::v1::ProbeSandbox {}))
            .await
    }

    /// Next event; `None` when the broker closes.
    pub async fn next(&mut self) -> Result<Option<Event>> {
        loop {
            let Some(frame) = read_message::<_, ExecFrame>(&mut self.stream).await? else {
                return Ok(None);
            };
            if let Some(event) = event_of(frame) {
                return event.map(Some);
            }
        }
    }

    /// Split the connection into an independent reader and writer (PX-043):
    /// a relay that reads the broker's frames while it sends acknowledgements
    /// cannot share one `&mut` stream, and cancelling a half-read frame in a
    /// `select!` would corrupt it. The reader is the only reader.
    #[must_use]
    pub fn into_split(self) -> (ExecReader, ExecWriter) {
        let (r, w) = tokio::io::split(self.stream);
        (
            ExecReader { stream: r },
            ExecWriter {
                stream: w,
                principal: self.principal,
            },
        )
    }
}

/// The read half of a split [`ExecClient`].
pub struct ExecReader {
    stream: tokio::io::ReadHalf<BoxedStream>,
}

impl std::fmt::Debug for ExecReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExecReader")
    }
}

impl ExecReader {
    /// Next event; `None` when the broker closes. Not cancel-safe: run it
    /// to completion in a task of its own that forwards to a channel.
    pub async fn next(&mut self) -> Result<Option<Event>> {
        loop {
            let Some(frame) = read_message::<_, ExecFrame>(&mut self.stream).await? else {
                return Ok(None);
            };
            if let Some(event) = event_of(frame) {
                return event.map(Some);
            }
        }
    }
}

/// The write half of a split [`ExecClient`].
pub struct ExecWriter {
    stream: tokio::io::WriteHalf<BoxedStream>,
    principal: String,
}

impl std::fmt::Debug for ExecWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExecWriter")
    }
}

impl ExecWriter {
    async fn send(&mut self, body: Body) -> Result<()> {
        write_message(&mut self.stream, &ExecFrame { body: Some(body) }).await?;
        Ok(())
    }

    /// Attach with a flow-control window and a strict cursor.
    pub async fn attach_with(
        &mut self,
        session_id: &str,
        after_cursor: u64,
        options: AttachOptions,
    ) -> Result<()> {
        let body = attach_body(&self.principal, session_id, after_cursor, options);
        self.send(body).await
    }

    /// Acknowledge the output consumed up to `cursor`.
    pub async fn ack(&mut self, session_id: &str, cursor: u64) -> Result<()> {
        self.send(ack_body(session_id, cursor)).await
    }

    /// A person's keystrokes (needs the input lease on this connection); the
    /// broker answers `StdinWritten` or the reason it refused.
    pub async fn write_user_input(&mut self, session_id: &str, data: &[u8]) -> Result<()> {
        let body = stdin_body(&self.principal, session_id, data, true, true);
        self.send(body).await
    }

    /// Change the PTY's size.
    pub async fn resize(&mut self, session_id: &str, rows: u32, cols: u32) -> Result<()> {
        let body = resize_body(&self.principal, session_id, rows, cols);
        self.send(body).await
    }

    /// Take the input lease for a person (host connection only).
    pub async fn acquire_lease(
        &mut self,
        session_id: &str,
        holder: &str,
        steal: bool,
    ) -> Result<()> {
        let body = acquire_body(&self.principal, session_id, holder, steal);
        self.send(body).await
    }

    /// Give the input lease back.
    pub async fn release_lease(&mut self, session_id: &str) -> Result<()> {
        let body = release_body(&self.principal, session_id);
        self.send(body).await
    }
}

impl ExecClient {
    /// Take the session's input lease for a person (PX-099); only the host
    /// connection may.
    pub async fn acquire_lease(
        &mut self,
        session_id: &str,
        holder: &str,
        steal: bool,
    ) -> Result<()> {
        let body = acquire_body(&self.principal, session_id, holder, steal);
        self.send(body).await
    }

    /// Give the input lease back.
    pub async fn release_lease(&mut self, session_id: &str) -> Result<()> {
        let body = release_body(&self.principal, session_id);
        self.send(body).await
    }

    /// A person's keystrokes: accepted only from the connection that holds
    /// the session's input lease (PX-099).
    pub async fn write_user_input(&mut self, session_id: &str, data: &[u8]) -> Result<()> {
        let body = stdin_body(&self.principal, session_id, data, true, true);
        self.send(body).await
    }
}
