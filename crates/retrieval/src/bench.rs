//! Retrieval benchmark harness (docs/18 "Benchmark gates inherited from
//! project research"; M3.9).
//!
//! Same corpus, same cases, three profiles — A baseline exact/search tools
//! (L0 only), B hybrid exact + BM25 + vector (up to L1), C the structural
//! context engine (the full L0–L3 planner) — measured on relevant-file
//! recall@K, evidence precision@K, changed-code impact accuracy, retrieval
//! steps (tool calls), latency, cold-index time and incremental-index
//! latency. The report states what was measured on which corpus revision;
//! the docs/18 reduction targets stay targets until a same-model,
//! same-task run reproduces them.

use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::graph::EvidenceGraph;
use crate::index::RepositoryIndex;
use crate::lexical::LexicalIndex;
use crate::planner::{Level, PlanRequest, Sources, retrieve};
use crate::semantic::{HashingEmbedder, SemanticIndex};
use crate::symbols::SymbolIndex;

/// One benchmark case.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    /// Id.
    pub id: String,
    /// Query.
    pub query: String,
    /// Explicit intent (optional).
    #[serde(default)]
    pub intent: String,
    /// Relevant paths (ground truth).
    pub relevant: Vec<String>,
    /// For impact cases: the paths a change to `relevant[0]` should surface.
    #[serde(default)]
    pub impacted: Vec<String>,
}

/// A retrieval profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Profile {
    /// Baseline exact/search tools (L0).
    ABaseline,
    /// Hybrid exact + BM25 + vector (up to L1).
    BHybrid,
    /// Modbit structural context engine (L0–L3).
    CStructural,
    /// BM25 only (a lexical-only baseline for the fusion A/B).
    LexicalOnly,
    /// Vectors only (a semantic-only baseline for the fusion A/B).
    SemanticOnly,
}

impl Profile {
    /// All profiles, in order.
    pub const ALL: [Self; 5] = [
        Self::ABaseline,
        Self::BHybrid,
        Self::CStructural,
        Self::LexicalOnly,
        Self::SemanticOnly,
    ];

    fn request(self, case: &Case, k: usize) -> PlanRequest {
        let (intent, max_level) = match self {
            Self::ABaseline => ("exact".to_owned(), Some(Level::L0Exact)),
            Self::BHybrid => ("hybrid".to_owned(), Some(Level::L1Hybrid)),
            Self::CStructural => (case.intent.clone(), None),
            Self::LexicalOnly | Self::SemanticOnly => ("hybrid".to_owned(), Some(Level::L1Hybrid)),
        };
        PlanRequest {
            query: case.query.clone(),
            intent,
            max_hits: k.max(1) * 3,
            min_paths: 0,
            diagnostics: vec![],
            max_level,
        }
    }
}

/// One case under one profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaseResult {
    /// Case.
    pub case_id: String,
    /// Profile.
    pub profile: Profile,
    /// Ranked unique paths (top K).
    pub paths: Vec<String>,
    /// Relevant paths found in the top K / relevant paths.
    pub recall_at_k: f32,
    /// Relevant paths in the top K / paths returned (evidence precision).
    pub precision_at_k: f32,
    /// Impacted paths found in the top K / impacted paths (`None` when the case has none).
    pub impact_accuracy: Option<f32>,
    /// Retrieval steps executed (tool calls a baseline agent would make).
    pub steps: usize,
    /// Level the plan ended at.
    pub ended_at: Level,
    /// Wall time.
    pub latency_ms: f64,
    /// Estimated tokens (bytes/4) of the whole files in the top K: the context
    /// cost of handing the agent those files without packing.
    pub context_tokens_at_k: u64,
}

