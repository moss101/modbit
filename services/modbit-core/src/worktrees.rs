//! Worktree isolation, the worktree registry and its lifecycle records
//! (PX-118, PX-065; docs/65 AFW-J05..J07, docs/20).
//!
//! A worktree is Modbit's own checkout of the user's repository, made for one
//! task under `<profile>/worktrees/<task id>`, outside the user's tree. Every
//! fact about it — it was made, from which checkout and base, what its setup
//! did, that its result was applied, merged or discarded, that it was removed
//! — is an event on the log (`WorkspaceEvent`-shaped payloads on the
//! `Workspace` aggregate named by the worktree), and the registry is the fold
//! of those events. Nothing about a worktree lives anywhere else: a Core
//! restarted from the log knows exactly which worktrees it owns, which
//! results are still unapplied, and which may be removed.
//!
//! * [`provision`] makes one (branch, `git worktree add`, the user's dirty and
//!   untracked files carried by copy under path policy) through the hardened
//!   Git runner, and reads the repository's setup-command list as data;
//! * [`run_setup`] runs that list before the first turn, only in a trusted
//!   repository, as sandboxed processes with a hard timeout, and records what
//!   happened without ever failing the task;
//! * [`registry`] folds the log into [`Record`]s; [`inspect`] adds what the
//!   filesystem and Git say now; [`eligibility`] is the one place that decides
//!   whether a worktree may be removed — the cleanup and the list view both
//!   ask it, so they cannot disagree.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::state::StateMachine;
use modbit_domain::task::Task;
use modbit_domain::{SessionId, TaskId, TenantId, Timestamp};
use modbit_event_store::{AppendRequest, EventStore, NewEvent, StoredEvent};
use modbit_git::Repo;
use modbit_protocol::v1 as wire;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::runtime::Lineage;
use crate::server::{Core, wire_id};

/// The worktree was made.
pub(crate) const PROVISIONED: &str = "WorktreeProvisioned";
/// Its setup commands ran (or were skipped, or none were declared).
pub(crate) const SETUP: &str = "WorktreeSetupRecorded";
/// Its result was applied, merged, discarded or exported.
pub(crate) const DISPOSED: &str = "WorktreeDisposed";
/// It was removed.
pub(crate) const REMOVED: &str = "WorktreeRemoved";
/// An apply-back began (write-ahead record carrying the pre-apply checkpoint).
pub(crate) const APPLY_STARTED: &str = "WorktreeApplyStarted";
/// An apply-back completed.
pub(crate) const APPLIED: &str = "WorktreeApplied";
/// An apply-back was rolled back (crash recovery).
pub(crate) const APPLY_ROLLED_BACK: &str = "WorktreeApplyRolledBack";
/// An apply-back was undone.
pub(crate) const APPLY_UNDONE: &str = "WorktreeApplyUndone";
/// The person chose a non-destructive conflict option to remember.
pub(crate) const CHOICE: &str = "WorktreeChoiceRemembered";
/// The cleanup kept a worktree because a person must decide about it.
pub(crate) const NEEDS_DECISION: &str = "WorktreeNeedsDecision";
/// A merge transaction's state.
pub(crate) const MERGE_TX: &str = "MergeTransactionRecorded";
/// A merge's post-merge verification.
pub(crate) const MERGE_VERIFIED: &str = "MergeVerificationRecorded";
/// What the hardened Git runner refused to run for a repository.
pub(crate) const NEUTRALIZED: &str = "GitProgramsNeutralized";

/// Every event type the registry folds.
pub(crate) const REGISTRY_TYPES: &[&str] = &[
    PROVISIONED,
    SETUP,
    DISPOSED,
    REMOVED,
    APPLY_STARTED,
    APPLIED,
    APPLY_ROLLED_BACK,
    APPLY_UNDONE,
    CHOICE,
    NEEDS_DECISION,
];

/// A setup command can run this long (docs/65 AFW-J05).
pub(crate) const SETUP_TIMEOUT_MS: u64 = 300_000;

/// The aggregate a worktree's records live on.
pub(crate) fn aggregate_id(key: &str) -> [u8; 16] {
    let h = Sha256::digest(format!("modbit-worktree:{key}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    b
}

/// A path as Modbit records and compares it: no Windows verbatim prefix.
pub(crate) fn plain_path(p: &Path) -> String {
    p.to_string_lossy().trim_start_matches(r"\\?\").to_owned()
}

/// A path in the one form Modbit compares: the longest existing ancestor
/// canonicalised (so `C:/x` and `C:\x`, a short 8.3 name and a verbatim `\\?\`
/// form all agree) with the not-yet-existing (or already removed) tail
/// appended. A path that is gone still resolves the same as when it existed.
pub(crate) fn resolve_path(p: &Path) -> PathBuf {
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = cur.canonicalize() {
            let mut out = PathBuf::from(plain_path(&c));
            out.extend(tail.iter().rev());
            return out;
        }
        match (cur.file_name().map(ToOwned::to_owned), cur.parent()) {
            (Some(name), Some(parent)) if !parent.as_os_str().is_empty() => {
                tail.push(name);
                cur = parent.to_path_buf();
            }
            _ => {
                return PathBuf::from(plain_path(p));
            }
        }
    }
}

/// Whether two recorded paths name the same place, compared as `Path`s.
pub(crate) fn same_path(a: &str, b: &str) -> bool {
    resolve_path(Path::new(a)) == resolve_path(Path::new(b))
}

/// A new event with its time.
pub(crate) fn ev(event_type: &str, payload: Value, actor: &Actor) -> NewEvent {
    let mut e = NewEvent::new(event_type, payload, actor.clone());
    e.occurred_at = Some(Timestamp::now());
    e
}

/// Append `events` to the aggregate `agg` under the task's lineage; returns
/// the last offset. The caller announces it (`core.last_offset`) when it has a
/// Core to announce to.
pub(crate) fn append_events(
    store: &mut EventStore,
    tenant: TenantId,
    session: SessionId,
    task: TaskId,
    agg: [u8; 16],
    events: Vec<NewEvent>,
) -> Result<u64, String> {
    let stored = store
        .append(AppendRequest {
            tenant_id: tenant,
            session_id: session,
            task_id: Some(task),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Workspace,
            aggregate_id: agg,
            expected_sequence: None,
            events,
        })
        .map_err(|e| e.to_string())?;
    Ok(stored.last().map_or(0, |e| e.offset))
}

// ---- the registry ----

/// One setup command a repository declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SetupCommand {
    pub id: String,
    pub argv: Vec<String>,
    pub timeout_ms: u64,
}

/// What the setup did.
#[derive(Clone, Debug, Default)]
pub(crate) struct SetupOutcome {
    /// `NONE` | `SKIPPED_UNTRUSTED` | `PASSED` | `FAILED` | `TIMED_OUT`.
    pub status: String,
    pub detail: String,
}

/// A decision about a worktree's result.
#[derive(Clone, Debug)]
pub(crate) struct Disposition {
    /// `APPLIED` | `MERGED` | `DISCARDED` | `EXPORTED`.
    pub kind: String,
    /// The worktree's tree the decision covered (`None` for a discard).
    pub tree: Option<String>,
    pub by: String,
    pub reason: String,
    /// The apply that made the decision (an undo of it withdraws it).
    pub apply_id: Option<String>,
}

/// One apply-back of this worktree.
#[derive(Clone, Debug)]
pub(crate) struct ApplyEntry {
    pub apply_id: String,
    pub manifest_ref: String,
    /// `STARTED` | `APPLIED` | `ROLLED_BACK` | `UNDONE`.
    pub state: String,
    pub option: String,
    pub plan_digest: String,
    pub applied_tree: String,
    pub checkout_root: String,
    pub unresolved: Vec<String>,
    pub stash_ref: Option<String>,
}

/// A worktree as the log knows it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Record {
    pub worktree_id: String,
    pub task_id: Option<TaskId>,
    pub session_id: Option<SessionId>,
    /// `TASK` | `FORK` | `SUBAGENT` | `TOOL`.
    pub kind: String,
    pub path: String,
    pub origin_root: String,
    pub branch: String,
    /// `""` for a worktree of a repository with no commit.
    pub base_revision: String,
    pub created_at_ms: i64,
    pub setup_commands: Vec<SetupCommand>,
    pub setup_error: Option<String>,
    pub setup: Option<SetupOutcome>,
    pub disposition: Option<Disposition>,
    pub removed: bool,
    pub removed_reason: String,
    pub applies: Vec<ApplyEntry>,
    pub remembered: Option<String>,
    pub needs_decision: bool,
    pub neutralized: Vec<String>,
}

