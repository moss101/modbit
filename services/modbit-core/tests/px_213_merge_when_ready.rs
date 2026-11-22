//! PX-213 (QUAL-PX-213, DR-PX-2026-11-11-021 MWS-17): merge when ready — a
//! person asks for it on a task's pull request; while a check is pending it
//! waits; once the checks pass the Core raises the approval of
//! `forge.pr.merge` with the exact intent (repository, number, head, method);
//! approval merges that head only and its receipt is in the effect ledger; a
//! head that moves before approval voids the ask. Negative: nothing merges
//! without approval, a head other than the approved one is never merged, a
//! merge while a check fails or is pending is refused with its reason, and
//! neither an allow rule nor the AUTONOMOUS run mode covers the merge.
//!
//! The real `modbit-core` binary over its real socket, the real Capability
//! Kernel, approvals, effect ledger and event store, a real git repository
//! pushing to a real bare remote, a scripted model, and the GitHub REST fake
//! over a real TCP socket (`tests/support/github_fake.rs`). What this is
//! NOT: a proof against real GitHub.

#[path = "px_common/forge_support.rs"]
mod forge_support;
#[path = "../../../tests/support/github_fake.rs"]
mod github_fake;
mod px_common;

use forge_support::*;
use github_fake::GithubFake;
use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    AcquireSessionLease, AddAllowRule, AgentHeader, AgentHeaders, ApprovalList, ApprovalView,
    GetAgentHeaders, GetReviewBundle, Id, ListApprovals, MergeWhenReadyState, OpenPullRequest,
    PullRequestAck, PullRequestRefreshed, RefreshPullRequest, ResolveApproval, ReviewBundle,
    SessionLeaseAcquired, SetMergeWhenReady, SetRunMode,
};
use prost::Message;
use px_common::mcp_support::invoke_tool;
use px_common::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const TOKEN: &str = "ghp_testtoken_0213aaaaaaaaaaaa";

