//! The failure taxonomy (REQ-EV-0073, REQ-EV-0245): one deterministic
//! classifier from what a source reported — a tool result, a provider, the
//! store, the loop's own boundaries — to a [`FailureDiagnostic`] with a
//! class, whether a retry can help, what the user can do, how the system
//! recovers, the evidence, and stable feature tags an evaluator keys on.
//! Nothing here guesses from prose: the class comes from the source's
//! typed status and code, and the same input always yields the same
//! diagnosis (the corpus under `tests/fixtures/diagnostics` pins it).

use modbit_domain::failure::{FailureClass, FailureDiagnostic};

/// Longest detail kept verbatim.
const DETAIL_CEILING: usize = 512;

/// Where a failure came from, as typed as the source reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureSource<'a> {
    /// A tool call's result (docs/30 `ToolCallResult.status`).
    Tool {
        /// Tool name.
        tool: &'a str,
        /// `TIMEOUT` etc. — the `ToolStatus` label, upper snake.
        status: &'a str,
        /// The result's error code, when any.
        code: Option<&'a str>,
        /// The result's message.
        message: Option<&'a str>,
        /// Object hash of the recorded result.
        result_ref: &'a str,
        /// A command whose process timed out.
        timed_out: bool,
        /// A command that was cancelled.
        cancelled: bool,
    },
    /// The model provider (docs/38 route / stream errors).
    Provider {
        /// Code.
        code: &'a str,
        /// Message.
        message: &'a str,
    },
    /// The event store or an object read.
    Store {
        /// `STALE_LEASE`, `OBJECT_MISMATCH`, `SEQUENCE_CONFLICT`, `SQLITE`, ...
        code: &'a str,
        /// Message.
        message: &'a str,
    },
    /// A harness budget ran out.
    Budget {
        /// Which.
        budget: &'a str,
        /// Limit.
        limit: u64,
        /// Used.
        used: u64,
    },
    /// The loop stopped for the user (question, repair escalation, no progress).
    Loop {
        /// `QUESTION_PENDING`, `REPAIR_ESCALATED`, `NO_PROGRESS`.
        code: &'a str,
        /// Message.
        message: &'a str,
    },
    /// A restart found the task at a boundary (docs/19 resume).
    Restart {
        /// `RECONCILING`, `AWAITING_APPROVAL`, `AWAITING_ANSWER`, `EXECUTING`, `TURN_START`.
        boundary: &'a str,
        /// Message.
        message: &'a str,
        /// Calls of unknown outcome, as ids.
        unknown_calls: &'a [String],
    },
    /// The session lease was superseded.
    LeaseLost {
        /// Held.
        held: u64,
        /// Current.
        current: u64,
        /// Owner.
        owner: &'a str,
    },
}

fn bounded(s: &str) -> String {
    let mut cut = s.len().min(DETAIL_CEILING);
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s[..cut].to_owned()
}

#[allow(clippy::too_many_arguments)]
fn diag(
    class: FailureClass,
    code: &str,
    retryable: bool,
    user_action: &str,
    recovery_path: &str,
    evidence_refs: Vec<String>,
    mut features: Vec<String>,
    detail: &str,
) -> FailureDiagnostic {
    features.push(format!("class:{}", class.label().to_lowercase()));
    features.push(format!("code:{}", code.to_lowercase()));
    features.push(if retryable {
        "retryable".into()
    } else {
        "not_retryable".into()
    });
    features.sort();
    features.dedup();
    FailureDiagnostic {
        class,
        code: code.to_owned(),
        retryable,
        user_action: user_action.to_owned(),
        recovery_path: recovery_path.to_owned(),
        evidence_refs,
        features,
        detail: bounded(detail),
    }
}

