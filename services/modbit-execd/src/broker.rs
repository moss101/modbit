//! Broker: sessions, process/PTY lifecycle, durable output log, replay.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use modbit_protocol::client::BoxedStream;
use modbit_protocol::framing::{FrameError, read_message, write_message};
use modbit_protocol::local::{Endpoint, ReadyLine, encode_hex};
use modbit_protocol::v1::exec_frame::Body;
use modbit_protocol::v1::{
    Attach, Cancel, ExecError, ExecFrame, ExecRequest, ExecStarted, HelloAck, OutputChunk,
    ProcessExited, SessionInfo, SessionList, WriteStdin,
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, mpsc, watch};

/// Largest output frame sent to a client (REQ-EV-0108).
const CHUNK: usize = 64 * 1024;

/// Index entry: 8-byte data offset, 1-byte stream tag, 4-byte length. The
/// data log holds raw output bytes only, so a client cursor is a pure byte
/// offset and `cursor + data.len()` is the next cursor.
const TAG_STDOUT: u8 = 1;
const TAG_STDERR: u8 = 2;
const TAG_PTY: u8 = 3;

fn tag_name(t: u8) -> &'static str {
    match t {
        TAG_STDOUT => "stdout",
        TAG_STDERR => "stderr",
        _ => "pty",
    }
}

/// What can write to a running process.
enum Stdin {
    Pipe(tokio::process::ChildStdin),
    Pty(Box<dyn Write + Send>),
    Closed,
}

/// How to kill a running process.
enum Killer {
    Child(Arc<Mutex<Option<tokio::process::Child>>>),
    Pty(Arc<std::sync::Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>>),
}

struct Session {
    id: String,
    request_id: String,
    argv: Vec<String>,
    log_path: PathBuf,
    /// Bytes written to the log (cursor high-water mark); watchers wake on change.
    written: watch::Sender<u64>,
    running: AtomicBool,
    exited: Mutex<Option<ProcessExited>>,
    stdin: Mutex<Stdin>,
    killer: Mutex<Option<Killer>>,
    cancelled: AtomicBool,
    data_bytes: AtomicU64,
    hasher: std::sync::Mutex<Sha256>,
    log: std::sync::Mutex<(std::fs::File, std::fs::File)>,
    index_path: PathBuf,
}

impl Session {
    /// Append one record; returns the new data high-water mark.
    fn append(&self, tag: u8, data: &[u8]) -> u64 {
        let mut guard = self.log.lock().expect("log");
        let (log, index) = &mut *guard;
        let offset = *self.written.borrow();
        let _ = log.write_all(data);
        let _ = log.flush();
        let mut entry = Vec::with_capacity(13);
        entry.extend_from_slice(&offset.to_be_bytes());
        entry.push(tag);
        entry.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let _ = index.write_all(&entry);
        let _ = index.flush();
        self.hasher.lock().expect("hasher").update(data);
        self.data_bytes
            .fetch_add(data.len() as u64, Ordering::SeqCst);
        let new = offset + data.len() as u64;
        self.written.send_replace(new);
        new
    }
}

struct Broker {
    data_dir: PathBuf,
    boot_secret: Vec<u8>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    by_request: Mutex<HashMap<String, String>>,
}

/// Run until the listener fails or Ctrl-C.
pub async fn run(data_dir: PathBuf) -> Result<()> {
    std::fs::create_dir_all(data_dir.join("sessions"))?;
    std::fs::create_dir_all(data_dir.join("objects"))?;
    let boot_secret: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
    let nonce = encode_hex(&(0..6).map(|_| rand::random::<u8>()).collect::<Vec<_>>());
    let endpoint =
        Endpoint::for_dir(&data_dir.join("execd"), &nonce).context("choosing endpoint")?;
    let broker = Arc::new(Broker {
        data_dir,
        boot_secret: boot_secret.clone(),
        sessions: Mutex::new(HashMap::new()),
        by_request: Mutex::new(HashMap::new()),
    });
    let listener = Listener::bind(&endpoint)
        .await
        .context("binding endpoint")?;
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
    std::io::stdout().flush().ok();
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let stream = accepted.context("accept")?;
                let b = Arc::clone(&broker);
                tokio::spawn(async move {
                    if let Err(e) = serve(b, stream).await {
                        eprintln!("modbit-execd: connection ended: {e}");
                    }
                });
            }
            _ = &mut shutdown => break,
        }
    }
    listener.cleanup();
    Ok(())
}

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

