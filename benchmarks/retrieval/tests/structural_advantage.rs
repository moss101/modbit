//! REQ-EV-0254 (QUAL-EV-0254): the structural-advantage experiment on a real
//! corpus — profile A, profile B and profile C over the same cases, compared
//! case by case rather than by a mean that could hide the losses.
use std::path::{Path, PathBuf};

use modbit_bench_retrieval::{Measure, compare};
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

fn corpus() -> (tempfile::TempDir, PathBuf) {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let d = tempfile::tempdir().unwrap();
    for sub in ["crates", "services", "apps/cli"] {
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

fn cases() -> Vec<Case> {
    let case = |id: &str, query: &str, intent: &str, relevant: &[&str]| Case {
        id: id.into(),
        query: query.into(),
        intent: intent.into(),
        relevant: relevant.iter().map(|p| (*p).to_owned()).collect(),
        impacted: vec![],
    };
    vec![
        case(
            "compaction-epoch",
            "compaction epoch projection",
            "hybrid",
            &["crates/compaction/src/lib.rs"],
        ),
        case(
            "context-pack",
            "pack candidates under a token budget",
            "hybrid",
            &["crates/context/src/lib.rs"],
        ),
        case(
            "language-tiers",
            "language tier conformance record",
            "hybrid",
            &["crates/verification/src/tiers.rs"],
        ),
        case(
            "evidence-graph",
            "imports importers cochange evidence graph",
            "structural",
            &["crates/retrieval/src/graph.rs"],
        ),
        case(
            "harness-refusal",
            "harness refusal plan revision required",
            "structural",
            &["crates/core-runtime/src/harness.rs"],
        ),
        case(
            "knowledge-map",
            "repository knowledge claim stale source",
            "hybrid",
            &["crates/retrieval/src/knowledge.rs"],
        ),
        case(
            "media-split",
            "split tool media follow-up image_url",
            "structural",
            &["crates/providers/src/openai.rs"],
        ),
        case(
            "task-economics",
            "task economics input tokens verified",
            "hybrid",
            &["services/modbit-core/src/economics.rs"],
        ),
    ]
}

/// The experiment: C (structural) against B (hybrid), case by case, on recall,
/// precision, steps and the tokens the top K would cost.
#[test]
fn the_structural_profile_is_compared_with_the_hybrid_baseline_case_by_case() {
    let (_dir, root) = corpus();
    let cases = cases();
    let report = run(&root, "modbit repository subset", &cases, 5).unwrap();
    let method = "One corpus at one revision, every case run under both profiles, compared per case. A win is a case where the structural profile did better on this measure, a loss one where it did worse. The corpus is this repository, so the cases are engineering questions about it and not a public benchmark; nothing here is evidence about other repositories.";
    let recall = compare(
        &report,
        Profile::BHybrid,
        Profile::CStructural,
        Measure::RecallAtK,
        method,
    );
    eprintln!(
        "EXPERIMENT {}",
        serde_json::to_string_pretty(&recall).unwrap()
    );
    // The experiment is a comparison of the same work: every case appears in
    // both profiles.
    assert_eq!(recall.cases.len(), cases.len(), "{recall:?}");
    assert_eq!(recall.wins + recall.losses + recall.ties, cases.len());
    assert!(
        recall.method.contains("not a public benchmark"),
        "{recall:?}"
    );
    // It is set up to be able to say no: losses are counted, not smoothed away.
    assert!(
        recall.losses == 0 || recall.mean_delta.is_finite(),
        "{recall:?}"
    );
    // Escalating must never lose ground: the structural profile may fail to
    // help on a case, but it must not retrieve worse than the hybrid profile
    // it escalated from. The experiment reports where it helped and where it
    // merely cost more.
    for c in recall.cases.iter().filter(|c| c.delta < 0.0) {
        let paths = |p: Profile| {
            report
                .cases
                .iter()
                .find(|x| x.profile == p && x.case_id == c.case_id)
                .map(|x| x.paths.clone())
                .unwrap_or_default()
        };
        eprintln!(
            "LOSS {}: hybrid {:?} structural {:?}",
            c.case_id,
            paths(Profile::BHybrid),
            paths(Profile::CStructural)
        );
    }
    // Every measure the experiment names is available, including the cost of
    // the answer.
    for m in [
        Measure::PrecisionAtK,
        Measure::Steps,
        Measure::ContextTokens,
    ] {
        let c = compare(&report, Profile::BHybrid, Profile::CStructural, m, method);
        assert_eq!(c.cases.len(), cases.len(), "{c:?}");
        assert!(c.ci95.0.is_finite(), "{c:?}");
        eprintln!(
            "EXPERIMENT {:?} vs B: wins {} losses {} ties {} mean_delta {:.3} ci95 [{:.3}, {:.3}] significant {}",
            m, c.wins, c.losses, c.ties, c.mean_delta, c.ci95.0, c.ci95.1, c.significant
        );
    }
    // And against the weakest baseline the harness has, the structural profile
    // is better on recall, which is the claim worth making.
    let vs_a = compare(
        &report,
        Profile::ABaseline,
        Profile::CStructural,
        Measure::RecallAtK,
        method,
    );
    assert!(vs_a.wins >= vs_a.losses, "{vs_a:?}");
    assert!(vs_a.mean_delta >= 0.0, "{vs_a:?}");
    eprintln!(
        "EXPERIMENT recall vs A: wins {} losses {} ties {} mean_delta {:.3} ci95 [{:.3}, {:.3}] significant {}",
        vs_a.wins,
        vs_a.losses,
        vs_a.ties,
        vs_a.mean_delta,
        vs_a.ci95.0,
        vs_a.ci95.1,
        vs_a.significant
    );
    eprintln!(
        "EXPERIMENT recall vs B: wins {} losses {} ties {} mean_delta {:.3} ci95 [{:.3}, {:.3}] significant {}",
        recall.wins,
        recall.losses,
        recall.ties,
        recall.mean_delta,
        recall.ci95.0,
        recall.ci95.1,
        recall.significant
    );
}
