//! L0–L3 retrieval planner and rank fusion (docs/18 "Retrieval planner",
//! "Fusion"; M3.7).
//!
//! The planner starts at the cheapest level the query's features support and
//! escalates only while coverage is insufficient (or the intent asks for a
//! structural / engineering answer). Candidates from every source are fused
//! by reciprocal rank plus deterministic boosts — exact symbol/path match,
//! workspace freshness, changed lines, diagnostic linkage and dependency
//! distance — and duplicates collapse on code-span identity. No LLM is in
//! the ranking path.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::graph::{EvidenceGraph, GraphQuery};
use crate::index::{RepositoryIndex, SearchOptions};
use crate::lexical::LexicalIndex;
use crate::semantic::SemanticIndex;
use crate::symbols::{SymbolIndex, SymbolQuery};

/// A retrieval level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    /// Known path / identifier → exact, regex, symbol.
    L0Exact,
    /// Unknown wording → BM25 + semantic + exact fusion.
    L1Hybrid,
    /// Cross-file relation → AST / dependency-graph expansion.
    L2Structural,
    /// Impact / debug → L2 + Git + diagnostics + tests + runtime evidence.
    L3Engineering,
}

impl Level {
    fn next(self) -> Option<Self> {
        match self {
            Self::L0Exact => Some(Self::L1Hybrid),
            Self::L1Hybrid => Some(Self::L2Structural),
            Self::L2Structural => Some(Self::L3Engineering),
            Self::L3Engineering => None,
        }
    }
}

/// Features the planner read from the query.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryFeatures {
    /// Single token that looks like a path or glob.
    pub path_like: bool,
    /// Single token that looks like an identifier.
    pub identifier_like: bool,
    /// More than one word.
    pub natural_language: bool,
    /// Wording asks for a cross-file relation.
    pub structural_intent: bool,
    /// Wording asks for impact / debugging / history.
    pub engineering_intent: bool,
}

/// A planned request.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanRequest {
    /// The query.
    pub query: String,
    /// `exact` | `hybrid` | `structural` | `engineering` overrides the
    /// feature-derived start; empty = derive.
    pub intent: String,
    /// Result ceiling.
    pub max_hits: usize,
    /// Unique paths considered sufficient coverage (0 = 3).
    pub min_paths: usize,
    /// Diagnostic locations (path, 1-based line) to link.
    pub diagnostics: Vec<(String, u32)>,
    /// Highest level the plan may reach (benchmark profiles); `None` = L3.
    pub max_level: Option<Level>,
}

/// One executed step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// Level.
    pub level: Level,
    /// Source (`exact` | `symbols` | `paths` | `lexical` | `semantic` |
    /// `graph.imports` | `graph.importers` | `graph.tests` |
    /// `graph.cochange` | `graph.evidence`).
    pub source: String,
    /// The query the source was asked.
    pub query: String,
    /// Candidates it returned.
    pub candidates: usize,
    /// Typed error, if the source failed.
    pub error: Option<String>,
}

/// An escalation the planner took.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Escalation {
    /// From.
    pub from: Level,
    /// To.
    pub to: Level,
    /// Why.
    pub reason: String,
}

/// A fused hit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FusedHit {
    /// Path.
    pub path: String,
    /// 1-based line range, if the span is narrower than the file.
    pub lines: Option<(u32, u32)>,
    /// Byte span, if the span is narrower than the file.
    pub span: Option<(u64, u64)>,
    /// Fused score.
    pub score: f32,
    /// Sources that produced the candidate.
    pub sources: Vec<String>,
    /// Boosts applied.
    pub reasons: Vec<String>,
    /// Content hash of the file, when a source carried it.
    pub content_hash: Option<String>,
    /// Retrieval level that first produced it.
    pub level: Level,
}

/// The plan result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanResult {
    /// Query.
    pub query: String,
    /// Features.
    pub features: QueryFeatures,
    /// Level the plan started at.
    pub started_at: Level,
    /// Level the plan ended at.
    pub ended_at: Level,
    /// Escalations taken.
    pub escalations: Vec<Escalation>,
    /// Steps executed.
    pub steps: Vec<Step>,
    /// Fused hits, best first.
    pub hits: Vec<FusedHit>,
    /// Unique paths in the hits.
    pub coverage_paths: usize,
    /// Exact index revision.
    pub index_revision: u64,
    /// Semantic embedding generation used (if L1 ran).
    pub embedding_generation: Option<u64>,
}

