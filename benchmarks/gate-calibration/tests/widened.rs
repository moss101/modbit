//! QUAL-PX-135 / PX-E2E-135 (REQ-PX-135; docs/62): the widened gate
//! calibration. Real fixtures for each Tier A language with a real
//! verification command (cargo, pytest, `node --test` on JavaScript and on
//! TypeScript), hidden oracle labels, the unchanged Acceptance Gate measured
//! twice per case (as sealed, and with the generated adversarial checks as
//! extra evidence), rates with Wilson intervals and sample counts per
//! language and slice, an oracle-leak search, a reproducible report digest and
//! no attested gate.
//!
//! Candidates here come from fixtures: templates of the four failure classes.
//! The candidate-producing runs against a live gateway are
//! `gate-calibration-live` (`MODBIT_LIVE_*`); nothing in this file claims they
//! ran. A language whose toolchain is missing is reported UNAVAILABLE, never
//! faked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use modbit_bench_gate_calibration::live::tree_diff;
use modbit_bench_gate_calibration::widened::{
    INCORRECT_CLASSES, LANGUAGES, MINIMA, RunOptions, WidenedCorpus, adversarial_config, digest_of,
    load, run, toolchain,
};
use modbit_bench_gate_calibration::{
    AcceptanceCorpus, AdversarialConfig, FileOp, Refusal, RiskCorpus, check_separation, find_leaks,
    run_case, tuning_lineages,
};

fn here() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn repo_root() -> PathBuf {
    here().join("../..").canonicalize().unwrap()
}

fn corpora() -> PathBuf {
    here().join("corpora")
}

fn corpus() -> WidenedCorpus {
    load(&corpora().join("widened")).unwrap()
}

// ---- The corpora, without running anything ---------------------------------

#[test]
fn the_widened_corpora_have_the_declared_shape_and_reach_the_floors() {
    let c = corpus();
    // 4 languages x 4 lineages x (2 correct + 8 incorrect).
    assert_eq!(c.cases.len(), 160);
    let mut by: BTreeMap<(&str, &str), u32> = BTreeMap::new();
    for wc in &c.cases {
        *by.entry((wc.language.as_str(), wc.class.as_str()))
            .or_default() += 1;
        // The corpus declares its label; the oracle must agree at run time.
        assert_eq!(
            wc.case.declared,
            if wc.class == "correct" {
                "correct"
            } else {
                "incorrect"
            },
            "{}",
            wc.case.id
        );
        assert!(!wc.case.seed.is_empty(), "{}: no seeded defect", wc.case.id);
        assert!(
            !wc.case.candidate.is_empty(),
            "{}: no candidate",
            wc.case.id
        );
        assert!(!wc.goal.is_empty(), "{}: no goal", wc.case.id);
    }
    for l in LANGUAGES {
        assert!(by[&(l, "correct")] >= MINIMA.correct, "{l}: correct");
        for class in INCORRECT_CLASSES {
            assert!(
                by[&(l, class)] >= MINIMA.incorrect_per_class,
                "{l}: {class} {}",
                by[&(l, class)]
            );
        }
    }
    // Risk cases per language, each with its own lineage.
    let mut risk: BTreeMap<&str, (u32, u32, u32)> = BTreeMap::new();
    for r in &c.risk {
        let e = risk.entry(r.language.as_str()).or_default();
        if r.case.oracle.review_required || r.case.oracle.human_required {
            e.0 += 1;
        } else {
            e.1 += 1;
        }
        if r.case.oracle.critical_surface {
            e.2 += 1;
        }
    }
    for l in LANGUAGES {
        let (obliged, benign, critical) = risk[l];
        assert!(obliged >= MINIMA.risk_obliged, "{l} obliged {obliged}");
        assert!(benign >= MINIMA.risk_benign, "{l} benign {benign}");
        assert!(critical >= MINIMA.risk_critical, "{l} critical {critical}");
    }
    let mut ids: Vec<&str> = c.cases.iter().map(|w| w.case.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 160, "case ids are unique");
}

