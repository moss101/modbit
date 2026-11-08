//! PX-186: the memory-ranking comparison of the memory axis.
//!
//! A ranking change is kept only when it beats the current ranking on the
//! same tasks at the same token budget. The corpus ([`Corpus`]) is one
//! team's curated memory, seeded through the product in its recorded order,
//! and a set of tasks with the items each needs (its gold). The harness runs
//! every task through the real Core once per ranking on a fresh profile and
//! reads, from the Core's own records, what reached the first prompt: the
//! ContextCompile step's memory record (the ranking's version and weights,
//! the token budget and the tokens used) and the `MemoryUsed` events. Each
//! ranking runs twice on independent profiles, so a ranking that is not
//! deterministic is caught. [`compare_rankings`] refuses a comparison at
//! unequal budgets, a record of another ranking, a missing task, a
//! nondeterministic ranking and any sensitive item offered; otherwise it
//! gives recall and injected tokens per ranking and the keep-or-revert
//! decision.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One curated item of the corpus, seeded with `memory propose|promote`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CorpusItem {
    /// Key the gold names it by.
    pub key: String,
    /// Scope kind (`repository` unless stated).
    #[serde(default = "repository")]
    pub scope: String,
    /// Record type.
    pub record_type: String,
    /// Topic.
    pub topic: String,
    /// Content.
    pub content: String,
    /// Confidence (`--confidence`), when not the product's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Proposed `--sensitive` (in a scope that permits it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sensitive: bool,
    /// Seeded before the repository's revision moves on, so a repository
    /// fact among these is stale when the tasks run.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub before_revision_move: bool,
}

fn repository() -> String {
    "repository".into()
}

/// One task of the comparison and the items it needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusQuery {
    /// Stable id.
    pub id: String,
    /// The task goal the Core selects memory for.
    pub goal: String,
    /// Gold groups: each group is items that carry the same information; a
    /// group is recalled when any one of them reaches the prompt.
    pub gold: Vec<Vec<String>>,
    /// Why these items are the gold.
    #[serde(default)]
    pub why: String,
}

/// The corpus and its tasks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Corpus {
    /// Corpus id.
    pub id: String,
    /// Corpus version.
    pub version: String,
    /// What the corpus is and how it was designed.
    #[serde(default)]
    pub about: String,
    /// The fixture repository (a directory under the fixtures root).
    pub fixture: String,
    /// The items, oldest first (seeded in this order).
    pub items: Vec<CorpusItem>,
    /// The tasks.
    pub queries: Vec<CorpusQuery>,
}

impl Corpus {
    /// Load and check a corpus; returns it and the SHA-256 of its bytes.
    ///
    /// # Errors
    /// Unreadable, malformed or structurally invalid.
    pub fn load(path: &Path) -> Result<(Self, String), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let c: Corpus =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        c.check()?;
        Ok((c, crate::sha256_hex(&bytes)))
    }

    /// Structural checks: unique keys and ids, every gold key an item, no
    /// gold item sensitive or stale, at least one sensitive item and one
    /// stale item (so the comparison proves both are never offered).
    ///
    /// # Errors
    /// The first problem found.
    pub fn check(&self) -> Result<(), String> {
        let mut keys = BTreeSet::new();
        for it in &self.items {
            if it.key.trim().is_empty()
                || it.topic.trim().is_empty()
                || it.content.trim().is_empty()
            {
                return Err(format!("item {:?} has an empty field", it.key));
            }
            if !keys.insert(it.key.as_str()) {
                return Err(format!("item key {:?} is duplicated", it.key));
            }
        }
        if !self.items.iter().any(|i| i.sensitive) {
            return Err("the corpus has no sensitive item".into());
        }
        if !self.items.iter().any(|i| i.before_revision_move) {
            return Err("the corpus has no item from an older revision".into());
        }
        if self.queries.is_empty() {
            return Err("the corpus has no task".into());
        }
        let mut ids = BTreeSet::new();
        for q in &self.queries {
            if !ids.insert(q.id.as_str()) {
                return Err(format!("task id {:?} is duplicated", q.id));
            }
            if q.goal.trim().is_empty() || q.gold.is_empty() {
                return Err(format!("task {:?} has no goal or no gold", q.id));
            }
            for k in q.gold.iter().flatten() {
                let Some(it) = self.items.iter().find(|i| &i.key == k) else {
                    return Err(format!("task {:?}: gold {k:?} is not an item", q.id));
                };
                if it.sensitive || it.before_revision_move {
                    return Err(format!("task {:?}: gold {k:?} may never be offered", q.id));
                }
            }
            if q.gold.iter().any(Vec::is_empty) {
                return Err(format!("task {:?} has an empty gold group", q.id));
            }
        }
        Ok(())
    }

    /// The keys that may never reach a prompt: sensitive items and items
    /// from an older revision.
    #[must_use]
    pub fn never_offered(&self) -> BTreeSet<String> {
        self.items
            .iter()
            .filter(|i| i.sensitive || i.before_revision_move)
            .map(|i| i.key.clone())
            .collect()
    }
}

