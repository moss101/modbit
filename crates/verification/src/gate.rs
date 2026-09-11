//! The Acceptance Gate (REQ-EPR-017; docs/27 §9.4, docs/38
//! "EvaluateAcceptanceGate"): does the current evidence satisfy the
//! required assurance? It is not the risk classifier — it consumes the
//! policy-owned `RealizedRisk` as an obligation and answers with
//! ACCEPT, REJECT or INCONCLUSIVE from what evidence exists at the exact
//! candidate revision. Static checks precede review; explicit failure
//! rejects; missing, stale, cancelled or timed-out mandatory evidence
//! cannot accept; correct tests never erase a review or human obligation.

use modbit_policy::AssuranceLevel;
use serde::{Deserialize, Serialize};

/// Gate schema version.
pub const GATE_SCHEMA_VERSION: u32 = 1;
/// Gate rules version; bumps when a rule changes.
pub const GATE_VERSION: &str = "gate-1";

/// The verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    /// Every required valid evidence exists and none failed.
    Accept,
    /// Explicit failed evidence.
    Reject,
    /// Mandatory evidence missing, stale, cancelled or timed out.
    Inconclusive,
}

impl Verdict {
    /// Stable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Accept => "ACCEPT",
            Self::Reject => "REJECT",
            Self::Inconclusive => "INCONCLUSIVE",
        }
    }
}

/// The state of one piece of evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceStatus {
    /// Present at the candidate revision and passed.
    Pass,
    /// Present and failed.
    Fail,
    /// Not there.
    Missing,
    /// Present, but for another revision.
    Stale,
    /// Present, but cancelled or timed out.
    Incomplete,
}

/// One piece of evidence the gate weighed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceItem {
    /// `build` | `typecheck` | `lint` | `tests` | `security` |
    /// `api_compatibility` | `invariants` | `effects` | `independent_review`
    /// | `human_decision` | `check:<id>`.
    pub kind: String,
    /// Status.
    pub status: EvidenceStatus,
    /// Whether the gate required it.
    pub required: bool,
    /// Refs (verification run ids, report objects, event offsets).
    pub refs: Vec<String>,
    /// Detail.
    pub detail: String,
}

/// What acceptance needs: the policy's minimum plus what the realized
/// risk added (docs/38: "PolicyEnvelope minima plus current RealizedRisk").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredAssurance {
    /// Level.
    pub level: AssuranceLevel,
    /// Independent review required.
    pub independent_review: bool,
    /// Human decision required.
    pub human: bool,
    /// Check ids the policy requires present and passing.
    pub required_checks: Vec<String>,
    /// The risk record.
    pub realized_risk_ref: String,
    /// Risk rules version.
    pub risk_version: String,
    /// Policy version.
    pub policy_version: String,
    /// Forbidden effects the candidate requested: never acceptable.
    pub forbidden_effects_requested: Vec<String>,
}

/// One check of the completion run, as evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckEvidence {
    /// Check id.
    pub check_id: String,
    /// `build` | `typecheck` | `lint` | `test` | `diagnostic` | `command` | ...
    pub kind: String,
    /// `PASS` | `FAIL` | `ERROR` | `TIMEOUT` | `SKIP` | `FLAKY` | `UNKNOWN`.
    pub status: String,
}

/// The completion run's evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationEvidence {
    /// Run id.
    pub verification_run_id: String,
    /// Stage (`COMPLETION` is the one that counts).
    pub stage: String,
    /// The revision it ran at.
    pub candidate_revision: u64,
    /// `PASSED` | `FAILED` | `CANCELLED` | `TIMED_OUT` | ...
    pub status: String,
    /// Checks.
    pub checks: Vec<CheckEvidence>,
    /// Report objects.
    pub report_refs: Vec<String>,
    /// Regression attribution per check (docs/64 §1): `REGRESSION` |
    /// `NEW_FAILING` | `KNOWN_FAILING` | `FLAKY` | `DECLARED_CHANGE` |
    /// `COLLATERAL_FIX` | `PASS`. A failure the baseline already had is
    /// not evidence against the candidate.
    #[serde(default)]
    pub attribution: Vec<(String, String)>,
}

