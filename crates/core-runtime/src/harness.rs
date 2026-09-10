//! Agent harness contracts (docs/14 "Agent harness contracts (PX-040)",
//! docs/28): the Core-enforced rules the one-agent runtime applies between
//! model output and tool execution. Pure: no I/O, no clocks.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Per-task budgets (docs/14 contract 5; Alpha defaults).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budgets {
    /// Turns.
    pub max_turns: u32,
    /// Tool calls.
    pub max_tool_calls: u32,
    /// Consecutive turns without progress.
    pub max_consecutive_no_progress_turns: u32,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            max_turns: 60,
            max_tool_calls: 300,
            max_consecutive_no_progress_turns: 3,
        }
    }
}

/// The recorded plan (docs/28 PX-014).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Observable outcome.
    pub outcome: String,
    /// Files expected to change.
    #[serde(default)]
    pub expected_files: Vec<String>,
    /// Verification the agent intends to run.
    #[serde(default)]
    pub verification: Vec<String>,
    /// Protected effects foreseen.
    #[serde(default)]
    pub protected_effects: Vec<String>,
    /// Version (1 = original).
    #[serde(default)]
    pub version: u32,
}

/// Durable harness state carried into every Context Pack (docs/14 contract 4).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessState {
    /// Budgets.
    pub budgets: Budgets,
    /// Turns used.
    pub turns: u32,
    /// Tool calls used.
    pub tool_calls: u32,
    /// Current plan.
    pub plan: Option<Plan>,
    /// Original write set (frozen by the first plan).
    pub original_write_set: Vec<String>,
    /// Files written outside the original write set.
    pub out_of_plan_files: Vec<String>,
    /// Open failure signatures (last failing command/test observations).
    pub open_failures: Vec<String>,
    /// Consecutive turns without progress.
    pub no_progress_turns: u32,
    /// Candidate revision (workspace revision after the last write).
    pub candidate_revision: Option<u64>,
    /// Whether a self-review was recorded with no unresolved findings.
    pub self_review_clean: bool,
    /// Steering inputs applied so far.
    pub steers: u32,
    /// BASELINE verification recorded (docs/64 §1); writes wait for it.
    #[serde(default)]
    pub baseline_recorded: bool,
    /// Derived verification plan object.
    #[serde(default)]
    pub verification_plan_ref: Option<String>,
    /// Test symbols failing at BASELINE (KNOWN_FAILING; DI-3 protected).
    #[serde(default)]
    pub baseline_failing: Vec<String>,
    /// Every BASELINE check with its status (regression attribution input).
    #[serde(default)]
    pub baseline_checks: Vec<(String, String)>,
    /// Quarantined (FLAKY) check ids.
    #[serde(default)]
    pub quarantined: Vec<String>,
    /// Open FLAG-class diff-invariant findings (block the SelfReview).
    #[serde(default)]
    pub open_flags: Vec<String>,
    /// Deferred tools activated by `tool.search` (projected with schemas from
    /// the next turn on; REQ-EV-0134).
    #[serde(default)]
    pub activated_tools: Vec<String>,
    /// Repair policy (docs/28 §5).
    #[serde(default)]
    pub repair_policy: RepairPolicy,
    /// Recorded repair attempts, in order.
    #[serde(default)]
    pub repair_attempts: Vec<RepairAttempt>,
    /// Ordinal of the attempt whose change and verification are pending.
    #[serde(default)]
    pub pending_attempt: Option<u32>,
    /// Revision of the last COMPLETION run that passed attribution.
    #[serde(default)]
    pub completion_verified_revision: Option<u64>,
}