/// Classify a failure. Deterministic: the same source yields the same
/// diagnosis, byte for byte.
#[must_use]
pub fn classify(source: &FailureSource<'_>) -> FailureDiagnostic {
    match source {
        FailureSource::Tool {
            tool,
            status,
            code,
            message,
            result_ref,
            timed_out,
            cancelled,
        } => {
            let code_s = code.unwrap_or("");
            let msg = message.unwrap_or("");
            // `ToolStatus` reaches here either as its serde label
            // (`UNKNOWN_OUTCOME`) or as its upper-cased Debug form
            // (`UNKNOWNOUTCOME`); both name the same status.
            let norm = status.replace('_', "").to_uppercase();
            let status: &str = match norm.as_str() {
                "SUCCESS" => "SUCCESS",
                "APPLICATIONFAILURE" => "APPLICATION_FAILURE",
                "INFRAFAILURE" => "INFRA_FAILURE",
                "CANCELLED" => "CANCELLED",
                "UNKNOWNOUTCOME" => "UNKNOWN_OUTCOME",
                "POLICYDENIED" => "POLICY_DENIED",
                "APPROVALPENDING" => "APPROVAL_PENDING",
                "INVALIDARGUMENTS" => "INVALID_ARGUMENTS",
                "UNKNOWNTOOL" => "UNKNOWN_TOOL",
                _ => status,
            };
            let ev = if result_ref.is_empty() {
                vec![]
            } else {
                vec![result_ref.to_string()]
            };
            let mut f = vec!["source:tool".to_owned(), format!("tool:{tool}")];
            if *timed_out || code_s == "TIMEOUT" {
                f.push("timed_out".into());
                return diag(
                    FailureClass::Timeout,
                    "TIMEOUT",
                    true,
                    "",
                    "run it again with a larger timeout_ms, or split the command; a process that timed out was killed and left no result to trust",
                    ev,
                    f,
                    msg,
                );
            }
            if *cancelled || status == "CANCELLED" {
                return diag(
                    FailureClass::Cancelled,
                    "CANCELLED",
                    false,
                    "",
                    "the run stopped at a safe boundary; resume with StartTask to continue",
                    ev,
                    f,
                    msg,
                );
            }
            match status {
                "UNKNOWN_OUTCOME" => diag(
                    FailureClass::UnknownOutcome,
                    "UNKNOWN_OUTCOME",
                    false,
                    "check the target of the effect and record the outcome with ReconcileToolCall (EFFECT_CONFIRMED or EFFECT_ABSENT) when the Core cannot tell",
                    "the resumed run reconciles the call against the effect ledger and the target before anything else runs; it never replays the effect",
                    ev,
                    f,
                    msg,
                ),
                "APPROVAL_PENDING" => diag(
                    FailureClass::Approval,
                    "APPROVAL_REQUIRED",
                    true,
                    "resolve the approval bound to this intent (ResolveApproval); the same call re-enters once it is approved",
                    "the run waits at the approval; approving once runs the effect once",
                    ev,
                    f,
                    msg,
                ),
                "POLICY_DENIED" => {
                    if code_s == "TOOL_NOT_PROJECTED" {
                        // docs/16 (M5.1): the model was not offered the tool
                        // this turn; the plan unlocks it, not the user.
                        diag(
                            FailureClass::Policy,
                            code_s,
                            true,
                            "",
                            "declare the files (expected_files) or the effect (protected_effects) in `plan.update`, or activate the tool with tool.search; call it once it is projected",
                            ev,
                            f,
                            msg,
                        )
                    } else if code_s == "APPROVAL_DENIED" {
                        diag(
                            FailureClass::Approval,
                            "APPROVAL_DENIED",
                            false,
                            "",
                            "the effect did not happen; the run continues without it",
                            ev,
                            f,
                            msg,
                        )
                    } else {
                        diag(
                            FailureClass::Policy,
                            if code_s.is_empty() {
                                "POLICY_DENIED"
                            } else {
                                code_s
                            },
                            false,
                            "widen the task's lease or profile if the effect is wanted",
                            "the effect did not happen; the run continues without it",
                            ev,
                            f,
                            msg,
                        )
                    }
                }
                "INVALID_ARGUMENTS" | "UNKNOWN_TOOL" => diag(
                    FailureClass::Invalid,
                    if code_s.is_empty() { status } else { code_s },
                    false,
                    "",
                    "call again with arguments that fit the tool's schema, or a tool that exists",
                    ev,
                    f,
                    msg,
                ),
                "INFRA_FAILURE" => {
                    if matches!(code_s, "OBJECT_MISMATCH" | "INTEGRITY" | "CORRUPT") {
                        f.push("corrupt_state".into());
                        diag(
                            FailureClass::CorruptState,
                            code_s,
                            false,
                            "the stored object does not match its digest; restore it from a backup or discard the checkpoint that names it",
                            "nothing is read past a digest mismatch; the operation stops before writing",
                            ev,
                            f,
                            msg,
                        )
                    } else if code_s == "JOURNAL_FAILED" {
                        diag(
                            FailureClass::Infrastructure,
                            code_s,
                            true,
                            "",
                            "the dispatch was not journaled so nothing ran; call again",
                            ev,
                            f,
                            msg,
                        )
                    } else {
                        diag(
                            FailureClass::Infrastructure,
                            if code_s.is_empty() {
                                "INFRA_FAILURE"
                            } else {
                                code_s
                            },
                            true,
                            "check the terminal broker and the workspace are reachable",
                            "the operation did not run; call again once the infrastructure is back",
                            ev,
                            f,
                            msg,
                        )
                    }
                }
                _ => diag(
                    FailureClass::Application,
                    if code_s.is_empty() { "FAILED" } else { code_s },
                    true,
                    "",
                    "the operation ran and reported failure; read its output and change the command or the code before running it again",
                    ev,
                    f,
                    msg,
                ),
            }
        }
        FailureSource::Provider { code, message } => {
            let f = vec!["source:provider".to_owned()];
            let (class, retryable, action, recovery) = match *code {
                "MODEL_REVOKED" | "PIN_NOT_ELIGIBLE" => (
                    FailureClass::Policy,
                    false,
                    "choose a model the registry allows, or lift the pin",
                    "resume with StartTask on an eligible model",
                ),
                "TIMEOUT" | "STREAM_TIMEOUT" => (
                    FailureClass::Timeout,
                    true,
                    "",
                    "resume with StartTask; the interrupted attempt is recorded with its cost unknown",
                ),
                "NO_PROVIDER"
                | "ROUTE_REFUSED"
                | "NO_ELIGIBLE_BINDING"
                | "MODEL_NOT_IN_REGISTRY"
                | "MODEL_NOT_ELIGIBLE" => (
                    FailureClass::Provider,
                    false,
                    "configure a provider (ConfigureProvider) or pick a served model",
                    "resume with StartTask once a provider serves the model",
                ),
                "AUTH_REJECTED" | "PROVIDER_REJECTED" => (
                    FailureClass::Provider,
                    false,
                    "the provider rejected the credential; configure it again (ConfigureProvider)",
                    "resume with StartTask once the provider accepts the credential",
                ),
                "CONNECT_FAILED" | "TRANSPORT" | "STREAM_INTERRUPTED" | "MALFORMED_STREAM" => (
                    FailureClass::Provider,
                    true,
                    "check the provider endpoint is reachable",
                    "resume with StartTask; the interrupted attempt is recorded and not repeated",
                ),
                "RATE_LIMITED" => (
                    FailureClass::Provider,
                    true,
                    "",
                    "resume with StartTask after the provider's retry window",
                ),
                _ => (
                    FailureClass::Provider,
                    true,
                    "",
                    "resume with StartTask; the failed attempt is recorded and not repeated",
                ),
            };
            diag(class, code, retryable, action, recovery, vec![], f, message)
        }
        FailureSource::Store { code, message } => {
            let f = vec!["source:store".to_owned()];
            match *code {
                "STALE_LEASE" => diag(
                    FailureClass::Lease,
                    code,
                    false,
                    "re-acquire the session lease (AcquireSessionLease) or let the current owner continue",
                    "a stale owner cannot advance state; the current owner resumes with StartTask",
                    vec![],
                    f,
                    message,
                ),
                "OBJECT_MISMATCH" | "INTEGRITY" => diag(
                    FailureClass::CorruptState,
                    code,
                    false,
                    "the store's record does not verify; recovery refuses to proceed past it",
                    "nothing is applied past an integrity failure",
                    vec![],
                    f,
                    message,
                ),
                "SEQUENCE_CONFLICT" => diag(
                    FailureClass::Infrastructure,
                    code,
                    true,
                    "",
                    "another writer moved the aggregate; the operation re-reads and retries",
                    vec![],
                    f,
                    message,
                ),
                _ => diag(
                    FailureClass::Infrastructure,
                    code,
                    true,
                    "check the profile directory is writable and the disk has space",
                    "the write did not land; the operation may be retried",
                    vec![],
                    f,
                    message,
                ),
            }
        }
        FailureSource::Budget {
            budget,
            limit,
            used,
        } => diag(
            FailureClass::Budget,
            "BUDGET_EXHAUSTED",
            false,
            &format!("raise `{budget}` (used {used} of {limit}) and resume, or narrow the goal"),
            "the run suspended with its partial evidence; resume with StartTask and a larger budget",
            vec![],
            vec!["source:harness".into(), format!("budget:{budget}")],
            &format!("{budget}: {used}/{limit}"),
        ),
        FailureSource::Loop { code, message } => {
            let f = vec!["source:harness".to_owned()];
            match *code {
                "QUESTION_PENDING" => diag(
                    FailureClass::Harness,
                    code,
                    false,
                    "answer the question (RespondToQuestion); the run resumes from the answer",
                    "the run waits at the question boundary",
                    vec![],
                    f,
                    message,
                ),
                "REPAIR_ESCALATED" => diag(
                    FailureClass::Harness,
                    code,
                    false,
                    "read the attempt history and steer, or return the task to work with a new hypothesis",
                    "the repair loop stopped by policy; resume with StartTask and a steer",
                    vec![],
                    f,
                    message,
                ),
                "NO_PROGRESS" => diag(
                    FailureClass::Harness,
                    code,
                    false,
                    "steer the task or narrow the goal",
                    "the run suspended after consecutive turns without progress; resume with StartTask",
                    vec![],
                    f,
                    message,
                ),
                _ => diag(
                    FailureClass::Harness,
                    code,
                    false,
                    "",
                    "resume with StartTask",
                    vec![],
                    f,
                    message,
                ),
            }
        }
        FailureSource::Restart {
            boundary,
            message,
            unknown_calls,
        } => {
            let f = vec![
                "source:restart".to_owned(),
                format!("boundary:{}", boundary.to_lowercase()),
            ];
            match *boundary {
                "RECONCILING" => diag(
                    FailureClass::UnknownOutcome,
                    "RESTART_RECONCILING",
                    false,
                    "resume with StartTask to reconcile the calls, or record their outcome with ReconcileToolCall first",
                    "the resumed run reconciles each unknown outcome against the ledger and the target before anything else runs",
                    unknown_calls.to_vec(),
                    f,
                    message,
                ),
                "AWAITING_APPROVAL" => diag(
                    FailureClass::Approval,
                    "RESTART_AWAITING_APPROVAL",
                    true,
                    "resolve the open approval and resume with StartTask",
                    "the same approval, bound to the same intent, is still open; approving once runs the effect once",
                    vec![],
                    f,
                    message,
                ),
                "AWAITING_ANSWER" => diag(
                    FailureClass::Harness,
                    "RESTART_AWAITING_ANSWER",
                    true,
                    "answer the question and resume with StartTask",
                    "the run continues from the answer",
                    vec![],
                    f,
                    message,
                ),
                _ => diag(
                    FailureClass::Infrastructure,
                    "RESTART_SUSPENDED",
                    true,
                    "resume with StartTask",
                    "the run continues from its last recorded boundary; nothing is repeated",
                    vec![],
                    f,
                    message,
                ),
            }
        }
        FailureSource::LeaseLost {
            held,
            current,
            owner,
        } => diag(
            FailureClass::Lease,
            "LEASE_LOST",
            false,
            &format!(
                "the session lease moved from generation {held} to {current} (owner {owner}); the new owner resumes the run"
            ),
            "the stale owner stopped at a safe boundary; the current lease holder resumes with StartTask",
            vec![],
            vec!["source:lease".into()],
            &format!("held {held}, current {current}, owner {owner}"),
        ),
    }
}
