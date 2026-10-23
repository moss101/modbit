//! Workspace and user rules for a run (REQ-EV-0059, 0105, 0129): loaded
//! once per run from the workspace's `.modbit/rules/` and the profile's
//! `rules/`, selected every turn from the paths the task has made active —
//! read, retrieved, written or planned — and recorded on the task whenever
//! the selection changes, with every activation's reason, every conflict's
//! winner and source, and what expired or did not parse.
//!
//! REQ-PX-107: the repository's own `AGENTS.md` and `CLAUDE.md` are a native
//! layer of the same selection (`modbit_prompt_compiler::instructions`):
//! read afresh every turn so an edit shows up in the next request and in its
//! recorded hash, in a trusted repository only. An untrusted repository's
//! files are listed as not loaded, with the reason, and nothing from them
//! reaches the prompt. The text is project instruction data: it goes into
//! the prompt's rules segment with its provenance, is run past the injection
//! scanner (a finding is a security record, not a policy decision) and
//! changes nothing the Capability Kernel decides — no tool is projected, no
//! approval is given, no permission is granted on its say-so.

use std::sync::Arc;

use modbit_core_runtime::harness::HarnessState;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_prompt_compiler::rules::{Layer, RuleSet, RulesSelection};

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// What a repository's rules wait for: they are instructions the repository
/// asks the Core to put in the model's prompt, as its hooks are code it asks
/// the Core to run, so they are in force only once the session trusts it.
const UNTRUSTED: &str = "a repository's rules are instructions it asks the Core to give the model: they are in force only once the session trusts the repository (TrustRepository)";

/// The same wait for the repository's own instruction files.
const UNTRUSTED_INSTRUCTIONS: &str = "not loaded: a repository's AGENTS.md / CLAUDE.md are instructions it asks the Core to give the model: they are in force only once the session trusts the repository (TrustRepository)";

/// The layers a run reads rules from, project first (it wins). The
/// project's are read only when the repository is `trusted` (FIX-04).
#[must_use]
pub fn layers(core: &Core, task: &Task, trusted: bool) -> Vec<Layer> {
    let mut out = Vec::new();
    if let Some(root) = task.workspace_root.as_ref().filter(|_| trusted) {
        out.push(Layer {
            name: "project".into(),
            dir: std::path::PathBuf::from(root).join(".modbit").join("rules"),
        });
    }
    // REQ-EV-0137/0183: an active extension's rules (an imported agent's
    // instructions among them) come after the project's, before the user's.
    for dir in crate::extensions::active_dirs(core, task.session_id, "rules") {
        let name = dir
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| format!("extension:{}", n.to_string_lossy()))
            .unwrap_or_else(|| "extension".into());
        out.push(Layer { name, dir });
    }
    out.push(Layer {
        name: "user".into(),
        dir: core.data_dir.join("rules"),
    });
    out
}

/// The paths a task has made active: read or retrieved (the context
/// ledger), written or planned (the harness state).
pub async fn active_paths(core: &Core, task: &Task, state: &HarnessState) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    {
        let ledger = core.tools.ledger(&core.store, task.task_id).await;
        let ledger = ledger.lock().await;
        paths.extend(ledger.reads.iter().map(|r| r.path.clone()));
        paths.extend(ledger.entries.iter().map(|e| e.path.clone()));
    }
    paths.extend(state.original_write_set.iter().cloned());
    paths.extend(state.out_of_plan_files.iter().cloned());
    if let Some(plan) = &state.plan {
        paths.extend(plan.expected_files.iter().cloned());
    }
    paths.sort();
    paths.dedup();
    paths
}

/// The rules of a run and the last selection recorded.
pub struct RunRules {
    set: RuleSet,
    last: Option<RulesSelection>,
    /// The workspace whose `AGENTS.md` / `CLAUDE.md` are read every turn:
    /// set only for a trusted repository.
    instructions_root: Option<std::path::PathBuf>,
    /// Instruction files whose scanner findings are already on the log, as
    /// `source@hash`: one security record per version of a file per run.
    reported: std::collections::BTreeSet<String>,
}

