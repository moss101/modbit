//! The gateway's side of the guest RPC (docs/33 "Sandbox Gateway": the
//! gateway exchanges the ephemeral guest credential and every guest call is
//! authenticated on the channel). [`GuestLink::admit`] performs the
//! negotiation — the guest's hello, the version and method check, the
//! admission with the credential and the compiled policy — and
//! [`GuestLink::call`] sends one typed, signed call and verifies the signed
//! reply. Calls are sequential per link; a call's id is its nonce.

use std::time::Duration;

use modbit_protocol::framing::{read_message, write_message};
use modbit_protocol::v1::{self as wire, GuestFrame, guest_call, guest_frame, guest_reply};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::policy::CompiledPolicy;
use crate::{
    GUEST_PROTOCOL_MAJOR, GUEST_PROTOCOL_MINOR, REQUIRED_METHODS, Result, SandboxError, auth,
};

/// An admitted guest on its private channel.
pub struct GuestLink<S> {
    stream: S,
    credential: Vec<u8>,
    /// The guest's hello.
    pub hello: wire::GuestHello,
    /// The sandbox this link serves.
    pub sandbox_id: String,
    call_timeout: Duration,
}

/// A typed reply body.
pub type ReplyBody = guest_reply::Body;

impl<S: AsyncRead + AsyncWrite + Unpin + Send> GuestLink<S> {
    /// Negotiate and admit the guest at the other end of `stream` under
    /// `policy`; refuses a guest of another protocol major or one missing a
    /// required method (the guest is told why and the link dropped).
    pub async fn admit(
        stream: S,
        sandbox_id: &str,
        policy: &CompiledPolicy,
        timeout: Duration,
    ) -> Result<Self> {
        Self::admit_image(stream, sandbox_id, policy, timeout, None).await
    }

    /// `admit`, and the guest must be the one `expected` names (M8.4): a
    /// guest of another version or protocol than the verified image is
    /// refused `IMAGE_VERSION_MISMATCH` before any credential is issued.
    pub async fn admit_image(
        mut stream: S,
        sandbox_id: &str,
        policy: &CompiledPolicy,
        timeout: Duration,
        expected: Option<&crate::image::ImageManifest>,
    ) -> Result<Self> {
        let hello = tokio::time::timeout(timeout, read_message::<_, GuestFrame>(&mut stream))
            .await
            .map_err(|_| SandboxError::Guest("no hello within the admission timeout".into()))?
            .map_err(|e| SandboxError::Protocol(e.to_string()))?
            .ok_or_else(|| SandboxError::Guest("channel closed before hello".into()))?;
        let Some(guest_frame::Body::Hello(hello)) = hello.body else {
            let _ = write_message(
                &mut stream,
                &refused("BAD_FRAME", "expected GuestHello first"),
            )
            .await;
            return Err(SandboxError::Protocol("first frame was not a hello".into()));
        };
        if hello.protocol_major != GUEST_PROTOCOL_MAJOR {
            let msg = format!(
                "guest speaks protocol {}.{}, this gateway {GUEST_PROTOCOL_MAJOR}.{GUEST_PROTOCOL_MINOR}",
                hello.protocol_major, hello.protocol_minor
            );
            let _ = write_message(&mut stream, &refused("PROTOCOL_UNSUPPORTED", &msg)).await;
            return Err(SandboxError::Refused {
                code: "PROTOCOL_UNSUPPORTED".into(),
                message: msg,
            });
        }
        if let Some(missing) = REQUIRED_METHODS
            .iter()
            .find(|m| !hello.methods.iter().any(|h| h == *m))
        {
            let msg = format!("guest lacks method `{missing}`");
            let _ = write_message(&mut stream, &refused("METHOD_MISSING", &msg)).await;
            return Err(SandboxError::Refused {
                code: "METHOD_MISSING".into(),
                message: msg,
            });
        }
        if let Some(manifest) = expected
            && let Err(e) = crate::image::check_guest(manifest, &hello)
        {
            let msg = e.to_string();
            let _ = write_message(&mut stream, &refused("IMAGE_VERSION_MISMATCH", &msg)).await;
            return Err(e);
        }
        let credential = auth::fresh_credential();
        let admit = GuestFrame {
            body: Some(guest_frame::Body::Admit(wire::GuestAdmit {
                protocol_major: GUEST_PROTOCOL_MAJOR,
                protocol_minor: GUEST_PROTOCOL_MINOR,
                credential: credential.clone(),
                sandbox_id: sandbox_id.to_owned(),
                policy: Some(policy.guest_policy()),
            })),
        };
        write_message(&mut stream, &admit)
            .await
            .map_err(|e| SandboxError::Protocol(e.to_string()))?;
        let ack = tokio::time::timeout(timeout, read_message::<_, GuestFrame>(&mut stream))
            .await
            .map_err(|_| SandboxError::Guest("no admission acknowledgement".into()))?
            .map_err(|e| SandboxError::Protocol(e.to_string()))?
            .ok_or_else(|| SandboxError::Guest("channel closed during admission".into()))?;
        match ack.body {
            Some(guest_frame::Body::Admitted(a))
                if a.sandbox_id == sandbox_id && a.boot_id == hello.boot_id => {}
            Some(guest_frame::Body::Refused(r)) => {
                return Err(SandboxError::Refused {
                    code: r.code,
                    message: r.message,
                });
            }
            other => {
                return Err(SandboxError::Protocol(format!(
                    "unexpected admission reply: {other:?}"
                )));
            }
        }
        Ok(Self {
            stream,
            credential,
            hello,
            sandbox_id: sandbox_id.to_owned(),
            call_timeout: timeout,
        })
    }

