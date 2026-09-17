//! The guest RPC server: hello, admission, authenticated calls, policy.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use modbit_protocol::framing::{read_message, write_message};
use modbit_protocol::v1::{self as wire, GuestFrame, guest_call, guest_frame, guest_reply};
use modbit_sandbox::policy::{map_to_host, normalize, read_allowed, write_allowed};
use modbit_sandbox::{GUEST_PROTOCOL_MAJOR, GUEST_PROTOCOL_MINOR, auth};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The methods this guest implements.
pub const METHODS: &[&str] = &["health", "exec", "fs.read", "fs.write", "net.probe"];

/// Serve on a loopback TCP listener (the reference backend), announcing
/// the address on stdout.
pub async fn serve_tcp(addr: &str, mapping: Option<PathBuf>) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    println!("ready listen={local}");
    let boot_id = uuid::Uuid::now_v7().to_string();
    let started = Instant::now();
    loop {
        let (stream, _) = listener.accept().await?;
        stream.set_nodelay(true)?;
        let mapping = mapping.clone();
        let boot_id = boot_id.clone();
        // One gateway link at a time: a new connection is served after the
        // previous ended (the gateway reconnects after a link loss).
        if let Err(e) = serve_stream(stream, mapping, &boot_id, started).await {
            eprintln!("modbit-guest: link ended: {e}");
        }
    }
}

/// Serve one admitted link on `stream`.
pub async fn serve_stream<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    mapping: Option<PathBuf>,
    boot_id: &str,
    started: Instant,
) -> std::io::Result<()> {
    let hello = GuestFrame {
        body: Some(guest_frame::Body::Hello(wire::GuestHello {
            protocol_major: GUEST_PROTOCOL_MAJOR,
            protocol_minor: GUEST_PROTOCOL_MINOR,
            guest_version: env!("CARGO_PKG_VERSION").to_owned(),
            methods: METHODS.iter().map(|s| (*s).to_owned()).collect(),
            boot_id: boot_id.to_owned(),
        })),
    };
    write_message(&mut stream, &hello)
        .await
        .map_err(std::io::Error::other)?;
    let admit = read_message::<_, GuestFrame>(&mut stream)
        .await
        .map_err(std::io::Error::other)?;
    let admit = match admit.and_then(|f| f.body) {
        Some(guest_frame::Body::Admit(a)) => a,
        Some(guest_frame::Body::Refused(r)) => {
            eprintln!(
                "modbit-guest: refused by the gateway: {}: {}",
                r.code, r.message
            );
            return Ok(());
        }
        other => {
            return Err(std::io::Error::other(format!(
                "expected GuestAdmit, got {other:?}"
            )));
        }
    };
    if admit.protocol_major != GUEST_PROTOCOL_MAJOR {
        let refused = GuestFrame {
            body: Some(guest_frame::Body::Refused(wire::GuestRefused {
                code: "PROTOCOL_UNSUPPORTED".into(),
                message: format!(
                    "gateway speaks {}.{}, this guest {GUEST_PROTOCOL_MAJOR}.{GUEST_PROTOCOL_MINOR}",
                    admit.protocol_major, admit.protocol_minor
                ),
            })),
        };
        write_message(&mut stream, &refused)
            .await
            .map_err(std::io::Error::other)?;
        return Ok(());
    }
    let Some(policy) = admit.policy else {
        return Err(std::io::Error::other("admission without a policy"));
    };
    if admit.credential.len() < 16 {
        return Err(std::io::Error::other("admission without a credential"));
    }
    let admitted = GuestFrame {
        body: Some(guest_frame::Body::Admitted(wire::GuestAdmitted {
            sandbox_id: admit.sandbox_id.clone(),
            boot_id: boot_id.to_owned(),
        })),
    };
    write_message(&mut stream, &admitted)
        .await
        .map_err(std::io::Error::other)?;
    let mut guest = Guest {
        policy,
        credential: admit.credential,
        seen: HashSet::new(),
        boot_id: boot_id.to_owned(),
        started,
        mapping,
        running: 0,
    };
    while let Some(frame) = read_message::<_, GuestFrame>(&mut stream)
        .await
        .map_err(std::io::Error::other)?
    {
        let Some(guest_frame::Body::Call(call)) = frame.body else {
            continue;
        };
        let mut reply = guest.handle(call).await;
        auth::sign_reply(&guest.credential, &mut reply);
        write_message(
            &mut stream,
            &GuestFrame {
                body: Some(guest_frame::Body::Reply(reply)),
            },
        )
        .await
        .map_err(std::io::Error::other)?;
    }
    Ok(())
}

struct Guest {
    policy: wire::GuestPolicy,
    credential: Vec<u8>,
    seen: HashSet<String>,
    boot_id: String,
    started: Instant,
    /// The host directory the guest's workspace maps onto (reference
    /// backend); `None` when the guest owns its root (a MicroVM).
    mapping: Option<PathBuf>,
    running: u32,
}

