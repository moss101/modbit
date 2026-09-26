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
    // REQ-EV-0137/0183: an active extension's skills (an imported one's
    // among them) come after the project's and before the operator's.
    out.extend(crate::extensions::active_dirs(
        core,
        task.session_id,
        "skills",
    ));
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

/// Where the operator's revocations live: `<data dir>/skills/revoked.json`, a
/// list of `name` (every version) or `name@<content hash>` (EPR-013).
fn revocations(core: &Core) -> std::collections::BTreeSet<String> {
    std::fs::read(core.data_dir.join("skills").join("revoked.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// EPR-013: evaluation-qualified — a trusted signature over the content, a
/// PROMOTE evaluation of that exact content, and a package outside the
/// task's workspace (repository content is untrusted input: a skill there
/// could be handed an evaluation by the very agent it steers, which would
/// be self-promotion).
fn qualified(reg: &modbit_skills::RegisteredSkill, task: &Task) -> bool {
    let evaluated = reg
        .evaluation
        .as_ref()
        .is_some_and(|e| e.disposition == "PROMOTE" && e.content_hash == reg.package.content_hash);
    let in_workspace = task.workspace_root.as_deref().is_some_and(|w| {
        let root = std::path::Path::new(w);
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let pkg = reg
            .package
            .root
            .canonicalize()
            .unwrap_or_else(|_| reg.package.root.clone());
        pkg.starts_with(root)
    });
    reg.attestation.is_some() && evaluated && !in_workspace
}

/// What a run of `task` selects, before anything is recorded: the registry,
/// the selected skills with their qualification, and every refusal as
/// `(name, code, reason)`. The same function answers the run (which records
/// it) and the compiler (which keys the request's statistics on it), so the
/// plan's skill set is the run's.
pub(crate) struct Choice {
    registry: SkillRegistry,
    selected: Vec<(modbit_skills::Selection, bool)>,
    refused: Vec<(String, String, String)>,
}

pub(crate) fn choose(core: &Core, task: &Task, explicit: &[String]) -> Choice {
    let trusted = std::env::var("MODBIT_SKILL_KEYS")
        .map(|raw| modbit_skills::trusted_keys_from_env(&raw))
        .unwrap_or_default();
    let registry = SkillRegistry::discover(&roots(core, task), &trusted, &policy());
    let (picked, rejected) = modbit_skills::select(&registry, &task.goal_text, explicit);
    let revoked = revocations(core);
    // A reviewer or a replay runs under a ceiling (EPR-018); a skill asking
    // for more than that ceiling grants is refused, not trimmed.
    let ceiling: Option<Vec<String>> = matches!(
        task.origin,
        modbit_domain::task::TaskOrigin::Review | modbit_domain::task::TaskOrigin::Replay
    )
    .then(|| {
        // The lease is `operation:resource`; the ceiling is its operations.
        let mut ops: Vec<String> = modbit_policy::kernel::default_lease_for_profile(
            &task.execution_profile,
            task.workspace_root.as_deref(),
        )
        .0
        .iter()
        .map(|g| g.split(':').next().unwrap_or_default().to_owned())
        .collect();
        ops.sort();
        ops.dedup();
        ops
    });
    let mut selected = Vec::new();
    let mut refused: Vec<(String, String, String)> = rejected
        .iter()
        .map(|r| {
            let code = serde_json::to_value(&r.error)
                .ok()
                .and_then(|v| v["code"].as_str().map(str::to_owned))
                .unwrap_or_else(|| "REFUSED".into());
            (r.name.clone(), code, r.error.to_string())
        })
        .collect();
    for sel in picked {
        let Some(reg) = registry.get(&sel.name) else {
            continue;
        };
        if revoked.contains(&sel.name)
            || revoked.contains(&format!("{}@{}", sel.name, sel.content_hash))
        {
            refused.push((
                sel.name.clone(),
                "SKILL_REVOKED".into(),
                format!(
                    "{}@{} is revoked by the operator; nothing it could reach changes",
                    sel.name, sel.content_hash
                ),
            ));
            continue;
        }
        if let Some(ops) = &ceiling {
            let beyond: Vec<&String> = reg
                .package
                .manifest
                .capability_ceiling
                .iter()
                .filter(|c| !ops.contains(c))
                .collect();
            if !beyond.is_empty() {
                refused.push((
                    sel.name.clone(),
                    "SKILL_EXCEEDS_REVIEWER_CEILING".into(),
                    format!(
                        "the skill asks for {beyond:?}; a {} task grants only {ops:?}",
                        task.execution_profile
                    ),
                ));
                continue;
            }
        }
        let q = qualified(reg, task);
        selected.push((sel, q));
    }
    Choice {
        registry,
        selected,
        refused,
    }
}

/// The skill set a run of `task` with `explicit` skills would select.
pub(crate) fn preview_set(core: &Core, task: &Task, explicit: &[String]) -> String {
    let c = choose(core, task, explicit);
    modbit_bench_outcome_statistics::skill_set(
        &c.selected
            .iter()
            .map(|(s, _)| (s.name.clone(), s.content_hash.clone()))
            .collect::<Vec<_>>(),
    )
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
    let Choice {
        registry,
        selected,
        refused,
    } = choose(core, task, explicit);
    if selected.is_empty() && refused.is_empty() && registry.rejected.is_empty() {
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
    for (sel, qualified) in &selected {
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
                qualified: *qualified,
            },
            actor.clone(),
        ));
        instructions.push(compiled.instructions);
    }
    for (name, code, reason) in refused {
        events.push(typed(
            "SkillRejected",
            &TaskEvent::SkillRejected { name, code, reason },
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