impl Record {
    /// The apply that is in force: the latest APPLIED one not undone.
    pub(crate) fn applied_in_force(&self) -> Option<&ApplyEntry> {
        self.applies.iter().rev().find(|a| a.state == "APPLIED")
    }
}

fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_owned()
}

fn strings(v: &Value, k: &str) -> Vec<String> {
    v[k].as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Fold `events` (oldest first) into the registry.
pub(crate) fn fold(store: &EventStore, events: &[StoredEvent]) -> BTreeMap<String, Record> {
    let mut map: BTreeMap<String, Record> = BTreeMap::new();
    for e in events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let id = s(&p, "worktree_id");
        if id.is_empty() {
            continue;
        }
        let at = e.envelope.occurred_at.millis();
        match e.envelope.event_type.as_str() {
            PROVISIONED => {
                let r = map.entry(id.clone()).or_default();
                r.worktree_id = id;
                r.task_id = e.envelope.task_id;
                r.session_id = Some(e.envelope.session_id);
                r.kind = s(&p, "kind");
                r.path = s(&p, "path");
                r.origin_root = s(&p, "origin_root");
                r.branch = s(&p, "branch");
                r.base_revision = s(&p, "base_revision");
                r.created_at_ms = p["created_at_ms"].as_i64().unwrap_or(at);
                r.setup_commands = p["setup_commands"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|c| SetupCommand {
                                id: s(c, "id"),
                                argv: strings(c, "argv"),
                                timeout_ms: c["timeout_ms"].as_u64().unwrap_or(SETUP_TIMEOUT_MS),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                r.setup_error = p["setup_error"].as_str().map(str::to_owned);
                r.neutralized = strings(&p, "neutralized");
            }
            SETUP => {
                if let Some(r) = map.get_mut(&id) {
                    r.setup = Some(SetupOutcome {
                        status: s(&p, "status"),
                        detail: s(&p, "detail"),
                    });
                }
            }
            DISPOSED => {
                if let Some(r) = map.get_mut(&id) {
                    r.disposition = Some(Disposition {
                        kind: s(&p, "disposition"),
                        tree: p["tree"].as_str().map(str::to_owned),
                        by: s(&p, "by"),
                        reason: s(&p, "reason"),
                        apply_id: p["apply_id"].as_str().map(str::to_owned),
                    });
                }
            }
            REMOVED => {
                if let Some(r) = map.get_mut(&id) {
                    r.removed = true;
                    r.removed_reason = s(&p, "reason");
                }
            }
            APPLY_STARTED => {
                if let Some(r) = map.get_mut(&id) {
                    r.applies.push(ApplyEntry {
                        apply_id: s(&p, "apply_id"),
                        manifest_ref: s(&p, "manifest_ref"),
                        state: "STARTED".into(),
                        option: s(&p, "option"),
                        plan_digest: s(&p, "plan_digest"),
                        applied_tree: s(&p, "candidate_tree"),
                        checkout_root: s(&p, "checkout_root"),
                        unresolved: vec![],
                        stash_ref: p["stash_ref"].as_str().map(str::to_owned),
                    });
                }
            }
            APPLIED | APPLY_ROLLED_BACK | APPLY_UNDONE => {
                if let Some(r) = map.get_mut(&id) {
                    let apply_id = s(&p, "apply_id");
                    if let Some(a) = r.applies.iter_mut().rev().find(|a| a.apply_id == apply_id) {
                        a.state = match e.envelope.event_type.as_str() {
                            APPLIED => "APPLIED",
                            APPLY_ROLLED_BACK => "ROLLED_BACK",
                            _ => "UNDONE",
                        }
                        .into();
                        if e.envelope.event_type == APPLIED {
                            a.unresolved = strings(&p, "unresolved_paths");
                        }
                    }
                    // An undo withdraws the decision its apply made.
                    if e.envelope.event_type == APPLY_UNDONE
                        && r.disposition
                            .as_ref()
                            .is_some_and(|d| d.apply_id.as_deref() == Some(apply_id.as_str()))
                    {
                        r.disposition = None;
                    }
                }
            }
            CHOICE => {
                if let Some(r) = map.get_mut(&id) {
                    r.remembered = p["option"].as_str().map(str::to_owned);
                }
            }
            NEEDS_DECISION => {
                if let Some(r) = map.get_mut(&id) {
                    r.needs_decision = true;
                }
            }
            _ => {}
        }
    }
    map
}

/// Every worktree any session of this Core ever made.
pub(crate) fn registry(store: &EventStore) -> Vec<Record> {
    let events = store
        .read_all_of_types_to_end(REGISTRY_TYPES, 0)
        .unwrap_or_default();
    fold(store, &events).into_values().collect()
}

/// The worktrees one session made.
pub(crate) fn registry_of_session(store: &EventStore, session: &SessionId) -> Vec<Record> {
    let events = store
        .read_session_of_types(session, REGISTRY_TYPES, 0)
        .unwrap_or_default();
    fold(store, &events).into_values().collect()
}

/// The worktree record of `task`, when one was made for it.
pub(crate) fn record_for_task(store: &EventStore, task: &Task) -> Option<Record> {
    registry_of_session(store, &task.session_id)
        .into_iter()
        .find(|r| r.task_id == Some(task.task_id) && r.kind != "TOOL" && !r.removed)
}

/// For a root that is one of this session's worktrees, the checkout it was
/// made from. A worktree inherits the trust its checkout was given.
pub(crate) fn origin_of(store: &EventStore, session: SessionId, root: &str) -> Option<String> {
    registry_of_session(store, &session)
        .into_iter()
        .find(|r| !r.removed && same_path(&r.path, root))
        .map(|r| r.origin_root)
}

// ---- provisioning ----

/// A worktree just made, ready to be recorded.
pub(crate) struct Provisioned {
    pub worktree_id: String,
    /// Canonical path, without a verbatim prefix.
    pub path: String,
    pub branch: String,
    pub base_revision: String,
    /// The `WorktreeProvisioned` event.
    pub event: NewEvent,
}

fn refuse(code: &str, msg: impl Into<String>) -> (String, String) {
    (code.to_owned(), msg.into())
}

/// The setup list of a repository, read as data: `.modbit/worktree-setup.json`
/// of the user's checkout, at the moment the worktree is made — what a task
/// writes later is never what runs.
fn read_setup(origin: &Path) -> (Vec<Value>, Option<String>) {
    let p = origin.join(".modbit").join("worktree-setup.json");
    let bytes = match std::fs::read(&p) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (vec![], None),
        Err(e) => return (vec![], Some(format!("{}: {e}", p.display()))),
    };
    if bytes.len() > 64 * 1024 {
        return (vec![], Some(format!("{} is over 64 KiB", p.display())));
    }
    let v: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => return (vec![], Some(format!("{}: {e}", p.display()))),
    };
    let mut out = Vec::new();
    for (i, c) in v["commands"].as_array().into_iter().flatten().enumerate() {
        let argv: Vec<String> = c["argv"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if argv.is_empty() || out.len() >= 16 {
            return (
                vec![],
                Some(format!(
                    "{}: command {i} needs a non-empty argv, and at most 16 commands are read",
                    p.display()
                )),
            );
        }
        let id = c["id"]
            .as_str()
            .map_or_else(|| format!("setup-{i}"), str::to_owned);
        let timeout = c["timeout_ms"]
            .as_u64()
            .filter(|t| *t > 0)
            .map_or(SETUP_TIMEOUT_MS, |t| t.min(SETUP_TIMEOUT_MS));
        out.push(json!({"id": id, "argv": argv, "timeout_ms": timeout}));
    }
    (out, None)
}

/// What was carried (paths) and what was not (path, why).
type Carried = (Vec<String>, Vec<(String, String)>);

/// Copy the user's uncommitted and untracked files into the new worktree (by
/// copy, never a stash), skipping what path policy protects. Returns what
/// was carried and what was not, with why.
fn carry_dirty(origin: &Repo, wt: &Repo) -> Result<Carried, (String, String)> {
    const FILE_CAP: u64 = 64 * 1024 * 1024;
    const TOTAL_CAP: u64 = 512 * 1024 * 1024;
    let policy = modbit_workspace::PathPolicy::new(origin.dir(), &[])
        .map_err(|e| refuse("ISOLATION_WORKTREE_FAILED", e.to_string()))?;
    let mut carried = Vec::new();
    let mut skipped = Vec::new();
    let mut total = 0u64;
    let status = origin
        .status()
        .map_err(|e| refuse("ISOLATION_WORKTREE_FAILED", e.to_string()))?;
    for e in status {
        let rel = e.path;
        if rel.ends_with('/') {
            skipped.push((rel, "a directory (an embedded repository)".to_owned()));
            continue;
        }
        if policy.protected_match(&rel).is_some() {
            skipped.push((rel, "protected path".to_owned()));
            continue;
        }
        let (src, dst) = (origin.dir().join(&rel), wt.dir().join(&rel));
        match std::fs::symlink_metadata(&src) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                // Deleted in the user's tree: deleted in the worktree.
                if std::fs::remove_file(&dst).is_ok() {
                    carried.push(rel);
                }
            }
            Err(err) => skipped.push((rel, err.to_string())),
            Ok(m) if m.file_type().is_symlink() => {
                skipped.push((rel, "a symbolic link".to_owned()));
            }
            Ok(m) if m.is_file() => {
                if m.len() > FILE_CAP || total + m.len() > TOTAL_CAP {
                    skipped.push((rel, format!("{} bytes: over the carry limit", m.len())));
                    continue;
                }
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|er| refuse("ISOLATION_WORKTREE_FAILED", er.to_string()))?;
                }
                std::fs::copy(&src, &dst)
                    .map_err(|er| refuse("ISOLATION_WORKTREE_FAILED", format!("{rel}: {er}")))?;
                total += m.len();
                carried.push(rel);
            }
            Ok(_) => skipped.push((rel, "not a regular file".to_owned())),
        }
    }
    Ok((carried, skipped))
}

