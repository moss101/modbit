//! QUAL-PX-214 (DR-PX-2026-11-11-021 MWS-18) on the real Core: the actual
//! `modbit-core` binary over its real socket and its real SQLite event store,
//! with a real isolated review (the EPR-018 environment, the review task, the
//! `review.report` tool and its validation against the tree). The model is the
//! repository's scripted OpenAI-compatible stand-in; everything the Core does
//! with its answers is real.
//!
//! * The reviewer returns three findings. The bundle lists each with its
//!   severity, path, line, claim, suggested action and whether the Core's own
//!   check against the tree resolved it; the finding about a file that is not
//!   there is not shown as verified.
//! * `FixReviewFinding` queues exactly one input, through the task's input
//!   queue, that names that finding and no other. An unknown finding, a result
//!   the person was not looking at, and a dismissed finding are refused.
//! * `DismissReviewFinding` records who and why on the candidate's log; it needs
//!   a reason; a second dismissal is refused; a repeated command id is replayed.
//! * The next review's brief lists the findings still open (not the dismissed
//!   one) to re-evaluate.
//! * While the next review runs, the old result is marked STALE; a result whose
//!   object no longer hashes to the digest the log names shows nothing.

#![cfg(unix)]

mod px_common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    ActivateModelRegistry, DismissReviewFinding, FixReviewFinding, GetReviewBundle, GetTaskStatus,
    Id, ModelRegistryView, ReviewBundle, ReviewFindingDismissal, ReviewFindingFixQueued, StartTask,
    TaskRunStarted, TaskStatus,
};
use modbit_providers::registry::{
    Economics, Governance, Latency, QualityFloor, REGISTRY_SCHEMA_VERSION, RegistryDocument,
    RegistryEntry, SignedRegistry,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};
use sha2::Digest;

const LOGIN: &str = "def login(u, p):\n    return u == 'a'\n";

fn sha256_of(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

/// A registry with a solver and a reviewer on different models, signed by a
/// key the Core is told to trust.
fn signed_registry(key: &SigningKey) -> String {
    let now = modbit_domain::Timestamp::now().0;
    let entry = |model: &str, roles: &[&str], input: u64, output: u64| RegistryEntry {
        endpoint: "openai".into(),
        provider: "openai".into(),
        family: "gpt-5".into(),
        model: model.into(),
        roles: roles.iter().map(|r| (*r).to_owned()).collect(),
        input_modalities: vec!["text".into()],
        context_tokens: 400_000,
        max_output_tokens: 64_000,
        tools: true,
        vision: false,
        reasoning: true,
        structured_output: true,
        economics: Economics {
            input_per_mtok_minor: input,
            output_per_mtok_minor: output,
            currency: "USD".into(),
            scale: 2,
            cached_input_per_mtok_minor: None,
            cache_write_per_mtok_minor: None,
            cache_ttl_ms: None,
        },
        latency: Latency {
            p50_ms: 900,
            p95_ms: 4_200,
        },
        governance: Governance {
            data_residency: "us".into(),
            retains_prompts: false,
            allowed_profiles: vec![],
        },
        revoked: false,
        fallbacks: vec![],
    };
    let document = RegistryDocument {
        promotion: None,
        schema_version: REGISTRY_SCHEMA_VERSION,
        registry_generation: "registry-px214".into(),
        stats_version: "stats-1".into(),
        issued_at_ms: now - 60_000,
        expires_at_ms: now + 86_400_000,
        quality_floors: vec![QualityFloor {
            mode: "auto".into(),
            min_quality: 0.72,
            max_cost_minor: 5_000,
            currency: "USD".into(),
            scale: 2,
        }],
        entries: vec![
            entry("gpt-5-mini", &["solver"], 25, 200),
            entry("gpt-5", &["solver", "reviewer"], 125, 1_000),
        ],
    };
    let json = serde_json::to_string(&document).unwrap();
    serde_json::to_string(&SignedRegistry {
        key_id: "ops".into(),
        signature_hex: hex::encode(key.sign(json.as_bytes()).to_bytes()),
        document_json: json,
    })
    .unwrap()
}

/// The solver: a risky change (an auth edit and a migration) so that the
/// acceptance gate requires independent review; then, once the person's fix
/// has been queued and the task resumed, the revision.
fn solver_script() -> Vec<Value> {
    let hash = sha256_of(LOGIN.as_bytes());
    vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "validate the login and add the migration", "expected_files": ["src/auth/login.py", "db/migrations/002_add.sql"], "verification": ["sh check.sh"]}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "src/auth/login.py"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "src/auth/login.py", "op": "replace", "content": "def login(u, p):\n    # validated\n    return u == 'a' and p\n", "expected_content_hash": hash}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "db/migrations/002_add.sql", "op": "create", "content": "alter table t add column name text;\n"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "validate the login and add the migration", "expected_files": ["src/auth/login.py", "db/migrations/002_add.sql"], "verification": ["sh check.sh"], "reason": "the migration is authored by hand: there is no generator in this repository"}}]}),
        json!({"calls": [{"name": "test.run", "args": {"argv": ["sh", "check.sh"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "validated login, migration added", "self_review": {"findings": []}, "verification": ["sh check.sh"]}}]}),
        // After the fix is queued and the task resumed.
        json!({"calls": [{"name": "change.apply", "args": {"path": "src/auth/login.py", "op": "replace", "content": "import hashlib\n# FIXED-HASH\ndef login(u, p):\n    # validated\n    return u == 'a' and hashlib.sha256(p.encode()).hexdigest() == STORED\n"}}]}),
        json!({"calls": [{"name": "test.run", "args": {"argv": ["sh", "check.sh"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "the password is compared against a stored hash", "self_review": {"findings": []}, "verification": ["sh check.sh"]}}]}),
    ]
}

