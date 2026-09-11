//! Typed failure diagnostics (REQ-EV-0073 "failures carry class,
//! retryability, user action, evidence and recovery path"; REQ-EV-0245
//! "the adaptive evaluator receives a typed failure taxonomy, not a raw
//! guess"). The record; the classifier lives in `modbit-core-runtime`.

use serde::{Deserialize, Serialize};

/// The class of a failure: what kind of thing went wrong, independent of
/// the words the source used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureClass {
    /// A bounded operation ran out of time.
    Timeout,
    /// The transport, broker, store or host could not do its part.
    Infrastructure,
    /// The application (a command, a test, a tool's own logic) reported
    /// failure; the operation itself ran.
    Application,
    /// Policy refused it before any effect.
    Policy,
    /// An approval is needed, or was denied.
    Approval,
    /// Stored state does not match its digest or its record.
    CorruptState,
    /// An effect may or may not have happened; reconcile before retrying.
    UnknownOutcome,
    /// The session lease was superseded.
    Lease,
    /// The model provider failed or refused.
    Provider,
    /// A harness budget was exhausted.
    Budget,
    /// A harness rule refused the action (plan, retrieval, scope, repair).
    Harness,
    /// Cancelled by the user or the run.
    Cancelled,
    /// The input itself was invalid (schema, unknown tool).
    Invalid,
}

impl FailureClass {
    /// Stable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Timeout => "TIMEOUT",
            Self::Infrastructure => "INFRASTRUCTURE",
            Self::Application => "APPLICATION",
            Self::Policy => "POLICY",
            Self::Approval => "APPROVAL",
            Self::CorruptState => "CORRUPT_STATE",
            Self::UnknownOutcome => "UNKNOWN_OUTCOME",
            Self::Lease => "LEASE",
            Self::Provider => "PROVIDER",
            Self::Budget => "BUDGET",
            Self::Harness => "HARNESS",
            Self::Cancelled => "CANCELLED",
            Self::Invalid => "INVALID",
        }
    }
}

/// One typed failure: what it is, whether a retry can help, what the user
/// can do, how the system recovers, and the evidence behind it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureDiagnostic {
    /// Class.
    pub class: FailureClass,
    /// The source's own code (`TIMEOUT`, `OBJECT_MISMATCH`, `STALE_LEASE`, ...).
    pub code: String,
    /// Whether the same action may be retried as it was.
    pub retryable: bool,
    /// What the user can do, in words; empty when nothing is asked of them.
    pub user_action: String,
    /// How the system recovers, in words (the command or boundary that
    /// continues).
    pub recovery_path: String,
    /// Object hashes and ids behind the diagnosis.
    pub evidence_refs: Vec<String>,
    /// Stable feature tags an evaluator can key on (`class:timeout`,
    /// `retryable`, `source:tool`, ...), sorted.
    pub features: Vec<String>,
    /// The source's message, verbatim and bounded.
    pub detail: String,
}

impl FailureDiagnostic {
    /// The lines a model or a person reads under a result.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "failure_class: {}\nretryable: {}\nrecovery: {}\n",
            self.class.label(),
            self.retryable,
            self.recovery_path
        );
        if !self.user_action.is_empty() {
            out.push_str(&format!("user_action: {}\n", self.user_action));
        }
        out
    }
}
