//! PX-110: symbol reference, call and implementor edges, and impact beyond
//! tests, over hand-labelled fixture repositories (`ts-webapp`,
//! `python-service`, `rust-cli`). The labels are in `labels.json` next to the
//! fixtures: for each query the files, edges or tests a reader of the code
//! would name. The suite asserts a recall and precision floor over them,
//! prints the measured numbers, checks that ambiguity is labelled, that an
//! edit invalidates edges incrementally and correctly, and that a mutation
//! that drops implementor edges fails the interface-change queries.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use modbit_retrieval::refs::{self, EdgeKind, FileFacts, RefGraph};
use modbit_retrieval::{EvidenceGraph, RepositoryIndex, SymbolIndex, select_impacted_with_refs};
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/code-graph")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

/// Every language fixture is its own repository root.
struct Repo {
    _dir: tempfile::TempDir,
    root: PathBuf,
    index: RepositoryIndex,
    symbols: SymbolIndex,
    graph: EvidenceGraph,
    refs: RefGraph,
}

fn facts_of(index: &RepositoryIndex) -> BTreeMap<String, FileFacts> {
    index
        .texts_with_hash()
        .filter_map(|(p, t, l, h)| l.map(|l| (p.to_owned(), refs::extract(p, l, t, h))))
        .collect()
}

fn load(name: &str) -> Repo {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(name);
    copy_dir(&fixtures().join(name), &root);
    let index = RepositoryIndex::build(&root, 1).unwrap();
    let symbols = SymbolIndex::build(index.texts_with_hash(), 1);
    let graph = EvidenceGraph::build(index.texts(), vec![], Default::default(), 1);
    let refs = RefGraph::from_facts(facts_of(&index), &symbols, [], 1);
    Repo {
        _dir: dir,
        root,
        index,
        symbols,
        graph,
        refs,
    }
}

impl Repo {
    fn impact(&self, changed: &[&str], depth: u32) -> modbit_retrieval::ImpactSelection {
        let changed: Vec<String> = changed.iter().map(|s| (*s).to_owned()).collect();
        select_impacted_with_refs(
            &self.index,
            &self.symbols,
            &self.graph,
            &self.refs,
            &changed,
            depth,
            50,
        )
    }

    /// Edit a file the way a tool write does: new bytes on disk, then the
    /// indexes refresh from the changed path alone.
    fn write(&mut self, rel: &str, text: &str, revision: u64) {
        std::fs::write(self.root.join(rel), text).unwrap();
        self.index.refresh(&[rel.to_owned()], revision);
        let changed: Vec<modbit_retrieval::ChangedSymbols> = self
            .index
            .texts_with_hash()
            .filter(|(p, _, _, _)| *p == rel)
            .map(|(p, t, l, h)| {
                (
                    p.to_owned(),
                    Some((t.to_owned(), l.map(str::to_owned), h.to_owned())),
                )
            })
            .collect();
        self.symbols.refresh(&changed, revision);
        let facts: Vec<(String, Option<FileFacts>)> = self
            .index
            .texts_with_hash()
            .filter(|(p, _, _, _)| *p == rel)
            .map(|(p, t, l, h)| (p.to_owned(), l.map(|l| refs::extract(p, l, t, h))))
            .collect();
        self.refs.refresh(facts, &self.symbols, revision);
        self.graph.refresh(
            self.index
                .texts()
                .filter(|(p, _, _)| *p == rel)
                .map(|(p, t, l)| (p, Some((t, l)))),
            Default::default(),
            None,
            revision,
        );
    }
}

fn set(v: &Value) -> BTreeSet<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn labels() -> Vec<Value> {
    let text = std::fs::read_to_string(fixtures().join("labels.json")).unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["queries"]
        .as_array()
        .unwrap()
        .clone()
}

/// The recall and precision floors the qualification records (QUAL-PX-110);
/// measured values are printed and recorded in the handoff.
const RECALL_FLOOR: f64 = 0.90;
const PRECISION_FLOOR: f64 = 0.80;

