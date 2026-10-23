//! PX-126 (QUAL-PX-126, docs/24 "Forge webhook intake"): check-suite,
//! check-run and comment deliveries, signed exactly as GitHub signs them,
//! become durable ingestion commands for the Core that owns the pull
//! request's task — in the tenant the repository's mapping names, never
//! another's. Runs on the real API over a real Postgres
//! (`MODBIT_CLOUD_TEST_DATABASE_URL`); the worker half (the Core reading
//! the forge and recording `ci` evidence) is `apps/cloud-worker/tests/
//! px_126_ingestion.rs`.

use hmac::{Hmac, KeyInit, Mac};
use modbit_cloud_api::{Config, Extras, Served, serve_with};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::TaskEvent;
use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_event_store::AppendRequest;
use modbit_event_store::cloud::{CloudStoreConfig, new_event};
use serde_json::{Value, json};
use sha2::Sha256;

const SECRET: &[u8] = b"px126-wh-secret-never-logged";

fn config() -> Option<Config> {
    let database_url = std::env::var("MODBIT_CLOUD_TEST_DATABASE_URL").ok()?;
    Some(Config {
        store: CloudStoreConfig {
            database_url,
            s3: None,
        },
        token_key: Some(vec![7u8; 32]),
        bind: "127.0.0.1:0".into(),
        rate_capacity: 500,
        rate_per_second: 100.0,
        worker_key: None,
        github_webhook_secret: Some(SECRET.to_vec()),
    })
}

struct Api {
    served: Served,
    http: reqwest::Client,
    base: String,
}

impl Api {
    async fn start(cfg: Config) -> Api {
        let served = serve_with(cfg, Extras::default()).await.expect("serve");
        let base = format!("http://{}", served.addr);
        Api {
            served,
            http: reqwest::Client::new(),
            base,
        }
    }

