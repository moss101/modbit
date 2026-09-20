//! Offline proofs of the Eval Harness (PX-020): the frozen suite and its
//! digest, the bundle refusals QUAL-PX-020 names, event scoring, and the
//! real `competence-baseline` binary driving the real CLI and Core on one
//! task against a scripted OpenAI-compatible model. The live baseline itself
//! is only ever produced with a real model (docs/82).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

use modbit_bench_agent_engineering::{
    Bundle, BundleRefused, Counts, Economics, Environment, EventLog, Protocol, Rate, Suite,
    TargetRecord, TrialOutcome, count, metrics, parse_events,
};
use serde_json::json;

/// JSON float parsing is not byte-stable to the last ulp, so a parsed rate
/// is compared with a tolerance.
fn same_rate(a: &Rate, b: &Rate) -> bool {
    a.successes == b.successes
        && a.n == b.n
        && (a.rate - b.rate).abs() < 1e-9
        && (a.ci95.0 - b.ci95.0).abs() < 1e-9
        && (a.ci95.1 - b.ci95.1).abs() < 1e-9
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn suite_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("suites/internal/tasks.json")
}

#[test]
fn the_internal_suite_is_frozen_and_its_digest_covers_hidden_acceptance() {
    let (suite, digest) = Suite::load(&suite_path()).unwrap();
    assert_eq!(suite.id, "internal-competence");
    assert_eq!(suite.tasks.len(), 6);
    assert_eq!(suite.protocol.trials, 3);
    assert_eq!(suite.protocol.approvals, "deny");
    let languages: std::collections::BTreeSet<&str> =
        suite.tasks.iter().map(|t| t.language.as_str()).collect();
    assert_eq!(
        languages.into_iter().collect::<Vec<_>>(),
        ["python", "rust", "typescript"]
    );
    for t in &suite.tasks {
        assert!(
            !t.acceptance.protected_tests.is_empty(),
            "{}: DI-3 needs a protected test",
            t.id
        );
        assert!(
            ["easy", "medium"].contains(&t.difficulty.as_str()),
            "{}",
            t.id
        );
        for f in &t.acceptance.hidden_files {
            assert!(
                Suite::hidden_source(&suite_path(), f).is_file(),
                "{}: {}",
                t.id,
                f.from
            );
        }
    }
    // The same suite copied elsewhere digests identically; a changed hidden
    // file changes the digest although tasks.json is byte-identical.
    let tmp = tempfile::tempdir().unwrap();
    let copy = tmp.path().join("suite");
    copy_dir(suite_path().parent().unwrap(), &copy);
    let (_, same) = Suite::load(&copy.join("tasks.json")).unwrap();
    assert_eq!(same, digest);
    let hidden = copy.join("hidden/python-service/test_hidden_zero.py");
    std::fs::write(&hidden, "def test_weakened():\n    pass\n").unwrap();
    let (_, changed) = Suite::load(&copy.join("tasks.json")).unwrap();
    assert_ne!(changed, digest);
    // Structural refusals.
    let mut broken = suite.clone();
    broken.tasks[0].acceptance.commands.clear();
    assert!(broken.check().unwrap_err().contains("names no command"));
    let mut dup = suite.clone();
    dup.tasks[1].id = dup.tasks[0].id.clone();
    assert!(dup.check().unwrap_err().contains("duplicate"));
    let mut none = suite;
    none.protocol.trials = 0;
    assert!(none.check().unwrap_err().contains("trial count"));
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &dst.join(e.file_name()));
        } else {
            std::fs::copy(&p, dst.join(e.file_name())).unwrap();
        }
    }
}

fn trial(task: &str, n: u32, verified: bool, repairs: u32) -> TrialOutcome {
    TrialOutcome {
        task: task.into(),
        trial: n,
        state: if verified { "ReadyForReview" } else { "Failed" }.into(),
        exit_code: i32::from(!verified) * 3,
        core_verified: verified,
        acceptance_passed: verified,
        test_integrity_ok: true,
        verified_success: verified,
        first_pass: verified && repairs == 0,
        first_candidate: verified && repairs <= 1,
        counts: Counts {
            repair_attempts: repairs,
            plan_recorded: true,
            self_review_unresolved: Some(0),
            ready_for_review: verified,
            ..Counts::default()
        },
        economics: Economics {
            cost_usd: Some(0.01),
            wall_ms: 1000,
            ..Economics::default()
        },
        event_log: EventLog::default(),
        diff_sha256: String::new(),
        interactions: 0,
        notes: vec![],
    }
}

