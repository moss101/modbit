//! Scoring from the Core's own event log, as `modbit-cli events tail --json`
//! prints it: one object per event with `offset`, `event_type`,
//! `aggregate_type`, `task_id` and the unwrapped `payload`. Every metric of
//! docs/63 that comes from events is counted here; nothing is inferred from
//! transcripts.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One event line.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// Log offset.
    pub offset: u64,
    /// Event type name (docs/30).
    pub event_type: String,
    /// Aggregate type.
    #[serde(default)]
    pub aggregate_type: String,
    /// Task id (hex) when the event belongs to a task.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Payload.
    #[serde(default)]
    pub payload: Value,
}

/// Parse JSON lines; lines that are not event objects are ignored.
#[must_use]
pub fn parse_events(jsonl: &str) -> Vec<Event> {
    jsonl
        .lines()
        .filter_map(|l| serde_json::from_str::<Event>(l).ok())
        .collect()
}

/// What the log says about one trial (docs/63 metrics).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Counts {
    /// `RepairAttemptRecorded` events.
    pub repair_attempts: u32,
    /// `RepairEscalated` events (equivalent-hypothesis escalations).
    pub repair_escalations: u32,
    /// `NoProgressDetected` events.
    pub no_progress_escalations: u32,
    /// `ToolCallPolicyDecision` events with `allowed == false` (wrong-effect attempts blocked).
    pub policy_denies: u32,
    /// Files the original plan (`PlanRecorded` version 1) expected to change.
    pub plan_v1_files: Vec<String>,
    /// `PlanRevised` events (scope expansions).
    pub plan_revisions: u32,
    /// `UserQuestionAsked` events (scope and other questions).
    pub questions: u32,
    /// Distinct paths changed (`FileChanged`).
    pub files_changed: Vec<String>,
    /// Changed paths outside the original plan's write set.
    pub files_outside_plan: Vec<String>,
    /// Edits of an existing file with no earlier `RetrievalRecorded` for that path (must be zero).
    pub edits_without_retrieval: u32,
    /// `RegressionAttributed` events.
    pub regressions_attributed: u32,
    /// `FlakyCheckQuarantined` events.
    pub flaky_quarantines: u32,
    /// `DiffInvariantViolated` events whose invariant is `DI-3`.
    pub di3_violations: u32,
    /// All `DiffInvariantViolated` events by invariant.
    pub diff_invariants: BTreeMap<String, u32>,
    /// Last `AcceptanceGateEvaluated` verdict.
    pub gate_verdict: Option<String>,
    /// Last `VerificationRunRecorded` stage and status.
    pub last_verification: Option<(String, String)>,
    /// `SelfReviewRecorded` seen with its unresolved count.
    pub self_review_unresolved: Option<u32>,
    /// A plan was recorded.
    pub plan_recorded: bool,
    /// Terminal markers.
    pub ready_for_review: bool,
    /// `TaskCompleted` seen.
    pub completed: bool,
    /// `TaskFailed` seen.
    pub failed: bool,
    /// `TaskNeedsAttention` events.
    pub needs_attention: u32,
    /// Model calls (`ModelUsageRecorded`).
    pub model_calls: u32,
    /// First and last offsets seen.
    pub offsets: Option<(u64, u64)>,
}

/// Count one task's events. Events carrying another task's id are ignored;
/// events without a task id (session-level) are counted, which is exact for
/// the one-task sessions the harness creates.
#[must_use]
pub fn count(events: &[Event], task_hex: &str) -> Counts {
    let mut c = Counts::default();
    let mut retrieved: BTreeSet<String> = BTreeSet::new();
    let mut changed: BTreeSet<String> = BTreeSet::new();
    let want = task_hex.replace('-', "").to_ascii_lowercase();
    for e in events {
        if let Some(t) = &e.task_id
            && !t.is_empty()
            && t.replace('-', "").to_ascii_lowercase() != want
        {
            continue;
        }
        c.offsets = Some(match c.offsets {
            None => (e.offset, e.offset),
            Some((a, b)) => (a.min(e.offset), b.max(e.offset)),
        });
        let p = &e.payload;
        match e.event_type.as_str() {
            "RepairAttemptRecorded" => c.repair_attempts += 1,
            "RepairEscalated" => c.repair_escalations += 1,
            "NoProgressDetected" => c.no_progress_escalations += 1,
            "ToolCallPolicyDecision" => {
                if p["allowed"] == Value::Bool(false) {
                    c.policy_denies += 1;
                }
            }
            "PlanRecorded" => {
                c.plan_recorded = true;
                if p["version"].as_u64().unwrap_or(1) == 1 {
                    c.plan_v1_files = strings(&p["expected_files"]);
                }
            }
            "PlanRevised" => c.plan_revisions += 1,
            "UserQuestionAsked" => c.questions += 1,
            "RetrievalRecorded" => {
                if let Some(path) = p["path"].as_str() {
                    retrieved.insert(path.to_owned());
                }
            }
            "FileChanged" => {
                if let Some(path) = p["path"].as_str() {
                    let existing = !p["before_hash"].is_null();
                    if existing && !retrieved.contains(path) && !changed.contains(path) {
                        c.edits_without_retrieval += 1;
                    }
                    changed.insert(path.to_owned());
                }
            }
            "RegressionAttributed" => c.regressions_attributed += 1,
            "FlakyCheckQuarantined" => c.flaky_quarantines += 1,
            "DiffInvariantViolated" => {
                let inv = p["invariant"].as_str().unwrap_or("?").to_owned();
                if inv == "DI-3" {
                    c.di3_violations += 1;
                }
                *c.diff_invariants.entry(inv).or_insert(0) += 1;
            }
            "AcceptanceGateEvaluated" => {
                c.gate_verdict = p["verdict"].as_str().map(str::to_owned);
            }
            "VerificationRunRecorded" => {
                c.last_verification = Some((
                    p["stage"].as_str().unwrap_or("").to_owned(),
                    p["status"].as_str().unwrap_or("").to_owned(),
                ));
            }
            "SelfReviewRecorded" => {
                c.self_review_unresolved =
                    Some(u32::try_from(p["unresolved"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX));
            }
            "TaskReadyForReview" => c.ready_for_review = true,
            "TaskCompleted" => c.completed = true,
            "TaskFailed" => c.failed = true,
            "TaskNeedsAttention" => c.needs_attention += 1,
            "ModelUsageRecorded" => c.model_calls += 1,
            _ => {}
        }
    }
    c.files_changed = changed.iter().cloned().collect();
    let plan: BTreeSet<&str> = c.plan_v1_files.iter().map(String::as_str).collect();
    c.files_outside_plan = changed
        .iter()
        .filter(|p| !plan.contains(p.as_str()))
        .cloned()
        .collect();
    c
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