/// Make `task`'s worktree (blocking: Git and the filesystem). Typed refusals:
/// `ISOLATION_NO_WORKSPACE`, `ISOLATION_NOT_A_REPOSITORY`,
/// `ISOLATION_UNSAFE_REPOSITORY`, `ISOLATION_ROOT_UNWRITABLE`,
/// `ISOLATION_BRANCH_IN_USE`, `ISOLATION_PATH_COLLISION`,
/// `ISOLATION_WORKTREE_FAILED`. A refusal leaves nothing behind.
pub(crate) fn provision(
    data_dir: &Path,
    task_id: TaskId,
    origin_root: &str,
    kind: &str,
) -> Result<Provisioned, (String, String)> {
    if origin_root.trim().is_empty() {
        return Err(refuse(
            "ISOLATION_NO_WORKSPACE",
            "worktree isolation needs the workspace_root of a repository to make the worktree from",
        ));
    }
    let origin_dir = Path::new(origin_root);
    if !origin_dir.is_dir() {
        return Err(refuse(
            "ISOLATION_NO_WORKSPACE",
            format!("`{origin_root}` is not a directory on this machine"),
        ));
    }
    let repo = Repo::open(origin_dir).map_err(|e| match e {
        modbit_git::Error::UnsafeRepository(_) => {
            refuse("ISOLATION_UNSAFE_REPOSITORY", e.to_string())
        }
        _ => refuse(
            "ISOLATION_NOT_A_REPOSITORY",
            format!("`{origin_root}` is not a Git repository: worktree isolation needs one"),
        ),
    })?;
    // The Core-managed root. It must exist and take a write now: a task is
    // never started on the strength of a directory it cannot create.
    let root = data_dir.join("worktrees");
    let unwritable = |why: String| {
        refuse(
            "ISOLATION_ROOT_UNWRITABLE",
            format!(
                "the worktree root `{}` cannot be written: {why}; nothing was written to the checkout",
                root.display()
            ),
        )
    };
    std::fs::create_dir_all(&root).map_err(|e| unwritable(e.to_string()))?;
    let probe = root.join(format!(".probe-{}", std::process::id()));
    std::fs::write(&probe, b"").map_err(|e| unwritable(e.to_string()))?;
    let _ = std::fs::remove_file(&probe);
    // Stale entries (a directory removed behind Git's back) free their
    // branches.
    let _ = repo.worktree_prune();

    let id = task_id.to_string();
    let branch = format!("modbit/task-{id}");
    let mut path = None;
    for i in 0..10 {
        let candidate = root.join(if i == 0 {
            id.clone()
        } else {
            format!("{id}-{i}")
        });
        if !candidate.exists() {
            path = Some(candidate);
            break;
        }
    }
    let path = path.ok_or_else(|| {
        refuse(
            "ISOLATION_PATH_COLLISION",
            format!("ten directories named for task {id} already exist under the worktree root"),
        )
    })?;
    let unborn = repo.is_unborn();
    let base_revision = if unborn {
        String::new()
    } else {
        repo.head()
            .map_err(|e| refuse("ISOLATION_WORKTREE_FAILED", e.to_string()))?
    };
    let in_use = |e: modbit_git::Error| match e {
        modbit_git::Error::BranchInUse { branch, path } => refuse(
            "ISOLATION_BRANCH_IN_USE",
            format!("branch `{branch}` is already checked out at `{path}`"),
        ),
        other => refuse("ISOLATION_WORKTREE_FAILED", other.to_string()),
    };
    let wt = if unborn {
        repo.worktree_add_orphan(&path, &branch).map_err(in_use)?
    } else {
        // The branch is in Modbit's namespace and named for this task: if it
        // exists, a replay or a crash left it, and it is reset to the base
        // unless some worktree holds it.
        if repo.ref_exists(&format!("refs/heads/{branch}")) {
            let held = repo
                .worktree_list()
                .map_err(|e| refuse("ISOLATION_WORKTREE_FAILED", e.to_string()))?
                .into_iter()
                .find(|w| w.branch.as_deref() == Some(branch.as_str()));
            if let Some(w) = held {
                return Err(refuse(
                    "ISOLATION_BRANCH_IN_USE",
                    format!(
                        "branch `{branch}` is already checked out at `{}`",
                        w.path.display()
                    ),
                ));
            }
            repo.set_branch(&branch, &base_revision)
                .map_err(|e| refuse("ISOLATION_WORKTREE_FAILED", e.to_string()))?;
        } else {
            repo.create_branch(&branch, &base_revision)
                .map_err(|e| refuse("ISOLATION_WORKTREE_FAILED", e.to_string()))?;
        }
        match repo.worktree_add(&path, &branch) {
            Ok(w) => w,
            Err(e) => {
                let _ = repo.branch_delete(&branch);
                return Err(in_use(e));
            }
        }
    };
    let undo = |repo: &Repo| {
        let _ = repo.worktree_remove(&path);
        let _ = repo.branch_delete(&branch);
        let _ = std::fs::remove_dir_all(&path);
    };
    let (carried, skipped) = match carry_dirty(&repo, &wt) {
        Ok(c) => c,
        Err(e) => {
            undo(&repo);
            return Err(e);
        }
    };
    let canonical = plain_path(
        &wt.dir()
            .canonicalize()
            .unwrap_or_else(|_| wt.dir().to_path_buf()),
    );
    let neutralized: Vec<String> = repo
        .neutralized()
        .unwrap_or_default()
        .into_iter()
        .map(|n| n.kind)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let (setup_commands, setup_error) = read_setup(origin_dir);
    let origin_canonical = plain_path(
        &origin_dir
            .canonicalize()
            .unwrap_or_else(|_| origin_dir.to_path_buf()),
    );
    let event = ev(
        PROVISIONED,
        json!({
            "worktree_id": id,
            "task_id": id,
            "kind": kind,
            "path": canonical,
            "origin_root": origin_canonical,
            "branch": branch,
            "base_revision": base_revision,
            "created_at_ms": Timestamp::now().millis(),
            "branch_policy": if unborn { "ORPHAN" } else { "NEW_BRANCH" },
            "carried": carried,
            "skipped": skipped.iter().map(|(p, why)| json!({"path": p, "why": why})).collect::<Vec<_>>(),
            "setup_commands": setup_commands,
            "setup_error": setup_error,
            "neutralized": neutralized,
        }),
        &Actor::Core("worktrees".into()),
    );
    Ok(Provisioned {
        worktree_id: id,
        path: canonical,
        branch,
        base_revision,
        event,
    })
}

