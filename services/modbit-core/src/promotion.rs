//! Policy promotion (REQ-EPR-012; docs/27 §12-14, docs/38
//! "PromoteRoutingPolicy", docs/61 "Thresholds, priors and independent gate
//! calibration"). A candidate registry generation carrying a `promotion`
//! becomes active only through this gate, which extends the one publisher
//! there is (`model_registry::activate`), and then only as a canary:
//!
//! 1. it replaces exactly the active generation — a compare-and-swap, so
//!    two promotions cannot both win and a stale one is refused;
//! 2. it is compatible: it joins the statistics version it was searched on;
//! 3. its exploration is bounded and never reaches critical or
//!    high-assurance requests without a policy that authorizes it (none
//!    does);
//! 4. the offline Policy Lab chose it: the search (`SearchPolicy`) is run
//!    again here, through the router's own compiler, on the statistics this
//!    Core materialized for the train and the untouched holdout partitions,
//!    and must reproduce the recorded report — feasible with confidence at
//!    the pinned target, confirmed on the holdout, choosing the floor the
//!    candidate carries;
//! 5. the independent gate calibration (EPR-019) is re-checked against the
//!    approved threshold profile it cites — no profile, an unapproved one,
//!    too few samples or an unsafe gate all block;
//! 6. its replays (EPR-011) were observed, from decisions whose propensity
//!    was recorded, and none regressed;
//! 7. it does not lower the quality floor it replaces;
//! 8. it names how many canary requests must be observed before it may
//!    become production (`canary_holds`, at `PromoteCanary`).
//!
//! Evidence enters by content reference into this Core's object store; the
//! registry itself holds no measurement.

use modbit_bench_gate_calibration::{Bundle, ThresholdProfile, release_check};
use modbit_bench_outcome_statistics::Snapshot;
use modbit_bench_policy_lab as lab;
use modbit_domain::{SessionId, TaskId};
use modbit_protocol::v1 as wire;
use modbit_providers::registry::{ModelRegistry, Promotion, RegistryDocument};

use crate::server::Core;

/// The most exploration a promotion may enable, in basis points.
const MAX_EXPLORATION_BP: u32 = 1_000;

/// Evidence kinds `RecordPromotionEvidence` accepts. The search report is
/// written only by `SearchPolicy`.
const KINDS: &[&str] = &["CALIBRATION", "THRESHOLD_PROFILE"];

