//! CI results as provenance-bearing evidence (PX-009; docs/29 "CI
//! evidence", REQ-EV-0010).
//!
//! The forge's check runs for the commit this Core pushed for the task's pull
//! request are read through `forge.ci.status` — under the task's lease and
//! the kernel like any other forge read, with the forge token in the Core's
//! custody — normalized by the Verification Engine (`modbit_verification::ci`)
//! and recorded on the task as `CiEvidenceRecorded` with provenance `ci`,
//! each run's own output kept as an object a reader pages by range. Review
//! shows them. They are never a verification result: nothing here writes a
//! verification run, a check result or a gate record, so a green run on the
//! forge cannot pass a check Modbit has not run.

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{CiCheckRecord, CiRejectedRecord, TaskEvent};
use modbit_domain::{TaskId, ToolCallId};
use modbit_protocol::v1 as wire;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

fn refuse(code: &str, detail: impl Into<String>) -> (String, String) {
    (code.to_owned(), detail.into())
}

/// The CI evidence recorded for a task, as the wire shows it: every accepted
/// run of every ingestion, newest last.
pub(crate) fn views(
    store: &modbit_event_store::EventStore,
    task_id: TaskId,
) -> Vec<wire::CiCheckView> {
    let mut out = Vec::new();
    for e in store
        .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default()
        .iter()
        .filter(|e| e.envelope.event_type == "CiEvidenceRecorded")
    {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let Ok(TaskEvent::CiEvidenceRecorded {
            commit,
            checks,
            provenance,
            ..
        }) = serde_json::from_value::<TaskEvent>(p)
        else {
            continue;
        };
        for c in checks {
            out.push(view(&c, &commit, &provenance));
        }
    }
    out
}

fn view(c: &CiCheckRecord, commit: &str, provenance: &str) -> wire::CiCheckView {
    wire::CiCheckView {
        name: c.name.clone(),
        run_id: c.run_id,
        status: c.status.clone(),
        conclusion: c.conclusion.clone(),
        url: c.url.clone(),
        completed_at: c.completed_at.clone(),
        log_ref: c.log_ref.clone(),
        log_truncated: c.log_truncated,
        commit: commit.to_owned(),
        provenance: provenance.to_owned(),
    }
}

