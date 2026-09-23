//! Pull requests from a reviewed result (PX-007; docs/20 "Pull requests",
//! docs/29): `OpenPullRequest` and `UpdatePullRequest`. The Core points a
//! dedicated branch at the commit the review accepted; the pull request is
//! `forge.pr.create` / `forge.pr.update` through the tool pipeline under the
//! task's lease — an approval-bound external effect with a receipt — and the
//! branch is pushed through the typed Git operation only once that approval
//! stands, right before the forge call, so a denied approval leaves no
//! branch on the remote. Everything is bound to the exact candidate revision
//! the review accepted; the idempotency key and the tool call id derive from
//! task and revision, so a retry after a crash — before or after the push,
//! before or after the receipt — converges on the one pull request.

use modbit_domain::event::Actor;
use modbit_domain::state::StateMachine;
use modbit_domain::task::{Task, TaskState};
use modbit_domain::{TaskId, ToolCallId};
use modbit_git::Repo;
use modbit_protocol::v1 as wire;
use sha2::Digest;

use crate::server::Core;

/// What the log says the review accepted, most recent first.
struct Accepted {
    candidate_revision: u64,
    commit: String,
    note: String,
    accepted: usize,
    rejected: usize,
}

fn refuse(code: &str, msg: impl Into<String>) -> (String, String) {
    (code.to_owned(), msg.into())
}

/// The task's latest ACCEPT decision with a commit.
fn latest_accept(
    store: &modbit_event_store::EventStore,
    task_id: TaskId,
) -> Option<(Accepted, Vec<serde_json::Value>)> {
    let events = store
        .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    let mut accepted = None;
    let mut all = Vec::new();
    for e in &events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        if e.envelope.event_type == "ReviewDecisionRecorded"
            && p["decision"] == "ACCEPT"
            && let Some(commit) = p["commit"].as_str().filter(|c| !c.is_empty())
        {
            accepted = Some(Accepted {
                candidate_revision: p["candidate_revision"].as_u64().unwrap_or(0),
                commit: commit.to_owned(),
                note: p["note"].as_str().unwrap_or_default().to_owned(),
                accepted: p["accepted"].as_array().map_or(0, Vec::len),
                rejected: p["rejected"].as_array().map_or(0, Vec::len),
            });
        }
        let mut v = p;
        v["__type"] = serde_json::json!(e.envelope.event_type);
        v["__offset"] = serde_json::json!(e.offset);
        all.push(v);
    }
    accepted.map(|a| (a, all))
}

/// Every event of the task with its type and offset (for the summary).
fn all_events(store: &modbit_event_store::EventStore, task_id: TaskId) -> Vec<serde_json::Value> {
    store
        .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            let mut v = store.payload(&e.envelope).ok()?;
            v["__type"] = serde_json::json!(e.envelope.event_type);
            v["__offset"] = serde_json::json!(e.offset);
            Some(v)
        })
        .collect()
}

/// `owner`, `repo` of a remote URL on the forge's web host.
fn parse_remote(url: &str, web_host: &str) -> Result<(String, String), (String, String)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("ssh://git@"))
        .or_else(|| url.strip_prefix("git@"))
        .ok_or_else(|| refuse("FORGE_REMOTE", format!("remote `{url}` is not a forge URL")))?;
    let (host, path) = rest
        .split_once('/')
        .or_else(|| rest.split_once(':'))
        .ok_or_else(|| {
            refuse(
                "FORGE_REMOTE",
                format!("remote `{url}` names no repository"),
            )
        })?;
    let host = host.split('@').next_back().unwrap_or(host);
    if host != web_host {
        return Err(refuse(
            "FORGE_HOST_MISMATCH",
            format!("remote `{url}` is on `{host}`, the configured forge is `{web_host}`"),
        ));
    }
    let mut parts = path
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .split('/');
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    if owner.is_empty() || repo.is_empty() {
        return Err(refuse(
            "FORGE_REMOTE",
            format!("remote `{url}` names no owner/repo"),
        ));
    }
    Ok((owner.to_owned(), repo.to_owned()))
}

