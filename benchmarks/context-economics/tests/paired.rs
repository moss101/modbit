//! The paired statistics of the context economics benchmark (REQ-EV-0250):
//! what pairing means, what the interval says, and what an unpaired trial
//! contributes (nothing).
use modbit_bench_context_economics::{Metric, Trial, metric_of, paired_report};

fn trial(variant: &str, task: &str, repeat: u32, tokens: u64, tools: u32, ms: u64) -> Trial {
    Trial {
        variant: variant.into(),
        task: task.into(),
        repeat,
        input_tokens: tokens,
        output_tokens: tokens / 10,
        cached_input_tokens: 0,
        tool_calls: tools,
        model_calls: tools + 1,
        agent_ms: ms,
        cold_ms: ms + 500,
        verified: true,
    }
}

#[test]
fn a_paired_report_measures_the_difference_and_says_what_it_cannot() {
    let mut trials = Vec::new();
    for (i, (b, t)) in [
        (1000, 700),
        (1200, 800),
        (900, 650),
        (1100, 780),
        (1000, 690),
    ]
    .into_iter()
    .enumerate()
    {
        let r = u32::try_from(i).unwrap();
        trials.push(trial("baseline", "fix-totals", r, b, 8, 4000));
        trials.push(trial("treatment", "fix-totals", r, t, 5, 3600));
    }
    let report = paired_report(
        &trials,
        "the context machinery",
        &["model", "task", "environment"],
        "a deterministic local model; the numbers measure the product, not model behaviour",
    );
    assert_eq!((report.pairs, report.tasks.len()), (5, 1));
    assert!(report.unpaired.is_empty());
    let tokens = metric_of(&report, Metric::InputTokens).unwrap();
    assert_eq!(tokens.pairs, 5);
    assert!(tokens.mean_delta < 0.0, "{tokens:?}");
    assert!(
        tokens.ci95.1 < 0.0,
        "the whole interval is a saving: {tokens:?}"
    );
    assert!(tokens.significant, "{tokens:?}");
    assert!(
        tokens.relative.unwrap() < -0.2,
        "a saving of more than a fifth: {tokens:?}"
    );
    assert_eq!(tokens.baseline_median, 1000.0);
    assert_eq!(tokens.treatment_median, 700.0);
    // Every metric is reported, including the two times.
    for m in [
        Metric::InputTokens,
        Metric::ToolCalls,
        Metric::AgentMs,
        Metric::ColdMs,
    ] {
        assert!(metric_of(&report, m).is_some(), "{m:?}");
    }
    assert_eq!(report.verified, (5, 5));
    assert!(report.method.contains("not model behaviour"), "{report:?}");
}

#[test]
fn a_difference_that_is_not_there_is_not_claimed() {
    let mut trials = Vec::new();
    for (i, (b, t)) in [(1000, 1010), (1000, 990), (1000, 1005), (1000, 995)]
        .into_iter()
        .enumerate()
    {
        let r = u32::try_from(i).unwrap();
        trials.push(trial("baseline", "noop", r, b, 5, 1000));
        trials.push(trial("treatment", "noop", r, t, 5, 1000));
    }
    let report = paired_report(&trials, "nothing", &["everything"], "same everything");
    let tokens = metric_of(&report, Metric::InputTokens).unwrap();
    assert!(!tokens.significant, "{tokens:?}");
    assert!(tokens.ci95.0 < 0.0 && tokens.ci95.1 > 0.0, "{tokens:?}");
    let tools = metric_of(&report, Metric::ToolCalls).unwrap();
    assert_eq!((tools.mean_delta, tools.median_delta), (0.0, 0.0));
}

#[test]
fn an_unpaired_trial_says_nothing_about_a_difference() {
    let trials = vec![
        trial("baseline", "a", 0, 1000, 8, 4000),
        trial("treatment", "a", 0, 700, 5, 3600),
        // No partner for either of these.
        trial("treatment", "b", 0, 100, 1, 100),
        trial("baseline", "c", 3, 5000, 40, 40000),
    ];
    let report = paired_report(&trials, "x", &["y"], "z");
    assert_eq!(report.pairs, 1);
    assert_eq!(report.tasks, vec!["a".to_owned()]);
    assert_eq!(
        report.unpaired,
        vec!["baseline c #3".to_owned(), "treatment b #0".to_owned()]
    );
    // One pair cannot carry an interval, and the report does not invent one.
    let tokens = metric_of(&report, Metric::InputTokens).unwrap();
    assert!(tokens.ci95.0.is_nan() && !tokens.significant, "{tokens:?}");
    assert_eq!(tokens.mean_delta, -300.0);
}
