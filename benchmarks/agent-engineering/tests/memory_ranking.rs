//! QUAL-PX-186 / PX-E2E-186: the memory-ranking comparison of the memory
//! axis.
//!
//! The offline proofs cover the corpus's shape and the comparison's rule and
//! refusals (unequal budgets, a record of another ranking, a nondeterministic
//! ranking, a sensitive or stale item offered, a default that is not the
//! decision's winner). The retained comparison is recomputed from its
//! retained passes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use modbit_bench_agent_engineering::ranking::{
    Corpus, KEEP_RULE, PassObservation, Passes, QueryObservation, RankingComparison,
    RankingRefused, compare_rankings,
};
use serde_json::json;

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("suites/memory-ranking/corpus.json")
}

fn retained_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("baselines/memory-ranking")
}

const CURRENT: &str = "keyword-v1";
const CANDIDATE: &str = "candidate-v2";

/// A pass in which every task's prompt carries `injected(task)` at
/// `tokens(task)` of an 800-token budget.
fn pass(
    corpus: &Corpus,
    ranking: &str,
    injected: impl Fn(&str) -> Vec<String>,
    tokens: impl Fn(&str) -> u64,
) -> PassObservation {
    let mut queries = BTreeMap::new();
    for q in &corpus.queries {
        let inj = injected(&q.id);
        let mut used = inj.clone();
        used.sort();
        queries.insert(
            q.id.clone(),
            QueryObservation {
                ranking_version: ranking.into(),
                ranking_weights: json!({"relevance": 0.5}),
                token_budget: 800,
                token_used: tokens(&q.id),
                injected: inj,
                memory_used: used,
                unknown_ids: Vec::new(),
            },
        );
    }
    PassObservation {
        ranking: ranking.into(),
        queries,
    }
}

/// Every gold group's first item, less the groups `drop` names (task id,
/// group index).
fn gold_less(corpus: &Corpus, drop: &[(&str, usize)]) -> impl Fn(&str) -> Vec<String> {
    let c = corpus.clone();
    let drop: Vec<(String, usize)> = drop.iter().map(|(t, i)| ((*t).to_owned(), *i)).collect();
    move |task: &str| {
        let q = c.queries.iter().find(|q| q.id == task).unwrap();
        q.gold
            .iter()
            .enumerate()
            .filter(|(i, _)| !drop.iter().any(|(t, d)| t == task && d == i))
            .map(|(_, g)| g[0].clone())
            .collect()
    }
}

fn passes(default: &str, current: &PassObservation, candidate: &PassObservation) -> Passes {
    let mut d = if default == current.ranking {
        current.clone()
    } else {
        candidate.clone()
    };
    d.ranking = String::new();
    Passes {
        default: d,
        current: [current.clone(), current.clone()],
        candidate: [candidate.clone(), candidate.clone()],
    }
}

#[test]
fn qual_px_186_the_corpus_is_designed_before_measurement_and_refuses_a_defective_gold() {
    let (corpus, digest) = Corpus::load(&corpus_path()).unwrap();
    assert_eq!(digest.len(), 64);
    assert!(corpus.queries.len() >= 10, "{}", corpus.queries.len());
    assert!(corpus.items.len() >= 40, "{}", corpus.items.len());
    // Ranking matters: more items than an 800-token memory segment holds
    // (each entry costs tens of tokens), near-duplicates, a carried
    // conflict, and never-offered items that share the tasks' words.
    let never = corpus.never_offered();
    assert!(never.contains("sensitive-staging") && never.contains("sensitive-owner"));
    assert!(never.contains("retries-old"));
    assert!(
        corpus
            .queries
            .iter()
            .any(|q| q.gold.iter().any(|g| g.len() > 1))
    );
    let conflict = corpus
        .items
        .iter()
        .filter(|i| i.topic == "api version")
        .count();
    assert_eq!(conflict, 2);

    let mut bad = corpus.clone();
    bad.queries[0].gold = vec![vec!["sensitive-staging".into()]];
    assert!(bad.check().unwrap_err().contains("may never be offered"));
    let mut bad = corpus.clone();
    bad.queries[0].gold = vec![vec!["nope".into()]];
    assert!(bad.check().unwrap_err().contains("is not an item"));
    let mut bad = corpus.clone();
    bad.items[2].key = bad.items[1].key.clone();
    assert!(bad.check().unwrap_err().contains("duplicated"));
    let mut bad = corpus.clone();
    for i in &mut bad.items {
        i.sensitive = false;
    }
    assert!(bad.check().unwrap_err().contains("no sensitive item"));
    let mut bad = corpus.clone();
    bad.queries[1].gold.push(Vec::new());
    assert!(bad.check().unwrap_err().contains("empty gold group"));
}

