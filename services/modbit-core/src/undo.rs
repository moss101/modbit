//! Typed undo (REQ-EV-0064/0065): the inverse of one tool call's
//! `FileChanged` events, applied only when every path still carries its
//! post-edit content. Never a blind checkout.

use std::collections::HashMap;
use std::sync::Arc;

use modbit_domain::workspace::{UndoPlan, UndoStep, WorkspaceEvent};
use modbit_domain::{SessionId, TaskId, TenantId, ToolCallId};
use modbit_protocol::v1 as wire;
use modbit_workspace::{ChangeOp, ChangeOpKind, WritePrecondition, content_hash};

use crate::server::Core;
use crate::tools::{
    append_file_events, file_changed_events, read_workspace_file, workspace_aggregate_id,
    workspace_changes,
};

/// Build the plan from the call's `FileChanged` events (latest change first).
pub(crate) async fn plan(
    core: &Arc<Core>,
    root: &str,
    tool_call_id: ToolCallId,
) -> anyhow::Result<UndoPlan> {
    let store = core.store.lock().await;
    let events = store.read_aggregate(&workspace_aggregate_id(root), 0, 100_000)?;
    let mut steps = Vec::new();
    for e in &events {
        let Ok(WorkspaceEvent::FileChanged {
            tool_call_id: call,
            path,
            op,
            before_hash,
            after_hash,
            before_ref,
            ..
        }) = serde_json::from_value::<WorkspaceEvent>(store.payload(&e.envelope)?)
        else {
            continue;
        };
        if call != tool_call_id || op.starts_with("undo:") {
            continue;
        }
        let step = match (&before_hash, &after_hash) {
            (None, Some(after)) => UndoStep {
                path,
                action: "delete".into(),
                expected_content_hash: Some(after.clone()),
                expect_absent: false,
                restore_ref: None,
                resulting_hash: None,
            },
            (Some(before), None) => UndoStep {
                path,
                action: "restore".into(),
                expected_content_hash: None,
                expect_absent: true,
                restore_ref: before_ref,
                resulting_hash: Some(before.clone()),
            },
            (Some(before), Some(after)) => UndoStep {
                path,
                action: "replace".into(),
                expected_content_hash: Some(after.clone()),
                expect_absent: false,
                restore_ref: before_ref,
                resulting_hash: Some(before.clone()),
            },
            (None, None) => continue,
        };
        steps.push(step);
    }
    steps.reverse();
    Ok(UndoPlan {
        tool_call_id,
        steps,
    })
}

/// Apply the plan: every precondition is checked before anything is written;
/// a path whose content moved on since the change refuses the whole revert.
pub(crate) async fn apply(
    core: &Arc<Core>,
    tenant_id: TenantId,
    session_id: SessionId,
    task_id: TaskId,
    root: &str,
    plan: &UndoPlan,
) -> anyhow::Result<(bool, Vec<wire::UndoRefusal>, u64)> {
    let (ws, _) = core.tools.workspace(root).await?;
    let mut ws = ws.lock().await;
    let mut refusals = Vec::new();
    for s in &plan.steps {
        let current = read_workspace_file(&ws, &s.path);
        match (&s.expected_content_hash, s.expect_absent, &current) {
            (Some(exp), _, Some(bytes)) if content_hash(bytes) != *exp => {
                refusals.push(wire::UndoRefusal {
                    path: s.path.clone(),
                    code: "USER_EDITED".into(),
                    detail: format!(
                        "expected post-edit hash {exp}, current {}",
                        content_hash(bytes)
                    ),
                });
            }
            (Some(_), _, None) => refusals.push(wire::UndoRefusal {
                path: s.path.clone(),
                code: "PATH_MISSING".into(),
                detail: "the changed file no longer exists".into(),
            }),
            (None, true, Some(_)) => refusals.push(wire::UndoRefusal {
                path: s.path.clone(),
                code: "PATH_PRESENT".into(),
                detail: "the deleted path was re-created since".into(),
            }),
            _ => {}
        }
    }
    if !refusals.is_empty() {
        return Ok((false, refusals, ws.revision().number));
    }
    let objects = core.store.lock().await.objects().clone();
    let mut ops = Vec::new();
    let mut pre_bytes: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    for s in &plan.steps {
        pre_bytes.insert(s.path.clone(), read_workspace_file(&ws, &s.path));
        let restore = match &s.restore_ref {
            Some(r) => Some(objects.get(r)?),
            None => None,
        };
        let pre = WritePrecondition {
            expected_content_hash: s.expected_content_hash.clone(),
            expected_workspace_revision: None,
            expect_absent: s.expect_absent,
        };
        let kind = match (s.action.as_str(), restore) {
            ("delete", _) => ChangeOpKind::Delete,
            ("restore", Some(b)) => ChangeOpKind::Create(b),
            ("replace", Some(b)) => ChangeOpKind::Replace(b),
            (a, _) => anyhow::bail!("undo step {a} on {} has no content to restore", s.path),
        };
        ops.push(ChangeOp {
            path: s.path.clone(),
            kind,
            pre,
        });
    }
    let pre_revision = ws.revision().number;
    match ws.apply_transaction(&ops) {
        Ok(changes) => {
            let value = serde_json::json!({ "changes": changes });
            let events = file_changed_events(
                &objects,
                &ws,
                task_id,
                plan.tool_call_id,
                &workspace_changes(&value),
                &pre_bytes,
                pre_revision,
                "undo:",
            );
            let mut st = core.store.lock().await;
            append_file_events(&mut st, tenant_id, session_id, task_id, root, events)?;
            Ok((true, vec![], ws.revision().number))
        }
        Err(e) => Ok((
            false,
            vec![wire::UndoRefusal {
                path: String::new(),
                code: "STEP_FAILED".into(),
                detail: e.to_string(),
            }],
            ws.revision().number,
        )),
    }
}

/// Wire view of a plan.
pub(crate) fn view(
    plan: &UndoPlan,
    applied: bool,
    refusals: Vec<wire::UndoRefusal>,
    workspace_revision: u64,
) -> wire::UndoPlanView {
    wire::UndoPlanView {
        tool_call_id: Some(wire::Id {
            value: plan.tool_call_id.as_bytes().to_vec(),
        }),
        steps: plan
            .steps
            .iter()
            .map(|s| wire::UndoStepView {
                path: s.path.clone(),
                action: s.action.clone(),
                expected_content_hash: s.expected_content_hash.clone().unwrap_or_default(),
                expect_absent: s.expect_absent,
                restore_ref: s.restore_ref.clone().unwrap_or_default(),
                resulting_hash: s.resulting_hash.clone().unwrap_or_default(),
            })
            .collect(),
        applied,
        refusals,
        workspace_revision,
    }
}
