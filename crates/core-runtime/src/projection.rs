//! Tool-projection modes and the schema-bytes budget (PX-114, docs/16
//! "Dynamic task-scoped projection").
//!
//! The model is offered a *projection* of the tool surface. The default
//! (`direct`) offers every projected tool with its schema. `exec_only`
//! offers the small procedural surface — `proc.exec`, `proc.wait`,
//! `user.ask`, `plan.update`, `task.complete`, `tool.search` — and the
//! projected registry tools are reached from a program as `tools.*`
//! bindings, each call an ordinary governed tool call (policy decision,
//! approval, receipt). A hidden tool is simply not in the request; hiding is
//! never authority and showing is never authority.
//!
//! The budget is the other half: a configurable `max_projection_bytes`
//! bounds what one request's tool schemas may cost. It is enforced when the
//! projection is built, drops the lowest-priority tools first, never drops
//! the tools a run's safety depends on, and is a typed result — what was
//! projected, what was dropped and what the budget was — not a silent
//! truncation.
//!
//! Everything here is pure: the Core's runtime owns what is projected, this
//! module decides the mode, the measure and the degradation order.

use serde::{Deserialize, Serialize};

/// How the model's tool surface is projected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMode {
    /// Every projected tool with its schema (the default until the paired
    /// trial decides otherwise).
    #[default]
    Direct,
    /// The small procedural surface; registry tools through `tools.*`.
    ExecOnly,
    /// The typed tools only: `proc.exec` and `proc.wait` are not offered.
    /// The paired trial's `direct` arm (no program runtime at all); not a
    /// default anywhere.
    Typed,
}

impl ProjectionMode {
    /// The stable label (events, configuration, the wire).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::ExecOnly => "exec_only",
            Self::Typed => "typed",
        }
    }

    /// Parse a label; `None` for anything else (an unknown mode is never
    /// guessed at).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "direct" => Some(Self::Direct),
            "exec_only" => Some(Self::ExecOnly),
            "typed" => Some(Self::Typed),
            _ => None,
        }
    }
}

/// The tools an `exec_only` request carries with their schemas, always.
pub const EXEC_ONLY_SURFACE: [&str; 6] = [
    "proc.exec",
    "proc.wait",
    "user.ask",
    "plan.update",
    "task.complete",
    "tool.search",
];

/// The harness tools `exec_only` defers: discoverable through the deferred
/// tool search, projected only while the task needs them, never authorised
/// by being found.
pub const EXEC_ONLY_DEFERRED: [&str; 3] = ["repair.attempt", "context.fast", "verify.run"];

/// The program surface `typed` withholds.
pub const PROGRAM_TOOLS: [&str; 2] = ["proc.exec", "proc.wait"];

/// The `agent.*` family, deferred until the task delegates.
pub const AGENT_TOOLS: [&str; 8] = [
    "agent.spawn",
    "agent.wait",
    "agent.result",
    "agent.cancel",
    "agent.attend",
    "agent.park",
    "agent.resume",
    "agent.steer",
];

/// The bytes one projected tool costs a request: the name, the description
/// and the schema as the providers serialise them. One measure, used by the
/// budget, the projection event and the benchmark alike.
#[must_use]
pub fn schema_bytes(name: &str, description: &str, input_schema: &serde_json::Value) -> usize {
    name.len() + description.len() + input_schema.to_string().len()
}

/// How reluctantly a tool is dropped when the budget is exceeded. Lower
/// values go first.
#[must_use]
pub fn drop_rank(name: &str, activated: bool) -> u8 {
    match name {
        // Delegation controls for children already running go first; the
        // task works alone unless it delegates.
        "agent.attend" | "agent.park" | "agent.resume" | "agent.steer" => 0,
        "agent.cancel" | "agent.result" | "agent.wait" | "agent.spawn" => 1,
        "context.fast" => 2,
        "verify.run" => 3,
        // Host tools the model discovered and activated.
        n if activated && !is_core_registry(n) => 4,
        // The stable core, least central first.
        "search.retrieve" | "context.pack" => 5,
        "search.exact" | "shell.start" | "shell.read" | "shell.cancel" => 6,
        "shell.exec" => 7,
        n if n.starts_with("fs.") => 8,
        n if n.starts_with("change.") => 9,
        "proc.exec" | "proc.wait" => 10,
        // Anything else projected with a schema (host tools that are not
        // deferred) sits above the activated tail and below the core.
        _ => 4,
    }
}