/// What the Core recorded for one task's first prompt.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QueryObservation {
    /// The ranking version the selection record names.
    pub ranking_version: String,
    /// The weights it names.
    pub ranking_weights: Value,
    /// The memory segment's token budget.
    pub token_budget: u64,
    /// Tokens the memory segment used.
    pub token_used: u64,
    /// Keys of the items the prompt carried (`injected_memory`), in order.
    pub injected: Vec<String>,
    /// Keys named by the turn's `MemoryUsed` events (sorted).
    pub memory_used: Vec<String>,
    /// Ids the Core injected that are no corpus item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_ids: Vec<String>,
}

/// One pass: every task through one Core profile under one ranking.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PassObservation {
    /// The ranking the pass asked the Core for (`MODBIT_MEMORY_RANKING`).
    pub ranking: String,
    /// Task id → what its first prompt carried.
    pub queries: BTreeMap<String, QueryObservation>,
}

/// One ranking's totals.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RankingTotals {
    /// Version label.
    pub version: String,
    /// The weights its records name.
    pub weights: Value,
    /// Gold groups recalled.
    pub recalled: u32,
    /// Gold groups.
    pub groups: u32,
    /// `recalled / groups`.
    pub recall: f64,
    /// Memory tokens injected over all tasks.
    pub injected_tokens: u64,
    /// The token budget over all tasks.
    pub budget_tokens: u64,
    /// Gold groups recalled per thousand injected tokens.
    pub recall_per_1k_tokens: f64,
}

/// One task's row of the comparison.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryRow {
    /// Task id.
    pub id: String,
    /// Gold groups.
    pub gold: Vec<Vec<String>>,
    /// Groups recalled by the current ranking.
    pub current_recalled: u32,
    /// Groups recalled by the candidate ranking.
    pub candidate_recalled: u32,
    /// The budget both ran at.
    pub token_budget: u64,
    /// Tokens used by the current ranking.
    pub current_tokens: u64,
    /// Tokens used by the candidate ranking.
    pub candidate_tokens: u64,
    /// What the current ranking injected (keys, in order).
    pub current_injected: Vec<String>,
    /// What the candidate ranking injected (keys, in order).
    pub candidate_injected: Vec<String>,
}

/// The retained comparison.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RankingComparison {
    /// Corpus id and version.
    pub corpus: String,
    /// SHA-256 of the corpus file.
    pub corpus_sha256: String,
    /// The current ranking.
    pub current: RankingTotals,
    /// The candidate ranking.
    pub candidate: RankingTotals,
    /// Per task.
    pub queries: Vec<QueryRow>,
    /// Both rankings reproduced every task's injected items and tokens on
    /// an independent profile.
    pub deterministic: bool,
    /// Sensitive or stale items that reached a prompt (must be empty).
    pub never_offered_seen: Vec<String>,
    /// `keep` or `revert`.
    pub decision: String,
    /// The rule the decision applies.
    pub rule: String,
    /// The ranking the Core selects with when none is named (a pass with
    /// `MODBIT_MEMORY_RANKING` empty): it must be the one the decision
    /// keeps, so a change kept without a comparison that supports it, or a
    /// revert not applied, is refused.
    pub product_default: String,
    /// Where the comparison ran (revision, build digest, OS); set by the
    /// harness.
    #[serde(default)]
    pub environment: Value,
}

/// The rule [`compare_rankings`] applies.
pub const KEEP_RULE: &str = "keep the candidate only when, on the same tasks at the same memory token budget, it recalls more gold groups than the current ranking and its recall per thousand injected tokens is not lower; otherwise revert";