#[test]
fn the_widened_holdout_is_held_out_of_every_tuning_set_and_of_the_epr_019_holdout() {
    let c = corpus();
    let legacy_acc: AcceptanceCorpus = serde_json::from_str(
        &std::fs::read_to_string(corpora().join("acceptance-holdout.json")).unwrap(),
    )
    .unwrap();
    let legacy_risk: RiskCorpus = serde_json::from_str(
        &std::fs::read_to_string(corpora().join("risk-holdout.json")).unwrap(),
    )
    .unwrap();
    let tuning = tuning_lineages(&repo_root());
    let mut acc = legacy_acc.clone();
    acc.cases.extend(c.cases.iter().map(|w| w.case.clone()));
    let mut risk = legacy_risk.clone();
    risk.cases.extend(c.risk.iter().map(|r| r.case.clone()));
    assert!(check_separation(&acc, &risk, &tuning).is_ok());
    // A widened case that shares a lineage with the competence suite is refused.
    let mut contaminated = acc.clone();
    let last = contaminated.cases.len() - 1;
    contaminated.cases[last].lineage = "rust-cli/reject-negative".into();
    assert!(matches!(
        check_separation(&contaminated, &risk, &tuning),
        Err(Refusal::Contaminated { .. })
    ));
    // A risk case that shares a lineage with an acceptance case is refused.
    let mut shared = risk.clone();
    let rl = shared.cases.len() - 1;
    shared.cases[rl].lineage = acc.cases[20].lineage.clone();
    assert!(matches!(
        check_separation(&acc, &shared, &tuning),
        Err(Refusal::SharedLineage { .. })
    ));
}

#[test]
fn no_oracle_label_is_in_anything_a_gate_or_a_model_is_given() {
    let c = corpus();
    assert!(
        c.needles
            .iter()
            .any(|n| n.starts_with("MODBIT-ORACLE-CANARY-rust"))
    );
    assert!(
        c.needles
            .iter()
            .any(|n| n.starts_with("MODBIT-ORACLE-CANARY-typescript"))
    );
    for wc in &c.cases {
        // Candidate files, seed files and the goal are everything a model or
        // the gate is handed before the oracle runs.
        let mut hay: Vec<(String, String)> =
            vec![(format!("{}:goal", wc.case.id), wc.goal.clone())];
        for op in wc.case.candidate.iter().chain(&wc.case.seed) {
            if let FileOp::Write { path, content } = op {
                hay.push((format!("{}:{path}", wc.case.id), content.clone()));
            }
        }
        assert_eq!(
            find_leaks(&hay, &c.needles),
            Vec::<String>::new(),
            "{}",
            wc.case.id
        );
    }
    // The fixtures themselves (the visible repositories) carry no oracle.
    for fx in ["rust-shop", "py-shop", "js-shop", "ts-shop"] {
        let root = repo_root().join("tests/fixtures/repos").join(fx);
        let mut stack = vec![root];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    if name != "target" && name != "__pycache__" {
                        stack.push(p);
                    }
                } else if let Ok(t) = std::fs::read_to_string(&p) {
                    let hits = find_leaks(&[(p.display().to_string(), t)], &c.needles);
                    assert!(hits.is_empty(), "{hits:?}");
                }
            }
        }
    }
}

#[test]
fn the_leak_search_finds_a_needle_wherever_it_is_and_nowhere_else() {
    let needles = vec![
        "MODBIT-ORACLE-CANARY-x-1".to_owned(),
        "\"declared\"".to_owned(),
    ];
    let clean = vec![("gate-input".to_owned(), "{\"checks\":[]}".to_owned())];
    assert!(find_leaks(&clean, &needles).is_empty());
    let dirty = vec![
        ("gate-input".to_owned(), "{\"checks\":[]}".to_owned()),
        (
            "tree:test/x.mjs".to_owned(),
            "// MODBIT-ORACLE-CANARY-x-1".to_owned(),
        ),
        (
            "retained-output:3".to_owned(),
            "{\"declared\":\"correct\"}".to_owned(),
        ),
    ];
    assert_eq!(
        find_leaks(&dirty, &needles),
        vec![
            "retained-output:3: \"declared\"".to_owned(),
            "tree:test/x.mjs: MODBIT-ORACLE-CANARY-x-1".to_owned()
        ]
    );
}

// ---- Live mode: refused without a provider; its pure parts ---------------------

