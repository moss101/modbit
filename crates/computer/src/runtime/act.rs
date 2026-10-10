//! Inputs (PX-069, PX-070): the checks that come before one, the intent an
//! approval binds, the call to the actuator, and what its outcome means.
//!
//! The rule that matters most is CUC-D02/D03: an input is never retried
//! after it may have been delivered, and an input whose outcome is unknown
//! (a timeout or a lost connection after the write) latches the task - every
//! further input is refused `OUTCOME_UNKNOWN` until a new control session
//! observes the real state.

use std::sync::Arc;
use std::time::Instant;

use modbit_domain::task::TaskEvent;
use modbit_protocol::v1 as wire;
use serde_json::{Value, json};

use super::observe::Use;
use super::{
    Answer, CallCtx, ComputerRuntime, Failure, FrameHandle, call_refusal, remote_refusal,
    window_json,
};
use crate::actuator::{Call, CallError, Reply};
use crate::model::{AppIdentity, Node, Scope, Window, app_from_wire, identity_to_wire};
use crate::ops::Act;
use crate::policy;
use crate::taxonomy::{Code, Refusal};
use crate::tree;

/// What the checks learned about an input's target.
struct Plan {
    control_id: String,
    app: AppIdentity,
    window: Window,
    scope: Scope,
    snapshot_id: String,
    element: Option<Node>,
    frame: Option<FrameHandle>,
    gate: Arc<tokio::sync::Mutex<()>>,
}

/// Codes an actuator gives for a refusal made before it injected anything.
fn pre_delivery(code: &str) -> bool {
    matches!(
        code,
        "TARGET_STALE"
            | "TARGET_OCCLUDED"
            | "WINDOW_UNVERIFIABLE"
            | "ACCESSIBILITY_UNAVAILABLE"
            | "HUMAN_ACTIVE"
            | "MODAL_BLOCKING"
            | "TARGET_NOT_EDITABLE"
            | "ACTION_UNSAFE"
            | "PERMISSION_REQUIRED"
            | "SECURE_DESKTOP"
            | "TARGET_ELEVATED"
            | "SESSION_BUSY"
            | "USER_ABORTED"
            | "UNSUPPORTED_REQUEST"
            | "INVALID_ARGUMENTS"
    )
}

fn changed_summary(changes: &[wire::ElementChange]) -> Vec<Value> {
    changes
        .iter()
        .take(30)
        .map(|c| json!({"element": c.element_id, "kind": c.kind, "before": c.before, "after": c.after}))
        .collect()
}