/// The diff invariants' evidence at the completion run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantEvidence {
    /// A DENY-class violation stands.
    pub deny: bool,
    /// FLAG-class findings still unjustified.
    pub open_flags: Vec<String>,
    /// Refs.
    pub refs: Vec<String>,
}

/// A recorded review decision, as evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvidence {
    /// `ACCEPT` | `RETURN`.
    pub decision: String,
    /// The revision reviewed.
    pub candidate_revision: u64,
    /// `user_review` | `independent_reviewer`.
    pub provenance: String,
    /// Ref (event offset or record hash).
    pub reference: String,
}

/// Everything the gate looks at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateInput {
    /// Plan and leg the gate decides for (empty when the run has none).
    pub plan_id: String,
    /// Leg.
    pub leg_id: String,
    /// The candidate revision acceptance is asked about.
    pub candidate_revision: u64,
    /// Required assurance.
    pub required: RequiredAssurance,
    /// The latest COMPLETION run, when any.
    pub verification: Option<VerificationEvidence>,
    /// Invariants.
    pub invariants: InvariantEvidence,
    /// Review decisions on record.
    pub reviews: Vec<ReviewEvidence>,
}

/// The result (docs/27 §9.4 `AcceptanceGateResult`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceGateResult {
    /// Schema version.
    pub schema_version: u32,
    /// Gate rules version.
    pub gate_version: String,
    /// Plan.
    pub plan_id: String,
    /// Leg.
    pub leg_id: String,
    /// Candidate revision.
    pub candidate_revision: u64,
    /// The risk record consumed.
    pub realized_risk_ref: String,
    /// Required assurance level label.
    pub required_assurance: String,
    /// Independent review required.
    pub independent_review_required: bool,
    /// Human required.
    pub human_required: bool,
    /// Evidence weighed.
    pub evidence: Vec<EvidenceItem>,
    /// Verdict.
    pub verdict: Verdict,
    /// What is missing (kinds).
    pub missing_evidence: Vec<String>,
    /// Why it rejected, when it did.
    pub reject_reasons: Vec<String>,
    /// Every ref.
    pub evidence_refs: Vec<String>,
    /// Policy version.
    pub policy_version: String,
    /// Risk rules version.
    pub risk_version: String,
}

fn kind_of_check(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "build" => "build",
        "typecheck" => "typecheck",
        "lint" => "lint",
        "diagnostic" => "lint",
        "security" => "security",
        "api" | "api_compatibility" => "api_compatibility",
        _ => "tests",
    }
}

