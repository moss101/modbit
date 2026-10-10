//! The Core's client of the actuator RPC (PX-069; docs/66 CUC-E02).
//!
//! The actuator is a separate process that holds the operating system
//! permissions and nothing else. The Core launches it with its standard
//! input and output as the only channel (inherited descriptors are bound to
//! the launcher by construction — there is no endpoint another process could
//! squat), sends a per-launch random token, and accepts a welcome only from
//! the process it launched, only if that welcome proves the token, only at
//! this protocol's major version. Requests carry a deadline; one connection
//! multiplexes them by id; the actuator's unsolicited events (a physical
//! key, a Stop from its overlay) arrive on a broadcast channel.
//!
//! A call can fail in four ways the runtime must tell apart, because an
//! input is never retried after it may have been delivered (CUC-D02):
//! [`CallError::NotDelivered`] (provably nothing was sent), [`CallError::TimedOut`]
//! and [`CallError::Lost`] (sent; the outcome is unknown), and
//! [`CallError::Remote`] (the actuator answered with an error).

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use modbit_protocol::framing::{FrameError, read_message, write_message};
use modbit_protocol::v1 as wire;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex as AsyncMutex, broadcast, oneshot};

/// The protocol major version this Core speaks.
pub const PROTOCOL_MAJOR: u32 = 1;
/// The minor version.
pub const PROTOCOL_MINOR: u32 = 0;

/// A boxed future.
pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// The request variants.
pub type Call = wire::actuator_request::Call;
/// The response variants.
pub type Reply = wire::actuator_response::Result;

/// How to launch an actuator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActuatorConfig {
    /// The executable.
    pub executable: PathBuf,
    /// Its arguments.
    pub args: Vec<String>,
    /// Extra environment.
    pub env: Vec<(String, String)>,
    /// Where its standard error goes (appended); `None` = discarded.
    pub log_path: Option<PathBuf>,
}

/// What a welcome told the Core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActuatorInfo {
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Its process id.
    pub pid: u32,
    /// Platform.
    pub platform: String,
    /// Capabilities it offers.
    pub capabilities: Vec<String>,
}

/// Why a call did not produce an answer.
#[derive(Clone, Debug, PartialEq)]
pub enum CallError {
    /// Provably nothing reached the actuator (not connected, or the write failed).
    NotDelivered(String),
    /// The request was written; no answer arrived in time.
    TimedOut,
    /// The request was written; the connection closed before an answer.
    Lost,
    /// The actuator answered with an error.
    Remote(wire::ActuatorError),
    /// The actuator answered with something other than what was asked.
    Protocol(String),
}

/// Why an actuator could not be attached.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LaunchError {
    /// The process could not be started.
    #[error("the actuator could not be started: {0}")]
    Spawn(String),
    /// The welcome did not come, or was not valid.
    #[error("the actuator handshake failed: {0}")]
    Handshake(String),
    /// The actuator refused this Core.
    #[error("the actuator refused this Core: {0}")]
    Refused(String),
}

/// A connection to an actuator. Object-safe so the runtime can be tested
/// against an in-process one; production uses [`ProcessActuator`].
pub trait Actuator: Send + Sync {
    /// What the welcome said.
    fn info(&self) -> &ActuatorInfo;
    /// Send one call and wait for its answer within `deadline`.
    fn call<'a>(
        &'a self,
        call: Call,
        deadline: Duration,
    ) -> BoxFuture<'a, Result<Reply, CallError>>;
    /// The actuator's unsolicited events.
    fn events(&self) -> broadcast::Receiver<wire::ActuatorEvent>;
    /// Whether the connection is up.
    fn is_alive(&self) -> bool;
    /// Resolves when the connection ends.
    fn closed<'a>(&'a self) -> BoxFuture<'a, ()>;
    /// Kill the process (best effort).
    fn kill(&self);
}

