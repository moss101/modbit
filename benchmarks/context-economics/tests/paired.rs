//! The paired statistics of the context economics benchmark (REQ-EV-0250):
//! what pairing means, what the interval says, and what an unpaired trial
//! contributes (nothing).
use modbit_bench_context_economics::{
    Metric, SeenPrompt, Trial, metric_of, normalized_tool_calls, paired_report, prompt_parity,
};

fn trial(variant: &str, task: &str, repeat: u32, tokens: u64, tools: u32, ms: u64) -> Trial {
    Trial {
        variant: variant.into(),
        task: task.into(),
        repeat,
        input_tokens: tokens,
        output_tokens: tokens / 10,
        cached_input_tokens: 0,
        tool_calls: tools,
        normalized_tool_calls: tools.saturating_sub(1),
        model_calls: tools + 1,
        agent_ms: ms,
        cold_ms: ms + 500,
        verified: true,
        tool_schema_bytes: 0,
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

/// REQ-EV-0251: protocol calls are not work, a batch is its operations, and
/// a read, a search, a pack or a shell command are one unit each whatever
/// they are named — so counts compare across variants with different tools.
#[test]
fn tool_calls_are_normalized_across_tool_families() {
    let baseline = vec![
        ("plan.update".to_owned(), 1),
        ("fs.read".to_owned(), 1),
        ("fs.read".to_owned(), 1),
        ("fs.read".to_owned(), 1),
        ("search.exact".to_owned(), 1),
        ("change.batch".to_owned(), 3),
        ("task.complete".to_owned(), 1),
    ];
    let treatment = vec![
        ("plan.update".to_owned(), 1),
        ("context.pack".to_owned(), 1),
        ("change.apply".to_owned(), 1),
        ("change.apply".to_owned(), 1),
        ("change.apply".to_owned(), 1),
        ("task.complete".to_owned(), 1),
    ];
    // Baseline: three reads, one search, three batched edits = 7; the plan
    // and the completion handshake are not counted.
    assert_eq!(normalized_tool_calls(&baseline), 7);
    // Treatment: one pack and three edits = 4, the same work in fewer calls.
    assert_eq!(normalized_tool_calls(&treatment), 4);
    // The raw counts would have told a different story (7 vs 6): the batch
    // hides work and the protocol calls pad both sides.
    assert_eq!((baseline.len(), treatment.len()), (7, 6));
    assert_eq!(normalized_tool_calls(&[]), 0);
    assert_eq!(normalized_tool_calls(&[("change.batch".to_owned(), 0)]), 1);
}

/// REQ-EV-0253: the variants must be asked the same thing. Only the
/// capability profile may differ, and a prompt that steers the agent toward
/// the machinery under test invalidates the comparison.
#[test]
fn a_benchmark_is_unbiased_only_when_its_prompts_are_identical_and_force_nothing() {
    let prompt = |tools: &[&str]| SeenPrompt {
        system: "You are working in a repository. Use the tools you have.".into(),
        request: "find where totals are computed".into(),
        tools: tools.iter().map(|t| (*t).to_owned()).collect(),
    };
    let same = prompt_parity(
        &prompt(&["fs.read", "search.exact"]),
        &prompt(&["fs.read", "search.exact", "context.pack"]),
    );
    assert!(same.identical_system && same.identical_request);
    assert_eq!(same.capability_difference, vec!["context.pack".to_owned()]);
    assert!(same.forcing_instructions.is_empty());
    assert!(same.unbiased, "{same:?}");
    // A treatment prompt that tells the agent what to use is steering.
    let mut steered = prompt(&["fs.read", "context.pack"]);
    steered
        .system
        .push_str(" You must use the context.pack tool before reading anything.");
    let biased = prompt_parity(&prompt(&["fs.read"]), &steered);
    assert!(!biased.identical_system);
    assert!(
        biased
            .forcing_instructions
            .iter()
            .any(|f| f.starts_with("treatment:")),
        "{biased:?}"
    );
    assert!(!biased.unbiased);
    // A different request is a different task, not a variant of the same one.
    let mut other = prompt(&["fs.read"]);
    other.request = "find where discounts are applied".into();
    assert!(!prompt_parity(&prompt(&["fs.read"]), &other).unbiased);
}
