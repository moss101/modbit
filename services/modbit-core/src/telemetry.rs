//! Observability (REQ-PX-139, docs/34): OpenTelemetry export derived from the
//! canonical log and the accounting record, and component health that
//! survives a restart.
//!
//! **Export.** Off unless the Core is configured with an OTLP/HTTP endpoint
//! (`MODBIT_OTLP_ENDPOINT`; header values come from the environment variables
//! named by `MODBIT_OTLP_HEADERS_ENV`, never from the configuration itself),
//! and then only to a host the configuration layers' `network` allow-list
//! admits. When a run ends the Core reads that run's records off the log and
//! turns them into spans — the run, its turns, model requests, tool calls,
//! subagents (linked into the same trace as their parent), compaction epochs
//! and verification runs — with token, cache and cost attributes. The cost on
//! a run span *is* the request accounting record's (`accounting::derive`,
//! REQ-EPR-010), children rolled up; nothing is recomputed here. Spans carry
//! ids, enums, counts, durations and costs only: no prompt, no file body, no
//! tool argument, no path, no goal text — and every string passes the one
//! redactor on its way out. The sender runs on its own task with hard
//! timeouts; a collector that is slow, dead or wrong costs a run nothing and
//! every loss is a counter.
//!
//! **Health.** The components a restart used to forget — provider
//! reachability, the terminal broker, the sandbox gateway, the indexes, the
//! exporter — are observed on an interval and persisted
//! (`modbit_observability::health`); `GetComponentHealth` reports the last
//! known state with its age and says whether this run has observed it yet.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use modbit_domain::{RunId, TaskId};
use modbit_event_store::{EventStore, StoredEvent};
use modbit_observability::health::{HealthState, HealthStore};
use modbit_observability::otlp::{
    Attr, ExportQueue, Headers, Metric, MetricKind, OtlpConfig, Point, Redact, Span,
};
use modbit_protocol::v1 as wire;
use prost::Message;
use sha2::{Digest, Sha256};

use crate::server::{Core, accept, reject};

/// Events read from the log in one pass.
const SCAN_LIMIT: usize = 2_000;
/// Spans one run may contribute (a runaway run cannot flood the queue).
const MAX_SPANS_PER_RUN: usize = 4_000;

/// How export is configured.
#[derive(Clone, Debug)]
pub(crate) struct ExportSettings {
    /// The collector's base URL.
    pub endpoint: String,
    /// `(header name, environment variable holding its value)`.
    pub headers: Vec<(String, String)>,
    /// Sender interval.
    pub interval: Duration,
    /// Export history from the start of the log instead of from now.
    pub from_start: bool,
}

