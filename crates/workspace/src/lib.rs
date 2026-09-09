//! `modbit-workspace` — canonical file service and revisioning (docs/20
//! "Canonical workspace", "Workspace File Service"; docs/23 "Protected paths").
//!
//! Canonical owner: workspace-git; THE doc 81 `workspace/change engine`.
//!
//! - Filesystem + Git revision are authoritative; there is no editor buffer.
//! - Every path is normalized, then resolved through symlinks, then checked
//!   against the allowed root and protected patterns, before any open — for
//!   reads and writes alike (docs/23: after symlink resolution, before each
//!   write/open, not only at task creation).
//! - Writes are typed operations with optimistic preconditions (expected file
//!   hash and/or expected workspace revision); a stale precondition is a typed
//!   error and nothing is written. Replacements are atomic (temp + fsync +
//!   rename).
//! - Every successful write advances a persisted, monotonic `WorkspaceRevision`
//!   bound to the Git HEAD, the worktree identity and a content fingerprint of
//!   the files changed since the last Git state, and yields a typed
//!   `WorkspaceChange` record (the basis of REQ-EV-0106 change events).

#![forbid(unsafe_code)]

pub mod paths;
pub mod revision;
pub mod service;

pub use paths::{PathPolicy, ResolvedPath};
pub use revision::WorkspaceRevision;
pub use service::{
    ApplyPatch, Edit, EntryKind, FileRead, FileStat, WorkspaceChange, WorkspaceService,
    WritePrecondition,
};

/// Errors of the file service.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O failure.
    #[error("io on {path}: {source}")]
    Io {
        /// Path involved.
        path: String,
        /// Cause.
        #[source]
        source: std::io::Error,
    },
    /// The path escapes the workspace root (after normalization or symlink resolution).
    #[error("path `{path}` resolves outside the workspace root ({detail})")]
    OutsideRoot {
        /// Offending path as given.
        path: String,
        /// Why.
        detail: String,
    },
    /// The path matches a protected pattern.
    #[error("path `{path}` is protected by `{pattern}` (docs/23); access denied")]
    Protected {
        /// Path.
        path: String,
        /// Matching pattern.
        pattern: String,
    },
    /// Optimistic precondition failed.
    #[error("precondition failed on `{path}`: {detail}")]
    Precondition {
        /// Path.
        path: String,
        /// Detail.
        detail: String,
    },
    /// The operation is invalid for the target.
    #[error("invalid operation on `{path}`: {detail}")]
    Invalid {
        /// Path.
        path: String,
        /// Detail.
        detail: String,
    },
    /// Edit ranges do not fit the content.
    #[error("edit out of bounds on `{path}`: {detail}")]
    EditOutOfBounds {
        /// Path.
        path: String,
        /// Detail.
        detail: String,
    },
}

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;
