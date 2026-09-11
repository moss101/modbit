//! The Acceptance Gate in the Core (REQ-EPR-017; docs/27 §9.4, docs/38
//! "EvaluateAcceptanceGate"): gathers the evidence on the log for a
//! candidate revision — the latest COMPLETION run and its checks, the diff
//! invariants, the review decisions, the policy-owned realized risk as the
//! obligation — and evaluates `modbit_verification::evaluate_gate`
//! independently of the risk classification. Evaluated at every COMPLETION
//! run and again at a review decision; every result is an object the run
//! names (`AcceptanceGateEvaluated`).

use modbit_domain::task::Task;
use modbit_event_store::EventStore;
use modbit_policy::AssuranceLevel;
use modbit_protocol::v1 as wire;
use modbit_verification::{
    AcceptanceGateResult, CheckEvidence, GateInput, InvariantEvidence, RequiredAssurance,
    ReviewEvidence, Verdict, VerificationEvidence, evaluate_gate,
};

/// The evidence on the log for a task, as the gate reads it.
pub(crate) struct Gathered {
    /// The latest COMPLETION run.
    pub verification: Option<VerificationEvidence>,
    /// DENY invariants and refs from that run.
    pub invariants: InvariantEvidence,
    /// Every review decision, with its offset as the reference.
    pub reviews: Vec<ReviewEvidence>,
    /// Plan and slot of the latest run.
    pub plan_id: String,
    pub leg_id: String,
}

fn rev_of(s: &str) -> u64 {
    s.trim_start_matches("ws-rev-").parse().unwrap_or(0)
}

