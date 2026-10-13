//! PX-137 (QUAL-PX-137), the live harness proven offline: the agent loop,
//! the real tools (a real ripgrep process, the real planner), the oracle, the
//! retained transcripts, the digest and the failure semantics, with the model
//! replaced by a scripted OpenAI-compatible server. That server is a
//! stand-in for the MODEL only and says so: it derives its answers from what
//! the tools return, so a broken tool surface cannot pass. Nothing here is a
//! live run; the numbers of a live run come from the `live-evals` workflow.
//!
//! The tests that need ripgrep (the real baseline tool) skip with a loud
//! notice when it is not installed, like `external_baseline.rs`, unless
//! `MODBIT_BENCH_REQUIRE_EXTERNAL` is set, which makes its absence a failure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use modbit_bench_context_economics::chat::Chat;
use modbit_bench_context_economics::chat::scripted::{Scripted, serve};
use modbit_bench_context_economics::spend::{LiveStatus, SpendMeter};
use modbit_bench_retrieval::external::detect_rg_from_env;
use modbit_bench_retrieval::live::{
    CaseFile, MIN_CASES_LIVE, Transcript, answered_paths, assemble, judge, load_transcripts,
    rescore, validate_transcript,
};
use modbit_bench_retrieval::live_run::{
    DEFAULT_CASES, Knobs, Spec, build_padded, default_limits, execute, live_spec, prepare_checkout,
    verify_labels,
};
use modbit_bench_retrieval::live_tools::{Retriever, ToolBox, ToolSurface, schemas};
use serde_json::{Value, json};

const KEY: &str = "sk-live-0123456789abcdef0123456789";
const MODEL: &str = "scripted-model";
const TOKENS: [&str; 4] = ["alpha", "bravo", "charlie", "delta"];

fn rg_or_skip() -> bool {
    if detect_rg_from_env().is_ok() {
        return true;
    }
    assert!(
        std::env::var_os("MODBIT_BENCH_REQUIRE_EXTERNAL").is_none(),
        "ripgrep is required (MODBIT_BENCH_REQUIRE_EXTERNAL is set)"
    );
    eprintln!(
        "RIPGREP: NOT INSTALLED; this test is skipped. Set MODBIT_BENCH_REQUIRE_EXTERNAL=1 to make that a failure"
    );
    false
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=b",
            "-c",
            "user.email=b@e",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A small pinned Git repository: one module per token, plus decoys.
fn make_repo(dir: &Path) -> String {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for t in TOKENS {
        std::fs::write(
            dir.join("src").join(format!("mod_{t}.rs")),
            format!("// quokka{t} handler lives here\npub fn handle_{t}() -> u32 {{\n    1\n}}\n"),
        )
        .unwrap();
    }
    for i in 0..6 {
        std::fs::write(
            dir.join("src").join(format!("decoy_{i}.rs")),
            format!("pub fn unrelated_{i}() -> u32 {{\n    {i}\n}}\n"),
        )
        .unwrap();
    }
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "pinned"]);
    git(dir, &["rev-parse", "HEAD"])
}

fn case_file_text(repo: &Path, commit: &str, n: usize) -> String {
    let cases: Vec<Value> = (0..n)
        .map(|i| {
            let t = TOKENS[i % TOKENS.len()];
            json!({"id": format!("case-{i:03}"), "kind": if i % 2 == 0 { "behavior" } else { "multi_hop" },
                "question": format!("Which file contains the handler marked quokka{t}?"),
                "labels": [{"path": format!("src/mod_{t}.rs"), "evidence": format!("quokka{t}")}]})
        })
        .collect();
    json!({"schema": "modbit-retrieval-live-cases-v1", "name": "local-fixture",
        "repo_url": repo.to_str().unwrap(), "commit": commit,
        "description": "fixture", "cases": cases})
    .to_string()
}

fn scripted_model() -> Arc<dyn Fn(&Value) -> Scripted + Send + Sync> {
    Arc::new(|body| {
        let msgs = body["messages"].as_array().unwrap();
        let has_retrieve = body["tools"]
            .as_array()
            .is_some_and(|t| t.iter().any(|x| x["function"]["name"] == "retrieve"));
        let question = msgs[1]["content"].as_str().unwrap_or_default();
        let token = question
            .split(|c: char| !c.is_alphanumeric())
            .find(|w| w.starts_with("quokka"))
            .unwrap_or("quokkaalpha")
            .to_owned();
        let usage = Some((100_000, 100));
        let last = msgs.last().unwrap();
        if last["role"] == "tool" {
            // The answer comes from the tool result, never from the question.
            let text = last["content"].as_str().unwrap_or_default();
            let path = text
                .split(|c: char| c.is_whitespace() || c == ':')
                .find(|t| t.ends_with(".rs"));
            Scripted {
                text: json!({"files": path.into_iter().collect::<Vec<_>>()}).to_string(),
                usage,
                ..Scripted::default()
            }
        } else {
            let call = if has_retrieve {
                ("retrieve".to_owned(), json!({"query": token}))
            } else {
                ("search".to_owned(), json!({"pattern": token}))
            };
            Scripted {
                calls: vec![call],
                usage,
                ..Scripted::default()
            }
        }
    })
}

