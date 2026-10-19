//! REQ-PX-060 / QUAL-PX-060, the policy leg: "a limit action that changes a
//! budget without the policy's approval is refused". On the real Core: the
//! actual `modbit-core` binary over its real socket and SQLite log, the real
//! approval aggregate and its expiry sweep, a Core killed with SIGKILL and
//! started again on the same data directory. No model is involved: the
//! decision is the Core's, on the task's own log.
//!
//! * a raise above the organization's ceiling is refused with a typed code
//!   whoever asks and whatever an approval says, and the refusal is an event;
//! * a raise within the ceiling is applied for the person holding the session
//!   (rule `within-ceiling`), a lowering never needs a decision;
//! * under `raise_approval=always` a raise opens one exact-intent approval,
//!   leaves the caps alone, is answered `BUDGET_RAISE_APPROVAL_PENDING` while
//!   it waits, applies once approved and asked again, and the approval is
//!   spent by the log moving on;
//! * an unreadable policy refuses every raise (fail closed);
//! * the ceiling holds the effective caps of a task that never set one.
#![cfg(unix)]

mod px_common;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    ApprovalList, ApprovalResolvedAck, ApprovalView, ContextAccountingView, GetContextAccounting,
    Id, ListApprovals, ResolveApproval, SetTaskBudgets, TaskBudgetsView,
};
use prost::Message;
use px_common::*;
use serde_json::Value;

type Refusal = (String, String);

async fn set(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
    cost: u64,
    wall: u64,
) -> Result<TaskBudgetsView, Refusal> {
    match c
        .command(envelope_fenced(
            rand_id(),
            "SetTaskBudgets",
            SetTaskBudgets {
                task_id: Some(task.clone()),
                max_cost_minor: cost,
                max_wall_ms: wall,
                max_children: 0,
                forbid_spawn: false,
            }
            .encode_to_vec(),
            g,
        ))
        .await
    {
        Ok(ack) => Ok(Client::result(&ack).unwrap()),
        Err(ClientError::Rejected { code, message }) => Err((code, message)),
        Err(e) => panic!("SetTaskBudgets: {e}"),
    }
}

