//! `modbit-policy` — capability kernel, approvals, protected paths, PolicyEnvelope and RealizedRisk.
//!
//! Canonical owner: effects-security (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! `config` (M1.5) is the typed ConfigurationResolver; `kernel` (M2.5) is the
//! Capability Kernel: leases, the admin PolicyEnvelope, approval binding and
//! the emergency stop. Protected paths live in `modbit-workspace`.

pub mod assurance;
pub mod config;
pub mod kernel;
pub mod ledger;

pub use assurance::{
    Advisory, AssuranceLayer, AssuranceLevel, AssurancePolicy, CandidateFacts, ChangeKind,
    ChangedPath, ProtectedSurface, RealizedRisk, RequestedEffect, RiskLevel, RiskReason,
    SurfaceKind, derive_realized_risk, strengthen_only,
};
pub use config::{Authority, Layer, Permission, Provenance, Resolved, ResolvedConfig, resolve};
pub use kernel::{
    CapabilityKernel, KernelDecision, KernelRequest, PolicyEnvelope, default_lease_for_profile,
};
