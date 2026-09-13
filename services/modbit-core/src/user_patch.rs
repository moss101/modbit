//! Constrained inline patch (PX-005; docs/20 "Constrained inline patch",
//! docs/29): a person's small direct edit from the review surface or a thin
//! client. The only path is the canonical ChangeTransaction on the Workspace
//! File Service — the workspace revision the client saw is a precondition,
//! the path is resolved through symlinks and checked against the protected
//! patterns before anything is opened, the edit target must match exactly
//! once at a ladder tier — and it lands as one `FileChanged` on the
//! workspace log (provenance `user_direct_edit`, the command id where a tool
//! call id would be) and one `UserPatchApplied` on the task's, in one
//! transaction under the command record, with one revision advance. A retry
//! with the same command id replays the record and writes nothing. No client
//! holds a buffer: what is not applied here does not exist.

use std::collections::HashMap;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::TaskEvent;
use modbit_domain::{TaskId, ToolCallId};
use modbit_event_store::{AppendRequest, CommandOutcome, CommandRecord};
use modbit_protocol::v1 as wire;
use modbit_workspace::{ChangeOp, ChangeOpKind, TextEdit, WritePrecondition, locate};

use crate::runtime::typed;
use crate::server::Core;
use crate::tools::{file_changed_events, read_workspace_file, workspace_aggregate_id};

/// The refusal code for a Workspace File Service error (docs/23, docs/20).
fn code_of(e: &modbit_workspace::Error) -> &'static str {
    use modbit_workspace::Error as E;
    match e {
        E::StepFailed { cause, .. } => code_of(cause),
        E::Protected { .. } => "PROTECTED_PATH",
        E::OutsideRoot { .. } => "PATH_OUTSIDE_ROOT",
        E::Precondition { .. } => "STALE_REVISION",
        E::AmbiguousTarget { .. } | E::NoMatch { .. } => "NO_UNIQUE_MATCH",
        E::Invalid { .. } | E::EditOutOfBounds { .. } => "BAD_PAYLOAD",
        E::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => "NO_SUCH_FILE",
        E::Io { .. } => "WORKSPACE",
    }
}

/// The ack a recorded `UserPatchApplied` event describes (a replay).
fn ack_of(
    events: &[modbit_event_store::StoredEvent],
    store: &modbit_event_store::EventStore,
) -> Option<wire::UserPatchAppliedAck> {
    let e = events
        .iter()
        .find(|e| e.envelope.event_type == "UserPatchApplied")?;
    let p = store.payload(&e.envelope).ok()?;
    Some(wire::UserPatchAppliedAck {
        workspace_revision: p["workspace_revision"].as_u64().unwrap_or(0),
        previous_revision: p["previous_revision"].as_u64().unwrap_or(0),
        file_revision: p["after_hash"].as_str().unwrap_or_default().to_owned(),
        before_hash: p["before_hash"].as_str().unwrap_or_default().to_owned(),
        match_tier: p["match_tier"].as_str().unwrap_or_default().to_owned(),
        offset: e.offset,
        replayed: true,
    })
}

