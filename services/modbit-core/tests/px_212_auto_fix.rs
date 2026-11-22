//! PX-212 (QUAL-PX-212, DR-PX-2026-11-11-021 MWS-17): auto-fix — with the
//! task's switch on, a failing check the Core reads, or a review comment the
//! existing ingestion accepts, queues one repair input naming its cause and
//! asks for the task's turn through the ordinary start; the turn runs under
//! the task's own run mode, so its push asks as a person's would; an attempt
//! cap stops the loop and says so; with the switch off nothing is queued.
//!
//! The real `modbit-core` binary over its real socket, the real Capability
//! Kernel, approvals, input queue and event store, a real git repository
//! pushing to a real bare remote, a scripted model, and the GitHub REST fake
//! over a real TCP socket (`tests/support/github_fake.rs`). What this is
//! NOT: a proof against real GitHub (the read paths are PX-125's and
//! PX-008's, proven there).

#[path = "px_common/forge_support.rs"]
mod forge_support;
#[path = "../../../tests/support/github_fake.rs"]
mod github_fake;
mod px_common;

use std::process::Command;
use std::time::{Duration, Instant};

use forge_support::*;
use github_fake::GithubFake;
use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    AcquireSessionLease, AgentHeader, AgentHeaders, ApprovalList, AutoFixState, GetAgentHeaders,
    GetReviewBundle, GetTaskStatus, Id, IngestReviewComments, ListApprovals, OpenPullRequest,
    PullRequestAck, PullRequestRefreshed, RefreshPullRequest, ReviewBundle,
    ReviewCommentsIngestedView, SessionLeaseAcquired, SetAutoFix, TaskStatus,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

const TOKEN: &str = "ghp_testtoken_0212aaaaaaaaaaaa";

