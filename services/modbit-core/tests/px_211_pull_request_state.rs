//! PX-211 (QUAL-PX-211, DR-PX-2026-11-11-021 MWS-17): a task's pull request
//! after it was opened — the Core records the request, reads its state and
//! checks again through the forge adapter (on request and on a schedule),
//! and the task's header carries it; archive after merge is a preference
//! with an undo.
//!
//! The real `modbit-core` binary over its real socket, the real Capability
//! Kernel, approvals and event store, a real git repository pushing to a
//! real bare remote, a scripted model, and the GitHub REST fake that speaks
//! GitHub's documented shapes over a real TCP socket
//! (`tests/support/github_fake.rs`). What this is NOT: a proof against real
//! GitHub (the forge adapter's read path is, by PX-125's live leg).

#[path = "px_common/forge_support.rs"]
mod forge_support;
#[path = "../../../tests/support/github_fake.rs"]
mod github_fake;
mod px_common;

use forge_support::*;
use github_fake::{Fault, GithubFake};
use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    AcquireSessionLease, AgentHeader, AgentHeaders, ArchiveTask, EmergencyStop, EmergencyStopped,
    GetAgentHeaders, GetPullRequestPreferences, GetReviewBundle, GetTranscript, Id,
    OpenPullRequest, PullRequestAck, PullRequestPreferences, PullRequestRefreshed,
    RefreshPullRequest, ReviewBundle, SessionLeaseAcquired, SetPullRequestPreferences,
    TaskArchived, TranscriptDensity, TranscriptPage,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};

const TOKEN: &str = "ghp_testtoken_0211aaaaaaaaaaaa";