/// Read the export configuration from the environment: `None` when export
/// is not configured (the default).
pub(crate) fn settings_from_env() -> Option<ExportSettings> {
    let endpoint = std::env::var("MODBIT_OTLP_ENDPOINT")
        .ok()
        .map(|e| e.trim().to_owned())
        .filter(|e| !e.is_empty())?;
    let headers = std::env::var("MODBIT_OTLP_HEADERS_ENV")
        .ok()
        .map(|v| {
            v.split(',')
                .filter_map(|pair| {
                    let (name, var) = pair.split_once('=')?;
                    let (name, var) = (name.trim(), var.trim());
                    (!name.is_empty() && !var.is_empty()).then(|| (name.to_owned(), var.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default();
    let interval = std::env::var("MODBIT_OTLP_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v >= 50)
        .map_or(Duration::from_secs(5), Duration::from_millis);
    Some(ExportSettings {
        endpoint,
        headers,
        interval,
        from_start: std::env::var("MODBIT_OTLP_FROM_START").is_ok_and(|v| v == "1"),
    })
}

/// The scheme, host and port of an endpoint, without anything that could
/// carry a credential (userinfo, path, query).
fn origin_of(endpoint: &str) -> Option<(String, String)> {
    let (scheme, rest) = endpoint.split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    if authority.is_empty() {
        return None;
    }
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next()?.to_owned()
    } else {
        authority.split(':').next()?.to_owned()
    };
    Some((host, format!("{scheme}://{authority}")))
}

/// Whether the configuration layers in force admit exporting to `host`: the
/// machine's telemetry level (`off` forbids it) and the network allow-list
/// (no list means nothing restricts egress).
fn policy_admits(data_dir: &Path, host: &str, authority_origin: &str) -> Result<(), String> {
    let layers = crate::config::layers_for(data_dir, None);
    let cfg = modbit_policy::config::resolve(&layers);
    if cfg
        .device
        .as_ref()
        .and_then(|d| d.value.telemetry.as_deref())
        .is_some_and(|level| level.eq_ignore_ascii_case("off"))
    {
        return Err("the device policy sets telemetry to `off`".into());
    }
    let Some(allow) = cfg.network_allow else {
        return Ok(());
    };
    let hostport = authority_origin
        .split_once("://")
        .map_or(authority_origin, |(_, r)| r);
    if allow.value.iter().any(|a| a == host || a == hostport) {
        Ok(())
    } else {
        Err(format!(
            "`{host}` is not in the network allow-list in force"
        ))
    }
}

// ---------------------------------------------------------------- the state

/// What the Core holds for observability.
pub struct Telemetry {
    /// Persisted component health.
    pub(crate) health: HealthStore,
    export: Mutex<Option<Active>>,
    disabled_reason: Mutex<String>,
    counters: Mutex<Counters>,
}

struct Active {
    queue: Arc<ExportQueue>,
    origin: String,
}

#[derive(Default)]
struct Counters {
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
    cost_minor: u64,
    last_model_ms: u64,
    runs_exported: u64,
}

impl Telemetry {
    /// Telemetry for a Core whose profile is `data_dir`.
    #[must_use]
    pub fn new(data_dir: &Path) -> Self {
        Self {
            health: HealthStore::open(data_dir.join("component-health.json")),
            export: Mutex::new(None),
            disabled_reason: Mutex::new(String::new()),
            counters: Mutex::new(Counters::default()),
        }
    }

    /// Record an observation of a component now.
    pub(crate) fn observe(&self, name: &str, state: HealthState, detail: &str) {
        self.health.observe(name, state, detail, now_ms());
    }
}

fn now_ms() -> i64 {
    modbit_domain::Timestamp::now().millis()
}

fn id8(kind: &str, id: &str) -> [u8; 8] {
    let h = Sha256::digest(format!("{kind}:{id}").as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&h[..8]);
    out
}

fn id16(kind: &str, id: &str) -> [u8; 16] {
    let h = Sha256::digest(format!("{kind}:{id}").as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&h[..16]);
    out
}

fn ns(ms: i64) -> u64 {
    u64::try_from(ms).unwrap_or(0).saturating_mul(1_000_000)
}

// --------------------------------------------------------------- the spans

/// Everything the span builder reads about one run.
struct RunFacts<'a> {
    task: TaskId,
    trace: [u8; 16],
    /// The parent span of the run span: the subagent span of the parent
    /// request, for a child's run.
    run_parent: Option<[u8; 8]>,
    events: Vec<(&'a StoredEvent, serde_json::Value)>,
}

/// A task's root: the task, or its parent request when it is a subagent.
fn root_of(store: &EventStore, task: TaskId) -> (TaskId, Option<TaskId>) {
    let Ok(Some(t)) = store.task(&task) else {
        return (task, None);
    };
    let mut parent_of: HashMap<TaskId, TaskId> = HashMap::new();
    for other in store.session_tasks(&t.session_id).unwrap_or_default() {
        for n in store.agent_nodes(&other.task_id).unwrap_or_default() {
            if n.kind != "SUBAGENT" {
                continue;
            }
            if let Some(child) = n.child_task_id
                && child != other.task_id
            {
                parent_of.insert(child, other.task_id);
            }
        }
    }
    let direct = parent_of.get(&task).copied();
    let mut root = task;
    let mut guard = 0;
    while let Some(p) = parent_of.get(&root) {
        root = *p;
        guard += 1;
        if guard > 8 {
            break;
        }
    }
    (root, direct)
}

fn text(v: &serde_json::Value) -> &str {
    v.as_str().unwrap_or_default()
}

/// The spans of one run, from the log and the accounting record.
#[allow(clippy::too_many_lines)]
fn spans_of_run(core: &Core, store: &EventStore, task: TaskId, run: RunId) -> Vec<Span> {
    let Ok(Some(t)) = store.task(&task) else {
        return vec![];
    };
    let (root, direct_parent) = root_of(store, task);
    let facts_trace = id16("trace", &root.to_string());
    let Ok(all) = store.read_session(&t.session_id, 0, usize::MAX) else {
        return vec![];
    };
    // The run's own events, and the task-level records (a subagent's
    // admission and result, a compaction epoch) written inside its window
    // without a run lineage.
    let own: Vec<&StoredEvent> = all
        .iter()
        .filter(|e| e.envelope.run_id == Some(run) && e.envelope.task_id == Some(task))
        .collect();
    let (first, last) = (
        own.iter().map(|e| e.offset).min().unwrap_or(0),
        own.iter().map(|e| e.offset).max().unwrap_or(0),
    );
    let mine: Vec<&StoredEvent> = all
        .iter()
        .filter(|e| {
            e.envelope.task_id == Some(task)
                && (e.envelope.run_id == Some(run)
                    || (e.envelope.run_id.is_none() && (first..=last).contains(&e.offset)))
        })
        .collect();
    let facts = RunFacts {
        task,
        trace: facts_trace,
        run_parent: direct_parent.map(|_| id8("subagent", &task.to_string())),
        events: mine
            .iter()
            .map(|e| (*e, store.payload(&e.envelope).unwrap_or_default()))
            .collect(),
    };
    let registry = core.gateway.registry();
    let (calls, pricing) = crate::usage::calls(store, &t.session_id, registry.as_ref());
    let run_s = run.to_string();
    let run_calls: Vec<&crate::usage::Call> = calls
        .iter()
        .filter(|c| c.task_id == Some(task) && c.run_id == run_s)
        .collect();

    let run_span_id = id8("run", &run_s);
    let mut spans: Vec<Span> = Vec::new();
    let (mut run_start, mut run_end) = (i64::MAX, 0i64);
    let mut run_ok = true;
    let mut terminal = false;
    let mut run_state = String::from("RUNNING");

    // ---- turns, model requests, tool calls, subagents, epochs, verification
    struct Open {
        start_ms: i64,
        attrs: Vec<(String, Attr)>,
        parent: [u8; 8],
        id: [u8; 8],
    }
    let mut turns: HashMap<String, Open> = HashMap::new();
    let mut turn_order: HashMap<String, u32> = HashMap::new();
    let mut tool_open: HashMap<[u8; 16], Open> = HashMap::new();
    let mut tool_epoch: HashMap<[u8; 16], u64> = HashMap::new();
    let mut model_open: Option<(Open, String)> = None;
    let mut model_index: HashMap<String, usize> = HashMap::new();
    let mut subagent_open: HashMap<String, Open> = HashMap::new();
    // The window of log offsets each turn spans, so a record written inside
    // a turn without a turn lineage (a subagent's admission) is attributed
    // to the turn it happened in.
    let windows: Vec<(u64, u64, String)> = {
        let mut open: HashMap<String, u64> = HashMap::new();
        let mut out = Vec::new();
        for (e, _) in &facts.events {
            let Some(t) = e.envelope.turn_id.map(|t| t.to_string()) else {
                continue;
            };
            match e.envelope.event_type.as_str() {
                "TurnPrepared" => {
                    open.insert(t, e.offset);
                }
                "TurnCompleted" | "TurnFailed" | "TurnInterrupted" => {
                    if let Some(start) = open.remove(&t) {
                        out.push((start, e.offset, t));
                    }
                }
                _ => {}
            }
        }
        out.extend(open.into_iter().map(|(t, start)| (start, u64::MAX, t)));
        out
    };
    for (e, p) in &facts.events {
        let env = &e.envelope;
        let at = env.occurred_at.0;
        run_start = run_start.min(at);
        run_end = run_end.max(at);
        let turn_s = env.turn_id.map(|t| t.to_string()).unwrap_or_else(|| {
            windows
                .iter()
                .find(|(a, b, _)| (*a..=*b).contains(&e.offset))
                .map(|(_, _, t)| t.clone())
                .unwrap_or_default()
        });
        let turn_span = if turn_s.is_empty() {
            run_span_id
        } else {
            id8("turn", &turn_s)
        };
        match env.event_type.as_str() {
            "RunCompleted" => {
                terminal = true;
                run_state = "COMPLETED".into();
            }
            "RunFailed" => {
                terminal = true;
                run_ok = false;
                run_state = format!("FAILED:{}", text(&p["failure_code"]));
            }
            "RunCancelled" => {
                terminal = true;
                run_ok = false;
                run_state = "CANCELLED".into();
            }
            "RunSuspended" => run_state = "SUSPENDED".into(),
            "TurnPrepared" => {
                let ordinal = p["ordinal"].as_u64().unwrap_or(0);
                turn_order.insert(turn_s.clone(), u32::try_from(ordinal).unwrap_or(0));
                turns.insert(
                    turn_s.clone(),
                    Open {
                        start_ms: at,
                        attrs: vec![("modbit.turn.ordinal".into(), Attr::from(ordinal))],
                        parent: run_span_id,
                        id: id8("turn", &turn_s),
                    },
                );
            }
            "TurnCompleted" | "TurnFailed" | "TurnInterrupted" => {
                if let Some(mut o) = turns.remove(&turn_s) {
                    let ok = env.event_type == "TurnCompleted";
                    o.attrs.push((
                        "modbit.turn.outcome".into(),
                        Attr::from(match env.event_type.as_str() {
                            "TurnCompleted" => "COMPLETED",
                            "TurnFailed" => "FAILED",
                            _ => "INTERRUPTED",
                        }),
                    ));
                    // The turn's tokens and cost, from the usage ledger the
                    // accounting prices.
                    let in_turn: Vec<&&crate::usage::Call> =
                        run_calls.iter().filter(|c| c.turn_id == turn_s).collect();
                    let (i, c, out) = in_turn.iter().fold((0, 0, 0), |a, k| {
                        (a.0 + k.input, a.1 + k.cached, a.2 + k.output)
                    });
                    o.attrs
                        .push(("gen_ai.usage.input_tokens".into(), Attr::from(i)));
                    o.attrs
                        .push(("gen_ai.usage.output_tokens".into(), Attr::from(out)));
                    o.attrs.push(("modbit.tokens.cached".into(), Attr::from(c)));
                    o.attrs.push((
                        "modbit.cost.minor".into(),
                        Attr::from(in_turn.iter().filter_map(|k| k.cost_minor).sum::<u64>()),
                    ));
                    o.attrs.push((
                        "modbit.cost.unpriced_calls".into(),
                        Attr::from(
                            u64::try_from(
                                in_turn.iter().filter(|k| k.cost_minor.is_none()).count(),
                            )
                            .unwrap_or(0),
                        ),
                    ));
                    spans.push(Span {
                        trace_id: facts.trace,
                        span_id: o.id,
                        parent_span_id: Some(o.parent),
                        name: "modbit.turn".into(),
                        start_ns: ns(o.start_ms),
                        end_ns: ns(at),
                        attrs: o.attrs,
                        ok,
                    });
                }
            }
            "ModelInvocationStarted" => {
                let n = model_index.entry(turn_s.clone()).or_insert(0);
                *n += 1;
                let mut attrs = vec![
                    (
                        "gen_ai.request.model".into(),
                        Attr::from(text(&p["model_route"]["model"]).to_owned()),
                    ),
                    (
                        "modbit.endpoint".into(),
                        Attr::from(text(&p["model_route"]["endpoint"]).to_owned()),
                    ),
                    ("modbit.model.request_ordinal".into(), Attr::from(*n as u64)),
                ];
                if let Some(c) = run_calls.iter().filter(|c| c.turn_id == turn_s).nth(*n - 1) {
                    attrs.push(("gen_ai.usage.input_tokens".into(), Attr::from(c.input)));
                    attrs.push(("gen_ai.usage.output_tokens".into(), Attr::from(c.output)));
                    attrs.push(("modbit.tokens.cached".into(), Attr::from(c.cached)));
                    attrs.push(("modbit.usage.reported".into(), Attr::from(c.reported)));
                    if let Some(m) = c.cost_minor {
                        attrs.push(("modbit.cost.minor".into(), Attr::from(m)));
                    }
                    if !pricing.currency.is_empty() {
                        attrs.push((
                            "modbit.cost.currency".into(),
                            Attr::from(pricing.currency.clone()),
                        ));
                    }
                }
                model_open = Some((
                    Open {
                        start_ms: at,
                        attrs,
                        parent: turn_span,
                        id: id8("model", &format!("{turn_s}:{n}")),
                    },
                    turn_s.clone(),
                ));
            }
            "ModelInvocationCompleted" => {
                if let Some((o, _)) = model_open.take() {
                    spans.push(Span {
                        trace_id: facts.trace,
                        span_id: o.id,
                        parent_span_id: Some(o.parent),
                        name: "modbit.model_request".into(),
                        start_ns: ns(o.start_ms),
                        end_ns: ns(at),
                        attrs: o.attrs,
                        ok: true,
                    });
                    if let Ok(mut c) = core.tools.telemetry.counters.lock() {
                        c.last_model_ms = u64::try_from(at - o.start_ms).unwrap_or(0);
                    }
                }
            }
            "ToolCallProposed" => {
                tool_open.insert(
                    env.aggregate_id,
                    Open {
                        start_ms: at,
                        attrs: vec![
                            (
                                "modbit.tool.name".into(),
                                Attr::from(text(&p["tool_name"]).to_owned()),
                            ),
                            (
                                "modbit.tool.effect_class".into(),
                                Attr::from(text(&p["effect_class"]).to_owned()),
                            ),
                        ],
                        parent: turn_span,
                        id: id8("tool", &hex::encode(env.aggregate_id)),
                    },
                );
            }
            "ToolCallPolicyDecision" => {
                if let Some(epoch) = p["authorization"]["epoch"].as_u64() {
                    tool_epoch.insert(env.aggregate_id, epoch);
                }
            }
            "ToolCallSucceeded"
            | "ToolCallFailed"
            | "ToolCallCancelled"
            | "ToolCallUnknownOutcome" => {
                if let Some(mut o) = tool_open.remove(&env.aggregate_id) {
                    o.attrs.push((
                        "modbit.tool.outcome".into(),
                        Attr::from(env.event_type.trim_start_matches("ToolCall").to_uppercase()),
                    ));
                    if env.event_type == "ToolCallFailed" {
                        o.attrs.push((
                            "modbit.tool.failure_code".into(),
                            Attr::from(text(&p["failure_code"]).to_owned()),
                        ));
                    }
                    if let Some(epoch) = tool_epoch.remove(&env.aggregate_id) {
                        o.attrs
                            .push(("modbit.authorization.epoch".into(), Attr::from(epoch)));
                    }
                    spans.push(Span {
                        trace_id: facts.trace,
                        span_id: o.id,
                        parent_span_id: Some(o.parent),
                        name: "modbit.tool_call".into(),
                        start_ns: ns(o.start_ms),
                        end_ns: ns(at),
                        attrs: o.attrs,
                        ok: env.event_type == "ToolCallSucceeded",
                    });
                }
            }
            "SubagentAdmitted" => {
                let child = text(&p["child_task_id"]).to_owned();
                subagent_open.insert(
                    child.clone(),
                    Open {
                        start_ms: at,
                        attrs: vec![
                            ("modbit.subagent.task".into(), Attr::from(child.clone())),
                            (
                                "modbit.subagent.mode".into(),
                                Attr::from(text(&p["mode"]).to_owned()),
                            ),
                        ],
                        parent: turn_span,
                        id: id8("subagent", &child),
                    },
                );
            }
            "SubagentResultRecorded" => {
                let child = text(&p["child_task_id"]).to_owned();
                if let Some(mut o) = subagent_open.remove(&child) {
                    let status = text(&p["status"]).to_owned();
                    o.attrs
                        .push(("modbit.subagent.status".into(), Attr::from(status.clone())));
                    // The child's rolled-up cost, from the accounting record
                    // its parent's own record sums.
                    if let Ok(cid) = TaskId::parse(&child)
                        && let Some(rec) =
                            crate::accounting::derive(store, core.tenant_id, cid, None)
                    {
                        o.attrs.push((
                            "modbit.cost.minor".into(),
                            Attr::from(rec.cost.subtree_minor),
                        ));
                    }
                    spans.push(Span {
                        trace_id: facts.trace,
                        span_id: o.id,
                        parent_span_id: Some(o.parent),
                        name: "modbit.subagent".into(),
                        start_ns: ns(o.start_ms),
                        end_ns: ns(at),
                        attrs: o.attrs,
                        ok: status == "COMPLETED",
                    });
                }
            }
            "ContextEpochOpened" => {
                spans.push(Span {
                    trace_id: facts.trace,
                    span_id: id8("epoch", &format!("{run_s}:{}", e.offset)),
                    parent_span_id: Some(turn_span),
                    name: "modbit.compaction_epoch".into(),
                    start_ns: ns(at),
                    end_ns: ns(at),
                    attrs: vec![
                        (
                            "modbit.compaction.epoch".into(),
                            Attr::from(p["epoch"].as_u64().unwrap_or(0)),
                        ),
                        (
                            "modbit.compaction.source_entries".into(),
                            Attr::from(p["source_entries"].as_u64().unwrap_or(0)),
                        ),
                        (
                            "modbit.compaction.projection_tokens".into(),
                            Attr::from(p["projection_tokens"].as_u64().unwrap_or(0)),
                        ),
                    ],
                    ok: true,
                });
            }
            "VerificationRunRecorded" => {
                let ms: u64 = p["checks"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|c| c["duration_ms"].as_u64().unwrap_or(0))
                            .sum()
                    })
                    .unwrap_or(0);
                let status = text(&p["status"]).to_owned();
                spans.push(Span {
                    trace_id: facts.trace,
                    span_id: id8("verification", &format!("{run_s}:{}", e.offset)),
                    parent_span_id: Some(turn_span),
                    name: "modbit.verification_run".into(),
                    start_ns: ns(at).saturating_sub(ms.saturating_mul(1_000_000)),
                    end_ns: ns(at),
                    attrs: vec![
                        (
                            "modbit.verification.stage".into(),
                            Attr::from(text(&p["stage"]).to_owned()),
                        ),
                        (
                            "modbit.verification.status".into(),
                            Attr::from(status.clone()),
                        ),
                        (
                            "modbit.verification.checks".into(),
                            Attr::from(p["checks"].as_array().map_or(0, Vec::len) as u64),
                        ),
                    ],
                    ok: status == "PASSED",
                });
            }
            _ => {}
        }
    }
    // A turn, tool call or model request still open when the run ended (a
    // kill, a cancel) is closed at the run's last event and marked.
    for (_, mut o) in turns {
        o.attrs
            .push(("modbit.turn.outcome".into(), Attr::from("OPEN")));
        spans.push(Span {
            trace_id: facts.trace,
            span_id: o.id,
            parent_span_id: Some(o.parent),
            name: "modbit.turn".into(),
            start_ns: ns(o.start_ms),
            end_ns: ns(run_end),
            attrs: o.attrs,
            ok: false,
        });
    }
    for (_, mut o) in tool_open {
        o.attrs
            .push(("modbit.tool.outcome".into(), Attr::from("OPEN")));
        spans.push(Span {
            trace_id: facts.trace,
            span_id: o.id,
            parent_span_id: Some(o.parent),
            name: "modbit.tool_call".into(),
            start_ns: ns(o.start_ms),
            end_ns: ns(run_end),
            attrs: o.attrs,
            ok: false,
        });
    }
    // ---- the run span: the accounting record's cost, children rolled up
    if terminal && run_start != i64::MAX {
        let mut attrs: Vec<(String, Attr)> = vec![
            ("modbit.task.id".into(), Attr::from(facts.task.to_string())),
            ("modbit.run.id".into(), Attr::from(run_s.clone())),
            ("modbit.run.state".into(), Attr::from(run_state)),
            ("modbit.turns".into(), Attr::from(turn_order.len() as u64)),
            (
                "modbit.subagent".into(),
                Attr::from(facts.run_parent.is_some()),
            ),
        ];
        if let Some(rec) = crate::accounting::derive(store, core.tenant_id, task, None) {
            attrs.push(("modbit.cost.minor".into(), Attr::from(rec.cost.total_minor)));
            attrs.push((
                "modbit.cost.children_minor".into(),
                Attr::from(rec.cost.children_minor),
            ));
            attrs.push((
                "modbit.cost.subtree_minor".into(),
                Attr::from(rec.cost.subtree_minor),
            ));
            attrs.push((
                "modbit.cost.complete".into(),
                Attr::from(rec.cost.subtree_complete),
            ));
            attrs.push((
                "modbit.cost.currency".into(),
                Attr::from(rec.cost.currency.clone()),
            ));
            attrs.push((
                "gen_ai.usage.input_tokens".into(),
                Attr::from(rec.cost.input_tokens),
            ));
            attrs.push((
                "gen_ai.usage.output_tokens".into(),
                Attr::from(rec.cost.output_tokens),
            ));
            attrs.push((
                "modbit.tokens.cached".into(),
                Attr::from(rec.cost.cached_input_tokens),
            ));
            attrs.push((
                "modbit.tokens.cache_write".into(),
                Attr::from(rec.cost.cache_write_tokens),
            ));
            attrs.push((
                "modbit.verified".into(),
                Attr::from(rec.request.verified_success),
            ));
            if let Ok(mut c) = core.tools.telemetry.counters.lock() {
                c.input_tokens += rec.cost.input_tokens;
                c.output_tokens += rec.cost.output_tokens;
                c.cached_tokens += rec.cost.cached_input_tokens;
                c.cost_minor += rec.cost.total_minor;
                c.runs_exported += 1;
            }
        }
        spans.push(Span {
            trace_id: facts.trace,
            span_id: run_span_id,
            parent_span_id: facts.run_parent,
            name: "modbit.run".into(),
            start_ns: ns(run_start),
            end_ns: ns(run_end),
            attrs,
            ok: run_ok,
        });
    }
    spans.truncate(MAX_SPANS_PER_RUN);
    spans
}

// -------------------------------------------------------------- the exporter

fn cursor_path(data_dir: &Path) -> PathBuf {
    data_dir.join("otlp-cursor.json")
}

fn read_cursor(data_dir: &Path) -> Option<u64> {
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cursor_path(data_dir)).ok()?).ok()?;
    v["offset"].as_u64()
}

fn write_cursor(data_dir: &Path, offset: u64) {
    let tmp = cursor_path(data_dir).with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::json!({"offset": offset}).to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, cursor_path(data_dir));
    }
}

