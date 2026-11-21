//! A task's pull request after it was opened (PX-211, REQ-PX-211,
//! DR-PX-2026-11-11-021 MWS-17).
//!
//! The Core keeps the request a task opened (`ForgePullRequestOpened`, PX-007)
//! and reads it again — when a person asks (`RefreshPullRequest`) and on a
//! schedule while it is open — through `forge.pr.read`, under the task's lease
//! and the Capability Kernel like any other forge read, with the forge token
//! in the Core's custody. Each read is recorded on the task's log as
//! `ForgePullRequestStateRead`: the state (open, closed, merged), whether it
//! is a draft, the head the forge reported beside the commit this Core
//! pushed, and its check runs as counts and the names that did not pass. The
//! task's header carries the request and the latest read of it
//! (`AgentHeader.pull_request`, from the store's digest).
//!
//! Four rules, each a test (`tests/px_211_pull_request_state.rs`):
//!
//! * **A state that was not read is never shown.** Nothing writes a state but
//!   a successful read; before one the header names the request with no
//!   state. A read that is refused or fails records nothing, and the last
//!   read stands with the time it was made.
//! * **No read goes around the adapter.** The only path to the forge is the
//!   tool pipeline under the task's lease; a revoked lease, a missing token
//!   or a forge refusal is a refusal here.
//! * **Archive after merge is a preference.** Off unless the person turned it
//!   on for the session (`PullRequestPreferencesSet`). On, the read that
//!   first finds the request merged archives the task with the ordinary
//!   `ConversationArchived` record, so `ArchiveTask` with `archived = false`
//!   is its undo, and a later read of the same merge does not archive again.
//! * **The schedule only reads.** It refreshes requests that are not yet
//!   merged or closed, of tasks that are not archived, and stops when there
//!   is none.

use std::sync::Arc;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::session::SessionEvent;
use modbit_domain::task::{TaskEvent, TaskState};
use modbit_domain::{SessionId, TaskId, ToolCallId};
use modbit_event_store::digest::PullRequestDigest;
use modbit_protocol::v1 as wire;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

type Refusal = (String, String);

fn refuse(code: &str, detail: impl Into<String>) -> Refusal {
    (code.to_owned(), detail.into())
}

/// One read at a time: two reads that both find the merge first would both
/// archive.
fn one_at_a_time() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// The actor of the schedule's reads (the transcript draws no card for them).
pub(crate) const SCHEDULE_ACTOR: &str = "pull_request.refresh";

/// Most names of not-passing runs a record keeps.
const FAILING_NAMES_MAX: usize = 20;
/// Conclusions a check run passes with.
const PASSING: [&str; 3] = ["success", "neutral", "skipped"];
/// Conclusions a check run fails with (as `forge.pr.read` counts them).
const FAILING: [&str; 5] = [
    "failure",
    "timed_out",
    "cancelled",
    "action_required",
    "startup_failure",
];

/// The request's state from the adapter's read.
fn state_of(read: &serde_json::Value) -> Option<&'static str> {
    if read["merged"].as_bool() == Some(true) {
        return Some("MERGED");
    }
    match read["state"].as_str()? {
        "open" => Some("OPEN"),
        "closed" => Some("CLOSED"),
        _ => None,
    }
}

/// The check summary `forge.pr.read` returned, as the record keeps it.
struct Checks {
    word: &'static str,
    total: u32,
    passing: u32,
    failing: u32,
    pending: u32,
    failing_names: Vec<String>,
}

fn checks_of(summary: &serde_json::Value) -> Checks {
    if summary["available"].as_bool() != Some(true) {
        return Checks {
            word: "UNAVAILABLE",
            total: 0,
            passing: 0,
            failing: 0,
            pending: 0,
            failing_names: Vec::new(),
        };
    }
    let n = |v: &serde_json::Value| u32::try_from(v.as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
    let by = &summary["by_conclusion"];
    let sum = |names: &[&str]| names.iter().map(|k| n(&by[*k])).sum::<u32>();
    let total = n(&summary["total"]);
    let pending = n(&summary["not_completed"]);
    let passing = sum(&PASSING);
    let failing = sum(&FAILING);
    let failing_names = summary["not_passing"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .take(FAILING_NAMES_MAX)
                .map(|s| s.chars().take(200).collect())
                .collect()
        })
        .unwrap_or_default();
    let word = if total == 0 {
        "NONE"
    } else if failing > 0 {
        "FAILING"
    } else if pending > 0 {
        "PENDING"
    } else {
        "PASSING"
    };
    Checks {
        word,
        total,
        passing,
        failing,
        pending,
        failing_names,
    }
}