fn bundle(trials_per_task: u32, trials: Vec<TrialOutcome>) -> Bundle {
    let tasks: Vec<String> = {
        let mut v: Vec<String> = trials.iter().map(|t| t.task.clone()).collect();
        v.dedup();
        v
    };
    Bundle {
        schema: modbit_bench_agent_engineering::bundle::SCHEMA.into(),
        suite_id: "internal-competence".into(),
        suite_version: "1.0.0".into(),
        task_list_digest: "d".repeat(64),
        tasks,
        protocol: Protocol {
            trials_per_task,
            max_turns: 24,
            configuration: "direct".into(),
            endpoint: "openai".into(),
            model: "glm-5.3-flash".into(),
            base_url_host: "api.z.ai".into(),
            catalog_prices_usd_per_mtok: Some((0.15, 0.5)),
            gold_patch_access: false,
            images: vec![],
            question_answer: "proceed".into(),
            approvals: "deny".into(),
        },
        environment: Environment {
            modbit_revision: "abc".into(),
            build_digest: "b".repeat(64),
            harness_version: modbit_bench_agent_engineering::HARNESS_VERSION.into(),
            toolchains: BTreeMap::new(),
            os: "test".into(),
        },
        metrics: metrics(&trials),
        trials,
        generated_at: "2026-09-20T00:00:00Z".into(),
    }
}

#[test]
fn bundle_validation_refuses_what_the_frozen_protocol_forbids_and_no_target_precedes_a_baseline() {
    let good = bundle(
        2,
        vec![
            trial("a", 1, true, 0),
            trial("a", 2, false, 2),
            trial("b", 1, true, 1),
            trial("b", 2, true, 0),
        ],
    );
    good.validate().unwrap();
    let good_digest = modbit_bench_agent_engineering::digest_of(&good.to_file_bytes());
    assert_eq!(good_digest.len(), 64);
    let mut gold = good.clone();
    gold.protocol.gold_patch_access = true;
    assert_eq!(gold.validate(), Err(BundleRefused::GoldPatchAccess));
    let mut unpinned = good.clone();
    unpinned.protocol.images =
        vec!["ghcr.io/swebench/sweb.eval.x86_64.django_1776_django-11099:latest".into()];
    assert!(matches!(
        unpinned.validate(),
        Err(BundleRefused::UnpinnedImage(_))
    ));
    let mut pinned = good.clone();
    pinned.protocol.images = vec![format!(
        "ghcr.io/swebench/sweb.eval.x86_64.django_1776_django-11099@sha256:{}",
        "0".repeat(64)
    )];
    pinned.validate().unwrap();
    let mut no_trials = good.clone();
    no_trials.protocol.trials_per_task = 0;
    assert_eq!(no_trials.validate(), Err(BundleRefused::MissingTrialCount));
    let mut short = good.clone();
    short.trials.pop();
    assert!(
        matches!(short.validate(), Err(BundleRefused::TrialCountMismatch { ref task, expected: 2, found: 1 }) if task == "b")
    );
    let mut no_model = good.clone();
    no_model.protocol.model.clear();
    assert_eq!(
        no_model.validate(),
        Err(BundleRefused::Unpinned("model".into()))
    );
    // Metrics: 3/4 verified, 2/4 first pass, the repair-loop distribution, Wilson intervals.
    let m = &good.metrics;
    assert_eq!((m.verified_success.successes, m.verified_success.n), (3, 4));
    assert_eq!(
        (m.first_pass_success.successes, m.first_pass_success.n),
        (2, 4)
    );
    assert!(m.verified_success.ci95.0 > 0.2 && m.verified_success.ci95.1 < 1.0);
    assert_eq!(m.repair_loops, BTreeMap::from([(0, 2), (1, 1), (2, 1)]));
    assert_eq!(m.per_task["a"].verified_success, Rate::of(1, 2));
    assert_eq!(m.per_task["b"].first_pass_success, Rate::of(1, 2));
    assert_eq!(
        m.cost_and_time
            .cost_per_verified_success_usd
            .map(|c| (c * 1000.0).round()),
        Some(13.0)
    );
    assert!(m.test_selection_quality.contains("not measured"));
    // PX-021's precondition: a target carries the baseline digest and refuses a bad baseline.
    let t = TargetRecord::against(
        &good,
        &good_digest,
        "verified_success",
        "A",
        0.5,
        "Wilson 95%, 3 trials",
    )
    .unwrap();
    assert_eq!(t.baseline_digest, good_digest);
    assert!(
        TargetRecord::against(&gold, &good_digest, "verified_success", "A", 0.5, "m")
            .unwrap_err()
            .contains("no target before a valid baseline")
    );
    assert!(
        TargetRecord::against(&good, "not-a-digest", "verified_success", "A", 0.5, "m")
            .unwrap_err()
            .contains("not a bundle digest")
    );
    assert!(TargetRecord::against(&good, &good_digest, "", "A", 0.5, "m").is_err());
}

