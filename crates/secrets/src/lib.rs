//! `modbit-secrets` — credential handles, broker interfaces and the one
//! redactor.
//!
//! Canonical owner: effects-security (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! [`redact`] (REQ-EV-0017, docs/23 "Secrets") is the only place a secret is
//! recognised and replaced: the provider gateway, the forge and external
//! tool clients, the Core's error channels and the Cloud API all use it.

pub mod redact;

pub use redact::{MIN_HELD_LEN, REDACTED, Redacted, Redactor, error_text, shape_of};
