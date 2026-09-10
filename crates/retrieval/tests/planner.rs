//! M3.7: the L0–L3 retrieval planner and rank fusion over the real indexes.
use std::collections::BTreeMap;

use modbit_retrieval::planner::{features, retrieve, start_level};
use modbit_retrieval::{
    CommitRecord, EvidenceGraph, HashingEmbedder, Level, LexicalIndex, PlanRequest,
    RepositoryIndex, SemanticIndex, Sources, SymbolIndex,
};

fn git(dir: &std::path::Path, args: &[&str]) {
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

struct Built {
    idx: RepositoryIndex,
    lx: LexicalIndex,
    sx: SymbolIndex,
    sem: SemanticIndex,
    graph: EvidenceGraph,
}

fn build(dir: &std::path::Path, changed: &[(&str, (u32, u32))]) -> Built {
    let idx = RepositoryIndex::build(dir, 3).unwrap();
    let lx = LexicalIndex::build(idx.texts(), 3).unwrap();
    let sx = SymbolIndex::build(idx.texts_with_hash(), 3);
    let spans = |p: &str| {
        sx.symbols_in(p)
            .iter()
            .map(|s| (s.name.clone(), s.span.0, s.span.1))
            .collect::<Vec<_>>()
    };
    let files: Vec<modbit_retrieval::FileSource> =
        idx.texts().map(|(p, t, _)| (p, t, spans(p))).collect();
    let sem =
        SemanticIndex::build(Box::new(HashingEmbedder::default()), files.into_iter(), 3).unwrap();
    let commits = vec![CommitRecord {
        sha: "a".into(),
        author: "ann".into(),
        date: "2026-09-10".into(),
        subject: "totals".into(),
        files: vec!["src/lib.rs".into(), "tests/total_test.rs".into()],
    }];
    let mut lines = BTreeMap::new();
    for (p, r) in changed {
        lines.insert((*p).to_owned(), vec![*r]);
    }
    let graph = EvidenceGraph::build(idx.texts(), commits, lines, 3);
    Built {
        idx,
        lx,
        sx,
        sem,
        graph,
    }
}

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
        "pub mod money;\n\n/// Total of the cart in cents.\npub fn compute_total(quantity: u32, unit_cents: u32) -> u32 {\n    money::round(quantity * unit_cents)\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("src/money.rs"),
        "pub fn round(cents: u32) -> u32 {\n    cents\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("src/main.rs"),
        "use cart::compute_total;\nfn main() {\n    println!(\"{}\", compute_total(2, 150));\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("tests/total_test.rs"),
        "use cart::compute_total;\n#[test]\nfn totals_multiply() {\n    assert_eq!(compute_total(2, 150), 300);\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("README.md"),
        "# cart\nThe shopping cart computes order totals from quantity and unit price.\n",
    )
    .unwrap();
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

fn req(q: &str) -> PlanRequest {
    PlanRequest {
        query: q.into(),
        intent: String::new(),
        max_hits: 20,
        min_paths: 0,
        diagnostics: vec![],
    }
}

#[test]
fn query_features_pick_the_cheapest_level() {
    let f = features("compute_total");
    assert!(f.identifier_like && !f.path_like && !f.natural_language);
    assert_eq!(start_level(&f, ""), Level::L0Exact);
    let f = features("src/money.rs");
    assert!(f.path_like);
    assert_eq!(start_level(&f, ""), Level::L0Exact);
    let f = features("how are order totals computed");
    assert!(f.natural_language && !f.identifier_like);
    assert_eq!(start_level(&f, ""), Level::L1Hybrid);
    assert_eq!(
        start_level(&features("callers of compute_total"), ""),
        Level::L2Structural
    );
    assert_eq!(
        start_level(&features("which tests cover compute_total"), ""),
        Level::L3Engineering
    );
    assert_eq!(
        start_level(&features("compute_total"), "hybrid"),
        Level::L1Hybrid
    );
}

