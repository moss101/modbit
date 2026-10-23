//! PX-135 (QUAL-PX-135, REQ-EPR-017) on the real Core: the generated
//! adversarial checks are evidence of the COMPLETION run the one Acceptance
//! Gate weighs, so a candidate they flag is refused completion with a typed
//! reason on the event log, and a correct candidate still reaches review.
//!
//! Real: the `modbit-core` binary and its socket, the SQLite event store, the
//! `modbit-execd` broker, a real git repository and a real `pytest` run (the
//! repository's own configured check), the boundary mutants re-run through
//! the broker, a Core killed and restarted. Stand-in: the model, a scripted
//! OpenAI-compatible server (the repository's standard stand-in).
#![cfg(unix)]

mod px_common;

use px_common::*;
use serde_json::{Value, json};
use sha2::Digest;

const SHOP_OLD: &str =
    "def check_quantity(q):\n    if q < 1:\n        raise ValueError(\"low\")\n    return q\n";
const TEST_SHOP: &str = "import pytest\nfrom shop import check_quantity\n\n\ndef test_acceptance_quantity_upper_bound():\n    with pytest.raises(ValueError):\n        check_quantity(500)\n    assert check_quantity(10) == 10\n";
const TEST_OTHER: &str = "import pytest\nfrom shop import check_quantity\n\n\ndef test_basic_quantity_lower():\n    with pytest.raises(ValueError):\n        check_quantity(0)\n    assert check_quantity(1) == 1\n";
const TEST_OTHER_WEAK: &str = "import pytest\nfrom shop import check_quantity\n\n\ndef test_basic_quantity_lower():\n    assert check_quantity(0) == 0\n    assert check_quantity(1) == 1\n";
const CONFIG: &str = "{\"commands\": [{\"id\": \"pytest\", \"argv\": [\"python3\", \"-m\", \"pytest\", \"-q\", \"-p\", \"no:cacheprovider\"]}]}";

const SHOP_FIXED: &str = "def check_quantity(q):\n    if q < 1:\n        raise ValueError(\"low\")\n    if q > 100:\n        raise ValueError(\"high\")\n    return q\n";
const SHOP_OVERFIT: &str = "def check_quantity(q):\n    if q < 1:\n        raise ValueError(\"low\")\n    if q == 500:\n        raise ValueError(\"high\")\n    return q\n";
const SHOP_FIXED_REGRESSED: &str = "def check_quantity(q):\n    if q < 0:\n        raise ValueError(\"low\")\n    if q > 100:\n        raise ValueError(\"high\")\n    return q\n";
const PINNED: &str = "\n\ndef test_boundaries_pinned():\n    assert check_quantity(100) == 100\n    assert check_quantity(99) == 99\n    with pytest.raises(ValueError):\n        check_quantity(101)\n";

fn sha(s: &str) -> String {
    hex::encode(sha2::Sha256::digest(s.as_bytes()))
}

fn pytest_here() -> bool {
    std::process::Command::new("python3")
        .args(["-m", "pytest", "--version"])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn repo() -> (tempfile::TempDir, String) {
    plain_repo(&[
        ("shop.py", SHOP_OLD),
        ("test_shop.py", TEST_SHOP),
        ("test_other.py", TEST_OTHER),
        (".modbit/verification.json", CONFIG),
    ])
}

fn script(edits: &[(&str, &str, &str)], expected: &[&str]) -> Vec<Value> {
    let mut s = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "reject quantities over 100", "expected_files": expected}}]}),
    ];
    for (path, old, new) in edits {
        s.push(json!({"calls": [{"name": "fs.read", "args": {"path": path}}]}));
        s.push(json!({"calls": [{"name": "change.apply", "args": {"path": path, "op": "replace", "content": new, "expected_content_hash": sha(old)}}]}));
    }
    s.push(json!({"calls": [{"name": "task.complete", "args": {"summary": "quantities over 100 are rejected", "self_review": {"findings": []}}}]}));
    s
}

