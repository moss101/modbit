//! PX-035: impact-based test selection from the evidence graph.
use std::collections::BTreeMap;
use std::path::Path;

use modbit_retrieval::{
    CommitRecord, EvidenceGraph, RepositoryIndex, SymbolIndex, precision_recall, select_impacted,
};

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

/// A small crate: lib -> money, two tests, one unrelated file.
fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::create_dir_all(r.join("src")).unwrap();
    std::fs::create_dir_all(r.join("tests")).unwrap();
    std::fs::write(
        r.join("Cargo.toml"),
        "[package]\nname = \"cart\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        r.join("src/lib.rs"),
        "pub mod money;\npub fn compute_total(q: u32, u: u32) -> u32 {\n    money::round(q * u)\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("src/money.rs"),
        "pub fn round(c: u32) -> u32 {\n    c\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("tests/total_test.rs"),
        "use cart::compute_total;\n#[test]\nfn totals() {\n    assert_eq!(compute_total(2, 150), 300);\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("tests/round_test.rs"),
        "#[test]\nfn rounds() {\n    assert_eq!(cart::money::round(1), 1);\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("tests/unrelated_test.rs"),
        "#[test]\nfn other() {}\n",
    )
    .unwrap();
    std::fs::write(r.join("README.md"), "# cart\n").unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(
        r,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    d
}

#[test]
fn selection_uses_test_links_dependencies_symbol_references_and_cochange() {
    let d = repo();
    let idx = RepositoryIndex::build(d.path(), 3).unwrap();
    let symbols = SymbolIndex::build(idx.texts_with_hash(), 3);
    let commits = vec![CommitRecord {
        sha: "a1".into(),
        author: "ann".into(),
        date: "2026-09-10".into(),
        subject: "money and its test".into(),
        files: vec!["src/money.rs".into(), "tests/round_test.rs".into()],
    }];
    let graph = EvidenceGraph::build(idx.texts(), commits, BTreeMap::new(), 3);
    // A change to src/money.rs: the round test is linked by co-change and by
    // the symbol it references; the totals test reaches it through lib.rs.
    let sel = select_impacted(&idx, &symbols, &graph, &["src/money.rs".to_owned()], 3, 50);
    assert_eq!(sel.depth, 3);
    assert_eq!(sel.revision, 3);
    assert!(sel.limitation.contains("heuristic"), "{sel:?}");
    assert!(sel.symbols.iter().any(|s| s == "round"), "{sel:?}");
    let paths: Vec<&str> = sel.tests.iter().map(|t| t.path.as_str()).collect();
    assert!(paths.contains(&"tests/round_test.rs"), "{sel:?}");
    assert!(!paths.contains(&"tests/unrelated_test.rs"), "{sel:?}");
    let round = sel
        .tests
        .iter()
        .find(|t| t.path == "tests/round_test.rs")
        .unwrap();
    assert!(round.reasons.contains(&"cochange".to_owned()), "{round:?}");
    assert!(
        round.reasons.contains(&"symbol_reference".to_owned()),
        "{round:?}"
    );
    // The strongest evidence ranks first.
    assert_eq!(sel.tests[0].path, "tests/round_test.rs", "{sel:?}");
    // A change to the test file itself selects it.
    let sel2 = select_impacted(
        &idx,
        &symbols,
        &graph,
        &["tests/total_test.rs".to_owned()],
        2,
        50,
    );
    let t = sel2
        .tests
        .iter()
        .find(|t| t.path == "tests/total_test.rs")
        .unwrap();
    assert_eq!(
        (t.reasons.first().map(String::as_str), t.distance),
        (Some("changed"), 0)
    );
    // A change to lib.rs reaches the test that imports it.
    let sel3 = select_impacted(&idx, &symbols, &graph, &["src/lib.rs".to_owned()], 2, 50);
    let paths: Vec<&str> = sel3.tests.iter().map(|t| t.path.as_str()).collect();
    assert!(paths.contains(&"tests/total_test.rs"), "{sel3:?}");
    let total = sel3
        .tests
        .iter()
        .find(|t| t.path == "tests/total_test.rs")
        .unwrap();
    assert!(
        total.reasons.contains(&"test_link".to_owned())
            || total.reasons.contains(&"dependency".to_owned()),
        "{total:?}"
    );
    // Bounded.
    assert!(
        select_impacted(&idx, &symbols, &graph, &["src/lib.rs".to_owned()], 2, 1)
            .tests
            .len()
            <= 1
    );
}

#[test]
fn precision_and_recall_are_measured_against_the_ground_truth() {
    let truth = ["tests/a.rs".to_owned(), "tests/b.rs".to_owned()];
    assert_eq!(
        precision_recall(&["tests/a.rs".to_owned()], &truth),
        (1.0, 0.5)
    );
    assert_eq!(
        precision_recall(&["tests/a.rs".into(), "tests/c.rs".into()], &truth),
        (0.5, 0.5)
    );
    assert_eq!(precision_recall(&[], &truth), (0.0, 0.0));
    assert_eq!(
        precision_recall(&["tests/a.rs".to_owned()], &[]),
        (1.0, 1.0)
    );
}