/// Remove a worktree made by [`provision`] whose task never got created.
pub(crate) fn unprovision(origin_root: &str, p: &Provisioned) {
    if let Ok(repo) = Repo::open(Path::new(origin_root)) {
        let _ = repo.worktree_remove(Path::new(&p.path));
        let _ = repo.branch_delete(&p.branch);
    }
    let _ = std::fs::remove_dir_all(&p.path);
}

/// Record a worktree a fork, a subagent or a tool made (kinds `FORK`,
/// `SUBAGENT`, `TOOL`), so the registry — and with it the cleanup — knows it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn provisioned_event(
    kind: &str,
    worktree_id: &str,
    task_id: TaskId,
    path: &str,
    origin_root: &str,
    branch: &str,
    base_revision: &str,
    actor: &Actor,
) -> NewEvent {
    ev(
        PROVISIONED,
        json!({
            "worktree_id": worktree_id,
            "task_id": task_id.to_string(),
            "kind": kind,
            "path": path,
            "origin_root": origin_root,
            "branch": branch,
            "base_revision": base_revision,
            "created_at_ms": Timestamp::now().millis(),
            "branch_policy": "NEW_BRANCH",
            "carried": [],
            "skipped": [],
            "setup_commands": [],
            "neutralized": [],
        }),
        actor,
    )
}

/// The event that records a worktree's removal.
pub(crate) fn removed_event(
    worktree_id: &str,
    why: &str,
    bytes: u64,
    by: &str,
    actor: &Actor,
) -> NewEvent {
    ev(
        REMOVED,
        json!({"worktree_id": worktree_id, "reason": why, "bytes": bytes, "by": by}),
        actor,
    )
}

