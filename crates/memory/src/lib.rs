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

    /// The scope's kind label (`run`, `session`, `user`, `agent_profile`,
    /// `repository`, `space`, `organization`) — the part of [`Scope::key`]
    /// before the colon.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Scope::Run { .. } => "run",
            Scope::Session { .. } => "session",
            Scope::User { .. } => "user",
            Scope::AgentProfile { .. } => "agent_profile",
            Scope::Repository { .. } => "repository",
            Scope::Space { .. } => "space",
            Scope::Organization { .. } => "organization",
        }
    }

    /// The canonical kind label for a name a person or tool may write
    /// (`agent` and `agent-profile` are `agent_profile`; case and `-` are
    /// forgiven). `None` for a name that is no scope — a caller refuses it
    /// instead of guessing a scope.
    #[must_use]
    pub fn canonical_kind(named: &str) -> Option<&'static str> {
        let n = named.trim().to_ascii_lowercase().replace('-', "_");
        Some(match n.as_str() {
            "run" => "run",
            "session" => "session",
            "user" => "user",
            "agent" | "agent_profile" | "agentprofile" => "agent_profile",
            "repository" | "repo" => "repository",
            "space" => "space",
            "organization" | "org" | "tenant" => "organization",
            _ => return None,
        })
    }

    /// Whether memory in this scope may be sensitive (docs/19: "sensitive
    /// memory requires policy-permitted scope"). The policy is the narrow
    /// ones: what stays with one run, one session or one person. A
    /// repository, a space, an organization or an agent profile is shared
    /// with whoever works there, so sensitive memory is refused in them.
    #[must_use]
    pub fn permits_sensitive(&self) -> bool {
        matches!(
            self,
            Scope::Run { .. } | Scope::Session { .. } | Scope::User { .. }
        )
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

    /// The item after a person's edit (PX-113). Only a proposal or a curated
    /// item can be edited. An edit that changes the topic or the content is
    /// a different item (ids are content-addressed): the result has a new id,
    /// names the edited one in `supersedes`, and the caller retires the old
    /// one in the same transaction. An edit of confidence or time to live
    /// alone keeps the id and the item is replaced in place. What a person
    /// edits is what that person states, so an edit by a `user:` author makes
    /// the item `UserStated` and validated; any other author leaves the source
    /// as it was and the item as unvalidated as it was.
    pub fn edited(
        &self,
        edit: &MemoryEdit,
        author: &str,
        now_ms: i64,
    ) -> Result<Self, EditRefusal> {
        if !matches!(self.status, Status::Proposed | Status::Curated) {
            return Err(EditRefusal::NotEditable {
                status: self.status,
            });
        }
        let topic = edit
            .topic
            .as_deref()
            .map_or_else(|| self.topic.clone(), |t| t.trim().to_owned());
        let content = edit.content.clone().unwrap_or_else(|| self.content.clone());
        if topic.is_empty() || content.trim().is_empty() {
            return Err(EditRefusal::Empty);
        }
        let mut next = MemoryItem::propose(
            self.scope.clone(),
            self.record_type,
            topic,
            content,
            self.source,
            self.author.clone(),
            edit.confidence.unwrap_or(self.confidence),
            self.created_at_ms,
        );
        next.expires_at_ms = match edit.ttl_ms {
            Some(Some(ttl)) => Some(now_ms.saturating_add(ttl)),
            Some(None) => None,
            None => self.expires_at_ms,
        };
        next.sensitivity = edit.sensitivity.unwrap_or(self.sensitivity);
        next.supersedes = self.supersedes.clone();
        next.conflicts = self.conflicts.clone();
        next.last_validation_revision = self.last_validation_revision.clone();
        next.validated = self.validated;
        next.status = self.status;
        if author.starts_with("user:") {
            next.source = Source::UserStated;
            next.validated = true;
            next.author = author.to_owned();
        }
        if next.id != self.id {
            next.created_at_ms = now_ms;
            if !next.supersedes.contains(&self.id) {
                next.supersedes.push(self.id.clone());
            }
        }
        Ok(next)
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

/// What a person changes in an item ([`MemoryItem::edited`]). Absent fields
/// stay as they are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryEdit {
    /// A new topic.
    pub topic: Option<String>,
    /// New content.
    pub content: Option<String>,
    /// A new confidence.
    pub confidence: Option<f32>,
    /// `Some(Some(ms))` sets a time to live from now, `Some(None)` removes it.
    pub ttl_ms: Option<Option<i64>>,
    /// A new sensitivity.
    pub sensitivity: Option<Sensitivity>,
}

/// Why an edit was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EditRefusal {
    /// Only a proposal or a curated item is editable.
    NotEditable {
        /// The state it is in.
        status: Status,
    },
    /// The topic or the content would be empty.
    Empty,
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
                // Promotion is the governed validation: the gate has run and a
                // person or policy has said yes, so the curated item carries
                // that validation from here on.
                item.validated = true;
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

/// What a task offers when it asks which curated memory applies to it.
#[derive(Clone, Debug, Default)]
pub struct SelectionInput<'a> {
    /// The scope keys of the task's chain (any order; precedence comes from
    /// each item's [`Scope::rank`]).
    pub scope_keys: &'a [String],
    /// The task goal (data; only its words are used).
    pub goal: &'a str,
    /// Further words that say what the task is about: selected paths,
    /// symbols, failing checks.
    pub hints: &'a [String],
    /// Now (ms): expired items are not selected.
    pub now_ms: i64,
    /// The repository's current revision, when known: a repository fact
    /// validated against another revision is stale.
    pub current_revision: Option<&'a str>,
}

