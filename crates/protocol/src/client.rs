//! Authenticated local SurfaceProtocol client (thin: no orchestration, no
//! policy; docs/29). Used by the CLI and by tests against a real Core.

use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::framing::{FrameError, read_frame, write_frame};
use crate::local::Endpoint;
use crate::v1::surface_frame::Body;
use crate::v1::{
    Auth, ClientHello, ClientKind, CommandAck, CommandEnvelope, CommandStatus, Hello, Id,
    StoredEventFrame, SubscribeEvents, SurfaceFrame,
};

/// Client errors.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Framing / transport.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// The Core refused the handshake.
    #[error("handshake refused: {0}")]
    Refused(String),
    /// Unexpected frame.
    #[error("unexpected frame from Core: {0}")]
    Unexpected(String),
    /// The Core reported a protocol error and closed.
    #[error("protocol error {code}: {message}")]
    Protocol {
        /// Code.
        code: String,
        /// Message.
        message: String,
    },
    /// Command rejected with a typed error.
    #[error("command rejected {code}: {message}")]
    Rejected {
        /// Code.
        code: String,
        /// Message.
        message: String,
    },
}

/// A boxed duplex stream over either transport.
pub type BoxedStream = Box<dyn Duplex + Send + Unpin>;

/// Marker for async read+write streams.
pub trait Duplex: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite> Duplex for T {}

/// Open the raw local transport.
pub async fn connect_raw(endpoint: &Endpoint) -> std::io::Result<BoxedStream> {
    #[cfg(unix)]
    {
        let s = tokio::net::UnixStream::connect(endpoint.path()).await?;
        Ok(Box::new(s))
    }
    #[cfg(windows)]
    {
        // The server creates a fresh pipe instance per client; connecting can
        // briefly hit "pipe busy" between accepts, so retry for a moment.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match tokio::net::windows::named_pipe::ClientOptions::new().open(&endpoint.0) {
                Ok(c) => return Ok(Box::new(c)),
                Err(e) if e.raw_os_error() == Some(231) && std::time::Instant::now() < deadline => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// An authenticated connection.
pub struct Client {
    stream: BoxedStream,
    /// Negotiated protocol version from the Core.
    pub protocol_version: Option<crate::v1::ProtocolVersion>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("protocol_version", &self.protocol_version)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Connect and authenticate with the boot secret.
    pub async fn connect(
        endpoint: &Endpoint,
        boot_secret: &[u8],
        kind: ClientKind,
        client_build: &str,
    ) -> Result<Self, ClientError> {
        let mut stream = connect_raw(endpoint).await.map_err(FrameError::Io)?;
        let hello = SurfaceFrame {
            body: Some(Body::ClientHello(ClientHello {
                hello: Some(Hello {
                    protocol_version: Some(crate::PROTOCOL_VERSION),
                    client_kind: kind as i32,
                    client_build: client_build.to_owned(),
                    supported_command_types: vec![],
                }),
                auth: Some(Auth {
                    boot_secret: boot_secret.to_vec(),
                }),
            })),
        };
        write_frame(&mut stream, &hello).await?;
        match read_frame(&mut stream).await? {
            Some(SurfaceFrame {
                body: Some(Body::HelloAck(ack)),
            }) => {
                if !ack.compatible {
                    return Err(ClientError::Refused(ack.reason));
                }
                Ok(Self {
                    stream,
                    protocol_version: ack.protocol_version,
                })
            }
            Some(SurfaceFrame {
                body: Some(Body::Error(e)),
            }) => Err(ClientError::Protocol {
                code: e.code,
                message: e.message,
            }),
            None => Err(ClientError::Refused(
                "connection closed during handshake".into(),
            )),
            Some(other) => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    /// Send a command and wait for its acknowledgement.
    pub async fn command(&mut self, envelope: CommandEnvelope) -> Result<CommandAck, ClientError> {
        write_frame(
            &mut self.stream,
            &SurfaceFrame {
                body: Some(Body::Command(envelope)),
            },
        )
        .await?;
        loop {
            match read_frame(&mut self.stream).await? {
                Some(SurfaceFrame {
                    body: Some(Body::CommandAck(ack)),
                }) => {
                    if ack.status == CommandStatus::Rejected as i32 {
                        return Err(ClientError::Rejected {
                            code: ack.error_code,
                            message: ack.error_message,
                        });
                    }
                    return Ok(ack);
                }
                Some(SurfaceFrame {
                    body: Some(Body::Error(e)),
                }) => {
                    return Err(ClientError::Protocol {
                        code: e.code,
                        message: e.message,
                    });
                }
                // Live events may interleave when a subscription is active.
                Some(SurfaceFrame {
                    body: Some(Body::Event(_)),
                }) => continue,
                None => return Err(ClientError::Refused("connection closed".into())),
                Some(other) => return Err(ClientError::Unexpected(format!("{other:?}"))),
            }
        }
    }

    /// Subscribe to a session's events after `after_offset`.
    pub async fn subscribe(
        &mut self,
        session_id: Id,
        after_offset: u64,
    ) -> Result<(), ClientError> {
        write_frame(
            &mut self.stream,
            &SurfaceFrame {
                body: Some(Body::Subscribe(SubscribeEvents {
                    session_id: Some(session_id),
                    after_offset,
                })),
            },
        )
        .await?;
        Ok(())
    }

    /// Next event frame (blocks until one arrives); `None` when the Core closes.
    pub async fn next_event(&mut self) -> Result<Option<StoredEventFrame>, ClientError> {
        loop {
            match read_frame(&mut self.stream).await? {
                Some(SurfaceFrame {
                    body: Some(Body::Event(e)),
                }) => return Ok(Some(e)),
                Some(SurfaceFrame {
                    body: Some(Body::Error(e)),
                }) => {
                    return Err(ClientError::Protocol {
                        code: e.code,
                        message: e.message,
                    });
                }
                None => return Ok(None),
                Some(SurfaceFrame {
                    body: Some(Body::CommandAck(_)),
                }) => continue,
                Some(other) => return Err(ClientError::Unexpected(format!("{other:?}"))),
            }
        }
    }

    /// Decode a typed result payload from an ack.
    pub fn result<M: Message + Default>(ack: &CommandAck) -> Result<M, ClientError> {
        M::decode(ack.result.as_slice())
            .map_err(|e| ClientError::Unexpected(format!("undecodable result: {e}")))
    }
}