impl ComputerRuntime {
    /// Every check that can be made before an input is sent, in order.
    async fn plan(&self, ctx: &CallCtx, act: &Act) -> Result<Plan, Refusal> {
        // A stop is final and an unknown outcome refuses every input whatever
        // state the actuator is in; only then does the actuator matter.
        self.check_aborted(ctx)?;
        if let Some(l) = self.lock().latches.get(&ctx.task_id) {
            return Err(Self::latched_refusal(l));
        }
        self.ready_actuator().await?;
        if ctx.policy.act_denied || ctx.policy.observe_denied {
            return Err(Refusal::new(
                Code::ActionUnsafe,
                "administrative policy forbids computer control",
            ));
        }
        let found = self.session_for(ctx, Use::Input).await?;
        let (app, window, scope, snapshot, frame, gate) = {
            let st = self.lock();
            let Some(s) = st.sessions.get(&found.control_id) else {
                return Err(Refusal::new(Code::SessionRequired, "the session ended"));
            };
            (
                s.app.clone(),
                s.last_window.clone(),
                s.scope,
                s.snapshot.clone(),
                s.frame.clone(),
                Arc::clone(&s.gate),
            )
        };
        if ctx.stop_on_blockers {
            let blockers = self
                .lock()
                .sessions
                .get(&found.control_id)
                .map(|s| s.blockers.clone())
                .unwrap_or_default();
            if !blockers.is_empty() {
                return Err(Refusal::new(
                    Code::ActionUnsafe,
                    format!(
                        "the window shows a blocker that is the person's to deal with ({}): report what you observed, what blocks you and the best next step; do not enter credentials or work around it",
                        blockers.join(", ")
                    ),
                )
                .with_facts(json!({"blockers": blockers})));
            }
        }
        ctx.policy
            .admits_scope(scope)
            .map_err(|m| Refusal::new(Code::ActionUnsafe, m))?;
        ctx.policy
            .admits_app(&app)
            .map_err(|m| Refusal::new(Code::WindowUnverifiable, m))?;
        if let Some(why) = policy::non_drivable(&app, self.shared.cfg.own_pid) {
            return Err(Refusal::new(
                Code::WindowUnverifiable,
                format!("`{}` is never driven ({why})", app.bundle_id),
            ));
        }
        if act.needs_screen() && scope != Scope::Screen {
            return Err(Refusal::new(
                Code::UnsupportedRequest,
                "moving the pointer needs a SCREEN session (computer.start_screen): APP scope never touches the real cursor",
            ));
        }
        // The window an input lands in: the one the tree was read from, the
        // one the screenshot was taken of, else the session's.
        let window = match act {
            Act::Press { .. } | Act::SetValue { .. } | Act::Type { .. } => {
                snapshot.as_ref().map_or(window, |s| s.window.clone())
            }
            Act::Click { .. } | Act::Move { .. } | Act::Drag { .. } | Act::Scroll { .. } => {
                match &frame {
                    Some(f) if f.window_id != window.id => Window {
                        id: f.window_id.clone(),
                        ..window
                    },
                    _ => window,
                }
            }
            Act::Key { .. } => snapshot.as_ref().map_or(window, |s| s.window.clone()),
        };
        let mut plan = Plan {
            control_id: found.control_id,
            app,
            window,
            scope,
            snapshot_id: String::new(),
            element: None,
            frame: None,
            gate,
        };
        match act {
            Act::Press { element, action } => {
                let (snap, node) = self.element_of(&snapshot, &element.snapshot, &element.id)?;
                if !node.enabled {
                    return Err(Refusal::new(
                        Code::TargetNotEditable,
                        format!("`{}` is disabled", element.text()),
                    ));
                }
                if !node.actions.iter().any(|a| a == action) {
                    return Err(Refusal::new(
                        Code::UnsupportedRequest,
                        format!(
                            "`{}` offers {} and not `{action}`",
                            element.text(),
                            if node.actions.is_empty() {
                                "no actions".to_owned()
                            } else {
                                format!("the actions {}", node.actions.join(", "))
                            }
                        ),
                    )
                    .with_facts(json!({"available": node.actions})));
                }
                plan.snapshot_id = snap;
                plan.element = Some(node);
            }
            Act::SetValue { element, .. } => {
                let (snap, node) = self.element_of(&snapshot, &element.snapshot, &element.id)?;
                if tree::is_secure(&node) {
                    return Err(Refusal::new(
                        Code::ActionUnsafe,
                        format!(
                            "`{}` is a secure field: it is never set from model text; a credential is entered only through the credential broker's handle, bound to the application and the field",
                            element.text()
                        ),
                    ));
                }
                if !node.settable || !node.enabled {
                    return Err(Refusal::new(
                        Code::TargetNotEditable,
                        format!("`{}` does not take a value", element.text()),
                    ));
                }
                plan.snapshot_id = snap;
                plan.element = Some(node);
            }
            Act::Click { frame: token, .. }
            | Act::Move { frame: token, .. }
            | Act::Drag { frame: token, .. }
            | Act::Scroll { frame: token, .. } => {
                let Some(f) = frame else {
                    return Err(Refusal::new(
                        Code::ScreenshotRequired,
                        "no screenshot has been taken since the last input",
                    ));
                };
                if &f.token != token {
                    return Err(Refusal::new(
                        Code::TargetStale,
                        format!("frame `{token}` is not the current one (`{}`)", f.token),
                    )
                    .with_facts(json!({"current_frame": f.token})));
                }
                plan.frame = Some(f);
            }
            Act::Type { .. } => {
                // Typed input goes to a verified focus: the last tree names
                // one, and it takes text.
                let focus = snapshot.as_ref().and_then(|s| {
                    s.nodes
                        .iter()
                        .find(|n| n.focused)
                        .cloned()
                        .map(|n| (s.id.clone(), n))
                });
                let Some((snap, node)) = focus else {
                    return Err(Refusal::new(
                        Code::TargetNotEditable,
                        "no verified keyboard focus: read the state (computer.state), focus a text field, read it again, then type; or set a field's value directly",
                    ));
                };
                if tree::is_secure(&node) {
                    return Err(Refusal::new(
                        Code::ActionUnsafe,
                        "the focused element is a secure field: text from the model is never typed into it",
                    ));
                }
                if !node.settable || !node.enabled {
                    return Err(Refusal::new(
                        Code::TargetNotEditable,
                        format!("the focused element ({}) does not take text", node.role),
                    ));
                }
                plan.snapshot_id = snap;
                plan.element = Some(node);
            }
            Act::Key { .. } => {}
        }
        Ok(plan)
    }

