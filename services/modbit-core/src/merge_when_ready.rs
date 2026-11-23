//! Merge when ready (PX-213, REQ-PX-213, DR-PX-2026-11-11-021 MWS-17).
//!
//! A request a person makes on a task's pull request (`SetMergeWhenReady`,
//! `MergeWhenReadySet` on the task's log); off until they do. With it on,
//! every read of the request (PX-211: on request, on the schedule, or the
//! command's own read) is looked at here:
//!
//! * open, not a draft and its checks passed (or it has none) at head `H`:
//!   the Core raises the approval of `forge.pr.merge` through the tool
//!   pipeline under the task's lease, with the exact intent — repository,
//!   number, head `H` and method (`MergeAsked`). The kernel decides as for
//!   any external effect: the merge is in the always-ask NETWORK and SECRET
//!   classes, so no run mode passes it, and an allow rule (a literal argv
//!   prefix) cannot name it.
//! * a person approves: the same call runs under that approval, once. The
//!   tool reads the request again and refuses unless it is still open at
//!   `H` with every check of `H` completed and none failing, then asks the
//!   forge to merge pinned to `H` (the forge refuses a moved head itself).
//!   The kernel appends the receipt; `MergeCompleted` names it.
//! * the head moves, the checks stop passing or the request becomes a draft
//!   while an ask stands: the ask is voided (`MergeAskVoided`; a pending
//!   approval is closed so nothing can approve it) and asked again for the
//!   new head once its checks pass.
//! * a denial, a request closed or merged by someone else, or a refusal no
//!   later read can clear turns it off (`MergeWhenReadyCleared`) and says why.
//!
//! The Core never approves anything here and never retries a spent approval:
//! each ask is its own call id, derived from the task, the request, the head
//! and the ask's number, so a retry after a crash re-enters the same call.

use std::collections::HashMap;
use std::sync::Arc;

use modbit_domain::approval::ApprovalState;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::state::StateMachine as _;
use modbit_domain::task::TaskEvent;
use modbit_domain::{TaskId, ToolCallId};
use modbit_protocol::v1 as wire;
use sha2::Digest;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// The actor of the Core's merge-when-ready records and calls.
pub(crate) const ACTOR: &str = "merge_when_ready";

/// The methods a person may ask for.
pub(crate) const METHODS: [&str; 3] = ["merge", "squash", "rebase"];

const TYPES: [&str; 6] = [
    "MergeWhenReadySet",
    "MergeAsked",
    "MergeAskVoided",
    "MergeCompleted",
    "MergeRefused",
    "MergeWhenReadyCleared",
];

/// One step at a time: two reads that both find the checks passing would
/// both ask.
fn one_at_a_time() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// An ask as the log recorded it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Ask {
    pub head_sha: String,
    pub method: String,
    pub attempt: u32,
    pub approval_id: String,
    pub tool_call_id: String,
}

/// Merge when ready for one task, as its log says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Status {
    pub enabled: bool,
    pub method: String,
    /// The ask that stands (not voided, refused or spent on a merge).
    pub open: Option<Ask>,
    /// Asks per head since the request was made.
    pub asks: HashMap<String, u32>,
    /// The head merged and the forge's merge commit.
    pub merged: Option<(String, String)>,
    /// Why the Core turned it off ("" = it did not).
    pub cleared: String,
    /// The latest refusal a later read may clear: (head, code, detail).
    pub refused: Option<(String, String, String)>,
    /// The latest record; 0 = none.
    pub offset: u64,
}

