//! PX-128 (QUAL-PX-128, docs/21 "Handoff local → cloud", docs/24), the CLI
//! half: the real `modbit-cli` hands a task running on a real local
//! `modbit-core` to the real cloud API over a real Postgres — after a
//! confirmation that lists what leaves the machine — a real worker
//! continues it in a sandbox (the reference backend: isolation is the CI
//! job's MicroVM), the approval the continuation raises is answered from one
//! client and refused to the second, and a lost connection resumes by
//! cursor. A policy that forbids handoff and a secret in the workspace each
//! stop the handoff before a byte leaves.
//!
//! Not proven here: the desktop's Continue in Cloud (not built), `cloud
//! return` (not built: the cloud's delta does not come back), a real
//! staging cloud and a packaged app.
//!
//! Runs only where `MODBIT_CLOUD_TEST_DATABASE_URL` names a database.

mod px_cloud_common;
#[path = "../../../tests/support/scripted_openai.rs"]
mod scripted_openai;

use std::path::Path;
use std::time::Duration;

use ed25519_dalek::SigningKey;
use modbit_cloud_api::{Config as ApiConfig, Extras, serve_with};
use modbit_cloud_worker::start;
use modbit_domain::policy_bundle::{self, BundleDocument, KIND, SCHEMA_VERSION};
use px_cloud_common::*;
use scripted_openai::scripted_openai;
use serde_json::json;

const ADMIN: &str = "px128-platform-admin";

struct Cli<'a> {
    profile: &'a Path,
    env: Vec<(String, String)>,
}

impl Cli<'_> {
    async fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = tokio::process::Command::new(cli_bin())
            .env("MODBIT_CORE_BIN", core_bin())
            .envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .arg("--data-dir")
            .arg(self.profile)
            .args(args)
            .output()
            .await
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    async fn ok(&self, args: &[&str]) -> String {
        let (code, out, err) = self.run(args).await;
        assert_eq!(code, 0, "{args:?}: {out}{err}");
        out
    }
}

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|w| w.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("`{key}=` in {line}"))
}