    fn element_of(
        &self,
        snapshot: &Option<super::Snapshot>,
        want_snapshot: &str,
        id: &str,
    ) -> Result<(String, Node), Refusal> {
        let Some(snap) = snapshot else {
            return Err(Refusal::new(
                Code::TargetStale,
                "no tree has been read in this session (or it was invalidated): read the state first",
            ));
        };
        if snap.id != want_snapshot {
            return Err(Refusal::new(
                Code::TargetStale,
                format!(
                    "snapshot `{want_snapshot}` is not the current one (`{}`)",
                    snap.id
                ),
            )
            .with_facts(json!({"current_snapshot": snap.id})));
        }
        match snap.nodes.iter().find(|n| n.id == id) {
            Some(n) => Ok((snap.id.clone(), n.clone())),
            None => Err(Refusal::new(
                Code::TargetStale,
                format!("snapshot `{}` has no element `{id}`", snap.id),
            )),
        }
    }

    /// The intent an approval of an input binds (CUC-C01, QUAL-PX-070): the
    /// application's identity as the actuator vouches for it now, the window,
    /// the snapshot or the frame, the element, the action and its arguments
    /// in full, the scope.
    pub(super) async fn prepare_act(
        &self,
        ctx: &CallCtx,
        tool: &str,
        act: &Act,
    ) -> Result<Value, Refusal> {
        let plan = self.plan(ctx, act).await?;
        let a = self.ready_actuator().await?;
        // Re-verify the identity now: the same four facts the approval will
        // name, from the actuator, never from a title or from memory.
        let reply = a
            .call(
                Call::ResolveApplication(wire::ResolveApplicationRequest {
                    bundle_id: plan.app.bundle_id.clone(),
                    name: String::new(),
                    pid: plan.app.pid,
                }),
                self.shared.cfg.observe_deadline,
            )
            .await
            .map_err(|e| call_refusal(&e, true))?;
        let Reply::ResolveApplication(r) = reply else {
            return Err(Refusal::new(
                Code::WindowUnverifiable,
                "the actuator could not resolve the application",
            ));
        };
        let now = r.application.as_ref().map(app_from_wire);
        let Some(now) = now.filter(|n| n.identity.same_as(&plan.app)) else {
            return Err(Refusal::new(
                Code::WindowUnverifiable,
                "the application running as that process is no longer the one the session was approved for (its identity changed)",
            ));
        };
        let window = now
            .windows
            .iter()
            .find(|w| w.id == plan.window.id)
            .cloned()
            .ok_or_else(|| Refusal::new(Code::WindowUnverifiable, "the window is gone"))?;
        if window.modal {
            return Err(Refusal::new(
                Code::ModalBlocking,
                "a modal dialog blocks the window",
            ));
        }
        let target = match (&plan.element, &plan.frame) {
            (Some(e), _) => json!({
                "kind": "element",
                "snapshot": plan.snapshot_id,
                "id": e.id,
                "role": e.role,
                "name": (ctx.redact)(&e.name),
            }),
            (None, Some(f)) => {
                json!({"kind": "coordinate", "frame": f.token, "window_version": f.window_version})
            }
            (None, None) => json!({"kind": "focus_or_keys"}),
        };
        Ok(json!({
            "computer": {
                "tool": tool,
                "kind": "action",
                "control_id": plan.control_id,
                "scope": plan.scope.name(),
                "application": plan.app.binding(),
                "application_name": (ctx.redact)(&plan.app.name),
                "window": {"id": window.id, "title": (ctx.redact)(&window.title), "version": window.version},
                "target": target,
                "action": act.kind(),
                "modality": act.modality(),
                "arguments": act.arguments(),
                "reversible": false,
            }
        }))
    }

