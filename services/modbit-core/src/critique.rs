//! Isolated review and bounded revision (REQ-EPR-007; docs/27 §9.5, §9.6,
//! §9.9, docs/49 EPR-007): when the Acceptance Gate cannot accept a
//! candidate without independent review, the plan's prevalidated reviewer
//! slot activates — never a slot invented here — inside the EPR-018
//! disposable environment, on its own review task, with a brief that
//! carries the request, the acceptance criteria, the candidate diff and the
//! static evidence and nothing of the solver's reasoning. The reviewer's
//! structured result is untrusted until each finding is validated against
//! the tree at the candidate revision; a `PASS` discharges the review
//! obligation at that revision through the gate; a `REVISE` returns the
//! candidate to work with the validated findings as a bounded revision on
//! the reviser (or solver) binding; a `BLOCK` or an `UNCERTAIN` result, or
//! a revision bound reached, is a person's decision. CRITIQUE is the label
//! the path earns afterwards.

use std::sync::Arc;

use modbit_core_runtime::admission::{self, RunLedger};
use modbit_core_runtime::harness::HarnessState;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::routing::{Money, Trigger};
use modbit_domain::run::RunEvent;
use modbit_domain::task::{InputMode, Task, TaskEvent, TaskOrigin, TaskState, WaitReason};
use modbit_domain::{RunId, TaskId};
use modbit_event_store::AppendRequest;
use modbit_event_store::NewEvent;
use serde::{Deserialize, Serialize};

use crate::runtime::{Lineage, StartConfig, append, typed};
use crate::server::Core;

/// The reviewer's report (docs/27 §9.5 `ReviewerResult`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ReviewerResult {
    /// `PASS` | `REVISE` | `BLOCK` | `UNCERTAIN`.
    #[serde(default)]
    pub verdict: String,
    /// 0..=100.
    #[serde(default)]
    pub confidence: u32,
    /// One line.
    #[serde(default)]
    pub summary: String,
    /// Findings.
    #[serde(default)]
    pub findings: Vec<Finding>,
    /// Questions the reviewer could not settle.
    #[serde(default)]
    pub unresolved_questions: Vec<String>,
    /// Filled by the Core: the revision reviewed.
    #[serde(default)]
    pub candidate_revision: u64,
    /// Filled by the Core: the review task.
    #[serde(default)]
    pub review_task_id: String,
}

/// One finding.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Finding {
    #[serde(default)]
    pub id: String,
    /// `CRITICAL` | `HIGH` | `MEDIUM` | `LOW`.
    #[serde(default)]
    pub severity: String,
    /// `correctness` | `security` | `api` | `regression` | `style` | …
    #[serde(default)]
    pub category: String,
    /// The path the finding is about, relative to the tree.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub claim: String,
    /// Evidence refs (object hashes, verification run ids, `path:line`).
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub suggested_action: String,
    /// Filled by the Core: whether the finding resolved against the tree at
    /// the candidate revision.
    #[serde(default)]
    pub validated: bool,
    /// Filled by the Core: why it did not.
    #[serde(default)]
    pub unsupported_reason: String,
}

/// The reviewer's only way to answer (docs/17 `review.report`).
pub const REPORT_TOOL: &str = "review.report";

/// The projection of `review.report`, for a review task only.
pub(crate) fn projection() -> modbit_providers::ToolProjection {
    modbit_providers::ToolProjection {
        name: REPORT_TOOL.into(),
        description: "Deliver your review: verdict PASS (the candidate meets its acceptance criteria), REVISE (validated findings the solver must address), BLOCK (must not be accepted) or UNCERTAIN; confidence 0-100; findings with severity, category, path, line, claim, evidence_refs and suggested_action; unresolved questions. Findings are validated against the tree at the candidate revision; one you cannot tie to a path stays an unresolved assertion. This ends your review.".into(),
        input_schema: serde_json::json!({"type":"object","properties":{
            "verdict":{"type":"string","enum":["PASS","REVISE","BLOCK","UNCERTAIN"]},
            "confidence":{"type":"integer","minimum":0,"maximum":100},
            "summary":{"type":"string"},
            "findings":{"type":"array","items":{"type":"object","properties":{
                "id":{"type":"string"},"severity":{"type":"string","enum":["CRITICAL","HIGH","MEDIUM","LOW"]},
                "category":{"type":"string"},"path":{"type":"string"},"line":{"type":"integer"},
                "claim":{"type":"string"},"evidence_refs":{"type":"array","items":{"type":"string"}},
                "suggested_action":{"type":"string"}},"required":["severity","claim"]}},
            "unresolved_questions":{"type":"array","items":{"type":"string"}}
        },"required":["verdict","summary"]}),
    }
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "CRITICAL" => 4,
        "HIGH" => 3,
        "MEDIUM" => 2,
        "LOW" => 1,
        _ => 0,
    }
}

