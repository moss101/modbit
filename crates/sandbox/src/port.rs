//! The port a Core's tools use to act inside a task's sandbox (M8.5): the
//! typed guest operations, substrate-agnostic. The Core holds one
//! [`SandboxPort`] per task under `cloud_isolated`; the tools that have a
//! sandbox path (`shell.exec`, `test.run`, `fs.read`, `fs.list`, `fs.stat`)
//! route through it instead of the host's workspace and broker. Nothing
//! here names a substrate (REQ-EV-0291).

use std::pin::Pin;

use modbit_protocol::v1 as wire;

use crate::Result;

/// A future a port returns.
pub type PortFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// What a sandbox is, for the record.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SandboxIdentity {
    /// The gateway's id for it.
    pub sandbox_id: String,
    /// `microvm` | `reference`.
    pub backend: String,
    /// Whether the backend isolates.
    pub isolated: bool,
    /// The verified image's guest version, when the backend is verified.
    pub image_version: Option<String>,
    /// The guest's boot id.
    pub boot_id: String,
    /// The workspace root inside the guest.
    pub workspace_root: String,
    /// The egress the broker admits (`host:port`), for the record.
    #[serde(default)]
    pub egress: Vec<String>,
    /// The credentialed virtual hosts, for the record (handles, never secrets).
    #[serde(default)]
    pub credentials: Vec<String>,
    /// Whether the policy grants a browser (M8.8).
    #[serde(default)]
    pub browser: bool,
}

/// The guest's browser as started (M8.8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserEndpoint {
    /// The DevTools port on the guest's loopback (reachable only through
    /// the gateway).
    pub port: u32,
    /// `/devtools/browser/<id>`.
    pub ws_path: String,
    /// Whether it was already running.
    pub already_running: bool,
}

/// A message transport to the browser's DevTools (M8.8): text frames of a
/// WebSocket the gateway relays. Substrate-agnostic; the Core's bridge
/// speaks CDP over it.
pub trait CdpTransport: Send {
    /// Send one text frame.
    fn send(&mut self, text: String) -> PortFuture<'_, Result<()>>;
    /// The next text frame; `None` when the relay closed.
    fn recv(&mut self) -> PortFuture<'_, Option<Result<String>>>;
}

/// A task's sandbox.
pub trait SandboxPort: Send + Sync {
    /// Who this is.
    fn identity(&self) -> &SandboxIdentity;
    /// Run a process to completion.
    fn exec<'a>(
        &'a self,
        task_id: &'a str,
        effect_id: &'a str,
        exec: wire::GuestExec,
    ) -> PortFuture<'a, Result<wire::GuestExecResult>>;
    /// Start a followed process.
    fn proc_start<'a>(
        &'a self,
        task_id: &'a str,
        effect_id: &'a str,
        start: wire::GuestProcStart,
    ) -> PortFuture<'a, Result<wire::GuestProcStarted>>;
    /// Follow a process from a cursor.
    fn proc_follow<'a>(
        &'a self,
        task_id: &'a str,
        proc_id: &'a str,
        after_cursor: u64,
        max_bytes: u64,
        wait_ms: u64,
    ) -> PortFuture<'a, Result<wire::GuestProcOutput>>;
    /// Feed stdin.
    fn proc_write<'a>(
        &'a self,
        task_id: &'a str,
        proc_id: &'a str,
        data: Vec<u8>,
        close_stdin: bool,
    ) -> PortFuture<'a, Result<()>>;
    /// Cancel.
    fn proc_cancel<'a>(&'a self, task_id: &'a str, proc_id: &'a str) -> PortFuture<'a, Result<()>>;
    /// Read a file.
    fn read_file<'a>(
        &'a self,
        task_id: &'a str,
        path: &'a str,
        max_bytes: u64,
    ) -> PortFuture<'a, Result<wire::GuestFileContent>>;
    /// Write a file.
    fn write_file<'a>(
        &'a self,
        task_id: &'a str,
        effect_id: &'a str,
        path: &'a str,
        content: Vec<u8>,
    ) -> PortFuture<'a, Result<wire::GuestFileWritten>>;
    /// List a directory.
    fn list_dir<'a>(
        &'a self,
        task_id: &'a str,
        path: &'a str,
        max_entries: u32,
    ) -> PortFuture<'a, Result<wire::GuestDirListing>>;
    /// Stat a path.
    fn stat<'a>(
        &'a self,
        task_id: &'a str,
        path: &'a str,
    ) -> PortFuture<'a, Result<wire::GuestStatResult>>;
    /// Remove a file (M8.9: a restore's deletions).
    fn remove<'a>(
        &'a self,
        task_id: &'a str,
        effect_id: &'a str,
        path: &'a str,
    ) -> PortFuture<'a, Result<()>>;
    /// Every regular file under `root` (the workspace when empty) with
    /// its size and sha256 (M8.9: what a checkpoint compares).
    fn fs_snapshot<'a>(
        &'a self,
        task_id: &'a str,
        root: &'a str,
    ) -> PortFuture<'a, Result<wire::GuestFsSnapshotResult>>;
    /// Start the guest's browser (M8.8), if the policy grants one.
    fn browser_start<'a>(
        &'a self,
        task_id: &'a str,
        width: u32,
        height: u32,
    ) -> PortFuture<'a, Result<BrowserEndpoint>>;
    /// Stop it.
    fn browser_stop<'a>(&'a self, task_id: &'a str) -> PortFuture<'a, Result<()>>;
    /// A DevTools transport to the running browser, through the gateway.
    fn browser_connect<'a>(&'a self) -> PortFuture<'a, Result<Box<dyn CdpTransport>>>;
}