/// The indexes of one workspace.
pub struct Sources<'a> {
    /// Exact / regex / path index.
    pub index: &'a RepositoryIndex,
    /// BM25.
    pub lexical: &'a LexicalIndex,
    /// AST symbols.
    pub symbols: &'a SymbolIndex,
    /// Semantic chunks.
    pub semantic: &'a SemanticIndex,
    /// Evidence graph.
    pub graph: &'a EvidenceGraph,
}

const STRUCTURAL_WORDS: &[&str] = &[
    "callers",
    "caller",
    "calls",
    "importers",
    "imports",
    "import",
    "depends",
    "dependency",
    "dependencies",
    "dependents",
    "references",
    "uses",
    "used",
    "usages",
    "implements",
    "implementations",
];

const ENGINEERING_WORDS: &[&str] = &[
    "impact",
    "impacted",
    "regression",
    "failing",
    "fails",
    "failed",
    "broke",
    "broken",
    "bug",
    "debug",
    "why",
    "test",
    "tests",
    "blame",
    "changed",
    "change",
    "recent",
    "recently",
    "owner",
    "owners",
    "history",
    "diagnostic",
    "diagnostics",
    "error",
];

/// Read the query's features.
#[must_use]
pub fn features(query: &str) -> QueryFeatures {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    let single = tokens.len() == 1;
    let t = tokens.first().copied().unwrap_or_default();
    let path_like = single
        && (t.contains('/')
            || t.contains('*')
            || t.rsplit_once('.').is_some_and(|(stem, ext)| {
                !stem.is_empty()
                    && (1..=5).contains(&ext.len())
                    && ext.chars().all(|c| c.is_ascii_alphanumeric())
            }));
    let identifier_like = single
        && !path_like
        && t.chars().any(|c| c.is_ascii_alphabetic())
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '#' | '$'));
    let lower: Vec<String> = tokens
        .iter()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_ascii_lowercase()
        })
        .collect();
    let has = |set: &[&str]| lower.iter().any(|w| set.contains(&w.as_str()));
    QueryFeatures {
        path_like,
        identifier_like,
        natural_language: tokens.len() > 1,
        structural_intent: has(STRUCTURAL_WORDS),
        engineering_intent: has(ENGINEERING_WORDS),
    }
}

/// The level a query starts at, given its features and an explicit intent.
#[must_use]
pub fn start_level(f: &QueryFeatures, intent: &str) -> Level {
    match intent {
        "exact" => Level::L0Exact,
        "hybrid" => Level::L1Hybrid,
        "structural" => Level::L2Structural,
        "engineering" => Level::L3Engineering,
        _ => {
            if f.engineering_intent {
                Level::L3Engineering
            } else if f.structural_intent {
                Level::L2Structural
            } else if f.path_like || f.identifier_like {
                Level::L0Exact
            } else {
                Level::L1Hybrid
            }
        }
    }
}

/// Identifier-like tokens of a natural-language query (for exact overlay and
/// structural seeds).
fn identifier_tokens(query: &str) -> Vec<String> {
    let mut out: Vec<String> = query
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '"' | '\'' | '`'))
        .map(|t| t.trim_matches(|c: char| matches!(c, '.' | '?' | '!' | ':')))
        .filter(|t| t.len() >= 3)
        .filter(|t| {
            t.chars().any(|c| c.is_ascii_alphabetic())
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '/'))
        })
        .filter(|t| {
            t.contains('_')
                || t.contains("::")
                || t.contains('/')
                || t.contains('.')
                || t.chars().skip(1).any(|c| c.is_ascii_uppercase())
        })
        .map(str::to_owned)
        .collect();
    out.dedup();
    out
}