    /// Per-call timeout (beyond the guest's own exec timeout).
    pub fn set_call_timeout(&mut self, t: Duration) {
        self.call_timeout = t;
    }

    /// One signed call; the reply verified.
    pub async fn call(
        &mut self,
        task_id: &str,
        effect_id: &str,
        capability: &str,
        body: guest_call::Body,
        extra: Duration,
    ) -> Result<ReplyBody> {
        let call_id = uuid::Uuid::now_v7().to_string();
        let mut call = wire::GuestCall {
            call_id: call_id.clone(),
            task_id: task_id.to_owned(),
            effect_id: effect_id.to_owned(),
            capability: capability.to_owned(),
            auth: vec![],
            body: Some(body),
        };
        auth::sign_call(&self.credential, &mut call);
        write_message(
            &mut self.stream,
            &GuestFrame {
                body: Some(guest_frame::Body::Call(call)),
            },
        )
        .await
        .map_err(|e| SandboxError::Guest(format!("send: {e}")))?;
        let frame = tokio::time::timeout(
            self.call_timeout + extra,
            read_message::<_, GuestFrame>(&mut self.stream),
        )
        .await
        .map_err(|_| SandboxError::Guest(format!("no reply to {capability} within the timeout")))?
        .map_err(|e| SandboxError::Guest(format!("recv: {e}")))?
        .ok_or_else(|| SandboxError::Guest("channel closed".into()))?;
        let Some(guest_frame::Body::Reply(reply)) = frame.body else {
            return Err(SandboxError::Protocol("expected a reply".into()));
        };
        if reply.call_id != call_id {
            return Err(SandboxError::Protocol(format!(
                "reply to {} for call {call_id}",
                reply.call_id
            )));
        }
        if !auth::verify_reply(&self.credential, &reply) {
            return Err(SandboxError::Protocol("reply failed authentication".into()));
        }
        reply
            .body
            .ok_or_else(|| SandboxError::Protocol("empty reply".into()))
    }

    /// Health.
    pub async fn health(&mut self, task_id: &str) -> Result<wire::GuestHealthReport> {
        match self
            .call(
                task_id,
                "",
                "health",
                guest_call::Body::Health(wire::GuestHealth {}),
                Duration::ZERO,
            )
            .await?
        {
            ReplyBody::Health(h) => Ok(h),
            ReplyBody::Refusal(r) => Err(SandboxError::Refused {
                code: r.code,
                message: r.message,
            }),
            other => Err(SandboxError::Protocol(format!(
                "unexpected reply: {other:?}"
            ))),
        }
    }