#[test]
fn live_mode_refuses_without_a_provider_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("live.json");
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_gate-calibration-live"));
    cmd.args([
        "--work",
        dir.path().to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    for (k, _) in std::env::vars() {
        if k.starts_with("MODBIT_LIVE") {
            cmd.env_remove(k);
        }
    }
    let o = cmd.output().unwrap();
    assert_eq!(o.status.code(), Some(2));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.starts_with("LIVE: NOT RUN"), "{err}");
    assert!(!out.exists());
    // A key alone does not start a paid run.
    let o = cmd
        .env("MODBIT_LIVE_API_KEY", "sk-live-0123456789abcdef0123456789")
        .env("MODBIT_LIVE_MODEL", "m")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("MODBIT_LIVE=1 is not set"));
}

#[test]
fn a_live_candidate_is_the_diff_the_model_left_and_nothing_else() {
    let before = tempfile::tempdir().unwrap();
    let after = tempfile::tempdir().unwrap();
    for (root, files) in [
        (
            before.path(),
            vec![("src/a.rs", "x"), ("tests/t.rs", "t"), ("Cargo.lock", "l")],
        ),
        (
            after.path(),
            vec![
                ("src/a.rs", "y"),
                ("src/new.rs", "n"),
                ("Cargo.lock", "l2"),
                (".modbit-pytest.xml", "<x/>"),
            ],
        ),
    ] {
        for (p, t) in files {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        }
    }
    std::fs::create_dir_all(after.path().join("target")).unwrap();
    std::fs::write(after.path().join("target/junk"), "j").unwrap();
    let ops = tree_diff(before.path(), after.path());
    assert_eq!(
        ops,
        vec![
            FileOp::Write {
                path: "src/a.rs".into(),
                content: "y".into()
            },
            FileOp::Write {
                path: "src/new.rs".into(),
                content: "n".into()
            },
            FileOp::Delete {
                path: "tests/t.rs".into()
            },
        ]
    );
}

// ---- The real thing -----------------------------------------------------------------

fn first_available(order: &[&str]) -> Option<String> {
    order
        .iter()
        .find(|l| toolchain(l).is_ok())
        .map(|l| (*l).to_owned())
}

#[tokio::test]
async fn a_mutation_that_exposes_the_oracle_to_the_gate_fails_the_leak_search() {
    let Some(lang) = first_available(&["javascript", "python", "rust"]) else {
        eprintln!("no Tier A toolchain here: nothing to run");
        return;
    };
    let c = corpus();
    let wc = c
        .cases
        .iter()
        .find(|w| w.language == lang && w.class == "correct" && w.flavor == "a")
        .unwrap();
    let work = tempfile::tempdir().unwrap();
    let policy = modbit_policy::AssurancePolicy::default();
    let honest = adversarial_config(&c);
    let run = run_case(
        &wc.case,
        &repo_root(),
        &corpora().join("widened"),
        work.path(),
        &policy,
        Some(&honest),
    )
    .await
    .unwrap();
    let w = run.widened.unwrap();
    assert!(w.leaks.is_empty(), "{:?}", w.leaks);
    // The mutation: the oracle is copied into the tree the gate reads.
    let exposed = AdversarialConfig {
        needles: c.needles.clone(),
        expose_oracle_to_gate: true,
    };
    let work = tempfile::tempdir().unwrap();
    let run = run_case(
        &wc.case,
        &repo_root(),
        &corpora().join("widened"),
        work.path(),
        &policy,
        Some(&exposed),
    )
    .await
    .unwrap();
    let w = run.widened.unwrap();
    assert!(
        w.leaks.iter().any(|l| l.contains("MODBIT-ORACLE-CANARY-")),
        "{:?}",
        w.leaks
    );
}

/// The subset the reproducibility test runs: every case of one lineage per
/// available fast language.
fn subset(c: &WidenedCorpus) -> (Vec<String>, Vec<String>) {
    let mut skip: Vec<String> = vec!["rust".into()];
    let mut ids = Vec::new();
    for l in ["python", "javascript", "typescript"] {
        if toolchain(l).is_err() {
            skip.push(l.into());
            continue;
        }
        ids.extend(
            c.cases
                .iter()
                .filter(|w| w.language == l && w.case.lineage.ends_with("/shipping"))
                .map(|w| w.case.id.clone()),
        );
    }
    (ids, skip)
}

