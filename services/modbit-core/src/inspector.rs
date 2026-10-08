//! Context Inspector (REQ-EV-0035 / 0131 / 0175, docs/18 "Context Pack"):
//! what the Context Pack selected, why, at which revision and token cost,
//! what it left out, and what the prompt envelope actually injected on the
//! last compiled turn. Nothing here is invented: every field comes from the
//! task's Context Ledger and from the ContextCompile step the runtime
//! recorded, so the inspector's ids are the envelope's ids.

use modbit_domain::TaskId;
use modbit_protocol::v1 as wire;

use crate::server::Core;

/// The inspector view of a task.
pub(crate) async fn view(core: &Core, task_id: TaskId) -> wire::ContextInspectorView {
    let ledger = core.tools.ledger(&core.store, task_id).await;
    let ledger = ledger.lock().await;
    let mut v = wire::ContextInspectorView::default();
    // What the last compiled turn injected and refused (the prompt envelope).
    if let Some((pack_id, injected, rejected, memory)) = last_compiled(core, task_id).await {
        v.context_pack_id = pack_id;
        v.injected_refs = injected;
        v.rejected_refs = rejected;
        // PX-113: the memory the envelope injected, with ids and provenance.
        v.memory = memory.as_ref().map(memory_view);
    }
    // What the user has selected (REQ-EV-0141 / 0160): the same selection
    // retrieval prefers, so a client can see why an entry is in the pack.
    let selection = crate::tools::selection_of(&core.store, task_id).await;
    v.selection_paths = selection.paths.clone();
    v.selection_symbol = selection.symbol.clone().unwrap_or_default();
    v.selection_source = selection.source.clone();
    v.selection_review_hunks = selection.review_hunks.clone();
    // What compaction did to the transcript, and what that cost the prompt
    // cache (docs/19; REQ-EV-0111 / 0268).
    let economy = epochs_and_cache(core, task_id).await;
    v.compaction_epoch = economy.epoch;
    v.compaction_epochs = economy.epochs;
    v.compacted_entries = economy.compacted_entries;
    v.compactions = core
        .store
        .lock()
        .await
        .compaction_epochs(&task_id)
        .unwrap_or_default()
        .into_iter()
        .map(|r| wire::CompactionRequestView {
            compaction_id: r.epoch_id,
            epoch: r.epoch,
            branch_generation: r.branch_generation,
            status: r.status,
            mode: r.mode,
            source_event_start: r.source_event_start,
            source_event_end: r.source_event_end,
            source_entries: r.source_entries,
            result_object_hash: r.result_object_hash.unwrap_or_default(),
            rejection: r.rejection.unwrap_or_default(),
            created_at_ms: r.created_at.0,
            committed_at_ms: r.committed_at.map_or(0, |t| t.0),
        })
        .collect();
    // REQ-PX-107/108/109: the instruction layers in force, the goal-seeded
    // pre-turn step and how each summary was made, from the task's log.
    let task_log = task_context_log(core, task_id).await;
    v.instructions = task_log.instructions;
    v.pre_turn_pack = task_log.pre_turn_pack;
    v.compaction_summaries = task_log.summaries;
    // What the trigger derives from: the numbers the last compaction started
    // under, else what the routed model's window gives now.
    v.compaction_thresholds = task_log.last_started.or_else(|| {
        let (endpoint, model) = economy.last_route.as_ref()?;
        let th = crate::compaction_model::thresholds(core, endpoint, model, 0);
        Some(wire::CompactionThresholdView {
            context_window_tokens: th.window,
            hard_budget_tokens: th.hard,
            soft_budget_tokens: th.soft,
            source: th.source.label().to_owned(),
            estimator: th.estimator().to_owned(),
        })
    });
    v.manifest_ref = economy.manifest_ref;
    v.prefix_cache_hits = economy.hits;
    v.prefix_cache_misses = economy.misses;
    v.reported_input_tokens = economy.reported_input_tokens;
    v.reported_cached_input_tokens = economy.reported_cached_input_tokens;
    v.reported_invocations = economy.reported_invocations;
    // REQ-PX-059: the same breakdown `GetContextAccounting` serves.
    v.accounting = Some(accounting(core, task_id).await);
    let Some(pack) = ledger.last_pack.as_ref() else {
        return v;
    };
    v.pack_id = pack.pack_id.clone();
    v.workspace_revision = pack.workspace_revision;
    v.token_budget = pack.token_budget;
    v.token_used = pack.token_used;
    v.complete = pack.complete;
    v.compiler_version = pack.compiler_version.clone();
    v.token_estimator = pack.token_estimator.clone();
    v.omitted_count = u32::try_from(pack.omitted_summary.count).unwrap_or(u32::MAX);
    v.omitted_tokens = pack.omitted_summary.token_cost;
    v.omitted_paths = pack.omitted_summary.paths.clone();
    let used_of = |entry_id: &str| {
        ledger
            .entries
            .iter()
            .filter(|e| e.entry_id == entry_id)
            .any(|e| e.used.is_some())
    };
    for e in &pack.entries {
        let injected = v.injected_refs.contains(&e.source_ref);
        v.entries.push(wire::ContextEntryView {
            entry_id: e.entry_id.clone(),
            source_ref: e.source_ref.clone(),
            path: e.provenance.path.clone(),
            line_start: e.lines.map_or(0, |(a, _)| a),
            line_end: e.lines.map_or(0, |(_, b)| b),
            reason: e.reason.clone(),
            retrieval_reasons: e.provenance.retrieval_reasons.clone(),
            sources: e.provenance.sources.clone(),
            freshness: e.freshness.clone(),
            token_cost: e.token_cost,
            content_hash: e.provenance.content_hash.clone().unwrap_or_default(),
            workspace_revision: e.provenance.workspace_revision,
            stub: false,
            injected,
            used: used_of(&e.entry_id),
            language_state: Some(language_state(&e.provenance.path)),
        });
        if injected {
            v.injected_tokens += u64::from(e.token_cost);
        }
    }
    for st in &pack.stubs {
        v.entries.push(wire::ContextEntryView {
            entry_id: st.entry_id.clone(),
            source_ref: st.source_ref.clone(),
            path: st.provenance.path.clone(),
            line_start: st.lines.map_or(0, |(a, _)| a),
            line_end: st.lines.map_or(0, |(_, b)| b),
            reason: format!("stub; hydrate with `{}`", st.hydrate),
            retrieval_reasons: st.provenance.retrieval_reasons.clone(),
            sources: st.provenance.sources.clone(),
            freshness: "committed".into(),
            token_cost: st.token_cost,
            content_hash: st.provenance.content_hash.clone().unwrap_or_default(),
            workspace_revision: st.provenance.workspace_revision,
            stub: true,
            injected: false,
            used: used_of(&st.entry_id),
            language_state: Some(language_state(&st.provenance.path)),
        });
    }
    v
}

