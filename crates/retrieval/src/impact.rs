//! Impact-based test selection (docs/64 §6, PX-035): which tests a change
//! could break, chosen from the evidence graph — test links, import
//! dependencies, symbol references and Git co-change — within a bounded
//! depth, plus the checks the task named.
//!
//! The selection is heuristic and says so: every entry carries the evidence
//! that put it there, and the caller records precision and recall against a
//! full-suite ground truth (the benchmark harness does this for the
//! fixtures).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::graph::{EvidenceGraph, GraphQuery};
use crate::index::{RepositoryIndex, SearchOptions};
use crate::refs::{Confidence, EXPANSION_CAP, RefGraph};
use crate::symbols::SymbolIndex;

/// One selected test with the evidence that selected it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactedTest {
    /// Root-relative path of the test file.
    pub path: String,
    /// Evidence kinds that selected it, in the order they were found:
    /// `test_link`, `dependency`, `symbol_reference`, `cochange`, `changed`.
    pub reasons: Vec<String>,
    /// Distance from the changed file along import edges (0 = the file itself).
    pub distance: u32,
    /// The changed or dependent files this test covers (PX-110), when the
    /// symbol graph says; empty for a selection made without it.
    #[serde(default)]
    pub covers: Vec<String>,
}

/// One step of the path that put a dependent in an impact result (PX-110).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactStep {
    /// The file the edge starts in (the dependent side).
    pub from: String,
    /// The file the edge lands in (toward the change).
    pub to: String,
    /// `import` | `reference` | `call` | `implements` | `extends`.
    pub kind: String,
    /// `resolved` | `ambiguous` | `unresolved`.
    pub confidence: String,
    /// The symbol the edge is about.
    pub symbol: String,
    /// 1-based line of the edge in `from`.
    pub line: u32,
}

/// A non-test file a change could break, with why (PX-110).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpactedFile {
    /// Root-relative path.
    pub path: String,
    /// Ranking score (higher is more likely affected).
    pub rank: f64,
    /// Edges between the changed file and this one on the best path.
    pub distance: u32,
    /// The weakest class on the best path.
    pub confidence: String,
    /// The best path, from the dependent toward the change.
    pub edge_path: Vec<ImpactStep>,
    /// Edge kinds that reach it (`call`, `reference`, `implements`, `import`,
    /// `dangling_reference`), in the order found.
    pub reasons: Vec<String>,
    /// Tests that cover this file.
    pub tests: Vec<String>,
}

/// What a selection covered.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImpactSelection {
    /// Changed paths the selection was computed for.
    pub changed: Vec<String>,
    /// Selected tests, strongest evidence first.
    pub tests: Vec<ImpactedTest>,
    /// Depth bound used for import expansion.
    pub depth: u32,
    /// Symbols of the changed files whose references were searched.
    pub symbols: Vec<String>,
    /// Graph revision the selection was computed at.
    pub revision: u64,
    /// The honest limitation carried into the verification plan.
    pub limitation: String,
    /// Impacted non-test source files, ranked, with the edge path and
    /// confidence of each (PX-110); empty for a selection made without the
    /// symbol graph.
    #[serde(default)]
    pub dependents: Vec<ImpactedFile>,
    /// An expansion cap or the result limit cut the answer short.
    #[serde(default)]
    pub partial: bool,
    /// Why, when it did.
    #[serde(default)]
    pub partial_reason: String,
    /// Ambiguous edges the expansion walked.
    #[serde(default)]
    pub ambiguous_edges: u32,
    /// Occurrences in the workspace that resolved to no definition.
    #[serde(default)]
    pub unresolved_edges: u32,
}

/// The limitation every impact selection states until PX-035 is COMPLETE and
/// its precision and recall are recorded (docs/64 §6).
pub const HEURISTIC_LIMITATION: &str =
    "impact-based test targeting is heuristic: it never replaces the mandatory COMPLETION run";

fn push(out: &mut BTreeMap<String, ImpactedTest>, path: &str, reason: &str, distance: u32) {
    let e = out.entry(path.to_owned()).or_insert_with(|| ImpactedTest {
        path: path.to_owned(),
        reasons: vec![],
        distance,
        covers: vec![],
    });
    if !e.reasons.iter().any(|r| r == reason) {
        e.reasons.push(reason.to_owned());
    }
    e.distance = e.distance.min(distance);
}

