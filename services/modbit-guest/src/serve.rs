//! The guest RPC server: hello, admission, authenticated calls, policy.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::procs::{ProcTable, StartSpec};

use modbit_protocol::framing::{read_message, write_message};
use modbit_protocol::v1::{self as wire, GuestFrame, guest_call, guest_frame, guest_reply};
use modbit_sandbox::policy::{map_to_host, normalize, read_allowed, write_allowed};
use modbit_sandbox::{GUEST_PROTOCOL_MAJOR, GUEST_PROTOCOL_MINOR, auth};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

static BROKER: std::sync::OnceLock<crate::proxy::BrokerAddr> = std::sync::OnceLock::new();
static PROXY_STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// Where the egress broker is, from here (set once at start).
pub fn set_broker(addr: crate::proxy::BrokerAddr) {
    let _ = BROKER.set(addr);
}

/// Start the local egress proxy once, when the admitted policy grants egress.
fn start_proxy_if_granted(policy: &wire::GuestPolicy) -> bool {
    if !policy.egress_proxy {
        return false;
    }
    let Some(addr) = BROKER.get().cloned() else {
        eprintln!("modbit-guest: the policy grants egress but no broker address is known here");
        return false;
    };
    if PROXY_STARTED.set(()).is_ok()
        && let Err(e) = crate::proxy::start(addr)
    {
        eprintln!("modbit-guest: egress proxy: {e}");
        return false;
    }
    crate::proxy::addr().is_some()
}

/// The environment a process gets so its HTTP clients use the local proxy.
fn proxy_env() -> Vec<String> {
    let Some(addr) = crate::proxy::addr() else {
        return vec![];
    };
    let url = format!("http://{addr}");
    vec![
        format!("http_proxy={url}"),
        format!("https_proxy={url}"),
        format!("HTTP_PROXY={url}"),
        format!("HTTPS_PROXY={url}"),
    ]
}

/// The methods this guest implements.
pub const METHODS: &[&str] = &[
    "health",
    "exec",
    "fs.read",
    "fs.write",
    "net.probe",
    "proc",
    "pty",
    "fs.dir",
    "net.egress",
    "browser",
];

/// Serve on a loopback TCP listener (the reference backend), announcing
/// the address on stdout.
pub async fn serve_tcp(addr: &str, mapping: Option<PathBuf>) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    println!("ready listen={local}");
    // A term stops the browser this guest runs (M8.8) before the exit.
    #[cfg(unix)]
    tokio::spawn(async {
        if let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            term.recv().await;
            crate::browser::stop();
            std::process::exit(0);
        }
    });
    let boot_id = uuid::Uuid::now_v7().to_string();
    let started = Instant::now();
    let procs = Arc::new(ProcTable::default());
    loop {
        let (stream, _) = listener.accept().await?;
        stream.set_nodelay(true)?;
        let mapping = mapping.clone();
        let boot_id = boot_id.clone();
        // Every link is served on its own task: a relink after a lost
        // channel is admitted at once, whatever the old channel's fate;
        // the processes and their output are shared and survive the link.
        let procs = Arc::clone(&procs);
        tokio::spawn(async move {
            if let Err(e) = serve_stream(stream, mapping, &boot_id, started, procs).await {
                eprintln!("modbit-guest: link ended: {e}");
            }
        });
    }
}