    async fn tenant(&self, name: &str) -> (TenantId, String) {
        let t = self.served.state.store.create_tenant(name).await.unwrap();
        let (_p, secret) = self
            .served
            .state
            .store
            .create_principal(t, "user", name)
            .await
            .unwrap();
        let tok: Value = self
            .http
            .post(format!("{}/v1/auth/token", self.base))
            .json(&json!({"secret": secret}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (t, tok["access_token"].as_str().unwrap().to_owned())
    }

    async fn post(&self, token: &str, path: &str, body: Value) -> (u16, Value) {
        let r = self
            .http
            .post(format!("{}{path}", self.base))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }

    async fn get(&self, token: &str, path: &str) -> (u16, Value) {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }

    /// A signed delivery, as GitHub sends it.
    async fn deliver(&self, id: &str, event: &str, body: &Value) -> (u16, Value) {
        let bytes = serde_json::to_vec(body).unwrap();
        self.deliver_raw(id, event, bytes.clone(), Some(sign(SECRET, &bytes)))
            .await
    }

    async fn deliver_raw(
        &self,
        id: &str,
        event: &str,
        bytes: Vec<u8>,
        signature: Option<String>,
    ) -> (u16, Value) {
        let mut r = self
            .http
            .post(format!("{}/v1/forge/github/webhook", self.base))
            .header("content-type", "application/json")
            .header("x-github-delivery", id)
            .header("x-github-event", event)
            .header("user-agent", "GitHub-Hookshot/test");
        if let Some(s) = signature {
            r = r.header("x-hub-signature-256", s);
        }
        let r = r.body(bytes).send().await.unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
}

fn sign(secret: &[u8], body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

fn iso(secs_ago: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let secs = now - secs_ago;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// A task in the tenant's session that opened pull request `number` of
/// `repo` on `head` (the `ForgePullRequestOpened` the Core records).
async fn task_with_pull_request(
    api: &Api,
    token: &str,
    tenant: TenantId,
    session: &str,
    repo: &str,
    number: u64,
    head: &str,
) -> TaskId {
    let (s, t) = api
        .post(
            token,
            &format!("/v1/sessions/{session}/tasks"),
            json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": "open a pull request"}),
        )
        .await;
    assert_eq!(s, 201, "{t}");
    let task = TaskId::parse(t["task_id"].as_str().unwrap()).unwrap();
    let (owner, name) = repo.split_once('/').unwrap();
    let ev = new_event(
        "ForgePullRequestOpened",
        &TaskEvent::ForgePullRequestOpened {
            idempotency_key: format!("key-{number}-{repo}"),
            owner: owner.into(),
            repo: name.into(),
            number,
            url: format!("https://github.com/{repo}/pull/{number}"),
            head: head.into(),
            head_sha: "a".repeat(40),
            base: "main".into(),
            result: json!({}),
        },
        Actor::External("test".into()),
    )
    .unwrap();
    api.served
        .state
        .store
        .append(
            AppendRequest {
                tenant_id: tenant,
                session_id: SessionId::parse(session).unwrap(),
                task_id: Some(task),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task.as_bytes(),
                expected_sequence: None,
                events: vec![ev],
            },
            None,
        )
        .await
        .unwrap();
    task
}

fn check_suite(
    repo: &str,
    installation: i64,
    pr: Option<u64>,
    branch: &str,
    updated: &str,
) -> Value {
    json!({
        "action": "completed",
        "check_suite": {
            "id": 1, "head_branch": branch, "head_sha": "a".repeat(40), "status": "completed",
            "conclusion": "failure", "updated_at": updated,
            "pull_requests": pr.map(|n| vec![json!({"number": n, "head": {"ref": branch}})]).unwrap_or_default(),
        },
        "repository": {"full_name": repo},
        "installation": {"id": installation},
        "sender": {"login": "github-actions[bot]", "type": "Bot"},
    })
}

fn comment_event(repo: &str, number: u64, installation: i64, sender_type: &str) -> Value {
    json!({
        "action": "created",
        "issue": {"number": number, "pull_request": {"url": format!("https://api.github.com/repos/{repo}/pulls/{number}")}},
        "comment": {"id": 4242, "user": {"login": "maintainer"}, "body": "@modbit please guard negatives", "created_at": iso(5)},
        "repository": {"full_name": repo},
        "installation": {"id": installation},
        "sender": {"login": "maintainer", "type": sender_type},
    })
}

/// The commands queued for a session, from the ledger: `(kind, task_id)`.
async fn queued(api: &Api, tenant: TenantId, session: &str) -> Vec<(String, String)> {
    api.served
        .state
        .store
        .pending_commands(tenant, SessionId::parse(session).unwrap(), 100)
        .await
        .unwrap()
        .into_iter()
        .filter(|(_, k, _)| k.starts_with("Ingest:"))
        .map(|(_, k, b)| (k, b["task_id"].as_str().unwrap_or_default().to_owned()))
        .collect()
}

#[tokio::test]
async fn qual_px_126_check_and_comment_deliveries_become_durable_ingestion_commands_for_the_owning_task_and_nothing_else()
 {
    let Some(cfg) = config() else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    let api = Api::start(cfg.clone()).await;
    let tag = uuid::Uuid::now_v7().simple().to_string();
    let repo = format!("acme/widgets-{tag}");
    let did = |n: &str| format!("px126-{tag}-{n}");
    let (ta, a) = api.tenant("tenant-a").await;
    let (tb, b) = api.tenant("tenant-b").await;
    let (_, sa) = api
        .post(
            &a,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sa = sa["session_id"].as_str().unwrap().to_owned();
    let (_, sb) = api
        .post(
            &b,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sb = sb["session_id"].as_str().unwrap().to_owned();
    let (s, m) = api
        .post(
            &a,
            "/v1/forge/repositories",
            json!({"repository": &repo, "session_id": sa, "installation_id": 42}),
        )
        .await;
    assert_eq!(s, 201, "{m}");
    // A's task opened pull request 3 on `modbit/pr-aaaaaaaaaaaa`; B's task
    // opened 5 on the same repository (B cannot map it, but its own Core
    // may open a pull request there with its own token).
    let head = "modbit/pr-aaaaaaaaaaaa";
    let task_a = task_with_pull_request(&api, &a, ta, &sa, &repo, 3, head).await;
    let task_b =
        task_with_pull_request(&api, &b, tb, &sb, &repo, 5, "modbit/pr-bbbbbbbbbbbb").await;
    let before_b = queued(&api, tb, &sb).await;

    // 1. A signed failing check suite for A's pull request: queued for A's session, for A's task.
    let suite = check_suite(&repo, 42, Some(3), head, &iso(10));
    let (s, r) = api.deliver(&did("1"), "check_suite", &suite).await;
    assert_eq!(
        (s, r["code"].as_str()),
        (202, Some("INGESTION_QUEUED")),
        "{r}"
    );
    assert_eq!(r["task_id"], task_a.to_string());
    assert_eq!(r["kind"], "Ingest:ci");
    let cmd = r["command_id"].as_str().unwrap().to_owned();
    let (s, rec) = api.get(&a, &format!("/v1/commands/{cmd}")).await;
    assert_eq!(
        (s, rec["status"].as_str(), rec["kind"].as_str()),
        (200, Some("PENDING"), Some("Ingest:ci")),
        "{rec}"
    );
    assert_eq!(
        queued(&api, ta, &sa).await,
        vec![("Ingest:ci".to_owned(), task_a.to_string())]
    );
    // The session is made ready for a worker (nobody holds it).
    let lease = api
        .served
        .state
        .store
        .lease(ta, SessionId::parse(&sa).unwrap())
        .await
        .unwrap();
    assert!(
        lease.is_some_and(|(_, _, _, ready)| ready),
        "a worker can claim it"
    );

    // 2. The same delivery id again: a replay, whatever the body says now.
    let (s, r) = api
        .deliver(
            &did("1"),
            "check_suite",
            &check_suite(&repo, 42, Some(3), head, &iso(1)),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (409, Some("WEBHOOK_REPLAYED")),
        "{r}"
    );
    assert_eq!(
        queued(&api, ta, &sa).await.len(),
        1,
        "a replay queues nothing"
    );

    // 3. Unsigned and mis-signed: refused, audited, nothing queued.
    let bytes = serde_json::to_vec(&suite).unwrap();
    let (s, r) = api
        .deliver_raw(&did("2"), "check_suite", bytes.clone(), None)
        .await;
    assert_eq!((s, r["code"].as_str()), (401, Some("WEBHOOK_UNSIGNED")));
    let (s, r) = api
        .deliver_raw(
            &did("2"),
            "check_suite",
            bytes.clone(),
            Some(sign(b"another", &bytes)),
        )
        .await;
    assert_eq!((s, r["code"].as_str()), (401, Some("WEBHOOK_SIGNATURE")));

    // 4. A stale event time (a capture replayed under a fresh delivery id).
    let (s, r) = api
        .deliver(
            &did("3"),
            "check_suite",
            &check_suite(&repo, 42, Some(3), head, &iso(3 * 3600)),
        )
        .await;
    assert_eq!((s, r["code"].as_str()), (400, Some("WEBHOOK_STALE")), "{r}");
    let (s, r) = api
        .deliver(
            &did("3b"),
            "check_suite",
            &check_suite(&repo, 42, Some(3), head, &iso(-3600)),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (400, Some("WEBHOOK_STALE")),
        "an event from the future: {r}"
    );
    let audit = api
        .served
        .state
        .store
        .denials_for_resource(&format!("webhook:github:{}", did("3")))
        .await
        .unwrap();
    assert!(
        audit
            .iter()
            .any(|(t, why)| t.is_some() && why.contains("outside the accepted window")),
        "{audit:?}"
    );

    // 5. An unknown repository, and a delivery from another installation.
    let (s, r) = api
        .deliver(
            &did("4"),
            "check_suite",
            &check_suite("nobody/mapped-nothing", 42, Some(3), head, &iso(5)),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (404, Some("REPOSITORY_UNMAPPED")),
        "{r}"
    );
    let (s, r) = api
        .deliver(
            &did("5"),
            "check_suite",
            &check_suite(&repo, 99, Some(3), head, &iso(5)),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (403, Some("INSTALLATION_MISMATCH")),
        "{r}"
    );

    // 6. The wrong tenant: pull request 5 is B's task's; the mapping names A.
    let (s, r) = api
        .deliver(
            &did("6"),
            "check_suite",
            &check_suite(&repo, 42, Some(5), "modbit/pr-bbbbbbbbbbbb", &iso(5)),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (404, Some("PULL_REQUEST_UNKNOWN")),
        "{r}"
    );
    assert!(
        !r.to_string().contains(&task_b.to_string()) && !r.to_string().contains(&sb),
        "the other tenant is not named: {r}"
    );
    let a_audit = api.served.state.store.denials(ta).await.unwrap();
    assert!(
        a_audit
            .iter()
            .any(|(res, why)| *res == format!("webhook:github:{}", did("6"))
                && why.contains("another tenant")),
        "{a_audit:?}"
    );
    assert_eq!(
        queued(&api, tb, &sb).await,
        before_b,
        "nothing reached the other tenant"
    );
    assert_eq!(queued(&api, ta, &sa).await.len(), 1);

    // 7. A pull request nobody's task opened is an ordinary one: ignored, on the ledger.
    let (s, r) = api
        .deliver(
            &did("7"),
            "check_suite",
            &check_suite(&repo, 42, Some(99), "human/branch", &iso(5)),
        )
        .await;
    assert_eq!(
        (s, r["outcome"].as_str()),
        (202, Some("ignored:no_task")),
        "{r}"
    );

    // 8. A check run that names no pull request, only the branch Modbit pushed.
    let run = json!({
        "action": "completed",
        "check_run": {"id": 9, "name": "ci", "status": "completed", "conclusion": "success", "head_sha": "a".repeat(40),
                      "completed_at": iso(20), "check_suite": {"head_branch": head}, "pull_requests": []},
        "repository": {"full_name": repo}, "installation": {"id": 42}, "sender": {"login": "ci[bot]", "type": "Bot"},
    });
    let (s, r) = api.deliver(&did("8"), "check_run", &run).await;
    assert_eq!(
        (s, r["code"].as_str()),
        (202, Some("INGESTION_QUEUED")),
        "{r}"
    );
    assert_eq!(r["task_id"], task_a.to_string());

    // 9. Comments: on the pull request, queued; by a bot, on a plain issue,
    //    or edited, ignored; the inline kind queued.
    let (s, r) = api
        .deliver(
            &did("9"),
            "issue_comment",
            &comment_event(&repo, 3, 42, "User"),
        )
        .await;
    assert_eq!(
        (s, r["kind"].as_str()),
        (202, Some("Ingest:review_comments")),
        "{r}"
    );
    let (s, r) = api
        .deliver(
            &did("10"),
            "issue_comment",
            &comment_event(&repo, 3, 42, "Bot"),
        )
        .await;
    assert_eq!(
        (s, r["outcome"].as_str()),
        (202, Some("ignored:issue_comment.bot")),
        "{r}"
    );
    let mut plain = comment_event(&repo, 3, 42, "User");
    plain["issue"]
        .as_object_mut()
        .unwrap()
        .remove("pull_request");
    let (s, r) = api.deliver(&did("11"), "issue_comment", &plain).await;
    assert_eq!((s, r["code"].as_str()), (202, Some("IGNORED")), "{r}");
    let mut edited = comment_event(&repo, 3, 42, "User");
    edited["action"] = json!("edited");
    let (s, r) = api.deliver(&did("12"), "issue_comment", &edited).await;
    assert_eq!((s, r["code"].as_str()), (202, Some("IGNORED")), "{r}");
    let inline = json!({
        "action": "created", "pull_request": {"number": 3},
        "comment": {"id": 77, "user": {"login": "maintainer"}, "body": "@modbit on this line", "path": "src/a.rs", "created_at": iso(3)},
        "repository": {"full_name": &repo}, "installation": {"id": 42}, "sender": {"login": "maintainer", "type": "User"},
    });
    let (s, r) = api
        .deliver(&did("13"), "pull_request_review_comment", &inline)
        .await;
    assert_eq!(
        (s, r["kind"].as_str()),
        (202, Some("Ingest:review_comments")),
        "{r}"
    );
    let kinds: Vec<String> = queued(&api, ta, &sa)
        .await
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        kinds,
        [
            "Ingest:ci",
            "Ingest:ci",
            "Ingest:review_comments",
            "Ingest:review_comments"
        ]
    );

    // 10. The control plane restarts between two deliveries: what was queued
    //     is still queued once, a replay of an earlier delivery is still a
    //     replay, and the next delivery queues exactly one more.
    api.served.stop();
    let api2 = Api::start(cfg).await;
    assert_eq!(
        queued(&api2, ta, &sa).await.len(),
        4,
        "nothing lost across the restart"
    );
    let (s, r) = api2.deliver(&did("1"), "check_suite", &suite).await;
    assert_eq!(
        (s, r["code"].as_str()),
        (409, Some("WEBHOOK_REPLAYED")),
        "{r}"
    );
    let (s, r) = api2
        .deliver(
            &did("9"),
            "issue_comment",
            &comment_event(&repo, 3, 42, "User"),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (409, Some("WEBHOOK_REPLAYED")),
        "{r}"
    );
    let (s, r) = api2
        .deliver(
            &did("14"),
            "check_suite",
            &check_suite(&repo, 42, Some(3), head, &iso(2)),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (202, Some("INGESTION_QUEUED")),
        "{r}"
    );
    assert_eq!(queued(&api2, ta, &sa).await.len(), 5, "no duplicate");
    // The ledger shows each delivery once with what came of it.
    let (_, ledger) = api2.get(&a, "/v1/forge/repositories").await;
    let outcomes: Vec<String> = ledger["deliveries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["delivery_id"].as_str().is_some_and(|i| i.contains(&tag)))
        .map(|d| {
            format!(
                "{}={}",
                d["delivery_id"]
                    .as_str()
                    .unwrap()
                    .rsplit('-')
                    .next()
                    .unwrap(),
                d["outcome"].as_str().unwrap()
            )
        })
        .collect();
    assert!(
        outcomes.iter().any(|o| o == "1=queued:Ingest:ci"),
        "{outcomes:?}"
    );
    assert!(outcomes.iter().any(|o| o == "3=stale"), "{outcomes:?}");
    assert!(
        outcomes.iter().any(|o| o == "6=wrong_tenant"),
        "{outcomes:?}"
    );
    assert!(
        outcomes.iter().any(|o| o == "7=ignored:no_task"),
        "{outcomes:?}"
    );
    api2.served.stop();
}
