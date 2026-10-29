//! `handover.request` (PX-177, DR-PX-2026-10-26-018 D1/D2): a typed handover
//! to the person for a step only the person can do - a CAPTCHA, a payment
//! confirmation, a second factor, a secure field.
//!
//! The call is an external side effect that is approved by the person like
//! any input: the run parks on the approval with the typed reason and the
//! instruction, nothing runs while it waits, an unanswered approval expires
//! with the existing parked-approval expiry and acts on nothing, and the
//! person's approval means "I did the step; carry on". The tool does nothing
//! itself: it never types, clicks or reads anything of the target, so the
//! agent cannot complete the step. After it, the model must observe the
//! target again; the frames and snapshots it held before are stale.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::pipeline::InvokeContext;
use crate::registry::{BoxFuture, Idempotency, Tool, ToolOutcome, ToolRegistry, ToolSpec};
use crate::{EffectClass, Result};

/// The tool's name.
pub const NAME: &str = "handover.request";

/// The steps only the person does.
pub const KINDS: &[&str] = &[
    "captcha",
    "payment_confirmation",
    "second_factor",
    "secure_field",
    "other",
];

/// The surfaces a handover can name.
pub const TARGETS: &[&str] = &["browser", "computer"];

struct Handover(ToolSpec);

impl Tool for Handover {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    fn prepare<'a>(
        &'a self,
        _ctx: &'a InvokeContext,
        args: &'a Value,
    ) -> BoxFuture<'a, std::result::Result<Option<Value>, ToolOutcome>> {
        Box::pin(async move {
            // What the person is asked to do is part of what they approve, so
            // it is bound to the approval and shown with it; a handover that
            // names nothing valid is refused before anyone is asked.
            let kind = args["kind"].as_str().unwrap_or_default();
            let target = args["target"].as_str().unwrap_or_default();
            let instruction = args["instruction"].as_str().unwrap_or_default().trim();
            if !KINDS.contains(&kind) || !TARGETS.contains(&target) || instruction.is_empty() {
                return Err(ToolOutcome::fail(
                    "HANDOVER_INVALID",
                    format!(
                        "kind is one of {}, target is one of {} and instruction says what the person should do",
                        KINDS.join(", "),
                        TARGETS.join(", ")
                    ),
                ));
            }
            Ok(Some(json!({"handover": {
                "kind": kind,
                "target": target,
                "instruction": instruction,
                "note": "the person does this step themselves; approving means it is done",
            }})))
        })
    }

    fn invoke<'a>(&'a self, _ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move {
            let kind = args["kind"].as_str().unwrap_or_default();
            let target = args["target"].as_str().unwrap_or_default();
            if !KINDS.contains(&kind) || !TARGETS.contains(&target) {
                return ToolOutcome::fail(
                    "HANDOVER_INVALID",
                    format!(
                        "kind is one of {} and target is one of {}",
                        KINDS.join(", "),
                        TARGETS.join(", ")
                    ),
                );
            }
            // Reaching here means the person approved: they did the step.
            ToolOutcome::ok(json!({
                "handed_back": true,
                "kind": kind,
                "target": target,
                "next": "the person says the step is done. Every frame, snapshot and element you held before is stale: observe the target again (browser.snapshot, or computer.state / computer.screenshot) and continue from what you see, not from what you expect. If the step is not done, say so and stop; never attempt it yourself.",
            }))
        })
    }
}

/// Register the tool.
pub fn register_handover(registry: &mut ToolRegistry) -> Result<()> {
    let spec = ToolSpec {
        name: NAME.into(),
        version: "1".into(),
        description: "Hand a step to the person that only the person can do (a CAPTCHA, a payment confirmation, a second factor, a secure field). The run parks until they approve that they did it; you never do the step yourself and never enter a credential or payment detail. Arguments: kind (captcha | payment_confirmation | second_factor | secure_field | other), target (browser | computer), instruction (what the person should do, up to 300 characters). An approval that is not answered expires and nothing is done. After it returns, observe the target again.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "kind": {"type": "string", "enum": KINDS},
                "target": {"type": "string", "enum": TARGETS},
                "instruction": {"type": "string", "maxLength": 300},
            },
            "required": ["kind", "target", "instruction"],
        }),
        output_schema: json!({"type": "object"}),
        effect_class: EffectClass::ExternalSideEffect,
        required_capabilities: vec![crate::browser::CAPABILITY.to_owned()],
        execution_profiles: ["local_trusted", "local_autonomous", "cloud_isolated"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        timeout_ms: 30_000,
        output_budget_bytes: 8 * 1024,
        idempotency: Idempotency::NonIdempotent,
        compensation: None,
    };
    registry.register(Arc::new(Handover(spec)))
}
