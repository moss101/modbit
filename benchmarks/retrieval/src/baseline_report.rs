//! The retrieval report with an external baseline column (REQ-PX-137): the
//! planner profiles of `modbit_retrieval::bench` and the real ripgrep (and
//! zvec-grep, when installed) profiles run over the same corpus and the same
//! hand-labelled cases, scored by one function from the ranked paths each
//! returned, with Wilson intervals on proportions and bootstrap intervals on
//! paired differences.
//!
//! What is measured here: recall at 1, 5 and 10 (labels found among the top
//! results, pooled over cases), the share of cases answered completely, recall
//! inside a token budget (the top results are taken whole-file in rank order
//! until the budget is spent), latency, retrieval steps (tool calls) and the
//! tokens the top five files would cost. What is not: how an agent behaves
//! when given these results. That needs the same model on the same tasks and
//! is reported as `LIVE: NOT RUN`.

use std::path::Path;

use modbit_bench_context_economics::bootstrap_ci95;
use modbit_bench_context_economics::mean;
use modbit_bench_context_economics::projection_trial::{Proportion, wilson};
use modbit_retrieval::bench::{Case, IndexTimes, Profile, run};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::external::{
    BaselineMissing, detect_rg_from_env, detect_zvec_from_env, rg_answer, zvec_answer,
};

/// Marker for the part of the claim that needs a live provider.
pub const LIVE_MARKER: &str = "LIVE: NOT RUN - same-model agent accuracy, input tokens and tool calls with each profile (QUAL-PX-137) need a configured provider; the columns here are retrieval-only and say nothing about an agent's answers";

/// Token budgets the budgeted recall is measured at.
pub const BUDGETS: [u64; 3] = [2_000, 8_000, 32_000];

/// The most results any profile is asked for.
pub const K: usize = 10;

/// What happened to a column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "detail", rename_all = "snake_case")]
pub enum Status {
    /// The profile ran on every case.
    Ran,
    /// An optional tool is not installed; the column is empty and says so.
    NotInstalled(String),
}

/// One column of the report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    /// Profile name.
    pub name: String,
    /// `planner` | `external`.
    pub kind: String,
    /// Whether it ran.
    pub status: Status,
    /// Recall at 1, 5 and 10: labels found / labels, pooled over cases.
    pub recall: Vec<(usize, Proportion)>,
    /// Cases whose every label is in the top 5 / top 10.
    pub full_recall: Vec<(usize, Proportion)>,
    /// Recall within a token budget, per budget.
    pub recall_at_budget: Vec<(u64, Proportion)>,
    /// Mean latency per case.
    pub mean_latency_ms: f64,
    /// Median latency per case.
    pub median_latency_ms: f64,
    /// Mean retrieval steps (tool calls) per case.
    pub mean_steps: f64,
    /// Mean tokens of the top five files, whole.
    pub mean_tokens_top5: f64,
}

/// A paired comparison of two columns on per-case recall at 10.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pairwise {
    /// The profile under test.
    pub treatment: String,
    /// The baseline.
    pub baseline: String,
    /// Cases where the treatment found more labels.
    pub wins: usize,
    /// Cases where it found fewer.
    pub losses: usize,
    /// Cases with no difference.
    pub ties: usize,
    /// Mean per-case difference in recall at 10.
    pub mean_delta: f64,
    /// Deterministic bootstrap 95% interval of the mean difference.
    pub ci95: (f64, f64),
}

/// One corpus.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CorpusReport {
    /// Name.
    pub corpus: String,
    /// Files indexed.
    pub files: usize,
    /// Searchable bytes.
    pub bytes: u64,
    /// Hand-labelled cases.
    pub cases: usize,
    /// Cold-index times of the planner's indexes.
    pub cold_index: IndexTimes,
    /// Refreshing one file across every index.
    pub incremental_ms: f64,
    /// Columns.
    pub columns: Vec<Column>,
    /// Every planner profile against ripgrep.
    pub pairwise: Vec<Pairwise>,
    /// sha256 over `(corpus, profile, case, ranked paths)`: the scoring input.
    pub ranking_digest: String,
}