/// Aggregate of one profile.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProfileSummary {
    /// Profile.
    pub profile: Option<Profile>,
    /// Mean recall@K.
    pub mean_recall_at_k: f32,
    /// Mean precision@K.
    pub mean_precision_at_k: f32,
    /// Mean impact accuracy over impact cases.
    pub mean_impact_accuracy: Option<f32>,
    /// Mean steps.
    pub mean_steps: f32,
    /// Mean latency.
    pub mean_latency_ms: f64,
    /// Cases with every relevant path in the top K.
    pub full_recall_cases: usize,
    /// Mean estimated context tokens of the top K files.
    pub mean_context_tokens_at_k: f64,
}

/// Cold-index times.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexTimes {
    /// Exact/regex/path index.
    pub exact_ms: f64,
    /// BM25.
    pub lexical_ms: f64,
    /// AST symbols.
    pub symbols_ms: f64,
    /// Semantic chunks.
    pub semantic_ms: f64,
    /// Evidence graph.
    pub graph_ms: f64,
    /// Total.
    pub total_ms: f64,
}

/// Impact-selection measurement for one case (PX-035, docs/64 §6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpactResult {
    /// Case.
    pub case_id: String,
    /// The changed file the selection was computed for.
    pub changed: String,
    /// Tests the selector chose.
    pub selected: Vec<String>,
    /// The evidence kinds behind them.
    pub reasons: Vec<String>,
    /// Ground truth: the tests the full suite says cover the change.
    pub truth: Vec<String>,
    /// Precision against the ground truth.
    pub precision: f32,
    /// Recall against the ground truth.
    pub recall: f32,
}

/// The report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Harness version.
    pub harness_version: String,
    /// Corpus description (as given by the caller).
    pub corpus: String,
    /// Files in the exact index.
    pub files: usize,
    /// Searchable bytes.
    pub bytes: u64,
    /// K.
    pub k: usize,
    /// Cold-index times.
    pub cold_index: IndexTimes,
    /// Incremental refresh of one changed file across all indexes.
    pub incremental_ms: f64,
    /// The file refreshed for the incremental measurement.
    pub incremental_path: String,
    /// Per-profile summaries.
    pub profiles: Vec<ProfileSummary>,
    /// Per-case results.
    pub cases: Vec<CaseResult>,
    /// Impact-selection measurements for the cases that carry ground truth.
    pub impact: Vec<ImpactResult>,
    /// Mean impact precision and recall.
    pub impact_precision: f32,
    /// Mean impact recall.
    pub impact_recall: f32,
    /// Method note.
    pub method: String,
}

/// Harness version.
pub const HARNESS_VERSION: &str = "retrieval-bench-v1";

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn frac(found: usize, total: usize) -> f32 {
    if total == 0 {
        1.0
    } else {
        found as f32 / total as f32
    }
}