/// Last path/scope segment of an identifier (`a::b::c` → `c`, `a.b` → `b`).
fn leaf(name: &str) -> &str {
    name.rsplit([':', '.', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or(name)
}

#[derive(Clone, Debug)]
struct Candidate {
    path: String,
    lines: Option<(u32, u32)>,
    span: Option<(u64, u64)>,
    content_hash: Option<String>,
    source: String,
    rank: usize,
    level: Level,
    /// Dependency distance from a seed (graph sources).
    distance: Option<u32>,
}

const RRF_K: f32 = 60.0;
/// Graph-expansion candidates start this many ranks below the primary lists.
const EXPANSION_RANK_OFFSET: usize = 20;

struct Fused {
    hit: FusedHit,
    distance: Option<u32>,
}

/// Fuse ranked candidate lists (reciprocal rank) and apply the boosts.
fn fuse(
    candidates: &[Candidate],
    query: &str,
    graph: &EvidenceGraph,
    diagnostics: &[(String, u32)],
) -> Vec<FusedHit> {
    let mut by_key: BTreeMap<(String, Option<(u64, u64)>), Fused> = BTreeMap::new();
    for c in candidates {
        let key = (c.path.clone(), c.span);
        let rrf = 1.0 / (RRF_K + c.rank as f32 + 1.0);
        let e = by_key.entry(key).or_insert_with(|| Fused {
            hit: FusedHit {
                path: c.path.clone(),
                lines: c.lines,
                span: c.span,
                score: 0.0,
                sources: vec![],
                reasons: vec![],
                content_hash: c.content_hash.clone(),
                level: c.level,
            },
            distance: c.distance,
        });
        e.hit.score += rrf;
        if !e.hit.sources.contains(&c.source) {
            e.hit.sources.push(c.source.clone());
        }
        if e.hit.content_hash.is_none() {
            e.hit.content_hash = c.content_hash.clone();
        }
        if e.hit.lines.is_none() {
            e.hit.lines = c.lines;
        }
        e.distance = match (e.distance, c.distance) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if c.level < e.hit.level {
            e.hit.level = c.level;
        }
    }
    let q_leaf = leaf(query.trim()).to_ascii_lowercase();
    let q_lower = query.trim().to_ascii_lowercase();
    let mut out: Vec<FusedHit> = by_key
        .into_values()
        .map(|mut f| {
            let h = &mut f.hit;
            // Exact symbol / path match.
            if h.sources.iter().any(|s| s == "symbols") && !q_leaf.is_empty() {
                h.score += 0.05;
                h.reasons.push("exact_symbol".into());
            }
            let p_lower = h.path.to_ascii_lowercase();
            if !q_lower.is_empty()
                && (p_lower == q_lower
                    || p_lower.ends_with(&format!("/{q_lower}"))
                    || leaf(&p_lower) == q_lower)
            {
                h.score += 0.05;
                h.reasons.push("exact_path".into());
            }
            // Workspace freshness and changed lines.
            let changed = graph
                .query(&GraphQuery {
                    path: h.path.clone(),
                    relation: "changed_lines".into(),
                    depth: 1,
                    max: 0,
                })
                .changed_lines;
            if !changed.is_empty() {
                h.score += 0.015;
                h.reasons.push("fresh_in_worktree".into());
                if let Some((a, b)) = h.lines
                    && changed.iter().any(|(s, e)| *s <= b && *e >= a)
                {
                    h.score += 0.015;
                    h.reasons.push("changed_lines".into());
                }
            }
            // Diagnostic linkage.
            if diagnostics
                .iter()
                .any(|(p, l)| *p == h.path && h.lines.is_none_or(|(a, b)| (a..=b).contains(l)))
            {
                h.score += 0.03;
                h.reasons.push("diagnostic".into());
            }
            // Dependency distance: a neighbour of a seed is evidence, but never
            // outranks a hit the query itself produced (benchmark M3.9: the
            // expansion must not flood the top K).
            if let Some(d) = f.distance {
                h.score += 0.004 / d.max(1) as f32;
                h.reasons.push(format!("dependency_distance:{d}"));
            }
            f.hit
        })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.path.cmp(&b.path))
            .then(a.span.cmp(&b.span))
    });
    out
}

fn unique_paths(hits: &[FusedHit]) -> usize {
    let mut v: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    v.sort_unstable();
    v.dedup();
    v.len()
}

