//! PX-131 (QUAL-PX-131) on the real Core: a model round runs against one
//! frozen capability snapshot under one authorization epoch.
//!
//! Real: the `modbit-core` binary and its socket, the SQLite store, the
//! broker and its PTY/process runtime (`shell.exec` runs real processes), the
//! Capability Kernel, the receipt chain, policy files on disk. Stand-in: the
//! model, a scripted OpenAI-compatible server (the repository's standard
//! stand-in). Unix only, like the other suites that run `sh`.
#![cfg(unix)]

mod px_common;

use std::time::Duration;

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    ApprovalList, ApprovalResolvedAck, CapabilitySnapshotList, EffectReceiptList, EmergencyStop,
    GetCapabilitySnapshots, GetEffectReceipts, Id, ListApprovals, ResolveApproval,
    SessionLeaseAcquired, StartTask, TaskRunStarted,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

fn plan() -> Value {
    json!({"calls": [{"name": "plan.update", "args": {"outcome": "run two checks", "expected_files": []}}]})
}

fn complete() -> Value {
    json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]})
}

fn shell(script: &str) -> Value {
    json!({"name": "shell.exec", "args": {"argv": ["sh", "-c", script]}})
}

fn env_for(base: &str) -> Vec<(String, String)> {
    model_env(base)
}

fn spawn(dir: &std::path::Path, base: &str) -> CoreProcess {
    let env = env_for(base);
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    CoreProcess::spawn_with_env(dir, &refs)
}

