//! Scheduled cleanup of worktrees (PX-065; docs/65 AFW-J06, AFW-J07).
//!
//! * A run takes the single cleanup lease, with an owner and a reason; a
//!   second request while it is held is refused (a scheduled attempt that
//!   finds it held is recorded as SKIPPED, never as completed).
//! * The schedule: every 6 hours; at startup, if the last completed run is
//!   older than the interval (or there was none), a catch-up runs after
//!   30 seconds. The time of the last completed run is persisted.
//! * Retention: at most 25 worktrees and 50 GiB. Only when a cap is exceeded
//!   does a run remove anything, and then only worktrees [`eligibility`]
//!   allows — orphaned ones first, then least recently used — down to
//!   80 percent of the cap that fired (hysteresis). A worktree with a
//!   running task, younger than the protection window, dirty or holding an
//!   unapplied result is never removed; it is reported as needing a decision
//!   and recorded on the log once.
//! * Every removal re-checks its worktree right before it acts and is
//!   confined to the worktree root after symlinks are resolved. The removal
//!   is an event on the owning task's log (`WorktreeRemoved`), and the report
//!   is made from what was actually removed, checked on the filesystem.
//!
//! The removal is an effect with events, not an `EffectReceipt`: receipts are
//! bound to a tool call and its kernel decision (docs/23), and a scheduled
//! cleanup has neither.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use modbit_domain::Timestamp;
use modbit_domain::event::Actor;
use modbit_domain::state::StateMachine;
use modbit_git::Repo;
use modbit_protocol::v1 as wire;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::server::Core;
use crate::worktrees::{self, Entry, Policy};

const INTERVAL_MS: i64 = 6 * 60 * 60 * 1000;
const CATCH_UP_MS: i64 = 30 * 1000;

/// Who holds the cleanup lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Holder {
    pub owner: String,
    pub reason: String,
}

/// The cleanup lease and the schedule's next time.
#[derive(Default)]
pub(crate) struct Manager {
    lease: std::sync::Mutex<Option<Holder>>,
    next_at_ms: AtomicI64,
}

/// Held for the duration of one run; released on drop.
pub(crate) struct LeaseGuard<'a> {
    manager: &'a Manager,
}

impl Drop for LeaseGuard<'_> {
    fn drop(&mut self) {
        *self.manager.lease.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

impl Manager {
    /// Take the lease, or learn who holds it.
    pub(crate) fn try_lease(&self, owner: &str, reason: &str) -> Result<LeaseGuard<'_>, Holder> {
        let mut g = self.lease.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = g.as_ref() {
            return Err(h.clone());
        }
        *g = Some(Holder {
            owner: owner.to_owned(),
            reason: reason.to_owned(),
        });
        Ok(LeaseGuard { manager: self })
    }

    /// When the schedule next fires (0: it is not running).
    pub(crate) fn next_cleanup_at_ms(&self) -> i64 {
        self.next_at_ms.load(Ordering::Relaxed)
    }
}

/// What one run did.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Report {
    pub run_id: String,
    pub owner: String,
    pub reason: String,
    /// `COMPLETED` | `SKIPPED` | `DRY_RUN`.
    pub status: String,
    pub skipped_reason: String,
    pub scanned: u32,
    pub removed: u32,
    pub bytes_freed: u64,
    pub errors: Vec<String>,
    pub removed_ids: Vec<String>,
    pub kept: Vec<(String, String)>,
    pub attention: Vec<String>,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub policy: Policy,
    pub remaining: u32,
    pub remaining_bytes: u64,
    pub holder_owner: String,
    pub holder_reason: String,
}