// ---- security event: what the hardened runner refused ----

/// Record, once per distinct finding, what the repository at `root` asked Git
/// to run that the hardened runner refused (PX-067): a hook, `core.fsmonitor`,
/// a filter, a merge driver, an ssh command, a credential helper… on the
/// task's log. Returns how many items the finding holds.
pub(crate) async fn record_neutralized(
    store: &Arc<Mutex<EventStore>>,
    tenant: TenantId,
    session: SessionId,
    task: TaskId,
    root: &str,
    actor: &Actor,
) -> usize {
    let root_owned = root.to_owned();
    let found = tokio::task::spawn_blocking(move || {
        Repo::open(Path::new(&root_owned)).and_then(|r| r.neutralized())
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    if found.is_empty() {
        return 0;
    }
    let items: Vec<Value> = found
        .iter()
        .map(|n| json!({"kind": n.kind, "name": n.name, "scope": n.scope}))
        .collect();
    let digest = hex::encode(Sha256::digest(
        serde_json::to_vec(&items).unwrap_or_default(),
    ));
    let agg = aggregate_id(&format!("security:{task}"));
    let mut st = store.lock().await;
    // Already recorded for this finding?
    let already = st
        .read_aggregate(&agg, 0, usize::MAX)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.envelope.event_type == NEUTRALIZED)
        .filter_map(|e| st.payload(&e.envelope).ok())
        .any(|p| p["digest"].as_str() == Some(digest.as_str()) && p["root"].as_str() == Some(root));
    if already {
        return found.len();
    }
    let _ = append_events(
        &mut st,
        tenant,
        session,
        task,
        agg,
        vec![ev(
            NEUTRALIZED,
            json!({
                "root": root,
                "digest": digest,
                "items": items,
                "note": "the hardened Git runner never ran these; the repository is data, not authority",
            }),
            actor,
        )],
    );
    found.len()
}

// ---- setup ----

/// Run the setup commands of `task`'s worktree before its first turn
/// (docs/65 AFW-J05): only in a trusted repository, each as a sandboxed
/// process under the Capability Kernel with a hard timeout (300 s), its
/// output kept as an OutputRef, stopping at the first failure. A failure is
/// recorded and visible on the task; the task goes on. Idempotent: a worktree
/// whose setup is on the log is left alone.
pub(crate) async fn run_setup(core: &Arc<Core>, task: &Task, lt: Lineage, actor: &Actor) {
    let rec = {
        let store = core.store.lock().await;
        let Some(rec) = record_for_task(&store, task) else {
            return;
        };
        if rec.kind != "TASK" || rec.setup.is_some() {
            return;
        }
        rec
    };
    let mut commands_out: Vec<Value> = Vec::new();
    let (status, detail) = if let Some(err) = &rec.setup_error {
        ("FAILED", format!("the setup list is unreadable: {err}"))
    } else if rec.setup_commands.is_empty() {
        ("NONE", String::new())
    } else if !crate::onboarding::is_trusted(
        &*core.store.lock().await,
        task.session_id,
        &rec.origin_root,
    ) {
        (
            "SKIPPED_UNTRUSTED",
            format!(
                "{} setup command(s) declared by `{}` did not run: the repository is not trusted in this session (TrustRepository)",
                rec.setup_commands.len(),
                rec.origin_root
            ),
        )
    } else {
        let runner = crate::verify::BrokerRunner {
            target: core.tools.execd.as_ref().map(|e| e.target.clone()),
            execution_profile: task.execution_profile.clone(),
            cancel: core.runtime.cancel_token(&task.task_id).await,
            kernel: {
                let store = core.store.lock().await;
                Some(crate::verify::KernelGate {
                    lease: store
                        .leases_for_task(&task.task_id)
                        .ok()
                        .and_then(|l| l.into_iter().next()),
                    execution_profile: task.execution_profile.clone(),
                    mode: core
                        .tools
                        .tasking
                        .mode_in_force(&store, task.task_id)
                        .unwrap_or(modbit_domain::mode::TaskMode::Ask),
                    emergency_stopped: store
                        .session(&task.session_id)
                        .ok()
                        .flatten()
                        .and_then(|s| s.emergency_stopped_at)
                        .is_some(),
                    config: core.tools.configurations.for_task(
                        task.task_id,
                        &core.data_dir,
                        Some(rec.path.as_str()),
                    ),
                })
            },
        };
        let hard_cap = std::env::var("MODBIT_WORKTREE_SETUP_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok());
        let mut outcome = ("PASSED", String::new());
        for c in &rec.setup_commands {
            use modbit_verification::CommandRunner;
            if outcome.0 != "PASSED" {
                commands_out.push(json!({"id": c.id, "argv": c.argv, "skipped": true}));
                continue;
            }
            let started = std::time::Instant::now();
            if let Err(why) = runner.authorize(&c.id, &c.argv).await {
                commands_out.push(json!({"id": c.id, "argv": c.argv, "refused": why}));
                outcome = ("FAILED", format!("`{}` was refused: {why}", c.id));
                continue;
            }
            let timeout = hard_cap.map_or(c.timeout_ms, |h| h.min(c.timeout_ms));
            let raw = runner
                .run(&c.argv, Path::new(&rec.path), &[], timeout)
                .await;
            let output = {
                let mut bytes = raw.stdout.clone().into_bytes();
                bytes.extend_from_slice(raw.stderr.as_bytes());
                core.store
                    .lock()
                    .await
                    .objects()
                    .put(&bytes)
                    .unwrap_or_default()
            };
            commands_out.push(json!({
                "id": c.id,
                "argv": c.argv,
                "exit_code": raw.exit_code,
                "timed_out": raw.timed_out,
                "cancelled": raw.cancelled,
                "duration_ms": started.elapsed().as_millis() as u64,
                "output_ref": output,
            }));
            if raw.timed_out {
                outcome = (
                    "TIMED_OUT",
                    format!("`{}` was stopped after {} ms", c.id, timeout),
                );
            } else if raw.exit_code != Some(0) {
                outcome = ("FAILED", format!("`{}` exited {:?}", c.id, raw.exit_code));
            }
        }
        outcome
    };
    let mut store = core.store.lock().await;
    let _ = append_events(
        &mut store,
        core.tenant_id,
        task.session_id,
        task.task_id,
        aggregate_id(&rec.worktree_id),
        vec![ev(
            SETUP,
            json!({
                "worktree_id": rec.worktree_id,
                "status": status,
                "detail": detail,
                "commands": commands_out,
            }),
            actor,
        )],
    );
    let _ = lt;
    let offset = store.last_offset().unwrap_or(0);
    drop(store);
    core.last_offset.send_replace(offset);
}

// ---- what the filesystem and Git say now ----

/// A worktree's present state.
#[derive(Clone, Debug, Default)]
pub(crate) struct Info {
    pub exists: bool,
    pub is_git_worktree: bool,
    pub bytes: u64,
    pub dirty: bool,
    pub changed_files: u32,
    /// It holds work beyond its base: uncommitted files or commits.
    pub has_changes: bool,
    /// The tree of its whole state now.
    pub tree: Option<String>,
    pub error: Option<String>,
}

fn dir_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.file_type().is_symlink() {
                continue;
            }
            if m.is_dir() {
                stack.push(e.path());
            } else {
                total += m.len();
            }
        }
    }
    total
}