#[test]
fn qual_px_186_keep_needs_more_recall_at_the_same_budget_and_the_default_must_follow_the_decision()
{
    let (corpus, digest) = Corpus::load(&corpus_path()).unwrap();
    let flat = |_: &str| 700;
    let current = pass(
        &corpus,
        CURRENT,
        gold_less(&corpus, &[("deploy", 1), ("release-prep", 0)]),
        flat,
    );
    let better = pass(
        &corpus,
        CANDIDATE,
        gold_less(&corpus, &[("deploy", 1)]),
        flat,
    );

    // More recall at the same tokens: keep, and the product must select with
    // the candidate.
    let c = compare_rankings(&corpus, &digest, &passes(CANDIDATE, &current, &better)).unwrap();
    assert_eq!(c.decision, "keep");
    assert_eq!((c.current.recalled, c.candidate.recalled), (14, 15));
    assert_eq!(c.current.groups, 16);
    assert_eq!(c.product_default, CANDIDATE);
    assert_eq!(c.rule, KEEP_RULE);
    assert_eq!(c.current.injected_tokens, 12 * 700);
    // The same comparison with the product still on the current ranking: a
    // kept change that the product does not run is refused.
    assert!(matches!(
        compare_rankings(&corpus, &digest, &passes(CURRENT, &current, &better)),
        Err(RankingRefused::DecisionNotApplied { .. })
    ));

    // Equal recall: revert (and the default must be the current ranking).
    let equal = pass(
        &corpus,
        CANDIDATE,
        gold_less(&corpus, &[("deploy", 1), ("release-prep", 0)]),
        flat,
    );
    let c = compare_rankings(&corpus, &digest, &passes(CURRENT, &current, &equal)).unwrap();
    assert_eq!(c.decision, "revert");
    assert!(matches!(
        compare_rankings(&corpus, &digest, &passes(CANDIDATE, &current, &equal)),
        Err(RankingRefused::DecisionNotApplied { .. })
    ));

    // More recall bought with more tokens per recalled group: revert.
    let costly = pass(
        &corpus,
        CANDIDATE,
        gold_less(&corpus, &[("deploy", 1)]),
        |_| 799,
    );
    let c = compare_rankings(&corpus, &digest, &passes(CURRENT, &current, &costly)).unwrap();
    assert!(c.candidate.recalled > c.current.recalled);
    assert!(c.candidate.recall_per_1k_tokens < c.current.recall_per_1k_tokens);
    assert_eq!(c.decision, "revert");

    // Fewer recalled: revert.
    let worse = pass(
        &corpus,
        CANDIDATE,
        gold_less(
            &corpus,
            &[("deploy", 1), ("release-prep", 0), ("changelog", 0)],
        ),
        flat,
    );
    let c = compare_rankings(&corpus, &digest, &passes(CURRENT, &current, &worse)).unwrap();
    assert_eq!(c.decision, "revert");
    let row = c.queries.iter().find(|q| q.id == "changelog").unwrap();
    assert_eq!((row.current_recalled, row.candidate_recalled), (1, 0));
}

