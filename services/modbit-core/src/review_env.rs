//! The Isolated Non-Committing Reviewer's environment (EPR-018, docs/27
//! §9.5, docs/21 `review_isolated`, docs/49): a disposable review
//! environment on a candidate revision, made of things that already have
//! owners — a scratch git worktree (the worktree owner), a capability lease
//! confined to it (the Capability Kernel), the host's real process sandbox
//! (the terminal broker: no network, no inherited or secret-looking
//! environment, writes confined where the host can confine them), and its
//! own task under the `review_isolated` profile that carries nothing of the
//! solver's reasoning. The canonical tree is never written: no lease names
//! it for writing, the review task's workspace is the scratch tree, and the
//! profile holds no `git.worktree`, commit, push, deploy or network
//! capability. A host without a sandbox admits no environment. Disposal
//! ends the environment's processes, revokes its lease and removes the
//! worktree; evidence the Core extracted before that stays on the log.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::lease::CapabilityLeaseEvent;
use modbit_domain::task::{Task, TaskEvent, TaskOrigin, TaskState};
use modbit_domain::toolcall::EffectClass;
use modbit_domain::{CapabilityLeaseId, TaskId};
use modbit_event_store::AppendRequest;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// An admitted review environment.
#[derive(Clone, Debug)]
pub(crate) struct ReviewEnvironment {
    pub env_id: String,
    pub candidate_task_id: TaskId,
    pub review_task_id: TaskId,
    pub worktree: String,
    pub branch: String,
    pub revision: String,
    pub lease_id: CapabilityLeaseId,
    pub sandbox: String,
}

/// What disposal did.
#[derive(Clone, Debug, Default)]
pub(crate) struct Disposed {
    pub killed: u32,
    pub worktree_removed: bool,
}

/// The host's review sandbox, asked of the live terminal broker.
pub(crate) async fn probe_sandbox(core: &Core) -> Result<String, (String, String)> {
    let Some(execd) = core.tools.execd.as_ref() else {
        return Err((
            "SANDBOX_UNAVAILABLE".into(),
            "no terminal broker: a review process cannot be confined here".into(),
        ));
    };
    let mut client =
        modbit_terminal::ExecClient::connect(&execd.target.endpoint, &execd.target.boot_secret)
            .await
            .map_err(|e| ("SANDBOX_UNAVAILABLE".to_owned(), e.to_string()))?;
    client
        .probe_sandbox()
        .await
        .map_err(|e| ("SANDBOX_UNAVAILABLE".to_owned(), e.to_string()))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let ev = tokio::time::timeout(
            deadline.saturating_duration_since(std::time::Instant::now()),
            client.next(),
        )
        .await
        .map_err(|_| {
            (
                "SANDBOX_UNAVAILABLE".to_owned(),
                "the broker did not answer the sandbox probe".to_owned(),
            )
        })?
        .map_err(|e| ("SANDBOX_UNAVAILABLE".to_owned(), e.to_string()))?;
        match ev {
            Some(modbit_terminal::Event::SandboxProbed(p)) => {
                return if p.available {
                    Ok(p.kind)
                } else {
                    Err(("SANDBOX_UNAVAILABLE".into(), p.detail))
                };
            }
            Some(_) => continue,
            None => {
                return Err((
                    "SANDBOX_UNAVAILABLE".into(),
                    "the broker closed before answering the sandbox probe".into(),
                ));
            }
        }
    }
}

