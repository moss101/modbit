//! Skills for a run (docs/16 "Skills", docs/26; M5.5, REQ-PX-052, REQ-PX-105):
//! discovered from the System scope, the workspace, the active extensions and
//! the profile, trusted by the owner's content-hash decision (or a signature
//! or the System scope's authority), selected explicitly or by trigger,
//! indexed under one aggregate token budget and read on demand with
//! `skill.load`. Nothing here grants a tool or a capability: a skill's
//! instructions are prompt text, its tool list is intersected with what the
//! node already projects, and what it asks for beyond that is told to the
//! model as unavailable.
//!
//! Trust is re-read at every round boundary, from the files on disk: a skill
//! the owner trusted a moment ago reaches the next request, one revoked or
//! edited by a single byte does not — including a project skill the agent
//! itself rewrote.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use modbit_core_runtime::harness::HarnessState;
use modbit_domain::TaskId;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_protocol::v1 as wire;
use modbit_skills::index::{IndexEntry, IndexForm};
use modbit_skills::{
    Lifecycle, RegisteredSkill, SelectionReason, SkillPolicy, SkillRegistry, SkillScope,
    SkillSource, TrustContext, TrustState,
};
use prost::Message;

use crate::runtime::{Lineage, append, typed};
use crate::server::{Core, accept, error_code, id16, reject};

/// Byte budget for one skill's injected instructions.
pub const INSTRUCTION_BUDGET_BYTES: usize = 8 * 1024;

/// The aggregate budget of the skill segment, in tokens: the injected
/// bodies of selected skills and the index of the rest together
/// (`MODBIT_SKILL_BUDGET_TOKENS` overrides it).
pub const DEFAULT_BUDGET_TOKENS: u32 = 2_000;

/// The share of the budget selected skills' bodies may take; the rest is
/// kept for the index, so a body never crowds out discovery.
const BODY_SHARE_PERCENT: u32 = 75;

/// The segment's budget in tokens.
#[must_use]
pub fn budget_tokens() -> u32 {
    std::env::var("MODBIT_SKILL_BUDGET_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_BUDGET_TOKENS)
}

/// Where the System scope's skills are read from: `MODBIT_SYSTEM_SKILLS`,
/// else a `skills` directory beside the device policy file (the machine's,
/// outside anything a user or a repository writes to).
#[must_use]
pub fn system_root() -> PathBuf {
    std::env::var("MODBIT_SYSTEM_SKILLS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            crate::config::device_policy_path()
                .parent()
                .map_or_else(|| PathBuf::from("skills"), |p| p.join("skills"))
        })
}

/// The profile's skill directory (user scope, the owner's trust and
/// revocation files).
#[must_use]
pub fn profile_root(data_dir: &Path) -> PathBuf {
    data_dir.join("skills")
}

/// The scopes a run discovers skills under, first (highest precedence)
/// first: the System scope, the workspace's `.modbit/skills`, an active
/// extension's (an imported one's among them), the profile's.
#[must_use]
pub fn sources(
    data_dir: &Path,
    workspace_root: Option<&str>,
    extension_dirs: Vec<PathBuf>,
) -> Vec<SkillSource> {
    let mut out = vec![SkillSource {
        root: system_root(),
        scope: SkillScope::System,
    }];
    if let Some(root) = workspace_root {
        out.push(SkillSource {
            root: PathBuf::from(root).join(".modbit").join("skills"),
            scope: SkillScope::Project,
        });
    }
    out.extend(extension_dirs.into_iter().map(|root| SkillSource {
        root,
        scope: SkillScope::Extension,
    }));
    out.push(SkillSource {
        root: profile_root(data_dir),
        scope: SkillScope::User,
    });
    out
}

/// The policy: signed skills are enabled; unsigned ones only by the
/// development flag (`MODBIT_SKILLS_ENABLE_INCUBATOR=1`), which a debug
/// build honours and a release build ignores — it is not a way a person
/// trusts a skill (`skill trust` is).
#[must_use]
pub fn policy() -> SkillPolicy {
    SkillPolicy {
        enable_signed: true,
        enable_incubator: cfg!(debug_assertions)
            && std::env::var("MODBIT_SKILLS_ENABLE_INCUBATOR")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
    }
}