/// Why the harness refused something.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HarnessRefusal {
    /// A write before the plan (docs/28 PX-014).
    PlanRequired,
    /// A write to a path the current plan does not declare (docs/28 §3, PX-016):
    /// scope is never widened silently — revise the plan first.
    PlanRevisionRequired {
        /// The path.
        path: String,
        /// Plan version the write was checked against.
        plan_version: u32,
    },
    /// An edit of a file the task has not retrieved at the current workspace
    /// revision (docs/28 §2, PX-015): retrieve before edit.
    RetrievalRequired {
        /// Paths without a current retrieval record.
        paths: Vec<String>,
        /// The current workspace revision.
        workspace_revision: u64,
    },
    /// A change after a failed verification without a recorded repair attempt
    /// (docs/28 §5, PX-018).
    RepairAttemptRequired {
        /// The open verification failure signatures.
        signatures: Vec<String>,
    },
    /// Completion proposed without a self-review (docs/28 PX-019).
    SelfReviewRequired,
    /// Completion proposed with unresolved findings.
    UnresolvedFindings {
        /// Count.
        unresolved: u32,
    },
    /// Completion proposed with an open failure (docs/14 contract 11).
    OpenFailures {
        /// Signatures.
        failures: Vec<String>,
    },
    /// Completion proposed with unresolved FLAG-class diff-invariant findings (docs/64 §4).
    OpenFlags {
        /// Findings.
        flags: Vec<String>,
    },
    /// The COMPLETION run blocked acceptance (regression, indeterminate, deny invariant).
    CompletionBlocked {
        /// Reasons.
        reasons: Vec<String>,
    },
}

/// A budget that ran out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exhausted {
    /// Budget name.
    pub budget: String,
    /// Limit.
    pub limit: u64,
    /// Used.
    pub used: u64,
}

/// Tools the harness itself serves (not effects; recorded on the Task).
pub const PLAN_TOOL: &str = "plan.update";
/// Completion proposal tool.
pub const COMPLETE_TOOL: &str = "task.complete";

/// Verification tool the harness serves (TARGETED run through the engine).
pub const VERIFY_TOOL: &str = "verify.run";
/// Harness tool: a typed question to the user (REQ-EV-0222); the run suspends until answered.
pub const ASK_TOOL: &str = "user.ask";
/// Harness tool: record a repair attempt before changing code after a failed
/// verification (docs/28 §5, PX-018).
pub const REPAIR_TOOL: &str = "repair.attempt";

/// Repair policy (docs/28 §5 "Repair policy"; Alpha defaults, policy-overridable).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairPolicy {
    /// Attempts per failure signature.
    pub max_attempts_per_signature: u32,
    /// Attempts per task.
    pub max_attempts_per_task: u32,
    /// WORSENED attempts tolerated before escalation.
    pub max_worsened_before_escalation: u32,
}

impl Default for RepairPolicy {
    fn default() -> Self {
        Self {
            max_attempts_per_signature: 2,
            max_attempts_per_task: 6,
            max_worsened_before_escalation: 1,
        }
    }
}

/// A recorded repair attempt (docs/28 §5 `RepairAttempt`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairAttempt {
    /// 1-based ordinal within the task.
    pub attempt_ordinal: u32,
    /// The open failure signature (`verify:<check_id>:<hash>`).
    pub failure_signature: String,
    /// Hypothesis as given.
    pub hypothesis: String,
    /// Normalized fingerprint of the hypothesis.
    pub hypothesis_fingerprint: String,
    /// Evidence the agent cited.
    pub evidence_refs: Vec<String>,
    /// Intended fix.
    pub intended_fix: String,
    /// Workspace revision when the attempt started.
    pub start_revision: u64,
    /// `RESOLVED` | `PARTIAL` | `UNCHANGED` | `WORSENED` once concluded.
    pub outcome: Option<String>,
    /// Fingerprint of the change, once concluded.
    pub change_fingerprint: Option<String>,
}

/// Normalized equivalence key of a hypothesis: lower-case alphanumeric words,
/// stop words dropped, sorted and de-duplicated.
#[must_use]
pub fn hypothesis_fingerprint(text: &str) -> String {
    const STOP: &[&str] = &[
        "the", "a", "an", "is", "are", "to", "of", "in", "on", "and", "or", "it", "that", "this",
        "be", "by", "for", "with", "as", "at", "so", "we", "i", "should", "must", "then",
        "because", "since", "when", "if", "not", "no", "will", "can", "which", "its", "from",
        "into", "than", "also", "but", "still",
    ];
    let mut words: Vec<String> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .filter(|w| !STOP.contains(&w.as_str()))
        .collect();
    words.sort();
    words.dedup();
    words.join(" ")
}