/// Admit a review environment for `candidate` at `revision` (its HEAD when
/// empty). Everything taken is recorded on the candidate's log and the
/// review task's own; a failure after the worktree removes it.
pub(crate) async fn admit(
    core: &Arc<Core>,
    candidate: &Task,
    revision: &str,
    actor: &Actor,
) -> Result<ReviewEnvironment, (String, String)> {
    let Some(root) = candidate.workspace_root.clone() else {
        return Err((
            "NO_WORKSPACE".into(),
            "the candidate task has no repository".into(),
        ));
    };
    // 1. The host's sandbox, or nothing (docs/49: an unsupported platform
    //    admits no review slot).
    let sandbox = probe_sandbox(core).await?;
    // 2. A scratch worktree at the candidate revision, on its own branch.
    let repo = modbit_git::Repo::open(std::path::Path::new(&root))
        .map_err(|e| ("NO_REPOSITORY".to_owned(), e.to_string()))?;
    let revision = if revision.trim().is_empty() {
        repo.head()
            .map_err(|e| ("NO_REPOSITORY".to_owned(), e.to_string()))?
    } else {
        revision.trim().to_owned()
    };
    let env_id = modbit_domain::CapabilityLeaseId::new().to_string();
    let branch = format!("modbit/review-{env_id}");
    repo.create_branch(&branch, &revision)
        .map_err(|e| ("BAD_REVISION".to_owned(), e.to_string()))?;
    let worktree_dir: PathBuf = core.data_dir.join("review").join(&env_id);
    if let Some(parent) = worktree_dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let scratch = match repo.worktree_add(&worktree_dir, &branch) {
        Ok(r) => r,
        Err(e) => {
            return Err(("WORKTREE_FAILED".into(), e.to_string()));
        }
    };
    let worktree = scratch
        .dir()
        .canonicalize()
        .unwrap_or_else(|_| worktree_dir.clone())
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    // 3. The review task: carries nothing, works the scratch tree only.
    let review_task_id = TaskId::new();
    let lease_id = CapabilityLeaseId::new();
    let canonical = std::path::Path::new(&root)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&root))
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let created = {
        let mut store = core.store.lock().await;
        let task_events = vec![
            typed(
                "TaskCreated",
                &TaskEvent::TaskCreated {
                    session_id: candidate.session_id,
                    goal_text: format!(
                        "Review the candidate of task {} at revision {revision}: gather evidence in the disposable review tree; propose nothing canonical",
                        candidate.task_id
                    ),
                    workspace_id: modbit_domain::WorkspaceId::new(),
                    workspace_root: Some(worktree.clone()),
                    base_revision: Some(revision.clone()),
                    execution_profile: modbit_policy::kernel::PROFILE_REVIEW_ISOLATED.to_owned(),
                    policy_profile_id: None,
                    origin: TaskOrigin::Review,
                },
                actor.clone(),
            ),
            typed("TaskQueued", &TaskEvent::TaskQueued, actor.clone()),
            typed(
                "ReviewEnvironmentBound",
                &TaskEvent::ReviewEnvironmentBound {
                    env_id: env_id.clone(),
                    candidate_task_id: candidate.task_id,
                    worktree: worktree.clone(),
                    revision: revision.clone(),
                },
                actor.clone(),
            ),
        ];
        let r1 = store.append(AppendRequest {
            tenant_id: core.tenant_id,
            session_id: candidate.session_id,
            task_id: Some(review_task_id),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Task,
            aggregate_id: *review_task_id.as_bytes(),
            expected_sequence: Some(0),
            events: task_events,
        });
        // The lease: reads of the canonical and scratch trees, writes and
        // processes inside the scratch tree only; no worktree, no git
        // write, ceiling REVERSIBLE_WRITE.
        let r2 = r1.and_then(|_| {
            store.append(AppendRequest {
                tenant_id: core.tenant_id,
                session_id: candidate.session_id,
                task_id: Some(review_task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::CapabilityLease,
                aggregate_id: *lease_id.as_bytes(),
                expected_sequence: Some(0),
                events: vec![typed(
                    "CapabilityLeaseGranted",
                    &CapabilityLeaseEvent::CapabilityLeaseGranted {
                        tenant_id: core.tenant_id,
                        task_id: review_task_id,
                        agent_id: None,
                        resources: vec![
                            format!("fs.read:{canonical}/**"),
                            format!("fs.read:{worktree}/**"),
                            format!("fs.write:{worktree}/**"),
                            format!("shell.exec:{worktree}/**"),
                            format!("git.read:{worktree}/**"),
                        ],
                        operations: vec![
                            "fs.read".into(),
                            "fs.write".into(),
                            "shell.exec".into(),
                            "git.read".into(),
                        ],
                        effect_ceiling: EffectClass::ReversibleWrite,
                        execution_profile: modbit_policy::kernel::PROFILE_REVIEW_ISOLATED
                            .to_owned(),
                        generation: 1,
                        expires_at: None,
                    },
                    actor.clone(),
                )],
            })
        });
        r2.map(|ev| ev.last().map(|e| e.offset).unwrap_or(0))
    };
    let offset = match created {
        Ok(o) => o,
        Err(e) => {
            let _ = repo.worktree_remove(&worktree_dir);
            return Err(("STORE".into(), e.to_string()));
        }
    };
    core.last_offset.send_replace(offset);
    // 4. The record on the candidate's log.
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            Lineage::task(core.tenant_id, candidate.session_id, candidate.task_id),
            AggregateType::Task,
            *candidate.task_id.as_bytes(),
            vec![typed(
                "ReviewEnvironmentAdmitted",
                &TaskEvent::ReviewEnvironmentAdmitted {
                    env_id: env_id.clone(),
                    review_task_id,
                    worktree: worktree.clone(),
                    branch: branch.clone(),
                    revision: revision.clone(),
                    lease_id,
                    sandbox: sandbox.clone(),
                },
                actor.clone(),
            )],
        );
    }
    Ok(ReviewEnvironment {
        env_id,
        candidate_task_id: candidate.task_id,
        review_task_id,
        worktree,
        branch,
        revision,
        lease_id,
        sandbox,
    })
}

