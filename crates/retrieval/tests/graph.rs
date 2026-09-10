//! M3.6: the dependency / Git / test / runtime-evidence graph.
use std::collections::{BTreeMap, BTreeSet};

use modbit_retrieval::graph::{
    changed_lines_from_unified, import_specifiers, is_test_path, resolve_import,
};
use modbit_retrieval::{CommitRecord, EvidenceGraph, GraphQuery};

fn set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn import_specifiers_come_from_the_real_parse_of_each_language() {
    let rs = "mod util;\nuse crate::util::Thing;\nuse std::fmt;\npub mod inline { }\n";
    assert_eq!(
        import_specifiers("rust", "src/lib.rs", rs),
        ["mod util", "crate::util::Thing", "std::fmt"]
    );
    let py = "import os\nfrom .service import handler\nfrom pkg.sub import x as y\nimport a.b, c\n";
    assert_eq!(
        import_specifiers("python", "app/main.py", py),
        ["os", ".service", "pkg.sub", "a.b", "c"]
    );
    let ts = "import { a } from './a';\nexport * from \"./b\";\nconst c = require('./c');\nimport x from 'react';\n";
    assert_eq!(
        import_specifiers("typescript", "src/index.ts", ts),
        ["./a", "./b", "./c", "react"]
    );
    assert!(import_specifiers("markdown", "README.md", "# hi").is_empty());
}

#[test]
fn specifiers_resolve_to_workspace_paths_per_language_convention() {
    let paths = set(&[
        "src/lib.rs",
        "src/util.rs",
        "src/net/mod.rs",
        "tests/it.rs",
        "app/service.py",
        "app/lib/__init__.py",
        "src/a.ts",
        "src/b/index.ts",
    ]);
    let r = |l, f, s| resolve_import(l, f, s, &paths);
    assert_eq!(
        r("rust", "src/lib.rs", "mod util").as_deref(),
        Some("src/util.rs")
    );
    assert_eq!(
        r("rust", "src/lib.rs", "mod net").as_deref(),
        Some("src/net/mod.rs")
    );
    assert_eq!(
        r("rust", "src/net/mod.rs", "crate::util::Thing").as_deref(),
        Some("src/util.rs")
    );
    assert_eq!(
        r("rust", "tests/it.rs", "mylib::util::*").as_deref(),
        Some("src/util.rs")
    );
    assert_eq!(
        r("rust", "tests/it.rs", "mylib::parse").as_deref(),
        Some("src/lib.rs")
    );
    assert_eq!(r("rust", "src/lib.rs", "std::fmt"), None);
    assert_eq!(
        r("python", "app/main.py", ".service").as_deref(),
        Some("app/service.py")
    );
    assert_eq!(
        r("python", "app/main.py", "app.lib").as_deref(),
        Some("app/lib/__init__.py")
    );
    assert_eq!(r("python", "app/main.py", "os"), None);
    assert_eq!(
        r("typescript", "src/index.ts", "./a").as_deref(),
        Some("src/a.ts")
    );
    assert_eq!(
        r("typescript", "src/index.ts", "./b").as_deref(),
        Some("src/b/index.ts")
    );
    assert_eq!(r("typescript", "src/index.ts", "react"), None);
    assert!(
        is_test_path("tests/it.rs")
            && is_test_path("src/a.test.ts")
            && is_test_path("app/test_x.py")
    );
    assert!(!is_test_path("src/lib.rs"));
}

#[test]
fn changed_lines_follow_the_new_side_hunks_of_a_unified_diff() {
    let diff = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,4 @@\n x\n+y\n@@ -10 +11,2 @@\n+z\n+w\ndiff --git a/gone.rs b/gone.rs\n--- a/gone.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-q\n";
    let lines = changed_lines_from_unified(diff);
    assert_eq!(lines.get("src/a.rs").unwrap(), &vec![(1, 4), (11, 12)]);
    assert!(!lines.contains_key("gone.rs"), "{lines:?}");
}

fn commit(sha: &str, author: &str, files: &[&str]) -> CommitRecord {
    CommitRecord {
        sha: sha.into(),
        author: author.into(),
        date: "2026-09-10T00:00:00Z".into(),
        subject: format!("commit {sha}"),
        files: files.iter().map(|f| (*f).to_owned()).collect(),
    }
}

