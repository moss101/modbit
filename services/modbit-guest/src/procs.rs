//! Processes inside the guest (M8.5; docs/21 "`modbit-guest`": process
//! start/wait/cancel, PTY attach/replay). Every process keeps its output
//! in a bounded ring with a cursor; a follower reads after its cursor and
//! learns when bytes fell out of the ring, so a link that was lost is
//! resumed from where it left off, never re-run. A PTY-backed process is
//! the same table entry with one interleaved stream and a resizable
//! window.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;

/// Bytes each process keeps.
pub const RING_BYTES: usize = 4 * 1024 * 1024;
/// Exited processes kept for replay before the oldest is dropped.
const KEEP_EXITED: usize = 64;

struct Ring {
    buf: VecDeque<u8>,
    /// Cursor of `buf[0]`.
    start: u64,
    /// Bytes ever appended.
    total: u64,
}

impl Ring {
    fn append(&mut self, data: &[u8]) {
        for &b in data {
            if self.buf.len() >= RING_BYTES {
                self.buf.pop_front();
                self.start += 1;
            }
            self.buf.push_back(b);
        }
        self.total += data.len() as u64;
    }

    /// Bytes after `cursor`, up to `max`; whether bytes before `cursor` are gone.
    fn read(&self, cursor: u64, max: usize) -> (Vec<u8>, u64, bool) {
        let truncated = cursor < self.start;
        let from = cursor.max(self.start);
        let offset = (from - self.start) as usize;
        let out: Vec<u8> = self.buf.iter().skip(offset).take(max).copied().collect();
        let next = from + out.len() as u64;
        (out, next, truncated)
    }
}

#[derive(Clone, Copy, Default)]
struct State {
    running: bool,
    exit_code: Option<i32>,
    timed_out: bool,
    cancelled: bool,
}

enum Stdin {
    Pipe(tokio::process::ChildStdin),
    Pty(Box<dyn std::io::Write + Send>),
    Closed,
}

enum Killer {
    Pipe(Arc<tokio::sync::Mutex<tokio::process::Child>>),
    Pty(Arc<Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>>),
}

/// One process.
pub struct Proc {
    /// Id.
    pub id: String,
    /// Pid.
    pub pid: u32,
    ring: Mutex<Ring>,
    state: Mutex<State>,
    changed: Notify,
    stdin: tokio::sync::Mutex<Stdin>,
    killer: Mutex<Option<Killer>>,
    pty_master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    started: Instant,
}

impl Proc {
    fn append(&self, data: &[u8]) {
        self.ring.lock().expect("ring").append(data);
        self.changed.notify_waiters();
    }

    fn finish(&self, exit_code: Option<i32>, timed_out: bool, cancelled: bool) {
        let mut st = self.state.lock().expect("state");
        st.running = false;
        st.exit_code = exit_code;
        st.timed_out |= timed_out;
        st.cancelled |= cancelled;
        drop(st);
        self.changed.notify_waiters();
    }

    /// A snapshot of the output after `cursor`.
    #[must_use]
    pub fn follow_now(&self, cursor: u64, max: usize) -> modbit_protocol::v1::GuestProcOutput {
        let (data, next, truncated) = self.ring.lock().expect("ring").read(cursor, max);
        let total = self.ring.lock().expect("ring").total;
        let st = *self.state.lock().expect("state");
        modbit_protocol::v1::GuestProcOutput {
            proc_id: self.id.clone(),
            data,
            cursor: next,
            truncated,
            running: st.running,
            exit_code: st.exit_code,
            timed_out: st.timed_out,
            cancelled: st.cancelled,
            total_bytes: total,
        }
    }

    /// Follow: wait up to `wait` for bytes after `cursor` or the exit.
    pub async fn follow(
        &self,
        cursor: u64,
        max: usize,
        wait: Duration,
    ) -> modbit_protocol::v1::GuestProcOutput {
        let deadline = Instant::now() + wait;
        loop {
            let notified = self.changed.notified();
            let out = self.follow_now(cursor, max);
            if !out.data.is_empty() || !out.running || out.truncated {
                return out;
            }
            let now = Instant::now();
            if now >= deadline {
                return out;
            }
            let _ = tokio::time::timeout(deadline - now, notified).await;
        }
    }