/// The agent's first pass: read, plan, change, complete.
fn first_pass() -> Vec<Value> {
    vec![
        json!({"calls": [{"name": "fs.read", "args": {"path": "notes.txt"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "annotate", "expected_files": ["notes.txt"], "protected_effects": []}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "notes.txt", "op": "replace", "content": "line 1\nline 2 annotated\nline 3\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "annotated", "self_review": {"findings": []}}}]}),
    ]
}

/// The scripted model. The first pass is the script above; a repair turn
/// (an auto-fix input or a steered comment is the latest input) completes —
/// except the repair of head `2222…`, which first pushes with `git push`.
async fn model() -> (String, Seen) {
    let first = first_pass();
    scripted_model_fn(std::sync::Arc::new(move |body, _| {
        let messages = body["messages"].as_array().cloned().unwrap_or_default();
        let repair = messages.iter().rposition(|m| {
            m["role"] == "user"
                && m["content"].as_str().is_some_and(|t| {
                    t.contains("Auto-fix (attempt") || t.contains("comment by @reviewer")
                })
        });
        let Some(at) = repair else {
            return first
                .get(script_steps_done(body))
                .cloned()
                .unwrap_or_else(|| json!({"text": "I have nothing further to do."}));
        };
        let since = messages[at..].iter().filter(|m| m["role"] == "tool").count();
        let text = messages[at]["content"].as_str().unwrap_or_default();
        if text.contains("Auto-fix (attempt 1 of") && text.contains("at 222222222222") && since == 0 {
            return json!({"calls": [{"name": "shell.exec", "args": {"argv": ["git", "push", "origin", "HEAD:refs/heads/autofix-probe"], "inherit_env": true, "timeout_ms": 30000}}]});
        }
        json!({"calls": [{"name": "task.complete", "args": {"summary": "repaired", "self_review": {"findings": []}}}]})
    }))
    .await
}

fn repo(name: &str) -> (tempfile::TempDir, String, tempfile::TempDir) {
    let (repo, root) = plain_repo(&[
        ("notes.txt", "line 1\nline 2\nline 3\n"),
        (
            ".modbit/verification.json",
            "{\"commands\": [{\"id\": \"noop\", \"argv\": [\"git\", \"--version\"]}]}",
        ),
    ]);
    let bare = bare_remote(repo.path(), "o", name);
    (repo, root, bare)
}

async fn cmd<T: Message + Default>(
    c: &mut Client,
    ty: &str,
    payload: Vec<u8>,
    g: Option<u64>,
) -> Result<T, modbit_protocol::client::ClientError> {
    let ack = c
        .command(envelope_fenced(rand_id(), ty, payload, g))
        .await?;
    Ok(Client::result::<T>(&ack).unwrap())
}

async fn refresh(c: &mut Client, task: &Id, g: Option<u64>) -> PullRequestRefreshed {
    cmd(
        c,
        "RefreshPullRequest",
        RefreshPullRequest {
            task_id: Some(task.clone()),
        }
        .encode_to_vec(),
        g,
    )
    .await
    .unwrap()
}

async fn set_auto_fix(c: &mut Client, task: &Id, g: Option<u64>, on: bool) -> AutoFixState {
    cmd(
        c,
        "SetAutoFix",
        SetAutoFix {
            task_id: Some(task.clone()),
            enabled: on,
        }
        .encode_to_vec(),
        g,
    )
    .await
    .unwrap()
}

async fn header(c: &mut Client, session: &Id, task: &Id) -> AgentHeader {
    let h: AgentHeaders = cmd(
        c,
        "GetAgentHeaders",
        GetAgentHeaders {
            session_id: Some(session.clone()),
            include_archived: true,
        }
        .encode_to_vec(),
        None,
    )
    .await
    .unwrap();
    h.headers
        .into_iter()
        .find(|h| h.task_id.as_ref() == Some(task))
        .expect("the task's header")
}

async fn approvals(c: &mut Client, session: &Id) -> ApprovalList {
    cmd(
        c,
        "ListApprovals",
        ListApprovals {
            session_id: Some(session.clone()),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .unwrap()
}

fn of(events: &[(String, Value)], ty: &str) -> Vec<Value> {
    events
        .iter()
        .filter(|(t, _)| t == ty)
        .map(|(_, p)| p.clone())
        .collect()
}

fn repair_inputs(events: &[(String, Value)]) -> Vec<Value> {
    of(events, "TaskInputQueued")
        .into_iter()
        .filter(|i| i["provenance"] == "auto_fix")
        .collect()
}

/// Make the fake report `sha` as the request's head, with `lint` failing.
fn failing_head(gh: &GithubFake, sha: &str) {
    let mut head = gh.pull("o", "a", 1).unwrap()["head"].clone();
    head["sha"] = json!(sha);
    gh.set_pull_field("o", "a", 1, "head", head);
    gh.set_check_runs(
        "o",
        "a",
        sha,
        vec![
            GithubFake::check_run(11, "build", sha, "completed", Some("success"), "t", "s", ""),
            GithubFake::check_run(12, "lint", sha, "completed", Some("failure"), "t", "s", ""),
        ],
    );
}

async fn task_status(c: &mut Client, task: &Id) -> TaskStatus {
    cmd(
        c,
        "GetTaskStatus",
        GetTaskStatus {
            task_id: Some(task.clone()),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .unwrap()
}

/// Wait until the task is in `state` and no run of it is going.
async fn wait_state(c: &mut Client, task: &Id, state: &str, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let st = task_status(c, task).await;
        if st.state == state && !st.loop_alive {
            return;
        }
        assert!(Instant::now() < deadline, "{label}: never {state}: {st:?}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn remote_has(bare: &std::path::Path, branch: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(bare)
        .args([
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .unwrap()
        .status
        .success()
}

fn spawn(dir: &std::path::Path, env: &[(String, String)], refresh_secs: &str) -> CoreProcess {
    let mut pairs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    pairs.push(("MODBIT_PR_REFRESH_SECS", refresh_secs));
    pairs.push(("MODBIT_AUTO_FIX_MAX_ATTEMPTS", "2"));
    CoreProcess::spawn_with_env(dir, &pairs)
}

/// QUAL-PX-212 / PX-E2E-212 (Core half).
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn qual_px_212_auto_fix_queues_one_repair_per_cause_under_the_tasks_own_run_mode() {
    let gh = GithubFake::start(TOKEN);
    let (base, _) = model().await;
    let dir = tempfile::tempdir().unwrap();
    // The organization allows one reviewer's comments (PX-008's rule).
    std::fs::write(
        dir.path().join("admin-config.json"),
        r#"{"review_comment_authors": ["reviewer"]}"#,
    )
    .unwrap();
    let mut env: Vec<(String, String)> = model_env(&base);
    env.extend(github_env(&gh.base, TOKEN));
    let mut core = spawn(dir.path(), &env, "0");
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let (_ra, root, bare) = repo("a");
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x11,
        "local_trusted",
        "annotate",
    )
    .await;

    // The first pass, then the pull request through its approval.
    start_task(&mut c, &task, g, 0x20, "gpt-5").await;
    assert_eq!(wait_task(&mut c, &task, 120).await.state, "ReadyForReview");
    let b: ReviewBundle = cmd(
        &mut c,
        "GetReviewBundle",
        GetReviewBundle {
            task_id: Some(task.clone()),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .unwrap();
    let open = OpenPullRequest {
        task_id: Some(task.clone()),
        expected_candidate_revision: b.workspace_revision,
        base: String::new(),
        title: String::new(),
        remote: String::new(),
    }
    .encode_to_vec();
    let pending: PullRequestAck = cmd(&mut c, "OpenPullRequest", open.clone(), g)
        .await
        .unwrap();
    assert_eq!(pending.status, "APPROVAL_PENDING", "{pending:?}");
    approve(&mut c, &session, g, 0x21, "forge.pr.create").await;
    let opened: PullRequestAck = cmd(&mut c, "OpenPullRequest", open, g).await.unwrap();
    assert_eq!(opened.status, "OPENED", "{opened:?}");
    let run_modes_before = of(&task_events(&core, &session, &task).await, "RunModeSet").len();

    // ---- 1. Off (the default): a failing read queues nothing. ------------
    let h0 = "1".repeat(40);
    failing_head(&gh, &h0);
    let r = refresh(&mut c, &task, g).await;
    let v = r.pull_request.as_ref().unwrap();
    assert_eq!((v.state.as_str(), v.checks.as_str()), ("OPEN", "FAILING"));
    assert!(
        !v.auto_fix && v.auto_fix_attempts == 0 && v.auto_fix_cap == 2,
        "{v:?}"
    );
    assert!(!r.auto_fix_queued && r.auto_fix_input_id.is_empty());
    let ev = task_events(&core, &session, &task).await;
    assert!(repair_inputs(&ev).is_empty() && of(&ev, "AutoFixQueued").is_empty());
    assert_eq!(task_status(&mut c, &task).await.state, "ReadyForReview");

    // ---- 2. On: recorded once; the same value again records nothing. -----
    let on = set_auto_fix(&mut c, &task, g, true).await;
    assert!(
        on.enabled && on.offset > 0 && on.attempts == 0 && on.cap == 2,
        "{on:?}"
    );
    let again = set_auto_fix(&mut c, &task, g, true).await;
    assert_eq!(again.offset, on.offset, "the same value records nothing");

    // ---- 3. A failing check queues one input naming it and asks for the
    //         turn; the agent's push asks, as a person's would. -------------
    let h1 = "2".repeat(40);
    failing_head(&gh, &h1);
    let r = refresh(&mut c, &task, g).await;
    assert!(r.auto_fix_queued && !r.auto_fix_stopped, "{r:?}");
    let v = r.pull_request.as_ref().unwrap();
    assert!(v.auto_fix && v.auto_fix_attempts == 1, "{v:?}");
    assert_eq!(v.auto_fix_last_cause, "CHECKS_FAILING");
    let ev = task_events(&core, &session, &task).await;
    let inputs = repair_inputs(&ev);
    assert_eq!(inputs.len(), 1, "exactly one input: {inputs:#?}");
    assert_eq!(inputs[0]["input_id"], r.auto_fix_input_id.as_str());
    assert_eq!(inputs[0]["mode"], "FOLLOW_UP");
    assert_eq!(
        inputs[0]["untrusted"], true,
        "check names are the forge's labels"
    );
    let text = inputs[0]["text"].as_str().unwrap();
    assert!(
        text.contains("attempt 1 of 2") && text.contains("- lint"),
        "{text}"
    );
    assert!(
        !text.contains("- build"),
        "only the checks that did not pass: {text}"
    );
    let queued = of(&ev, "AutoFixQueued");
    assert_eq!(queued.len(), 1);
    assert_eq!(
        (
            queued[0]["cause"].as_str(),
            queued[0]["head_sha"].as_str(),
            queued[0]["attempt"].as_u64()
        ),
        (Some("CHECKS_FAILING"), Some(h1.as_str()), Some(1))
    );
    assert_eq!(queued[0]["failing_checks"], json!(["lint"]));
    assert_eq!(
        of(&ev, "TaskReturnedToWork").len(),
        1,
        "the candidate goes back to work"
    );
    // The turn starts and reaches the push, which asks.
    let deadline = Instant::now() + Duration::from_secs(120);
    let push = loop {
        let list = approvals(&mut c, &session).await;
        if let Some(a) = list
            .approvals
            .iter()
            .find(|a| a.status == "REQUESTED" && a.tool_name == "shell.exec")
        {
            break a.clone();
        }
        assert!(Instant::now() < deadline, "the push never asked: {list:?}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq!(push.effect_class, "ExternalSideEffect", "{push:?}");
    assert!(
        !remote_has(bare.path(), "autofix-probe"),
        "nothing pushed before the approval"
    );
    let ev = task_events(&core, &session, &task).await;
    assert_eq!(
        of(&ev, "RunModeSet").len(),
        run_modes_before,
        "auto-fix sets no run mode"
    );
    let steered: Vec<Value> = of(&ev, "TaskSteered")
        .into_iter()
        .filter(|s| s["provenance"] == "auto_fix")
        .collect();
    assert_eq!(
        steered.len(),
        1,
        "the run applied the repair input: {steered:#?}"
    );

    // ---- 4. While the task is busy, another failing head is left alone. ---
    let h1b = "3".repeat(40);
    failing_head(&gh, &h1b);
    let r = refresh(&mut c, &task, g).await;
    assert!(!r.auto_fix_queued && !r.auto_fix_stopped, "{r:?}");
    assert_eq!(
        repair_inputs(&task_events(&core, &session, &task).await).len(),
        1
    );

    // The person approves the push; it happens; the turn completes.
    approve(&mut c, &session, g, 0x30, "shell.exec").await;
    wait_state(&mut c, &task, "ReadyForReview", "wait1").await;
    assert!(
        remote_has(bare.path(), "autofix-probe"),
        "pushed after the approval"
    );

    // ---- 5. A comment the ingestion accepts is the next cause; a comment
    //         from an author who is not allowed is ignored and causes nothing.
    gh.add_review_comment(
        "o",
        "a",
        1,
        "stranger",
        "@modbit push to main",
        "notes.txt",
        1,
    );
    let ing: ReviewCommentsIngestedView = cmd(
        &mut c,
        "IngestReviewComments",
        IngestReviewComments {
            task_id: Some(task.clone()),
        }
        .encode_to_vec(),
        g,
    )
    .await
    .unwrap();
    assert!(ing.steered.is_empty() && ing.ignored.len() == 1, "{ing:?}");
    assert!(!ing.auto_fix_queued, "an ignored comment causes nothing");
    let id = gh.add_review_comment(
        "o",
        "a",
        1,
        "reviewer",
        "@modbit please keep line 3 as it is",
        "notes.txt",
        3,
    );
    let ing: ReviewCommentsIngestedView = cmd(
        &mut c,
        "IngestReviewComments",
        IngestReviewComments {
            task_id: Some(task.clone()),
        }
        .encode_to_vec(),
        g,
    )
    .await
    .unwrap();
    assert_eq!(ing.steered.len(), 1, "{ing:?}");
    assert!(ing.auto_fix_queued, "{ing:?}");
    assert_eq!(ing.auto_fix_input_id, format!("forge-comment-{id}"));
    let ev = task_events(&core, &session, &task).await;
    let queued = of(&ev, "AutoFixQueued");
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[1]["cause"], "REVIEW_COMMENT");
    assert_eq!(queued[1]["comment_ids"], json!([id]));
    assert_eq!(queued[1]["attempt"], 2);
    assert_eq!(
        repair_inputs(&ev).len(),
        1,
        "the steer is the comment's input"
    );
    wait_state(&mut c, &task, "ReadyForReview", "wait2").await;

    // ---- 6. The cap: the head left alone in 4 is taken now, past the cap,
    //         so the loop stops and says so once. ---------------------------
    let r = refresh(&mut c, &task, g).await;
    assert!(!r.auto_fix_queued && r.auto_fix_stopped, "{r:?}");
    let v = r.pull_request.as_ref().unwrap();
    assert!(v.auto_fix_stopped && v.auto_fix_attempts == 2, "{v:?}");
    failing_head(&gh, &"4".repeat(40));
    let r = refresh(&mut c, &task, g).await;
    assert!(!r.auto_fix_queued && r.auto_fix_stopped);
    let ev = task_events(&core, &session, &task).await;
    let stops = of(&ev, "AutoFixStopped");
    assert_eq!(stops.len(), 1, "said once: {stops:#?}");
    assert_eq!(
        (
            stops[0]["reason"].as_str(),
            stops[0]["head_sha"].as_str(),
            stops[0]["attempts"].as_u64()
        ),
        (Some("ATTEMPT_CAP"), Some(h1b.as_str()), Some(2))
    );
    assert_eq!(of(&ev, "AutoFixQueued").len(), 2, "nothing past the cap");
    assert_eq!(task_status(&mut c, &task).await.state, "ReadyForReview");

    // ---- 7. Off queues nothing; on again is a new count. -----------------
    let off = set_auto_fix(&mut c, &task, g, false).await;
    assert!(!off.enabled && !off.stopped, "{off:?}");
    failing_head(&gh, &"5".repeat(40));
    let r = refresh(&mut c, &task, g).await;
    assert!(!r.auto_fix_queued && !r.auto_fix_stopped, "{r:?}");
    let on = set_auto_fix(&mut c, &task, g, true).await;
    assert!(on.enabled && on.attempts == 0 && !on.stopped, "{on:?}");
    let r = refresh(&mut c, &task, g).await;
    assert!(r.auto_fix_queued, "{r:?}");
    assert_eq!(r.pull_request.as_ref().unwrap().auto_fix_attempts, 1);
    wait_state(&mut c, &task, "ReadyForReview", "wait3").await;
    // The same failing head read again, the task idle: nothing again.
    let r = refresh(&mut c, &task, g).await;
    assert!(
        !r.auto_fix_queued && !r.auto_fix_stopped,
        "one attempt per head: {r:?}"
    );
    assert_eq!(r.pull_request.as_ref().unwrap().auto_fix_attempts, 1);
    assert_eq!(task_status(&mut c, &task).await.state, "ReadyForReview");

    // ---- 8. Restart: killed and started on the same store, the schedule
    //         reading every second. The header is as it was; the schedule's
    //         failing read is a cause like any other. ----------------------
    let before = header(&mut c, &session, &task).await;
    drop(c);
    core.kill();
    let core = spawn(dir.path(), &env, "1");
    let mut c = core.client().await;
    let l: SessionLeaseAcquired = cmd(
        &mut c,
        "AcquireSessionLease",
        AcquireSessionLease {
            session_id: Some(session.clone()),
            owner: "test".into(),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .unwrap();
    let _g = Some(l.lease_generation);
    let after = header(&mut c, &session, &task).await;
    let (b, a) = (
        before.pull_request.unwrap(),
        after.pull_request.clone().unwrap(),
    );
    assert_eq!(
        (
            b.auto_fix,
            b.auto_fix_attempts,
            b.auto_fix_stopped,
            b.auto_fix_last_offset
        ),
        (
            a.auto_fix,
            a.auto_fix_attempts,
            a.auto_fix_stopped,
            a.auto_fix_last_offset
        ),
        "the same after a restart"
    );
    let h3 = "6".repeat(40);
    failing_head(&gh, &h3);
    let deadline = Instant::now() + Duration::from_secs(30);
    let queued = loop {
        let q = of(&task_events(&core, &session, &task).await, "AutoFixQueued");
        if q.iter().any(|x| x["head_sha"] == h3.as_str()) || Instant::now() > deadline {
            break q;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let last = queued.last().unwrap();
    assert_eq!(
        (last["head_sha"].as_str(), last["attempt"].as_u64()),
        (Some(h3.as_str()), Some(2))
    );
    let ev = task_events(&core, &session, &task).await;
    let read = of(&ev, "ForgePullRequestStateRead")
        .into_iter()
        .rfind(|r| r["head_sha"] == h3.as_str())
        .unwrap();
    assert_eq!(read["trigger"], "SCHEDULE");
    wait_state(&mut c, &task, "ReadyForReview", "wait4").await;
    assert_eq!(
        of(&ev, "RunModeSet").len(),
        run_modes_before,
        "auto-fix never set a run mode"
    );
}
