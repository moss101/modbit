//! The Core's governed engineering memory (M9.1, REQ-EV-0162, docs/19).
//!
//! `CoreMemory` implements `modbit_tools::pipeline::MemoryPort` over the
//! durable memory store (`memory_items`, event-store V16) and the modbit-memory
//! domain: it resolves the task's scope chain, records a proposal
//! (`memory.propose`) and reads curated memory (`memory.query`). It never
//! promotes — promotion is a separate governed step ([`promote`]), so a
//! proposal from a transcript summary never becomes durable memory on its
//! own (the M9.1 invariant).
//!
//! Memory is cross-session, mutable, governed state and is its own durable
//! store, not a projection of the per-session event log; a promotion is
//! also recorded on the acting session's log (`MemoryPromoted`) so the
//! governed act is auditable there.

use std::sync::Arc;

use modbit_domain::{SessionId, TenantId, Timestamp};
use modbit_event_store::{EventStore, MemoryRow};
use modbit_memory::{
    MemoryItem, MemoryStore, PromotionContext, PromotionRefusal, RecordType, Scope, Sensitivity,
    Source, Status, promotion_check,
};
use modbit_tools::registry::BoxFuture;
use serde_json::{Value, json};
use tokio::sync::Mutex;

/// The scope chain a task's memory query reads and a proposal defaults into.
/// Narrowest first: the run, the session, the user, the repository (the
/// canonical workspace root), and the organization (the tenant).
#[derive(Clone, Debug)]
pub struct ScopeChain {
    /// The scopes, narrowest first.
    pub scopes: Vec<Scope>,
}

impl ScopeChain {
    /// Build the chain from the task's identity. The repository scope is the
    /// canonical workspace root (a stable per-repo key); a task with no root
    /// has no repository scope.
    #[must_use]
    pub fn for_task(
        tenant: TenantId,
        session: SessionId,
        run: Option<&str>,
        user: &str,
        workspace_root: Option<&str>,
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
        if let Some(root) = workspace_root {
            scopes.push(Scope::Repository {
                id: root.to_owned(),
            });
        }
        scopes.push(Scope::Organization {
            id: tenant.to_string(),
        });
        ScopeChain { scopes }
    }

    fn keys(&self) -> Vec<String> {
        self.scopes.iter().map(Scope::key).collect()
    }

    /// The scope named by a `memory.propose` `scope` argument, resolved
    /// against the chain (so a proposal at, say, `user` scope binds to this
    /// task's user). Defaults to the session.
    fn resolve(&self, named: Option<&str>) -> Scope {
        let want = named.unwrap_or("session");
        self.scopes
            .iter()
            .find(|s| s.key().split(':').next() == Some(want))
            .cloned()
            .unwrap_or_else(|| Scope::Session {
                id: self
                    .scopes
                    .iter()
                    .find_map(|s| match s {
                        Scope::Session { id } => Some(id.clone()),
                        _ => None,
                    })
                    .unwrap_or_default(),
            })
    }
}

/// Build a scope chain for a task from the Core's identity — the same chain
/// its runs use, so a promotion or an inspection reads exactly what the
/// task's memory tools do.
#[must_use]
pub fn scope_chain_for(
    tenant: TenantId,
    user: modbit_domain::UserId,
    task: &modbit_domain::task::Task,
) -> ScopeChain {
    ScopeChain::for_task(
        tenant,
        task.session_id,
        None,
        &user.to_string(),
        task.workspace_root.as_deref(),
    )
}

/// Every memory item in a scope chain, for inspection: each item's view and
/// the conflict groups among the curated ones (nothing is resolved here).
pub async fn list_scoped(
    store: &Arc<Mutex<EventStore>>,
    chain: &ScopeChain,
) -> Result<(Vec<Value>, Vec<String>, Vec<Vec<String>>), String> {
    let keys = chain.keys();
    let rows = {
        let st = store.lock().await;
        st.memory_in_scopes(&keys).map_err(|e| e.to_string())?
    };
    let mut mem = MemoryStore::new();
    let mut views = Vec::new();
    for r in &rows {
        if let Some(item) = item_of(r) {
            views.push(view(&item));
            mem.upsert(item);
        }
    }
    let now = Timestamp::now().0;
    Ok((views, keys, mem.conflicts(now)))
}

/// The memory port bound to one task's invocation.
pub struct CoreMemory {
    store: Arc<Mutex<EventStore>>,
    chain: ScopeChain,
    author: String,
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
        author: String,
        workspace_root: Option<String>,
    ) -> Self {
        CoreMemory {
            store,
            chain,
            author,
            workspace_root,
        }
    }
}