type Refused = (&'static str, String);

/// Store one piece of promotion evidence, checked for its kind, and return
/// its content reference.
pub(crate) async fn record_evidence(
    core: &Core,
    p: &wire::RecordPromotionEvidence,
) -> Result<wire::PromotionEvidenceRecorded, (String, String)> {
    let bad = |d: String| ("BAD_EVIDENCE".to_owned(), d);
    match p.kind.as_str() {
        "CALIBRATION" => {
            serde_json::from_str::<Bundle>(&p.json)
                .map_err(|e| bad(format!("not a gate-calibration bundle: {e}")))?;
        }
        "THRESHOLD_PROFILE" => {
            serde_json::from_str::<ThresholdProfile>(&p.json)
                .map_err(|e| bad(format!("not a threshold profile: {e}")))?;
        }
        other => {
            return Err(bad(format!(
                "kind `{other}` is not one of {KINDS:?} (a search report comes from SearchPolicy)"
            )));
        }
    }
    let reference = core
        .store
        .lock()
        .await
        .objects()
        .put(p.json.as_bytes())
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    Ok(wire::PromotionEvidenceRecorded {
        kind: p.kind.clone(),
        evidence_ref: reference,
    })
}

/// What `SearchPolicy` stores: the report and the materialized snapshots it
/// was computed on, by session and reference, so the gate can run it again.
#[derive(serde::Serialize, serde::Deserialize)]
struct SearchEvidence {
    kind: String,
    report: lab::Report,
    train_session: String,
    train_snapshot_ref: String,
    holdout_session: String,
    holdout_snapshot_ref: String,
}

/// A snapshot this Core materialized on `session`'s log, by reference.
fn materialized(
    store: &modbit_event_store::EventStore,
    session: &SessionId,
    reference: &str,
) -> Option<Snapshot> {
    let events = store.read_session(session, 0, usize::MAX).ok()?;
    events.iter().find_map(|e| {
        if e.envelope.event_type != "OutcomeStatisticsMaterialized" {
            return None;
        }
        let p = store.payload(&e.envelope).ok()?;
        (p["snapshot_ref"].as_str() == Some(reference)).then_some(())?;
        serde_json::from_slice(&store.objects().get(reference).ok()?).ok()
    })
}

/// Compile the candidate with `floor` as its auto floor against the
/// snapshot's evidence, through the router's own compiler, for the spec's
/// representative request.
fn evaluate(
    core: &Core,
    candidate: &RegistryDocument,
    floor: f64,
    snapshot: &Snapshot,
    shape: &lab::RequestShape,
) -> lab::Evaluation {
    use modbit_providers::compiler::CompileInput;
    let refused = |code: &str| lab::Evaluation {
        floor,
        code: code.to_owned(),
        selected: vec![],
        selected_lcb: 0.0,
        expected_cost_minor: 0,
        target_met: false,
    };
    let mut document = candidate.clone();
    let Some(auto) = document
        .quality_floors
        .iter_mut()
        .find(|f| f.mode == "auto")
    else {
        return refused("NO_MODE_FLOOR");
    };
    auto.min_quality = floor;
    let mode_floor = auto.clone();
    // Evaluated on a partition, the candidate joins that partition's
    // statistics; which version it pins when signed is rule 2's question.
    document.stats_version.clone_from(&snapshot.stats_version);
    let registry = ModelRegistry {
        document,
        key_id: "policy-lab".into(),
        document_digest: String::new(),
    };
    let evidence = crate::routing::evidence_of(Some(snapshot));
    let thresholds = crate::routing::thresholds_for(&mode_floor, registry.generation(), 0);
    let cap = if shape.cap_minor == 0 {
        mode_floor.max_cost_minor
    } else {
        shape.cap_minor
    };
    let money = |m: u64| modbit_domain::routing::Money {
        minor_units: m,
        currency: mode_floor.currency.clone(),
        scale: mode_floor.scale,
    };
    let input = CompileInput {
        registry: &registry,
        evidence: &evidence,
        thresholds: &thresholds,
        // Offline: no session, task or run is created; fixed identifiers
        // keep the compile's input digest reproducible.
        scope: modbit_domain::routing::PlanScope {
            tenant_id: core.tenant_id,
            session_id: SessionId::from_bytes([0x5e; 16]),
            task_id: TaskId::from_bytes([0x7a; 16]),
            run_id: modbit_domain::RunId::from_bytes([0x2d; 16]),
            created_at_ms: 0,
        },
        lease_generation: 0,
        routing_epoch: 0,
        needs: crate::routing::needs(),
        execution_profile: if shape.execution_profile.is_empty() {
            "local_trusted".into()
        } else {
            shape.execution_profile.clone()
        },
        allowed_residencies: vec![],
        request_cap: money(cap),
        verification_reserve: money(cap / 10),
        assurance_available: shape.assurance_available,
        manual_pin: None,
        include_reviewer: shape.assurance_available,
        harness: crate::baseline::harness_version(),
        policy_version: core.gateway.policy().version(),
        profiler_version: "none".into(),
        gate_version: "gate-1".into(),
        risk_version: format!(
            "{}/{}",
            modbit_policy::assurance::REALIZED_RISK_RULES_VERSION,
            core.assurance_policy.version()
        ),
        expected_input_tokens: crate::routing::EXPECTED_INPUT_TOKENS,
        current_binding: None,
        allowed_models: None,
        skill_set: shape.skill_set.clone(),
    };
    match modbit_providers::compiler::compile(&input) {
        Ok(c) => {
            let chosen = c
                .selection
                .selected
                .as_ref()
                .and_then(|id| c.candidates.iter().find(|k| &k.plan_id == id));
            lab::Evaluation {
                floor,
                code: c.selection.code.clone(),
                selected: chosen.map(|k| k.bindings.clone()).unwrap_or_default(),
                selected_lcb: c.selection.selected_lcb,
                expected_cost_minor: chosen.map_or(0, |k| k.expected_cost_minor),
                target_met: c.selection.target_met,
            }
        }
        Err(r) => refused(r.code()),
    }
}

fn run_search(
    core: &Core,
    spec: &lab::Spec,
    candidate: &RegistryDocument,
    train: &Snapshot,
    holdout: &Snapshot,
) -> Result<lab::Report, lab::Refused> {
    lab::search(spec, train, holdout, |floor, s| {
        evaluate(core, candidate, floor, s, &spec.request)
    })
}

/// `SearchPolicy`: run the offline search on the statistics materialized in
/// the two sessions and store the report as promotion evidence.
pub(crate) async fn search(core: &Core, p: &wire::SearchPolicy) -> wire::PolicySearchView {
    let refuse = |code: &str, detail: String| wire::PolicySearchView {
        refusal_code: code.to_owned(),
        refusal_detail: detail,
        ..Default::default()
    };
    let spec: lab::Spec = match serde_json::from_str(&p.spec_json) {
        Ok(s) => s,
        Err(e) => return refuse("SEARCH_SPEC_INVALID", e.to_string()),
    };
    let candidate: RegistryDocument = match serde_json::from_str(&p.candidate_document_json) {
        Ok(d) => d,
        Err(e) => return refuse("REGISTRY_MALFORMED", e.to_string()),
    };
    if candidate.registry_generation != spec.candidate_generation {
        return refuse(
            "SEARCH_SPEC_INVALID",
            format!(
                "the spec searches `{}`, the document is `{}`",
                spec.candidate_generation, candidate.registry_generation
            ),
        );
    }
    let session = |id: Option<&wire::Id>| {
        id.and_then(|i| <[u8; 16]>::try_from(i.value.as_slice()).ok())
            .map(SessionId::from_bytes)
    };
    let (Some(train_session), Some(holdout_session)) = (
        session(p.train_session_id.as_ref()),
        session(p.holdout_session_id.as_ref()),
    ) else {
        return refuse(
            "BAD_PAYLOAD",
            "a search names its train and holdout sessions".into(),
        );
    };
    let tenant = core.tenant_id.to_string();
    let (train, train_ref, holdout, holdout_ref) = {
        let store = core.store.lock().await;
        let Some((train, train_ref)) = crate::statistics::latest_snapshot(&store, train_session)
        else {
            return refuse(
                "STATS_NOT_FOUND",
                format!("session {train_session} has no materialized statistics"),
            );
        };
        let Some((holdout, holdout_ref)) =
            crate::statistics::latest_snapshot(&store, holdout_session)
        else {
            return refuse(
                "STATS_NOT_FOUND",
                format!("session {holdout_session} has no materialized statistics"),
            );
        };
        (train, train_ref, holdout, holdout_ref)
    };
    if let Err(r) = train.for_tenant(&tenant) {
        return refuse(r.code(), format!("{r:?}"));
    }
    let report = match run_search(core, &spec, &candidate, &train, &holdout) {
        Ok(r) => r,
        Err(r) => return refuse(r.code(), r.detail().to_owned()),
    };
    let evidence = SearchEvidence {
        kind: "SEARCH_REPORT".into(),
        report: report.clone(),
        train_session: train_session.to_string(),
        train_snapshot_ref: train_ref,
        holdout_session: holdout_session.to_string(),
        holdout_snapshot_ref: holdout_ref,
    };
    let bytes = serde_json::to_vec(&evidence).unwrap_or_default();
    let report_ref = match core.store.lock().await.objects().put(&bytes) {
        Ok(r) => r,
        Err(e) => return refuse("STORE", e.to_string()),
    };
    wire::PolicySearchView {
        report_ref,
        verdict: report.verdict.clone(),
        chosen_floor: report.chosen.as_ref().map_or(0.0, |c| c.floor),
        report_json: serde_json::to_string(&report).unwrap_or_default(),
        refusal_code: String::new(),
        refusal_detail: String::new(),
    }
}

async fn object(core: &Core, reference: &str, what: &str) -> Result<Vec<u8>, Refused> {
    core.store
        .lock()
        .await
        .objects()
        .get(reference)
        .map_err(|_| {
            (
                "POLICY_EVIDENCE_MISSING",
                format!("the {what} `{reference}` is not in this Core's store"),
            )
        })
}

/// Rule 4: the search, run again, reproduces the report and chose this
/// candidate at this floor.
async fn search_holds(
    core: &Core,
    candidate: &ModelRegistry,
    reference: &str,
) -> Result<(), Refused> {
    let missing = |d: String| ("POLICY_EVIDENCE_MISSING", d);
    let evidence: SearchEvidence =
        serde_json::from_slice(&object(core, reference, "search report").await?)
            .ok()
            .filter(|e: &SearchEvidence| e.kind == "SEARCH_REPORT")
            .ok_or_else(|| {
                missing(format!(
                    "`{reference}` is not a search report SearchPolicy recorded"
                ))
            })?;
    let report = &evidence.report;
    if !lab::verify(report) {
        return Err(missing(
            "the search report does not verify against its digest".into(),
        ));
    }
    if report.spec.candidate_generation != candidate.generation() {
        return Err(missing(format!(
            "the search was for `{}`, not `{}`",
            report.spec.candidate_generation,
            candidate.generation()
        )));
    }
    if candidate.stats_version() != report.spec.train_stats_version {
        return Err((
            "REGISTRY_INCOMPATIBLE",
            format!(
                "the candidate joins statistics `{}` but it was searched on `{}`",
                candidate.stats_version(),
                report.spec.train_stats_version
            ),
        ));
    }
    let (train, holdout) = {
        let store = core.store.lock().await;
        let at = |sid: &str, r: &str| {
            SessionId::parse(sid)
                .ok()
                .and_then(|s| materialized(&store, &s, r))
        };
        (
            at(&evidence.train_session, &evidence.train_snapshot_ref),
            at(&evidence.holdout_session, &evidence.holdout_snapshot_ref),
        )
    };
    let (Some(train), Some(holdout)) = (train, holdout) else {
        return Err(missing(
            "the search's statistics are not snapshots this Core materialized".into(),
        ));
    };
    let again = run_search(core, &report.spec, &candidate.document, &train, &holdout)
        .map_err(|r| missing(format!("the search does not run again: {}", r.detail())))?;
    if again.digest != report.digest {
        return Err(missing(
            "the search, run again on the same statistics against this candidate, does not reproduce its report".into(),
        ));
    }
    match report.verdict.as_str() {
        lab::FEASIBLE => {}
        lab::HOLDOUT_REGRESSION => {
            return Err((
                "POLICY_QUALITY_REGRESSION",
                format!(
                    "the search's choice regressed on the holdout: {}",
                    report.reasons.join("; ")
                ),
            ));
        }
        other => {
            return Err(missing(format!(
                "the search found no promotable configuration ({other}): {}",
                report.reasons.join("; ")
            )));
        }
    }
    let chosen = report.chosen.as_ref().map(|c| c.floor);
    if chosen.is_none_or(|f| {
        candidate
            .auto_floor()
            .is_none_or(|g| f.total_cmp(&g).is_ne())
    }) {
        return Err(missing(format!(
            "the search chose auto floor {chosen:?}; the candidate carries {:?}",
            candidate.auto_floor()
        )));
    }
    Ok(())
}

/// The promotion gate. `Ok` only when every rule above holds.
pub(crate) async fn gate(
    core: &Core,
    candidate: &ModelRegistry,
    p: &Promotion,
    current: Option<&ModelRegistry>,
    expected_generation: &str,
) -> Result<(), Refused> {
    // 1. Compare-and-swap on the active generation.
    let Some(current) = current else {
        return Err((
            "REGISTRY_ACTIVATION_CONFLICT",
            "a promotion replaces an active generation; none is active".into(),
        ));
    };
    if expected_generation.is_empty()
        || expected_generation != current.generation()
        || p.previous_good_generation != current.generation()
    {
        return Err((
            "REGISTRY_ACTIVATION_CONFLICT",
            format!(
                "the promotion replaces `{}` and expects `{expected_generation}`, but `{}` is active",
                p.previous_good_generation,
                current.generation()
            ),
        ));
    }
    // 3. Exploration.
    if p.exploration_bp > 0 && p.exploration_critical {
        return Err((
            "UNSAFE_EXPLORATION",
            "exploration may not reach critical or high-assurance requests without a policy that authorizes it".into(),
        ));
    }
    if p.exploration_bp > MAX_EXPLORATION_BP {
        return Err((
            "UNSAFE_EXPLORATION",
            format!(
                "{} bp of exploration exceeds the bound of {MAX_EXPLORATION_BP} bp",
                p.exploration_bp
            ),
        ));
    }
    // 8. A canary to be observed.
    if p.min_canary_requests == 0 {
        return Err((
            "POLICY_EVIDENCE_MISSING",
            "a promotion names how many canary requests must be observed before it is production"
                .into(),
        ));
    }
    // 2 and 4. The search that chose it, on the statistics it joins.
    search_holds(core, candidate, &p.search_report_ref).await?;
    // 5. The calibration, re-checked against the approved profile.
    let bundle: Bundle =
        serde_json::from_slice(&object(core, &p.calibration_ref, "calibration bundle").await?)
            .map_err(|_| {
                (
                    "POLICY_EVIDENCE_MISSING",
                    "the calibration bundle is unreadable".to_owned(),
                )
            })?;
    let profile: ThresholdProfile =
        serde_json::from_slice(&object(core, &p.threshold_profile_ref, "threshold profile").await?)
            .map_err(|_| {
                (
                    "POLICY_EVIDENCE_MISSING",
                    "the threshold profile is unreadable".to_owned(),
                )
            })?;
    let check = release_check(
        &bundle.metrics,
        Some((&profile, &p.threshold_profile_ref)),
        0,
    );
    if check.verdict != "PASS" {
        let evidence_short = check.reasons.iter().any(|r| {
            r.starts_with("MISSING_")
                || r.starts_with("UNAPPROVED")
                || r.starts_with("INSUFFICIENT_SAMPLES")
        });
        return Err((
            if evidence_short {
                "POLICY_EVIDENCE_MISSING"
            } else {
                "POLICY_UNSAFE_GATE"
            },
            check.reasons.join("; "),
        ));
    }
    // 6. Replays: observed, with propensity recorded, none regressed.
    if p.replays.is_empty() {
        return Err((
            "POLICY_EVIDENCE_MISSING",
            "a promotion names the replays that observed its plans".into(),
        ));
    }
    for r in &p.replays {
        replay_holds(core, r).await?;
    }
    // 7. The quality floor it replaces.
    if let (Some(new), Some(old)) = (candidate.auto_floor(), current.auto_floor())
        && new < old
    {
        return Err((
            "POLICY_QUALITY_REGRESSION",
            format!("the candidate lowers the auto quality floor from {old} to {new}"),
        ));
    }
    Ok(())
}

/// The canary's own evidence (`PromoteCanary`): at least the number of
/// requests its promotion names, each routed under the canary generation
/// and observed to review. A request still running is missing evidence; one
/// that ended anywhere else is a regression.
pub(crate) async fn canary_holds(
    core: &Core,
    canary: &ModelRegistry,
    p: &Promotion,
    requests: &[String],
) -> Result<(), Refused> {
    let missing = |d: String| ("POLICY_EVIDENCE_MISSING", d);
    let mut distinct: Vec<&String> = requests.iter().collect();
    distinct.sort();
    distinct.dedup();
    if distinct.len() < p.min_canary_requests as usize {
        return Err(missing(format!(
            "{} canary request(s) observed; the promotion asks for {}",
            distinct.len(),
            p.min_canary_requests
        )));
    }
    let store = core.store.lock().await;
    for reference in distinct {
        let (sid, tid) = reference.split_once(':').ok_or_else(|| {
            missing(format!(
                "canary request `{reference}` is not `<session id>:<task id>`"
            ))
        })?;
        let (Ok(session), Ok(task_id)) = (SessionId::parse(sid), TaskId::parse(tid)) else {
            return Err(missing(format!(
                "canary request `{reference}` names no session and task"
            )));
        };
        let events = store
            .read_session(&session, 0, usize::MAX)
            .unwrap_or_default();
        let routed = events.iter().any(|e| {
            e.envelope.task_id == Some(task_id)
                && e.envelope.event_type == "RoutingPlanCompiled"
                && store.payload(&e.envelope).ok().is_some_and(|p| {
                    p["plan"]["provenance"]["registry_generation"].as_str()
                        == Some(canary.generation())
                })
        });
        if !routed {
            return Err(missing(format!(
                "canary request `{reference}` was not routed under `{}`",
                canary.generation()
            )));
        }
        use modbit_domain::task::TaskState;
        match store.task(&task_id).ok().flatten().map(|t| t.state) {
            Some(TaskState::ReadyForReview | TaskState::Completed) => {}
            Some(TaskState::Queued | TaskState::Running) => {
                return Err(missing(format!(
                    "canary request `{reference}` has not been observed to an end yet"
                )));
            }
            Some(other) => {
                return Err((
                    "POLICY_QUALITY_REGRESSION",
                    format!("canary request `{reference}` ended {other:?}, not in review"),
                ));
            }
            None => return Err(missing(format!("canary request `{reference}` is unknown"))),
        }
    }
    Ok(())
}

/// One replay (`<session id>:<replay id>`): recorded, from a decision whose
/// propensity was recorded, observed to an end, and not a regression.
async fn replay_holds(core: &Core, reference: &str) -> Result<(), Refused> {
    let missing = |d: String| ("POLICY_EVIDENCE_MISSING", d);
    let (sid, replay_id) = reference.split_once(':').ok_or_else(|| {
        missing(format!(
            "replay `{reference}` is not `<session id>:<replay id>`"
        ))
    })?;
    let session = SessionId::parse(sid)
        .map_err(|_| missing(format!("replay `{reference}` names no session")))?;
    let store = core.store.lock().await;
    let events = store
        .read_session(&session, 0, usize::MAX)
        .unwrap_or_default();
    let mut started: Option<(TaskId, TaskId)> = None;
    for e in &events {
        if e.envelope.event_type == "CounterfactualReplayStarted"
            && let Ok(p) = store.payload(&e.envelope)
            && p["replay_id"].as_str() == Some(replay_id)
            && let (Some(req), Ok(rt)) = (
                e.envelope.task_id,
                serde_json::from_value::<TaskId>(p["replay_task_id"].clone()),
            )
        {
            started = Some((req, rt));
        }
    }
    let (request, replay_task) = started.ok_or_else(|| {
        missing(format!(
            "replay `{replay_id}` is not on session {session}'s log"
        ))
    })?;
    // Propensity: the request's decisions recorded their choice probability.
    let decided = events.iter().any(|e| {
        e.envelope.task_id == Some(request)
            && e.envelope.event_type == "RoutingDecisionRecorded"
            && store
                .payload(&e.envelope)
                .ok()
                .and_then(|p| p["choice_probability_bp"].as_u64())
                .is_some_and(|bp| bp > 0)
    });
    if !decided {
        return Err(missing(format!(
            "the request replay `{replay_id}` came from recorded no choice probability (propensity)"
        )));
    }
    let state = store
        .task(&replay_task)
        .ok()
        .flatten()
        .map(|t| t.state)
        .ok_or_else(|| missing(format!("replay task of `{replay_id}` is unknown")))?;
    use modbit_domain::task::TaskState;
    match state {
        TaskState::ReadyForReview | TaskState::Completed => Ok(()),
        TaskState::Queued | TaskState::Running => Err(missing(format!(
            "replay `{replay_id}` has not been observed to an end yet"
        ))),
        other => Err((
            "POLICY_QUALITY_REGRESSION",
            format!("replay `{replay_id}` ended {other:?}: the replayed plan did not reach review"),
        )),
    }
}