async fn model() -> (String, Seen) {
    let first = [
        json!({"calls": [{"name": "fs.read", "args": {"path": "notes.txt"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "annotate", "expected_files": ["notes.txt"], "protected_effects": []}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "notes.txt", "op": "replace", "content": "line 1\nline 2 annotated\nline 3\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "annotated", "self_review": {"findings": []}}}]}),
    ];
    scripted_model_fn(std::sync::Arc::new(move |body, _| {
        first
            .get(script_steps_done(body))
            .cloned()
            .unwrap_or_else(|| json!({"text": "I have nothing further to do."}))
    }))
    .await
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

async fn mwr(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
    on: bool,
    method: &str,
) -> Result<MergeWhenReadyState, modbit_protocol::client::ClientError> {
    cmd(
        c,
        "SetMergeWhenReady",
        SetMergeWhenReady {
            task_id: Some(task.clone()),
            enabled: on,
            method: method.into(),
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

async fn merge_approvals(c: &mut Client, session: &Id) -> Vec<ApprovalView> {
    let l: ApprovalList = cmd(
        c,
        "ListApprovals",
        ListApprovals {
            session_id: Some(session.clone()),
        }
        .encode_to_vec(),
        None,
    )
    .await
    .unwrap();
    l.approvals
        .into_iter()
        .filter(|a| a.tool_name == "forge.pr.merge")
        .collect()
}

fn text_id(id: &Option<Id>) -> String {
    let b: [u8; 16] = id.as_ref().unwrap().value.as_slice().try_into().unwrap();
    modbit_domain::ApprovalId::from_bytes(b).to_string()
}

async fn approval(c: &mut Client, session: &Id, id: &str) -> ApprovalView {
    merge_approvals(c, session)
        .await
        .into_iter()
        .find(|a| text_id(&a.approval_id) == id)
        .unwrap_or_else(|| panic!("approval {id}"))
}

async fn decide(
    c: &mut Client,
    g: Option<u64>,
    a: &ApprovalView,
    approve: bool,
) -> Result<modbit_protocol::v1::ApprovalResolvedAck, modbit_protocol::client::ClientError> {
    cmd(
        c,
        "ResolveApproval",
        ResolveApproval {
            approval_id: a.approval_id.clone(),
            approve,
            reason: "test".into(),
            intent_hash: a.intent_hash.clone(),
        }
        .encode_to_vec(),
        g,
    )
    .await
}

fn of(events: &[(String, Value)], ty: &str) -> Vec<Value> {
    events
        .iter()
        .filter(|(t, _)| t == ty)
        .map(|(_, p)| p.clone())
        .collect()
}

/// The fake reports `sha` as the request's head with these check runs.
fn head(gh: &GithubFake, sha: &str, runs: &[(&str, &str, Option<&str>)]) {
    let mut h = gh.pull("o", "a", 1).unwrap()["head"].clone();
    h["sha"] = json!(sha);
    gh.set_pull_field("o", "a", 1, "head", h);
    let runs = runs
        .iter()
        .enumerate()
        .map(|(i, (name, status, conclusion))| {
            GithubFake::check_run(20 + i as u64, name, sha, status, *conclusion, "t", "s", "")
        })
        .collect();
    gh.set_check_runs("o", "a", sha, runs);
}

fn passing(gh: &GithubFake, sha: &str) {
    head(
        gh,
        sha,
        &[
            ("build", "completed", Some("success")),
            ("lint", "completed", Some("success")),
        ],
    );
}

fn merges(gh: &GithubFake) -> Vec<Value> {
    gh.requests_matching("PUT", "/pulls/1/merge")
        .into_iter()
        .map(|r| r.body)
        .collect()
}

async fn wait_phase(
    c: &mut Client,
    session: &Id,
    task: &Id,
    phase: &str,
    label: &str,
) -> AgentHeader {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let h = header(c, session, task).await;
        if h.pull_request
            .as_ref()
            .is_some_and(|p| p.merge_phase == phase)
        {
            return h;
        }
        assert!(Instant::now() < deadline, "{label}: never {phase}: {h:?}");
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

fn spawn(dir: &std::path::Path, env: &[(String, String)]) -> CoreProcess {
    let mut pairs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    pairs.push(("MODBIT_PR_REFRESH_SECS", "0"));
    CoreProcess::spawn_with_env(dir, &pairs)
}

/// QUAL-PX-213 / PX-E2E-213 (Core half).
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn qual_px_213_merge_when_ready_asks_once_checks_pass_and_merges_only_the_approved_head() {
    let gh = GithubFake::start(TOKEN);
    let (base, _) = model().await;
    let dir = tempfile::tempdir().unwrap();
    let mut env: Vec<(String, String)> = model_env(&base);
    env.extend(github_env(&gh.base, TOKEN));
    let mut core = spawn(dir.path(), &env);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x10).await;
    let (repo, root) = plain_repo(&[
        ("notes.txt", "line 1\nline 2\nline 3\n"),
        (
            ".modbit/verification.json",
            "{\"commands\": [{\"id\": \"noop\", \"argv\": [\"git\", \"--version\"]}]}",
        ),
    ]);
    let _bare = bare_remote(repo.path(), "o", "a");
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

    // No pull request yet: nothing to merge.
    let e = mwr(&mut c, &task, g, true, "squash").await.unwrap_err();
    assert!(rejected_with(&e, "NO_PULL_REQUEST"), "{e:?}");

    // The first pass and the pull request through its approval.
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

    // ---- 1. Off by default; a bad method is refused. ----------------------
    let ha = "a".repeat(40);
    head(&gh, &ha, &[("build", "in_progress", None)]);
    let r = refresh(&mut c, &task, g).await;
    let v = r.pull_request.as_ref().unwrap();
    assert_eq!((v.merge_when_ready, v.merge_phase.as_str()), (false, "OFF"));
    let e = mwr(&mut c, &task, g, true, "octopus").await.unwrap_err();
    assert!(rejected_with(&e, "BAD_PAYLOAD"), "{e:?}");

    // ---- 2. Asked for while a check is pending: it waits, nothing asks. ---
    let on = mwr(&mut c, &task, g, true, "squash").await.unwrap();
    assert!(on.enabled && on.method == "squash", "{on:?}");
    assert_eq!(on.phase, "WAITING", "{on:?}");
    assert!(on.detail.contains("1 check(s)"), "{on:?}");
    assert!(on.approval_id.is_empty());
    let again = mwr(&mut c, &task, g, true, "squash").await.unwrap();
    assert_eq!(again.offset, on.offset, "the same request records nothing");
    assert!(merge_approvals(&mut c, &session).await.is_empty());

    // ---- 3. Neither AUTONOMOUS nor an allow rule covers the merge. --------
    let _: modbit_protocol::v1::RunModeView = cmd(
        &mut c,
        "SetRunMode",
        SetRunMode {
            task_id: Some(task.clone()),
            mode: "AUTONOMOUS".into(),
            acknowledge_risk: true,
        }
        .encode_to_vec(),
        g,
    )
    .await
    .unwrap();
    let rule = c
        .command(envelope_fenced(
            rand_id(),
            "AddAllowRule",
            AddAllowRule {
                task_id: Some(task.clone()),
                rule_id: "merge-everything".into(),
                pattern: vec!["forge.pr.merge".into()],
                scope: "USER".into(),
                expires_at_ms: 0,
                covers_always_ask: true,
            }
            .encode_to_vec(),
            g,
        ))
        .await;
    // Recorded or refused, it covers nothing: a rule is an argv prefix and
    // the merge has no argv.
    let _ = rule;

    // ---- 4. The checks pass: the approval is raised with the exact intent;
    //         nothing is merged before a person decides. ----------------------
    passing(&gh, &ha);
    let r = refresh(&mut c, &task, g).await;
    assert_eq!(r.merge_phase, "ASKED", "{r:?}");
    let ask_a = r.merge_approval_id.clone();
    assert!(!ask_a.is_empty());
    let v = r.pull_request.as_ref().unwrap();
    assert_eq!(
        (v.merge_head_sha.as_str(), v.merge_approval_id.as_str()),
        (ha.as_str(), ask_a.as_str())
    );
    let a = approval(&mut c, &session, &ask_a).await;
    assert_eq!(
        (a.status.as_str(), a.effect_class.as_str()),
        ("REQUESTED", "ExternalSideEffect")
    );
    let ev = task_events(&core, &session, &task).await;
    let asked = of(&ev, "MergeAsked");
    assert_eq!(asked.len(), 1);
    assert_eq!(
        (
            asked[0]["head_sha"].as_str(),
            asked[0]["method"].as_str(),
            asked[0]["number"].as_u64()
        ),
        (Some(ha.as_str()), Some("squash"), Some(1))
    );
    assert_eq!(
        asked[0]["intent_hash"].as_str(),
        Some(a.intent_hash.as_str())
    );
    let scope: Value = serde_json::from_str(&a.scope_json).unwrap_or_default();
    assert_eq!(
        scope["run_mode"], "AUTONOMOUS",
        "decided under AUTONOMOUS: {scope}"
    );
    assert!(merges(&gh).is_empty(), "nothing merged without approval");
    // The same read again: the ask stands, no second ask.
    let r = refresh(&mut c, &task, g).await;
    assert_eq!(
        (r.merge_phase.as_str(), r.merge_approval_id.as_str()),
        ("ASKED", "")
    );
    assert_eq!(merge_approvals(&mut c, &session).await.len(), 1);

    // ---- 5. The head moves before approval: the ask is void and can no
    //         longer be approved; the new head is asked about. --------------
    let hb = "b".repeat(40);
    passing(&gh, &hb);
    let r = refresh(&mut c, &task, g).await;
    let ask_b = r.merge_approval_id.clone();
    assert!(!ask_b.is_empty() && ask_b != ask_a, "{r:?}");
    let old = approval(&mut c, &session, &ask_a).await;
    assert_eq!(old.status, "EXPIRED");
    let e = decide(&mut c, g, &old, true).await.unwrap_err();
    assert!(rejected_with(&e, "APPROVAL_EXPIRED"), "{e:?}");
    assert!(
        format!("{e:?}").contains("withdrawn"),
        "says it was withdrawn: {e:?}"
    );
    let ev = task_events(&core, &session, &task).await;
    let voided = of(&ev, "MergeAskVoided");
    assert_eq!(
        (voided[0]["reason"].as_str(), voided[0]["head_sha"].as_str()),
        (Some("HEAD_MOVED"), Some(ha.as_str()))
    );

    // ---- 6. The checks stop passing: the ask is void; it waits and says why.
    head(
        &gh,
        &hb,
        &[
            ("build", "completed", Some("success")),
            ("lint", "completed", Some("failure")),
        ],
    );
    let r = refresh(&mut c, &task, g).await;
    let v = r.pull_request.as_ref().unwrap();
    assert_eq!(v.merge_phase, "WAITING", "{v:?}");
    assert!(v.merge_detail.contains("lint"), "{v:?}");
    assert_eq!(approval(&mut c, &session, &ask_b).await.status, "EXPIRED");
    let ev = task_events(&core, &session, &task).await;
    assert_eq!(
        of(&ev, "MergeAskVoided").last().unwrap()["reason"],
        "CHECKS_NOT_PASSING"
    );

    // ---- 7. The tool refuses a merge while a check fails or is pending,
    //         with its reason, even under an approval. ----------------------
    for (n, runs, code) in [
        (
            0x50u8,
            vec![("lint", "completed", Some("failure"))],
            "CHECKS_FAILING",
        ),
        (0x52u8, vec![("lint", "queued", None)], "CHECKS_PENDING"),
    ] {
        head(&gh, &hb, &runs);
        let args = json!({"owner": "o", "repo": "a", "number": 1, "sha": hb, "merge_method": "merge", "idempotency_key": format!("direct-{n}")}).to_string();
        let first = invoke_tool(&mut c, &task, g, n, n, "forge.pr.merge", &args).await;
        assert_eq!(first.status, "APPROVAL_PENDING", "{first:?}");
        approve(&mut c, &session, g, n.wrapping_add(0x10), "forge.pr.merge").await;
        let r = invoke_tool(
            &mut c,
            &task,
            g,
            n.wrapping_add(1),
            n,
            "forge.pr.merge",
            &args,
        )
        .await;
        assert_eq!(r.error_code, code, "{r:?}");
    }
    assert!(merges(&gh).is_empty(), "a refused merge reached the forge");

    // ---- 8. Denied: merge when ready turns off and says so; asking again
    //         reads at once and asks again. ---------------------------------
    passing(&gh, &hb);
    let r = refresh(&mut c, &task, g).await;
    let ask_b2 = r.merge_approval_id.clone();
    assert!(!ask_b2.is_empty(), "{r:?}");
    let a = approval(&mut c, &session, &ask_b2).await;
    decide(&mut c, g, &a, false).await.unwrap();
    let h = wait_phase(&mut c, &session, &task, "CLEARED", "denied").await;
    let v = h.pull_request.unwrap();
    assert!(
        !v.merge_when_ready && v.merge_detail.contains("denied"),
        "{v:?}"
    );
    let r = refresh(&mut c, &task, g).await;
    assert!(r.merge_approval_id.is_empty(), "off asks nothing: {r:?}");
    let on = mwr(&mut c, &task, g, true, "squash").await.unwrap();
    assert_eq!(on.phase, "ASKED", "the command's own read asked: {on:?}");
    let ask_b3 = on.approval_id.clone();
    assert!(ask_b3 != ask_b2);

    // ---- 9. Restart: the same state, and the ask still answers. ----------
    let before = header(&mut c, &session, &task).await.pull_request.unwrap();
    drop(c);
    core.kill();
    let core = spawn(dir.path(), &env);
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
    let after = header(&mut c, &session, &task).await.pull_request.unwrap();
    assert_eq!(
        (
            &before.merge_phase,
            &before.merge_approval_id,
            &before.merge_head_sha,
            before.merge_offset
        ),
        (
            &after.merge_phase,
            &after.merge_approval_id,
            &after.merge_head_sha,
            after.merge_offset
        ),
        "the same after a restart"
    );

    // ---- 10. Approved, but the head moved on the forge before the Core read
    //          it: the approved head is not merged; the new head is asked. --
    let hc = "c".repeat(40);
    passing(&gh, &hc);
    let a = approval(&mut c, &session, &ask_b3).await;
    decide(&mut c, g, &a, true).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let ask_c = loop {
        let v = header(&mut c, &session, &task).await.pull_request.unwrap();
        if v.merge_phase == "ASKED" && v.merge_head_sha == hc {
            break v.merge_approval_id;
        }
        assert!(
            Instant::now() < deadline,
            "never asked for the new head: {v:?}"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    };
    assert!(
        merges(&gh).is_empty(),
        "the approved head {hb} was not merged"
    );
    let ev = task_events(&core, &session, &task).await;
    let v = of(&ev, "MergeAskVoided");
    let last = v.last().unwrap();
    assert_eq!(
        (last["reason"].as_str(), last["head_sha"].as_str()),
        (Some("HEAD_MOVED"), Some(hb.as_str()))
    );

    // ---- 11. Approved at the head it names: merged, that head only, with
    //          the method asked for; the receipt is in the effect ledger. ---
    let a = approval(&mut c, &session, &ask_c).await;
    decide(&mut c, g, &a, true).await.unwrap();
    let h = wait_phase(&mut c, &session, &task, "MERGED", "merged").await;
    let v = h.pull_request.unwrap();
    assert_eq!(v.state, "MERGED", "the read after the merge: {v:?}");
    assert_eq!(v.merge_head_sha, hc);
    assert!(!v.merge_sha.is_empty());
    let m = merges(&gh);
    assert_eq!(m.len(), 1, "{m:?}");
    assert_eq!(
        (m[0]["sha"].as_str(), m[0]["merge_method"].as_str()),
        (Some(hc.as_str()), Some("squash"))
    );
    let ev = task_events(&core, &session, &task).await;
    let done = of(&ev, "MergeCompleted");
    assert_eq!(done.len(), 1);
    assert_eq!(done[0]["approval_id"].as_str(), Some(ask_c.as_str()));
    let receipts = done[0]["effect_receipt_ids"].as_array().unwrap();
    assert!(!receipts.is_empty(), "{done:?}");
    let ledger = of(&ev, "EffectReceiptAppended");
    assert!(
        receipts.iter().all(|r| ledger
            .iter()
            .any(|e| e.to_string().contains(r.as_str().unwrap()))),
        "every receipt is on the ledger: {receipts:?}"
    );
    // Merged: asking again is refused, and nothing is merged twice.
    let e = mwr(&mut c, &task, g, true, "squash").await.unwrap_err();
    assert!(rejected_with(&e, "PR_NOT_OPEN"), "{e:?}");
    let _ = refresh(&mut c, &task, g).await;
    assert_eq!(merges(&gh).len(), 1);
}
