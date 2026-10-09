//! The model's tool surface: projection mode, schema-bytes budget and the
//! assembly of one request's tools (PX-114, docs/16).
//!
//! `runtime::projection` decides which tools a task may be offered (profile,
//! lease, policy, plan scope — what it has always decided). This module
//! decides how they are *shown*: every one with its schema (`direct`), or
//! only the small procedural surface with the rest reached as `tools.*`
//! bindings of a program (`exec_only`), within a schema-bytes budget that
//! drops the lowest-priority tools first and never the ones a run's safety
//! rests on. Showing is never authority: the Capability Kernel decides every
//! call, whichever way the tool was reached.

use std::collections::BTreeSet;

use modbit_core_runtime::projection::{
    AGENT_TOOLS, BudgetOutcome, Costed, EXEC_ONLY_DEFERRED, EXEC_ONLY_SURFACE, PROGRAM_TOOLS,
    ProjectionMode, compact_signature, enforce_budget, schema_bytes,
};
use modbit_domain::task::Task;
use modbit_providers::ToolProjection;

use crate::server::Core;

/// The budget a request's tool schemas get when neither the task, the model
/// nor the Core's environment names one: well above the ~14 KB the stable
/// core costs (docs/16, M5.6), so the default changes nothing today and a
/// registry that grows without bound is caught, not silently sent.
pub(crate) const DEFAULT_MAX_PROJECTION_BYTES: usize = 32 * 1024;

/// What the system segment adds in `exec_only`: the rules above name tools
/// by their direct names; here is how they are reached.
pub(crate) const EXEC_ONLY_NOTE: &str = "Tool surface for this task: you act through `proc.exec`. Inside a program, every tool its description lists is `await tools.<name>(args)` (for example `tools.fs.read({path})`, `tools.change.apply({...})`, `tools.shell.exec({argv})`); the rules above name those tools and apply to them unchanged, and each call is checked, approved and recorded exactly as a direct call would be. `plan.update`, `user.ask`, `task.complete` and `tool.search` you call directly; `tool.search` returns the schema of a tool the program needs and is not listed.";

/// The largest the deferred-tool catalog in `tool.search`'s description may
/// grow before it is summarised by toolset.
pub(crate) const MAX_CATALOG_BYTES: usize = 2400;

/// The largest the signature block in `proc.exec`'s description may grow
/// before the signatures give way to bare names (`tool.search` returns a
/// tool's schema on demand).
const MAX_SIGNATURE_BYTES: usize = 6000;

/// What governs one task's projection this round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    /// How the surface is shown.
    pub mode: ProjectionMode,
    /// The most schema bytes one request may carry.
    pub max_bytes: usize,
    /// Where the mode came from (`task`, `model`, `core`, `default`).
    pub source: &'static str,
}

/// The settings in force: what a person set for the task, else what the
/// routed model's catalog entry says, else the Core's environment
/// (`MODBIT_TOOL_PROJECTION`, `MODBIT_MAX_PROJECTION_BYTES`), else `direct`
/// under the default budget. A reviewer keeps its own small surface.
pub(crate) fn settings_for(
    core: &Core,
    task: &Task,
    endpoint: &str,
    model: &str,
    configured: &(Option<ProjectionMode>, Option<usize>),
) -> Settings {
    let capability = core.gateway.capability(endpoint, model);
    let env_mode = std::env::var("MODBIT_TOOL_PROJECTION")
        .ok()
        .and_then(|v| ProjectionMode::parse(&v));
    let (mode, source) = if let Some(m) = configured.0 {
        (m, "task")
    } else if let Some(m) = capability
        .as_ref()
        .and_then(|c| c.projection_mode.as_deref())
        .and_then(ProjectionMode::parse)
    {
        (m, "model")
    } else if let Some(m) = env_mode {
        (m, "core")
    } else {
        (ProjectionMode::Direct, "default")
    };
    let env_bytes = std::env::var("MODBIT_MAX_PROJECTION_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|b| *b > 0);
    let max_bytes = configured
        .1
        .or_else(|| {
            capability
                .as_ref()
                .map(|c| c.max_projection_bytes as usize)
                .filter(|b| *b > 0)
        })
        .or(env_bytes)
        .unwrap_or(DEFAULT_MAX_PROJECTION_BYTES);
    let mode = if crate::critique::is_review(task) {
        ProjectionMode::Direct
    } else {
        mode
    };
    Settings {
        mode,
        max_bytes,
        source,
    }
}

/// The harness tools `exec_only` defers, with the one line the deferred
/// tool search shows for each and the words that find it; `agent.*` is one
/// entry (delegation).
pub(crate) fn harness_deferred_catalog()
-> Vec<(&'static str, &'static str, &'static [&'static str])> {
    vec![
        (
            "agent.*",
            "delegate bounded subtasks to child agents (agent.spawn, agent.wait, agent.result, agent.cancel, agent.attend, agent.park, agent.resume, agent.steer)",
            &[
                "agent",
                "delegate",
                "delegation",
                "subtask",
                "child",
                "spawn",
                "parallel",
            ],
        ),
        (
            "repair.attempt",
            "record a repair attempt after a failed verification",
            &["repair", "fix", "failing", "failure", "hypothesis"],
        ),
        (
            "context.fast",
            "hand a context question to a bounded read-only retrieval specialist",
            &["context", "retrieval", "specialist", "pack"],
        ),
        (
            "verify.run",
            "run the derived verification plan as a targeted stage",
            &["verify", "verification", "test", "tests", "check", "build"],
        ),
    ]
}

