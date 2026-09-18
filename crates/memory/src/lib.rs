//! `modbit-memory` — governed engineering memory (one canonical memory
//! interface; REQ-EV-0162, docs/19 "Engineering Memory", docs/18).
//!
//! Canonical owner: memory subsystem (`docs/12`, `docs/81`); the dependency
//! direction is enforced by `tools/architecture-lint`.
//!
//! This crate is the **pure domain** of engineering memory: the record
//! schema (scope, type, provenance, confidence, TTL, sensitivity,
//! supersedes/conflicts links, last validation revision), the promotion
//! gate that decides whether a proposed item may become curated durable
//! memory, and the supersession/conflict resolution the product surfaces
//! for inspection. It performs no I/O: the Core wraps it with the SQLite
//! metadata store, the `memory.query` / `memory.propose` tools and the
//! promotion events (docs/17, docs/30). Nothing here reads a transcript or
//! grants authority — a proposal from a transcript summary stays a proposal
//! until an independent validation promotes it (the M9.1 invariant).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The breadth a memory item applies to (docs/19: narrowest → broadest).
/// A scope carries the id of the specific run/session/repository/… it is
/// bound to; the broader scopes (`User`, `Organization`) name the principal
/// or tenant they belong to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum Scope {
    /// A single run.
    Run {
        /// The run id.
        id: String,
    },
    /// A session (across its runs).
    Session {
        /// The session id.
        id: String,
    },
    /// A human user.
    User {
        /// The user id.
        id: String,
    },
    /// An agent profile (a configured agent identity).
    AgentProfile {
        /// The agent profile id.
        id: String,
    },
    /// A repository, bound to the workspace it names.
    Repository {
        /// The repository/workspace id.
        id: String,
    },
    /// A space (a grouping of repositories/sessions).
    Space {
        /// The space id.
        id: String,
    },
    /// An organization / tenant.
    Organization {
        /// The organization/tenant id.
        id: String,
    },
}

impl Scope {
    /// The scope's breadth rank — narrowest (`Run` = 0) to broadest
    /// (`Organization` = 6). A narrower scope's memory is visible within it;
    /// a query at a given scope reads that scope and every broader one it is
    /// contained in (the caller supplies the containing ids).
    #[must_use]
    pub fn rank(&self) -> u8 {
        match self {
            Scope::Run { .. } => 0,
            Scope::Session { .. } => 1,
            Scope::User { .. } => 2,
            Scope::AgentProfile { .. } => 3,
            Scope::Repository { .. } => 4,
            Scope::Space { .. } => 5,
            Scope::Organization { .. } => 6,
        }
    }

    /// A stable key for the scope (`kind:id`), used for grouping and dedup.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Scope::Run { id } => format!("run:{id}"),
            Scope::Session { id } => format!("session:{id}"),
            Scope::User { id } => format!("user:{id}"),
            Scope::AgentProfile { id } => format!("agent_profile:{id}"),
            Scope::Repository { id } => format!("repository:{id}"),
            Scope::Space { id } => format!("space:{id}"),
            Scope::Organization { id } => format!("organization:{id}"),
        }
    }
}

/// The kind of engineering knowledge an item records (docs/19).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordType {
    /// A decision that was made (and why).
    Decision,
    /// A convention to follow.
    Convention,
    /// A durable fact about the system.
    Fact,
    /// A procedure (how to do a thing).
    Procedure,
    /// A failure pattern (what went wrong, how it is recognized).
    FailurePattern,
    /// Knowledge about a dependency.
    DependencyKnowledge,
    /// A user's stated preference.
    UserPreference,
}

/// Where an item came from — the origin that decides how far it must be
/// validated before it can be promoted (docs/19 "Promotion rules").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The user stated it directly (trusted at the source).
    UserStated,
    /// The agent observed it while working (an inference, not yet checked).
    AgentObserved,
    /// A summary of a transcript — never promotable on its own (docs/19).
    TranscriptSummary,
    /// Content read from the web (untrusted until validated).
    WebContent,
    /// Output of a tool (untrusted until validated).
    ToolOutput,
    /// Told by a peer agent (untrusted until validated).
    PeerAgent,
    /// Scanned from the repository at a revision (binds to that revision).
    RepositoryScan,
}

impl Source {
    /// Whether an item from this source may be promoted on its own — i.e.
    /// without an independent validation recorded on it. Only what the user
    /// stated directly is trusted at its source.
    #[must_use]
    pub fn promotable_unvalidated(self) -> bool {
        matches!(self, Source::UserStated)
    }
}

