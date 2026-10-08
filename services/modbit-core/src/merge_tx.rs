//! Agent merge integration (PX-119; docs/62, docs/14, docs/17): the tools
//! `git.merge.prepare`, `git.merge.commit` and `git.merge.abort` over the
//! `modbit-git` merge transaction, with the transaction's state on the event
//! log, post-merge verification before the commit counts, and a startup
//! recovery that leaves no half-merged tree.
//!
//! A transaction is a series of snapshots on `MergeTransactionRecorded`:
//!
//! ```text
//! STARTED  (write-ahead: source, target, base — before Git is touched)
//!   -> STAGED | CONFLICTED        the merge ran; conflicts are per-file evidence
//!   -> VERIFY_FAILED              the resolved tree failed its mandatory checks
//!   -> COMMITTED | ABORTED        terminal
//! ```
//!
//! Git's own state (`MERGE_HEAD`, the index) is the other half of the truth.
//! They are reconciled at startup ([`recover`]): a transaction the log says is
//! open is resumed when Git still holds its merge, and aborted to exactly the
//! pre-merge commit when Git was killed part-way; one Git finished but the log
//! never heard of is recorded as committed. A merge never starts on a dirty
//! target, so "aborted to the pre-merge commit" is a whole tree by
//! construction.
//!
//! The source of a child's merge is the child's worktree *as it is*: its
//! committed work and its dirty files, snapshotted into a commit under
//! `refs/modbit/snapshots/` without touching the child's tree. The child is
//! settled — for the parent's completion gate — once its result is merged, or
//! explicitly discarded by a recorded decision.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use modbit_domain::event::Actor;
use modbit_domain::state::StateMachine;
use modbit_domain::{SessionId, TaskId};
use modbit_git::{MergeState, MergeTransaction, Repo};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::apply_back::fault_point;
use crate::git_state::CoreGitState;
use crate::worktrees::{self, MERGE_TX, MERGE_VERIFIED};

type Refusal = (String, String);

fn refuse(code: &str, msg: impl Into<String>) -> Refusal {
    (code.to_owned(), msg.into())
}

/// A merge transaction as the log holds it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tx {
    pub id: String,
    /// `STARTED` | `STAGED` | `CONFLICTED` | `VERIFY_FAILED` | `COMMITTED` | `ABORTED`.
    pub state: String,
    pub target_root: String,
    pub target_branch: String,
    pub target_before: String,
    pub source: String,
    pub source_label: String,
    pub base: String,
    pub conflicts: Vec<String>,
    pub resolutions: Vec<Value>,
    pub child_task_id: Option<String>,
    pub child_worktree_id: Option<String>,
    pub result: Option<String>,
    pub verdict: Option<String>,
    pub verification_ref: Option<String>,
    pub task: Option<TaskId>,
    pub session: Option<SessionId>,
}

impl Tx {
    pub(crate) fn open(&self) -> bool {
        matches!(
            self.state.as_str(),
            "STARTED" | "STAGED" | "CONFLICTED" | "VERIFY_FAILED"
        )
    }

