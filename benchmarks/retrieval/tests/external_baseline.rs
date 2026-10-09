//! REQ-PX-137 (QUAL-PX-137, offline half): the planner profiles against a real
//! external ripgrep baseline on 50+ hand-labelled cases over a larger
//! synthetic repository (a few thousand files) and over this repository.
//!
//! The full run needs ripgrep (`rg` on `PATH` or `MODBIT_RG_BIN`). Its absence
//! FAILS the report (`evaluate_corpus` returns the typed error, tested below);
//! the full-run tests below skip with a loud notice only when
//! `MODBIT_BENCH_REQUIRE_EXTERNAL` is unset, so a host without ripgrep can run
//! the rest of the workspace tests. Set it to make absence a failure.
//! `MODBIT_BENCH_RESULTS_DIR` retains the report.

use std::path::{Path, PathBuf};

use modbit_bench_retrieval::baseline_report::{
    BaselineReport, CorpusReport, LIVE_MARKER, Status, assemble, evaluate_corpus,
};
use modbit_bench_retrieval::external::{
    detect_rg, detect_rg_from_env, find_binary, query_terms, rg_answer,
};
use modbit_bench_retrieval::{repo_cases, synthetic};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn git(dir: &Path, args: &[&str]) {
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

fn commit_all(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=b",
            "-c",
            "user.email=b@e",
            "commit",
            "-q",
            "-m",
            "corpus",
        ],
    );
}

fn copy_tree(src: &Path, dst: &Path) {
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let name = e.file_name();
        let n = name.to_string_lossy();
        if matches!(&*n, "node_modules" | "target" | ".git" | "dist") {
            continue;
        }
        let to = dst.join(&name);
        if e.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&to).unwrap();
            copy_tree(&e.path(), &to);
        } else if e.file_type().unwrap().is_file() {
            std::fs::copy(e.path(), &to).unwrap();
        }
    }
}

fn skip_or_fail(why: &str) -> bool {
    if std::env::var_os("MODBIT_BENCH_REQUIRE_EXTERNAL").is_some() {
        panic!("{why}");
    }
    eprintln!(
        "EXTERNAL BASELINE: NOT RUN ({why}); set MODBIT_BENCH_REQUIRE_EXTERNAL=1 to make this a failure"
    );
    true
}

#[test]
fn the_label_sets_are_author_written_at_least_fifty_and_resolve_to_real_files() {
    assert!(synthetic::feature_count() >= 50);
    assert!(synthetic::keywords_are_unique());
    let dir = tempfile::tempdir().unwrap();
    let cases = synthetic::write_repo(dir.path(), 400).unwrap();
    assert!(cases.len() >= 50, "{}", cases.len());
    for c in &cases {
        assert!(
            c.relevant.iter().all(|p| dir.path().join(p).is_file()),
            "{c:?}"
        );
        assert!(!c.query.is_empty());
    }
    // Deterministic: the same bytes on every run.
    let again = tempfile::tempdir().unwrap();
    synthetic::write_repo(again.path(), 400).unwrap();
    for c in &cases {
        let rel = &c.relevant[0];
        assert_eq!(
            std::fs::read(dir.path().join(rel)).unwrap(),
            std::fs::read(again.path().join(rel)).unwrap()
        );
    }
    let repo = repo_cases::cases();
    assert!(repo.len() >= 50, "{}", repo.len());
    assert_eq!(
        repo_cases::labels_missing_from(&repo_root()),
        Vec::<String>::new(),
        "a renamed file must fail the benchmark, not score zero"
    );
}