/// `IngestCiResults`.
pub(crate) async fn ingest(
    core: &Core,
    task_id: TaskId,
    actor: Actor,
    lease_generation: Option<u64>,
) -> Result<wire::CiResultsIngested, (String, String)> {
    // The task, its session and the pull request its log says it opened.
    let (task, session, pull) = {
        let store = core.store.lock().await;
        let task = store
            .task(&task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_TASK", task_id.to_string()))?;
        let session = store
            .session(&task.session_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_SESSION", task.session_id.to_string()))?;
        let pull = store
            .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
            .unwrap_or_default()
            .iter()
            .rev()
            .find(|e| e.envelope.event_type == "ForgePullRequestOpened")
            .and_then(|e| store.payload(&e.envelope).ok())
            .map(|p| {
                (
                    p["owner"].as_str().unwrap_or_default().to_owned(),
                    p["repo"].as_str().unwrap_or_default().to_owned(),
                    p["number"].as_u64().unwrap_or(0),
                )
            });
        (task, session, pull)
    };
    let Some((owner, repo, number)) =
        pull.filter(|(o, r, n)| !o.is_empty() && !r.is_empty() && *n > 0)
    else {
        return Err(refuse(
            "NO_PULL_REQUEST",
            "no pull request was opened for this task (OpenPullRequest)",
        ));
    };
    // The commit is the one this Core pushed for the pull request — its
    // branch here — not whatever the forge says the head is now.
    let root = task
        .workspace_root
        .clone()
        .ok_or_else(|| refuse("NO_WORKSPACE", "task has no workspace root"))?;
    let git = modbit_git::Repo::open(std::path::Path::new(&root))
        .map_err(|e| refuse("GIT", e.to_string()))?;
    let branch = format!(
        "modbit/pr-{}",
        &task.task_id.to_string().replace('-', "")[..12]
    );
    let commit = git.rev_parse(&branch).map_err(|e| {
        refuse(
            "NO_PUSHED_COMMIT",
            format!("the pull request's branch `{branch}` is not here: {e}"),
        )
    })?;
    let lease = {
        let store = core.store.lock().await;
        store
            .leases_for_task(&task.task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .into_iter()
            .next()
    };
    let tool_call_id = ToolCallId::new();
    let arguments_json =
        serde_json::json!({"owner": owner, "repo": repo, "ref": commit}).to_string();
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
                tool_name: "forge.ci.status",
                arguments_json: &arguments_json,
                output_budget_bytes: 262_144,
                actor: actor.clone(),
                lease,
                approval: None,
                emergency_stopped: session.emergency_stopped_at.is_some(),
                existing: None,
                run_id: None,
                turn_id: None,
                call_id: None,
                lease_generation,
                projection: None,
                cancel: None,
            },
        )
        .await
        .map_err(|e| refuse("TOOL_HOST", e.to_string()))?;
    let r = &done.result;
    if !matches!(r.status, modbit_tools::ToolStatus::Success) {
        return Err(refuse(
            r.error_code.as_deref().unwrap_or("FORGE_READ"),
            r.error_message.clone().unwrap_or_default(),
        ));
    }
    let Some(checks) = r.structured_output["checks"].as_array() else {
        return Err(refuse(
            "FORGE_READ",
            "the forge's answer carried no check runs",
        ));
    };
    let found = modbit_verification::ci::ingest(&commit, checks);
    let mut store = core.store.lock().await;
    let mut records = Vec::new();
    for c in &found.accepted {
        let log_ref = if c.log.is_empty() {
            String::new()
        } else {
            store
                .objects()
                .put(c.log.as_bytes())
                .map_err(|e| refuse("STORE", e.to_string()))?
        };
        records.push(CiCheckRecord {
            name: c.name.clone(),
            run_id: c.run_id,
            status: c.status.clone(),
            conclusion: c.conclusion.clone(),
            url: c.url.clone(),
            completed_at: c.completed_at.clone(),
            log_ref,
            log_truncated: c.log_truncated,
        });
    }
    let rejected: Vec<CiRejectedRecord> = found
        .rejected
        .iter()
        .map(|x| CiRejectedRecord {
            name: x.name.clone(),
            head_sha: x.head_sha.clone(),
            reason: x.reason.clone(),
        })
        .collect();
    let offset = append(
        &mut store,
        core,
        Lineage::task(core.tenant_id, task.session_id, task.task_id),
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "CiEvidenceRecorded",
            &TaskEvent::CiEvidenceRecorded {
                provider: "github".into(),
                owner: owner.clone(),
                repo: repo.clone(),
                pull_number: number,
                commit: commit.clone(),
                checks: records.clone(),
                rejected: rejected.clone(),
                tool_call_id: tool_call_id.to_string(),
                provenance: modbit_verification::ci::CI_PROVENANCE.into(),
            },
            actor,
        )],
    )
    .map_err(|e| refuse("STORE", e))?;
    drop(store);
    core.last_offset.send_replace(offset);
    Ok(wire::CiResultsIngested {
        provider: "github".into(),
        owner,
        repo,
        pull_number: number,
        checks: records
            .iter()
            .map(|c| view(c, &commit, modbit_verification::ci::CI_PROVENANCE))
            .collect(),
        rejected: rejected
            .into_iter()
            .map(|x| wire::CiRejectedView {
                name: x.name,
                head_sha: x.head_sha,
                reason: x.reason,
            })
            .collect(),
        commit,
        offset,
        evidence_class: modbit_verification::ci::CI_EVIDENCE_CLASS.into(),
    })
}