/// Inspect the directory at `path` (blocking: Git and a tree walk).
/// `base_revision` is the commit the worktree was made from (`Some("")`: it
/// had none, so any commit is its own); `None` is a worktree the log does not
/// know, whose base is whatever its checkout's `HEAD` already holds.
pub(crate) fn inspect(path: &Path, base_revision: Option<&str>) -> Info {
    let mut info = Info::default();
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return info;
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        info.error = Some("not a real directory".into());
        return info;
    }
    info.exists = true;
    info.bytes = dir_bytes(path);
    if !path.join(".git").exists() {
        return info;
    }
    let repo = match Repo::open(path) {
        Ok(r) => r,
        Err(e) => {
            info.error = Some(e.to_string());
            return info;
        }
    };
    info.is_git_worktree = true;
    match repo.status() {
        Ok(st) => {
            info.dirty = !st.is_empty();
            info.changed_files = st.len() as u32;
        }
        Err(e) => info.error = Some(e.to_string()),
    }
    let commits = match base_revision {
        Some("") => u64::from(!repo.is_unborn()),
        Some(base) => repo.commits_between(base, "HEAD").unwrap_or(0),
        None => repo
            .common_checkout()
            .and_then(|main| Repo::open(&main).ok())
            .and_then(|main| main.head().ok())
            // Commits reachable from the checkout's HEAD are nobody's work in
            // particular; any other commit is the worktree's own.
            .map_or_else(
                || u64::from(!repo.is_unborn()),
                |head| repo.commits_between(&head, "HEAD").unwrap_or(1),
            ),
    };
    info.has_changes = info.dirty || commits > 0;
    info.tree = repo.dirty_tree().ok();
    info
}

/// The retention policy (docs/65 AFW-J07).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Policy {
    pub max_worktrees: u32,
    pub max_bytes: u64,
    pub protect_ms: i64,
    pub hysteresis_percent: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            max_worktrees: 25,
            max_bytes: 50 * 1024 * 1024 * 1024,
            protect_ms: 10 * 60 * 1000,
            hysteresis_percent: 80,
        }
    }
}

impl Policy {
    /// The policy in force: the defaults, with the environment's overrides
    /// (`MODBIT_WORKTREE_MAX`, `MODBIT_WORKTREE_MAX_BYTES`,
    /// `MODBIT_WORKTREE_PROTECT_MS`) and a request's non-zero fields on top.
    pub(crate) fn resolve(request: Option<&wire::WorktreePolicy>) -> Self {
        let mut p = Self::default();
        let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<u64>().ok());
        if let Some(v) = env("MODBIT_WORKTREE_MAX") {
            p.max_worktrees = v as u32;
        }
        if let Some(v) = env("MODBIT_WORKTREE_MAX_BYTES") {
            p.max_bytes = v;
        }
        if let Some(v) = env("MODBIT_WORKTREE_PROTECT_MS") {
            p.protect_ms = v as i64;
        }
        if let Some(r) = request {
            if r.max_worktrees > 0 {
                p.max_worktrees = r.max_worktrees;
            }
            if r.max_bytes > 0 {
                p.max_bytes = r.max_bytes;
            }
            if r.protect_ms > 0 {
                p.protect_ms = r.protect_ms;
            }
            if r.hysteresis_percent > 0 {
                p.hysteresis_percent = r.hysteresis_percent.min(100);
            }
        }
        p
    }

    pub(crate) fn to_wire(self) -> wire::WorktreePolicy {
        wire::WorktreePolicy {
            max_worktrees: self.max_worktrees,
            max_bytes: self.max_bytes,
            protect_ms: self.protect_ms,
            hysteresis_percent: self.hysteresis_percent,
        }
    }
}

/// Why a worktree may or may not be removed.
#[derive(Clone, Debug)]
pub(crate) struct Eligibility {
    /// A running task, or younger than the protection window.
    pub protected: bool,
    /// Removal would lose nothing a person has not already dealt with.
    pub removable: bool,
    pub reason: String,
    /// It holds a result a person must decide about.
    pub needs_decision: bool,
}

/// Whether a decision covers what the worktree holds now: a decision that
/// names the tree it covered covers exactly that tree; one that names none
/// (a merge of a child that had nothing) covers what there was.
fn covered(d: &Disposition, info: &Info) -> bool {
    match &d.tree {
        Some(t) => info.tree.as_ref() == Some(t),
        None => true,
    }
}

/// Whether the worktree holds a result no decision covers: changes beyond its
/// base with no disposition, or changes made after the one it has.
pub(crate) fn unapplied_of(rec: Option<&Record>, info: &Info) -> bool {
    if !info.exists || !info.is_git_worktree {
        return false;
    }
    match rec.and_then(|r| r.disposition.as_ref()) {
        Some(d) if d.kind == "DISCARDED" => false,
        Some(d) => !covered(d, info),
        None => info.has_changes,
    }
}

