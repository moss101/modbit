//! REQ-EPR-015 / EPR-FI-015: what a statistics snapshot counts, what it
//! refuses, and what it will not let a caller conclude.

use modbit_bench_outcome_statistics::{
    MIN_CONFIDENT_SAMPLES, Materialization, Sample, StatKey, StatsRefused, materialize,
    samples_from_baseline, wilson,
};

fn solver(model: &str) -> StatKey {
    StatKey::Solver {
        model: model.into(),
        skill: "none".into(),
        harness: "build-1".into(),
    }
}

fn sample(id: &str, key: StatKey, success: bool) -> Sample {
    Sample {
        outcome_id: id.into(),
        key,
        success,
        cost_minor: None,
        wall_ms: 1_000,
        attributed_to: "bundle-digest-1".into(),
    }
}

fn m<'a>(declared: Option<u32>) -> Materialization<'a> {
    Materialization {
        stats_version: "stats-1",
        tenant_id: "tenant-a",
        source_versions: vec![
            ("build".into(), "build-1".into()),
            ("registry_generation".into(), "registry-a".into()),
        ],
        created_at_ms: 1_700_000_000_000,
        declared_samples: declared,
    }
}

#[test]
fn counts_are_derived_and_the_same_outcome_counts_once() {
    let mut samples: Vec<Sample> = (0..40)
        .map(|i| sample(&format!("o{i}"), solver("gpt-5-mini"), i % 4 != 0))
        .collect();
    // The same outcome, replayed.
    samples.push(sample("o0", solver("gpt-5-mini"), false));
    samples.push(sample("o1", solver("gpt-5-mini"), true));
    let snap = materialize(&m(None), &samples).expect("materialized");
    assert_eq!(snap.samples, 40, "a replayed outcome is the same outcome");
    let a = snap.get(&solver("gpt-5-mini")).expect("aggregate");
    assert_eq!((a.samples, a.successes), (40, 30));
    assert!((a.mean - 0.75).abs() < 1e-9, "{a:?}");
    assert!(!a.low_confidence, "{a:?}");
    // Cost the provider never reported is never averaged as zero.
    assert_eq!(a.mean_cost_minor, None);
    assert_eq!(a.unknown_cost_samples, 40);
    // The snapshot is addressed by its own content, and materializing the same
    // inputs again gives the same address.
    assert_eq!(snap.digest.len(), 64);
    assert_eq!(materialize(&m(None), &samples).unwrap().digest, snap.digest);
    // A different observation is a different snapshot.
    let mut more = samples.clone();
    more.push(sample("o99", solver("gpt-5-mini"), false));
    assert_ne!(materialize(&m(None), &more).unwrap().digest, snap.digest);
}

#[test]
fn a_sparse_key_stays_low_confidence_and_qualifies_nothing() {
    let samples: Vec<Sample> = (0..3)
        .map(|i| sample(&format!("s{i}"), solver("gpt-5-mini"), true))
        .collect();
    let snap = materialize(&m(None), &samples).unwrap();
    let a = snap.get(&solver("gpt-5-mini")).unwrap();
    // Three for three is a mean of one, and it means almost nothing.
    assert!((a.mean - 1.0).abs() < 1e-9);
    assert!(a.low_confidence, "{a:?}");
    assert!(a.interval.0 < 0.5, "the bound stays wide: {a:?}");
    assert!(!a.qualifies(0.7), "a prior must not qualify a cheap plan");
    // The same mean with enough observations does qualify.
    let many: Vec<Sample> = (0..MIN_CONFIDENT_SAMPLES + 20)
        .map(|i| sample(&format!("m{i}"), solver("gpt-5-mini"), true))
        .collect();
    let dense = materialize(&m(None), &many).unwrap();
    let a = dense.get(&solver("gpt-5-mini")).unwrap();
    assert!(!a.low_confidence, "{a:?}");
    assert!(a.qualifies(0.7), "{a:?}");
    // Wilson stays inside [0, 1] and narrows with n.
    assert!(wilson(0, 0) == (0.0, 1.0));
    let (lo_small, hi_small) = wilson(9, 10);
    let (lo_big, hi_big) = wilson(900, 1000);
    assert!(lo_big > lo_small && hi_big < hi_small);
}