/// The header's view of a task's pull request: the request as opened, and
/// what the latest read of it found — nothing more.
pub(crate) fn view(d: &PullRequestDigest) -> wire::TaskPullRequest {
    let mut v = wire::TaskPullRequest {
        owner: d.owner.clone(),
        repo: d.repo.clone(),
        number: d.number,
        url: d.url.clone(),
        opened_offset: d.opened_offset,
        ..Default::default()
    };
    if let Some(r) = &d.read {
        let p = &r.payload;
        let s = |k: &str| p[k].as_str().unwrap_or_default().to_owned();
        let n = |k: &str| u32::try_from(p[k].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
        v.state = s("state");
        v.draft = p["draft"].as_bool().unwrap_or(false);
        v.checks = s("checks");
        v.checks_total = n("checks_total");
        v.checks_passing = n("checks_passing");
        v.checks_failing = n("checks_failing");
        v.checks_pending = n("checks_pending");
        v.failing_checks = p["failing_checks"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        v.head_sha = s("head_sha");
        v.pushed_sha = s("pushed_sha");
        if !s("url").is_empty() {
            v.url = s("url");
        }
        v.read_at = Some(prost_types::Timestamp {
            seconds: r.at.millis().div_euclid(1000),
            nanos: (r.at.millis().rem_euclid(1000) * 1_000_000) as i32,
        });
        v.read_offset = r.offset;
        v.trigger = s("trigger");
    }
    v
}

/// The session's archive-after-merge preference and the record that set it
/// (`false`, 0 before any).
pub(crate) fn preference(
    store: &modbit_event_store::EventStore,
    session_id: SessionId,
) -> (bool, u64) {
    store
        .read_aggregate(session_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default()
        .iter()
        .rev()
        .find(|e| e.envelope.event_type == "PullRequestPreferencesSet")
        .and_then(|e| {
            let p = store.payload(&e.envelope).ok()?;
            Some((p["archive_after_merge"].as_bool()?, e.offset))
        })
        .unwrap_or((false, 0))
}

/// The answer to a preference read or change.
pub(crate) fn preferences_view(
    store: &modbit_event_store::EventStore,
    session_id: SessionId,
) -> wire::PullRequestPreferences {
    let (archive_after_merge, offset) = preference(store, session_id);
    wire::PullRequestPreferences {
        session_id: Some(wire::Id {
            value: session_id.as_bytes().to_vec(),
        }),
        archive_after_merge,
        offset,
    }
}

/// The event a preference change records.
pub(crate) fn preference_event(archive_after_merge: bool) -> SessionEvent {
    SessionEvent::PullRequestPreferencesSet {
        archive_after_merge,
    }
}

/// Who asked for a read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// A person, through `RefreshPullRequest`.
    Request,
    /// The Core's schedule.
    Schedule,
}

impl Trigger {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Request => "REQUEST",
            Self::Schedule => "SCHEDULE",
        }
    }
}