#[test]
fn l0_serves_a_known_identifier_without_escalating_and_boosts_the_symbol() {
    let d = repo();
    let b = build(d.path(), &[("src/lib.rs", (4, 6))]);
    let src = Sources {
        index: &b.idx,
        lexical: &b.lx,
        symbols: &b.sx,
        semantic: &b.sem,
        graph: &b.graph,
    };
    let r = retrieve(&src, &req("compute_total"));
    assert_eq!((r.started_at, r.ended_at), (Level::L0Exact, Level::L0Exact));
    assert!(r.escalations.is_empty(), "{:?}", r.escalations);
    assert_eq!(r.index_revision, 3);
    assert_eq!(r.embedding_generation, None, "L1 never ran");
    let sources: Vec<&str> = r.steps.iter().map(|s| s.source.as_str()).collect();
    assert_eq!(sources, ["exact", "symbols"]);
    // The definition is first: exact symbol match + fresh + changed lines.
    let top = &r.hits[0];
    assert_eq!(top.path, "src/lib.rs");
    assert_eq!(top.lines, Some((4, 6)));
    assert!(top.sources.contains(&"symbols".to_owned()), "{top:?}");
    for reason in ["exact_symbol", "fresh_in_worktree", "changed_lines"] {
        assert!(top.reasons.iter().any(|x| x == reason), "{top:?}");
    }
    assert!(top.content_hash.is_some());
    assert!(r.coverage_paths >= 3, "{r:?}");
    // No duplicate span identity.
    let mut keys: Vec<(String, Option<(u64, u64)>)> =
        r.hits.iter().map(|h| (h.path.clone(), h.span)).collect();
    let n = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), n);
}

#[test]
fn unknown_wording_starts_hybrid_and_a_miss_escalates_only_while_coverage_is_short() {
    let d = repo();
    let b = build(d.path(), &[]);
    let src = Sources {
        index: &b.idx,
        lexical: &b.lx,
        symbols: &b.sx,
        semantic: &b.sem,
        graph: &b.graph,
    };
    let r = retrieve(&src, &req("how are order totals computed"));
    assert_eq!(r.started_at, Level::L1Hybrid);
    let sources: Vec<&str> = r.steps.iter().map(|s| s.source.as_str()).collect();
    assert!(
        sources.contains(&"lexical") && sources.contains(&"semantic"),
        "{sources:?}"
    );
    assert_eq!(r.embedding_generation, Some(3));
    assert!(r.hits.iter().any(|h| h.path == "README.md"), "{:?}", r.hits);
    // A miss at L0 escalates to L1; with nothing to seed, it stops there.
    let r = retrieve(&src, &req("zzz_nothing_here"));
    assert_eq!(
        (r.started_at, r.ended_at),
        (Level::L0Exact, Level::L1Hybrid)
    );
    assert_eq!(r.escalations.len(), 1);
    assert_eq!(
        (r.escalations[0].from, r.escalations[0].to),
        (Level::L0Exact, Level::L1Hybrid)
    );
    assert!(r.escalations[0].reason.contains("coverage 0"));
    // A path-like query reaches the path index.
    let r = retrieve(&src, &req("money.rs"));
    assert_eq!(r.steps[0].source, "paths");
    assert_eq!(r.hits[0].path, "src/money.rs");
    assert!(r.hits[0].reasons.iter().any(|x| x == "exact_path"));
}

#[test]
fn structural_and_engineering_intents_expand_through_the_graph_with_distance_and_diagnostic_boosts()
{
    let d = repo();
    let b = build(d.path(), &[]);
    let src = Sources {
        index: &b.idx,
        lexical: &b.lx,
        symbols: &b.sx,
        semantic: &b.sem,
        graph: &b.graph,
    };
    let r = retrieve(&src, &req("callers of compute_total"));
    assert_eq!(
        (r.started_at, r.ended_at),
        (Level::L2Structural, Level::L2Structural)
    );
    let levels: Vec<Level> = r.steps.iter().map(|s| s.level).collect();
    assert!(
        levels.contains(&Level::L0Exact) && levels.contains(&Level::L1Hybrid),
        "{levels:?}"
    );
    assert!(
        r.steps
            .iter()
            .any(|s| s.source == "graph.importers" && s.candidates > 0),
        "{:?}",
        r.steps
    );
    let main = r.hits.iter().find(|h| h.path == "src/main.rs").unwrap();
    assert!(
        main.reasons
            .iter()
            .any(|x| x.starts_with("dependency_distance:")),
        "{main:?}"
    );
    assert!(main.sources.iter().any(|s| s == "graph.importers"));
    let mut r3 = req("which tests changed recently for compute_total");
    r3.diagnostics = vec![("src/money.rs".into(), 1)];
    let r = retrieve(&src, &r3);
    assert_eq!(r.ended_at, Level::L3Engineering);
    assert!(
        r.steps
            .iter()
            .any(|s| s.source == "graph.tests" && s.candidates > 0),
        "{:?}",
        r.steps
    );
    assert!(
        r.steps
            .iter()
            .any(|s| s.source == "graph.cochange" && s.candidates > 0)
    );
    assert!(r.hits.iter().any(|h| h.path == "tests/total_test.rs"));
    let money = r.hits.iter().find(|h| h.path == "src/money.rs").unwrap();
    assert!(money.reasons.iter().any(|x| x == "diagnostic"), "{money:?}");
    // Bounded.
    let mut small = req("compute_total");
    small.max_hits = 2;
    assert!(retrieve(&src, &small).hits.len() <= 2);
}