    fn from_event(p: &Value, task: Option<TaskId>, session: SessionId) -> Self {
        let s = |k: &str| p[k].as_str().unwrap_or_default().to_owned();
        let list = |k: &str| -> Vec<String> {
            p[k].as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        Self {
            id: s("tx_id"),
            state: s("state"),
            target_root: s("target_root"),
            target_branch: s("target_branch"),
            target_before: s("target_before"),
            source: s("source"),
            source_label: s("source_label"),
            base: s("base"),
            conflicts: list("conflicts"),
            resolutions: p["resolutions"].as_array().cloned().unwrap_or_default(),
            child_task_id: p["child_task_id"].as_str().map(str::to_owned),
            child_worktree_id: p["child_worktree_id"].as_str().map(str::to_owned),
            result: p["result"].as_str().map(str::to_owned),
            verdict: p["verdict"].as_str().map(str::to_owned),
            verification_ref: p["verification_ref"].as_str().map(str::to_owned),
            task,
            session: Some(session),
        }
    }

    fn payload(&self, note: &str) -> Value {
        json!({
            "tx_id": self.id,
            "state": self.state,
            "target_root": self.target_root,
            "target_branch": self.target_branch,
            "target_before": self.target_before,
            "source": self.source,
            "source_label": self.source_label,
            "base": self.base,
            "conflicts": self.conflicts,
            "resolutions": self.resolutions,
            "child_task_id": self.child_task_id,
            "child_worktree_id": self.child_worktree_id,
            "result": self.result,
            "verdict": self.verdict,
            "verification_ref": self.verification_ref,
            "note": note,
        })
    }

    /// The library's transaction, rebuilt from the log's.
    fn to_git(&self) -> MergeTransaction {
        MergeTransaction {
            id: self.id.clone(),
            source: self.source.clone(),
            target_branch: self.target_branch.clone(),
            target_before: self.target_before.clone(),
            base: self.base.clone(),
            worktree: PathBuf::from(&self.target_root),
            conflicts: self.conflicts.clone(),
            resolutions: self
                .resolutions
                .iter()
                .filter_map(|r| r["path"].as_str().map(str::to_owned))
                .collect(),
            state: match self.state.as_str() {
                "CONFLICTED" => MergeState::Conflicted,
                "COMMITTED" => MergeState::Committed,
                "ABORTED" => MergeState::Aborted,
                _ => MergeState::Staged,
            },
            result: self.result.clone(),
        }
    }
}

fn agg(id: &str) -> [u8; 16] {
    worktrees::aggregate_id(&format!("merge:{id}"))
}

/// The latest state of transaction `id`.
pub(crate) fn latest(store: &modbit_event_store::EventStore, id: &str) -> Option<Tx> {
    let events = store.read_aggregate(&agg(id), 0, usize::MAX).ok()?;
    events
        .iter()
        .rev()
        .find(|e| e.envelope.event_type == MERGE_TX)
        .and_then(|e| {
            let p = store.payload(&e.envelope).ok()?;
            Some(Tx::from_event(
                &p,
                e.envelope.task_id,
                e.envelope.session_id,
            ))
        })
}

/// Every transaction on the log, by id, latest state.
pub(crate) fn all(store: &modbit_event_store::EventStore) -> Vec<Tx> {
    let mut by_id: std::collections::BTreeMap<String, Tx> = Default::default();
    for e in store.read_all_of_types(&[MERGE_TX], 0).unwrap_or_default() {
        if let Ok(p) = store.payload(&e.envelope) {
            let tx = Tx::from_event(&p, e.envelope.task_id, e.envelope.session_id);
            by_id.insert(tx.id.clone(), tx);
        }
    }
    by_id.into_values().collect()
}

async fn record(
    port: &CoreGitState,
    tx: &Tx,
    note: &str,
    extra: Vec<modbit_event_store::NewEvent>,
) -> Result<(), Refusal> {
    let mut events = vec![worktrees::ev(MERGE_TX, tx.payload(note), &port.actor)];
    events.extend(extra);
    let mut st = port.store.lock().await;
    worktrees::append_events(
        &mut st,
        port.tenant,
        port.session,
        port.task,
        agg(&tx.id),
        events,
    )
    .map(|_| ())
    .map_err(|e| refuse("STORE", e))
}

fn short(s: &str) -> &str {
    &s[..s.len().min(12)]
}

/// A child the task spawned, by idempotency key, agent id or task id: its
/// task, and the worktree record made for it.
fn find_child(
    store: &modbit_event_store::EventStore,
    session: &SessionId,
    parent: TaskId,
    which: &str,
) -> Result<(modbit_domain::task::Task, worktrees::Record, String), Refusal> {
    let nodes = store.agent_nodes(&parent).unwrap_or_default();
    let node = nodes.iter().find(|n| {
        n.kind == "SUBAGENT"
            && (n.idempotency_key == which
                || n.agent_id.to_string() == which
                || n.child_task_id.map(|t| t.to_string()).as_deref() == Some(which))
    });
    let child_id = node
        .and_then(|n| n.child_task_id)
        .or_else(|| TaskId::parse(which).ok())
        .ok_or_else(|| {
            refuse(
                "UNKNOWN_CHILD",
                format!("`{which}` names no child this task spawned"),
            )
        })?;
    let child = store.task(&child_id).ok().flatten().ok_or_else(|| {
        refuse(
            "UNKNOWN_CHILD",
            format!("child task {child_id} is not on the log"),
        )
    })?;
    if child.origin != modbit_domain::task::TaskOrigin::Subagent
        && !nodes.iter().any(|n| n.child_task_id == Some(child_id))
    {
        return Err(refuse(
            "UNKNOWN_CHILD",
            format!("task {child_id} is not a child of this task"),
        ));
    }
    let rec = worktrees::registry_of_session(store, session)
        .into_iter()
        .find(|r| r.task_id == Some(child_id) && !r.removed)
        .ok_or_else(|| {
            refuse(
                "CHILD_HAS_NO_WORKTREE",
                format!(
                    "child task {child_id} has no worktree on the log (it may have been removed)"
                ),
            )
        })?;
    // The child's own end is its node's: a child that finished its work sits
    // in review (its task is not terminal), but the node is COMPLETED.
    let status = node.map(|n| n.status.clone()).unwrap_or_else(|| {
        if child.state.is_terminal() {
            "COMPLETED".to_owned()
        } else {
            format!("{:?}", child.state)
        }
    });
    Ok((child, rec, status))
}

/// `git.merge.prepare`.
#[allow(clippy::too_many_lines)]
pub(crate) async fn prepare(port: &CoreGitState, args: &Value) -> Result<Value, Refusal> {
    let root = port.root.clone().ok_or_else(|| {
        refuse(
            "NO_WORKSPACE",
            "the task has no workspace root to merge into",
        )
    })?;
    let root_text = worktrees::plain_path(&root);
    // Resolve what is merged.
    let child = args["child"].as_str().map(str::to_owned);
    let source_arg = args["source"].as_str().map(str::to_owned);
    let (child_info, source_label) = match &child {
        Some(which) => {
            let st = port.store.lock().await;
            let (task, rec, status) = find_child(&st, &port.session, port.task, which)?;
            if !matches!(status.as_str(), "COMPLETED" | "FAILED" | "CANCELLED") {
                return Err(refuse(
                    "CHILD_NOT_SETTLED",
                    format!(
                        "child task {} is {status}; collect it with agent.wait before merging its work",
                        task.task_id
                    ),
                ));
            }
            let label = format!("child `{which}` ({})", rec.branch);
            (Some((task, rec)), label)
        }
        None => (None, source_arg.clone().unwrap_or_default()),
    };
    let child_path = child_info.as_ref().map(|(_, r)| r.path.clone());
    let child_base = child_info
        .as_ref()
        .map(|(_, r)| r.base_revision.clone())
        .unwrap_or_default();
    let source_rev = source_arg.clone();
    let id_arg = args["id"].as_str().map(str::to_owned);
    let task_id = port.task;
    let root2 = root.clone();
    // Git: the preconditions and the source commit (blocking).
    struct Pre {
        tx_id: String,
        source: String,
        target_branch: String,
        target_before: String,
        nothing: bool,
    }
    let pre = tokio::task::spawn_blocking(move || -> Result<Pre, Refusal> {
        let repo = Repo::open(&root2).map_err(|e| refuse("GIT", e.to_string()))?;
        let branch = repo
            .current_branch()
            .map_err(|e| refuse("GIT", e.to_string()))?
            .ok_or_else(|| {
                refuse(
                    "MERGE_TARGET_DETACHED",
                    "the checkout is not on a branch; a merge needs one to commit to",
                )
            })?;
        let head = repo.head().map_err(|e| refuse("GIT", e.to_string()))?;
        let dirty = repo.status().map_err(|e| refuse("GIT", e.to_string()))?;
        if !dirty.is_empty() {
            return Err(refuse(
                "MERGE_TARGET_DIRTY",
                format!(
                    "the checkout has uncommitted changes ({}); a merge starts on a clean tree so that aborting restores it exactly",
                    dirty
                        .iter()
                        .take(8)
                        .map(|e| e.path.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        let (source, nothing) = match (&child_path, &source_rev) {
            (Some(path), _) => {
                let wt = Repo::open(Path::new(path)).map_err(|e| refuse("GIT", e.to_string()))?;
                let snap_id = format!("merge-{}", &hex::encode(Sha256::digest(
                    format!("{task_id}|{path}|{head}").as_bytes()
                ))[..16]);
                let snap = wt
                    .snapshot_dirty(&snap_id)
                    .map_err(|e| refuse("GIT", e.to_string()))?;
                let base_tree = if child_base.is_empty() {
                    modbit_git::EMPTY_TREE.to_owned()
                } else {
                    wt.tree_of(&child_base)
                        .map_err(|e| refuse("GIT", e.to_string()))?
                };
                (snap.commit, snap.tree == base_tree)
            }
            (None, Some(rev)) => (
                repo.resolve(&format!("{rev}^{{commit}}"))
                    .map_err(|e| refuse("INVALID_REF", e.to_string()))?,
                false,
            ),
            (None, None) => return Err(refuse("BAD_PAYLOAD", "child or source required")),
        };
        let tx_id = id_arg.unwrap_or_else(|| {
            format!(
                "mx-{}",
                &hex::encode(Sha256::digest(format!("{task_id}|{source}|{head}").as_bytes()))[..16]
            )
        });
        Ok(Pre {
            tx_id,
            source,
            target_branch: branch,
            target_before: head,
            nothing,
        })
    })
    .await
    .map_err(|e| refuse("MERGE_FAILED", e.to_string()))??;

    // A child that changed nothing is settled without a merge.
    if pre.nothing
        && let Some((child_task, rec)) = &child_info
    {
        let mut st = port.store.lock().await;
        worktrees::append_events(
            &mut st,
            port.tenant,
            child_task.session_id,
            child_task.task_id,
            worktrees::aggregate_id(&rec.worktree_id),
            vec![worktrees::ev(
                worktrees::DISPOSED,
                json!({
                    "worktree_id": rec.worktree_id,
                    "disposition": "MERGED",
                    "tree": null,
                    "by": format!("{:?}", port.actor),
                    "reason": "the child changed nothing beyond its base",
                }),
                &port.actor,
            )],
        )
        .map_err(|e| refuse("STORE", e))?;
        return Ok(json!({
            "status": "NOTHING_TO_MERGE",
            "child": child,
            "note": "the child's worktree holds nothing beyond its base; it is settled",
        }));
    }

    // Re-entry: the same transaction, already open.
    if let Some(existing) = latest(&*port.store.lock().await, &pre.tx_id)
        && existing.open()
    {
        return describe(port, &existing, &root, true).await;
    }
    let mut tx = Tx {
        id: pre.tx_id.clone(),
        state: "STARTED".into(),
        target_root: root_text.clone(),
        target_branch: pre.target_branch.clone(),
        target_before: pre.target_before.clone(),
        source: pre.source.clone(),
        source_label,
        base: String::new(),
        child_task_id: child_info.as_ref().map(|(t, _)| t.task_id.to_string()),
        child_worktree_id: child_info.as_ref().map(|(_, r)| r.worktree_id.clone()),
        task: Some(port.task),
        session: Some(port.session),
        ..Default::default()
    };
    // Write-ahead: the intent is on the log before Git is touched.
    record(port, &tx, "merge intended", vec![]).await?;
    fault_point("MERGE_AFTER_STARTED");
    let (root3, txid, source) = (root.clone(), tx.id.clone(), tx.source.clone());
    let begun = tokio::task::spawn_blocking(move || {
        let repo = Repo::open(&root3).map_err(|e| e.to_string())?;
        repo.merge_begin(&txid, &source).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| refuse("MERGE_FAILED", e.to_string()))?;
    fault_point("MERGE_AFTER_BEGIN");
    let begun = match begun {
        Ok(b) => b,
        Err(why) => {
            // Git refused the merge itself (unrelated histories…): put the
            // tree back and say so.
            let root4 = root.clone();
            let before = tx.target_before.clone();
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(repo) = Repo::open(&root4) {
                    let mut g = MergeTransaction {
                        id: String::new(),
                        source: String::new(),
                        target_branch: String::new(),
                        target_before: before,
                        base: String::new(),
                        worktree: root4.clone(),
                        conflicts: vec![],
                        resolutions: vec![],
                        state: MergeState::Staged,
                        result: None,
                    };
                    let _ = repo.merge_abort(&mut g);
                }
            })
            .await;
            tx.state = "ABORTED".into();
            record(port, &tx, &format!("git refused the merge: {why}"), vec![]).await?;
            return Err(refuse("MERGE_FAILED", why));
        }
    };
    tx.base = begun.base.clone();
    tx.conflicts = begun.conflicts.clone();
    tx.state = match begun.state {
        MergeState::Conflicted => "CONFLICTED",
        _ => "STAGED",
    }
    .into();
    record(port, &tx, "merge staged in the worktree", vec![]).await?;
    worktrees::record_neutralized(
        &port.store,
        port.tenant,
        port.session,
        port.task,
        &root_text,
        &port.actor,
    )
    .await;
    describe(port, &tx, &root, false).await
}

/// The transaction as the tool answers: its state and, per conflicted file,
/// the evidence.
async fn describe(
    port: &CoreGitState,
    tx: &Tx,
    root: &Path,
    resumed: bool,
) -> Result<Value, Refusal> {
    let (root2, conflicts) = (root.to_path_buf(), tx.conflicts.clone());
    let evidence = tokio::task::spawn_blocking(move || {
        let repo = Repo::open(&root2).ok()?;
        Some(
            conflicts
                .iter()
                .filter_map(|p| repo.conflict_evidence(p).ok())
                .collect::<Vec<_>>(),
        )
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    let _ = port;
    Ok(json!({
        "status": tx.state,
        "tx_id": tx.id,
        "source": tx.source,
        "source_label": tx.source_label,
        "target_branch": tx.target_branch,
        "target_before": tx.target_before,
        "base": tx.base,
        "resumed": resumed,
        "conflicts": evidence.iter().map(|e| json!({
            "path": e.path,
            "base": e.base,
            "ours": e.ours,
            "theirs": e.theirs,
            "current": e.current,
            "has_markers": e.has_markers,
        })).collect::<Vec<_>>(),
        "next": if tx.conflicts.is_empty() {
            "the merge is staged: call git.merge.commit to verify and commit it, or git.merge.abort"
        } else {
            "edit each conflicted file to its resolution (no conflict markers may remain), then call git.merge.commit; or call git.merge.abort"
        },
    }))
}

/// The verdict of the post-merge verification.
struct Verdict {
    pass: bool,
    status: String,
    checks: Vec<Value>,
    run_ref: String,
    note: String,
}

/// Run the repository's mandatory checks on the merge result in `root`,
/// through the verification engine and the process broker, under the task's
/// lease (a repository-defined command is decided by the Capability Kernel
/// like any other shell effect). The plan is the task's pinned one — never the
/// merged tree's own `.modbit/verification.json`, which the merge may have
/// changed.
async fn verify(port: &CoreGitState, tx: &Tx, root: &Path) -> Verdict {
    let pinned = port.verification_json.clone();
    let before = tx.target_before.clone();
    let root2 = root.to_path_buf();
    let plan = tokio::task::spawn_blocking(move || {
        let configured = match pinned {
            Some(c) => c,
            // Not pinned (the task never resolved its configuration): the
            // target's own committed file, not the merged working tree's.
            None => Repo::open(&root2)
                .ok()
                .and_then(|r| r.show(&before, ".modbit/verification.json").ok())
                .map(|b| String::from_utf8_lossy(&b).into_owned()),
        };
        modbit_verification::plan::derive_with_configured(&root2, &[], &[], configured.as_deref())
    })
    .await;
    let Ok(plan) = plan else {
        return Verdict {
            pass: false,
            status: "ERROR".into(),
            checks: vec![],
            run_ref: String::new(),
            note: "the verification plan could not be derived".into(),
        };
    };
    let objects = port.store.lock().await.objects().clone();
    let plan_ref = objects
        .put(&serde_json::to_vec(&plan).unwrap_or_default())
        .unwrap_or_default();
    let runner = crate::verify::BrokerRunner {
        target: port.exec.clone(),
        execution_profile: port.profile.clone(),
        cancel: port.cancel.clone(),
        kernel: Some(crate::verify::KernelGate {
            lease: port.lease.clone(),
            execution_profile: port.profile.clone(),
            mode: port.mode,
            emergency_stopped: port.emergency,
            config: Arc::clone(&port.config),
        }),
    };
    let sink = crate::verify::ObjectSinkAdapter(objects.clone());
    let engine = modbit_verification::VerificationEngine::new(
        &runner,
        &sink,
        modbit_verification::VerificationPolicy::default(),
    );
    let candidate = format!("merge-{}", tx.id);
    let env: Vec<(String, String)> = vec![
        ("CARGO_TERM_COLOR".into(), "never".into()),
        ("NO_COLOR".into(), "1".into()),
    ];
    let (vrun, _quarantines) = engine
        .run_stage(
            &plan,
            &plan_ref,
            &format!("merge-{}", tx.id),
            modbit_verification::Stage::Completion,
            root,
            &candidate,
            &env,
            &["modbit-core".into()],
        )
        .await;
    let run_ref = objects
        .put(&serde_json::to_vec(&vrun).unwrap_or_default())
        .unwrap_or_default();
    let checks: Vec<Value> = vrun
        .checks()
        .into_iter()
        .map(
            |c| json!({"check_id": c.check_id, "status": format!("{:?}", c.status).to_uppercase()}),
        )
        .collect();
    let pass = vrun.status == modbit_verification::ReportStatus::Passed
        && vrun.indeterminate_reason.is_none()
        && !checks.is_empty();
    let note = if pass {
        String::new()
    } else if vrun.indeterminate_reason.is_some() {
        "no mandatory check is configured, so the merge cannot be verified (configure .modbit/verification.json)"
            .into()
    } else {
        "the resolved tree failed its mandatory checks".into()
    };
    Verdict {
        pass,
        status: format!("{:?}", vrun.status).to_uppercase(),
        checks,
        run_ref,
        note,
    }
}

/// `git.merge.commit`.
pub(crate) async fn commit(port: &CoreGitState, args: &Value) -> Result<Value, Refusal> {
    let id = args["id"].as_str().unwrap_or_default().to_owned();
    let message = args["message"]
        .as_str()
        .map(str::to_owned)
        .filter(|m| !m.trim().is_empty());
    let mut tx = latest(&*port.store.lock().await, &id).ok_or_else(|| {
        refuse(
            "NO_SUCH_TRANSACTION",
            format!("no merge transaction `{id}`"),
        )
    })?;
    if tx.state == "COMMITTED" {
        return Ok(json!({
            "status": "COMMITTED", "tx_id": tx.id, "result_commit": tx.result,
            "source": tx.source, "target_before": tx.target_before, "replayed": true,
        }));
    }
    if !matches!(tx.state.as_str(), "STAGED" | "CONFLICTED" | "VERIFY_FAILED") {
        return Err(refuse(
            "MERGE_NOT_OPEN",
            format!(
                "transaction `{id}` is {}; only a prepared merge commits",
                tx.state
            ),
        ));
    }
    let root = PathBuf::from(&tx.target_root);
    // Every conflicted file must be resolved: no marker left, then staged.
    let author = format!("{:?}", port.actor);
    let (root2, mut gtx) = (root.clone(), tx.to_git());
    let resolved = tokio::task::spawn_blocking(move || -> Result<(MergeTransaction, Vec<Value>), Refusal> {
        let repo = Repo::open(&root2).map_err(|e| refuse("GIT", e.to_string()))?;
        if !repo.merge_in_progress() {
            return Err(refuse(
                "MERGE_STATE_LOST",
                "git holds no merge in this checkout any more; call git.merge.abort and prepare again",
            ));
        }
        let mut resolutions = Vec::new();
        // Git's own list of what is still unmerged: the log's list may be
        // from before a restart.
        let pending = repo
            .unmerged_paths()
            .map_err(|e| refuse("GIT", e.to_string()))?;
        gtx.conflicts = pending.clone();
        if !pending.is_empty() {
            gtx.state = MergeState::Conflicted;
        }
        let mut unresolved = Vec::new();
        for path in &pending {
            let ev = repo
                .conflict_evidence(path)
                .map_err(|e| refuse("GIT", e.to_string()))?;
            if ev.has_markers {
                unresolved.push(path.clone());
            }
        }
        if !unresolved.is_empty() {
            return Err(refuse(
                "UNRESOLVED_MARKERS",
                format!(
                    "these files still carry conflict markers: {}; edit them to their resolution first",
                    unresolved.join(", ")
                ),
            ));
        }
        for path in &pending {
            let bytes = std::fs::read(root2.join(path)).unwrap_or_default();
            repo.merge_resolve(&mut gtx, path)
                .map_err(|e| refuse("GIT", e.to_string()))?;
            resolutions.push(json!({
                "path": path,
                "sha256": hex::encode(Sha256::digest(&bytes)),
            }));
        }
        Ok((gtx, resolutions))
    })
    .await
    .map_err(|e| refuse("MERGE_FAILED", e.to_string()))??;
    let (_gtx, resolutions) = resolved;
    for r in resolutions {
        let mut r = r;
        r["author"] = json!(author);
        tx.resolutions.push(r);
    }
    tx.conflicts.clear();
    tx.state = "STAGED".into();
    // Post-merge verification: the commit counts only on a pass.
    let verdict = verify(port, &tx, &root).await;
    tx.verdict = Some(if verdict.pass { "PASS" } else { "FAIL" }.into());
    tx.verification_ref = Some(verdict.run_ref.clone());
    let verified = worktrees::ev(
        MERGE_VERIFIED,
        json!({
            "tx_id": tx.id,
            "status": verdict.status,
            "pass": verdict.pass,
            "checks": verdict.checks,
            "verification_ref": verdict.run_ref,
            "note": verdict.note,
        }),
        &port.actor,
    );
    if !verdict.pass {
        tx.state = "VERIFY_FAILED".into();
        record(port, &tx, &verdict.note, vec![verified]).await?;
        return Err(refuse(
            "POST_MERGE_VERIFICATION_FAILED",
            format!(
                "{} ({}); the merge stays staged: fix the files and call git.merge.commit again, or git.merge.abort",
                verdict.note,
                verdict
                    .checks
                    .iter()
                    .map(|c| format!(
                        "{} {}",
                        c["check_id"].as_str().unwrap_or_default(),
                        c["status"].as_str().unwrap_or_default()
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    record(port, &tx, "verification passed", vec![verified]).await?;
    fault_point("MERGE_BEFORE_COMMIT");
    let (root4, mut g2, msg) = (
        root.clone(),
        tx.to_git(),
        message.unwrap_or_else(|| {
            format!(
                "Merge {} into {}",
                if tx.source_label.is_empty() {
                    short(&tx.source).to_owned()
                } else {
                    tx.source_label.clone()
                },
                tx.target_branch
            )
        }),
    );
    let sha = tokio::task::spawn_blocking(move || {
        let repo = Repo::open(&root4).map_err(|e| e.to_string())?;
        repo.merge_commit(&mut g2, &msg).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| refuse("MERGE_FAILED", e.to_string()))?
    .map_err(|e| refuse("MERGE_FAILED", e))?;
    fault_point("MERGE_AFTER_COMMIT");
    tx.state = "COMMITTED".into();
    tx.result = Some(sha.clone());
    let mut extra = Vec::new();
    // The child's work is integrated.
    let mut child_lineage = None;
    if let (Some(wid), Some(ctid)) = (&tx.child_worktree_id, &tx.child_task_id)
        && let Ok(ct) = TaskId::parse(ctid)
    {
        child_lineage = Some((ct, wid.clone()));
    }
    record(port, &tx, "committed", std::mem::take(&mut extra)).await?;
    if let Some((ct, wid)) = child_lineage {
        let mut st = port.store.lock().await;
        let _ = worktrees::append_events(
            &mut st,
            port.tenant,
            port.session,
            ct,
            worktrees::aggregate_id(&wid),
            vec![worktrees::ev(
                worktrees::DISPOSED,
                json!({
                    "worktree_id": wid,
                    "disposition": "MERGED",
                    "tree": null,
                    "by": format!("{:?}", port.actor),
                    "reason": format!("merged into {} by {}", tx.target_branch, tx.id),
                    "merge_id": tx.id,
                }),
                &port.actor,
            )],
        );
    }
    Ok(json!({
        "status": "COMMITTED",
        "tx_id": tx.id,
        "result_commit": sha,
        "source": tx.source,
        "target_before": tx.target_before,
        "base": tx.base,
        "target_branch": tx.target_branch,
        "resolutions": tx.resolutions,
        "verdict": "PASS",
        "verification_ref": tx.verification_ref,
        "checks": verdict.checks,
    }))
}

/// `git.merge.abort`.
pub(crate) async fn abort(port: &CoreGitState, args: &Value) -> Result<Value, Refusal> {
    let discard = args["discard"].as_bool() == Some(true);
    let reason = args["reason"].as_str().unwrap_or_default().to_owned();
    let mut out = json!({"status": "ABORTED"});
    let mut child: Option<(TaskId, String)> = None;
    if let Some(id) = args["id"].as_str() {
        let mut tx = latest(&*port.store.lock().await, id).ok_or_else(|| {
            refuse(
                "NO_SUCH_TRANSACTION",
                format!("no merge transaction `{id}`"),
            )
        })?;
        match tx.state.as_str() {
            "COMMITTED" => {
                return Err(refuse(
                    "MERGE_ALREADY_COMMITTED",
                    format!("transaction `{id}` is committed; it cannot be aborted"),
                ));
            }
            "ABORTED" => {
                out["replayed"] = json!(true);
            }
            _ => {
                let (root, mut g) = (PathBuf::from(&tx.target_root), tx.to_git());
                let before = tx.target_before.clone();
                tokio::task::spawn_blocking(move || -> Result<(), Refusal> {
                    let repo = Repo::open(&root).map_err(|e| refuse("GIT", e.to_string()))?;
                    repo.merge_abort(&mut g)
                        .map_err(|e| refuse("GIT", e.to_string()))?;
                    let head = repo.head().map_err(|e| refuse("GIT", e.to_string()))?;
                    let dirty = repo.status().map_err(|e| refuse("GIT", e.to_string()))?;
                    if head != before || !dirty.is_empty() || repo.merge_in_progress() {
                        return Err(refuse(
                            "ABORT_INCOMPLETE",
                            "the checkout is not back at the pre-merge commit",
                        ));
                    }
                    Ok(())
                })
                .await
                .map_err(|e| refuse("MERGE_FAILED", e.to_string()))??;
                tx.state = "ABORTED".into();
                tx.conflicts.clear();
                record(
                    port,
                    &tx,
                    "aborted: the checkout is back at the pre-merge commit",
                    vec![],
                )
                .await?;
            }
        }
        out["tx_id"] = json!(tx.id);
        out["restored_to"] = json!(tx.target_before);
        if let (Some(wid), Some(ctid)) = (&tx.child_worktree_id, &tx.child_task_id)
            && let Ok(ct) = TaskId::parse(ctid)
        {
            child = Some((ct, wid.clone()));
        }
    }
    if discard {
        if child.is_none()
            && let Some(which) = args["child"].as_str()
        {
            let st = port.store.lock().await;
            let (t, rec, _) = find_child(&st, &port.session, port.task, which)?;
            child = Some((t.task_id, rec.worktree_id));
        }
        let Some((ct, wid)) = child else {
            return Err(refuse(
                "BAD_PAYLOAD",
                "discard needs the child (or a transaction made for one)",
            ));
        };
        let mut st = port.store.lock().await;
        worktrees::append_events(
            &mut st,
            port.tenant,
            port.session,
            ct,
            worktrees::aggregate_id(&wid),
            vec![worktrees::ev(
                worktrees::DISPOSED,
                json!({
                    "worktree_id": wid,
                    "disposition": "DISCARDED",
                    "tree": null,
                    "by": format!("{:?}", port.actor),
                    "reason": if reason.is_empty() { "discarded by the parent".to_owned() } else { reason },
                }),
                &port.actor,
            )],
        )
        .map_err(|e| refuse("STORE", e))?;
        out["discarded_child"] = json!(ct.to_string());
    } else if args["id"].as_str().is_none() {
        return Err(refuse("BAD_PAYLOAD", "id (or child with discard) required"));
    }
    Ok(out)
}

// ---- recovery ----

/// Reconcile every transaction the log holds open with what Git holds, after a
/// restart: no tree is left half merged.
pub(crate) async fn recover(core: &Arc<crate::server::Core>) -> usize {
    let open: Vec<Tx> = {
        let store = core.store.lock().await;
        all(&store).into_iter().filter(Tx::open).collect()
    };
    let mut n = 0;
    for mut tx in open {
        let (Some(task), Some(session)) = (tx.task, tx.session) else {
            continue;
        };
        let root = PathBuf::from(&tx.target_root);
        let (r2, before, source) = (root.clone(), tx.target_before.clone(), tx.source.clone());
        // What Git says: Some((head, in_progress, dirty, merge_commit_has_source)).
        let truth = tokio::task::spawn_blocking(move || {
            let repo = Repo::open(&r2).ok()?;
            let head = repo.head().ok()?;
            let in_progress = repo.merge_in_progress();
            let dirty = !repo.status().ok()?.is_empty();
            let finished = head != before && repo.is_ancestor(&source, &head).unwrap_or(false);
            Some((head, in_progress, dirty, finished))
        })
        .await
        .ok()
        .flatten();
        let (note, state): (String, &str) = match truth {
            None => (
                "the checkout is gone; the transaction is abandoned".into(),
                "ABORTED",
            ),
            Some((head, _, _, true)) => {
                tx.result = Some(head);
                (
                    "git had committed the merge when the Core stopped; recorded now".into(),
                    "COMMITTED",
                )
            }
            Some((_, true, _, false)) => {
                // Git still holds the merge: resumable. Refresh the conflicts.
                let r3 = root.clone();
                let conflicts = tokio::task::spawn_blocking(move || {
                    let repo = Repo::open(&r3).ok()?;
                    repo.unmerged_paths().ok()
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
                tx.conflicts = conflicts;
                (
                    "resumed after a restart: git still holds the merge".into(),
                    if tx.conflicts.is_empty() {
                        "STAGED"
                    } else {
                        "CONFLICTED"
                    },
                )
            }
            Some(_) => {
                // Git does not hold the merge. Whatever the half-run left in
                // the (clean-before) tree is put back to the exact pre-merge
                // commit.
                let (r4, mut g) = (root.clone(), tx.to_git());
                let aborted = tokio::task::spawn_blocking(move || {
                    let repo = Repo::open(&r4).ok()?;
                    repo.merge_abort(&mut g).ok()
                })
                .await
                .ok()
                .flatten();
                if aborted.is_some() {
                    (
                        "the Core stopped part-way through the merge; the checkout was restored to the pre-merge commit".into(),
                        "ABORTED",
                    )
                } else {
                    (
                        "the Core stopped part-way through the merge and the checkout could not be restored".into(),
                        "VERIFY_FAILED",
                    )
                }
            }
        };
        tx.state = state.into();
        let mut store = core.store.lock().await;
        let _ = worktrees::append_events(
            &mut store,
            core.tenant_id,
            session,
            task,
            agg(&tx.id),
            vec![worktrees::ev(
                MERGE_TX,
                tx.payload(&note),
                &Actor::Core("merge-recovery".into()),
            )],
        );
        n += 1;
    }
    if n > 0 {
        let offset = core.store.lock().await.last_offset().unwrap_or(0);
        core.last_offset.send_replace(offset);
    }
    n
}
