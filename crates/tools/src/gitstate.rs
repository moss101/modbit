//! The governed tools over Core-hosted git state (PX-119, PX-066):
//! `git.merge.prepare`, `git.merge.commit`, `git.merge.abort`,
//! `git.apply.worktree` and `git.apply.undo`.
//!
//! Each is an ordinary registered tool: the pipeline validates its arguments,
//! the Capability Kernel judges its effect class under the task's lease (an
//! approval bound to the exact intent for a protected or destructive effect),
//! the dispatch is journaled before the effect runs, and the result is
//! receipted. What the tool *does* lives behind a [`GitStatePort`] the Core
//! implements, because the state it must keep — the merge transaction, the
//! pre-apply checkpoint, the worktree registry — is on the event log, and the
//! post-merge verification runs through the Core's verification engine. A
//! build with no port refuses the call (`NO_GIT_STATE`) instead of pretending.

use std::sync::Arc;

use modbit_domain::ToolCallId;
use serde_json::{Value, json};

use crate::pipeline::InvokeContext;
use crate::registry::{BoxFuture, Idempotency, Tool, ToolOutcome, ToolRegistry, ToolSpec};
use crate::{EffectClass, Result};

/// A typed refusal: `(code, message)`.
pub type Refusal = (String, String);

/// The Core's implementation of the git state the tools above keep.
pub trait GitStatePort: Send + Sync {
    /// Run `op` (`merge.prepare`, `merge.commit`, `merge.abort`,
    /// `worktree.apply`, `worktree.undo`, `worktree.created`,
    /// `worktree.closed`) for the call `tool_call_id`. `Ok` is the tool's
    /// structured output; `Err` a typed refusal.
    fn call<'a>(
        &'a self,
        op: &'a str,
        tool_call_id: Option<ToolCallId>,
        args: &'a Value,
    ) -> BoxFuture<'a, std::result::Result<Value, Refusal>>;
}

const PROFILES: &[&str] = &["local_trusted"];
const ABORT_PROFILES: &[&str] = &["local_trusted", "local_autonomous"];

fn spec(
    name: &str,
    description: &str,
    effect: EffectClass,
    input: Value,
    caps: &[&str],
    profiles: &[&str],
) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        version: "1".into(),
        description: description.into(),
        input_schema: input,
        output_schema: json!({"type": "object"}),
        effect_class: effect,
        required_capabilities: caps.iter().map(|s| (*s).to_owned()).collect(),
        execution_profiles: profiles.iter().map(|s| (*s).to_owned()).collect(),
        // A merge's verification runs the repository's checks.
        timeout_ms: 600_000,
        output_budget_bytes: 64 * 1024,
        idempotency: Idempotency::NonIdempotent,
        compensation: None,
    }
}

/// One tool: a spec, the op it runs on the port, and how its arguments raise
/// its effect class.
struct GitStateTool {
    spec: ToolSpec,
    op: &'static str,
    raise: fn(&Value) -> EffectClass,
}

impl Tool for GitStateTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn effect_of(&self, args: &Value) -> EffectClass {
        (self.raise)(args).max(self.spec.effect_class)
    }

    fn invoke<'a>(&'a self, ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move {
            let Some(port) = &ctx.git_state else {
                return ToolOutcome::fail(
                    "NO_GIT_STATE",
                    "this host keeps no merge or apply state; the call was not run",
                );
            };
            // The effect class the call was judged under: an overwrite is a
            // destructive effect, and a call judged lower may not do it.
            let judged = ctx.effect_class.unwrap_or(self.spec.effect_class);
            if (self.raise)(&args) > judged {
                return ToolOutcome::fail(
                    "EFFECT_CLASS_MISMATCH",
                    "the call asks for more than the class it was approved under",
                );
            }
            match port.call(self.op, ctx.tool_call_id, &args).await {
                Ok(v) => ToolOutcome::ok(v),
                Err((code, message)) => ToolOutcome::fail(&code, message),
            }
        })
    }
}

fn plain(_: &Value) -> EffectClass {
    EffectClass::ReadOnly
}

fn abort_class(args: &Value) -> EffectClass {
    if args.get("discard").and_then(Value::as_bool) == Some(true) {
        EffectClass::ProtectedWrite
    } else {
        EffectClass::ReversibleWrite
    }
}

fn apply_class(args: &Value) -> EffectClass {
    match args.get("option").and_then(Value::as_str) {
        Some("OVERWRITE" | "FULL_OVERWRITE") => EffectClass::Destructive,
        _ => EffectClass::ProtectedWrite,
    }
}