/// The first review's report: three findings. One is about a file that is not
/// in the tree, one has no id (the Core names it by position).
fn first_report() -> Value {
    json!({"calls": [{"name": "review.report", "args": {
        "verdict": "BLOCK", "confidence": 77,
        "summary": "the password is only truthiness-checked",
        "findings": [
            {"id": "f1", "severity": "HIGH", "category": "security", "path": "src/auth/login.py", "line": 3,
             "claim": "`and p` only checks the password is non-empty", "evidence_refs": ["src/auth/login.py:3"],
             "suggested_action": "compare against a stored hash"},
            {"id": "f2", "severity": "LOW", "category": "style", "path": "src/auth/phantom.py", "line": 1,
             "claim": "unused import of phantom helpers", "evidence_refs": [],
             "suggested_action": "remove the import"},
            {"severity": "MEDIUM", "category": "regression", "path": "db/migrations/002_add.sql", "line": 1,
             "claim": "the migration has no down step", "evidence_refs": [],
             "suggested_action": "add the reverse migration"}
        ],
        "unresolved_questions": ["where is the stored hash kept?"]}}]})
}

/// The scripted stand-in for both models. The reviewer (`gpt-5`) is told from
/// the solver by its model name; its first review reports three findings, a
/// later one (its brief carries the earlier open findings) first waits for the
/// test to release it and then passes.
async fn model(release_second_review: PathBuf) -> (String, Seen) {
    let solver = solver_script();
    scripted_model_fn(std::sync::Arc::new(move |body, results| {
        let text = serde_json::to_string(&body["messages"]).unwrap_or_default();
        if body["model"] == "gpt-5" && text.contains("[REVIEW BRIEF]") {
            let later = text.contains("Earlier findings still open");
            return match (later, results) {
                (false, 0) => {
                    json!({"calls": [{"name": "fs.read", "args": {"path": "src/auth/login.py"}}]})
                }
                (false, _) => first_report(),
                (true, 0) => json!({
                    "calls": [{"name": "fs.read", "args": {"path": "src/auth/login.py"}}],
                    "wait_file": release_second_review.to_string_lossy(),
                }),
                (true, _) => json!({"calls": [{"name": "review.report", "args": {
                    "verdict": "PASS", "confidence": 90,
                    "summary": "the password is compared against a stored hash",
                    "findings": [], "unresolved_questions": []}}]}),
            };
        }
        solver
            .get(script_steps_done(body))
            .cloned()
            .unwrap_or_else(|| json!({"text": "I have nothing further to do."}))
    }))
    .await
}

async fn cmd<R: Message + Default>(
    c: &mut Client,
    command_type: &str,
    msg: impl Message,
    g: Option<u64>,
) -> Result<R, (String, String)> {
    cmd_id(c, rand_id(), command_type, msg, g).await
}

async fn cmd_id<R: Message + Default>(
    c: &mut Client,
    id: Id,
    command_type: &str,
    msg: impl Message,
    g: Option<u64>,
) -> Result<R, (String, String)> {
    match c
        .command(envelope_fenced(id, command_type, msg.encode_to_vec(), g))
        .await
    {
        Ok(ack) => Ok(Client::result(&ack).unwrap()),
        Err(ClientError::Rejected { code, message }) => Err((code, message)),
        Err(e) => panic!("{command_type}: {e}"),
    }
}