/// `ApplyUserPatch`.
pub async fn apply(
    core: &Core,
    p: &wire::ApplyUserPatch,
    command_id: [u8; 16],
    record: CommandRecord,
    actor: Actor,
) -> Result<wire::UserPatchAppliedAck, (String, String)> {
    let bad = |m: &str| Err(("BAD_PAYLOAD".to_owned(), m.to_owned()));
    let Some(task_id) = p
        .task_id
        .as_ref()
        .and_then(|i| <[u8; 16]>::try_from(i.value.as_slice()).ok())
        .map(TaskId::from_bytes)
    else {
        return bad("task_id required");
    };
    if p.path.trim().is_empty() {
        return bad("path required");
    }
    if p.old.is_empty() {
        return bad("old required: the text the patch replaces");
    }
    if p.old.len() > 256 * 1024 || p.new.len() > 256 * 1024 {
        return bad("a constrained inline patch is at most 256 KiB each side");
    }
    if p.expected_workspace_revision == 0 {
        return bad(
            "expected_workspace_revision required: the edit is bound to the revision the client saw",
        );
    }
    let source = match p.source.as_str() {
        "" => "cli".to_owned(),
        "review" | "cli" | "ide_adapter" => p.source.clone(),
        other => return bad(&format!("unknown patch source `{other}`")),
    };
    // A retry replays the record; the file is not written twice.
    {
        let store = core.store.lock().await;
        match store.prior_command(&record) {
            Ok(Some(CommandOutcome::Replayed(events))) => {
                return ack_of(&events, &store).ok_or_else(|| {
                    (
                        "REPLAY".to_owned(),
                        "the recorded command carries no patch".to_owned(),
                    )
                });
            }
            Ok(_) => {}
            Err(e) => return Err((crate::server::error_code(&e).to_owned(), e.to_string())),
        }
    }
    let task = core
        .store
        .lock()
        .await
        .task(&task_id)
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?
        .ok_or_else(|| ("UNKNOWN_TASK".to_owned(), task_id.to_string()))?;
    let root = task.workspace_root.clone().ok_or_else(|| {
        (
            "NO_WORKSPACE".to_owned(),
            "task has no workspace root".to_owned(),
        )
    })?;
    // The loop owns the tree while it runs; a person edits between runs.
    if core.runtime.is_running(&task_id).await {
        return Err((
            "TASK_RUNNING".to_owned(),
            "the task's loop is running; a direct edit waits for it to stop".to_owned(),
        ));
    }
    let (ws, _) = core
        .tools
        .workspace(&root)
        .await
        .map_err(|e| ("WORKSPACE".to_owned(), e.to_string()))?;
    let mut svc = ws.lock().await;
    let current_revision = svc.revision().number;
    if p.expected_workspace_revision != current_revision {
        return Err((
            "STALE_REVISION".to_owned(),
            format!(
                "the edit was made against workspace revision {}, the workspace is at {current_revision}; reload before editing",
                p.expected_workspace_revision
            ),
        ));
    }
    // Path policy after symlink resolution, before anything is opened
    // (docs/23): the resolved target decides, not the name.
    let resolved = svc
        .resolve(&p.path)
        .map_err(|e| (code_of(&e).to_owned(), e.to_string()))?;
    let read = svc
        .read(&p.path)
        .map_err(|e| (code_of(&e).to_owned(), e.to_string()))?;
    if !p.expected_file_revision.is_empty() && p.expected_file_revision != read.content_hash {
        return Err((
            "STALE_REVISION".to_owned(),
            format!(
                "the file is at revision {}, the edit was made against {}; reload before editing",
                read.content_hash, p.expected_file_revision
            ),
        ));
    }
    let text = std::str::from_utf8(&read.bytes).map_err(|_| {
        (
            "BAD_PAYLOAD".to_owned(),
            "a patch edits UTF-8 text".to_owned(),
        )
    })?;
    let (_, _, tier) =
        locate(text, &p.old, &p.path).map_err(|e| (code_of(&e).to_owned(), e.to_string()))?;
    let match_tier = match tier {
        modbit_workspace::MatchTier::Exact => "exact",
        modbit_workspace::MatchTier::WhitespaceRemap => "whitespace_remap",
    };
    let mut pre_bytes: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    pre_bytes.insert(resolved.relative.clone(), Some(read.bytes.clone()));
    let before_hash = read.content_hash.clone();
    let changes = svc
        .apply_transaction(&[ChangeOp {
            path: resolved.relative.clone(),
            kind: ChangeOpKind::Edit(vec![TextEdit {
                old: p.old.clone(),
                new: p.new.clone(),
            }]),
            pre: WritePrecondition {
                expected_content_hash: Some(before_hash.clone()),
                expected_workspace_revision: Some(current_revision),
                expect_absent: false,
            },
        }])
        .map_err(|e| (code_of(&e).to_owned(), e.to_string()))?;
    let Some(change) = changes.first() else {
        return Err(("WORKSPACE".to_owned(), "no change recorded".to_owned()));
    };
    let after_hash = change.after_hash.clone().unwrap_or_default();
    let workspace_revision = change.workspace_revision.number;
    let mut store = core.store.lock().await;
    let objects = store.objects().clone();
    let mut file_events = file_changed_events(
        &objects,
        &svc,
        task_id,
        ToolCallId::from_bytes(command_id),
        &changes,
        &pre_bytes,
        current_revision,
        "user_direct_edit:",
        "user_direct_edit",
    );
    for e in &mut file_events {
        e.actor = actor.clone();
    }
    drop(svc);
    let task_event = typed(
        "UserPatchApplied",
        &TaskEvent::UserPatchApplied {
            path: resolved.relative.clone(),
            before_hash: before_hash.clone(),
            after_hash: after_hash.clone(),
            workspace_revision,
            previous_revision: current_revision,
            provenance: "user_direct_edit".to_owned(),
            source,
            match_tier: match_tier.to_owned(),
            command_id: hex::encode(command_id),
        },
        actor,
    );
    let base = |aggregate_type: AggregateType, aggregate_id: [u8; 16], events| AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type,
        aggregate_id,
        expected_sequence: None,
        events,
    };
    let outcome = store
        .execute_command_all(
            record,
            vec![
                base(
                    AggregateType::Workspace,
                    workspace_aggregate_id(&root),
                    file_events,
                ),
                base(AggregateType::Task, *task_id.as_bytes(), vec![task_event]),
            ],
        )
        .map_err(|e| (crate::server::error_code(&e).to_owned(), e.to_string()))?;
    let events = match outcome {
        CommandOutcome::Applied(e) | CommandOutcome::Replayed(e) => e,
    };
    let offset = events.last().map_or(0, |e| e.offset);
    core.last_offset.send_replace(offset);
    let _ = read_workspace_file;
    Ok(wire::UserPatchAppliedAck {
        workspace_revision,
        previous_revision: current_revision,
        file_revision: after_hash,
        before_hash,
        match_tier: match_tier.to_owned(),
        offset,
        replayed: false,
    })
}
