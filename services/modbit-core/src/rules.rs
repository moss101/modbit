//! Workspace and user rules for a run (REQ-EV-0059, 0105, 0129): loaded
//! once per run from the workspace's `.modbit/rules/` and the profile's
//! `rules/`, selected every turn from the paths the task has made active —
//! read, retrieved, written or planned — and recorded on the task whenever
//! the selection changes, with every activation's reason, every conflict's
//! winner and source, and what expired or did not parse.

use std::sync::Arc;

use modbit_core_runtime::harness::HarnessState;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_prompt_compiler::rules::{Layer, RuleSet, RulesSelection};

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// The layers a run reads rules from, project first (it wins).
#[must_use]
pub fn layers(core: &Core, task: &Task) -> Vec<Layer> {
    let mut out = Vec::new();
    if let Some(root) = &task.workspace_root {
        out.push(Layer {
            name: "project".into(),
            dir: std::path::PathBuf::from(root).join(".modbit").join("rules"),
        });
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
        let ledger = core.tools.ledger(task.task_id).await;
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
}

impl RunRules {
    /// Load the layers.
    #[must_use]
    pub fn load(core: &Core, task: &Task) -> Self {
        Self {
            set: modbit_prompt_compiler::rules::load(&layers(core, task)),
            last: None,
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
        if self.set.rules.is_empty() && self.set.invalid.is_empty() {
            return vec![];
        }
        let paths = active_paths(core, task, state).await;
        let selection = self
            .set
            .select(&paths, modbit_domain::Timestamp::now().millis());
        if self.last.as_ref() != Some(&selection) {
            let mut store = core.store.lock().await;
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
                    },
                    actor.clone(),
                )],
            );
            self.last = Some(selection.clone());
        }
        self.set.texts(&selection)
    }
}