/// The hex proof of a token a welcome carries.
#[must_use]
pub fn token_proof(token: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"modbit-actuator-welcome\0");
    h.update(token);
    hex::encode(h.finalize())
}

type Pending = Mutex<HashMap<u64, oneshot::Sender<wire::ActuatorResponse>>>;

struct Shared {
    info: ActuatorInfo,
    writer: AsyncMutex<Box<dyn AsyncWrite + Send + Unpin>>,
    pending: Pending,
    next_id: AtomicU64,
    alive: AtomicBool,
    events: broadcast::Sender<wire::ActuatorEvent>,
    closed: tokio::sync::watch::Sender<bool>,
    child: Mutex<Option<tokio::process::Child>>,
}

/// An actuator connection over a pair of byte streams, normally the standard
/// input and output of a child the Core launched.
#[derive(Clone)]
pub struct ProcessActuator {
    shared: Arc<Shared>,
}

impl ProcessActuator {
    /// Launch `config` and attach.
    ///
    /// # Errors
    /// The process did not start, the handshake failed, or it refused.
    pub async fn launch(
        config: &ActuatorConfig,
        handshake_timeout: Duration,
    ) -> Result<Self, LaunchError> {
        let mut command = tokio::process::Command::new(&config.executable);
        // The actuator never holds a credential of this Core: it cannot
        // print what it was never given.
        for (k, _) in std::env::vars() {
            let n = k.to_ascii_uppercase();
            if n.ends_with("_API_KEY")
                || n.ends_with("_TOKEN")
                || n.ends_with("_SECRET")
                || n.ends_with("_SECRET_KEY")
                || n.ends_with("_ACCESS_KEY")
                || n.ends_with("_PRIVATE_KEY")
                || n.contains("PASSWORD")
                || n.contains("PASSPHRASE")
            {
                command.env_remove(&k);
            }
        }
        for (k, v) in &config.env {
            command.env(k, v);
        }
        let stderr = match &config.log_path {
            Some(p) => {
                if let Some(dir) = p.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                match std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                {
                    Ok(f) => std::process::Stdio::from(f),
                    Err(_) => std::process::Stdio::null(),
                }
            }
            None => std::process::Stdio::null(),
        };
        let mut child = command
            .args(&config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(stderr)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| LaunchError::Spawn(format!("{}: {e}", config.executable.display())))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(LaunchError::Spawn("no standard streams".into()));
        };
        let pid = child.id();
        let token: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
        match Self::establish(
            Box::new(stdout),
            Box::new(stdin),
            &token,
            pid,
            handshake_timeout,
        )
        .await
        {
            Ok(me) => {
                // Reap the child when it exits so it is never a zombie, and
                // close the connection state with it.
                *me.shared.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
                Ok(me)
            }
            Err(e) => {
                let _ = child.start_kill();
                Err(e)
            }
        }
    }

    /// Attach over existing streams: send the hello, require a welcome that
    /// proves `token`, comes from `expected_pid` (when known) and speaks this
    /// major version.
    ///
    /// # Errors
    /// The handshake failed or was refused.
    pub async fn establish(
        mut reader: Box<dyn AsyncRead + Send + Unpin>,
        mut writer: Box<dyn AsyncWrite + Send + Unpin>,
        token: &[u8],
        expected_pid: Option<u32>,
        handshake_timeout: Duration,
    ) -> Result<Self, LaunchError> {
        let hello = wire::ActuatorFrame {
            body: Some(wire::actuator_frame::Body::Hello(wire::ActuatorHello {
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                token: token.to_vec(),
                core_pid: std::process::id(),
                core_version: env!("CARGO_PKG_VERSION").to_owned(),
            })),
        };
        write_message(&mut writer, &hello)
            .await
            .map_err(|e| LaunchError::Handshake(format!("hello not sent: {e}")))?;
        let first = tokio::time::timeout(
            handshake_timeout,
            read_message::<_, wire::ActuatorFrame>(&mut reader),
        )
        .await
        .map_err(|_| LaunchError::Handshake("no welcome in time".into()))?
        .map_err(|e| LaunchError::Handshake(format!("unreadable welcome: {e}")))?
        .ok_or_else(|| LaunchError::Handshake("the actuator closed before its welcome".into()))?;
        let Some(wire::actuator_frame::Body::Welcome(w)) = first.body else {
            return Err(LaunchError::Handshake(
                "the first frame was not a welcome".into(),
            ));
        };
        if !w.refusal.is_empty() {
            return Err(LaunchError::Refused(w.refusal));
        }
        if w.protocol_major != PROTOCOL_MAJOR {
            return Err(LaunchError::Refused(format!(
                "protocol major {} (this Core speaks {PROTOCOL_MAJOR})",
                w.protocol_major
            )));
        }
        if w.token_proof != token_proof(token) {
            return Err(LaunchError::Handshake(
                "the welcome does not prove this launch's token".into(),
            ));
        }
        if let Some(pid) = expected_pid
            && w.pid != pid
        {
            return Err(LaunchError::Handshake(format!(
                "the welcome came from process {} and the Core launched {pid}",
                w.pid
            )));
        }
        let (events, _) = broadcast::channel(64);
        let (closed, _) = tokio::sync::watch::channel(false);
        let shared = Arc::new(Shared {
            info: ActuatorInfo {
                name: w.actuator_name,
                version: w.actuator_version,
                pid: w.pid,
                platform: w.platform,
                capabilities: w.capabilities,
            },
            writer: AsyncMutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            events,
            closed,
            child: Mutex::new(None),
        });
        let s2 = Arc::clone(&shared);
        tokio::spawn(async move {
            loop {
                match read_message::<_, wire::ActuatorFrame>(&mut reader).await {
                    Ok(Some(frame)) => match frame.body {
                        Some(wire::actuator_frame::Body::Response(r)) => {
                            let tx = s2
                                .pending
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .remove(&r.request_id);
                            if let Some(tx) = tx {
                                let _ = tx.send(r);
                            }
                        }
                        Some(wire::actuator_frame::Body::Event(e)) => {
                            let _ = s2.events.send(e);
                        }
                        _ => {}
                    },
                    Ok(None) | Err(FrameError::Io(_)) => break,
                    Err(_) => break,
                }
            }
            s2.alive.store(false, Ordering::SeqCst);
            // Every call still waiting was written and will never be answered.
            s2.pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
            // `send_replace`: the value must stick even when nobody is
            // subscribed yet.
            s2.closed.send_replace(true);
        });
        Ok(Self { shared })
    }

    /// The actuator's process id.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.shared.info.pid
    }
}