/// How restricted an item is. Sensitive memory may be promoted only into a
/// scope the policy permits (the caller decides that permission; the gate
/// enforces that a sensitive item is not promoted without it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    /// Ordinary engineering knowledge.
    Normal,
    /// Sensitive — a permitted scope is required to promote it.
    Sensitive,
}

/// The lifecycle state of a memory item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Proposed (`memory.propose`): a candidate, never read by `memory.query`.
    Proposed,
    /// Curated: promoted, durable, readable by scoped queries.
    Curated,
    /// Superseded by a later curated item.
    Superseded,
    /// Deleted by an allowed actor.
    Deleted,
    /// Past its TTL — no longer read, kept for inspection.
    Expired,
}

/// One governed engineering-memory item (docs/19: every field an item
/// stores). Values are the item's own; the store adds nothing implicit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryItem {
    /// Stable id (content-addressed at proposal; see [`MemoryItem::propose`]).
    pub id: String,
    /// The breadth it applies to.
    pub scope: Scope,
    /// The kind of knowledge.
    pub record_type: RecordType,
    /// A short, stable subject the item is about — its dedup/conflict key
    /// within a scope and record type (e.g. "test runner", "auth flow").
    pub topic: String,
    /// The knowledge itself (data, never authority).
    pub content: String,
    /// Where it came from.
    pub source: Source,
    /// Who recorded it (an actor label: `user:<id>`, `agent:<id>`, …).
    pub author: String,
    /// Confidence in the item, `0.0..=1.0`.
    pub confidence: f32,
    /// When it was created (ms).
    pub created_at_ms: i64,
    /// When it expires (ms); `None` = no TTL.
    pub expires_at_ms: Option<i64>,
    /// How restricted it is.
    pub sensitivity: Sensitivity,
    /// Items this one replaces (their ids) when it is promoted.
    pub supersedes: Vec<String>,
    /// Items this one is recorded to conflict with (their ids).
    pub conflicts: Vec<String>,
    /// The repository revision this item was last validated against
    /// (required to promote a `Repository`-scoped item; it staled when the
    /// revision moves on).
    pub last_validation_revision: Option<String>,
    /// Whether an independent validation has been recorded on the item (a
    /// check beyond the source itself). A proposal carries `false`.
    pub validated: bool,
    /// The lifecycle state.
    pub status: Status,
}

impl MemoryItem {
    /// A fresh proposal. Its id is content-addressed over the scope, record
    /// type, topic and content, so the same proposal made twice is one item
    /// (the store can dedup on id). A proposal is never validated and is
    /// always `Proposed`.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn propose(
        scope: Scope,
        record_type: RecordType,
        topic: impl Into<String>,
        content: impl Into<String>,
        source: Source,
        author: impl Into<String>,
        confidence: f32,
        created_at_ms: i64,
    ) -> Self {
        let topic = topic.into();
        let content = content.into();
        let mut h = Sha256::new();
        h.update(scope.key().as_bytes());
        h.update([0]);
        h.update(serde_json::to_vec(&record_type).unwrap_or_default());
        h.update([0]);
        h.update(topic.as_bytes());
        h.update([0]);
        h.update(content.as_bytes());
        let id = hex::encode(h.finalize());
        MemoryItem {
            id,
            scope,
            record_type,
            topic,
            content,
            source,
            author: author.into(),
            confidence: confidence.clamp(0.0, 1.0),
            created_at_ms,
            expires_at_ms: None,
            sensitivity: Sensitivity::Normal,
            supersedes: Vec::new(),
            conflicts: Vec::new(),
            last_validation_revision: None,
            validated: false,
            status: Status::Proposed,
        }
    }

    /// The key an item occupies within its scope: scope + record type +
    /// topic. Two curated items with the same key are in conflict unless one
    /// supersedes the other.
    #[must_use]
    pub fn conflict_key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.scope.key(),
            serde_json::to_string(&self.record_type).unwrap_or_default(),
            self.topic.trim().to_lowercase()
        )
    }

    /// Whether the item is past its TTL at `now_ms`.
    #[must_use]
    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.expires_at_ms.is_some_and(|e| now_ms >= e)
    }
}

/// What the caller knows at promotion time that the item itself cannot:
/// whether the item's scope is one the policy permits for sensitive memory,
/// and the current repository revision (to check a repository fact is not
/// stale). Supplied by the Core from the lease/policy and the workspace.
#[derive(Clone, Debug, Default)]
pub struct PromotionContext {
    /// The scope is permitted to hold sensitive memory (from policy).
    pub scope_permits_sensitive: bool,
    /// The current repository revision, when the workspace has one.
    pub current_repository_revision: Option<String>,
    /// Now (ms), to reject an already-expired proposal.
    pub now_ms: i64,
}