fn chat(base: &str) -> Chat {
    let m: HashMap<&str, String> = HashMap::from([
        ("OPENAI_API_KEY", KEY.to_owned()),
        ("MODBIT_OPENAI_BASE_URL", base.to_owned()),
        (
            "MODBIT_OPENAI_MODELS",
            format!("{MODEL}=0.15/0.50;ctx=200000"),
        ),
    ]);
    Chat::from_lookup(&|k| m.get(k).cloned(), MODEL).unwrap()
}

struct Fixture {
    _tmp: tempfile::TempDir,
    spec: Spec,
}

fn fixture(n_cases: usize, cap: f64) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("origin");
    let commit = make_repo(&repo);
    let text = case_file_text(&repo, &commit, n_cases);
    let mut spec = live_spec(&text, tmp.path().join("out"), tmp.path().join("work"), cap).unwrap();
    // The test-only lowering: a small corpus and four cases instead of fifty
    // and 100 MB. The CLI path cannot do this (see the CLI tests below).
    spec.min_cases = n_cases;
    spec.index_target_bytes = 3 * 1024 * 1024;
    Fixture { _tmp: tmp, spec }
}

fn read_json(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[tokio::test]
async fn a_good_run_scores_both_profiles_and_writes_a_complete_bundle() {
    if !rg_or_skip() {
        return;
    }
    let fx = fixture(4, 5.0);
    let (base, seen) = serve(scripted_model()).await;
    let chat = chat(&base);
    let meter = SpendMeter::new(5.0).unwrap();
    let status = execute(&fx.spec, &chat, &meter, Knobs::default())
        .await
        .unwrap();
    assert!(status.complete, "{:?}", status.missing);
    assert!(status.missing.is_empty() && !status.spend.stopped_by_cap);

    let out = &fx.spec.out_dir;
    let status_file: LiveStatus =
        serde_json::from_value(read_json(&out.join("status.json"))).unwrap();
    assert_eq!(
        (status_file.row.as_str(), status_file.complete),
        ("px137", true)
    );
    let result = read_json(&out.join("result.json"));
    assert_eq!(result["complete"], true);
    assert_eq!(result["digest_reproduced"], true);
    let rep = &result["report"];
    assert_eq!(
        (rep["case_count"].as_u64(), rep["paired_count"].as_u64()),
        (Some(4), Some(4))
    );
    for col in ["baseline", "treatment"] {
        let acc = &rep[col]["accuracy"];
        assert_eq!(
            (acc["k"].as_u64(), acc["n"].as_u64()),
            (Some(4), Some(4)),
            "{col}: {acc}"
        );
        assert!(
            acc["lo"].is_f64() && acc["hi"].is_f64(),
            "an interval on every proportion"
        );
        for m in ["input_tokens", "output_tokens", "tool_calls", "wall_ms"] {
            assert!(rep[col][m]["ci95"].is_array(), "{col}.{m} has an interval");
        }
        assert_eq!(rep[col]["usage_known"], true);
    }
    // The baseline column is real ripgrep; the treatment was offered retrieve
    // and chose it (the stand-in model does), the baseline never had it.
    assert_eq!(rep["baseline"]["tools"]["search"], 4);
    assert!(rep["baseline"]["tools"].get("retrieve").is_none());
    assert_eq!(rep["treatment"]["cases_using_retrieve"], 4);
    assert_eq!(rep["paired"].as_array().unwrap().len(), 5);
    assert!(rep["meta"]["index"]["indexed_bytes"].as_u64().unwrap() >= fx.spec.index_target_bytes);
    assert!(rep["meta"]["index"]["padding_files"].as_u64().unwrap() > 0);
    assert!(rep["meta"]["index"]["cold"]["total_ms"].as_f64().unwrap() > 0.0);
    let summary = std::fs::read_to_string(out.join("summary.md")).unwrap();
    assert!(
        summary.contains("Status: COMPLETE") && summary.contains("Wilson"),
        "{summary}"
    );

    // Every transcript: forced=false on every call, each call traceable to an
    // assistant message, and the files are where the rescore looks.
    let retained = load_transcripts(out).unwrap();
    assert_eq!(retained.len(), 8);
    for r in &retained {
        validate_transcript(&r.transcript).unwrap();
        assert!(r.transcript.tool_calls.iter().all(|c| !c.forced));
        assert!(
            out.join("transcripts")
                .join(&r.transcript.profile)
                .join(format!("{}.json", r.transcript.case_id))
                .is_file()
        );
    }
    // The surfaces the model saw: two tools for the baseline, three for the treatment.
    let seen = seen.lock().unwrap();
    let names = |b: &Value| -> Vec<String> {
        b["tools"]
            .as_array()
            .map(|t| {
                t.iter()
                    .map(|x| x["function"]["name"].as_str().unwrap().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert!(seen.iter().any(|b| names(b) == ["search", "read_file"]));
    assert!(
        seen.iter()
            .any(|b| names(b) == ["search", "read_file", "retrieve"])
    );
    assert!(seen.iter().all(|b| b["model"] == MODEL));
    drop(seen);

    // The offline rescore reproduces the digest from the transcripts alone.
    let again = rescore(out).unwrap();
    assert!(again.reproduced);
    assert_eq!(again.stored_digest, rep["report_digest"].as_str().unwrap());

    // ... and fails when a transcript is altered: the answer is changed.
    let victim = out
        .join("transcripts")
        .join("treatment")
        .join("case-000.json");
    let original = std::fs::read_to_string(&victim).unwrap();
    assert!(original.contains("src/mod_alpha.rs"));
    std::fs::write(
        &victim,
        original.replacen("src/mod_alpha.rs", "src/decoy_0.rs", 1),
    )
    .unwrap();
    let err = rescore(out).unwrap_err();
    assert!(err.contains("does not reproduce"), "{err}");
    std::fs::write(&victim, &original).unwrap();
    assert!(
        rescore(out).is_ok(),
        "restoring the bytes restores the digest"
    );
    // A stored table that was edited is caught too.
    let mut tampered = result.clone();
    tampered["report"]["baseline"]["accuracy"]["p"] = json!(0.123);
    std::fs::write(out.join("result.json"), tampered.to_string()).unwrap();
    assert!(rescore(out).unwrap_err().contains("does not reproduce"));
}

#[tokio::test]
async fn a_treatment_that_is_forced_to_retrieve_is_detected_and_fails_the_run() {
    if !rg_or_skip() {
        return;
    }
    let fx = fixture(4, 5.0);
    let (base, _) = serve(scripted_model()).await;
    let meter = SpendMeter::new(5.0).unwrap();
    let status = execute(
        &fx.spec,
        &chat(&base),
        &meter,
        Knobs {
            force_retrieve: true,
        },
    )
    .await
    .unwrap();
    assert!(!status.complete);
    assert!(
        status
            .missing
            .iter()
            .any(|m| m.contains("forced") && m.contains("never forced")),
        "{:?}",
        status.missing
    );
    // The mutation left its mark in the transcript, which is how it is caught.
    let t: Transcript = serde_json::from_value(read_json(
        &fx.spec.out_dir.join("transcripts/treatment/case-000.json"),
    ))
    .unwrap();
    assert!(t.tool_calls.iter().any(|c| c.forced));
    assert!(validate_transcript(&t).unwrap_err().contains("forced"));
    let b: Transcript = serde_json::from_value(read_json(
        &fx.spec.out_dir.join("transcripts/baseline/case-000.json"),
    ))
    .unwrap();
    assert!(
        validate_transcript(&b).is_ok(),
        "the baseline was not touched"
    );
}

#[tokio::test]
async fn the_spend_cap_stops_the_run_and_reports_a_partial_result() {
    if !rg_or_skip() {
        return;
    }
    // One scripted call costs 100000*0.15/M + 100*0.5/M = $0.01505.
    let fx = fixture(4, 0.04);
    let (base, _) = serve(scripted_model()).await;
    let meter = SpendMeter::new(0.04).unwrap();
    let status = execute(&fx.spec, &chat(&base), &meter, Knobs::default())
        .await
        .unwrap();
    assert!(!status.complete);
    assert!(status.spend.stopped_by_cap, "{:?}", status.spend);
    assert!(
        status.spend.spent_usd <= 0.04 + 0.016,
        "overshoot is bounded by one call"
    );
    assert!(
        status.missing.iter().any(|m| m.contains("spend cap")),
        "{:?}",
        status.missing
    );
    let result = read_json(&fx.spec.out_dir.join("result.json"));
    assert_eq!(result["complete"], false);
    assert!(result["report"]["paired_count"].as_u64().unwrap() < 4);
    assert_eq!(result["spend"]["stopped_by_cap"], true);
    // What was measured is still retained and still rescorable.
    assert!(!load_transcripts(&fx.spec.out_dir).unwrap().is_empty());
    assert!(
        fx.spec
            .out_dir
            .join("transcripts/treatment/case-000.error.json")
            .is_file()
    );
    assert!(rescore(&fx.spec.out_dir).is_ok());
}

#[tokio::test]
async fn a_failing_gateway_is_an_error_in_missing_not_a_score_of_zero() {
    if !rg_or_skip() {
        return;
    }
    let fx = fixture(4, 5.0);
    let (base, _) = serve(Arc::new(|_| Scripted {
        status: Some((400, "bad request".into())),
        ..Scripted::default()
    }))
    .await;
    let meter = SpendMeter::new(5.0).unwrap();
    let status = execute(&fx.spec, &chat(&base), &meter, Knobs::default())
        .await
        .unwrap();
    assert!(!status.complete);
    assert!(
        status.missing.iter().any(|m| m.contains("case error")),
        "{:?}",
        status.missing
    );
    let result = read_json(&fx.spec.out_dir.join("result.json"));
    assert_eq!(result["report"]["paired_count"], 0);
    assert!(
        load_transcripts(&fx.spec.out_dir).unwrap().is_empty(),
        "no zero-score transcript is invented"
    );
}

// ---- the oracle and the validator, without a model ----------------------

fn fixture_transcript(profile: &str, case: &str, answer: &str, retrieve: bool) -> Transcript {
    let mut messages = vec![
        json!({"role": "system", "content": "s"}),
        json!({"role": "user", "content": "q"}),
    ];
    let mut tool_calls = Vec::new();
    let mut model_calls = Vec::new();
    let call = |round: u32| modbit_bench_retrieval::live::ModelCall {
        round,
        input_tokens: 1000 * u64::from(round),
        output_tokens: 20,
        cached_input_tokens: 0,
        usage_known: true,
        latency_ms: 5,
        finish_reason: "stop".into(),
        cost_usd: 0.001,
        request_id: String::new(),
    };
    let tool = if retrieve { "retrieve" } else { "search" };
    messages.push(json!({"role": "assistant", "content": "", "tool_calls": [
        {"id": "c1", "type": "function", "function": {"name": tool, "arguments": "{}"}}]}));
    model_calls.push(call(1));
    tool_calls.push(modbit_bench_retrieval::live::ToolRecord {
        round: 1,
        call_id: "c1".into(),
        tool: tool.into(),
        arguments: "{}".into(),
        forced: false,
        error: false,
        truncated: false,
        result_chars: 3,
        ms: 1,
    });
    messages.push(json!({"role": "tool", "tool_call_id": "c1", "content": "x.rs"}));
    messages.push(json!({"role": "assistant", "content": answer}));
    model_calls.push(call(2));
    Transcript {
        schema: modbit_bench_retrieval::live::TRANSCRIPT_SCHEMA.into(),
        row: "px137".into(),
        profile: profile.into(),
        case_id: case.into(),
        kind: "behavior".into(),
        question: "q".into(),
        labels: vec!["a/b.rs".into()],
        model: "m".into(),
        messages,
        model_calls,
        tool_calls,
        final_text: answer.into(),
        capped: None,
        wall_ms: 10,
    }
}

#[test]
fn the_oracle_reads_the_json_answer_and_rejects_a_list_of_everything() {
    assert_eq!(
        answered_paths("{\"files\": [\"./a/b.rs\", \"`c/d.rs:12`\"]}"),
        ["a/b.rs", "c/d.rs"]
    );
    assert_eq!(
        answered_paths("Sure! ```json\n{\"files\": [\"a/b.rs\"]}\n```"),
        ["a/b.rs"],
        "fenced JSON is still JSON"
    );
    // Prose fallback: path-like tokens only.
    assert_eq!(answered_paths("It is in a/b.rs, near the top."), ["a/b.rs"]);
    assert!(answered_paths("no idea").is_empty());
    let labels = vec!["a/b.rs".to_owned()];
    assert!(judge(&labels, &["a/b.rs".into()]).correct);
    assert!(
        judge(&labels, &["a/b.rs".into(), "x.rs".into(), "y.rs".into()]).correct,
        "two extras allowed"
    );
    assert!(
        !judge(
            &labels,
            &["a/b.rs".into(), "x.rs".into(), "y.rs".into(), "z.rs".into()]
        )
        .correct
    );
    assert!(!judge(&labels, &["a/c.rs".into()]).correct);
    let two = vec!["a.rs".to_owned(), "b.rs".to_owned()];
    assert!(
        !judge(&two, &["a.rs".into()]).correct,
        "a multi-hop case needs every file"
    );
}

#[test]
fn the_validator_rejects_forced_calls_untraceable_calls_and_a_baseline_with_retrieve() {
    let good = fixture_transcript("treatment", "c", "{\"files\": [\"a/b.rs\"]}", true);
    validate_transcript(&good).unwrap();
    let mut forced = good.clone();
    forced.tool_calls[0].forced = true;
    assert!(validate_transcript(&forced).unwrap_err().contains("forced"));
    let mut untraceable = good.clone();
    untraceable.tool_calls[0].call_id = "nobody-asked".into();
    assert!(
        validate_transcript(&untraceable)
            .unwrap_err()
            .contains("not asked for")
    );
    let base_with_retrieve = fixture_transcript("baseline", "c", "{}", true);
    assert!(
        validate_transcript(&base_with_retrieve)
            .unwrap_err()
            .contains("no retrieve")
    );
    let mut miscounted = good;
    miscounted.model_calls.pop();
    assert!(validate_transcript(&miscounted).is_err());
}

#[test]
fn rescoring_retained_transcripts_reproduces_the_digest_and_catches_any_edit() {
    // Hand-built retained data (no model, no ripgrep): the rescore is pure.
    let dir = tempfile::tempdir().unwrap();
    let ids = ["c1", "c2", "c3"];
    let meta = modbit_bench_retrieval::live::ReportMeta {
        model: "m".into(),
        price: modbit_bench_context_economics::spend::Price {
            input_per_mtok_usd: 0.15,
            output_per_mtok_usd: 0.5,
        },
        repo: modbit_bench_retrieval::live::RepoInfo {
            url: "u".into(),
            commit: "0".repeat(40),
            head: "0".repeat(40),
            files: 1,
            bytes: 1,
            labels_verified: 3,
        },
        cases_name: "n".into(),
        cases_sha256: "s".into(),
        case_order: ids.iter().map(|s| (*s).to_owned()).collect(),
        limits: default_limits(),
        index: None,
    };
    for p in ["baseline", "treatment"] {
        std::fs::create_dir_all(dir.path().join("transcripts").join(p)).unwrap();
        for (i, id) in ids.iter().enumerate() {
            let ans = if p == "treatment" || i < 2 {
                "{\"files\": [\"a/b.rs\"]}"
            } else {
                "{\"files\": []}"
            };
            let t = fixture_transcript(p, id, ans, p == "treatment");
            std::fs::write(
                dir.path()
                    .join("transcripts")
                    .join(p)
                    .join(format!("{id}.json")),
                serde_json::to_vec_pretty(&t).unwrap(),
            )
            .unwrap();
        }
    }
    let retained = load_transcripts(dir.path()).unwrap();
    let report = assemble(&meta, &retained);
    assert_eq!(report.paired_count, 3);
    let acc = |p: &modbit_bench_retrieval::live::ProfileReport| p.accuracy.unwrap();
    assert_eq!((acc(&report.baseline).k, acc(&report.treatment).k), (2, 3));
    assert!(acc(&report.treatment).lo < acc(&report.treatment).p);
    let acc_row = report
        .paired
        .iter()
        .find(|p| p.measure == "accuracy")
        .unwrap();
    assert_eq!((acc_row.wins, acc_row.losses, acc_row.ties), (1, 0, 2));
    std::fs::write(
        dir.path().join("result.json"),
        json!({"report": report}).to_string(),
    )
    .unwrap();
    assert!(rescore(dir.path()).unwrap().reproduced);
    // Change one token count inside a transcript: the digest must move.
    let p = dir.path().join("transcripts/baseline/c1.json");
    let text = std::fs::read_to_string(&p).unwrap();
    std::fs::write(
        &p,
        text.replacen("\"input_tokens\": 1000", "\"input_tokens\": 1", 1),
    )
    .unwrap();
    assert!(
        rescore(dir.path())
            .unwrap_err()
            .contains("does not reproduce")
    );
    // A transcript removed: the tables change, so the rescore fails.
    std::fs::write(&p, &text).unwrap();
    assert!(rescore(dir.path()).is_ok());
    std::fs::remove_file(dir.path().join("transcripts/treatment/c3.json")).unwrap();
    assert!(
        rescore(dir.path())
            .unwrap_err()
            .contains("does not reproduce")
    );
}

// ---- the tools -----------------------------------------------------------

#[test]
fn read_file_cannot_leave_the_checkout() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.rs"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(tmp.path().join("secret.txt"), "top secret").unwrap();
    let tools = ToolBox::new(&repo, PathBuf::from("rg"), None).unwrap();
    let ok = tools.execute(
        "read_file",
        &json!({"path": "src/a.rs", "start_line": 2, "end_line": 3}),
    );
    assert!(
        !ok.error
            && ok.text.contains("2: two")
            && ok.text.contains("3: three")
            && !ok.text.contains("1: one"),
        "{}",
        ok.text
    );
    for bad in [
        "../secret.txt",
        "src/../../secret.txt",
        "/etc/passwd",
        "src/missing.rs",
        "src",
    ] {
        let r = tools.execute("read_file", &json!({ "path": bad }));
        assert!(
            r.error && !r.text.contains("top secret"),
            "{bad}: {}",
            r.text
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(tmp.path().join("secret.txt"), repo.join("src/link.txt"))
            .unwrap();
        let r = tools.execute("read_file", &json!({"path": "src/link.txt"}));
        assert!(
            r.error && !r.text.contains("top secret"),
            "a symlink out of the tree: {}",
            r.text
        );
    }
    assert!(
        tools.execute("retrieve", &json!({"query": "x"})).error,
        "the baseline has no retrieve"
    );
    assert!(
        !schemas(ToolSurface::Baseline)
            .iter()
            .any(|s| s["function"]["name"] == "retrieve")
    );
    let big = "x".repeat(20_000);
    std::fs::write(repo.join("big.txt"), format!("{big}\n")).unwrap();
    let r = tools.execute("read_file", &json!({"path": "big.txt"}));
    assert!(r.truncated && r.text.chars().count() < 6_100);
}

#[test]
fn search_treats_the_model_s_pattern_as_data_and_retrieve_is_the_real_planner() {
    if !rg_or_skip() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    make_repo(&repo);
    let rg = detect_rg_from_env().unwrap();
    let tools = ToolBox::new(&repo, rg.clone(), Some(Retriever::build(&repo).unwrap())).unwrap();
    let hit = tools.execute("search", &json!({"pattern": "quokkabravo"}));
    assert!(
        !hit.error && hit.text.starts_with("src/mod_bravo.rs:1:"),
        "{}",
        hit.text
    );
    // A pattern that looks like a flag is a pattern, not an option.
    let flaglike = tools.execute("search", &json!({"pattern": "--files"}));
    assert!(
        flaglike.text == "no matches" && !flaglike.error,
        "{}",
        flaglike.text
    );
    let bad_regex = tools.execute("search", &json!({"pattern": "("}));
    assert!(
        bad_regex.error && bad_regex.text.contains("ripgrep error"),
        "{}",
        bad_regex.text
    );
    let globbed = tools.execute(
        "search",
        &json!({"pattern": "handle_", "glob": "*charlie*"}),
    );
    assert!(
        globbed.text.contains("mod_charlie.rs") && !globbed.text.contains("mod_alpha.rs"),
        "{}",
        globbed.text
    );
    let planner = tools.execute("retrieve", &json!({"query": "quokkacharlie"}));
    assert!(
        !planner.error && planner.text.starts_with("src/mod_charlie.rs"),
        "{}",
        planner.text
    );
    assert!(planner.text.contains("found by"));
}

// ---- the case file, the checkout and the corpus ---------------------------

#[test]
fn the_committed_case_file_is_pinned_and_large_enough() {
    let f = CaseFile::parse(DEFAULT_CASES).unwrap();
    assert!(f.cases.len() >= MIN_CASES_LIVE, "{}", f.cases.len());
    assert_eq!(f.commit.len(), 40);
    assert!(f.repo_url.starts_with("https://github.com/"));
    assert!(f.cases.iter().any(|c| c.kind == "multi_hop"));
    assert!(
        f.cases
            .iter()
            .filter(|c| c.kind == "multi_hop")
            .all(|c| c.labels.len() >= 2)
    );
    for c in &f.cases {
        // A question must not hand the agent its answer.
        for l in &c.labels {
            assert!(!c.question.contains(&l.path), "{} leaks {}", c.id, l.path);
        }
    }
    let live = live_spec(DEFAULT_CASES, "o".into(), "w".into(), 1.0).unwrap();
    assert_eq!(
        (live.min_cases, live.index_target_bytes),
        (50, 100 * 1024 * 1024)
    );
}

#[test]
fn the_checkout_must_be_the_pinned_commit_and_every_label_must_resolve() {
    let tmp = tempfile::tempdir().unwrap();
    let origin = tmp.path().join("origin");
    let commit = make_repo(&origin);
    let work = tmp.path().join("work");
    let repo = prepare_checkout(origin.to_str().unwrap(), &commit, &work).unwrap();
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), commit);
    let f = CaseFile::parse(&case_file_text(&origin, &commit, 4)).unwrap();
    assert!(verify_labels(&repo, &f.cases).is_empty());
    let mut broken = f.clone();
    broken.cases[0].labels[0].path = "src/renamed.rs".into();
    broken.cases[1].labels[0].evidence = "not in the file".into();
    assert_eq!(verify_labels(&repo, &broken.cases).len(), 2);
    // A different pin is refused, as is a dirty tree.
    assert!(prepare_checkout(origin.to_str().unwrap(), &"1".repeat(40), &work).is_err());
    std::fs::write(repo.join("src/mod_alpha.rs"), "changed").unwrap();
    assert!(
        prepare_checkout(origin.to_str().unwrap(), &commit, &work)
            .unwrap_err()
            .contains("local changes")
    );
}

#[test]
fn the_padded_corpus_reaches_its_target_deterministically() {
    let tmp = tempfile::tempdir().unwrap();
    let origin = tmp.path().join("origin");
    make_repo(&origin);
    let a = build_padded(&origin, &tmp.path().join("a"), 200_000).unwrap();
    let b = build_padded(&origin, &tmp.path().join("b"), 200_000).unwrap();
    assert!(a.checkout_bytes + a.padding_bytes >= 200_000);
    assert_eq!(
        (a.padding_files, a.padding_bytes),
        (b.padding_files, b.padding_bytes)
    );
    let one =
        |d: &str| std::fs::read(tmp.path().join(d).join("padding/p0000/synth_000003.rs")).unwrap();
    assert_eq!(one("a"), one("b"));
    assert_eq!(one("a").len(), 16 * 1024);
    assert!(
        tmp.path().join("a/.git").exists(),
        "indexed like a real workspace"
    );
    assert!(tmp.path().join("a/src/mod_alpha.rs").is_file());
}

// ---- the CLI: refusals, exit codes ----------------------------------------

fn cli(env: &[(&str, &str)], args: &[&str]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_retrieval-live"));
    c.args(args)
        .env_remove("MODBIT_LIVE")
        .env_remove("MODBIT_LIVE_API_KEY")
        .env_remove("MODBIT_LIVE_MODEL")
        .env_remove("MODBIT_LIVE_BASE_URL")
        .env_remove("MODBIT_LIVE_MAX_COST_USD")
        .env_remove("MODBIT_RG_BIN");
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

fn live_env<'a>() -> Vec<(&'a str, &'a str)> {
    vec![
        ("MODBIT_LIVE", "1"),
        ("MODBIT_LIVE_API_KEY", KEY),
        ("MODBIT_LIVE_MODEL", MODEL),
        ("MODBIT_LIVE_BASE_URL", "http://127.0.0.1:9/v4"),
        ("MODBIT_LIVE_ALLOW_LOOPBACK", "1"),
        (
            "MODBIT_OPENAI_MODELS",
            "scripted-model=0.15/0.50;ctx=200000",
        ),
    ]
}

#[test]
fn live_mode_is_refused_without_the_switch_the_key_or_a_cap_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    let o = out.to_str().unwrap();
    // No MODBIT_LIVE=1.
    let r = cli(&[], &["--out-dir", o, "--max-cost-usd", "1"]);
    assert_eq!(
        r.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert!(String::from_utf8_lossy(&r.stderr).contains("LIVE: NOT RUN"));
    // A switch but no key.
    let r = cli(
        &[("MODBIT_LIVE", "1")],
        &["--out-dir", o, "--max-cost-usd", "1"],
    );
    assert_eq!(r.status.code(), Some(2));
    // Everything but a spend cap.
    let r = cli(&live_env(), &["--out-dir", o]);
    assert_eq!(r.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&r.stderr).contains("spend cap"));
    let r = cli(&live_env(), &["--out-dir", o, "--max-cost-usd", "0"]);
    assert_eq!(r.status.code(), Some(2));
    // No out dir.
    let r = cli(&live_env(), &["--max-cost-usd", "1"]);
    assert_eq!(r.status.code(), Some(2));
    assert!(!out.exists(), "a refused run writes nothing");
}

#[test]
fn a_case_set_below_fifty_is_refused_on_the_live_path() {
    let tmp = tempfile::tempdir().unwrap();
    let origin = tmp.path().join("origin");
    let commit = make_repo(&origin);
    let cases = tmp.path().join("cases.json");
    std::fs::write(&cases, case_file_text(&origin, &commit, 49)).unwrap();
    let out = tmp.path().join("out");
    let r = cli(
        &live_env(),
        &[
            "--out-dir",
            out.to_str().unwrap(),
            "--max-cost-usd",
            "1",
            "--cases",
            cases.to_str().unwrap(),
        ],
    );
    assert_eq!(
        r.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let status: LiveStatus = serde_json::from_value(read_json(&out.join("status.json"))).unwrap();
    assert!(!status.complete);
    assert!(
        status
            .missing
            .iter()
            .any(|m| m.contains("below the required 50")),
        "{:?}",
        status.missing
    );
    assert!(!out.join("transcripts").exists(), "no model call was made");
}

#[test]
fn a_missing_ripgrep_fails_the_run_instead_of_skipping_the_baseline_column() {
    let tmp = tempfile::tempdir().unwrap();
    let origin = tmp.path().join("origin");
    let commit = make_repo(&origin);
    let cases = tmp.path().join("cases.json");
    std::fs::write(&cases, case_file_text(&origin, &commit, 50)).unwrap();
    let out = tmp.path().join("out");
    let mut env = live_env();
    env.push(("MODBIT_RG_BIN", "/nonexistent/rg"));
    let r = cli(
        &env,
        &[
            "--out-dir",
            out.to_str().unwrap(),
            "--max-cost-usd",
            "1",
            "--cases",
            cases.to_str().unwrap(),
        ],
    );
    assert_eq!(
        r.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let status: LiveStatus = serde_json::from_value(read_json(&out.join("status.json"))).unwrap();
    assert!(!status.complete);
    assert!(
        status
            .missing
            .iter()
            .any(|m| m.starts_with("baseline binary") && m.contains("BASELINE MISSING")),
        "{:?}",
        status.missing
    );
    let result = read_json(&out.join("result.json"));
    assert_eq!(result["report"]["paired_count"], 0);
    assert!(String::from_utf8_lossy(&r.stderr).contains("INCOMPLETE"));
}

#[test]
fn rescore_from_the_cli_needs_no_environment_and_exits_nonzero_on_a_bad_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let r = cli(&[], &["--rescore", tmp.path().to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&r.stderr).contains("RESCORE FAILED"));
}

/// The whole live path against a scripted model: the real binary, the real
/// pinned ripgrep checkout over the network, the 100 MB index measurement.
/// Run by hand: `cargo test -p modbit-bench-retrieval --test live_agent
/// -- --ignored --nocapture full_cli_run`.
#[test]
#[ignore = "network (git clone of the pinned repository) and a 100 MB index build"]
fn full_cli_run_against_a_scripted_model() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (base, _) = rt.block_on(serve(Arc::new(|body| {
        let msgs = body["messages"].as_array().unwrap();
        let last = msgs.last().unwrap();
        if last["role"] == "tool" {
            let text = last["content"].as_str().unwrap_or_default();
            let path = text
                .split(|c: char| c.is_whitespace() || c == ':')
                .find(|t| t.ends_with(".rs"));
            return Scripted {
                text: json!({"files": path.into_iter().collect::<Vec<_>>()}).to_string(),
                usage: Some((3_000, 40)),
                ..Scripted::default()
            };
        }
        let q = msgs[1]["content"].as_str().unwrap_or_default();
        let word = q
            .split(|c: char| !c.is_alphanumeric())
            .max_by_key(|w| w.len())
            .unwrap_or("fn")
            .to_owned();
        Scripted {
            calls: vec![(
                "search".into(),
                json!({"pattern": word, "ignore_case": true}),
            )],
            usage: Some((3_000, 40)),
            ..Scripted::default()
        }
    })));
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    let mut env = live_env();
    env.retain(|(k, _)| *k != "MODBIT_LIVE_BASE_URL");
    env.push(("MODBIT_LIVE_BASE_URL", &base));
    let rg_bin = std::env::var("MODBIT_RG_BIN").unwrap_or_default();
    if !rg_bin.is_empty() {
        env.push(("MODBIT_RG_BIN", &rg_bin));
    }
    let r = cli(
        &env,
        &[
            "--out-dir",
            out.to_str().unwrap(),
            "--work-dir",
            tmp.path().join("work").to_str().unwrap(),
            "--max-cost-usd",
            "5",
        ],
    );
    eprintln!("{}", String::from_utf8_lossy(&r.stdout));
    eprintln!("{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(r.status.code(), Some(0));
    let result = read_json(&out.join("result.json"));
    assert_eq!(result["complete"], true);
    assert!(
        result["report"]["meta"]["index"]["indexed_bytes"]
            .as_u64()
            .unwrap()
            >= 100 * 1024 * 1024
    );
    assert!(rescore(&out).unwrap().reproduced);
}

/// Index-time probe at a chosen corpus size over a local checkout:
/// `MODBIT_PROBE_CHECKOUT=<dir> MODBIT_PROBE_MB=20 cargo test -p
/// modbit-bench-retrieval --test live_agent --release -- --ignored
/// --nocapture index_scale_probe`.
#[test]
#[ignore = "manual: sizes the index measurement before a live run"]
fn index_scale_probe() {
    let (Some(dir), Some(mb)) = (
        std::env::var_os("MODBIT_PROBE_CHECKOUT"),
        std::env::var("MODBIT_PROBE_MB")
            .ok()
            .and_then(|v| v.parse::<u64>().ok()),
    ) else {
        eprintln!("set MODBIT_PROBE_CHECKOUT and MODBIT_PROBE_MB");
        return;
    };
    let f = CaseFile::parse(DEFAULT_CASES).unwrap();
    let work = tempfile::tempdir().unwrap();
    let t = std::time::Instant::now();
    let ix = modbit_bench_retrieval::live_run::measure_index(
        Path::new(&dir),
        work.path(),
        mb * 1024 * 1024,
        &f.cases[0],
    )
    .unwrap();
    eprintln!("PROBE {mb} MB: {:#?} total {:?}", ix, t.elapsed());
}
