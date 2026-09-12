//! Skills for a run (docs/16 "Skills", docs/26; M5.5): discovered from the
//! workspace and the profile, selected explicitly or by trigger, compiled
//! against the turn's projection, and recorded on the log. Nothing here
//! grants a tool or a capability: a skill's instructions are prompt text,
//! its tool list is intersected with what the node already projects, and
//! what it asks for beyond that is told to the model as unavailable.

use std::path::PathBuf;
use std::sync::Arc;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_skills::{SelectionReason, SkillPolicy, SkillRegistry};

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// Byte budget for one skill's injected instructions.
pub const INSTRUCTION_BUDGET_BYTES: usize = 8 * 1024;

/// The roots a run discovers skills under: the workspace's `.modbit/skills`
/// (project skills, shadowing) and the profile's `skills` (user skills).
#[must_use]
pub fn roots(core: &Core, task: &Task) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(root) = &task.workspace_root {
        out.push(PathBuf::from(root).join(".modbit").join("skills"));
    }
    out.push(core.data_dir.join("skills"));
    out
}

/// The policy: signed skills are enabled; unsigned ones only when the
/// operator says so (`MODBIT_SKILLS_ENABLE_INCUBATOR=1`, development).
#[must_use]
pub fn policy() -> SkillPolicy {
    SkillPolicy {
        enable_signed: true,
        enable_incubator: std::env::var("MODBIT_SKILLS_ENABLE_INCUBATOR")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
    }
}

/// Discover, select and compile the skills of a run; record every selection
/// and rejection; return the instructions to inject, in selection order.
pub async fn select_for_run(
    core: &Arc<Core>,
    task: &Task,
    lineage: Lineage,
    actor: &Actor,
    explicit: &[String],
    projection: &[String],
) -> Vec<String> {
    let trusted = std::env::var("MODBIT_SKILL_KEYS")
        .map(|raw| modbit_skills::trusted_keys_from_env(&raw))
        .unwrap_or_default();
    let registry = SkillRegistry::discover(&roots(core, task), &trusted, &policy());
    let (selected, rejected) = modbit_skills::select(&registry, &task.goal_text, explicit);
    if selected.is_empty() && rejected.is_empty() && registry.rejected.is_empty() {
        return vec![];
    }
    let mut events = Vec::new();
    // A package on disk that could not be loaded (REQ-EV-0114: invalid
    // metadata fails, visibly): recorded by its directory.
    for (dir, error) in &registry.rejected {
        let code = serde_json::to_value(error)
            .ok()
            .and_then(|v| v["code"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "INVALID_PACKAGE".into());
        events.push(typed(
            "SkillRejected",
            &TaskEvent::SkillRejected {
                name: dir.clone(),
                code,
                reason: error.to_string(),
            },
            actor.clone(),
        ));
    }
    let mut instructions = Vec::new();
    for sel in &selected {
        let Some(reg) = registry.get(&sel.name) else {
            continue;
        };
        let compiled = modbit_skills::compile(&reg.package, projection, INSTRUCTION_BUDGET_BYTES);
        events.push(typed(
            "SkillSelected",
            &TaskEvent::SkillSelected {
                name: compiled.name.clone(),
                version: compiled.version.clone(),
                content_hash: compiled.content_hash.clone(),
                lifecycle: format!("{:?}", reg.lifecycle).to_uppercase(),
                source: reg.package.root.display().to_string(),
                reason: match &sel.reason {
                    SelectionReason::Explicit => "EXPLICIT".to_owned(),
                    SelectionReason::Trigger { phrase } => format!("TRIGGER:{phrase}"),
                },
                instructions_hash: compiled.instructions_hash.clone(),
                instructions_truncated: compiled.instructions_truncated,
                tool_projection: compiled.tool_projection.clone(),
                tools_unavailable: compiled.tools_unavailable.clone(),
            },
            actor.clone(),
        ));
        instructions.push(compiled.instructions);
    }
    for rej in &rejected {
        let code = serde_json::to_value(&rej.error)
            .ok()
            .and_then(|v| v["code"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "REFUSED".into());
        events.push(typed(
            "SkillRejected",
            &TaskEvent::SkillRejected {
                name: rej.name.clone(),
                code,
                reason: rej.error.to_string(),
            },
            actor.clone(),
        ));
    }
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lineage,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        events,
    );
    instructions
}
