//! Materializing outcome statistics (REQ-EPR-015, docs/27 §21).
//!
//! The observations come from what the product already recorded: the baseline
//! bundles published under this session (REQ-EPR-000), each of which is
//! attributable to a build, a repository revision and an environment. Nothing
//! here is a second state owner — the snapshot is an artifact in the object
//! store and an event on the log, the same as the baselines it is derived
//! from — and nothing here is a prediction.

use modbit_bench_outcome_statistics as stats;
use modbit_domain::{SessionId, Timestamp};
use modbit_observability::baseline::BaselineBundle;
use modbit_protocol::v1 as wire;

use crate::server::Core;

/// Every baseline bundle published under a session, oldest first, with the
/// digests the events pinned them by.
async fn published_baselines(core: &Core, session_id: SessionId) -> Vec<BaselineBundle> {
    let store = core.store.lock().await;
    let mut out = Vec::new();
    let mut after = 0u64;
    loop {
        let Ok(batch) = store.read_session(&session_id, after, 512) else {
            break;
        };
        if batch.is_empty() {
            break;
        }
        for ev in &batch {
            after = ev.offset;
            if ev.envelope.event_type != "OutcomeBaselinePublished" {
                continue;
            }
            let Ok(payload) = store.payload(&ev.envelope) else {
                continue;
            };
            let Some(bundle_ref) = payload["bundle_ref"].as_str() else {
                continue;
            };
            if let Ok(bytes) = store.objects().get(bundle_ref)
                && let Ok(bundle) = serde_json::from_slice::<BaselineBundle>(&bytes)
            {
                out.push(bundle);
            }
        }
    }
    out
}

/// Materialize a snapshot for a session under a version.
pub(crate) async fn materialize(
    core: &Core,
    session_id: SessionId,
    stats_version: &str,
) -> wire::OutcomeStatisticsView {
    let refuse = |code: &str, detail: String| wire::OutcomeStatisticsView {
        materialized: false,
        refusal_code: code.to_owned(),
        refusal_detail: detail,
        ..Default::default()
    };
    if stats_version.trim().is_empty() {
        return refuse(
            "STATS_VERSION_REQUIRED",
            "a snapshot is pinned by its version, so it must have one".into(),
        );
    }
    let bundles = published_baselines(core, session_id).await;
    if bundles.is_empty() {
        return refuse(
            "STATS_NO_SOURCE",
            "no outcome baseline is published for this session; statistics are derived from what was observed, never invented".into(),
        );
    }
    let samples: Vec<stats::Sample> = bundles
        .iter()
        .flat_map(stats::samples_from_baseline)
        .collect();
    let last = bundles.last().expect("non-empty");
    let mut source_versions = vec![
        ("build".to_owned(), last.build_digest.clone()),
        (
            "repository_revision".to_owned(),
            last.repository_revision.clone(),
        ),
        ("environment".to_owned(), last.environment_digest.clone()),
    ];
    if let Some(registry) = core.gateway.registry() {
        source_versions.push((
            "registry_generation".to_owned(),
            registry.generation().to_owned(),
        ));
    }
    let snapshot = match stats::materialize(
        &stats::Materialization {
            stats_version,
            tenant_id: &core.tenant_id.to_string(),
            source_versions,
            created_at_ms: Timestamp::now().0,
            declared_samples: None,
        },
        &samples,
    ) {
        Ok(s) => s,
        Err(r) => return refuse(r.code(), format!("{r:?}")),
    };
    let mut store = core.store.lock().await;
    let snapshot_ref = match store
        .objects()
        .put(serde_json::to_vec(&snapshot).unwrap_or_default().as_slice())
    {
        Ok(r) => r,
        Err(e) => return refuse("OBJECT_STORE", e.to_string()),
    };
    let low = u32::try_from(
        snapshot
            .aggregates
            .iter()
            .filter(|a| a.low_confidence)
            .count(),
    )
    .unwrap_or(u32::MAX);
    let event = crate::runtime::typed(
        "OutcomeStatisticsMaterialized",
        &modbit_domain::session::SessionEvent::OutcomeStatisticsMaterialized {
            stats_version: snapshot.stats_version.clone(),
            snapshot_digest: snapshot.digest.clone(),
            snapshot_ref: snapshot_ref.clone(),
            source_digests: snapshot.source_digests.clone(),
            samples: snapshot.samples,
            keys: u32::try_from(snapshot.aggregates.len()).unwrap_or(u32::MAX),
            low_confidence_keys: low,
        },
        modbit_domain::event::Actor::Core("statistics".into()),
    );
    if let Err(e) = crate::runtime::append(
        &mut store,
        core,
        crate::runtime::Lineage::session(core.tenant_id, session_id),
        modbit_domain::event::AggregateType::Session,
        *session_id.as_bytes(),
        vec![event],
    ) {
        return refuse("STORE", e.to_string());
    }
    view(&snapshot, &snapshot_ref)
}