fn is_core_registry(name: &str) -> bool {
    name.starts_with("fs.")
        || name.starts_with("change.")
        || name.starts_with("shell.")
        || matches!(
            name,
            "search.retrieve" | "search.exact" | "context.pack" | "proc.exec" | "proc.wait"
        )
}

/// Tools the budget never drops: a run's safety, its plan gate, its approval
/// path, its way to complete and its way to find what was deferred.
/// `repair.attempt` is named here only while a failed verification is open
/// (the caller passes it as `pinned`); `review.report` is a reviewer's only
/// way to answer.
#[must_use]
pub fn never_dropped(name: &str) -> bool {
    matches!(
        name,
        "user.ask" | "plan.update" | "task.complete" | "tool.search" | "review.report"
    )
}

/// One tool as the budget sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Costed {
    /// Tool name.
    pub name: String,
    /// What it costs a request, from [`schema_bytes`].
    pub bytes: usize,
    /// Whether the model activated it through `tool.search`.
    pub activated: bool,
    /// Pinned by the caller for this round (for instance `repair.attempt`
    /// while a failed verification is open; the `exec_only` surface).
    pub pinned: bool,
}

/// A tool the budget dropped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dropped {
    /// Tool name.
    pub name: String,
    /// The bytes it would have cost.
    pub bytes: usize,
}

/// What enforcing a budget on one projection decided.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetOutcome {
    /// The budget enforced, in bytes (`None` = unbounded).
    pub budget: Option<usize>,
    /// Bytes of the projection after dropping.
    pub projected_bytes: usize,
    /// Bytes of the projection before dropping.
    pub requested_bytes: usize,
    /// What was dropped, in the order it was dropped.
    pub dropped: Vec<Dropped>,
    /// Set when the tools that may never be dropped alone exceed the budget:
    /// the request cannot be built within it, and the caller must fail the
    /// round closed rather than send an over-budget request.
    pub over_budget: bool,
}

impl BudgetOutcome {
    /// An unbounded projection.
    #[must_use]
    pub fn unbounded(bytes: usize) -> Self {
        Self {
            budget: None,
            projected_bytes: bytes,
            requested_bytes: bytes,
            dropped: Vec::new(),
            over_budget: false,
        }
    }
}

/// Drop tools, lowest rank first, until the projection fits `budget`.
///
/// Never-dropped and pinned tools are kept whatever the budget; when they
/// alone exceed it the outcome says so.
#[must_use]
pub fn enforce_budget(tools: &[Costed], budget: Option<usize>) -> BudgetOutcome {
    let requested: usize = tools.iter().map(|t| t.bytes).sum();
    let Some(budget) = budget else {
        return BudgetOutcome::unbounded(requested);
    };
    let mut kept: Vec<&Costed> = tools.iter().collect();
    let mut total = requested;
    let mut dropped = Vec::new();
    while total > budget {
        // The droppable tool with the lowest rank; the largest of equals
        // first (it buys the most room), then by name for determinism.
        let victim = kept
            .iter()
            .enumerate()
            .filter(|(_, t)| !never_dropped(&t.name) && !t.pinned)
            .min_by(|(_, a), (_, b)| {
                drop_rank(&a.name, a.activated)
                    .cmp(&drop_rank(&b.name, b.activated))
                    .then(b.bytes.cmp(&a.bytes))
                    .then(a.name.cmp(&b.name))
            })
            .map(|(i, _)| i);
        let Some(i) = victim else {
            break;
        };
        let t = kept.remove(i);
        total -= t.bytes;
        dropped.push(Dropped {
            name: t.name.clone(),
            bytes: t.bytes,
        });
    }
    BudgetOutcome {
        budget: Some(budget),
        projected_bytes: total,
        requested_bytes: requested,
        dropped,
        over_budget: total > budget,
    }
}