#[test]
fn event_scoring_counts_the_doc_63_signals_from_the_log() {
    let task = "0123456789abcdef0123456789abcdef";
    let lines = [
        json!({"offset": 10, "event_type": "PlanRecorded", "task_id": task, "payload": {"version": 1, "expected_files": ["src/lib.rs"]}}),
        json!({"offset": 11, "event_type": "RetrievalRecorded", "task_id": task, "payload": {"path": "src/lib.rs"}}),
        json!({"offset": 12, "event_type": "FileChanged", "task_id": task, "payload": {"path": "src/lib.rs", "before_hash": "h1", "after_hash": "h2"}}),
        json!({"offset": 13, "event_type": "FileChanged", "task_id": task, "payload": {"path": "tests/new.rs", "before_hash": null, "after_hash": "h3"}}),
        json!({"offset": 14, "event_type": "FileChanged", "task_id": task, "payload": {"path": "README.md", "before_hash": "h4", "after_hash": "h5"}}),
        json!({"offset": 15, "event_type": "ToolCallPolicyDecision", "task_id": task, "payload": {"allowed": false, "decision": "DENY"}}),
        json!({"offset": 16, "event_type": "DiffInvariantViolated", "task_id": task, "payload": {"invariant": "DI-3", "class": "DENY", "paths": ["tests/quantities.rs"]}}),
        json!({"offset": 17, "event_type": "RepairAttemptRecorded", "task_id": task, "payload": {"attempt_ordinal": 1}}),
        json!({"offset": 18, "event_type": "RepairEscalated", "task_id": task, "payload": {"attempts": 2}}),
        json!({"offset": 19, "event_type": "NoProgressDetected", "task_id": task, "payload": {"turns": 3}}),
        json!({"offset": 20, "event_type": "PlanRevised", "task_id": task, "payload": {"version": 2, "added": ["README.md"]}}),
        json!({"offset": 21, "event_type": "UserQuestionAsked", "task_id": task, "payload": {}}),
        json!({"offset": 22, "event_type": "FlakyCheckQuarantined", "task_id": task, "payload": {"check_id": "c"}}),
        json!({"offset": 23, "event_type": "RegressionAttributed", "task_id": task, "payload": {"check_id": "c2"}}),
        json!({"offset": 24, "event_type": "VerificationRunRecorded", "task_id": task, "payload": {"stage": "COMPLETION", "status": "PASSED"}}),
        json!({"offset": 25, "event_type": "AcceptanceGateEvaluated", "task_id": task, "payload": {"verdict": "ACCEPT"}}),
        json!({"offset": 26, "event_type": "SelfReviewRecorded", "task_id": task, "payload": {"unresolved": 0}}),
        json!({"offset": 27, "event_type": "TaskReadyForReview", "task_id": task, "payload": {}}),
        json!({"offset": 28, "event_type": "ModelUsageRecorded", "task_id": task, "payload": {}}),
        json!({"offset": 29, "event_type": "FileChanged", "task_id": "ffffffffffffffffffffffffffffffff", "payload": {"path": "other.rs", "before_hash": "x"}}),
        json!({"offset": 30, "event_type": "SessionCreated", "task_id": null, "payload": {}}),
    ];
    let text: String = lines
        .iter()
        .map(|l| format!("{l}\n"))
        .chain(["not json\n".to_owned()])
        .collect();
    let events = parse_events(&text);
    assert_eq!(events.len(), 21);
    let c = count(&events, task);
    assert_eq!(c.plan_v1_files, ["src/lib.rs"]);
    assert_eq!(c.files_changed, ["README.md", "src/lib.rs", "tests/new.rs"]);
    assert_eq!(c.files_outside_plan, ["README.md", "tests/new.rs"]);
    assert_eq!(
        c.edits_without_retrieval, 1,
        "README.md was edited unread; the created file needs no retrieval"
    );
    assert_eq!(
        (c.policy_denies, c.di3_violations, c.diff_invariants["DI-3"]),
        (1, 1, 1)
    );
    assert_eq!(
        (
            c.repair_attempts,
            c.repair_escalations,
            c.no_progress_escalations
        ),
        (1, 1, 1)
    );
    assert_eq!(
        (
            c.plan_revisions,
            c.questions,
            c.flaky_quarantines,
            c.regressions_attributed
        ),
        (1, 1, 1, 1)
    );
    assert_eq!(c.gate_verdict.as_deref(), Some("ACCEPT"));
    assert_eq!(
        c.last_verification,
        Some(("COMPLETION".into(), "PASSED".into()))
    );
    assert_eq!(c.self_review_unresolved, Some(0));
    assert!(c.ready_for_review && c.plan_recorded && !c.failed);
    assert_eq!(c.model_calls, 1);
    assert_eq!(c.offsets, Some((10, 30)));
}