fn err_frame(request_id: &str, code: &str, message: impl Into<String>) -> ExecFrame {
    ExecFrame {
        body: Some(Body::Error(ExecError {
            request_id: request_id.into(),
            code: code.into(),
            message: message.into(),
        })),
    }
}

async fn serve(broker: Arc<Broker>, mut stream: BoxedStream) -> Result<()> {
    // Handshake: boot secret, constant-time compare.
    let first = match read_message::<_, ExecFrame>(&mut stream).await {
        Ok(Some(f)) => f,
        Ok(None) => return Ok(()),
        Err(e) => {
            let _ = write_message(
                &mut stream,
                &err_frame("", "MALFORMED_FRAME", e.to_string()),
            )
            .await;
            return Ok(());
        }
    };
    let Some(Body::ClientHello(hello)) = first.body else {
        let _ = write_message(
            &mut stream,
            &err_frame("", "HANDSHAKE_REQUIRED", "first frame must be ClientHello"),
        )
        .await;
        return Ok(());
    };
    let presented = hello
        .auth
        .as_ref()
        .map(|a| a.boot_secret.as_slice())
        .unwrap_or(&[]);
    if presented.len() != broker.boot_secret.len()
        || !bool::from(presented.ct_eq(&broker.boot_secret))
    {
        let _ = write_message(
            &mut stream,
            &err_frame("", "UNAUTHENTICATED", "boot secret rejected"),
        )
        .await;
        return Ok(());
    }
    let ours = modbit_protocol::PROTOCOL_VERSION;
    let compatible = hello
        .hello
        .as_ref()
        .and_then(|h| h.protocol_version)
        .is_some_and(|v| v.major == ours.major);
    write_message(
        &mut stream,
        &ExecFrame {
            body: Some(Body::HelloAck(HelloAck {
                protocol_version: Some(ours),
                compatible,
                upgrade_required: !compatible,
                reason: if compatible {
                    String::new()
                } else {
                    "protocol major mismatch".into()
                },
                supported_command_types: vec![],
            })),
        },
    )
    .await?;
    if !compatible {
        return Ok(());
    }

    // One bounded outbound queue per connection; forwarders push, one task writes.
    let (tx, mut rx) = mpsc::channel::<ExecFrame>(256);
    let (mut reader, mut writer) = tokio::io::split(stream);
    let writer_task = tokio::spawn(async move {
        while let Some(f) = rx.recv().await {
            if write_message(&mut writer, &f).await.is_err() {
                break;
            }
        }
    });
    loop {
        let frame = match read_message::<_, ExecFrame>(&mut reader).await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(FrameError::Io(_)) => break,
            Err(e) => {
                let _ = tx
                    .send(err_frame("", "MALFORMED_FRAME", e.to_string()))
                    .await;
                break;
            }
        };
        match frame.body {
            Some(Body::Exec(req)) => {
                let rid = req.request_id.clone();
                match broker.start(req).await {
                    Ok((session, replayed)) => {
                        let _ = tx
                            .send(ExecFrame {
                                body: Some(Body::Started(ExecStarted {
                                    request_id: rid,
                                    session_id: session.id.clone(),
                                    process_id: 0,
                                    replayed,
                                })),
                            })
                            .await;
                        spawn_forwarder(Arc::clone(&session), 0, tx.clone());
                    }
                    Err(e) => {
                        let _ = tx.send(err_frame(&rid, "EXEC_FAILED", e.to_string())).await;
                    }
                }
            }
            Some(Body::Attach(Attach {
                session_id,
                after_cursor,
            })) => match broker.sessions.lock().await.get(&session_id).cloned() {
                Some(s) => spawn_forwarder(s, after_cursor, tx.clone()),
                None => {
                    let _ = tx.send(err_frame("", "UNKNOWN_SESSION", session_id)).await;
                }
            },
            Some(Body::Stdin(WriteStdin { session_id, data })) => {
                if let Some(s) = broker.sessions.lock().await.get(&session_id).cloned() {
                    let mut guard = s.stdin.lock().await;
                    let result = match &mut *guard {
                        Stdin::Pipe(w) => {
                            use tokio::io::AsyncWriteExt;
                            w.write_all(&data)
                                .await
                                .and(w.flush().await)
                                .map_err(|e| e.to_string())
                        }
                        Stdin::Pty(w) => w
                            .write_all(&data)
                            .and_then(|()| w.flush())
                            .map_err(|e| e.to_string()),
                        Stdin::Closed => Err("stdin is closed".into()),
                    };
                    if let Err(e) = result {
                        let _ = tx.send(err_frame(&session_id, "STDIN_FAILED", e)).await;
                    }
                } else {
                    let _ = tx.send(err_frame("", "UNKNOWN_SESSION", session_id)).await;
                }
            }
            Some(Body::Cancel(Cancel { session_id })) => {
                if let Some(s) = broker.sessions.lock().await.get(&session_id).cloned() {
                    s.cancelled.store(true, Ordering::SeqCst);
                    kill(&s).await;
                } else {
                    let _ = tx.send(err_frame("", "UNKNOWN_SESSION", session_id)).await;
                }
            }
            Some(Body::List(_)) => {
                let sessions = broker.sessions.lock().await;
                let mut list: Vec<SessionInfo> = Vec::new();
                for s in sessions.values() {
                    let exit_code = s.exited.lock().await.as_ref().and_then(|e| e.exit_code);
                    list.push(SessionInfo {
                        session_id: s.id.clone(),
                        request_id: s.request_id.clone(),
                        argv: s.argv.clone(),
                        running: s.running.load(Ordering::SeqCst),
                        bytes_so_far: s.data_bytes.load(Ordering::SeqCst),
                        exit_code,
                    });
                }
                list.sort_by(|a, b| a.session_id.cmp(&b.session_id));
                let _ = tx
                    .send(ExecFrame {
                        body: Some(Body::Sessions(SessionList { sessions: list })),
                    })
                    .await;
            }
            other => {
                let _ = tx
                    .send(err_frame("", "UNEXPECTED_FRAME", format!("{other:?}")))
                    .await;
                break;
            }
        }
    }
    drop(tx);
    let _ = writer_task.await;
    Ok(())
}

