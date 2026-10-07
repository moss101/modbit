//! The memory commands of the SurfaceProtocol (PX-113; docs/19, docs/30):
//! `ListMemory` (with filters and one item's history), `ProposeMemory`,
//! `PromoteMemory`, `EditMemory` and `ForgetMemory`. The dispatcher in
//! `server.rs` decodes the payload, finds the task and checks the session
//! lease; what each command does lives here, and every mutation is an event
//! (`crate::memory`) appended under the command record, so a retry replays and
//! the CLI and the desktop reach the same behaviour.

use modbit_domain::event::Actor;
use modbit_domain::task::Task;
use modbit_event_store::CommandRecord;
use modbit_memory::{MemoryEdit, MemoryItem, RecordType, Scope, Sensitivity, Source, Status};
use modbit_protocol::v1 as wire;

use crate::memory::{self, ListFilter, MemoryCtx, Promoted};
use crate::server::{Core, wire_id};

type Failure = (String, String);

fn ctx_of(core: &Core, task: &Task, actor: Actor) -> MemoryCtx {
    MemoryCtx {
        tenant: core.tenant_id,
        session: task.session_id,
        task: Some(task.task_id),
        actor,
    }
}

fn status_label(s: Status) -> String {
    serde_json::to_value(s)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

async fn revision_of(task: &Task) -> Option<String> {
    memory::git_head(task.workspace_root.clone()?).await
}

/// `ListMemory`: the task's scope chain (proposals included, conflicts
/// surfaced), narrowed by the filters; with one item and `include_history`,
/// that item's events from the log.
pub(crate) async fn list(
    core: &Core,
    task: &Task,
    p: &wire::ListMemory,
) -> Result<wire::MemoryList, Failure> {
    let chain = memory::chain_of(&core.store, core.tenant_id, task, None).await;
    let filter = ListFilter {
        statuses: p.statuses.clone(),
        scope_kinds: p.scope_kinds.clone(),
        text: p.text.clone(),
        memory_id: p.memory_id.clone(),
    };
    let (items, scopes, conflicts) = memory::list_scoped(&core.store, &chain, &filter)
        .await
        .map_err(|e| ("MEMORY_STORE".to_owned(), e))?;
    let mut history = Vec::new();
    if p.include_history
        && let Some(id) = items.first().and_then(|v| v["id"].as_str())
    {
        history = memory::history_of(&core.store, id)
            .await
            .map_err(|e| ("MEMORY_STORE".to_owned(), e))?
            .into_iter()
            .map(
                |(offset, event_type, actor, occurred_at_ms, session_id, detail)| {
                    wire::MemoryEventView {
                        offset,
                        event_type,
                        actor,
                        occurred_at_ms,
                        session_id,
                        detail,
                    }
                },
            )
            .collect();
    }
    Ok(wire::MemoryList {
        task_id: Some(wire_id(task.task_id.as_bytes())),
        items: items.iter().map(crate::server::memory_item_view).collect(),
        scopes,
        conflicts: conflicts
            .into_iter()
            .map(|item_ids| wire::MemoryConflict { item_ids })
            .collect(),
        history,
    })
}

/// `ProposeMemory`: a person's proposal. It is a candidate on the log; the
/// source defaults to what the person stated.
pub(crate) async fn propose(
    core: &Core,
    task: &Task,
    p: &wire::ProposeMemory,
    record: CommandRecord,
    actor: Actor,
) -> Result<(wire::MemoryProposed, bool), Failure> {
    let bad = |m: &str| Err(("BAD_PAYLOAD".to_owned(), m.to_owned()));
    let Some(record_type) =
        serde_json::from_value::<RecordType>(serde_json::Value::String(p.record_type.clone())).ok()
    else {
        return bad(
            "record_type must be decision, convention, fact, procedure, failure_pattern, dependency_knowledge or user_preference",
        );
    };
    let source = if p.source.is_empty() {
        Source::UserStated
    } else {
        match serde_json::from_value::<Source>(serde_json::Value::String(p.source.clone())) {
            Ok(s) => s,
            Err(_) => return bad("unknown source"),
        }
    };
    if p.topic.trim().is_empty() || p.content.trim().is_empty() {
        return bad("topic and content required");
    }
    let chain = memory::chain_of(&core.store, core.tenant_id, task, None).await;
    let scope = chain
        .resolve((!p.scope.is_empty()).then_some(p.scope.as_str()))
        .map_err(|e| ("BAD_PAYLOAD".to_owned(), e))?;
    let now = modbit_domain::Timestamp::now().0;
    let author = match &actor {
        Actor::User(u) => format!("user:{u}"),
        other => format!("{other:?}"),
    };
    let default_confidence = if source == Source::UserStated {
        0.9
    } else {
        0.5
    };
    let mut item = MemoryItem::propose(
        scope,
        record_type,
        p.topic.trim(),
        p.content.clone(),
        source,
        author,
        if p.confidence > 0.0 {
            p.confidence
        } else {
            default_confidence
        },
        now,
    );
    if p.ttl_ms > 0 {
        item.expires_at_ms = Some(now.saturating_add(p.ttl_ms));
    }
    if p.sensitive {
        item.sensitivity = Sensitivity::Sensitive;
    }
    if matches!(item.scope, Scope::Repository { .. }) {
        item.last_validation_revision = revision_of(task).await;
    }
    let done = memory::propose(&core.store, &ctx_of(core, task, actor), item, Some(record))
        .await
        .map_err(|e| ("MEMORY_STORE".to_owned(), e))?;
    let replayed = done.replayed;
    Ok((
        wire::MemoryProposed {
            memory_id: done.id,
            status: status_label(done.status),
            scope: done.scope,
            existed: done.existed,
            offset: done.offset,
        },
        replayed,
    ))
}

/// A prefix or a full id, resolved against the task's scope chain.
async fn resolve_id(core: &Core, task: &Task, given: &str) -> Result<String, Failure> {
    let chain = memory::chain_of(&core.store, core.tenant_id, task, None).await;
    let rows = {
        let st = core.store.lock().await;
        st.memory_in_scopes(&chain.keys())
            .map_err(|e| ("MEMORY_STORE".to_owned(), e.to_string()))?
    };
    match memory::resolve_prefix(&rows, given) {
        Ok(Some(id)) => Ok(id),
        // Unknown ids are reported by the command itself, with its own shape.
        Ok(None) => Ok(given.to_owned()),
        Err(e) => Err(("AMBIGUOUS_MEMORY_ID".to_owned(), e)),
    }
}

/// `PromoteMemory`: the governed step, refused with a typed reason when the
/// rules do not allow it.
pub(crate) async fn promote(
    core: &Core,
    task: &Task,
    p: &wire::PromoteMemory,
    record: CommandRecord,
    actor: Actor,
) -> Result<(wire::MemoryPromoted, bool), Failure> {
    let id = resolve_id(core, task, &p.memory_id).await?;
    let revision = revision_of(task).await;
    let outcome = memory::promote(
        &core.store,
        &ctx_of(core, task, actor),
        &id,
        revision,
        Some(record),
    )
    .await
    .map_err(|e| ("MEMORY_STORE".to_owned(), e))?;
    let (out, code, detail, superseded, offset, replayed) = match outcome {
        Promoted::Curated {
            superseded,
            offset,
            replayed,
        } => (
            "curated",
            String::new(),
            String::new(),
            superseded,
            offset,
            replayed,
        ),
        Promoted::Unknown => (
            "unknown",
            "UNKNOWN_MEMORY".to_owned(),
            "no such proposal".to_owned(),
            vec![],
            0,
            false,
        ),
        Promoted::Refused(refusal) => {
            let v = serde_json::to_value(&refusal).unwrap_or_default();
            let code = v
                .get("code")
                .and_then(|c| c.as_str())
                .unwrap_or("REFUSED")
                .to_owned();
            ("refused", code, v.to_string(), vec![], 0, false)
        }
    };
    Ok((
        wire::MemoryPromoted {
            memory_id: id,
            outcome: out.to_owned(),
            refusal_code: code,
            refusal_detail: detail,
            superseded,
            offset: if offset > 0 {
                offset
            } else {
                *core.last_offset.borrow()
            },
        },
        replayed,
    ))
}

/// `EditMemory`: a person's edit of a proposal or a curated item.
pub(crate) async fn edit(
    core: &Core,
    task: &Task,
    p: &wire::EditMemory,
    record: CommandRecord,
    actor: Actor,
) -> Result<(wire::MemoryEdited, bool), Failure> {
    let id = resolve_id(core, task, &p.memory_id).await?;
    let change = MemoryEdit {
        topic: p.topic.clone(),
        content: p.content.clone(),
        confidence: p.confidence,
        ttl_ms: if p.clear_ttl {
            Some(None)
        } else {
            p.ttl_ms.map(Some)
        },
        sensitivity: p.sensitive.map(|s| {
            if s {
                Sensitivity::Sensitive
            } else {
                Sensitivity::Normal
            }
        }),
    };
    let revision = revision_of(task).await;
    let done = memory::edit(
        &core.store,
        &ctx_of(core, task, actor),
        &id,
        &change,
        revision,
        Some(record),
    )
    .await
    .map_err(|e| ("MEMORY_STORE".to_owned(), e))?;
    Ok(match done {
        memory::Edited::Done {
            old_id,
            new_id,
            status,
            retired,
            offset,
            replayed,
        } => (
            wire::MemoryEdited {
                memory_id: old_id,
                new_memory_id: new_id,
                outcome: "edited".into(),
                status: status_label(status),
                retired,
                offset,
                ..Default::default()
            },
            replayed,
        ),
        memory::Edited::Unknown => (
            wire::MemoryEdited {
                memory_id: id,
                outcome: "unknown".into(),
                refusal_code: "UNKNOWN_MEMORY".into(),
                refusal_detail: "no such memory item in this task's scopes".into(),
                ..Default::default()
            },
            false,
        ),
        memory::Edited::Refused { code, detail } => (
            wire::MemoryEdited {
                memory_id: id,
                outcome: "refused".into(),
                refusal_code: code,
                refusal_detail: detail,
                ..Default::default()
            },
            false,
        ),
    })
}

/// `ForgetMemory`: delete (it leaves the store) or supersede (it stays
/// inspectable, marked).
pub(crate) async fn forget(
    core: &Core,
    task: &Task,
    p: &wire::ForgetMemory,
    record: CommandRecord,
    actor: Actor,
) -> Result<(wire::MemoryForgotten, bool), Failure> {
    let id = resolve_id(core, task, &p.memory_id).await?;
    let done = memory::forget(
        &core.store,
        &ctx_of(core, task, actor),
        &id,
        p.supersede,
        Some(record),
    )
    .await
    .map_err(|e| ("MEMORY_STORE".to_owned(), e))?;
    Ok((
        wire::MemoryForgotten {
            memory_id: id,
            changed: done.changed,
            status: if done.changed {
                done.status
            } else {
                String::new()
            },
        },
        done.replayed,
    ))
}