/// The scripted agent: read, plan, change, complete.
fn script() -> Vec<Value> {
    vec![
        json!({"calls": [{"name": "fs.read", "args": {"path": "notes.txt"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "annotate", "expected_files": ["notes.txt"], "protected_effects": []}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "notes.txt", "op": "replace", "content": "line 1\nline 2 annotated\nline 3\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "annotated", "self_review": {"findings": []}}}]}),
    ]
}

/// A repository whose `origin` is `https://github.test/o/<name>.git`,
/// resolved by git to a local bare remote.
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

async fn refresh(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
) -> Result<PullRequestRefreshed, modbit_protocol::client::ClientError> {
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

async fn set_pref(
    c: &mut Client,
    session: &Id,
    g: Option<u64>,
    on: bool,
) -> PullRequestPreferences {
    cmd(
        c,
        "SetPullRequestPreferences",
        SetPullRequestPreferences {
            session_id: Some(session.clone()),
            archive_after_merge: on,
        }
        .encode_to_vec(),
        g,
    )
    .await
    .unwrap()
}

async fn archive(c: &mut Client, task: &Id, g: Option<u64>, on: bool) -> TaskArchived {
    cmd(
        c,
        "ArchiveTask",
        ArchiveTask {
            task_id: Some(task.clone()),
            archived: on,
        }
        .encode_to_vec(),
        g,
    )
    .await
    .unwrap()
}

/// Run the task to review and open its pull request through its approval.
async fn run_and_open(
    c: &mut Client,
    session: &Id,
    g: Option<u64>,
    task: &Id,
    n: u8,
) -> PullRequestAck {
    start_task(c, task, g, n, "gpt-5").await;
    let st = wait_task(c, task, 120).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}");
    let b: ReviewBundle = cmd(
        c,
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
    let pending: PullRequestAck = cmd(c, "OpenPullRequest", open.clone(), g).await.unwrap();
    assert_eq!(pending.status, "APPROVAL_PENDING", "{pending:?}");
    approve(c, session, g, n.wrapping_add(1), "forge.pr.create").await;
    let opened: PullRequestAck = cmd(c, "OpenPullRequest", open, g).await.unwrap();
    assert_eq!(opened.status, "OPENED", "{opened:?}");
    opened
}

/// A Core on `dir` whose schedule reads every `refresh_secs` (0 = never).
fn spawn(dir: &std::path::Path, env: &[(String, String)], refresh_secs: &str) -> CoreProcess {
    let mut pairs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    pairs.push(("MODBIT_PR_REFRESH_SECS", refresh_secs));
    CoreProcess::spawn_with_env(dir, &pairs)
}

fn pr(h: &AgentHeader) -> &modbit_protocol::v1::TaskPullRequest {
    h.pull_request
        .as_ref()
        .expect("the header carries the pull request")
}

fn reads(events: &[(String, Value)]) -> Vec<Value> {
    events
        .iter()
        .filter(|(t, _)| t == "ForgePullRequestStateRead")
        .map(|(_, p)| p.clone())
        .collect()
}

fn run(id: u64, name: &str, sha: &str, status: &str, conclusion: Option<&str>) -> Value {
    GithubFake::check_run(id, name, sha, status, conclusion, "t", "s", "")
}

/// QUAL-PX-211 / PX-E2E-211 (Core half): a request opened through its
/// approval is recorded and the header names it with no state; refresh reads
/// open, checks failing, checks passing and merged in turn, each equal to the
/// server's; a failed read records nothing and the last read stands with the
/// time it was made; the header is the same after the Core is killed and
/// started again; a merge read with the preference off archives nothing; with
/// it on, the schedule's read of the merge archives the task and the undo
/// brings it back for good; with the session's leases revoked a refresh is
/// refused by the kernel and the forge is not asked.
#[tokio::test]
async fn qual_px_211_the_core_records_reads_and_carries_a_tasks_pull_request_state() {
    let gh = GithubFake::start(TOKEN);
    let (base, _) = scripted_model(script(), vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let mut env: Vec<(String, String)> = model_env(&base);
    env.extend(github_env(&gh.base, TOKEN));
    // The schedule is off for the first Core: every read below is asked for.
    let mut core = spawn(dir.path(), &env, "0");
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let (_ra, root_a, _ba) = repo("a");
    let (_rb, root_b, _bb) = repo("b");
    let task_a = create_task(
        &mut c,
        &session,
        g,
        &root_a,
        0x11,
        "local_trusted",
        "annotate a",
    )
    .await;
    let task_b = create_task(
        &mut c,
        &session,
        g,
        &root_b,
        0x12,
        "local_trusted",
        "annotate b",
    )
    .await;

    // 0. Before a request exists there is nothing to read, and the header
    //    names none.
    let err = refresh(&mut c, &task_a, g).await.unwrap_err();
    assert!(rejected_with(&err, "NO_PULL_REQUEST"), "{err:?}");
    assert!(
        header(&mut c, &session, &task_a)
            .await
            .pull_request
            .is_none()
    );

    // 1. Opened through its approval: recorded, and the header names it with
    //    no state — nothing has been read.
    let opened = run_and_open(&mut c, &session, g, &task_a, 0x20).await;
    assert_eq!(opened.number, 1);
    let h = header(&mut c, &session, &task_a).await;
    let p = pr(&h);
    assert_eq!((p.owner.as_str(), p.repo.as_str(), p.number), ("o", "a", 1));
    assert_eq!(p.url, "https://github.test/o/a/pull/1");
    assert!(p.opened_offset > 0);
    assert_eq!(p.state, "", "a state that was not read is never shown");
    assert_eq!(p.checks, "");
    assert!(p.read_at.is_none() && p.read_offset == 0);
    let head = gh.pull("o", "a", 1).unwrap()["head"]["sha"]
        .as_str()
        .unwrap()
        .to_owned();

    // 2. Open, checks running.
    gh.set_check_runs(
        "o",
        "a",
        &head,
        vec![run(11, "build", &head, "in_progress", None)],
    );
    let r1 = refresh(&mut c, &task_a, g).await.unwrap();
    let v = r1.pull_request.as_ref().unwrap();
    assert_eq!((v.state.as_str(), v.checks.as_str()), ("OPEN", "PENDING"));
    assert_eq!((v.checks_total, v.checks_pending), (1, 1));
    assert_eq!(v.head_sha, head);
    assert_eq!(v.pushed_sha, opened.head_sha, "the commit this Core pushed");
    assert_eq!(v.trigger, "REQUEST");
    assert!(v.read_at.is_some() && v.read_offset == r1.offset && !r1.archived);
    assert_eq!(
        &header(&mut c, &session, &task_a).await.pull_request,
        &r1.pull_request
    );
    // The read went to the forge, as the Core, for this request.
    assert!(
        gh.requests_matching("GET", "/repos/o/a/pulls/1")
            .iter()
            .all(|s| s.authorized),
        "read under the Core's token"
    );
    assert!(!gh.requests_matching("GET", "/repos/o/a/pulls/1").is_empty());

    // 3. Checks failing.
    gh.set_check_runs(
        "o",
        "a",
        &head,
        vec![
            run(11, "build", &head, "completed", Some("success")),
            run(12, "lint", &head, "completed", Some("failure")),
        ],
    );
    let r2 = refresh(&mut c, &task_a, g).await.unwrap();
    let v = r2.pull_request.as_ref().unwrap();
    assert_eq!((v.state.as_str(), v.checks.as_str()), ("OPEN", "FAILING"));
    assert_eq!(
        (
            v.checks_total,
            v.checks_passing,
            v.checks_failing,
            v.checks_pending
        ),
        (2, 1, 1, 0)
    );
    assert_eq!(v.failing_checks, vec!["lint".to_owned()]);

    // 4. Checks passing.
    gh.set_check_runs(
        "o",
        "a",
        &head,
        vec![
            run(11, "build", &head, "completed", Some("success")),
            run(13, "lint", &head, "completed", Some("success")),
        ],
    );
    let r3 = refresh(&mut c, &task_a, g).await.unwrap();
    let v = r3.pull_request.as_ref().unwrap();
    assert_eq!((v.state.as_str(), v.checks.as_str()), ("OPEN", "PASSING"));
    assert!(v.failing_checks.is_empty());

    // 5. A read that fails records nothing; the last read stands and says
    //    when it was made.
    gh.inject(Some("GET"), "/repos/o/a/pulls/1", Fault::ServerError, 1);
    let err = refresh(&mut c, &task_a, g).await.unwrap_err();
    assert!(
        matches!(err, modbit_protocol::client::ClientError::Rejected { .. }),
        "{err:?}"
    );
    let h = header(&mut c, &session, &task_a).await;
    assert_eq!(
        &h.pull_request, &r3.pull_request,
        "the stale read stands as read"
    );
    assert_eq!(pr(&h).read_at, r3.pull_request.as_ref().unwrap().read_at);
    gh.clear_faults();

    // 6. Merged, with the preference off (the default): recorded, not archived.
    let pref: PullRequestPreferences = cmd(
        &mut c,
        "GetPullRequestPreferences",
        GetPullRequestPreferences {
            session_id: Some(session.clone()),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .unwrap();
    assert!(
        !pref.archive_after_merge && pref.offset == 0,
        "off by default"
    );
    gh.set_pull_field("o", "a", 1, "state", json!("closed"));
    gh.set_pull_field("o", "a", 1, "merged", json!(true));
    let r4 = refresh(&mut c, &task_a, g).await.unwrap();
    assert_eq!(r4.pull_request.as_ref().unwrap().state, "MERGED");
    assert!(
        !r4.archived && r4.archive_offset == 0,
        "no archive without the preference"
    );
    let h = header(&mut c, &session, &task_a).await;
    assert!(!h.archived);
    // Every read on the log equals the server's state at that moment.
    let ev = task_events(&core, &session, &task_a).await;
    let states: Vec<(String, String)> = reads(&ev)
        .iter()
        .map(|r| {
            (
                r["state"].as_str().unwrap().to_owned(),
                r["checks"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        states,
        [
            ("OPEN", "PENDING"),
            ("OPEN", "FAILING"),
            ("OPEN", "PASSING"),
            ("MERGED", "PASSING")
        ]
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
    );
    assert!(
        reads(&ev)
            .iter()
            .all(|r| r.get("body").is_none() && r.get("title").is_none()),
        "the request's author text is not on the log"
    );

    // 7. Task b: opened; the preference turned on.
    run_and_open(&mut c, &session, g, &task_b, 0x30).await;
    let on = set_pref(&mut c, &session, g, true).await;
    assert!(on.archive_after_merge && on.offset > 0);
    let again = set_pref(&mut c, &session, g, true).await;
    assert_eq!(
        again.offset, on.offset,
        "the same value again records nothing"
    );
    let r = refresh(&mut c, &task_b, g).await.unwrap();
    assert_eq!(r.pull_request.as_ref().unwrap().state, "OPEN");
    assert!(!r.archived, "an open request archives nothing");

    // 8. Restart: the Core is killed and started on the same store, with the
    //    schedule reading every second. The headers are as they were.
    let before_a = header(&mut c, &session, &task_a).await;
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
    let g = Some(l.lease_generation);
    let after_a = header(&mut c, &session, &task_a).await;
    assert_eq!(
        after_a.pull_request, before_a.pull_request,
        "the same after a restart"
    );
    assert_eq!(pr(&after_a).state, "MERGED");

    // 9. The schedule: b merges on the server; the Core reads it on its own,
    //    and with the preference on archives the task.
    let head_b = gh.pull("o", "b", 1).unwrap()["head"]["sha"]
        .as_str()
        .unwrap()
        .to_owned();
    gh.set_check_runs(
        "o",
        "b",
        &head_b,
        vec![run(21, "build", &head_b, "completed", Some("success"))],
    );
    gh.set_pull_field("o", "b", 1, "state", json!("closed"));
    gh.set_pull_field("o", "b", 1, "merged", json!(true));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let h = loop {
        let h = header(&mut c, &session, &task_b).await;
        if h.archived || std::time::Instant::now() > deadline {
            break h;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    };
    assert!(h.archived, "the merged task was archived: {h:?}");
    let p = pr(&h);
    assert_eq!(
        (p.state.as_str(), p.checks.as_str(), p.trigger.as_str()),
        ("MERGED", "PASSING", "SCHEDULE")
    );
    // The schedule did not read a, whose request is settled.
    let a_reads = reads(&task_events(&core, &session, &task_a).await).len();
    assert_eq!(a_reads, 4, "a settled request is not read on a schedule");
    let ev_b = task_events(&core, &session, &task_b).await;
    let archives: Vec<&(String, Value)> = ev_b
        .iter()
        .filter(|(t, _)| t == "ConversationArchived")
        .collect();
    assert_eq!(archives.len(), 1, "archived once");

    // 10. Undo brings it back, and the schedule does not archive it again.
    let undone = archive(&mut c, &task_b, g, false).await;
    assert!(!undone.archived && undone.offset > 0);
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let h = header(&mut c, &session, &task_b).await;
    assert!(!h.archived, "the undo stands");
    let ev_b = task_events(&core, &session, &task_b).await;
    assert_eq!(
        ev_b.iter()
            .filter(|(t, _)| t == "ConversationArchived")
            .count(),
        1
    );
    // A person's refresh of the same merge does not archive it either.
    let r = refresh(&mut c, &task_b, g).await.unwrap();
    assert!(!r.archived);
    assert_eq!(r.pull_request.as_ref().unwrap().trigger, "REQUEST");
    // The conversation shows the reads a person asked for, and none of the
    // schedule's: those stay on the log only.
    let page: TranscriptPage = cmd(
        &mut c,
        "GetTranscript",
        GetTranscript {
            task_id: Some(task_b.clone()),
            density: TranscriptDensity::Detailed as i32,
            after_row: 0,
            limit: 500,
            as_of_offset: 0,
        }
        .encode_to_vec(),
        None,
    )
    .await
    .unwrap();
    let cards = page
        .rows
        .iter()
        .filter(|r| r.text.starts_with("forge.pr.read"))
        .count();
    let b_reads = reads(&task_events(&core, &session, &task_b).await);
    let asked = b_reads.iter().filter(|r| r["trigger"] == "REQUEST").count();
    assert!(b_reads.iter().any(|r| r["trigger"] == "SCHEDULE"));
    assert_eq!(
        cards, asked,
        "one card per read a person asked for, none for the schedule's"
    );

    // 11. No read goes around the adapter's policy: with the session's leases
    //     revoked the kernel refuses the read, the forge is not asked, and
    //     nothing is recorded.
    let stopped: EmergencyStopped = cmd(
        &mut c,
        "EmergencyStop",
        EmergencyStop {
            session_id: Some(session.clone()),
            reason: "qual".into(),
        }
        .encode_to_vec(),
        g,
    )
    .await
    .unwrap();
    assert!(stopped.leases_revoked > 0);
    let asked = gh.requests_matching("GET", "/repos/o/a/pulls/1").len();
    let before = reads(&task_events(&core, &session, &task_a).await).len();
    let err = refresh(&mut c, &task_a, g).await.unwrap_err();
    assert!(
        matches!(err, modbit_protocol::client::ClientError::Rejected { .. }),
        "{err:?}"
    );
    assert_eq!(
        gh.requests_matching("GET", "/repos/o/a/pulls/1").len(),
        asked
    );
    assert_eq!(
        reads(&task_events(&core, &session, &task_a).await).len(),
        before
    );
}
