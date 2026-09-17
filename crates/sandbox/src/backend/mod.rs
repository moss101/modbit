//! The backend boundary (REQ-EV-0291): a [`SandboxBackend`] boots a guest
//! under a [`CompiledPolicy`] and returns a private byte channel to it;
//! everything above (admission, calls, policy) is backend-agnostic. Two
//! implementations: [`microvm::MicrovmBackend`] (Firecracker over KVM — the
//! cloud substrate) and [`reference::ReferenceBackend`] (the guest as a
//! child process on this host — the contract's reference, no isolation).

use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::Result;
use crate::policy::CompiledPolicy;

pub mod reference;

#[cfg(unix)]
pub mod microvm;

/// A byte channel to a guest.
pub trait GuestChannel: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> GuestChannel for T {}

/// The channel, boxed.
pub type Channel = Box<dyn GuestChannel>;

/// A future a backend returns.
pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// A booted guest, not yet admitted.
pub struct Provisioned {
    /// The sandbox.
    pub sandbox_id: String,
    /// The private channel to the guest.
    pub channel: Channel,
    /// `microvm` | `reference`.
    pub backend: &'static str,
    /// Whether this backend isolates the guest from the host (the reference
    /// backend does not, and says so).
    pub isolated: bool,
    /// Backend detail for the audit (the VM's console log path, the child's pid).
    pub detail: String,
    /// Channels the guest opens towards the host for egress (M8.6): one per
    /// client connection of the guest's local proxy; the gateway's broker
    /// consumes them. `None` when the policy grants no egress.
    pub egress: Option<tokio::sync::mpsc::Receiver<Channel>>,
}

/// What isolates a guest.
pub trait SandboxBackend: Send + Sync {
    /// `microvm` | `reference`.
    fn kind(&self) -> &'static str;
    /// Whether guests are isolated from the host and each other.
    fn isolates(&self) -> bool;
    /// The verified image this backend boots (M8.4): the guest that comes
    /// up must be the one it names; `None` for an unverified development
    /// backend.
    fn image(&self) -> Option<&crate::image::ImageManifest> {
        None
    }
    /// Boot a guest for `sandbox_id` under `policy`.
    fn provision<'a>(
        &'a self,
        sandbox_id: &'a str,
        policy: &'a CompiledPolicy,
    ) -> BoxFuture<'a, Result<Provisioned>>;
    /// Tear a guest down; idempotent.
    fn destroy<'a>(&'a self, sandbox_id: &'a str) -> BoxFuture<'a, Result<()>>;
    /// A fresh channel to a guest already booted (a link that was lost is
    /// replaced; the guest, its processes and their output are unchanged
    /// and the new link is admitted anew — M8.5 attach/replay).
    fn reconnect<'a>(&'a self, sandbox_id: &'a str) -> BoxFuture<'a, Result<Channel>>;
}

/// Copy a directory tree (the workspace source into a sandbox's own copy).
pub(crate) fn copy_tree(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<u64> {
    let mut n = 0;
    std::fs::create_dir_all(dst)?;
    if !src.is_dir() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            n += copy_tree(&entry.path(), &to)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &to)?;
            n += 1;
        }
        // Symlinks are not carried into a sandbox.
    }
    Ok(n)
}