/// Fold the task's merge-when-ready records.
pub(crate) fn status(store: &modbit_event_store::EventStore, task_id: TaskId) -> Status {
    let events = store
        .read_aggregate_of_types(task_id.as_bytes(), &TYPES, 0)
        .unwrap_or_default();
    let mut s = Status::default();
    for e in events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let st = |k: &str| p[k].as_str().unwrap_or_default().to_owned();
        s.offset = e.offset;
        match e.envelope.event_type.as_str() {
            "MergeWhenReadySet" => {
                let enabled = p["enabled"].as_bool().unwrap_or(false);
                if enabled {
                    // A new request: what an earlier one did is history.
                    s = Status {
                        offset: e.offset,
                        ..Status::default()
                    };
                }
                s.enabled = enabled;
                s.method = st("method");
                if !enabled {
                    s.open = None;
                }
            }
            "MergeAsked" => {
                let ask = Ask {
                    head_sha: st("head_sha"),
                    method: st("method"),
                    attempt: u32::try_from(p["attempt"].as_u64().unwrap_or(0)).unwrap_or(0),
                    approval_id: st("approval_id"),
                    tool_call_id: st("tool_call_id"),
                };
                *s.asks.entry(ask.head_sha.clone()).or_default() += 1;
                s.refused = None;
                s.open = Some(ask);
            }
            "MergeAskVoided" => {
                if s.open
                    .as_ref()
                    .is_some_and(|a| a.approval_id == st("approval_id"))
                {
                    s.open = None;
                }
            }
            "MergeCompleted" => {
                s.open = None;
                s.enabled = false;
                s.merged = Some((st("head_sha"), st("merge_sha")));
            }
            "MergeRefused" => {
                s.open = None;
                s.refused = Some((st("head_sha"), st("code"), st("detail")));
            }
            "MergeWhenReadyCleared" => {
                s.open = None;
                s.enabled = false;
                s.cleared = st("reason");
            }
            _ => {}
        }
    }
    s
}

fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

/// What a person reads for a reason the Core turned it off.
fn cleared_words(reason: &str) -> String {
    match reason {
        "DENIED" => "The merge was denied, so merge when ready is off.".into(),
        "CLOSED" => "The request was closed, so merge when ready is off.".into(),
        "MERGED_ELSEWHERE" => "The request was merged outside Modbit.".into(),
        "NOT_MERGEABLE" => {
            "The forge says the request cannot be merged (a conflict or a branch rule), so merge when ready is off.".into()
        }
        "PR_DRAFT" => "The request is a draft, so merge when ready is off.".into(),
        "PR_NOT_OPEN" => "The request is no longer open, so merge when ready is off.".into(),
        other => format!("Merge when ready is off: the forge refused the merge ({other})."),
    }
}