/// Why a promotion was refused (docs/19 "Promotion rules"). Each is a
/// distinct, inspectable reason — the product shows it, the agent never
/// works around it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PromotionRefusal {
    /// The item is not a proposal (already curated/superseded/deleted).
    NotProposed {
        /// The state it is in instead of `Proposed`.
        status: Status,
    },
    /// A transcript summary cannot promote on its own — it needs an
    /// independent validation first.
    TranscriptNotValidated,
    /// Web/tool/peer content is untrusted until validated.
    UntrustedSourceNotValidated {
        /// The untrusted source.
        source: Source,
    },
    /// A repository-scoped fact must bind to the current revision; it is
    /// missing or stale.
    RepositoryRevisionStale {
        /// The revision the item is bound to, if any.
        pinned: Option<String>,
        /// The workspace's current revision, if any.
        current: Option<String>,
    },
    /// Sensitive memory needs a policy-permitted scope.
    SensitiveScopeNotPermitted,
    /// The proposal is already expired.
    Expired,
    /// The content is empty.
    Empty,
}

/// Decide whether a proposed item may become curated durable memory. Pure:
/// the same item and context always give the same verdict, and the reason
/// (when refused) is one the product can show. Promotion never happens for a
/// transcript summary that carries no independent validation — the M9.1
/// invariant.
pub fn promotion_check(item: &MemoryItem, ctx: &PromotionContext) -> Result<(), PromotionRefusal> {
    if item.status != Status::Proposed {
        return Err(PromotionRefusal::NotProposed {
            status: item.status,
        });
    }
    if item.content.trim().is_empty() {
        return Err(PromotionRefusal::Empty);
    }
    if item.is_expired(ctx.now_ms) {
        return Err(PromotionRefusal::Expired);
    }
    // A transcript summary alone cannot promote (docs/19).
    if item.source == Source::TranscriptSummary && !item.validated {
        return Err(PromotionRefusal::TranscriptNotValidated);
    }
    // Web/tool/peer content is untrusted until validated.
    if !item.source.promotable_unvalidated()
        && item.source != Source::RepositoryScan
        && item.source != Source::AgentObserved
        && !item.validated
    {
        return Err(PromotionRefusal::UntrustedSourceNotValidated {
            source: item.source,
        });
    }
    // An agent's own observation is an inference — it too needs a validation
    // before it becomes durable (only what the user stated is trusted at the
    // source).
    if item.source == Source::AgentObserved && !item.validated {
        return Err(PromotionRefusal::UntrustedSourceNotValidated {
            source: item.source,
        });
    }
    // Repository facts bind to the revision; a scan must carry the current
    // one (docs/19: they stale).
    if matches!(item.scope, Scope::Repository { .. }) || item.source == Source::RepositoryScan {
        match (
            &item.last_validation_revision,
            &ctx.current_repository_revision,
        ) {
            (Some(pinned), Some(current)) if pinned == current => {}
            (pinned, current) => {
                return Err(PromotionRefusal::RepositoryRevisionStale {
                    pinned: pinned.clone(),
                    current: current.clone(),
                });
            }
        }
    }
    // Sensitive memory needs a policy-permitted scope.
    if item.sensitivity == Sensitivity::Sensitive && !ctx.scope_permits_sensitive {
        return Err(PromotionRefusal::SensitiveScopeNotPermitted);
    }
    Ok(())
}

/// An in-memory index of curated and proposed items, for query and for
/// surfacing conflicts. The Core mirrors the durable SQLite store into one
/// of these to answer `memory.query` and the inspection view; this type
/// holds no I/O and is the single place the query and conflict semantics
/// live.
#[derive(Default)]
pub struct MemoryStore {
    items: BTreeMap<String, MemoryItem>,
}