    /// Perform one approved input.
    pub(super) async fn act(&self, ctx: &CallCtx, tool: &str, act: Act) -> Result<Answer, Failure> {
        let first = self.plan(ctx, &act).await?;
        // One call at a time per session (CUC-D02): wait for the one in
        // front, then decide again - it may have latched the task.
        let gate = Arc::clone(&first.gate);
        let _serial = gate.lock().await;
        let plan = self.plan(ctx, &act).await?;
        let a = self.ready_actuator().await?;
        let request = wire::PerformRequest {
            control_id: plan.control_id.clone(),
            application: Some(identity_to_wire(&plan.app)),
            window_id: plan.window.id.clone(),
            snapshot_id: plan.snapshot_id.clone(),
            frame_token: plan
                .frame
                .as_ref()
                .map(|f| f.token.clone())
                .unwrap_or_default(),
            window_version: plan
                .frame
                .as_ref()
                .map_or(plan.window.version, |f| f.window_version),
            scope: plan.scope.to_wire() as i32,
            idempotency_key: ctx.tool_call_id.clone(),
            action: Some(match &act {
                Act::Press { element, action } => {
                    wire::perform_request::Action::Press(wire::PressAction {
                        element_id: element.id.clone(),
                        action: action.clone(),
                    })
                }
                Act::SetValue { element, value } => {
                    wire::perform_request::Action::SetValue(wire::SetValueAction {
                        element_id: element.id.clone(),
                        value: value.clone(),
                    })
                }
                Act::Click {
                    x,
                    y,
                    button,
                    count,
                    ..
                } => wire::perform_request::Action::Click(wire::ClickAction {
                    at: Some(wire::Point { x: *x, y: *y }),
                    button: button.clone(),
                    count: *count,
                }),
                Act::Move { x, y, .. } => wire::perform_request::Action::Move(wire::MoveAction {
                    to: Some(wire::Point { x: *x, y: *y }),
                }),
                Act::Drag { from, to, .. } => {
                    wire::perform_request::Action::Drag(wire::DragAction {
                        from: Some(wire::Point {
                            x: from.0,
                            y: from.1,
                        }),
                        to: Some(wire::Point { x: to.0, y: to.1 }),
                    })
                }
                Act::Type { text, then } => wire::perform_request::Action::Type(wire::TypeAction {
                    text: text.clone(),
                    then: then.clone().unwrap_or_default(),
                }),
                Act::Key { keys, .. } => {
                    wire::perform_request::Action::Key(wire::KeyAction { keys: keys.clone() })
                }
                Act::Scroll { x, y, dx, dy, .. } => {
                    wire::perform_request::Action::Scroll(wire::ScrollAction {
                        at: Some(wire::Point { x: *x, y: *y }),
                        dx: *dx,
                        dy: *dy,
                        element_id: String::new(),
                    })
                }
            }),
        };
        let started = Instant::now();
        if let Some(s) = self.lock().sessions.get_mut(&plan.control_id) {
            s.in_flight = true;
        }
        let result = a
            .call(Call::Perform(request), self.shared.cfg.action_deadline)
            .await;
        if let Some(s) = self.lock().sessions.get_mut(&plan.control_id) {
            s.in_flight = false;
        }
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let kind = act.kind();
        match result {
            Ok(Reply::Perform(p)) => {
                self.performed(ctx, tool, &act, &plan, &p, duration_ms, &a)
                    .await
            }
            Ok(_) => Err(self
                .unknown_input(
                    ctx,
                    tool,
                    kind,
                    &plan.control_id,
                    "the actuator answered something other than a Perform result",
                    duration_ms,
                    false,
                    &a,
                )
                .await),
            Err(CallError::Remote(e)) => {
                let delivered = match wire::Delivery::try_from(e.delivery)
                    .unwrap_or(wire::Delivery::Unspecified)
                {
                    wire::Delivery::NotDelivered => false,
                    wire::Delivery::Delivered | wire::Delivery::Unknown => true,
                    wire::Delivery::Unspecified => !pre_delivery(&e.code),
                };
                if delivered {
                    Err(self
                        .unknown_input(
                            ctx,
                            tool,
                            kind,
                            &plan.control_id,
                            &format!(
                                "the actuator reported {} after delivering the input: {}",
                                e.code, e.message
                            ),
                            duration_ms,
                            false,
                            &a,
                        )
                        .await)
                } else {
                    let r = remote_refusal(&e, Code::InputFailed);
                    if r.code == Code::HumanActive {
                        let until = super::now_ms()
                            + i64::try_from(self.shared.cfg.human_cooldown.as_millis())
                                .unwrap_or(0);
                        if let Some(s) = self.lock().sessions.get_mut(&plan.control_id) {
                            s.controller = super::Controller::Parked {
                                until_ms: u64::try_from(until).unwrap_or(0),
                            };
                        }
                    }
                    self.failed_input(ctx, &plan.control_id, &act, &r, duration_ms);
                    Err(r.into())
                }
            }
            Err(CallError::NotDelivered(m)) => {
                let r = Refusal::new(
                    Code::ActuatorUnavailable,
                    format!(
                        "{m}; the input was not sent and the control session ended with the actuator"
                    ),
                );
                self.failed_input(ctx, &plan.control_id, &act, &r, duration_ms);
                Err(r.into())
            }
            Err(CallError::TimedOut) => Err(self
                .unknown_input(
                    ctx,
                    tool,
                    kind,
                    &plan.control_id,
                    "TIMEOUT: the actuator did not answer within the deadline",
                    duration_ms,
                    true,
                    &a,
                )
                .await),
            Err(CallError::Lost) => Err(self
                .unknown_input(
                    ctx,
                    tool,
                    kind,
                    &plan.control_id,
                    "ACTUATOR_LOST: the actuator's connection ended after the input was sent",
                    duration_ms,
                    false,
                    &a,
                )
                .await),
            Err(CallError::Protocol(m)) => Err(self
                .unknown_input(
                    ctx,
                    tool,
                    kind,
                    &plan.control_id,
                    &format!("PROTOCOL: {m}"),
                    duration_ms,
                    false,
                    &a,
                )
                .await),
        }
    }

