//! PX-156 (QUAL-PX-156): time parked on a person's approval is excluded from
//! the call's timeout and from the task's wall-clock budget, and the
//! accounting names it separately. On the real Core: the `modbit-core`
//! binary, its store, the Capability Kernel under an `ASK` policy, the
//! execution broker running real processes; the model is the scripted
//! stand-in.
#![cfg(unix)]

mod px_common;

use std::time::Duration;

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    ApprovalList, ContextAccountingView, GetContextAccounting, Id, ListApprovals, ResolveApproval,
    SetTaskBudgets, TaskBudgetsView,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

const WALL_CAP_MS: u64 = 3_000;
const PARKED_MS: u64 = 3_500;

fn script(shell_args: Value) -> Vec<Value> {
    vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "run one command", "expected_files": [], "protected_effects": ["shell.exec"]}}]}),
        json!({"calls": [{"name": "shell.exec", "args": shell_args}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ]
}

async fn set_wall_cap(c: &mut Client, task: &Id, g: Option<u64>) {
    let ack = c
        .command(envelope_fenced(
            rand_id(),
            "SetTaskBudgets",
            SetTaskBudgets {
                task_id: Some(task.clone()),
                max_cost_minor: 0,
                max_wall_ms: WALL_CAP_MS,
                max_children: 0,
                forbid_spawn: false,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let v: TaskBudgetsView = Client::result(&ack).unwrap();
    assert_eq!(v.max_wall_ms, WALL_CAP_MS, "{v:?}");
}

async fn accounting(c: &mut Client, task: &Id) -> modbit_protocol::v1::BudgetAccountingView {
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
    v.budgets.expect("the accounting carries the budgets")
}

fn shell_results(bodies: &[Value]) -> Vec<String> {
    bodies
        .last()
        .and_then(|b| b["messages"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|m| m["role"] == "tool")
        .filter_map(|m| m["content"].as_str().map(str::to_owned))
        .filter(|t| t.contains("timed_out") || t.contains("exit_code"))
        .collect()
}

/// A call whose timeout is shorter than the time its approval is left
/// waiting completes after the approval with no timeout, and a wall-clock
/// budget shorter than the wait does not end the task: the parked time is
/// named apart from the wall clock used.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_156_a_call_parked_on_an_approval_is_neither_timed_out_nor_charged_to_the_wall_clock()
 {
    let (repo, root) = plain_repo(&[("a.txt", "a\n")]);
    let (base, seen) = scripted_model(
        script(json!({"argv": ["sh", "-c", "echo ran > ran.txt"], "timeout_ms": 1500})),
        vec![],
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("admin-config.json"),
        r#"{"permissions": {"shell.exec": "ASK"}}"#,
    )
    .unwrap();
    let env = model_env(&base);
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x61).await;
    let task = create_task(&mut c, &session, g, &root, 0x62, "local_trusted", "run it").await;
    set_wall_cap(&mut c, &task, g).await;
    start_task(&mut c, &task, g, 0x63, "gpt-5-mini").await;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let approval = loop {
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
        let list: ApprovalList = Client::result(&ack).unwrap();
        if let Some(a) = list.approvals.iter().find(|a| a.status == "REQUESTED") {
            break a.clone();
        }
        assert!(std::time::Instant::now() < deadline, "no approval opened");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(approval.tool_name, "shell.exec");
    // The person takes longer than both the call's timeout and the budget.
    tokio::time::sleep(Duration::from_millis(PARKED_MS)).await;
    let ack = c
        .command(envelope_fenced(
            rand_id(),
            "ResolveApproval",
            ResolveApproval {
                approval_id: approval.approval_id.clone(),
                approve: true,
                reason: "go".into(),
                intent_hash: approval.intent_hash.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: modbit_protocol::v1::ApprovalResolvedAck = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");
    assert_eq!(
        st.state, "ReadyForReview",
        "the wait was the person's: {st:?}"
    );
    assert!(
        repo.path().join("ran.txt").exists(),
        "the call ran after the approval"
    );
    let results = shell_results(&seen.lock().unwrap());
    assert!(
        results
            .iter()
            .any(|t| t.contains("\"timed_out\":false") || t.contains("timed_out: false")),
        "no timeout: {results:?}"
    );
    let b = accounting(&mut c, &task).await;
    assert!(
        b.wall_ms_parked >= PARKED_MS,
        "the parked time is named: {b:?}"
    );
    assert!(b.wall_ms_used < WALL_CAP_MS, "and not charged: {b:?}");
}

/// A call that is not parked still times out on schedule.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_156_a_call_that_is_not_parked_still_times_out_on_schedule() {
    let (_repo, root) = plain_repo(&[("a.txt", "a\n")]);
    let (base, seen) = scripted_model(
        script(json!({"argv": ["sh", "-c", "sleep 5"], "timeout_ms": 400})),
        vec![],
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let env = model_env(&base);
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x64).await;
    let task = create_task(&mut c, &session, g, &root, 0x65, "local_trusted", "run it").await;
    let started = std::time::Instant::now();
    start_task(&mut c, &task, g, 0x66, "gpt-5-mini").await;
    let st = wait_task(&mut c, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it did not wait out the sleep"
    );
    let results = shell_results(&seen.lock().unwrap());
    assert!(
        results
            .iter()
            .any(|t| t.contains("\"timed_out\":true") || t.contains("TIMEOUT")),
        "the call timed out: {results:?}"
    );
    let b = accounting(&mut c, &task).await;
    assert_eq!(b.wall_ms_parked, 0, "{b:?}");
}