/// A compact call signature for a tool, from its JSON Schema: the name and
/// its properties with types, optional ones marked — what a program needs
/// to call a binding without the schema's full text. Not a schema, never
/// validated against; the Core validates the real call.
#[must_use]
pub fn compact_signature(name: &str, schema: &serde_json::Value) -> String {
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let props = schema["properties"].as_object();
    let mut fields: Vec<String> = Vec::new();
    if let Some(props) = props {
        for (k, v) in props {
            let ty = match v["type"].as_str() {
                Some("array") => format!("{}[]", v["items"]["type"].as_str().unwrap_or("any")),
                Some(t) => t.to_owned(),
                None => "any".to_owned(),
            };
            let opt = if required.contains(&k.as_str()) {
                ""
            } else {
                "?"
            };
            fields.push(format!("{k}{opt}: {ty}"));
        }
    }
    format!("tools.{name}({{{}}})", fields.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(name: &str, bytes: usize) -> Costed {
        Costed {
            name: name.into(),
            bytes,
            activated: false,
            pinned: false,
        }
    }

    #[test]
    fn modes_parse_and_never_guess() {
        assert_eq!(
            ProjectionMode::parse("exec_only"),
            Some(ProjectionMode::ExecOnly)
        );
        assert_eq!(
            ProjectionMode::parse(" direct "),
            Some(ProjectionMode::Direct)
        );
        assert_eq!(ProjectionMode::parse("exec-only"), None);
        assert_eq!(ProjectionMode::parse(""), None);
        assert_eq!(ProjectionMode::default().as_str(), "direct");
    }

    #[test]
    fn an_unbounded_projection_drops_nothing() {
        let o = enforce_budget(&[c("fs.read", 500), c("agent.spawn", 900)], None);
        assert!(o.dropped.is_empty() && !o.over_budget);
        assert_eq!(o.projected_bytes, 1400);
    }

    #[test]
    fn the_lowest_priority_tools_go_first_and_safety_tools_stay() {
        let tools = [
            c("plan.update", 1000),
            c("user.ask", 600),
            c("task.complete", 500),
            c("tool.search", 400),
            c("fs.read", 300),
            c("change.apply", 700),
            c("agent.steer", 400),
            c("agent.spawn", 900),
            c("context.fast", 350),
            c("verify.run", 200),
        ];
        // 5350 total; 4500 forces the cheapest-to-lose out, in rank order.
        let o = enforce_budget(&tools, Some(4500));
        let names: Vec<&str> = o.dropped.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["agent.steer", "agent.spawn"], "{o:?}");
        assert!(o.projected_bytes <= 4500 && !o.over_budget);
        // A far smaller budget empties every droppable tool and keeps the
        // safety four; the outcome is typed, not a silent truncation.
        let o = enforce_budget(&tools, Some(100));
        assert!(o.over_budget, "{o:?}");
        assert_eq!(o.projected_bytes, 1000 + 600 + 500 + 400);
        let kept_safety = ["plan.update", "user.ask", "task.complete", "tool.search"];
        for s in kept_safety {
            assert!(!o.dropped.iter().any(|d| d.name == s), "{s} was dropped");
        }
        assert_eq!(o.dropped.len(), 6);
        assert_eq!(o.dropped[0].name, "agent.steer");
        assert_eq!(o.dropped.last().unwrap().name, "change.apply");
    }

    #[test]
    fn pinned_tools_are_kept_even_when_the_budget_cannot_hold_them() {
        let mut repair = c("repair.attempt", 800);
        repair.pinned = true;
        let o = enforce_budget(&[repair, c("fs.read", 300)], Some(500));
        assert!(o.over_budget);
        assert_eq!(o.dropped.len(), 1);
        assert_eq!(o.dropped[0].name, "fs.read");
    }

    #[test]
    fn activated_host_tools_drop_before_the_core_but_after_delegation() {
        let mut git = c("git.diff", 400);
        git.activated = true;
        let o = enforce_budget(
            &[
                git,
                c("fs.read", 300),
                c("agent.spawn", 400),
                c("shell.exec", 300),
            ],
            Some(700),
        );
        let names: Vec<&str> = o.dropped.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["agent.spawn", "git.diff"], "{o:?}");
    }

    #[test]
    fn a_signature_names_the_arguments_and_marks_the_optional_ones() {
        let schema = serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"limit":{"type":"integer"},"tags":{"type":"array","items":{"type":"string"}}},"required":["path"]});
        assert_eq!(
            compact_signature("fs.read", &schema),
            "tools.fs.read({limit?: integer, path: string, tags?: string[]})"
        );
    }

    #[test]
    fn the_measure_counts_name_description_and_schema() {
        let s = serde_json::json!({"type":"object"});
        assert_eq!(schema_bytes("a.b", "four", &s), 3 + 4 + s.to_string().len());
    }
}
