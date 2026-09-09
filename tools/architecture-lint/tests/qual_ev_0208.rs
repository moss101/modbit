//! QUAL-EV-0208 — "Architecture dependency test shows production runtime has
//! one Engineering Memory interface." Runs against the real workspace with
//! real `cargo metadata` and the shipped rules, plus a disposable negative.

use std::fs;
use std::path::{Path, PathBuf};

use architecture_lint::modules::{GraphFacts, check_modules, collect};
use architecture_lint::{Graph, Rules, check};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn qual_ev_0208_exactly_one_engineering_memory_owner_and_no_parallel_paths() {
    let root = root();
    let rules = Rules::load(&root.join("tools/architecture-lint/rules.toml")).unwrap();
    let facts = GraphFacts::load(&root.join("graph/project-graph.json")).unwrap();
    let modules = collect(&root.join("Cargo.toml"), &[]).unwrap();

    // One and only one workspace member claims the canonical engineering-memory system.
    let claimants: Vec<_> = modules
        .iter()
        .filter(|m| {
            m.registration
                .as_ref()
                .is_some_and(|r| r.canonical.iter().any(|c| c == "engineering-memory"))
        })
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(
        claimants,
        vec!["modbit-memory"],
        "engineering-memory claimants: {claimants:?}"
    );
    assert!(check_modules(&root, &modules, &facts, &rules).is_empty());

    // The Skill Lab and other context producers reach neither memory persistence
    // nor recovery stores through any dependency path.
    let graph = Graph::from_cargo_metadata(&root.join("Cargo.toml"), false).unwrap();
    for from in [
        "modbit-skills",
        "modbit-procedural-runtime",
        "modbit-prompt-compiler",
        "modbit-context",
        "modbit-retrieval",
    ] {
        assert!(
            graph.path(from, "modbit-memory").is_none(),
            "{from} reaches modbit-memory"
        );
    }
    for to in [
        "modbit-protocol-state",
        "modbit-checkpoint",
        "modbit-compaction",
        "modbit-event-store",
    ] {
        assert!(
            graph.path("modbit-skills", to).is_none(),
            "modbit-skills reaches {to}"
        );
    }
    assert!(check(&graph, &rules).is_empty());
    assert!(
        rules
            .forbid
            .iter()
            .any(|r| r.reason.contains("REQ-EV-0208")),
        "REQ-EV-0208 rules are shipped"
    );
}

#[test]
fn qual_ev_0208_negative_second_memory_system_is_rejected() {
    // A disposable workspace where `skills` declares its wiki as a second
    // engineering memory AND depends on the checkpoint store: both rejected.
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let w = |rel: &str, t: &str| {
        let p = r.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, t).unwrap();
    };
    w(
        "Cargo.toml",
        "[workspace]\nresolver = \"3\"\nmembers = [\"modbit-memory\", \"modbit-skills\", \"modbit-checkpoint\"]\n",
    );
    w(
        "modbit-memory/Cargo.toml",
        "[package]\nname = \"modbit-memory\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[package.metadata.modbit]\nowner = \"memory\"\ncanonical = [\"engineering-memory\"]\n",
    );
    w("modbit-memory/src/lib.rs", "");
    w(
        "modbit-checkpoint/Cargo.toml",
        "[package]\nname = \"modbit-checkpoint\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[package.metadata.modbit]\nowner = \"durability\"\n",
    );
    w("modbit-checkpoint/src/lib.rs", "");
    w(
        "modbit-skills/Cargo.toml",
        "[package]\nname = \"modbit-skills\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[dependencies]\nmodbit-checkpoint = { path = \"../modbit-checkpoint\" }\n[package.metadata.modbit]\nowner = \"skills\"\ncanonical = [\"engineering-memory\"]\n",
    );
    w("modbit-skills/src/lib.rs", "");
    w(
        "graph/project-graph.json",
        r#"{"nodes":[{"id":"memory","type":"subsystem"},{"id":"skills","type":"subsystem"},{"id":"durability","type":"subsystem"}]}"#,
    );
    let rules = Rules::load(&root().join("tools/architecture-lint/rules.toml")).unwrap();
    let facts = GraphFacts::load(&r.join("graph/project-graph.json")).unwrap();
    let modules = collect(&r.join("Cargo.toml"), &[]).unwrap();
    let mv = check_modules(r, &modules, &facts, &rules);
    assert!(
        mv.iter().any(|v| v
            .problem
            .contains("`engineering-memory` has 2 active implementations")),
        "{mv:?}"
    );
    let graph = Graph::from_cargo_metadata(&r.join("Cargo.toml"), false).unwrap();
    let dv = check(&graph, &rules);
    assert!(
        dv.iter().any(|v| v.from == "modbit-skills"
            && v.to == "modbit-checkpoint"
            && v.reason.contains("REQ-EV-0208")),
        "{dv:?}"
    );
}