    /// Feed stdin; close it when asked.
    pub async fn write_stdin(&self, data: &[u8], close: bool) -> std::io::Result<()> {
        let mut s = self.stdin.lock().await;
        match &mut *s {
            Stdin::Pipe(w) => {
                w.write_all(data).await?;
                w.flush().await?;
                if close {
                    w.shutdown().await?;
                    *s = Stdin::Closed;
                }
            }
            Stdin::Pty(w) => {
                w.write_all(data)?;
                w.flush()?;
                if close {
                    *s = Stdin::Closed;
                }
            }
            Stdin::Closed => {
                if !data.is_empty() {
                    return Err(std::io::Error::other("stdin is closed"));
                }
            }
        }
        Ok(())
    }

    /// Kill.
    pub async fn cancel(&self) {
        let killer = self.killer.lock().expect("killer").take();
        match killer {
            Some(Killer::Pipe(child)) => {
                let mut c = child.lock().await;
                let _ = c.start_kill();
            }
            Some(Killer::Pty(child)) => {
                if let Some(c) = child.lock().expect("pty child").as_mut() {
                    let _ = c.kill();
                }
            }
            None => {}
        }
        self.finish(None, false, true);
    }

    /// Resize the PTY window.
    pub fn resize(&self, cols: u16, rows: u16) -> std::io::Result<()> {
        match self.pty_master.lock().expect("master").as_ref() {
            Some(m) => m
                .resize(portable_pty::PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| std::io::Error::other(e.to_string())),
            None => Err(std::io::Error::other("not a PTY")),
        }
    }

    /// Whether it still runs.
    #[must_use]
    pub fn running(&self) -> bool {
        self.state.lock().expect("state").running
    }
}

/// The table.
#[derive(Default)]
pub struct ProcTable {
    procs: Mutex<HashMap<String, Arc<Proc>>>,
}

/// What a start needs.
pub struct StartSpec<'a> {
    /// Program and arguments.
    pub argv: &'a [String],
    /// Working directory (host-resolved).
    pub cwd: std::path::PathBuf,
    /// `KEY=VALUE` pairs; nothing else.
    pub env: &'a [String],
    /// Wall-clock ceiling.
    pub timeout: Duration,
    /// PTY.
    pub pty: bool,
    /// Columns and rows for a PTY.
    pub size: (u16, u16),
    /// Keep stdin open for later writes.
    pub stdin_open: bool,
}

impl ProcTable {
    /// Running processes.
    #[must_use]
    pub fn running(&self) -> u32 {
        self.procs
            .lock()
            .expect("procs")
            .values()
            .filter(|p| p.running())
            .count() as u32
    }