struct Run {
    session: modbit_protocol::v1::Id,
    task: modbit_protocol::v1::Id,
}

async fn run_task(core: &CoreProcess, n: u8, root: &str) -> (Run, modbit_protocol::v1::TaskStatus) {
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, n).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        root,
        n.wrapping_add(1),
        "local_trusted",
        "reject quantities over 100",
    )
    .await;
    start_task(&mut c, &task, g, n.wrapping_add(2), "gpt-5").await;
    let st = wait_task(&mut c, &task, 120).await;
    (Run { session, task }, st)
}

fn events_of(all: &[Value], ty: &str) -> Vec<Value> {
    all.iter()
        .filter(|e| e["event_type"] == ty)
        .map(|e| e["payload"].get("payload").unwrap_or(&e["payload"]).clone())
        .collect()
}

fn check<'a>(run: &'a Value, id: &str) -> Option<&'a Value> {
    run["checks"]
        .as_array()?
        .iter()
        .find(|c| c["check_id"] == id)
}

/// One Core per scripted model: the model address is fixed at spawn.
async fn with_core<F, Fut>(script: Vec<Value>, f: F)
where
    F: FnOnce(CoreProcess, tempfile::TempDir, String) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (base, _seen) = scripted_model(script, vec![]).await;
    let dir = tempfile::tempdir().unwrap();
    let env = model_env(&base);
    let env_ref: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &env_ref);
    f(core, dir, base).await;
}

/// An overfit candidate: the visible test passes, the checks flag it, the
/// Core refuses completion, and a killed and restarted Core holds the same
/// verdict and typed reason.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn qual_px_135_an_overfit_candidate_is_refused_completion_and_survives_a_restart() {
    if !pytest_here() {
        eprintln!("python3 with pytest is not installed: this slice is gated, not faked");
        return;
    }
    let (_repo_a, root_a) = repo();
    let script_a = script(&[("shop.py", SHOP_OLD, SHOP_OVERFIT)], &["shop.py"]);
    with_core(script_a, |core, dir, base| async move {
        let (run, st) = run_task(&core, 0x40, &root_a).await;
        assert_ne!(st.state, "ReadyForReview", "{st:?}");
        let evs = replay(&core, &run.session).await;
        let runs = events_of(&evs, "VerificationRunRecorded");
        let completion = runs
            .iter()
            .find(|r| r["stage"] == "COMPLETION")
            .expect("a completion run");
        // The visible check itself passed: the previous gate would have accepted.
        let overfit = check(completion, "adv:overfit-literal")
            .expect("the generated check is on the run record");
        assert_eq!(overfit["status"], "FAIL", "{overfit}");
        assert_eq!(overfit["kind"], "adversarial");
        assert_eq!(overfit["error_class"], "ADVERSARIAL_OVERFIT_LITERAL");
        let gates = events_of(&evs, "AcceptanceGateEvaluated");
        let gate = gates.last().expect("a gate verdict");
        assert_eq!(gate["verdict"], "REJECT", "{gate}");
        assert!(
            gate["reject_reasons"]
                .to_string()
                .contains("adv:overfit-literal"),
            "{gate}"
        );
        assert!(
            !evs.iter()
                .any(|e| e["event_type"] == "TaskReadyForReview"
                    || e["event_type"] == "TaskCompleted")
        );
        // The findings are retained in full under the run's report refs.
        assert!(
            completion["report_refs"]
                .as_array()
                .is_some_and(|r| r.len() >= 2)
        );

        // Restart injection: the Core is killed; the verdict and the typed
        // reason are what the log holds, and nothing completed.
        let mut core = core;
        core.kill();
        let env = model_env(&base);
        let env_ref: Vec<(&str, &str)> =
            env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let core = CoreProcess::spawn_with_env(dir.path(), &env_ref);
        let again = replay(&core, &run.session).await;
        let runs = events_of(&again, "VerificationRunRecorded");
        let completion = runs.iter().find(|r| r["stage"] == "COMPLETION").unwrap();
        assert_eq!(
            check(completion, "adv:overfit-literal").unwrap()["status"],
            "FAIL"
        );
        let mut c = core.client().await;
        let st = px_common::wait_task(&mut c, &run.task, 5).await;
        assert_ne!(st.state, "ReadyForReview", "{st:?}");
        assert!(!again.iter().any(|e| e["event_type"] == "TaskCompleted"));
    })
    .await;
}