/// One item chosen for a task, with why.
#[derive(Clone, Debug)]
pub struct Selected<'a> {
    /// The item.
    pub item: &'a MemoryItem,
    /// Ranking score, `0.0..=1.0`.
    pub score: f32,
    /// Human-readable reasons (`relevant: topic,content`, `standing`,
    /// `scope:user`, …).
    pub reasons: Vec<String>,
    /// Other curated items on the same scope, type and topic (an unresolved
    /// conflict, shown to the model as one rather than silently picked).
    pub conflicts_with: Vec<String>,
}

/// Why an item was left out of a selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excluded {
    /// The item.
    pub id: String,
    /// `sensitive` | `stale_revision` | `shadowed_by:<id>` | `irrelevant`.
    pub reason: String,
}

/// The outcome of [`MemoryStore::select`].
#[derive(Clone, Debug, Default)]
pub struct Selection<'a> {
    /// Chosen items, best first (score descending, then narrower scope,
    /// then id — deterministic).
    pub selected: Vec<Selected<'a>>,
    /// What was left out and why.
    pub excluded: Vec<Excluded>,
}

/// Words that carry no relevance signal.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "into", "are", "was", "were", "not",
    "you", "your", "how", "what", "when", "where", "why", "who", "can", "should", "would", "could",
    "has", "have", "had", "its", "our", "use", "using", "add", "fix", "make", "all",
];