/// Run the plan.
#[must_use]
pub fn retrieve(src: &Sources<'_>, req: &PlanRequest) -> PlanResult {
    let max = if req.max_hits == 0 { 20 } else { req.max_hits };
    let min_paths = if req.min_paths == 0 { 3 } else { req.min_paths }.min(max);
    let f = features(&req.query);
    let ceiling = req.max_level.unwrap_or(Level::L3Engineering);
    let start = start_level(&f, &req.intent).min(ceiling);
    let mut res = PlanResult {
        query: req.query.clone(),
        features: f.clone(),
        started_at: start,
        ended_at: start,
        escalations: vec![],
        steps: vec![],
        hits: vec![],
        coverage_paths: 0,
        index_revision: src.index.revision(),
        embedding_generation: None,
    };
    let mut cands: Vec<Candidate> = Vec::new();
    let mut level = start;
    // An explicit or feature-derived L2/L3 start runs the cheaper levels first
    // so the structural expansion has seeds.
    let mut lvl = Level::L0Exact;
    loop {
        if lvl <= level {
            run_level(src, req, lvl, max, &mut cands, &mut res);
        }
        res.hits = fuse(&cands, &req.query, src.graph, &req.diagnostics);
        res.hits.truncate(max);
        res.coverage_paths = unique_paths(&res.hits);
        if lvl < level {
            lvl = lvl.next().unwrap_or(lvl);
            continue;
        }
        res.ended_at = level;
        // Escalate only while coverage is insufficient; structural expansion
        // needs seeds; L3 only by intent.
        let insufficient = res.coverage_paths < min_paths;
        let Some(next) = level.next() else { break };
        if next > ceiling {
            break;
        }
        let allowed = match next {
            Level::L0Exact => false,
            Level::L1Hybrid => insufficient,
            Level::L2Structural => insufficient && !res.hits.is_empty(),
            Level::L3Engineering => false,
        };
        if !allowed {
            break;
        }
        res.escalations.push(Escalation {
            from: level,
            to: next,
            reason: format!("coverage {} path(s) below {min_paths}", res.coverage_paths),
        });
        level = next;
        lvl = next;
    }
    res
}

fn push_step(
    res: &mut PlanResult,
    level: Level,
    source: &str,
    query: &str,
    n: usize,
    err: Option<String>,
) {
    res.steps.push(Step {
        level,
        source: source.into(),
        query: query.into(),
        candidates: n,
        error: err,
    });
}