/// The evidence summary the pull request body carries: what the log holds
/// for the accepted revision, never a claim the log does not.
fn evidence_summary(task: &Task, a: &Accepted, events: &[serde_json::Value]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "## Modbit evidence summary\n\n- task `{}`: {}\n- candidate workspace revision {}; commit `{}`\n- review: ACCEPT, {} hunk(s) accepted, {} rejected{}\n",
        task.task_id,
        task.goal_text,
        a.candidate_revision,
        a.commit,
        a.accepted,
        a.rejected,
        if a.note.is_empty() {
            String::new()
        } else {
            format!(" — \"{}\"", a.note.replace('\n', " "))
        }
    ));
    let mut runs = 0;
    for e in events {
        match e["__type"].as_str().unwrap_or_default() {
            "VerificationRunRecorded" => {
                let checks = e["checks"].as_array().cloned().unwrap_or_default();
                let pass = checks.iter().filter(|c| c["status"] == "PASS").count();
                out.push_str(&format!(
                    "- verification {} {}: {}/{} checks passed (revision {})\n",
                    e["stage"].as_str().unwrap_or_default(),
                    e["status"].as_str().unwrap_or_default(),
                    pass,
                    checks.len(),
                    e["candidate_revision"].as_str().unwrap_or_default()
                ));
                runs += 1;
            }
            "AcceptanceGateEvaluated" => out.push_str(&format!(
                "- acceptance gate {} at revision {} ({})\n",
                e["verdict"].as_str().unwrap_or_default(),
                e["candidate_revision"].as_u64().unwrap_or(0),
                e["trigger"].as_str().unwrap_or_default()
            )),
            "ReviewerResultRecorded" => out.push_str(&format!(
                "- independent reviewer: {} (confidence {})\n",
                e["verdict"].as_str().unwrap_or_default(),
                e["confidence"]
            )),
            "RealizedRiskDerived" => out.push_str(&format!(
                "- realized risk: {}\n",
                e["level"].as_str().unwrap_or_default()
            )),
            _ => {}
        }
    }
    let receipts = events
        .iter()
        .filter(|e| e["__type"] == "EffectReceiptAppended")
        .count();
    out.push_str(&format!(
        "- {runs} verification run(s), {receipts} effect receipt(s) on the task's log\n"
    ));
    out.push_str("\nGenerated by Modbit from the canonical log; the pull request is bound to the revision above.\n");
    out
}