    /// Run a process in the guest.
    pub async fn exec(
        &mut self,
        task_id: &str,
        effect_id: &str,
        exec: wire::GuestExec,
    ) -> Result<wire::GuestExecResult> {
        let extra = Duration::from_millis(exec.timeout_ms);
        match self
            .call(
                task_id,
                effect_id,
                "proc.exec",
                guest_call::Body::Exec(exec),
                extra,
            )
            .await?
        {
            ReplyBody::Exec(r) => Ok(r),
            ReplyBody::Refusal(r) => Err(SandboxError::Refused {
                code: r.code,
                message: r.message,
            }),
            other => Err(SandboxError::Protocol(format!(
                "unexpected reply: {other:?}"
            ))),
        }
    }

    /// Read a file.
    pub async fn read_file(
        &mut self,
        task_id: &str,
        path: &str,
        max_bytes: u64,
    ) -> Result<wire::GuestFileContent> {
        match self
            .call(
                task_id,
                "",
                "fs.read",
                guest_call::Body::ReadFile(wire::GuestReadFile {
                    path: path.into(),
                    max_bytes,
                }),
                Duration::ZERO,
            )
            .await?
        {
            ReplyBody::File(f) => Ok(f),
            ReplyBody::Refusal(r) => Err(SandboxError::Refused {
                code: r.code,
                message: r.message,
            }),
            other => Err(SandboxError::Protocol(format!(
                "unexpected reply: {other:?}"
            ))),
        }
    }

    /// Write a file.
    pub async fn write_file(
        &mut self,
        task_id: &str,
        effect_id: &str,
        path: &str,
        content: Vec<u8>,
    ) -> Result<wire::GuestFileWritten> {
        match self
            .call(
                task_id,
                effect_id,
                "fs.write",
                guest_call::Body::WriteFile(wire::GuestWriteFile {
                    path: path.into(),
                    content,
                }),
                Duration::ZERO,
            )
            .await?
        {
            ReplyBody::Written(w) => Ok(w),
            ReplyBody::Refusal(r) => Err(SandboxError::Refused {
                code: r.code,
                message: r.message,
            }),
            other => Err(SandboxError::Protocol(format!(
                "unexpected reply: {other:?}"
            ))),
        }
    }

    /// Probe a TCP destination from inside the guest.
    pub async fn net_probe(
        &mut self,
        task_id: &str,
        host: &str,
        port: u16,
        timeout_ms: u64,
    ) -> Result<wire::GuestNetProbeResult> {
        let extra = Duration::from_millis(timeout_ms);
        match self
            .call(
                task_id,
                "",
                "net.probe",
                guest_call::Body::NetProbe(wire::GuestNetProbe {
                    host: host.into(),
                    port: u32::from(port),
                    timeout_ms,
                }),
                extra,
            )
            .await?
        {
            ReplyBody::Probe(p) => Ok(p),
            ReplyBody::Refusal(r) => Err(SandboxError::Refused {
                code: r.code,
                message: r.message,
            }),
            other => Err(SandboxError::Protocol(format!(
                "unexpected reply: {other:?}"
            ))),
        }
    }

    /// Send a call exactly as given — no signing, the caller's `call_id` —
    /// and return the raw reply (the conformance suite proves refusals of
    /// unauthenticated and replayed calls this way).
    pub async fn send_raw(&mut self, call: wire::GuestCall) -> Result<wire::GuestReply> {
        write_message(
            &mut self.stream,
            &GuestFrame {
                body: Some(guest_frame::Body::Call(call)),
            },
        )
        .await
        .map_err(|e| SandboxError::Guest(format!("send: {e}")))?;
        let frame = tokio::time::timeout(
            self.call_timeout,
            read_message::<_, GuestFrame>(&mut self.stream),
        )
        .await
        .map_err(|_| SandboxError::Guest("no reply within the timeout".into()))?
        .map_err(|e| SandboxError::Guest(format!("recv: {e}")))?
        .ok_or_else(|| SandboxError::Guest("channel closed".into()))?;
        match frame.body {
            Some(guest_frame::Body::Reply(r)) => Ok(r),
            other => Err(SandboxError::Protocol(format!(
                "expected a reply: {other:?}"
            ))),
        }
    }

