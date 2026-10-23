//! The Core's governed engineering memory (M9.1, REQ-EV-0162; PX-113;
//! docs/19).
//!
//! Every mutation of memory is an event. A proposal, a promotion, an edit, a
//! supersession and a forget each append to the `Memory` aggregate of the item
//! they concern, on the acting session's log, in one transaction (under the
//! command record when a person issued it, so a retry replays and appends
//! nothing); the `memory_items` table is the projection of those events
//! (`modbit_event_store::projections`), kept in the same transaction and rebuilt
//! from the log with the rest. Nothing here writes the table. Memory is
//! cross-session governed state, so its events live on whichever session's log
//! the acting task belongs to and are read back across sessions by aggregate
//! type; it is prompt context and never recovery state (MOD-STATE-001), so a
//! rebuild replays it but a run's recovery never needs it.
//!
//! `CoreMemory` implements `modbit_tools::pipeline::MemoryPort` for the
//! agent's tools: it resolves the task's scope chain (run, session, user,
//! agent profile, repository, space, organization), records a proposal and
//! reads curated memory. It never promotes — promotion is a separate governed
//! command ([`promote`]), so a proposal from a transcript summary never
//! becomes durable memory on its own (the M9.1 invariant). Selection for the
//! prompt ([`select_for_prompt`]) is by scope and relevance and goes through
//! the Context Pack compiler's memory segment.

use std::sync::Arc;

use modbit_context::memory::{MemoryCandidate, MemoryPack, pack_memory};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::Task;
use modbit_domain::{SessionId, TaskId, TenantId, Timestamp};
use modbit_event_store::{
    AppendRequest, CommandOutcome, CommandRecord, EventStore, MemoryRow, NewEvent,
    memory_aggregate_id,
};
use modbit_memory::{
    EditRefusal, MemoryEdit, MemoryItem, MemoryStore, PromotionContext, PromotionRefusal,
    RecordType, Scope, Selection, SelectionInput, Sensitivity, Source, Status, promotion_check,
};
use modbit_prompt_compiler::MemoryFragment;
use modbit_tools::registry::BoxFuture;
use serde_json::{Value, json};
use tokio::sync::Mutex;

/// The token budget of the memory segment of one prompt.
pub const PROMPT_MEMORY_BUDGET_TOKENS: u32 = 800;
/// The longest topic and content a memory item may carry.
const MAX_TOPIC_CHARS: usize = 200;
const MAX_CONTENT_BYTES: usize = 16 * 1024;

/// The session the legacy-row import is recorded on: a fixed identity that no
/// real session shares.
pub const MEMORY_LEDGER_SESSION: [u8; 16] = [0x4D; 16];

/// The scope chain a task's memory query reads and a proposal defaults into.
/// Narrowest first: the run, the session, the user, the agent profile, the
/// repository (the canonical workspace root), the space and the organization
/// (the tenant) — the order of [`Scope::rank`], which is also the order of
/// precedence when two scopes hold the same topic.
#[derive(Clone, Debug)]
pub struct ScopeChain {
    /// The scopes, narrowest first.
    pub scopes: Vec<Scope>,
}

impl ScopeChain {
    /// Build the chain from the task's identity. The repository scope is the
    /// canonical workspace root (a stable per-repo key); a task with no root
    /// has no repository scope, and one with no run, user, space or agent
    /// profile has none of those.
    #[must_use]
    pub fn for_task(
        tenant: TenantId,
        session: SessionId,
        run: Option<&str>,
        user: &str,
        workspace_root: Option<&str>,
        space: Option<&str>,
        agent_profile: Option<&str>,
    ) -> Self {
        let mut scopes = Vec::new();
        if let Some(r) = run {
            scopes.push(Scope::Run { id: r.to_owned() });
        }
        scopes.push(Scope::Session {
            id: session.to_string(),
        });
        if !user.is_empty() {
            scopes.push(Scope::User {
                id: user.to_owned(),
            });
        }
        if let Some(a) = agent_profile.filter(|a| !a.is_empty()) {
            scopes.push(Scope::AgentProfile { id: a.to_owned() });
        }
        if let Some(root) = workspace_root {
            scopes.push(Scope::Repository {
                id: root.to_owned(),
            });
        }
        if let Some(s) = space.filter(|s| !s.is_empty()) {
            scopes.push(Scope::Space { id: s.to_owned() });
        }
        scopes.push(Scope::Organization {
            id: tenant.to_string(),
        });
        debug_assert!(scopes.windows(2).all(|w| w[0].rank() <= w[1].rank()));
        ScopeChain { scopes }
    }

    /// The scope keys of the chain.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.scopes.iter().map(Scope::key).collect()
    }

    /// The scope named by a `scope` argument, resolved against the chain (so
    /// a proposal at, say, `user` scope binds to this task's user). Defaults
    /// to the session. A name that is no scope, or a scope this task does not
    /// have (`repository` with no workspace), is refused — never quietly
    /// replaced by another scope.
    pub fn resolve(&self, named: Option<&str>) -> Result<Scope, String> {
        let want = match named {
            None => "session",
            Some(n) => Scope::canonical_kind(n).ok_or_else(|| {
                format!(
                    "unknown memory scope `{n}`; use run, session, user, agent_profile, repository, space or organization"
                )
            })?,
        };
        self.scopes
            .iter()
            .find(|s| s.kind() == want)
            .cloned()
            .ok_or_else(|| format!("the memory scope `{want}` is not available to this task"))
    }
}

/// The agent profile of the agent that works a task: the profile named in a
/// subagent's capsule, and `primary` for any other task.
fn agent_profile_of(store: &EventStore, task_id: &TaskId) -> String {
    let events = store
        .read_aggregate(task_id.as_bytes(), 0, 64)
        .unwrap_or_default();
    for e in &events {
        if e.envelope.event_type != "SubagentCapsuleBound" {
            continue;
        }
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        if let Some(r) = p["capsule_ref"].as_str()
            && let Ok(bytes) = store.objects().get(r)
            && let Ok(c) =
                serde_json::from_slice::<modbit_domain::agent::AgentExecutionCapsule>(&bytes)
            && !c.profile.trim().is_empty()
        {
            return c.profile;
        }
    }
    "primary".to_owned()
}

