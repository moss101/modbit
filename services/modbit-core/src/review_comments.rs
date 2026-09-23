//! Review-comment steering (PX-008; docs/29 "Review-comment steering",
//! REQ-EV-0010).
//!
//! The task's pull-request comments are read through `forge.pr.comments.read`
//! — under the task's lease and the kernel like any other forge read — and
//! each one is decided once:
//!
//! * from an identity the organization allows (the admin layer's
//!   `review_comment_authors`; a lower layer may only narrow it) and
//!   addressed to Modbit (`@modbit`): queued as a STEER input through the
//!   ordinary steering path (`TaskInputQueued`, provenance
//!   `forge_review_comment`, untrusted), which the loop applies at its next
//!   boundary fenced as external content;
//! * anything else: recorded with why it was ignored, and nothing more.
//!
//! A comment is text, whoever wrote it. It is never an approval, a grant or a
//! configuration change: approvals are decided by a person through
//! `ResolveApproval`, capabilities come from the task's lease and policy, and
//! a steer that asks for either changes neither.

use std::collections::HashSet;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{InputMode, ReviewCommentRecord, TaskEvent};
use modbit_domain::{TaskId, ToolCallId};
use modbit_protocol::v1 as wire;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// Provenance of a steer that came from a forge review comment.
pub(crate) const PROVENANCE: &str = "forge_review_comment";

/// The mention that addresses Modbit.
const MENTION: &str = "@modbit";

/// Bytes of one comment kept as steering text.
const MAX_COMMENT_BYTES: usize = 8 * 1024;

fn refuse(code: &str, detail: impl Into<String>) -> (String, String) {
    (code.to_owned(), detail.into())
}

fn view(r: &ReviewCommentRecord) -> wire::ReviewCommentView {
    wire::ReviewCommentView {
        comment_id: r.comment_id,
        kind: r.kind.clone(),
        author: r.author.clone(),
        url: r.url.clone(),
        input_id: r.input_id.clone(),
        reason: r.reason.clone(),
    }
}

