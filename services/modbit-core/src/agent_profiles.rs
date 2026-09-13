//! Agent profiles on this Core (REQ-EV-0115 / 0182 / 0241; docs/14 "Agent
//! profiles"): declarative files under `<data_dir>/agents/<name>.md` (the
//! operator's) and `<workspace>/.modbit/agents/<name>.md` (the project's,
//! shadowing), installed only after validation, compiled into a child's
//! capsule at admission — and only ever narrowing: the compiler drops and
//! names every tool the surface does not serve, every `agent.*` tool (a
//! child delegates nothing) and every tool above the child's ceiling.

use std::path::PathBuf;

use modbit_domain::agent_profile::{AgentProfile, ProfileError, parse_profile};
use modbit_domain::task::Task;
use modbit_domain::toolcall::EffectClass;

use crate::server::Core;

/// Where a run discovers profiles: the project's, then the operator's.
#[must_use]
pub fn roots(core: &Core, task: &Task) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(root) = &task.workspace_root {
        out.push(PathBuf::from(root).join(".modbit").join("agents"));
    }
    out.push(core.data_dir.join("agents"));
    out
}

/// Load a profile by name from the first root that has it.
///
/// # Errors
/// The file is unreadable or invalid (a profile on disk is re-validated on
/// every load: nothing edited in place widens anything).
pub fn load(roots: &[PathBuf], name: &str) -> Result<Option<AgentProfile>, ProfileError> {
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || name.is_empty()
    {
        return Err(ProfileError::BadName {
            name: name.to_owned(),
        });
    }
    for r in roots {
        let p = r.join(format!("{name}.md"));
        if let Ok(text) = std::fs::read_to_string(&p) {
            return parse_profile(&text).map(Some);
        }
    }
    Ok(None)
}

/// What a profile compiles to for one child: the tools it may see (never
/// more than the surface serves under the child's ceiling), and every tool
/// it asked for that was dropped, with why.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Compiled {
    /// Tools the child's projection is narrowed to.
    pub tools: Vec<String>,
    /// `name: why` for each dropped tool.
    pub narrowed: Vec<String>,
}

/// Compile a profile's tool request against the compiled surface for the
/// parent's execution profile and the child's effect ceiling.
#[must_use]
pub fn compile(
    core: &Core,
    parent: &Task,
    profile: &AgentProfile,
    ceiling: EffectClass,
) -> Compiled {
    let surface = core
        .tools
        .visible_specs(Some(&parent.execution_profile), None);
    let harness = [
        "plan.update",
        "task.complete",
        "verify.run",
        "user.ask",
        "fs.read",
    ];
    let mut out = Compiled::default();
    for t in &profile.tools {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("agent.") {
            out.narrowed
                .push(format!("{t}: a child delegates nothing (REQ-EV-0051)"));
            continue;
        }
        if harness.contains(&t) {
            out.tools.push(t.to_owned());
            continue;
        }
        match surface.iter().find(|s| s.name == t) {
            None => out.narrowed.push(format!(
                "{t}: not served by this Core's surface for `{}`",
                parent.execution_profile
            )),
            Some(s) if s.effect_class > ceiling => out.narrowed.push(format!(
                "{t}: {:?} is above the child's ceiling {ceiling:?}",
                s.effect_class
            )),
            Some(_) => out.tools.push(t.to_owned()),
        }
    }
    out
}
