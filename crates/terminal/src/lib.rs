//! `modbit-terminal` — exec client, streams and OutputRef handling (docs/21):
//! the Core-side client of `modbit-execd`. It sends structured `ExecRequest`s,
//! consumes cursor-ordered output, attaches to durable sessions from a cursor,
//! writes stdin and cancels. It never runs a process itself.
//!
//! Canonical owner: execution (`docs/12`).

#![forbid(unsafe_code)]

use modbit_protocol::client::{BoxedStream, connect_raw};
use modbit_protocol::framing::{FrameError, read_message, write_message};
use modbit_protocol::local::Endpoint;
use modbit_protocol::v1::exec_frame::Body;
use modbit_protocol::v1::{Auth, ClientHello, ClientKind, ExecFrame, ExecRequest, Hello};

pub use modbit_protocol::v1::{ExecError, ExecStarted, OutputChunk, ProcessExited, SessionInfo};

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
}

/// An authenticated connection to `modbit-execd`.
pub struct ExecClient {
    stream: BoxedStream,
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
            }) if a.compatible => Ok(Self { stream }),
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

    async fn send(&mut self, body: Body) -> Result<()> {
        write_message(&mut self.stream, &ExecFrame { body: Some(body) }).await?;
        Ok(())
    }

    /// Submit a structured request.
    pub async fn exec(&mut self, req: ExecRequest) -> Result<()> {
        self.send(Body::Exec(req)).await
    }

    /// Attach to a session from a cursor.
    pub async fn attach(&mut self, session_id: &str, after_cursor: u64) -> Result<()> {
        self.send(Body::Attach(modbit_protocol::v1::Attach {
            session_id: session_id.into(),
            after_cursor,
        }))
        .await
    }

    /// Write stdin bytes.
    pub async fn write_stdin(&mut self, session_id: &str, data: &[u8]) -> Result<()> {
        self.send(Body::Stdin(modbit_protocol::v1::WriteStdin {
            session_id: session_id.into(),
            data: data.to_vec(),
        }))
        .await
    }

    /// Cancel (kill) a session's process.
    pub async fn cancel(&mut self, session_id: &str) -> Result<()> {
        self.send(Body::Cancel(modbit_protocol::v1::Cancel {
            session_id: session_id.into(),
        }))
        .await
    }

    /// List sessions.
    pub async fn list(&mut self) -> Result<()> {
        self.send(Body::List(modbit_protocol::v1::ListSessions {}))
            .await
    }

    /// Next event; `None` when the broker closes.
    pub async fn next(&mut self) -> Result<Option<Event>> {
        loop {
            return match read_message::<_, ExecFrame>(&mut self.stream).await? {
                None => Ok(None),
                Some(ExecFrame {
                    body: Some(Body::Started(s)),
                }) => Ok(Some(Event::Started(s))),
                Some(ExecFrame {
                    body: Some(Body::Output(o)),
                }) => Ok(Some(Event::Output(o))),
                Some(ExecFrame {
                    body: Some(Body::Exited(e)),
                }) => Ok(Some(Event::Exited(e))),
                Some(ExecFrame {
                    body: Some(Body::Sessions(l)),
                }) => Ok(Some(Event::Sessions(l.sessions))),
                Some(ExecFrame {
                    body: Some(Body::Error(e)),
                }) => Err(Error::Exec {
                    code: e.code,
                    message: e.message,
                }),
                Some(ExecFrame {
                    body: Some(Body::HelloAck(_)),
                }) => continue,
                other => Err(Error::Unexpected(format!("{other:?}"))),
            };
        }
    }
}