/// The phase and its words, from the log and the latest read's payload.
pub(crate) fn phase(s: &Status, read: Option<&serde_json::Value>) -> (&'static str, String) {
    if let Some((head, merge_sha)) = &s.merged {
        return (
            "MERGED",
            format!("Merged {} as {}.", short(head), short(merge_sha)),
        );
    }
    if !s.enabled {
        if !s.cleared.is_empty() {
            return ("CLEARED", cleared_words(&s.cleared));
        }
        return ("OFF", String::new());
    }
    if let Some(a) = &s.open {
        return (
            "ASKED",
            format!(
                "Checks passed at {}. Approve the merge ({}) to merge that head.",
                short(&a.head_sha),
                a.method
            ),
        );
    }
    let Some(r) = read else {
        return ("WAITING", "Waiting for the request to be read.".into());
    };
    let refused = s
        .refused
        .as_ref()
        .filter(|(h, ..)| r["head_sha"].as_str() == Some(h.as_str()))
        .map(|(_, _, d)| format!(" The last attempt was refused: {d}"))
        .unwrap_or_default();
    let words = if r["draft"].as_bool() == Some(true) {
        "Waiting: the request is a draft.".to_owned()
    } else {
        match r["checks"].as_str().unwrap_or_default() {
            "PENDING" => format!(
                "Waiting for {} check(s) to finish.",
                r["checks_pending"].as_u64().unwrap_or(0)
            ),
            "FAILING" => {
                let names: Vec<&str> = r["failing_checks"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                format!("Waiting: checks failing ({}).", names.join(", "))
            }
            "UNAVAILABLE" => "Waiting: the checks could not be read.".to_owned(),
            _ => "Waiting for the next read.".to_owned(),
        }
    };
    ("WAITING", format!("{words}{refused}"))
}

/// The header's merge-when-ready fields on a task's pull request.
pub(crate) fn decorate(
    v: &mut wire::TaskPullRequest,
    store: &modbit_event_store::EventStore,
    task_id: TaskId,
) {
    let s = status(store, task_id);
    let read = latest_read(store, task_id);
    let (phase, detail) = phase(&s, read.as_ref());
    v.merge_when_ready = s.enabled;
    v.merge_method = s.method.clone();
    v.merge_phase = phase.into();
    v.merge_detail = detail;
    v.merge_approval_id = s
        .open
        .as_ref()
        .map(|a| a.approval_id.clone())
        .unwrap_or_default();
    v.merge_head_sha = s
        .open
        .as_ref()
        .map(|a| a.head_sha.clone())
        .or_else(|| s.merged.as_ref().map(|m| m.0.clone()))
        .unwrap_or_default();
    v.merge_sha = s.merged.map(|m| m.1).unwrap_or_default();
    v.merge_offset = s.offset;
}

/// The answer to `SetMergeWhenReady`.
pub(crate) fn state_view(
    store: &modbit_event_store::EventStore,
    task_id: TaskId,
) -> wire::MergeWhenReadyState {
    let mut v = wire::TaskPullRequest::default();
    decorate(&mut v, store, task_id);
    wire::MergeWhenReadyState {
        task_id: Some(crate::server::wire_id(task_id.as_bytes())),
        enabled: v.merge_when_ready,
        method: v.merge_method,
        phase: v.merge_phase,
        detail: v.merge_detail,
        approval_id: v.merge_approval_id,
        head_sha: v.merge_head_sha,
        merge_sha: v.merge_sha,
        offset: v.merge_offset,
    }
}

/// The latest `ForgePullRequestStateRead` payload of the task.
fn latest_read(
    store: &modbit_event_store::EventStore,
    task_id: TaskId,
) -> Option<serde_json::Value> {
    let events = store
        .read_aggregate_of_types(task_id.as_bytes(), &["ForgePullRequestStateRead"], 0)
        .ok()?;
    let e = events.last()?;
    store.payload(&e.envelope).ok()
}

/// What a read said, as this module needs it.
#[derive(Clone, Debug)]
pub(crate) struct Read {
    pub owner: String,
    pub repo: String,
    pub number: u64,
    /// OPEN | CLOSED | MERGED.
    pub state: String,
    pub draft: bool,
    pub head_sha: String,
    /// NONE | PENDING | FAILING | PASSING | UNAVAILABLE.
    pub checks: String,
    /// The `ForgePullRequestStateRead`.
    pub offset: u64,
}

impl Read {
    fn ready(&self) -> bool {
        self.state == "OPEN"
            && !self.draft
            && self.head_sha.len() == 40
            && matches!(self.checks.as_str(), "PASSING" | "NONE")
    }
}

/// What a step did.
#[derive(Clone, Debug, Default)]
pub(crate) struct Outcome {
    /// An approval raised now.
    pub asked: String,
    /// The merge happened now.
    pub merged: bool,
}

/// The tool call an ask always maps to.
fn call_id(task_id: TaskId, number: u64, head: &str, attempt: u32) -> ToolCallId {
    let h =
        sha2::Sha256::digest(format!("pr-merge:{task_id}:{number}:{head}:{attempt}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    ToolCallId::from_bytes(b)
}

fn record(
    core: &Arc<Core>,
    store: &mut modbit_event_store::EventStore,
    task_id: TaskId,
    session_id: modbit_domain::SessionId,
    event: &TaskEvent,
) -> Result<u64, String> {
    let offset = append(
        store,
        core,
        Lineage::task(core.tenant_id, session_id, task_id),
        AggregateType::Task,
        *task_id.as_bytes(),
        vec![typed(event.event_type(), event, Actor::Core(ACTOR.into()))],
    )?;
    core.last_offset.send_replace(offset);
    Ok(offset)
}

/// Void the standing ask (closing its approval if it is still pending).
async fn void(
    core: &Arc<Core>,
    task_id: TaskId,
    session_id: modbit_domain::SessionId,
    number: u64,
    ask: &Ask,
    reason: &str,
) -> Result<(), String> {
    if let Ok(id) = modbit_domain::ApprovalId::parse(&ask.approval_id) {
        crate::approval_expiry::void(core, id, "APPROVAL_VOIDED").await;
    }
    let mut store = core.store.lock().await;
    record(
        core,
        &mut store,
        task_id,
        session_id,
        &TaskEvent::MergeAskVoided {
            number,
            head_sha: ask.head_sha.clone(),
            approval_id: ask.approval_id.clone(),
            reason: reason.into(),
        },
    )
    .map(|_| ())
}

async fn clear(
    core: &Arc<Core>,
    task_id: TaskId,
    session_id: modbit_domain::SessionId,
    reason: &str,
) -> Result<(), String> {
    let mut store = core.store.lock().await;
    record(
        core,
        &mut store,
        task_id,
        session_id,
        &TaskEvent::MergeWhenReadyCleared {
            reason: reason.into(),
        },
    )
    .map(|_| ())
}

/// A person's decision on a merge ask (from `ResolveApproval`): a denial
/// turns merge when ready off; an approval is acted on by the caller's next
/// read. Returns whether the approval was this task's standing merge ask.
pub(crate) async fn on_denied(core: &Arc<Core>, task_id: TaskId, approval_id: &str) -> bool {
    let _one = one_at_a_time().lock().await;
    let (s, session_id) = {
        let store = core.store.lock().await;
        let Ok(Some(task)) = store.task(&task_id) else {
            return false;
        };
        (status(&store, task_id), task.session_id)
    };
    if !s.enabled || s.open.as_ref().is_none_or(|a| a.approval_id != approval_id) {
        return false;
    }
    if let Err(e) = clear(core, task_id, session_id, "DENIED").await {
        eprintln!("modbit-core: merge when ready for task {task_id}: {e}");
    }
    true
}

/// A person's decision on a `forge.pr.merge` approval, handed from
/// `ResolveApproval` to the worker (the handler itself never waits on a
/// read): (task, approval, approved).
type Decision = (TaskId, String, bool);

fn decisions() -> &'static std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<Decision>> {
    static TX: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<Decision>> =
        std::sync::OnceLock::new();
    &TX
}

/// Queue a decision for the worker. Before the worker runs (a Core without
/// one) the next read acts on the approval instead.
pub(crate) fn decided(task_id: TaskId, approval_id: String, approved: bool) {
    if let Some(tx) = decisions().get() {
        let _ = tx.send((task_id, approval_id, approved));
    }
}

/// The worker: a denial of the standing ask turns merge when ready off; an
/// approval of it runs the read that merges the approved head.
pub(crate) fn spawn_worker(core: Arc<Core>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Decision>();
    if decisions().set(tx).is_err() {
        return;
    }
    tokio::spawn(async move {
        while let Some((task_id, id, approved)) = rx.recv().await {
            if !approved {
                on_denied(&core, task_id, &id).await;
                continue;
            }
            if !is_open_ask(&core, task_id, &id).await {
                continue;
            }
            if let Err((code, detail)) = crate::pull_request_state::refresh(
                &core,
                task_id,
                Actor::Core(ACTOR.into()),
                None,
                crate::pull_request_state::Trigger::Approval,
            )
            .await
            {
                eprintln!("modbit-core: merge after approval for task {task_id}: {code}: {detail}");
            }
        }
    });
}

/// Whether `approval_id` is the task's standing merge ask.
pub(crate) async fn is_open_ask(core: &Arc<Core>, task_id: TaskId, approval_id: &str) -> bool {
    let store = core.store.lock().await;
    status(&store, task_id)
        .open
        .is_some_and(|a| a.approval_id == approval_id)
}

/// Look at a read: ask, void, merge, clear or nothing.
pub(crate) async fn on_read(
    core: &Arc<Core>,
    task_id: TaskId,
    read: &Read,
    lease_generation: Option<u64>,
) -> Result<Outcome, String> {
    let _one = one_at_a_time().lock().await;
    let (s, task, session) = {
        let store = core.store.lock().await;
        let task = store
            .task(&task_id)
            .map_err(|e| e.to_string())?
            .ok_or("unknown task")?;
        let session = store
            .session(&task.session_id)
            .map_err(|e| e.to_string())?
            .ok_or("unknown session")?;
        (status(&store, task_id), task, session)
    };
    let mut out = Outcome::default();
    if !s.enabled {
        return Ok(out);
    }
    let sid = task.session_id;
    // Settled by someone else: nothing to merge.
    if read.state != "OPEN" {
        let why = if read.state == "MERGED" {
            "MERGED"
        } else {
            "CLOSED"
        };
        if let Some(a) = &s.open {
            void(core, task_id, sid, read.number, a, why).await?;
        }
        let reason = if read.state == "MERGED" {
            "MERGED_ELSEWHERE"
        } else {
            "CLOSED"
        };
        clear(core, task_id, sid, reason).await?;
        return Ok(out);
    }
    // The standing ask: still the intent? approved? decided otherwise?
    if let Some(a) = &s.open {
        let approval = {
            let store = core.store.lock().await;
            modbit_domain::ApprovalId::parse(&a.approval_id)
                .ok()
                .and_then(|id| store.approval(&id).ok().flatten())
        };
        let state = approval.as_ref().map(|x| x.state);
        if a.head_sha != read.head_sha {
            void(core, task_id, sid, read.number, a, "HEAD_MOVED").await?;
        } else if !read.ready() {
            let why = if read.draft {
                "DRAFT"
            } else {
                "CHECKS_NOT_PASSING"
            };
            void(core, task_id, sid, read.number, a, why).await?;
            return Ok(out);
        } else {
            match state {
                Some(ApprovalState::Requested) => return Ok(out),
                Some(ApprovalState::Denied) => {
                    clear(core, task_id, sid, "DENIED").await?;
                    return Ok(out);
                }
                Some(ApprovalState::Approved) => {
                    out.merged = dispatch(core, &task, &session, read, a, lease_generation).await?;
                    return Ok(out);
                }
                Some(ApprovalState::Expired) | None => {
                    void(core, task_id, sid, read.number, a, "EXPIRED").await?;
                }
            }
        }
    }
    if !read.ready() {
        return Ok(out);
    }
    // Ask: the exact intent, through the pipeline; the kernel raises the
    // approval (nothing approves a forge merge but a person).
    let method = if s.method.is_empty() {
        "merge".to_owned()
    } else {
        s.method.clone()
    };
    let mut attempt = s.asks.get(&read.head_sha).copied().unwrap_or(0);
    // A call that a policy refusal ended (an emergency stop, a revoked
    // token) is terminal; the next ask is the next id.
    let (tool_call_id, existing, approval, lease) = loop {
        let id = call_id(task_id, read.number, &read.head_sha, attempt);
        let store = core.store.lock().await;
        let existing = store.tool_call(&id).map_err(|e| e.to_string())?;
        if existing.as_ref().is_some_and(|c| c.state.is_terminal()) && attempt < 64 {
            attempt += 1;
            continue;
        }
        let approval = store.approval_for_call(&id).map_err(|e| e.to_string())?;
        let lease = store
            .leases_for_task(&task_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .next();
        break (id, existing, approval, lease);
    };
    let args = serde_json::json!({
        "owner": read.owner, "repo": read.repo, "number": read.number,
        "sha": read.head_sha, "merge_method": method,
        "idempotency_key": format!("pr-merge:{task_id}:{}:{}", read.number, read.head_sha),
    })
    .to_string();
    let done = core
        .tools
        .invoke(
            &core.store,
            crate::tools::InvokeRequest {
                tenant_id: core.tenant_id,
                session_id: sid,
                task_id,
                workspace_root: task.workspace_root.clone(),
                execution_profile: &task.execution_profile,
                tool_call_id,
                tool_name: "forge.pr.merge",
                arguments_json: &args,
                output_budget_bytes: 16_384,
                actor: Actor::Core(ACTOR.into()),
                lease,
                approval,
                emergency_stopped: session.emergency_stopped_at.is_some(),
                existing,
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
        .map_err(|e| e.to_string())?;
    match done.result.status {
        modbit_tools::ToolStatus::ApprovalPending => {
            let approval_id = done.approval_id.map(|a| a.to_string()).unwrap_or_default();
            let mut store = core.store.lock().await;
            record(
                core,
                &mut store,
                task_id,
                sid,
                &TaskEvent::MergeAsked {
                    number: read.number,
                    head_sha: read.head_sha.clone(),
                    method,
                    attempt,
                    approval_id: approval_id.clone(),
                    tool_call_id: tool_call_id.to_string(),
                    intent_hash: modbit_tools::arguments_hash(&args).unwrap_or_default(),
                    read_offset: read.offset,
                },
            )?;
            out.asked = approval_id;
        }
        _ => {
            // Refused before asking (an emergency stop, no token, the
            // lease): nothing recorded; the next read tries again.
            eprintln!(
                "modbit-core: merge when ready for task {task_id} could not ask: {}: {}",
                done.result.error_code.clone().unwrap_or_default(),
                done.result.error_message.clone().unwrap_or_default()
            );
        }
    }
    Ok(out)
}

/// Run the approved ask once. Returns whether the request was merged.
async fn dispatch(
    core: &Arc<Core>,
    task: &modbit_domain::task::Task,
    session: &modbit_domain::session::Session,
    read: &Read,
    a: &Ask,
    lease_generation: Option<u64>,
) -> Result<bool, String> {
    let task_id = task.task_id;
    let id = ToolCallId::parse(&a.tool_call_id).map_err(|e| e.to_string())?;
    let (existing, approval, lease) = {
        let store = core.store.lock().await;
        (
            store.tool_call(&id).map_err(|e| e.to_string())?,
            store.approval_for_call(&id).map_err(|e| e.to_string())?,
            store
                .leases_for_task(&task_id)
                .map_err(|e| e.to_string())?
                .into_iter()
                .next(),
        )
    };
    // The call carries the arguments the approval bound: rebuild them from
    // the ask, never from the read (the read's head is the ask's here).
    let args = serde_json::json!({
        "owner": read.owner, "repo": read.repo, "number": read.number,
        "sha": a.head_sha, "merge_method": a.method,
        "idempotency_key": format!("pr-merge:{task_id}:{}:{}", read.number, a.head_sha),
    })
    .to_string();
    let done = core
        .tools
        .invoke(
            &core.store,
            crate::tools::InvokeRequest {
                tenant_id: core.tenant_id,
                session_id: task.session_id,
                task_id,
                workspace_root: task.workspace_root.clone(),
                execution_profile: &task.execution_profile,
                tool_call_id: id,
                tool_name: "forge.pr.merge",
                arguments_json: &args,
                output_budget_bytes: 16_384,
                actor: Actor::Core(ACTOR.into()),
                lease,
                approval,
                emergency_stopped: session.emergency_stopped_at.is_some(),
                existing,
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
        .map_err(|e| e.to_string())?;
    let r = &done.result;
    let mut store = core.store.lock().await;
    match r.status {
        modbit_tools::ToolStatus::Success => {
            record(
                core,
                &mut store,
                task_id,
                task.session_id,
                &TaskEvent::MergeCompleted {
                    number: read.number,
                    head_sha: a.head_sha.clone(),
                    method: a.method.clone(),
                    merge_sha: r.structured_output["merge_sha"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    approval_id: a.approval_id.clone(),
                    effect_receipt_ids: r.effect_receipt_ids.clone(),
                },
            )?;
            Ok(true)
        }
        modbit_tools::ToolStatus::ApprovalPending => Ok(false),
        _ => {
            let code = r
                .error_code
                .clone()
                .unwrap_or_else(|| "MERGE_FAILED".into());
            let retryable = modbit_tools::forge::MERGE_RETRYABLE.contains(&code.as_str());
            record(
                core,
                &mut store,
                task_id,
                task.session_id,
                &TaskEvent::MergeRefused {
                    number: read.number,
                    head_sha: a.head_sha.clone(),
                    approval_id: a.approval_id.clone(),
                    code: code.clone(),
                    detail: r.error_message.clone().unwrap_or_default(),
                    retryable,
                },
            )?;
            if !retryable {
                record(
                    core,
                    &mut store,
                    task_id,
                    task.session_id,
                    &TaskEvent::MergeWhenReadyCleared { reason: code },
                )?;
            }
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn on() -> Status {
        Status {
            enabled: true,
            method: "squash".into(),
            ..Status::default()
        }
    }

    #[test]
    fn the_phase_says_what_the_merge_waits_for() {
        assert_eq!(phase(&Status::default(), None).0, "OFF");
        assert_eq!(phase(&on(), None).0, "WAITING");
        let pending = json!({"checks": "PENDING", "checks_pending": 2, "head_sha": "a"});
        assert_eq!(
            phase(&on(), Some(&pending)),
            ("WAITING", "Waiting for 2 check(s) to finish.".to_owned())
        );
        let failing = json!({"checks": "FAILING", "failing_checks": ["lint"], "head_sha": "a"});
        assert_eq!(
            phase(&on(), Some(&failing)).1,
            "Waiting: checks failing (lint)."
        );
        let draft = json!({"checks": "PASSING", "draft": true});
        assert_eq!(
            phase(&on(), Some(&draft)).1,
            "Waiting: the request is a draft."
        );
        let mut asked = on();
        asked.open = Some(Ask {
            head_sha: "1".repeat(40),
            method: "squash".into(),
            ..Ask::default()
        });
        assert_eq!(phase(&asked, None).0, "ASKED");
        let merged = Status {
            merged: Some(("1".repeat(40), "2".repeat(40))),
            ..Status::default()
        };
        assert_eq!(
            phase(&merged, None),
            (
                "MERGED",
                format!("Merged {} as {}.", "1".repeat(12), "2".repeat(12))
            )
        );
        let cleared = Status {
            cleared: "DENIED".into(),
            ..Status::default()
        };
        assert_eq!(phase(&cleared, None).0, "CLEARED");
    }

    #[test]
    fn only_an_open_request_whose_checks_passed_is_ready() {
        let r = |state: &str, draft: bool, checks: &str| Read {
            owner: "o".into(),
            repo: "r".into(),
            number: 1,
            state: state.into(),
            draft,
            head_sha: "a".repeat(40),
            checks: checks.into(),
            offset: 1,
        };
        assert!(r("OPEN", false, "PASSING").ready());
        assert!(r("OPEN", false, "NONE").ready());
        assert!(!r("OPEN", false, "PENDING").ready());
        assert!(!r("OPEN", false, "FAILING").ready());
        assert!(!r("OPEN", false, "UNAVAILABLE").ready());
        assert!(!r("OPEN", true, "PASSING").ready());
        assert!(!r("CLOSED", false, "PASSING").ready());
        let mut short_head = r("OPEN", false, "PASSING");
        short_head.head_sha = "abc".into();
        assert!(!short_head.ready());
    }

    #[test]
    fn an_ask_maps_to_one_call_per_head_and_attempt() {
        let t = TaskId::new();
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        assert_eq!(call_id(t, 1, &a, 0), call_id(t, 1, &a, 0));
        assert_ne!(call_id(t, 1, &a, 0), call_id(t, 1, &a, 1));
        assert_ne!(call_id(t, 1, &a, 0), call_id(t, 1, &b, 0));
    }
}