/// The git HEAD of `root`, if it is a repository with a commit.
async fn git_head(root: String) -> Option<String> {
    tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()?;
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

/// Serialize an item to a store row.
fn row_of(item: &MemoryItem) -> MemoryRow {
    MemoryRow {
        id: item.id.clone(),
        scope_key: item.scope.key(),
        record_type: serde_json::to_value(item.record_type)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
        topic: item.topic.clone(),
        status: serde_json::to_value(item.status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
        sensitivity: serde_json::to_value(item.sensitivity)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
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
fn view(item: &MemoryItem) -> Value {
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
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(50)
                .clamp(1, 200) as usize;
            let items: Vec<Value> = store
                .query(&keys, now)
                .into_iter()
                .filter(|i| want_type.is_none_or(|t| i.record_type == t))
                .filter(|i| {
                    want_topic
                        .as_ref()
                        .is_none_or(|t| i.topic.trim().to_lowercase() == *t)
                })
                .take(limit)
                .map(view)
                .collect();
            Ok(json!({
                "items": items,
                "scopes": keys,
                "conflicts": store.conflicts(now),
            }))
        })
    }

    fn propose<'a>(&'a self, args: &'a Value) -> BoxFuture<'a, Result<Value, (String, String)>> {
        Box::pin(async move {
            let Some(record_type) = args
                .get("record_type")
                .and_then(Value::as_str)
                .and_then(parse_record_type)
            else {
                return Err(("BAD_ARGUMENT".to_owned(), "record_type required".to_owned()));
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
                return Err((
                    "BAD_ARGUMENT".to_owned(),
                    "topic and content required".to_owned(),
                ));
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
                .resolve(args.get("scope").and_then(Value::as_str));
            let now = Timestamp::now().0;
            let mut item = MemoryItem::propose(
                scope,
                record_type,
                topic,
                content,
                source,
                self.author.clone(),
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
            // A proposal is stored as-is (status `Proposed`); it is never
            // promoted here.
            let row = row_of(&item);
            {
                let st = self.store.lock().await;
                st.memory_upsert(&row)
                    .map_err(|e| ("MEMORY_STORE".to_owned(), e.to_string()))?;
            }
            Ok(json!({
                "id": item.id,
                "status": "proposed",
                "scope": item.scope.key(),
                "promotion": "separate",
                "note": "recorded as a candidate; promotion to curated memory is a separate governed step (a person or policy).",
            }))
        })
    }
}

/// The outcome of a promotion attempt (a governed step, not a tool).
#[derive(Debug)]
pub enum Promoted {
    /// Promoted to curated; the ids it superseded (now `Superseded`).
    Curated {
        /// Ids it superseded.
        superseded: Vec<String>,
    },
    /// The proposal is gone.
    Unknown,
    /// Refused with a typed reason.
    Refused(PromotionRefusal),
}

/// Promote a proposed item to curated, applying the promotion gate against
/// the durable store. `scope_permits_sensitive` and
/// `current_repository_revision` come from the caller's policy and
/// workspace. This is the only path that makes durable memory; the tools
/// never call it.
pub async fn promote(
    store: &Arc<Mutex<EventStore>>,
    id: &str,
    ctx: &PromotionContext,
) -> Result<Promoted, String> {
    let st = store.lock().await;
    let Some(row) = st.memory_get(id).map_err(|e| e.to_string())? else {
        return Ok(Promoted::Unknown);
    };
    let Some(item) = item_of(&row) else {
        return Ok(Promoted::Unknown);
    };
    if let Err(refusal) = promotion_check(&item, ctx) {
        return Ok(Promoted::Refused(refusal));
    }
    // Load the item's scope into a MemoryStore to apply supersession.
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
    // Persist the item as curated and each superseded item.
    for changed_id in std::iter::once(id.to_owned()).chain(superseded.iter().cloned()) {
        if let Some(it) = mem.get(&changed_id) {
            st.memory_upsert(&row_of(it)).map_err(|e| e.to_string())?;
        }
    }
    Ok(Promoted::Curated { superseded })
}

/// Set an item's status directly (delete/supersede by an allowed actor).
/// Returns whether a row was changed.
pub async fn set_status(
    store: &Arc<Mutex<EventStore>>,
    id: &str,
    status: Status,
) -> Result<bool, String> {
    let st = store.lock().await;
    let Some(row) = st.memory_get(id).map_err(|e| e.to_string())? else {
        return Ok(false);
    };
    let Some(mut item) = item_of(&row) else {
        return Ok(false);
    };
    item.status = status;
    st.memory_upsert(&row_of(&item))
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_chain_is_narrowest_first_and_resolves_a_named_scope() {
        let chain = ScopeChain::for_task(
            TenantId::new(),
            SessionId::new(),
            Some("run1"),
            "user1",
            Some("/repo"),
        );
        let keys = chain.keys();
        assert!(keys[0].starts_with("run:"));
        assert!(keys.iter().any(|k| k.starts_with("session:")));
        assert!(keys.iter().any(|k| k == "repository:/repo"));
        assert!(keys.last().unwrap().starts_with("organization:"));
        // A named scope resolves to this task's scope of that kind.
        assert_eq!(chain.resolve(Some("repository")).key(), "repository:/repo");
        assert!(chain.resolve(Some("user")).key().starts_with("user:"));
        // Unknown/absent → session.
        assert!(chain.resolve(None).key().starts_with("session:"));
        // A task with no repository/run still has a chain (session, user, org).
        let bare = ScopeChain::for_task(TenantId::new(), SessionId::new(), None, "", None);
        assert!(
            bare.scopes
                .iter()
                .all(|s| !matches!(s, Scope::Repository { .. }))
        );
        assert!(
            bare.resolve(Some("repository"))
                .key()
                .starts_with("session:")
        );
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
}
