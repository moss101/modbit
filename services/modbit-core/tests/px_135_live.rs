//! PX-135 (QUAL-PX-135 live half): the candidate-producing runs of the widened
//! gate calibration, driven end to end against the REAL Core through the real
//! `modbit-cli`, with a SCRIPTED OpenAI-compatible provider standing in for
//! the gateway. This proves the harness (the Core-driving path, the scoring
//! of what the model left through the previous gate, the widened gate and the
//! hidden oracle, the result contract, the spend cap, the leak search); it is
//! not a live result and the bundle it writes says nothing about a model.
//!
//! Needs `modbit-cli` built beside `modbit-core`, and the toolchain of the
//! language under test (cargo for the Rust lineages used here).

mod px_common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use modbit_bench_context_economics::spend::LiveStatus;
use modbit_bench_gate_calibration::live::{
    LiveOptions, LiveResult, execute, exit, live_config, run_live,
};
use modbit_bench_gate_calibration::widened::{WidenedCorpus, load};
use modbit_bench_gate_calibration::{FileOp, find_leaks};
use px_common::*;
use serde_json::json;

const KEY: &str = "sk-live-0123456789abcdef0123456789";
const MODEL: &str = "glm-5.3-flash";

fn gate_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/gate-calibration")
}

fn corpus() -> WidenedCorpus {
    load(&gate_dir().join("corpora/widened")).unwrap()
}

fn cli_and_core() -> (PathBuf, PathBuf) {
    let core = PathBuf::from(env!("CARGO_BIN_EXE_modbit-core"));
    let name = if cfg!(windows) {
        "modbit-cli.exe"
    } else {
        "modbit-cli"
    };
    let cli = core.with_file_name(name);
    assert!(cli.exists(), "build modbit-cli first: {}", cli.display());
    (cli, core)
}

/// What a model that fixes `lineage` would do: read the file, replace it with
/// the corpus's correct candidate, complete.
fn fix_script(
    c: &WidenedCorpus,
    language: &str,
    lineage: &str,
    class: &str,
    extra_outcome: &str,
) -> Vec<serde_json::Value> {
    let wc = c
        .cases
        .iter()
        .find(|w| {
            w.language == language
                && w.case.lineage.ends_with(&format!("/{lineage}"))
                && w.class == class
                && w.flavor == "a"
        })
        .unwrap();
    let writes: Vec<(&str, &str)> = wc
        .case
        .candidate
        .iter()
        .filter_map(|op| match op {
            FileOp::Write { path, content } => Some((path.as_str(), content.as_str())),
            FileOp::Delete { .. } => None,
        })
        .collect();
    let paths: Vec<&str> = writes.iter().map(|(p, _)| *p).collect();
    let mut steps = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": format!("fix {lineage}{extra_outcome}"), "expected_files": paths}}]}),
    ];
    for (p, content) in &writes {
        steps.push(json!({"calls": [{"name": "fs.read", "args": {"path": p}}]}));
        steps.push(json!({"calls": [{"name": "change.apply", "args": {"path": p, "op": "replace", "content": content}}]}));
    }
    steps.push(json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}));
    steps
}

/// The goal this request is about, by the head of the lineage's goal text.
fn goal_marker(goal: &str) -> String {
    let q = serde_json::to_string(goal).unwrap();
    q[1..q.len().min(40)].to_owned()
}

type Seen = px_common::Seen;

/// A scripted gateway answering each lineage's task with its correct fix.
async fn gateway(extra_outcome: &'static str, ask_first: bool) -> (String, Seen) {
    gateway_of_class("correct", extra_outcome, ask_first).await
}

/// The same, for a model whose attempt is the corpus's `class` candidate.
async fn gateway_of_class(
    class: &'static str,
    extra_outcome: &'static str,
    ask_first: bool,
) -> (String, Seen) {
    let c = Arc::new(corpus());
    scripted_model_fn(Arc::new(move |body, results| {
        let text = body["messages"].to_string();
        let wc = c
            .cases
            .iter()
            .find(|w| text.contains(&goal_marker(&w.goal)))
            .expect("the request names a lineage's goal");
        let lineage = wc.case.lineage.rsplit('/').next().unwrap().to_owned();
        let mut steps = fix_script(&c, &wc.language, &lineage, class, extra_outcome);
        if ask_first {
            steps.insert(
                1,
                json!({"calls": [{"name": "user.ask", "args": {"question": "Which approach do you prefer for this fix?", "allow_free_text": true}}]}),
            );
        }
        steps
            .get(results)
            .cloned()
            .unwrap_or_else(|| json!({"text": "nothing further"}))
    }))
    .await
}