async fn bundle(c: &mut Client, task: &Id) -> ReviewBundle {
    cmd(
        c,
        "GetReviewBundle",
        GetReviewBundle {
            task_id: Some(task.clone()),
        },
        None,
    )
    .await
    .unwrap()
}

async fn status(c: &mut Client, task: &Id) -> TaskStatus {
    let ack = c
        .command(envelope(
            rand_id(),
            "GetTaskStatus",
            GetTaskStatus {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

async fn fix(
    c: &mut Client,
    task: &Id,
    finding: &str,
    digest: &str,
    g: Option<u64>,
) -> Result<ReviewFindingFixQueued, (String, String)> {
    cmd(
        c,
        "FixReviewFinding",
        FixReviewFinding {
            task_id: Some(task.clone()),
            finding_id: finding.into(),
            result_digest: digest.into(),
        },
        g,
    )
    .await
}

async fn dismiss(
    c: &mut Client,
    id: Id,
    task: &Id,
    finding: &str,
    digest: &str,
    reason: &str,
    g: Option<u64>,
) -> Result<ReviewFindingDismissal, (String, String)> {
    cmd_id(
        c,
        id,
        "DismissReviewFinding",
        DismissReviewFinding {
            task_id: Some(task.clone()),
            finding_id: finding.into(),
            result_digest: digest.into(),
            reason: reason.into(),
        },
        g,
    )
    .await
}

fn of(evs: &[Value], ty: &str) -> Vec<Value> {
    evs.iter()
        .filter(|e| e["event_type"] == ty)
        .map(|e| e["payload"]["payload"].clone())
        .collect()
}

/// A path under `root` whose name is `name`.
fn find_object(root: &Path, hash: &str) -> PathBuf {
    let want = (&hash[..2], &hash[2..]);
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == want.0) && p.join(want.1).is_file() {
                    return p.join(want.1);
                }
                stack.push(p);
            }
        }
    }
    panic!("no object {hash} under {}", root.display());
}

/// What the stand-in's reviewer was sent, oldest first, as one text each:
/// the brief opens every request of a review, so the first of them is the
/// brief of the first review.
fn briefs(seen: &Seen) -> Vec<String> {
    seen.lock()
        .unwrap()
        .iter()
        .filter(|b| b["model"] == "gpt-5")
        .map(|b| serde_json::to_string(&b["messages"]).unwrap())
        .filter(|t| t.contains("[REVIEW BRIEF]"))
        .collect()
}