/// What the product claims about one path's language (PX-029), as clients
/// show it.
fn language_state(path: &str) -> wire::LanguageStateView {
    let s = crate::tools::state_of(path);
    wire::LanguageStateView {
        path: path.to_owned(),
        language: s.language,
        tier: s.tier.map(|t| format!("{t:?}")).unwrap_or_default(),
        structural: s.structural,
        needs_opt_in: s.needs_opt_in,
        label: s.label,
        degradation: s.degradation,
    }
}

/// The compaction epochs of a task and the prompt-cache economics of the
/// prefix they move.
#[derive(Default)]
struct Economy {
    epoch: u32,
    epochs: u32,
    compacted_entries: u64,
    manifest_ref: String,
    /// By cache key: what the prompt asked the provider to reuse.
    hits: u32,
    misses: u32,
    /// What the provider reported serving from its cache, over the
    /// invocations whose usage it actually reported.
    reported_input_tokens: u64,
    reported_cached_input_tokens: u64,
    reported_invocations: u32,
    /// The routed model of the newest invocation, as `(endpoint, model)`.
    last_route: Option<(String, String)>,
}

/// Counted from the log, never estimated: every epoch the task opened, and
/// every model invocation's cache key in order. A turn is a hit when it routed
/// on the same stable prefix as the turn before it.
async fn epochs_and_cache(core: &Core, task_id: TaskId) -> Economy {
    let mut out = Economy::default();
    let store = core.store.lock().await;
    let Ok(Some(task)) = store.task(&task_id) else {
        return out;
    };
    let Ok(events) = store.read_session(&task.session_id, 0, 200_000) else {
        return out;
    };
    let mut last_key: Option<String> = None;
    for e in &events {
        if e.envelope.task_id != Some(task_id) {
            continue;
        }
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        match e.envelope.event_type.as_str() {
            "ContextEpochOpened" => {
                out.epochs += 1;
                out.epoch = u32::try_from(p["epoch"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                out.compacted_entries += p["source_entries"].as_u64().unwrap_or(0);
                out.manifest_ref = p["manifest_ref"].as_str().unwrap_or_default().to_owned();
            }
            "ModelInvocationStarted" => {
                if let (Some(ep), Some(m)) = (
                    p["model_route"]["endpoint"].as_str(),
                    p["model_route"]["model"].as_str(),
                ) {
                    out.last_route = Some((ep.to_owned(), m.to_owned()));
                }
                let Some(key) = p["model_route"]["cache_key"].as_str() else {
                    continue;
                };
                if last_key.as_deref() == Some(key) {
                    out.hits += 1;
                } else {
                    out.misses += 1;
                }
                last_key = Some(key.to_owned());
            }
            // The provider's own number (FIX-13): a key that repeats says the
            // prompt could be reused, only the provider's usage says it was.
            // An invocation that dropped before its usage frame is unknown,
            // never counted as a miss.
            "ModelUsageRecorded" if p["reported"].as_bool().unwrap_or(false) => {
                out.reported_invocations += 1;
                out.reported_input_tokens += p["input_tokens"].as_u64().unwrap_or(0);
                out.reported_cached_input_tokens += p["cached_input_tokens"].as_u64().unwrap_or(0);
            }
            _ => {}
        }
    }
    out
}

/// The memory record of a ContextCompile step as the Inspector shows it
/// (PX-113): ids and provenance only, never the memory text.
fn memory_view(m: &serde_json::Value) -> wire::MemoryInjectionView {
    let text = |v: &serde_json::Value, k: &str| v[k].as_str().unwrap_or_default().to_owned();
    let strings = |v: &serde_json::Value, k: &str| -> Vec<String> {
        v[k].as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let count = |v: &serde_json::Value, k: &str| {
        u32::try_from(v[k].as_u64().unwrap_or(0)).unwrap_or(u32::MAX)
    };
    wire::MemoryInjectionView {
        pack_id: text(m, "pack_id"),
        token_budget: count(m, "token_budget"),
        token_used: count(m, "token_used"),
        omitted_count: count(m, "omitted_count"),
        entries: m["entries"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| wire::MemoryInjectedEntry {
                        memory_id: text(e, "memory_id"),
                        scope: text(e, "scope"),
                        record_type: text(e, "record_type"),
                        topic: text(e, "topic"),
                        source: text(e, "source"),
                        author: text(e, "author"),
                        confidence: e["confidence"].as_f64().unwrap_or(0.0) as f32,
                        validated: e["validated"].as_bool().unwrap_or(false),
                        token_cost: count(e, "token_cost"),
                        reasons: strings(e, "reasons"),
                        conflicts_with: strings(e, "conflicts_with"),
                        clipped: e["clipped"].as_bool().unwrap_or(false),
                        created_at_ms: e["created_at_ms"].as_i64().unwrap_or(0),
                        expires_at_ms: e["expires_at_ms"].as_i64().unwrap_or(0),
                        last_validation_revision: text(e, "last_validation_revision"),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        excluded: m["excluded"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| wire::MemoryExclusionView {
                        memory_id: text(e, "memory_id"),
                        reason: text(e, "reason"),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        rejected_ids: strings(m, "rejected_ids"),
        compiler_version: text(m, "compiler_version"),
    }
}

/// What a ContextCompile step recorded: (context pack id, injected refs,
/// rejected refs, the memory record).
type Compiled = (String, Vec<String>, Vec<String>, Option<serde_json::Value>);

/// The last ContextCompile step's record.
/// The step events live on their own RunStep aggregates, so the session log is
/// the place that has them all in order.
async fn last_compiled(core: &Core, task_id: TaskId) -> Option<Compiled> {
    let store = core.store.lock().await;
    let task = store.task(&task_id).ok()??;
    let events = store.read_session(&task.session_id, 0, 200_000).ok()?;
    let mut found = None;
    for e in &events {
        if e.envelope.task_id != Some(task_id) || e.envelope.event_type != "StepSucceeded" {
            continue;
        }
        let Ok(payload) = store.payload(&e.envelope) else {
            continue;
        };
        let Some(r) = payload["output_ref"].as_str() else {
            continue;
        };
        let Ok(bytes) = store.objects().get(r) else {
            continue;
        };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        if v.get("segment_hashes").is_some() {
            let strings = |k: &str| -> Vec<String> {
                v[k].as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let memory = v.get("memory").cloned().map(|mut m| {
                m["rejected_ids"] = serde_json::json!(strings("rejected_memory"));
                m
            });
            found = Some((
                v["segment_hashes"][3]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                strings("injected_fragments"),
                strings("rejected_fragments"),
                memory,
            ));
        }
    }
    found
}

/// What the Inspector reads from the task aggregate in one pass.
#[derive(Default)]
struct TaskContextLog {
    instructions: Vec<wire::InstructionLayerView>,
    pre_turn_pack: Option<wire::PreTurnPackView>,
    summaries: Vec<wire::CompactionSummaryView>,
    last_started: Option<wire::CompactionThresholdView>,
}

/// The newest `RulesSelected` (every instruction layer in force, and every
/// file that exists and is not), the newest pre-turn pack record and every
/// epoch's summary provenance (REQ-PX-107 / 108 / 109). Counted from the log,
/// so a restarted Core shows the same.
async fn task_context_log(core: &Core, task_id: TaskId) -> TaskContextLog {
    const PAGE: usize = 5_000;
    let mut out = TaskContextLog::default();
    let store = core.store.lock().await;
    let mut after = 0u64;
    loop {
        let Ok(events) = store.read_aggregate(task_id.as_bytes(), after, PAGE) else {
            break;
        };
        for e in &events {
            after = e.envelope.sequence;
            let Ok(p) = store.payload(&e.envelope) else {
                continue;
            };
            match e.envelope.event_type.as_str() {
                "RulesSelected" => out.instructions = instruction_views(&p),
                "ContextPackRecorded" if !p["trigger"].as_str().unwrap_or_default().is_empty() => {
                    let n = |k: &str| u32::try_from(p[k].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                    let s = |k: &str| p[k].as_str().unwrap_or_default().to_owned();
                    out.pre_turn_pack = Some(wire::PreTurnPackView {
                        trigger: s("trigger"),
                        status: s("status"),
                        reason: s("reason"),
                        pack_id: s("pack_id"),
                        pack_ref: s("pack_ref"),
                        token_budget: n("token_budget"),
                        token_used: n("token_used"),
                        entries: n("entries"),
                        stubs: n("stubs"),
                        workspace_revision: p["workspace_revision"].as_u64().unwrap_or(0),
                        seed_digest: s("seed_digest"),
                        offset: e.offset,
                    });
                }
                "CompactionStarted" => {
                    let n = |k: &str| u32::try_from(p[k].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                    let source = p["budget_source"].as_str().unwrap_or_default();
                    out.last_started =
                        (!source.is_empty()).then(|| wire::CompactionThresholdView {
                            context_window_tokens: n("window_tokens"),
                            hard_budget_tokens: n("budget_tokens"),
                            soft_budget_tokens: if source == "MODEL_WINDOW" {
                                n("budget_tokens") * 14 / 17
                            } else {
                                n("budget_tokens") * 3 / 4
                            },
                            source: source.to_owned(),
                            estimator: if source == "MODEL_WINDOW" {
                                "tokens-v2+calibration".into()
                            } else {
                                "bytes/4".into()
                            },
                        });
                }
                "ContextEpochOpened" => {
                    let s = |k: &str| p[k].as_str().unwrap_or_default().to_owned();
                    out.summaries.push(wire::CompactionSummaryView {
                        epoch: u32::try_from(p["epoch"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX),
                        // A record from before REQ-PX-109 was always extractive.
                        summary_source: if s("summary_source").is_empty() {
                            "EXTRACTIVE".into()
                        } else {
                            s("summary_source")
                        },
                        summarizer: s("summarizer"),
                        fallback_reason: s("fallback_reason"),
                        transcript_ref: s("transcript_ref"),
                        transcript_entries: u32::try_from(
                            p["source_entries"].as_u64().unwrap_or(0),
                        )
                        .unwrap_or(u32::MAX),
                        summary_tokens: u32::try_from(p["projection_tokens"].as_u64().unwrap_or(0))
                            .unwrap_or(u32::MAX),
                    });
                }
                _ => {}
            }
        }
        if events.len() < PAGE {
            break;
        }
    }
    out
}

/// The instruction layers of one `RulesSelected` payload: what is in force,
/// then what exists and is not, each with its provenance.
fn instruction_views(p: &serde_json::Value) -> Vec<wire::InstructionLayerView> {
    let mut out = Vec::new();
    for a in p["active"].as_array().into_iter().flatten() {
        let s = |k: &str| a[k].as_str().unwrap_or_default().to_owned();
        let reason = match a["reason"]["kind"].as_str() {
            Some("PATH") => format!(
                "PATH:{} matched {}",
                a["reason"]["path"].as_str().unwrap_or_default(),
                a["reason"]["glob"].as_str().unwrap_or_default()
            ),
            Some(kind) => kind.to_owned(),
            None => String::new(),
        };
        out.push(wire::InstructionLayerView {
            layer: s("layer"),
            id: s("id"),
            source: s("source"),
            content_hash: s("hash"),
            reason,
            loaded: true,
            not_loaded_reason: String::new(),
            bytes: a["bytes"].as_u64().unwrap_or(0),
            truncated: a["truncated"].as_bool().unwrap_or(false),
            scanner_findings: a["findings"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f.as_str().map(str::to_owned))
                .collect(),
        });
    }
    for n in p["not_loaded"].as_array().into_iter().flatten() {
        out.push(wire::InstructionLayerView {
            source: n["source"].as_str().unwrap_or_default().to_owned(),
            loaded: false,
            not_loaded_reason: n["reason"].as_str().unwrap_or_default().to_owned(),
            ..Default::default()
        });
    }
    out
}

// ---- REQ-PX-059: accounting by category ----

/// The bound the estimator declares against a provider's own count of the same
/// request, in basis points. The estimator is a deterministic text heuristic
/// (`tokens-v2`); the provider's count is authoritative whenever it reports
/// one, and the view always shows the error it observed beside this bound.
pub(crate) const DECLARED_ESTIMATOR_ERROR_BP: u32 = 2500;

/// How wide the model's window is, and where that is known from: the signed
/// registry's binding, else the endpoint's own catalog, else not at all. Never
/// a constant.
pub(crate) fn window_of(core: &Core, endpoint: &str, model: &str) -> (u64, &'static str) {
    if let Some(e) = core
        .gateway
        .registry()
        .and_then(|r| r.entry(endpoint, model).map(|e| e.context_tokens))
        .filter(|w| *w > 0)
    {
        return (u64::from(e), "REGISTRY");
    }
    match core
        .gateway
        .capability(endpoint, model)
        .map(|c| c.context_tokens)
        .filter(|w| *w > 0)
    {
        Some(w) => (u64::from(w), "ENDPOINT"),
        None => (0, "UNKNOWN"),
    }
}

/// What the loop knows of the request it just compiled.
pub(crate) struct CompiledRequest<'a> {
    pub endpoint: &'a str,
    pub model: &'a str,
    pub ordinal: u32,
    pub text: &'a modbit_prompt_compiler::accounting::CategoryText,
    pub tools: &'a [modbit_providers::ToolProjection],
    pub injected_memory: &'a [String],
    pub request_estimate: u32,
}

/// What a ContextCompile step keeps for the accounting: the estimator's count
/// of each part of the request it just compiled, the model's window, and the
/// names the parts are made of. The provider's own count arrives later with
/// the turn's usage; the view joins them.
pub(crate) fn compile_record(core: &Core, r: &CompiledRequest<'_>) -> serde_json::Value {
    use modbit_prompt_compiler::accounting::{Category, tool_category};
    let CompiledRequest {
        endpoint,
        model,
        ordinal,
        text,
        tools,
        injected_memory,
        request_estimate,
    } = *r;
    let est = |t: &str| u64::from(modbit_compaction::estimate_tokens_v2(t));
    let mut counts = [0u64; 9];
    for (i, c) in Category::ALL.iter().enumerate().take(8) {
        counts[i] = est(text.of(*c));
    }
    // The conversation is the rest of the request: its goal, its transcript
    // and the run state. Defined as the remainder so the parts add up to the
    // estimator's count of the whole request.
    let others: u64 = counts[..8].iter().sum();
    counts[8] = u64::from(request_estimate).saturating_sub(others);
    let mut estimates = serde_json::Map::new();
    for (i, c) in Category::ALL.iter().enumerate() {
        estimates.insert(c.name().to_owned(), serde_json::json!(counts[i]));
    }
    let names = |cat: Category| -> Vec<String> {
        tools
            .iter()
            .filter(|t| tool_category(&t.name) == cat)
            .map(|t| t.name.clone())
            .collect()
    };
    let (window, window_source) = window_of(core, endpoint, model);
    serde_json::json!({
        "estimates": estimates,
        "request_estimate": request_estimate,
        "endpoint": endpoint,
        "model": model,
        "window": window,
        "window_source": window_source,
        "ordinal": ordinal,
        "tools": names(Category::Tools),
        "mcp": names(Category::Mcp),
        "subagent_tools": names(Category::Subagents),
        "memory": injected_memory,
    })
}

/// What the log says about the last compiled request.
struct LastAccounting {
    record: serde_json::Value,
    turn_id: Option<[u8; 16]>,
    offset: u64,
    /// The provider's count of that request, when it reported one.
    reported_input: Option<u64>,
    skills: Vec<String>,
    children: Vec<String>,
}

async fn last_accounting(core: &Core, task_id: TaskId) -> Option<LastAccounting> {
    let store = core.store.lock().await;
    let task = store.task(&task_id).ok()??;
    let events = store.read_session(&task.session_id, 0, 200_000).ok()?;
    let mut last: Option<LastAccounting> = None;
    let mut usage: std::collections::HashMap<[u8; 16], u64> = std::collections::HashMap::new();
    let mut skills: Vec<String> = Vec::new();
    let mut children: Vec<String> = Vec::new();
    for e in &events {
        if e.envelope.task_id != Some(task_id) {
            continue;
        }
        match e.envelope.event_type.as_str() {
            "StepSucceeded" => {
                let Ok(payload) = store.payload(&e.envelope) else {
                    continue;
                };
                let Some(r) = payload["output_ref"].as_str() else {
                    continue;
                };
                let Ok(bytes) = store.objects().get(r) else {
                    continue;
                };
                let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                    continue;
                };
                if let Some(acc) = v.get("accounting") {
                    last = Some(LastAccounting {
                        record: acc.clone(),
                        turn_id: e.envelope.turn_id.map(|t| *t.as_bytes()),
                        offset: e.offset,
                        reported_input: None,
                        skills: Vec::new(),
                        children: Vec::new(),
                    });
                }
            }
            "ModelUsageRecorded" => {
                let Ok(p) = store.payload(&e.envelope) else {
                    continue;
                };
                if p["reported"].as_bool().unwrap_or(false)
                    && let Some(t) = e.envelope.turn_id
                {
                    usage.insert(*t.as_bytes(), p["input_tokens"].as_u64().unwrap_or(0));
                }
            }
            "SkillSelected" => {
                if let Ok(p) = store.payload(&e.envelope)
                    && let Some(n) = p["name"].as_str()
                    && !skills.iter().any(|s| s == n)
                {
                    skills.push(n.to_owned());
                }
            }
            "SubagentAdmitted" => {
                if let Ok(p) = store.payload(&e.envelope)
                    && let Some(c) = p["child_task_id"].as_str()
                {
                    children.push(c.to_owned());
                }
            }
            _ => {}
        }
    }
    let mut last = last?;
    last.reported_input = last
        .turn_id
        .and_then(|t| usage.get(&t).copied())
        .filter(|n| *n > 0);
    last.skills = skills;
    last.children = children;
    Some(last)
}

/// The Core's breakdown of the last compiled request (REQ-PX-059).
pub(crate) async fn accounting(core: &Core, task_id: TaskId) -> wire::ContextAccountingView {
    use modbit_prompt_compiler::accounting::{Category, ROUNDING_RULE, apportion};
    let log = task_context_log(core, task_id).await;
    let loaded_layers: Vec<String> = log
        .instructions
        .iter()
        .filter(|i| i.loaded)
        .map(|i| format!("{}:{}", i.layer, i.source))
        .collect();
    let mut v = wire::ContextAccountingView {
        rounding_rule: ROUNDING_RULE.into(),
        declared_error_bp: DECLARED_ESTIMATOR_ERROR_BP,
        instruction_layers_in_force: u32::try_from(loaded_layers.len()).unwrap_or(u32::MAX),
        compaction_epoch: log
            .summaries
            .iter()
            .map(|s| s.epoch)
            .max()
            .unwrap_or_default(),
        compaction_summaries: u32::try_from(log.summaries.len()).unwrap_or(u32::MAX),
        budgets: Some(budget_view(core, task_id).await),
        ..Default::default()
    };
    let Some(last) = last_accounting(core, task_id).await else {
        return v;
    };
    let rec = &last.record;
    let mut estimates = [0u64; 9];
    for (i, c) in Category::ALL.iter().enumerate() {
        estimates[i] = rec["estimates"][c.name()].as_u64().unwrap_or(0);
    }
    let estimated_total: u64 = estimates.iter().sum();
    // The provider counted the request when it said so; the estimator's own
    // count stands otherwise, and says it is one.
    let (total, source) = match last.reported_input {
        Some(n) => (n, "PROVIDER_REPORTED"),
        None => (estimated_total, "ESTIMATED"),
    };
    let tokens = apportion(&estimates, total);
    let strings = |k: &str| -> Vec<String> {
        rec[k]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let sources = |c: Category| -> Vec<String> {
        match c {
            Category::System => vec!["system segment".into()],
            Category::Tools => strings("tools"),
            Category::Rules => loaded_layers.clone(),
            Category::Skills => last.skills.clone(),
            Category::Mcp => strings("mcp"),
            Category::Memory => strings("memory"),
            Category::Summary => log
                .summaries
                .iter()
                .map(|s| format!("epoch {}", s.epoch))
                .collect(),
            Category::Subagents => {
                let mut s = strings("subagent_tools");
                s.extend(last.children.iter().map(|c| format!("child {c}")));
                s
            }
            Category::Conversation => vec!["goal, transcript, run state".into()],
        }
    };
    v.available = true;
    v.total_tokens = total;
    v.total_source = source.into();
    v.estimated_total = estimated_total;
    v.provider_reported_input = last.reported_input.unwrap_or(0);
    v.estimator_error_bp = last.reported_input.map_or(0, |r| {
        u32::try_from(estimated_total.abs_diff(r).saturating_mul(10_000) / r.max(1))
            .unwrap_or(u32::MAX)
    });
    v.window_tokens = rec["window"].as_u64().unwrap_or(0);
    v.window_source = rec["window_source"]
        .as_str()
        .unwrap_or("UNKNOWN")
        .to_owned();
    v.used_bp = total
        .saturating_mul(10_000)
        .checked_div(v.window_tokens)
        .map_or(0, |bp| u32::try_from(bp).unwrap_or(u32::MAX));
    v.endpoint = rec["endpoint"].as_str().unwrap_or_default().to_owned();
    v.model = rec["model"].as_str().unwrap_or_default().to_owned();
    v.turn_id = last
        .turn_id
        .map(|t| modbit_domain::TurnId::from_bytes(t).to_string())
        .unwrap_or_default();
    v.turn_ordinal = u32::try_from(rec["ordinal"].as_u64().unwrap_or(0)).unwrap_or(0);
    v.offset = last.offset;
    v.memory_items_injected = u32::try_from(strings("memory").len()).unwrap_or(u32::MAX);
    v.categories = Category::ALL
        .iter()
        .enumerate()
        .map(|(i, c)| wire::ContextCategoryView {
            category: c.name().into(),
            tokens: tokens[i],
            estimated_tokens: estimates[i],
            share_bp: tokens[i]
                .saturating_mul(10_000)
                .checked_div(total)
                .map_or(0, |bp| u32::try_from(bp).unwrap_or(u32::MAX)),
            sources: sources(*c),
        })
        .collect();
    v
}

/// The budgets of REQ-PX-116 as the accounting reports them beside the
/// context numbers: from the task's own log, the way its loop reads them.
async fn budget_view(core: &Core, task_id: TaskId) -> wire::BudgetAccountingView {
    let task = core.store.lock().await.task(&task_id).ok().flatten();
    let Some(task) = task else {
        return wire::BudgetAccountingView::default();
    };
    let (_, mut state, _, _) =
        crate::runtime::rebuild(core, &task, modbit_core_runtime::Budgets::default()).await;
    crate::spawn::refresh_children_held(core, &task, &mut state).await;
    wire::BudgetAccountingView {
        max_cost_minor: state.budgets.max_cost_minor.unwrap_or(0),
        spent_minor: state.cost_minor,
        held_by_children_minor: state.children_held.cost_minor,
        max_wall_ms: state.budgets.max_wall_ms.unwrap_or(0),
        wall_ms_used: state.wall_ms,
        max_children: state.budgets.max_children,
        live_children: state.live_children,
        forbid_spawn: state.budgets.max_children == 0,
    }
}
