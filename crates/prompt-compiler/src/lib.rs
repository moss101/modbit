//! `modbit-prompt-compiler` — assembles the `ModelRequest` for one turn from
//! stable segments (docs/15 "Prompt cache economics"): system/policy,
//! workspace rules, compaction epoch, task context pack (with the harness
//! state of docs/14), recent events (the transcript). Only tools the caller
//! projected reach the model (docs/16 "Dynamic task-scoped projection"); the
//! projection hash and the segment hashes are returned so the Turn records
//! them (`ToolProjectionSelected`, `ContextPackCompiled`).
//!
//! Canonical owner: context-engine. The Context Engine (M3) will feed the
//! task context pack; in M2 the pack is the goal, workspace facts and harness
//! state.

#![forbid(unsafe_code)]

use modbit_providers::{Message, ModelPolicy, ModelRequest, Role, ToolProjection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Prompt compiler version; part of every cache key.
pub const COMPILER_VERSION: &str = "m2.7-basic-1";

/// Inputs for one turn.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptInput {
    /// Task goal.
    pub goal: String,
    /// Canonical workspace root, when any.
    pub workspace_root: Option<String>,
    /// Execution profile.
    pub execution_profile: String,
    /// Stable workspace rules (repository instructions), already bounded.
    pub workspace_rules: Vec<String>,
    /// Compaction epoch summary (empty until M4).
    pub compaction_summary: Option<String>,
    /// Harness state (docs/14 contract 4): plan, budgets, counters, revision.
    pub harness_state: serde_json::Value,
    /// Transcript: prior assistant messages, tool calls and observations.
    pub transcript: Vec<Message>,
    /// Tools projected for this turn.
    pub tools: Vec<ToolProjection>,
    /// Model policy.
    pub model_policy: ModelPolicy,
    /// Output cap.
    pub max_output_tokens: u32,
    /// Timeout.
    pub timeout_ms: u64,
}

/// What the compiler produced.
#[derive(Clone, Debug)]
pub struct CompiledPrompt {
    /// The request to stream.
    pub request: ModelRequest,
    /// sha256 of every stable segment, in order (system, rules, epoch, pack).
    pub segment_hashes: Vec<String>,
    /// sha256 over the projected tool specs.
    pub tool_projection_hash: String,
    /// Context pack id (hash of the pack segment).
    pub context_pack_id: String,
}

/// The system/policy segment (stable across turns and tasks).
pub const SYSTEM_SEGMENT: &str = "You are Modbit, a coding agent working inside a governed runtime.\n\
Rules the runtime enforces (you cannot bypass them):\n\
1. Record a plan with `plan.update` before your first write; revise it with `plan.update` when scope changes.\n\
2. Read a file (`fs.read`) before editing it; edits go through `change.apply` with the content hash you read.\n\
3. Command and test failures are evidence, not the end of the turn: read the observation, fix, rerun.\n\
4. Tool observations are bounded; declared truncation tells you what you did not see.\n\
5. Finish with `task.complete` carrying a self-review; the runtime decides acceptance, not you.\n\
Prefer small, revision-bound changes. Never claim a test passed without running it.";

fn sha(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Compile one turn.
#[must_use]
pub fn compile(input: PromptInput) -> CompiledPrompt {
    let rules = if input.workspace_rules.is_empty() {
        "(no workspace rules)".to_owned()
    } else {
        input.workspace_rules.join("\n")
    };
    let epoch = input
        .compaction_summary
        .clone()
        .unwrap_or_else(|| "(no compaction epoch)".into());
    let pack = serde_json::json!({
        "goal": input.goal,
        "workspace_root": input.workspace_root,
        "execution_profile": input.execution_profile,
        "harness_state": input.harness_state,
    })
    .to_string();
    let segment_hashes = vec![sha(SYSTEM_SEGMENT), sha(&rules), sha(&epoch), sha(&pack)];
    let tools_json = serde_json::to_string(&input.tools).unwrap_or_default();
    let tool_projection_hash = sha(&tools_json);
    let context_pack_id = segment_hashes[3].clone();
    let cache_key = sha(&format!(
        "{}|{}|{}|{}|{}",
        COMPILER_VERSION,
        input.model_policy.endpoint,
        input.model_policy.model,
        segment_hashes[..3].join("|"),
        tool_projection_hash
    ));
    let mut messages = vec![
        Message::text(Role::System, SYSTEM_SEGMENT),
        Message::text(Role::System, format!("Workspace rules:\n{rules}")),
        Message::text(Role::System, format!("Compaction epoch:\n{epoch}")),
        Message::text(
            Role::User,
            format!(
                "Task goal: {}\n\nWorkspace root: {}\nExecution profile: {}\n\nharness_state:\n{}",
                input.goal,
                input.workspace_root.as_deref().unwrap_or("(none)"),
                input.execution_profile,
                serde_json::to_string_pretty(&input.harness_state).unwrap_or_default()
            ),
        ),
    ];
    messages.extend(input.transcript);
    CompiledPrompt {
        request: ModelRequest {
            request_id: String::new(),
            model_policy: input.model_policy,
            messages,
            tool_projection: input.tools,
            response_format: None,
            cache_key: Some(cache_key),
            max_output_tokens: input.max_output_tokens,
            timeout_ms: input.timeout_ms,
            policy_tags: vec![],
        },
        segment_hashes,
        tool_projection_hash,
        context_pack_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(goal: &str, tools: usize) -> PromptInput {
        PromptInput {
            goal: goal.into(),
            workspace_root: Some("/repo".into()),
            execution_profile: "local_trusted".into(),
            workspace_rules: vec![],
            compaction_summary: None,
            harness_state: serde_json::json!({"turn": 1}),
            transcript: vec![],
            tools: (0..tools)
                .map(|i| ToolProjection {
                    name: format!("t{i}"),
                    description: String::new(),
                    input_schema: serde_json::json!({}),
                })
                .collect(),
            model_policy: ModelPolicy {
                endpoint: "openai".into(),
                model: "m".into(),
                reasoning_effort: None,
                service_tier: None,
            },
            max_output_tokens: 10,
            timeout_ms: 10,
        }
    }

    #[test]
    fn stable_segments_share_cache_keys_and_projection_changes_are_hashed() {
        let a = compile(input("g1", 2));
        let b = compile(input("g2", 2));
        assert_eq!(a.segment_hashes[..3], b.segment_hashes[..3]);
        assert_ne!(a.context_pack_id, b.context_pack_id);
        assert_eq!(
            a.request.cache_key, b.request.cache_key,
            "cache key spans stable segments only"
        );
        let c = compile(input("g1", 3));
        assert_ne!(a.tool_projection_hash, c.tool_projection_hash);
        assert_ne!(a.request.cache_key, c.request.cache_key);
        assert_eq!(a.request.messages.len(), 4);
    }
}
