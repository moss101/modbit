//! Factual assurance in the Core (REQ-EPR-008; docs/27 §9.3, docs/38
//! "DeriveRealizedRisk"): the policy the Core runs under — the base
//! envelope strengthened by an organization layer (`MODBIT_POLICY_FILE`)
//! and, per repository, by `.modbit/policy.json`, neither of which can
//! weaken it — and the facts of a candidate change at its COMPLETION run,
//! from which `modbit_policy::derive_realized_risk` decides how much
//! assurance acceptance needs. The result is persisted on the run
//! (`RealizedRiskDerived`) beside, and independent of, test correctness.

use std::path::Path;

use modbit_domain::approval::ApprovalState;
use modbit_domain::task::Task;
use modbit_event_store::EventStore;
use modbit_policy::{
    AssuranceLayer, AssurancePolicy, CandidateFacts, ChangeKind, ChangedPath, RealizedRisk,
    RequestedEffect,
};
use modbit_protocol::v1 as wire;
use modbit_verification::ChangedFile;

/// The organization's assurance layer, from the file `MODBIT_POLICY_FILE`
/// names; absent = the base policy alone. A file that does not parse is a
/// startup error: a policy that silently fell back would be weaker than
/// the operator believes.
pub(crate) fn org_policy() -> anyhow::Result<(AssurancePolicy, Vec<String>)> {
    let base = AssurancePolicy::default();
    let Ok(path) = std::env::var("MODBIT_POLICY_FILE") else {
        return Ok((base, vec![]));
    };
    if path.trim().is_empty() {
        return Ok((base, vec![]));
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| anyhow::anyhow!("reading MODBIT_POLICY_FILE `{path}`: {e}"))?;
    let layer: AssuranceLayer = serde_json::from_slice(&bytes).map_err(|e| {
        anyhow::anyhow!("MODBIT_POLICY_FILE `{path}` is not an assurance layer: {e}")
    })?;
    Ok(base.strengthen_with(&layer))
}

/// The policy a task's candidate is judged under: the Core's policy
/// strengthened by the repository's `.modbit/policy.json`, when present
/// and well-formed (a repository cannot weaken anything; a malformed file
/// is ignored and named).
pub(crate) fn policy_for(
    core_policy: &AssurancePolicy,
    root: &Path,
) -> (AssurancePolicy, Vec<String>) {
    let file = root.join(".modbit").join("policy.json");
    let Ok(bytes) = std::fs::read(&file) else {
        return (core_policy.clone(), vec![]);
    };
    match serde_json::from_slice::<AssuranceLayer>(&bytes) {
        Ok(layer) => {
            let (p, mut ignored) = core_policy.strengthen_with(&layer);
            for i in &mut ignored {
                *i = format!(".modbit/policy.json: {i}");
            }
            (p, ignored)
        }
        Err(e) => (
            core_policy.clone(),
            vec![format!(".modbit/policy.json ignored: {e}")],
        ),
    }
}

fn line_delta(old: Option<&str>, new: Option<&str>) -> (u32, u32) {
    match (old, new) {
        (None, Some(n)) => (n.lines().count() as u32, 0),
        (Some(o), None) => (0, o.lines().count() as u32),
        (Some(o), Some(n)) => {
            let diff = similar::TextDiff::from_lines(o, n);
            let mut added = 0u32;
            let mut removed = 0u32;
            for c in diff.iter_all_changes() {
                match c.tag() {
                    similar::ChangeTag::Insert => added += 1,
                    similar::ChangeTag::Delete => removed += 1,
                    similar::ChangeTag::Equal => {}
                }
            }
            (added, removed)
        }
        (None, None) => (0, 0),
    }
}

/// The facts of the candidate: what changed and by how much, what the
/// plan declared, which effects were requested on the task.
pub(crate) fn facts(
    store: &EventStore,
    task: &Task,
    files: &[ChangedFile],
    candidate_revision: u64,
    plan_write_set: Option<Vec<String>>,
) -> CandidateFacts {
    let changed = files
        .iter()
        .map(|f| {
            let (lines_added, lines_removed) = line_delta(f.old.as_deref(), f.new.as_deref());
            ChangedPath {
                path: f.path.clone(),
                change: match (&f.old, &f.new) {
                    (None, _) => ChangeKind::Created,
                    (_, None) => ChangeKind::Deleted,
                    _ => ChangeKind::Modified,
                },
                lines_added,
                lines_removed,
            }
        })
        .collect();
    let requested_effects = store
        .approvals_for_task(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .map(|a| RequestedEffect {
            tool: a.tool_name.clone(),
            effect_class: format!("{:?}", a.effect_class),
            capability: a.tool_name.clone(),
            approved: a.state == ApprovalState::Approved,
        })
        .collect();
    CandidateFacts {
        candidate_revision,
        changed,
        plan_write_set,
        requested_effects,
        advisory: Default::default(),
    }
}

/// The wire view of a risk record.
pub(crate) fn view(r: &RealizedRisk, realized_risk_ref: &str) -> wire::RealizedRiskView {
    wire::RealizedRiskView {
        realized_risk_ref: realized_risk_ref.to_owned(),
        schema_version: r.schema_version,
        rules_version: r.realized_risk_version.clone(),
        policy_version: r.policy_version.clone(),
        candidate_revision: r.candidate_revision,
        level: r.level.label().to_owned(),
        minimum_assurance: r.minimum_assurance.label().to_owned(),
        independent_review_required: r.independent_review_required,
        human_required: r.human_required,
        reasons: r
            .reasons
            .iter()
            .map(|x| wire::RiskReasonView {
                code: x.code.clone(),
                surface: x.surface.map(|s| s.label().to_owned()).unwrap_or_default(),
                level: x.level.label().to_owned(),
                paths: x.paths.clone(),
                detail: x.detail.clone(),
            })
            .collect(),
        forbidden_effects_requested: r.forbidden_effects_requested.clone(),
        evidence_refs: r.evidence_refs.clone(),
        facts_digest: r.facts_digest.clone(),
    }
}

/// The latest `RealizedRiskDerived` of a task, with its record.
pub(crate) fn latest(store: &EventStore, task: &Task) -> Option<(RealizedRisk, String, u64)> {
    let events = store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default();
    let ev = events.iter().rev().find(|e| {
        e.envelope.task_id == Some(task.task_id) && e.envelope.event_type == "RealizedRiskDerived"
    })?;
    let p = store.payload(&ev.envelope).ok()?;
    let r = p["realized_risk_ref"].as_str()?.to_owned();
    let bytes = store.objects().get(&r).ok()?;
    let risk: RealizedRisk = serde_json::from_slice(&bytes).ok()?;
    Some((risk, r, ev.offset))
}

/// A one-line summary for the model's observation.
pub(crate) fn summary(r: &RealizedRisk) -> String {
    let reasons: Vec<String> = r
        .reasons
        .iter()
        .map(|x| match x.surface {
            Some(s) => format!("{}:{} {}", x.code, s.label(), x.paths.join(",")),
            None => format!("{} {}", x.code, x.paths.join(",")),
        })
        .collect();
    format!(
        "realized_risk: {} (required assurance {}; independent review {}; human decision {}){}",
        r.level.label(),
        r.minimum_assurance.label(),
        if r.independent_review_required {
            "required"
        } else {
            "not required"
        },
        if r.human_required {
            "required"
        } else {
            "not required"
        },
        if reasons.is_empty() {
            String::new()
        } else {
            format!("; reasons: {}", reasons.join("; "))
        }
    )
}
