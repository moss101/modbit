//! REQ-EPR-000: what a baseline is, and what it refuses to guess.
use modbit_observability::baseline::{
    Interventions, TaskOutcome, Usage, bundle_digest, digest_of, publish,
};

fn outcome(id: &str, verified: bool, usage: Usage) -> TaskOutcome {
    TaskOutcome {
        task_id: id.into(),
        goal_digest: digest_of("fix the totals"),
        state: "Completed".into(),
        verification: if verified { "PASSED" } else { "FAILED" }.into(),
        verified,
        checks: (5, u32::from(!verified)),
        model_calls: 4,
        retries: 1,
        cache_units: (3, 1),
        tool_calls: 6,
        usage,
        wall_ms: 4200,
        model_ms: 900,
        tool_ms: 2600,
        interventions: Interventions {
            review_decisions: 1,
            ..Interventions::default()
        },
        model: "gpt-5-mini".into(),
        endpoint: "openai".into(),
    }
}

#[test]
fn unknown_cost_stays_unknown_and_is_counted() {
    let mut known = Usage::default();
    known.add_reported(1000, 100, 200);
    known.add_reported(500, 50, 0);
    assert_eq!(
        (
            known.input_tokens,
            known.output_tokens,
            known.cached_input_tokens
        ),
        (Some(1500), Some(150), Some(200))
    );
    assert!(known.complete());

    let mut dropped = Usage::default();
    dropped.add_unreported();
    assert_eq!(dropped.input_tokens, None, "unknown is not zero");
    assert!(!dropped.complete());
    assert_eq!(dropped.unreported_invocations, 1);

    // A task that reported once and lost one attempt keeps both facts.
    let mut mixed = Usage::default();
    mixed.add_reported(700, 70, 0);
    mixed.add_unreported();
    assert_eq!(mixed.input_tokens, Some(700));
    assert!(!mixed.complete(), "the total is a floor, not the cost");
}

#[test]
fn a_bundle_is_pinned_sealed_and_honest_about_what_it_does_not_know() {
    let mut complete = Usage::default();
    complete.add_reported(1000, 100, 0);
    let mut incomplete = Usage::default();
    incomplete.add_unreported();
    let bundle = publish(
        "build-digest",
        "0f0f0f0f",
        "environment-digest",
        1_700_000_000_000,
        vec![
            outcome("a", true, complete),
            outcome("b", false, incomplete),
        ],
    );
    assert_eq!(bundle.schema_version, 1);
    assert_eq!(bundle.repository_revision, "0f0f0f0f");
    assert_eq!(bundle.verified_tasks, 1);
    assert_eq!(bundle.tasks_with_unknown_usage, 1);
    assert!(bundle.note.contains("Unknown cost stays unknown"));
    // The digest covers the content, and changing anything changes it.
    assert_eq!(bundle_digest(&bundle), bundle.bundle_digest);
    let mut edited = bundle.clone();
    edited.tasks[0].usage.input_tokens = Some(1);
    assert_ne!(bundle_digest(&edited), bundle.bundle_digest);
    // The goal is a digest, so a bundle carries no prose.
    assert_eq!(bundle.tasks[0].goal_digest.len(), 64);
    assert!(
        !serde_json::to_string(&bundle)
            .unwrap()
            .contains("fix the totals")
    );
}
