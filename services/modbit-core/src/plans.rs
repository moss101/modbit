//! Plan versions outside the transcript (REQ-EV-0118; docs/14 "Plan
//! contract", docs/19): every version the model or a person recorded is an
//! object on the log (`PlanRecorded` / `PlanRevised` → `plan_ref`), a
//! person's review lands as `PlanAnnotated` (and, when the plan was edited,
//! the `PlanRevised` before it), and each turn records the version it ran
//! under (`TurnPrepared.plan_version`), so a plan reviewed between runs is
//! the exact plan the resumed run executes.

use modbit_core_runtime::harness::Plan;
use modbit_domain::TaskId;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_event_store::EventStore;
use modbit_protocol::v1 as wire;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// Every plan version of a task with its annotations, and the versions
/// its turns have run under.
pub(crate) fn view(store: &EventStore, task_id: TaskId) -> wire::PlanView {
    let events = store
        .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    let mut versions: Vec<wire::PlanVersionView> = Vec::new();
    for e in &events {
        let p = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "PlanRecorded" | "PlanRevised" => {
                let plan_ref = p["plan_ref"].as_str().unwrap_or_default().to_owned();
                let plan = store
                    .objects()
                    .get(&plan_ref)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Plan>(&b).ok())
                    .unwrap_or_default();
                let provenance = match &e.envelope.actor {
                    Actor::User(_) => "user_review".to_owned(),
                    Actor::Agent(_) => "model".to_owned(),
                    other => format!("{other:?}").to_lowercase(),
                };
                versions.push(wire::PlanVersionView {
                    version: p["version"].as_u64().unwrap_or(0) as u32,
                    plan_ref,
                    outcome: plan.outcome,
                    expected_files: plan.expected_files,
                    verification: plan.verification,
                    protected_effects: plan.protected_effects,
                    steps_json: serde_json::to_string(&plan.steps).unwrap_or_default(),
                    provenance,
                    reason: p["reason"].as_str().unwrap_or_default().to_owned(),
                    offset: e.offset,
                    annotations: vec![],
                });
            }
            "PlanAnnotated" => {
                let version = p["version"].as_u64().unwrap_or(0) as u32;
                if let Some(v) = versions.iter_mut().rev().find(|v| v.version == version) {
                    v.annotations.push(wire::PlanAnnotationView {
                        note: p["note"].as_str().unwrap_or_default().to_owned(),
                        provenance: p["provenance"].as_str().unwrap_or_default().to_owned(),
                        offset: e.offset,
                    });
                }
            }
            _ => {}
        }
    }
    // The versions turns ran under, from every run's turns.
    let mut executed: Vec<u32> = Vec::new();
    if let Ok(session_events) = store.read_session(
        &events
            .first()
            .map(|e| e.envelope.session_id)
            .unwrap_or_default(),
        0,
        usize::MAX,
    ) {
        for e in session_events.iter().filter(|e| {
            e.envelope.task_id == Some(task_id) && e.envelope.event_type == "TurnPrepared"
        }) {
            let p = store.payload(&e.envelope).unwrap_or_default();
            executed.push(p["plan_version"].as_u64().unwrap_or(0) as u32);
        }
    }
    wire::PlanView {
        task_id: Some(crate::server::wire_id(task_id.as_bytes())),
        current_version: versions.last().map_or(0, |v| v.version),
        versions,
        executed_versions: executed,
    }
}

/// A person's revision and/or annotation. An edited plan becomes the next
/// version (a `PlanRevised` by the user, which the resumed run rebuilds as
/// its plan in force); the note lands as `PlanAnnotated` on that version.
///
/// # Errors
/// `BAD_PLAN` for an unparsable or invalid edited plan; `NO_PLAN` for an
/// annotation on a task that has no plan yet; `STORE` for an append failure.
pub(crate) fn revise(
    store: &mut EventStore,
    core: &Core,
    task: &Task,
    note: &str,
    plan_json: &str,
    provenance: &str,
) -> Result<(u32, String, u64), (String, String)> {
    let existing = view(store, task.task_id);
    let current = existing.versions.last().cloned();
    let actor = Actor::User(core.user_id);
    let lt = Lineage::task(core.tenant_id, task.session_id, task.task_id);
    let mut events = Vec::new();
    let (version, plan_ref) = if plan_json.is_empty() {
        let Some(cur) = current else {
            return Err((
                "NO_PLAN".into(),
                "the task has no plan to annotate yet".into(),
            ));
        };
        (cur.version, cur.plan_ref)
    } else {
        let mut plan: Plan = serde_json::from_str(plan_json)
            .map_err(|e| ("BAD_PLAN".to_owned(), format!("plan_json: {e}")))?;
        if plan.outcome.trim().is_empty() {
            return Err(("BAD_PLAN".into(), "a plan states its outcome".into()));
        }
        // The work graph must still be a whole (M6.1).
        let mut graph = modbit_domain::agent::WorkGraph {
            nodes: store.work_nodes(&task.task_id).unwrap_or_default(),
        };
        let version = current.as_ref().map_or(1, |c| c.version + 1);
        if !plan.steps.is_empty()
            && let Err(e) = graph.apply(task.task_id, version, &plan.steps)
        {
            return Err(("BAD_PLAN".into(), format!("steps: {e:?}")));
        }
        plan.version = version;
        let plan_ref = store
            .objects()
            .put(serde_json::to_vec(&plan).unwrap_or_default().as_slice())
            .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
        let prev_files: Vec<String> = current
            .as_ref()
            .map(|c| c.expected_files.clone())
            .unwrap_or_default();
        let added: Vec<String> = plan
            .expected_files
            .iter()
            .filter(|f| !prev_files.contains(f))
            .cloned()
            .collect();
        let removed: Vec<String> = prev_files
            .iter()
            .filter(|f| !plan.expected_files.contains(f))
            .cloned()
            .collect();
        events.push(if version == 1 {
            typed(
                "PlanRecorded",
                &TaskEvent::PlanRecorded {
                    plan_ref: plan_ref.clone(),
                    expected_files: plan.expected_files.clone(),
                    version,
                },
                actor.clone(),
            )
        } else {
            typed(
                "PlanRevised",
                &TaskEvent::PlanRevised {
                    plan_ref: plan_ref.clone(),
                    added,
                    removed,
                    reason: if note.is_empty() {
                        format!("revised by {provenance}")
                    } else {
                        note.to_owned()
                    },
                    version,
                },
                actor.clone(),
            )
        });
        (version, plan_ref)
    };
    if !note.is_empty() {
        events.push(typed(
            "PlanAnnotated",
            &TaskEvent::PlanAnnotated {
                version,
                plan_ref: plan_ref.clone(),
                note: note.to_owned(),
                provenance: provenance.to_owned(),
            },
            actor.clone(),
        ));
        // The note reaches the model on resume as a typed follow-up.
        events.push(typed(
            "TaskInputQueued",
            &TaskEvent::TaskInputQueued {
                input_id: format!("plan-note-v{version}-{}", modbit_domain::Timestamp::now().0),
                mode: modbit_domain::task::InputMode::FollowUp,
                text: format!("Plan review note on version {version}: {note}"),
            },
            actor,
        ));
    }
    let offset = append(
        store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        events,
    )
    .map_err(|e| ("STORE".to_owned(), e))?;
    Ok((version, plan_ref, offset))
}
