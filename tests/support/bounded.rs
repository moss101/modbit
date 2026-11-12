//! A subprocess a test waits for within a bound (`#[path]`-included, like
//! `program_wait.rs`).
//!
//! `Command::output()` waits forever: a CLI that never returns turns a test
//! into a CI job that dies at its timeout with nothing to read. Here the wait
//! has a deadline, and a process that outlives it is killed and handed back
//! with what it wrote, so the caller can fail with an explanation instead of
//! hanging.
#![allow(dead_code)]

use std::io::Read;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A process that did not finish within its bound.
#[derive(Debug)]
pub struct TimedOut {
    /// The command line (environment values that look like credentials are
    /// redacted).
    pub command: String,
    /// The bound it outlived.
    pub limit: Duration,
    /// `Some` when the process itself exited but its stdout or stderr stayed
    /// open past the bound: another process inherited them (a descendant, or
    /// one spawned while they were being created; see [`SPAWN_LOCK`]).
    pub exited: Option<ExitStatus>,
    /// What it wrote to stdout before it was killed.
    pub stdout: String,
    /// What it wrote to stderr before it was killed.
    pub stderr: String,
}

impl std::fmt::Display for TimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.exited {
            None => writeln!(
                f,
                "still running after {:?}, killed: {}",
                self.limit, self.command
            )?,
            Some(status) => writeln!(
                f,
                "exited ({status}) but its stdout/stderr stayed open for {:?} (another process inherited them: a descendant, or one spawned while they were created): {}",
                self.limit, self.command
            )?,
        }
        writeln!(f, "--- stdout ---\n{}", self.stdout.trim_end())?;
        write!(f, "--- stderr ---\n{}", self.stderr.trim_end())
    }
}

/// `cmd`'s program, arguments and the environment it sets, the way a shell
/// would read them; values of variables named like a credential are redacted.
pub fn command_line(cmd: &Command) -> String {
    let mut line = String::new();
    for (k, v) in cmd.get_envs() {
        let k = k.to_string_lossy();
        let secret = ["KEY", "SECRET", "TOKEN", "PASSWORD"]
            .iter()
            .any(|s| k.to_ascii_uppercase().contains(s));
        let v = match v {
            Some(v) if secret && !v.is_empty() => "<redacted>".to_owned(),
            Some(v) => format!("{:?}", v.to_string_lossy()),
            None => "<removed>".to_owned(),
        };
        line.push_str(&format!("{k}={v} "));
    }
    line.push_str(&format!("{:?}", cmd.get_program().to_string_lossy()));
    for a in cmd.get_args() {
        line.push_str(&format!(" {:?}", a.to_string_lossy()));
    }
    line
}

fn drain(mut pipe: impl Read + Send + 'static, done: mpsc::Sender<()>) -> Arc<Mutex<Vec<u8>>> {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buf);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink.lock().unwrap().extend_from_slice(&chunk[..n]),
            }
        }
        let _ = done.send(());
    });
    buf
}

fn take(buf: &Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    std::mem::take(&mut *buf.lock().unwrap())
}

/// Every spawn in a test binary that uses this module, one at a time.
///
/// macOS has no `pipe2`: std creates a child's stdout/stderr pipe with
/// `pipe()` and only then sets `FD_CLOEXEC` on each end. A process another
/// thread spawns inside that window inherits the write end without the flag,
/// and holds it for as long as it lives — the first child's pipes then never
/// reach EOF after it exits, and a caller reading them to the end waits for
/// the other process instead. In `headless.rs` the other process was the
/// `question answer --wait` CLI: the approver thread's `approval list` waited
/// on it, it waited on the approval the approver never got to resolve (the
/// macOS CI hangs of 2026-11-08). Holding this lock across `spawn()` closes
/// the window: no spawn starts while another is between `pipe()` and
/// `FD_CLOEXEC`. Linux (`pipe2(O_CLOEXEC)`) and Windows (std serialises
/// handle inheritance itself) never had it; the lock costs them nothing.
static SPAWN_LOCK: Mutex<()> = Mutex::new(());

/// `cmd.spawn()` under [`SPAWN_LOCK`]: the way every child of a test binary
/// that includes this module is started, including one it does not wait for.
pub fn spawn_serialized(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    let _one_at_a_time = SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cmd.spawn()
}

/// Run `cmd` to completion with stdin closed and stdout/stderr captured, as
/// `Command::output()` does, but give up after `limit`: the process is then
/// killed and returned as [`TimedOut`] with whatever it had written.
///
/// # Panics
/// When the process cannot be spawned at all.
pub fn output_within(cmd: &mut Command, limit: Duration) -> Result<Output, TimedOut> {
    let deadline = Instant::now() + limit;
    let mut child = spawn_serialized(
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .unwrap_or_else(|e| panic!("spawning {}: {e}", command_line(cmd)));
    let (tx, rx) = mpsc::channel();
    let out = drain(child.stdout.take().expect("piped stdout"), tx.clone());
    let err = drain(child.stderr.take().expect("piped stderr"), tx);
    // Both pipes reach EOF when the process (and anything it handed them to)
    // is gone; that is the common case, with no polling delay.
    let mut open = 2;
    while open > 0 {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(()) => open -= 1,
            Err(_) => break,
        }
    }
    let status = loop {
        if let Some(s) = child.try_wait().expect("try_wait") {
            break Some(s);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if let (Some(status), 0) = (status, open) {
        return Ok(Output {
            status,
            stdout: take(&out),
            stderr: take(&err),
        });
    }
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    // What the process wrote before it died is in the pipes; give the readers
    // a moment for it (another process may hold them open forever).
    let grace = Instant::now() + Duration::from_secs(2);
    while open > 0
        && rx
            .recv_timeout(grace.saturating_duration_since(Instant::now()))
            .is_ok()
    {
        open -= 1;
    }
    Err(TimedOut {
        command: command_line(cmd),
        limit,
        exited: status,
        stdout: String::from_utf8_lossy(&take(&out)).into_owned(),
        stderr: String::from_utf8_lossy(&take(&err)).into_owned(),
    })
}

/// The last `n` lines of a log file, or why there are none.
pub fn file_tail(path: &std::path::Path, n: usize) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();
            let from = lines.len().saturating_sub(n);
            format!(
                "last {} of {} lines of {}:\n{}",
                lines.len() - from,
                lines.len(),
                path.display(),
                lines[from..].join("\n")
            )
        }
        Err(e) => format!("{}: {e}", path.display()),
    }
}

/// The processes whose command line contains `needle` (Unix `ps`; empty on
/// Windows), with their state and age.
pub fn processes_matching(needle: &str) -> String {
    if !cfg!(unix) {
        return String::new();
    }
    match output_within(
        Command::new("ps").args(["-axo", "pid,ppid,stat,etime,command"]),
        Duration::from_secs(10),
    ) {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| l.contains(needle))
            .collect::<Vec<_>>()
            .join("\n"),
        Err(e) => format!("ps: {e}"),
    }
}