fn refusal(call_id: &str, code: &str, message: impl Into<String>) -> wire::GuestReply {
    wire::GuestReply {
        call_id: call_id.to_owned(),
        auth: vec![],
        body: Some(guest_reply::Body::Refusal(wire::GuestRefusal {
            code: code.into(),
            message: message.into(),
        })),
    }
}

fn reply(call_id: &str, body: guest_reply::Body) -> wire::GuestReply {
    wire::GuestReply {
        call_id: call_id.to_owned(),
        auth: vec![],
        body: Some(body),
    }
}

impl Guest {
    fn host_path(&self, guest_path: &str) -> PathBuf {
        match &self.mapping {
            Some(dir) => map_to_host(dir, &self.policy.workspace_root, guest_path)
                .unwrap_or_else(|| PathBuf::from(normalize(guest_path))),
            None => PathBuf::from(normalize(guest_path)),
        }
    }

    /// The path as the guest sees it, with the symlinks it can resolve
    /// resolved (so a link out of the workspace does not carry a write out).
    fn resolved_guest_path(&self, guest_path: &str) -> String {
        let p = normalize(guest_path);
        // Resolve the deepest existing ancestor.
        let host = self.host_path(&p);
        let mut existing = host.clone();
        let mut rest: Vec<std::ffi::OsString> = Vec::new();
        while !existing.exists() {
            let Some(name) = existing.file_name() else {
                break;
            };
            rest.push(name.to_owned());
            if !existing.pop() {
                break;
            }
        }
        let Ok(canon) = std::fs::canonicalize(&existing) else {
            return p;
        };
        let Ok(canon_root) = self.canonical_workspace_host() else {
            return p;
        };
        // Map the canonical host path back into guest terms.
        let mapped = if let Ok(rel) = canon.strip_prefix(&canon_root) {
            let mut g = PathBuf::from(&self.policy.workspace_root);
            g.push(rel);
            g
        } else if self.mapping.is_some() {
            // Outside the mapped workspace: report it in host terms so the
            // policy refuses it (nothing outside /workspace is writable there).
            canon
        } else {
            canon
        };
        let mut full = mapped;
        for r in rest.iter().rev() {
            full.push(r);
        }
        full.to_string_lossy().replace('\\', "/")
    }

    fn canonical_workspace_host(&self) -> std::io::Result<PathBuf> {
        match &self.mapping {
            Some(dir) => std::fs::canonicalize(dir),
            None => std::fs::canonicalize(&self.policy.workspace_root),
        }
    }

    async fn handle(&mut self, call: wire::GuestCall) -> wire::GuestReply {
        let id = call.call_id.clone();
        if !auth::verify_call(&self.credential, &call) {
            return refusal(
                &id,
                "UNAUTHENTICATED",
                "the call does not verify under this sandbox's credential",
            );
        }
        if !self.seen.insert(id.clone()) {
            return refusal(&id, "REPLAYED", "a call with this id was already answered");
        }
        // The capability named must be the one the body exercises
        // (REQ-EV-0289: calls are capability-bound, not just typed).
        let expected = match &call.body {
            Some(guest_call::Body::Health(_)) => "health",
            Some(guest_call::Body::Exec(_)) => "proc.exec",
            Some(guest_call::Body::ReadFile(_)) => "fs.read",
            Some(guest_call::Body::WriteFile(_)) => "fs.write",
            Some(guest_call::Body::NetProbe(_)) => "net.probe",
            None => "",
        };
        if !expected.is_empty() && call.capability != expected {
            return refusal(
                &id,
                "BAD_CALL",
                format!(
                    "capability `{}` does not cover this call (`{expected}`)",
                    call.capability
                ),
            );
        }
        match call.body {
            Some(guest_call::Body::Health(_)) => reply(
                &id,
                guest_reply::Body::Health(wire::GuestHealthReport {
                    boot_id: self.boot_id.clone(),
                    uptime_ms: self.started.elapsed().as_millis() as u64,
                    processes: self.running,
                    kernel: kernel_release(),
                }),
            ),
            Some(guest_call::Body::Exec(e)) => self.exec(&id, e).await,
            Some(guest_call::Body::ReadFile(r)) => self.read_file(&id, &r),
            Some(guest_call::Body::WriteFile(w)) => self.write_file(&id, &w),
            Some(guest_call::Body::NetProbe(p)) => self.net_probe(&id, &p).await,
            None => refusal(&id, "BAD_CALL", "empty call body"),
        }
    }

    fn read_file(&self, id: &str, r: &wire::GuestReadFile) -> wire::GuestReply {
        let resolved = self.resolved_guest_path(&r.path);
        if let Err((code, msg)) = read_allowed(&self.policy, &resolved) {
            return refusal(id, code, msg);
        }
        let host = self.host_path(&resolved);
        match std::fs::read(&host) {
            Ok(mut bytes) => {
                let max = if r.max_bytes == 0 {
                    self.policy.max_output_bytes
                } else {
                    r.max_bytes.min(self.policy.max_output_bytes)
                } as usize;
                let truncated = bytes.len() > max;
                bytes.truncate(max);
                reply(
                    id,
                    guest_reply::Body::File(wire::GuestFileContent {
                        content: bytes,
                        truncated,
                    }),
                )
            }
            Err(e) => refusal(id, "BAD_CALL", format!("read `{}`: {e}", r.path)),
        }
    }