fn env_map(
    base: &str,
    catalog: Option<&str>,
    cap: Option<&str>,
) -> std::collections::BTreeMap<String, String> {
    let (cli, core) = cli_and_core();
    let mut m = std::collections::BTreeMap::new();
    for (k, v) in [
        ("MODBIT_LIVE", "1"),
        ("MODBIT_LIVE_API_KEY", KEY),
        ("MODBIT_LIVE_MODEL", MODEL),
        ("MODBIT_LIVE_BASE_URL", base),
        ("MODBIT_LIVE_ALLOW_LOOPBACK", "1"),
    ] {
        m.insert(k.to_owned(), v.to_owned());
    }
    m.insert("MODBIT_LIVE_CLI".into(), cli.to_string_lossy().into_owned());
    m.insert(
        "MODBIT_LIVE_CORE".into(),
        core.to_string_lossy().into_owned(),
    );
    if let Some(c) = catalog {
        m.insert("MODBIT_OPENAI_MODELS".into(), c.to_owned());
    }
    if let Some(c) = cap {
        m.insert("MODBIT_LIVE_MAX_COST_USD".into(), c.to_owned());
    }
    m
}

fn args(out: &Path, more: &[&str]) -> Vec<String> {
    let mut a = vec!["--out-dir".to_owned(), out.to_string_lossy().into_owned()];
    a.extend(more.iter().map(|s| (*s).to_owned()));
    a
}

fn read_result(out: &Path) -> (LiveResult, LiveStatus) {
    let result: LiveResult =
        serde_json::from_str(&std::fs::read_to_string(out.join("result.json")).unwrap()).unwrap();
    let status: LiveStatus =
        serde_json::from_str(&std::fs::read_to_string(out.join("status.json")).unwrap()).unwrap();
    assert_eq!(result.status, status, "status.json is the result's status");
    assert!(out.join("summary.md").exists());
    (result, status)
}

const CATALOG: &str = "glm-5.3-flash=0.15/0.50;ctx=200000";

async fn exec(env: std::collections::BTreeMap<String, String>, a: Vec<String>) -> u8 {
    let var = move |k: &str| env.get(k).cloned();
    execute(&a, &var, &gate_dir()).await
}