#[tokio::test]
async fn px_214_the_reviewers_findings_one_by_one_with_fix_and_dismiss() {
    let key = SigningKey::from_bytes(&[41u8; 32]);
    let keys = format!("ops:{}", hex::encode(key.verifying_key().to_bytes()));
    let files: [(&str, &str); 5] = [
        ("src/auth/login.py", LOGIN),
        ("db/migrations/001_init.sql", "create table t (id int);\n"),
        (
            "tests/test_login.py",
            "def test_login():\n    assert True\n",
        ),
        ("check.sh", "grep -q 'validated' src/auth/login.py\n"),
        (
            ".modbit/verification.json",
            "{\"commands\": [{\"id\": \"gate\", \"argv\": [\"sh\", \"check.sh\"]}]}",
        ),
    ];
    let (_repo, root) = plain_repo(&files);
    let hold = tempfile::tempdir().unwrap();
    let release = hold.path().join("release-second-review");
    let (base, seen) = model(release.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let env: Vec<(&str, &str)> = vec![
        ("MODBIT_OPENAI_BASE_URL", base.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
        ("MODBIT_REGISTRY_KEYS", keys.as_str()),
    ];
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let view: ModelRegistryView = cmd(
        &mut c,
        "ActivateModelRegistry",
        ActivateModelRegistry {
            signed_json: signed_registry(&key),
            expected_generation: String::new(),
        },
        None,
    )
    .await
    .unwrap();
    assert!(view.active, "{view:?}");
    let (session, g) = session_with_lease(&mut c, 0x71).await;
    let task = create_task(&mut c, &session, g, &root, 0x72, "local_trusted", "x").await;
    let start = StartTask {
        task_id: Some(task.clone()),
        endpoint: String::new(),
        model: String::new(),
        max_turns: 20,
        max_tool_calls: 0,
        max_no_progress_turns: 4,
        skills: vec![],
        ..Default::default()
    };
    let _: TaskRunStarted = cmd(&mut c, "StartTask", start.clone(), g).await.unwrap();

    // The task has no reviewer result until the reviewer has reported.
    // ---- The reviewer reports; the review ends; a person decides. --------
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let evs = loop {
        let evs = replay(&core, &session).await;
        if !of(&evs, "ReviewerResultRecorded").is_empty()
            && !of(&evs, "TaskNeedsAttention").is_empty()
        {
            break evs;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no review result: {} events, {:?}",
            evs.len(),
            status(&mut c, &task).await
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    };
    let recorded = of(&evs, "ReviewerResultRecorded");
    assert_eq!(recorded.len(), 1, "{recorded:#?}");
    let digest = recorded[0]["result_ref"].as_str().unwrap().to_owned();
    assert_eq!(recorded[0]["verdict"], "BLOCK");
    assert_eq!(recorded[0]["validated_findings"], 2);
    assert_eq!(recorded[0]["unsupported_findings"], 1);
    assert_eq!(status(&mut c, &task).await.state, "ReadyForReview");

    // ---- 1. The bundle lists each finding, with what the Core checked. ----
    let b = bundle(&mut c, &task).await;
    assert_eq!(b.reviewer_state, "CURRENT", "{b:?}");
    assert_eq!(b.reviewer_verdict, "BLOCK");
    assert_eq!(b.reviewer_confidence, 77);
    assert_eq!(
        b.reviewer_summary,
        "the password is only truthiness-checked"
    );
    assert_eq!(b.reviewer_result_digest, digest);
    assert_eq!(b.reviewer_questions, vec!["where is the stored hash kept?"]);
    assert_eq!(
        b.reviewer_task_id.as_ref().map(hex_id).unwrap().len(),
        32,
        "the review task that reported"
    );
    let ids: Vec<&str> = b
        .reviewer_findings
        .iter()
        .map(|f| f.finding_id.as_str())
        .collect();
    assert_eq!(
        ids,
        ["f1", "f2", "F3"],
        "the reviewer's id, else its position"
    );
    let (f1, f2, f3) = (
        &b.reviewer_findings[0],
        &b.reviewer_findings[1],
        &b.reviewer_findings[2],
    );
    assert_eq!(
        (
            f1.severity.as_str(),
            f1.category.as_str(),
            f1.path.as_str(),
            f1.line
        ),
        ("HIGH", "security", "src/auth/login.py", 3)
    );
    assert_eq!(f1.claim, "`and p` only checks the password is non-empty");
    assert_eq!(f1.suggested_action, "compare against a stored hash");
    assert!(f1.verified && f1.unsupported_reason.is_empty(), "{f1:?}");
    // The finding about a file that is not in the tree is not verified, and
    // says why.
    assert!(!f2.verified, "{f2:?}");
    assert!(
        f2.unsupported_reason.contains("src/auth/phantom.py"),
        "{f2:?}"
    );
    assert!(f3.verified, "{f3:?}");
    assert_eq!(f3.path, "db/migrations/002_add.sql");
    assert!(
        b.reviewer_findings
            .iter()
            .all(|f| !f.dismissed && f.fix_requests == 0)
    );

    // ---- 2. Refusals that write nothing. ----------------------------------
    let before = of(&replay(&core, &session).await, "TaskInputQueued").len();
    let unknown = fix(&mut c, &task, "nope", &digest, g).await.unwrap_err();
    assert_eq!(unknown.0, "UNKNOWN_FINDING", "{unknown:?}");
    let other = fix(&mut c, &task, "f1", &"0".repeat(64), g)
        .await
        .unwrap_err();
    assert_eq!(other.0, "RESULT_CHANGED", "{other:?}");
    let none = fix(&mut c, &task, "f1", "", g).await.unwrap_err();
    assert_eq!(none.0, "BAD_PAYLOAD", "{none:?}");
    let unfenced = fix(&mut c, &task, "f1", &digest, None).await.unwrap_err();
    assert!(
        unfenced.0.contains("LEASE"),
        "a fix without the session's lease is refused: {unfenced:?}"
    );
    let no_reason = dismiss(&mut c, rand_id(), &task, "F3", &digest, "   ", g)
        .await
        .unwrap_err();
    assert_eq!(
        no_reason.0, "BAD_PAYLOAD",
        "a dismissal records why: {no_reason:?}"
    );
    let long = dismiss(&mut c, rand_id(), &task, "F3", &digest, &"x".repeat(501), g)
        .await
        .unwrap_err();
    assert_eq!(long.0, "BAD_PAYLOAD", "{long:?}");
    let missing = dismiss(
        &mut c,
        rand_id(),
        &task,
        "nope",
        &digest,
        "no such finding",
        g,
    )
    .await
    .unwrap_err();
    assert_eq!(missing.0, "UNKNOWN_FINDING", "{missing:?}");
    let evs = replay(&core, &session).await;
    assert_eq!(
        of(&evs, "TaskInputQueued").len(),
        before,
        "nothing was queued"
    );
    assert!(of(&evs, "ReviewFindingDismissed").is_empty());

    // ---- 3. Dismiss one: who and why, on the candidate's log. -------------
    let command = rand_id();
    let reason = "that migration is reversible by restoring the snapshot";
    let d = dismiss(&mut c, command.clone(), &task, "F3", &digest, reason, g)
        .await
        .unwrap();
    assert_eq!(d.finding_id, "F3");
    assert_eq!(d.reason, reason);
    assert!(d.dismissed_by.starts_with("user:"), "{d:?}");
    let evs = replay(&core, &session).await;
    let dismissals: Vec<&Value> = evs
        .iter()
        .filter(|e| e["event_type"] == "ReviewFindingDismissed")
        .collect();
    assert_eq!(dismissals.len(), 1, "{dismissals:#?}");
    let e = dismissals[0];
    assert_eq!(
        e["task_id"].as_str().unwrap(),
        hex_id(&task),
        "on the candidate's log"
    );
    assert_eq!(e["aggregate_type"], "task");
    assert_eq!(e["payload"]["payload"]["result_ref"], digest.as_str());
    assert_eq!(e["payload"]["payload"]["finding_id"], "F3");
    assert_eq!(e["payload"]["payload"]["reason"], reason);
    assert_eq!(
        e["payload"]["payload"]["dismissed_by"],
        d.dismissed_by.as_str()
    );
    assert_eq!(e["offset"].as_u64().unwrap(), d.offset);
    // A repeated command id is replayed, not recorded twice.
    let again = dismiss(&mut c, command, &task, "F3", &digest, reason, g)
        .await
        .unwrap();
    assert_eq!(again.offset, d.offset);
    assert_eq!(
        of(&replay(&core, &session).await, "ReviewFindingDismissed").len(),
        1
    );
    // A second dismissal of the same finding is refused, and so is a fix.
    let twice = dismiss(&mut c, rand_id(), &task, "F3", &digest, "again", g)
        .await
        .unwrap_err();
    assert_eq!(twice.0, "ALREADY_DISMISSED", "{twice:?}");
    let fix_dismissed = fix(&mut c, &task, "F3", &digest, g).await.unwrap_err();
    assert_eq!(fix_dismissed.0, "FINDING_DISMISSED", "{fix_dismissed:?}");
    let b = bundle(&mut c, &task).await;
    let f3 = &b.reviewer_findings[2];
    assert!(f3.dismissed, "{f3:?}");
    assert_eq!(f3.dismissed_by, d.dismissed_by);
    assert_eq!(f3.dismissed_reason, reason);
    assert_eq!(f3.dismissed_offset, d.offset);
    assert!(!b.reviewer_findings[0].dismissed && !b.reviewer_findings[1].dismissed);

    // ---- 4. A result whose object was altered shows nothing. ---------------
    let object = find_object(dir.path(), &digest);
    let original = std::fs::read(&object).unwrap();
    std::fs::write(&object, [original.as_slice(), b" "].concat()).unwrap();
    let b = bundle(&mut c, &task).await;
    assert_eq!(b.reviewer_state, "UNREADABLE", "{b:?}");
    assert!(b.reviewer_findings.is_empty(), "{b:?}");
    assert!(b.reviewer_verdict.is_empty() && b.reviewer_summary.is_empty());
    assert_eq!(b.reviewer_result_digest, digest, "the digest the log names");
    assert!(!b.reviewer_note.is_empty());
    let unreadable = fix(&mut c, &task, "f1", &digest, g).await.unwrap_err();
    assert_eq!(unreadable.0, "RESULT_UNREADABLE", "{unreadable:?}");
    std::fs::remove_file(&object).unwrap();
    assert_eq!(
        bundle(&mut c, &task).await.reviewer_state,
        "UNREADABLE",
        "a missing object too"
    );
    std::fs::write(&object, &original).unwrap();
    assert_eq!(bundle(&mut c, &task).await.reviewer_findings.len(), 3);

    // ---- 5. Fix one: one input, naming that finding only. ------------------
    let queued = fix(&mut c, &task, "f1", &digest, g).await.unwrap();
    assert_eq!(queued.finding_id, "f1");
    assert!(queued.task_state.starts_with("Waiting"), "{queued:?}");
    let evs = replay(&core, &session).await;
    let inputs: Vec<Value> = of(&evs, "TaskInputQueued")
        .into_iter()
        .filter(|i| i["provenance"] == "review_finding")
        .collect();
    assert_eq!(inputs.len(), 1, "exactly one input: {inputs:#?}");
    let text = inputs[0]["text"].as_str().unwrap();
    assert_eq!(inputs[0]["input_id"], queued.input_id.as_str());
    assert_eq!(inputs[0]["mode"], "FOLLOW_UP");
    assert!(
        text.contains("Finding f1 [HIGH] security at src/auth/login.py:3"),
        "{text}"
    );
    assert!(
        text.contains("`and p` only checks the password is non-empty"),
        "{text}"
    );
    assert!(text.contains("compare against a stored hash"), "{text}");
    assert!(text.contains("only this one"), "{text}");
    for other in ["phantom", "no down step", "reversible"] {
        assert!(
            !text.contains(other),
            "the input names no other finding: {text}"
        );
    }
    assert_eq!(
        of(&evs, "TaskReturnedToWork").len(),
        1,
        "the candidate goes back to work"
    );
    let b = bundle(&mut c, &task).await;
    assert_eq!(b.reviewer_findings[0].fix_requests, 1);
    assert_eq!(b.reviewer_findings[1].fix_requests, 0);

    // ---- 6. Resume: the fix is made and the candidate is reviewed again. ---
    let _: TaskRunStarted = cmd(&mut c, "StartTask", start, g).await.unwrap();
    // While the next review runs, the old result is marked stale.
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    loop {
        let b = bundle(&mut c, &task).await;
        if b.reviewer_state == "STALE" {
            assert_eq!(b.reviewer_result_digest, digest);
            assert_eq!(
                b.reviewer_findings.len(),
                3,
                "the old findings are still shown"
            );
            assert!(b.reviewer_note.contains("changed since"), "{b:?}");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "never marked stale: {:?} {:?}",
            b.reviewer_state,
            status(&mut c, &task).await
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    // The second review's brief names the open findings and not the dismissed.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let second = loop {
        let b = briefs(&seen);
        if let Some(s) = b.iter().find(|t| t.contains("Earlier findings still open")) {
            break s.clone();
        }
        assert!(std::time::Instant::now() < deadline, "no second brief");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert!(second.contains("[f1] HIGH security"), "{second}");
    assert!(second.contains("[f2] LOW style"), "{second}");
    assert!(
        second.contains("did not resolve against the tree"),
        "{second}"
    );
    assert!(
        !second.contains("no down step") && !second.contains("[F3]"),
        "the dismissed finding is not re-evaluated: {second}"
    );
    let first = briefs(&seen).into_iter().next().unwrap();
    assert!(
        !first.contains("Earlier findings still open"),
        "the first review had none"
    );
    // The solver received the fix input on resuming.
    let solver_saw_fix = seen.lock().unwrap().iter().any(|b| {
        let text = serde_json::to_string(&b["messages"]).unwrap();
        !text.contains("[REVIEW BRIEF]") && text.contains("Finding f1 [HIGH]")
    });
    assert!(solver_saw_fix, "the agent was given the one finding");
    std::fs::write(&release, b"go").unwrap();

    // ---- 7. The second review passes: its result replaces the first. -------
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let b = loop {
        let b = bundle(&mut c, &task).await;
        if b.reviewer_verdict == "PASS" {
            break b;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no second result: {:?} {:?}",
            b.reviewer_state,
            status(&mut c, &task).await
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    };
    assert_eq!(b.reviewer_state, "CURRENT");
    assert!(b.reviewer_findings.is_empty());
    assert_ne!(b.reviewer_result_digest, digest);
    // A command about the first result's finding is refused: it is not the
    // latest result.
    let stale = fix(&mut c, &task, "f1", &digest, g).await.unwrap_err();
    assert_eq!(stale.0, "RESULT_CHANGED", "{stale:?}");
}