fn line_with<'a>(out: &'a str, prefix: &str) -> &'a str {
    out.lines()
        .find(|l| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("a `{prefix}` line in:\n{out}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qual_px_128_the_cli_hands_a_local_task_to_the_cloud_after_a_confirmation_and_watches_and_answers_its_approval()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    assert!(
        core_bin().exists() && cli_bin().exists(),
        "modbit-core and modbit-cli at target/debug"
    );
    let keep = tempfile::tempdir().unwrap();
    let data = keep.path().to_path_buf();

    // ---- the cloud: an API on a port we can bring back, a tenant, the organisation's policy ----
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = probe.local_addr().unwrap().to_string();
    drop(probe);
    let api_config = |store: &modbit_event_store::cloud::CloudStoreConfig| ApiConfig {
        store: store.clone(),
        token_key: Some(vec![3u8; 32]),
        bind: bind.clone(),
        rate_capacity: 1000,
        rate_per_second: 200.0,
        worker_key: None,
        github_webhook_secret: None,
    };
    let extras = || Extras::default().with_admin_secret(ADMIN);
    let served = serve_with(api_config(&store_cfg), extras())
        .await
        .expect("api");
    let api = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let (s, t) = api
        .post(ADMIN, "/v1/admin/tenants", json!({"name": "px128"}))
        .await;
    assert_eq!(s, 201, "{t}");
    let tenant = t["tenant_id"].as_str().unwrap().to_owned();
    let (s, p) = api
        .post(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/principals"),
            json!({"label": "ada", "role": "admin"}),
        )
        .await;
    assert_eq!(s, 201, "{p}");
    let secret = p["secret"].as_str().unwrap().to_owned();
    let (_, pair) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    let admin_token = pair["access_token"].as_str().unwrap().to_owned();
    let org = SigningKey::from_bytes(&[88u8; 32]);
    let (s, _) = api
        .call_put(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/org-keys/org-1"),
            json!({"public_key_hex": hex::encode(org.verifying_key().to_bytes())}),
        )
        .await;
    assert_eq!(s, 200);
    // The organisation asks about every file a cloud task reads.
    let now = modbit_domain::Timestamp::now().millis();
    let doc = BundleDocument {
        kind: KIND.into(),
        schema_version: SCHEMA_VERSION,
        tenant_id: tenant.clone(),
        generation: 1,
        issued_at_ms: now,
        expires_at_ms: now + 3_600_000,
        min_protocol_major: 1,
        admin_config: json!({"permissions": {"fs.read": "ASK"}}),
    };
    let (s, r) = api
        .call_put(
            &admin_token,
            "/v1/policy/bundle",
            serde_json::to_value(policy_bundle::sign(&doc, "org-1", &org).unwrap()).unwrap(),
        )
        .await;
    assert_eq!(s, 201, "{r}");

    // ---- the laptop: a local Core (spawned by the CLI) running a task ----
    let root = repo(&data.join("laptop-repo"));
    let (laptop_model, _) = scripted_openai(
        std::iter::once(json!({"calls": [{"name": "plan.update", "args": {"outcome": "summary.md written from the notes", "expected_files": ["summary.md"]}}]}))
            .chain(std::iter::once(json!({"calls": [{"name": "change.apply", "args": {"path": "summary.md", "op": "replace", "content": "handed off from the laptop\n"}}]})))
            .chain((0..40).map(|_| json!({"calls": [{"name": "fs.read", "args": {"path": "NOTES.md"}}]})))
            .collect(),
        Duration::from_millis(1200),
    );
    let profile = data.join("laptop");
    std::fs::create_dir_all(&profile).unwrap();
    let cli = Cli {
        profile: &profile,
        env: vec![
            ("MODBIT_OPENAI_BASE_URL".into(), laptop_model),
            ("OPENAI_API_KEY".into(), String::new()),
            ("ANTHROPIC_API_KEY".into(), String::new()),
            ("MODBIT_CLOUD_NO_BROWSER".into(), "1".into()),
            ("MODBIT_CLOUD_SECRET".into(), secret.clone()),
        ],
    };
    let out = cli.ok(&["session", "create"]).await;
    let sid = out.trim().strip_prefix("session ").unwrap().to_owned();
    let out = cli
        .ok(&[
            "task",
            "create",
            "--session",
            &sid,
            "--workspace",
            &root,
            "summarize the notes into summary.md",
        ])
        .await;
    let tid = out.trim().strip_prefix("task ").unwrap().to_owned();
    cli.ok(&[
        "task",
        "run",
        "--session",
        &sid,
        "--task",
        &tid,
        "--endpoint",
        "openai",
        "--model",
        "gpt-5",
        "--max-turns",
        "60",
    ])
    .await;
    until("the laptop's edit to land", 120, async || {
        std::fs::read_to_string(Path::new(&root).join("summary.md"))
            .ok()
            .filter(|s| s.contains("handed off"))
            .map(|_| ())
    })
    .await;

    // ---- sign in (the secret from the environment, never an argument) ----
    let out = cli.ok(&["cloud", "login", "--api", &api.base]).await;
    assert!(out.contains("signed in") && !out.contains(&secret), "{out}");
    let session_file = profile.join("cloud").join("session.json");
    let saved = std::fs::read_to_string(&session_file).unwrap();
    assert!(
        !saved.contains(&secret),
        "the secret itself is not kept, only the token pair"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&session_file)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "readable by its owner only"
        );
    }
    let status = cli.ok(&["cloud", "status"]).await;
    assert!(
        status.contains("health=ok") && status.contains(&tenant),
        "{status}"
    );
    let nothing_sent = async || {
        // The cloud has no record of the task: its session is not found.
        let (s, _) = api
            .get(
                &admin_token,
                &format!("/v1/sessions/{}", "00000000-0000-0000-0000-000000000000"),
            )
            .await;
        s
    };
    let _ = nothing_sent().await;

    // ---- a policy that forbids handoff: refused with its reason, nothing leaves ----
    std::fs::write(
        profile.join("admin-config.json"),
        r#"{"permissions": {"task.handoff": "DENY"}}"#,
    )
    .unwrap();
    let (code, out, err) = cli
        .run(&["cloud", "handoff", "--session", &sid, "--task", &tid])
        .await;
    assert_ne!(code, 0, "{out}{err}");
    assert!(
        err.contains("HANDOFF_FORBIDDEN") && err.contains("nothing left this machine"),
        "{out}{err}"
    );
    assert!(
        !profile
            .join("cloud")
            .join("bundles")
            .join(&tid)
            .join("events.jsonl")
            .exists(),
        "nothing was written either"
    );
    std::fs::remove_file(profile.join("admin-config.json")).unwrap();

    // ---- a secret in the workspace: refused, naming the file, never the value ----
    let planted = "GITHUB_TOKEN=ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    std::fs::write(Path::new(&root).join(".env"), planted).unwrap();
    let (code, out, err) = cli
        .run(&["cloud", "handoff", "--session", &sid, "--task", &tid])
        .await;
    assert_ne!(code, 0, "{out}{err}");
    assert!(
        err.contains("HANDOFF_SECRET_FOUND") && err.contains("`.env`"),
        "{out}{err}"
    );
    assert!(
        !out.contains("ghp_") && !err.contains("ghp_0123"),
        "the value is never printed"
    );
    std::fs::remove_file(Path::new(&root).join(".env")).unwrap();

    // ---- the confirmation: the plan is printed, nothing is sent ----
    let (code, plan, err) = cli
        .run(&["cloud", "handoff", "--session", &sid, "--task", &tid])
        .await;
    assert_eq!(code, 2, "confirmation required: {plan}{err}");
    for needed in [
        "handoff-plan ",
        "leaves-the-machine repository history",
        "leaves-the-machine session log",
        "leaves-the-machine stored objects",
        "leaves-the-machine worktree files",
        "summary.md",
        "secret-handles",
        "confirm-required",
    ] {
        assert!(plan.contains(needed), "`{needed}` in:\n{plan}");
    }
    let manifest_line = line_with(&plan, "manifest ");
    let manifest_hash = manifest_line
        .trim_start_matches("manifest ")
        .trim()
        .to_owned();
    let cloud_sid = field(line_with(&plan, "handoff-plan "), "session").to_owned();
    let (s, _) = api
        .get(&admin_token, &format!("/v1/sessions/{cloud_sid}"))
        .await;
    assert_eq!(
        s, 404,
        "before the confirmation the cloud knows nothing of the task"
    );
    // The wrong confirmation sends nothing either.
    let (code, out, err) = cli
        .run(&[
            "cloud",
            "handoff",
            "--session",
            &sid,
            "--task",
            &tid,
            "--confirm",
            "deadbeefdeadbeef",
        ])
        .await;
    assert_ne!(code, 0, "{out}{err}");
    assert!(err.contains("nothing was sent"), "{err}");
    let (s, _) = api
        .get(&admin_token, &format!("/v1/sessions/{cloud_sid}"))
        .await;
    assert_eq!(s, 404);
    // The confirmation names the manifest the person saw: uploaded, admitted.
    let admitted = cli
        .ok(&[
            "cloud",
            "handoff",
            "--session",
            &sid,
            "--task",
            &tid,
            "--confirm",
            &manifest_hash[..16],
        ])
        .await;
    let line = line_with(&admitted, "handoff-admitted ");
    assert_eq!(field(line, "session"), cloud_sid);
    let cloud_tid = field(line, "task").to_owned();
    // The same confirmation again is the same admission.
    let again = cli
        .ok(&[
            "cloud",
            "handoff",
            "--session",
            &sid,
            "--task",
            &tid,
            "--confirm",
            &manifest_hash[..16],
        ])
        .await;
    assert_eq!(
        field(line_with(&again, "handoff-admitted "), "bundle"),
        field(line, "bundle"),
        "{again}"
    );
    let listed = cli.ok(&["cloud", "list"]).await;
    assert!(
        listed.contains("location=cloud") && listed.contains(&cloud_tid),
        "{listed}"
    );

    // ---- the cloud continues it in a sandbox; the organisation's policy asks about the file it reads ----
    let cloud_script = vec![
        json!({"calls": [{"name": "fs.read", "args": {"path": "summary.md"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "continued in the cloud", "self_review": {"findings": []}}}]}),
    ];
    let (cloud_model, cloud_seen) = scripted_model(cloud_script).await;
    let gateway = Gateway::start(&store_cfg, &data.join("gateway")).await;
    let worker = start(worker_config_gateway(
        &store_cfg,
        "worker-px128",
        &data.join("w"),
        &cloud_model,
        &gateway,
    ))
    .await
    .expect("worker");

    // Watch: the approval appears with the intent a decision names.
    let (code, watched, err) = cli
        .run(&[
            "cloud",
            "watch",
            "--session",
            &cloud_sid,
            "--follow",
            "--until",
            "ApprovalRequested",
            "--timeout-secs",
            "240",
        ])
        .await;
    assert_eq!(code, 0, "{watched}{err}");
    let ask = watched
        .lines()
        .find(|l| l.split_whitespace().nth(2) == Some("ApprovalRequested"))
        .unwrap_or_else(|| panic!("{watched}"));
    let approval = field(ask, "approval").to_owned();
    let intent = field(ask, "intent").to_owned();
    assert_eq!(field(ask, "tool"), "fs.read");
    let approvals = cli
        .ok(&["cloud", "approvals", "--session", &cloud_sid])
        .await;
    assert!(
        approvals.contains(&format!(
            "approval {approval} status=REQUESTED tool=fs.read"
        )),
        "{approvals}"
    );
    // Nothing ran before the approval: the model has seen no shell result.
    assert!(
        !cloud_seen
            .lock()
            .unwrap()
            .iter()
            .any(|b| b["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["role"] == "tool"
                    && m["content"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("\"sandbox\""))),
        "the read did not happen before it was approved"
    );
    // A decision that names another intent is refused.
    let (code, out, err) = cli
        .run(&[
            "cloud",
            "approve",
            "--approval",
            &approval,
            "--intent",
            &"0".repeat(64),
        ])
        .await;
    assert_ne!(code, 0, "{out}{err}");
    // Answered from one client (the desktop's call: the same HTTP) ...
    let command_id = uuid::Uuid::now_v7().to_string();
    let (s, r) = api
        .post(
            &admin_token,
            &format!("/v1/approvals/{approval}:approve"),
            json!({"command_id": command_id, "intent_hash": intent, "reason": "from the desktop"}),
        )
        .await;
    assert!(s == 200 || s == 202, "{s}: {r}");
    until("the first answer to be applied", 60, async || {
        let (_, c) = api
            .get(&admin_token, &format!("/v1/commands/{command_id}"))
            .await;
        (s == 200 || c["status"] == "ACCEPTED").then_some(())
    })
    .await;
    // ... and refused to the second.
    let (code, out, err) = cli
        .run(&[
            "cloud",
            "approve",
            "--approval",
            &approval,
            "--intent",
            &intent,
        ])
        .await;
    assert_ne!(code, 0, "the second answer is refused: {out}{err}");
    // The continuation reaches review, having run the command once approved.
    let (code, done, err) = cli
        .run(&[
            "cloud",
            "watch",
            "--session",
            &cloud_sid,
            "--follow",
            "--until",
            "TaskReadyForReview,TaskFailed",
            "--timeout-secs",
            "90",
        ])
        .await;
    if code != 0 {
        // Diagnosability: what the cloud model was shown.
        for (i, b) in cloud_seen.lock().unwrap().iter().enumerate() {
            let tools: Vec<String> = b["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["role"] == "tool")
                .map(|m| {
                    m["content"]
                        .as_str()
                        .unwrap_or_default()
                        .chars()
                        .take(300)
                        .collect()
                })
                .collect();
            eprintln!(
                "cloud model request {i}: {} tool results; last: {:?}",
                tools.len(),
                tools.last()
            );
        }
    }
    assert_eq!(
        code,
        0,
        "{}{err}",
        done.lines().rev().take(40).collect::<Vec<_>>().join("\n")
    );
    assert!(done.contains("TaskReadyForReview"), "{done}");
    let seen = cloud_seen.lock().unwrap().clone();
    let tool_texts: Vec<String> = seen.last().unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        tool_texts
            .iter()
            .any(|t| t.contains("handed off from the laptop")),
        "the laptop's edit arrived: {tool_texts:?}"
    );
    assert!(
        tool_texts
            .iter()
            .any(|t| t.contains("\"sandbox\"") && t.contains("handed off from the laptop")),
        "the approved read ran in the sandbox: {tool_texts:?}"
    );
    let approvals = cli
        .ok(&["cloud", "approvals", "--session", &cloud_sid])
        .await;
    assert!(approvals.contains("status=APPROVED"), "{approvals}");
    let events = api.events(&admin_token, &cloud_sid).await;
    assert_eq!(
        events
            .iter()
            .filter(|(t, _)| t == "ApprovalResolved")
            .count(),
        1,
        "decided once"
    );
    let listed = cli.ok(&["cloud", "list"]).await;
    assert!(listed.contains("state=READY_FOR_REVIEW"), "{listed}");

    // ---- a lost connection resumes by cursor ----
    let cursor_before: u64 =
        std::fs::read_to_string(profile.join("cloud").join("cursors").join(&cloud_sid))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
    let mut follow = tokio::process::Command::new(cli_bin())
        .env("MODBIT_CORE_BIN", core_bin())
        .arg("--data-dir")
        .arg(&profile)
        .args([
            "cloud",
            "watch",
            "--session",
            &cloud_sid,
            "--follow",
            "--timeout-secs",
            "90",
            "--after",
            &cursor_before.to_string(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    api.served.stop();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let served = serve_with(api_config(&store_cfg), extras())
        .await
        .expect("api again");
    let api2 = Api {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    // A new event after the outage: the person steers the finished task.
    let (s, r) = api2
        .post(
            &admin_token,
            &format!("/v1/tasks/{cloud_tid}:steer"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "text": "after the outage"}),
        )
        .await;
    assert!(s == 200 || s == 202, "{s}: {r}");
    tokio::time::sleep(Duration::from_secs(4)).await;
    let _ = follow.start_kill();
    let out = follow.wait_with_output().await.unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        err.contains("connection lost")
            && err.contains(&format!("retrying from cursor {cursor_before}")),
        "{err}"
    );
    let offsets: Vec<u64> = text
        .lines()
        .filter_map(|l| l.strip_prefix("event "))
        .filter_map(|l| l.split_whitespace().next())
        .filter_map(|n| n.parse().ok())
        .collect();
    assert!(
        !offsets.is_empty() && offsets[0] > cursor_before,
        "resumed after the cursor: {offsets:?}"
    );
    assert!(
        offsets.windows(2).all(|w| w[1] == w[0] + 1),
        "no gap and no duplicate: {offsets:?}"
    );
    assert!(
        text.contains("TaskInputQueued"),
        "the event after the outage arrived: {text}"
    );

    worker.stop().await;
    api2.served.stop();
    gateway.served.stop();
}