/// Whether a search query finds a harness catalog entry: a query word of
/// three letters or more that is, or begins, one of its keywords (or the
/// tool's own name).
pub(crate) fn harness_matches(
    name: &str,
    keywords: &[&str],
    words: &[&str],
    explicit: &[String],
) -> bool {
    explicit
        .iter()
        .any(|e| e == name || (name == "agent.*" && e.starts_with("agent.")))
        || words.iter().any(|w| {
            w.len() >= 3
                && (keywords
                    .iter()
                    .any(|k| k.starts_with(*w) || w.starts_with(*k))
                    || name.contains(*w))
        })
}

/// The facts that decide which deferred harness tools `exec_only` projects
/// this round.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Facts {
    /// The task has a child that has not finished.
    pub delegating: bool,
    /// The model asked for delegation through the tool search at the last
    /// boundary: the `agent.*` family is offered for this one turn.
    pub delegation_offered: bool,
    /// A failed verification is open: a change needs a repair attempt first.
    pub repair_open: bool,
    /// The task owns a terminal that is running now: `shell.input` and
    /// `shell.attach` are offered (REQ-PX-099).
    pub live_shell: bool,
}

/// The facts of this round, from the log and the harness state. The
/// delegation offer is consumed: it lasts one round.
pub(crate) async fn facts_for(
    core: &Core,
    task: &Task,
    state: &mut modbit_core_runtime::harness::HarnessState,
) -> Facts {
    let delegating = {
        let store = core.store.lock().await;
        store.agent_nodes(&task.task_id).is_ok_and(|nodes| {
            nodes.iter().any(|n| {
                n.kind == "SUBAGENT"
                    && !matches!(n.status.as_str(), "COMPLETED" | "FAILED" | "CANCELLED")
            })
        })
    };
    Facts {
        delegating,
        delegation_offered: std::mem::take(&mut state.delegation_offered),
        repair_open: !state.open_verify_signatures().is_empty() || state.pending_attempt.is_some(),
        live_shell: crate::background_process::owns_live_shell(core, task).await,
    }
}

/// One request's tools.
pub(crate) struct Assembled {
    /// What the model is shown, sorted by name.
    pub tools: Vec<ToolProjection>,
    /// What a program may bind: every tool the task may be offered, shown or
    /// not (the registry tools the model reaches as `tools.*`).
    pub program_names: Vec<String>,
    /// What the budget decided.
    pub outcome: BudgetOutcome,
    /// Harness tools left out of the request on purpose (deferred) or by the
    /// budget.
    pub hidden: Vec<String>,
}

fn costed(t: &ToolProjection, activated: &[String], pinned: bool) -> Costed {
    Costed {
        name: t.name.clone(),
        bytes: schema_bytes(&t.name, &t.description, &t.input_schema),
        activated: activated.contains(&t.name),
        pinned,
    }
}