/// `review.report` from the review task: validate, record on the candidate,
/// end the review. Returns the tool result text and whether the review is
/// over.
pub(crate) async fn handle_report(
    core: &Arc<Core>,
    review_task: &Task,
    state: &mut HarnessState,
    args: &str,
    actor: &Actor,
) -> (String, bool) {
    let mut result: ReviewerResult = match serde_json::from_str(args) {
        Ok(r) => r,
        Err(e) => {
            return (format!("status: INVALID_ARGUMENTS\nerror: {e}"), false);
        }
    };
    if !matches!(
        result.verdict.as_str(),
        "PASS" | "REVISE" | "BLOCK" | "UNCERTAIN"
    ) {
        return (
            "status: INVALID_ARGUMENTS\nerror: verdict must be PASS, REVISE, BLOCK or UNCERTAIN"
                .into(),
            false,
        );
    }
    let bound = {
        let store = core.store.lock().await;
        crate::review_env::bound_to(&store, review_task)
    };
    let Some((env_id, candidate_task_id, worktree, revision)) = bound else {
        return (
            "status: REFUSED\nerror_code: NOT_A_REVIEW\nerror: this task reviews nothing".into(),
            false,
        );
    };
    // Validation at the candidate revision: a finding names a path that is
    // in the tree the reviewer was given (the candidate's changed set or
    // any file of it); anything else is an unresolved assertion.
    let tree = std::path::Path::new(&worktree);
    for f in &mut result.findings {
        let p = f.path.trim().trim_start_matches("./");
        if p.is_empty() {
            f.validated = false;
            f.unsupported_reason = "no path".into();
            continue;
        }
        if p.contains("..") || std::path::Path::new(p).is_absolute() {
            f.validated = false;
            f.unsupported_reason = "path outside the tree".into();
            continue;
        }
        if tree.join(p).is_file() {
            f.validated = true;
        } else {
            f.validated = false;
            f.unsupported_reason = format!("`{p}` is not in the tree at the candidate revision");
        }
        if f.severity.is_empty() {
            f.severity = "LOW".into();
        }
    }
    let candidate_revision = {
        let store = core.store.lock().await;
        crate::review_env::candidate_revision_of(&store, candidate_task_id)
    };
    result.candidate_revision = candidate_revision;
    result.review_task_id = review_task.task_id.to_string();
    let validated = result.findings.iter().filter(|f| f.validated).count() as u32;
    let unsupported = result.findings.len() as u32 - validated;
    let highest = result
        .findings
        .iter()
        .filter(|f| f.validated)
        .map(|f| f.severity.as_str())
        .max_by_key(|s| severity_rank(s))
        .unwrap_or("NONE")
        .to_owned();
    let candidate = {
        let store = core.store.lock().await;
        store.task(&candidate_task_id).ok().flatten()
    };
    let Some(candidate) = candidate else {
        return (
            "status: REFUSED\nerror_code: UNKNOWN_TASK\nerror: the candidate task is gone".into(),
            false,
        );
    };
    let (result_ref, ok) = {
        let mut store = core.store.lock().await;
        let result_ref = store
            .objects()
            .put(&serde_json::to_vec(&result).unwrap_or_default())
            .unwrap_or_default();
        let ok = append(
            &mut store,
            core,
            Lineage::task(core.tenant_id, candidate.session_id, candidate.task_id),
            AggregateType::Task,
            *candidate.task_id.as_bytes(),
            vec![typed(
                "ReviewerResultRecorded",
                &TaskEvent::ReviewerResultRecorded {
                    review_task_id: review_task.task_id,
                    env_id: env_id.clone(),
                    candidate_revision,
                    verdict: result.verdict.clone(),
                    confidence: result.confidence.min(100),
                    result_ref: result_ref.clone(),
                    validated_findings: validated,
                    unsupported_findings: unsupported,
                    highest_severity: highest.clone(),
                    summary: result.summary.clone(),
                },
                actor.clone(),
            )],
        )
        .is_ok();
        (result_ref, ok)
    };
    if !ok {
        return (
            "status: INFRA_FAILURE\nerror: the result could not be recorded".into(),
            false,
        );
    }
    state.completion_summary = Some(format!("review {}: {}", result.verdict, result.summary));
    let _ = revision;
    (
        format!(
            "status: SUCCESS\nverdict: {}\ncandidate_revision: {candidate_revision}\nvalidated_findings: {validated}\nunsupported_findings: {unsupported}\nhighest_severity: {highest}\nresult_ref: {result_ref}\nnote: the review is recorded on the candidate; this review ends here",
            result.verdict
        ),
        true,
    )
}

