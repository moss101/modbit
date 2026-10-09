//! PX-138 (offline half): the compaction evaluation that is not
//! self-referential. Ground truth comes from the event-log generator, the
//! reader sees only what an arm retains, and the metric can fail.

use modbit_bench_context_economics::compaction_eval::{
    Arm, LIVE_MARKER, Probe, answers_from, context_for, evaluate, invalid_probes, long_runs,
    validate_report,
};

fn arm(
    r: &modbit_bench_context_economics::compaction_eval::CompactionReport,
    a: Arm,
) -> &modbit_bench_context_economics::compaction_eval::ArmResult {
    r.arms.iter().find(|x| x.arm == a).unwrap()
}

#[test]
fn there_are_at_least_twenty_long_runs_and_every_probe_is_answerable_from_its_event_log() {
    let runs = long_runs(24);
    assert!(runs.len() >= 20);
    for r in &runs {
        assert!(
            (30..=50).contains(&r.entries.len()),
            "{}: {} entries",
            r.id,
            r.entries.len()
        );
        assert_eq!(invalid_probes(r), Vec::<String>::new());
        assert!(r.probes.len() >= 9, "{}", r.id);
    }
    // Deterministic: the fixtures are the same on every host and run.
    let again = long_runs(24);
    assert!(
        runs.iter()
            .zip(&again)
            .all(|(a, b)| a.entries == b.entries && a.probes == b.probes)
    );
}

#[test]
fn the_metric_reaches_its_bound_and_can_fail_a_lossy_summarizer() {
    let runs = long_runs(24);
    let report = evaluate(&runs);
    validate_report(&report).unwrap();
    let text = serde_json::to_string_pretty(&report).unwrap();
    eprintln!("{text}");
    // `MODBIT_BENCH_RESULTS_DIR` retains the report (the committed copy was
    // produced this way); the digest reproduces from the retained content.
    if let Ok(dir) = std::env::var("MODBIT_BENCH_RESULTS_DIR") {
        std::fs::write(
            std::path::Path::new(&dir).join("px-138-compaction-eval-scripted.json"),
            format!("{text}\n"),
        )
        .unwrap();
    }
    let full = arm(&report, Arm::Uncompacted);
    assert_eq!(
        full.answered.k, full.answered.n,
        "the reader answers every probe from the whole log: {:?}",
        full.dropped
    );
    let (ext, structured, lossy) = (
        arm(&report, Arm::Extractive),
        arm(&report, Arm::Structured),
        arm(&report, Arm::Lossy),
    );
    // The structured summary recovers what the extractive epoch loses, and a
    // summarizer that drops files, failures and decisions scores worse than
    // the structured one with a non-overlapping interval.
    assert!(
        structured.answered.p > ext.answered.p,
        "structured {:?} extractive {:?}",
        structured.answered,
        ext.answered
    );
    assert!(
        lossy.answered.p < structured.answered.p && lossy.answered.hi < structured.answered.lo,
        "lossy {:?} structured {:?}",
        lossy.answered,
        structured.answered
    );
    // Compaction is not free: the structured arm does not reach the bound
    // (early decisions are beyond its budget), and the report names what it
    // dropped.
    assert!(
        structured.answered.k < structured.answered.n && !structured.dropped.is_empty(),
        "{structured:?}"
    );
    // It saves tokens against the whole log, with a paired interval.
    for a in [ext, structured, lossy] {
        assert!(
            a.tokens_saved.mean > 0.0 && a.tokens_saved.ci95.0 > 0.0,
            "{a:?}"
        );
        assert!(a.refused.is_empty(), "{:?}", a.refused);
    }
    // The pointer makes every probe recoverable, whichever summary is kept.
    for a in &report.arms {
        assert_eq!(a.recoverable.k, a.recoverable.n, "{:?}", a.arm);
    }
    // The report says what it did not measure.
    assert_eq!(report.live, LIVE_MARKER);
    assert!(report.live.starts_with("LIVE: NOT RUN"));
}

#[test]
fn the_oracle_is_not_the_summary_a_probe_the_log_cannot_answer_is_a_fixture_bug() {
    let mut runs = long_runs(2);
    runs[0].probes.push(Probe {
        id: "bad".into(),
        kind: "tool_result".into(),
        question: "What is the airspeed of a swallow?".into(),
        answer: vec!["african or european".into()],
        entry: 0,
    });
    assert_eq!(invalid_probes(&runs[0]), vec!["bad".to_owned()]);
    assert!(std::panic::catch_unwind(|| evaluate(&runs)).is_err());
}

#[test]
fn the_reader_answers_from_the_retained_context_only_not_from_the_full_log() {
    let run = &long_runs(1)[0];
    let probe = run
        .probes
        .iter()
        .find(|p| p.kind == "assistant_decision")
        .unwrap();
    let whole = context_for(Arm::Uncompacted, &run.entries).unwrap();
    assert!(answers_from(&whole, probe));
    // A context that leaves the passage out cannot answer it, wherever else
    // the words of the question appear.
    let without: String = whole
        .lines()
        .filter(|l| !probe.answer.iter().all(|a| l.contains(a.as_str())))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!answers_from(&without, probe));
}

#[test]
fn a_tampered_report_does_not_validate() {
    let mut report = evaluate(&long_runs(3));
    validate_report(&report).unwrap();
    let mut hidden = report.clone();
    let lossy = hidden
        .arms
        .iter_mut()
        .find(|a| a.arm == Arm::Lossy)
        .unwrap();
    lossy.dropped.pop();
    assert!(validate_report(&hidden).unwrap_err().contains("hides"));
    report.arms[2].answered.k += 0; // content unchanged: digest reproduces
    validate_report(&report).unwrap();
    report.runs += 1;
    assert!(validate_report(&report).unwrap_err().contains("digest"));
}