impl Report {
    pub(crate) fn to_wire(&self) -> wire::WorktreeCleanupReport {
        wire::WorktreeCleanupReport {
            run_id: self.run_id.clone(),
            owner: self.owner.clone(),
            reason: self.reason.clone(),
            status: self.status.clone(),
            skipped_reason: self.skipped_reason.clone(),
            scanned: self.scanned,
            removed: self.removed,
            bytes_freed: self.bytes_freed,
            errors: self.errors.clone(),
            removed_ids: self.removed_ids.clone(),
            kept: self
                .kept
                .iter()
                .map(|(id, why)| wire::WorktreeKept {
                    worktree_id: id.clone(),
                    why: why.clone(),
                })
                .collect(),
            attention: self.attention.clone(),
            started_at_ms: self.started_at_ms,
            finished_at_ms: self.finished_at_ms,
            policy: Some(self.policy.to_wire()),
            remaining: self.remaining,
            remaining_bytes: self.remaining_bytes,
            holder_owner: self.holder_owner.clone(),
            holder_reason: self.holder_reason.clone(),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct StateFile {
    #[serde(default)]
    last_completed_at_ms: i64,
    #[serde(default)]
    last_run: Option<Report>,
}

/// The persisted state, as the list view serves it.
pub(crate) struct State {
    pub last_completed_at_ms: i64,
    pub last_run: Option<wire::WorktreeCleanupReport>,
}

fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join("worktree-cleanup.json")
}

fn read_state(data_dir: &Path) -> StateFile {
    std::fs::read(state_path(data_dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub(crate) fn load_state(data_dir: &Path) -> State {
    let f = read_state(data_dir);
    State {
        last_completed_at_ms: f.last_completed_at_ms,
        last_run: f.last_run.as_ref().map(Report::to_wire),
    }
}

/// Persist atomically: write beside, then rename over.
fn save_state(data_dir: &Path, f: &StateFile) {
    let path = state_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    if let Ok(bytes) = serde_json::to_vec_pretty(f)
        && std::fs::write(&tmp, bytes).is_ok()
    {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// What a run is asked to do.
pub(crate) struct Request {
    pub owner: String,
    pub reason: String,
    pub dry_run: bool,
    pub policy: Policy,
}

fn now_ms() -> i64 {
    Timestamp::now().millis()
}

/// Confine `path` to `root` after symlinks are resolved; a path that is a
/// symlink itself is refused.
fn confined(path: &Path, root: &Path) -> Result<PathBuf, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{} is a symbolic link", path.display()));
    }
    let real = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let root = root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", root.display()))?;
    if real == root || !real.starts_with(&root) {
        return Err(format!(
            "{} is outside the worktree root {}",
            real.display(),
            root.display()
        ));
    }
    Ok(real)
}

/// Remove one worktree (blocking), right after re-checking it.
fn remove_one(data_dir: &Path, e: &Entry) -> Result<u64, String> {
    // The roots a Core-managed worktree may live under.
    let managed = data_dir.join("worktrees");
    let root = match e.rec.as_ref().filter(|r| r.kind == "TOOL") {
        Some(r) => modbit_tools::direct::worktree_root(Path::new(&r.origin_root))
            .ok_or_else(|| "its checkout has no parent directory".to_owned())?,
        None => managed,
    };
    let real = confined(&e.path, &root)?;
    let bytes = worktrees::inspect(&real, e.rec.as_ref().map(|r| r.base_revision.as_str())).bytes;
    // The repository that owns the worktree: its origin, or — for an orphan —
    // the one its `.git` file points at.
    let owner = e
        .rec
        .as_ref()
        .and_then(|r| Repo::open(Path::new(&r.origin_root)).ok())
        .or_else(|| {
            Repo::open(&real)
                .ok()
                .and_then(|w| w.common_checkout())
                .and_then(|p| Repo::open(&p).ok())
        });
    match &owner {
        Some(repo) => repo.worktree_remove(&real).map_err(|er| er.to_string())?,
        None => {
            // No repository owns it any more: a plain directory under the root.
            std::fs::remove_dir_all(&real).map_err(|er| er.to_string())?;
        }
    }
    if let (Some(repo), Some(r)) = (&owner, e.rec.as_ref())
        && r.branch.starts_with("modbit/")
    {
        // The branch belonged to the worktree; its work is applied, merged or
        // discarded. (A branch that is gone already is fine.)
        let _ = repo.branch_delete(&r.branch);
    }
    if real.exists() {
        // `git worktree remove` can leave ignored build output behind.
        std::fs::remove_dir_all(&real).map_err(|er| er.to_string())?;
    }
    if real.exists() {
        return Err(format!("{} is still there after removal", real.display()));
    }
    if let Some(repo) = owner {
        let _ = repo.worktree_prune();
    }
    Ok(bytes)
}

/// Whether a missing directory must not be recorded as a removal yet: its
/// task is still running, is live on the runtime, or is not on this log.
pub(crate) fn directory_gone_is_premature(e: &Entry) -> bool {
    e.rec.is_some()
        && (e.live
            || e.task
                .as_ref()
                .is_none_or(|t| !modbit_domain::state::StateMachine::is_terminal(t.state)))
}

/// Run one cleanup. `Err(holder)`: the lease is held, nothing was done.
pub(crate) async fn run(core: &Arc<Core>, req: Request) -> Result<Report, Holder> {
    let _guard = core.worktrees.try_lease(&req.owner, &req.reason)?;
    if let Some(ms) = std::env::var("MODBIT_FAULT_WORKTREE_CLEANUP_HOLD_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }
    let started = now_ms();
    let run_id = modbit_domain::EventId::new().to_string();
    let policy = req.policy;
    let mut report = Report {
        run_id,
        owner: req.owner.clone(),
        reason: req.reason.clone(),
        status: if req.dry_run { "DRY_RUN" } else { "COMPLETED" }.into(),
        skipped_reason: String::new(),
        scanned: 0,
        removed: 0,
        bytes_freed: 0,
        errors: vec![],
        removed_ids: vec![],
        kept: vec![],
        attention: vec![],
        started_at_ms: started,
        finished_at_ms: started,
        policy,
        remaining: 0,
        remaining_bytes: 0,
        holder_owner: String::new(),
        holder_reason: String::new(),
    };
    let entries = worktrees::entries(core, &policy, now_ms()).await;
    report.scanned = entries.len() as u32;
    let actor = Actor::Core(format!("worktree-cleanup:{}", req.owner));
    let id_of = |e: &Entry| {
        e.rec
            .as_ref()
            .map_or_else(|| worktrees::plain_path(&e.path), |r| r.worktree_id.clone())
    };
    // A record whose directory is gone: the log says so.
    for e in entries.iter().filter(|e| !e.info.exists) {
        // A running task's worktree is never recorded as removed: a missing
        // directory under a live task is the task's business (it may be
        // between steps), and a later scan decides once the task has ended.
        if directory_gone_is_premature(e) {
            report.kept.push((
                id_of(e),
                "its directory was not found, but its task has not ended; left for a later scan"
                    .into(),
            ));
            continue;
        }
        if let Some(r) = &e.rec
            && !req.dry_run
            && let Some(task) = e.task.as_ref()
        {
            let mut store = core.store.lock().await;
            let _ = worktrees::append_events(
                &mut store,
                core.tenant_id,
                task.session_id,
                task.task_id,
                worktrees::aggregate_id(&r.worktree_id),
                vec![worktrees::removed_event(
                    &r.worktree_id,
                    "its directory was gone",
                    0,
                    &req.owner,
                    &actor,
                )],
            );
        }
        report.kept.push((
            id_of(e),
            "its directory was gone; recorded as removed".into(),
        ));
    }
    let present: Vec<&Entry> = entries.iter().filter(|e| e.info.exists).collect();
    let mut count = present.len() as u64;
    let mut bytes: u64 = present.iter().map(|e| e.info.bytes).sum();
    let over_count = count > u64::from(policy.max_worktrees);
    let over_bytes = bytes > policy.max_bytes;
    let target_count = u64::from(policy.max_worktrees) * u64::from(policy.hysteresis_percent) / 100;
    let target_bytes = policy.max_bytes / 100 * u64::from(policy.hysteresis_percent);
    // Orphans first, then least recently used.
    let mut candidates: Vec<&Entry> = present
        .iter()
        .copied()
        .filter(|e| e.eligibility.removable)
        .collect();
    candidates.sort_by_key(|e| (!e.orphan, e.last_activity_ms));
    let mut chosen: Vec<&Entry> = Vec::new();
    if over_count || over_bytes {
        for e in candidates {
            let count_high = over_count && count > target_count;
            let bytes_high = over_bytes && bytes > target_bytes;
            if !count_high && !bytes_high {
                break;
            }
            count = count.saturating_sub(1);
            bytes = bytes.saturating_sub(e.info.bytes);
            chosen.push(e);
        }
    }
    let chosen_ids: Vec<String> = chosen.iter().map(|e| id_of(e)).collect();
    for e in &present {
        let id = id_of(e);
        if chosen_ids.contains(&id) {
            continue;
        }
        let why = if e.eligibility.removable {
            if over_count || over_bytes {
                "removable, but the caps were met without it".to_owned()
            } else {
                "removable, and the caps do not ask for it".to_owned()
            }
        } else {
            e.eligibility.reason.clone()
        };
        report.kept.push((id, why));
    }
    // Worktrees that need a decision, said once on their task's log, and in
    // the report whenever a cap is exceeded.
    let pressure = over_count || over_bytes;
    let needing: Vec<&Entry> = present
        .iter()
        .copied()
        .filter(|e| e.eligibility.needs_decision)
        .collect();
    report.attention =
        worktrees::attention_lines(&needing.iter().map(|e| (*e).clone()).collect::<Vec<_>>());
    if pressure && !req.dry_run {
        for e in &needing {
            if let (Some(r), Some(task)) = (&e.rec, &e.task)
                && !r.needs_decision
            {
                let mut store = core.store.lock().await;
                let _ = worktrees::append_events(
                    &mut store,
                    core.tenant_id,
                    task.session_id,
                    task.task_id,
                    worktrees::aggregate_id(&r.worktree_id),
                    vec![worktrees::ev(
                        worktrees::NEEDS_DECISION,
                        json!({
                            "worktree_id": r.worktree_id,
                            "reason": e.eligibility.reason,
                            "cleanup_id": report.run_id,
                        }),
                        &actor,
                    )],
                );
            }
        }
    }
    // Remove what was chosen, each after a fresh look.
    for e in chosen {
        let id = id_of(e);
        if req.dry_run {
            report.removed += 1;
            report.removed_ids.push(id);
            report.bytes_freed += e.info.bytes;
            continue;
        }
        // Fresh state: the task may have started again, the user may have
        // edited the worktree since the decision.
        let fresh = {
            let task = match e.rec.as_ref().and_then(|r| r.task_id) {
                Some(t) => core.store.lock().await.task(&t).ok().flatten(),
                None => None,
            };
            let live = match &task {
                Some(t) => core.runtime.is_running(&t.task_id).await,
                None => false,
            };
            let rec = e.rec.clone();
            let path = e.path.clone();
            let created = e.created_ms;
            let task2 = task.clone();
            let info = tokio::task::spawn_blocking(move || {
                worktrees::inspect(&path, rec.as_ref().map(|r| r.base_revision.as_str()))
            })
            .await
            .unwrap_or_default();
            let elig = worktrees::eligibility(
                e.rec.as_ref(),
                task2.as_ref(),
                live,
                &info,
                now_ms(),
                created,
                &policy,
            );
            (task, elig)
        };
        if !fresh.1.removable {
            report.kept.push((
                id,
                format!("changed since it was chosen: {}", fresh.1.reason),
            ));
            continue;
        }
        let data_dir = core.data_dir.clone();
        let entry_for_blocking = e.clone();
        let removed =
            tokio::task::spawn_blocking(move || remove_one(&data_dir, &entry_for_blocking))
                .await
                .unwrap_or_else(|er| Err(er.to_string()));
        match removed {
            Ok(freed) => {
                report.removed += 1;
                report.bytes_freed += freed;
                report.removed_ids.push(id.clone());
                if let (Some(r), Some(task)) = (&e.rec, &fresh.0) {
                    let mut store = core.store.lock().await;
                    let _ = worktrees::append_events(
                        &mut store,
                        core.tenant_id,
                        task.session_id,
                        task.task_id,
                        worktrees::aggregate_id(&r.worktree_id),
                        vec![worktrees::removed_event(
                            &r.worktree_id,
                            &format!("cleanup {}: {}", report.run_id, fresh.1.reason),
                            freed,
                            &req.owner,
                            &actor,
                        )],
                    );
                }
            }
            Err(why) => {
                report.errors.push(format!("{id}: {why}"));
                report.kept.push((id, format!("removal failed: {why}")));
            }
        }
    }
    // The report is made from the filesystem, not from the intent.
    let mut remaining = 0u32;
    let mut remaining_bytes = 0u64;
    for e in &present {
        if e.path.exists() {
            remaining += 1;
            remaining_bytes += e.info.bytes;
        }
    }
    report.remaining = remaining;
    report.remaining_bytes = remaining_bytes;
    report.finished_at_ms = now_ms();
    let offset = core.store.lock().await.last_offset().unwrap_or(0);
    core.last_offset.send_replace(offset);
    if !req.dry_run {
        let mut f = read_state(&core.data_dir);
        f.last_completed_at_ms = report.finished_at_ms;
        f.last_run = Some(report.clone());
        save_state(&core.data_dir, &f);
    }
    Ok(report)
}

/// A scheduled attempt that found the lease held: recorded as SKIPPED, and
/// not as a completed run (the last-completed time does not move).
fn skipped(req: &Request, holder: &Holder) -> Report {
    let t = now_ms();
    Report {
        run_id: modbit_domain::EventId::new().to_string(),
        owner: req.owner.clone(),
        reason: req.reason.clone(),
        status: "SKIPPED".into(),
        skipped_reason: format!(
            "CLEANUP_LEASE_HELD: held by {} for `{}`",
            holder.owner, holder.reason
        ),
        scanned: 0,
        removed: 0,
        bytes_freed: 0,
        errors: vec![],
        removed_ids: vec![],
        kept: vec![],
        attention: vec![],
        started_at_ms: t,
        finished_at_ms: t,
        policy: req.policy,
        remaining: 0,
        remaining_bytes: 0,
        holder_owner: holder.owner.clone(),
        holder_reason: holder.reason.clone(),
    }
}

fn env_ms(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// Start the schedule: the first run at the interval after the last
/// completed one, or — at startup with none, or an overdue one — after the
/// catch-up delay.
pub(crate) fn start_schedule(core: &Arc<Core>) {
    let core = Arc::clone(core);
    tokio::spawn(async move {
        let interval = env_ms("MODBIT_WORKTREE_CLEANUP_INTERVAL_MS", INTERVAL_MS);
        let catch_up = env_ms("MODBIT_WORKTREE_CLEANUP_CATCHUP_MS", CATCH_UP_MS);
        let mut first = true;
        loop {
            let last = read_state(&core.data_dir).last_completed_at_ms;
            let now = now_ms();
            let overdue = last == 0 || now - last >= interval;
            let wait = if first && overdue {
                catch_up
            } else if overdue {
                interval
            } else {
                interval - (now - last)
            };
            core.worktrees
                .next_at_ms
                .store(now + wait, Ordering::Relaxed);
            tokio::time::sleep(std::time::Duration::from_millis(wait.max(1) as u64)).await;
            let req = Request {
                owner: format!("core-{}:scheduler", core.recovery().boot_generation),
                reason: if first && overdue {
                    "catch-up".into()
                } else {
                    "scheduled".into()
                },
                dry_run: false,
                policy: Policy::resolve(None),
            };
            first = false;
            match run(
                &core,
                Request {
                    policy: req.policy,
                    owner: req.owner.clone(),
                    reason: req.reason.clone(),
                    dry_run: false,
                },
            )
            .await
            {
                Ok(r) => {
                    if !r.errors.is_empty() {
                        eprintln!("modbit-core: worktree cleanup: {}", r.errors.join("; "));
                    }
                }
                Err(holder) => {
                    let mut f = read_state(&core.data_dir);
                    f.last_run = Some(skipped(&req, &holder));
                    save_state(&core.data_dir, &f);
                    // A skipped run does not count as one that happened.
                    tokio::time::sleep(std::time::Duration::from_millis(
                        interval.min(60_000) as u64
                    ))
                    .await;
                }
            }
        }
    });
}

/// `RunWorktreeCleanup`.
pub(crate) async fn command(
    core: &Arc<Core>,
    p: &wire::RunWorktreeCleanup,
    actor: &Actor,
) -> Result<wire::WorktreeCleanupReport, (String, String)> {
    let req = Request {
        owner: format!("core-{}:{actor:?}", core.recovery().boot_generation),
        reason: if p.reason.is_empty() {
            "requested".into()
        } else {
            p.reason.clone()
        },
        dry_run: p.dry_run,
        policy: Policy::resolve(p.policy.as_ref()),
    };
    match run(core, req).await {
        Ok(r) => Ok(r.to_wire()),
        Err(h) => Err((
            "CLEANUP_LEASE_HELD".into(),
            format!(
                "a worktree cleanup is already running: held by {} for `{}`",
                h.owner, h.reason
            ),
        )),
    }
}

/// `RemoveWorktree` (PX-068): remove the one worktree a person chose, by the
/// rule the cleanup applies and under the same single lease. The Core decides
/// again right now whether it is removable; a worktree that is not is refused
/// with a typed reason and left exactly as it is. With `dry_run` nothing is
/// removed and the answer says what a removal would take with it.
pub(crate) async fn remove_command(
    core: &Arc<Core>,
    p: &wire::RemoveWorktree,
    actor: &Actor,
) -> Result<wire::WorktreeRemoval, (String, String)> {
    let refuse = |code: &str, msg: String| (code.to_owned(), msg);
    let id = p.worktree_id.trim();
    if id.is_empty() {
        return Err(refuse("BAD_PAYLOAD", "worktree_id required".into()));
    }
    let owner = format!("core-{}:{actor:?}", core.recovery().boot_generation);
    let _guard = core
        .worktrees
        .try_lease(&owner, "remove one worktree")
        .map_err(|h| {
            refuse(
                "CLEANUP_LEASE_HELD",
                format!(
                    "a worktree cleanup is already running: held by {} for `{}`",
                    h.owner, h.reason
                ),
            )
        })?;
    let policy = Policy::resolve(None);
    let entries = worktrees::entries(core, &policy, now_ms()).await;
    let id_of = |e: &Entry| {
        e.rec
            .as_ref()
            .map_or_else(|| worktrees::plain_path(&e.path), |r| r.worktree_id.clone())
    };
    let Some(e) = entries.iter().find(|e| id_of(e) == id) else {
        return Err(refuse(
            "UNKNOWN_WORKTREE",
            format!("`{id}` is not a worktree this Core manages"),
        ));
    };
    let el = &e.eligibility;
    let branch = e
        .rec
        .as_ref()
        .map_or_else(String::new, |r| r.branch.clone());
    let path = worktrees::plain_path(&e.path);
    let gone = !e.info.exists;
    if !el.removable && !gone {
        let code = if e.task.as_ref().is_some_and(|t| !t.state.is_terminal()) || e.live {
            "TASK_RUNNING"
        } else if el.protected {
            "WORKTREE_PROTECTED"
        } else if el.needs_decision {
            "WORKTREE_NEEDS_DECISION"
        } else {
            "WORKTREE_NOT_REMOVABLE"
        };
        let hint = if el.needs_decision {
            "; apply, merge or discard its result first"
        } else {
            ""
        };
        return Err(refuse(code, format!("{}{hint}", el.reason)));
    }
    let mut loses = Vec::new();
    if gone {
        loses.push(
            "nothing on disk: its directory is already gone; the record is closed".to_owned(),
        );
    } else {
        loses.push(format!(
            "the worktree directory {path} ({} bytes)",
            e.info.bytes
        ));
        if branch.starts_with("modbit/") {
            loses.push(format!(
                "the branch {branch}, which belonged to the worktree"
            ));
        }
        if let Some(d) = e.rec.as_ref().and_then(|r| r.disposition.as_ref()) {
            loses.push(format!(
                "nothing of the result: it was {} and has not changed since",
                d.kind.to_lowercase()
            ));
        } else {
            loses.push(
                "nothing of the result: the worktree holds nothing beyond its base".to_owned(),
            );
        }
    }
    let mut out = wire::WorktreeRemoval {
        worktree_id: id.to_owned(),
        removed: false,
        dry_run: p.dry_run,
        branch,
        path,
        bytes: if gone { 0 } else { e.info.bytes },
        reason: el.reason.clone(),
        loses,
        offset: 0,
    };
    if p.dry_run {
        return Ok(out);
    }
    let freed = if gone {
        0
    } else {
        let data_dir = core.data_dir.clone();
        let entry = e.clone();
        tokio::task::spawn_blocking(move || remove_one(&data_dir, &entry))
            .await
            .unwrap_or_else(|er| Err(er.to_string()))
            .map_err(|why| refuse("WORKTREE_REMOVE_FAILED", format!("{id}: {why}")))?
    };
    out.removed = true;
    out.bytes = freed;
    if let (Some(r), Some(task)) = (&e.rec, &e.task) {
        let mut store = core.store.lock().await;
        let offset = worktrees::append_events(
            &mut store,
            core.tenant_id,
            task.session_id,
            task.task_id,
            worktrees::aggregate_id(&r.worktree_id),
            vec![worktrees::removed_event(
                &r.worktree_id,
                &format!("removed by request: {}", el.reason),
                freed,
                &owner,
                &Actor::Core(format!("worktree-remove:{owner}")),
            )],
        )
        .map_err(|why| refuse("STORE", why))?;
        core.last_offset.send_replace(offset);
        out.offset = offset;
    }
    Ok(out)
}
