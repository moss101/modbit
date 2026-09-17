//! `modbit-sandbox` — the sandbox backend boundary (M8.3; docs/21 "Sandbox
//! substrate boundary", docs/24 "Sandbox Gateway", docs/33 "Sandbox
//! Gateway"; REQ-EV-0285, REQ-EV-0287, REQ-EV-0290, REQ-EV-0291).
//!
//! Canonical owner of `sandbox-gateway-abstraction` (docs/81): the contract
//! between the Modbit-owned Sandbox Gateway and whatever isolates a guest —
//! a [`SandboxSpec`] the gateway compiles into a [`CompiledPolicy`] (what the
//! substrate enforces: no network unless granted, the mounts; what the
//! guest enforces: the writable tree, the protected paths, the bounds), a
//! [`backend::SandboxBackend`] that boots `modbit-guest` under that policy
//! and hands back a private channel, and the [`link::GuestLink`] that speaks
//! the versioned, authenticated guest RPC over it. Nothing substrate-specific
//! leaves this crate: the gateway, the worker, the tools and the domain see
//! a sandbox id, a policy and typed calls (REQ-EV-0291).
//!
//! Two backends live here. The MicroVM backend drives a Firecracker
//! MicroVM (KVM; the kernel and the immutable guest root image are the
//! gateway's, the workspace is a per-sandbox image; the guest is `/init`;
//! the channel is vsock) — the cloud substrate (REQ-EV-0285). The reference
//! backend runs the same guest as a child process on this host over a
//! loopback socket: it implements the whole contract so the conformance
//! suite and development can run anywhere, and it isolates nothing — it
//! says so, and a gateway serves tenants with it only when configured to
//! (`cloud_isolated` never runs on it in production).

pub mod auth;
pub mod backend;
#[cfg(feature = "client")]
pub mod client;
pub mod conformance;
pub mod image;
pub mod link;
pub mod policy;
pub mod port;

pub use policy::{CompiledPolicy, EgressRule, NetworkPolicy, Resources, SandboxSpec};

/// The guest RPC protocol this crate speaks (docs/30 "Version
/// compatibility": guest and gateway negotiate before admission).
pub const GUEST_PROTOCOL_MAJOR: u32 = 1;
/// Minor version.
pub const GUEST_PROTOCOL_MINOR: u32 = 1;

/// The methods a guest must offer to be admitted for a task (1.1 adds the
/// followed processes, PTYs and directory operations of M8.5).
pub const REQUIRED_METHODS: &[&str] = &[
    "health",
    "exec",
    "fs.read",
    "fs.write",
    "net.probe",
    "proc",
    "pty",
    "fs.dir",
];

/// Errors at the boundary.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// The substrate could not provide a sandbox.
    #[error("substrate: {0}")]
    Substrate(String),
    /// The guest did not come up or went away.
    #[error("guest: {0}")]
    Guest(String),
    /// The guest refused (admission: version, methods; a call: policy).
    #[error("refused: {code}: {message}")]
    Refused {
        /// Code.
        code: String,
        /// Detail.
        message: String,
    },
    /// A frame failed authentication or framing.
    #[error("protocol: {0}")]
    Protocol(String),
    /// The backend cannot serve this policy (an egress grant on a backend
    /// without a network path, say).
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// I/O.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, SandboxError>;