/// At the candidate's acceptance boundary (its loop proposing completion):
/// the events to land on its run before `RunCompleted`, having activated
/// the reviewer leg when the gate requires independent review and the plan
/// in force has a prevalidated reviewer slot with an activation left.
pub(crate) async fn at_acceptance(
    core: &Arc<Core>,
    task: &Task,
    run_id: RunId,
    cfg: &StartConfig,
    state: &HarnessState,
    lt: Lineage,
    actor: &Actor,
) -> Vec<NewEvent> {
    let Some(acc) = state.acceptance.as_ref() else {
        return vec![];
    };
    let review_required = acc["independent_review_required"]
        .as_bool()
        .unwrap_or(false);
    let verdict = acc["verdict"].as_str().unwrap_or_default().to_owned();
    if !review_required || verdict == "ACCEPT" {
        return vec![];
    }
    let rev = acc["candidate_revision"].as_u64().unwrap_or(0);
    let gate_ref = acc["gate_ref"].as_str().unwrap_or_default().to_owned();
    // Reviewed at this revision already: nothing to activate.
    {
        let store = core.store.lock().await;
        if latest_result(&store, task).is_some_and(|r| r.candidate_revision == rev) {
            return vec![];
        }
    }
    let plan = {
        let store = core.store.lock().await;
        crate::escalation::plan_in_force(&store, task, run_id)
    };
    let context = crate::routing::RouteContext {
        endpoint: cfg.endpoint.clone(),
        model: cfg.model.clone(),
        cache: None,
        plan_id: cfg.plan_id.clone(),
    };
    let current = format!("{}/{}", cfg.endpoint, cfg.model);
    let Some(plan) = plan else {
        return vec![];
    };
    let stay = |reason: String| -> Vec<NewEvent> {
        vec![crate::routing::reevaluated_event(
            "REVIEW",
            plan.routing_epoch,
            Some(&context),
            &current,
            "STAY",
            reason,
            None,
            &plan.plan_id,
            actor.clone(),
        )]
    };
    let Some(slot) = plan
        .slots
        .iter()
        .find(|s| s.trigger == Trigger::ReviewRequired)
        .cloned()
    else {
        return stay(format!(
            "the acceptance gate requires independent review at revision {rev} (gate {gate_ref}); the plan in force ({}) has no prevalidated reviewer slot; the obligation stands for a person",
            plan.plan_id
        ));
    };
    // Admission against the run's ledger, like any other leg.
    let activation = {
        let store = core.store.lock().await;
        let activations: Vec<(String, u32)> = store
            .routing_activations(&run_id, &plan.plan_id)
            .unwrap_or_default()
            .into_iter()
            .fold(Vec::new(), |mut acc, a| {
                match acc.iter_mut().find(|(s, _)| *s == a.slot_id) {
                    Some((_, n)) => *n = (*n).max(a.activation),
                    None => acc.push((a.slot_id, a.activation)),
                }
                acc
            });
        let legs: u32 = store
            .routing_plans(&run_id)
            .unwrap_or_default()
            .iter()
            .map(|p| {
                store
                    .routing_activations(&run_id, &p.plan_id)
                    .map(|a| a.len() as u32)
                    .unwrap_or(0)
            })
            .sum();
        let ledger = RunLedger {
            activations,
            attempts: legs,
            spent: Money::zero(&plan.total_budget.currency, plan.total_budget.scale),
            in_flight: Money::zero(&plan.total_budget.currency, plan.total_budget.scale),
        };
        admission::admit_activation(&plan, &ledger, &slot.slot_id, Trigger::ReviewRequired)
    };
    let activation = match activation {
        Ok(a) => a,
        Err(r) => {
            return stay(format!(
                "the reviewer slot `{}` is not admitted: {} ({r:?}); the obligation stands for a person",
                slot.slot_id,
                r.code()
            ));
        }
    };
    // The environment (EPR-018) with the candidate materialized in it.
    let env = match crate::review_env::admit(core, task, "", actor).await {
        Ok(e) => e,
        Err((code, detail)) => {
            return stay(format!(
                "the reviewer slot `{}` cannot run: {code}: {detail}; the obligation stands for a person",
                slot.slot_id
            ));
        }
    };
    let brief = match materialize_and_brief(core, task, &env, state, rev, &gate_ref).await {
        Ok(b) => b,
        Err(e) => {
            crate::review_env::dispose(core, &env, &format!("brief failed: {e}"), actor).await;
            return stay(format!(
                "the review environment could not take the candidate: {e}; the obligation stands for a person"
            ));
        }
    };
    let brief_ref = {
        let mut store = core.store.lock().await;
        let r = store.objects().put(brief.as_bytes()).unwrap_or_default();
        let _ = append(
            &mut store,
            core,
            Lineage::task(core.tenant_id, task.session_id, env.review_task_id),
            AggregateType::Task,
            *env.review_task_id.as_bytes(),
            vec![typed(
                "TaskInputQueued",
                &TaskEvent::TaskInputQueued {
                    input_id: format!("review-brief-{}", env.env_id),
                    mode: InputMode::FollowUp,
                    text: brief.clone(),
                },
                actor.clone(),
            )],
        );
        r
    };
    // The review task on the reviewer's binding.
    let review_task = {
        let store = core.store.lock().await;
        store.task(&env.review_task_id).ok().flatten()
    };
    let Some(review_task) = review_task else {
        return stay("the review task is gone".into());
    };
    let review_cfg = StartConfig {
        endpoint: slot.endpoint.clone(),
        model: slot.model.clone(),
        budgets: modbit_core_runtime::Budgets {
            max_turns: 10,
            max_tool_calls: 40,
            max_consecutive_no_progress_turns: 4,
        },
        pinned: true,
        plan_id: String::new(),
        slot_id: String::new(),
        skills: vec![],
        lease_generation: cfg.lease_generation,
        ticket_id: String::new(),
    };
    let review_actor = Actor::Agent(format!("reviewer:{}", env.review_task_id));
    if let Err((code, detail)) = core
        .runtime
        .start_boxed(
            core,
            review_task,
            review_cfg,
            cfg.lease_generation,
            review_actor,
        )
        .await
    {
        crate::review_env::dispose(core, &env, &format!("start refused: {code}"), actor).await;
        return stay(format!(
            "the reviewer could not start: {code}: {detail}; the obligation stands for a person"
        ));
    }
    let _ = lt;
    vec![
        typed(
            "SlotActivated",
            &RunEvent::SlotActivated {
                plan_id: plan.plan_id.clone(),
                slot_id: activation.slot_id.clone(),
                activation: activation.activation,
                reserved_minor: activation.reserved.minor_units,
            },
            actor.clone(),
        ),
        crate::routing::reevaluated_event(
            "REVIEW",
            plan.routing_epoch,
            Some(&context),
            &format!("{}/{}", slot.endpoint, slot.model),
            "SWITCH",
            format!(
                "the acceptance gate requires independent review at revision {rev} (gate {gate_ref}); prevalidated reviewer slot `{}` activated in review environment {}",
                slot.slot_id, env.env_id
            ),
            None,
            &plan.plan_id,
            actor.clone(),
        ),
        typed(
            "ReviewLegActivated",
            &RunEvent::ReviewLegActivated {
                plan_id: plan.plan_id.clone(),
                slot_id: slot.slot_id.clone(),
                activation: activation.activation,
                endpoint: slot.endpoint.clone(),
                model: slot.model.clone(),
                candidate_revision: rev,
                gate_ref,
                env_id: env.env_id.clone(),
                review_task_id: env.review_task_id,
                brief_ref,
                reserved_minor: activation.reserved.minor_units,
            },
            actor.clone(),
        ),
    ]
}

