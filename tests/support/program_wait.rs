//! What a scripted model does with a program that outlived `proc.exec`'s 2 s
//! inline grace (docs/16 "Procedural runtime": a slow runner's), shared by the
//! test binaries whose scripts run a program (`#[path]`-included, like
//! `scripted_openai.rs`).
#![allow(dead_code)]

/// The reply of a model whose last tool result says a program is still
/// running (`status: RUNNING`, `handle: <id>`): wait for it. `proc.exec`
/// hands a handle back when a program outlives its 2 s inline grace, which on
/// a loaded Windows runner a program of two tool calls does; a scripted model
/// that went on to its next step instead would complete the task while the
/// program ran (`PROGRAM_RUNNING`), run out of script, and be suspended
/// `NO_PROGRESS`. `None` when no program is pending.
#[must_use]
pub fn program_wait_reply(body: &serde_json::Value) -> Option<serde_json::Value> {
    let last = body["messages"]
        .as_array()?
        .iter()
        .rev()
        .find(|m| m["role"] == "tool")?["content"]
        .as_str()?;
    if !last.starts_with("status: RUNNING") {
        return None;
    }
    let handle = last
        .lines()
        .find_map(|l| l.strip_prefix("handle: "))?
        .trim();
    Some(
        serde_json::json!({"calls": [{"name": "proc.wait", "args": {"handle": handle, "timeout_ms": 60_000}}]}),
    )
}

/// How many steps of a script the conversation has taken: the tool results
/// it carries, not counting those that answer a `proc.wait` (the model's wait
/// for a program that outlived `proc.exec`'s grace is not a step of the
/// script, so the script resumes where the program left it).
#[must_use]
pub fn script_steps_done(body: &serde_json::Value) -> usize {
    let Some(messages) = body["messages"].as_array() else {
        return 0;
    };
    let waits: std::collections::HashSet<&str> = messages
        .iter()
        .filter(|m| m["role"] == "assistant")
        .flat_map(|m| m["tool_calls"].as_array().into_iter().flatten())
        .filter(|c| c["function"]["name"] == "proc.wait")
        .filter_map(|c| c["id"].as_str())
        .collect();
    messages
        .iter()
        .filter(|m| m["role"] == "tool")
        .filter(|m| {
            m["tool_call_id"]
                .as_str()
                .is_none_or(|id| !waits.contains(id))
        })
        .count()
}
