//! The telemetry, cost and SLO dashboard (M10.1; docs/34 "Metrics",
//! docs/77: "dashboards over the real usage ledger, SLO events and cost
//! records, never a mock feed"). One session's picture, aggregated from the
//! canonical log when it is asked for: task states, tool outcomes, the usage
//! ledger (IMP-EV-0032) by model and by task, the SLO ladder (IMP-EV-0023)
//! across its cloud tasks, recent failures and the providers' health.
//! Read-only; nothing here is stored or sampled.

use std::collections::{BTreeMap, HashMap};

use modbit_domain::{SessionId, TaskId};
use modbit_protocol::v1 as wire;

use crate::server::Core;

/// Recent failures shown.
const FAILURES: usize = 20;

/// The dashboard for `session`.
///
/// # Errors
/// `(UNKNOWN_SESSION, …)` when the log holds nothing of it.
pub(crate) async fn view(
    core: &Core,
    session: SessionId,
) -> Result<wire::DashboardView, (String, String)> {
    let registry = core.gateway.registry();
    let store = core.store.lock().await;
    let events = store
        .read_session(&session, 0, usize::MAX)
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    if events.is_empty() {
        return Err((
            "UNKNOWN_SESSION".into(),
            format!("the log holds no events of session {session}"),
        ));
    }
    let mut v = wire::DashboardView {
        session_id: session.to_string(),
        generated_at_ms: modbit_domain::Timestamp::now().0,
        ..Default::default()
    };
    // Tasks in first-seen order, with their tool calls and ladder rungs.
    let mut order: Vec<TaskId> = Vec::new();
    let mut tool_calls: HashMap<TaskId, u32> = HashMap::new();
    let mut stages: HashMap<TaskId, Vec<modbit_observability::slo::Stage>> = HashMap::new();
    let mut failures = Vec::new();
    for e in &events {
        let env = &e.envelope;
        if let Some(t) = env.task_id
            && !order.contains(&t)
        {
            order.push(t);
        }
        match env.event_type.as_str() {
            "ToolCallProposed" => {
                if let Some(t) = env.task_id {
                    *tool_calls.entry(t).or_default() += 1;
                }
            }
            "ToolCallSucceeded" => v.tool_succeeded += 1,
            "ToolCallFailed" => v.tool_failed += 1,
            "ToolCallUnknownOutcome" => v.tool_unknown_outcome += 1,
            "ToolCallCancelled" => v.tool_cancelled += 1,
            _ => {}
        }
        let failure = env.event_type.ends_with("Failed")
            || env.event_type.ends_with("NeedsAttention")
            || env.event_type.ends_with("UnknownOutcome");
        if env.event_type == "SloStageRecorded" || failure {
            let Ok(p) = store.payload(env) else {
                continue;
            };
            if env.event_type == "SloStageRecorded" {
                if let Some(t) = env.task_id {
                    stages
                        .entry(t)
                        .or_default()
                        .push(modbit_observability::slo::Stage {
                            stage: p["stage"].as_str().unwrap_or_default().to_owned(),
                            at_ms: p["at_ms"].as_i64().unwrap_or_default(),
                            run_id: p["run_id"].as_str().map(str::to_owned),
                            warm: p["warm"].as_bool(),
                            detail: p["detail"].as_str().unwrap_or_default().to_owned(),
                        });
                }
            } else {
                let diag = &p["diagnostic"];
                let code = diag["code"]
                    .as_str()
                    .or_else(|| p["failure_code"].as_str())
                    .or_else(|| p["code"].as_str())
                    .unwrap_or_default();
                if !code.is_empty() {
                    failures.push(wire::DashboardFailureRow {
                        offset: e.offset,
                        task_id: env.task_id.map(|t| t.to_string()).unwrap_or_default(),
                        event_type: env.event_type.clone(),
                        class: diag["class"].as_str().unwrap_or_default().to_owned(),
                        code: code.to_owned(),
                    });
                }
            }
        }
    }
    let skip = failures.len().saturating_sub(FAILURES);
    v.recent_failures = failures.split_off(skip);
    // The usage ledger: totals, by model, by task.
    let (calls, pricing) = crate::usage::calls(&store, &session, registry.as_ref());
    let mut models: BTreeMap<String, wire::DashboardModelRow> = BTreeMap::new();
    let mut by_task: HashMap<TaskId, wire::DashboardTaskRow> = HashMap::new();
    for c in &calls {
        v.input_tokens += c.input;
        v.cached_input_tokens += c.cached;
        v.output_tokens += c.output;
        v.cost_minor += c.cost_minor.unwrap_or(0);
        match (c.reported, c.cost_minor) {
            (_, Some(_)) => v.priced_calls += 1,
            (false, None) => v.unreported_calls += 1,
            (true, None) => v.unpriced_calls += 1,
        }
        let m = models
            .entry(c.binding.clone())
            .or_insert_with(|| wire::DashboardModelRow {
                model: c.binding.clone(),
                ..Default::default()
            });
        m.calls += 1;
        m.input_tokens += c.input;
        m.cached_input_tokens += c.cached;
        m.output_tokens += c.output;
        m.cost_minor += c.cost_minor.unwrap_or(0);
        if c.cost_minor.is_none() {
            m.unpriced_calls += 1;
        }
        if let Some(t) = c.task_id {
            let r = by_task.entry(t).or_insert_with(|| wire::DashboardTaskRow {
                cost_complete: true,
                ..Default::default()
            });
            r.model_calls += 1;
            r.input_tokens += c.input;
            r.output_tokens += c.output;
            r.cost_minor += c.cost_minor.unwrap_or(0);
            r.cost_complete &= c.cost_minor.is_some();
        }
    }
    if v.priced_calls > 0 {
        v.currency = pricing.currency;
        v.scale = pricing.scale;
    }
    v.models = models.into_values().collect();
    // Tasks: state, cost, tools, starts; and the ladder across them.
    let mut starts = Vec::new();
    for t in &order {
        let state = match store.task(t) {
            Ok(Some(task)) => match task.state {
                modbit_domain::task::TaskState::Waiting(_) => "Waiting".to_owned(),
                other => format!("{other:?}"),
            },
            _ => continue,
        };
        *v.tasks_by_state.entry(state.clone()).or_default() += 1;
        let ladder = stages
            .get(t)
            .map(|s| modbit_observability::slo::ladder(s))
            .unwrap_or_default();
        let mut row = by_task.remove(t).unwrap_or_default();
        row.task_id = t.to_string();
        row.state = state;
        row.tool_calls = tool_calls.get(t).copied().unwrap_or(0);
        row.starts = u32::try_from(ladder.len()).unwrap_or(u32::MAX);
        if row.model_calls == 0 {
            row.cost_complete = false;
        }
        starts.extend(ladder);
        v.tasks.push(row);
    }
    drop(store);
    let summary = modbit_observability::slo::summary(&starts);
    let ms = |x: Option<u64>| x.and_then(|y| i64::try_from(y).ok()).unwrap_or(-1);
    let fig = |f: &modbit_observability::slo::Figures| wire::SloFiguresView {
        starts: f.starts,
        ready_p50_ms: ms(f.ready_p50_ms),
        ready_max_ms: ms(f.ready_max_ms),
        first_token_p50_ms: ms(f.first_token_p50_ms),
        first_token_max_ms: ms(f.first_token_max_ms),
    };
    v.slo_cold = Some(fig(&summary.cold));
    v.slo_warm = Some(fig(&summary.warm));
    v.providers = core
        .gateway
        .endpoints()
        .into_iter()
        .map(|ep| {
            let h = core.gateway.health(&ep.name);
            format!(
                "{}: requests={} successes={} failures={} rate_limited={} first_token_ms={}",
                ep.name,
                h.requests,
                h.successes,
                h.failures,
                h.rate_limited,
                h.last_first_token_ms
                    .map_or_else(|| "-".to_owned(), |x| x.to_string())
            )
        })
        .collect();
    Ok(v)
}