/// Copy the candidate's working-tree changes into the scratch tree and
/// write the brief the reviewer starts from: the request, the acceptance
/// criteria, the candidate diff, the static evidence — and nothing of the
/// solver's reasoning.
async fn materialize_and_brief(
    core: &Arc<Core>,
    task: &Task,
    env: &crate::review_env::ReviewEnvironment,
    state: &HarnessState,
    rev: u64,
    gate_ref: &str,
) -> Result<String, String> {
    let root = task
        .workspace_root
        .clone()
        .ok_or_else(|| "no workspace".to_owned())?;
    let repo = modbit_git::Repo::open(std::path::Path::new(&root)).map_err(|e| e.to_string())?;
    let scratch = std::path::Path::new(&env.worktree);
    let mut changed: Vec<String> = Vec::new();
    for entry in repo.status().map_err(|e| e.to_string())? {
        let rel = entry.path.trim_end_matches('/').to_owned();
        if rel.is_empty() || rel.contains("..") {
            continue;
        }
        let src = std::path::Path::new(&root).join(&rel);
        let dst = scratch.join(&rel);
        if src.is_file() {
            if let Some(parent) = dst.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::copy(&src, &dst).map_err(|e| format!("{rel}: {e}"))?;
            changed.push(rel);
        } else if !src.exists() && dst.exists() {
            let _ = std::fs::remove_file(&dst);
            changed.push(format!("{rel} (deleted)"));
        }
    }
    let scratch_repo = modbit_git::Repo::open(scratch).map_err(|e| e.to_string())?;
    let diff = scratch_repo
        .diff_worktree()
        .map(|d| d.unified)
        .unwrap_or_default();
    let mut untracked = String::new();
    let mut budget = 32 * 1024usize;
    for entry in scratch_repo.status().unwrap_or_default() {
        if entry.code == "??" {
            let p = scratch.join(&entry.path);
            if let Ok(text) = std::fs::read_to_string(&p) {
                let take = text.len().min(8 * 1024).min(budget);
                if take == 0 {
                    break;
                }
                let slice: String = text.chars().take(take).collect();
                untracked.push_str(&format!("--- new file: {}\n{}\n", entry.path, slice));
                budget = budget.saturating_sub(take);
            }
        }
    }
    let plan = state.plan.as_ref();
    let verification: String = state
        .acceptance
        .as_ref()
        .map(|a| {
            format!(
                "gate verdict {}; required assurance {}; missing evidence [{}]; reject reasons [{}]; gate record {gate_ref}",
                a["verdict"].as_str().unwrap_or_default(),
                a["required_assurance"].as_str().unwrap_or_default(),
                a["missing_evidence"]
                    .as_array()
                    .map(|x| x.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default(),
                a["reject_reasons"]
                    .as_array()
                    .map(|x| x.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default(),
            )
        })
        .unwrap_or_default();
    let risk = state
        .realized_risk
        .as_ref()
        .map(|r| {
            format!(
                "realized risk {} (reasons: {})",
                r["level"].as_str().unwrap_or_default(),
                r["reasons"]
                    .as_array()
                    .map(|x| x
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("; "))
                    .unwrap_or_default()
            )
        })
        .unwrap_or_default();
    let open: Vec<String> = state.open_failures.clone();
    let brief = format!(
        "[REVIEW BRIEF] You are the Isolated Non-Committing Reviewer of task {} at candidate revision {rev}. You work in a disposable copy of the candidate tree ({}); you may read anything in it and run bounded checks (shell.exec, verify.run), and nothing you do here reaches the canonical repository. Report with review.report when done.\n\nOriginal request:\n{}\n\nAcceptance criteria (the solver's plan):\n- outcome: {}\n- expected files: {}\n- verification: {}\n\nCandidate changes (working tree vs base):\n{}\n{}\nStatic evidence:\n- {}\n- {}\n- open failures: {}\n\nChanged paths: {}\n",
        task.task_id,
        env.worktree,
        task.goal_text,
        plan.map(|p| p.outcome.clone()).unwrap_or_default(),
        plan.map(|p| p.expected_files.join(", "))
            .unwrap_or_default(),
        plan.map(|p| p.verification.join("; ")).unwrap_or_default(),
        if diff.is_empty() {
            "(no tracked-file diff)".to_owned()
        } else {
            diff
        },
        untracked,
        verification,
        risk,
        if open.is_empty() {
            "none".to_owned()
        } else {
            open.join("; ")
        },
        changed.join(", ")
    );
    let _ = core;
    Ok(brief)
}

/// The latest reviewer result on a candidate's log.
pub(crate) fn latest_result(
    store: &modbit_event_store::EventStore,
    task: &Task,
) -> Option<ReviewerResultSummary> {
    let events = store
        .read_aggregate(task.task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    events.iter().rev().find_map(|e| {
        if e.envelope.event_type != "ReviewerResultRecorded" {
            return None;
        }
        let p = store.payload(&e.envelope).ok()?;
        Some(ReviewerResultSummary {
            review_task_id: TaskId::parse(p["review_task_id"].as_str()?).ok()?,
            candidate_revision: p["candidate_revision"].as_u64().unwrap_or(0),
            verdict: p["verdict"].as_str().unwrap_or_default().to_owned(),
            result_ref: p["result_ref"].as_str().unwrap_or_default().to_owned(),
            validated_findings: p["validated_findings"].as_u64().unwrap_or(0) as u32,
            highest_severity: p["highest_severity"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            summary: p["summary"].as_str().unwrap_or_default().to_owned(),
            offset: e.offset,
        })
    })
}

/// A reviewer result as the log summarizes it.
#[derive(Clone, Debug)]
pub(crate) struct ReviewerResultSummary {
    pub review_task_id: TaskId,
    pub candidate_revision: u64,
    pub verdict: String,
    pub result_ref: String,
    pub validated_findings: u32,
    pub highest_severity: String,
    pub summary: String,
    pub offset: u64,
}

/// When a review task's loop is over: dispose its environment, and apply
/// its verdict to the candidate — or a person's decision when there is no
/// usable result.
pub(crate) async fn on_review_end(core: Arc<Core>, review_task: Task, actor: Actor) {
    let bound = {
        let store = core.store.lock().await;
        crate::review_env::bound_to(&store, &review_task)
    };
    let Some((env_id, candidate_task_id, _, _)) = bound else {
        return;
    };
    let env = {
        let store = core.store.lock().await;
        crate::review_env::find(&store, &env_id)
    };
    if let Some(env) = env {
        crate::review_env::dispose(&core, &env, "review over", &actor).await;
    }
    let candidate = {
        let store = core.store.lock().await;
        store.task(&candidate_task_id).ok().flatten()
    };
    let Some(candidate) = candidate else {
        return;
    };
    let result = {
        let store = core.store.lock().await;
        latest_result(&store, &candidate).filter(|r| r.review_task_id == review_task.task_id)
    };
    let lt = Lineage::task(core.tenant_id, candidate.session_id, candidate.task_id);
    let Some(result) = result else {
        // The reviewer ended without reporting: a person decides.
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            &core,
            lt,
            AggregateType::Task,
            *candidate.task_id.as_bytes(),
            vec![typed(
                "TaskNeedsAttention",
                &TaskEvent::TaskNeedsAttention {
                    reason: format!(
                        "the independent reviewer (task {}) ended without a report; the review obligation stands: review the candidate yourself or start another review",
                        review_task.task_id
                    ),
                    diagnostic: None,
                },
                actor.clone(),
            )],
        );
        return;
    };
    match result.verdict.as_str() {
        "PASS" => {
            // The obligation is discharged at this revision through the
            // gate, by the reviewer's ACCEPT as evidence.
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *candidate.task_id.as_bytes(),
                vec![typed(
                    "ReviewDecisionRecorded",
                    &TaskEvent::ReviewDecisionRecorded {
                        decision: "ACCEPT".into(),
                        candidate_revision: result.candidate_revision,
                        accepted: vec![],
                        rejected: vec![],
                        commit: None,
                        note: format!("independent reviewer PASS: {}", result.summary),
                        provenance: "independent_reviewer".into(),
                    },
                    actor.clone(),
                )],
            );
            let root = candidate.workspace_root.clone().unwrap_or_default();
            let (policy, _) =
                crate::assurance::policy_for(&core.assurance_policy, std::path::Path::new(&root));
            let (gate, gate_ref) = crate::gate::evaluate(
                &store,
                &candidate,
                &policy,
                result.candidate_revision,
                &[],
                &[],
            );
            if let Some(run) = store
                .runs_for_task(&candidate.task_id)
                .ok()
                .and_then(|r| r.into_iter().next())
            {
                let _ = store.append(AppendRequest {
                    tenant_id: core.tenant_id,
                    session_id: candidate.session_id,
                    task_id: Some(candidate.task_id),
                    run_id: Some(run.run_id),
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::Run,
                    aggregate_id: *run.run_id.as_bytes(),
                    expected_sequence: None,
                    events: vec![typed(
                        "AcceptanceGateEvaluated",
                        &crate::gate::event(&gate, &gate_ref, "", "INDEPENDENT_REVIEW"),
                        actor.clone(),
                    )],
                });
            }
        }
        "REVISE" if result.validated_findings > 0 => {
            revise(&core, &candidate, &result, &actor).await;
        }
        other => {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                &core,
                lt,
                AggregateType::Task,
                *candidate.task_id.as_bytes(),
                vec![
                    typed(
                        "ReviewDecisionRecorded",
                        &TaskEvent::ReviewDecisionRecorded {
                            decision: "RETURN".into(),
                            candidate_revision: result.candidate_revision,
                            accepted: vec![],
                            rejected: vec![],
                            commit: None,
                            note: format!("independent reviewer {other}: {}", result.summary),
                            provenance: "independent_reviewer".into(),
                        },
                        actor.clone(),
                    ),
                    typed(
                        "TaskNeedsAttention",
                        &TaskEvent::TaskNeedsAttention {
                            reason: format!(
                                "the independent reviewer says {other} at revision {} ({}; {} validated finding(s), highest {}); a person decides: review the candidate, return it with notes, or cancel",
                                result.candidate_revision,
                                result.summary,
                                result.validated_findings,
                                result.highest_severity
                            ),
                            diagnostic: None,
                        },
                        actor.clone(),
                    ),
                ],
            );
        }
    }
}

/// A bounded revision (docs/27 §9.5): the candidate returns to work with the
/// validated findings as its typed input and a new attempt starts on the
/// reviser slot's binding when the plan has one, else the solver's — while
/// the plan's revision bound allows; past it, a person decides.
async fn revise(core: &Arc<Core>, candidate: &Task, result: &ReviewerResultSummary, actor: &Actor) {
    let lt = Lineage::task(core.tenant_id, candidate.session_id, candidate.task_id);
    let (plan, prior_revisions, binding) = {
        let store = core.store.lock().await;
        let run = store
            .runs_for_task(&candidate.task_id)
            .ok()
            .and_then(|r| r.into_iter().next());
        let plan = run.and_then(|r| crate::escalation::plan_in_force(&store, candidate, r.run_id));
        let prior = store
            .read_aggregate(candidate.task_id.as_bytes(), 0, usize::MAX)
            .unwrap_or_default()
            .iter()
            .filter(|e| e.envelope.event_type == "RevisionActivated")
            .count() as u32;
        let binding = store
            .agent_nodes(&candidate.task_id)
            .unwrap_or_default()
            .into_iter()
            .find(|n| n.kind == "PRIMARY")
            .map(|n| (n.endpoint, n.model))
            .unwrap_or_default();
        (plan, prior, binding)
    };
    let max_revisions = plan.as_ref().map_or(0, |p| p.max_revisions);
    let findings: Vec<Finding> = {
        let store = core.store.lock().await;
        store
            .objects()
            .get(&result.result_ref)
            .ok()
            .and_then(|b| serde_json::from_slice::<ReviewerResult>(&b).ok())
            .map(|r| r.findings.into_iter().filter(|f| f.validated).collect())
            .unwrap_or_default()
    };
    let findings_text: Vec<String> = findings
        .iter()
        .map(|f| {
            format!(
                "- [{}] {} {}{}: {}{}",
                f.severity,
                f.category,
                f.path,
                if f.line > 0 {
                    format!(":{}", f.line)
                } else {
                    String::new()
                },
                f.claim,
                if f.suggested_action.is_empty() {
                    String::new()
                } else {
                    format!(" (suggested: {})", f.suggested_action)
                }
            )
        })
        .collect();
    if prior_revisions >= max_revisions {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *candidate.task_id.as_bytes(),
            vec![
                typed(
                    "ReviewDecisionRecorded",
                    &TaskEvent::ReviewDecisionRecorded {
                        decision: "RETURN".into(),
                        candidate_revision: result.candidate_revision,
                        accepted: vec![],
                        rejected: vec![],
                        commit: None,
                        note: format!("independent reviewer REVISE: {}", result.summary),
                        provenance: "independent_reviewer".into(),
                    },
                    actor.clone(),
                ),
                typed(
                    "TaskNeedsAttention",
                    &TaskEvent::TaskNeedsAttention {
                        reason: format!(
                            "the independent reviewer asks for a revision at revision {} but the plan's revision bound ({max_revisions}) is spent after {prior_revisions}; a person decides. Findings:\n{}",
                            result.candidate_revision,
                            findings_text.join("\n")
                        ),
                        diagnostic: None,
                    },
                    actor.clone(),
                ),
            ],
        );
        return;
    }
    let (endpoint, model) = plan
        .as_ref()
        .and_then(|p| p.slots.iter().find(|s| s.role == "reviser").cloned())
        .map(|s| (s.endpoint, s.model))
        .unwrap_or(binding);
    let revision = prior_revisions + 1;
    {
        let mut store = core.store.lock().await;
        let mut events = vec![typed(
            "ReviewDecisionRecorded",
            &TaskEvent::ReviewDecisionRecorded {
                decision: "RETURN".into(),
                candidate_revision: result.candidate_revision,
                accepted: vec![],
                rejected: vec![],
                commit: None,
                note: format!("independent reviewer REVISE: {}", result.summary),
                provenance: "independent_reviewer".into(),
            },
            actor.clone(),
        )];
        if candidate.state == TaskState::ReadyForReview {
            events.push(typed(
                "TaskReturnedToWork",
                &TaskEvent::TaskReturnedToWork,
                actor.clone(),
            ));
        }
        events.push(typed(
            "TaskWaiting",
            &TaskEvent::TaskWaiting {
                reason: WaitReason::UserInput,
            },
            actor.clone(),
        ));
        events.push(typed(
            "TaskInputQueued",
            &TaskEvent::TaskInputQueued {
                input_id: format!("review-revision-{revision}-{}", result.offset),
                mode: InputMode::FollowUp,
                text: format!(
                    "Independent review of revision {} (result {}): REVISE — {}. Validated findings to address:\n{}\nThis is revision {revision} of {max_revisions}; address the findings, then verify and propose completion again.",
                    result.candidate_revision,
                    result.result_ref,
                    result.summary,
                    findings_text.join("\n")
                ),
            },
            actor.clone(),
        ));
        events.push(typed(
            "RevisionActivated",
            &TaskEvent::RevisionActivated {
                revision,
                max_revisions,
                result_ref: result.result_ref.clone(),
                endpoint: endpoint.clone(),
                model: model.clone(),
            },
            actor.clone(),
        ));
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *candidate.task_id.as_bytes(),
            events,
        );
    }
    let fresh = {
        let store = core.store.lock().await;
        store.task(&candidate.task_id).ok().flatten()
    };
    let Some(fresh) = fresh else {
        return;
    };
    let generation = {
        let store = core.store.lock().await;
        store
            .session(&candidate.session_id)
            .ok()
            .flatten()
            .map(|s| s.lease_generation)
            .unwrap_or(0)
    };
    let cfg = StartConfig {
        endpoint,
        model,
        budgets: modbit_core_runtime::Budgets::default(),
        pinned: true,
        plan_id: String::new(),
        slot_id: String::new(),
        skills: vec![],
        lease_generation: generation,
        ticket_id: String::new(),
    };
    let solver_actor = Actor::Agent(format!("solver:{}", candidate.task_id));
    if let Err((code, detail)) = core
        .runtime
        .start_boxed(core, fresh, cfg, generation, solver_actor)
        .await
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *candidate.task_id.as_bytes(),
            vec![typed(
                "TaskNeedsAttention",
                &TaskEvent::TaskNeedsAttention {
                    reason: format!(
                        "the revision could not start: {code}: {detail}; StartTask to resume with the reviewer's findings queued"
                    ),
                    diagnostic: None,
                },
                actor.clone(),
            )],
        );
    }
}

/// Whether a task is a review task.
pub(crate) fn is_review(task: &Task) -> bool {
    task.origin == TaskOrigin::Review
}