/// Read the task's pull request through the forge adapter and record what it
/// said; archive the task when the read first finds it merged and the
/// session's preference is on.
pub(crate) async fn refresh(
    core: &Core,
    task_id: TaskId,
    actor: Actor,
    lease_generation: Option<u64>,
    trigger: Trigger,
) -> Result<wire::PullRequestRefreshed, Refusal> {
    let _one = one_at_a_time().lock().await;
    let (task, session, before) = {
        let store = core.store.lock().await;
        let task = store
            .task(&task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_TASK", task_id.to_string()))?;
        let session = store
            .session(&task.session_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_SESSION", task.session_id.to_string()))?;
        let before = store
            .task_digests(&task.session_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .into_iter()
            .find(|d| d.task_id == task_id);
        (task, session, before)
    };
    let archived_before = before.as_ref().is_some_and(|d| d.archived);
    let Some(pr) = before.and_then(|d| d.pull_request) else {
        return Err(refuse(
            "NO_PULL_REQUEST",
            "no pull request was opened for this task (OpenPullRequest)",
        ));
    };
    let state_before = pr
        .read
        .as_ref()
        .and_then(|r| r.payload["state"].as_str().map(str::to_owned));
    // The commit this Core pushed: the request's branch here.
    let pushed_sha = task
        .workspace_root
        .as_deref()
        .and_then(|root| modbit_git::Repo::open(std::path::Path::new(root)).ok())
        .and_then(|git| {
            let branch = format!(
                "modbit/pr-{}",
                &task.task_id.to_string().replace('-', "")[..12]
            );
            git.rev_parse(&branch).ok()
        })
        .unwrap_or_default();
    let lease = {
        let store = core.store.lock().await;
        store
            .leases_for_task(&task.task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .into_iter()
            .next()
    };
    let tool_call_id = ToolCallId::new();
    let arguments_json =
        serde_json::json!({"owner": pr.owner, "repo": pr.repo, "number": pr.number}).to_string();
    let done = core
        .tools
        .invoke(
            &core.store,
            crate::tools::InvokeRequest {
                tenant_id: core.tenant_id,
                session_id: task.session_id,
                task_id: task.task_id,
                workspace_root: task.workspace_root.clone(),
                execution_profile: &task.execution_profile,
                tool_call_id,
                tool_name: "forge.pr.read",
                arguments_json: &arguments_json,
                output_budget_bytes: 65_536,
                actor: actor.clone(),
                lease,
                approval: None,
                emergency_stopped: session.emergency_stopped_at.is_some(),
                existing: None,
                run_id: None,
                turn_id: None,
                call_id: None,
                lease_generation,
                projection: None,
                cancel: None,
                compensates: None,
            },
        )
        .await
        .map_err(|e| refuse("TOOL_HOST", e.to_string()))?;
    let r = &done.result;
    if !matches!(r.status, modbit_tools::ToolStatus::Success) {
        return Err(refuse(
            r.error_code.as_deref().unwrap_or("FORGE_READ"),
            r.error_message.clone().unwrap_or_default(),
        ));
    }
    let read = &r.structured_output;
    // The adapter must have read this request; an answer about another one
    // (or none) is not recorded as this one's state.
    if read["number"].as_u64() != Some(pr.number) {
        return Err(refuse(
            "FORGE_READ",
            format!(
                "the forge's answer does not name pull request #{}",
                pr.number
            ),
        ));
    }
    let Some(state) = state_of(read) else {
        return Err(refuse(
            "FORGE_READ",
            "the forge's answer carried no state the Core knows",
        ));
    };
    let checks = checks_of(&read["checks"]);
    let event = TaskEvent::ForgePullRequestStateRead {
        owner: pr.owner.clone(),
        repo: pr.repo.clone(),
        number: pr.number,
        url: read["url"].as_str().unwrap_or(&pr.url).to_owned(),
        state: state.to_owned(),
        draft: read["draft"].as_bool().unwrap_or(false),
        head_sha: read["head"]["sha"].as_str().unwrap_or_default().to_owned(),
        pushed_sha,
        checks: checks.word.to_owned(),
        checks_total: checks.total,
        checks_passing: checks.passing,
        checks_failing: checks.failing,
        checks_pending: checks.pending,
        failing_checks: checks.failing_names,
        trigger: trigger.as_str().to_owned(),
        tool_call_id: tool_call_id.to_string(),
    };
    let mut store = core.store.lock().await;
    let offset = append(
        &mut store,
        core,
        Lineage::task(core.tenant_id, task.session_id, task.task_id),
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(event.event_type(), &event, actor)],
    )
    .map_err(|e| refuse("STORE", e))?;
    // Archive after merge: only on the read that first finds the merge, only
    // with the preference on, never under a running task (archiving would
    // stop it silently) and never twice.
    let mut archive_offset = 0;
    let first_merge = state == "MERGED" && state_before.as_deref() != Some("MERGED");
    if first_merge && !archived_before && preference(&store, task.session_id).0 {
        let running = store
            .task(&task_id)
            .ok()
            .flatten()
            .is_some_and(|t| t.state == TaskState::Running);
        if !running {
            let archive = modbit_domain::conversation::ConversationEvent::ConversationArchived;
            archive_offset = append(
                &mut store,
                core,
                Lineage::task(core.tenant_id, task.session_id, task.task_id),
                AggregateType::Conversation,
                modbit_domain::conversation::aggregate_id(task.task_id),
                vec![typed(
                    archive.event_type(),
                    &archive,
                    Actor::Core("pull_request.archive_after_merge".into()),
                )],
            )
            .map_err(|e| refuse("STORE", e))?;
        }
    }
    let after = store
        .task_digests(&task.session_id)
        .map_err(|e| refuse("STORE", e.to_string()))?
        .into_iter()
        .find(|d| d.task_id == task_id)
        .and_then(|d| d.pull_request);
    drop(store);
    core.last_offset.send_replace(if archive_offset > 0 {
        archive_offset
    } else {
        offset
    });
    Ok(wire::PullRequestRefreshed {
        pull_request: after.as_ref().map(view),
        offset,
        archived: archive_offset > 0,
        archive_offset,
    })
}

/// How often the schedule reads open requests; `None` = never.
/// `MODBIT_PR_REFRESH_SECS` (0 turns it off); five minutes by default.
fn refresh_every() -> Option<std::time::Duration> {
    let secs = std::env::var("MODBIT_PR_REFRESH_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(300);
    (secs > 0).then(|| std::time::Duration::from_secs(secs))
}

/// The tasks the schedule reads now: a recorded request not yet read as
/// merged or closed, of a task that is not archived.
async fn due(core: &Core) -> Vec<TaskId> {
    let store = core.store.lock().await;
    let Ok(sessions) = store.sessions_with_pull_requests() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for s in sessions {
        for d in store.task_digests(&s).unwrap_or_default() {
            let Some(pr) = &d.pull_request else { continue };
            let settled = pr
                .read
                .as_ref()
                .is_some_and(|r| matches!(r.payload["state"].as_str(), Some("MERGED" | "CLOSED")));
            if !d.archived && !settled {
                out.push(d.task_id);
            }
        }
    }
    out
}

/// The Core's schedule: every `MODBIT_PR_REFRESH_SECS`, read each open
/// request once. A read that fails is said on stderr and tried at the next
/// turn; nothing is recorded for it.
pub(crate) fn spawn_watcher(core: Arc<Core>) {
    let Some(every) = refresh_every() else {
        return;
    };
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(every).await;
            if core.tools.forge.get().is_none() {
                continue;
            }
            for task_id in due(&core).await {
                if let Err((code, detail)) = refresh(
                    &core,
                    task_id,
                    Actor::Core(SCHEDULE_ACTOR.into()),
                    None,
                    Trigger::Schedule,
                )
                .await
                {
                    eprintln!(
                        "modbit-core: scheduled pull request read for task {task_id} refused: {code}: {detail}"
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_state_is_merged_only_when_the_forge_says_merged() {
        assert_eq!(
            state_of(&json!({"state": "closed", "merged": true})),
            Some("MERGED")
        );
        assert_eq!(
            state_of(&json!({"state": "closed", "merged": false})),
            Some("CLOSED")
        );
        assert_eq!(state_of(&json!({"state": "open"})), Some("OPEN"));
        assert_eq!(state_of(&json!({"state": "weird"})), None);
        assert_eq!(state_of(&json!({})), None);
    }

    #[test]
    fn checks_are_counted_as_the_adapter_summarised_them() {
        let c = checks_of(&json!({"available": false, "reason": "FORGE_FORBIDDEN"}));
        assert_eq!(c.word, "UNAVAILABLE");
        let c = checks_of(
            &json!({"available": true, "total": 0, "not_completed": 0, "by_conclusion": {}, "not_passing": []}),
        );
        assert_eq!(c.word, "NONE");
        let c = checks_of(
            &json!({"available": true, "total": 3, "not_completed": 1, "by_conclusion": {"success": 1, "failure": 1}, "not_passing": ["lint"]}),
        );
        assert_eq!(
            (c.word, c.total, c.passing, c.failing, c.pending),
            ("FAILING", 3, 1, 1, 1)
        );
        assert_eq!(c.failing_names, vec!["lint".to_owned()]);
        let c = checks_of(
            &json!({"available": true, "total": 2, "not_completed": 1, "by_conclusion": {"success": 1}, "not_passing": []}),
        );
        assert_eq!(c.word, "PENDING");
        let c = checks_of(
            &json!({"available": true, "total": 2, "not_completed": 0, "by_conclusion": {"success": 1, "skipped": 1}, "not_passing": []}),
        );
        assert_eq!((c.word, c.passing), ("PASSING", 2));
    }
}