/// Why the repair loop refused to run an attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairEscalation {
    /// Signature.
    pub failure_signature: String,
    /// Reason.
    pub reason: String,
    /// Attempts on the signature so far.
    pub attempts: u32,
}

/// Harness tool: search the deferred tool catalog and activate matches for
/// the next turns (REQ-EV-0134 / 0177 / 0229). Discovery never authorizes.
pub const TOOL_SEARCH: &str = "tool.search";

/// Tools projected every turn with their schemas (the stable core); every
/// other host tool is deferred: named in `tool.search`, hydrated with its
/// schema only after discovery (REQ-EV-0177 lazy tool/schema context).
pub const CORE_TOOLS: &[&str] = &[
    "shell.exec",
    "shell.start",
    "shell.read",
    "shell.cancel",
    "search.retrieve",
    "search.exact",
    "context.pack",
];
/// Namespaces that are always core.
pub const CORE_NAMESPACES: &[&str] = &["fs", "change"];

/// The toolset (namespace) of a tool name.
#[must_use]
pub fn toolset_of(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

/// Whether a host tool is deferred until discovered.
#[must_use]
pub fn is_deferred(name: &str) -> bool {
    !CORE_TOOLS.contains(&name) && !CORE_NAMESPACES.contains(&toolset_of(name))
}

/// The workspace paths a write tool call targets, from its arguments.
#[must_use]
pub fn write_targets(tool_name: &str, arguments_json: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(arguments_json) else {
        return vec![];
    };
    match tool_name {
        "change.apply" => v["path"]
            .as_str()
            .map(|p| vec![p.to_owned()])
            .unwrap_or_default(),
        "change.batch" => v["ops"]
            .as_array()
            .map(|ops| {
                ops.iter()
                    .filter_map(|o| o["path"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        _ => vec![],
    }
}

/// Tool names that write the workspace (need a plan first).
pub const WRITE_TOOLS: &[&str] = &[
    "change.apply",
    "change.batch",
    "git.worktree.create",
    "git.worktree.close",
];

impl HarnessState {
    /// Check a turn may start; `Err` names the exhausted budget.
    pub fn check_turn_budget(&self) -> Result<(), Exhausted> {
        if self.turns >= self.budgets.max_turns {
            return Err(Exhausted {
                budget: "max_turns".into(),
                limit: u64::from(self.budgets.max_turns),
                used: u64::from(self.turns),
            });
        }
        if self.no_progress_turns >= self.budgets.max_consecutive_no_progress_turns {
            return Err(Exhausted {
                budget: "max_consecutive_no_progress_turns".into(),
                limit: u64::from(self.budgets.max_consecutive_no_progress_turns),
                used: u64::from(self.no_progress_turns),
            });
        }
        Ok(())
    }

    /// Check a tool call may run.
    pub fn check_tool_budget(&self) -> Result<(), Exhausted> {
        if self.tool_calls >= self.budgets.max_tool_calls {
            return Err(Exhausted {
                budget: "max_tool_calls".into(),
                limit: u64::from(self.budgets.max_tool_calls),
                used: u64::from(self.tool_calls),
            });
        }
        Ok(())
    }

    /// Harness rule for a tool the model requested (before the kernel sees it).
    pub fn check_tool(&self, tool_name: &str) -> Result<(), HarnessRefusal> {
        if WRITE_TOOLS.contains(&tool_name) && self.plan.is_none() {
            return Err(HarnessRefusal::PlanRequired);
        }
        Ok(())
    }

    /// Open verification failure signatures (`verify:` entries).
    #[must_use]
    pub fn open_verify_signatures(&self) -> Vec<String> {
        self.open_failures
            .iter()
            .filter(|f| f.starts_with("verify:"))
            .cloned()
            .collect()
    }

    /// Resolve a check id or a full signature to an open signature.
    #[must_use]
    pub fn resolve_signature(&self, given: &str) -> Option<String> {
        let g = given.trim();
        self.open_verify_signatures().into_iter().find(|s| {
            s == g
                || s == &format!("verify:{g}")
                || s.strip_prefix("verify:")
                    .is_some_and(|rest| rest.starts_with(&format!("{g}:")))
        })
    }

    /// docs/28 §5: after a failed verification a change needs a recorded
    /// repair attempt first.
    pub fn check_repair_gate(&self) -> Result<(), HarnessRefusal> {
        let open = self.open_verify_signatures();
        if !open.is_empty() && self.pending_attempt.is_none() {
            return Err(HarnessRefusal::RepairAttemptRequired { signatures: open });
        }
        Ok(())
    }

    /// Record a new attempt or refuse it with the escalation reason (docs/28 §5:
    /// equivalent hypothesis, per-signature bound, per-task bound).
    pub fn start_attempt(
        &mut self,
        failure_signature: &str,
        hypothesis: &str,
        evidence_refs: Vec<String>,
        intended_fix: &str,
        start_revision: u64,
    ) -> Result<&RepairAttempt, RepairEscalation> {
        let fp = hypothesis_fingerprint(hypothesis);
        let on_sig: Vec<&RepairAttempt> = self
            .repair_attempts
            .iter()
            .filter(|a| a.failure_signature == failure_signature)
            .collect();
        let attempts = on_sig.len() as u32;
        if on_sig.iter().any(|a| a.hypothesis_fingerprint == fp) {
            return Err(RepairEscalation {
                failure_signature: failure_signature.into(),
                reason: "equivalent hypothesis already attempted for this failure signature".into(),
                attempts,
            });
        }
        if attempts >= self.repair_policy.max_attempts_per_signature {
            return Err(RepairEscalation {
                failure_signature: failure_signature.into(),
                reason: format!(
                    "max_attempts_per_signature ({}) exhausted",
                    self.repair_policy.max_attempts_per_signature
                ),
                attempts,
            });
        }
        if self.repair_attempts.len() as u32 >= self.repair_policy.max_attempts_per_task {
            return Err(RepairEscalation {
                failure_signature: failure_signature.into(),
                reason: format!(
                    "max_attempts_per_task ({}) exhausted",
                    self.repair_policy.max_attempts_per_task
                ),
                attempts,
            });
        }
        let ordinal = self.repair_attempts.len() as u32 + 1;
        self.repair_attempts.push(RepairAttempt {
            attempt_ordinal: ordinal,
            failure_signature: failure_signature.into(),
            hypothesis: hypothesis.into(),
            hypothesis_fingerprint: fp,
            evidence_refs,
            intended_fix: intended_fix.into(),
            start_revision,
            outcome: None,
            change_fingerprint: None,
        });
        self.pending_attempt = Some(ordinal);
        Ok(self.repair_attempts.last().expect("pushed"))
    }

    /// Conclude the pending attempt from the verification signatures before
    /// and after it. Returns the outcome and, when the loop must stop, the
    /// escalation (oscillation / equivalent change, too many WORSENED).
    pub fn conclude_attempt(
        &mut self,
        before: &[String],
        after: &[String],
        change_fingerprint: &str,
    ) -> Option<(u32, String, Option<RepairEscalation>)> {
        let ordinal = self.pending_attempt.take()?;
        let idx = self
            .repair_attempts
            .iter()
            .position(|a| a.attempt_ordinal == ordinal)?;
        let target = self.repair_attempts[idx].failure_signature.clone();
        let target_gone = !after.contains(&target);
        let new_failures = after.iter().any(|s| !before.contains(s));
        let outcome = match (target_gone, new_failures) {
            (true, false) => "RESOLVED",
            (true, true) => "PARTIAL",
            (false, false) => "UNCHANGED",
            (false, true) => "WORSENED",
        };
        let equivalent_change = !change_fingerprint.is_empty()
            && self.repair_attempts[..idx]
                .iter()
                .any(|a| a.change_fingerprint.as_deref() == Some(change_fingerprint));
        self.repair_attempts[idx].outcome = Some(outcome.into());
        self.repair_attempts[idx].change_fingerprint = Some(change_fingerprint.into());
        let worsened = self
            .repair_attempts
            .iter()
            .filter(|a| a.outcome.as_deref() == Some("WORSENED"))
            .count() as u32;
        let attempts = self
            .repair_attempts
            .iter()
            .filter(|a| a.failure_signature == target)
            .count() as u32;
        let escalation = if equivalent_change {
            Some(RepairEscalation {
                failure_signature: target,
                reason: "the attempt's change is equivalent to a prior attempt's (no progress)"
                    .into(),
                attempts,
            })
        } else if worsened > self.repair_policy.max_worsened_before_escalation {
            Some(RepairEscalation {
                failure_signature: target,
                reason: format!(
                    "{worsened} WORSENED attempts exceed max_worsened_before_escalation ({})",
                    self.repair_policy.max_worsened_before_escalation
                ),
                attempts,
            })
        } else {
            None
        };
        Some((ordinal, outcome.into(), escalation))
    }

    /// A write must stay inside the current plan's expected files (docs/28
    /// §3): a path outside it is refused until a `plan.update` declares it
    /// (the `PlanRevised` event carries the scope delta). An expected entry
    /// ending in `/` covers a directory.
    pub fn check_write(&self, path: &str) -> Result<(), HarnessRefusal> {
        let Some(plan) = &self.plan else {
            return Err(HarnessRefusal::PlanRequired);
        };
        let covered = plan
            .expected_files
            .iter()
            .any(|e| e == path || (e.ends_with('/') && path.starts_with(e.as_str())) || e == "*");
        if covered {
            Ok(())
        } else {
            Err(HarnessRefusal::PlanRevisionRequired {
                path: path.to_owned(),
                plan_version: plan.version,
            })
        }
    }

    /// Record a plan (first call freezes the original write set).
    pub fn record_plan(&mut self, mut plan: Plan) -> (u32, Vec<String>, Vec<String>) {
        let version = self.plan.as_ref().map(|p| p.version + 1).unwrap_or(1);
        plan.version = version;
        let (added, removed) = match &self.plan {
            None => {
                self.original_write_set = plan.expected_files.clone();
                (plan.expected_files.clone(), vec![])
            }
            Some(prev) => (
                plan.expected_files
                    .iter()
                    .filter(|f| !prev.expected_files.contains(f))
                    .cloned()
                    .collect(),
                prev.expected_files
                    .iter()
                    .filter(|f| !plan.expected_files.contains(f))
                    .cloned()
                    .collect(),
            ),
        };
        self.plan = Some(plan);
        (version, added, removed)
    }

    /// Note a write to `path`; returns true when it is outside the original write set.
    pub fn note_write(&mut self, path: &str) -> bool {
        let outside = !self.original_write_set.iter().any(|f| f == path);
        if outside && !self.out_of_plan_files.iter().any(|f| f == path) {
            self.out_of_plan_files.push(path.to_owned());
        }
        outside
    }

    /// Completion handshake (docs/14 contract 11, docs/28 PX-019).
    pub fn check_completion(&self, unresolved: u32) -> Result<(), HarnessRefusal> {
        if self.plan.is_none() {
            return Err(HarnessRefusal::PlanRequired);
        }
        if unresolved > 0 {
            return Err(HarnessRefusal::UnresolvedFindings { unresolved });
        }
        if !self.open_failures.is_empty() {
            return Err(HarnessRefusal::OpenFailures {
                failures: self.open_failures.clone(),
            });
        }
        if !self.open_flags.is_empty() {
            return Err(HarnessRefusal::OpenFlags {
                flags: self.open_flags.clone(),
            });
        }
        Ok(())
    }

    /// Resolve FLAG findings whose paths the plan now declares.
    pub fn resolve_flags_by_plan(&mut self) {
        if let Some(plan) = &self.plan {
            let files = plan.expected_files.clone();
            self.open_flags
                .retain(|f| !files.iter().any(|p| f.contains(p.as_str())));
        }
    }
}

/// A failure signature normalized from a command/test observation
/// (docs/28 §5: failing check, error class, stable message fingerprint).
#[must_use]
pub fn failure_signature(tool_name: &str, error_code: &str, excerpt: &str) -> String {
    let fingerprint: String = excerpt
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join("|");
    let h = hex::encode(Sha256::digest(fingerprint.as_bytes()));
    format!("{tool_name}:{error_code}:{}", &h[..12])
}

/// Bounded observation with declared truncation (docs/14 contract 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    /// Text the model receives.
    pub text: String,
    /// Bytes of the full structured output.
    pub bytes_total: usize,
    /// Bytes included inline.
    pub bytes_included: usize,
    /// Whether anything was omitted.
    pub truncated: bool,
}

/// Build the observation the model sees for one tool result.
#[must_use]
pub fn observe(
    status: &str,
    error_code: Option<&str>,
    error_message: Option<&str>,
    structured_output: &str,
    stdout_ref: Option<&str>,
    result_ref: &str,
    ceiling_bytes: usize,
) -> Observation {
    let total = structured_output.len();
    let included = total.min(ceiling_bytes);
    let mut cut = included;
    while cut > 0 && !structured_output.is_char_boundary(cut) {
        cut -= 1;
    }
    let body = &structured_output[..cut];
    let mut text = format!("status: {status}\n");
    if let Some(c) = error_code {
        text.push_str(&format!("error_code: {c}\n"));
    }
    if let Some(m) = error_message {
        text.push_str(&format!("error: {m}\n"));
    }
    text.push_str(&format!("bytes_total: {total}\nbytes_included: 0..{cut}\n"));
    if cut < total {
        text.push_str(&format!(
            "omitted: {cut}..{total} (page the full result with artifact.range on result_ref {result_ref})\n"
        ));
    }
    if let Some(s) = stdout_ref {
        text.push_str(&format!("stdout_ref: {s}\n"));
    }
    text.push_str("output:\n");
    text.push_str(body);
    Observation {
        text,
        bytes_total: total,
        bytes_included: cut,
        truncated: cut < total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_loop_records_attempts_and_escalates_on_equivalence_bounds_and_worsening() {
        let mut h = HarnessState::default();
        h.open_failures.push("verify:cargo:t.rs::a:abcd".into());
        assert!(matches!(
            h.check_repair_gate(),
            Err(HarnessRefusal::RepairAttemptRequired { .. })
        ));
        assert_eq!(
            h.resolve_signature("cargo:t.rs::a").as_deref(),
            Some("verify:cargo:t.rs::a:abcd")
        );
        assert_eq!(h.resolve_signature("other"), None);
        let sig = "verify:cargo:t.rs::a:abcd";
        let a = h
            .start_attempt(sig, "Reject the zero quantity", vec![], "guard", 3)
            .unwrap();
        assert_eq!((a.attempt_ordinal, a.start_revision), (1, 3));
        assert!(
            h.check_repair_gate().is_ok(),
            "pending attempt admits the change"
        );
        // Equivalent wording is refused.
        let e = h
            .start_attempt(sig, "reject zero quantity!", vec![], "x", 4)
            .unwrap_err();
        assert!(e.reason.contains("equivalent"));
        // WORSENED: the target stays and a new failure appears.
        let (o, outcome, esc) = h
            .conclude_attempt(
                &[sig.into()],
                &[sig.into(), "verify:other:1".into()],
                "fp-1",
            )
            .unwrap();
        assert_eq!((o, outcome.as_str(), esc.is_none()), (1, "WORSENED", true));
        assert!(h.pending_attempt.is_none());
        // Second attempt, different hypothesis; an equivalent change escalates.
        h.start_attempt(sig, "Parse before validating", vec![], "y", 5)
            .unwrap();
        let (_, outcome, esc) = h
            .conclude_attempt(&[sig.into()], &[sig.into()], "fp-1")
            .unwrap();
        assert_eq!(outcome, "UNCHANGED");
        assert!(esc.unwrap().reason.contains("equivalent"));
        // The per-signature bound (2) is exhausted.
        let e = h
            .start_attempt(sig, "Something new", vec![], "z", 6)
            .unwrap_err();
        assert!(e.reason.contains("max_attempts_per_signature"), "{e:?}");
        // RESOLVED when the target is gone and nothing new failed.
        let mut h2 = HarnessState::default();
        h2.start_attempt(sig, "h", vec![], "f", 1).unwrap();
        let (_, outcome, esc) = h2.conclude_attempt(&[sig.into()], &[], "fp-9").unwrap();
        assert_eq!((outcome.as_str(), esc.is_none()), ("RESOLVED", true));
        assert_eq!(
            hypothesis_fingerprint("The parser accepts zero; reject it"),
            "accepts parser reject zero"
        );
    }

    #[test]
    fn plan_gates_writes_and_completion_needs_clean_state() {
        let mut h = HarnessState::default();
        assert_eq!(
            h.check_tool("change.apply"),
            Err(HarnessRefusal::PlanRequired)
        );
        assert_eq!(h.check_tool("fs.read"), Ok(()));
        let (v, added, _) = h.record_plan(Plan {
            outcome: "x".into(),
            expected_files: vec!["a.rs".into()],
            ..Default::default()
        });
        assert_eq!((v, added.len()), (1, 1));
        assert_eq!(h.check_tool("change.apply"), Ok(()));
        assert!(!h.note_write("a.rs"));
        assert!(h.note_write("b.rs"));
        // PX-016: writes outside the current plan are refused until revised.
        assert!(h.check_write("a.rs").is_ok());
        assert!(matches!(
            h.check_write("b.rs"),
            Err(HarnessRefusal::PlanRevisionRequired { .. })
        ));
        let (v, added, _) = h.record_plan(Plan {
            outcome: "o".into(),
            expected_files: vec!["a.rs".into(), "b.rs".into(), "docs/".into()],
            ..Plan::default()
        });
        assert_eq!((v, added), (2, vec!["b.rs".to_owned(), "docs/".to_owned()]));
        assert!(h.check_write("b.rs").is_ok() && h.check_write("docs/x.md").is_ok());
        assert!(h.check_write("src/z.rs").is_err());
        assert_eq!(
            write_targets("change.batch", r#"{"ops":[{"path":"x"},{"path":"y"}]}"#),
            ["x", "y"]
        );
        let (v, added, removed) = h.record_plan(Plan {
            outcome: "x".into(),
            expected_files: vec!["b.rs".into()],
            ..Default::default()
        });
        assert_eq!(
            (v, added, removed),
            (3, vec![], vec!["a.rs".to_owned(), "docs/".to_owned()])
        );
        assert_eq!(
            h.original_write_set,
            vec!["a.rs".to_owned()],
            "original set is frozen"
        );
        h.open_failures.push("test.run:FAILED:abc".into());
        assert!(matches!(
            h.check_completion(0),
            Err(HarnessRefusal::OpenFailures { .. })
        ));
        h.open_failures.clear();
        assert!(matches!(
            h.check_completion(2),
            Err(HarnessRefusal::UnresolvedFindings { unresolved: 2 })
        ));
        assert_eq!(h.check_completion(0), Ok(()));
        h.budgets.max_turns = 1;
        h.turns = 1;
        assert_eq!(h.check_turn_budget().unwrap_err().budget, "max_turns");
    }

    #[test]
    fn observations_declare_truncation() {
        let o = observe("SUCCESS", None, None, "0123456789", None, "ref", 4);
        assert!(o.truncated && o.bytes_included == 4 && o.bytes_total == 10);
        assert!(o.text.contains("omitted: 4..10"));
        let o = observe(
            "APPLICATION_FAILURE",
            Some("EXIT"),
            Some("boom"),
            "x",
            Some("s"),
            "ref",
            100,
        );
        assert!(
            !o.truncated && o.text.contains("error_code: EXIT") && o.text.contains("stdout_ref: s")
        );
        assert_eq!(
            failure_signature("test.run", "EXIT", "a\n\nb\nc\nd"),
            failure_signature("test.run", "EXIT", "a\nb\nc\nzzz")
        );
    }
}
