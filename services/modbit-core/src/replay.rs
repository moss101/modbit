//! Isolated counterfactual replay (EPR-011, REQ-EPR-011; docs/38
//! "CounterfactualReplay", docs/27 §10.2 and §12.3).
//!
//! A request's first run pins the repository it started from: the worktree,
//! dirty state included, as a commit under `refs/modbit/snapshots/<id>`
//! (`RequestSnapshotRecorded`) — immutable, and HEAD, the index and the files
//! untouched. Replaying an alternative validated plan of that request is
//! offline and isolated:
//!
//! 1. admitted only when the policy in force allows replay (`eval.replay`
//!    ALLOW: privacy and retention approval), for a plan the request's own
//!    decision record holds as a hard-eligible alternative, under the
//!    registry generation it was decided in, from a snapshot that still
//!    resolves to the same commit — anything else is refused, never
//!    approximated;
//! 2. run in a scratch repository fetched from the snapshot — never a
//!    worktree or a branch of the original — with credential-bearing files
//!    removed, no remote to push to, and the replay-only ceiling
//!    (`review_isolated`: writes and processes in the scratch tree only, the
//!    host sandbox confining them, no egress, no secret, no commit, no push,
//!    no external call); provider inference goes through the existing
//!    Gateway, whose credentials never reach the replay;
//! 3. recorded as its own task and on the request (`CounterfactualReplayStarted`);
//!    the request's outcome record reads the replay's outcome as an
//!    *observed* counterfactual, kept apart from the *estimated* one.

use std::path::{Path, PathBuf};
use std::process::Command;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::lease::CapabilityLeaseEvent;
use modbit_domain::task::{Task, TaskEvent, TaskOrigin};
use modbit_domain::toolcall::EffectClass;
use modbit_domain::{CapabilityLeaseId, TaskId};
use modbit_event_store::AppendRequest;
use modbit_protocol::v1 as wire;

use crate::runtime::{Lineage, StartConfig, append, typed};
use crate::server::Core;

fn refuse(code: &str, detail: impl Into<String>) -> (String, String) {
    (code.to_owned(), detail.into())
}

/// Credential-bearing files a replay's scratch copy never carries (the
/// credential subset of docs/23's protected paths).
const SANITIZED: &[&str] = &[
    "**/.ssh/**",
    "**/id_rsa*",
    "**/id_ed25519*",
    "**/*.pem",
    "**/.netrc",
    "**/.npmrc",
    "**/.pypirc",
    "**/.aws/credentials",
    "**/.env",
    "**/.env.*",
];