/// The one rule. A worktree is removable only when its task is over, it is
/// past the protection window, and it holds nothing unapplied: no changes at
/// all, or its result was applied, merged or exported and has not changed
/// since, or it was explicitly discarded. A dirty or unapplied worktree is
/// never removable — it needs a decision.
pub(crate) fn eligibility(
    rec: Option<&Record>,
    task: Option<&Task>,
    live: bool,
    info: &Info,
    now_ms: i64,
    created_ms: i64,
    policy: &Policy,
) -> Eligibility {
    let no = |reason: &str, protected: bool, needs: bool| Eligibility {
        protected,
        removable: false,
        reason: reason.to_owned(),
        needs_decision: needs,
    };
    if !info.exists {
        return Eligibility {
            protected: false,
            removable: false,
            reason: "its directory is gone".into(),
            needs_decision: false,
        };
    }
    if let Some(t) = task
        && (!t.state.is_terminal() || live)
    {
        return no("its task has not ended", true, false);
    }
    if rec.is_some() && task.is_none() {
        return no("its task is not on this Core's log", false, true);
    }
    if now_ms.saturating_sub(created_ms) < policy.protect_ms {
        return no("younger than the protection window", true, false);
    }
    if !info.is_git_worktree {
        return no("not a Git worktree Modbit can inspect", false, true);
    }
    if let Some(e) = &info.error {
        return no(&format!("it could not be inspected: {e}"), false, true);
    }
    let disposition = rec.and_then(|r| r.disposition.as_ref());
    match disposition {
        Some(d) if d.kind == "DISCARDED" => Eligibility {
            protected: false,
            removable: true,
            reason: "its result was discarded".into(),
            needs_decision: false,
        },
        Some(d) => {
            if covered(d, info) {
                Eligibility {
                    protected: false,
                    removable: true,
                    reason: format!(
                        "its result was {} ({}) and has not changed since",
                        d.kind.to_lowercase(),
                        d.reason
                    ),
                    needs_decision: false,
                }
            } else {
                no(
                    &format!(
                        "it changed after its result was {} by {}",
                        d.kind.to_lowercase(),
                        d.by
                    ),
                    false,
                    true,
                )
            }
        }
        None if !info.has_changes => Eligibility {
            protected: false,
            removable: true,
            reason: "it holds nothing beyond its base".into(),
            needs_decision: false,
        },
        None => no(
            if info.dirty {
                "it is dirty and its result is not applied, merged or discarded"
            } else {
                "it holds commits that are not applied, merged or discarded"
            },
            false,
            true,
        ),
    }
}

/// A worktree joined with the task that owns it and what the disk says now.
#[derive(Clone)]
pub(crate) struct Entry {
    pub rec: Option<Record>,
    pub task: Option<Task>,
    pub live: bool,
    pub path: PathBuf,
    pub info: Info,
    pub created_ms: i64,
    pub last_activity_ms: i64,
    pub orphan: bool,
    pub eligibility: Eligibility,
}

/// The managed directories under `<data>/worktrees` that no live record names.
fn orphan_dirs(data_dir: &Path, known: &[PathBuf]) -> Vec<PathBuf> {
    let root = data_dir.join("worktrees");
    let Ok(rd) = std::fs::read_dir(&root) else {
        return vec![];
    };
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let known: Vec<PathBuf> = known.iter().map(|p| canon(p)).collect();
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| {
            // A symbolic link in the managed root is neither adopted nor
            // followed: it is not a directory Modbit made.
            std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
                && !known.contains(&canon(p))
        })
        .collect()
}

/// Everything the Core knows about its worktrees, with the disk inspected
/// (blocking work runs off the async threads).
pub(crate) async fn entries(core: &Arc<Core>, policy: &Policy, now_ms: i64) -> Vec<Entry> {
    let (records, tasks) = {
        let store = core.store.lock().await;
        let records = registry(&store);
        let mut tasks: HashMap<TaskId, Task> = HashMap::new();
        for r in &records {
            if let Some(t) = r.task_id
                && let Ok(Some(task)) = store.task(&t)
            {
                tasks.insert(t, task);
            }
        }
        (records, tasks)
    };
    let mut live: HashMap<TaskId, bool> = HashMap::new();
    for r in &records {
        if let Some(t) = r.task_id {
            live.insert(t, core.runtime.is_running(&t).await);
        }
    }
    let data_dir = core.data_dir.clone();
    let policy = *policy;
    tokio::task::spawn_blocking(move || {
        let active: Vec<Record> = records.iter().filter(|r| !r.removed).cloned().collect();
        let known: Vec<PathBuf> = active.iter().map(|r| PathBuf::from(&r.path)).collect();
        let mut out = Vec::new();
        for rec in active {
            let path = PathBuf::from(&rec.path);
            let info = inspect(&path, Some(&rec.base_revision));
            let task = rec.task_id.and_then(|t| tasks.get(&t).cloned());
            let is_live = rec
                .task_id
                .is_some_and(|t| live.get(&t).copied().unwrap_or(false));
            let activity = task.as_ref().map_or(rec.created_at_ms, |t| {
                [
                    Some(rec.created_at_ms),
                    t.started_at.map(Timestamp::millis),
                    t.completed_at.map(Timestamp::millis),
                ]
                .into_iter()
                .flatten()
                .max()
                .unwrap_or(rec.created_at_ms)
            });
            let elig = eligibility(
                Some(&rec),
                task.as_ref(),
                is_live,
                &info,
                now_ms,
                rec.created_at_ms,
                &policy,
            );
            out.push(Entry {
                created_ms: rec.created_at_ms,
                last_activity_ms: activity,
                rec: Some(rec),
                task,
                live: is_live,
                path,
                info,
                orphan: false,
                eligibility: elig,
            });
        }
        for path in orphan_dirs(&data_dir, &known) {
            let info = inspect(&path, None);
            let created = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(now_ms, |d| d.as_millis() as i64);
            // An orphan has no recorded base: it is judged on its dirt and
            // its commits beyond any ancestor it shares with HEAD of nothing,
            // so only a worktree that is clean *and* has no commit of its own
            // is removable; anything else needs a person.
            let mut elig = eligibility(None, None, false, &info, now_ms, created, &policy);
            if info.is_git_worktree && info.has_changes {
                elig.removable = false;
                elig.needs_decision = true;
            }
            out.push(Entry {
                rec: None,
                task: None,
                live: false,
                path,
                info,
                created_ms: created,
                last_activity_ms: created,
                orphan: true,
                eligibility: elig,
            });
        }
        out
    })
    .await
    .unwrap_or_default()
}

fn view_of(e: &Entry) -> wire::WorktreeView {
    let r = e.rec.as_ref();
    wire::WorktreeView {
        worktree_id: r.map_or_else(
            || {
                e.path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            },
            |r| r.worktree_id.clone(),
        ),
        task_id: r.and_then(|r| r.task_id).map(|t| wire_id(t.as_bytes())),
        kind: r.map_or_else(String::new, |r| r.kind.clone()),
        path: plain_path(&e.path),
        origin_root: r.map_or_else(String::new, |r| r.origin_root.clone()),
        branch: r.map_or_else(String::new, |r| r.branch.clone()),
        base_revision: r.map_or_else(String::new, |r| r.base_revision.clone()),
        state: if e.info.exists { "ACTIVE" } else { "MISSING" }.into(),
        disposition: r
            .and_then(|r| r.disposition.as_ref())
            .map_or_else(String::new, |d| d.kind.clone()),
        dirty: e.info.dirty,
        unapplied: unapplied_of(e.rec.as_ref(), &e.info),
        changed_files: e.info.changed_files,
        bytes: e.info.bytes,
        created_at_ms: e.created_ms,
        last_activity_ms: e.last_activity_ms,
        task_running: e.task.as_ref().is_some_and(|t| !t.state.is_terminal()) || e.live,
        orphan: e.orphan,
        protected: e.eligibility.protected,
        removable: e.eligibility.removable,
        removable_reason: e.eligibility.reason.clone(),
        setup_status: r
            .and_then(|r| r.setup.as_ref())
            .map_or_else(String::new, |s| s.status.clone()),
        setup_detail: r
            .and_then(|r| r.setup.as_ref())
            .map_or_else(String::new, |s| s.detail.clone()),
        neutralized: r.map(|r| r.neutralized.clone()).unwrap_or_default(),
    }
}