async fn snapshots(core: &CoreProcess, task: &Id) -> CapabilitySnapshotList {
    let mut c = core.client().await;
    let ack = c
        .command(envelope(
            rand_id(),
            "GetCapabilitySnapshots",
            GetCapabilitySnapshots {
                task_id: Some(task.clone()),
                after_epoch: 0,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

fn task_events(all: &[Value], task: &Id, ty: &str) -> Vec<Value> {
    all.iter()
        .filter(|e| e["event_type"] == ty && e["task_id"].as_str() == Some(&hex_id(task)))
        .map(|e| {
            // The log wraps an inline payload; tests read the event itself.
            let mut e = e.clone();
            let inner = e["payload"]["payload"].clone();
            if !inner.is_null() {
                e["payload"] = inner;
            }
            e
        })
        .collect()
}

async fn wait_for<F: FnMut(&[Value]) -> bool>(
    core: &CoreProcess,
    session: &Id,
    secs: u64,
    mut done: F,
) -> Vec<Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let evs = replay(core, session).await;
        if done(&evs) {
            return evs;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the awaited event never came: {:#?}",
            evs.iter()
                .map(|e| e["event_type"].clone())
                .collect::<Vec<_>>()
        );
    }
}

/// A call to `shell.exec` has been dispatched (it is running).
fn shell_dispatched(evs: &[Value], task: &Id) -> bool {
    task_events(evs, task, "ToolCallProposed")
        .iter()
        .any(|e| e["payload"]["tool_name"] == "shell.exec")
        && !task_events(evs, task, "ToolCallDispatched").is_empty()
}

/// The model of the two-call round: round 1 plans, round 2 holds a slow
/// call and one more, round 3 tries again, round 4 completes. Replies are
/// keyed by the tool results the request carries (1, then 1 + 2, then 3 + 1).
fn two_call_round_model() -> Reply {
    std::sync::Arc::new(|_body, results| match results {
        0 => plan(),
        1 => json!({"calls": [
            shell("sleep 3; echo first-done > first-ran"),
            shell("touch second-ran"),
        ]}),
        3 => json!({"calls": [shell("touch third-ran")]}),
        _ => complete(),
    })
}

/// QUAL-PX-131: a policy tightened while a round is running is not applied to
/// the rest of that round and is applied from the next; both epochs are on
/// the decisions, and the snapshots record which tools each round offered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_131_a_policy_tightened_mid_round_applies_from_the_next_round_with_both_epochs_recorded()
 {
    let (repo, root) = plain_repo(&[("notes.txt", "n\n")]);
    let (base, _seen) = scripted_model_fn(two_call_round_model()).await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &base);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x31).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x32,
        "local_trusted",
        "two checks",
    )
    .await;
    start_task(&mut c, &task, g, 0x33, "gpt-5-mini").await;
    // The slow call is running: the organization tightens the policy now.
    wait_for(&core, &session, 60, |e| shell_dispatched(e, &task)).await;
    std::fs::write(
        dir.path().join("admin-config.json"),
        r#"{"permissions": {"shell.exec": "DENY"}}"#,
    )
    .unwrap();
    let st = wait_task(&mut c, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");
    let evs = replay(&core, &session).await;

    // The rest of the round it was tightened in ran under the policy the round
    // was frozen with; the round after it did not get the tool.
    assert!(
        repo.path().join("second-ran").exists(),
        "the round's remaining call is decided under the round's frozen policy"
    );
    assert!(repo.path().join("first-ran").exists());
    assert!(
        !repo.path().join("third-ran").exists(),
        "from the next round the tightened policy applies"
    );

    let snaps = snapshots(&core, &task).await;
    assert!(snaps.valid, "{}", snaps.detail);
    assert!(snaps.snapshots.len() >= 3, "{snaps:#?}");
    for (i, s) in snaps.snapshots.iter().enumerate() {
        assert_eq!(s.epoch, i as u64 + 1, "monotonic from 1, no gap");
    }
    let (r2, r3) = (&snaps.snapshots[1], &snaps.snapshots[2]);
    assert_ne!(
        r2.config_generation, r3.config_generation,
        "the round after the change is decided under a new generation"
    );
    assert_eq!(
        snaps.snapshots[0].config_generation, r2.config_generation,
        "nothing changed between rounds 1 and 2"
    );
    assert!(r2.projected_tools.iter().any(|t| t == "shell.exec"));
    assert!(
        !r3.projected_tools.iter().any(|t| t == "shell.exec"),
        "{r3:?}"
    );
    assert_eq!(r2.mode, "AGENT");
    assert!(!r2.lease_id.is_empty() && r2.lease_generation >= 1);

    // Each decision names the epoch of the round that made it.
    let decisions = task_events(&evs, &task, "ToolCallPolicyDecision");
    let stamped = |e: &Value| e["payload"]["authorization"]["epoch"].as_u64();
    let allowed: Vec<&Value> = decisions
        .iter()
        .filter(|e| e["payload"]["allowed"] == true)
        .collect();
    // plan.update (round 1), the two shell calls (round 2), then a refused
    // attempt in round 3 and task.complete.
    let shell_decisions: Vec<u64> = allowed
        .iter()
        .filter(|e| {
            e["payload"]["decision"]
                .as_str()
                .is_some_and(|d| d.contains("reversiblewrite"))
        })
        .filter_map(|e| stamped(e))
        .collect();
    assert_eq!(
        shell_decisions.iter().filter(|e| **e == 2).count(),
        2,
        "both calls of round 2 carry epoch 2: {shell_decisions:?} {decisions:#?}"
    );
    for d in &decisions {
        let a = &d["payload"]["authorization"];
        assert!(a["epoch"].as_u64().is_some(), "an unstamped decision: {d}");
        let epoch = a["epoch"].as_u64().unwrap();
        let hash = snaps
            .snapshots
            .iter()
            .find(|s| s.epoch == epoch)
            .map(|s| s.snapshot_hash.clone());
        assert_eq!(
            hash.as_deref(),
            a["snapshot_hash"].as_str(),
            "the stamp names the recorded snapshot"
        );
    }
    let refused = decisions
        .iter()
        .find(|e| {
            e["payload"]["allowed"] == false
                && e["payload"]["decision"]
                    .as_str()
                    .is_some_and(|d| d.starts_with("TOOL_NOT_PROJECTED"))
        })
        .expect("the third round's attempt is refused at the projection");
    assert_eq!(stamped(refused), Some(3));
    // The recorded events agree with the read model.
    assert_eq!(
        task_events(&evs, &task, "CapabilitySnapshotRecorded").len(),
        snaps.snapshots.len()
    );
    assert_eq!(task_events(&evs, &task, "PolicyGenerationChanged").len(), 1);
}

/// An emergency stop is not deferred to the round boundary: the round's
/// remaining call never runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_131_an_emergency_stop_takes_effect_inside_the_round() {
    let (repo, root) = plain_repo(&[("notes.txt", "n\n")]);
    let (base, _seen) = scripted_model_fn(two_call_round_model()).await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &base);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x41).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x42,
        "local_trusted",
        "two checks",
    )
    .await;
    start_task(&mut c, &task, g, 0x43, "gpt-5-mini").await;
    wait_for(&core, &session, 60, |e| shell_dispatched(e, &task)).await;
    c.command(envelope_fenced(
        id16(0x44),
        "EmergencyStop",
        EmergencyStop {
            session_id: Some(session.clone()),
            reason: "test".into(),
        }
        .encode_to_vec(),
        g,
    ))
    .await
    .unwrap();
    let st = wait_task(&mut c, &task, 60).await;
    assert!(!st.loop_alive, "{st:?}");
    assert!(
        !repo.path().join("second-ran").exists(),
        "the stop applies at once, not at the next boundary"
    );
    assert!(!repo.path().join("third-ran").exists());
}