#[test]
fn a_missing_ripgrep_fails_the_run_instead_of_leaving_the_column_out() {
    // Nothing named, nothing on the path.
    let err = detect_rg(None, Some(std::ffi::OsStr::new(""))).unwrap_err();
    assert!(err.to_string().starts_with("BASELINE MISSING"), "{err}");
    // A named binary that does not exist.
    let err = detect_rg(Some("/nonexistent/rg"), None).unwrap_err();
    assert!(err.to_string().contains("ripgrep"), "{err}");
    // A binary that exists but is not ripgrep is not accepted either.
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join(if cfg!(windows) { "rg.exe" } else { "rg" });
    std::fs::write(&fake, "not a program").unwrap();
    assert!(detect_rg(Some(fake.to_str().unwrap()), None).is_err());
    assert!(find_binary("zvec-grep", None, Some(dir.path().as_os_str())).is_none());
    // The whole evaluation inherits the failure: with the override pointing
    // nowhere the report cannot be produced.
    // (Run in a child so the process environment of this test binary is not
    // changed for the other tests.)
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_run_without_ripgrep",
            "--ignored",
            "--nocapture",
        ])
        .env("MODBIT_RG_BIN", "/nonexistent/rg")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
#[ignore = "run by the parent test with MODBIT_RG_BIN pointing nowhere"]
fn child_run_without_ripgrep() {
    assert_eq!(
        std::env::var("MODBIT_RG_BIN").as_deref(),
        Ok("/nonexistent/rg")
    );
    let dir = tempfile::tempdir().unwrap();
    let cases = synthetic::write_repo(dir.path(), 200).unwrap();
    commit_all(dir.path());
    let err = evaluate_corpus(dir.path(), "synthetic", &cases[..3]).unwrap_err();
    assert!(err.contains("BASELINE MISSING"), "{err}");
}

#[test]
fn the_query_words_are_the_content_words_in_order_without_repeats() {
    assert_eq!(
        query_terms("Where do the failed requests wait with jitter, jitter and a retry?"),
        vec!["failed", "requests", "wait", "jitter", "retry"]
    );
}

fn print_corpus(c: &CorpusReport) {
    eprintln!(
        "CORPUS {} files={} bytes={} cases={} cold_total_ms={:.0} incremental_ms={:.1} digest={}",
        c.corpus,
        c.files,
        c.bytes,
        c.cases,
        c.cold_index.total_ms,
        c.incremental_ms,
        c.ranking_digest
    );
    for col in &c.columns {
        match &col.status {
            Status::NotInstalled(why) => eprintln!("  {:<18} NOT_INSTALLED ({why})", col.name),
            Status::Ran => {
                let r = |k: usize| col.recall.iter().find(|(x, _)| *x == k).unwrap().1;
                let f = |k: usize| col.full_recall.iter().find(|(x, _)| *x == k).unwrap().1;
                eprintln!(
                    "  {:<18} R@1 {:.3} [{:.3},{:.3}]  R@5 {:.3} [{:.3},{:.3}]  R@10 {:.3} [{:.3},{:.3}]  full@10 {}/{}  budget2k {:.3} 8k {:.3} 32k {:.3}  lat_mean {:.1}ms steps {:.1} tok_top5 {:.0}",
                    col.name,
                    r(1).p,
                    r(1).lo,
                    r(1).hi,
                    r(5).p,
                    r(5).lo,
                    r(5).hi,
                    r(10).p,
                    r(10).lo,
                    r(10).hi,
                    f(10).k,
                    f(10).n,
                    col.recall_at_budget[0].1.p,
                    col.recall_at_budget[1].1.p,
                    col.recall_at_budget[2].1.p,
                    col.mean_latency_ms,
                    col.mean_steps,
                    col.mean_tokens_top5
                );
            }
        }
    }
    for p in &c.pairwise {
        eprintln!(
            "  {} vs ripgrep: wins {} losses {} ties {} mean_delta {:+.3} ci95 [{:+.3},{:+.3}]",
            p.treatment, p.wins, p.losses, p.ties, p.mean_delta, p.ci95.0, p.ci95.1
        );
    }
}