    fn write_file(&self, id: &str, w: &wire::GuestWriteFile) -> wire::GuestReply {
        let resolved = self.resolved_guest_path(&w.path);
        if let Err((code, msg)) = write_allowed(&self.policy, &resolved) {
            return refusal(id, code, msg);
        }
        if w.content.len() as u64 > self.policy.max_output_bytes.max(1) * 16 {
            return refusal(id, "LIMIT_EXCEEDED", "content exceeds the write bound");
        }
        let host = self.host_path(&resolved);
        if let Some(parent) = host.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return refusal(
                id,
                "BAD_CALL",
                format!("create `{}`: {e}", parent.display()),
            );
        }
        match std::fs::write(&host, &w.content) {
            Ok(()) => reply(
                id,
                guest_reply::Body::Written(wire::GuestFileWritten {
                    bytes: w.content.len() as u64,
                }),
            ),
            Err(e) => refusal(id, "BAD_CALL", format!("write `{}`: {e}", w.path)),
        }
    }

    async fn exec(&mut self, id: &str, e: wire::GuestExec) -> wire::GuestReply {
        if e.argv.is_empty() {
            return refusal(id, "BAD_CALL", "argv is empty");
        }
        if e.timeout_ms == 0 || e.timeout_ms > self.policy.exec_timeout_ms.max(1) {
            return refusal(
                id,
                "LIMIT_EXCEEDED",
                format!(
                    "timeout {} ms is outside (0, {}]",
                    e.timeout_ms, self.policy.exec_timeout_ms
                ),
            );
        }
        if self.running >= self.policy.max_processes.max(1) {
            return refusal(id, "LIMIT_EXCEEDED", "too many processes");
        }
        let cwd_guest = if e.cwd.is_empty() {
            self.policy.workspace_root.clone()
        } else {
            normalize(&e.cwd)
        };
        if let Err((code, msg)) = read_allowed(&self.policy, &cwd_guest) {
            return refusal(id, code, msg);
        }
        let cwd = self.host_path(&cwd_guest);
        let mut cmd = tokio::process::Command::new(&e.argv[0]);
        cmd.args(&e.argv[1..]).current_dir(&cwd).env_clear();
        for kv in &e.env {
            if let Some((k, v)) = kv.split_once('=') {
                cmd.env(k, v);
            }
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let started = Instant::now();
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(err) => return refusal(id, "BAD_CALL", format!("spawn `{}`: {err}", e.argv[0])),
        };
        self.running += 1;
        let max = self.policy.max_output_bytes as usize;
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let input = e.stdin.clone();
        let feed = async move {
            if let Some(mut s) = stdin.take() {
                let _ = s.write_all(&input).await;
                let _ = s.shutdown().await;
            }
        };
        let out = tokio::spawn(drain(stdout, max));
        let err = tokio::spawn(drain(stderr, max));
        let timeout = Duration::from_millis(e.timeout_ms);
        feed.await;
        let (status, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(s)) => (Some(s), false),
            Ok(Err(_)) => (None, false),
            Err(_) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                (None, true)
            }
        };
        self.running -= 1;
        let (stdout, stdout_truncated) = out.await.unwrap_or_default();
        let (stderr, stderr_truncated) = err.await.unwrap_or_default();
        let exit_code = status.and_then(|s| s.code()).unwrap_or(-1);
        reply(
            id,
            guest_reply::Body::Exec(wire::GuestExecResult {
                exit_code,
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
                timed_out,
                duration_ms: started.elapsed().as_millis() as u64,
            }),
        )
    }

    async fn net_probe(&self, id: &str, p: &wire::GuestNetProbe) -> wire::GuestReply {
        let timeout = Duration::from_millis(p.timeout_ms.clamp(100, 30_000));
        let target = format!("{}:{}", p.host, p.port);
        let result = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&target)).await;
        let (reachable, error) = match result {
            Ok(Ok(_)) => (true, String::new()),
            Ok(Err(e)) => (false, e.to_string()),
            Err(_) => (false, "timed out".into()),
        };
        reply(
            id,
            guest_reply::Body::Probe(wire::GuestNetProbeResult { reachable, error }),
        )
    }
}

/// Read a stream to its end, keeping the first `max` bytes.
async fn drain<R: AsyncRead + Unpin>(mut r: R, max: usize) -> (Vec<u8>, bool) {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if kept.len() < max {
                    let room = max - kept.len();
                    kept.extend_from_slice(&buf[..n.min(room)]);
                    if n > room {
                        truncated = true;
                    }
                } else {
                    truncated = true;
                }
            }
        }
    }
    (kept, truncated)
}

fn kernel_release() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}