async fn kill(s: &Arc<Session>) {
    let mut k = s.killer.lock().await;
    match k.take() {
        Some(Killer::Child(child)) => {
            if let Some(c) = child.lock().await.as_mut() {
                // Windows: kill the whole tree, or a grandchild keeps the output
                // pipes open and the exit is never observed (REQ-EV-0221 cancel).
                #[cfg(windows)]
                if let Some(pid) = c.id() {
                    let _ = tokio::process::Command::new("taskkill")
                        .args(["/T", "/F", "/PID", &pid.to_string()])
                        .output()
                        .await;
                }
                let _ = c.start_kill();
            }
        }
        Some(Killer::Pty(child)) => {
            if let Some(c) = child.lock().expect("pty child").as_mut() {
                let _ = c.kill();
            }
        }
        None => {}
    }
}

/// Replay the log from `after_cursor` then follow live output until exit.
fn spawn_forwarder(s: Arc<Session>, after_cursor: u64, tx: mpsc::Sender<ExecFrame>) {
    tokio::spawn(async move {
        let mut cursor = after_cursor;
        let mut rx = s.written.subscribe();
        loop {
            let high = *rx.borrow_and_update();
            if cursor < high {
                match read_records(&s.log_path, &s.index_path, cursor, high) {
                    Ok(records) => {
                        for (rec_cursor, tag, data) in records {
                            for (i, piece) in data.chunks(CHUNK).enumerate() {
                                let frame = ExecFrame {
                                    body: Some(Body::Output(OutputChunk {
                                        session_id: s.id.clone(),
                                        cursor: rec_cursor + (i * CHUNK) as u64,
                                        stream: tag_name(tag).into(),
                                        data: piece.to_vec(),
                                    })),
                                };
                                if tx.send(frame).await.is_err() {
                                    return;
                                }
                            }
                        }
                        cursor = high;
                    }
                    Err(_) => return,
                }
            }
            if !s.running.load(Ordering::SeqCst) && cursor >= *s.written.borrow() {
                if let Some(e) = s.exited.lock().await.clone() {
                    let _ = tx
                        .send(ExecFrame {
                            body: Some(Body::Exited(e)),
                        })
                        .await;
                }
                return;
            }
            if rx.changed().await.is_err() {
                return;
            }
        }
    });
}

