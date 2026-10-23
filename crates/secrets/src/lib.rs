//! `modbit-secrets` — credential handles, the credential broker and the one
//! redactor.
//!
//! Canonical owner: effects-security (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! [`broker`] (REQ-PX-130) is the one interface through which every
//! credential the product uses is registered, granted, used, rotated and
//! revoked: scoped, short-lived, audience-bound grants; replay and call-cap
//! protection; a counted, audited use. A value is read only inside the broker
//! and only handed out as a [`Secret`] for the effect.
//!
//! [`redact`] (REQ-EV-0017, docs/23 "Secrets") is the only place a secret is
//! recognised and replaced: the provider gateway, the forge and external
//! tool clients, the Core's error channels and the Cloud API all use it.

pub mod broker;
pub mod redact;

pub use broker::{
    AuditRecord, CredentialBroker, CredentialId, CredentialStatus, GrantHandle, GrantStatus,
    IssueRequest, Kind, Refusal, Registration, Secret, SecretHandle, Stats, Use,
};
pub use redact::{MIN_HELD_LEN, REDACTED, Redacted, Redactor, error_text, shape_of};