// ---- the real binary through the real CLI and Core, scripted model ---------

const RESPONSE_HEAD: &str = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: {id}\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n";

/// A scripted OpenAI-compatible chat-completions server: the reply to a
/// request is the script step indexed by the number of tool-result messages
/// in the request (one tool call per turn), as the Core's own tests do.
fn scripted_server(script: Vec<serde_json::Value>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for sock in listener.incoming() {
            let Ok(mut sock) = sock else { return };
            let script = script.clone();
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
                let body: serde_json::Value =
                    serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
                let results = body["messages"]
                    .as_array()
                    .map(|m| m.iter().filter(|x| x["role"] == "tool").count())
                    .unwrap_or(0);
                let reply = script
                    .get(results)
                    .cloned()
                    .unwrap_or_else(|| json!({"text": "I have nothing further to do."}));
                let mut frames: Vec<String> = Vec::new();
                if let Some(t) = reply["text"].as_str() {
                    frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"content":t},"finish_reason":null}]}).to_string());
                }
                let calls = reply["calls"].as_array().cloned().unwrap_or_default();
                for (i, c) in calls.iter().enumerate() {
                    frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":i,"id":format!("call_{results}_{i}"),"type":"function","function":{"name":c["name"],"arguments":c["args"].to_string()}}]},"finish_reason":null}]}).to_string());
                }
                let finish = if calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                };
                let prompt_tokens = body["messages"].to_string().len().div_ceil(4);
                frames.push(json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":prompt_tokens,"completion_tokens":32}}).to_string());
                frames.push("[DONE]".into());
                let _ = sock.write_all(
                    RESPONSE_HEAD
                        .replace("{id}", &format!("req_{results}"))
                        .as_bytes(),
                );
                for f in frames {
                    let frame = format!("data: {f}\n\n");
                    let _ =
                        sock.write_all(format!("{:x}\r\n{}\r\n", frame.len(), frame).as_bytes());
                }
                let _ = sock.write_all(b"0\r\n\r\n");
                let _ = sock.flush();
                let _ = sock.shutdown(std::net::Shutdown::Both);
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// The product binaries next to this test binary, built when missing (as the
/// CLI's own smoke test does).
fn product_binaries() -> (PathBuf, PathBuf) {
    let dir = root().join("target/debug");
    let cli = dir.join(format!("modbit-cli{}", std::env::consts::EXE_SUFFIX));
    let core = dir.join(format!("modbit-core{}", std::env::consts::EXE_SUFFIX));
    if !cli.is_file() || !core.is_file() {
        let st = Command::new("cargo")
            .args(["build", "-p", "modbit-cli", "-p", "modbit-core"])
            .current_dir(root())
            .status()
            .unwrap();
        assert!(st.success(), "building modbit-cli and modbit-core");
    }
    (cli, core)
}

fn reject_zero_script() -> Vec<serde_json::Value> {
    let test_zero = "from service import parse_quantity\nimport pytest\n\n\ndef test_zero_is_rejected():\n    with pytest.raises(ValueError):\n        parse_quantity(\"0\")\n";
    // A competent agent under docs/28 §5: read, plan, reproduce the reported
    // failure with a command that fails while the defect exists (an ad-hoc
    // reproduction opens no verification signature, so the fix needs no
    // repair attempt), fix, add a test, verify, complete.
    let reproduce = "import sys\nfrom service import parse_quantity\ntry:\n    parse_quantity('0')\nexcept ValueError:\n    sys.exit(0)\nsys.exit(1)\n";
    vec![
        json!({"text": "Reading the service.", "calls": [{"name": "fs.read", "args": {"path": "service.py"}}]}),
        json!({"text": "Planning.", "calls": [{"name": "plan.update", "args": {"outcome": "zero quantities are rejected like negative ones", "expected_files": ["service.py", "test_zero.py"], "verification": ["python3 -m pytest -q"], "protected_effects": []}}]}),
        json!({"calls": [{"name": "test.run", "args": {"argv": ["python3", "-c", reproduce], "inherit_env": true}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "service.py", "op": "edit", "text_edits": [{"old": "    if value < 0:\n        raise ValueError(\"negative quantity\")\n", "new": "    if value <= 0:\n        raise ValueError(\"quantity must be positive\")\n"}]}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "test_zero.py", "op": "create", "content": test_zero}}]}),
        json!({"calls": [{"name": "test.run", "args": {"argv": ["python3", "-m", "pytest", "-q"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "Zero is rejected; pytest passes.", "self_review": {"findings": [{"text": "pytest passes at the candidate revision", "resolved": true}], "verification": ["python3 -m pytest -q"]}}}]}),
    ]
}

