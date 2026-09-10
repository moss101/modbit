//! M3.9: the retrieval benchmark harness over a real corpus (a subset of this
//! repository copied into a temporary Git worktree), three profiles, the
//! measured metrics, and a JSON report.
use std::path::{Path, PathBuf};

use modbit_retrieval::bench::{Case, Profile, run};

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

fn copy_tree(src: &Path, dst: &Path) {
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let name = e.file_name();
        let n = name.to_string_lossy();
        if n == "node_modules" || n == "target" || n == ".git" || n == "dist" {
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

/// A temporary Git worktree holding the benchmarked subset of this repository.
fn corpus() -> (tempfile::TempDir, PathBuf) {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let d = tempfile::tempdir().unwrap();
    for sub in ["crates", "services", "apps/cli", "docs"] {
        let to = d.path().join(sub);
        std::fs::create_dir_all(&to).unwrap();
        copy_tree(&repo_root.join(sub), &to);
    }
    git(d.path(), &["init", "-q", "-b", "main"]);
    git(d.path(), &["add", "-A"]);
    git(
        d.path(),
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
    let root = d.path().to_path_buf();
    (d, root)
}

fn cases() -> (String, usize, Vec<Case>) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/retrieval-bench/cases.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    (
        v["corpus"].as_str().unwrap().to_owned(),
        usize::try_from(v["k"].as_u64().unwrap()).unwrap(),
        serde_json::from_value(v["cases"].clone()).unwrap(),
    )
}

#[test]
fn harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them() {
    let (_d, root) = corpus();
    let (corpus_name, k, cases) = cases();
    assert!(cases.len() >= 8);
    let report = run(&root, &corpus_name, &cases, k).unwrap();
    assert_eq!(report.harness_version, "retrieval-bench-v1");
    assert!(report.files > 100, "{}", report.files);
    assert_eq!(report.k, 5);
    assert!(report.cold_index.total_ms > 0.0 && report.incremental_ms > 0.0);
    assert!(
        report.cold_index.total_ms
            >= report.cold_index.exact_ms
                + report.cold_index.lexical_ms
                + report.cold_index.symbols_ms
                + report.cold_index.semantic_ms
                + report.cold_index.graph_ms
    );
    assert_eq!(
        report.incremental_path,
        "services/modbit-core/src/server.rs"
    );
    assert_eq!(report.cases.len(), cases.len() * 3);
    assert_eq!(report.profiles.len(), 3);
    let summary = |p: Profile| {
        report
            .profiles
            .iter()
            .find(|s| s.profile == Some(p))
            .unwrap()
    };
    let (a, b, c) = (
        summary(Profile::ABaseline),
        summary(Profile::BHybrid),
        summary(Profile::CStructural),
    );
    // Profile ceilings hold: A never leaves L0, B never leaves L1.
    for r in &report.cases {
        match r.profile {
            Profile::ABaseline => assert_eq!(r.ended_at, modbit_retrieval::Level::L0Exact, "{r:?}"),
            Profile::BHybrid => assert!(r.ended_at <= modbit_retrieval::Level::L1Hybrid, "{r:?}"),
            Profile::CStructural => {}
        }
        assert!(r.paths.len() <= 5);
        assert!((0.0..=1.0).contains(&r.recall_at_k) && (0.0..=1.0).contains(&r.precision_at_k));
    }
    // Ground truth the corpus actually holds: the L0 cases are found by every profile.
    for id in ["l0-symbol-singleton-lock", "l0-path-planner"] {
        for r in report.cases.iter().filter(|r| r.case_id == id) {
            assert_eq!(r.recall_at_k, 1.0, "{r:?}");
        }
    }
    // The structural profile is never worse than the baseline on recall, and
    // it is the only one that answers the impact cases.
    assert!(c.mean_recall_at_k >= a.mean_recall_at_k, "A {a:?} C {c:?}");
    assert!(c.full_recall_cases >= a.full_recall_cases);
    assert!(c.mean_impact_accuracy.unwrap_or(0.0) > 0.0, "{c:?}");
    // Hybrid covers the natural-language cases the baseline cannot.
    let nl = |p: Profile| {
        report
            .cases
            .iter()
            .filter(|r| r.profile == p && r.case_id.starts_with("l1-"))
            .map(|r| r.recall_at_k)
            .sum::<f32>()
    };
    assert!(
        nl(Profile::BHybrid) > nl(Profile::ABaseline),
        "B {b:?} A {a:?}"
    );
    // The report serialises and names its method honestly.
    let json = serde_json::to_string_pretty(&report).unwrap();
    assert!(json.contains("not claimed by this report"));
    if let Ok(out) = std::env::var("MODBIT_BENCH_REPORT") {
        std::fs::write(out, json).unwrap();
    }
}