/// Start observability: the health monitor always, the exporter when the
/// Core is configured to export.
pub(crate) fn spawn(core: Arc<Core>) {
    spawn_health(Arc::clone(&core));
    let Some(settings) = settings_from_env() else {
        core.tools.telemetry.observe(
            "telemetry",
            HealthState::Disabled,
            "export is not configured (MODBIT_OTLP_ENDPOINT)",
        );
        return;
    };
    let Some((host, origin)) = origin_of(&settings.endpoint) else {
        *core
            .tools
            .telemetry
            .disabled_reason
            .lock()
            .unwrap_or_else(|e| e.into_inner()) =
            "the configured endpoint is not an http(s) URL".into();
        core.tools.telemetry.observe(
            "telemetry",
            HealthState::Disabled,
            "the configured endpoint is not an http(s) URL",
        );
        return;
    };
    if let Err(why) = policy_admits(&core.data_dir, &host, &origin) {
        *core
            .tools
            .telemetry
            .disabled_reason
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = why.clone();
        core.tools
            .telemetry
            .observe("telemetry", HealthState::Disabled, &why);
        return;
    }
    let queue = ExportQueue::new(2_048);
    *core
        .tools
        .telemetry
        .export
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(Active {
        queue: Arc::clone(&queue),
        origin: origin.clone(),
    });
    let mut cfg = OtlpConfig::new(settings.endpoint.clone());
    cfg.interval = settings.interval;
    cfg.resource = vec![(
        "modbit.core.boot_generation".into(),
        Attr::from(core.recovery().boot_generation),
    )];
    let weak: Weak<Core> = Arc::downgrade(&core);
    let redact: Redact = {
        let weak = weak.clone();
        Arc::new(move |s: &str| match weak.upgrade() {
            Some(core) => core.tools.redactor().error_text(s),
            None => s.to_owned(),
        })
    };
    // The exporter's headers are credentials like any other: registered with
    // the broker (their source is the environment variable the configuration
    // names, never a value of the configuration) and obtained from it at
    // each send, so a rotation or a revocation applies to the next request.
    let audience = format!("otlp:{host}");
    let mut header_ids: Vec<(String, modbit_secrets::CredentialId)> = Vec::new();
    for (name, var) in &settings.headers {
        let id = modbit_secrets::CredentialId::new(
            modbit_secrets::Kind::Telemetry,
            &name.to_ascii_lowercase(),
        );
        core.tools.broker().register(modbit_secrets::Registration {
            id: id.clone(),
            kind: modbit_secrets::Kind::Telemetry,
            source: modbit_secrets::SecretHandle::Env(var.clone()),
            audience: "otlp:*".into(),
        });
        header_ids.push((name.clone(), id));
    }
    let header_broker = Arc::clone(core.tools.broker());
    let headers: Headers = Arc::new(move || {
        header_ids
            .iter()
            .filter_map(|(name, id)| {
                let secret = header_broker
                    .acquire(
                        id,
                        &modbit_secrets::Use {
                            principal: "core".into(),
                            audience: audience.clone(),
                            purpose: "telemetry.export".into(),
                            nonce: None,
                        },
                    )
                    .ok()?;
                Some((name.clone(), secret.expose().to_owned()))
            })
            .collect()
    });
    tokio::spawn(modbit_observability::otlp::run_sender(
        Arc::clone(&queue),
        cfg,
        headers,
        redact,
    ));
    // The scanner: when a run ends, its spans are built and queued.
    let from_start = settings.from_start;
    tokio::spawn(async move {
        let mut cursor = match read_cursor(&core.data_dir) {
            Some(c) => c,
            None if from_start => 0,
            None => core.store.lock().await.last_offset().unwrap_or(0),
        };
        let mut done: HashSet<(TaskId, RunId, u64)> = HashSet::new();
        loop {
            tokio::time::sleep(Duration::from_millis(
                100.max(u64::try_from(settings.interval.as_millis() / 4).unwrap_or(250)),
            ))
            .await;
            let ended = {
                let store = core.store.lock().await;
                let batch = store.read_all_after(cursor, SCAN_LIMIT).unwrap_or_default();
                if let Some(last) = batch.last() {
                    cursor = last.offset;
                }
                batch
                    .iter()
                    .filter(|e| {
                        matches!(
                            e.envelope.event_type.as_str(),
                            "RunCompleted" | "RunFailed" | "RunCancelled"
                        )
                    })
                    .filter_map(|e| Some((e.envelope.task_id?, e.envelope.run_id?, e.offset)))
                    .collect::<Vec<_>>()
            };
            for (task, run, offset) in ended {
                if !done.insert((task, run, offset)) {
                    continue;
                }
                let spans = {
                    let store = core.store.lock().await;
                    spans_of_run(&core, &store, task, run)
                };
                if !spans.is_empty() {
                    queue.push(spans);
                }
            }
            if cursor > 0 {
                write_cursor(&core.data_dir, cursor);
            }
            queue.set_metrics(metrics(&core, &queue));
        }
    });
}