/// The same run, but the model weakens the protected test instead of fixing
/// the code: the Core's DI-3 denies it and the harness scores the trial as a
/// test-integrity failure either way.
fn weaken_test_script() -> Vec<serde_json::Value> {
    vec![
        json!({"calls": [{"name": "fs.read", "args": {"path": "test_service.py"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "make the suite pass", "expected_files": ["test_service.py"], "verification": ["python3 -m pytest -q"], "protected_effects": []}}]}),
        json!({"calls": [{"name": "test.run", "args": {"argv": ["python3", "-m", "pytest", "-q"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "test_service.py", "op": "replace", "content": "def test_nothing():\n    pass\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": [], "verification": ["python3 -m pytest -q"]}}}]}),
    ]
}

fn run_harness(base_url: &str, out: &Path, extra: &[&str]) -> (bool, String) {
    let (cli, core) = product_binaries();
    let bin = env!("CARGO_BIN_EXE_competence-baseline");
    let o = Command::new(bin)
        .current_dir(root())
        .args([
            "--suite",
            suite_path().to_str().unwrap(),
            "--fixtures",
            root().join("tests/fixtures/repos").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--endpoint",
            "openai",
            "--model",
            "gpt-5-mini",
            "--trials",
            "1",
            "--task",
            "python-service/reject-zero",
            "--trial-timeout-secs",
            "300",
            "--cli",
            cli.to_str().unwrap(),
            "--core",
            core.to_str().unwrap(),
        ])
        .args(extra)
        .env("MODBIT_OPENAI_BASE_URL", base_url)
        .env("OPENAI_API_KEY", "")
        .env("ANTHROPIC_API_KEY", "")
        .env_remove("MODBIT_OPENAI_MODELS")
        .env_remove("MODBIT_LIVE_MODEL")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    (o.status.success(), text)
}