#[test]
fn qual_px_186_a_comparison_at_unequal_budgets_nondeterministic_or_offering_a_sensitive_item_is_refused()
 {
    let (corpus, digest) = Corpus::load(&corpus_path()).unwrap();
    let flat = |_: &str| 700;
    let current = pass(&corpus, CURRENT, gold_less(&corpus, &[]), flat);
    let candidate = pass(&corpus, CANDIDATE, gold_less(&corpus, &[]), flat);
    let ok = passes(CURRENT, &current, &candidate);
    assert!(compare_rankings(&corpus, &digest, &ok).is_ok());

    // Unequal budgets.
    let mut p = ok.clone();
    for c in &mut p.candidate {
        c.queries.get_mut("timeout").unwrap().token_budget = 1200;
    }
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::UnequalBudget { .. })
    ));
    // Over budget.
    let mut p = ok.clone();
    p.current[0].queries.get_mut("timeout").unwrap().token_used = 801;
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::OverBudget { .. })
    ));
    // A record of another ranking than the pass asked for.
    let mut p = ok.clone();
    p.candidate[1]
        .queries
        .get_mut("branch")
        .unwrap()
        .ranking_version = CURRENT.into();
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::WrongRanking { .. })
    ));
    // Nondeterministic: the rerun on an independent profile differs.
    let mut p = ok.clone();
    let o = p.candidate[1].queries.get_mut("branch").unwrap();
    o.injected.push("region".into());
    o.memory_used.push("region".into());
    o.memory_used.sort();
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::Nondeterministic { .. })
    ));
    let mut p = ok.clone();
    p.current[1].queries.get_mut("branch").unwrap().token_used = 701;
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::Nondeterministic { .. })
    ));
    // A sensitive item, or a fact of an older revision, offered.
    for key in ["sensitive-owner", "retries-old"] {
        let mut p = ok.clone();
        for c in &mut p.candidate {
            let o = c.queries.get_mut("release-owner").unwrap();
            o.injected.push(key.into());
            o.memory_used.push(key.into());
            o.memory_used.sort();
        }
        assert!(
            matches!(
                compare_rankings(&corpus, &digest, &p),
                Err(RankingRefused::NeverOffered { key: ref k, .. }) if k == key
            ),
            "{key}"
        );
    }
    // The record and the MemoryUsed events disagree.
    let mut p = ok.clone();
    p.current[0]
        .queries
        .get_mut("deploy")
        .unwrap()
        .memory_used
        .clear();
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::UsageMismatch { .. })
    ));
    // An item the corpus never seeded.
    let mut p = ok.clone();
    p.current[0]
        .queries
        .get_mut("deploy")
        .unwrap()
        .unknown_ids
        .push("ab".into());
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::Unknown { .. })
    ));
    // A task without a record.
    let mut p = ok.clone();
    p.candidate[0].queries.remove("migration");
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::Missing { .. })
    ));
    // One ranking compared with itself.
    let p = passes(CURRENT, &current, &current);
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::SameRanking(_))
    ));
    // A default pass whose records name two rankings.
    let mut p = ok.clone();
    p.default.queries.get_mut("deploy").unwrap().ranking_version = CANDIDATE.into();
    assert!(matches!(
        compare_rankings(&corpus, &digest, &p),
        Err(RankingRefused::AmbiguousDefault(_))
    ));
}

/// The retained comparison is what its retained passes give, for this
/// corpus.
#[test]
fn qual_px_186_the_retained_comparison_is_recomputed_from_its_retained_passes() {
    let dir = retained_dir();
    if !dir.join("comparison.json").is_file() {
        // Retained with the measurement (see evidence/m10/PX-186).
        return;
    }
    let (corpus, digest) = Corpus::load(&corpus_path()).unwrap();
    let retained: RankingComparison =
        serde_json::from_slice(&std::fs::read(dir.join("comparison.json")).unwrap()).unwrap();
    let passes: Passes =
        serde_json::from_slice(&std::fs::read(dir.join("passes.json")).unwrap()).unwrap();
    assert_eq!(
        retained.corpus_sha256, digest,
        "the corpus is the one measured"
    );
    let mut again = compare_rankings(&corpus, &digest, &passes).unwrap();
    again.environment = retained.environment.clone();
    assert_eq!(again, retained);
    assert!(
        retained.environment["modbit_revision"]
            .as_str()
            .unwrap()
            .len()
            == 40
    );
}