/// The caps the Core runs the task under, as the accounting reports them.
async fn caps(c: &mut Client, task: &Id) -> (u64, u64) {
    let ack = c
        .command(envelope(
            rand_id(),
            "GetContextAccounting",
            GetContextAccounting {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let v: ContextAccountingView = Client::result(&ack).unwrap();
    let b = v.budgets.expect("the accounting carries the budgets");
    (b.max_cost_minor, b.max_wall_ms)
}

async fn approvals(c: &mut Client, session: &Id) -> Vec<ApprovalView> {
    let ack = c
        .command(envelope(
            rand_id(),
            "ListApprovals",
            ListApprovals {
                session_id: Some(session.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result::<ApprovalList>(&ack).unwrap().approvals
}

async fn resolve(
    c: &mut Client,
    g: Option<u64>,
    a: &ApprovalView,
    approve: bool,
    hash: &str,
) -> Result<ApprovalResolvedAck, Refusal> {
    match c
        .command(envelope_fenced(
            rand_id(),
            "ResolveApproval",
            ResolveApproval {
                approval_id: a.approval_id.clone(),
                approve,
                reason: "test".into(),
                intent_hash: hash.into(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
    {
        Ok(ack) => Ok(Client::result(&ack).unwrap()),
        Err(ClientError::Rejected { code, message }) => Err((code, message)),
        Err(e) => panic!("ResolveApproval: {e}"),
    }
}

fn code<T: std::fmt::Debug>(r: Result<T, Refusal>) -> String {
    match r {
        Ok(v) => panic!("expected a refusal, the Core accepted: {v:?}"),
        Err((c, _)) => c,
    }
}

fn of(events: &[Value], kind: &str, task: &Id) -> Vec<Value> {
    events
        .iter()
        .filter(|e| e["event_type"] == kind && e["task_id"] == hex_id(task))
        .map(|e| e["payload"]["payload"].clone())
        .collect()
}

fn raise_approvals(all: Vec<ApprovalView>) -> Vec<ApprovalView> {
    all.into_iter()
        .filter(|a| a.tool_name == "task.budget.raise")
        .collect()
}

struct World {
    _repo: tempfile::TempDir,
    dir: tempfile::TempDir,
    root: String,
}

fn world() -> World {
    let (repo, root) = plain_repo(&[("a.txt", "alpha\n")]);
    World {
        _repo: repo,
        dir: tempfile::tempdir().unwrap(),
        root,
    }
}

fn core_with(w: &World, policy: &str, extra: &[(&str, &str)]) -> CoreProcess {
    let mut env = vec![("MODBIT_BUDGET_POLICY", policy)];
    env.extend_from_slice(extra);
    CoreProcess::spawn_with_env(w.dir.path(), &env)
}

async fn lease(c: &mut Client, session: &Id) -> Option<u64> {
    let ack = c
        .command(envelope(
            rand_id(),
            "AcquireSessionLease",
            modbit_protocol::v1::AcquireSessionLease {
                session_id: Some(session.clone()),
                owner: "test".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Some(
        Client::result::<modbit_protocol::v1::SessionLeaseAcquired>(&ack)
            .unwrap()
            .lease_generation,
    )
}

/// A raise above the ceiling is refused typed and recorded, a raise within it
/// is applied, a lowering needs nothing, the ceiling holds a task that never
/// set a cap, and all of it is the same after the Core is killed.
#[tokio::test]
async fn qual_px_060_the_ceiling_refuses_a_raise_typed_records_it_and_survives_a_kill() {
    let w = world();
    let mut core = core_with(&w, "ceiling_cost_minor=1000;ceiling_wall_ms=3600000", &[]);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x60).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &w.root,
        0x61,
        "local_trusted",
        "budgets",
    )
    .await;

    // A task that never set a cap runs under the ceiling.
    assert_eq!(caps(&mut c, &task).await, (1000, 3_600_000));
    // Asking for more than the ceiling on a fresh task, or for no cap, is a raise over what it runs under.
    assert_eq!(
        code(set(&mut c, &task, g, 5000, 60_000).await),
        "BUDGET_CEILING_EXCEEDED"
    );
    assert_eq!(
        code(set(&mut c, &task, g, 500, 0).await),
        "BUDGET_CEILING_EXCEEDED"
    );
    assert_eq!(
        caps(&mut c, &task).await,
        (1000, 3_600_000),
        "a refusal changes nothing"
    );

    // Lowering needs no decision; a raise within the ceiling is the person's.
    set(&mut c, &task, g, 500, 60_000).await.unwrap();
    set(&mut c, &task, g, 800, 60_000).await.unwrap();
    assert_eq!(caps(&mut c, &task).await, (800, 60_000));
    assert_eq!(
        code(set(&mut c, &task, g, 1001, 60_000).await),
        "BUDGET_CEILING_EXCEEDED"
    );
    assert_eq!(
        code(set(&mut c, &task, g, 800, 0).await),
        "BUDGET_CEILING_EXCEEDED"
    );
    assert_eq!(caps(&mut c, &task).await, (800, 60_000));
    // At the ceiling exactly is allowed.
    set(&mut c, &task, g, 1000, 3_600_000).await.unwrap();
    assert!(
        raise_approvals(approvals(&mut c, &session).await).is_empty(),
        "a ceiling is not approvable, so no approval was opened"
    );

    let events = replay(&core, &session).await;
    let sets = of(&events, "TaskBudgetsSet", &task);
    let rules: Vec<&str> = sets
        .iter()
        .map(|e| e["rule"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        rules,
        ["not-a-raise", "within-ceiling", "within-ceiling"],
        "{sets:?}"
    );
    let refused = of(&events, "TaskBudgetRaiseDecided", &task);
    assert_eq!(refused.len(), 4, "{refused:?}");
    assert!(
        refused.iter().all(|e| e["decision"] == "REFUSED"
            && e["code"] == "BUDGET_CEILING_EXCEEDED"
            && e["reason"]
                .as_str()
                .is_some_and(|r| r.contains("organization policy"))),
        "{refused:?}"
    );
    assert_eq!(refused[2]["requested_cost_minor"], 1001);

    // SIGKILL: the log is the state.
    core.kill();
    let core = core_with(&w, "ceiling_cost_minor=1000;ceiling_wall_ms=3600000", &[]);
    let mut c = core.client().await;
    let g = lease(&mut c, &session).await;
    assert_eq!(caps(&mut c, &task).await, (1000, 3_600_000));
    set(&mut c, &task, g, 700, 60_000).await.unwrap();
    assert_eq!(
        code(set(&mut c, &task, g, 1001, 60_000).await),
        "BUDGET_CEILING_EXCEEDED"
    );
    let events = replay(&core, &session).await;
    assert_eq!(of(&events, "TaskBudgetRaiseDecided", &task).len(), 5);
    assert_eq!(of(&events, "TaskBudgetsSet", &task).len(), 4);
}

/// An unreadable policy refuses every raise and still lets a person lower.
#[tokio::test]
async fn qual_px_060_an_unreadable_budget_policy_refuses_every_raise() {
    let w = world();
    let core = core_with(&w, "ceiling_cost_minor=lots", &[]);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x62).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &w.root,
        0x63,
        "local_trusted",
        "budgets",
    )
    .await;
    set(&mut c, &task, g, 100, 1000).await.unwrap();
    assert_eq!(
        code(set(&mut c, &task, g, 200, 1000).await),
        "BUDGET_POLICY_INVALID"
    );
    assert_eq!(
        code(set(&mut c, &task, g, 100, 0).await),
        "BUDGET_POLICY_INVALID"
    );
    set(&mut c, &task, g, 50, 500).await.unwrap();
    assert_eq!(caps(&mut c, &task).await, (50, 500));
    let events = replay(&core, &session).await;
    let refused = of(&events, "TaskBudgetRaiseDecided", &task);
    assert_eq!(refused.len(), 2);
    assert!(refused.iter().all(|e| e["code"] == "BUDGET_POLICY_INVALID"));
}

/// Under `raise_approval=always`: a raise opens one approval that binds the
/// exact caps; the caps stay; it applies once approved and asked again; the
/// approval is spent; a changed ask is another approval; a ceiling is not
/// approvable; and pending approvals survive a SIGKILL of the Core.
#[tokio::test]
async fn qual_px_060_a_raise_that_asks_waits_for_an_exact_intent_approval_and_spends_it() {
    let w = world();
    let policy = "ceiling_cost_minor=1000;raise_approval=always";
    let mut core = core_with(&w, policy, &[]);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x64).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &w.root,
        0x65,
        "local_trusted",
        "budgets",
    )
    .await;
    set(&mut c, &task, g, 100, 0).await.unwrap();

    // The raise asks; nothing changes; one approval of the effect is open.
    let (c1, m1) = set(&mut c, &task, g, 200, 0).await.unwrap_err();
    assert_eq!(c1, "BUDGET_RAISE_NEEDS_APPROVAL", "{m1}");
    assert_eq!(caps(&mut c, &task).await, (100, 0), "caps unchanged");
    let open = raise_approvals(approvals(&mut c, &session).await);
    assert_eq!(open.len(), 1, "{open:?}");
    let a1 = open[0].clone();
    assert_eq!(
        (a1.status.as_str(), a1.effect_class.as_str()),
        ("REQUESTED", "ProtectedWrite")
    );
    let scope: Value = serde_json::from_str(&a1.scope_json).unwrap();
    assert_eq!(scope["ask_reason"], "BUDGET_RAISE");
    assert_eq!(scope["budget_raise"]["to"]["cost_minor"], 200);
    assert!(
        m1.contains(&hex_id_of(&a1)),
        "the refusal names the approval: {m1}"
    );
    // Asking again is answered pending and opens nothing new.
    assert_eq!(
        code(set(&mut c, &task, g, 200, 0).await),
        "BUDGET_RAISE_APPROVAL_PENDING"
    );
    assert_eq!(raise_approvals(approvals(&mut c, &session).await).len(), 1);
    // Another ask is another intent, another approval.
    assert_eq!(
        code(set(&mut c, &task, g, 300, 0).await),
        "BUDGET_RAISE_NEEDS_APPROVAL"
    );
    let two = raise_approvals(approvals(&mut c, &session).await);
    assert_eq!(two.len(), 2);
    let a2 = two
        .iter()
        .find(|a| a.approval_id != a1.approval_id)
        .unwrap()
        .clone();
    assert_ne!(a1.intent_hash, a2.intent_hash);
    // A ceiling is not approvable and opens no approval.
    assert_eq!(
        code(set(&mut c, &task, g, 2000, 0).await),
        "BUDGET_CEILING_EXCEEDED"
    );
    assert_eq!(raise_approvals(approvals(&mut c, &session).await).len(), 2);
    // A decision on another intent than the card's is refused.
    assert_eq!(
        code(resolve(&mut c, g, &a1, true, &a2.intent_hash).await),
        "INTENT_MISMATCH"
    );

    // SIGKILL with both pending: both are still pending, still unapplied.
    core.kill();
    let core = core_with(&w, policy, &[]);
    let mut c = core.client().await;
    let g = lease(&mut c, &session).await;
    let after = raise_approvals(approvals(&mut c, &session).await);
    assert_eq!(after.len(), 2);
    assert!(after.iter().all(|a| a.status == "REQUESTED"), "{after:?}");
    assert_eq!(
        code(set(&mut c, &task, g, 200, 0).await),
        "BUDGET_RAISE_APPROVAL_PENDING"
    );
    assert_eq!(caps(&mut c, &task).await.0, 100);

    // Approve the first; asked again, it applies, under that approval.
    assert_eq!(
        resolve(&mut c, g, &a1, true, &a1.intent_hash)
            .await
            .unwrap()
            .status,
        "APPROVED"
    );
    let ok = set(&mut c, &task, g, 200, 0).await.unwrap();
    assert_eq!(ok.max_cost_minor, 200);
    assert_eq!(caps(&mut c, &task).await.0, 200);
    let events = replay(&core, &session).await;
    let applied = of(&events, "TaskBudgetsSet", &task);
    let last = applied.last().unwrap();
    assert_eq!(
        (last["rule"].as_str(), last["approval_id"].as_str()),
        (Some("approved"), Some(hex_id_of(&a1).as_str())),
        "{applied:?}"
    );

    // Spent: the second approval (for 300, bound to the log as it was) names a
    // raise the log has moved past; approving it authorizes nothing, and a
    // new ask opens a new approval.
    assert_eq!(
        resolve(&mut c, g, &a2, true, &a2.intent_hash)
            .await
            .unwrap()
            .status,
        "APPROVED"
    );
    assert_eq!(
        code(set(&mut c, &task, g, 300, 0).await),
        "BUDGET_RAISE_NEEDS_APPROVAL"
    );
    assert_eq!(caps(&mut c, &task).await.0, 200);
    // Lowering and raising back to the very same numbers is a new raise: the first approval is not reusable.
    set(&mut c, &task, g, 100, 0).await.unwrap();
    assert_eq!(
        code(set(&mut c, &task, g, 200, 0).await),
        "BUDGET_RAISE_NEEDS_APPROVAL"
    );
    assert_eq!(caps(&mut c, &task).await.0, 100);

    // A denial leaves the caps; the person may ask again and is asked again.
    let pending: Vec<ApprovalView> = raise_approvals(approvals(&mut c, &session).await)
        .into_iter()
        .filter(|a| a.status == "REQUESTED")
        .collect();
    assert_eq!(pending.len(), 2, "the 300 ask and the 200 ask: {pending:?}");
    for a in &pending {
        assert_eq!(
            resolve(&mut c, g, a, false, &a.intent_hash)
                .await
                .unwrap()
                .status,
            "DENIED"
        );
    }
    assert_eq!(
        code(set(&mut c, &task, g, 200, 0).await),
        "BUDGET_RAISE_NEEDS_APPROVAL"
    );
    assert_eq!(caps(&mut c, &task).await.0, 100);

    let events = replay(&core, &session).await;
    let asked = of(&events, "TaskBudgetRaiseDecided", &task);
    assert!(
        asked.iter().any(|e| e["decision"] == "APPROVAL_REQUESTED"
            && e["code"] == "BUDGET_RAISE_NEEDS_APPROVAL"
            && e["approval_id"].is_string()
            && e["intent_hash"].is_string()),
        "{asked:?}"
    );
    assert!(asked.iter().any(|e| e["code"] == "BUDGET_CEILING_EXCEEDED"));
}

fn hex_id_of(a: &ApprovalView) -> String {
    // The approval id as the events spell it (a UUID text).
    let b: [u8; 16] = a
        .approval_id
        .as_ref()
        .unwrap()
        .value
        .clone()
        .try_into()
        .unwrap();
    modbit_domain::ApprovalId::from_bytes(b).to_string()
}

/// An approval of a raise that nobody answers expires by the Core's own
/// sweep; it can no longer be approved, authorizes nothing, and a new ask
/// opens a new one.
#[tokio::test]
async fn qual_px_060_an_unanswered_raise_approval_expires_and_asks_again() {
    let w = world();
    let core = core_with(
        &w,
        "raise_approval=always",
        &[("MODBIT_APPROVAL_TTL_MS", "2000")],
    );
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x66).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &w.root,
        0x67,
        "local_trusted",
        "budgets",
    )
    .await;
    set(&mut c, &task, g, 100, 0).await.unwrap();
    assert_eq!(
        code(set(&mut c, &task, g, 200, 0).await),
        "BUDGET_RAISE_NEEDS_APPROVAL"
    );
    let first = raise_approvals(approvals(&mut c, &session).await)[0].clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let a = raise_approvals(approvals(&mut c, &session).await);
        if a.iter()
            .any(|x| x.approval_id == first.approval_id && x.status == "EXPIRED")
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "never expired: {a:?}");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        code(resolve(&mut c, g, &first, true, &first.intent_hash).await),
        "APPROVAL_EXPIRED"
    );
    assert_eq!(
        code(set(&mut c, &task, g, 200, 0).await),
        "BUDGET_RAISE_NEEDS_APPROVAL"
    );
    assert_eq!(caps(&mut c, &task).await.0, 100);
    assert_eq!(raise_approvals(approvals(&mut c, &session).await).len(), 2);
}