/// The scope chain of a task, from the Core's records of it: the session's
/// user and space, the task's workspace root and agent profile.
pub async fn chain_of(
    store: &Arc<Mutex<EventStore>>,
    tenant: TenantId,
    task: &Task,
    run: Option<&str>,
) -> ScopeChain {
    let st = store.lock().await;
    chain_in(&st, tenant, task, run)
}

/// [`chain_of`] over a store the caller already holds.
pub fn chain_in(
    store: &EventStore,
    tenant: TenantId,
    task: &Task,
    run: Option<&str>,
) -> ScopeChain {
    let (user, space) = store
        .session(&task.session_id)
        .ok()
        .flatten()
        .map(|s| (s.user_id.to_string(), s.space_id.to_string()))
        .unwrap_or_default();
    ScopeChain::for_task(
        tenant,
        task.session_id,
        run,
        &user,
        task.workspace_root.as_deref(),
        Some(space.as_str()),
        Some(agent_profile_of(store, &task.task_id).as_str()),
    )
}

/// Who acts and on which log: the session and task the events are written
/// under, the tenant, and the actor they are attributed to.
#[derive(Clone, Debug)]
pub struct MemoryCtx {
    /// Tenant.
    pub tenant: TenantId,
    /// The acting session (the log the events go on).
    pub session: SessionId,
    /// The acting task.
    pub task: Option<TaskId>,
    /// The actor the events are attributed to.
    pub actor: Actor,
}

/// The git HEAD of `root`, if it is a repository with a commit.
pub async fn git_head(root: String) -> Option<String> {
    tokio::task::spawn_blocking(move || {
        let out = modbit_git::output(std::path::Path::new(&root), &["rev-parse", "HEAD"]).ok()?;
        if !out.status.success() {
            return None;
        }
        let rev = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (!rev.is_empty()).then_some(rev)
    })
    .await
    .ok()
    .flatten()
}