impl RunRules {
    /// Load the layers; `trusted` is whether the session trusts the task's
    /// repository (`onboarding::is_trusted`). Rules in an untrusted
    /// repository are not loaded and the refusal is recorded with the
    /// selection, where the other unusable rule files are.
    #[must_use]
    pub fn load(core: &Core, task: &Task, trusted: bool) -> Self {
        let mut set = modbit_prompt_compiler::rules::load(&layers(core, task, trusted));
        if !trusted && let Some(root) = &task.workspace_root {
            let dir = std::path::PathBuf::from(root).join(".modbit").join("rules");
            let has_rules = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .any(|e| e.path().extension().is_some_and(|x| x == "md"));
            if has_rules {
                set.invalid
                    .push((dir.display().to_string(), UNTRUSTED.into()));
            }
            // The repository's own instruction files wait for trust the same
            // way: named, never read into the prompt.
            for source in modbit_prompt_compiler::instructions::present(std::path::Path::new(root))
            {
                set.invalid.push((source, UNTRUSTED_INSTRUCTIONS.into()));
            }
        }
        let instructions_root = task
            .workspace_root
            .as_ref()
            .filter(|_| trusted)
            .map(std::path::PathBuf::from);
        Self {
            set,
            last: None,
            instructions_root,
            reported: std::collections::BTreeSet::new(),
        }
    }

    /// Select for this turn and record the selection when it changed;
    /// return the prompt texts.
    pub async fn select(
        &mut self,
        core: &Arc<Core>,
        task: &Task,
        lineage: Lineage,
        actor: &Actor,
        state: &HarnessState,
    ) -> Vec<String> {
        if self.set.rules.is_empty()
            && self.set.invalid.is_empty()
            && self.instructions_root.is_none()
        {
            return vec![];
        }
        let paths = active_paths(core, task, state).await;
        // The layers this turn: the rules loaded at the start, and the
        // repository's instruction files as they are on disk now.
        let mut set = self.set.clone();
        let mut findings: Vec<(String, String, Vec<modbit_browser::injection::Finding>)> =
            Vec::new();
        if let Some(root) = &self.instructions_root {
            let mut found = modbit_prompt_compiler::instructions::discover(root, &paths);
            for rule in &mut found.rules {
                let seen = modbit_browser::injection::scan(&rule.body);
                rule.findings = seen.iter().map(|f| f.shape.clone()).collect();
                if !seen.is_empty() {
                    findings.push((rule.source.clone(), rule.hash.clone(), seen));
                }
            }
            // Just below the project's `.modbit/rules`, above extensions and
            // the user's own.
            set.insert_layer(1, found.rules);
            set.invalid.extend(found.not_loaded);
        }
        let selection = set.select(&paths, modbit_domain::Timestamp::now().millis());
        // A finding is evidence, once per version of a file (the scanner
        // names shapes; the kernel, not the scanner, decides).
        findings.retain(|(source, hash, _)| self.reported.insert(format!("{source}@{hash}")));
        if findings.is_empty() && self.last.as_ref() == Some(&selection) {
            return set.texts(&selection);
        }
        let mut store = core.store.lock().await;
        for (source, hash, seen) in findings {
            let _ = append(
                &mut store,
                core,
                lineage,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "SecurityEventRecorded",
                    &TaskEvent::SecurityEventRecorded {
                        kind: "PROMPT_INJECTION_SUSPECTED".into(),
                        tool_name: "rules".into(),
                        tool_call_id: String::new(),
                        patterns: seen.iter().map(|f| f.shape.clone()).collect(),
                        detail: format!(
                            "{source} sha256:{}: {}",
                            &hash[..hash.len().min(12)],
                            seen.first().map(|f| f.excerpt.as_str()).unwrap_or_default()
                        ),
                        action: "MARKED".into(),
                    },
                    actor.clone(),
                )],
            );
        }
        if self.last.as_ref() != Some(&selection) {
            let _ = append(
                &mut store,
                core,
                lineage,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "RulesSelected",
                    &TaskEvent::RulesSelected {
                        active: selection
                            .active
                            .iter()
                            .map(|a| serde_json::to_value(a).unwrap_or_default())
                            .collect(),
                        dormant: selection.dormant.clone(),
                        expired: selection
                            .expired
                            .iter()
                            .map(|(id, src)| format!("{id}:{src}"))
                            .collect(),
                        conflicts: selection
                            .conflicts
                            .iter()
                            .map(|c| serde_json::to_value(c).unwrap_or_default())
                            .collect(),
                        invalid: selection
                            .invalid
                            .iter()
                            .map(|(src, why)| format!("{src}: {why}"))
                            .collect(),
                        not_loaded: selection
                            .invalid
                            .iter()
                            .map(|(src, why)| serde_json::json!({"source": src, "reason": why}))
                            .collect(),
                    },
                    actor.clone(),
                )],
            );
            self.last = Some(selection.clone());
        }
        drop(store);
        set.texts(&selection)
    }
}