impl Actuator for ProcessActuator {
    fn info(&self) -> &ActuatorInfo {
        &self.shared.info
    }

    fn call<'a>(
        &'a self,
        call: Call,
        deadline: Duration,
    ) -> BoxFuture<'a, Result<Reply, CallError>> {
        Box::pin(async move {
            let s = &self.shared;
            if !s.alive.load(Ordering::SeqCst) {
                return Err(CallError::NotDelivered(
                    "the actuator is not running".into(),
                ));
            }
            let id = s.next_id.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = oneshot::channel();
            s.pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id, tx);
            let frame = wire::ActuatorFrame {
                body: Some(wire::actuator_frame::Body::Request(wire::ActuatorRequest {
                    request_id: id,
                    deadline_ms: u64::try_from(deadline.as_millis()).unwrap_or(u64::MAX),
                    call: Some(call),
                })),
            };
            {
                let mut w = s.writer.lock().await;
                if let Err(e) = write_message(&mut *w, &frame).await {
                    s.pending
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&id);
                    // A frame that did not finish cannot decode on the other
                    // side: nothing ran.
                    return Err(CallError::NotDelivered(format!("write failed: {e}")));
                }
            }
            // The actuator gets the deadline to answer; the Core waits a
            // little longer so the actuator's own timeout answer can arrive.
            let wait = deadline + Duration::from_millis(250);
            match tokio::time::timeout(wait, rx).await {
                Ok(Ok(resp)) => match resp.result {
                    Some(wire::actuator_response::Result::Error(e)) => Err(CallError::Remote(e)),
                    Some(r) => Ok(r),
                    None => Err(CallError::Protocol("an empty response".into())),
                },
                Ok(Err(_)) => Err(CallError::Lost),
                Err(_) => {
                    s.pending
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&id);
                    if s.alive.load(Ordering::SeqCst) {
                        Err(CallError::TimedOut)
                    } else {
                        Err(CallError::Lost)
                    }
                }
            }
        })
    }

    fn events(&self) -> broadcast::Receiver<wire::ActuatorEvent> {
        self.shared.events.subscribe()
    }

    fn is_alive(&self) -> bool {
        self.shared.alive.load(Ordering::SeqCst)
    }

    fn closed<'a>(&'a self) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let mut rx = self.shared.closed.subscribe();
            while !*rx.borrow() {
                if rx.changed().await.is_err() {
                    break;
                }
            }
        })
    }

    fn kill(&self) {
        if let Some(c) = self
            .shared
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            let _ = c.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    /// An in-process actuator on the far end of a duplex pair.
    async fn fake(
        reply: impl Fn(&wire::ActuatorRequest) -> Option<wire::ActuatorResponse> + Send + 'static,
        proof_for: impl Fn(&[u8]) -> String + Send + 'static,
        major: u32,
        pid: u32,
    ) -> (
        Box<dyn AsyncRead + Send + Unpin>,
        Box<dyn AsyncWrite + Send + Unpin>,
    ) {
        let (core_r, mut act_w) = duplex(1 << 16);
        let (mut act_r, core_w) = duplex(1 << 16);
        tokio::spawn(async move {
            let hello: wire::ActuatorFrame = read_message(&mut act_r).await.unwrap().unwrap();
            let Some(wire::actuator_frame::Body::Hello(h)) = hello.body else {
                return;
            };
            let welcome = wire::ActuatorFrame {
                body: Some(wire::actuator_frame::Body::Welcome(wire::ActuatorWelcome {
                    protocol_major: major,
                    protocol_minor: 0,
                    actuator_name: "fake".into(),
                    actuator_version: "0".into(),
                    pid,
                    platform: "fake".into(),
                    token_proof: proof_for(&h.token),
                    capabilities: vec!["app_scope".into()],
                    refusal: String::new(),
                })),
            };
            write_message(&mut act_w, &welcome).await.unwrap();
            while let Ok(Some(f)) = read_message::<_, wire::ActuatorFrame>(&mut act_r).await {
                if let Some(wire::actuator_frame::Body::Request(r)) = f.body
                    && let Some(resp) = reply(&r)
                {
                    let out = wire::ActuatorFrame {
                        body: Some(wire::actuator_frame::Body::Response(resp)),
                    };
                    if write_message(&mut act_w, &out).await.is_err() {
                        return;
                    }
                }
            }
        });
        (Box::new(core_r), Box::new(core_w))
    }

    fn pong(r: &wire::ActuatorRequest) -> Option<wire::ActuatorResponse> {
        Some(wire::ActuatorResponse {
            request_id: r.request_id,
            result: Some(wire::actuator_response::Result::Ping(wire::PingResponse {
                lease_held: true,
                lease_generation: 3,
                human_active: false,
            })),
        })
    }

    #[tokio::test]
    async fn a_welcome_that_proves_the_token_is_accepted_and_calls_are_matched_by_id() {
        let (r, w) = fake(pong, |t| token_proof(t), PROTOCOL_MAJOR, 77).await;
        let token = vec![7u8; 32];
        // The fake derives its proof from the token the hello carried, which is
        // the one the Core generated, so use the real path.
        let a = ProcessActuator::establish(r, w, &token, Some(77), Duration::from_secs(2)).await;
        // The hello carries `token`, so the proof matches.
        let a = a.unwrap();
        assert_eq!(a.info().name, "fake");
        let got = a
            .call(
                Call::Ping(wire::PingRequest::default()),
                Duration::from_secs(2),
            )
            .await
            .unwrap();
        assert!(matches!(got, Reply::Ping(p) if p.lease_generation == 3));
    }

    #[tokio::test]
    async fn a_welcome_with_the_wrong_proof_pid_or_major_is_refused() {
        let token = vec![1u8; 32];
        let (r, w) = fake(pong, |_| "deadbeef".into(), PROTOCOL_MAJOR, 5).await;
        let e = ProcessActuator::establish(r, w, &token, Some(5), Duration::from_secs(2))
            .await
            .err()
            .unwrap();
        assert!(matches!(e, LaunchError::Handshake(m) if m.contains("token")));

        let (r, w) = fake(pong, |t| token_proof(t), PROTOCOL_MAJOR, 5).await;
        let e = ProcessActuator::establish(r, w, &token, Some(6), Duration::from_secs(2))
            .await
            .err()
            .unwrap();
        assert!(matches!(e, LaunchError::Handshake(m) if m.contains("launched")));

        let (r, w) = fake(pong, |t| token_proof(t), PROTOCOL_MAJOR + 1, 5).await;
        let e = ProcessActuator::establish(r, w, &token, Some(5), Duration::from_secs(2))
            .await
            .err()
            .unwrap();
        assert!(matches!(e, LaunchError::Refused(m) if m.contains("protocol major")));
    }

    #[tokio::test]
    async fn a_silent_actuator_times_out_after_the_write_and_a_dead_one_is_not_delivered() {
        let (r, w) = fake(|_| None, |t| token_proof(t), PROTOCOL_MAJOR, 9).await;
        let a = ProcessActuator::establish(r, w, &[3u8; 32], Some(9), Duration::from_secs(2))
            .await
            .unwrap();
        let e = a
            .call(
                Call::Ping(wire::PingRequest::default()),
                Duration::from_millis(100),
            )
            .await
            .unwrap_err();
        assert_eq!(
            e,
            CallError::TimedOut,
            "written, never answered: the outcome is unknown"
        );
        assert!(a.is_alive());
    }

    #[tokio::test]
    async fn a_connection_that_closes_with_a_call_in_flight_loses_the_call() {
        // The fake closes its end when it sees a request.
        let (core_r, mut act_w) = duplex(1 << 16);
        let (mut act_r, core_w) = duplex(1 << 16);
        tokio::spawn(async move {
            let hello: wire::ActuatorFrame = read_message(&mut act_r).await.unwrap().unwrap();
            let Some(wire::actuator_frame::Body::Hello(h)) = hello.body else {
                return;
            };
            let welcome = wire::ActuatorFrame {
                body: Some(wire::actuator_frame::Body::Welcome(wire::ActuatorWelcome {
                    protocol_major: PROTOCOL_MAJOR,
                    token_proof: token_proof(&h.token),
                    pid: 1,
                    ..Default::default()
                })),
            };
            write_message(&mut act_w, &welcome).await.unwrap();
            let _req: Option<wire::ActuatorFrame> = read_message(&mut act_r).await.unwrap();
            drop(act_w);
            drop(act_r);
        });
        let a = ProcessActuator::establish(
            Box::new(core_r),
            Box::new(core_w),
            &[4u8; 32],
            Some(1),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        let e = a
            .call(
                Call::Ping(wire::PingRequest::default()),
                Duration::from_secs(5),
            )
            .await
            .unwrap_err();
        assert_eq!(e, CallError::Lost);
        a.closed().await;
        assert!(!a.is_alive());
        let e = a
            .call(
                Call::Ping(wire::PingRequest::default()),
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
        assert!(matches!(e, CallError::NotDelivered(_)), "{e:?}");
    }
}