/// The environment `env_id` stands for on `candidate`'s log, if it was
/// admitted and not yet disposed.
pub(crate) fn find(
    store: &modbit_event_store::EventStore,
    env_id: &str,
) -> Option<ReviewEnvironment> {
    let mut found: Option<ReviewEnvironment> = None;
    for t in store.unfinished_tasks().unwrap_or_default() {
        let events = store
            .read_aggregate(t.task_id.as_bytes(), 0, usize::MAX)
            .unwrap_or_default();
        for e in &events {
            let p = store.payload(&e.envelope).unwrap_or_default();
            match e.envelope.event_type.as_str() {
                "ReviewEnvironmentAdmitted" if p["env_id"] == env_id => {
                    found = Some(ReviewEnvironment {
                        env_id: env_id.to_owned(),
                        candidate_task_id: t.task_id,
                        review_task_id: p["review_task_id"]
                            .as_str()
                            .and_then(|s| TaskId::parse(s).ok())
                            .unwrap_or_default(),
                        worktree: p["worktree"].as_str().unwrap_or_default().to_owned(),
                        branch: p["branch"].as_str().unwrap_or_default().to_owned(),
                        revision: p["revision"].as_str().unwrap_or_default().to_owned(),
                        lease_id: p["lease_id"]
                            .as_str()
                            .and_then(|s| CapabilityLeaseId::parse(s).ok())
                            .unwrap_or_default(),
                        sandbox: p["sandbox"].as_str().unwrap_or_default().to_owned(),
                    });
                }
                "ReviewEnvironmentDisposed" if p["env_id"] == env_id => found = None,
                _ => {}
            }
        }
        if found.is_some() {
            break;
        }
    }
    found
}