#[test]
fn graph_answers_imports_importers_history_tests_evidence_and_refreshes_per_path() {
    let files = [
        ("src/lib.rs", "mod util;\nmod net;\n", Some("rust")),
        ("src/util.rs", "use crate::net::Sock;\n", Some("rust")),
        ("src/net.rs", "pub struct Sock;\n", Some("rust")),
        ("tests/util_test.rs", "use mylib::util::*;\n", Some("rust")),
        ("README.md", "# x\n", None),
    ];
    let commits = vec![
        commit("a1", "ann", &["src/util.rs", "src/net.rs"]),
        commit("b2", "bob", &["src/util.rs", "tests/util_test.rs"]),
        commit("c3", "ann", &["src/util.rs", "src/net.rs", "README.md"]),
        commit("d4", "ann", &["README.md"]),
    ];
    let mut lines = BTreeMap::new();
    lines.insert("src/util.rs".to_owned(), vec![(1, 1)]);
    let mut g = EvidenceGraph::build(files.iter().copied(), commits, lines, 7);
    assert_eq!(g.revision(), 7);
    assert_eq!(g.edge_count(), 4);
    g.set_evidence(vec![(
        "src/util.rs".into(),
        "cargo:tests/util_test.rs::works".into(),
        "PASS".into(),
    )]);
    let v = g.query(&GraphQuery {
        path: "src/util.rs".into(),
        relation: "all".into(),
        depth: 1,
        max: 0,
    });
    assert_eq!(v.revision, 7);
    assert_eq!(v.imports, [("src/net.rs".to_owned(), 1)]);
    assert_eq!(
        v.importers,
        [
            ("src/lib.rs".to_owned(), 1),
            ("tests/util_test.rs".to_owned(), 1)
        ]
    );
    assert_eq!(
        v.cochange,
        [
            ("src/net.rs".to_owned(), 2),
            ("README.md".to_owned(), 1),
            ("tests/util_test.rs".to_owned(), 1)
        ]
    );
    assert_eq!(v.owners, [("ann".to_owned(), 2), ("bob".to_owned(), 1)]);
    assert_eq!(
        v.commits.iter().map(|c| c.sha.as_str()).collect::<Vec<_>>(),
        ["a1", "b2", "c3"]
    );
    assert_eq!(v.changed_lines, [(1, 1)]);
    assert_eq!(v.tests, ["tests/util_test.rs"]);
    assert_eq!(
        v.evidence,
        [(
            "cargo:tests/util_test.rs::works".to_owned(),
            "PASS".to_owned()
        )]
    );
    // Depth: lib.rs -> util.rs -> net.rs.
    let v = g.query(&GraphQuery {
        path: "src/lib.rs".into(),
        relation: "imports".into(),
        depth: 2,
        max: 0,
    });
    assert_eq!(
        v.imports,
        [("src/net.rs".to_owned(), 1), ("src/util.rs".to_owned(), 1)]
    );
    // tests reach net.rs through util.rs (transitively).
    let v = g.query(&GraphQuery {
        path: "src/net.rs".into(),
        relation: "tests".into(),
        depth: 1,
        max: 0,
    });
    assert_eq!(v.tests, ["tests/util_test.rs"]);
    // Refresh: util.rs drops its import; the test file is removed; revision moves.
    g.refresh(
        [
            ("src/util.rs", Some(("pub fn f() {}\n", Some("rust")))),
            ("tests/util_test.rs", None),
        ]
        .into_iter(),
        BTreeMap::new(),
        None,
        8,
    );
    let v = g.query(&GraphQuery {
        path: "src/util.rs".into(),
        relation: "all".into(),
        depth: 1,
        max: 0,
    });
    assert_eq!(v.revision, 8);
    assert!(v.imports.is_empty() && v.changed_lines.is_empty() && v.tests.is_empty());
    assert_eq!(v.importers, [("src/lib.rs".to_owned(), 1)]);
    let v = g.query(&GraphQuery {
        path: "src/net.rs".into(),
        relation: "importers".into(),
        depth: 3,
        max: 0,
    });
    assert_eq!(
        v.importers,
        [("src/lib.rs".to_owned(), 1)],
        "only lib.rs still reaches net.rs"
    );
}
