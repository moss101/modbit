//! PX-129 (QUAL-PX-129, docs/24 "Execution policy distribution"), the
//! worker half: the real worker verifies the tenant's policy bundle itself
//! and hands what it accepts to its real `modbit-core` as the administrator
//! layer. A bundle accepted and served by the API reaches the Core; a
//! tampered row, a bundle another key signed and a row put back to an older
//! generation (all written straight into Postgres, past the API) change
//! nothing — the last good bundle stays; a worker restarted past a
//! rollback keeps the floor it had; and a tenant whose bundle has expired
//! starts no task until a fresh one is published.
//!
//! Runs only where `MODBIT_CLOUD_TEST_DATABASE_URL` names a database.

mod px_cloud_common;

use std::time::Duration;

use ed25519_dalek::SigningKey;
use modbit_cloud_api::Extras;
use modbit_cloud_worker::start;
use modbit_domain::policy_bundle::{self, BundleDocument, KIND, SCHEMA_VERSION};
use px_cloud_common::*;
use serde_json::{Value, json};

const ADMIN: &str = "px129-worker-platform-admin";

fn document(tenant: &str, generation: u64, ttl_ms: i64, authors: &[&str]) -> BundleDocument {
    let now = modbit_domain::Timestamp::now().millis();
    BundleDocument {
        kind: KIND.into(),
        schema_version: SCHEMA_VERSION,
        tenant_id: tenant.into(),
        generation,
        issued_at_ms: now,
        expires_at_ms: now + ttl_ms,
        min_protocol_major: 1,
        admin_config: json!({"permissions": {"task.handoff": "DENY"}, "review_comment_authors": authors}),
    }
}

fn signed(d: &BundleDocument, key_id: &str, k: &SigningKey) -> Value {
    serde_json::to_value(policy_bundle::sign(d, key_id, k).unwrap()).unwrap()
}