#[test]
fn labelled_queries_meet_the_recall_and_precision_floors() {
    let repos: BTreeMap<&str, Repo> = [
        ("ts-webapp", load("ts-webapp")),
        ("python-service", load("python-service")),
        ("rust-cli", load("rust-cli")),
    ]
    .into_iter()
    .collect();
    let queries = labels();
    assert!(queries.len() >= 30, "{} labelled queries", queries.len());
    let (mut tp, mut fp, mut fnn) = (0usize, 0usize, 0usize);
    let (mut test_tp, mut test_fn) = (0usize, 0usize);
    let mut misses: Vec<String> = Vec::new();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for q in &queries {
        let id = q["id"].as_str().unwrap();
        let repo = &repos[q["repo"].as_str().unwrap()];
        let kind = q["kind"].as_str().unwrap();
        *by_kind.entry(kind.to_owned()).or_default() += 1;
        match kind {
            "impact" => {
                let changed: Vec<&str> = q["changed"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_str().unwrap())
                    .collect();
                let depth = q["depth"].as_u64().unwrap_or(1) as u32;
                let got = repo.impact(&changed, depth);
                let got_deps: BTreeSet<String> =
                    got.dependents.iter().map(|d| d.path.clone()).collect();
                let want = set(&q["dependents"]);
                tp += got_deps.intersection(&want).count();
                fp += got_deps.difference(&want).count();
                fnn += want.difference(&got_deps).count();
                for m in want.difference(&got_deps) {
                    misses.push(format!("{id}: missing dependent {m}"));
                }
                for x in got_deps.difference(&want) {
                    misses.push(format!("{id}: unexpected dependent {x}"));
                }
                let got_tests: BTreeSet<String> =
                    got.tests.iter().map(|t| t.path.clone()).collect();
                let want_tests = set(&q["tests"]);
                test_tp += got_tests.intersection(&want_tests).count();
                test_fn += want_tests.difference(&got_tests).count();
                for m in want_tests.difference(&got_tests) {
                    misses.push(format!("{id}: missing test {m}"));
                }
                // Every dependent carries an edge path, a confidence and the
                // revision of the graph.
                for d in &got.dependents {
                    assert!(!d.edge_path.is_empty(), "{id}: {} has no edge path", d.path);
                    assert!(
                        ["resolved", "ambiguous", "unresolved"].contains(&d.confidence.as_str()),
                        "{id}: {d:?}"
                    );
                }
                assert_eq!(got.revision, 1);
            }
            "callers" | "callees" | "references" | "implementors" | "implemented_by" => {
                let sym = q["symbol"].as_str().unwrap();
                let res = repo.refs.symbol_edges(sym, q["path"].as_str(), kind, 100);
                let got: BTreeSet<String> = res
                    .edges
                    .iter()
                    .map(|e| match kind {
                        "callees" | "implemented_by" => format!("{}:{}", e.to_path, e.to_symbol),
                        _ => format!(
                            "{}:{}",
                            e.from_path,
                            e.from_symbol.clone().unwrap_or_default()
                        ),
                    })
                    .collect();
                let want = set(&q["edges"]);
                tp += got.intersection(&want).count();
                fp += got.difference(&want).count();
                fnn += want.difference(&got).count();
                for m in want.difference(&got) {
                    misses.push(format!("{id}: missing edge {m}"));
                }
                for x in got.difference(&want) {
                    misses.push(format!("{id}: unexpected edge {x}"));
                }
            }
            other => panic!("unknown query kind {other}"),
        }
    }
    let recall = tp as f64 / (tp + fnn).max(1) as f64;
    let precision = tp as f64 / (tp + fp).max(1) as f64;
    let test_recall = test_tp as f64 / (test_tp + test_fn).max(1) as f64;
    eprintln!(
        "PX-110 impact/edge quality over {} labelled queries {by_kind:?}: recall {recall:.3} precision {precision:.3} (tp {tp} fp {fp} fn {fnn}); covering-test recall {test_recall:.3}",
        queries.len()
    );
    for m in &misses {
        eprintln!("  {m}");
    }
    assert!(
        recall >= RECALL_FLOOR,
        "recall {recall:.3} < {RECALL_FLOOR}: {misses:#?}"
    );
    assert!(
        precision >= PRECISION_FLOOR,
        "precision {precision:.3} < {PRECISION_FLOOR}: {misses:#?}"
    );
    assert!(
        test_recall >= RECALL_FLOOR,
        "test recall {test_recall:.3}: {misses:#?}"
    );
}

