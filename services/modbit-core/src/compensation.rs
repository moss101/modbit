//! Reversibility and compensation (REQ-EV-0066; docs/23 "Reversibility").
//!
//! Every effect receipt says how far its effect can be taken back, from the
//! tool's own declaration: a workspace write is `REVERSIBLE` (its typed undo
//! restores the exact content), a protected surface `PARTIALLY_REVERSIBLE`, an
//! external effect `COMPENSATABLE` when its tool declares a counteracting tool
//! and `IRREVERSIBLE` when it does not, a disclosure or a destruction
//! `IRREVERSIBLE`. An external effect is never offered an undo.
//!
//! A compensation is not an undo: it is a new effect — here, the declared
//! tool, through the kernel, under an approval like any external effect —
//! whose receipt names the effect it compensates. The original receipt is
//! never edited, and the chain holds both.

use modbit_domain::event::Actor;
use modbit_domain::toolcall::{Reversibility, ToolCall, ToolCallState};
use modbit_domain::{EffectId, TaskId, ToolCallId};
use modbit_protocol::v1 as wire;
use sha2::Digest;

use crate::server::Core;

fn refuse(code: &str, detail: impl Into<String>) -> (String, String) {
    (code.to_owned(), detail.into())
}

/// A recorded call's reversibility and the tool that compensates it.
fn class_of(core: &Core, call: &ToolCall) -> (Reversibility, Option<String>) {
    match core.tools.runtime.registry().get(&call.tool_name) {
        Some(t) => (t.spec().reversibility(), t.spec().compensation.clone()),
        None => (Reversibility::of(call.effect_class, false), None),
    }
}

/// Why `UndoToolCall` refuses a call, when its effect is not reversible:
/// the class and what, if anything, counteracts it. `None` for a reversible
/// or partially reversible effect, whose content the typed undo restores.
pub(crate) fn not_undoable(core: &Core, call: &ToolCall) -> Option<(&'static str, String)> {
    let (class, compensation) = class_of(core, call);
    match class {
        Reversibility::Reversible | Reversibility::PartiallyReversible => None,
        Reversibility::Compensatable => Some((
            "NOT_UNDOABLE",
            format!(
                "`{}` is COMPENSATABLE, not undoable: its effect is outside the workspace; `{}` counteracts it as a new effect with a receipt of its own (CompensateEffect)",
                call.tool_name,
                compensation.unwrap_or_default()
            ),
        )),
        Reversibility::Irreversible => Some((
            "NOT_UNDOABLE",
            format!(
                "`{}` is IRREVERSIBLE: its effect can be neither undone nor counteracted",
                call.tool_name
            ),
        )),
    }
}

