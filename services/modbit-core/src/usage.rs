//! Per-run and per-step usage attribution and invoice reconciliation
//! (IMP-EV-0032, REQ-EV-0032; docs/34 "Verified workflow economics").
//!
//! One row per model invocation, from the canonical `ModelUsageRecorded` —
//! the agent's own calls and the ones made for it (a context specialist) —
//! priced at its own binding in the active model registry with the request
//! accounting's rules (`accounting::price`, EPR-010): minor units, cached
//! input at its own price, each part rounded up. A call whose usage the
//! provider never reported is unknown, never zero. Tool and verification
//! time are attributed to the step (turn) they ran in.
//!
//! An invoice sample is compared row by row, by the provider's request id,
//! against that ledger. Nothing is settled here: an unknown usage is named
//! and settled with `ReconcileUsage`, and a row nobody recorded is named, not
//! absorbed. A side question appends nothing to the log (QUAL-EV-0261), so
//! its invoice rows are `NOT_IN_LOG` by design.

use std::collections::{BTreeMap, HashMap};

use modbit_domain::TaskId;
use modbit_event_store::EventStore;
use modbit_protocol::v1 as wire;
use serde_json::Value;
use sha2::Digest;

/// The invoice schema this reconciles.
const INVOICE_SCHEMA: &str = "modbit.invoice-sample/1";

/// One model invocation as the log recorded it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Call {
    pub task_id: Option<TaskId>,
    pub run_id: String,
    pub turn_id: String,
    /// The model the provider answered as (the binding when it said none).
    pub model: String,
    /// The binding the call was made under (the requested model).
    pub binding: String,
    pub provider_request_id: Option<String>,
    pub input: u64,
    pub cached: u64,
    pub output: u64,
    pub reported: bool,
    /// `None`: unpriced (no registry for the binding) or unknown usage.
    pub cost_minor: Option<u64>,
}

/// The pricing context of a ledger.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pricing {
    pub currency: String,
    pub scale: u32,
    pub generation: String,
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

/// Every model invocation of the session, in log order, priced.
pub(crate) fn calls(
    store: &EventStore,
    session: &modbit_domain::SessionId,
    registry: Option<&modbit_providers::registry::ModelRegistry>,
) -> (Vec<Call>, Pricing) {
    let mut out = Vec::new();
    let mut pricing = Pricing::default();
    for e in store
        .read_session(session, 0, usize::MAX)
        .unwrap_or_default()
    {
        if e.envelope.event_type != "ModelUsageRecorded" {
            continue;
        }
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let route = &p["route"];
        let endpoint = text(&route["endpoint"]);
        let binding = text(&route["requested_model"]);
        let model = route["resolved_model"]
            .as_str()
            .filter(|m| !m.is_empty())
            .map_or_else(|| binding.clone(), str::to_owned);
        let reported = p["reported"].as_bool().unwrap_or(false);
        let usage = modbit_providers::Usage {
            input_tokens: p["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: p["output_tokens"].as_u64().unwrap_or(0),
            cached_input_tokens: p["cached_input_tokens"].as_u64().unwrap_or(0),
            cache_write_input_tokens: 0,
        };
        let priced = reported
            .then(|| crate::accounting::price(registry, &endpoint, &binding, &usage))
            .flatten();
        if let (Some(r), Some(pr)) = (registry, &priced)
            && let Some(entry) = r.entry(&endpoint, &binding)
        {
            pricing.currency.clone_from(&entry.economics.currency);
            pricing.scale = u32::from(entry.economics.scale);
            pricing.generation.clone_from(&pr.registry_generation);
        }
        out.push(Call {
            task_id: e.envelope.task_id,
            run_id: e.envelope.run_id.map(|r| r.to_string()).unwrap_or_default(),
            turn_id: e
                .envelope
                .turn_id
                .map(|t| t.to_string())
                .unwrap_or_default(),
            model,
            binding,
            provider_request_id: route["provider_request_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            input: usage.input_tokens,
            cached: usage.cached_input_tokens,
            output: usage.output_tokens,
            reported,
            cost_minor: priced.map(|p| p.minor),
        });
    }
    (out, pricing)
}

type Runs = BTreeMap<String, (u64, BTreeMap<String, wire::StepUsageView>)>;

/// The step (turn) of a run, created on first use; runs keep the order they
/// were first seen in.
fn step_of<'a>(
    runs: &'a mut Runs,
    order: &mut u64,
    run: &str,
    turn: &str,
) -> &'a mut wire::StepUsageView {
    *order += 1;
    let first = *order;
    let (_, steps) = runs
        .entry(run.to_owned())
        .or_insert((first, BTreeMap::new()));
    let s = steps.entry(turn.to_owned()).or_default();
    s.turn_id = turn.to_owned();
    s
}