/// Dispose an environment: end its processes, revoke its lease, cancel its
/// task, remove its worktree and branch, and say so on the candidate's log.
pub(crate) async fn dispose(
    core: &Arc<Core>,
    env: &ReviewEnvironment,
    reason: &str,
    actor: &Actor,
) -> Disposed {
    let mut out = Disposed::default();
    // 1. The review task's loop, if any, then every broker session whose
    //    working directory is inside the scratch tree.
    let _ = core.runtime.cancel(&env.review_task_id).await;
    if let Some(execd) = core.tools.execd.as_ref()
        && let Ok(mut client) =
            modbit_terminal::ExecClient::connect(&execd.target.endpoint, &execd.target.boot_secret)
                .await
        && client.list().await.is_ok()
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut sessions = Vec::new();
        while let Ok(Ok(Some(ev))) = tokio::time::timeout(
            deadline.saturating_duration_since(std::time::Instant::now()),
            client.next(),
        )
        .await
        {
            if let modbit_terminal::Event::Sessions(list) = ev {
                sessions = list;
                break;
            }
        }
        for s in sessions
            .iter()
            .filter(|s| s.running && s.cwd.starts_with(&env.worktree))
        {
            if client.cancel(&s.session_id).await.is_ok() {
                out.killed += 1;
            }
        }
        // Give the broker a moment to end them.
        if out.killed > 0 {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
    // 2. The lease.
    {
        let mut store = core.store.lock().await;
        let session_id = core_session(&store, env);
        let _ = store.append(AppendRequest {
            tenant_id: core.tenant_id,
            session_id,
            task_id: Some(env.review_task_id),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::CapabilityLease,
            aggregate_id: *env.lease_id.as_bytes(),
            expected_sequence: None,
            events: vec![typed(
                "CapabilityLeaseRevoked",
                &CapabilityLeaseEvent::CapabilityLeaseRevoked {
                    reason: format!("review environment {} disposed: {reason}", env.env_id),
                },
                actor.clone(),
            )],
        });
        // The review task ends if it has not.
        if let Ok(Some(t)) = store.task(&env.review_task_id)
            && !matches!(
                t.state,
                TaskState::Completed | TaskState::Failed | TaskState::Cancelled
            )
        {
            let _ = store.append(AppendRequest {
                tenant_id: core.tenant_id,
                session_id: t.session_id,
                task_id: Some(t.task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *t.task_id.as_bytes(),
                expected_sequence: None,
                events: vec![typed(
                    "TaskCancelled",
                    &TaskEvent::TaskCancelled,
                    actor.clone(),
                )],
            });
        }
    }
    // 3. The worktree and its branch.
    let candidate = {
        let store = core.store.lock().await;
        store.task(&env.candidate_task_id).ok().flatten()
    };
    if let Some(root) = candidate.as_ref().and_then(|c| c.workspace_root.clone())
        && let Ok(repo) = modbit_git::Repo::open(std::path::Path::new(&root))
    {
        let wt = std::path::Path::new(&env.worktree);
        out.worktree_removed = repo.worktree_remove(wt).is_ok() || !wt.exists();
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["branch", "-D", &env.branch])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    if !out.worktree_removed && !std::path::Path::new(&env.worktree).exists() {
        out.worktree_removed = true;
    }
    // 4. The record.
    if let Some(c) = candidate {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            Lineage::task(core.tenant_id, c.session_id, c.task_id),
            AggregateType::Task,
            *c.task_id.as_bytes(),
            vec![typed(
                "ReviewEnvironmentDisposed",
                &TaskEvent::ReviewEnvironmentDisposed {
                    env_id: env.env_id.clone(),
                    killed: out.killed,
                    worktree_removed: out.worktree_removed,
                    reason: reason.to_owned(),
                },
                actor.clone(),
            )],
        );
    }
    out
}

fn core_session(
    store: &modbit_event_store::EventStore,
    env: &ReviewEnvironment,
) -> modbit_domain::SessionId {
    store
        .task(&env.review_task_id)
        .ok()
        .flatten()
        .map(|t| t.session_id)
        .unwrap_or_default()
}

/// The environment a review task runs in, from its own log.
pub(crate) fn bound_to(
    store: &modbit_event_store::EventStore,
    review_task: &Task,
) -> Option<(String, TaskId, String, String)> {
    let events = store
        .read_aggregate(review_task.task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    events.iter().find_map(|e| {
        if e.envelope.event_type != "ReviewEnvironmentBound" {
            return None;
        }
        let p = store.payload(&e.envelope).ok()?;
        Some((
            p["env_id"].as_str()?.to_owned(),
            TaskId::parse(p["candidate_task_id"].as_str()?).ok()?,
            p["worktree"].as_str().unwrap_or_default().to_owned(),
            p["revision"].as_str().unwrap_or_default().to_owned(),
        ))
    })
}

/// The candidate's current workspace revision, from its latest completion
/// verification record (the gate's revision), or 0.
pub(crate) fn candidate_revision_of(
    store: &modbit_event_store::EventStore,
    task_id: TaskId,
) -> u64 {
    let events = store
        .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    let mut rev = 0u64;
    for e in &events {
        if e.envelope.event_type == "ReviewEnvironmentAdmitted" {
            continue;
        }
    }
    if let Ok(session) = store.task(&task_id).map(|t| t.map(|t| t.session_id))
        && let Some(session) = session
        && let Ok(all) = store.read_session(&session, 0, usize::MAX)
    {
        for e in all.iter().rev() {
            if e.envelope.task_id == Some(task_id)
                && e.envelope.event_type == "AcceptanceGateEvaluated"
                && let Ok(p) = store.payload(&e.envelope)
            {
                rev = p["candidate_revision"].as_u64().unwrap_or(0);
                break;
            }
        }
    }
    let _ = events;
    rev
}