/// Evaluate the gate. Deterministic in its input.
#[must_use]
pub fn evaluate(input: &GateInput) -> AcceptanceGateResult {
    let mut evidence: Vec<EvidenceItem> = Vec::new();
    let mut refs: Vec<String> = Vec::new();
    let mut reject: Vec<String> = Vec::new();
    // 1. Static evidence: the COMPLETION run at this revision.
    match &input.verification {
        None => evidence.push(EvidenceItem {
            kind: "tests".into(),
            status: EvidenceStatus::Missing,
            required: true,
            refs: vec![],
            detail: "no COMPLETION verification run".into(),
        }),
        Some(v) if v.stage != "COMPLETION" => evidence.push(EvidenceItem {
            kind: "tests".into(),
            status: EvidenceStatus::Missing,
            required: true,
            refs: vec![v.verification_run_id.clone()],
            detail: format!("latest run is a {} stage, not COMPLETION", v.stage),
        }),
        Some(v) if v.candidate_revision != input.candidate_revision => {
            refs.push(v.verification_run_id.clone());
            evidence.push(EvidenceItem {
                kind: "tests".into(),
                status: EvidenceStatus::Stale,
                required: true,
                refs: vec![v.verification_run_id.clone()],
                detail: format!(
                    "COMPLETION run at revision {}, candidate is {}",
                    v.candidate_revision, input.candidate_revision
                ),
            });
        }
        Some(v) => {
            refs.push(v.verification_run_id.clone());
            refs.extend(v.report_refs.iter().cloned());
            let incomplete = matches!(
                v.status.as_str(),
                "CANCELLED" | "TIMED_OUT" | "INFRA_FAILURE"
            );
            // Per kind present in the run.
            let mut kinds: Vec<&'static str> =
                v.checks.iter().map(|c| kind_of_check(&c.kind)).collect();
            kinds.sort_unstable();
            kinds.dedup();
            if kinds.is_empty() {
                kinds.push("tests");
            }
            for k in kinds {
                let excused = |id: &str| {
                    v.attribution.iter().any(|(c, a)| {
                        c == id
                            && matches!(
                                a.as_str(),
                                "KNOWN_FAILING" | "FLAKY" | "DECLARED_CHANGE" | "COLLATERAL_FIX"
                            )
                    })
                };
                let failed: Vec<&CheckEvidence> = v
                    .checks
                    .iter()
                    .filter(|c| kind_of_check(&c.kind) == k)
                    .filter(|c| matches!(c.status.as_str(), "FAIL" | "ERROR" | "TIMEOUT"))
                    .filter(|c| !excused(&c.check_id))
                    .collect();
                let status = if incomplete {
                    EvidenceStatus::Incomplete
                } else if !failed.is_empty()
                    || (k == "tests" && v.status == "FAILED" && v.checks.is_empty())
                {
                    EvidenceStatus::Fail
                } else {
                    EvidenceStatus::Pass
                };
                if status == EvidenceStatus::Fail {
                    reject.push(format!(
                        "{k} failed: {}",
                        failed
                            .iter()
                            .map(|c| format!("{} {}", c.check_id, c.status))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                evidence.push(EvidenceItem {
                    kind: k.into(),
                    status,
                    required: true,
                    refs: vec![v.verification_run_id.clone()],
                    detail: format!("{} ({} check(s))", v.status, v.checks.len()),
                });
            }
            // Policy-required checks must be present and passing.
            for id in &input.required.required_checks {
                match v.checks.iter().find(|c| &c.check_id == id) {
                    Some(c) if c.status == "PASS" => evidence.push(EvidenceItem {
                        kind: format!("check:{id}"),
                        status: EvidenceStatus::Pass,
                        required: true,
                        refs: vec![v.verification_run_id.clone()],
                        detail: "required by policy".into(),
                    }),
                    Some(c) => {
                        reject.push(format!("required check {id} is {}", c.status));
                        evidence.push(EvidenceItem {
                            kind: format!("check:{id}"),
                            status: EvidenceStatus::Fail,
                            required: true,
                            refs: vec![v.verification_run_id.clone()],
                            detail: format!("required by policy; {}", c.status),
                        });
                    }
                    None => evidence.push(EvidenceItem {
                        kind: format!("check:{id}"),
                        status: EvidenceStatus::Missing,
                        required: true,
                        refs: vec![],
                        detail: "required by policy; not in the completion run".into(),
                    }),
                }
            }
        }
    }
    // 1b. The risk record itself: the gate consumes it; without it the
    // required assurance is a guess, and a guess cannot accept.
    if input.required.realized_risk_ref.is_empty() {
        evidence.push(EvidenceItem {
            kind: "realized_risk".into(),
            status: EvidenceStatus::Missing,
            required: true,
            refs: vec![],
            detail: "no RealizedRisk derived for this candidate".into(),
        });
    }
    // 2. Invariants and effects.
    refs.extend(input.invariants.refs.iter().cloned());
    if input.invariants.deny {
        reject.push("a DENY-class diff invariant stands".into());
        evidence.push(EvidenceItem {
            kind: "invariants".into(),
            status: EvidenceStatus::Fail,
            required: true,
            refs: input.invariants.refs.clone(),
            detail: "DENY-class violation".into(),
        });
    } else if !input.invariants.open_flags.is_empty() {
        evidence.push(EvidenceItem {
            kind: "invariants".into(),
            status: EvidenceStatus::Missing,
            required: true,
            refs: input.invariants.refs.clone(),
            detail: format!(
                "unjustified flags: {}",
                input.invariants.open_flags.join(", ")
            ),
        });
    } else {
        evidence.push(EvidenceItem {
            kind: "invariants".into(),
            status: EvidenceStatus::Pass,
            required: true,
            refs: input.invariants.refs.clone(),
            detail: "clean".into(),
        });
    }
    if !input.required.forbidden_effects_requested.is_empty() {
        reject.push(format!(
            "forbidden effect requested: {}",
            input.required.forbidden_effects_requested.join(", ")
        ));
        evidence.push(EvidenceItem {
            kind: "effects".into(),
            status: EvidenceStatus::Fail,
            required: true,
            refs: vec![input.required.realized_risk_ref.clone()],
            detail: "policy forbids the effect".into(),
        });
    }
    // 3. Obligations the risk imposes: a review or a human, at this revision.
    // A human's review is independent by definition; a reviewer's is not a
    // human's. Correct tests never discharge either.
    let at_revision: Vec<&ReviewEvidence> = input
        .reviews
        .iter()
        .filter(|r| r.candidate_revision == input.candidate_revision)
        .collect();
    let returned = at_revision.iter().any(|r| r.decision == "RETURN");
    let human_accept = at_revision
        .iter()
        .find(|r| r.decision == "ACCEPT" && r.provenance == "user_review");
    let reviewer_accept = at_revision.iter().find(|r| {
        r.decision == "ACCEPT"
            && (r.provenance == "independent_reviewer" || r.provenance == "user_review")
    });
    if input.required.independent_review {
        match reviewer_accept {
            Some(r) => evidence.push(EvidenceItem {
                kind: "independent_review".into(),
                status: EvidenceStatus::Pass,
                required: true,
                refs: vec![r.reference.clone()],
                detail: format!("{} at revision {}", r.provenance, r.candidate_revision),
            }),
            None if returned => {
                reject.push("the review returned the candidate".into());
                evidence.push(EvidenceItem {
                    kind: "independent_review".into(),
                    status: EvidenceStatus::Fail,
                    required: true,
                    refs: at_revision.iter().map(|r| r.reference.clone()).collect(),
                    detail: "RETURN".into(),
                });
            }
            None => evidence.push(EvidenceItem {
                kind: "independent_review".into(),
                status: EvidenceStatus::Missing,
                required: true,
                refs: vec![],
                detail: "no independent review at this revision".into(),
            }),
        }
    }
    if input.required.human {
        match human_accept {
            Some(r) => evidence.push(EvidenceItem {
                kind: "human_decision".into(),
                status: EvidenceStatus::Pass,
                required: true,
                refs: vec![r.reference.clone()],
                detail: format!("user review at revision {}", r.candidate_revision),
            }),
            None if returned => {
                if !reject.iter().any(|r| r.contains("returned")) {
                    reject.push("the review returned the candidate".into());
                }
                evidence.push(EvidenceItem {
                    kind: "human_decision".into(),
                    status: EvidenceStatus::Fail,
                    required: true,
                    refs: at_revision.iter().map(|r| r.reference.clone()).collect(),
                    detail: "RETURN".into(),
                });
            }
            None => evidence.push(EvidenceItem {
                kind: "human_decision".into(),
                status: EvidenceStatus::Missing,
                required: true,
                refs: vec![],
                detail: "no human decision at this revision".into(),
            }),
        }
    }
    let missing: Vec<String> = evidence
        .iter()
        .filter(|e| {
            e.required
                && matches!(
                    e.status,
                    EvidenceStatus::Missing | EvidenceStatus::Stale | EvidenceStatus::Incomplete
                )
        })
        .map(|e| e.kind.clone())
        .collect();
    let verdict = if !reject.is_empty() {
        Verdict::Reject
    } else if !missing.is_empty() {
        Verdict::Inconclusive
    } else {
        Verdict::Accept
    };
    for e in &evidence {
        refs.extend(e.refs.iter().cloned());
    }
    refs.push(input.required.realized_risk_ref.clone());
    refs.retain(|r| !r.is_empty());
    refs.sort();
    refs.dedup();
    AcceptanceGateResult {
        schema_version: GATE_SCHEMA_VERSION,
        gate_version: GATE_VERSION.into(),
        plan_id: input.plan_id.clone(),
        leg_id: input.leg_id.clone(),
        candidate_revision: input.candidate_revision,
        realized_risk_ref: input.required.realized_risk_ref.clone(),
        required_assurance: input.required.level.label().into(),
        independent_review_required: input.required.independent_review,
        human_required: input.required.human,
        evidence,
        verdict,
        missing_evidence: missing,
        reject_reasons: reject,
        evidence_refs: refs,
        policy_version: input.required.policy_version.clone(),
        risk_version: input.required.risk_version.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required(level: AssuranceLevel, review: bool, human: bool) -> RequiredAssurance {
        RequiredAssurance {
            level,
            independent_review: review,
            human,
            required_checks: vec![],
            realized_risk_ref: "r".repeat(64),
            risk_version: "risk-rules-1".into(),
            policy_version: "assurance-x".into(),
            forbidden_effects_requested: vec![],
        }
    }

    fn passed(rev: u64) -> VerificationEvidence {
        VerificationEvidence {
            verification_run_id: "vr-1".into(),
            stage: "COMPLETION".into(),
            candidate_revision: rev,
            status: "PASSED".into(),
            checks: vec![
                CheckEvidence {
                    check_id: "cargo_build".into(),
                    kind: "build".into(),
                    status: "PASS".into(),
                },
                CheckEvidence {
                    check_id: "t1".into(),
                    kind: "test".into(),
                    status: "PASS".into(),
                },
            ],
            report_refs: vec!["rep".repeat(20)],
            attribution: vec![],
        }
    }

    fn input(rev: u64, req: RequiredAssurance, v: Option<VerificationEvidence>) -> GateInput {
        GateInput {
            plan_id: "p".into(),
            leg_id: "initial".into(),
            candidate_revision: rev,
            required: req,
            verification: v,
            invariants: InvariantEvidence::default(),
            reviews: vec![],
        }
    }

    #[test]
    fn complete_current_evidence_accepts_a_standard_candidate() {
        let g = evaluate(&input(
            3,
            required(AssuranceLevel::Standard, false, false),
            Some(passed(3)),
        ));
        assert_eq!(g.verdict, Verdict::Accept, "{g:?}");
        assert!(g.missing_evidence.is_empty());
        assert!(
            g.evidence
                .iter()
                .any(|e| e.kind == "build" && e.status == EvidenceStatus::Pass)
        );
        assert!(
            g.evidence
                .iter()
                .any(|e| e.kind == "tests" && e.status == EvidenceStatus::Pass)
        );
    }

    #[test]
    fn passing_tests_never_erase_a_review_or_human_obligation() {
        let g = evaluate(&input(
            3,
            required(AssuranceLevel::HighAssurance, true, true),
            Some(passed(3)),
        ));
        assert_eq!(g.verdict, Verdict::Inconclusive, "{g:?}");
        assert_eq!(
            g.missing_evidence,
            vec!["independent_review", "human_decision"]
        );
        // A human's accept at the revision discharges both.
        let mut i = input(
            3,
            required(AssuranceLevel::HighAssurance, true, true),
            Some(passed(3)),
        );
        i.reviews.push(ReviewEvidence {
            decision: "ACCEPT".into(),
            candidate_revision: 3,
            provenance: "user_review".into(),
            reference: "offset:90".into(),
        });
        assert_eq!(evaluate(&i).verdict, Verdict::Accept);
        // A reviewer's accept discharges review, not the human.
        i.reviews[0].provenance = "independent_reviewer".into();
        let g = evaluate(&i);
        assert_eq!(g.verdict, Verdict::Inconclusive);
        assert_eq!(g.missing_evidence, vec!["human_decision"]);
    }

    #[test]
    fn deterministic_failure_rejects_and_precedes_review() {
        let mut v = passed(3);
        v.status = "FAILED".into();
        v.checks[1].status = "FAIL".into();
        let mut i = input(
            3,
            required(AssuranceLevel::HighAssurance, true, true),
            Some(v),
        );
        i.reviews.push(ReviewEvidence {
            decision: "ACCEPT".into(),
            candidate_revision: 3,
            provenance: "user_review".into(),
            reference: "offset:90".into(),
        });
        let g = evaluate(&i);
        assert_eq!(g.verdict, Verdict::Reject, "{g:?}");
        assert!(g.reject_reasons[0].contains("tests failed"));
    }

    #[test]
    fn stale_missing_or_incomplete_evidence_cannot_accept() {
        let stale = evaluate(&input(
            4,
            required(AssuranceLevel::Standard, false, false),
            Some(passed(3)),
        ));
        assert_eq!(stale.verdict, Verdict::Inconclusive);
        assert!(
            stale
                .evidence
                .iter()
                .any(|e| e.status == EvidenceStatus::Stale)
        );
        let none = evaluate(&input(
            4,
            required(AssuranceLevel::Standard, false, false),
            None,
        ));
        assert_eq!(none.verdict, Verdict::Inconclusive);
        assert_eq!(none.missing_evidence, vec!["tests"]);
        let mut v = passed(4);
        v.status = "TIMED_OUT".into();
        let timed = evaluate(&input(
            4,
            required(AssuranceLevel::Standard, false, false),
            Some(v),
        ));
        assert_eq!(timed.verdict, Verdict::Inconclusive);
        assert!(
            timed
                .evidence
                .iter()
                .any(|e| e.status == EvidenceStatus::Incomplete)
        );
    }

    #[test]
    fn forbidden_effects_deny_invariants_and_returns_reject() {
        let mut req = required(AssuranceLevel::Standard, false, false);
        req.forbidden_effects_requested = vec!["deploy".into()];
        let g = evaluate(&input(3, req, Some(passed(3))));
        assert_eq!(g.verdict, Verdict::Reject);
        let mut i = input(
            3,
            required(AssuranceLevel::Standard, false, false),
            Some(passed(3)),
        );
        i.invariants.deny = true;
        assert_eq!(evaluate(&i).verdict, Verdict::Reject);
        let mut i = input(
            3,
            required(AssuranceLevel::Governed, true, false),
            Some(passed(3)),
        );
        i.reviews.push(ReviewEvidence {
            decision: "RETURN".into(),
            candidate_revision: 3,
            provenance: "user_review".into(),
            reference: "offset:91".into(),
        });
        assert_eq!(evaluate(&i).verdict, Verdict::Reject);
    }

    #[test]
    fn a_policy_required_check_must_be_present_and_passing() {
        let mut req = required(AssuranceLevel::Standard, false, false);
        req.required_checks = vec!["security_scan".into()];
        let g = evaluate(&input(3, req.clone(), Some(passed(3))));
        assert_eq!(g.verdict, Verdict::Inconclusive);
        assert_eq!(g.missing_evidence, vec!["check:security_scan"]);
        let mut v = passed(3);
        v.checks.push(CheckEvidence {
            check_id: "security_scan".into(),
            kind: "security".into(),
            status: "PASS".into(),
        });
        assert_eq!(evaluate(&input(3, req, Some(v))).verdict, Verdict::Accept);
    }
}