fn metrics(core: &Core, queue: &ExportQueue) -> Vec<Metric> {
    let c = core
        .tools
        .telemetry
        .counters
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let st = queue.stats();
    let sum = |name: &str, unit: &str, points: Vec<Point>| Metric {
        name: name.into(),
        unit: unit.into(),
        kind: MetricKind::Sum,
        points,
    };
    let int = |attrs: Vec<(String, Attr)>, v: u64| Point {
        attrs,
        value: v as f64,
        integral: true,
    };
    vec![
        sum(
            "modbit.tokens",
            "{token}",
            vec![
                int(
                    vec![("modbit.token.type".into(), Attr::from("input"))],
                    c.input_tokens,
                ),
                int(
                    vec![("modbit.token.type".into(), Attr::from("output"))],
                    c.output_tokens,
                ),
                int(
                    vec![("modbit.token.type".into(), Attr::from("cached"))],
                    c.cached_tokens,
                ),
            ],
        ),
        sum("modbit.cost", "{minor}", vec![int(vec![], c.cost_minor)]),
        sum(
            "modbit.runs.exported",
            "{run}",
            vec![int(vec![], c.runs_exported)],
        ),
        Metric {
            name: "modbit.model.latency".into(),
            unit: "ms".into(),
            kind: MetricKind::Gauge,
            points: vec![int(vec![], c.last_model_ms)],
        },
        Metric {
            name: "modbit.export.queue_depth".into(),
            unit: "{span}".into(),
            kind: MetricKind::Gauge,
            points: vec![int(vec![], st.queue_depth)],
        },
    ]
}