impl MemoryStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace an item by id (the caller's durable record is the
    /// source of truth; this mirrors it).
    pub fn upsert(&mut self, item: MemoryItem) {
        self.items.insert(item.id.clone(), item);
    }

    /// The item by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MemoryItem> {
        self.items.get(id)
    }

    /// Every item, in id order.
    pub fn all(&self) -> impl Iterator<Item = &MemoryItem> {
        self.items.values()
    }

    /// Promote a proposal to curated at `now_ms`, superseding the items it
    /// names (they become `Superseded`). Returns the ids that were
    /// superseded. The caller has already run [`promotion_check`]; this
    /// applies the state change to the mirror.
    pub fn promote(&mut self, id: &str) -> Vec<String> {
        let (supersedes, curated) = match self.items.get_mut(id) {
            Some(item) if item.status == Status::Proposed => {
                item.status = Status::Curated;
                (item.supersedes.clone(), true)
            }
            _ => (Vec::new(), false),
        };
        if !curated {
            return Vec::new();
        }
        let mut done = Vec::new();
        for sid in supersedes {
            if let Some(prior) = self.items.get_mut(&sid)
                && prior.status == Status::Curated
            {
                prior.status = Status::Superseded;
                done.push(sid);
            }
        }
        done
    }

    /// Curated items visible to a query at the given scope keys (the scope
    /// the run is in and every broader scope that contains it, which the
    /// caller supplies), newest first, expired items excluded. Never returns
    /// a proposal — the M9.1 invariant that `memory.query` reads only
    /// curated memory.
    #[must_use]
    pub fn query(&self, scope_keys: &[String], now_ms: i64) -> Vec<&MemoryItem> {
        let mut out: Vec<&MemoryItem> = self
            .items
            .values()
            .filter(|i| i.status == Status::Curated)
            .filter(|i| !i.is_expired(now_ms))
            .filter(|i| scope_keys.iter().any(|k| k == &i.scope.key()))
            .collect();
        out.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms).then(a.id.cmp(&b.id)));
        out
    }

    /// The conflicts among curated items: groups of two or more curated,
    /// unexpired items sharing a [`MemoryItem::conflict_key`] where none is
    /// superseded — the inspectable state QUAL-EV-0162 requires. Each group
    /// is the item ids, in id order.
    #[must_use]
    pub fn conflicts(&self, now_ms: i64) -> Vec<Vec<String>> {
        let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for item in self.items.values() {
            if item.status == Status::Curated && !item.is_expired(now_ms) {
                by_key
                    .entry(item.conflict_key())
                    .or_default()
                    .push(item.id.clone());
            }
        }
        by_key
            .into_values()
            .filter(|ids| ids.len() > 1)
            .map(|mut ids| {
                ids.sort();
                ids
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(source: Source, scope: Scope) -> MemoryItem {
        MemoryItem::propose(
            scope,
            RecordType::Fact,
            "test runner",
            "the project uses cargo nextest",
            source,
            "agent:a",
            0.8,
            1_000,
        )
    }

    #[test]
    fn a_transcript_summary_never_promotes_on_its_own() {
        let it = item(
            Source::TranscriptSummary,
            Scope::Session { id: "s1".into() },
        );
        let ctx = PromotionContext {
            now_ms: 2_000,
            ..Default::default()
        };
        assert_eq!(
            promotion_check(&it, &ctx),
            Err(PromotionRefusal::TranscriptNotValidated)
        );
        // Even validated, a transcript summary needs the validation flag set
        // — with it, it may promote.
        let mut ok = it.clone();
        ok.validated = true;
        assert_eq!(promotion_check(&ok, &ctx), Ok(()));
    }

    #[test]
    fn untrusted_sources_need_validation_but_the_user_is_trusted() {
        let ctx = PromotionContext {
            now_ms: 2_000,
            ..Default::default()
        };
        for s in [
            Source::WebContent,
            Source::ToolOutput,
            Source::PeerAgent,
            Source::AgentObserved,
        ] {
            let it = item(s, Scope::User { id: "u1".into() });
            assert_eq!(
                promotion_check(&it, &ctx),
                Err(PromotionRefusal::UntrustedSourceNotValidated { source: s }),
                "{s:?}"
            );
            let mut v = it.clone();
            v.validated = true;
            assert_eq!(promotion_check(&v, &ctx), Ok(()), "{s:?} validated");
        }
        // What the user stated is trusted at the source.
        let u = item(Source::UserStated, Scope::User { id: "u1".into() });
        assert_eq!(promotion_check(&u, &ctx), Ok(()));
    }

    #[test]
    fn repository_facts_bind_to_the_current_revision() {
        let ctx_at_r2 = PromotionContext {
            current_repository_revision: Some("r2".into()),
            now_ms: 2_000,
            ..Default::default()
        };
        let mut it = item(Source::UserStated, Scope::Repository { id: "repo".into() });
        // No revision on the item: refused.
        assert!(matches!(
            promotion_check(&it, &ctx_at_r2),
            Err(PromotionRefusal::RepositoryRevisionStale { .. })
        ));
        // Bound to an old revision: stale, refused.
        it.last_validation_revision = Some("r1".into());
        assert!(matches!(
            promotion_check(&it, &ctx_at_r2),
            Err(PromotionRefusal::RepositoryRevisionStale { .. })
        ));
        // Bound to the current revision: promotes.
        it.last_validation_revision = Some("r2".into());
        assert_eq!(promotion_check(&it, &ctx_at_r2), Ok(()));
    }

    #[test]
    fn sensitive_memory_needs_a_permitted_scope() {
        let mut it = item(Source::UserStated, Scope::Organization { id: "org".into() });
        it.sensitivity = Sensitivity::Sensitive;
        let denied = PromotionContext {
            now_ms: 2_000,
            ..Default::default()
        };
        assert_eq!(
            promotion_check(&it, &denied),
            Err(PromotionRefusal::SensitiveScopeNotPermitted)
        );
        let permitted = PromotionContext {
            scope_permits_sensitive: true,
            now_ms: 2_000,
            ..Default::default()
        };
        assert_eq!(promotion_check(&it, &permitted), Ok(()));
    }

    #[test]
    fn an_expired_proposal_does_not_promote() {
        let mut it = item(Source::UserStated, Scope::User { id: "u1".into() });
        it.expires_at_ms = Some(1_500);
        let ctx = PromotionContext {
            now_ms: 2_000,
            ..Default::default()
        };
        assert_eq!(promotion_check(&it, &ctx), Err(PromotionRefusal::Expired));
    }

    #[test]
    fn query_reads_only_curated_in_scope_and_promote_supersedes() {
        let mut store = MemoryStore::new();
        let scope = Scope::User { id: "u1".into() };
        let mut a = item(Source::UserStated, scope.clone());
        a.id = "a".into();
        store.upsert(a.clone());
        // A proposal is never returned by query.
        assert!(store.query(&[scope.key()], 2_000).is_empty());
        // Promote a: now visible.
        assert!(store.promote("a").is_empty());
        assert_eq!(
            store
                .query(&[scope.key()], 2_000)
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a"]
        );
        // A later item that supersedes a: promoting it retires a.
        let mut b = MemoryItem::propose(
            scope.clone(),
            RecordType::Fact,
            "test runner",
            "the project uses cargo test now",
            Source::UserStated,
            "user:u1",
            0.9,
            3_000,
        );
        b.id = "b".into();
        b.supersedes = vec!["a".into()];
        store.upsert(b);
        assert_eq!(store.promote("b"), vec!["a".to_owned()]);
        let visible: Vec<&str> = store
            .query(&[scope.key()], 4_000)
            .iter()
            .map(|i| i.id.as_str())
            .collect();
        assert_eq!(visible, vec!["b"], "a is superseded, only b is visible");
        // No conflict now — b superseded a.
        assert!(store.conflicts(4_000).is_empty());
    }

    #[test]
    fn two_curated_items_on_one_topic_without_supersession_are_an_inspectable_conflict() {
        let mut store = MemoryStore::new();
        let scope = Scope::Repository { id: "repo".into() };
        for (id, content) in [("x", "use tabs"), ("y", "use spaces")] {
            let mut it = MemoryItem::propose(
                scope.clone(),
                RecordType::Convention,
                "indentation",
                content,
                Source::UserStated,
                "user:u1",
                0.7,
                1_000,
            );
            it.id = id.into();
            store.upsert(it);
            store.promote(id);
        }
        let conflicts = store.conflicts(2_000);
        assert_eq!(conflicts, vec![vec!["x".to_owned(), "y".to_owned()]]);
        // A query in the scope surfaces both (the product shows the
        // conflict; it does not silently pick one).
        assert_eq!(store.query(&[scope.key()], 2_000).len(), 2);
    }

    #[test]
    fn a_query_reads_the_run_scope_and_the_broader_scopes_it_names() {
        let mut store = MemoryStore::new();
        let run = Scope::Run { id: "r1".into() };
        let org = Scope::Organization { id: "org".into() };
        for (id, scope) in [("in_run", run.clone()), ("in_org", org.clone())] {
            let mut it = MemoryItem::propose(
                scope,
                RecordType::Fact,
                format!("topic-{id}"),
                "x",
                Source::UserStated,
                "user:u1",
                0.5,
                1_000,
            );
            it.id = id.into();
            store.upsert(it);
            store.promote(id);
        }
        // A query naming only the run scope sees the run item, not the org's.
        let run_only: Vec<&str> = store
            .query(&[run.key()], 2_000)
            .iter()
            .map(|i| i.id.as_str())
            .collect();
        assert_eq!(run_only, vec!["in_run"]);
        // Naming both (the run and the org that contains it) sees both.
        let both = store.query(&[run.key(), org.key()], 2_000);
        assert_eq!(both.len(), 2);
    }
}