    /// A signed call with the caller's id (to replay it).
    #[must_use]
    pub fn signed(
        &self,
        call_id: &str,
        task_id: &str,
        capability: &str,
        body: guest_call::Body,
    ) -> wire::GuestCall {
        let mut call = wire::GuestCall {
            call_id: call_id.to_owned(),
            task_id: task_id.to_owned(),
            effect_id: String::new(),
            capability: capability.to_owned(),
            auth: vec![],
            body: Some(body),
        };
        auth::sign_call(&self.credential, &mut call);
        call
    }

    /// Give the channel back (to close it).
    pub fn into_inner(self) -> S {
        self.stream
    }
}

fn refused(code: &str, message: &str) -> GuestFrame {
    GuestFrame {
        body: Some(guest_frame::Body::Refused(wire::GuestRefused {
            code: code.into(),
            message: message.into(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{NetworkPolicy, Resources, SandboxSpec, compile};
    use modbit_domain::{SessionId, TaskId, TenantId};

    fn policy() -> CompiledPolicy {
        compile(&SandboxSpec {
            tenant_id: TenantId::new(),
            session_id: SessionId::new(),
            task_id: TaskId::new(),
            workspace_source: std::path::PathBuf::from("/nowhere"),
            protected_paths: vec![],
            network: NetworkPolicy::default(),
            resources: Resources::default(),
        })
        .unwrap()
    }

    async fn guest_says(
        hello: wire::GuestHello,
    ) -> (
        Result<GuestLink<tokio::io::DuplexStream>>,
        Option<GuestFrame>,
    ) {
        let (host, mut guest) = tokio::io::duplex(64 * 1024);
        let guest_side = tokio::spawn(async move {
            write_message(
                &mut guest,
                &GuestFrame {
                    body: Some(guest_frame::Body::Hello(hello)),
                },
            )
            .await
            .unwrap();
            let answer = read_message::<_, GuestFrame>(&mut guest).await.unwrap();
            if let Some(GuestFrame {
                body: Some(guest_frame::Body::Admit(a)),
            }) = &answer
            {
                let admitted = GuestFrame {
                    body: Some(guest_frame::Body::Admitted(wire::GuestAdmitted {
                        sandbox_id: a.sandbox_id.clone(),
                        boot_id: "b".into(),
                    })),
                };
                write_message(&mut guest, &admitted).await.unwrap();
            }
            answer
        });
        let link = GuestLink::admit(host, "sb", &policy(), Duration::from_secs(5)).await;
        (link, guest_side.await.unwrap())
    }

    fn hello(major: u32, methods: &[&str]) -> wire::GuestHello {
        wire::GuestHello {
            protocol_major: major,
            protocol_minor: 0,
            guest_version: "t".into(),
            methods: methods.iter().map(|s| (*s).to_owned()).collect(),
            boot_id: "b".into(),
        }
    }

    /// QUAL-EV-0289: an unknown or stale protocol version and a missing
    /// method are refused before admission, and the guest is told which.
    #[tokio::test]
    async fn a_guest_of_another_protocol_major_or_missing_a_method_is_refused_before_admission() {
        let (link, answer) = guest_says(hello(GUEST_PROTOCOL_MAJOR + 1, REQUIRED_METHODS)).await;
        assert!(
            matches!(&link, Err(SandboxError::Refused { code, .. }) if code == "PROTOCOL_UNSUPPORTED"),
            "{:?}",
            link.err()
        );
        assert!(
            matches!(answer, Some(GuestFrame { body: Some(guest_frame::Body::Refused(r)) }) if r.code == "PROTOCOL_UNSUPPORTED")
        );
        let (link, answer) = guest_says(hello(GUEST_PROTOCOL_MAJOR, &["health", "exec"])).await;
        assert!(
            matches!(&link, Err(SandboxError::Refused { code, .. }) if code == "METHOD_MISSING"),
            "{:?}",
            link.err()
        );
        assert!(
            matches!(answer, Some(GuestFrame { body: Some(guest_frame::Body::Refused(r)) }) if r.message.contains("fs.read"))
        );
        let (link, answer) = guest_says(hello(GUEST_PROTOCOL_MAJOR, REQUIRED_METHODS)).await;
        assert!(link.is_ok(), "{:?}", link.err());
        assert!(
            matches!(answer, Some(GuestFrame { body: Some(guest_frame::Body::Admit(a)) }) if a.credential.len() == 32 && a.policy.is_some())
        );
    }
}