// ------------------------------------------------------------ health monitor

fn health_interval() -> Duration {
    Duration::from_millis(
        std::env::var("MODBIT_HEALTH_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v >= 50)
            .unwrap_or(2_000),
    )
}

fn spawn_health(core: Arc<Core>) {
    tokio::spawn(async move {
        loop {
            observe_components(&core).await;
            tokio::time::sleep(health_interval()).await;
        }
    });
}

async fn observe_components(core: &Arc<Core>) {
    let t = &core.tools.telemetry;
    // Providers: reachability from what the gateway has seen.
    for ep in core.gateway.endpoints() {
        let h = core.gateway.health(&ep.name);
        let (state, detail) = if h.requests == 0 {
            (HealthState::Unknown, "no request has been made".to_owned())
        } else if h.failures == 0 {
            (
                HealthState::Ok,
                format!("{} request(s), none failed", h.requests),
            )
        } else if h.successes > 0 {
            (
                HealthState::Degraded,
                format!("{} of {} request(s) failed", h.failures, h.requests),
            )
        } else {
            (
                HealthState::Down,
                format!("{} request(s), all failed", h.failures),
            )
        };
        t.observe(&format!("provider:{}", ep.name), state, &detail);
    }
    // The sandbox gateway, when this Core provisions from one.
    let gateway = core.tools.sandbox_gateway.lock().await.clone();
    match gateway {
        None => t.observe(
            "sandbox_gateway",
            HealthState::Disabled,
            "no sandbox gateway is configured",
        ),
        Some(g) => {
            let probe = tokio::time::timeout(Duration::from_secs(2), g.client.features()).await;
            match probe {
                Ok(Ok(_)) => t.observe(
                    "sandbox_gateway",
                    HealthState::Ok,
                    "answered a features request",
                ),
                Ok(Err(e)) => t.observe(
                    "sandbox_gateway",
                    HealthState::Down,
                    &core.tools.redactor().error_text(&e.to_string()),
                ),
                Err(_) => t.observe("sandbox_gateway", HealthState::Down, "no answer in 2s"),
            }
        }
    }
    // The exporter.
    let export = core
        .tools
        .telemetry
        .export
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|a| a.queue.stats());
    if let Some(st) = export {
        let (state, detail) = if st.requests_failed > 0 && st.last_success_ms == 0 {
            (
                HealthState::Down,
                format!(
                    "{}: {} failed request(s)",
                    st.last_error, st.requests_failed
                ),
            )
        } else if !st.last_error.is_empty() {
            (HealthState::Degraded, st.last_error.clone())
        } else if st.requests_ok > 0 {
            (HealthState::Ok, format!("{} span(s) sent", st.spans_sent))
        } else {
            (HealthState::Unknown, "nothing exported yet".to_owned())
        };
        t.observe("telemetry", state, &detail);
    }
}

