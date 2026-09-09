//! `modbit-tools` — typed tool registry and execution envelopes (docs/16,
//! docs/17). THE doc 81 tool registry.
//!
//! Every invocation passes the canonical pipeline (REQ-EV-0239):
//! `normalize → validate (schema) → policy (Capability Kernel port) → execute
//! (real effector) → postprocess (OutputRef spill) → evidence`. The policy
//! verdict is monotonic: once denied, no later stage can allow. The policy
//! port never reads argument text as instructions, so prompt-injected pleas to
//! "ignore policy" change nothing (REQ-EV-0080).
//!
//! Direct tools shipped here (docs/17 canonical inventory): `fs.list`,
//! `fs.read`, `fs.stat`, `fs.glob`, `change.apply`, `git.status`, `git.diff`,
//! `git.worktree.create`, `git.worktree.close`, `shell.exec`, `shell.attach`,
//! `shell.input`, `shell.cancel`, `test.run`. Each runs against its real
//! substrate crate (`modbit-workspace`, `modbit-git`, `modbit-terminal` →
//! `modbit-execd`).

#![forbid(unsafe_code)]

pub mod direct;
pub mod pipeline;
pub mod policy;
pub mod registry;

pub use modbit_domain::toolcall::EffectClass;
pub use pipeline::{
    InvokeContext, ObjectSink, PipelineOutcome, StageRecord, ToolCallResult, ToolRuntime,
    ToolStatus, arguments_hash,
};
pub use policy::{CapabilityPort, PolicyDecision, PolicyRequest, ProfilePolicy};
pub use registry::{Idempotency, Tool, ToolOutcome, ToolRegistry, ToolSpec};

/// Errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Registration conflict or invalid spec.
    #[error("registry: {0}")]
    Registry(String),
    /// Object sink failure.
    #[error("object sink: {0}")]
    Sink(String),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;
