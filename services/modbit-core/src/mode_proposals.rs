//! Mode-switch proposals (REQ-PX-051, REQ-PX-055; docs/65 AFW-D06;
//! QUAL-PX-051 "a proposed switch with no answer expires skipped after 15 s
//! and the posture is unchanged", QUAL-PX-055).
//!
//! The agent may ask the person to move its task to another mode with the
//! harness tool `task.propose_mode`. A proposal is **data**: two facts on the
//! task's own log (`ModeSwitchProposed`, then exactly one `ModeSwitchDecided`)
//! and nothing else. It never changes a mode, a posture or a policy by itself;
//! the agent's words in it are untrusted text shown to the person.
//!
//! * the person decides with `DecideModeProposal`. Accepting writes the same
//!   `TaskModeSet` a `SetTaskMode` writes, through the same function
//!   (`tasking::apply_mode`: open task, session check, lease check, the
//!   Kernel follows the event), in one transaction with the
//!   `ModeSwitchDecided { ACCEPTED }` that closes the proposal;
//! * a proposal not decided within 15 s ends `SKIPPED` / `UNANSWERED_15S`.
//!   It never ends accepted. A late accept is refused `PROPOSAL_NOT_PENDING`;
//! * a proposal pending when a Core died is `SKIPPED` / `CORE_RESTARTED` when
//!   the next Core starts: the run that asked is gone, so the question is not
//!   put to the person again. The mode is unchanged;
//! * a proposal on a task that ended is `SKIPPED` / `TASK_ENDED`; one whose
//!   `from` mode is no longer the task's mode is `SKIPPED` / `MODE_CHANGED`
//!   and cannot be accepted.
//!
//! Only the Core's own tool handler creates a proposal (a client cannot
//! forge one on the agent's behalf), at most one is pending per task, and the
//! tool is offered only to an interactive primary agent in a mode other than
//! AGENT (the default posture needs no switch, and an unconstrained task pays
//! no request bytes for a tool it does not need).

use std::collections::HashMap;
use std::sync::Arc;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::mode::TaskMode as Mode;
use modbit_domain::state::StateMachine;
use modbit_domain::task::{Task, TaskEvent, TaskOrigin};
use modbit_domain::{SessionId, TaskId, Timestamp};
use modbit_event_store::{AppendRequest, CommandOutcome, EventStore};
use modbit_protocol::v1 as wire;
use prost::Message;

use crate::runtime::{Lineage, TranscriptEntry, append, typed};
use crate::server::Core;
use crate::tasking::{Refusal, refuse};

/// The harness tool with which the agent proposes a mode.
pub(crate) const TOOL: &str = "task.propose_mode";
/// How long a proposal waits for the person (docs/65 AFW-D06).
pub(crate) const TTL_MS: i64 = 15_000;
/// How often the expiry sweep runs.
const TICK: std::time::Duration = std::time::Duration::from_millis(250);
/// The longest reason the log keeps.
const MAX_REASON: usize = 512;

const EVENT_TYPES: &[&str] = &["ModeSwitchProposed", "ModeSwitchDecided"];

/// The proposals awaiting an answer in this Core, with their deadlines. The
/// log is the record; this is only what the sweep walks.
#[derive(Default)]
pub struct Host {
    /// Keyed by task and proposal: two tasks may hold the same call id.
    pending: std::sync::Mutex<HashMap<(TaskId, String), i64>>,
}

impl Host {
    fn add(&self, id: &str, task_id: TaskId, expires_at_ms: i64) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((task_id, id.to_owned()), expires_at_ms);
    }

    fn forget(&self, task_id: TaskId, id: &str) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&(task_id, id.to_owned()));
    }

    fn all(&self) -> Vec<(String, TaskId, i64)> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|((task, id), at)| (id.clone(), *task, *at))
            .collect()
    }
}

// ---- the fold of a task's proposals ----

/// One proposal as the log says.
#[derive(Clone, Debug)]
pub(crate) struct Proposal {
    pub id: String,
    pub task_id: TaskId,
    pub from: Mode,
    pub to: Mode,
    pub reason: String,
    pub proposed_at_ms: i64,
    pub expires_at_ms: i64,
    pub proposed_offset: u64,
    /// `PENDING` | `ACCEPTED` | `DECLINED` | `SKIPPED`.
    pub status: String,
    pub outcome_reason: String,
    pub decided_at_ms: i64,
    pub decided_offset: u64,
}