/// The task's usage by run and step, with tool and verification time.
pub(crate) fn attribution(
    store: &EventStore,
    task: &modbit_domain::task::Task,
    calls: &[Call],
) -> Vec<wire::RunUsageView> {
    let mut runs: Runs = BTreeMap::new();
    let mut order = 0u64;
    for c in calls.iter().filter(|c| c.task_id == Some(task.task_id)) {
        let s = step_of(&mut runs, &mut order, &c.run_id, &c.turn_id);
        let first_call = s.model_calls == 0;
        s.model_calls += 1;
        s.input_tokens += c.input;
        s.cached_input_tokens += c.cached;
        s.output_tokens += c.output;
        s.cost_minor += c.cost_minor.unwrap_or(0);
        if !s.models.contains(&c.model) {
            s.models.push(c.model.clone());
        }
        if let Some(id) = &c.provider_request_id {
            s.provider_request_ids.push(id.clone());
        }
        // Complete only while every call so far was priced and reported.
        s.cost_complete = (first_call || s.cost_complete) && c.cost_minor.is_some();
    }
    let mut tool_started: HashMap<[u8; 16], (i64, String, String)> = HashMap::new();
    for e in store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.envelope.task_id == Some(task.task_id))
    {
        let env = &e.envelope;
        let run = env.run_id.map(|r| r.to_string()).unwrap_or_default();
        let turn = env.turn_id.map(|t| t.to_string()).unwrap_or_default();
        match env.event_type.as_str() {
            "ToolCallProposed" if !run.is_empty() => {
                tool_started.insert(
                    env.aggregate_id,
                    (env.occurred_at.0, run.clone(), turn.clone()),
                );
                step_of(&mut runs, &mut order, &run, &turn).tool_calls += 1;
            }
            "ToolCallSucceeded" | "ToolCallFailed" | "ToolCallCancelled" => {
                if let Some((at, run, turn)) = tool_started.remove(&env.aggregate_id) {
                    step_of(&mut runs, &mut order, &run, &turn).tool_ms +=
                        u64::try_from(env.occurred_at.0 - at).unwrap_or(0);
                }
            }
            "VerificationRunRecorded" if !run.is_empty() => {
                let Ok(p) = store.payload(env) else {
                    continue;
                };
                let ms: u64 = p["checks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| c["duration_ms"].as_u64().unwrap_or(0))
                    .sum();
                step_of(&mut runs, &mut order, &run, &turn).verification_ms += ms;
            }
            _ => {}
        }
    }
    let mut out: Vec<(u64, wire::RunUsageView)> = runs
        .into_iter()
        .map(|(run_id, (first, steps))| {
            let steps: Vec<wire::StepUsageView> = steps.into_values().collect();
            let with_calls: Vec<&wire::StepUsageView> =
                steps.iter().filter(|s| s.model_calls > 0).collect();
            (
                first,
                wire::RunUsageView {
                    input_tokens: steps.iter().map(|s| s.input_tokens).sum(),
                    output_tokens: steps.iter().map(|s| s.output_tokens).sum(),
                    cost_minor: steps.iter().map(|s| s.cost_minor).sum(),
                    cost_complete: !with_calls.is_empty()
                        && with_calls.iter().all(|s| s.cost_complete),
                    run_id,
                    steps,
                },
            )
        })
        .collect();
    out.sort_by_key(|(first, _)| *first);
    out.into_iter().map(|(_, r)| r).collect()
}