// (a) A good run: the model fixes a Rust lineage through the Core's tools, the
// harness scores the candidate and the result contract is met.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_good_run_is_scored_by_both_gates_and_the_oracle_and_the_bundle_is_complete() {
    let (base, seen) = gateway("", false).await;
    let out = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let code = exec(
        env_map(&base, Some(CATALOG), Some("5")),
        args(
            out.path(),
            &[
                "--language",
                "rust",
                "--lineage",
                "discount",
                "--repeats",
                "2",
                "--work",
                work.path().to_str().unwrap(),
            ],
        ),
    )
    .await;
    let (result, status) = read_result(out.path());
    assert_eq!(code, exit::COMPLETE, "{status:?}\n{:?}", result.errors);
    assert!(status.complete && status.missing.is_empty(), "{status:?}");
    assert!(!status.spend.stopped_by_cap && status.spend.spent_usd > 0.0);
    assert_eq!(status.row, "px135");
    assert_eq!(result.scope.expected_runs, 2);
    assert!(result.scope.filtered);
    assert_eq!(result.report.rows.len(), 2, "two repeats of one lineage");
    for r in &result.report.rows {
        assert!(
            r.row.oracle_correct,
            "the correct fix passes the hidden oracle: {r:?}"
        );
        assert_eq!(r.row.previous.verdict, "ACCEPT", "{r:?}");
        assert_eq!(r.row.widened.verdict, "ACCEPT", "{r:?}");
        assert!(
            r.changed.iter().any(|p| p == "src/lib.rs"),
            "{:?}",
            r.changed
        );
        assert!(
            r.task_state == "ReadyForReview" || r.task_state == "Completed",
            "{}",
            r.task_state
        );
        assert!(
            r.run.cost_usd.is_some_and(|c| c > 0.0),
            "priced from the catalog: {:?}",
            r.run
        );
        assert!(r.run.model_calls > 0 && r.run.input_tokens > 0);
        assert!(
            r.run.models_answered.iter().any(|m| m == MODEL),
            "{:?}",
            r.run.models_answered
        );
        assert!(r.leaks.is_empty());
        assert!(
            r.risk.is_some(),
            "the realized-risk outcome is recorded on the candidate's facts"
        );
        let dir = out.path().join("runs").join(&r.run.run_id);
        for f in [
            "goal.txt",
            "candidate.json",
            "events.jsonl",
            "meta.json",
            "cli.log",
            "economics.txt",
            "diff.patch",
            "objects",
        ] {
            assert!(dir.join(f).exists(), "{} retained", f);
        }
    }
    // Rates carry their sample counts and intervals, per language and overall.
    let rust = result.rates.iter().find(|r| r.language == "rust").unwrap();
    assert_eq!((rust.candidates, rust.correct, rust.incorrect), (2, 2, 0));
    assert_eq!(rust.false_reject.widened.n, 2);
    assert_eq!(rust.false_reject.widened.count, 0);
    assert!(rust.false_reject.widened.upper > 0.0 && rust.false_reject.widened.upper < 1.0);
    assert_eq!(
        rust.false_accept.previous.n, 0,
        "no incorrect candidate: nothing known, upper 1"
    );
    assert!((rust.false_accept.previous.upper - 1.0).abs() < 1e-9);
    assert!(result.rates.iter().any(|r| r.language == "all"));
    assert_eq!(result.comparison.last().unwrap().language, "all");
    assert!(result.digest_reproduced && !result.report_digest.is_empty());
    assert!(
        result.spend.cap_usd > 0.0
            && result.model == MODEL
            && result.gateway.starts_with("127.0.0.1")
    );
    // No oracle needle is in anything the model was sent: searched in the
    // requests the provider actually received, independently of the harness.
    let bodies: Vec<(String, String)> = seen
        .lock()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, b)| (format!("request:{i}"), b.to_string()))
        .collect();
    assert!(!bodies.is_empty());
    assert!(find_leaks(&bodies, &corpus().needles).is_empty());
    // The retained bundle reproduces the digest without a provider ...
    let verify = |dir: &Path| {
        let (dir, w) = (dir.to_owned(), tempfile::tempdir().unwrap());
        let v = vec![
            "--verify".to_owned(),
            dir.to_string_lossy().into_owned(),
            "--work".to_owned(),
            w.path().to_string_lossy().into_owned(),
        ];
        async move { execute(&v, &|_| None, &gate_dir()).await }
    };
    assert_eq!(verify(out.path()).await, exit::COMPLETE);
    // ... and a retained candidate that was changed afterwards does not.
    let cand = out
        .path()
        .join("runs")
        .join(&result.report.rows[0].run.run_id)
        .join("candidate.json");
    std::fs::write(&cand, "[]").unwrap();
    assert_eq!(verify(out.path()).await, exit::INCOMPLETE);
}

// (b) A missing measurement: the model is not in the price catalog. The Core
// refuses to route it (an unknown price is not free), so the run is an error
// that is listed, nothing is charged as free, and the partial result is still
// written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_model_the_catalog_does_not_price_is_listed_as_missing_and_never_charged_as_free() {
    let (base, seen) = gateway("", false).await;
    let out = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let code = exec(
        env_map(&base, None, Some("5")),
        args(
            out.path(),
            &[
                "--language",
                "rust",
                "--lineage",
                "discount",
                "--repeats",
                "1",
                "--work",
                work.path().to_str().unwrap(),
            ],
        ),
    )
    .await;
    assert_eq!(code, exit::INCOMPLETE);
    let (result, status) = read_result(out.path());
    assert!(!status.complete);
    assert!(
        result.report.rows.is_empty(),
        "a run the Core refused is not a candidate"
    );
    assert_eq!(result.errors.len(), 1);
    assert!(
        status.missing.iter().any(|m| m.contains("rust-discount-0")),
        "{:?}",
        status.missing
    );
    assert!(
        status
            .missing
            .iter()
            .any(|m| m.contains("rust/discount: 0 of 1"))
    );
    assert_eq!(status.spend.spent_usd, 0.0);
    assert!(seen.lock().unwrap().is_empty(), "no model request was made");
}