/// The whole report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BaselineReport {
    /// Corpora.
    pub corpora: Vec<CorpusReport>,
    /// What a live provider would have measured.
    pub live: String,
    /// Where the labels come from.
    pub labels: String,
    /// What the numbers do not establish.
    pub method: String,
}

struct Ranked {
    name: String,
    kind: &'static str,
    status: Status,
    /// Per case, in case order: ranked paths, steps, latency.
    cases: Vec<(Vec<String>, usize, f64)>,
}

fn tokens_of(root: &Path, rel: &str) -> u64 {
    std::fs::metadata(root.join(rel)).map_or(0, |m| m.len().div_ceil(4))
}

fn found(ranked: &[String], k: usize, relevant: &[String]) -> usize {
    relevant
        .iter()
        .filter(|r| ranked.iter().take(k).any(|p| p == *r))
        .count()
}

fn in_budget(root: &Path, ranked: &[String], budget: u64, relevant: &[String]) -> usize {
    let (mut spent, mut kept) = (0u64, Vec::new());
    for p in ranked {
        let t = tokens_of(root, p);
        if spent + t > budget {
            break;
        }
        spent += t;
        kept.push(p.clone());
    }
    found(&kept, kept.len(), relevant)
}

fn column(root: &Path, cases: &[Case], r: &Ranked) -> Column {
    if let Status::NotInstalled(_) = r.status {
        return Column {
            name: r.name.clone(),
            kind: r.kind.into(),
            status: r.status.clone(),
            recall: vec![],
            full_recall: vec![],
            recall_at_budget: vec![],
            mean_latency_ms: f64::NAN,
            median_latency_ms: f64::NAN,
            mean_steps: f64::NAN,
            mean_tokens_top5: f64::NAN,
        };
    }
    let labels: usize = cases.iter().map(|c| c.relevant.len()).sum();
    let recall = [1usize, 5, 10]
        .into_iter()
        .map(|k| {
            let hit: usize = cases
                .iter()
                .zip(&r.cases)
                .map(|(c, (p, _, _))| found(p, k, &c.relevant))
                .sum();
            (k, wilson(hit, labels))
        })
        .collect();
    let full_recall = [5usize, 10]
        .into_iter()
        .map(|k| {
            let hit = cases
                .iter()
                .zip(&r.cases)
                .filter(|(c, (p, _, _))| found(p, k, &c.relevant) == c.relevant.len())
                .count();
            (k, wilson(hit, cases.len()))
        })
        .collect();
    let recall_at_budget = BUDGETS
        .into_iter()
        .map(|b| {
            let hit: usize = cases
                .iter()
                .zip(&r.cases)
                .map(|(c, (p, _, _))| in_budget(root, p, b, &c.relevant))
                .sum();
            (b, wilson(hit, labels))
        })
        .collect();
    let lat: Vec<f64> = r.cases.iter().map(|c| c.2).collect();
    let steps: Vec<f64> = r.cases.iter().map(|c| c.1 as f64).collect();
    let top5: Vec<f64> = r
        .cases
        .iter()
        .map(|(p, _, _)| p.iter().take(5).map(|x| tokens_of(root, x)).sum::<u64>() as f64)
        .collect();
    Column {
        name: r.name.clone(),
        kind: r.kind.into(),
        status: r.status.clone(),
        recall,
        full_recall,
        recall_at_budget,
        mean_latency_ms: mean(&lat),
        median_latency_ms: modbit_bench_context_economics::median(lat),
        mean_steps: mean(&steps),
        mean_tokens_top5: mean(&top5),
    }
}

fn recall10(cases: &[Case], r: &Ranked) -> Vec<f64> {
    cases
        .iter()
        .zip(&r.cases)
        .map(|(c, (p, _, _))| found(p, 10, &c.relevant) as f64 / c.relevant.len().max(1) as f64)
        .collect()
}