/// Records overlapping `[from, to)` of the data log, clipped to the range:
/// (data cursor, tag, data).
fn read_records(
    log_path: &Path,
    index_path: &Path,
    from: u64,
    to: u64,
) -> std::io::Result<Vec<(u64, u8, Vec<u8>)>> {
    use std::io::{Seek, SeekFrom};
    let index = std::fs::read(index_path)?;
    let mut f = std::fs::File::open(log_path)?;
    let mut out = Vec::new();
    for e in index.chunks_exact(13) {
        let offset = u64::from_be_bytes(e[..8].try_into().expect("8 bytes"));
        let tag = e[8];
        let len = u32::from_be_bytes(e[9..13].try_into().expect("4 bytes")) as u64;
        let end = offset + len;
        if end <= from || offset >= to {
            continue;
        }
        let start = offset.max(from);
        let stop = end.min(to);
        f.seek(SeekFrom::Start(start))?;
        let mut buf = vec![0u8; (stop - start) as usize];
        f.read_exact(&mut buf)?;
        out.push((start, tag, buf));
    }
    Ok(out)
}

impl Broker {
    /// Start (or replay) a request.
    async fn start(self: &Arc<Self>, req: ExecRequest) -> Result<(Arc<Session>, bool)> {
        if req.request_id.is_empty() {
            anyhow::bail!("request_id is required");
        }
        if let Some(id) = self.by_request.lock().await.get(&req.request_id).cloned()
            && let Some(s) = self.sessions.lock().await.get(&id).cloned()
        {
            return Ok((s, true));
        }
        if req.argv.is_empty() {
            anyhow::bail!("argv is required");
        }
        let cwd = if req.cwd.is_empty() {
            std::env::current_dir()?
        } else {
            PathBuf::from(&req.cwd)
        };
        if !cwd.is_dir() {
            anyhow::bail!("cwd `{}` is not a directory", cwd.display());
        }
        let id = encode_hex(&(0..8).map(|_| rand::random::<u8>()).collect::<Vec<_>>());
        let log_path = self.data_dir.join("sessions").join(format!("{id}.log"));
        let index_path = self.data_dir.join("sessions").join(format!("{id}.idx"));
        let log = std::fs::File::create(&log_path)?;
        let index = std::fs::File::create(&index_path)?;
        let (written, _) = watch::channel(0u64);
        let session = Arc::new(Session {
            id: id.clone(),
            request_id: req.request_id.clone(),
            argv: req.argv.clone(),
            log_path,
            written,
            running: AtomicBool::new(true),
            exited: Mutex::new(None),
            stdin: Mutex::new(Stdin::Closed),
            killer: Mutex::new(None),
            cancelled: AtomicBool::new(false),
            data_bytes: AtomicU64::new(0),
            hasher: std::sync::Mutex::new(Sha256::new()),
            log: std::sync::Mutex::new((log, index)),
            index_path,
        });
        self.sessions
            .lock()
            .await
            .insert(id.clone(), Arc::clone(&session));
        self.by_request
            .lock()
            .await
            .insert(req.request_id.clone(), id.clone());
        let started = Instant::now();
        let stdin_open = req.stdin_mode == "open";
        if req.pty {
            self.spawn_pty(Arc::clone(&session), &req, &cwd, started)
                .await?;
        } else {
            self.spawn_piped(Arc::clone(&session), &req, &cwd, stdin_open, started)
                .await?;
        }
        Ok((session, false))
    }

