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

impl FailureClass {
    /// The class in a person's words.
    #[must_use]
    pub const fn explained(self) -> &'static str {
        match self {
            Self::Timeout => "An operation ran out of time",
            Self::Infrastructure => "Something the agent depends on could not do its part",
            Self::Application => "A tool or command reported a failure",
            Self::Policy => "Policy refused an action before it had any effect",
            Self::Approval => "An action needs, or was refused, your approval",
            Self::CorruptState => "Stored state failed its integrity check",
            Self::UnknownOutcome => "An action may or may not have taken effect",
            Self::Lease => "Another owner has taken over this work",
            Self::Provider => "The model provider failed or refused the request",
            Self::Budget => "A budget for this run was used up",
            Self::Harness => "A rule of the agent harness refused an action",
            Self::Cancelled => "The work was cancelled",
            Self::Invalid => "A request was not valid",
        }
    }
}

// REQ-EV-0017: one diagnosis, two renderings. The person reads what kind of
// failure it is and what they can do; the model reads the same identity as
// fields it can repair from. Neither carries the source's own words: those
// stay in `detail` as evidence, redacted before the diagnosis is built, and
// the model sees them (redacted) on the result's own `error:` line.
impl FailureDiagnostic {
    /// The lines a model reads under a result: the classic fields, then the
    /// structured repair payload on one line.
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
        out.push_str(&format!("repair: {}\n", self.model_repair()));
        out
    }

    /// The structured repair payload for a model.
    #[must_use]
    pub fn model_repair(&self) -> serde_json::Value {
        serde_json::json!({
            "failure_class": self.class.label(),
            "code": self.code,
            "retryable": self.retryable,
            "recovery_path": self.recovery_path,
            "user_action": self.user_action,
            "evidence_refs": self.evidence_refs,
        })
    }

    /// The explanation a person reads.
    #[must_use]
    pub fn user_explanation(&self) -> String {
        let mut out = format!("{} ({}).", self.class.explained(), self.code);
        out.push_str(if self.retryable {
            " Trying again may succeed."
        } else {
            " Trying again as it is will not help."
        });
        if !self.user_action.is_empty() {
            out.push(' ');
            out.push_str(&self.user_action);
            if !self.user_action.ends_with('.') {
                out.push('.');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> FailureDiagnostic {
        FailureDiagnostic {
            class: FailureClass::Provider,
            code: "AUTH_REJECTED".into(),
            retryable: false,
            user_action: "Check the endpoint's credential".into(),
            recovery_path: "fix the credential, then StartTask".into(),
            evidence_refs: vec!["run:1".into()],
            features: vec![],
            detail: "HTTP 401: key [redacted] refused".into(),
        }
    }

    #[test]
    fn one_identity_renders_for_a_person_and_for_a_model() {
        let d = d();
        assert_eq!(
            d.user_explanation(),
            "The model provider failed or refused the request (AUTH_REJECTED). Trying again as it is will not help. Check the endpoint's credential."
        );
        let repair = d.model_repair();
        assert_eq!(repair["code"], "AUTH_REJECTED");
        assert_eq!(repair["failure_class"], "PROVIDER");
        assert_eq!(repair["retryable"], false);
        let text = d.render();
        assert!(
            text.starts_with("failure_class: PROVIDER\nretryable: false\n"),
            "{text}"
        );
        assert!(text.contains(&format!("repair: {repair}\n")), "{text}");
        // The source's words are evidence, not either rendering.
        assert!(!text.contains("HTTP 401") && !d.user_explanation().contains("HTTP 401"));
    }
}