/// The tool call id a (task, revision, verb, attempt) always maps to, so a
/// retry re-enters the same call; a denied attempt is terminal and the next
/// attempt is the next id, under the same idempotency key.
fn call_id(task_id: TaskId, revision: u64, verb: &str, attempt: u32) -> ToolCallId {
    let h = sha2::Sha256::digest(format!("{verb}:{task_id}:{revision}:{attempt}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    ToolCallId::from_bytes(b)
}

/// The ledger's key for the same call.
fn idempotency_key(task_id: TaskId, revision: u64, verb: &str) -> String {
    format!("{verb}:{task_id}:{revision}")
}

pub struct Request<'a> {
    pub task_id: TaskId,
    pub expected_candidate_revision: u64,
    pub base: &'a str,
    pub title: &'a str,
    pub remote: &'a str,
    pub actor: Actor,
    pub lease_generation: Option<u64>,
}

/// `OpenPullRequest` (`update = false`) and `UpdatePullRequest` (`update = true`).
pub async fn run(
    core: &Core,
    req: Request<'_>,
    update: bool,
) -> Result<wire::PullRequestAck, (String, String)> {
    let (task, session) = {
        let store = core.store.lock().await;
        let task = store
            .task(&req.task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_TASK", req.task_id.to_string()))?;
        let session = store
            .session(&task.session_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_SESSION", task.session_id.to_string()))?;
        (task, session)
    };
    if !matches!(task.state, TaskState::Completed | TaskState::ReadyForReview) {
        return Err(refuse(
            "NOT_REVIEWED",
            format!(
                "task is {:?}; a pull request opens for a result under review or accepted",
                task.state
            ),
        ));
    }
    let root = task
        .workspace_root
        .clone()
        .ok_or_else(|| refuse("NO_WORKSPACE", "task has no workspace root"))?;
    let repo = Repo::open(std::path::Path::new(&root)).map_err(|e| refuse("GIT", e.to_string()))?;
    let (accepted, events) = if task.state == TaskState::Completed {
        let store = core.store.lock().await;
        latest_accept(&store, task.task_id).ok_or_else(|| {
            refuse(
                "NOT_REVIEWED",
                "no accepted review with a commit on the task's log",
            )
        })?
    } else {
        // Under review: the candidate is the worktree at its current revision,
        // offered as a snapshot commit (the worktree itself is untouched).
        let (ws, _) = core
            .tools
            .workspace(&root)
            .await
            .map_err(|e| refuse("WORKSPACE", e.to_string()))?;
        let revision = ws.lock().await.revision().number;
        // One snapshot per (task, revision): the commit the approval binds
        // must be the commit the approved call pushes.
        let snapshot_id = format!("pr-{}-{revision}", task.task_id);
        let snapshot_ref = format!("refs/modbit/snapshots/{snapshot_id}");
        let commit = if repo.ref_exists(&snapshot_ref) {
            repo.rev_parse(&snapshot_ref)
                .map_err(|e| refuse("GIT", e.to_string()))?
        } else {
            repo.snapshot_dirty(&snapshot_id)
                .map_err(|e| refuse("GIT", e.to_string()))?
                .commit
        };
        let events = {
            let store = core.store.lock().await;
            all_events(&store, task.task_id)
        };
        (
            Accepted {
                candidate_revision: revision,
                commit,
                note: String::new(),
                accepted: 0,
                rejected: 0,
            },
            events,
        )
    };
    if req.expected_candidate_revision != accepted.candidate_revision {
        return Err(refuse(
            "STALE_REVISION",
            format!(
                "the review accepted candidate revision {}, the request names {}",
                accepted.candidate_revision, req.expected_candidate_revision
            ),
        ));
    }
    let cfg = core.tools.forge.get().ok_or_else(|| {
        refuse(
            "NO_FORGE",
            "no forge is configured for this Core (ConfigureForge)",
        )
    })?;
    let remote = if req.remote.is_empty() {
        "origin"
    } else {
        req.remote
    };
    let url = repo
        .remote_url(remote)
        .map_err(|e| refuse("FORGE_REMOTE", format!("remote `{remote}`: {e}")))?;
    let (owner, repo_name) = parse_remote(&url, &cfg.web_host)?;
    let branch = format!(
        "modbit/pr-{}",
        &task.task_id.to_string().replace('-', "")[..12]
    );
    let base = if req.base.is_empty() {
        repo.current_branch()
            .ok()
            .flatten()
            .unwrap_or_else(|| "main".to_owned())
    } else {
        req.base.to_owned()
    };
    let verb = if update { "pr-update" } else { "pr-open" };
    let revision = accepted.candidate_revision;
    let key = idempotency_key(task.task_id, revision, verb);
    // The first attempt not denied (a denial is terminal for its call; the
    // person may decide again on a new call, still under the one key).
    let tool_call_id = {
        let store = core.store.lock().await;
        let mut chosen = call_id(task.task_id, revision, verb, 0);
        for attempt in 0..16u32 {
            let id = call_id(task.task_id, revision, verb, attempt);
            chosen = id;
            match store
                .tool_call(&id)
                .map_err(|e| refuse("STORE", e.to_string()))?
            {
                Some(c) if c.state.is_terminal() && c.result_ref.is_none() => continue,
                _ => break,
            }
        }
        chosen
    };
    let body = evidence_summary(&task, &accepted, &events);
    let title = if req.title.is_empty() {
        if accepted.note.trim().is_empty() {
            task.goal_text.clone()
        } else {
            accepted.note.lines().next().unwrap_or_default().to_owned()
        }
    } else {
        req.title.to_owned()
    };
    // The prior pull request (an update needs one; an open reports one).
    let prior = events
        .iter()
        .rfind(|e| e["__type"] == "ForgePullRequestOpened")
        .cloned();
    let (tool_name, args) = if update {
        let Some(prior) = &prior else {
            return Err(refuse(
                "NO_PULL_REQUEST",
                "no pull request was opened for this task",
            ));
        };
        (
            "forge.pr.update",
            serde_json::json!({
                "owner": owner, "repo": repo_name,
                "number": prior["number"].as_u64().unwrap_or(0),
                "body": body,
                "idempotency_key": key,
            }),
        )
    } else {
        (
            "forge.pr.create",
            serde_json::json!({
                "owner": owner, "repo": repo_name,
                "head": branch, "base": base, "title": title, "body": body,
                "idempotency_key": key,
            }),
        )
    };
    let arguments_json = args.to_string();
    // The call's state: pending, approved, denied or done.
    let (existing, lease, approval) = {
        let store = core.store.lock().await;
        let existing = store
            .tool_call(&tool_call_id)
            .map_err(|e| refuse("STORE", e.to_string()))?;
        let lease = store
            .leases_for_task(&task.task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .into_iter()
            .next();
        let approval = store
            .approval_for_call(&tool_call_id)
            .map_err(|e| refuse("STORE", e.to_string()))?;
        (existing, lease, approval)
    };
    if let Some(c) = &existing {
        if let Some(r) = &c.result_ref {
            // Done before: the recorded result answers again.
            let store = core.store.lock().await;
            if let Ok(bytes) = store.objects().get(r)
                && let Ok(prior) = serde_json::from_slice::<modbit_tools::ToolCallResult>(&bytes)
            {
                return Ok(ack_of(
                    &prior,
                    c.approval_id,
                    &branch,
                    revision,
                    &accepted.commit,
                    &arguments_json,
                    true,
                ));
            }
        }
        if c.state.is_terminal() {
            return Ok(wire::PullRequestAck {
                status: "DENIED".into(),
                approval_id: c.approval_id.map(|a| a.to_string()).unwrap_or_default(),
                branch,
                candidate_revision: revision,
                detail: c.policy_decision.clone().unwrap_or_default(),
                ..Default::default()
            });
        }
    }
    // Approved and not yet run: the branch goes to the remote now, then the
    // forge call runs under the approval. Nothing before this point touched
    // the remote.
    let approved = approval
        .as_ref()
        .is_some_and(|a| a.state == modbit_domain::approval::ApprovalState::Approved);
    if approved {
        repo.set_branch(&branch, &accepted.commit)
            .map_err(|e| refuse("GIT", e.to_string()))?;
        let force = update
            || repo
                .remote_branch_head(remote, &branch)
                .ok()
                .flatten()
                .is_some_and(|h| h != accepted.commit);
        repo.push_branch(remote, &branch, force)
            .map_err(|e| refuse("GIT_PUSH", format!("pushing `{branch}` to `{remote}`: {e}")))?;
    }
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
                tool_name,
                arguments_json: &arguments_json,
                output_budget_bytes: 65_536,
                actor: req.actor,
                lease,
                approval,
                emergency_stopped: session.emergency_stopped_at.is_some(),
                existing,
                run_id: None,
                turn_id: None,
                call_id: None,
                lease_generation: req.lease_generation,
                projection: None,
                cancel: None,
                compensates: None,
            },
        )
        .await
        .map_err(|e| refuse("TOOL_HOST", e.to_string()))?;
    let offset = core.store.lock().await.last_offset().unwrap_or(0);
    core.last_offset.send_replace(offset);
    Ok(ack_of(
        &done.result,
        done.approval_id,
        &branch,
        revision,
        &accepted.commit,
        &arguments_json,
        false,
    ))
}

fn ack_of(
    r: &modbit_tools::ToolCallResult,
    approval_id: Option<modbit_domain::ApprovalId>,
    branch: &str,
    revision: u64,
    head_sha: &str,
    arguments_json: &str,
    replayed: bool,
) -> wire::PullRequestAck {
    let status = format!("{:?}", r.status).to_uppercase();
    let approval_hex = approval_id.map(|a| a.to_string()).unwrap_or_default();
    match status.as_str() {
        "SUCCESS" => wire::PullRequestAck {
            status: if r.tool_name == "forge.pr.update" {
                "UPDATED"
            } else {
                "OPENED"
            }
            .into(),
            approval_id: approval_hex,
            number: r.structured_output["number"].as_u64().unwrap_or(0),
            url: r.structured_output["url"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            branch: branch.to_owned(),
            // The commit this Core pushed: the forge's view of the head can lag.
            head_sha: head_sha.to_owned(),
            candidate_revision: revision,
            effect_receipt_ids: r.effect_receipt_ids.clone(),
            replayed: replayed || r.structured_output["replayed"].as_bool().unwrap_or(false),
            ..Default::default()
        },
        "APPROVALPENDING" => wire::PullRequestAck {
            status: "APPROVAL_PENDING".into(),
            approval_id: approval_hex,
            intent_hash: modbit_tools::arguments_hash(arguments_json).unwrap_or_default(),
            branch: branch.to_owned(),
            candidate_revision: revision,
            detail: "decide the approval, then call again".into(),
            ..Default::default()
        },
        _ => wire::PullRequestAck {
            status: "DENIED".into(),
            approval_id: approval_hex,
            branch: branch.to_owned(),
            candidate_revision: revision,
            detail: format!(
                "{}: {}",
                r.error_code.clone().unwrap_or_default(),
                r.error_message.clone().unwrap_or_default()
            ),
            ..Default::default()
        },
    }
}