fn label<T: serde::Serialize>(v: T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Serialize an item to a store row.
fn row_of(item: &MemoryItem) -> MemoryRow {
    MemoryRow {
        id: item.id.clone(),
        scope_key: item.scope.key(),
        record_type: label(item.record_type),
        topic: item.topic.clone(),
        status: label(item.status),
        sensitivity: label(item.sensitivity),
        created_at_ms: item.created_at_ms,
        expires_at_ms: item.expires_at_ms,
        updated_at_ms: Timestamp::now().0,
        doc: serde_json::to_string(item).unwrap_or_default(),
    }
}

/// Parse a store row back into an item (the whole item is the `doc`).
fn item_of(row: &MemoryRow) -> Option<MemoryItem> {
    serde_json::from_str(&row.doc).ok()
}

/// The public view of an item (no internal-only fields beyond what the
/// product shows).
pub fn view(item: &MemoryItem) -> Value {
    json!({
        "id": item.id,
        "scope": item.scope.key(),
        "record_type": item.record_type,
        "topic": item.topic,
        "content": item.content,
        "source": item.source,
        "author": item.author,
        "confidence": item.confidence,
        "created_at_ms": item.created_at_ms,
        "expires_at_ms": item.expires_at_ms,
        "sensitivity": item.sensitivity,
        "status": item.status,
        "supersedes": item.supersedes,
        "conflicts": item.conflicts,
        "last_validation_revision": item.last_validation_revision,
        "validated": item.validated,
    })
}

fn parse_record_type(s: &str) -> Option<RecordType> {
    serde_json::from_value(Value::String(s.to_owned())).ok()
}

fn parse_source(s: &str) -> Option<Source> {
    serde_json::from_value(Value::String(s.to_owned())).ok()
}

/// Memory events as the actor wrote them: the payload names the event, the
/// item, and (for anything that leaves a row in the store) the row it leaves.
fn memory_event(
    kind: &str,
    id: &str,
    row: Option<&MemoryRow>,
    extra: Value,
    actor: &Actor,
) -> NewEvent {
    let mut payload = json!({"event_type": kind, "memory_id": id});
    if let Some(row) = row {
        payload["row"] = serde_json::to_value(row).unwrap_or_default();
    }
    if let (Some(dst), Some(src)) = (payload.as_object_mut(), extra.as_object()) {
        for (k, v) in src {
            dst.insert(k.clone(), v.clone());
        }
    }
    NewEvent::new(kind, payload, actor.clone())
}

fn request(ctx: &MemoryCtx, id: &str, events: Vec<NewEvent>) -> AppendRequest {
    AppendRequest {
        tenant_id: ctx.tenant,
        session_id: ctx.session,
        task_id: ctx.task,
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Memory,
        aggregate_id: memory_aggregate_id(id),
        expected_sequence: None,
        events,
    }
}

/// Append the requests, under the command record when a person issued the
/// command: a retry of the same command id replays what the first one
/// appended and writes nothing. Returns the events and whether they were
/// replayed.
fn commit(
    store: &mut EventStore,
    command: Option<CommandRecord>,
    reqs: Vec<AppendRequest>,
) -> Result<(Vec<modbit_event_store::StoredEvent>, bool), String> {
    match command {
        Some(cmd) => match store
            .execute_command_all(cmd, reqs)
            .map_err(|e| e.to_string())?
        {
            CommandOutcome::Applied(e) => Ok((e, false)),
            CommandOutcome::Replayed(e) => Ok((e, true)),
        },
        None => store
            .append_all(reqs, None)
            .map(|e| (e, false))
            .map_err(|e| e.to_string()),
    }
}

/// What an earlier execution of this command appended, when there was one: a
/// retry replays it and appends nothing, whatever state the item is in now.
fn replayed(
    store: &EventStore,
    command: Option<&CommandRecord>,
) -> Result<Option<Vec<modbit_event_store::StoredEvent>>, String> {
    let Some(cmd) = command else {
        return Ok(None);
    };
    match store.prior_command(cmd).map_err(|e| e.to_string())? {
        Some(CommandOutcome::Replayed(events) | CommandOutcome::Applied(events)) => {
            Ok(Some(events))
        }
        None => Ok(None),
    }
}

fn load(store: &EventStore, id: &str) -> Result<Option<MemoryItem>, String> {
    Ok(store
        .memory_get(id)
        .map_err(|e| e.to_string())?
        .and_then(|r| item_of(&r)))
}

/// The ids an unambiguous prefix of at least eight characters names, among
/// the items in `rows`. `Ok(None)` when none, `Err` when several.
pub fn resolve_prefix(rows: &[MemoryRow], prefix: &str) -> Result<Option<String>, String> {
    if let Some(r) = rows.iter().find(|r| r.id == prefix) {
        return Ok(Some(r.id.clone()));
    }
    if prefix.len() < 8 {
        return Ok(None);
    }
    let hits: Vec<&MemoryRow> = rows.iter().filter(|r| r.id.starts_with(prefix)).collect();
    match hits.len() {
        0 => Ok(None),
        1 => Ok(Some(hits[0].id.clone())),
        _ => Err(format!(
            "`{prefix}` names {} memory items; use more characters",
            hits.len()
        )),
    }
}

/// The filters of [`list_scoped`].
#[derive(Clone, Debug, Default)]
pub struct ListFilter {
    /// Only these statuses (empty = all).
    pub statuses: Vec<String>,
    /// Only these scope kinds (empty = all).
    pub scope_kinds: Vec<String>,
    /// Case-insensitive substring of topic or content.
    pub text: String,
    /// Only this item (an id or an unambiguous prefix).
    pub memory_id: String,
}

/// Every memory item in a scope chain, for inspection: each item's view and
/// the conflict groups among the curated ones (nothing is resolved here).
pub async fn list_scoped(
    store: &Arc<Mutex<EventStore>>,
    chain: &ScopeChain,
    filter: &ListFilter,
) -> Result<(Vec<Value>, Vec<String>, Vec<Vec<String>>), String> {
    let keys = chain.keys();
    let rows = {
        let st = store.lock().await;
        st.memory_in_scopes(&keys).map_err(|e| e.to_string())?
    };
    let only = if filter.memory_id.is_empty() {
        None
    } else {
        Some(resolve_prefix(&rows, &filter.memory_id)?.ok_or_else(|| {
            format!(
                "no memory item `{}` in this task's scopes",
                filter.memory_id
            )
        })?)
    };
    let needle = filter.text.trim().to_lowercase();
    let mut mem = MemoryStore::new();
    let mut views = Vec::new();
    for r in &rows {
        let Some(item) = item_of(r) else { continue };
        mem.upsert(item.clone());
        if only.as_ref().is_some_and(|id| id != &item.id) {
            continue;
        }
        if !filter.statuses.is_empty() && !filter.statuses.iter().any(|s| *s == label(item.status))
        {
            continue;
        }
        if !filter.scope_kinds.is_empty()
            && !filter
                .scope_kinds
                .iter()
                .filter_map(|k| Scope::canonical_kind(k))
                .any(|k| k == item.scope.kind())
        {
            continue;
        }
        if !needle.is_empty()
            && !item.topic.to_lowercase().contains(&needle)
            && !item.content.to_lowercase().contains(&needle)
        {
            continue;
        }
        views.push(view(&item));
    }
    let now = Timestamp::now().0;
    Ok((views, keys, mem.conflicts(now)))
}

/// The events of one item, oldest first: `(offset, type, actor, at_ms,
/// session, detail)`.
pub async fn history_of(
    store: &Arc<Mutex<EventStore>>,
    memory_id: &str,
) -> Result<Vec<(u64, String, String, i64, String, String)>, String> {
    let st = store.lock().await;
    let events = st
        .read_aggregate(&memory_aggregate_id(memory_id), 0, 1000)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for e in &events {
        let p = st.payload(&e.envelope).unwrap_or_default();
        let actor = match &e.envelope.actor {
            Actor::User(u) => format!("user:{u}"),
            Actor::Core(c) => format!("core:{c}"),
            Actor::Agent(a) => format!("agent:{a}"),
            Actor::External(x) => format!("external:{x}"),
        };
        let detail = match e.envelope.event_type.as_str() {
            "MemoryPromoted" => format!("superseded {}", p["superseded"]),
            "MemoryEdited" => format!("now {}", p["new_memory_id"].as_str().unwrap_or_default()),
            "MemorySuperseded" => format!(
                "by {} ({})",
                p["by"].as_str().unwrap_or_default(),
                p["cause"].as_str().unwrap_or_default()
            ),
            "MemoryForgotten" => p["mode"].as_str().unwrap_or_default().to_owned(),
            _ => String::new(),
        };
        out.push((
            e.offset,
            e.envelope.event_type.clone(),
            actor,
            e.envelope.occurred_at.0,
            e.envelope.session_id.to_string(),
            detail,
        ));
    }
    Ok(out)
}

/// What [`propose`] did.
#[derive(Debug)]
pub struct Proposed {
    /// The item's id.
    pub id: String,
    /// Its status after the call.
    pub status: Status,
    /// The scope key.
    pub scope: String,
    /// The same content was already on record; nothing was appended.
    pub existed: bool,
    /// The offset of the last event appended (0 when nothing was).
    pub offset: u64,
    /// The command was a retry that replayed.
    pub replayed: bool,
}

/// Record a proposal. Content-addressed ids make this idempotent by nature:
/// an item already on record (in any state but forgotten) is returned as it
/// stands — a re-proposal never turns a curated item back into a candidate.
/// A repository-scoped proposal binds to the repository's current revision.
pub async fn propose(
    store: &Arc<Mutex<EventStore>>,
    ctx: &MemoryCtx,
    mut item: MemoryItem,
    command: Option<CommandRecord>,
) -> Result<Proposed, String> {
    if item.topic.trim().is_empty() || item.content.trim().is_empty() {
        return Err("topic and content required".to_owned());
    }
    if item.topic.chars().count() > MAX_TOPIC_CHARS || item.content.len() > MAX_CONTENT_BYTES {
        return Err(format!(
            "a memory item's topic is at most {MAX_TOPIC_CHARS} characters and its content at most {MAX_CONTENT_BYTES} bytes"
        ));
    }
    item.status = Status::Proposed;
    let mut st = store.lock().await;
    if let Some(events) = replayed(&st, command.as_ref())? {
        return Ok(Proposed {
            id: item.id.clone(),
            status: Status::Proposed,
            scope: item.scope.key(),
            existed: false,
            offset: events.last().map_or(0, |e| e.offset),
            replayed: true,
        });
    }
    if let Some(existing) = load(&st, &item.id)? {
        return Ok(Proposed {
            id: existing.id,
            status: existing.status,
            scope: existing.scope.key(),
            existed: true,
            offset: 0,
            replayed: false,
        });
    }
    let row = row_of(&item);
    let ev = memory_event(
        "MemoryProposed",
        &item.id,
        Some(&row),
        json!({"scope": item.scope.key(), "source": item.source}),
        &ctx.actor,
    );
    let (events, replayed) = commit(&mut st, command, vec![request(ctx, &item.id, vec![ev])])?;
    Ok(Proposed {
        id: item.id.clone(),
        status: Status::Proposed,
        scope: item.scope.key(),
        existed: false,
        offset: events.last().map_or(0, |e| e.offset),
        replayed,
    })
}

/// The outcome of a promotion attempt (a governed step, not a tool).
#[derive(Debug)]
pub enum Promoted {
    /// Promoted to curated; the ids it superseded (now `Superseded`).
    Curated {
        /// Ids it superseded.
        superseded: Vec<String>,
        /// Offset of the last event appended.
        offset: u64,
        /// A retry that replayed.
        replayed: bool,
    },
    /// The proposal is gone.
    Unknown,
    /// Refused with a typed reason.
    Refused(PromotionRefusal),
}

/// Promote a proposed item to curated, applying the promotion gate against
/// the durable store: a `MemoryPromoted` event for the item and a
/// `MemorySuperseded` for each curated item it replaces, in one transaction.
/// `current_repository_revision` comes from the caller's workspace; whether
/// the item's scope may hold sensitive memory is the scope's own policy
/// ([`Scope::permits_sensitive`]). This is the only path that makes durable
/// memory; the tools never call it.
pub async fn promote(
    store: &Arc<Mutex<EventStore>>,
    ctx: &MemoryCtx,
    id: &str,
    current_repository_revision: Option<String>,
    command: Option<CommandRecord>,
) -> Result<Promoted, String> {
    let mut st = store.lock().await;
    if let Some(events) = replayed(&st, command.as_ref())? {
        let superseded = events
            .iter()
            .find(|e| e.envelope.event_type == "MemoryPromoted")
            .and_then(|e| st.payload(&e.envelope).ok())
            .and_then(|p| {
                p["superseded"].as_array().map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_default();
        return Ok(Promoted::Curated {
            superseded,
            offset: events.last().map_or(0, |e| e.offset),
            replayed: true,
        });
    }
    let Some(item) = load(&st, id)? else {
        return Ok(Promoted::Unknown);
    };
    let pctx = PromotionContext {
        scope_permits_sensitive: item.scope.permits_sensitive(),
        current_repository_revision,
        now_ms: Timestamp::now().0,
    };
    if let Err(refusal) = promotion_check(&item, &pctx) {
        return Ok(Promoted::Refused(refusal));
    }
    let rows = st
        .memory_in_scopes(&[item.scope.key()])
        .map_err(|e| e.to_string())?;
    let mut mem = MemoryStore::new();
    for r in &rows {
        if let Some(it) = item_of(r) {
            mem.upsert(it);
        }
    }
    let superseded = mem.promote(id);
    let mut reqs = Vec::new();
    if let Some(curated) = mem.get(id) {
        let row = row_of(curated);
        reqs.push(request(
            ctx,
            id,
            vec![memory_event(
                "MemoryPromoted",
                id,
                Some(&row),
                json!({"superseded": superseded, "scope": curated.scope.key()}),
                &ctx.actor,
            )],
        ));
    }
    for sid in &superseded {
        if let Some(prior) = mem.get(sid) {
            let row = row_of(prior);
            reqs.push(request(
                ctx,
                sid,
                vec![memory_event(
                    "MemorySuperseded",
                    sid,
                    Some(&row),
                    json!({"by": id, "cause": "promotion"}),
                    &ctx.actor,
                )],
            ));
        }
    }
    let (events, replayed) = commit(&mut st, command, reqs)?;
    // A replay reports what the first execution recorded.
    let superseded = events
        .iter()
        .find(|e| e.envelope.event_type == "MemoryPromoted")
        .and_then(|e| st.payload(&e.envelope).ok())
        .and_then(|p| {
            p["superseded"].as_array().map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or(superseded);
    Ok(Promoted::Curated {
        superseded,
        offset: events.last().map_or(0, |e| e.offset),
        replayed,
    })
}

/// What [`forget`] did.
#[derive(Debug)]
pub struct Forgotten {
    /// Whether an item was changed.
    pub changed: bool,
    /// The status it was left in (`deleted` means it left the store).
    pub status: String,
    /// A retry that replayed.
    pub replayed: bool,
}

/// Forget an item: delete it (it leaves the store, the next prompt and every
/// projection; the log keeps that it was forgotten and by whom) or supersede
/// it (it stays inspectable, marked, and no query or prompt reads it).
pub async fn forget(
    store: &Arc<Mutex<EventStore>>,
    ctx: &MemoryCtx,
    id: &str,
    supersede: bool,
    command: Option<CommandRecord>,
) -> Result<Forgotten, String> {
    let mut st = store.lock().await;
    if let Some(events) = replayed(&st, command.as_ref())? {
        let mode = events
            .first()
            .and_then(|e| st.payload(&e.envelope).ok())
            .and_then(|p| p["mode"].as_str().map(str::to_owned))
            .unwrap_or_default();
        return Ok(Forgotten {
            changed: true,
            status: if mode == "supersede" {
                "superseded"
            } else {
                "deleted"
            }
            .to_owned(),
            replayed: true,
        });
    }
    let Some(mut item) = load(&st, id)? else {
        return Ok(Forgotten {
            changed: false,
            status: String::new(),
            replayed: false,
        });
    };
    let prior = label(item.status);
    let ev = if supersede {
        item.status = Status::Superseded;
        let row = row_of(&item);
        memory_event(
            "MemoryForgotten",
            id,
            Some(&row),
            json!({"mode": "supersede", "prior_status": prior, "scope": item.scope.key()}),
            &ctx.actor,
        )
    } else {
        memory_event(
            "MemoryForgotten",
            id,
            None,
            json!({"mode": "delete", "removed": true, "prior_status": prior, "scope": item.scope.key()}),
            &ctx.actor,
        )
    };
    let (_events, replayed) = commit(&mut st, command, vec![request(ctx, id, vec![ev])])?;
    Ok(Forgotten {
        changed: true,
        status: if supersede { "superseded" } else { "deleted" }.to_owned(),
        replayed,
    })
}

/// What [`edit`] did.
#[derive(Debug)]
pub enum Edited {
    /// The edit landed.
    Done {
        /// The item the edit started from.
        old_id: String,
        /// The item that now holds it (the same id for an in-place edit).
        new_id: String,
        /// Its status.
        status: Status,
        /// Ids the edit retired.
        retired: Vec<String>,
        /// Offset of the last event.
        offset: u64,
        /// A retry that replayed.
        replayed: bool,
    },
    /// No such item.
    Unknown,
    /// Refused, with a code and a reason.
    Refused {
        /// Code.
        code: String,
        /// Detail.
        detail: String,
    },
}

/// Edit an item as a person (the actor must be a user; the item becomes
/// `UserStated` and validated, since a person stated it). A change of topic
/// or content is a new item, content-addressed, that supersedes the edited
/// one in the same transaction; a change of confidence or time to live alone
/// replaces the item in place. A curated item must still pass the promotion
/// rules as edited (revision binding, sensitive scope, not expired); the
/// edited item takes the place of the old one only if it does.
pub async fn edit(
    store: &Arc<Mutex<EventStore>>,
    ctx: &MemoryCtx,
    id: &str,
    change: &MemoryEdit,
    current_repository_revision: Option<String>,
    command: Option<CommandRecord>,
) -> Result<Edited, String> {
    let mut st = store.lock().await;
    if let Some(events) = replayed(&st, command.as_ref())? {
        let edited = events
            .iter()
            .find(|e| e.envelope.event_type == "MemoryEdited")
            .and_then(|e| st.payload(&e.envelope).ok())
            .unwrap_or_default();
        let new_id = edited["new_memory_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let old_id = edited["old_memory_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let status = edited["row"]["status"]
            .as_str()
            .and_then(|s| serde_json::from_value(Value::String(s.to_owned())).ok())
            .unwrap_or(Status::Curated);
        let retired = if new_id == old_id {
            vec![]
        } else {
            vec![old_id.clone()]
        };
        return Ok(Edited::Done {
            old_id,
            new_id,
            status,
            retired,
            offset: events.last().map_or(0, |e| e.offset),
            replayed: true,
        });
    }
    let Some(old) = load(&st, id)? else {
        return Ok(Edited::Unknown);
    };
    let author = match &ctx.actor {
        Actor::User(u) => format!("user:{u}"),
        Actor::Agent(a) => format!("agent:{a}"),
        Actor::Core(c) => format!("core:{c}"),
        Actor::External(x) => format!("external:{x}"),
    };
    let now = Timestamp::now().0;
    let mut next = match old.edited(change, &author, now) {
        Ok(n) => n,
        Err(EditRefusal::NotEditable { status }) => {
            return Ok(Edited::Refused {
                code: "NOT_EDITABLE".into(),
                detail: format!("a {} item cannot be edited", label(status)),
            });
        }
        Err(EditRefusal::Empty) => {
            return Ok(Edited::Refused {
                code: "EMPTY".into(),
                detail: "topic and content must not be empty".into(),
            });
        }
    };
    if next.topic.chars().count() > MAX_TOPIC_CHARS || next.content.len() > MAX_CONTENT_BYTES {
        return Ok(Edited::Refused {
            code: "TOO_LONG".into(),
            detail: format!(
                "a memory item's topic is at most {MAX_TOPIC_CHARS} characters and its content at most {MAX_CONTENT_BYTES} bytes"
            ),
        });
    }
    // A repository-scoped edit is re-bound to the revision it was made at.
    if matches!(next.scope, Scope::Repository { .. })
        && let Some(rev) = &current_repository_revision
        && (next.id != old.id || old.last_validation_revision.is_none())
    {
        next.last_validation_revision = Some(rev.clone());
    }
    if old.status == Status::Curated {
        let mut as_proposed = next.clone();
        as_proposed.status = Status::Proposed;
        let pctx = PromotionContext {
            scope_permits_sensitive: next.scope.permits_sensitive(),
            current_repository_revision,
            now_ms: now,
        };
        if let Err(refusal) = promotion_check(&as_proposed, &pctx) {
            let v = serde_json::to_value(&refusal).unwrap_or_default();
            return Ok(Edited::Refused {
                code: v["code"].as_str().unwrap_or("REFUSED").to_owned(),
                detail: v.to_string(),
            });
        }
    }
    let in_place = next.id == old.id;
    // The edited item must not collide with a different live item already on
    // record under that content (a person made the same statement twice).
    if !in_place
        && let Some(existing) = load(&st, &next.id)?
        && matches!(existing.status, Status::Proposed | Status::Curated)
    {
        return Ok(Edited::Refused {
            code: "DUPLICATE".into(),
            detail: format!(
                "an item with this topic and content already exists ({})",
                existing.id
            ),
        });
    }
    let mut reqs = Vec::new();
    let mut retired = Vec::new();
    let new_row = row_of(&next);
    reqs.push(request(
        ctx,
        &next.id,
        vec![memory_event(
            "MemoryEdited",
            &next.id,
            Some(&new_row),
            json!({
                "old_memory_id": old.id,
                "new_memory_id": next.id,
                "in_place": in_place,
                "scope": next.scope.key(),
            }),
            &ctx.actor,
        )],
    ));
    if !in_place {
        retired.push(old.id.clone());
        let mut gone = old.clone();
        if old.status == Status::Curated {
            // The superseded item stays on record, marked.
            gone.status = Status::Superseded;
            let row = row_of(&gone);
            reqs.push(request(
                ctx,
                &old.id,
                vec![memory_event(
                    "MemorySuperseded",
                    &old.id,
                    Some(&row),
                    json!({"by": next.id, "cause": "edit"}),
                    &ctx.actor,
                )],
            ));
        } else {
            // A proposal that was edited is replaced, not kept.
            reqs.push(request(
                ctx,
                &old.id,
                vec![memory_event(
                    "MemoryForgotten",
                    &old.id,
                    None,
                    json!({"mode": "delete", "removed": true, "prior_status": "proposed", "replaced_by": next.id}),
                    &ctx.actor,
                )],
            ));
        }
    }
    let (events, replayed) = commit(&mut st, command, reqs)?;
    Ok(Edited::Done {
        old_id: old.id,
        new_id: next.id,
        status: next.status,
        retired,
        offset: events.last().map_or(0, |e| e.offset),
        replayed,
    })
}

/// What was chosen for a task's prompt, and what the Inspector shows of it.
#[derive(Debug, Default)]
pub struct PromptMemory {
    /// The fragments for the prompt compiler, best first.
    pub fragments: Vec<MemoryFragment>,
    /// The pack the fragments came from.
    pub pack: Option<MemoryPack>,
    /// In scope, but left out, with the reason.
    pub excluded: Vec<(String, String)>,
}

impl PromptMemory {
    /// The record kept with the turn's ContextCompile step and read by the
    /// Inspector (no memory text: the ids and provenance, never the content).
    #[must_use]
    pub fn record(&self) -> Value {
        let Some(pack) = &self.pack else {
            return json!({
                "pack_id": "",
                "entries": [],
                "excluded": self.excluded.iter().map(|(id, why)| json!({"memory_id": id, "reason": why})).collect::<Vec<_>>(),
            });
        };
        json!({
            "pack_id": pack.pack_id,
            "token_budget": pack.token_budget,
            "token_used": pack.token_used,
            "omitted_count": pack.omitted_count,
            "compiler_version": pack.compiler_version,
            "entries": pack.entries.iter().map(|e| json!({
                "memory_id": e.memory_id,
                "scope": e.provenance.scope,
                "record_type": e.record_type,
                "topic": e.topic,
                "source": e.provenance.source,
                "author": e.provenance.author,
                "confidence": e.provenance.confidence,
                "validated": e.provenance.validated,
                "token_cost": e.token_cost,
                "reasons": e.reasons,
                "conflicts_with": e.conflicts_with,
                "clipped": e.clipped,
                "created_at_ms": e.provenance.created_at_ms,
                "expires_at_ms": e.provenance.expires_at_ms,
                "last_validation_revision": e.provenance.last_validation_revision,
            })).collect::<Vec<_>>(),
            "excluded": self.excluded.iter().map(|(id, why)| json!({"memory_id": id, "reason": why})).collect::<Vec<_>>(),
        })
    }
}

/// Choose the memory that goes into a task's prompt: the curated items of the
/// task's scope chain, by scope and relevance to the goal
/// ([`MemoryStore::select`]), scanned like any other untrusted text (an item
/// shaped like an instruction to the agent is left out and says why, never
/// injected), and packed under the segment's token budget by the Context Pack
/// compiler. Nothing here is authority: the result is prompt text.
pub async fn select_for_prompt(
    store: &Arc<Mutex<EventStore>>,
    chain: &ScopeChain,
    goal: &str,
    workspace_root: Option<&str>,
) -> PromptMemory {
    let keys = chain.keys();
    let rows = {
        let st = store.lock().await;
        st.memory_in_scopes(&keys).unwrap_or_default()
    };
    let mut mem = MemoryStore::new();
    let mut needs_revision = false;
    for r in &rows {
        if let Some(item) = item_of(r) {
            needs_revision |= item.status == Status::Curated
                && matches!(item.scope, Scope::Repository { .. })
                && (item.record_type == RecordType::Fact || item.source == Source::RepositoryScan);
            mem.upsert(item);
        }
    }
    if mem.all().next().is_none() {
        return PromptMemory::default();
    }
    let revision = match (needs_revision, workspace_root) {
        (true, Some(root)) => git_head(root.to_owned()).await,
        _ => None,
    };
    let now = Timestamp::now().0;
    let selection: Selection<'_> = mem.select(&SelectionInput {
        scope_keys: &keys,
        goal,
        hints: &[],
        now_ms: now,
        current_revision: revision.as_deref(),
    });
    let mut excluded: Vec<(String, String)> = selection
        .excluded
        .iter()
        .map(|e| (e.id.clone(), e.reason.clone()))
        .collect();
    let mut candidates: Vec<MemoryCandidate> = Vec::new();
    for s in &selection.selected {
        let item = s.item;
        // Untrusted-origin text: the same scan every tool result gets.
        let findings =
            modbit_browser::injection::scan_all([item.topic.as_str(), item.content.as_str()]);
        if let Some(f) = findings.first() {
            excluded.push((item.id.clone(), format!("injection_suspected:{}", f.shape)));
            continue;
        }
        candidates.push(MemoryCandidate {
            memory_id: item.id.clone(),
            scope: item.scope.key(),
            scope_kind: item.scope.kind().to_owned(),
            record_type: label(item.record_type),
            topic: item.topic.clone(),
            content: item.content.clone(),
            source: label(item.source),
            author: item.author.clone(),
            confidence: item.confidence,
            created_at_ms: item.created_at_ms,
            expires_at_ms: item.expires_at_ms,
            validated: item.validated,
            last_validation_revision: item.last_validation_revision.clone(),
            score: s.score,
            reasons: s.reasons.clone(),
            conflicts_with: s.conflicts_with.clone(),
        });
    }
    let pack = pack_memory(&candidates, PROMPT_MEMORY_BUDGET_TOKENS);
    let fragments = pack
        .entries
        .iter()
        .map(|e| MemoryFragment {
            memory_id: e.memory_id.clone(),
            scope: e.provenance.scope.clone(),
            record_type: e.record_type.clone(),
            topic: e.topic.clone(),
            source: e.provenance.source.clone(),
            author: e.provenance.author.clone(),
            confidence: e.provenance.confidence,
            validated: e.provenance.validated,
            created_at_ms: e.provenance.created_at_ms,
            expires_at_ms: e.provenance.expires_at_ms,
            text: e.text.clone(),
            conflicts_with: e.conflicts_with.clone(),
        })
        .collect();
    excluded.sort();
    PromptMemory {
        fragments,
        pack: Some(pack),
        excluded,
    }
}

/// The memory port bound to one task's invocation.
pub struct CoreMemory {
    store: Arc<Mutex<EventStore>>,
    chain: ScopeChain,
    ctx: MemoryCtx,
    workspace_root: Option<String>,
}

impl CoreMemory {
    /// A port for a task, over the Core's store and the task's scope chain.
    /// `workspace_root` lets a repository-scoped proposal bind to the
    /// repository's current revision at propose time (docs/19: repository
    /// facts bind to a revision and can stale).
    #[must_use]
    pub fn new(
        store: Arc<Mutex<EventStore>>,
        chain: ScopeChain,
        ctx: MemoryCtx,
        workspace_root: Option<String>,
    ) -> Self {
        CoreMemory {
            store,
            chain,
            ctx,
            workspace_root,
        }
    }

    fn author(&self) -> String {
        match &self.ctx.actor {
            Actor::User(u) => format!("user:{u}"),
            Actor::Agent(a) => format!("agent:{a}"),
            Actor::Core(c) => format!("core:{c}"),
            Actor::External(x) => format!("external:{x}"),
        }
    }
}

impl modbit_tools::pipeline::MemoryPort for CoreMemory {
    fn query<'a>(&'a self, args: &'a Value) -> BoxFuture<'a, Result<Value, (String, String)>> {
        Box::pin(async move {
            let keys = self.chain.keys();
            let rows = {
                let st = self.store.lock().await;
                st.memory_in_scopes(&keys)
                    .map_err(|e| ("MEMORY_STORE".to_owned(), e.to_string()))?
            };
            let mut store = MemoryStore::new();
            for r in &rows {
                if let Some(item) = item_of(r) {
                    store.upsert(item);
                }
            }
            let now = Timestamp::now().0;
            let want_type = args
                .get("record_type")
                .and_then(Value::as_str)
                .and_then(parse_record_type);
            let want_topic = args
                .get("topic")
                .and_then(Value::as_str)
                .map(|t| t.trim().to_lowercase());
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty());
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(50)
                .clamp(1, 200) as usize;
            // With `text`, the items that share words with it, best first;
            // without, every curated item in scope, newest first.
            let relevant: Option<Vec<String>> = text.map(|t| {
                store
                    .select(&SelectionInput {
                        scope_keys: &keys,
                        goal: t,
                        hints: &[],
                        now_ms: now,
                        current_revision: None,
                    })
                    .selected
                    .iter()
                    .filter(|s| s.reasons.iter().any(|r| r.starts_with("relevant")))
                    .map(|s| s.item.id.clone())
                    .collect()
            });
            let mut items: Vec<&MemoryItem> = store
                .query(&keys, now)
                .into_iter()
                .filter(|i| want_type.is_none_or(|t| i.record_type == t))
                .filter(|i| {
                    want_topic
                        .as_ref()
                        .is_none_or(|t| i.topic.trim().to_lowercase() == *t)
                })
                .collect();
            if let Some(order) = &relevant {
                items.retain(|i| order.contains(&i.id));
                items.sort_by_key(|i| order.iter().position(|id| id == &i.id));
            }
            let items: Vec<Value> = items.into_iter().take(limit).map(view).collect();
            Ok(json!({
                "items": items,
                "scopes": keys,
                "conflicts": store.conflicts(now),
            }))
        })
    }

    fn propose<'a>(&'a self, args: &'a Value) -> BoxFuture<'a, Result<Value, (String, String)>> {
        Box::pin(async move {
            let bad = |m: &str| Err(("BAD_ARGUMENT".to_owned(), m.to_owned()));
            let Some(record_type) = args
                .get("record_type")
                .and_then(Value::as_str)
                .and_then(parse_record_type)
            else {
                return bad("record_type required");
            };
            let topic = args
                .get("topic")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            let content = args
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if topic.is_empty() || content.trim().is_empty() {
                return bad("topic and content required");
            }
            let source = args
                .get("source")
                .and_then(Value::as_str)
                .and_then(parse_source)
                .unwrap_or(Source::AgentObserved);
            let confidence = args
                .get("confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.5) as f32;
            let scope = self
                .chain
                .resolve(args.get("scope").and_then(Value::as_str))
                .map_err(|e| ("BAD_ARGUMENT".to_owned(), e))?;
            let now = Timestamp::now().0;
            let mut item = MemoryItem::propose(
                scope,
                record_type,
                topic,
                content,
                source,
                self.author(),
                confidence,
                now,
            );
            if let Some(ttl) = args.get("ttl_ms").and_then(Value::as_i64) {
                item.expires_at_ms = Some(now.saturating_add(ttl));
            }
            if args.get("sensitivity").and_then(Value::as_str) == Some("sensitive") {
                item.sensitivity = Sensitivity::Sensitive;
            }
            // A repository-scoped proposal binds to the repository's current
            // revision at propose time, so a later promotion can tell it is
            // still current (docs/19: repository facts stale as the tree
            // moves on).
            if matches!(item.scope, Scope::Repository { .. })
                && let Some(root) = &self.workspace_root
            {
                item.last_validation_revision = git_head(root.clone()).await;
            }
            // A proposal is a candidate on the log (status `Proposed`); it is
            // never promoted here.
            let done = propose(&self.store, &self.ctx, item, None)
                .await
                .map_err(|e| ("MEMORY_STORE".to_owned(), e))?;
            Ok(json!({
                "id": done.id,
                "status": label(done.status),
                "scope": done.scope,
                "existed": done.existed,
                "promotion": "separate",
                "note": if done.existed {
                    "this exact item is already on record; nothing changed"
                } else {
                    "recorded as a candidate; promotion to curated memory is a separate governed step (a person or policy)."
                },
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_chain_is_narrowest_first_includes_agent_and_space_and_refuses_what_it_lacks() {
        let chain = ScopeChain::for_task(
            TenantId::new(),
            SessionId::new(),
            Some("run1"),
            "user1",
            Some("/repo"),
            Some("space1"),
            Some("primary"),
        );
        let keys = chain.keys();
        assert!(keys[0].starts_with("run:"));
        let kinds: Vec<&str> = chain.scopes.iter().map(Scope::kind).collect();
        assert_eq!(
            kinds,
            [
                "run",
                "session",
                "user",
                "agent_profile",
                "repository",
                "space",
                "organization"
            ]
        );
        assert!(keys.iter().any(|k| k == "repository:/repo"));
        assert!(keys.iter().any(|k| k == "agent_profile:primary"));
        assert!(keys.iter().any(|k| k == "space:space1"));
        assert!(keys.last().unwrap().starts_with("organization:"));
        // A named scope resolves to this task's scope of that kind.
        assert_eq!(
            chain.resolve(Some("repository")).unwrap().key(),
            "repository:/repo"
        );
        assert_eq!(
            chain.resolve(Some("agent")).unwrap().key(),
            "agent_profile:primary"
        );
        assert_eq!(chain.resolve(Some("space")).unwrap().key(), "space:space1");
        assert!(
            chain
                .resolve(Some("user"))
                .unwrap()
                .key()
                .starts_with("user:")
        );
        assert!(chain.resolve(None).unwrap().key().starts_with("session:"));
        // A name that is no scope is refused, not turned into the session.
        assert!(
            chain
                .resolve(Some("galaxy"))
                .unwrap_err()
                .contains("unknown memory scope")
        );
        // A task with no repository, run, space or agent profile still has a
        // chain (session, user, org), and asking for a scope it lacks is an
        // error rather than a quiet fallback to the session.
        let bare = ScopeChain::for_task(
            TenantId::new(),
            SessionId::new(),
            None,
            "",
            None,
            None,
            None,
        );
        assert!(
            bare.scopes
                .iter()
                .all(|s| !matches!(s, Scope::Repository { .. } | Scope::Space { .. }))
        );
        assert!(
            bare.resolve(Some("repository"))
                .unwrap_err()
                .contains("not available")
        );
        assert!(bare.resolve(Some("space")).is_err());
    }

    #[test]
    fn row_round_trips_through_item() {
        let item = MemoryItem::propose(
            Scope::User { id: "u".into() },
            RecordType::Decision,
            "topic",
            "content",
            Source::UserStated,
            "user:u",
            0.9,
            1234,
        );
        let row = row_of(&item);
        assert_eq!(row.status, "proposed");
        assert_eq!(row.scope_key, "user:u");
        assert_eq!(row.record_type, "decision");
        assert_eq!(item_of(&row).unwrap(), item);
    }

    #[test]
    fn an_id_prefix_resolves_only_when_it_is_unambiguous() {
        let row = |id: &str| MemoryRow {
            id: id.to_owned(),
            scope_key: "user:u".into(),
            record_type: "fact".into(),
            topic: "t".into(),
            status: "curated".into(),
            sensitivity: "normal".into(),
            created_at_ms: 1,
            expires_at_ms: None,
            updated_at_ms: 1,
            doc: String::new(),
        };
        let rows = vec![row("abcdef0123"), row("abcdef9999"), row("ffff000011")];
        assert_eq!(
            resolve_prefix(&rows, "ffff0000").unwrap().as_deref(),
            Some("ffff000011")
        );
        assert!(
            resolve_prefix(&rows, "abcdef").unwrap().is_none(),
            "too short"
        );
        assert!(resolve_prefix(&rows, "abcdef01").unwrap().is_some());
        assert!(resolve_prefix(&rows, "abcdef9").unwrap().is_none());
        let both = vec![row("abcdef0123"), row("abcdef0199")];
        assert!(resolve_prefix(&both, "abcdef01").is_err());
        assert_eq!(
            resolve_prefix(&both, "abcdef0123").unwrap().as_deref(),
            Some("abcdef0123")
        );
    }
}