/// Rank: more evidence first, then nearer, then by path.
fn rank(a: &ImpactedTest, b: &ImpactedTest) -> std::cmp::Ordering {
    b.reasons
        .len()
        .cmp(&a.reasons.len())
        .then(a.distance.cmp(&b.distance))
        .then(a.path.cmp(&b.path))
}

/// Select the tests a change to `changed` could break.
#[must_use]
pub fn select_impacted(
    index: &RepositoryIndex,
    symbols: &SymbolIndex,
    graph: &EvidenceGraph,
    changed: &[String],
    depth: u32,
    max: usize,
) -> ImpactSelection {
    let depth = depth.clamp(1, 3);
    let max = if max == 0 { 50 } else { max };
    let mut out: BTreeMap<String, ImpactedTest> = BTreeMap::new();
    let mut searched_symbols = Vec::new();
    for path in changed {
        // The changed file may itself be a test.
        if crate::graph::is_test_path(path) {
            push(&mut out, path, "changed", 0);
        }
        let view = graph.query(&GraphQuery {
            path: path.clone(),
            relation: "all".into(),
            depth,
            max,
        });
        // Test links: tests whose imports reach the changed file.
        for t in &view.tests {
            push(&mut out, t, "test_link", 1);
        }
        // Import dependencies: test files among the importers within the depth.
        for (p, d) in &view.importers {
            if crate::graph::is_test_path(p) {
                push(&mut out, p, "dependency", *d);
            }
        }
        // Git co-change: tests that historically changed with this file.
        for (p, _) in &view.cochange {
            if crate::graph::is_test_path(p) {
                push(&mut out, p, "cochange", depth);
            }
        }
        // Symbol references: tests that mention a symbol the changed file defines.
        for s in symbols.symbols_in(path) {
            // A short or ambiguous name is not evidence: only a definition
            // this file alone owns links a test to the change.
            if s.container.is_some() || s.name.len() < 4 {
                continue;
            }
            let unique = symbols
                .query(&crate::symbols::SymbolQuery {
                    name: Some(s.name.clone()),
                    max: 8,
                    ..crate::symbols::SymbolQuery::default()
                })
                .map(|defs| {
                    let mut files: Vec<&str> = defs.iter().map(|d| d.path.as_str()).collect();
                    files.sort_unstable();
                    files.dedup();
                    files.len() == 1
                })
                .unwrap_or(false);
            if !unique {
                continue;
            }
            if !searched_symbols.contains(&s.name) {
                searched_symbols.push(s.name.clone());
            }
            let opts = SearchOptions {
                max_hits: max,
                max_hits_per_file: 1,
                ..SearchOptions::default()
            };
            for hit in index.search_exact(&s.name, &opts) {
                if hit.path != *path && crate::graph::is_test_path(&hit.path) {
                    push(&mut out, &hit.path, "symbol_reference", 1);
                }
            }
        }
    }
    let mut tests: Vec<ImpactedTest> = out.into_values().collect();
    tests.sort_by(rank);
    tests.truncate(max);
    ImpactSelection {
        changed: changed.to_vec(),
        tests,
        depth,
        symbols: searched_symbols,
        revision: graph.revision(),
        limitation: HEURISTIC_LIMITATION.to_owned(),
        ..ImpactSelection::default()
    }
}

/// Decay of an edge's weight per step away from the change.
const STEP_DECAY: f64 = 0.6;

#[derive(Clone)]
struct Reached {
    score: f64,
    distance: u32,
    steps: Vec<ImpactStep>,
    weakest: Confidence,
    reasons: Vec<String>,
    extra_edges: u32,
}

type Frontier = Vec<(String, f64, Confidence, Vec<ImpactStep>)>;