/// Whether a comment addresses Modbit: the mention as a word of its own.
fn addressed(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.match_indices(MENTION).any(|(i, _)| {
        let after = lower[i + MENTION.len()..].chars().next();
        !after.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

/// Cut `s` to at most `max` bytes on a character boundary.
fn bounded(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [cut at {max} bytes]", &s[..end])
}

/// `IngestReviewComments`.
#[allow(clippy::too_many_lines)]
pub(crate) async fn ingest(
    core: &Core,
    task_id: TaskId,
    actor: Actor,
    lease_generation: Option<u64>,
) -> Result<wire::ReviewCommentsIngestedView, (String, String)> {
    let (task, session, pull, taken) = {
        let store = core.store.lock().await;
        let task = store
            .task(&task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_TASK", task_id.to_string()))?;
        let session = store
            .session(&task.session_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_SESSION", task.session_id.to_string()))?;
        let events = store
            .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
            .unwrap_or_default();
        let pull = events
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
        // Every comment decided before, steered or ignored, is not taken again.
        let mut taken: HashSet<u64> = HashSet::new();
        for e in events
            .iter()
            .filter(|e| e.envelope.event_type == "ReviewCommentsIngested")
        {
            if let Ok(TaskEvent::ReviewCommentsIngested {
                steered, ignored, ..
            }) = store
                .payload(&e.envelope)
                .map_err(|e| e.to_string())
                .and_then(|p| serde_json::from_value::<TaskEvent>(p).map_err(|e| e.to_string()))
            {
                taken.extend(steered.iter().chain(ignored.iter()).map(|c| c.comment_id));
            }
        }
        (task, session, pull, taken)
    };
    let Some((owner, repo, number)) =
        pull.filter(|(o, r, n)| !o.is_empty() && !r.is_empty() && *n > 0)
    else {
        return Err(refuse(
            "NO_PULL_REQUEST",
            "no pull request was opened for this task (OpenPullRequest)",
        ));
    };
    // The organization's allow-list, as resolved now; nobody when unset.
    let allowed: HashSet<String> = modbit_policy::config::resolve(&crate::config::layers_for(
        &core.data_dir,
        task.workspace_root.as_deref(),
    ))
    .review_comment_authors
    .map(|r| {
        r.value
            .into_iter()
            .map(|a| a.to_ascii_lowercase())
            .collect()
    })
    .unwrap_or_default();
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
        serde_json::json!({"owner": owner, "repo": repo, "number": number}).to_string();
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
                tool_name: "forge.pr.comments.read",
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
                compensates: None,
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
    let Some(comments) = r.structured_output["comments"].as_array() else {
        return Err(refuse(
            "FORGE_READ",
            "the forge's answer carried no comments",
        ));
    };
    let mut comments: Vec<&serde_json::Value> = comments.iter().collect();
    comments.sort_by(|a, b| {
        (a["created_at"].as_str(), a["id"].as_u64())
            .cmp(&(b["created_at"].as_str(), b["id"].as_u64()))
    });
    let mut steered = Vec::new();
    let mut ignored = Vec::new();
    let mut inputs = Vec::new();
    let mut already_taken = 0u32;
    for c in comments {
        let Some(comment_id) = c["id"].as_u64() else {
            continue;
        };
        if taken.contains(&comment_id) {
            already_taken += 1;
            continue;
        }
        let author = c["author"].as_str().unwrap_or_default().to_owned();
        let body = c["body"].as_str().unwrap_or_default();
        let mut rec = ReviewCommentRecord {
            comment_id,
            kind: c["kind"].as_str().unwrap_or_default().to_owned(),
            author: author.clone(),
            url: c["url"].as_str().unwrap_or_default().to_owned(),
            input_id: String::new(),
            reason: String::new(),
        };
        if !allowed.contains(&author.to_ascii_lowercase()) {
            rec.reason = "DISALLOWED_AUTHOR".into();
            ignored.push(rec);
            continue;
        }
        if !addressed(body) {
            rec.reason = "NOT_ADDRESSED".into();
            ignored.push(rec);
            continue;
        }
        if body.trim().is_empty() {
            rec.reason = "EMPTY".into();
            ignored.push(rec);
            continue;
        }
        rec.input_id = format!("forge-comment-{comment_id}");
        let location = match (c["path"].as_str(), c["line"].as_u64()) {
            (Some(p), Some(l)) => format!(" on {p}:{l}"),
            (Some(p), None) => format!(" on {p}"),
            _ => String::new(),
        };
        inputs.push(typed(
            "TaskInputQueued",
            &TaskEvent::TaskInputQueued {
                input_id: rec.input_id.clone(),
                mode: InputMode::Steer,
                text: format!(
                    "Pull request #{number} comment by @{author}{location} ({}):\n{}",
                    rec.url,
                    bounded(body, MAX_COMMENT_BYTES)
                ),
                provenance: PROVENANCE.into(),
                untrusted: true,
            },
            actor.clone(),
        ));
        steered.push(rec);
    }
    let mut events = inputs;
    events.push(typed(
        "ReviewCommentsIngested",
        &TaskEvent::ReviewCommentsIngested {
            owner: owner.clone(),
            repo: repo.clone(),
            pull_number: number,
            steered: steered.clone(),
            ignored: ignored.clone(),
            tool_call_id: tool_call_id.to_string(),
        },
        actor,
    ));
    let offset = {
        let mut store = core.store.lock().await;
        append(
            &mut store,
            core,
            Lineage::task(core.tenant_id, task.session_id, task.task_id),
            AggregateType::Task,
            *task.task_id.as_bytes(),
            events,
        )
        .map_err(|e| refuse("STORE", e))?
    };
    core.last_offset.send_replace(offset);
    Ok(wire::ReviewCommentsIngestedView {
        owner,
        repo,
        pull_number: number,
        steered: steered.iter().map(view).collect(),
        ignored: ignored.iter().map(view).collect(),
        already_taken,
        offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mention_must_stand_as_a_word() {
        assert!(addressed("@modbit please guard negatives"));
        assert!(addressed("LGTM. @Modbit, one more thing."));
        assert!(!addressed("@modbitbot do it"));
        assert!(!addressed("@modbit-ci ran"));
        assert!(!addressed("please guard negatives"));
    }

    #[test]
    fn a_long_comment_is_cut_on_a_character_boundary_and_says_so() {
        let s = bounded(&"é".repeat(10), 5);
        assert_eq!(s, "éé… [cut at 5 bytes]");
    }
}
