//! Repository Knowledge Artifact (REQ-EV-0060): a generated map that names its
//! sources, and says so when they move.
use std::collections::BTreeMap;

use modbit_retrieval::knowledge::{
    AUTHORITY, ClaimKind, FileFacts, KnowledgeArtifact, build, check, module_of,
};

fn facts<'a>(
    path: &'a str,
    hash: &'a str,
    exports: &[(&str, &str)],
    imports: &[&str],
    tests: &[&str],
) -> FileFacts<'a> {
    FileFacts {
        path,
        content_hash: hash,
        exports: exports
            .iter()
            .map(|(k, n)| ((*k).to_owned(), (*n).to_owned()))
            .collect(),
        imports: imports.iter().map(|i| (*i).to_owned()).collect(),
        tests: tests.iter().map(|t| (*t).to_owned()).collect(),
    }
}

fn corpus() -> KnowledgeArtifact {
    build(
        7,
        &[
            facts(
                "src/cart.rs",
                &"a".repeat(64),
                &[("fn", "total_cents"), ("struct", "Cart")],
                &["src/money.rs"],
                &["tests/cart.rs"],
            ),
            facts(
                "src/money.rs",
                &"b".repeat(64),
                &[("fn", "to_cents")],
                &[],
                &[],
            ),
            facts("tests/cart.rs", &"c".repeat(64), &[], &["src/cart.rs"], &[]),
        ],
    )
}

#[test]
fn the_map_names_modules_exports_dependencies_and_its_own_sources() {
    let a = corpus();
    assert_eq!((a.revision, a.files), (7, 3));
    assert_eq!(a.artifact_hash.len(), 64);
    assert_eq!(a.authority, AUTHORITY);
    assert!(a.authority.contains("never authority"), "{}", a.authority);
    let src = a.modules.iter().find(|m| m.path == "src").unwrap();
    assert_eq!(src.files, ["src/cart.rs", "src/money.rs"]);
    assert!(
        src.exports.contains(&"fn total_cents".to_owned()),
        "{src:?}"
    );
    assert!(src.exports.contains(&"struct Cart".to_owned()), "{src:?}");
    assert_eq!(src.tests, ["tests/cart.rs"]);
    // A dependency and its mirror cannot disagree: `tests` imports `src`, so
    // `src` is imported by `tests`.
    let tests = a.modules.iter().find(|m| m.path == "tests").unwrap();
    assert_eq!(tests.depends_on, ["src"]);
    assert_eq!(src.depended_on_by, ["tests"]);
    // Every claim names the files it came from, with the hash each had.
    assert!(!a.claims.is_empty());
    for c in &a.claims {
        assert!(!c.sources.is_empty(), "{c:?}");
        assert!(
            c.sources.iter().all(|s| s.content_hash.len() == 64),
            "{c:?}"
        );
    }
    assert!(a.claims.iter().any(|c| c.kind == ClaimKind::Exports));
    assert!(a.claims.iter().any(|c| c.kind == ClaimKind::Tests));
    assert_eq!(module_of("a.rs"), ".");
    assert_eq!(module_of("src/deep/a.rs"), "src/deep");
}

#[test]
fn a_claim_whose_source_moved_is_stale_and_one_whose_source_is_gone_is_missing() {
    let a = corpus();
    let mut now: BTreeMap<String, String> = a
        .claims
        .iter()
        .flat_map(|c| c.sources.iter())
        .map(|s| (s.path.clone(), s.content_hash.clone()))
        .collect();
    // Nothing changed yet.
    let checked = check(&a, &now, None);
    assert!(checked.iter().all(|c| c.status == "fresh"), "{checked:?}");
    // Edit one file: every claim derived from it goes stale, and says how.
    now.insert("src/cart.rs".into(), "d".repeat(64));
    let checked = check(&a, &now, None);
    let src: Vec<_> = checked.iter().filter(|c| c.claim.module == "src").collect();
    assert!(!src.is_empty());
    assert!(src.iter().all(|c| c.status == "stale"), "{src:?}");
    assert!(
        src[0].changed[0].starts_with("src/cart.rs changed (aaaaaaaa -> dddddddd)"),
        "{:?}",
        src[0].changed
    );
    // Claims about untouched modules stay fresh: staleness is per source.
    assert!(
        checked
            .iter()
            .filter(|c| c.claim.module == "tests")
            .all(|c| c.status == "fresh"),
        "{checked:?}"
    );
    // A file that is gone is worse than changed, and named as such.
    now.remove("src/money.rs");
    let checked = check(&a, &now, Some("src"));
    assert!(checked.iter().all(|c| c.status == "missing"), "{checked:?}");
    assert!(
        checked[0].changed.iter().any(|c| c.contains("is gone")),
        "{checked:?}"
    );
    // The module filter answers about one module only.
    assert!(checked.iter().all(|c| c.claim.module == "src"));
}