fn check_corpus(c: &CorpusReport, min_cases: usize) {
    assert!(c.cases >= min_cases, "{} cases", c.cases);
    // The baseline column exists, ran, and counted every case.
    let rg = c
        .columns
        .iter()
        .find(|x| x.name == "ExternalRipgrep")
        .expect("the ripgrep column is in the report");
    assert_eq!(rg.status, Status::Ran);
    assert!(
        rg.recall
            .iter()
            .all(|(_, p)| p.n > 0 && p.lo <= p.p && p.p <= p.hi)
    );
    // Every planner profile is present with intervals and a latency.
    for name in [
        "ABaseline",
        "BHybrid",
        "CStructural",
        "LexicalOnly",
        "SemanticOnly",
    ] {
        let col = c.columns.iter().find(|x| x.name == name).unwrap();
        assert_eq!(col.status, Status::Ran, "{name}");
        assert!(col.mean_latency_ms.is_finite() && col.recall_at_budget.len() == 3);
    }
    // zvec-grep is either a measured column or an explicit NOT_INSTALLED.
    let z = c
        .columns
        .iter()
        .find(|x| x.name == "ExternalZvecGrep")
        .unwrap();
    assert!(matches!(z.status, Status::Ran | Status::NotInstalled(_)));
    // Budgeted recall never exceeds unbudgeted recall at 10.
    for col in c.columns.iter().filter(|x| x.status == Status::Ran) {
        let r10 = col.recall.iter().find(|(k, _)| *k == 10).unwrap().1.p;
        assert!(
            col.recall_at_budget.iter().all(|(_, p)| p.p <= r10),
            "{}",
            col.name
        );
    }
    assert_eq!(c.pairwise.len(), 5);
}

fn retain(report: &BaselineReport, name: &str) {
    if let Ok(dir) = std::env::var("MODBIT_BENCH_RESULTS_DIR") {
        std::fs::write(
            Path::new(&dir).join(name),
            format!("{}\n", serde_json::to_string_pretty(report).unwrap()),
        )
        .unwrap();
    }
}

#[test]
fn planner_profiles_against_ripgrep_on_a_few_thousand_synthetic_files() {
    if detect_rg_from_env().is_err() && skip_or_fail("ripgrep is not installed") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let cases = synthetic::write_repo(dir.path(), 3_000).unwrap();
    commit_all(dir.path());
    let report = evaluate_corpus(dir.path(), "synthetic-3000", &cases).unwrap();
    assert!(report.files >= 3_000, "{} files", report.files);
    print_corpus(&report);
    check_corpus(&report, 50);
    // Rerunning the scoring on the same rankings reproduces the digest: the
    // digest is over what each profile returned, not over timings.
    let again = evaluate_corpus(dir.path(), "synthetic-3000", &cases).unwrap();
    assert_eq!(report.ranking_digest, again.ranking_digest);
    let full = assemble(vec![report]);
    assert_eq!(full.live, LIVE_MARKER);
    retain(&full, "px-137-retrieval-synthetic.json");
}

#[test]
fn planner_profiles_against_ripgrep_on_this_repository() {
    if detect_rg_from_env().is_err() && skip_or_fail("ripgrep is not installed") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    for sub in ["crates", "services", "apps/cli"] {
        let to = dir.path().join(sub);
        std::fs::create_dir_all(&to).unwrap();
        copy_tree(&repo_root().join(sub), &to);
    }
    commit_all(dir.path());
    let cases = repo_cases::cases();
    let report = evaluate_corpus(dir.path(), "modbit-repository", &cases).unwrap();
    print_corpus(&report);
    check_corpus(&report, 50);
    let full = assemble(vec![report]);
    retain(&full, "px-137-retrieval-repository.json");
}

#[test]
fn ripgrep_ranks_by_distinct_words_and_runs_as_a_real_process() {
    let Ok(rg) = detect_rg_from_env() else {
        skip_or_fail("ripgrep is not installed");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "alpha beta\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "alpha beta gamma gamma\n").unwrap();
    std::fs::write(dir.path().join("c.txt"), "alphabet soup\n").unwrap();
    let a = rg_answer(&rg, dir.path(), "alpha beta gamma", 5).unwrap();
    assert_eq!(
        a.paths,
        vec!["b.txt", "a.txt"],
        "whole words, most words first"
    );
    assert_eq!(a.steps, 3, "one invocation per query word");
    assert!(a.latency_ms > 0.0);
    // No match is an empty answer, not an error.
    assert!(
        rg_answer(&rg, dir.path(), "zzzzzz", 5)
            .unwrap()
            .paths
            .is_empty()
    );
}