impl Proposal {
    fn pending(&self) -> bool {
        self.status == "PENDING"
    }

    fn view(&self) -> wire::ModeProposalView {
        wire::ModeProposalView {
            proposal_id: self.id.clone(),
            task_id: Some(crate::server::wire_id(self.task_id.as_bytes())),
            from_mode: crate::tasking::wire_mode(self.from) as i32,
            to_mode: crate::tasking::wire_mode(self.to) as i32,
            reason: self.reason.clone(),
            status: self.status.clone(),
            outcome_reason: self.outcome_reason.clone(),
            proposed_at_ms: self.proposed_at_ms,
            expires_at_ms: self.expires_at_ms,
            decided_at_ms: self.decided_at_ms,
            proposed_offset: self.proposed_offset,
            decided_offset: self.decided_offset,
        }
    }
}

fn fold_events(
    store: &EventStore,
    events: &[modbit_event_store::StoredEvent],
) -> Result<Vec<Proposal>, String> {
    let mut out: Vec<Proposal> = Vec::new();
    for e in events {
        let payload = store
            .payload(&e.envelope)
            .map_err(|e| format!("reading a proposal event: {e}"))?;
        match serde_json::from_value::<TaskEvent>(payload) {
            Ok(TaskEvent::ModeSwitchProposed {
                proposal_id,
                from,
                to,
                reason,
                expires_at_ms,
                ..
            }) => {
                let Some(task_id) = e.envelope.task_id else {
                    continue;
                };
                out.push(Proposal {
                    id: proposal_id,
                    task_id,
                    from,
                    to,
                    reason,
                    proposed_at_ms: e.envelope.occurred_at.millis(),
                    expires_at_ms,
                    proposed_offset: e.offset,
                    status: "PENDING".into(),
                    outcome_reason: String::new(),
                    decided_at_ms: 0,
                    decided_offset: 0,
                });
            }
            Ok(TaskEvent::ModeSwitchDecided {
                proposal_id,
                outcome,
                reason_code,
                ..
            }) => {
                // The first decision stands; the log never holds a second.
                if let Some(p) = out.iter_mut().find(|p| p.id == proposal_id && p.pending()) {
                    p.status = outcome;
                    p.outcome_reason = reason_code;
                    p.decided_at_ms = e.envelope.occurred_at.millis();
                    p.decided_offset = e.offset;
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The proposals of one task, oldest first.
pub(crate) fn proposals_of(store: &EventStore, task: TaskId) -> Result<Vec<Proposal>, String> {
    let events = store
        .read_aggregate_of_types(task.as_bytes(), EVENT_TYPES, 0)
        .map_err(|e| format!("reading the proposals of a task: {e}"))?;
    fold_events(store, &events)
}

// ---- the tool ----

/// The tool's projection.
pub(crate) fn projection() -> modbit_providers::ToolProjection {
    modbit_providers::ToolProjection {
        name: TOOL.into(),
        description: "Ask the user to move this task to another mode (for example from PLAN to AGENT once the plan is ready). It is only a request: nothing changes unless the user accepts it in a card, and an unanswered request is skipped after 15 seconds with the mode unchanged. Keep working in the mode you have; do not assume the answer. At most one request is open at a time.".into(),
        input_schema: serde_json::json!({"type":"object","properties":{
            "mode":{"type":"string","enum":["AGENT","PLAN","DEBUG","MULTITASK","ASK"]},
            "reason":{"type":"string","maxLength":512}
        },"required":["mode","reason"],"additionalProperties":false}),
    }
}

/// Whether the tool is offered to this task: an interactive primary agent
/// (a person is there to answer) whose mode is not the unconstrained default.
pub(crate) fn offered(task: &Task, mode_in_force: Mode) -> bool {
    mode_in_force != Mode::Agent
        && matches!(task.origin, TaskOrigin::Desktop | TaskOrigin::IdeAdapter)
}

fn tool_result(call_id: &str, text: String, progress: bool) -> TranscriptEntry {
    TranscriptEntry::ToolResult {
        call_id: call_id.into(),
        name: TOOL.into(),
        text,
        failure_signature: None,
        clears: vec![],
        wrote: None,
        progress,
        media: vec![],
    }
}

/// `task.propose_mode`: record the proposal and say it is pending. The run is
/// not suspended and the mode is not touched.
pub(crate) async fn handle_tool(
    core: &Arc<Core>,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    call_id: &str,
    arguments_json: &str,
) -> (TranscriptEntry, bool) {
    let v: serde_json::Value = serde_json::from_str(arguments_json).unwrap_or_default();
    let refused = |code: &str, why: String| {
        (
            tool_result(
                call_id,
                format!("status: REFUSED\nerror_code: {code}\nerror: {why}"),
                false,
            ),
            false,
        )
    };
    let Some(to) = v["mode"].as_str().and_then(Mode::parse) else {
        return refused(
            "INVALID_ARGUMENTS",
            "mode must be one of AGENT, PLAN, DEBUG, MULTITASK, ASK".into(),
        );
    };
    let reason: String = v["reason"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(MAX_REASON)
        .collect();
    if reason.is_empty() {
        return refused(
            "INVALID_ARGUMENTS",
            "say in a sentence why the switch would help".into(),
        );
    }
    let mut store = core.store.lock().await;
    let from = match core.tools.tasking.facts(&store, task.task_id) {
        Ok(f) => f.mode,
        Err(e) => return refused("STORE_ERROR", e),
    };
    if to == from {
        return refused(
            "ALREADY_IN_MODE",
            format!("the task is already in {}", from.name()),
        );
    }
    let existing = match proposals_of(&store, task.task_id) {
        Ok(p) => p,
        Err(e) => return refused("STORE_ERROR", e),
    };
    if let Some(open) = existing.iter().find(|p| p.pending()) {
        return refused(
            "PROPOSAL_PENDING",
            format!(
                "proposal {} is still waiting for the user; keep working in {}",
                open.id,
                from.name()
            ),
        );
    }
    let clean: String = call_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(24)
        .collect();
    let proposal_id = format!("mp-{clean}");
    let expires_at_ms = Timestamp::now().0 + TTL_MS;
    if let Err(e) = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "ModeSwitchProposed",
            &TaskEvent::ModeSwitchProposed {
                proposal_id: proposal_id.clone(),
                call_id: call_id.into(),
                from,
                to,
                reason,
                expires_at_ms,
            },
            actor.clone(),
        )],
    ) {
        return refused("STORE_ERROR", e);
    }
    drop(store);
    core.mode_proposals
        .add(&proposal_id, task.task_id, expires_at_ms);
    (
        tool_result(
            call_id,
            format!(
                "status: PENDING\nproposal_id: {proposal_id}\nfrom: {}\nto: {}\nthe user has been asked; the mode is unchanged and stays {} until they accept. Unanswered, it is skipped after 15 seconds. Continue in the current mode.",
                from.name(),
                to.name(),
                from.name()
            ),
            true,
        ),
        true,
    )
}

// ---- deciding ----

fn decided_event(
    outcome: &str,
    reason_code: &str,
    proposal_id: &str,
    resolver: String,
    actor: Actor,
) -> modbit_event_store::NewEvent {
    typed(
        "ModeSwitchDecided",
        &TaskEvent::ModeSwitchDecided {
            proposal_id: proposal_id.into(),
            outcome: outcome.into(),
            reason_code: reason_code.into(),
            resolver,
        },
        actor,
    )
}

/// Close a pending proposal the Core itself ends: expiry, a restart, a task
/// that ended, a mode that moved. Returns whether this call wrote the
/// decision; the log is re-read under the store lock so a person's answer
/// that landed first wins and nothing is written twice. Never an acceptance.
async fn settle(
    core: &Arc<Core>,
    task_id: TaskId,
    session_id: Option<SessionId>,
    proposal_id: &str,
    reason_code: &str,
) -> bool {
    let mut store = core.store.lock().await;
    let still_pending = proposals_of(&store, task_id)
        .map(|p| p.iter().any(|p| p.id == proposal_id && p.pending()))
        .unwrap_or(false);
    if !still_pending {
        core.mode_proposals.forget(task_id, proposal_id);
        return false;
    }
    let session = match session_id {
        Some(s) => s,
        None => match store.task(&task_id) {
            Ok(Some(t)) => t.session_id,
            _ => return false,
        },
    };
    let req = AppendRequest {
        tenant_id: core.tenant_id,
        session_id: session,
        task_id: Some(task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *task_id.as_bytes(),
        expected_sequence: None,
        events: vec![decided_event(
            "SKIPPED",
            reason_code,
            proposal_id,
            "core".into(),
            Actor::Core("mode-proposals".into()),
        )],
    };
    match store.append(req) {
        Ok(ev) => {
            if let Some(last) = ev.last() {
                core.last_offset.send_replace(last.offset);
            }
            core.mode_proposals.forget(task_id, proposal_id);
            true
        }
        Err(e) => {
            eprintln!("modbit-core: skipping proposal {proposal_id}: {e}; tried again");
            false
        }
    }
}

/// End every proposal that is due or whose task ended. Returns how many.
pub(crate) async fn sweep(core: &Arc<Core>) -> usize {
    let now = Timestamp::now().0;
    let mut closed = 0;
    for (id, task_id, expires_at_ms) in core.mode_proposals.all() {
        let ended = {
            let store = core.store.lock().await;
            match store.task(&task_id) {
                Ok(Some(t)) => t.state.is_terminal(),
                _ => false,
            }
        };
        let code = if ended {
            "TASK_ENDED"
        } else if now >= expires_at_ms {
            "UNANSWERED_15S"
        } else {
            continue;
        };
        if settle(core, task_id, None, &id, code).await {
            closed += 1;
        }
    }
    closed
}

/// The start of the Core: a proposal the last Core left pending is skipped
/// (the run that asked is gone), then the 15 s sweep runs for the life of
/// this one.
pub(crate) async fn start(core: &Arc<Core>) {
    let stale: Vec<(TaskId, String)> = {
        let store = core.store.lock().await;
        match store.read_all_of_types_to_end(EVENT_TYPES, 0) {
            Ok(events) => {
                let mut by_task: HashMap<TaskId, Vec<modbit_event_store::StoredEvent>> =
                    HashMap::new();
                for e in events {
                    if let Some(t) = e.envelope.task_id {
                        by_task.entry(t).or_default().push(e);
                    }
                }
                let mut out = Vec::new();
                for (task, evs) in by_task {
                    if let Ok(ps) = fold_events(&store, &evs) {
                        out.extend(
                            ps.into_iter()
                                .filter(Proposal::pending)
                                .map(|p| (task, p.id)),
                        );
                    }
                }
                out
            }
            Err(e) => {
                eprintln!("modbit-core: reading mode proposals: {e}");
                Vec::new()
            }
        }
    };
    let mut closed = 0;
    for (task, id) in stale {
        if settle(core, task, None, &id, "CORE_RESTARTED").await {
            closed += 1;
        }
    }
    if closed > 0 {
        eprintln!(
            "modbit-core: {closed} mode-switch proposal(s) the last Core left pending were skipped"
        );
    }
    let core = Arc::clone(core);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            sweep(&core).await;
        }
    });
}

// ---- commands ----

/// `ListModeProposals` and `DecideModeProposal`.
pub(crate) async fn handle(core: &Arc<Core>, env: &wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    let reject = |r: Refusal| crate::tasking::reject(cid.clone(), r);
    let Some(command_id) = env.command_id.as_ref().and_then(crate::tasking::id16) else {
        return reject(refuse("BAD_COMMAND_ID", "command_id must be 16 bytes"));
    };
    match env.command_type.as_str() {
        "ListModeProposals" => {
            let Ok(p) = wire::ListModeProposals::decode(env.payload.as_slice()) else {
                return reject(refuse("BAD_PAYLOAD", "ListModeProposals"));
            };
            let Some(task_id) = p
                .task_id
                .as_ref()
                .and_then(crate::tasking::id16)
                .map(TaskId::from_bytes)
            else {
                return reject(refuse("BAD_PAYLOAD", "task_id required"));
            };
            // A proposal past its deadline is closed before it is listed.
            sweep(core).await;
            let store = core.store.lock().await;
            match store.task(&task_id) {
                Ok(Some(_)) => {}
                Ok(None) => return reject(refuse("UNKNOWN_TASK", task_id.to_string())),
                Err(e) => return reject(refuse("STORE_ERROR", e.to_string())),
            }
            match proposals_of(&store, task_id) {
                Ok(ps) => crate::server::accept(
                    cid,
                    false,
                    wire::ModeProposalList {
                        task_id: Some(crate::server::wire_id(task_id.as_bytes())),
                        proposals: ps.iter().map(Proposal::view).collect(),
                        now_ms: Timestamp::now().0,
                        offset: store.last_offset().unwrap_or(0),
                    }
                    .encode_to_vec(),
                ),
                Err(e) => reject(refuse("STORE_ERROR", e)),
            }
        }
        _ => {
            let Ok(p) = wire::DecideModeProposal::decode(env.payload.as_slice()) else {
                return reject(refuse("BAD_PAYLOAD", "DecideModeProposal"));
            };
            decide(core, env, cid, command_id, p).await
        }
    }
}

async fn decide(
    core: &Arc<Core>,
    env: &wire::CommandEnvelope,
    cid: Option<wire::Id>,
    command_id: [u8; 16],
    p: wire::DecideModeProposal,
) -> wire::CommandAck {
    let reject = |r: Refusal| crate::tasking::reject(cid.clone(), r);
    let Some(task_id) = p
        .task_id
        .as_ref()
        .and_then(crate::tasking::id16)
        .map(TaskId::from_bytes)
    else {
        return reject(refuse("BAD_PAYLOAD", "task_id required"));
    };
    if p.proposal_id.is_empty() || p.proposal_id.len() > 64 {
        return reject(refuse("BAD_PAYLOAD", "proposal_id required"));
    }
    let task = match crate::tasking::open_task(core, task_id).await {
        Ok(t) => t,
        Err(r) => return reject(r),
    };
    if let Some(ack) = crate::tasking::wrong_session(&cid, env, &task) {
        return ack;
    }
    if let Err(ack) = crate::server::require_lease(core, &cid, env, &task.session_id).await {
        return ack;
    }
    // The same answer sent again (same command id) is replayed, not refused:
    // the proposal it settled is read back from the log.
    {
        let store = core.store.lock().await;
        match store.prior_command(&crate::tasking::record_of(core, env, command_id)) {
            Ok(Some(prior)) => {
                let events = match prior {
                    CommandOutcome::Applied(e) | CommandOutcome::Replayed(e) => e,
                };
                let settled = proposals_of(&store, task_id)
                    .ok()
                    .and_then(|ps| ps.into_iter().find(|x| x.id == p.proposal_id));
                return crate::server::accept(
                    cid,
                    true,
                    wire::ModeProposalDecided {
                        proposal: settled.as_ref().map(Proposal::view),
                        mode_change: None,
                        offset: events.last().map_or(0, |e| e.offset),
                    }
                    .encode_to_vec(),
                );
            }
            Ok(None) => {}
            Err(e) => return reject(refuse(crate::server::error_code(&e), e.to_string())),
        }
    }
    // A proposal past its deadline is skipped by the Core first, so a late
    // answer cannot be an acceptance whatever the sweep's timing.
    sweep(core).await;
    let found = {
        let store = core.store.lock().await;
        match proposals_of(&store, task_id) {
            Ok(ps) => ps.into_iter().find(|x| x.id == p.proposal_id),
            Err(e) => return reject(refuse("STORE_ERROR", e)),
        }
    };
    let Some(proposal) = found else {
        return reject(refuse("UNKNOWN_PROPOSAL", p.proposal_id));
    };
    if !proposal.pending() {
        return reject(refuse(
            "PROPOSAL_NOT_PENDING",
            format!(
                "the proposal ended {} ({}); it can no longer be answered",
                proposal.status, proposal.outcome_reason
            ),
        ));
    }
    if Timestamp::now().0 >= proposal.expires_at_ms {
        settle(
            core,
            task_id,
            Some(task.session_id),
            &proposal.id,
            "UNANSWERED_15S",
        )
        .await;
        return reject(refuse(
            "PROPOSAL_NOT_PENDING",
            "the proposal was not answered within 15 seconds and was skipped".to_owned(),
        ));
    }
    let resolver = format!("user:{}", core.user_id);
    if !p.accept {
        let mut store = core.store.lock().await;
        let still = proposals_of(&store, task_id)
            .map(|ps| ps.iter().any(|x| x.id == proposal.id && x.pending()))
            .unwrap_or(false);
        if !still {
            return reject(refuse(
                "PROPOSAL_NOT_PENDING",
                "the proposal ended while the answer was being recorded",
            ));
        }
        let req = AppendRequest {
            tenant_id: core.tenant_id,
            session_id: task.session_id,
            task_id: Some(task_id),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Task,
            aggregate_id: *task_id.as_bytes(),
            expected_sequence: None,
            events: vec![decided_event(
                "DECLINED",
                "DECLINED_BY_USER",
                &proposal.id,
                resolver,
                Actor::User(core.user_id),
            )],
        };
        return match store.execute_command(crate::tasking::record_of(core, env, command_id), req) {
            Ok(outcome) => {
                let (events, replayed) = match outcome {
                    CommandOutcome::Applied(e) => (e, false),
                    CommandOutcome::Replayed(e) => (e, true),
                };
                let offset = events.last().map_or(0, |e| e.offset);
                if !replayed {
                    core.last_offset.send_replace(offset);
                }
                core.mode_proposals.forget(task_id, &proposal.id);
                let mut settled = proposal.clone();
                settled.status = "DECLINED".into();
                settled.outcome_reason = "DECLINED_BY_USER".into();
                settled.decided_offset = offset;
                settled.decided_at_ms = Timestamp::now().0;
                crate::server::accept(
                    cid,
                    replayed,
                    wire::ModeProposalDecided {
                        proposal: Some(settled.view()),
                        mode_change: None,
                        offset,
                    }
                    .encode_to_vec(),
                )
            }
            Err(e) => reject(refuse(crate::server::error_code(&e), e.to_string())),
        };
    }
    // Accept: the mode changes through the one mode path, and the proposal is
    // closed in the same transaction. The guard re-checks, under the lock the
    // write holds, that the proposal is still pending and that the task is
    // still in the mode it was proposed from.
    let id = proposal.id.clone();
    let from = proposal.from;
    let guard = move |store: &EventStore, facts: &crate::tasking::Facts| -> Result<(), Refusal> {
        let pending = proposals_of(store, task_id)
            .map(|ps| ps.iter().any(|x| x.id == id && x.pending()))
            .unwrap_or(false);
        if !pending {
            return Err(refuse(
                "PROPOSAL_NOT_PENDING",
                "the proposal ended while the answer was being recorded",
            ));
        }
        if facts.mode != from {
            return Err(refuse(
                "MODE_CHANGED",
                format!(
                    "the task is no longer in {}, the mode this was proposed from",
                    from.name()
                ),
            ));
        }
        Ok(())
    };
    let ack = crate::tasking::apply_mode(
        core,
        env,
        cid.clone(),
        command_id,
        &task,
        proposal.to,
        &format!("accepted proposal {}: {}", proposal.id, proposal.reason),
        Some((
            &guard,
            vec![decided_event(
                "ACCEPTED",
                "ACCEPTED_BY_USER",
                &proposal.id,
                resolver,
                Actor::User(core.user_id),
            )],
        )),
    )
    .await;
    if ack.error_code == "MODE_CHANGED" {
        // The mode moved under the proposal: it can never be accepted now.
        settle(
            core,
            task_id,
            Some(task.session_id),
            &proposal.id,
            "MODE_CHANGED",
        )
        .await;
        return ack;
    }
    if ack.error_code.is_empty() {
        core.mode_proposals.forget(task_id, &proposal.id);
        let change = wire::TaskModeChanged::decode(ack.result.as_slice()).ok();
        let offset = change.as_ref().map_or(0, |c| c.offset);
        let mut settled = proposal.clone();
        settled.status = "ACCEPTED".into();
        settled.outcome_reason = "ACCEPTED_BY_USER".into();
        settled.decided_offset = offset;
        settled.decided_at_ms = Timestamp::now().0;
        return crate::server::accept(
            cid,
            ack.status == wire::CommandStatus::Replayed as i32,
            wire::ModeProposalDecided {
                proposal: Some(settled.view()),
                mode_change: change,
                offset,
            }
            .encode_to_vec(),
        );
    }
    ack
}