/// Pin the repository a request starts from, once per request, before its
/// first round. A workspace that is not a Git repository has no snapshot and
/// cannot be replayed.
pub(crate) async fn capture(core: &Core, task: &Task, lt: Lineage, actor: &Actor) {
    let Some(root) = task.workspace_root.as_deref() else {
        return;
    };
    let already = {
        let store = core.store.lock().await;
        store
            .read_aggregate(task.task_id.as_bytes(), 0, usize::MAX)
            .unwrap_or_default()
            .iter()
            .any(|e| e.envelope.event_type == "RequestSnapshotRecorded")
    };
    if already {
        return;
    }
    let Ok(repo) = modbit_git::Repo::open(Path::new(root)) else {
        return;
    };
    let Ok(snap) = repo.snapshot_dirty(&format!("request-{}", task.task_id)) else {
        return;
    };
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "RequestSnapshotRecorded",
            &TaskEvent::RequestSnapshotRecorded {
                snapshot_ref: snap.reference,
                commit: snap.commit,
                tree: snap.tree,
                parent: snap.parent,
                dirty_paths: snap.paths,
            },
            actor.clone(),
        )],
    );
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// Every file under `dir` (not `.git`), root-relative with `/`.
fn files(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, d: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if p.is_dir() {
                walk(root, &p, out);
            } else if let Ok(r) = p.strip_prefix(root) {
                out.push(r.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The scratch repository: fetched from the snapshot's ref into a fresh
/// repository (the original gains nothing — no worktree, no branch), checked
/// out detached at the snapshot commit, with no remote and without the
/// credential-bearing files.
fn scratch(
    root: &Path,
    snapshot_ref: &str,
    commit: &str,
    dir: &Path,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    git(dir, &["init", "-q", "-b", "replay"])?;
    git(dir, &["config", "core.autocrlf", "false"])?;
    let source = root.to_string_lossy().into_owned();
    git(dir, &["fetch", "-q", "--no-tags", &source, snapshot_ref])?;
    let fetched = git(dir, &["rev-parse", "FETCH_HEAD"])?;
    if fetched != commit {
        return Err(format!(
            "the snapshot ref resolves to {fetched}, not the recorded {commit}"
        ));
    }
    git(dir, &["checkout", "-q", "--detach", commit])?;
    let mut builder = globset::GlobSetBuilder::new();
    for p in SANITIZED {
        if let Ok(g) = globset::Glob::new(p) {
            builder.add(g);
        }
    }
    let set = builder.build().map_err(|e| e.to_string())?;
    let mut removed = Vec::new();
    for f in files(dir) {
        if set.is_match(&f) || set.is_match(format!("./{f}")) {
            let _ = std::fs::remove_file(dir.join(&f));
            removed.push(f);
        }
    }
    Ok(removed)
}

/// `ReplayCounterfactual`: replay the alternative `plan_id` of a request.
#[allow(clippy::too_many_lines)]
pub(crate) async fn start(
    core: &std::sync::Arc<Core>,
    task_id: TaskId,
    plan_id: &str,
    lease_generation: u64,
    actor: Actor,
) -> Result<wire::CounterfactualReplayView, (String, String)> {
    let (task, events) = {
        let store = core.store.lock().await;
        let task = store
            .task(&task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_TASK", task_id.to_string()))?;
        let mut events = Vec::new();
        for e in store
            .read_session(&task.session_id, 0, usize::MAX)
            .unwrap_or_default()
        {
            if e.envelope.task_id == Some(task_id)
                && let Ok(p) = store.payload(&e.envelope)
            {
                events.push((e.envelope.event_type.clone(), p));
            }
        }
        (task, events)
    };
    if task.origin == TaskOrigin::Replay {
        return Err(refuse("REPLAY_OF_REPLAY", "a replay is not replayed"));
    }
    // 1. Admission: the policy in force approves replaying this request's
    //    trajectory (privacy and retention).
    let cfg =
        core.tools
            .configurations
            .for_task(task_id, &core.data_dir, task.workspace_root.as_deref());
    let admitted = cfg
        .permissions
        .get("eval.replay")
        .is_some_and(|p| p.value == modbit_policy::config::Permission::Allow);
    if !admitted {
        return Err(refuse(
            "REPLAY_NOT_ADMITTED",
            "the policy in force does not allow replaying this request (`eval.replay` is not ALLOW)",
        ));
    }
    // 2. The snapshot: recorded, and still the same commit.
    let Some((_, snap)) = events
        .iter()
        .rev()
        .find(|(t, _)| t == "RequestSnapshotRecorded")
    else {
        return Err(refuse(
            "REPLAY_NO_SNAPSHOT",
            "the request has no recorded repository snapshot",
        ));
    };
    let snapshot_ref = snap["snapshot_ref"].as_str().unwrap_or_default().to_owned();
    let commit = snap["commit"].as_str().unwrap_or_default().to_owned();
    let root = PathBuf::from(task.workspace_root.clone().unwrap_or_default());
    match git(&root, &["rev-parse", "--verify", "-q", &snapshot_ref]) {
        Ok(found) if found == commit => {}
        Ok(found) => {
            return Err(refuse(
                "REPLAY_REVISION_MISMATCH",
                format!("{snapshot_ref} is {found}, not the recorded {commit}"),
            ));
        }
        Err(_) => {
            return Err(refuse(
                "REPLAY_REVISION_MISMATCH",
                format!("{snapshot_ref} no longer resolves in {}", root.display()),
            ));
        }
    }
    // 3. The alternative: a hard-eligible candidate of the request's own
    //    decision, not the plan it ran.
    let Some((_, decision)) = events.iter().find(|(t, _)| t == "RoutingDecisionRecorded") else {
        return Err(refuse(
            "REPLAY_NO_DECISION",
            "the request has no decision record to take an alternative from",
        ));
    };
    let chosen = decision["plan_id"].as_str().unwrap_or_default();
    if plan_id == chosen {
        return Err(refuse(
            "REPLAY_PLAN_CHOSEN",
            "that is the plan the request ran: its outcome is the factual one",
        ));
    }
    let Some(candidate) = decision["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["plan_id"] == plan_id)
    else {
        return Err(refuse(
            "REPLAY_PLAN_UNKNOWN",
            format!("`{plan_id}` is not a candidate of the request's decision"),
        ));
    };
    if candidate["hard_eligible"] != true {
        return Err(refuse(
            "REPLAY_PLAN_INELIGIBLE",
            format!("`{plan_id}` was not hard eligible when the request was decided"),
        ));
    }
    let bindings: Vec<String> = candidate["bindings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b.as_str().map(str::to_owned))
        .collect();
    let Some((endpoint, model)) = bindings.first().and_then(|b| b.split_once('/')) else {
        return Err(refuse(
            "REPLAY_PLAN_UNKNOWN",
            "the candidate names no binding",
        ));
    };
    let (endpoint, model) = (endpoint.to_owned(), model.to_owned());
    // 4. Fresh: the decision's registry generation is the one in force, so
    //    the alternative is what it was when the request was decided.
    let decided_under = events
        .iter()
        .find(|(t, p)| t == "RoutingPlanCompiled" && p["plan"]["plan_id"] == chosen)
        .map(|(_, p)| {
            p["plan"]["provenance"]["registry_generation"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .unwrap_or_default();
    let stats_version = events
        .iter()
        .find(|(t, p)| t == "RoutingPlanAdmitted" && p["plan_id"] == chosen)
        .map(|(_, p)| p["stats_version"].as_str().unwrap_or_default().to_owned())
        .unwrap_or_default();
    let current = crate::model_registry::for_task(core, &task)
        .map(|r| r.generation().to_owned())
        .unwrap_or_default();
    if decided_under.is_empty() || decided_under != current {
        return Err(refuse(
            "REPLAY_STALE",
            format!(
                "the request was decided under registry `{decided_under}`; `{current}` is active, so the alternative is not what it was"
            ),
        ));
    }
    // 5. Isolation: the host sandbox confines the replay's processes.
    let sandbox = crate::review_env::probe_sandbox(core).await?;
    let replay_id = CapabilityLeaseId::new().to_string();
    let dir = core.data_dir.join("replays").join(&replay_id);
    let workspace = dir.join("workspace");
    let sanitized = match scratch(&root, &snapshot_ref, &commit, &workspace) {
        Ok(s) => s,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(refuse("REPLAY_SCRATCH_FAILED", e));
        }
    };
    let workspace = workspace
        .canonicalize()
        .unwrap_or(workspace)
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    // 6. The replay task: the request's goal, the scratch tree, the
    //    replay-only ceiling and a lease that reaches nothing else.
    let replay_task_id = TaskId::new();
    let lease_id = CapabilityLeaseId::new();
    let profile = modbit_policy::kernel::PROFILE_REVIEW_ISOLATED.to_owned();
    {
        let mut store = core.store.lock().await;
        let created = store
            .append(AppendRequest {
                tenant_id: core.tenant_id,
                session_id: task.session_id,
                task_id: Some(replay_task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *replay_task_id.as_bytes(),
                expected_sequence: Some(0),
                events: vec![
                    typed(
                        "TaskCreated",
                        &TaskEvent::TaskCreated {
                            session_id: task.session_id,
                            goal_text: task.goal_text.clone(),
                            workspace_id: modbit_domain::WorkspaceId::new(),
                            workspace_root: Some(workspace.clone()),
                            base_revision: Some(commit.clone()),
                            execution_profile: profile.clone(),
                            policy_profile_id: None,
                            origin: TaskOrigin::Replay,
                        },
                        actor.clone(),
                    ),
                    typed("TaskQueued", &TaskEvent::TaskQueued, actor.clone()),
                ],
            })
            .and_then(|_| {
                store.append(AppendRequest {
                    tenant_id: core.tenant_id,
                    session_id: task.session_id,
                    task_id: Some(replay_task_id),
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
                            task_id: replay_task_id,
                            agent_id: None,
                            resources: vec![
                                format!("fs.read:{workspace}/**"),
                                format!("fs.write:{workspace}/**"),
                                format!("shell.exec:{workspace}/**"),
                                format!("git.read:{workspace}/**"),
                            ],
                            operations: vec![
                                "fs.read".into(),
                                "fs.write".into(),
                                "shell.exec".into(),
                                "git.read".into(),
                            ],
                            effect_ceiling: EffectClass::ReversibleWrite,
                            execution_profile: profile.clone(),
                            generation: 1,
                            expires_at: None,
                        },
                        actor.clone(),
                    )],
                })
            });
        if let Err(e) = created {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(refuse("STORE", e.to_string()));
        }
        let _ = append(
            &mut store,
            core,
            Lineage::task(core.tenant_id, task.session_id, task_id),
            AggregateType::Task,
            *task_id.as_bytes(),
            vec![typed(
                "CounterfactualReplayStarted",
                &TaskEvent::CounterfactualReplayStarted {
                    replay_id: replay_id.clone(),
                    replay_task_id,
                    plan_id: plan_id.to_owned(),
                    bindings: bindings.clone(),
                    snapshot_commit: commit.clone(),
                    scratch: workspace.clone(),
                    sanitized: sanitized.clone(),
                    registry_generation: current.clone(),
                    stats_version: stats_version.clone(),
                    capability_ceiling: profile.clone(),
                },
                actor.clone(),
            )],
        );
    }
    // 7. Run it, pinned to the alternative's binding, through the Gateway.
    let replay_task = core
        .store
        .lock()
        .await
        .task(&replay_task_id)
        .ok()
        .flatten()
        .ok_or_else(|| refuse("STORE", "the replay task is gone"))?;
    let cfg = StartConfig {
        endpoint: endpoint.clone(),
        model: model.clone(),
        budgets: modbit_core_runtime::Budgets {
            max_turns: 20,
            max_tool_calls: 80,
            max_consecutive_no_progress_turns: 4,
        },
        pinned: true,
        plan_id: String::new(),
        slot_id: String::new(),
        skills: vec![],
        lease_generation,
        ticket_id: String::new(),
    };
    let replay_actor = Actor::Agent(format!("replay:{replay_task_id}"));
    core.runtime
        .start_boxed(core, replay_task, cfg, lease_generation, replay_actor)
        .await?;
    Ok(wire::CounterfactualReplayView {
        replay_id,
        replay_task_id: Some(crate::server::wire_id(replay_task_id.as_bytes())),
        plan_id: plan_id.to_owned(),
        bindings,
        snapshot_commit: commit,
        scratch: workspace,
        sanitized,
        capability_ceiling: profile,
        sandbox,
        registry_generation: current,
    })
}
