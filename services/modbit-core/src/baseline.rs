//! The fixed-revision verified-outcome baseline (REQ-EPR-000, docs/27 §22):
//! what the direct single-model path did on this session's tasks, assembled
//! from the canonical log and pinned to the build, the repository revision and
//! the environment that produced it.
//!
//! Nothing here estimates. A task whose provider never reported usage keeps a
//! null cost and is counted as incomplete, because a later routing change must
//! be compared against what was measured and not against a filled-in blank.

use modbit_domain::SessionId;
use modbit_observability::baseline::{BaselineBundle, Interventions, TaskOutcome, Usage, publish};
use modbit_protocol::v1 as wire;

use crate::server::Core;

/// The build this Core was compiled as: the version and the target it runs on.
/// It is what pins a bundle to a binary.
#[must_use]
pub(crate) fn build_digest() -> String {
    modbit_observability::baseline::digest_of(&format!(
        "modbit-core {} {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    ))
}

/// The environment the tasks ran in: the platform and the toolchains the
/// verification engine would use, hashed the same way it hashes them.
#[must_use]
pub(crate) fn environment_digest() -> String {
    let mut parts = vec![
        format!("os={}", std::env::consts::OS),
        format!("arch={}", std::env::consts::ARCH),
    ];
    for (tool, args) in [("cargo", "--version"), ("git", "--version")] {
        let out = std::process::Command::new(tool)
            .arg(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|| format!("{tool}=absent"));
        parts.push(out);
    }
    modbit_observability::baseline::digest_of(&parts.join("\n"))
}

/// Assemble the session's baseline from the log.
pub(crate) async fn assemble(
    core: &Core,
    session_id: SessionId,
    repository_revision: &str,
) -> BaselineBundle {
    let mut tasks: Vec<TaskOutcome> = Vec::new();
    {
        let store = core.store.lock().await;
        let events = store
            .read_session(&session_id, 0, 500_000)
            .unwrap_or_default();
        let mut order: Vec<modbit_domain::TaskId> = Vec::new();
        for e in &events {
            if let Some(t) = e.envelope.task_id
                && !order.contains(&t)
            {
                order.push(t);
            }
        }
        for task_id in order {
            let Ok(Some(task)) = store.task(&task_id) else {
                continue;
            };
            let mut o = TaskOutcome {
                task_id: hex::encode(task_id.as_bytes()),
                goal_digest: modbit_observability::baseline::digest_of(&task.goal_text),
                state: format!("{:?}", task.state),
                verification: "NOT_RUN".into(),
                verified: false,
                checks: (0, 0),
                model_calls: 0,
                retries: 0,
                cache_units: (0, 0),
                tool_calls: 0,
                usage: Usage::default(),
                wall_ms: 0,
                model_ms: 0,
                tool_ms: 0,
                interventions: Interventions::default(),
                model: String::new(),
                endpoint: String::new(),
            };
            let (mut first, mut last) = (i64::MAX, 0_i64);
            let mut model_started: Option<i64> = None;
            let mut tool_started: std::collections::HashMap<[u8; 16], i64> =
                std::collections::HashMap::new();
            let mut last_key: Option<String> = None;
            let mut checks: std::collections::HashMap<String, bool> =
                std::collections::HashMap::new();
            let mut completion_status = String::new();
            for e in &events {
                if e.envelope.task_id != Some(task_id) {
                    continue;
                }
                let at = e.envelope.occurred_at.0;
                first = first.min(at);
                last = last.max(at);
                let Ok(p) = store.payload(&e.envelope) else {
                    continue;
                };
                match e.envelope.event_type.as_str() {
                    "ModelInvocationStarted" => {
                        o.model_calls += 1;
                        model_started = Some(at);
                        if let Some(m) = p["model_route"]["model"].as_str() {
                            o.model = m.to_owned();
                        }
                        if let Some(ep) = p["model_route"]["endpoint"].as_str() {
                            o.endpoint = ep.to_owned();
                        }
                        if let Some(k) = p["model_route"]["cache_key"].as_str() {
                            if last_key.as_deref() == Some(k) {
                                o.cache_units.0 += 1;
                            } else {
                                o.cache_units.1 += 1;
                            }
                            last_key = Some(k.to_owned());
                        }
                    }
                    "ModelInvocationCompleted" | "TurnFailed" | "TurnInterrupted" => {
                        if let Some(s) = model_started.take() {
                            o.model_ms += u64::try_from(at - s).unwrap_or(0);
                        }
                    }
                    "ModelUsageRecorded" => {
                        o.retries +=
                            u32::try_from(p["route"]["retries"].as_u64().unwrap_or(0)).unwrap_or(0);
                        if p["reported"].as_bool() == Some(true) {
                            o.usage.add_reported(
                                p["input_tokens"].as_u64().unwrap_or(0),
                                p["output_tokens"].as_u64().unwrap_or(0),
                                p["cached_input_tokens"].as_u64().unwrap_or(0),
                            );
                        } else {
                            o.usage.add_unreported();
                        }
                    }
                    "ToolCallProposed" => {
                        o.tool_calls += 1;
                        tool_started.insert(e.envelope.aggregate_id, at);
                    }
                    "ToolCallSucceeded" | "ToolCallFailed" => {
                        if let Some(s) = tool_started.remove(&e.envelope.aggregate_id) {
                            o.tool_ms += u64::try_from(at - s).unwrap_or(0);
                        }
                    }
                    "UserQuestionAsked" => o.interventions.questions_asked += 1,
                    "UserQuestionAnswered" => o.interventions.questions_answered += 1,
                    "ApprovalRequested" => o.interventions.approvals_requested += 1,
                    "ApprovalResolved" => {
                        if p["approved"].as_bool() == Some(true)
                            || p["decision"].as_str() == Some("APPROVED")
                            || p["state"].as_str() == Some("Approved")
                        {
                            o.interventions.approvals_granted += 1;
                        } else {
                            o.interventions.approvals_denied += 1;
                        }
                    }
                    "TaskInputQueued" => o.interventions.steering_inputs += 1,
                    "ReviewDecisionRecorded" => o.interventions.review_decisions += 1,
                    "RepairEscalated" | "HarnessBudgetExhausted" => {
                        o.interventions.attention_stops += 1;
                    }
                    "VerificationRunRecorded" | "VerificationBaselineRecorded" => {
                        if p["stage"].as_str() == Some("COMPLETION") {
                            completion_status = p["status"].as_str().unwrap_or_default().to_owned();
                            checks.clear();
                        }
                        for c in p["checks"].as_array().into_iter().flatten() {
                            let Some(id) = c["check_id"].as_str() else {
                                continue;
                            };
                            match c["status"].as_str().unwrap_or_default() {
                                "PASS" => {
                                    checks.insert(id.to_owned(), true);
                                }
                                "FAIL" | "ERROR" | "TIMEOUT" => {
                                    checks.insert(id.to_owned(), false);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            o.checks = (
                u32::try_from(checks.values().filter(|ok| **ok).count()).unwrap_or(u32::MAX),
                u32::try_from(checks.values().filter(|ok| !**ok).count()).unwrap_or(u32::MAX),
            );
            o.verification = match completion_status.as_str() {
                "" => "NOT_RUN",
                "PASSED" if checks.is_empty() => "NO_CHECKS",
                "PASSED" => "PASSED",
                _ => "FAILED",
            }
            .to_owned();
            o.verified = o.verification == "PASSED";
            if last > first {
                o.wall_ms = u64::try_from(last - first).unwrap_or(0);
            }
            tasks.push(o);
        }
    }
    publish(
        &build_digest(),
        repository_revision,
        &environment_digest(),
        modbit_domain::Timestamp::now().0,
        tasks,
    )
}

/// The wire answer for a published bundle.
pub(crate) fn published(
    bundle: &BaselineBundle,
    bundle_ref: String,
    offset: u64,
) -> wire::OutcomeBaselinePublished {
    wire::OutcomeBaselinePublished {
        bundle_digest: bundle.bundle_digest.clone(),
        bundle_ref,
        tasks: u32::try_from(bundle.tasks.len()).unwrap_or(u32::MAX),
        verified_tasks: u32::try_from(bundle.verified_tasks).unwrap_or(u32::MAX),
        tasks_with_unknown_usage: u32::try_from(bundle.tasks_with_unknown_usage)
            .unwrap_or(u32::MAX),
        build_digest: bundle.build_digest.clone(),
        environment_digest: bundle.environment_digest.clone(),
        offset,
    }
}
