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
    let ledger = core.tools.ledger(task_id).await;
    let ledger = ledger.lock().await;
    let mut v = wire::ContextInspectorView::default();
    // What the last compiled turn injected and refused (the prompt envelope).
    if let Some((pack_id, injected, rejected)) = last_compiled(core, task_id).await {
        v.context_pack_id = pack_id;
        v.injected_refs = injected;
        v.rejected_refs = rejected;
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
    v.manifest_ref = economy.manifest_ref;
    v.prefix_cache_hits = economy.hits;
    v.prefix_cache_misses = economy.misses;
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
        });
    }
    v
}

/// The compaction epochs of a task and the prompt-cache economics of the
/// prefix they move.
#[derive(Default)]
struct Economy {
    epoch: u32,
    epochs: u32,
    compacted_entries: u64,
    manifest_ref: String,
    hits: u32,
    misses: u32,
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
            _ => {}
        }
    }
    out
}

/// The last ContextCompile step's record: (context pack id, injected, rejected).
/// The step events live on their own RunStep aggregates, so the session log is
/// the place that has them all in order.
async fn last_compiled(core: &Core, task_id: TaskId) -> Option<(String, Vec<String>, Vec<String>)> {
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
            found = Some((
                v["segment_hashes"][3]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                strings("injected_fragments"),
                strings("rejected_fragments"),
            ));
        }
    }
    found
}
