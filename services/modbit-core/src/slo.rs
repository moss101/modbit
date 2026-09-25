//! The cloud SLO event ladder on the log (IMP-EV-0023, REQ-EV-0023; the
//! derivation is `modbit_observability::slo`). A `cloud_isolated` task's
//! start is recorded rung by rung as `SloStageRecorded`: the run asked for,
//! what a warm pool offered (none exists: `NONE`), the sandbox requested and
//! ready (warm when the task's own sandbox was reused), the model's first
//! output and the first tool dispatch of each run. Local tasks record
//! nothing: their start involves no sandbox, and their logs stay as they
//! were. An observation: nothing waits on it, and a failure to record it is
//! not a failure of the run.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// Whether this task's starts are on the ladder.
pub(crate) fn on_ladder(task: &Task) -> bool {
    task.execution_profile == modbit_policy::kernel::PROFILE_CLOUD_ISOLATED
}

/// Record one rung.
pub(crate) async fn record(
    core: &Core,
    task: &Task,
    actor: &Actor,
    stage: &str,
    run_id: Option<String>,
    warm: Option<bool>,
    detail: String,
) {
    if !on_ladder(task) {
        return;
    }
    let lt = Lineage::task(core.tenant_id, task.session_id, task.task_id);
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "SloStageRecorded",
            &TaskEvent::SloStageRecorded {
                stage: stage.to_owned(),
                at_ms: modbit_domain::Timestamp::now().0,
                run_id,
                warm,
                detail,
            },
            actor.clone(),
        )],
    );
}

/// Record a per-run rung (`FIRST_TOKEN`, `FIRST_TOOL`) once per run in this
/// process.
pub(crate) async fn first(
    core: &Core,
    task: &Task,
    actor: &Actor,
    run_id: &str,
    stage: &'static str,
    detail: String,
) {
    if !on_ladder(task) {
        return;
    }
    static SEEN: OnceLock<Mutex<HashSet<(String, &'static str)>>> = OnceLock::new();
    let fresh = SEEN
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .map(|mut s| s.insert((run_id.to_owned(), stage)))
        .unwrap_or(false);
    if fresh {
        record(
            core,
            task,
            actor,
            stage,
            Some(run_id.to_owned()),
            None,
            detail,
        )
        .await;
    }
}

/// The task's ladder, from its log.
pub(crate) async fn ladder(core: &Core, task: &Task) -> Vec<modbit_observability::slo::Stage> {
    let store = core.store.lock().await;
    store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| {
            e.envelope.task_id == Some(task.task_id) && e.envelope.event_type == "SloStageRecorded"
        })
        .filter_map(|e| {
            let p = store.payload(&e.envelope).ok()?;
            Some(modbit_observability::slo::Stage {
                stage: p["stage"].as_str()?.to_owned(),
                at_ms: p["at_ms"].as_i64()?,
                run_id: p["run_id"].as_str().map(str::to_owned),
                warm: p["warm"].as_bool(),
                detail: p["detail"].as_str().unwrap_or_default().to_owned(),
            })
        })
        .collect()
}
