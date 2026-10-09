//! Helpers for the GitHub-native tests (PX-125, PX-127): approvals driven
//! over the real socket, the event log of a task, and the repository with
//! a bare remote the pull-request path pushes to. Included by `#[path]`
//! (not through `px_common`'s `mod.rs`) so other areas' edits to that file
//! never conflict with it.
#![allow(dead_code)]

use std::process::Command;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    ApprovalList, ApprovalResolvedAck, Id, ListApprovals, ResolveApproval, ToolInvoked,
};
use prost::Message;

use super::{CoreProcess, envelope, envelope_fenced, id16, replay};

/// The `GITHUB_*` environment that points a Core at the fake.
pub fn github_env(base: &str, token: &str) -> Vec<(String, String)> {
    vec![
        ("MODBIT_GITHUB_API_BASE_URL".into(), base.into()),
        ("MODBIT_GITHUB_TOKEN".into(), token.into()),
        ("MODBIT_GITHUB_WEB_HOST".into(), "github.test".into()),
    ]
}

/// Approve the open approval for `want_tool` (the person's decision with the
/// intent hash they saw). Returns the approval's tool name and effect class.
pub async fn approve(c: &mut Client, session: &Id, g: Option<u64>, cmd: u8, want_tool: &str) {
    let ack = c
        .command(envelope(
            id16(cmd),
            "ListApprovals",
            ListApprovals {
                session_id: Some(session.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let list: ApprovalList = Client::result(&ack).unwrap();
    let a = list
        .approvals
        .iter()
        .find(|a| a.status == "REQUESTED" && a.tool_name == want_tool)
        .cloned()
        .unwrap_or_else(|| panic!("a REQUESTED approval for {want_tool}: {list:?}"));
    assert_eq!(a.effect_class, "ExternalSideEffect");
    let ack = c
        .command(envelope_fenced(
            id16(cmd.wrapping_add(1)),
            "ResolveApproval",
            ResolveApproval {
                approval_id: a.approval_id.clone(),
                approve: true,
                reason: "ok".into(),
                intent_hash: a.intent_hash.clone(),
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
}

/// The events of one task (and the aggregates under it) as
/// `(event_type, payload)`, oldest first.
pub async fn task_events(
    core: &CoreProcess,
    session: &Id,
    task: &Id,
) -> Vec<(String, serde_json::Value)> {
    let want = hex::encode(&task.value);
    replay(core, session)
        .await
        .into_iter()
        .filter(|e| e["task_id"].as_str() == Some(want.as_str()))
        .map(|e| {
            // An inline payload is wrapped `{kind, payload}` on the wire.
            let p = if e["payload"]["kind"] == "inline" {
                e["payload"]["payload"].clone()
            } else {
                e["payload"].clone()
            };
            (e["event_type"].as_str().unwrap_or_default().to_owned(), p)
        })
        .collect()
}

/// The structured output of a finished tool call.
pub fn structured(r: &ToolInvoked) -> serde_json::Value {
    serde_json::from_str(&r.structured_output_json).unwrap_or_default()
}

/// Run a git command in `dir`, asserting it succeeds.
pub fn git(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A bare remote `https://github.test/<owner>/<repo>.git` resolves to (by
/// git's own `insteadOf`), wired into the repository at `repo`.
pub fn bare_remote(repo: &std::path::Path, owner: &str, name: &str) -> tempfile::TempDir {
    let bare = tempfile::tempdir().unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q", "--bare", "-b", "main"])
            .arg(bare.path())
            .status()
            .unwrap()
            .success()
    );
    let url = format!("https://github.test/{owner}/{name}.git");
    git(repo, &["remote", "add", "origin", &url]);
    git(
        repo,
        &[
            "config",
            &format!("url.{}.insteadOf", bare.path().to_str().unwrap()),
            &url,
        ],
    );
    bare
}

/// Whether a client error is a typed rejection with `code`.
pub fn rejected_with(e: &ClientError, code: &str) -> bool {
    matches!(e, ClientError::Rejected { code: c, .. } if c == code)
}