/// A fix that regresses a neighbour and edits that neighbour's test.
#[tokio::test]
async fn qual_px_135_a_weakened_test_is_refused_completion() {
    if !pytest_here() {
        eprintln!("python3 with pytest is not installed: this slice is gated, not faked");
        return;
    }
    let (_repo_b, root_b) = repo();
    let script_b = script(
        &[
            ("shop.py", SHOP_OLD, SHOP_FIXED_REGRESSED),
            ("test_shop.py", TEST_SHOP, &format!("{TEST_SHOP}{PINNED}")),
            ("test_other.py", TEST_OTHER, TEST_OTHER_WEAK),
        ],
        &["shop.py", "test_shop.py", "test_other.py"],
    );
    with_core(script_b, |core, dir, _base| async move {
        // The data directory lives as long as the Core uses it.
        let _keep = &dir;
        let (run, st) = run_task(&core, 0x50, &root_b).await;
        assert_ne!(st.state, "ReadyForReview", "{st:?}");
        let evs = replay(&core, &run.session).await;
        let runs = events_of(&evs, "VerificationRunRecorded");
        let completion = runs
            .iter()
            .find(|r| r["stage"] == "COMPLETION")
            .expect("a completion run");
        let weak = check(completion, "adv:test-weakening").expect("on the record");
        assert_eq!(weak["status"], "FAIL", "{weak}");
        assert_eq!(weak["error_class"], "ADVERSARIAL_TEST_WEAKENING");
        let gate = events_of(&evs, "AcceptanceGateEvaluated").pop().unwrap();
        assert_eq!(gate["verdict"], "REJECT", "{gate}");
    })
    .await;
}

/// The correct candidate, with tests that pin its boundary, still reaches
/// review with every generated check passing.
#[tokio::test]
async fn qual_px_135_a_correct_candidate_still_reaches_review() {
    if !pytest_here() {
        eprintln!("python3 with pytest is not installed: this slice is gated, not faked");
        return;
    }
    let (_repo_c, root_c) = repo();
    let script_c = script(
        &[
            ("shop.py", SHOP_OLD, SHOP_FIXED),
            ("test_shop.py", TEST_SHOP, &format!("{TEST_SHOP}{PINNED}")),
        ],
        &["shop.py", "test_shop.py"],
    );
    with_core(script_c, |core, dir, _base| async move {
        // The data directory lives as long as the Core uses it.
        let _keep = &dir;
        let (run, st) = run_task(&core, 0x60, &root_c).await;
        assert_eq!(st.state, "ReadyForReview", "{st:?}");
        let evs = replay(&core, &run.session).await;
        let runs = events_of(&evs, "VerificationRunRecorded");
        let completion = runs
            .iter()
            .find(|r| r["stage"] == "COMPLETION")
            .expect("a completion run");
        for id in [
            "adv:test-weakening",
            "adv:vacuous-pass",
            "adv:overfit-literal",
            "adv:boundary-pinned",
        ] {
            assert_eq!(
                check(completion, id).unwrap_or_else(|| panic!("{id}"))["status"],
                "PASS",
                "{id}"
            );
        }
        let gate = events_of(&evs, "AcceptanceGateEvaluated").pop().unwrap();
        assert_eq!(gate["verdict"], "ACCEPT", "{gate}");
    })
    .await;
}