#[test]
fn a_snapshot_refuses_what_it_cannot_stand_behind() {
    let good = vec![sample("o1", solver("gpt-5-mini"), true)];
    // A declared count that the samples do not support.
    let err = materialize(&m(Some(50)), &good).unwrap_err();
    assert_eq!(err.code(), "STATS_FABRICATED_SAMPLE_COUNT");
    assert!(
        matches!(
            err,
            StatsRefused::FabricatedSampleCount {
                claimed: 50,
                actual: 1
            }
        ),
        "{err:?}"
    );
    // A joint key missing the gate or the verification is not a key.
    let mut bad = good.clone();
    bad[0].key = StatKey::Escalation {
        from_model: "gpt-5-mini".into(),
        to_model: "gpt-5".into(),
        gate: String::new(),
        repository: "repo-1".into(),
        verification: "cargo-test".into(),
    };
    let err = materialize(&m(None), &bad).unwrap_err();
    assert_eq!(err.code(), "STATS_MISSING_KEY_COMPONENT");
    assert!(
        matches!(err, StatsRefused::MissingKeyComponent { ref component, .. } if component == "gate"),
        "{err:?}"
    );
    bad[0].key = StatKey::Escalation {
        from_model: "gpt-5-mini".into(),
        to_model: "gpt-5".into(),
        gate: "acceptance".into(),
        repository: "repo-1".into(),
        verification: String::new(),
    };
    assert!(matches!(materialize(&m(None), &bad).unwrap_err(),
            StatsRefused::MissingKeyComponent { ref component, .. } if component == "verification"));
    // An observation attributed to nothing.
    let mut orphan = good.clone();
    orphan[0].attributed_to = String::new();
    assert_eq!(
        materialize(&m(None), &orphan).unwrap_err().code(),
        "STATS_UNATTRIBUTED_SAMPLE"
    );
}

#[test]
fn the_registry_and_the_statistics_are_joined_independently() {
    let samples: Vec<Sample> = (0..MIN_CONFIDENT_SAMPLES)
        .map(|i| sample(&format!("j{i}"), solver("gpt-5-mini"), i % 3 != 0))
        .collect();
    let snap = materialize(&m(None), &samples).unwrap();
    // A compiler pins the statistics version and gets exactly that snapshot.
    assert!(snap.joined("stats-1").is_ok());
    let err = snap.joined("stats-2").unwrap_err();
    assert_eq!(err.code(), "STATS_STALE");
    assert!(
        matches!(err, StatsRefused::StaleStats { ref pinned, ref found } if pinned == "stats-2" && found == "stats-1"),
        "{err:?}"
    );
    // The registry generation is recorded as a source, and it is not part of
    // the lookup: a new generation cannot rewrite an observation.
    assert!(
        snap.source_versions
            .contains(&("registry_generation".to_owned(), "registry-a".to_owned()))
    );
    let before = snap.get(&solver("gpt-5-mini")).unwrap().clone();
    let mut later = m(None);
    later.source_versions = vec![
        ("build".into(), "build-1".into()),
        ("registry_generation".into(), "registry-b".into()),
    ];
    let after = materialize(&later, &samples).unwrap();
    assert_eq!(
        after.get(&solver("gpt-5-mini")).unwrap().mean,
        before.mean,
        "a changed registry does not move an observation"
    );
    assert_ne!(
        after.digest, snap.digest,
        "but it is a different snapshot, and says which generation it came from"
    );
    // Tenancy is checked on read, not assumed.
    assert!(snap.for_tenant("tenant-a").is_ok());
    let err = snap.for_tenant("tenant-b").unwrap_err();
    assert_eq!(err.code(), "STATS_WRONG_TENANT");
}

#[test]
fn baseline_outcomes_become_attributable_samples() {
    use modbit_observability::baseline::{Interventions, TaskOutcome, Usage, publish};
    let outcome = |task: &str, verified: bool| TaskOutcome {
        task_id: task.into(),
        goal_digest: "a".repeat(64),
        state: "Completed".into(),
        verification: if verified { "PASSED" } else { "FAILED" }.into(),
        verified,
        checks: (2, u32::from(!verified)),
        model_calls: 4,
        retries: 0,
        cache_units: (3, 1),
        tool_calls: 2,
        usage: Usage::default(),
        wall_ms: 1_500,
        model_ms: 300,
        tool_ms: 40,
        interventions: Interventions::default(),
        model: "gpt-5-mini".into(),
        endpoint: "openai".into(),
    };
    let bundle = publish(
        "build-1",
        "revision-1",
        "env-1",
        1_700_000_000_000,
        vec![outcome("t1", true), outcome("t2", false)],
    );
    let samples = samples_from_baseline(&bundle);
    assert_eq!(samples.len(), 2);
    assert!(
        samples
            .iter()
            .all(|s| s.attributed_to == bundle.bundle_digest)
    );
    assert_eq!(
        samples[0].key,
        StatKey::Solver {
            model: "gpt-5-mini".into(),
            skill: "none".into(),
            harness: "build-1".into(),
        }
    );
    // Cost the bundle could not know stays unknown in the sample.
    assert!(samples.iter().all(|s| s.cost_minor.is_none()));
    let snap = materialize(&m(None), &samples).unwrap();
    let a = snap.get(&samples[0].key).unwrap();
    assert_eq!((a.samples, a.successes), (2, 1));
    assert!(a.low_confidence, "two observations are a prior: {a:?}");
    assert_eq!(snap.source_digests, vec![bundle.bundle_digest.clone()]);
}