/// Read the evidence off the log.
pub(crate) fn gather(store: &EventStore, task: &Task) -> Gathered {
    let events = store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default();
    let mut verification: Option<VerificationEvidence> = None;
    let mut invariants = InvariantEvidence::default();
    let mut reviews = Vec::new();
    let mut plan_id = String::new();
    let mut leg_id = String::new();
    let mut last_run_id: Option<modbit_domain::RunId> = None;
    for e in events
        .iter()
        .filter(|e| e.envelope.task_id == Some(task.task_id))
    {
        let p = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "VerificationRunRecorded" if p["stage"] == "COMPLETION" => {
                verification = Some(VerificationEvidence {
                    verification_run_id: p["verification_run_id"]
                        .as_str()
                        .unwrap_or_default()
                        .into(),
                    stage: "COMPLETION".into(),
                    candidate_revision: rev_of(
                        p["candidate_revision"].as_str().unwrap_or_default(),
                    ),
                    status: p["status"].as_str().unwrap_or_default().into(),
                    checks: p["checks"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|c| CheckEvidence {
                                    check_id: c["check_id"].as_str().unwrap_or_default().into(),
                                    kind: c["kind"].as_str().unwrap_or_default().into(),
                                    status: c["status"].as_str().unwrap_or_default().into(),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    report_refs: p["report_refs"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default(),
                    attribution: vec![],
                });
                // Invariants are re-read for this run.
                invariants = InvariantEvidence::default();
            }
            "RegressionAttributed" => {
                if let Some(v) = verification.as_mut()
                    && p["verification_run_id"]
                        == serde_json::Value::String(v.verification_run_id.clone())
                {
                    v.attribution.push((
                        p["check_id"].as_str().unwrap_or_default().into(),
                        p["attribution"].as_str().unwrap_or_default().into(),
                    ));
                }
            }
            "DiffInvariantViolated" if p["stage"] == "COMPLETION" => {
                if p["class"] == "DENY" {
                    invariants.deny = true;
                }
                invariants.refs.push(format!("offset:{}", e.offset));
            }
            "ReviewDecisionRecorded" => reviews.push(ReviewEvidence {
                decision: p["decision"].as_str().unwrap_or_default().into(),
                candidate_revision: p["candidate_revision"].as_u64().unwrap_or(0),
                provenance: p["provenance"].as_str().unwrap_or("user_review").into(),
                reference: format!("offset:{}", e.offset),
            }),
            "RoutingPlanAdmitted" => {
                plan_id = p["plan_id"].as_str().unwrap_or_default().into();
                last_run_id = e.envelope.run_id;
            }
            "SlotActivated" if e.envelope.run_id == last_run_id => {
                leg_id = p["slot_id"].as_str().unwrap_or_default().into();
            }
            _ => {}
        }
    }
    Gathered {
        verification,
        invariants,
        reviews,
        plan_id,
        leg_id,
    }
}

/// The obligation the gate must satisfy: the policy's minimum plus the
/// latest realized risk (docs/38 step 1).
pub(crate) fn required(
    policy: &modbit_policy::AssurancePolicy,
    risk: Option<&(modbit_policy::RealizedRisk, String, u64)>,
) -> RequiredAssurance {
    match risk {
        Some((r, rref, _)) => RequiredAssurance {
            level: policy.minimum_assurance.max(r.minimum_assurance),
            independent_review: r.independent_review_required,
            human: r.human_required,
            required_checks: policy.required_checks.clone(),
            realized_risk_ref: rref.clone(),
            risk_version: r.realized_risk_version.clone(),
            policy_version: r.policy_version.clone(),
            forbidden_effects_requested: r.forbidden_effects_requested.clone(),
        },
        None => RequiredAssurance {
            level: policy.minimum_assurance.max(AssuranceLevel::Standard),
            independent_review: false,
            human: false,
            required_checks: policy.required_checks.clone(),
            realized_risk_ref: String::new(),
            risk_version: modbit_policy::assurance::REALIZED_RISK_RULES_VERSION.into(),
            policy_version: policy.version(),
            forbidden_effects_requested: vec![],
        },
    }
}

/// Evaluate the gate for `candidate_revision` from what the log holds,
/// plus `extra_reviews` not yet on the log (a decision being recorded) and
/// `open_flags` the loop still holds. Returns the result and its object.
pub(crate) fn evaluate(
    store: &EventStore,
    task: &Task,
    policy: &modbit_policy::AssurancePolicy,
    candidate_revision: u64,
    open_flags: &[String],
    extra_reviews: &[ReviewEvidence],
) -> (AcceptanceGateResult, String) {
    let g = gather(store, task);
    let risk = crate::assurance::latest(store, task);
    let mut invariants = g.invariants;
    invariants.open_flags = open_flags.to_vec();
    let mut reviews = g.reviews;
    reviews.extend(extra_reviews.iter().cloned());
    let input = GateInput {
        plan_id: g.plan_id,
        leg_id: g.leg_id,
        candidate_revision,
        required: required(policy, risk.as_ref()),
        verification: g.verification,
        invariants,
        reviews,
    };
    let result = evaluate_gate(&input);
    let gate_ref = store
        .objects()
        .put(&serde_json::to_vec(&result).unwrap_or_default())
        .unwrap_or_default();
    (result, gate_ref)
}

/// The run event for a result.
pub(crate) fn event(
    r: &AcceptanceGateResult,
    gate_ref: &str,
    verification_run_id: &str,
    trigger: &str,
) -> modbit_domain::run::RunEvent {
    modbit_domain::run::RunEvent::AcceptanceGateEvaluated {
        verification_run_id: verification_run_id.to_owned(),
        gate_ref: gate_ref.to_owned(),
        gate_version: r.gate_version.clone(),
        candidate_revision: r.candidate_revision,
        verdict: r.verdict.label().into(),
        required_assurance: r.required_assurance.clone(),
        independent_review_required: r.independent_review_required,
        human_required: r.human_required,
        missing_evidence: r.missing_evidence.clone(),
        reject_reasons: r.reject_reasons.clone(),
        realized_risk_ref: r.realized_risk_ref.clone(),
        trigger: trigger.to_owned(),
    }
}

/// One line for the model.
pub(crate) fn summary(r: &AcceptanceGateResult) -> String {
    match r.verdict {
        Verdict::Accept => format!(
            "acceptance_gate: ACCEPT (required assurance {} satisfied by current evidence)",
            r.required_assurance
        ),
        Verdict::Reject => format!("acceptance_gate: REJECT ({})", r.reject_reasons.join("; ")),
        Verdict::Inconclusive => format!(
            "acceptance_gate: INCONCLUSIVE (required assurance {}; missing: {})",
            r.required_assurance,
            r.missing_evidence.join(", ")
        ),
    }
}

/// The latest gate result on the log for a task, with the offset.
pub(crate) fn latest(
    store: &EventStore,
    task: &Task,
) -> Option<(AcceptanceGateResult, String, u64, String)> {
    let events = store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default();
    let ev = events.iter().rev().find(|e| {
        e.envelope.task_id == Some(task.task_id)
            && e.envelope.event_type == "AcceptanceGateEvaluated"
    })?;
    let p = store.payload(&ev.envelope).ok()?;
    let r = p["gate_ref"].as_str()?.to_owned();
    let bytes = store.objects().get(&r).ok()?;
    let result: AcceptanceGateResult = serde_json::from_slice(&bytes).ok()?;
    let trigger = p["trigger"].as_str().unwrap_or_default().to_owned();
    Some((result, r, ev.offset, trigger))
}

/// Wire view.
pub(crate) fn view(
    r: &AcceptanceGateResult,
    gate_ref: &str,
    at_offset: u64,
    trigger: &str,
) -> wire::AcceptanceGateView {
    wire::AcceptanceGateView {
        gate_ref: gate_ref.to_owned(),
        gate_version: r.gate_version.clone(),
        candidate_revision: r.candidate_revision,
        verdict: r.verdict.label().into(),
        required_assurance: r.required_assurance.clone(),
        independent_review_required: r.independent_review_required,
        human_required: r.human_required,
        missing_evidence: r.missing_evidence.clone(),
        reject_reasons: r.reject_reasons.clone(),
        evidence: r
            .evidence
            .iter()
            .map(|e| wire::GateEvidenceView {
                kind: e.kind.clone(),
                status: format!("{:?}", e.status).to_uppercase(),
                required: e.required,
                detail: e.detail.clone(),
                refs: e.refs.clone(),
            })
            .collect(),
        evidence_refs: r.evidence_refs.clone(),
        realized_risk_ref: r.realized_risk_ref.clone(),
        policy_version: r.policy_version.clone(),
        risk_version: r.risk_version.clone(),
        plan_id: r.plan_id.clone(),
        leg_id: r.leg_id.clone(),
        trigger: trigger.to_owned(),
        at_offset,
    }
}