/// One row of an invoice sample.
#[derive(Clone, Debug, Default, serde::Deserialize)]
struct InvoiceRow {
    request_id: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cost_minor: Option<u64>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct Invoice {
    schema: String,
    #[serde(default)]
    rows: Vec<InvoiceRow>,
}

/// The largest relative difference, in basis points.
fn delta_bp(pairs: &[(u64, u64)]) -> u32 {
    pairs
        .iter()
        .map(|(a, b)| {
            let (hi, lo) = if a >= b { (*a, *b) } else { (*b, *a) };
            if hi == 0 {
                0
            } else {
                u32::try_from((u128::from(hi - lo) * 10_000).div_ceil(u128::from(hi)))
                    .unwrap_or(u32::MAX)
            }
        })
        .max()
        .unwrap_or(0)
}

fn signed(v: Option<u64>) -> i64 {
    v.and_then(|x| i64::try_from(x).ok()).unwrap_or(-1)
}

/// Compare an invoice sample with a task's canonical usage.
///
/// # Errors
/// `(BAD_INVOICE, detail)` when it is not a `modbit.invoice-sample/1`.
pub(crate) fn reconcile_invoice(
    task: TaskId,
    calls: &[Call],
    invoice_json: &str,
    tolerance_bp: u32,
) -> Result<wire::InvoiceReconciliationView, (String, String)> {
    let invoice: Invoice = serde_json::from_str(invoice_json).map_err(|e| {
        (
            "BAD_INVOICE".to_owned(),
            format!("not an invoice sample: {e}"),
        )
    })?;
    if invoice.schema != INVOICE_SCHEMA {
        return Err((
            "BAD_INVOICE".into(),
            format!("schema `{}` is not {INVOICE_SCHEMA}", invoice.schema),
        ));
    }
    let mut view = wire::InvoiceReconciliationView {
        tolerance_bp,
        invoice_digest: hex::encode(sha2::Sha256::digest(invoice_json.as_bytes())),
        ..Default::default()
    };
    let by_id: HashMap<&str, &Call> = calls
        .iter()
        .filter_map(|c| c.provider_request_id.as_deref().map(|id| (id, c)))
        .collect();
    let mut billed = std::collections::HashSet::new();
    for r in &invoice.rows {
        let mut row = wire::InvoiceRowView {
            provider_request_id: r.request_id.clone(),
            model: r.model.clone(),
            invoice_input_tokens: r.input_tokens,
            invoice_cached_input_tokens: r.cached_input_tokens,
            invoice_output_tokens: r.output_tokens,
            invoice_cost_minor: signed(r.cost_minor),
            log_cost_minor: -1,
            ..Default::default()
        };
        match by_id.get(r.request_id.as_str()) {
            None => {
                row.status = "NOT_IN_LOG".into();
                view.not_in_log += 1;
            }
            Some(c) if c.task_id != Some(task) => {
                row.status = "OTHER_TASK".into();
            }
            Some(c) => {
                billed.insert(r.request_id.as_str());
                row.run_id.clone_from(&c.run_id);
                row.turn_id.clone_from(&c.turn_id);
                row.log_input_tokens = c.input;
                row.log_cached_input_tokens = c.cached;
                row.log_output_tokens = c.output;
                row.log_cost_minor = signed(c.cost_minor);
                let mut pairs = vec![
                    (r.input_tokens, c.input),
                    (r.cached_input_tokens, c.cached),
                    (r.output_tokens, c.output),
                ];
                if let (Some(a), Some(b)) = (r.cost_minor, c.cost_minor) {
                    pairs.push((a, b));
                }
                row.delta_bp = delta_bp(&pairs);
                if !c.reported {
                    row.status = "UNKNOWN_USAGE".into();
                    view.unknown_usage += 1;
                } else if !r.model.is_empty() && r.model != c.model && r.model != c.binding {
                    // An invoice may name the alias the call was made under or
                    // the snapshot the provider answered as; neither is a
                    // mismatch.
                    row.status = "MODEL_MISMATCH".into();
                    view.out_of_tolerance += 1;
                } else if row.delta_bp > tolerance_bp {
                    row.status = "OUT_OF_TOLERANCE".into();
                    view.out_of_tolerance += 1;
                } else {
                    row.status = "MATCHED".into();
                    view.matched += 1;
                }
            }
        }
        view.rows.push(row);
    }
    for c in calls.iter().filter(|c| c.task_id == Some(task)) {
        let id = c.provider_request_id.as_deref().unwrap_or_default();
        if id.is_empty() || !billed.contains(id) {
            view.not_on_invoice += 1;
            view.rows.push(wire::InvoiceRowView {
                provider_request_id: id.to_owned(),
                status: "NOT_ON_INVOICE".into(),
                model: c.model.clone(),
                log_input_tokens: c.input,
                log_cached_input_tokens: c.cached,
                log_output_tokens: c.output,
                log_cost_minor: signed(c.cost_minor),
                invoice_cost_minor: -1,
                run_id: c.run_id.clone(),
                turn_id: c.turn_id.clone(),
                ..Default::default()
            });
        }
    }
    view.within_tolerance = view.matched > 0
        && view.out_of_tolerance == 0
        && view.not_in_log == 0
        && view.not_on_invoice == 0
        && view.unknown_usage == 0;
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(task: TaskId, id: &str, input: u64, output: u64, cost: Option<u64>) -> Call {
        Call {
            task_id: Some(task),
            run_id: "r".into(),
            turn_id: "t".into(),
            model: "m".into(),
            provider_request_id: Some(id.into()),
            input,
            output,
            reported: true,
            cost_minor: cost,
            ..Call::default()
        }
    }

    #[test]
    fn an_invoice_reconciles_row_by_row_within_its_tolerance() {
        let t = TaskId::from_bytes([1; 16]);
        let other = TaskId::from_bytes([2; 16]);
        let mut unknown = call(t, "d", 5, 5, None);
        unknown.reported = false;
        let calls = vec![
            call(t, "a", 1000, 100, Some(50)),
            call(t, "b", 2000, 200, Some(90)),
            call(other, "c", 1, 1, Some(1)),
            unknown,
            call(t, "e", 10, 10, Some(1)),
        ];
        let inv = serde_json::json!({"schema": INVOICE_SCHEMA, "rows": [
            {"request_id": "a", "model": "m", "input_tokens": 1000, "output_tokens": 100, "cost_minor": 50},
            {"request_id": "b", "input_tokens": 2100, "output_tokens": 200},
            {"request_id": "c", "input_tokens": 1, "output_tokens": 1},
            {"request_id": "d", "input_tokens": 5, "output_tokens": 5},
            {"request_id": "z", "input_tokens": 7, "output_tokens": 7},
        ]})
        .to_string();
        let v = reconcile_invoice(t, &calls, &inv, 100).unwrap();
        let status = |id: &str| {
            v.rows
                .iter()
                .find(|r| r.provider_request_id == id)
                .map(|r| r.status.clone())
                .unwrap()
        };
        assert_eq!(status("a"), "MATCHED");
        assert_eq!(status("b"), "OUT_OF_TOLERANCE", "5% over a 1% tolerance");
        assert_eq!(status("c"), "OTHER_TASK");
        assert_eq!(status("d"), "UNKNOWN_USAGE");
        assert_eq!(status("z"), "NOT_IN_LOG");
        assert_eq!(status("e"), "NOT_ON_INVOICE");
        assert!(!v.within_tolerance);
        // At a 5% tolerance, b is within.
        let v = reconcile_invoice(t, &calls, &inv, 500).unwrap();
        assert_eq!(
            v.rows
                .iter()
                .find(|r| r.provider_request_id == "b")
                .unwrap()
                .status,
            "MATCHED"
        );
        assert!(reconcile_invoice(t, &calls, r#"{"schema":"x"}"#, 0).is_err());
    }

    #[test]
    fn the_difference_is_relative_and_rounded_up() {
        assert_eq!(delta_bp(&[(100, 100)]), 0);
        assert_eq!(delta_bp(&[(101, 100)]), 100);
        assert_eq!(delta_bp(&[(0, 0), (3, 2)]), 3334);
    }
}