async fn acquire(c: &mut Client, session: &Id, n: u8) -> Option<u64> {
    let ack = c
        .command(envelope(
            id16(n),
            "AcquireSessionLease",
            modbit_protocol::v1::AcquireSessionLease {
                session_id: Some(session.clone()),
                owner: "resumer".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Some(
        Client::result::<SessionLeaseAcquired>(&ack)
            .unwrap()
            .lease_generation,
    )
}

/// QUAL-PX-131: SIGKILL the Core in the middle of a round (a call running),
/// change the policy on disk while it is down, restart and resume. The rest
/// of the interrupted round is decided under the snapshot the round was
/// frozen with — the same epoch, not a new one — and the policy the files now
/// say applies from the next round. Epochs stay monotonic across the kill.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_131_a_core_killed_mid_round_resumes_the_same_snapshot_and_epoch() {
    let (repo, root) = plain_repo(&[("notes.txt", "n\n")]);
    let (base, _seen) = scripted_model_fn(two_call_round_model()).await;
    let dir = tempfile::tempdir().unwrap();
    let mut core = spawn(dir.path(), &base);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x51).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x52,
        "local_trusted",
        "two checks",
    )
    .await;
    start_task(&mut c, &task, g, 0x53, "gpt-5-mini").await;
    wait_for(&core, &session, 60, |e| shell_dispatched(e, &task)).await;
    let before = snapshots(&core, &task).await;
    let in_round = before.snapshots.last().unwrap().clone();
    assert_eq!(in_round.epoch, 2, "the slow call belongs to round 2");
    // A real kill, mid-round.
    core.kill();
    drop(c);
    // While it is down the policy tightens.
    std::fs::write(
        dir.path().join("admin-config.json"),
        r#"{"permissions": {"shell.exec": "DENY"}}"#,
    )
    .unwrap();
    let core2 = spawn(dir.path(), &base);
    let mut c2 = core2.client().await;
    let g2 = acquire(&mut c2, &session, 0x54).await;
    let ack = c2
        .command(envelope_fenced(
            id16(0x55),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 14,
                max_tool_calls: 0,
                max_no_progress_turns: 6,
                skills: vec![],
                ..Default::default()
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    let started: TaskRunStarted = Client::result(&ack).unwrap();
    assert!(started.resumed);
    let st = wait_task(&mut c2, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");
    let evs = replay(&core2, &session).await;
    let snaps = snapshots(&core2, &task).await;
    assert!(snaps.valid, "{}", snaps.detail);
    let epochs: Vec<u64> = snaps.snapshots.iter().map(|s| s.epoch).collect();
    assert_eq!(
        epochs,
        (1..=epochs.len() as u64).collect::<Vec<_>>(),
        "monotonic, gapless, and the interrupted round was not frozen twice"
    );
    // The remaining call of the interrupted round was decided under epoch 2
    // and ran, though the files now forbid the tool.
    assert!(
        repo.path().join("second-ran").exists(),
        "the interrupted round finishes under the snapshot it began with"
    );
    let second = task_events(&evs, &task, "ToolCallPolicyDecision")
        .into_iter()
        .filter(|e| e["payload"]["allowed"] == true)
        .filter(|e| e["payload"]["authorization"]["epoch"] == 2)
        .count();
    assert!(second >= 1, "a decision of the resumed round names epoch 2");
    let resumed_snapshot = snaps.snapshots.iter().find(|s| s.epoch == 2).unwrap();
    assert_eq!(
        resumed_snapshot.snapshot_hash, in_round.snapshot_hash,
        "replay yields the same snapshot"
    );
    // From the next round the new policy applies.
    assert!(!repo.path().join("third-ran").exists());
    let last = snaps.snapshots.last().unwrap();
    assert!(last.epoch >= 3);
    assert_ne!(last.config_generation, in_round.config_generation);
    assert!(!last.projected_tools.iter().any(|t| t == "shell.exec"));
}

/// Kill the Core right after a snapshot is recorded (armed fault, before the
/// round's model is asked): the restarted run continues the sequence.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_131_a_core_killed_between_epoch_changes_continues_the_sequence() {
    let (_repo, root) = plain_repo(&[("notes.txt", "n\n")]);
    let (base, _seen) = scripted_model_fn(two_call_round_model()).await;
    let dir = tempfile::tempdir().unwrap();
    let env = env_for(&base);
    let mut armed: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    armed.push((
        "MODBIT_FAULT_KILL_AFTER_EVENT",
        "CapabilitySnapshotRecorded:2",
    ));
    let core = CoreProcess::spawn_with_env(dir.path(), &armed);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x61).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x62,
        "local_trusted",
        "two checks",
    )
    .await;
    let _ = c
        .command(envelope_fenced(
            id16(0x63),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 14,
                max_tool_calls: 0,
                max_no_progress_turns: 6,
                skills: vec![],
                ..Default::default()
            }
            .encode_to_vec(),
            g,
        ))
        .await;
    // The armed Core aborts the moment epoch 2 is durable.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while c
        .command(envelope(rand_id(), "GetRecoveryReport", vec![]))
        .await
        .is_ok()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the fault never fired"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    drop(c);
    let core2 = spawn(dir.path(), &base);
    let held = snapshots(&core2, &task).await;
    assert!(held.valid, "{}", held.detail);
    let epochs: Vec<u64> = held.snapshots.iter().map(|s| s.epoch).collect();
    assert_eq!(epochs, vec![1, 2], "epoch 2 is durable, nothing after it");
    let mut c2 = core2.client().await;
    let g2 = acquire(&mut c2, &session, 0x64).await;
    let ack = c2
        .command(envelope_fenced(
            id16(0x65),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 14,
                max_tool_calls: 0,
                max_no_progress_turns: 6,
                skills: vec![],
                ..Default::default()
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c2, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");
    let after = snapshots(&core2, &task).await;
    assert!(after.valid, "{}", after.detail);
    let epochs: Vec<u64> = after.snapshots.iter().map(|s| s.epoch).collect();
    assert!(epochs.len() >= 3, "{epochs:?}");
    assert_eq!(epochs, (1..=epochs.len() as u64).collect::<Vec<_>>());
    assert_eq!(
        after.snapshots[1].snapshot_hash, held.snapshots[1].snapshot_hash,
        "the durable record is untouched by the restart"
    );
}

async fn receipts_of(c: &mut Client, task: &Id) -> EffectReceiptList {
    let ack = c
        .command(envelope(
            rand_id(),
            "GetEffectReceipts",
            GetEffectReceipts {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

/// Receipts carry the epoch of their decision; a forged receipt (re-sealed so
/// its own hash is true) that names another epoch fails verification.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_131_receipts_carry_the_decision_epoch_and_a_forged_epoch_fails_the_chain() {
    let (repo, root) = plain_repo(&[("a.txt", "a\n")]);
    let wt = repo.path().join("wt-epoch");
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["worktree", "add", "-q", "-b", "task/epoch"])
            .arg(&wt)
            .status()
            .unwrap()
            .success()
    );
    let wt_s = wt
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "close the stale worktree", "expected_files": [], "protected_effects": ["git.worktree.close"]}}]}),
        json!({"calls": [{"name": "git.worktree.close", "args": {"path": wt_s}}]}),
        complete(),
    ];
    let (base, _seen) = scripted_model(script, vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let core = spawn(dir.path(), &base);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x71).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x72,
        "local_trusted",
        "close it",
    )
    .await;
    start_task(&mut c, &task, g, 0x73, "gpt-5-mini").await;
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
    let ack = c
        .command(envelope_fenced(
            id16(0x74),
            "ResolveApproval",
            ResolveApproval {
                approval_id: approval.approval_id.clone(),
                approve: true,
                reason: "ok".into(),
                intent_hash: approval.intent_hash.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    assert_eq!(
        Client::result::<ApprovalResolvedAck>(&ack).unwrap().status,
        "APPROVED"
    );
    let st = wait_task(&mut c, &task, 120).await;
    assert!(!st.loop_alive, "{st:?}");

    let held = receipts_of(&mut c, &task).await;
    assert!(held.chain_valid, "{}", held.detail);
    assert_eq!(held.receipts.len(), 2, "{held:?}");
    let snaps = snapshots(&core, &task).await;
    let evs = replay(&core, &session).await;
    let close_epoch = held.receipts[0].authorization_epoch;
    assert!(close_epoch >= 2, "a round's epoch, not a default: {held:?}");
    for r in &held.receipts {
        assert_eq!(r.authorization_epoch, close_epoch, "{r:?}");
        let snap = snaps
            .snapshots
            .iter()
            .find(|s| s.epoch == r.authorization_epoch)
            .expect("the receipt's epoch has a recorded snapshot");
        assert_eq!(r.capability_snapshot_hash, snap.snapshot_hash);
    }
    // The decision at dispatch names the same epoch.
    let dispatched_decision = task_events(&evs, &task, "ToolCallPolicyDecision")
        .into_iter()
        .find(|e| {
            e["payload"]["allowed"] == true
                && e["payload"]["decision"]
                    .as_str()
                    .is_some_and(|d| d.starts_with("approval:"))
        })
        .expect("the approved call's decision");
    assert_eq!(
        dispatched_decision["payload"]["authorization"]["epoch"],
        close_epoch
    );

    // Forge: the result receipt claims another epoch, re-sealed so its own
    // hash is true (it is the tail, so nothing after it breaks).
    drop(c);
    let mut core = core;
    core.kill();
    {
        let store = modbit_event_store::EventStore::open(&dir.path().join("core")).unwrap();
        let all = store.receipts(None).unwrap();
        let mut forged = all.last().unwrap().clone();
        forged.authorization.as_mut().unwrap().epoch = close_epoch + 1;
        let forged = modbit_policy::ledger::seal(forged);
        assert!(
            modbit_policy::ledger::verify_chain(&[all[0].clone(), forged.clone()]).is_ok(),
            "the forgery is a valid hash chain"
        );
        let conn = rusqlite::Connection::open(dir.path().join("core").join("core.db")).unwrap();
        conn.execute(
            "UPDATE effect_receipts SET authorization_epoch = ?1, receipt_hash = ?2 WHERE effect_id = ?3",
            rusqlite::params![
                i64::try_from(forged.authorization.as_ref().unwrap().epoch).unwrap(),
                forged.receipt_hash,
                forged.effect_id.as_bytes().as_slice(),
            ],
        )
        .unwrap();
    }
    let core2 = spawn(dir.path(), &base);
    let mut c2 = core2.client().await;
    let tampered = receipts_of(&mut c2, &task).await;
    assert!(
        !tampered.chain_valid,
        "a receipt whose epoch differs from its decision fails verification"
    );
    assert!(tampered.detail.contains("epoch"), "{}", tampered.detail);
}
