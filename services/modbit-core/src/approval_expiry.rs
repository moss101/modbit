//! Expiry of pending approvals (REQ-PX-058; docs/65 AFW-F01, AFW-F02, AFW-F09;
//! QUAL-PX-058: "an approval that expired shows as expired and asks again").
//!
//! An approval binds the intent hash, the scope and an expiry (docs/23). The
//! expiry was always recorded on `ApprovalRequested` (`expires_at`) and
//! checked when an approved approval is spent, but nothing ever closed a
//! *pending* one: it stayed `REQUESTED` after its expiry, and a late `Run`
//! could still approve it. This module is the Core's owner of that close:
//!
//! * a pending approval whose recorded `expires_at` has passed is resolved by
//!   the Core as `ApprovalExpired` (one decision event on the approval's own
//!   log, atomic with the tool call's `ToolCallFailed { APPROVAL_EXPIRED }`),
//!   so the waiting task is told in a typed way and the effect never runs;
//! * the sweep runs once at start (an approval pending past its expiry when
//!   the last Core died is expired before any client can look, with the same
//!   events a live sweep writes) and then twice a second, and lazily on every
//!   `ListApprovals` and `ResolveApproval`;
//! * expiry only ever moves an approval to `EXPIRED`: it never approves,
//!   never widens anything, and a clock that steps backwards merely delays the
//!   close (the approval stays pending and still needs a person). The decision
//!   is a recorded event with its own time, so replaying the log reproduces it
//!   whatever the clock says later;
//! * an approval of an automation run is left to the automation host, whose
//!   own wait limit also cancels the run with a typed reason (AUT-D01).
//!
//! The duration is policy data (`MODBIT_APPROVAL_TTL_MS`, default 24 h: the
//! value `docs/68` AUT-D01 gives for unattended approvals and the one this
//! Core already recorded as `expires_at`); the spec states no interactive
//! value, so the owner can change it by a later record.

use std::sync::{Arc, OnceLock};

use modbit_domain::Timestamp;
use modbit_domain::approval::{Approval, ApprovalEvent, ApprovalState};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::TaskOrigin;
use modbit_domain::toolcall::{ToolCallEvent, ToolCallState};
use modbit_event_store::AppendRequest;

use crate::runtime::typed;
use crate::server::Core;

/// The default time a protected effect may wait for a person (24 h).
pub(crate) const DEFAULT_TTL_MS: i64 = 24 * 60 * 60 * 1000;
/// The shortest configurable wait; anything smaller is ignored.
const MIN_TTL_MS: i64 = 1_000;
/// How often the sweep runs.
const SWEEP_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

/// How long a requested approval stays open, from its request.
pub(crate) fn ttl_ms() -> i64 {
    static TTL: OnceLock<i64> = OnceLock::new();
    *TTL.get_or_init(|| {
        std::env::var("MODBIT_APPROVAL_TTL_MS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v >= MIN_TTL_MS)
            .unwrap_or(DEFAULT_TTL_MS)
    })
}

/// Whether `a` is pending and past its recorded expiry at `now`.
pub(crate) fn is_due(a: &Approval, now: Timestamp) -> bool {
    a.state == ApprovalState::Requested && a.expires_at.is_some_and(|e| now.0 >= e.0)
}

/// Close `a` as expired if it is still pending and due. Returns whether this
/// call wrote the decision. The record is re-read under the store lock, so a
/// decision made in the meantime wins and nothing is written twice.
pub(crate) async fn expire(core: &Arc<Core>, a: &Approval) -> bool {
    let mut store = core.store.lock().await;
    let Ok(Some(cur)) = store.approval(&a.approval_id) else {
        return false;
    };
    if !is_due(&cur, Timestamp::now()) {
        return false;
    }
    let Ok(Some(task)) = store.task(&cur.task_id) else {
        return false;
    };
    let actor = Actor::Core("approval-expiry".into());
    let mut reqs = vec![AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task.task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Approval,
        aggregate_id: *cur.approval_id.as_bytes(),
        expected_sequence: Some(cur.generation),
        events: vec![typed(
            "ApprovalExpired",
            &ApprovalEvent::ApprovalExpired,
            actor.clone(),
        )],
    }];
    // The waiting call is failed in the same transaction, so a task never
    // sees an expired approval beside a call that still waits for it.
    if let Ok(Some(call)) = store.tool_call(&cur.tool_call_id)
        && call.state == ToolCallState::ApprovalPending
    {
        reqs.push(AppendRequest {
            tenant_id: core.tenant_id,
            session_id: task.session_id,
            task_id: Some(task.task_id),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::ToolCall,
            aggregate_id: *call.tool_call_id.as_bytes(),
            expected_sequence: Some(call.generation),
            events: vec![typed(
                "ToolCallFailed",
                &ToolCallEvent::ToolCallFailed {
                    failure_code: "APPROVAL_EXPIRED".into(),
                    result_ref: None,
                },
                actor,
            )],
        });
    }
    match store.append_all(reqs, None) {
        Ok(events) => {
            if let Some(last) = events.last() {
                core.last_offset.send_replace(last.offset);
            }
            true
        }
        Err(e) => {
            eprintln!(
                "modbit-core: expiring approval {}: {e}; it stays pending and is tried again",
                cur.approval_id
            );
            false
        }
    }
}

/// Expire every pending approval that is due. Returns how many were closed.
pub(crate) async fn sweep(core: &Arc<Core>) -> usize {
    let now = Timestamp::now();
    let due: Vec<Approval> = {
        let store = core.store.lock().await;
        store
            .requested_approvals()
            .unwrap_or_default()
            .into_iter()
            .filter(|a| is_due(a, now))
            .collect()
    };
    let mut closed = 0;
    for a in due {
        let automation = {
            let store = core.store.lock().await;
            matches!(
                store.task(&a.task_id),
                Ok(Some(t)) if t.origin == TaskOrigin::Automation
            )
        };
        if automation {
            continue;
        }
        if expire(core, &a).await {
            closed += 1;
        }
    }
    closed
}

/// The start of the Core: close what the last Core left pending past its
/// expiry, then keep sweeping for the life of this one.
pub(crate) async fn start(core: &Arc<Core>) {
    let closed = sweep(core).await;
    if closed > 0 {
        eprintln!(
            "modbit-core: {closed} approval(s) the last Core left pending past their expiry were expired"
        );
    }
    let core = Arc::clone(core);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SWEEP_EVERY).await;
            sweep(&core).await;
        }
    });
}
