//! QUAL-PX-186 / PX-E2E-186: the memory-ranking comparison of the memory
//! axis.
//!
//! The offline proofs cover the corpus's shape and the comparison's rule and
//! refusals (unequal budgets, a record of another ranking, a nondeterministic
//! ranking, a sensitive or stale item offered, a default that is not the
//! decision's winner). The retained comparison is recomputed from its
//! retained passes and says revert. The real-Core proof runs the
//! `competence-baseline memory-ranking` harness against the real CLI and Core
//! with a scripted model: the reverted candidate is no ranking the Core has,
//! so its passes record the default and the harness refuses the comparison;
//! the default and current passes, measured now, are compared with the
//! retained candidate passes at the same budget and give the retained
//! decision again.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// A comparison as it reads back from its retained JSON (a float's last
/// digit can differ between the value computed and the value parsed).
fn as_retained(c: &RankingComparison) -> RankingComparison {
    serde_json::from_slice(&serde_json::to_vec_pretty(c).unwrap()).unwrap()
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
    assert_eq!(as_retained(&again), retained);
    assert_eq!(
        retained.environment["modbit_revision"]
            .as_str()
            .unwrap()
            .len(),
        40
    );
    // The decision, with the comparison it rests on.
    assert_eq!(retained.decision, "revert");
    assert_eq!(retained.current.version, "keyword-v1");
    assert_eq!(
        retained.candidate.version,
        "recency-importance-diversity-v2"
    );
    assert_eq!(retained.product_default, "keyword-v1");
    assert!(retained.candidate.recalled < retained.current.recalled);
    assert_eq!(
        retained.current.budget_tokens, retained.candidate.budget_tokens,
        "the same budget"
    );
    let summary = std::fs::read_to_string(dir.join("summary.md")).unwrap();
    assert!(summary.contains("Decision: **revert**"));
}

// ------------------------------------------------------- the real Core

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn product_binaries() -> (PathBuf, PathBuf) {
    let dir = root().join("target/debug");
    let cli = dir.join(format!("modbit-cli{}", std::env::consts::EXE_SUFFIX));
    let core = dir.join(format!("modbit-core{}", std::env::consts::EXE_SUFFIX));
    let execd = dir.join(format!("modbit-execd{}", std::env::consts::EXE_SUFFIX));
    if !cli.is_file() || !core.is_file() || !execd.is_file() {
        let st = Command::new("cargo")
            .args([
                "build",
                "-p",
                "modbit-cli",
                "-p",
                "modbit-core",
                "-p",
                "modbit-execd",
            ])
            .current_dir(root())
            .status()
            .unwrap();
        assert!(st.success(), "building the product binaries");
    }
    (cli, core)
}

/// A scripted OpenAI-compatible model that answers every request with one
/// line of text: the measurement is of the prompt the Core builds, never of
/// the model.
fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for sock in listener.incoming() {
            let Ok(mut sock) = sock else { return };
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let (head_end, len) = loop {
                    let n = sock.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..i]).to_string();
                        let len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        break (i + 4, len);
                    }
                };
                while buf.len() < head_end + len {
                    let n = sock.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let frames = [
                    json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"content":"Nothing to change."},"finish_reason":null}]}),
                    json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":8}}),
                ];
                let mut payload = String::new();
                for f in frames {
                    payload.push_str(&format!("data: {f}\n\n"));
                }
                payload.push_str("data: [DONE]\n\n");
                let _ = sock.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{payload}",
                        payload.len()
                    )
                    .as_bytes(),
                );
                let _ = sock.flush();
                let _ = sock.shutdown(std::net::Shutdown::Both);
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

#[test]
fn qual_px_186_through_the_real_core_the_reverted_candidate_is_refused_and_the_current_ranking_reproduces_the_decision()
 {
    let (corpus, digest) = Corpus::load(&corpus_path()).unwrap();
    let (cli, core) = product_binaries();
    let base = serve();
    let out = tempfile::tempdir().unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_competence-baseline"))
        .current_dir(root())
        .args([
            "memory-ranking",
            "--corpus",
            corpus_path().to_str().unwrap(),
            "--fixtures",
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("suites/four-axis/fixtures")
                .to_str()
                .unwrap(),
            "--out",
            out.path().to_str().unwrap(),
            "--endpoint",
            "openai",
            "--model",
            "gpt-5-mini",
            "--candidate",
            "recency-importance-diversity-v2",
            "--cli",
            cli.to_str().unwrap(),
            "--core",
            core.to_str().unwrap(),
        ])
        .env("MODBIT_OPENAI_BASE_URL", &base)
        .env("OPENAI_API_KEY", "")
        .env("ANTHROPIC_API_KEY", "")
        .env_remove("MODBIT_MEMORY_RANKING")
        .env_remove("MODBIT_OPENAI_MODELS")
        .env_remove("MODBIT_LIVE_MODEL")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    // The candidate was reverted: the Core does not have it, records the
    // ranking that ran instead, and the harness refuses to compare.
    assert_eq!(o.status.code(), Some(1), "{text}");
    let refused: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.path().join("refused.json")).unwrap()).unwrap();
    assert!(
        refused["refused"].as_str().unwrap().contains(
            "the candidate pass asked for recency-importance-diversity-v2 but the record names keyword-v1"
        ),
        "{refused}"
    );
    assert!(!out.path().join("comparison.json").exists());

    // What the product does now, from its own records: every first prompt
    // of the default and current passes names keyword-v1 and its weights,
    // ran at the 800-token budget, and carried what MemoryUsed names.
    let live: Passes =
        serde_json::from_slice(&std::fs::read(out.path().join("passes.json")).unwrap()).unwrap();
    for p in [&live.default, &live.current[0], &live.current[1]] {
        assert_eq!(p.queries.len(), corpus.queries.len());
        for (task, o) in &p.queries {
            assert_eq!(o.ranking_version, "keyword-v1", "{task}");
            assert_eq!(o.ranking_weights["relevance"].as_f64(), Some(0.5), "{task}");
            assert_eq!(o.token_budget, 800, "{task}");
            assert!(o.token_used <= 800 && o.token_used > 0, "{task}");
            assert!(o.unknown_ids.is_empty(), "{task}");
        }
    }

    // The retained candidate passes against the current ranking measured
    // now, at the same budget: deterministic, nothing sensitive or stale
    // offered, the default applied, and the same decision.
    let retained_dir = retained_dir();
    let retained: RankingComparison =
        serde_json::from_slice(&std::fs::read(retained_dir.join("comparison.json")).unwrap())
            .unwrap();
    let measured: Passes =
        serde_json::from_slice(&std::fs::read(retained_dir.join("passes.json")).unwrap()).unwrap();
    let now = Passes {
        default: live.default.clone(),
        current: live.current.clone(),
        candidate: measured.candidate.clone(),
    };
    let c = compare_rankings(&corpus, &digest, &now).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.decision, "revert");
    assert_eq!(c.product_default, "keyword-v1");
    assert_eq!(as_retained(&c).candidate, retained.candidate);
    assert_eq!(
        (c.current.recalled, c.current.groups),
        (retained.current.recalled, retained.current.groups),
        "the current ranking recalls what it recalled when measured"
    );
    assert!(c.current.injected_tokens <= c.current.budget_tokens);
}