/// Why a comparison was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RankingRefused {
    /// A task has no record in a pass.
    #[error("task {task} has no memory record in the {pass} pass")]
    Missing {
        /// Task id.
        task: String,
        /// Pass label.
        pass: String,
    },
    /// A record names another ranking than its pass asked for.
    #[error("task {task}: the {pass} pass asked for {asked} but the record names {recorded}")]
    WrongRanking {
        /// Task id.
        task: String,
        /// Pass label.
        pass: String,
        /// Asked.
        asked: String,
        /// Recorded.
        recorded: String,
    },
    /// The two rankings ran at different budgets.
    #[error("task {task}: the rankings ran at unequal token budgets ({current} and {candidate})")]
    UnequalBudget {
        /// Task id.
        task: String,
        /// Current ranking's budget.
        current: u64,
        /// Candidate's budget.
        candidate: u64,
    },
    /// A segment used more than its budget.
    #[error("task {task}: {used} memory tokens over a budget of {budget}")]
    OverBudget {
        /// Task id.
        task: String,
        /// Used.
        used: u64,
        /// Budget.
        budget: u64,
    },
    /// A ranking gave a different result on an independent profile.
    #[error("{ranking} is not deterministic on task {task}: {first:?} then {second:?}")]
    Nondeterministic {
        /// Ranking.
        ranking: String,
        /// Task id.
        task: String,
        /// First pass.
        first: Vec<String>,
        /// Second pass.
        second: Vec<String>,
    },
    /// A sensitive or stale item reached a prompt.
    #[error("task {task}: {key} reached the prompt under {ranking} and may never be offered")]
    NeverOffered {
        /// Task id.
        task: String,
        /// Key.
        key: String,
        /// Ranking.
        ranking: String,
    },
    /// The prompt carried an item that is no corpus item.
    #[error("task {task}: the prompt carried {ids:?}, which are no corpus item")]
    Unknown {
        /// Task id.
        task: String,
        /// Ids.
        ids: Vec<String>,
    },
    /// The record and the `MemoryUsed` events disagree.
    #[error("task {task}: the record injected {injected:?} but MemoryUsed names {used:?}")]
    UsageMismatch {
        /// Task id.
        task: String,
        /// Injected.
        injected: Vec<String>,
        /// Used.
        used: Vec<String>,
    },
    /// The passes compared are of one ranking.
    #[error("the current and candidate passes are both {0}")]
    SameRanking(String),
    /// The product's default ranking is not the one the decision keeps.
    #[error(
        "the decision is to {decision} ({winner}) but the Core selects with {default} by default"
    )]
    DecisionNotApplied {
        /// `keep` or `revert`.
        decision: String,
        /// The ranking the decision keeps.
        winner: String,
        /// The Core's default.
        default: String,
    },
    /// The default pass's records do not agree on one ranking.
    #[error("the default pass names more than one ranking: {0:?}")]
    AmbiguousDefault(Vec<String>),
}

fn recalled(gold: &[Vec<String>], injected: &[String]) -> u32 {
    gold.iter()
        .filter(|g| g.iter().any(|k| injected.contains(k)))
        .count() as u32
}

fn check_pass(corpus: &Corpus, pass: &PassObservation, label: &str) -> Result<(), RankingRefused> {
    let never = corpus.never_offered();
    for q in &corpus.queries {
        let o = pass
            .queries
            .get(&q.id)
            .ok_or_else(|| RankingRefused::Missing {
                task: q.id.clone(),
                pass: label.into(),
            })?;
        if o.ranking_version != pass.ranking {
            return Err(RankingRefused::WrongRanking {
                task: q.id.clone(),
                pass: label.into(),
                asked: pass.ranking.clone(),
                recorded: o.ranking_version.clone(),
            });
        }
        if o.token_used > o.token_budget {
            return Err(RankingRefused::OverBudget {
                task: q.id.clone(),
                used: o.token_used,
                budget: o.token_budget,
            });
        }
        if !o.unknown_ids.is_empty() {
            return Err(RankingRefused::Unknown {
                task: q.id.clone(),
                ids: o.unknown_ids.clone(),
            });
        }
        let mut injected = o.injected.clone();
        injected.sort();
        if injected != o.memory_used {
            return Err(RankingRefused::UsageMismatch {
                task: q.id.clone(),
                injected,
                used: o.memory_used.clone(),
            });
        }
        if let Some(k) = o.injected.iter().find(|k| never.contains(*k)) {
            return Err(RankingRefused::NeverOffered {
                task: q.id.clone(),
                key: k.clone(),
                ranking: pass.ranking.clone(),
            });
        }
    }
    Ok(())
}

fn same(
    corpus: &Corpus,
    first: &PassObservation,
    second: &PassObservation,
) -> Result<(), RankingRefused> {
    for q in &corpus.queries {
        let (a, b) = (&first.queries[&q.id], &second.queries[&q.id]);
        if a.injected != b.injected || a.token_used != b.token_used {
            return Err(RankingRefused::Nondeterministic {
                ranking: first.ranking.clone(),
                task: q.id.clone(),
                first: a.injected.clone(),
                second: b.injected.clone(),
            });
        }
    }
    Ok(())
}