    fn failed_input(
        &self,
        ctx: &CallCtx,
        control_id: &str,
        act: &Act,
        r: &Refusal,
        duration_ms: u64,
    ) {
        if let Some(s) = self.lock().sessions.get_mut(control_id) {
            s.counts.failed += 1;
        }
        self.emit(
            ctx.task_id,
            ctx.session_id,
            &ctx.run_id,
            TaskEvent::ComputerActionPerformed {
                control_id: control_id.to_owned(),
                tool_call_id: ctx.tool_call_id.clone(),
                approval_id: ctx.approval_id.clone(),
                intent_hash: ctx.intent_hash.clone(),
                kind: act.kind().to_owned(),
                modality: act.modality().to_owned(),
                modality_reason: String::new(),
                outcome: "FAILED".into(),
                code: r.code.as_str().to_owned(),
                duration_ms,
                verified: None,
            },
        );
    }

    /// An input of unknown outcome: latch the task, stop what may still be
    /// running, close the session, record it (CUC-D03).
    #[allow(clippy::too_many_arguments)]
    async fn unknown_input(
        &self,
        ctx: &CallCtx,
        tool: &str,
        kind: &str,
        control_id: &str,
        reason: &str,
        duration_ms: u64,
        stop_actuator: bool,
        a: &Arc<dyn crate::actuator::Actuator>,
    ) -> Failure {
        let latch = self.latch(ctx, control_id, tool, kind, reason);
        self.emit(
            ctx.task_id,
            ctx.session_id,
            &ctx.run_id,
            TaskEvent::ComputerActionPerformed {
                control_id: control_id.to_owned(),
                tool_call_id: ctx.tool_call_id.clone(),
                approval_id: ctx.approval_id.clone(),
                intent_hash: ctx.intent_hash.clone(),
                kind: kind.to_owned(),
                modality: String::new(),
                modality_reason: String::new(),
                outcome: "UNKNOWN".into(),
                code: Code::OutcomeUnknown.as_str().to_owned(),
                duration_ms,
                verified: None,
            },
        );
        if stop_actuator {
            // The actuator is alive and may still be injecting: stop it.
            let _ = a
                .call(
                    Call::Stop(wire::StopRequest {
                        reason: "an input did not finish within its deadline".into(),
                        source: "core-watchdog".into(),
                    }),
                    std::time::Duration::from_millis(500),
                )
                .await;
        }
        self.close_session(control_id, "LATCHED", stop_actuator)
            .await;
        Failure::Unknown {
            reason: reason.to_owned(),
            output: json!({
                "code": Code::OutcomeUnknown.as_str(),
                "escalation": Code::OutcomeUnknown.escalation().label(),
                "latched": true,
                "tool": latch.tool,
                "action": latch.action,
                "reason": latch.reason,
                "recovery": Code::OutcomeUnknown.recovery(),
                "control_session": "closed",
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn performed(
        &self,
        ctx: &CallCtx,
        _tool: &str,
        act: &Act,
        plan: &Plan,
        p: &wire::PerformResponse,
        duration_ms: u64,
        a: &Arc<dyn crate::actuator::Actuator>,
    ) -> Result<Answer, Failure> {
        let delivery = wire::Delivery::try_from(p.delivery).unwrap_or(wire::Delivery::Unspecified);
        if delivery != wire::Delivery::Delivered {
            // A Perform answer that does not say DELIVERED is not a success.
            return Err(self
                .unknown_input(
                    ctx,
                    _tool,
                    act.kind(),
                    &plan.control_id,
                    "the actuator's answer does not say the input was delivered",
                    duration_ms,
                    false,
                    a,
                )
                .await);
        }
        let verified = match act {
            Act::SetValue { element, value } => Some(
                p.changes
                    .iter()
                    .any(|c| c.element_id == element.id && c.kind == "value" && c.after == *value),
            ),
            _ => None,
        };
        let modality = if p.modality.is_empty() {
            act.modality().to_owned()
        } else {
            p.modality.clone()
        };
        let win = p.window.as_ref().map(crate::model::window_from_wire);
        {
            let mut st = self.lock();
            if let Some(s) = st.sessions.get_mut(&plan.control_id) {
                *s.counts
                    .actions_by_kind
                    .entry(act.kind().to_owned())
                    .or_insert(0) += 1;
                if !ctx.approval_id.is_empty() && !s.counts.approvals.contains(&ctx.approval_id) {
                    s.counts.approvals.push(ctx.approval_id.clone());
                }
                s.last_activity = Instant::now();
                // A coordinate token is good for one look; the next input
                // needs a fresh screenshot (CUC-B04).
                s.frame = None;
                if p.structure_changed {
                    s.snapshot = None;
                }
                if let Some(w) = &win {
                    s.last_window = w.clone();
                }
            }
        }
        self.emit(
            ctx.task_id,
            ctx.session_id,
            &ctx.run_id,
            TaskEvent::ComputerActionPerformed {
                control_id: plan.control_id.clone(),
                tool_call_id: ctx.tool_call_id.clone(),
                approval_id: ctx.approval_id.clone(),
                intent_hash: ctx.intent_hash.clone(),
                kind: act.kind().to_owned(),
                modality: modality.clone(),
                modality_reason: p.modality_reason.clone(),
                outcome: "SUCCEEDED".into(),
                code: String::new(),
                duration_ms,
                verified,
            },
        );
        let mut out = json!({
            "performed": act.kind(),
            "control_id": plan.control_id,
            "modality": modality,
            "modality_reason": p.modality_reason,
            "structure_changed": p.structure_changed,
            "changes": changed_summary(&p.changes),
            "verified": verified,
            "duration_ms": duration_ms,
            "provenance": tree::PROVENANCE,
        });
        if let Some(w) = &win {
            out["window"] = window_json(w);
        }
        out["next"] = json!(if plan.scope == Scope::Screen {
            "the screen after the input is attached"
        } else if p.structure_changed {
            "the tree was replaced (a dialog or sheet opened, or the window changed): read the state again before acting on an element"
        } else {
            "APP scope: observe again (computer.state, or computer.screenshot for coordinates) before the next coordinate use"
        });
        if verified == Some(false) {
            out["warning"] = json!(
                "the field's value does not read back as the value that was set; observe before relying on it"
            );
        }
        // SCREEN scope returns the screen after the input (CUC-B04).
        let mut frame = None;
        if plan.scope == Scope::Screen
            && let Ok((ans, _)) = self
                .capture_inner(
                    ctx,
                    a,
                    &plan.control_id,
                    &plan.app,
                    &plan.window.id,
                    plan.scope,
                    "screenshot",
                )
                .await
        {
            out["frame"] = ans.output["frame"].clone();
            out["screenshot"] = ans.output;
            frame = ans.frame;
        }
        Ok(Answer { output: out, frame })
    }
}