/// The attention lines for the worktrees a person must decide about.
pub(crate) fn attention_lines(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .filter(|e| e.eligibility.needs_decision)
        .map(|e| {
            format!(
                "worktree {} ({}): {}; ApplyWorktree or DiscardWorktree decides it",
                e.rec
                    .as_ref()
                    .map_or_else(|| plain_path(&e.path), |r| r.worktree_id.clone()),
                e.rec.as_ref().map_or("orphan", |r| r.kind.as_str()),
                e.eligibility.reason
            )
        })
        .collect()
}

/// `ListWorktrees`.
pub(crate) async fn list(core: &Arc<Core>, p: &wire::ListWorktrees) -> wire::WorktreeList {
    let policy = Policy::resolve(None);
    let now = Timestamp::now().millis();
    let mut all = entries(core, &policy, now).await;
    if let Some(t) = p.task_id.as_ref().and_then(crate::server::id16) {
        let t = TaskId::from_bytes(t);
        all.retain(|e| e.rec.as_ref().and_then(|r| r.task_id) == Some(t));
    }
    if let Some(s) = p.session_id.as_ref().and_then(crate::server::id16) {
        let s = SessionId::from_bytes(s);
        all.retain(|e| e.rec.as_ref().and_then(|r| r.session_id) == Some(s));
    }
    all.sort_by_key(|e| e.created_ms);
    let attention = attention_lines(&all);
    let active: Vec<&Entry> = all.iter().filter(|e| e.info.exists).collect();
    let state = crate::worktree_cleanup::load_state(&core.data_dir);
    wire::WorktreeList {
        count: active.len() as u32,
        total_bytes: active.iter().map(|e| e.info.bytes).sum(),
        worktrees: all.iter().map(view_of).collect(),
        policy: Some(policy.to_wire()),
        last_cleanup: state.last_run.clone(),
        attention,
        next_cleanup_at_ms: core.worktrees.next_cleanup_at_ms(),
        last_completed_at_ms: state.last_completed_at_ms,
    }
}

/// The session of the task a worktree command names.
pub(crate) async fn session_of_task(
    core: &Core,
    task: Option<&wire::Id>,
) -> Result<SessionId, (String, String)> {
    let Some(tid) = task.and_then(crate::server::id16) else {
        return Err(refuse("BAD_PAYLOAD", "task_id required"));
    };
    let task_id = TaskId::from_bytes(tid);
    match core.store.lock().await.task(&task_id) {
        Ok(Some(t)) => Ok(t.session_id),
        Ok(None) => Err(refuse("UNKNOWN_TASK", task_id.to_string())),
        Err(e) => Err(refuse("STORE", e.to_string())),
    }
}

/// `DiscardWorktree`: record the decision to drop a worktree's result. Not a
/// deletion — the worktree becomes removable and the cleanup removes it.
pub(crate) async fn discard(
    core: &Arc<Core>,
    p: &wire::DiscardWorktree,
    actor: &Actor,
) -> Result<wire::WorktreeDiscarded, (String, String)> {
    let Some(task_id) = p.task_id.as_ref().and_then(crate::server::id16) else {
        return Err(refuse("BAD_PAYLOAD", "task_id required"));
    };
    let task_id = TaskId::from_bytes(task_id);
    let mut store = core.store.lock().await;
    let task = store
        .task(&task_id)
        .ok()
        .flatten()
        .ok_or_else(|| refuse("UNKNOWN_TASK", task_id.to_string()))?;
    let rec = record_for_task(&store, &task)
        .ok_or_else(|| refuse("NO_WORKTREE", "the task has no worktree of its own"))?;
    if p.confirm != rec.branch {
        return Err(refuse(
            "DISCARD_NOT_CONFIRMED",
            format!(
                "discarding drops the worktree's result for good: type its branch name `{}` to confirm",
                rec.branch
            ),
        ));
    }
    if !task.state.is_terminal() && core.runtime.is_running(&task_id).await {
        return Err(refuse(
            "TASK_RUNNING",
            "the task is still running; cancel it before discarding its worktree",
        ));
    }
    let offset = append_events(
        &mut store,
        core.tenant_id,
        task.session_id,
        task.task_id,
        aggregate_id(&rec.worktree_id),
        vec![ev(
            DISPOSED,
            json!({
                "worktree_id": rec.worktree_id,
                "disposition": "DISCARDED",
                "by": format!("{actor:?}"),
                "reason": p.reason,
            }),
            actor,
        )],
    )
    .map_err(|e| refuse("STORE", e))?;
    core.last_offset.send_replace(offset);
    Ok(wire::WorktreeDiscarded {
        worktree_id: rec.worktree_id,
        disposition: "DISCARDED".into(),
        offset,
    })
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn a_removed_directory_resolves_as_it_did_while_it_existed() {
        let dir = tempfile::tempdir().unwrap();
        let wt = dir.path().join("a").join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        let before = resolve_path(&wt);
        std::fs::remove_dir_all(&wt).unwrap();
        assert_eq!(resolve_path(&wt), before);
        assert!(same_path(&wt.to_string_lossy(), &plain_path(&before)));
        // Another directory is another place.
        assert!(!same_path(
            &wt.to_string_lossy(),
            &dir.path().join("a").join("other").to_string_lossy()
        ));
    }

    #[test]
    fn a_dotted_path_resolves_to_the_place_it_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        let dotted = dir.path().join("a").join("..").join("a").join("gone");
        assert!(same_path(
            &dotted.to_string_lossy(),
            &dir.path().join("a").join("gone").to_string_lossy()
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_spellings_of_one_directory_agree() {
        let dir = tempfile::tempdir().unwrap();
        let wt = dir.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        let plain = plain_path(&wt.canonicalize().unwrap());
        let forward = plain.replace('\\', "/");
        let verbatim = format!(r"\\?\{plain}");
        std::fs::remove_dir_all(&wt).unwrap();
        assert!(same_path(&plain, &forward));
        assert!(same_path(&plain, &verbatim));
        assert!(same_path(&forward, &verbatim));
    }
}