/// Where the operator's revocations live: `<data dir>/skills/revoked.json`, a
/// list of `name` (every version) or `name@<content hash>` (EPR-013).
fn revocations(data_dir: &Path) -> BTreeSet<String> {
    std::fs::read(profile_root(data_dir).join("revoked.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// Everything the registry and the trust files say right now.
pub(crate) struct World {
    pub registry: SkillRegistry,
    pub revoked: BTreeSet<String>,
    /// Trust files that could not be read (nothing they held is trusted).
    pub problems: Vec<(String, String)>,
}

pub(crate) fn world(
    data_dir: &Path,
    workspace_root: Option<&str>,
    extension_dirs: Vec<PathBuf>,
) -> World {
    let keys = std::env::var("MODBIT_SKILL_KEYS")
        .map(|raw| modbit_skills::trusted_keys_from_env(&raw))
        .unwrap_or_default();
    let srcs = sources(data_dir, workspace_root, extension_dirs);
    let system = system_root();
    let (ctx, problems) = TrustContext::load(&profile_root(data_dir), Some(&system));
    World {
        registry: SkillRegistry::discover_scoped(&srcs, &keys, &policy(), &ctx),
        revoked: revocations(data_dir),
        problems,
    }
}

impl World {
    /// The state a client shows for a skill, with the operator's revocation
    /// folded in: a revoked skill is never enabled, whatever else says.
    pub(crate) fn effective(&self, s: &RegisteredSkill) -> (&'static str, bool) {
        let m = &s.package.manifest;
        if self.revoked.contains(&m.name)
            || self
                .revoked
                .contains(&format!("{}@{}", m.name, s.package.content_hash))
        {
            return ("REVOKED", false);
        }
        (s.trust.label(), s.lifecycle == Lifecycle::Enabled)
    }

    /// Whether the model may be told of, and load, this skill.
    pub(crate) fn model_loadable(&self, s: &RegisteredSkill) -> bool {
        self.effective(s).1 && s.package.manifest.model_invocable
    }
}

/// EPR-013: evaluation-qualified — a trusted signature over the content, a
/// PROMOTE evaluation of that exact content, and a package outside the
/// task's workspace (repository content is untrusted input: a skill there
/// could be handed an evaluation by the very agent it steers, which would
/// be self-promotion).
fn qualified(reg: &RegisteredSkill, task: &Task) -> bool {
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
    world: World,
    selected: Vec<(modbit_skills::Selection, bool)>,
    refused: Vec<(String, String, String)>,
}

pub(crate) fn choose(core: &Core, task: &Task, explicit: &[String]) -> Choice {
    choose_with(
        world(
            &core.data_dir,
            task.workspace_root.as_deref(),
            crate::extensions::active_dirs(core, task.session_id, "skills"),
        ),
        task,
        explicit,
        &[],
        false,
    )
}

/// `choose` over a world already read. `active` is the paths the task has
/// made active; `gate` says whether a skill's `paths` gate its selection by
/// trigger (the run's turns) or is left to the index (a preview).
fn choose_with(
    world: World,
    task: &Task,
    explicit: &[String],
    active: &[String],
    gate: bool,
) -> Choice {
    let (picked, rejected) = modbit_skills::select(&world.registry, &task.goal_text, explicit);
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
        let Some(reg) = world.registry.get(&sel.name) else {
            continue;
        };
        if world.effective(reg).0 == "REVOKED" {
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
        // A skill that is gated to paths is selected by trigger only while a
        // matching path is active; named explicitly, the person asked.
        if gate
            && matches!(sel.reason, SelectionReason::Trigger { .. })
            && !modbit_skills::paths_active(&reg.package.manifest, active)
        {
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
        world,
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

/// What one turn's skill segment is.
pub(crate) struct Plan {
    /// Injected bodies of selected skills, in selection order, with the
    /// tokens each takes.
    pub bodies: Vec<(String, String, u32)>,
    /// Selected skills whose body did not fit the budget share: they are in
    /// the index instead.
    pub demoted: Vec<String>,
    /// The index of the other skills the model may load.
    pub index: modbit_skills::index::SkillIndex,
    /// The tokens the bodies take in all.
    pub body_tokens: u32,
    /// Skills the model may load (the index's, and the selected ones').
    pub loadable: usize,
}

fn precedence(a: &RegisteredSkill, b: &RegisteredSkill) -> std::cmp::Ordering {
    (a.scope, &a.package.manifest.name).cmp(&(b.scope, &b.package.manifest.name))
}

/// Compile the selected bodies and build the index under the aggregate
/// budget. Pure over its inputs: the same skills, paths and budget give the
/// same text.
pub(crate) fn plan(c: &Choice, active: &[String], projection: &[String], budget: u32) -> Plan {
    let body_cap = budget * BODY_SHARE_PERCENT / 100;
    let mut bodies: Vec<(String, String, u32)> = Vec::new();
    let mut demoted: Vec<String> = Vec::new();
    let mut body_tokens = 0u32;
    for (sel, _) in &c.selected {
        let Some(reg) = c.world.registry.get(&sel.name) else {
            continue;
        };
        let compiled = modbit_skills::compile(&reg.package, projection, INSTRUCTION_BUDGET_BYTES);
        let tokens = modbit_skills::index::estimate_tokens(&compiled.instructions);
        if body_tokens + tokens > body_cap {
            demoted.push(sel.name.clone());
            continue;
        }
        body_tokens += tokens;
        bodies.push((sel.name.clone(), compiled.instructions, tokens));
    }
    let mut eligible: Vec<&RegisteredSkill> = c
        .world
        .registry
        .skills
        .iter()
        .filter(|s| c.world.model_loadable(s))
        .filter(|s| modbit_skills::paths_active(&s.package.manifest, active))
        .collect();
    eligible.sort_by(|a, b| precedence(a, b));
    let loadable = eligible.len();
    let entries: Vec<IndexEntry> = eligible
        .iter()
        .filter(|s| !bodies.iter().any(|(n, _, _)| *n == s.package.manifest.name))
        .map(|s| IndexEntry {
            name: s.package.manifest.name.clone(),
            description: s.package.manifest.description.clone(),
        })
        .collect();
    let index = modbit_skills::index::build(&entries, budget.saturating_sub(body_tokens));
    Plan {
        bodies,
        demoted,
        index,
        body_tokens,
        loadable,
    }
}

fn lock_key(c: &Choice, plan: &Plan) -> String {
    let mut parts: Vec<String> = c
        .selected
        .iter()
        .map(|(s, q)| format!("{}@{}:{q}", s.name, s.content_hash))
        .collect();
    parts.extend(
        c.refused
            .iter()
            .map(|(n, code, _)| format!("refused:{n}:{code}")),
    );
    parts.extend(
        c.world
            .registry
            .rejected
            .iter()
            .map(|(d, _)| format!("invalid:{d}")),
    );
    parts.extend(plan.demoted.iter().map(|d| format!("demoted:{d}")));
    parts.join("|")
}

/// The skills of a run, as each round of it sees them.
pub struct RunSkills {
    explicit: Vec<String>,
    last_selection: Option<String>,
    last_index: Option<String>,
}

impl RunSkills {
    /// A run that names `explicit` skills.
    #[must_use]
    pub fn new(explicit: Vec<String>) -> Self {
        Self {
            explicit,
            last_selection: None,
            last_index: None,
        }
    }

    /// The prompt texts for this round — the selected skills' bodies, then
    /// the index — recomputed from the files on disk so a trust decision, a
    /// revocation or an edit applies to the next request. A change of
    /// selection or index is recorded on the task. Sets whether the model
    /// has anything to load.
    pub async fn turn(
        &mut self,
        core: &Arc<Core>,
        task: &Task,
        lineage: Lineage,
        actor: &Actor,
        state: &mut HarnessState,
        projection: &[String],
    ) -> Vec<String> {
        let w = world(
            &core.data_dir,
            task.workspace_root.as_deref(),
            crate::extensions::active_dirs(core, task.session_id, "skills"),
        );
        if w.registry.skills.is_empty()
            && w.registry.rejected.is_empty()
            && self.explicit.is_empty()
        {
            state.skills_loadable = false;
            return vec![];
        }
        let active = crate::rules::active_paths(core, task, state).await;
        let c = choose_with(w, task, &self.explicit, &active, true);
        let p = plan(&c, &active, projection, budget_tokens());
        state.skills_loadable = p.loadable > 0;
        let key = lock_key(&c, &p);
        let index_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
            p.index.text.as_bytes(),
        ));
        let mut events = Vec::new();
        if self.last_selection.as_deref() != Some(key.as_str()) {
            // A package on disk that could not be loaded (REQ-EV-0114:
            // invalid metadata fails, visibly): recorded by its directory.
            for (dir, error) in &c.world.registry.rejected {
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
            for (sel, qualified) in &c.selected {
                let Some(reg) = c.world.registry.get(&sel.name) else {
                    continue;
                };
                if p.demoted.contains(&sel.name) {
                    continue;
                }
                let compiled =
                    modbit_skills::compile(&reg.package, projection, INSTRUCTION_BUDGET_BYTES);
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
            }
            for (name, code, reason) in &c.refused {
                events.push(typed(
                    "SkillRejected",
                    &TaskEvent::SkillRejected {
                        name: name.clone(),
                        code: code.clone(),
                        reason: reason.clone(),
                    },
                    actor.clone(),
                ));
            }
            for name in &p.demoted {
                events.push(typed(
                    "SkillRejected",
                    &TaskEvent::SkillRejected {
                        name: name.clone(),
                        code: "SKILL_BUDGET".into(),
                        reason: "its body did not fit the skill budget; it is listed in the index and the model can read it with skill.load".into(),
                    },
                    actor.clone(),
                ));
            }
            self.last_selection = Some(key);
        }
        if self.last_index.as_deref() != Some(index_hash.as_str()) && !p.index.text.is_empty() {
            events.push(typed(
                "SkillIndexRecorded",
                &TaskEvent::SkillIndexRecorded {
                    entries: p
                        .index
                        .forms
                        .iter()
                        .map(|(n, f)| format!("{n}:{}", f.label()))
                        .collect(),
                    omitted: p.index.omitted,
                    tokens: p.index.tokens,
                    budget_tokens: budget_tokens(),
                    body_tokens: p.body_tokens,
                    index_hash: index_hash.clone(),
                },
                actor.clone(),
            ));
            self.last_index = Some(index_hash);
        }
        if !events.is_empty() {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                core,
                lineage,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                events,
            );
        }
        let mut texts: Vec<String> = p.bodies.into_iter().map(|(_, text, _)| text).collect();
        if !p.index.text.is_empty() {
            texts.push(format!(
                "{}{}",
                modbit_prompt_compiler::SKILL_INDEX_PREFIX,
                p.index.text
            ));
        }
        texts
    }
}

// ---------------------------------------------------------------- skill.load

/// The port `skill.load` reads through: the registry as the files on disk
/// say right now, under the trust the owner has decided. A call re-reads it,
/// so a skill trusted a moment ago loads and one revoked a moment ago does
/// not.
pub(crate) struct SkillsPort {
    pub data_dir: PathBuf,
    pub workspace_root: Option<String>,
    pub extension_dirs: Vec<PathBuf>,
}

/// Most skills `skill.load` lists when asked for no name.
const MAX_LISTED: usize = 200;

impl modbit_tools::pipeline::SkillPort for SkillsPort {
    fn load(&self, args: &serde_json::Value) -> Result<serde_json::Value, (String, String)> {
        let w = world(
            &self.data_dir,
            self.workspace_root.as_deref(),
            self.extension_dirs.clone(),
        );
        let name = args["name"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        let Some(name) = name else {
            let mut listed: Vec<&RegisteredSkill> = w
                .registry
                .skills
                .iter()
                .filter(|s| w.model_loadable(s))
                .collect();
            listed.sort_by(|a, b| precedence(a, b));
            let total = listed.len();
            let skills: Vec<serde_json::Value> = listed
                .into_iter()
                .take(MAX_LISTED)
                .map(|s| {
                    serde_json::json!({
                        "name": s.package.manifest.name,
                        "description": s.package.manifest.description,
                        "scope": s.scope.label(),
                    })
                })
                .collect();
            return Ok(serde_json::json!({"skills": skills, "total": total}));
        };
        let Some(reg) = w.registry.get(name) else {
            return Err((
                "SKILL_UNKNOWN".into(),
                format!("no skill named `{name}`; skill.load with no name lists them"),
            ));
        };
        let (trust, enabled) = w.effective(reg);
        // A skill that is not enabled is not described: what an untrusted
        // package says is not the model's to read, and the reason is the
        // owner's to see (ListSkills), not text to hand a hostile package.
        if !enabled {
            return Err((
                "SKILL_NOT_ENABLED".into(),
                format!(
                    "skill `{name}` is not enabled for this profile; only the owner can change that"
                ),
            ));
        }
        if !reg.package.manifest.model_invocable {
            return Err((
                "SKILL_USER_ONLY".into(),
                format!("skill `{name}` is for the user to invoke; the model does not load it"),
            ));
        }
        let part = if let Some(p) = args["procedure"].as_str().filter(|p| !p.is_empty()) {
            modbit_skills::load::Part::Procedure(p.to_owned())
        } else if let Some(r) = args["resource"].as_str().filter(|r| !r.is_empty()) {
            modbit_skills::load::Part::Resource(r.to_owned())
        } else {
            modbit_skills::load::Part::Instructions
        };
        let offset = usize::try_from(args["offset"].as_u64().unwrap_or(0)).unwrap_or(0);
        let max =
            usize::try_from(args["max_bytes"].as_u64().unwrap_or(8 * 1024)).unwrap_or(8 * 1024);
        let loaded =
            modbit_skills::load::load_part(reg, reg.scope.label(), trust, &part, offset, max)
                .map_err(|e| {
                    let code = serde_json::to_value(&e)
                        .ok()
                        .and_then(|v| v["code"].as_str().map(str::to_owned))
                        .unwrap_or_else(|| "SKILL_LOAD_FAILED".into());
                    (code, e.to_string())
                })?;
        Ok(serde_json::to_value(&loaded).unwrap_or_default())
    }
}

// ------------------------------------------------- ListSkills / TrustSkill

/// The paths a task has made active, from its ledger alone (a preview has
/// no harness state to ask about the plan).
async fn ledger_paths(core: &Core, task: &Task) -> Vec<String> {
    let ledger = core.tools.ledger(&core.store, task.task_id).await;
    let ledger = ledger.lock().await;
    let mut paths: Vec<String> = ledger.reads.iter().map(|r| r.path.clone()).collect();
    paths.extend(ledger.entries.iter().map(|e| e.path.clone()));
    paths.sort();
    paths.dedup();
    paths
}

fn view_of(
    w: &World,
    s: &RegisteredSkill,
    index: Option<&Plan>,
    selected: bool,
    paths_ok: bool,
) -> wire::SkillView {
    let m = &s.package.manifest;
    let (trust, enabled) = w.effective(s);
    let (index_form, indexed, index_tokens) = match index {
        Some(p) => match p.index.forms.iter().find(|(n, _)| *n == m.name) {
            Some((_, f)) => (
                f.label().to_owned(),
                *f != IndexForm::Omitted,
                p.index
                    .line_tokens
                    .iter()
                    .find(|(n, _)| *n == m.name)
                    .map_or(0, |(_, t)| *t),
            ),
            None => (
                if p.bodies.iter().any(|(n, _, _)| *n == m.name) {
                    "BODY".to_owned()
                } else {
                    "NOT_INDEXED".to_owned()
                },
                false,
                p.bodies
                    .iter()
                    .find(|(n, _, _)| *n == m.name)
                    .map_or(0, |(_, _, t)| *t),
            ),
        },
        None => ("NOT_INDEXED".to_owned(), false, 0),
    };
    wire::SkillView {
        name: m.name.clone(),
        version: m.version.clone(),
        description: m.description.clone(),
        scope: s.scope.label().to_owned(),
        content_hash: s.package.content_hash.clone(),
        trust: trust.to_owned(),
        trust_detail: if enabled {
            String::new()
        } else if trust == "REVOKED" {
            "the operator revoked this skill (skill revoke)".to_owned()
        } else {
            s.note.clone()
        },
        enabled,
        invocation: if m.model_invocable {
            "BOTH"
        } else {
            "USER_ONLY"
        }
        .to_owned(),
        paths: m.paths.clone(),
        paths_active: paths_ok,
        index_tokens,
        indexed,
        index_form,
        selected,
        source: s.package.root.display().to_string(),
        provenance_source: m.provenance.source.clone(),
        provenance_author: m.provenance.author.clone(),
        provenance_license: m.provenance.license.clone(),
        required_tools: m.required_tools.clone(),
        lifecycle: format!("{:?}", s.lifecycle).to_uppercase(),
    }
}

/// The slash menu's typed union (REQ-PX-052, docs/65 AFW-D08) over the
/// registries that exist today: the skills just listed, the commands of the
/// task's session's loaded extensions (a quarantined extension's are listed
/// with their state and `enabled = false`), and the subagent profiles of the
/// project, the active extensions and the operator. Metadata only. Order:
/// built-in entries (System-scope skills) first, then everything else
/// alphabetically by what the menu shows (kind and id break ties); the
/// divider falls between them. Returns the entries, the index of the first
/// entry after the divider (0 = no divider) and the profile files that could
/// not be read.
async fn slash_inventory(
    core: &Core,
    task: Option<&Task>,
    skills: &[wire::SkillView],
) -> (Vec<wire::SlashEntry>, u32, Vec<wire::SkillRefusalView>) {
    use sha2::Digest;
    let mut entries: Vec<wire::SlashEntry> = skills
        .iter()
        .map(|v| wire::SlashEntry {
            kind: "SKILL".into(),
            id: v.name.clone(),
            display_name: v.name.clone(),
            description: v.description.clone(),
            scope: v.scope.clone(),
            trust: v.trust.clone(),
            trust_detail: v.trust_detail.clone(),
            enabled: v.enabled,
            invocation: v.invocation.clone(),
            built_in: v.scope == "SYSTEM",
            content_hash: v.content_hash.clone(),
            provenance_source: v.provenance_source.clone(),
            source: v.source.clone(),
        })
        .collect();
    // Commands: the session's extensions, as `run_command` sees them.
    if let Some(t) = task {
        let loaded = {
            let store = core.store.lock().await;
            core.tools.hooks.extensions_of(&store, t.session_id)
        };
        for ext in loaded {
            for c in &ext.manifest.commands {
                let id = format!("{}/{}", ext.manifest.name, c.name);
                entries.push(wire::SlashEntry {
                    kind: "COMMAND".into(),
                    display_name: id.clone(),
                    id,
                    description: c.description.clone(),
                    scope: "EXTENSION".into(),
                    trust: match &ext.quarantine {
                        Some(_) => "QUARANTINED".into(),
                        None => ext.signature.clone(),
                    },
                    trust_detail: ext.quarantine.clone().unwrap_or_default(),
                    enabled: ext.active(),
                    invocation: "USER_ONLY".into(),
                    built_in: false,
                    content_hash: ext.digest.clone(),
                    provenance_source: format!(
                        "extension:{}@{}",
                        ext.manifest.name, ext.manifest.version
                    ),
                    source: ext.path.clone(),
                });
            }
        }
    }
    // Subagent profiles: the roots a spawn reads, in its order (the first
    // root that has a name wins it).
    let mut roots: Vec<(std::path::PathBuf, &str)> = Vec::new();
    if let Some(t) = task {
        if let Some(root) = &t.workspace_root {
            roots.push((
                std::path::PathBuf::from(root)
                    .join(".modbit")
                    .join("agents"),
                "PROJECT",
            ));
        }
        for d in crate::extensions::active_dirs(core, t.session_id, "agents") {
            roots.push((d, "EXTENSION"));
        }
    }
    roots.push((core.data_dir.join("agents"), "USER"));
    let dirs: Vec<std::path::PathBuf> = roots.iter().map(|(d, _)| d.clone()).collect();
    let (profiles, bad) = modbit_domain::agent_profile::list(&dirs);
    for (p, path) in profiles {
        let scope = roots
            .iter()
            .find(|(d, _)| path.parent() == Some(d.as_path()))
            .map_or("USER", |(_, s)| *s);
        entries.push(wire::SlashEntry {
            kind: "SUBAGENT".into(),
            id: p.name.clone(),
            display_name: p.name.clone(),
            description: p.description.clone(),
            scope: scope.into(),
            trust: "PROFILE".into(),
            trust_detail: "a profile only ever narrows a child's tools; it grants nothing".into(),
            enabled: true,
            invocation: "MODEL_ONLY".into(),
            built_in: false,
            content_hash: std::fs::read(&path)
                .map(|b| hex::encode(sha2::Sha256::digest(b)))
                .unwrap_or_default(),
            provenance_source: p.source.clone(),
            source: path.display().to_string(),
        });
    }
    let rejected = bad
        .into_iter()
        .map(|(path, e)| wire::SkillRefusalView {
            source: path.display().to_string(),
            code: "INVALID_PROFILE".into(),
            reason: e.to_string(),
        })
        .collect();
    entries.sort_by(|a, b| {
        b.built_in
            .cmp(&a.built_in)
            .then_with(|| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            })
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });
    let built_in = entries.iter().take_while(|e| e.built_in).count();
    let divider = if built_in > 0 && built_in < entries.len() {
        u32::try_from(built_in).unwrap_or(u32::MAX)
    } else {
        0
    };
    (entries, divider, rejected)
}

/// `ListSkills`, `TrustSkill`, `UntrustSkill` (REQ-PX-052, REQ-PX-105).
pub(crate) async fn handle(
    core: &Core,
    cid: Option<wire::Id>,
    env: &wire::CommandEnvelope,
    _actor: Actor,
) -> wire::CommandAck {
    // The task whose project and extension skills are in view, when named.
    let task_of = |id: Option<wire::Id>| async move {
        let Some(id) = id.filter(|i| !i.value.is_empty()) else {
            return Ok::<Option<Task>, (String, String)>(None);
        };
        let Some(raw) = id16(&id) else {
            return Err(("BAD_PAYLOAD".into(), "task_id must be 16 bytes".into()));
        };
        let task_id = TaskId::from_bytes(raw);
        match core.store.lock().await.task(&task_id) {
            Ok(Some(t)) => Ok(Some(t)),
            Ok(None) => Err(("UNKNOWN_TASK".into(), task_id.to_string())),
            Err(e) => Err((error_code(&e).into(), e.to_string())),
        }
    };
    let world_of = |task: &Option<Task>| {
        world(
            &core.data_dir,
            task.as_ref().and_then(|t| t.workspace_root.as_deref()),
            task.as_ref()
                .map(|t| crate::extensions::active_dirs(core, t.session_id, "skills"))
                .unwrap_or_default(),
        )
    };
    match env.command_type.as_str() {
        "ListSkills" => {
            let Ok(p) = wire::ListSkills::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "ListSkills");
            };
            let task = match task_of(p.task_id.clone()).await {
                Ok(t) => t,
                Err((code, detail)) => return reject(cid, &code, detail),
            };
            // An inventory is a task's own: a command that names another
            // session than the task's is refused, not answered (QUAL-PX-052).
            if let (Some(t), Some(named)) = (&task, env.session_id.as_ref().and_then(id16))
                && named != *t.session_id.as_bytes()
            {
                return reject(cid, "WRONG_SESSION", "the task belongs to another session");
            }
            let active = match &task {
                Some(t) => ledger_paths(core, t).await,
                None => vec![],
            };
            let w = world_of(&task);
            let problems = w.problems.clone();
            let budget = budget_tokens();
            let mut skills: Vec<wire::SkillView> = Vec::new();
            let (plan_view, selected): (Option<Plan>, Vec<String>) = match &task {
                Some(t) => {
                    let c = choose_with(world_of(&task), t, &[], &active, true);
                    let selected = c.selected.iter().map(|(s, _)| s.name.clone()).collect();
                    (Some(plan(&c, &active, &[], budget)), selected)
                }
                None => {
                    // No task: the index of what the profile and the System
                    // scope carry, with nothing active and nothing selected.
                    let c = Choice {
                        world: world_of(&task),
                        selected: vec![],
                        refused: vec![],
                    };
                    (Some(plan(&c, &active, &[], budget)), vec![])
                }
            };
            let mut ordered: Vec<&RegisteredSkill> = w.registry.skills.iter().collect();
            ordered.sort_by(|a, b| precedence(a, b));
            for s in ordered {
                let paths_ok = if task.is_some() {
                    modbit_skills::paths_active(&s.package.manifest, &active)
                } else {
                    s.package.manifest.paths.is_empty()
                };
                skills.push(view_of(
                    &w,
                    s,
                    plan_view.as_ref(),
                    selected.contains(&s.package.manifest.name),
                    paths_ok,
                ));
            }
            let (used, omitted) = plan_view.as_ref().map_or((0, 0), |p| {
                (p.index.tokens + p.body_tokens, p.index.omitted)
            });
            let rejected: Vec<wire::SkillRefusalView> = w
                .registry
                .rejected
                .iter()
                .map(|(dir, e)| wire::SkillRefusalView {
                    source: dir.clone(),
                    code: serde_json::to_value(e)
                        .ok()
                        .and_then(|v| v["code"].as_str().map(str::to_owned))
                        .unwrap_or_else(|| "INVALID_PACKAGE".into()),
                    reason: e.to_string(),
                })
                .chain(
                    problems
                        .into_iter()
                        .map(|(source, reason)| wire::SkillRefusalView {
                            source,
                            code: "TRUST_FILE_UNREADABLE".into(),
                            reason,
                        }),
                )
                .collect();
            let (slash, slash_divider_at, bad_profiles) =
                slash_inventory(core, task.as_ref(), &skills).await;
            let mut rejected = rejected;
            rejected.extend(bad_profiles);
            accept(
                cid,
                false,
                wire::SkillList {
                    skills,
                    rejected,
                    index_budget_tokens: budget,
                    index_used_tokens: used,
                    index_omitted: omitted,
                    system_root: system_root().display().to_string(),
                    slash,
                    slash_divider_at,
                }
                .encode_to_vec(),
            )
        }
        "TrustSkill" => {
            let Ok(p) = wire::TrustSkill::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "TrustSkill");
            };
            let name = p.name.trim().to_owned();
            if name.is_empty() || !modbit_skills::trust::valid_hash(&p.content_hash) {
                return reject(
                    cid,
                    "BAD_PAYLOAD",
                    "a skill name and the 64-hex content hash the owner reviewed are required (skill trust <name>@<hash>)",
                );
            }
            let task = match task_of(p.task_id.clone()).await {
                Ok(t) => t,
                Err((code, detail)) => return reject(cid, &code, detail),
            };
            let w = world_of(&task);
            let Some(skill) = w.registry.get(&name) else {
                return reject(
                    cid,
                    "SKILL_UNKNOWN",
                    format!(
                        "no skill named `{name}` in the profile, the System scope or the task's scopes"
                    ),
                );
            };
            // The decision binds the bytes the owner read: the hash they
            // name must be the one the skill has now.
            if skill.package.content_hash != p.content_hash {
                return reject(
                    cid,
                    "HASH_MISMATCH",
                    format!(
                        "`{name}` is {} now, not {}; review the current content and trust that",
                        skill.package.content_hash, p.content_hash
                    ),
                );
            }
            if skill.trust == TrustState::Forbidden {
                return reject(
                    cid,
                    "SKILL_FORBIDDEN",
                    format!(
                        "the System scope forbids `{name}`; the owner's trust cannot override it"
                    ),
                );
            }
            if skill.scope == SkillScope::System {
                return reject(
                    cid,
                    "SKILL_NOT_TRUSTABLE",
                    format!(
                        "`{name}` is provisioned by the System scope; it needs no trust decision"
                    ),
                );
            }
            if w.effective(skill).0 == "REVOKED" {
                return reject(
                    cid,
                    "SKILL_REVOKED",
                    format!("`{name}` is revoked by the operator; remove the revocation first"),
                );
            }
            let now = modbit_domain::Timestamp::now().0;
            let records = match modbit_skills::trust::record_trust(
                &profile_root(&core.data_dir),
                &name,
                &p.content_hash,
                now,
                "owner-command",
            ) {
                Ok(n) => n,
                Err(e) => return reject(cid, "TRUST_WRITE_FAILED", e.to_string()),
            };
            let after = world_of(&task);
            let trust = after
                .registry
                .get(&name)
                .map_or("UNKNOWN", |s| after.effective(s).0);
            accept(
                cid,
                false,
                wire::SkillTrustResult {
                    name,
                    content_hash: p.content_hash,
                    trust: trust.to_owned(),
                    records: u32::try_from(records).unwrap_or(u32::MAX),
                }
                .encode_to_vec(),
            )
        }
        _ => {
            let Ok(p) = wire::UntrustSkill::decode(env.payload.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "UntrustSkill");
            };
            let name = p.name.trim().to_owned();
            if name.is_empty() {
                return reject(cid, "BAD_PAYLOAD", "a skill name is required");
            }
            let hash =
                (!p.content_hash.trim().is_empty()).then(|| p.content_hash.trim().to_owned());
            if let Err(e) = modbit_skills::trust::withdraw_trust(
                &profile_root(&core.data_dir),
                &name,
                hash.as_deref(),
            ) {
                return reject(cid, "TRUST_WRITE_FAILED", e.to_string());
            }
            let w = world_of(&None);
            let trust = w
                .registry
                .get(&name)
                .map_or("UNKNOWN", |s| w.effective(s).0);
            accept(
                cid,
                false,
                wire::SkillTrustResult {
                    name,
                    content_hash: hash.unwrap_or_default(),
                    trust: trust.to_owned(),
                    records: 0,
                }
                .encode_to_vec(),
            )
        }
    }
}