#[test]
fn every_edge_carries_a_confidence_class_and_the_revision_and_ambiguity_is_labelled() {
    let r = load("rust-cli");
    let mut ambiguous = 0;
    let mut resolved = 0;
    for (path, _) in r.refs.all_facts() {
        for e in r.refs.edges_from(path) {
            assert_eq!(e.revision, 1);
            match e.confidence {
                refs::Confidence::Resolved => resolved += 1,
                refs::Confidence::Ambiguous => {
                    ambiguous += 1;
                    assert!(e.candidates >= 1);
                }
                refs::Confidence::Unresolved => {
                    panic!("an unresolved occurrence is not an edge: {e:?}")
                }
            }
        }
    }
    assert!(
        resolved > 10 && ambiguous > 0,
        "resolved {resolved}, ambiguous {ambiguous}"
    );
    // `storage.save(..)` is called on a `dyn Storage`: three definitions of
    // `save` could be meant, and the edge says so.
    let calls = r.refs.symbol_edges("save", None, "callers", 20);
    assert!(
        calls.edges.iter().any(|e| e.from_path == "src/commands.rs"
            && e.confidence == refs::Confidence::Ambiguous
            && e.candidates >= 3),
        "{calls:#?}"
    );
    // Names that are not defined in the workspace are counted, not made up.
    assert!(r.refs.unresolved_count() > 0);
}

#[test]
fn an_edit_invalidates_the_changed_files_edges_and_the_answer_is_right_at_the_new_revision() {
    let mut r = load("python-service");
    let before = r.impact(&["app/pricing.py"], 1);
    let deps: Vec<&str> = before.dependents.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(deps, ["app/billing.py"], "{before:#?}");
    // Rename `total` to `grand_total`: billing still calls `total`.
    r.write(
        "app/pricing.py",
        "from app.models import Order\nfrom app.util import clamp\n\n\ndef tax(order: Order) -> float:\n    return clamp(order.price * 0.2, 0, 100)\n\n\ndef grand_total(order: Order) -> float:\n    return order.qty * order.price + tax(order)\n",
        2,
    );
    let after = r.impact(&["app/pricing.py"], 1);
    assert_eq!(after.revision, 2, "the graph answers at the new revision");
    assert_eq!(r.refs.revision(), 2);
    let billing = after
        .dependents
        .iter()
        .find(|d| d.path == "app/billing.py")
        .unwrap_or_else(|| {
            panic!("the dependent that still uses the old name is reported: {after:#?}")
        });
    assert!(
        billing
            .reasons
            .iter()
            .any(|x| x == "dangling_reference:total"),
        "{billing:#?}"
    );
    assert_eq!(billing.confidence, "unresolved");
    // The old edge is gone: nothing calls `total` any more.
    assert!(
        r.refs
            .symbol_edges("total", None, "callers", 20)
            .edges
            .is_empty(),
        "edges to a removed definition are dropped"
    );
    // The rename is followed in the dependent: its edge resolves again.
    r.write(
        "app/billing.py",
        "from app.models import Invoice\nfrom app.pricing import grand_total\n\n\ndef make_invoice(order):\n    return Invoice(grand_total(order))\n",
        3,
    );
    let callers = r.refs.symbol_edges("grand_total", None, "callers", 20);
    assert_eq!(callers.edges.len(), 1, "{callers:#?}");
    assert_eq!(callers.edges[0].from_path, "app/billing.py");
    assert_eq!(callers.edges[0].confidence, refs::Confidence::Resolved);
    assert_eq!(callers.revision, 3);
    // Equal to a fresh build at the new revision.
    let fresh = RefGraph::from_facts(facts_of(&r.index), &r.symbols, [], 3);
    for (path, _) in fresh.all_facts() {
        let mut a: Vec<_> = fresh.edges_from(path).to_vec();
        let mut b: Vec<_> = r.refs.edges_from(path).to_vec();
        a.sort_by_key(|e| (e.from_line, e.to_path.clone(), e.to_symbol.clone()));
        b.sort_by_key(|e| (e.from_line, e.to_path.clone(), e.to_symbol.clone()));
        assert_eq!(a, b, "incremental refresh equals a fresh build for {path}");
    }
    assert_eq!(fresh.unresolved_count(), r.refs.unresolved_count());
}