/// The first sentence of a description, bounded, with its effect tag.
fn one_line(description: &str) -> String {
    let effect = description
        .rfind("[effect: ")
        .map(|i| format!("[{}", description[i + 9..].trim_end_matches(']')))
        .map(|e| format!("{e}]"))
        .unwrap_or_default();
    let body = description
        .split_once(". ")
        .map_or(description, |(first, _)| first);
    let body = body.split(" [effect:").next().unwrap_or(body);
    let mut end = body.len().min(72);
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    let cut = if end < body.len() { "…" } else { "" };
    format!("{}{cut} {effect}", &body[..end]).trim().to_owned()
}

/// The `proc.exec` description in `exec_only`: the program contract, then
/// the tools a program reaches, as call signatures (or bare names once they
/// no longer fit).
fn exec_only_description(lean: &str, registry: &[&ToolProjection], signatures: bool) -> String {
    let mut text = lean.to_owned();
    if registry.is_empty() {
        text.push_str("\nTools: none projected.");
        return text;
    }
    if signatures {
        text.push_str("\nTools (`?` marks an optional argument):");
        for t in registry {
            text.push_str(&format!(
                "\n- {} — {}",
                compact_signature(&t.name, &t.input_schema),
                one_line(&t.description)
            ));
        }
    } else {
        text.push_str("\nTools (names only; `tool.search` returns a tool's schema): ");
        text.push_str(
            &registry
                .iter()
                .map(|t| format!("tools.{}", t.name))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    text
}

/// Assemble one request's tools from everything the task may be offered.
///
/// `all` is the projection `direct` would send (registry tools the profile,
/// lease, policy and plan scope allow, plus the harness tools);
/// `registry` names the ones that came from the registry; `exec_base` is
/// the `proc.exec` contract text before its mode-specific tail and
/// `exec_lean` the shorter contract `exec_only` uses.
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble(
    settings: &Settings,
    all: Vec<ToolProjection>,
    registry: &BTreeSet<String>,
    exec_base: &str,
    exec_lean: &str,
    exec_direct_tail: &str,
    activated: &[String],
    facts: Facts,
    review: bool,
) -> Assembled {
    let program_names: Vec<String> = all.iter().map(|t| t.name.clone()).collect();
    let pins = |name: &str| -> bool {
        (name == "repair.attempt" && facts.repair_open)
            || (facts.delegating && matches!(name, "agent.wait" | "agent.result" | "agent.cancel"))
    };
    let exec_only = settings.mode == ProjectionMode::ExecOnly && !review;
    let typed = settings.mode == ProjectionMode::Typed && !review;
    let mut hidden: Vec<String> = Vec::new();
    let mut shown: Vec<ToolProjection> = Vec::new();
    let mut hidden_registry: Vec<ToolProjection> = Vec::new();
    for mut t in all {
        if t.name == crate::procedural::EXEC_TOOL && !exec_only {
            t.description = format!("{exec_base}{exec_direct_tail}");
        }
        if typed && PROGRAM_TOOLS.contains(&t.name.as_str()) {
            hidden.push(t.name.clone());
            continue;
        }
        if !exec_only {
            shown.push(t);
            continue;
        }
        let keep = EXEC_ONLY_SURFACE.contains(&t.name.as_str())
            || (t.name == "repair.attempt" && (facts.repair_open || activated.contains(&t.name)))
            || (matches!(t.name.as_str(), "context.fast" | "verify.run")
                && activated.contains(&t.name))
            || (AGENT_TOOLS.contains(&t.name.as_str())
                && (facts.delegating || facts.delegation_offered));
        if keep {
            shown.push(t);
        } else if registry.contains(&t.name) {
            hidden_registry.push(t);
        } else {
            hidden.push(t.name.clone());
        }
    }
    let build = |signatures: bool, shown: &[ToolProjection]| -> Vec<ToolProjection> {
        let mut out = shown.to_vec();
        if exec_only {
            let reg: Vec<&ToolProjection> = hidden_registry.iter().collect();
            let sig_text = exec_only_description(exec_lean, &reg, signatures);
            // Signatures that would not fit give way to names at the
            // block's own bound, before the budget is even asked.
            let sig_text = if signatures && sig_text.len() > exec_lean.len() + MAX_SIGNATURE_BYTES {
                exec_only_description(exec_lean, &reg, false)
            } else {
                sig_text
            };
            if let Some(t) = out
                .iter_mut()
                .find(|t| t.name == crate::procedural::EXEC_TOOL)
            {
                t.description = sig_text;
            }
        }
        out
    };
    let budget = Some(settings.max_bytes);
    let mut tools = build(true, &shown);
    let measure = |tools: &[ToolProjection]| -> BudgetOutcome {
        let costs: Vec<Costed> = tools
            .iter()
            .map(|t| {
                let pinned = pins(&t.name)
                    || (exec_only && EXEC_ONLY_SURFACE.contains(&t.name.as_str()))
                    || t.name == crate::critique::REPORT_TOOL;
                costed(t, activated, pinned)
            })
            .collect();
        enforce_budget(&costs, budget)
    };
    let mut outcome = measure(&tools);
    // The exec description's signatures are the one block the budget may
    // shrink without dropping a tool: names instead of signatures.
    if exec_only && outcome.over_budget {
        tools = build(false, &shown);
        outcome = measure(&tools);
    }
    if !outcome.dropped.is_empty() {
        let dropped: BTreeSet<&str> = outcome.dropped.iter().map(|d| d.name.as_str()).collect();
        tools.retain(|t| !dropped.contains(t.name.as_str()));
    }
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    Assembled {
        tools,
        program_names,
        outcome,
        hidden,
    }
}

/// Whether `name` is a tool `exec_only` defers on the harness side (it is
/// never executed when it was not projected).
pub(crate) fn is_gated_harness_tool(name: &str) -> bool {
    AGENT_TOOLS.contains(&name)
        || EXEC_ONLY_DEFERRED.contains(&name)
        || PROGRAM_TOOLS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tp(name: &str, desc: &str, props: &[&str]) -> ToolProjection {
        let properties: serde_json::Map<String, serde_json::Value> = props
            .iter()
            .map(|p| ((*p).to_owned(), serde_json::json!({"type":"string"})))
            .collect();
        ToolProjection {
            name: name.into(),
            description: desc.into(),
            input_schema: serde_json::json!({"type":"object","properties":properties,"required":props.first().map(|p| vec![*p]).unwrap_or_default()}),
        }
    }

    fn surface() -> Vec<ToolProjection> {
        let mut v = vec![
            tp(
                "fs.read",
                "Read a file. More text here [effect: Read]",
                &["path"],
            ),
            tp("shell.exec", "Run a command [effect: Exec]", &["argv"]),
            tp("plan.update", "plan", &["outcome"]),
            tp("task.complete", "complete", &["summary"]),
            tp("user.ask", "ask", &["question"]),
            tp("tool.search", "search", &["query"]),
            tp("context.fast", "fast", &["query"]),
            tp("verify.run", "verify", &["reason"]),
            tp("repair.attempt", "repair", &["hypothesis"]),
            tp("proc.exec", "base", &["program"]),
            tp("proc.wait", "wait", &["handle"]),
        ];
        for a in AGENT_TOOLS {
            v.push(tp(a, "agent", &["x"]));
        }
        v
    }

    fn settings(mode: ProjectionMode, max: usize) -> Settings {
        Settings {
            mode,
            max_bytes: max,
            source: "test",
        }
    }

    fn registry() -> BTreeSet<String> {
        ["fs.read", "shell.exec"].map(String::from).into()
    }

    #[test]
    fn exec_only_shows_the_small_surface_and_binds_the_rest() {
        let a = assemble(
            &settings(ProjectionMode::ExecOnly, 64 * 1024),
            surface(),
            &registry(),
            "base",
            "lean",
            "",
            &[],
            Facts::default(),
            false,
        );
        let names: Vec<&str> = a.tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "plan.update",
                "proc.exec",
                "proc.wait",
                "task.complete",
                "tool.search",
                "user.ask"
            ]
        );
        assert!(a.program_names.contains(&"fs.read".to_owned()));
        assert!(a.program_names.contains(&"agent.spawn".to_owned()));
        let exec = a.tools.iter().find(|t| t.name == "proc.exec").unwrap();
        assert!(exec.description.starts_with("lean"), "{}", exec.description);
        assert!(
            exec.description.contains("tools.fs.read({path: string})"),
            "{}",
            exec.description
        );
        assert!(exec.description.contains("[Read]"), "{}", exec.description);
        for deferred in [
            "agent.spawn",
            "repair.attempt",
            "context.fast",
            "verify.run",
        ] {
            assert!(a.hidden.contains(&deferred.to_owned()), "{deferred}");
        }
    }

    #[test]
    fn delegation_repair_and_activation_bring_deferred_tools_back_for_that_round() {
        let s = settings(ProjectionMode::ExecOnly, 64 * 1024);
        let names = |facts: Facts, activated: &[String]| -> Vec<String> {
            assemble(
                &s,
                surface(),
                &registry(),
                "base",
                "lean",
                "",
                activated,
                facts,
                false,
            )
            .tools
            .into_iter()
            .map(|t| t.name)
            .collect()
        };
        let offered = names(
            Facts {
                delegation_offered: true,
                ..Facts::default()
            },
            &[],
        );
        assert!(offered.contains(&"agent.spawn".to_owned()));
        let after = names(Facts::default(), &[]);
        assert!(!after.contains(&"agent.spawn".to_owned()), "removed after");
        let repair = names(
            Facts {
                repair_open: true,
                ..Facts::default()
            },
            &[],
        );
        assert!(repair.contains(&"repair.attempt".to_owned()));
        let verify = names(Facts::default(), &["verify.run".to_owned()]);
        assert!(verify.contains(&"verify.run".to_owned()));
    }

    #[test]
    fn direct_mode_projects_everything_and_keeps_the_exec_tail() {
        let a = assemble(
            &settings(ProjectionMode::Direct, 1 << 20),
            surface(),
            &registry(),
            "base",
            "lean",
            " tail",
            &[],
            Facts::default(),
            false,
        );
        assert_eq!(a.tools.len(), surface().len());
        let exec = a.tools.iter().find(|t| t.name == "proc.exec").unwrap();
        assert_eq!(exec.description, "base tail");
        assert!(a.outcome.dropped.is_empty());
    }

    #[test]
    fn the_budget_drops_lowest_priority_first_and_records_it() {
        let full = assemble(
            &settings(ProjectionMode::Direct, 1 << 20),
            surface(),
            &registry(),
            "base",
            "lean",
            "",
            &[],
            Facts::default(),
            false,
        );
        let budget = full.outcome.projected_bytes - 300;
        let a = assemble(
            &settings(ProjectionMode::Direct, budget),
            surface(),
            &registry(),
            "base",
            "lean",
            "",
            &[],
            Facts::default(),
            false,
        );
        assert!(a.outcome.projected_bytes <= budget, "{:?}", a.outcome);
        assert!(!a.outcome.over_budget);
        assert!(!a.outcome.dropped.is_empty());
        assert!(
            a.outcome.dropped[0].name.starts_with("agent."),
            "agent tools are the first to go: {:?}",
            a.outcome.dropped
        );
        for kept in ["plan.update", "user.ask", "task.complete", "tool.search"] {
            assert!(a.tools.iter().any(|t| t.name == kept), "{kept} kept");
        }
        // And nothing sits above the budget unrecorded.
        assert_eq!(a.outcome.requested_bytes, full.outcome.projected_bytes);
    }

    #[test]
    fn a_budget_the_safety_tools_alone_exceed_is_a_typed_over_budget_result() {
        let a = assemble(
            &settings(ProjectionMode::Direct, 50),
            surface(),
            &registry(),
            "base",
            "lean",
            "",
            &[],
            Facts::default(),
            false,
        );
        assert!(a.outcome.over_budget);
        for kept in ["plan.update", "user.ask", "task.complete", "tool.search"] {
            assert!(a.tools.iter().any(|t| t.name == kept), "{kept} kept");
        }
    }

    #[test]
    fn typed_mode_withholds_the_program_runtime_and_keeps_every_typed_tool() {
        let a = assemble(
            &settings(ProjectionMode::Typed, 1 << 20),
            surface(),
            &registry(),
            "base",
            "lean",
            " tail",
            &[],
            Facts::default(),
            false,
        );
        assert!(!a.tools.iter().any(|t| t.name.starts_with("proc.")));
        assert!(a.tools.iter().any(|t| t.name == "fs.read"));
        assert!(a.tools.iter().any(|t| t.name == "agent.spawn"));
        assert!(a.hidden.contains(&"proc.exec".to_owned()));
        assert!(is_gated_harness_tool("proc.exec"));
    }

    #[test]
    fn a_reviewer_is_never_exec_only() {
        let a = assemble(
            &settings(ProjectionMode::ExecOnly, 1 << 20),
            surface(),
            &registry(),
            "base",
            "lean",
            "",
            &[],
            Facts::default(),
            true,
        );
        assert!(a.tools.iter().any(|t| t.name == "fs.read"));
    }
}