// (b') A run the Core could not carry out (no gateway answers) is an error
// that makes the result incomplete; it is not dropped and not a candidate.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_that_errors_outside_the_model_is_listed_and_the_result_is_incomplete() {
    // A closed port: nothing answers.
    let dead = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://127.0.0.1:{}", l.local_addr().unwrap().port())
    };
    let out = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let code = exec(
        env_map(&dead, Some(CATALOG), Some("5")),
        args(
            out.path(),
            &[
                "--language",
                "rust",
                "--lineage",
                "discount",
                "--repeats",
                "1",
                "--work",
                work.path().to_str().unwrap(),
                "--task-timeout-secs",
                "240",
            ],
        ),
    )
    .await;
    assert_eq!(code, exit::INCOMPLETE);
    let (result, status) = read_result(out.path());
    assert!(!status.complete);
    assert!(result.report.rows.is_empty());
    assert!(
        !result.errors.is_empty() || status.missing.iter().any(|m| m.contains("candidate runs")),
        "{status:?}"
    );
    assert!(
        status.missing.iter().any(|m| m.contains("rust/discount")),
        "{:?}",
        status.missing
    );
    assert!(!result.digest_reproduced);
}

// (c) The spend cap: after the first run the cap is spent, the harness stops,
// writes the partial result and does not call it complete.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_spend_cap_stops_the_run_after_the_first_task_with_a_partial_result() {
    let (base, _seen) = gateway("", false).await;
    let out = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    // A price so high that one scripted task spends far more than the cap.
    let code = exec(
        env_map(&base, Some("glm-5.3-flash=1000000/1000000"), Some("1.0")),
        args(
            out.path(),
            &[
                "--language",
                "rust",
                "--repeats",
                "2",
                "--work",
                work.path().to_str().unwrap(),
            ],
        ),
    )
    .await;
    assert_eq!(code, exit::INCOMPLETE);
    let (result, status) = read_result(out.path());
    assert!(status.spend.stopped_by_cap && !status.complete);
    assert_eq!(result.report.rows.len(), 1, "exactly one run was admitted");
    assert!(
        status.spend.spent_usd > status.spend.cap_usd,
        "the first task overshot; no second was started"
    );
    assert!(
        status.missing.iter().any(|m| m.contains("spend cap")),
        "{:?}",
        status.missing
    );
    assert!(
        status.missing.iter().any(|m| m.contains("candidate runs")),
        "{:?}",
        status.missing
    );
}

// A cap smaller than the reserve admits no task at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cap_below_the_reserve_starts_nothing() {
    let (base, seen) = gateway("", false).await;
    let out = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let code = exec(
        env_map(&base, Some(CATALOG), Some("0.05")),
        args(
            out.path(),
            &[
                "--language",
                "rust",
                "--lineage",
                "discount",
                "--work",
                work.path().to_str().unwrap(),
            ],
        ),
    )
    .await;
    assert_eq!(code, exit::INCOMPLETE);
    let (result, status) = read_result(out.path());
    assert!(status.spend.stopped_by_cap && result.report.rows.is_empty());
    assert!(seen.lock().unwrap().is_empty(), "no model request was made");
}