fn totals(corpus: &Corpus, pass: &PassObservation) -> RankingTotals {
    let mut rec = 0;
    let mut groups = 0;
    let mut used = 0;
    let mut budget = 0;
    let mut weights = Value::Null;
    for q in &corpus.queries {
        let o = &pass.queries[&q.id];
        rec += recalled(&q.gold, &o.injected);
        groups += q.gold.len() as u32;
        used += o.token_used;
        budget += o.token_budget;
        weights = o.ranking_weights.clone();
    }
    RankingTotals {
        version: pass.ranking.clone(),
        weights,
        recalled: rec,
        groups,
        recall: f64::from(rec) / f64::from(groups.max(1)),
        injected_tokens: used,
        budget_tokens: budget,
        recall_per_1k_tokens: if used == 0 {
            0.0
        } else {
            f64::from(rec) * 1000.0 / used as f64
        },
    }
}

/// The five passes of a comparison, each on a fresh profile: the Core's
/// default (no ranking named), the current ranking twice and the candidate
/// twice.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Passes {
    /// `MODBIT_MEMORY_RANKING` empty: what the product selects with.
    pub default: PassObservation,
    /// The current ranking, then again on an independent profile.
    pub current: [PassObservation; 2],
    /// The candidate, then again on an independent profile.
    pub candidate: [PassObservation; 2],
}

/// Compare a candidate ranking with the current one over the same corpus:
/// each ranking's two passes (independent profiles) must agree, every task
/// must have run at one budget under both, no sensitive or stale item may
/// reach a prompt, the record must name the ranking its pass asked for, and
/// the Core's default ranking must be the one the decision keeps.
///
/// # Errors
/// Any of those, as a [`RankingRefused`]; a refused comparison keeps
/// nothing.
pub fn compare_rankings(
    corpus: &Corpus,
    corpus_sha256: &str,
    passes: &Passes,
) -> Result<RankingComparison, RankingRefused> {
    let (current, candidate) = (&passes.current, &passes.candidate);
    if current[0].ranking == candidate[0].ranking {
        return Err(RankingRefused::SameRanking(current[0].ranking.clone()));
    }
    for p in [current, candidate] {
        if p[0].ranking != p[1].ranking {
            return Err(RankingRefused::SameRanking(format!(
                "not: {} and {}",
                p[0].ranking, p[1].ranking
            )));
        }
    }
    // The default pass names no ranking: every record must name one and the
    // same, and it is checked like any other pass under that name.
    let named: BTreeSet<String> = passes
        .default
        .queries
        .values()
        .map(|o| o.ranking_version.clone())
        .collect();
    if named.len() != 1 {
        return Err(RankingRefused::AmbiguousDefault(
            named.into_iter().collect(),
        ));
    }
    let default_version = named.into_iter().next().unwrap_or_default();
    let mut default = passes.default.clone();
    default.ranking.clone_from(&default_version);
    for (pass, label) in [
        (&default, "default"),
        (&current[0], "current"),
        (&current[1], "current rerun"),
        (&candidate[0], "candidate"),
        (&candidate[1], "candidate rerun"),
    ] {
        check_pass(corpus, pass, label)?;
    }
    for p in [current, candidate] {
        same(corpus, &p[0], &p[1])?;
    }
    let mut rows = Vec::new();
    for q in &corpus.queries {
        let (a, b) = (&current[0].queries[&q.id], &candidate[0].queries[&q.id]);
        if a.token_budget != b.token_budget {
            return Err(RankingRefused::UnequalBudget {
                task: q.id.clone(),
                current: a.token_budget,
                candidate: b.token_budget,
            });
        }
        rows.push(QueryRow {
            id: q.id.clone(),
            gold: q.gold.clone(),
            current_recalled: recalled(&q.gold, &a.injected),
            candidate_recalled: recalled(&q.gold, &b.injected),
            token_budget: a.token_budget,
            current_tokens: a.token_used,
            candidate_tokens: b.token_used,
            current_injected: a.injected.clone(),
            candidate_injected: b.injected.clone(),
        });
    }
    let cur = totals(corpus, &current[0]);
    let cand = totals(corpus, &candidate[0]);
    let keep =
        cand.recalled > cur.recalled && cand.recall_per_1k_tokens >= cur.recall_per_1k_tokens;
    let decision = if keep { "keep" } else { "revert" };
    let winner = if keep { &cand.version } else { &cur.version };
    if *winner != default_version {
        return Err(RankingRefused::DecisionNotApplied {
            decision: decision.into(),
            winner: winner.clone(),
            default: default_version,
        });
    }
    Ok(RankingComparison {
        corpus: format!("{}@{}", corpus.id, corpus.version),
        corpus_sha256: corpus_sha256.into(),
        current: cur,
        candidate: cand,
        queries: rows,
        deterministic: true,
        never_offered_seen: Vec::new(),
        decision: decision.into(),
        rule: KEEP_RULE.into(),
        product_default: default_version,
        environment: Value::Null,
    })
}