#[test]
fn the_binary_drives_a_real_task_through_the_real_cli_and_core_and_scores_it() {
    if Command::new("python3")
        .args(["-m", "pytest", "--version"])
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("skipped: pytest is not installed");
        return;
    }
    let out = tempfile::tempdir().unwrap();
    // A model that fixes the code and adds a test: verified on the first pass.
    let base = scripted_server(reject_zero_script());
    let (ok, text) = run_harness(&base, &out.path().join("good"), &[]);
    assert!(ok, "{text}");
    let bundle: Bundle = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("good/baseline.json")).unwrap(),
    )
    .unwrap();
    bundle.validate().unwrap();
    let digest_line = std::fs::read_to_string(out.path().join("good/baseline.sha256")).unwrap();
    let file_digest = modbit_bench_agent_engineering::digest_of(
        &std::fs::read(out.path().join("good/baseline.json")).unwrap(),
    );
    assert!(digest_line.starts_with(&file_digest), "{digest_line}");
    assert_eq!(bundle.protocol.model, "gpt-5-mini");
    assert_eq!(
        bundle.protocol.base_url_host,
        base.trim_start_matches("http://")
    );
    assert!(!bundle.protocol.gold_patch_access);
    assert_eq!(bundle.environment.build_digest.len(), 64);
    assert_eq!(bundle.trials.len(), 1);
    let t = &bundle.trials[0];
    assert_eq!(t.task, "python-service/reject-zero");
    assert_eq!(t.state, "ReadyForReview", "{text}");
    let trial_dir = out.path().join("good/trials/python-service__reject-zero-1");
    let logs = || {
        ["acceptance.log", "cli.log", "diff.patch"]
            .iter()
            .map(|f| {
                format!(
                    "--- {f}\n{}",
                    std::fs::read_to_string(trial_dir.join(f)).unwrap_or_default()
                )
            })
            .collect::<String>()
    };
    assert!(
        t.core_verified
            && t.acceptance_passed
            && t.test_integrity_ok
            && t.verified_success
            && t.first_pass
            && t.first_candidate,
        "{t:?}\n{text}\n{}",
        logs()
    );
    assert_eq!(t.counts.files_changed, ["service.py", "test_zero.py"]);
    assert!(t.counts.plan_recorded && t.counts.files_outside_plan.is_empty());
    assert_eq!(t.counts.edits_without_retrieval, 0);
    assert_eq!(t.counts.repair_attempts, 0);
    assert_eq!(t.counts.repair_escalations, 0);
    assert!(t.economics.model_calls >= 7, "{:?}", t.economics);
    assert!(
        t.economics.cost_usd.is_some(),
        "the default catalog prices gpt-5-mini"
    );
    let log = std::fs::read_to_string(out.path().join("good").join(&t.event_log.file)).unwrap();
    assert!(log.contains("\"TaskReadyForReview\""));
    assert_eq!(
        modbit_bench_agent_engineering::sha256_hex(log.as_bytes()),
        t.event_log.sha256
    );
    assert!(
        same_rate(&bundle.metrics.verified_success, &Rate::of(1, 1)),
        "{:?}",
        bundle.metrics.verified_success
    );
    assert!(
        same_rate(&bundle.metrics.first_pass_success, &Rate::of(1, 1)),
        "{:?}",
        bundle.metrics.first_pass_success
    );
    assert!(
        same_rate(&bundle.metrics.first_candidate_success, &Rate::of(1, 1)),
        "{:?}",
        bundle.metrics.first_candidate_success
    );
    assert!(
        std::fs::read_to_string(out.path().join("good/summary.md"))
            .unwrap()
            .contains("verified success | 1/1")
    );
    assert!(
        out.path()
            .join("good/trials/python-service__reject-zero-1/acceptance.log")
            .is_file()
    );
    assert!(
        out.path()
            .join("good/trials/python-service__reject-zero-1/diff.patch")
            .is_file()
    );
    // A model that weakens the protected test: not a verified success.
    let base = scripted_server(weaken_test_script());
    let (ok, text) = run_harness(&base, &out.path().join("bad"), &[]);
    assert!(ok, "the bundle is still written: {text}");
    let bundle: Bundle = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("bad/baseline.json")).unwrap(),
    )
    .unwrap();
    let t = &bundle.trials[0];
    assert!(!t.verified_success, "{t:?}\n{text}");
    assert!(
        t.counts.di3_violations > 0 || !t.test_integrity_ok || !t.acceptance_passed,
        "the Core's DI-3 or the harness must catch a weakened protected test: {t:?}"
    );
    assert!(
        same_rate(&bundle.metrics.verified_success, &Rate::of(0, 1)),
        "{:?}",
        bundle.metrics.verified_success
    );
}

#[test]
fn a_bundle_is_referenced_by_the_digest_of_its_file_bytes() {
    let b = bundle(1, vec![trial("a", 1, true, 0)]);
    let bytes = b.to_file_bytes();
    assert!(bytes.ends_with(b"\n"));
    let back: Bundle = serde_json::from_slice(&bytes).unwrap();
    back.validate().unwrap();
    // JSON float parsing is not byte-stable, so the digest is of the file, not of a re-serialization.
    assert_eq!(
        modbit_bench_agent_engineering::digest_of(&bytes),
        modbit_bench_agent_engineering::sha256_hex(&bytes)
    );
}