fn terms_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        // `compute_total` is both itself and `compute`, `total`.
        for piece in std::iter::once(w).chain(w.split('_')) {
            let t = piece.to_lowercase();
            if t.chars().count() >= 3 && !STOPWORDS.contains(&t.as_str()) && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

impl MemoryStore {
    /// Choose the curated memory that applies to a task: in scope, unexpired,
    /// not sensitive (a sensitive item is never offered to a prompt on its
    /// own), not a repository fact validated against a revision the
    /// repository has moved past, and relevant — it shares words with the
    /// task, or it is a standing convention or preference. An item whose
    /// topic and record type a narrower-scope item also holds is shadowed by
    /// it: precedence is run, session, user, agent profile, repository,
    /// space, organization, and the same key in one scope is a conflict that
    /// is carried, not resolved. Pure and deterministic.
    #[must_use]
    pub fn select<'a>(&'a self, input: &SelectionInput<'_>) -> Selection<'a> {
        let in_scope = self.query(input.scope_keys, input.now_ms);
        let mut terms = terms_of(input.goal);
        for h in input.hints {
            for t in terms_of(h) {
                if !terms.contains(&t) {
                    terms.push(t);
                }
            }
        }
        let mut excluded: Vec<Excluded> = Vec::new();
        let mut candidates: Vec<(&MemoryItem, f32, Vec<String>)> = Vec::new();
        for item in in_scope {
            if item.sensitivity == Sensitivity::Sensitive {
                excluded.push(Excluded {
                    id: item.id.clone(),
                    reason: "sensitive".into(),
                });
                continue;
            }
            let binds_revision = matches!(item.scope, Scope::Repository { .. })
                && (item.record_type == RecordType::Fact || item.source == Source::RepositoryScan);
            if binds_revision
                && let Some(current) = input.current_revision
                && item.last_validation_revision.as_deref() != Some(current)
            {
                excluded.push(Excluded {
                    id: item.id.clone(),
                    reason: "stale_revision".into(),
                });
                continue;
            }
            let topic = item.topic.to_lowercase();
            let content = item.content.to_lowercase();
            let topic_hits = terms.iter().filter(|t| topic.contains(t.as_str())).count();
            let content_hits = terms
                .iter()
                .filter(|t| content.contains(t.as_str()))
                .count();
            let standing = matches!(
                item.record_type,
                RecordType::UserPreference | RecordType::Convention
            );
            if topic_hits == 0 && content_hits == 0 && !standing {
                excluded.push(Excluded {
                    id: item.id.clone(),
                    reason: "irrelevant".into(),
                });
                continue;
            }
            let denom = (3 * terms.len().max(1)) as f32;
            let relevance = ((3 * topic_hits + content_hits) as f32 / denom).min(1.0);
            let precedence = 1.0 - f32::from(item.scope.rank()) / 6.0;
            let score = 0.5 * relevance
                + 0.2 * precedence
                + 0.2 * item.confidence
                + if item.validated || item.source == Source::UserStated {
                    0.1
                } else {
                    0.0
                };
            let mut reasons = Vec::new();
            if topic_hits + content_hits > 0 {
                reasons.push(format!(
                    "relevant: {}",
                    match (topic_hits > 0, content_hits > 0) {
                        (true, true) => "topic,content",
                        (true, false) => "topic",
                        _ => "content",
                    }
                ));
            }
            if standing {
                reasons.push("standing".into());
            }
            reasons.push(format!("scope:{}", item.scope.kind()));
            candidates.push((item, score, reasons));
        }
        // Precedence: the narrowest scope holding a (record type, topic)
        // wins; a broader scope's item on it is shadowed.
        let key_of = |i: &MemoryItem| {
            (
                serde_json::to_string(&i.record_type).unwrap_or_default(),
                i.topic.trim().to_lowercase(),
            )
        };
        let mut narrowest: BTreeMap<(String, String), (u8, String)> = BTreeMap::new();
        for (item, _, _) in &candidates {
            let e = narrowest
                .entry(key_of(item))
                .or_insert((item.scope.rank(), item.id.clone()));
            if (item.scope.rank(), &item.id) < (e.0, &e.1) {
                *e = (item.scope.rank(), item.id.clone());
            }
        }
        let mut kept: Vec<(&MemoryItem, f32, Vec<String>)> = Vec::new();
        for (item, score, reasons) in candidates {
            let (rank, winner) = &narrowest[&key_of(item)];
            if item.scope.rank() > *rank {
                excluded.push(Excluded {
                    id: item.id.clone(),
                    reason: format!("shadowed_by:{winner}"),
                });
            } else {
                kept.push((item, score, reasons));
            }
        }
        let mut selected: Vec<Selected<'_>> = kept
            .iter()
            .map(|(item, score, reasons)| Selected {
                item,
                score: *score,
                reasons: reasons.clone(),
                conflicts_with: kept
                    .iter()
                    .filter(|(o, _, _)| o.id != item.id && o.conflict_key() == item.conflict_key())
                    .map(|(o, _, _)| o.id.clone())
                    .collect(),
            })
            .collect();
        selected.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.item.scope.rank().cmp(&b.item.scope.rank()))
                .then(a.item.id.cmp(&b.item.id))
        });
        excluded.sort_by(|a, b| a.id.cmp(&b.id));
        Selection { selected, excluded }
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

    fn curated(
        store: &mut MemoryStore,
        scope: Scope,
        record_type: RecordType,
        topic: &str,
        content: &str,
    ) -> String {
        let it = MemoryItem::propose(
            scope,
            record_type,
            topic,
            content,
            Source::UserStated,
            "user:u1",
            0.8,
            1_000,
        );
        let id = it.id.clone();
        store.upsert(it);
        store.promote(&id);
        id
    }

    fn chain() -> Vec<String> {
        [
            Scope::Session { id: "s".into() },
            Scope::User { id: "u".into() },
            Scope::AgentProfile {
                id: "primary".into(),
            },
            Scope::Repository { id: "/r".into() },
            Scope::Space { id: "sp".into() },
            Scope::Organization { id: "o".into() },
        ]
        .iter()
        .map(Scope::key)
        .collect()
    }

    #[test]
    fn scope_names_resolve_and_only_the_narrow_scopes_permit_sensitive_memory() {
        assert_eq!(Scope::canonical_kind("agent"), Some("agent_profile"));
        assert_eq!(
            Scope::canonical_kind("Agent-Profile"),
            Some("agent_profile")
        );
        assert_eq!(Scope::canonical_kind("space"), Some("space"));
        assert_eq!(Scope::canonical_kind("galaxy"), None);
        for (scope, permitted) in [
            (Scope::Run { id: "r".into() }, true),
            (Scope::Session { id: "s".into() }, true),
            (Scope::User { id: "u".into() }, true),
            (Scope::AgentProfile { id: "a".into() }, false),
            (Scope::Repository { id: "r".into() }, false),
            (Scope::Space { id: "s".into() }, false),
            (Scope::Organization { id: "o".into() }, false),
        ] {
            assert_eq!(scope.permits_sensitive(), permitted, "{scope:?}");
            assert_eq!(scope.key().split(':').next(), Some(scope.kind()));
        }
    }

    #[test]
    fn a_narrower_scope_shadows_a_broader_one_on_the_same_topic() {
        let mut store = MemoryStore::new();
        let space = curated(
            &mut store,
            Scope::Space { id: "sp".into() },
            RecordType::Convention,
            "indentation",
            "use tabs",
        );
        let agent = curated(
            &mut store,
            Scope::AgentProfile {
                id: "primary".into(),
            },
            RecordType::Convention,
            "indentation",
            "use spaces",
        );
        let user = curated(
            &mut store,
            Scope::User { id: "u".into() },
            RecordType::Convention,
            "indentation",
            "use two spaces",
        );
        let keys = chain();
        let sel = store.select(&SelectionInput {
            scope_keys: &keys,
            goal: "format the module",
            now_ms: 2_000,
            ..Default::default()
        });
        let ids: Vec<&str> = sel.selected.iter().map(|s| s.item.id.as_str()).collect();
        assert_eq!(ids, vec![user.as_str()], "user < agent < space");
        let shadowed: Vec<(&str, &str)> = sel
            .excluded
            .iter()
            .map(|e| (e.id.as_str(), e.reason.as_str()))
            .collect();
        assert!(
            shadowed
                .iter()
                .any(|(id, r)| *id == agent && *r == format!("shadowed_by:{user}"))
                && shadowed
                    .iter()
                    .any(|(id, r)| *id == space && *r == format!("shadowed_by:{user}")),
            "{shadowed:?}"
        );
        // Without the user's own word, the agent profile outranks the space.
        let mut without_user = MemoryStore::new();
        for it in store.all().filter(|i| i.id != user) {
            without_user.upsert(it.clone());
        }
        let sel = without_user.select(&SelectionInput {
            scope_keys: &keys,
            goal: "format the module",
            now_ms: 2_000,
            ..Default::default()
        });
        assert_eq!(sel.selected.len(), 1);
        assert_eq!(sel.selected[0].item.id, agent);
    }

    #[test]
    fn selection_is_by_relevance_and_standing_and_never_offers_sensitive_expired_or_stale_items() {
        let mut store = MemoryStore::new();
        let relevant = curated(
            &mut store,
            Scope::Repository { id: "/r".into() },
            RecordType::Procedure,
            "release process",
            "run the signing step before tagging",
        );
        let unrelated = curated(
            &mut store,
            Scope::Repository { id: "/r".into() },
            RecordType::Procedure,
            "database migrations",
            "never edit an applied migration",
        );
        let standing = curated(
            &mut store,
            Scope::User { id: "u".into() },
            RecordType::UserPreference,
            "commit style",
            "short imperative subjects",
        );
        let mut secret = MemoryItem::propose(
            Scope::User { id: "u".into() },
            RecordType::Fact,
            "release process secret",
            "the release signing passphrase hint",
            Source::UserStated,
            "user:u1",
            0.9,
            1_000,
        );
        secret.sensitivity = Sensitivity::Sensitive;
        let secret_id = secret.id.clone();
        store.upsert(secret);
        store.promote(&secret_id);
        let mut stale = MemoryItem::propose(
            Scope::Repository { id: "/r".into() },
            RecordType::Fact,
            "release process owner",
            "the release process is owned by platform",
            Source::UserStated,
            "user:u1",
            0.9,
            1_000,
        );
        stale.last_validation_revision = Some("old".into());
        let stale_id = stale.id.clone();
        store.upsert(stale);
        store.promote(&stale_id);
        let mut expired = MemoryItem::propose(
            Scope::User { id: "u".into() },
            RecordType::Fact,
            "release process window",
            "releases happen on tuesday",
            Source::UserStated,
            "user:u1",
            0.9,
            1_000,
        );
        expired.expires_at_ms = Some(1_500);
        let expired_id = expired.id.clone();
        store.upsert(expired);
        store.promote(&expired_id);

        let keys = chain();
        let sel = store.select(&SelectionInput {
            scope_keys: &keys,
            goal: "fix the release process signing",
            now_ms: 2_000,
            current_revision: Some("new"),
            ..Default::default()
        });
        let ids: Vec<&str> = sel.selected.iter().map(|s| s.item.id.as_str()).collect();
        assert!(ids.contains(&relevant.as_str()), "{ids:?}");
        assert!(
            ids.contains(&standing.as_str()),
            "a standing preference applies to any task: {ids:?}"
        );
        for out in [&unrelated, &secret_id, &stale_id, &expired_id] {
            assert!(!ids.contains(&out.as_str()), "{out} must not be selected");
        }
        let reason = |id: &str| {
            sel.excluded
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.reason.clone())
        };
        assert_eq!(reason(&unrelated).as_deref(), Some("irrelevant"));
        assert_eq!(reason(&secret_id).as_deref(), Some("sensitive"));
        assert_eq!(reason(&stale_id).as_deref(), Some("stale_revision"));
        // Expired items are not even candidates.
        assert_eq!(reason(&expired_id), None);
        // The relevant item outranks the merely standing one.
        assert_eq!(ids[0], relevant);
    }

    #[test]
    fn two_curated_items_in_one_scope_stay_a_conflict_in_the_selection() {
        let mut store = MemoryStore::new();
        let a = curated(
            &mut store,
            Scope::Repository { id: "/r".into() },
            RecordType::Convention,
            "branch naming",
            "feature/<name>",
        );
        let b = curated(
            &mut store,
            Scope::Repository { id: "/r".into() },
            RecordType::Convention,
            "branch naming",
            "<name>-wip",
        );
        let keys = chain();
        let sel = store.select(&SelectionInput {
            scope_keys: &keys,
            goal: "name the branch",
            now_ms: 2_000,
            ..Default::default()
        });
        assert_eq!(sel.selected.len(), 2, "no silent pick");
        for s in &sel.selected {
            let other = if s.item.id == a { &b } else { &a };
            assert_eq!(s.conflicts_with, vec![other.clone()]);
        }
    }

    #[test]
    fn an_edit_of_content_is_a_new_item_that_supersedes_and_a_user_edit_states_it() {
        let it = MemoryItem::propose(
            Scope::Repository { id: "/r".into() },
            RecordType::Convention,
            "indentation",
            "use tabs",
            Source::AgentObserved,
            "agent:a",
            0.5,
            1_000,
        );
        let edit = MemoryEdit {
            content: Some("use spaces".into()),
            ..Default::default()
        };
        let by_agent = it.edited(&edit, "agent:a", 2_000).unwrap();
        assert_ne!(by_agent.id, it.id);
        assert_eq!(by_agent.supersedes, vec![it.id.clone()]);
        assert!(!by_agent.validated, "an agent edit validates nothing");
        assert_eq!(by_agent.source, Source::AgentObserved);
        let by_user = it.edited(&edit, "user:u1", 2_000).unwrap();
        assert!(by_user.validated);
        assert_eq!(by_user.source, Source::UserStated);
        assert_eq!(by_user.author, "user:u1");
        assert_eq!(by_user.id, by_agent.id, "the id is content-addressed");
        // Confidence or time to live alone keeps the id: replaced in place.
        let ttl = it
            .edited(
                &MemoryEdit {
                    confidence: Some(0.9),
                    ttl_ms: Some(Some(5_000)),
                    ..Default::default()
                },
                "user:u1",
                2_000,
            )
            .unwrap();
        assert_eq!(ttl.id, it.id);
        assert_eq!(ttl.expires_at_ms, Some(7_000));
        assert!(ttl.supersedes.is_empty());
        // A retired or empty item is refused.
        let mut gone = it.clone();
        gone.status = Status::Superseded;
        assert!(matches!(
            gone.edited(&edit, "user:u1", 2_000),
            Err(EditRefusal::NotEditable { .. })
        ));
        assert_eq!(
            it.edited(
                &MemoryEdit {
                    content: Some("  ".into()),
                    ..Default::default()
                },
                "user:u1",
                2_000
            ),
            Err(EditRefusal::Empty)
        );
    }
}