/// What a change could break, with the symbol graph (PX-110): everything
/// [`select_impacted`] selects, plus the non-test files that reference, call,
/// implement or import what the changed files define — ranked, each with the
/// edge path and confidence that put it there — and the tests that cover
/// them. The expansion is bounded; a result that hit a bound says so
/// (`partial`). A name a changed file no longer defines that dependents still
/// use is reported as a `dangling_reference`.
#[must_use]
pub fn select_impacted_with_refs(
    index: &RepositoryIndex,
    symbols: &SymbolIndex,
    graph: &EvidenceGraph,
    refs: &RefGraph,
    changed: &[String],
    depth: u32,
    max: usize,
) -> ImpactSelection {
    let mut base = select_impacted(index, symbols, graph, changed, depth, max);
    let depth = base.depth;
    let max = if max == 0 { 50 } else { max };
    let changed_set: std::collections::BTreeSet<&str> =
        changed.iter().map(String::as_str).collect();
    let mut reached: BTreeMap<String, Reached> = BTreeMap::new();
    let mut frontier: Frontier = changed
        .iter()
        .filter(|p| !crate::graph::is_test_path(p))
        .map(|p| (p.clone(), 1.0, Confidence::Resolved, vec![]))
        .collect();
    let mut examined = 0usize;
    let mut partial_reason = String::new();
    let mut ambiguous = 0u32;
    'levels: for d in 1..=depth {
        let mut next: Frontier = Vec::new();
        for (file, score_in, weakest_in, steps_in) in &frontier {
            // Symbol edges into the file, and the file-level import edge.
            let mut incoming: Vec<(String, ImpactStep, Confidence, String)> = Vec::new();
            for e in refs.dependents_of_file(file, None) {
                incoming.push((
                    e.from_path.clone(),
                    ImpactStep {
                        from: e.from_path.clone(),
                        to: file.clone(),
                        kind: e.kind.label().to_owned(),
                        confidence: e.confidence.label().to_owned(),
                        symbol: e.to_symbol.clone(),
                        line: e.from_line,
                    },
                    e.confidence,
                    e.kind.label().to_owned(),
                ));
            }
            let view = graph.query(&GraphQuery {
                path: file.clone(),
                relation: "importers".into(),
                depth: 1,
                max: usize::MAX,
            });
            let with_symbol_edges: std::collections::BTreeSet<String> =
                incoming.iter().map(|(from, ..)| from.clone()).collect();
            for (p, _) in &view.importers {
                // A file the symbol graph reads is impacted by what it uses,
                // not by an import it never uses: an unused import (and a Rust
                // `mod x;` declaration) is no dependency, and an import that
                // is used already has its symbol edges above. File-level
                // import evidence stays for files the graph cannot read, and
                // for imports kept for their side effects.
                if let Some(f) = refs.facts(p)
                    && (f.language == "rust" || with_symbol_edges.contains(p))
                {
                    continue;
                }
                incoming.push((
                    p.clone(),
                    ImpactStep {
                        from: p.clone(),
                        to: file.clone(),
                        kind: "import".into(),
                        confidence: Confidence::Resolved.label().into(),
                        symbol: String::new(),
                        line: 0,
                    },
                    Confidence::Resolved,
                    "import".to_owned(),
                ));
            }
            for (from, step, conf, reason) in incoming {
                examined += 1;
                if examined > EXPANSION_CAP {
                    partial_reason = format!(
                        "the expansion stopped after {EXPANSION_CAP} edges; dependents beyond them are not listed"
                    );
                    break 'levels;
                }
                if changed_set.contains(from.as_str()) {
                    continue;
                }
                if conf == Confidence::Ambiguous {
                    ambiguous += 1;
                }
                let score = score_in * conf.weight() * if d > 1 { STEP_DECAY } else { 1.0 };
                let weakest = (*weakest_in).max(conf);
                let mut steps = vec![step];
                steps.extend(steps_in.iter().cloned());
                let entry = reached.entry(from.clone()).or_insert_with(|| Reached {
                    score: 0.0,
                    distance: d,
                    steps: vec![],
                    weakest,
                    reasons: vec![],
                    extra_edges: 0,
                });
                if !entry.reasons.contains(&reason) {
                    entry.reasons.push(reason);
                }
                if score > entry.score {
                    if entry.score > 0.0 {
                        entry.extra_edges += 1;
                    }
                    entry.score = score;
                    entry.distance = d;
                    entry.steps = steps.clone();
                    entry.weakest = weakest;
                } else {
                    entry.extra_edges += 1;
                }
                if d < depth {
                    next.push((from, score, weakest, steps));
                }
            }
        }
        // A file reached by several edges expands once, by its best path.
        next.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
        });
        next.dedup_by(|a, b| a.0 == b.0);
        frontier = next
            .into_iter()
            .filter(|(f, _, _, _)| !crate::graph::is_test_path(f))
            .collect();
    }
    // Names a changed file no longer defines that others still use.
    for path in changed {
        for (file, name, line) in refs.dangling_dependents(path) {
            if changed_set.contains(file.as_str()) {
                continue;
            }
            let step = ImpactStep {
                from: file.clone(),
                to: path.clone(),
                kind: "reference".into(),
                confidence: Confidence::Unresolved.label().into(),
                symbol: name.clone(),
                line,
            };
            let entry = reached.entry(file.clone()).or_insert_with(|| Reached {
                score: 0.0,
                distance: 1,
                steps: vec![step.clone()],
                weakest: Confidence::Unresolved,
                reasons: vec![],
                extra_edges: 0,
            });
            let reason = format!("dangling_reference:{name}");
            if !entry.reasons.contains(&reason) {
                entry.reasons.push(reason);
            }
            // A reference to a name that is gone is the surest sign a
            // dependent breaks: it ranks with a resolved edge.
            let score = Confidence::Resolved.weight();
            if score >= entry.score {
                entry.score = score;
                entry.distance = 1;
                entry.steps = vec![step];
                entry.weakest = Confidence::Unresolved;
            }
        }
    }
    // Split into source dependents and the tests among them.
    let mut dependents: Vec<ImpactedFile> = Vec::new();
    let mut tests: BTreeMap<String, ImpactedTest> = base
        .tests
        .iter()
        .cloned()
        .map(|t| (t.path.clone(), t))
        .collect();
    for (path, r) in &reached {
        if crate::graph::is_test_path(path) {
            let t = tests.entry(path.clone()).or_insert_with(|| ImpactedTest {
                path: path.clone(),
                reasons: vec![],
                distance: r.distance,
                covers: vec![],
            });
            for reason in &r.reasons {
                if !t.reasons.contains(reason) {
                    t.reasons.push(reason.clone());
                }
            }
            t.distance = t.distance.min(r.distance);
            if let Some(s) = r.steps.last()
                && !t.covers.contains(&s.to)
            {
                t.covers.push(s.to.clone());
            }
            continue;
        }
        dependents.push(ImpactedFile {
            path: path.clone(),
            rank: r.score + 0.02 * f64::from(r.extra_edges.min(10)),
            distance: r.distance,
            confidence: r.weakest.label().to_owned(),
            edge_path: r.steps.clone(),
            reasons: r.reasons.clone(),
            tests: vec![],
        });
    }
    dependents.sort_by(|a, b| {
        b.rank
            .partial_cmp(&a.rank)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.distance.cmp(&b.distance))
            .then(a.path.cmp(&b.path))
    });
    if dependents.len() > max {
        dependents.truncate(max);
        if partial_reason.is_empty() {
            partial_reason = format!("more than {max} dependents; the list is cut at {max}");
        }
    }
    // The tests that cover each dependent: a test with an edge into it.
    for d in &mut dependents {
        let mut covering: Vec<String> = Vec::new();
        for e in refs.dependents_of_file(&d.path, None) {
            if crate::graph::is_test_path(&e.from_path) && !covering.contains(&e.from_path) {
                covering.push(e.from_path.clone());
            }
        }
        let view = graph.query(&GraphQuery {
            path: d.path.clone(),
            relation: "tests".into(),
            depth: 1,
            max: 50,
        });
        for t in view.tests {
            if !covering.contains(&t) {
                covering.push(t);
            }
        }
        covering.sort();
        for t in &covering {
            let e = tests.entry(t.clone()).or_insert_with(|| ImpactedTest {
                path: t.clone(),
                reasons: vec![],
                distance: d.distance + 1,
                covers: vec![],
            });
            if !e.reasons.iter().any(|r| r == "covers_dependent") {
                e.reasons.push("covers_dependent".into());
            }
            if !e.covers.contains(&d.path) {
                e.covers.push(d.path.clone());
            }
        }
        d.tests = covering;
    }
    let mut tests: Vec<ImpactedTest> = tests.into_values().collect();
    tests.sort_by(rank);
    tests.truncate(max);
    base.tests = tests;
    base.dependents = dependents;
    base.partial = !partial_reason.is_empty();
    base.partial_reason = partial_reason;
    base.ambiguous_edges = ambiguous;
    base.unresolved_edges = u32::try_from(refs.unresolved_count()).unwrap_or(u32::MAX);
    base
}

/// Precision and recall of a selection against a full-suite ground truth
/// (the tests that actually fail, or the full set of tests that cover the
/// change): `(precision, recall)`, both `1.0` when the truth is empty.
#[must_use]
pub fn precision_recall(selected: &[String], truth: &[String]) -> (f32, f32) {
    if truth.is_empty() {
        return (1.0, 1.0);
    }
    if selected.is_empty() {
        return (0.0, 0.0);
    }
    let hits = selected.iter().filter(|s| truth.contains(s)).count() as f32;
    (hits / selected.len() as f32, hits / truth.len() as f32)
}