/// Serve one admitted link on `stream`.
pub async fn serve_stream<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    mapping: Option<PathBuf>,
    boot_id: &str,
    started: Instant,
    procs: Arc<ProcTable>,
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
    eprintln!("modbit-guest: hello sent on a new link");
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
    // A MicroVM guest also pins the protected paths at the mount level, so
    // a process it runs cannot write them either (REQ-EV-0290); the
    // reference backend, which isolates nothing, enforces them only in its
    // own file operations.
    if mapping.is_none() {
        enforce_protected_mounts(&policy);
    }
    let egress = start_proxy_if_granted(&policy);
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
        procs,
        egress,
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
        // M8.8: a forward turns this link into a raw relay to the browser's
        // DevTools port once the answer is on the wire; no call follows.
        let forward_to = match &reply.body {
            Some(guest_reply::Body::BrowserForwarding(f)) => Some(f.port),
            _ => None,
        };
        write_message(
            &mut stream,
            &GuestFrame {
                body: Some(guest_frame::Body::Reply(reply)),
            },
        )
        .await
        .map_err(std::io::Error::other)?;
        if let Some(port) = forward_to {
            let mut cdp = tokio::net::TcpStream::connect(("127.0.0.1", port as u16)).await?;
            let _ = cdp.set_nodelay(true);
            eprintln!("modbit-guest: link forwarded to the browser's DevTools ({port})");
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut cdp).await;
            return Ok(());
        }
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
    procs: Arc<ProcTable>,
    /// Whether the policy grants egress (the proxy runs; processes get it).
    egress: bool,
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
            Some(guest_call::Body::ProcStart(p)) if p.pty => "pty",
            Some(guest_call::Body::ProcStart(_))
            | Some(guest_call::Body::ProcFollow(_))
            | Some(guest_call::Body::ProcWrite(_))
            | Some(guest_call::Body::ProcCancel(_)) => "proc.exec",
            Some(guest_call::Body::PtyResize(_)) => "pty",
            Some(guest_call::Body::ListDir(_))
            | Some(guest_call::Body::Stat(_))
            | Some(guest_call::Body::FsSnapshot(_)) => "fs.read",
            Some(guest_call::Body::Mkdir(_))
            | Some(guest_call::Body::Remove(_))
            | Some(guest_call::Body::Rename(_)) => "fs.write",
            Some(guest_call::Body::BrowserStart(_))
            | Some(guest_call::Body::BrowserStop(_))
            | Some(guest_call::Body::BrowserForward(_)) => "browser",
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
                    processes: self.running + self.procs.running(),
                    kernel: kernel_release(),
                }),
            ),
            Some(guest_call::Body::Exec(e)) => self.exec(&id, e).await,
            Some(guest_call::Body::ReadFile(r)) => self.read_file(&id, &r),
            Some(guest_call::Body::WriteFile(w)) => self.write_file(&id, &w),
            Some(guest_call::Body::NetProbe(p)) => self.net_probe(&id, &p).await,
            Some(guest_call::Body::ProcStart(p)) => self.proc_start(&id, p).await,
            Some(guest_call::Body::ProcFollow(f)) => self.proc_follow(&id, &f).await,
            Some(guest_call::Body::ProcWrite(w)) => self.proc_write(&id, &w).await,
            Some(guest_call::Body::ProcCancel(c)) => self.proc_cancel(&id, &c).await,
            Some(guest_call::Body::PtyResize(r)) => self.pty_resize(&id, &r),
            Some(guest_call::Body::ListDir(l)) => self.list_dir(&id, &l),
            Some(guest_call::Body::FsSnapshot(f)) => self.fs_snapshot(&id, &f),
            Some(guest_call::Body::Stat(st)) => self.stat(&id, &st),
            Some(guest_call::Body::Mkdir(m)) => self.mkdir(&id, &m),
            Some(guest_call::Body::Remove(r)) => self.remove(&id, &r),
            Some(guest_call::Body::Rename(r)) => self.rename(&id, &r),
            Some(guest_call::Body::BrowserStart(b)) => self.browser_start(&id, &b).await,
            Some(guest_call::Body::BrowserStop(_)) => {
                if !self.policy.browser {
                    return refusal(&id, "POLICY_DENIED", "the policy grants no browser");
                }
                crate::browser::stop();
                reply(
                    &id,
                    guest_reply::Body::FsDone(wire::GuestFsDone {
                        path: "browser".into(),
                    }),
                )
            }
            Some(guest_call::Body::BrowserForward(_)) => {
                if !self.policy.browser {
                    return refusal(&id, "POLICY_DENIED", "the policy grants no browser");
                }
                match crate::browser::running() {
                    Some(r) => reply(
                        &id,
                        guest_reply::Body::BrowserForwarding(wire::GuestBrowserForwarding {
                            port: u32::from(r.port),
                        }),
                    ),
                    None => refusal(&id, "BROWSER_NOT_RUNNING", "start the browser first"),
                }
            }
            None => refusal(&id, "BAD_CALL", "empty call body"),
        }
    }

    /// M8.8: the guest's headless Chromium, on the policy's grant; its
    /// requests go through the egress proxy (or nowhere).
    async fn browser_start(&mut self, id: &str, b: &wire::GuestBrowserStart) -> wire::GuestReply {
        if !self.policy.browser {
            return refusal(id, "POLICY_DENIED", "the policy grants no browser");
        }
        let proxy = if self.egress {
            crate::proxy::addr()
        } else {
            None
        };
        let data_dir = match &self.mapping {
            Some(dir) => dir
                .parent()
                .map(|p| p.join("browser"))
                .unwrap_or_else(|| dir.join(".modbit-browser")),
            None => PathBuf::from("/tmp/modbit-browser"),
        };
        match crate::browser::start(b.width, b.height, proxy.as_deref(), &data_dir).await {
            Ok((r, already)) => reply(
                id,
                guest_reply::Body::BrowserStarted(wire::GuestBrowserStarted {
                    port: u32::from(r.port),
                    ws_path: r.ws_path,
                    pid: r.pid,
                    version: String::new(),
                    already_running: already,
                }),
            ),
            Err((code, msg)) => refusal(id, code, msg),
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
        for kv in self.process_env(&e.env) {
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

impl Guest {
    /// The environment a process gets: exactly what the call names, plus
    /// the local proxy when egress is granted and the call set no proxy.
    fn process_env(&self, named: &[String]) -> Vec<String> {
        let mut env: Vec<String> = named.to_vec();
        if self.egress
            && !named.iter().any(|kv| {
                let l = kv.to_ascii_lowercase();
                l.starts_with("http_proxy=") || l.starts_with("https_proxy=")
            })
        {
            env.extend(proxy_env());
        }
        env
    }

    fn resolve_cwd(&self, cwd: &str) -> Result<PathBuf, (&'static str, String)> {
        let cwd_guest = if cwd.is_empty() {
            self.policy.workspace_root.clone()
        } else {
            normalize(cwd)
        };
        read_allowed(&self.policy, &cwd_guest)?;
        Ok(self.host_path(&cwd_guest))
    }

    async fn proc_start(&mut self, id: &str, p: wire::GuestProcStart) -> wire::GuestReply {
        if p.argv.is_empty() {
            return refusal(id, "BAD_CALL", "argv is empty");
        }
        let ceiling = self.policy.exec_timeout_ms.max(1);
        let timeout_ms = if p.timeout_ms == 0 {
            ceiling
        } else {
            p.timeout_ms
        };
        if timeout_ms > ceiling {
            return refusal(
                id,
                "LIMIT_EXCEEDED",
                format!("timeout {timeout_ms} ms is above the ceiling {ceiling}"),
            );
        }
        if self.running + self.procs.running() >= self.policy.max_processes.max(1) {
            return refusal(id, "LIMIT_EXCEEDED", "too many processes");
        }
        let cwd = match self.resolve_cwd(&p.cwd) {
            Ok(c) => c,
            Err((code, msg)) => return refusal(id, code, msg),
        };
        let env = self.process_env(&p.env);
        let spec = StartSpec {
            argv: &p.argv,
            cwd,
            env: &env,
            timeout: Duration::from_millis(timeout_ms),
            pty: p.pty,
            size: (
                u16::try_from(p.cols).unwrap_or(120),
                u16::try_from(p.rows).unwrap_or(40),
            ),
            stdin_open: p.stdin_open,
        };
        match self.procs.start(spec).await {
            Ok(proc) => reply(
                id,
                guest_reply::Body::ProcStarted(wire::GuestProcStarted {
                    proc_id: proc.id.clone(),
                    pid: proc.pid,
                }),
            ),
            Err(e) => refusal(id, "BAD_CALL", format!("start `{}`: {e}", p.argv[0])),
        }
    }

    async fn proc_follow(&self, id: &str, f: &wire::GuestProcFollow) -> wire::GuestReply {
        let Some(proc) = self.procs.get(&f.proc_id) else {
            return refusal(id, "BAD_CALL", format!("no process `{}`", f.proc_id));
        };
        let max = if f.max_bytes == 0 {
            self.policy.max_output_bytes
        } else {
            f.max_bytes.min(self.policy.max_output_bytes)
        } as usize;
        let wait = Duration::from_millis(f.wait_ms.min(30_000));
        let out = proc.follow(f.after_cursor, max, wait).await;
        reply(id, guest_reply::Body::ProcOutput(out))
    }

    async fn proc_write(&self, id: &str, w: &wire::GuestProcWrite) -> wire::GuestReply {
        let Some(proc) = self.procs.get(&w.proc_id) else {
            return refusal(id, "BAD_CALL", format!("no process `{}`", w.proc_id));
        };
        match proc.write_stdin(&w.data, w.close_stdin).await {
            Ok(()) => reply(
                id,
                guest_reply::Body::ProcAck(wire::GuestProcAck {
                    proc_id: w.proc_id.clone(),
                }),
            ),
            Err(e) => refusal(id, "BAD_CALL", format!("stdin: {e}")),
        }
    }

    async fn proc_cancel(&self, id: &str, c: &wire::GuestProcCancel) -> wire::GuestReply {
        let Some(proc) = self.procs.get(&c.proc_id) else {
            return refusal(id, "BAD_CALL", format!("no process `{}`", c.proc_id));
        };
        proc.cancel().await;
        reply(
            id,
            guest_reply::Body::ProcAck(wire::GuestProcAck {
                proc_id: c.proc_id.clone(),
            }),
        )
    }

    fn pty_resize(&self, id: &str, r: &wire::GuestPtyResize) -> wire::GuestReply {
        let Some(proc) = self.procs.get(&r.proc_id) else {
            return refusal(id, "BAD_CALL", format!("no process `{}`", r.proc_id));
        };
        match proc.resize(
            u16::try_from(r.cols).unwrap_or(120),
            u16::try_from(r.rows).unwrap_or(40),
        ) {
            Ok(()) => reply(
                id,
                guest_reply::Body::ProcAck(wire::GuestProcAck {
                    proc_id: r.proc_id.clone(),
                }),
            ),
            Err(e) => refusal(id, "BAD_CALL", format!("resize: {e}")),
        }
    }

    /// M8.9: every regular file under the root, hashed — what a checkpoint
    /// of the guest's worktree compares against the seed.
    fn fs_snapshot(&self, id: &str, f: &wire::GuestFsSnapshot) -> wire::GuestReply {
        let root_guest = if f.root.is_empty() {
            self.policy.workspace_root.clone()
        } else {
            self.resolved_guest_path(&f.root)
        };
        if let Err((code, msg)) = read_allowed(&self.policy, &root_guest) {
            return refusal(id, code, msg);
        }
        let host_root = self.host_path(&root_guest);
        let max = if f.max_entries == 0 {
            20_000
        } else {
            f.max_entries as usize
        };
        let mut entries = Vec::new();
        let mut truncated = false;
        let mut stack = vec![host_root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut items: Vec<_> = rd.flatten().collect();
            items.sort_by_key(|e| e.file_name());
            for e in items {
                let Ok(ft) = e.file_type() else { continue };
                let name = e.file_name().to_string_lossy().into_owned();
                if ft.is_dir() {
                    if name != ".git" {
                        stack.push(e.path());
                    }
                    continue;
                }
                if !ft.is_file() {
                    continue;
                }
                if entries.len() >= max {
                    truncated = true;
                    break;
                }
                let Ok(bytes) = std::fs::read(e.path()) else {
                    continue;
                };
                use sha2::Digest;
                let sha256 = hex::encode(sha2::Sha256::digest(&bytes));
                let rel = e
                    .path()
                    .strip_prefix(&host_root)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or(name);
                entries.push(wire::GuestFsEntry {
                    path: rel,
                    size: bytes.len() as u64,
                    sha256,
                });
            }
            if truncated {
                break;
            }
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        reply(
            id,
            guest_reply::Body::FsSnapshot(wire::GuestFsSnapshotResult { entries, truncated }),
        )
    }

    fn list_dir(&self, id: &str, l: &wire::GuestListDir) -> wire::GuestReply {
        let resolved = self.resolved_guest_path(&l.path);
        if let Err((code, msg)) = read_allowed(&self.policy, &resolved) {
            return refusal(id, code, msg);
        }
        let host = self.host_path(&resolved);
        let max = if l.max_entries == 0 {
            10_000
        } else {
            l.max_entries as usize
        };
        let rd = match std::fs::read_dir(&host) {
            Ok(rd) => rd,
            Err(e) => return refusal(id, "BAD_CALL", format!("list `{}`: {e}", l.path)),
        };
        let mut entries = Vec::new();
        let mut truncated = false;
        for e in rd.flatten() {
            if entries.len() >= max {
                truncated = true;
                break;
            }
            let meta = e.metadata().ok();
            let ft = e.file_type().ok();
            entries.push(wire::GuestDirEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                kind: match ft {
                    Some(t) if t.is_dir() => "dir".into(),
                    Some(t) if t.is_file() => "file".into(),
                    Some(t) if t.is_symlink() => "symlink".into(),
                    _ => "other".into(),
                },
                size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified_ms: modified_ms(meta.as_ref()),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        reply(
            id,
            guest_reply::Body::Listing(wire::GuestDirListing { entries, truncated }),
        )
    }

    fn stat(&self, id: &str, st: &wire::GuestStat) -> wire::GuestReply {
        let resolved = self.resolved_guest_path(&st.path);
        if let Err((code, msg)) = read_allowed(&self.policy, &resolved) {
            return refusal(id, code, msg);
        }
        let host = self.host_path(&resolved);
        let out = match std::fs::symlink_metadata(&host) {
            Ok(m) => wire::GuestStatResult {
                exists: true,
                kind: if m.is_dir() {
                    "dir".into()
                } else if m.is_file() {
                    "file".into()
                } else if m.file_type().is_symlink() {
                    "symlink".into()
                } else {
                    "other".into()
                },
                size: m.len(),
                modified_ms: modified_ms(Some(&m)),
                mode: mode_of(&m),
            },
            Err(_) => wire::GuestStatResult {
                exists: false,
                kind: String::new(),
                size: 0,
                modified_ms: 0,
                mode: 0,
            },
        };
        reply(id, guest_reply::Body::Stat(out))
    }

    fn mkdir(&self, id: &str, m: &wire::GuestMkdir) -> wire::GuestReply {
        let resolved = self.resolved_guest_path(&m.path);
        if let Err((code, msg)) = write_allowed(&self.policy, &resolved) {
            return refusal(id, code, msg);
        }
        match std::fs::create_dir_all(self.host_path(&resolved)) {
            Ok(()) => reply(
                id,
                guest_reply::Body::FsDone(wire::GuestFsDone { path: resolved }),
            ),
            Err(e) => refusal(id, "BAD_CALL", format!("mkdir `{}`: {e}", m.path)),
        }
    }

    fn remove(&self, id: &str, r: &wire::GuestRemove) -> wire::GuestReply {
        let resolved = self.resolved_guest_path(&r.path);
        if let Err((code, msg)) = write_allowed(&self.policy, &resolved) {
            return refusal(id, code, msg);
        }
        if normalize(&resolved) == normalize(&self.policy.workspace_root) {
            return refusal(
                id,
                "PROTECTED_PATH",
                "the workspace root itself is not removable",
            );
        }
        let host = self.host_path(&resolved);
        let res = match std::fs::symlink_metadata(&host) {
            Ok(m) if m.is_dir() && r.recursive => std::fs::remove_dir_all(&host),
            Ok(m) if m.is_dir() => std::fs::remove_dir(&host),
            Ok(_) => std::fs::remove_file(&host),
            Err(e) => Err(e),
        };
        match res {
            Ok(()) => reply(
                id,
                guest_reply::Body::FsDone(wire::GuestFsDone { path: resolved }),
            ),
            Err(e) => refusal(id, "BAD_CALL", format!("remove `{}`: {e}", r.path)),
        }
    }

    fn rename(&self, id: &str, r: &wire::GuestRename) -> wire::GuestReply {
        let from = self.resolved_guest_path(&r.from);
        let to = self.resolved_guest_path(&r.to);
        for p in [&from, &to] {
            if let Err((code, msg)) = write_allowed(&self.policy, p) {
                return refusal(id, code, msg);
            }
        }
        match std::fs::rename(self.host_path(&from), self.host_path(&to)) {
            Ok(()) => reply(
                id,
                guest_reply::Body::FsDone(wire::GuestFsDone { path: to }),
            ),
            Err(e) => refusal(id, "BAD_CALL", format!("rename `{}`: {e}", r.from)),
        }
    }
}

/// Bind-mount every existing protected path under the workspace onto
/// itself read-only (Linux; a no-op elsewhere, and where a mount fails the
/// guest says so on its console and keeps enforcing in its own calls).
#[cfg(target_os = "linux")]
fn enforce_protected_mounts(policy: &wire::GuestPolicy) {
    use nix::mount::{MsFlags, mount};
    let ws = policy.workspace_root.trim_end_matches('/');
    for p in &policy.protected_paths {
        if !p.starts_with(&format!("{ws}/")) || !std::path::Path::new(p).exists() {
            continue;
        }
        let bind = mount(
            Some(p.as_str()),
            p.as_str(),
            None::<&str>,
            MsFlags::MS_BIND,
            None::<&str>,
        );
        let ro = bind.and_then(|()| {
            mount(
                None::<&str>,
                p.as_str(),
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY,
                None::<&str>,
            )
        });
        match ro {
            Ok(()) => eprintln!("modbit-guest: protected path {p} pinned read-only"),
            Err(e) => eprintln!("modbit-guest: pinning {p} read-only: {e}"),
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn enforce_protected_mounts(_policy: &wire::GuestPolicy) {}

fn modified_ms(meta: Option<&std::fs::Metadata>) -> i64 {
    meta.and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(unix)]
fn mode_of(m: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    m.permissions().mode()
}

#[cfg(not(unix))]
fn mode_of(m: &std::fs::Metadata) -> u32 {
    if m.permissions().readonly() {
        0o444
    } else {
        0o666
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
