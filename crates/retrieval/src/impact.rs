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
}

/// What a selection covered.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    }
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