/// Run the cases against the repository at `root` (a Git worktree).
///
/// # Errors
/// Index build failures.
pub fn run(root: &Path, corpus: &str, cases: &[Case], k: usize) -> Result<Report, String> {
    let k = k.max(1);
    let t0 = Instant::now();
    let t = Instant::now();
    let mut idx = RepositoryIndex::build(root, 1).map_err(|e| e.to_string())?;
    let exact_ms = ms(t);
    let t = Instant::now();
    let mut lexical = LexicalIndex::build(idx.texts(), 1).map_err(|e| e.to_string())?;
    let lexical_ms = ms(t);
    let t = Instant::now();
    let mut symbols = SymbolIndex::build(idx.texts_with_hash(), 1);
    let symbols_ms = ms(t);
    let t = Instant::now();
    let spans = |sx: &SymbolIndex, p: &str| {
        sx.symbols_in(p)
            .iter()
            .filter(|s| s.container.is_none())
            .map(|s| (s.name.clone(), s.span.0, s.span.1))
            .collect::<Vec<_>>()
    };
    let files: Vec<crate::semantic::FileSource> = idx
        .texts()
        .map(|(p, t, _)| (p, t, spans(&symbols, p)))
        .collect();
    let mut semantic =
        SemanticIndex::build(Box::new(HashingEmbedder::default()), files.into_iter(), 1)?;
    let semantic_ms = ms(t);
    let t = Instant::now();
    let mut graph = EvidenceGraph::build(idx.texts(), vec![], Default::default(), 1);
    let graph_ms = ms(t);
    let cold_index = IndexTimes {
        exact_ms,
        lexical_ms,
        symbols_ms,
        semantic_ms,
        graph_ms,
        total_ms: ms(t0),
    };
    let stats = idx.stats();

    // Incremental: refresh one real file (the first relevant path, or the
    // first file) through every index at revision 2.
    let incremental_path = cases
        .first()
        .and_then(|c| c.relevant.first().cloned())
        .or_else(|| idx.texts().next().map(|(p, _, _)| p.to_owned()))
        .unwrap_or_default();
    let t = Instant::now();
    if !incremental_path.is_empty() {
        idx.refresh(std::slice::from_ref(&incremental_path), 2);
        let text = idx
            .texts()
            .find(|(p, _, _)| *p == incremental_path.as_str())
            .map(|(_, t, l)| (t.to_owned(), l.map(str::to_owned)));
        let hashed = idx
            .texts_with_hash()
            .find(|(p, _, _, _)| *p == incremental_path.as_str())
            .map(|(_, t, l, h)| (t.to_owned(), l.map(str::to_owned), h.to_owned()));
        lexical
            .refresh(&[(incremental_path.clone(), text.clone())], 2)
            .map_err(|e| e.to_string())?;
        symbols.refresh(&[(incremental_path.clone(), hashed)], 2);
        semantic.mark_changed(std::slice::from_ref(&incremental_path));
        let sp = spans(&symbols, &incremental_path);
        semantic.flush(
            [(
                incremental_path.as_str(),
                text.as_ref().map(|(t, _)| (t.as_str(), sp.clone())),
            )]
            .into_iter(),
            2,
        )?;
        graph.refresh(
            [(
                incremental_path.as_str(),
                text.as_ref().map(|(t, l)| (t.as_str(), l.as_deref())),
            )]
            .into_iter(),
            Default::default(),
            None,
            2,
        );
    }
    let incremental_ms = ms(t);

    let src = Sources {
        index: &idx,
        lexical: &lexical,
        symbols: &symbols,
        semantic: &semantic,
        graph: &graph,
    };
    let mut results = Vec::new();
    for case in cases {
        for profile in Profile::ALL {
            let req = profile.request(case, k);
            let t = Instant::now();
            let (ranked, steps, ended_at): (Vec<String>, usize, Level) = match profile {
                Profile::LexicalOnly => (
                    lexical
                        .search(&case.query, k * 3)
                        .map(|hits| hits.into_iter().map(|h| h.path).collect())
                        .unwrap_or_default(),
                    1,
                    Level::L1Hybrid,
                ),
                Profile::SemanticOnly => (
                    semantic
                        .search(&case.query, k * 3)
                        .map(|hits| hits.into_iter().map(|h| h.chunk.path).collect())
                        .unwrap_or_default(),
                    1,
                    Level::L1Hybrid,
                ),
                _ => {
                    let plan = retrieve(&src, &req);
                    (
                        plan.hits.iter().map(|h| h.path.clone()).collect(),
                        plan.steps.len(),
                        plan.ended_at,
                    )
                }
            };
            let latency_ms = ms(t);
            let mut paths: Vec<String> = Vec::new();
            for p in ranked {
                if !paths.contains(&p) {
                    paths.push(p);
                }
                if paths.len() >= k {
                    break;
                }
            }
            let context_tokens_at_k: u64 = paths
                .iter()
                .map(|p| {
                    idx.texts()
                        .find(|(path, _, _)| path == p)
                        .map_or(0, |(_, t, _)| t.len().div_ceil(4) as u64)
                })
                .sum();
            let found = case.relevant.iter().filter(|p| paths.contains(p)).count();
            let impact = if case.impacted.is_empty() {
                None
            } else {
                Some(frac(
                    case.impacted.iter().filter(|p| paths.contains(p)).count(),
                    case.impacted.len(),
                ))
            };
            results.push(CaseResult {
                case_id: case.id.clone(),
                profile,
                recall_at_k: frac(found, case.relevant.len()),
                precision_at_k: if paths.is_empty() {
                    0.0
                } else {
                    found as f32 / paths.len() as f32
                },
                impact_accuracy: impact,
                steps,
                ended_at,
                latency_ms,
                paths,
                context_tokens_at_k,
            });
        }
    }
    // PX-035: impact selection measured against the cases' ground truth.
    let mut impact = Vec::new();
    for case in cases.iter().filter(|c| !c.impacted.is_empty()) {
        let Some(changed) = case.relevant.first().cloned() else {
            continue;
        };
        let sel = crate::impact::select_impacted(
            &idx,
            &symbols,
            &graph,
            std::slice::from_ref(&changed),
            2,
            k * 4,
        );
        let selected: Vec<String> = sel.tests.iter().map(|t| t.path.clone()).collect();
        let mut reasons: Vec<String> = sel.tests.iter().flat_map(|t| t.reasons.clone()).collect();
        reasons.sort();
        reasons.dedup();
        let (precision, recall) = crate::impact::precision_recall(&selected, &case.impacted);
        impact.push(ImpactResult {
            case_id: case.id.clone(),
            changed,
            selected,
            reasons,
            truth: case.impacted.clone(),
            precision,
            recall,
        });
    }
    let n_impact = impact.len().max(1) as f32;
    let impact_precision = impact.iter().map(|i| i.precision).sum::<f32>() / n_impact;
    let impact_recall = impact.iter().map(|i| i.recall).sum::<f32>() / n_impact;
    let profiles = Profile::ALL
        .iter()
        .map(|&profile| {
            let rs: Vec<&CaseResult> = results.iter().filter(|r| r.profile == profile).collect();
            let n = rs.len().max(1) as f32;
            let impacts: Vec<f32> = rs.iter().filter_map(|r| r.impact_accuracy).collect();
            ProfileSummary {
                profile: Some(profile),
                mean_recall_at_k: rs.iter().map(|r| r.recall_at_k).sum::<f32>() / n,
                mean_precision_at_k: rs.iter().map(|r| r.precision_at_k).sum::<f32>() / n,
                mean_impact_accuracy: (!impacts.is_empty())
                    .then(|| impacts.iter().sum::<f32>() / impacts.len() as f32),
                mean_steps: rs.iter().map(|r| r.steps as f32).sum::<f32>() / n,
                mean_latency_ms: rs.iter().map(|r| r.latency_ms).sum::<f64>() / f64::from(n),
                full_recall_cases: rs.iter().filter(|r| r.recall_at_k >= 1.0).count(),
                mean_context_tokens_at_k: rs
                    .iter()
                    .map(|r| r.context_tokens_at_k as f64)
                    .sum::<f64>()
                    / f64::from(n),
            }
        })
        .collect();
    Ok(Report {
        harness_version: HARNESS_VERSION.into(),
        corpus: corpus.to_owned(),
        files: stats.files,
        bytes: stats.bytes,
        k,
        cold_index,
        incremental_ms,
        incremental_path,
        profiles,
        cases: results,
        impact,
        impact_precision,
        impact_recall,
        method: "Impact selection (PX-035) is measured against each case's ground truth and is heuristic: it never replaces the mandatory COMPLETION run. Same corpus and cases for every profile; A = L0 exact/symbol/path only, B = up to L1 (BM25 + hashing-v1 vectors + exact overlay), C = the full L0–L3 planner with graph expansion; LexicalOnly = BM25 alone and SemanticOnly = hashing-v1 vectors alone (fusion baselines). context_tokens_at_k is the bytes/4 estimate of the whole top-K files. Metrics are measured here; the docs/18 token/tool-call/agent-time reduction targets are not claimed by this report (they need a same-model, same-task agent run).".into(),
    })
}