// (d) Mutations: an oracle needle in the goal, or in what the model says, is
// found by the leak search, the run fails and the status lists it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_oracle_needle_in_the_goal_fails_the_leak_search() {
    let (base, _seen) = gateway("", false).await;
    let out = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let env = env_map(&base, Some(CATALOG), Some("5"));
    let var = |k: &str| env.get(k).cloned();
    let cfg = live_config(&var).unwrap();
    let mut c = corpus();
    for w in &mut c.cases {
        // The mutation: a label key reaches the goal the model is given.
        w.goal.push_str(" {\"declared\": \"correct\"}");
    }
    let opts = LiveOptions {
        out_dir: out.path().to_owned(),
        work: work.path().to_owned(),
        languages: vec!["rust".into()],
        lineages: vec!["discount".into()],
        max_cost_usd: 5.0,
        task_timeout: std::time::Duration::from_secs(600),
    };
    let gate = gate_dir();
    let o = run_live(&cfg, &opts, &gate.join("../.."), &gate.join("corpora"), &c)
        .await
        .unwrap();
    assert_eq!(o.exit_code, 3, "{:?}", o.result.status);
    let (result, status) = read_result(out.path());
    assert!(!status.complete);
    assert!(
        status
            .missing
            .iter()
            .any(|m| m.starts_with("oracle leak:") && m.contains("goal")),
        "{:?}",
        status.missing
    );
    assert!(!result.leak.hits.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_oracle_needle_in_the_retained_events_fails_the_leak_search() {
    // The (scripted) model writes a needle into its plan; the Core retains it.
    let (base, _seen) = gateway(" MODBIT-ORACLE-CANARY", false).await;
    let out = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let code = exec(
        env_map(&base, Some(CATALOG), Some("5")),
        args(
            out.path(),
            &[
                "--language",
                "rust",
                "--lineage",
                "discount",
                "--repeats",
                "1",
                "--work",
                work.path().to_str().unwrap(),
            ],
        ),
    )
    .await;
    assert_eq!(code, exit::LEAK);
    let (_, status) = read_result(out.path());
    assert!(!status.complete);
    assert!(
        status
            .missing
            .iter()
            .any(|m| m.starts_with("oracle leak:") && m.contains("object:")),
        "{:?}",
        status.missing
    );
}

// A Core that asks a typed question does not hang the run: the harness
// answers it by protocol and the task resumes to its candidate.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_typed_question_is_answered_by_protocol_and_the_run_finishes() {
    let (base, _seen) = gateway("", true).await;
    let out = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let code = exec(
        env_map(&base, Some(CATALOG), Some("5")),
        args(
            out.path(),
            &[
                "--language",
                "rust",
                "--lineage",
                "discount",
                "--repeats",
                "1",
                "--work",
                work.path().to_str().unwrap(),
                "--task-timeout-secs",
                "300",
            ],
        ),
    )
    .await;
    let (result, status) = read_result(out.path());
    assert_eq!(code, exit::COMPLETE, "{status:?} {:?}", result.errors);
    let r = &result.report.rows[0];
    assert!(r.run.interactions >= 1, "{:?}", r.run);
    assert!(
        r.run
            .notes
            .iter()
            .any(|n| n.contains("answered by protocol")),
        "{:?}",
        r.run.notes
    );
    assert!(r.row.oracle_correct);
}

// Refusal: no MODBIT_LIVE, no cap, a bad flag; nothing is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refusals_write_nothing() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("bundle");
    assert_eq!(
        exec(Default::default(), args(&out, &[])).await,
        exit::REFUSED
    );
    assert!(!out.exists());
    let (base, _seen) = gateway("", false).await;
    // No cap anywhere: refused to spend without one.
    assert_eq!(
        exec(env_map(&base, Some(CATALOG), None), args(&out, &[])).await,
        exit::USAGE
    );
    assert!(!out.exists());
    assert_eq!(
        exec(
            env_map(&base, Some(CATALOG), Some("1")),
            args(&out, &["--repeats", "0"])
        )
        .await,
        exit::USAGE
    );
    assert!(!out.exists());
}

// A model's wrong attempt is a candidate too: it is scored, labelled by the
// hidden oracle (incorrect) and counted in the false-accept denominators; the
// widened gate rejects what the previous gate accepted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wrong_attempt_is_still_a_candidate_and_the_oracle_labels_it() {
    let (base, _seen) = gateway_of_class("overfit", "", false).await;
    let out = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let code = exec(
        env_map(&base, Some(CATALOG), Some("5")),
        args(
            out.path(),
            &[
                "--language",
                "rust",
                "--lineage",
                "discount",
                "--repeats",
                "1",
                "--work",
                work.path().to_str().unwrap(),
            ],
        ),
    )
    .await;
    let (result, status) = read_result(out.path());
    assert_eq!(code, exit::COMPLETE, "{status:?} {:?}", result.errors);
    let r = &result.report.rows[0];
    assert!(
        !r.row.oracle_correct,
        "the overfit attempt fails the hidden oracle"
    );
    let rust = result.rates.iter().find(|x| x.language == "rust").unwrap();
    assert_eq!((rust.incorrect, rust.false_accept.widened.n), (1, 1));
    assert_ne!(r.row.widened.verdict, "ACCEPT", "{:?}", r.row);
    assert_eq!(
        r.row.previous.verdict, "ACCEPT",
        "the previous gate accepted it: {:?}",
        r.row
    );
    let all = result.comparison.last().unwrap();
    assert_eq!(
        (
            all.incorrect_accepted_previous,
            all.incorrect_accepted_widened
        ),
        (1, 0)
    );
}
