//! `modbit-verification` — the Verification Engine (docs/28 §4, docs/64):
//! derived verification plans, BASELINE/TARGETED/COMPLETION/RERUN stages over
//! real runners, normalized `TestReport`/`CheckResult` with failing-check
//! identity, the flaky-check rerun protocol, regression attribution and the
//! diff invariants DI-1..DI-9.
//!
//! Canonical owner: verification. Commands run through a [`CommandRunner`]
//! port the Core binds to the process broker; raw output is retained through
//! an [`ArtifactSink`].

#![forbid(unsafe_code)]

pub mod adapters;
pub mod engine;
pub mod gate;
pub mod invariants;
pub mod plan;
pub mod report;
pub mod tiers;

pub use adapters::{RawRun, detect, parse, parse_cargo, parse_junit_xml, parse_vitest_json};
pub use engine::{
    ArtifactSink, Attribution, AttributionReport, CommandRunner, Quarantine, VerificationEngine,
    VerificationPolicy, VerificationRun, attribute, attribute_against, environment_digest,
};
pub use gate::{
    AcceptanceGateResult, CheckEvidence, EvidenceItem, EvidenceStatus, GATE_VERSION, GateInput,
    InvariantEvidence, RequiredAssurance, ReviewEvidence, Verdict, VerificationEvidence,
    evaluate as evaluate_gate,
};
pub use invariants::{
    ChangedFile, Class, InvariantContext, Violation, denies, evaluate_diff, evaluate_file,
};
pub use plan::{CheckCommand, VerificationPlan, derive};
pub use report::{
    CheckKind, CheckResult, CheckStatus, Confidence, Counts, Location, ParserInfo, ReportStatus,
    RunnerFamily, RunnerInfo, Stage, TestReport, failure_signature, normalize_message,
};