#[tokio::test]
async fn rerunning_on_the_same_snapshot_reproduces_the_report_digest() {
    let c = corpus();
    let (ids, skip) = subset(&c);
    if ids.is_empty() {
        eprintln!("no fast Tier A toolchain here: nothing to run");
        return;
    }
    let opts = RunOptions {
        skip_languages: skip,
        only_cases: ids,
        include_legacy: false,
    };
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let r1 = run(&repo_root(), &corpora(), a.path(), &opts)
        .await
        .unwrap();
    let r2 = run(&repo_root(), &corpora(), b.path(), &opts)
        .await
        .unwrap();
    assert!(!r1.rows.is_empty());
    assert_eq!(r1.report_digest, r2.report_digest);
    assert_eq!(r1.report_digest, digest_of(&r1));
    assert_eq!(
        serde_json::to_string(&r1).unwrap(),
        serde_json::to_string(&r2).unwrap()
    );
    // A changed result changes the digest.
    let mut tampered = r1.clone();
    tampered.rows[0].widened.verdict = "ACCEPT".into();
    tampered.rows[0].widened.reasons.push("x".into());
    assert_ne!(digest_of(&tampered), r1.report_digest);
    // The report says what it is not: no attested gate, no threshold set.
    assert!(!r1.gates_attested);
    assert_eq!(r1.release.verdict, "FAIL");
    assert!(r1.release.reasons[0].starts_with("MISSING_THRESHOLD_PROFILE"));
    assert!(r1.leak.hits.is_empty());
}