/// The compensating call's id: one per compensated effect, so a retry after
/// the person decides finds the same call and the same approval.
fn compensating_call_id(original: EffectId) -> ToolCallId {
    let h = sha2::Sha256::digest(format!("compensate:{original}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    ToolCallId::from_bytes(b)
}

/// `CompensateEffect`.
#[allow(clippy::too_many_lines)]
pub(crate) async fn compensate(
    core: &Core,
    task_id: TaskId,
    call_id: ToolCallId,
    actor: Actor,
    lease_generation: Option<u64>,
) -> Result<wire::CompensationAck, (String, String)> {
    let (task, session, call, receipts) = {
        let store = core.store.lock().await;
        let task = store
            .task(&task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_TASK", task_id.to_string()))?;
        let session = store
            .session(&task.session_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .ok_or_else(|| refuse("UNKNOWN_SESSION", task.session_id.to_string()))?;
        let call = store
            .tool_call(&call_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .filter(|c| c.task_id == task_id)
            .ok_or_else(|| refuse("UNKNOWN_TOOL_CALL", call_id.to_string()))?;
        let receipts = store
            .receipts(Some(&task_id))
            .map_err(|e| refuse("STORE", e.to_string()))?;
        (task, session, call, receipts)
    };
    let (class, compensation) = class_of(core, &call);
    let Some(compensating_tool) = compensation.filter(|_| class == Reversibility::Compensatable)
    else {
        return Err(refuse(
            "NOT_COMPENSATABLE",
            format!(
                "`{}` is {}: its tool declares no compensation",
                call.tool_name,
                class.label()
            ),
        ));
    };
    if call.state != ToolCallState::Succeeded {
        return Err(refuse(
            "NOT_COMPENSATABLE",
            format!(
                "the call is {:?}; only an effect that happened is compensated",
                call.state
            ),
        ));
    }
    let Some(original) = receipts
        .iter()
        .find(|r| r.tool_call_id == call_id && r.compensates.is_none())
    else {
        return Err(refuse(
            "NOT_COMPENSATABLE",
            "the call has no effect receipt to compensate",
        ));
    };
    let original_effect = original.effect_id;
    let comp_call_id = compensating_call_id(original_effect);
    // The original result names what to counteract.
    let output = {
        let store = core.store.lock().await;
        call.result_ref
            .as_ref()
            .and_then(|r| store.objects().get(r).ok())
            .and_then(|b| serde_json::from_slice::<modbit_tools::ToolCallResult>(&b).ok())
            .map(|r| r.structured_output)
            .unwrap_or_default()
    };
    let key = format!("compensate-{original_effect}");
    let Some((tool, args)) = modbit_tools::forge::compensation_call(&call.tool_name, &output, &key)
        .filter(|(t, _)| *t == compensating_tool)
    else {
        return Err(refuse(
            "NOT_COMPENSATABLE",
            "the original result does not name what to counteract",
        ));
    };
    let arguments_json = args.to_string();
    let ack = |status: &str,
               approval_id: String,
               receipts: Vec<String>,
               replayed: bool,
               detail: String| wire::CompensationAck {
        status: status.to_owned(),
        compensating_tool: tool.clone(),
        compensating_tool_call_id: Some(crate::server::wire_id(comp_call_id.as_bytes())),
        approval_id,
        intent_hash: modbit_tools::arguments_hash(&arguments_json).unwrap_or_default(),
        original_effect_id: Some(crate::server::wire_id(original_effect.as_bytes())),
        effect_receipt_ids: receipts,
        replayed,
        detail,
    };
    // Compensated before: the receipt that names this effect answers.
    if let Some(done) = receipts
        .iter()
        .find(|r| r.compensates == Some(original_effect) && r.status == "SUCCESS")
    {
        return Ok(ack(
            "COMPENSATED",
            done.approval_id.map(|a| a.to_string()).unwrap_or_default(),
            vec![done.effect_id.to_string()],
            true,
            format!("already compensated by effect {}", done.effect_id),
        ));
    }
    let (existing, lease, approval) = {
        let store = core.store.lock().await;
        let existing = store
            .tool_call(&comp_call_id)
            .map_err(|e| refuse("STORE", e.to_string()))?;
        let lease = store
            .leases_for_task(&task.task_id)
            .map_err(|e| refuse("STORE", e.to_string()))?
            .into_iter()
            .next();
        let approval = store
            .approval_for_call(&comp_call_id)
            .map_err(|e| refuse("STORE", e.to_string()))?;
        (existing, lease, approval)
    };
    if let Some(c) = &existing
        && matches!(
            c.state,
            ToolCallState::Failed | ToolCallState::Cancelled | ToolCallState::UnknownOutcome
        )
    {
        return Ok(ack(
            "DENIED",
            c.approval_id.map(|a| a.to_string()).unwrap_or_default(),
            vec![],
            true,
            c.policy_decision.clone().unwrap_or_default(),
        ));
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
                tool_call_id: comp_call_id,
                tool_name: &tool,
                arguments_json: &arguments_json,
                output_budget_bytes: 65_536,
                actor,
                lease,
                approval,
                emergency_stopped: session.emergency_stopped_at.is_some(),
                existing,
                run_id: None,
                turn_id: None,
                call_id: None,
                lease_generation,
                projection: None,
                cancel: None,
                compensates: Some(original_effect),
            },
        )
        .await
        .map_err(|e| refuse("TOOL_HOST", e.to_string()))?;
    let offset = core.store.lock().await.last_offset().unwrap_or(0);
    core.last_offset.send_replace(offset);
    let r = &done.result;
    let approval_hex = done.approval_id.map(|a| a.to_string()).unwrap_or_default();
    Ok(match format!("{:?}", r.status).to_uppercase().as_str() {
        "SUCCESS" => ack(
            "COMPENSATED",
            approval_hex,
            r.effect_receipt_ids.clone(),
            false,
            format!("compensated effect {original_effect} with `{tool}`"),
        ),
        "APPROVALPENDING" => ack(
            "APPROVAL_PENDING",
            approval_hex,
            vec![],
            false,
            "decide the approval, then call again".into(),
        ),
        _ => ack(
            "DENIED",
            approval_hex,
            vec![],
            false,
            format!(
                "{}: {}",
                r.error_code.clone().unwrap_or_default(),
                r.error_message.clone().unwrap_or_default()
            ),
        ),
    })
}