/// `GetComponentHealth`.
pub(crate) async fn component_health(core: &Core, env: wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    if wire::GetComponentHealth::decode(env.payload.as_slice()).is_err() {
        return reject(cid, "BAD_PAYLOAD", "GetComponentHealth");
    }
    let now = now_ms();
    let components = core
        .tools
        .telemetry
        .health
        .view(now)
        .into_iter()
        .map(|c| wire::ComponentHealthView {
            name: c.observation.name,
            state: c.observation.state.name().to_owned(),
            detail: c.observation.detail,
            observed_at_ms: c.observation.observed_at_ms,
            age_ms: c.age_ms,
            observed_this_run: c.observed_this_run,
        })
        .collect();
    let guard = core
        .tools
        .telemetry
        .export
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let reason = core
        .tools
        .telemetry
        .disabled_reason
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let export = match guard.as_ref() {
        Some(a) => {
            let s = a.queue.stats();
            wire::TelemetryExportView {
                enabled: true,
                endpoint: a.origin.clone(),
                disabled_reason: String::new(),
                spans_queued: s.spans_queued,
                spans_sent: s.spans_sent,
                spans_dropped_overflow: s.spans_dropped_overflow,
                spans_lost: s.spans_lost,
                requests_ok: s.requests_ok,
                requests_failed: s.requests_failed,
                metric_exports_ok: s.metric_exports_ok,
                metric_exports_failed: s.metric_exports_failed,
                queue_depth: s.queue_depth,
                last_error: s.last_error,
                last_success_ms: s.last_success_ms,
                last_attempt_ms: s.last_attempt_ms,
            }
        }
        None => wire::TelemetryExportView {
            enabled: false,
            disabled_reason: if reason.is_empty() {
                "export is not configured".into()
            } else {
                reason
            },
            ..Default::default()
        },
    };
    accept(
        cid,
        false,
        wire::ComponentHealthList {
            components,
            now_ms: now,
            export: Some(export),
        }
        .encode_to_vec(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_origin_of_an_endpoint_never_carries_a_credential() {
        assert_eq!(
            origin_of("http://user:pw@127.0.0.1:4318/v1/traces?token=abc"),
            Some(("127.0.0.1".into(), "http://127.0.0.1:4318".into()))
        );
        assert_eq!(
            origin_of("https://otel.example.com/x"),
            Some(("otel.example.com".into(), "https://otel.example.com".into()))
        );
        assert_eq!(
            origin_of("http://[::1]:4318"),
            Some(("::1".into(), "http://[::1]:4318".into()))
        );
        assert_eq!(origin_of("ftp://x"), None);
        assert_eq!(origin_of("not a url"), None);
        assert_eq!(origin_of("http://"), None);
    }

    #[test]
    fn ids_are_deterministic_and_kind_separated() {
        assert_eq!(id8("run", "a"), id8("run", "a"));
        assert_ne!(id8("run", "a"), id8("turn", "a"));
        assert_ne!(id16("trace", "a"), id16("trace", "b"));
    }
}