    async fn spawn_piped(
        self: &Arc<Self>,
        s: Arc<Session>,
        req: &ExecRequest,
        cwd: &Path,
        stdin_open: bool,
        started: Instant,
    ) -> Result<()> {
        let mut cmd = tokio::process::Command::new(&req.argv[0]);
        cmd.args(&req.argv[1..])
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        cmd.stdin(if stdin_open {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        });
        if !req.inherit_env {
            cmd.env_clear();
            // A minimal PATH keeps argv resolvable when the caller gave none.
            if !req.env.contains_key("PATH")
                && let Ok(p) = std::env::var("PATH")
            {
                cmd.env("PATH", p);
            }
            #[cfg(windows)]
            for k in ["SYSTEMROOT", "SystemRoot", "COMSPEC", "TEMP", "TMP"] {
                if let Ok(v) = std::env::var(k) {
                    cmd.env(k, v);
                }
            }
        }
        cmd.envs(&req.env);
        cmd.kill_on_drop(true);
        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawning {:?}", req.argv))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        if let Some(stdin) = child.stdin.take() {
            *s.stdin.lock().await = Stdin::Pipe(stdin);
        }
        let child = Arc::new(Mutex::new(Some(child)));
        *s.killer.lock().await = Some(Killer::Child(Arc::clone(&child)));
        let pump = |s: Arc<Session>,
                    tag: u8,
                    mut r: Option<tokio::process::ChildStdout>,
                    mut e: Option<tokio::process::ChildStderr>| async move {
            use tokio::io::AsyncReadExt;
            let mut buf = vec![0u8; CHUNK];
            loop {
                let n = match (r.as_mut(), e.as_mut()) {
                    (Some(r), _) => r.read(&mut buf).await,
                    (_, Some(e)) => e.read(&mut buf).await,
                    _ => Ok(0),
                };
                match n {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        s.append(tag, &buf[..n]);
                    }
                }
            }
        };
        let p1 = tokio::spawn(pump(Arc::clone(&s), TAG_STDOUT, stdout, None));
        let p2 = tokio::spawn(pump(Arc::clone(&s), TAG_STDERR, None, stderr));
        let timeout = req.timeout_ms;
        let broker = Arc::clone(self);
        tokio::spawn(async move {
            let mut timed_out = false;
            // Poll without holding the child lock across the wait, so Cancel
            // can take the lock and kill the process at any time.
            let wait = async {
                loop {
                    let done = child
                        .lock()
                        .await
                        .as_mut()
                        .and_then(|c| c.try_wait().ok().flatten());
                    if let Some(st) = done {
                        return Some(st);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                }
            };
            let status = if timeout > 0 {
                match tokio::time::timeout(std::time::Duration::from_millis(timeout), wait).await {
                    Ok(st) => st,
                    Err(_) => {
                        timed_out = true;
                        if let Some(c) = child.lock().await.as_mut() {
                            let _ = c.kill().await;
                        }
                        None
                    }
                }
            } else {
                wait.await
            };
            // After a kill (cancel or timeout) a surviving grandchild may hold the
            // pipes: bound the reader wait so the exit is still reported.
            let abrupt = timed_out || s.cancelled.load(Ordering::SeqCst);
            if abrupt {
                let grace = std::time::Duration::from_secs(2);
                let _ = tokio::time::timeout(grace, p1).await;
                let _ = tokio::time::timeout(grace, p2).await;
            } else {
                let _ = p1.await;
                let _ = p2.await;
            }
            #[cfg(unix)]
            let signal = {
                use std::os::unix::process::ExitStatusExt;
                status.as_ref().and_then(|st| st.signal())
            };
            #[cfg(not(unix))]
            let signal: Option<i32> = None;
            broker
                .finish(
                    &s,
                    status.and_then(|st| st.code()),
                    signal,
                    timed_out,
                    started,
                )
                .await;
        });
        Ok(())
    }

    async fn spawn_pty(
        self: &Arc<Self>,
        s: Arc<Session>,
        req: &ExecRequest,
        cwd: &Path,
        started: Instant,
    ) -> Result<()> {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow::anyhow!("openpty: {e}"))?;
        let mut cmd = CommandBuilder::new(&req.argv[0]);
        cmd.args(&req.argv[1..]);
        cmd.cwd(cwd);
        if !req.inherit_env {
            cmd.env_clear();
            if !req.env.contains_key("PATH")
                && let Ok(p) = std::env::var("PATH")
            {
                cmd.env("PATH", p);
            }
            #[cfg(windows)]
            for k in ["SYSTEMROOT", "SystemRoot", "COMSPEC", "TEMP", "TMP"] {
                if let Ok(v) = std::env::var(k) {
                    cmd.env(k, v);
                }
            }
        }
        for (k, v) in &req.env {
            cmd.env(k, v);
        }
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| anyhow::anyhow!("spawn pty: {e}"))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| anyhow::anyhow!("pty reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| anyhow::anyhow!("pty writer: {e}"))?;
        *s.stdin.lock().await = Stdin::Pty(writer);
        let child = Arc::new(std::sync::Mutex::new(Some(child)));
        *s.killer.lock().await = Some(Killer::Pty(Arc::clone(&child)));
        let s_read = Arc::clone(&s);
        let reader_task = tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; CHUNK];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        s_read.append(TAG_PTY, &buf[..n]);
                    }
                }
            }
        });
        let master = pair.master;
        let timeout = req.timeout_ms;
        let broker = Arc::clone(self);
        tokio::spawn(async move {
            let child_wait = Arc::clone(&child);
            let wait = tokio::task::spawn_blocking(move || {
                loop {
                    let done = child_wait
                        .lock()
                        .expect("pty child")
                        .as_mut()
                        .and_then(|c| c.try_wait().ok().flatten());
                    if let Some(st) = done {
                        return Some(st);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
            });
            let mut timed_out = false;
            let status = if timeout > 0 {
                match tokio::time::timeout(std::time::Duration::from_millis(timeout), wait).await {
                    Ok(st) => st.ok().flatten(),
                    Err(_) => {
                        timed_out = true;
                        if let Some(c) = child.lock().expect("pty child").as_mut() {
                            let _ = c.kill();
                        }
                        None
                    }
                }
            } else {
                wait.await.ok().flatten()
            };
            drop(master);
            let _ = reader_task.await;
            let code = status.map(|st| st.exit_code() as i32);
            broker.finish(&s, code, None, timed_out, started).await;
        });
        Ok(())
    }

    /// Finalize: spill the complete output to a content-addressed object and
    /// record the exit.
    async fn finish(
        &self,
        s: &Arc<Session>,
        exit_code: Option<i32>,
        signal: Option<i32>,
        timed_out: bool,
        started: Instant,
    ) {
        let hash = {
            let h = std::mem::take(&mut *s.hasher.lock().expect("hasher"));
            hex::encode(h.finalize())
        };
        let total = s.data_bytes.load(Ordering::SeqCst);
        // The data log is already raw output: publish it under objects/<2>/<rest>, idempotent.
        let obj = self
            .data_dir
            .join("objects")
            .join(&hash[..2])
            .join(&hash[2..]);
        if !obj.exists() {
            let _ = std::fs::create_dir_all(obj.parent().expect("parent"));
            let tmp = obj.with_extension("tmp");
            if std::fs::copy(&s.log_path, &tmp).is_ok() {
                let _ = std::fs::rename(&tmp, &obj);
            }
        }
        let exited = ProcessExited {
            session_id: s.id.clone(),
            exit_code,
            signal,
            duration_ms: started.elapsed().as_millis() as u64,
            output_ref: hash,
            total_bytes: total,
            timed_out,
            cancelled: s.cancelled.load(Ordering::SeqCst),
        };
        *s.exited.lock().await = Some(exited);
        *s.stdin.lock().await = Stdin::Closed;
        s.running.store(false, Ordering::SeqCst);
        // Wake forwarders so they deliver the exit.
        s.written.send_modify(|_| {});
    }
}