async fn sql(url: &str) -> tokio_postgres::Client {
    let (c, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    c
}

fn applied(dir: &std::path::Path) -> Value {
    std::fs::read_to_string(dir.join("admin-config.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

#[tokio::test]
async fn qual_px_129_the_worker_enforces_only_bundles_it_verifies_keeps_the_last_good_one_and_starts_nothing_on_an_expired_one()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    assert!(
        core_bin().exists(),
        "modbit-core at {}",
        core_bin().display()
    );
    let keep = tempfile::tempdir().unwrap();
    let data = keep.path().to_path_buf();
    let api = Api::start_with(&store_cfg, None, Extras::default().with_admin_secret(ADMIN)).await;
    // The tenant, its administrator and the organisation's signing key.
    let (s, t) = api
        .post(ADMIN, "/v1/admin/tenants", json!({"name": "px129-worker"}))
        .await;
    assert_eq!(s, 201, "{t}");
    let tenant = t["tenant_id"].as_str().unwrap().to_owned();
    let tenant_id = modbit_domain::TenantId::parse(&tenant).unwrap();
    let (s, p) = api
        .post(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/principals"),
            json!({"label": "root", "role": "admin"}),
        )
        .await;
    assert_eq!(s, 201, "{p}");
    let (_, pair) = api
        .post("", "/v1/auth/token", json!({"secret": p["secret"]}))
        .await;
    let admin = pair["access_token"].as_str().unwrap().to_owned();
    let org = SigningKey::from_bytes(&[77u8; 32]);
    let (s, _) = api
        .call_put(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/org-keys/org-1"),
            json!({"public_key_hex": hex::encode(org.verifying_key().to_bytes())}),
        )
        .await;
    assert_eq!(s, 200);
    let publish = |doc: BundleDocument| {
        let (api, admin, org) = (&api, admin.clone(), org.clone());
        async move {
            api.call_put(&admin, "/v1/policy/bundle", signed(&doc, "org-1", &org))
                .await
        }
    };
    let (s, r) = publish(document(&tenant, 1, 3_600_000, &["reviewer"])).await;
    assert_eq!(s, 201, "{r}");

    // A session with a queued task, and a worker.
    let root = repo(&data.join("repo"));
    let (_, sess) = api
        .post(
            &admin,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    let sid = sess["session_id"].as_str().unwrap().to_owned();
    let new_task = |goal: &'static str| {
        let (api, admin, sid, root) = (&api, admin.clone(), sid.clone(), root.clone());
        async move {
            let (s, t) = api
                .post(&admin, &format!("/v1/sessions/{sid}/tasks"), json!({"command_id": uuid::Uuid::now_v7().to_string(), "goal_text": goal, "execution_profile": "local_trusted", "workspace_root": root}))
                .await;
            // 201: created by the API; 202: relayed to the worker that holds the session.
            assert!(s == 201 || s == 202, "{s}: {t}");
        }
    };
    new_task("first task").await;
    let script = |n: usize| -> Vec<Value> {
        (0..n)
            .flat_map(|_| {
                vec![
                    json!({"calls": [{"name": "plan.update", "args": {"outcome": "nothing to change", "expected_files": []}}]}),
                    json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
                ]
            })
            .collect()
    };
    let (model, _seen) = scripted_model(script(1)).await;
    let session_dir = data.join("w").join("sessions").join(&sid);
    let worker = start(worker_config(
        &store_cfg,
        "worker-policy",
        &data.join("w"),
        &model,
        None,
    ))
    .await
    .expect("worker");
    let states = || {
        let (api, admin, sid) = (&api, admin.clone(), sid.clone());
        async move {
            let (_, v) = api.get(&admin, &format!("/v1/sessions/{sid}")).await;
            v["tasks"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|t| t["state"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        }
    };

    // 1. The published bundle is the Core's administrator layer, and the task runs.
    let gen1 = document(&tenant, 1, 0, &["reviewer"]).admin_config;
    until(
        "generation 1 to reach the Core's administrator layer",
        60,
        async || (applied(&session_dir) == gen1).then_some(()),
    )
    .await;
    until("the first task to reach review", 120, async || {
        (states().await == ["READY_FOR_REVIEW"]).then_some(())
    })
    .await;
    // 2. A newer bundle replaces it.
    let (s, r) = publish(document(&tenant, 2, 3_600_000, &["reviewer", "second"])).await;
    assert_eq!(s, 201, "{r}");
    let gen2 = document(&tenant, 2, 0, &["reviewer", "second"]).admin_config;
    until("generation 2 to replace it", 30, async || {
        (applied(&session_dir) == gen2).then_some(())
    })
    .await;

    // 3. Past the API, straight into Postgres: a tampered row, a bundle another
    //    key signed, and — deleting what is newer — a row put back to an older
    //    generation. None changes what the Core runs under.
    let db = sql(&store_cfg.database_url).await;
    let tenant_uuid = uuid::Uuid::from_bytes(*tenant_id.as_bytes());
    let insert = |generation: i64, bundle: Value| {
        let (db, tenant_uuid) = (&db, tenant_uuid);
        async move {
            db.execute(
                "INSERT INTO policy_bundles (tenant_id, generation, key_id, signed, published_at_ms) VALUES ($1, $2, 'org-1', $3, 0)",
                &[&tenant_uuid, &generation, &bundle],
            )
            .await
            .unwrap();
        }
    };
    let mut tampered = signed(
        &document(&tenant, 3, 3_600_000, &["reviewer", "second"]),
        "org-1",
        &org,
    );
    tampered["document"] = json!(
        tampered["document"]
            .as_str()
            .unwrap()
            .replace("second", "mallory")
    );
    insert(3, tampered).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        applied(&session_dir),
        gen2,
        "a tampered row changed nothing"
    );
    insert(
        4,
        signed(
            &document(&tenant, 4, 3_600_000, &["mallory"]),
            "org-1",
            &SigningKey::from_bytes(&[78u8; 32]),
        ),
    )
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        applied(&session_dir),
        gen2,
        "another key's bundle changed nothing"
    );
    db.execute(
        "DELETE FROM policy_bundles WHERE tenant_id = $1 AND generation >= 2",
        &[&tenant_uuid],
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        applied(&session_dir),
        gen2,
        "a rollback to generation 1 changed nothing"
    );
    let saved: Value = serde_json::from_str(
        &std::fs::read_to_string(session_dir.join("policy-bundle.json")).unwrap(),
    )
    .unwrap();
    assert!(
        saved["document"]
            .as_str()
            .unwrap()
            .contains("\"generation\":2"),
        "the floor is remembered beside the Core"
    );

    // 4. A restart past the same rollback keeps the floor.
    worker.stop().await;
    let worker = start(worker_config(
        &store_cfg,
        "worker-policy-2",
        &data.join("w"),
        &model,
        None,
    ))
    .await
    .expect("worker 2");
    new_task("second task").await;
    until(
        "the second task to be started (a bundle is in force)",
        120,
        async || {
            (states()
                .await
                .iter()
                .filter(|s| s.as_str() == "READY_FOR_REVIEW")
                .count()
                == 2)
                .then_some(())
        },
    )
    .await;
    assert_eq!(
        applied(&session_dir),
        gen2,
        "the restarted worker did not move back to generation 1"
    );

    // 5. Fail closed: the bundle in force expires; a task queued meanwhile does not start.
    let (s, r) = publish(document(&tenant, 6, 3_000, &["reviewer"])).await;
    assert_eq!(s, 201, "{r}");
    let gen6 = document(&tenant, 6, 0, &["reviewer"]).admin_config;
    until("generation 6 to be applied", 30, async || {
        (applied(&session_dir) == gen6).then_some(())
    })
    .await;
    tokio::time::sleep(Duration::from_millis(3_500)).await;
    new_task("third task").await;
    for _ in 0..12 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            states()
                .await
                .iter()
                .filter(|s| s.as_str() == "QUEUED")
                .count()
                == 1,
            "no task starts on an expired bundle: {:?}",
            states().await
        );
    }
    // A fresh bundle ends it.
    let (s, r) = publish(document(&tenant, 7, 3_600_000, &["reviewer"])).await;
    assert_eq!(s, 201, "{r}");
    until(
        "the third task to run once a fresh bundle is in force",
        120,
        async || {
            (states()
                .await
                .iter()
                .filter(|s| s.as_str() == "READY_FOR_REVIEW")
                .count()
                == 3)
                .then_some(())
        },
    )
    .await;
    worker.stop().await;
    api.served.stop();
}
