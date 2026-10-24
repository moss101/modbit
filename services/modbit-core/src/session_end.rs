//! `EndSession` (REQ-PX-049, AFW-J11): the person is quitting.
//!
//! The desktop stops its Core when it quits, so anything that must happen
//! "at the end of the session" has to happen in the Core, before the host
//! tears it down, and the host has to wait for it. This is that step:
//!
//! * Every `session_end` hook in force for the session's tasks runs to
//!   completion, each under its own timeout, through the one Hook Bus
//!   (`hooks::HookScope::fire`). A hook that fails, times out or answers
//!   nonsense is on the task's log as a `HookInvoked` entry and is in the
//!   answer, so the host can show it; nothing fails silently because the
//!   host was already gone.
//! * The person's resume choice is recorded on the session log
//!   (`SessionEnding`), fenced by the session lease. It is a Core input:
//!   after the next start, [`resume_requests`] names the tasks recovery
//!   suspended that the person asked to have resumed, and the client resumes
//!   them with its ordinary `StartTask`. Nothing resumes without a client
//!   (and its provider) being there, and an old choice never applies twice.

use std::collections::{HashMap, HashSet};

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::session::SessionEvent;
use modbit_domain::task::TaskState;
use modbit_domain::{SessionId, TaskId};
use modbit_event_store::EventStore;
use modbit_protocol::v1 as wire;
use modbit_tools::hooks::HookPoint;

use crate::runtime::{Lineage, append, typed};
use crate::server::{Core, wire_id};

/// Run the session's `session_end` hooks and record the person's choice.
pub(crate) async fn end_session(
    core: &Core,
    session_id: SessionId,
    resume_on_reopen: bool,
    lease_generation: Option<u64>,
    actor: Actor,
) -> Result<wire::SessionEndedView, (String, String)> {
    let tasks = core
        .store
        .lock()
        .await
        .tasks_for_session(&session_id)
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    let running: Vec<TaskId> = tasks
        .iter()
        .filter(|t| t.state == TaskState::Running)
        .map(|t| t.task_id)
        .collect();
    // Hooks are declared per configuration scope, and a project's are the
    // workspace's: one run of the hooks per distinct workspace, through the
    // session's newest task there (its log carries the journal entries).
    let mut by_root: Vec<(Option<String>, &modbit_domain::task::Task)> = Vec::new();
    for t in &tasks {
        match by_root
            .iter_mut()
            .find(|(root, _)| *root == t.workspace_root)
        {
            Some(slot) => slot.1 = t,
            None => by_root.push((t.workspace_root.clone(), t)),
        }
    }
    let payload = serde_json::json!({
        "session_id": session_id.to_string(),
        "reason": "quit",
        "resume_on_reopen": resume_on_reopen,
        "running_tasks": running.iter().map(ToString::to_string).collect::<Vec<_>>(),
    });
    let mut runs = Vec::new();
    for (_, task) in by_root {
        let (scope, _) =
            crate::hooks::scope(core, task, None, None, lease_generation, &actor).await;
        if !scope.has(HookPoint::SessionEnd) {
            continue;
        }
        let effect = scope
            .fire(HookPoint::SessionEnd, None, payload.clone())
            .await;
        runs.extend(effect.records.into_iter().map(|r| wire::SessionEndHookRun {
            task_id: Some(wire_id(task.task_id.as_bytes())),
            hook: r.hook,
            outcome: r.outcome.label().to_owned(),
            duration_ms: r.duration_ms,
            detail: r.detail.chars().take(2_000).collect(),
        }));
    }
    let hooks_failed = u32::try_from(
        runs.iter()
            .filter(|r| matches!(r.outcome.as_str(), "TIMEOUT" | "FAILED" | "MALFORMED"))
            .count(),
    )
    .unwrap_or(u32::MAX);
    let offset = {
        let mut store = core.store.lock().await;
        let mut lineage = Lineage::session(core.tenant_id, session_id);
        if let Some(g) = lease_generation {
            lineage = lineage.fenced(g);
        }
        append(
            &mut store,
            core,
            lineage,
            AggregateType::Session,
            *session_id.as_bytes(),
            vec![typed(
                "SessionEnding",
                &SessionEvent::SessionEnding {
                    running_tasks: running.iter().map(ToString::to_string).collect(),
                    resume_on_reopen,
                    hooks_run: u32::try_from(runs.len()).unwrap_or(u32::MAX),
                    hooks_failed,
                },
                actor,
            )],
        )
        .map_err(|e| ("STORE".to_owned(), e))?
    };
    Ok(wire::SessionEndedView {
        hooks: runs,
        resume_tasks: if resume_on_reopen {
            running.iter().map(|t| wire_id(t.as_bytes())).collect()
        } else {
            Vec::new()
        },
        offset,
    })
}

/// After a restart: of the tasks recovery just suspended, the ones whose
/// session's latest `SessionEnding` asked for a resume and which nothing has
/// started or resumed since (a choice is spent by the resume it asked for).
pub(crate) fn resume_requests(store: &EventStore, suspended: &[TaskId]) -> Vec<TaskId> {
    let mut out = Vec::new();
    let mut seen: HashSet<SessionId> = HashSet::new();
    let mut latest: HashMap<SessionId, (u64, Vec<String>, bool)> = HashMap::new();
    for t in suspended {
        let Ok(Some(task)) = store.task(t) else {
            continue;
        };
        let sid = task.session_id;
        if seen.insert(sid) {
            let last = store
                .read_session_of_types(&sid, &["SessionEnding"], 0)
                .ok()
                .and_then(|v| v.into_iter().next_back())
                .and_then(|e| {
                    let p = store.payload(&e.envelope).ok()?;
                    let tasks = p["running_tasks"]
                        .as_array()?
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect();
                    Some((
                        e.offset,
                        tasks,
                        p["resume_on_reopen"].as_bool() == Some(true),
                    ))
                });
            if let Some(l) = last {
                latest.insert(sid, l);
            }
        }
        let Some((offset, tasks, resume)) = latest.get(&sid) else {
            continue;
        };
        if !*resume || !tasks.contains(&t.to_string()) {
            continue;
        }
        let spent = store
            .read_aggregate_of_types(t.as_bytes(), &["TaskStarted", "TaskResumed"], 0)
            .map(|events| events.iter().any(|e| e.offset > *offset))
            .unwrap_or(true);
        if !spent {
            out.push(*t);
        }
    }
    out
}