/// Run the planner profiles and the external baselines over one corpus.
///
/// # Errors
/// `BaselineMissing` when ripgrep is not installed (the run fails; it is not
/// reported without the column), or a failure of the planner or a process.
pub fn evaluate_corpus(root: &Path, corpus: &str, cases: &[Case]) -> Result<CorpusReport, String> {
    let rg = detect_rg_from_env().map_err(|e: BaselineMissing| e.to_string())?;
    let zvec = detect_zvec_from_env();
    let planner = run(root, corpus, cases, K)?;
    let mut ranked: Vec<Ranked> = Profile::ALL
        .iter()
        .map(|p| Ranked {
            name: format!("{p:?}"),
            kind: "planner",
            status: Status::Ran,
            cases: cases
                .iter()
                .map(|c| {
                    let r = planner
                        .cases
                        .iter()
                        .find(|x| x.profile == *p && x.case_id == c.id);
                    r.map_or((vec![], 0, 0.0), |r| {
                        (r.paths.clone(), r.steps, r.latency_ms)
                    })
                })
                .collect(),
        })
        .collect();
    let mut rg_cases = Vec::new();
    for c in cases {
        let a = rg_answer(&rg, root, &c.query, K)?;
        rg_cases.push((a.paths, a.steps, a.latency_ms));
    }
    ranked.push(Ranked {
        name: "ExternalRipgrep".into(),
        kind: "external",
        status: Status::Ran,
        cases: rg_cases,
    });
    match zvec {
        Some(z) => {
            let mut zc = Vec::new();
            for c in cases {
                let a = zvec_answer(&z, root, &c.query, K)?;
                zc.push((a.paths, a.steps, a.latency_ms));
            }
            ranked.push(Ranked {
                name: "ExternalZvecGrep".into(),
                kind: "external",
                status: Status::Ran,
                cases: zc,
            });
        }
        None => ranked.push(Ranked {
            name: "ExternalZvecGrep".into(),
            kind: "external",
            status: Status::NotInstalled(
                "zvec-grep was not found on PATH or in MODBIT_ZVEC_GREP_BIN; it is detected, never downloaded".into(),
            ),
            cases: vec![],
        }),
    }
    let columns: Vec<Column> = ranked.iter().map(|r| column(root, cases, r)).collect();
    let rg_deltas = ranked
        .iter()
        .find(|r| r.name == "ExternalRipgrep")
        .map(|r| recall10(cases, r))
        .unwrap_or_default();
    let pairwise = ranked
        .iter()
        .filter(|r| r.kind == "planner")
        .map(|r| {
            let t = recall10(cases, r);
            let d: Vec<f64> = t.iter().zip(&rg_deltas).map(|(a, b)| a - b).collect();
            Pairwise {
                treatment: r.name.clone(),
                baseline: "ExternalRipgrep".into(),
                wins: d.iter().filter(|x| **x > 0.0).count(),
                losses: d.iter().filter(|x| **x < 0.0).count(),
                ties: d.iter().filter(|x| **x == 0.0).count(),
                mean_delta: mean(&d),
                ci95: bootstrap_ci95(&d),
            }
        })
        .collect();
    let mut h = Sha256::new();
    for r in &ranked {
        for (c, (paths, _, _)) in cases.iter().zip(&r.cases) {
            h.update(format!("{corpus}|{}|{}|{}\n", r.name, c.id, paths.join(",")).as_bytes());
        }
    }
    Ok(CorpusReport {
        corpus: corpus.to_owned(),
        files: planner.files,
        bytes: planner.bytes,
        cases: cases.len(),
        cold_index: planner.cold_index,
        incremental_ms: planner.incremental_ms,
        columns,
        pairwise,
        ranking_digest: hex::encode(h.finalize()),
    })
}

/// Assemble the report text fields around corpus reports.
#[must_use]
pub fn assemble(corpora: Vec<CorpusReport>) -> BaselineReport {
    BaselineReport {
        corpora,
        live: LIVE_MARKER.into(),
        labels: "Every label is author-written: the synthetic corpus labels are the files its generator wrote for each row of a hand-written feature table; the repository labels were written by hand from the modules' own headers and checked to exist. The repository cases are easier than real task descriptions because their words come from the files they find, which favours every lexical profile alike. No label was derived from any profile's output.".into(),
        method: "Same corpus, same cases, same scoring function for every profile; the ranked paths of each profile are all the scorer sees. ripgrep runs as a real process, one invocation per query word, ranking files by distinct words then total matches. Intervals: Wilson for proportions, deterministic bootstrap for paired differences. Latencies are wall times on the machine that ran it and are not reproducible across machines; the ranking digest is. Index times are for the corpora named, not for a 100 MB repository, which was not run.".into(),
    }
}