fn run_level(
    src: &Sources<'_>,
    req: &PlanRequest,
    level: Level,
    max: usize,
    cands: &mut Vec<Candidate>,
    res: &mut PlanResult,
) {
    let q = req.query.trim();
    let f = res.features.clone();
    match level {
        Level::L0Exact => {
            if f.path_like {
                let glob = if q.contains('*') {
                    q.to_owned()
                } else {
                    format!("**/*{q}*")
                };
                match src.index.find_paths(&glob, max) {
                    Ok(hits) => {
                        let n = hits.len();
                        for (i, h) in hits.into_iter().enumerate() {
                            cands.push(Candidate {
                                path: h.path,
                                lines: None,
                                span: None,
                                content_hash: None,
                                source: "paths".into(),
                                rank: i,
                                level,
                                distance: None,
                            });
                        }
                        push_step(res, level, "paths", &glob, n, None);
                    }
                    Err(e) => push_step(res, level, "paths", &glob, 0, Some(e.to_string())),
                }
            }
            let needles: Vec<String> = if f.natural_language {
                identifier_tokens(q)
            } else {
                vec![q.to_owned()]
            };
            for needle in needles {
                let opts = SearchOptions {
                    max_hits: max,
                    max_hits_per_file: 5,
                    ..SearchOptions::default()
                };
                let hits = src.index.search_exact(&needle, &opts);
                let n = hits.len();
                for (i, h) in hits.into_iter().enumerate() {
                    cands.push(Candidate {
                        path: h.path,
                        lines: Some((h.line, h.line)),
                        span: Some(h.span),
                        content_hash: Some(h.content_hash),
                        source: "exact".into(),
                        rank: i,
                        level,
                        distance: None,
                    });
                }
                push_step(res, level, "exact", &needle, n, None);
                let name = leaf(&needle).to_owned();
                match src.symbols.query(&SymbolQuery {
                    name: Some(name.clone()),
                    max,
                    ..SymbolQuery::default()
                }) {
                    Ok(syms) => {
                        let n = syms.len();
                        for (i, s) in syms.into_iter().enumerate() {
                            cands.push(Candidate {
                                path: s.path,
                                lines: Some((s.line_start, s.line_end)),
                                span: Some(s.span),
                                content_hash: Some(s.content_hash),
                                source: "symbols".into(),
                                rank: i,
                                level,
                                distance: None,
                            });
                        }
                        push_step(res, level, "symbols", &name, n, None);
                    }
                    Err(e) => push_step(res, level, "symbols", &name, 0, Some(e)),
                }
            }
        }
        Level::L1Hybrid => {
            match src.lexical.search(q, max) {
                Ok(hits) => {
                    let n = hits.len();
                    for (i, h) in hits.into_iter().enumerate() {
                        cands.push(Candidate {
                            path: h.path,
                            lines: None,
                            span: None,
                            content_hash: None,
                            source: "lexical".into(),
                            rank: i,
                            level,
                            distance: None,
                        });
                    }
                    push_step(res, level, "lexical", q, n, None);
                }
                Err(e) => push_step(res, level, "lexical", q, 0, Some(e.to_string())),
            }
            match src.semantic.search(q, max) {
                Ok(hits) => {
                    res.embedding_generation = Some(src.semantic.generation());
                    let n = hits.len();
                    for (i, h) in hits.into_iter().enumerate() {
                        cands.push(Candidate {
                            path: h.chunk.path,
                            lines: Some(h.chunk.lines),
                            span: Some(h.chunk.span),
                            content_hash: None,
                            source: "semantic".into(),
                            rank: i,
                            level,
                            distance: None,
                        });
                    }
                    push_step(res, level, "semantic", q, n, None);
                }
                Err(e) => push_step(res, level, "semantic", q, 0, Some(e)),
            }
            // Exact overlay for identifier-like tokens not already run at L0.
            if f.natural_language && res.started_at != Level::L0Exact {
                for needle in identifier_tokens(q) {
                    let opts = SearchOptions {
                        max_hits: max,
                        max_hits_per_file: 5,
                        ..SearchOptions::default()
                    };
                    let hits = src.index.search_exact(&needle, &opts);
                    let n = hits.len();
                    for (i, h) in hits.into_iter().enumerate() {
                        cands.push(Candidate {
                            path: h.path,
                            lines: Some((h.line, h.line)),
                            span: Some(h.span),
                            content_hash: Some(h.content_hash),
                            source: "exact".into(),
                            rank: i,
                            level,
                            distance: None,
                        });
                    }
                    push_step(res, level, "exact", &needle, n, None);
                }
            }
        }
        Level::L2Structural | Level::L3Engineering => {
            let seeds: Vec<String> = {
                let mut s: Vec<String> = res.hits.iter().map(|h| h.path.clone()).collect();
                s.dedup();
                s.truncate(3);
                s
            };
            let relations: &[&str] = if level == Level::L2Structural {
                &["imports", "importers"]
            } else {
                &["tests", "cochange", "evidence"]
            };
            for seed in &seeds {
                for rel in relations {
                    let view = src.graph.query(&GraphQuery {
                        path: seed.clone(),
                        relation: (*rel).to_owned(),
                        depth: 2,
                        max,
                    });
                    let neighbours: Vec<(String, u32)> = match *rel {
                        "imports" => view.imports,
                        "importers" => view.importers,
                        "tests" => view.tests.into_iter().map(|t| (t, 1)).collect(),
                        "cochange" => view.cochange.into_iter().map(|(p, _)| (p, 1)).collect(),
                        _ => {
                            // Evidence keeps the seed itself (a file with run evidence
                            // is a stronger candidate).
                            if view.evidence.is_empty() {
                                vec![]
                            } else {
                                vec![(seed.clone(), 0)]
                            }
                        }
                    };
                    let n = neighbours.len();
                    for (i, (p, d)) in neighbours.into_iter().enumerate() {
                        cands.push(Candidate {
                            path: p,
                            lines: None,
                            span: None,
                            content_hash: None,
                            source: format!("graph.{rel}"),
                            // Expansion ranks below the primary lists.
                            rank: i + EXPANSION_RANK_OFFSET,
                            level,
                            distance: Some(d.max(1)),
                        });
                    }
                    push_step(res, level, &format!("graph.{rel}"), seed, n, None);
                }
            }
        }
    }
}