#[test]
fn an_interface_change_reaches_its_implementors_and_dropping_implementor_edges_fails_it() {
    let r = load("rust-cli");
    // The trait is defined in store.rs and implemented there too, so use the
    // TypeScript fixture, where the interface and its implementor are in
    // different files.
    let ts = load("ts-webapp");
    let res = ts.impact(&["src/types.ts"], 1);
    let db = res
        .dependents
        .iter()
        .find(|d| d.path == "src/db.ts")
        .unwrap_or_else(|| panic!("the implementor's file is impacted: {res:#?}"));
    assert!(db.reasons.iter().any(|r| r == "implements"), "{db:#?}");
    let imp = ts.refs.symbol_edges("Repository", None, "implementors", 20);
    assert!(
        imp.edges.iter().any(|e| e.kind == EdgeKind::Implements
            && e.from_symbol.as_deref() == Some("InMemoryUserRepo")),
        "{imp:#?}"
    );
    let ext = ts
        .refs
        .symbol_edges("InMemoryUserRepo", None, "implementors", 20);
    assert!(
        ext.edges
            .iter()
            .any(|e| e.kind == EdgeKind::Extends && e.from_symbol.as_deref() == Some("AuditedRepo")),
        "{ext:#?}"
    );
    // Rust: a trait's implementors in the same crate.
    let rs = r
        .refs
        .symbol_edges("Storage", Some("src/store.rs"), "implementors", 20);
    let who: BTreeSet<String> = rs
        .edges
        .iter()
        .filter_map(|e| e.from_symbol.clone())
        .collect();
    assert_eq!(
        who,
        ["FileStorage".to_owned(), "MemoryStorage".to_owned()].into()
    );
    // The mutation: facts with their inheritance edges removed. The same
    // query must no longer find the implementor.
    let mut facts = facts_of(&ts.index);
    for f in facts.values_mut() {
        f.heritage.clear();
    }
    let mutated = RefGraph::from_facts(facts, &ts.symbols, [], 1);
    let none = mutated.symbol_edges("Repository", None, "implementors", 20);
    assert!(none.edges.is_empty(), "the mutation removes the edges");
    let impact = select_impacted_with_refs(
        &ts.index,
        &ts.symbols,
        &ts.graph,
        &mutated,
        &["src/types.ts".to_owned()],
        1,
        50,
    );
    let db_reasons = impact
        .dependents
        .iter()
        .find(|d| d.path == "src/db.ts")
        .map(|d| d.reasons.clone())
        .unwrap_or_default();
    assert!(
        !db_reasons.iter().any(|r| r == "implements"),
        "without implementor edges the interface-change query loses the reason: {db_reasons:?}"
    );
}

#[test]
fn a_result_that_hits_the_expansion_limit_says_it_is_partial() {
    let r = load("rust-cli");
    let changed = vec!["src/store.rs".to_owned()];
    let limited =
        select_impacted_with_refs(&r.index, &r.symbols, &r.graph, &r.refs, &changed, 2, 1);
    assert_eq!(limited.dependents.len(), 1);
    assert!(
        limited.partial && limited.partial_reason.contains("cut at 1"),
        "{limited:#?}"
    );
    let full = r.impact(&["src/store.rs"], 2);
    assert!(!full.partial, "{full:#?}");
}

#[test]
fn unsupported_languages_have_no_facts_and_the_cap_bounds_a_file() {
    let f = refs::extract("a.go", "go", "package a\nfunc A() { B() }\n", "h");
    assert!(f.occurrences.is_empty() && f.bindings.is_empty());
    let big = format!("fn a() {{\n{}\n}}\n", "foo(bar);\n".repeat(30_000));
    let f = refs::extract("big.rs", "rust", &big, "h");
    assert!(f.truncated);
    assert!(f.occurrences.len() <= 20_000);
}