fn pin(
    report: &modbit_bench_gate_calibration::widened::WidenedReport,
    lang: &str,
    slice: &str,
) -> ((u32, u32), (u32, u32)) {
    let l = report
        .languages
        .iter()
        .find(|l| l.language == lang)
        .unwrap();
    let s = l.slices.iter().find(|s| s.slice == slice).unwrap();
    (
        (s.previous.count, s.previous.n),
        (s.widened.count, s.widened.n),
    )
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn qual_px_135_the_widened_gate_rejects_the_seeded_false_accepts_the_previous_gate_accepted()
{
    let work = tempfile::tempdir().unwrap();
    let report = run(
        &repo_root(),
        &corpora(),
        work.path(),
        &RunOptions {
            include_legacy: true,
            ..RunOptions::default()
        },
    )
    .await
    .expect("the widened holdout is held out and labelled");

    // Every measured language reached its floors; an unavailable one says why.
    assert!(report.languages.iter().any(|l| l.status == "MEASURED"));
    for l in &report.languages {
        match l.status.as_str() {
            "MEASURED" => {
                assert_eq!(l.cases, 40, "{}", l.language);
                for s in &l.slices {
                    assert!(s.minimum_met, "{}: {} n={}", l.language, s.slice, s.n);
                    // Rates carry their samples and a Wilson interval.
                    for r in [&s.previous, &s.widened] {
                        assert!(r.n > 0 && r.lower <= r.rate && r.rate <= r.upper, "{r:?}");
                    }
                }
                let risk = l.risk.as_ref().unwrap();
                assert!(risk.minimum_met, "{}", l.language);
                // Risk rules are not changed by this row: the two misses
                // EPR-019 measured (dependency bump, payments) recur in
                // each language, and no more.
                assert_eq!(
                    (risk.false_negative.count, risk.false_negative.n),
                    (2, 12),
                    "{}: {:?}",
                    l.language,
                    risk.misses
                );
                assert_eq!((risk.false_positive.count, risk.false_positive.n), (0, 4));
                assert_eq!((risk.critical_miss.count, risk.critical_miss.n), (0, 6));
            }
            "UNAVAILABLE" => {
                assert!(l.reason.is_some() && l.cases == 0 && !report.minima_met);
                eprintln!("{} slice gated: {:?}", l.language, l.reason);
            }
            other => panic!("{other}"),
        }
    }

    // Before and after, on the same candidates, from one verification run each.
    let measured: Vec<&str> = report
        .languages
        .iter()
        .filter(|l| l.status == "MEASURED")
        .map(|l| l.language.as_str())
        .collect();
    let all_measured = measured.len() == LANGUAGES.len();
    for class in INCORRECT_CLASSES {
        let row = report
            .before_after
            .iter()
            .find(|c| c.class == class)
            .unwrap();
        assert_eq!(
            row.seeded,
            8 * u32::try_from(measured.len()).unwrap(),
            "{class}"
        );
        // The premise: the previous gate accepted every one of them.
        assert_eq!(row.accepted_by_previous, row.seeded, "{class}");
        assert_eq!(
            row.newly_rejected,
            row.seeded - row.accepted_by_widened,
            "{class}"
        );
    }
    if all_measured {
        let by_class = |class: &str| {
            report
                .before_after
                .iter()
                .find(|c| c.class == class)
                .unwrap()
                .accepted_by_widened
        };
        // Test weakening and vacuous passes: every seeded one is now rejected.
        assert_eq!(by_class("weakening"), 0);
        assert_eq!(by_class("vacuous"), 0);
        // Overfit and off-by-one: pinned to what the checks achieve. The
        // residual is reported, not hidden (see the report's `rows`).
        assert_eq!(by_class("overfit"), PINNED_OVERFIT_ACCEPTED);
        assert_eq!(by_class("offbyone"), PINNED_OFFBYONE_ACCEPTED);
        let (fa, fr) = (
            &report.overall["acceptance_false_accept_rate"],
            &report.overall["acceptance_false_reject_rate"],
        );
        let (pfa, pfr) = (
            &report.overall_previous["acceptance_false_accept_rate"],
            &report.overall_previous["acceptance_false_reject_rate"],
        );
        assert_eq!((pfa.count, pfa.n), (128, 128));
        assert_eq!(
            (fa.count, fa.n),
            (PINNED_OVERFIT_ACCEPTED + PINNED_OFFBYONE_ACCEPTED, 128)
        );
        // Correct candidates are not rejected by the new evidence.
        assert_eq!((pfr.count, pfr.n), (0, 32));
        assert_eq!((fr.count, fr.n), (0, 32));
        assert!(report.minima_met);
        for l in LANGUAGES {
            assert_eq!(pin(&report, l, "correct"), ((0, 8), (0, 8)), "{l}");
            for class in ["weakening", "vacuous"] {
                assert_eq!(pin(&report, l, class), ((8, 8), (0, 8)), "{l} {class}");
            }
        }
    }
    // Every widened rejection is explained by a finding of a generated check
    // or an existing reason; no widened verdict is a bare rejection.
    for r in &report.rows {
        if r.previous.verdict == "ACCEPT" && r.widened.verdict == "REJECT" {
            assert!(!r.findings.is_empty(), "{}", r.id);
            assert!(
                r.widened.reasons.iter().any(|x| x.contains("adv:")),
                "{}: {:?}",
                r.id,
                r.widened.reasons
            );
        }
        // The oracle's label is the corpus's label.
        assert_eq!(r.oracle_correct, r.class == "correct", "{}", r.id);
    }
    // EPR-019's own corpus, unchanged: the generated checks add nothing there
    // (its candidates carry no tests, and its suite is never green because of
    // a seeded pre-existing failure, so mutants cannot be told apart).
    if measured.contains(&"rust") {
        let legacy: BTreeMap<&str, (&str, &str)> = report
            .legacy
            .iter()
            .map(|r| {
                (
                    r.id.as_str(),
                    (r.previous.verdict.as_str(), r.widened.verdict.as_str()),
                )
            })
            .collect();
        assert_eq!(legacy["zero-overfitted"], ("ACCEPT", "ACCEPT"));
        assert_eq!(legacy["max-off-by-one"], ("ACCEPT", "ACCEPT"));
        assert_eq!(legacy["zero-correct"], ("ACCEPT", "ACCEPT"));
        assert_eq!(legacy["max-breaks-format"], ("REJECT", "REJECT"));
    }
    // No oracle label reached the gate, the checks or the retained output.
    assert!(report.leak.hits.is_empty(), "{:?}", report.leak.hits);
    assert_eq!(
        usize::try_from(report.leak.cases_searched).unwrap(),
        report.rows.len() + report.legacy.len()
    );
    // No threshold set, no gate attested.
    assert!(!report.gates_attested);
    assert_eq!(report.release.verdict, "FAIL");
    assert!(report.release.reasons[0].starts_with("MISSING_THRESHOLD_PROFILE"));
    assert_eq!(report.report_digest, digest_of(&report));
    assert_eq!(report.gate_version, "gate-1");
}

/// Seeded overfit candidates the widened gate still accepts, over four
/// languages. Pinned from the measured run; lowering it needs a better check,
/// raising it is a regression.
const PINNED_OVERFIT_ACCEPTED: u32 = 0;
/// Seeded off-by-one candidates the widened gate still accepts.
const PINNED_OFFBYONE_ACCEPTED: u32 = 0;