fn tool(spec: ToolSpec, op: &'static str, raise: fn(&Value) -> EffectClass) -> Arc<dyn Tool> {
    Arc::new(GitStateTool { spec, op, raise })
}

/// Register the five tools.
pub fn register(registry: &mut ToolRegistry) -> Result<()> {
    for t in [
        tool(
            spec(
                "git.merge.prepare",
                "Start a merge transaction in this task's checkout: merge a child agent's work (`child`: its idempotency key, agent id or task id; its worktree's whole state is the source) or a branch/revision (`source`) without committing. The checkout must be clean and on a branch. A clean merge is STAGED; conflicts are reported per file with the base, our and their text. Resolve a conflicted file by editing it (no conflict markers may remain), then call git.merge.commit; or back out with git.merge.abort. Everything the merge did is on the log, so a crash resumes or aborts it to a whole tree.",
                EffectClass::ProtectedWrite,
                json!({"type":"object","properties":{
                    "child":{"type":"string","minLength":1},
                    "source":{"type":"string","minLength":1},
                    "id":{"type":"string","minLength":1,"maxLength":64}
                },"oneOf":[{"required":["child"]},{"required":["source"]}],"additionalProperties":false}),
                &["git.merge"],
                PROFILES,
            ),
            "merge.prepare",
            plain,
        ),
        tool(
            spec(
                "git.merge.commit",
                "Commit a merge transaction. Every conflicted file must be resolved (edited, no conflict markers). The resolved tree then runs the repository's mandatory checks (post-merge verification); the merge commits only if they pass. A failing verification does not count: the transaction stays staged for you to fix or git.merge.abort. The result names both merged revisions.",
                EffectClass::ProtectedWrite,
                json!({"type":"object","properties":{
                    "id":{"type":"string","minLength":1,"maxLength":64},
                    "message":{"type":"string","maxLength":2000}
                },"required":["id"],"additionalProperties":false}),
                &["git.merge"],
                PROFILES,
            ),
            "merge.commit",
            plain,
        ),
        tool(
            spec(
                "git.merge.abort",
                "Abort a merge transaction: the checkout returns to exactly the commit it was on before the merge, no staged or conflicted file left. With `discard: true` and a `child`, also record the decision to drop that child's work, which settles it for the parent's completion; that needs an approval.",
                EffectClass::ReversibleWrite,
                json!({"type":"object","properties":{
                    "id":{"type":"string","minLength":1,"maxLength":64},
                    "child":{"type":"string","minLength":1},
                    "discard":{"type":"boolean"},
                    "reason":{"type":"string","maxLength":2000}
                },"additionalProperties":false}),
                &["git.merge"],
                ABORT_PROFILES,
            ),
            "merge.abort",
            abort_class,
        ),
        tool(
            spec(
                "git.apply.worktree",
                "Apply a task worktree's result to the checkout it was made from: bound to the exact plan (`plan_digest`) the person reviewed, with a pre-apply checkpoint so git.apply.undo restores every byte. Issued by the Core's ApplyWorktree command, which classifies the plan and offers the conflict options first.",
                EffectClass::ProtectedWrite,
                json!({"type":"object","properties":{
                    "worktree_id":{"type":"string","minLength":1},
                    "plan_digest":{"type":"string","minLength":8},
                    "apply_id":{"type":"string","minLength":4,"maxLength":64},
                    "option":{"type":"string","enum":["MERGE_MANUALLY","STASH","OVERWRITE","FULL_OVERWRITE"]},
                    "confirm_paths":{"type":"array","items":{"type":"string"}},
                    "candidate_revision":{"type":"integer","minimum":0}
                },"required":["worktree_id","plan_digest","apply_id"],"additionalProperties":false}),
                &["git.apply"],
                PROFILES,
            ),
            "worktree.apply",
            apply_class,
        ),
        tool(
            spec(
                "git.apply.undo",
                "Undo an apply: put the checkout's pre-apply bytes back. A path edited since the apply is a reported conflict and nothing is changed.",
                EffectClass::ProtectedWrite,
                json!({"type":"object","properties":{
                    "worktree_id":{"type":"string","minLength":1},
                    "apply_id":{"type":"string","minLength":4,"maxLength":64}
                },"required":["worktree_id","apply_id"],"additionalProperties":false}),
                &["git.apply"],
                PROFILES,
            ),
            "worktree.undo",
            plain,
        ),
    ] {
        registry.register(t)?;
    }
    Ok(())
}