/// The memory record of a task's first prompt, from the objects its
/// `StepSucceeded` events name (the ContextCompile step's output carries
/// `memory` and `injected_memory`), and the ids of the `MemoryUsed`
/// events of that turn. `read_object` returns an object's bytes by its
/// hash; `ids` maps a Core memory id to its corpus key.
#[must_use]
pub fn observe(
    events: &[crate::Event],
    task_id: &str,
    read_object: &dyn Fn(&str) -> Option<Vec<u8>>,
    ids: &BTreeMap<String, String>,
) -> Option<QueryObservation> {
    let record = events
        .iter()
        .filter(|e| e.event_type == "StepSucceeded" && e.task_id.as_deref() == Some(task_id))
        .filter_map(|e| e.payload["output_ref"].as_str())
        .filter_map(read_object)
        .filter_map(|b| serde_json::from_slice::<Value>(&b).ok())
        .find(|v| v.get("memory").is_some() && v.get("injected_memory").is_some())?;
    let memory = &record["memory"];
    let key_of = |id: &str| ids.get(id).cloned();
    let raw: Vec<String> = record["injected_memory"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let injected: Vec<String> = raw.iter().filter_map(|i| key_of(i)).collect();
    let unknown_ids: Vec<String> = raw
        .iter()
        .filter(|i| key_of(i).is_none())
        .cloned()
        .collect();
    // The first turn's MemoryUsed events (their turn is the first one named).
    let used: Vec<&crate::Event> = events
        .iter()
        .filter(|e| e.event_type == "MemoryUsed" && e.task_id.as_deref() == Some(task_id))
        .collect();
    let first_turn = used
        .first()
        .and_then(|e| e.payload["turn_id"].as_str())
        .unwrap_or_default()
        .to_owned();
    let mut memory_used: Vec<String> = used
        .iter()
        .filter(|e| e.payload["turn_id"].as_str() == Some(first_turn.as_str()))
        .filter_map(|e| e.payload["memory_id"].as_str())
        .map(|i| key_of(i).unwrap_or_else(|| format!("unknown:{i}")))
        .collect();
    memory_used.sort();
    Some(QueryObservation {
        ranking_version: memory["ranking"]["version"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        ranking_weights: memory["ranking"]["weights"].clone(),
        // A record with no pack (nothing in scope) names no budget: read as
        // zero, so a comparison with a ranking that did pack is unequal.
        token_budget: memory["token_budget"].as_u64().unwrap_or(0),
        token_used: memory["token_used"].as_u64().unwrap_or(0),
        injected,
        memory_used,
        unknown_ids,
    })
}

/// The retained summary of a comparison.
#[must_use]
pub fn render(c: &RankingComparison) -> String {
    let mut s = format!(
        "# Memory ranking comparison (PX-186)\n\nCorpus `{}` (sha256 `{}`), {} tasks, {} gold groups. Both rankings ran every task through the real Core at the same memory budget; each ran twice on independent profiles with identical results.\n\n| ranking | gold groups recalled | recall | injected tokens | budget tokens | recall per 1k tokens |\n|---|---|---|---|---|---|\n",
        c.corpus,
        &c.corpus_sha256[..12.min(c.corpus_sha256.len())],
        c.queries.len(),
        c.current.groups
    );
    for t in [&c.current, &c.candidate] {
        s.push_str(&format!(
            "| `{}` | {}/{} | {:.3} | {} | {} | {:.3} |\n",
            t.version,
            t.recalled,
            t.groups,
            t.recall,
            t.injected_tokens,
            t.budget_tokens,
            t.recall_per_1k_tokens
        ));
    }
    s.push_str("\n| task | budget | current recalled | current tokens | candidate recalled | candidate tokens |\n|---|---|---|---|---|---|\n");
    for q in &c.queries {
        s.push_str(&format!(
            "| {} | {} | {}/{} | {} | {}/{} | {} |\n",
            q.id,
            q.token_budget,
            q.current_recalled,
            q.gold.len(),
            q.current_tokens,
            q.candidate_recalled,
            q.gold.len(),
            q.candidate_tokens
        ));
    }
    s.push_str(&format!(
        "\nDecision: **{}**. Rule: {}. The Core's default ranking (a pass naming none) is `{}`.\n",
        c.decision, c.rule, c.product_default
    ));
    s
}