    /// Look one up.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Arc<Proc>> {
        self.procs.lock().expect("procs").get(id).cloned()
    }

    fn insert(&self, p: Arc<Proc>) {
        let mut procs = self.procs.lock().expect("procs");
        // Bound the table: drop the oldest exited entries beyond the keep.
        let mut exited: Vec<(Instant, String)> = procs
            .values()
            .filter(|p| !p.running())
            .map(|p| (p.started, p.id.clone()))
            .collect();
        if exited.len() >= KEEP_EXITED {
            exited.sort();
            for (_, id) in exited.into_iter().take(exited_len_over(procs.len())) {
                procs.remove(&id);
            }
        }
        procs.insert(p.id.clone(), p);
    }

    /// Start a process.
    pub async fn start(self: &Arc<Self>, spec: StartSpec<'_>) -> std::io::Result<Arc<Proc>> {
        let id = uuid::Uuid::now_v7().to_string();
        if spec.pty {
            self.start_pty(id, spec)
        } else {
            self.start_piped(id, spec).await
        }
    }

    async fn start_piped(
        self: &Arc<Self>,
        id: String,
        spec: StartSpec<'_>,
    ) -> std::io::Result<Arc<Proc>> {
        let mut cmd = tokio::process::Command::new(&spec.argv[0]);
        cmd.args(&spec.argv[1..]).current_dir(&spec.cwd).env_clear();
        for kv in spec.env {
            if let Some((k, v)) = kv.split_once('=') {
                cmd.env(k, v);
            }
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let child = Arc::new(tokio::sync::Mutex::new(child));
        let stdin_state = match stdin {
            Some(mut w) if !spec.stdin_open => {
                let _ = w.shutdown().await;
                Stdin::Closed
            }
            Some(w) => Stdin::Pipe(w),
            None => Stdin::Closed,
        };
        let p = Arc::new(Proc {
            id: id.clone(),
            pid,
            ring: Mutex::new(Ring {
                buf: VecDeque::new(),
                start: 0,
                total: 0,
            }),
            state: Mutex::new(State {
                running: true,
                ..Default::default()
            }),
            changed: Notify::new(),
            stdin: tokio::sync::Mutex::new(stdin_state),
            killer: Mutex::new(Some(Killer::Pipe(Arc::clone(&child)))),
            pty_master: Mutex::new(None),
            started: Instant::now(),
        });
        self.insert(Arc::clone(&p));
        for mut reader in [
            tokio::io::BufReader::new(
                Box::pin(stdout) as std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>>
            ),
            tokio::io::BufReader::new(
                Box::pin(stderr) as std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>>
            ),
        ] {
            let p = Arc::clone(&p);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => p.append(&buf[..n]),
                    }
                }
            });
        }
        let p_wait = Arc::clone(&p);
        let timeout = spec.timeout;
        tokio::spawn(async move {
            let wait = async {
                let mut c = child.lock().await;
                c.wait().await
            };
            match tokio::time::timeout(timeout, wait).await {
                Ok(Ok(status)) => p_wait.finish(status.code(), false, false),
                Ok(Err(_)) => p_wait.finish(None, false, false),
                Err(_) => {
                    let killer = p_wait.killer.lock().expect("killer").take();
                    if let Some(Killer::Pipe(c)) = killer {
                        let _ = c.lock().await.start_kill();
                    }
                    p_wait.finish(None, true, false);
                }
            }
        });
        Ok(p)
    }

    fn start_pty(self: &Arc<Self>, id: String, spec: StartSpec<'_>) -> std::io::Result<Arc<Proc>> {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: spec.size.1.max(2),
                cols: spec.size.0.max(2),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| std::io::Error::other(format!("openpty: {e}")))?;
        let mut cmd = CommandBuilder::new(&spec.argv[0]);
        cmd.args(&spec.argv[1..]);
        cmd.cwd(&spec.cwd);
        cmd.env_clear();
        for kv in spec.env {
            if let Some((k, v)) = kv.split_once('=') {
                cmd.env(k, v);
            }
        }
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| std::io::Error::other(format!("spawn pty: {e}")))?;
        drop(pair.slave);
        let pid = child.process_id().unwrap_or(0);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| std::io::Error::other(format!("pty reader: {e}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| std::io::Error::other(format!("pty writer: {e}")))?;
        let child = Arc::new(Mutex::new(Some(child)));
        let p = Arc::new(Proc {
            id: id.clone(),
            pid,
            ring: Mutex::new(Ring {
                buf: VecDeque::new(),
                start: 0,
                total: 0,
            }),
            state: Mutex::new(State {
                running: true,
                ..Default::default()
            }),
            changed: Notify::new(),
            stdin: tokio::sync::Mutex::new(Stdin::Pty(writer)),
            killer: Mutex::new(Some(Killer::Pty(Arc::clone(&child)))),
            pty_master: Mutex::new(Some(pair.master)),
            started: Instant::now(),
        });
        self.insert(Arc::clone(&p));
        let p_read = Arc::clone(&p);
        tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => p_read.append(&buf[..n]),
                }
            }
        });
        let p_wait = Arc::clone(&p);
        let timeout = spec.timeout;
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
                        return st;
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
            });
            match tokio::time::timeout(timeout, wait).await {
                Ok(Ok(status)) => p_wait.finish(Some(status.exit_code() as i32), false, false),
                Ok(Err(_)) => p_wait.finish(None, false, false),
                Err(_) => {
                    if let Some(c) = child.lock().expect("pty child").as_mut() {
                        let _ = c.kill();
                    }
                    p_wait.finish(None, true, false);
                }
            }
        });
        Ok(p)
    }
}

fn exited_len_over(total: usize) -> usize {
    total.saturating_sub(KEEP_EXITED) + 1
}