/// The most recent snapshot materialized under a session, with its object
/// ref, read from the log.
pub(crate) fn latest_snapshot(
    store: &modbit_event_store::EventStore,
    session_id: SessionId,
) -> Option<(stats::Snapshot, String)> {
    let mut found = None;
    let mut after = 0u64;
    loop {
        let Ok(batch) = store.read_session(&session_id, after, 512) else {
            break;
        };
        if batch.is_empty() {
            break;
        }
        for ev in &batch {
            after = ev.offset;
            if ev.envelope.event_type != "OutcomeStatisticsMaterialized" {
                continue;
            }
            let Ok(payload) = store.payload(&ev.envelope) else {
                continue;
            };
            let Some(snapshot_ref) = payload["snapshot_ref"].as_str() else {
                continue;
            };
            if let Ok(bytes) = store.objects().get(snapshot_ref)
                && let Ok(snapshot) = serde_json::from_slice::<stats::Snapshot>(&bytes)
            {
                found = Some((snapshot, snapshot_ref.to_owned()));
            }
        }
    }
    found
}

/// Load a snapshot back: the pinned version, or the most recent one.
pub(crate) async fn get(
    core: &Core,
    session_id: SessionId,
    stats_version: &str,
) -> wire::OutcomeStatisticsView {
    let store = core.store.lock().await;
    let Some((snapshot, snapshot_ref)) = latest_snapshot(&store, session_id) else {
        return wire::OutcomeStatisticsView {
            materialized: false,
            refusal_code: "STATS_NOT_FOUND".into(),
            refusal_detail: "no statistics snapshot is materialized for this session".into(),
            ..Default::default()
        };
    };
    // The reader joins on the pinned version and on the tenant, so a caller
    // cannot be handed observations it did not ask for.
    let tenant = core.tenant_id.to_string();
    let checked = if stats_version.trim().is_empty() {
        snapshot.for_tenant(&tenant)
    } else {
        snapshot
            .for_tenant(&tenant)
            .and_then(|s| s.joined(stats_version))
    };
    match checked {
        Ok(s) => view(s, &snapshot_ref),
        Err(r) => wire::OutcomeStatisticsView {
            materialized: false,
            refusal_code: r.code().to_owned(),
            refusal_detail: format!("{r:?}"),
            ..Default::default()
        },
    }
}

fn view(snapshot: &stats::Snapshot, snapshot_ref: &str) -> wire::OutcomeStatisticsView {
    wire::OutcomeStatisticsView {
        materialized: true,
        stats_version: snapshot.stats_version.clone(),
        snapshot_digest: snapshot.digest.clone(),
        snapshot_ref: snapshot_ref.to_owned(),
        source_digests: snapshot.source_digests.clone(),
        source_versions: snapshot
            .source_versions
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect(),
        samples: snapshot.samples,
        aggregates: snapshot
            .aggregates
            .iter()
            .map(|a| wire::StatAggregateView {
                key_id: a.key_id.clone(),
                samples: a.samples,
                successes: a.successes,
                mean: a.mean,
                interval_low: a.interval.0,
                interval_high: a.interval.1,
                mean_cost_minor: a.mean_cost_minor.unwrap_or_default(),
                cost_known: a.mean_cost_minor.is_some(),
                unknown_cost_samples: a.unknown_cost_samples,
                mean_wall_ms: a.mean_wall_ms,
                low_confidence: a.low_confidence,
            })
            .collect(),
        note: snapshot.note.clone(),
        refusal_code: String::new(),
        refusal_detail: String::new(),
    }
}
