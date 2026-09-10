//! Context efficiency metrics (REQ-EV-0173, docs/18 "Context economy"): what a
//! task cost and what it bought, counted from the canonical log — never
//! estimated, never sampled. Quality and economics are reported together
//! because either one alone can be made to look good.

use modbit_domain::TaskId;
use modbit_protocol::v1 as wire;

use crate::server::Core;

/// The economics view of a task.
pub(crate) async fn view(core: &Core, task_id: TaskId) -> wire::TaskEconomicsView {
    let mut v = wire::TaskEconomicsView {
        task_id: hex::encode(task_id.as_bytes()),
        ..Default::default()
    };
    let store = core.store.lock().await;
    let Ok(Some(task)) = store.task(&task_id) else {
        return v;
    };
    v.state = format!("{:?}", task.state);
    let Ok(events) = store.read_session(&task.session_id, 0, 200_000) else {
        return v;
    };
    let (mut first_at, mut last_at) = (i64::MAX, 0_i64);
    // Model and tool time are measured between the start and the end of each
    // invocation, so they exclude the queueing around them.
    let mut model_started: Option<i64> = None;
    let mut tool_started: std::collections::HashMap<[u8; 16], i64> =
        std::collections::HashMap::new();
    let mut last_key: Option<String> = None;
    let mut checks: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut completion_status = String::new();
    let mut completion_run = String::new();
    let mut regressions: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for e in &events {
        if e.envelope.task_id != Some(task_id) {
            continue;
        }
        let at = e.envelope.occurred_at.0;
        first_at = first_at.min(at);
        last_at = last_at.max(at);
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        match e.envelope.event_type.as_str() {
            "ModelInvocationStarted" => {
                v.model_calls += 1;
                model_started = Some(at);
                if let Some(m) = p["model_route"]["model"].as_str() {
                    v.model = m.to_owned();
                }
                if let Some(key) = p["model_route"]["cache_key"].as_str() {
                    if last_key.as_deref() == Some(key) {
                        v.prefix_cache_hits += 1;
                    } else {
                        v.prefix_cache_misses += 1;
                    }
                    last_key = Some(key.to_owned());
                }
            }
            "ModelInvocationCompleted" | "TurnFailed" => {
                if let Some(started) = model_started.take() {
                    v.model_ms += u64::try_from(at - started).unwrap_or(0);
                }
            }
            "ModelUsageRecorded" => {
                v.input_tokens += p["input_tokens"].as_u64().unwrap_or(0);
                v.output_tokens += p["output_tokens"].as_u64().unwrap_or(0);
                v.cached_input_tokens += p["cached_input_tokens"].as_u64().unwrap_or(0);
            }
            "ToolCallProposed" => {
                v.tool_calls += 1;
                tool_started.insert(e.envelope.aggregate_id, at);
            }
            "ToolCallSucceeded" | "ToolCallFailed" => {
                if let Some(started) = tool_started.remove(&e.envelope.aggregate_id) {
                    v.tool_ms += u64::try_from(at - started).unwrap_or(0);
                }
            }
            "ContextEpochOpened" => {
                v.compaction_epochs += 1;
                v.compacted_entries += p["source_entries"].as_u64().unwrap_or(0);
            }
            // The last COMPLETION run decides the verdict, and the last state
            // of each check decides that check: one that failed and then
            // passed is a pass, and the reverse is a failure.
            "VerificationRunRecorded" | "VerificationBaselineRecorded" => {
                if p["stage"].as_str() == Some("COMPLETION") {
                    completion_status = p["status"].as_str().unwrap_or("").to_owned();
                    completion_run = p["verification_run_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned();
                    checks.clear();
                }
                for c in p["checks"].as_array().into_iter().flatten() {
                    let Some(id) = c["check_id"].as_str() else {
                        continue;
                    };
                    // A skipped or quarantined check is neither a pass nor a
                    // failure, so it is counted as neither.
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
            // What the completion run blamed on this change (docs/64 §1).
            "RegressionAttributed" if p["attribution"].as_str() == Some("REGRESSION") => {
                regressions.insert((
                    p["verification_run_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    p["check_id"].as_str().unwrap_or_default().to_owned(),
                ));
            }
            _ => {}
        }
    }
    v.checks_passed = u32::try_from(checks.values().filter(|ok| **ok).count()).unwrap_or(u32::MAX);
    v.checks_failed = u32::try_from(checks.values().filter(|ok| !**ok).count()).unwrap_or(u32::MAX);
    v.regressions = u32::try_from(
        regressions
            .iter()
            .filter(|(run, _)| *run == completion_run)
            .count(),
    )
    .unwrap_or(u32::MAX);
    // "Verified" means a completion run passed and something ran: a repository
    // with no derivable suite is reported as NO_CHECKS, never as verified.
    v.verification = match completion_status.as_str() {
        "" => "NOT_RUN",
        "PASSED" if checks.is_empty() => "NO_CHECKS",
        "PASSED" => "PASSED",
        _ => "FAILED",
    }
    .to_owned();
    v.verified = v.verification == "PASSED";
    if last_at > first_at {
        v.wall_ms = u64::try_from(last_at - first_at).unwrap_or(0);
    }
    // What the prompt envelope injected, from the task's Context Ledger.
    {
        let ledger = core.tools.ledger(task_id).await;
        let ledger = ledger.lock().await;
        if let Some(pack) = ledger.last_pack.as_ref() {
            v.context_tokens_injected = u64::from(pack.token_used);
        }
    }
    // Catalog list prices, no cache discount: this is what the run would cost
    // at the published rate, not a bill.
    if let Some(cap) = core
        .gateway
        .endpoints()
        .iter()
        .find_map(|ep| ep.models.iter().find(|m| m.model == v.model).cloned())
    {
        let m = 1_000_000.0_f64;
        v.cost_usd = (v.input_tokens as f64 / m) * cap.input_price_per_mtok
            + (v.output_tokens as f64 / m) * cap.output_price_per_mtok;
        v.pricing_known = 1;
    }
    v
}
