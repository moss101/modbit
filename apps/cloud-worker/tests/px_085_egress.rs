//! PX-085 (QUAL-PX-085: "a forced organisation allow-list denies a run's
//! disallowed host"; docs/68 AUT-D07), the cloud half: a cloud automation run
//! on a real worker, a real `modbit-core` and a real Sandbox Gateway (the
//! Firecracker MicroVM backend where the job provides it, the reference
//! backend elsewhere), over a real Postgres.
//!
//! * The owner's enable approval lists the hosts the run may reach; the
//!   run's sandbox is given egress to exactly those (and the forge's API
//!   host, when the lease carries `network.egress`) and nothing else.
//! * The organisation's allow-list is the tenant's signed policy bundle's
//!   `network_allow`. The gateway reads and verifies it itself when it
//!   provisions a sandbox, and its egress broker refuses, by a typed reason
//!   (`ORG_ALLOW_LIST`), any destination a rule admits and the list does not
//!   name. The definition cannot widen it: a host the approval lists and the
//!   list omits is denied, and the denial is on the run.
//! * The same definition, run before the organisation published its list,
//!   reaches both hosts - so it is the list that denied, not a missing rule.
//!
//! The model is the repository's scripted OpenAI-compatible stand-in; the
//! destinations are `.invalid` names (they resolve nowhere; what is proven
//! is the broker's decision) and the loopback fake forge (a real socket: an
//! admitted destination is relayed and the forge sees the real token).
//!
//! Runs only where `MODBIT_CLOUD_TEST_DATABASE_URL` names a database.

mod px_cloud_common;

use ed25519_dalek::SigningKey;
use modbit_automation::webhook;
use modbit_cloud_api::Extras;
use modbit_cloud_worker::start;
use modbit_domain::policy_bundle::{self, BundleDocument, KIND, SCHEMA_VERSION};
use px_cloud_common::*;
use serde_json::{Value, json};

const ADMIN: &str = "px085-egress-platform-admin";
const MASTER: &[u8] = b"px085-egress-master-key-0123456789abcdef";
const ALLOWED: &str = "allowed.example.invalid";
const DENIED: &str = "denied.example.invalid";
const OTHER: &str = "other.example.invalid";

fn org_bundle(tenant: &str, generation: u64, network_allow: &[&str]) -> Value {
    let now = modbit_domain::Timestamp::now().millis();
    let doc = BundleDocument {
        kind: KIND.into(),
        schema_version: SCHEMA_VERSION,
        tenant_id: tenant.into(),
        generation,
        issued_at_ms: now,
        expires_at_ms: now + 3_600_000,
        min_protocol_major: 1,
        admin_config: json!({"network_allow": network_allow}),
    };
    let org = SigningKey::from_bytes(&[85u8; 32]);
    serde_json::to_value(policy_bundle::sign(&doc, "org-85", &org).unwrap()).unwrap()
}

async fn deliver(api: &Api, endpoint: &str, secret: &str, nonce: &str) -> (u16, Value) {
    let bytes = serde_json::to_vec(&json!({"report": nonce})).unwrap();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let sig = webhook::sign(secret.as_bytes(), ts, nonce, &bytes);
    let r = api
        .http
        .post(format!("{}/v1/hooks/{endpoint}", api.base))
        .header(webhook::SIGNATURE_HEADER, sig)
        .header("x-modbit-delivery", format!("d-{nonce}"))
        .body(bytes)
        .send()
        .await
        .unwrap();
    (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
}

/// What the scripted model's shell calls printed, in call order.
fn tool_texts(seen: &std::sync::Arc<std::sync::Mutex<Vec<Value>>>) -> Vec<String> {
    seen.lock()
        .unwrap()
        .last()
        .and_then(|b| b["messages"].as_array().cloned())
        .map(|m| {
            m.iter()
                .filter(|x| x["role"] == "tool")
                .map(|x| x["content"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The egress audit of the sandbox of `session`'s task, from the gateway.
async fn audit_of(
    api: &Api,
    token: &str,
    gateway: &Gateway,
    worker: &str,
    tenant: &str,
    session: &str,
) -> Vec<Value> {
    let evs = api.events(token, session).await;
    let lease = evs
        .iter()
        .find(|(t, _)| t == "SandboxLeaseAcquired")
        .unwrap_or_else(|| {
            panic!(
                "SandboxLeaseAcquired on the run's log: {:?}",
                evs.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
            )
        })
        .1
        .clone();
    let sandbox = lease["sandbox_id"].as_str().unwrap().to_owned();
    let (s, audit) = gw_get(
        gateway,
        worker,
        &format!("/v1/sandboxes/{sandbox}/egress?tenant_id={tenant}"),
    )
    .await;
    assert_eq!(s, 200, "{audit}");
    audit["records"].as_array().cloned().unwrap_or_default()
}

fn record_for<'a>(records: &'a [Value], host: &str) -> Option<&'a Value> {
    records.iter().find(|r| {
        r["destination"]
            .as_str()
            .is_some_and(|d| d.starts_with(&format!("{host}:")))
    })
}

#[tokio::test]
async fn qual_px_085_a_cloud_runs_hosts_are_capped_by_the_organisations_allow_list_at_the_gateway_with_a_typed_reason_recorded_on_the_run()
 {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres and a sandbox)"
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
    let root = repo(&data.join("repo"));
    let api = Api::start_with(
        &store_cfg,
        None,
        Extras::default()
            .with_admin_secret(ADMIN)
            .with_webhook_master_key(MASTER),
    )
    .await;

    // The tenant, its administrator and the organisation's signing key.
    let (s, t) = api
        .post(ADMIN, "/v1/admin/tenants", json!({"name": "px085-egress"}))
        .await;
    assert_eq!(s, 201, "{t}");
    let tenant = t["tenant_id"].as_str().unwrap().to_owned();
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
    let a = pair["access_token"].as_str().unwrap().to_owned();
    let org = SigningKey::from_bytes(&[85u8; 32]);
    let (s, _) = api
        .call_put(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/org-keys/org-85"),
            json!({"public_key_hex": hex::encode(org.verifying_key().to_bytes())}),
        )
        .await;
    assert_eq!(s, 200);

    // A definition whose owner approved two hosts. (A cost limit is required
    // of a definition that can reach the network.)
    let def = json!({
        "schema": "modbit.automation/1", "name": "fetcher", "prompt": "Fetch the report hosts.",
        "triggers": [{"kind": "webhook", "id": "ping", "name": "ping"}],
        "profile": {"effects": "external_side_effect", "capabilities": ["network.egress"], "hosts": [ALLOWED, DENIED]},
        "concurrency": {"policy": "queue"},
        "limits": {"max_turns": 8, "max_tool_calls": 20, "deadline_minutes": 5, "max_cost_minor": 250},
    });
    let (s, created) = api
        .post(
            &a,
            "/v1/automations",
            json!({"definition": def, "workspace_root": root}),
        )
        .await;
    assert_eq!(s, 201, "{created}");
    let id = created["automation_id"].as_str().unwrap().to_owned();
    let (s, r) = api
        .post(
            &a,
            &format!("/v1/automations/{id}:enable"),
            json!({"version": 1, "definition_hash": created["definition_hash"], "effects": "external_side_effect", "capabilities": ["network.egress"], "paths": [], "hosts": [ALLOWED, DENIED]}),
        )
        .await;
    assert_eq!(s, 200, "{r}");
    // The enable approval above stated the cost limit it requires. The model
    // here is the scripted stand-in, which no registry prices, and a run with
    // a cost limit on a model it cannot price stops after its first turn
    // (`cost_unmetered`, PX-116): so the run's own copy of the limit is cleared
    // - the one thing this test sets aside; the approval, the hosts and the
    // profile are exactly what the owner approved.
    {
        let (client, conn) =
            tokio_postgres::connect(&store_cfg.database_url, tokio_postgres::NoTls)
                .await
                .unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let n = client
            .execute(
                "UPDATE automation_versions SET controls = jsonb_set(controls, '{max_cost_minor}', 'null') WHERE automation_id = $1::text::uuid",
                &[&id],
            )
            .await
            .unwrap();
        assert_eq!(n, 1);
    }
    let (s, ep) = api
        .post(
            &a,
            &format!("/v1/automations/{id}/endpoints"),
            json!({"trigger_id": "ping"}),
        )
        .await;
    assert_eq!(s, 201, "{ep}");
    let (endpoint, secret) = (
        ep["endpoint_id"].as_str().unwrap().to_owned(),
        ep["secret"].as_str().unwrap().to_owned(),
    );

    // What the run does in its sandbox: the forge through the broker's
    // credential, then the two approved hosts and one nobody approved.
    let fetch = "FETCH=$(command -v wget >/dev/null 2>&1 && echo 'wget -qO-' || echo 'curl -s')";
    let sh = |cmd: String| json!({"name": "shell.exec", "args": {"argv": ["/bin/sh", "-c", format!("{fetch}; {cmd}")]}});
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "the report hosts are fetched", "expected_files": []}}]}),
        json!({"calls": [
            sh("$FETCH http://forge.modbit.internal/user; echo".into()),
            sh(format!("$FETCH http://{ALLOWED}:443/report || echo allowed-failed")),
            sh(format!("$FETCH http://{DENIED}:443/report || echo denied-failed")),
            sh(format!("$FETCH http://{OTHER}:443/report || echo other-failed")),
        ]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "fetched", "self_review": {"findings": []}}}]}),
    ];
    let (model, seen) = scripted_model(script).await;
    let gateway = Gateway::start_preferring_microvm(&store_cfg, &data.join("gateway")).await;
    let forge = fake_forge().await;
    let worker = start(worker_config_gateway_forge(
        &store_cfg,
        "worker-px085-egress",
        &data.join("w"),
        &model,
        &gateway,
        &forge,
    ))
    .await
    .expect("worker");

    let run_to_review = async |nonce: &str| -> (String, Vec<Value>) {
        let (st, fired) = deliver(&api, &endpoint, &secret, nonce).await;
        assert_eq!(st, 201, "{fired}");
        let session = fired["session_id"].as_str().unwrap().to_owned();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(240);
        loop {
            let (_, v) = api.get(&a, &format!("/v1/sessions/{session}")).await;
            let state = v["tasks"][0]["state"].to_string();
            if state.contains("READY_FOR_REVIEW") {
                break;
            }
            if std::time::Instant::now() > deadline {
                let evs = api.events(&a, &session).await;
                let dump: Vec<String> = evs
                    .iter()
                    .map(|(t, p)| {
                        format!(
                            "{t} {}",
                            p.to_string().chars().take(260).collect::<String>()
                        )
                    })
                    .collect();
                panic!(
                    "the run did not reach review (state {state}); its log:\n{}",
                    dump.join("\n")
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        let records = audit_of(&api, &a, &gateway, "worker-px085-egress", &tenant, &session).await;
        (session, records)
    };

    // 1. Before the organisation published a list: the approval's two hosts
    //    are both reachable by the run (admitted; they resolve nowhere), the
    //    forge is relayed with the real token, and the host nobody approved
    //    has no rule.
    let (_session1, records1) = run_to_review("run-before-the-list").await;
    let allowed1 = record_for(&records1, ALLOWED).expect("the approved host was asked for");
    let denied1 = record_for(&records1, DENIED).expect("the second approved host was asked for");
    assert_eq!(allowed1["allowed"], true, "{records1:?}");
    assert_eq!(
        denied1["allowed"], true,
        "without an organisation list the approval's host is admitted: {records1:?}"
    );
    assert_eq!(denied1["capability"], "network.egress");
    let other1 = record_for(&records1, OTHER).expect("the unapproved host was asked for");
    assert_eq!(other1["allowed"], false);
    assert_eq!(other1["reason"], "NO_EGRESS_RULE", "{other1}");
    assert!(
        records1
            .iter()
            .any(|r| r["kind"] == "credentialed" && r["allowed"] == true),
        "{records1:?}"
    );
    let forge_seen = forge.seen.lock().unwrap().clone();
    assert!(
        forge_seen
            .iter()
            .any(|h| h == &format!("Bearer {}", forge.token)),
        "the fake forge saw the real token from the broker: {forge_seen:?}"
    );
    let (_, runs) = api.get(&a, &format!("/v1/automations/{id}/runs")).await;
    let reasons1: Vec<&str> = runs["runs"][0]["egress_denials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["reason"].as_str().unwrap())
        .collect();
    assert_eq!(
        reasons1,
        ["NO_EGRESS_RULE"],
        "only the unapproved host was refused: {runs}"
    );

    // 2. The organisation publishes its list: the forge's host and one of the
    //    two approved hosts. The definition still lists both.
    let (s, r) = api
        .call_put(
            &a,
            "/v1/policy/bundle",
            org_bundle(&tenant, 1, &["127.0.0.1", ALLOWED]),
        )
        .await;
    assert_eq!(s, 201, "{r}");
    let (session2, records2) = run_to_review("run-after-the-list").await;
    let allowed2 = record_for(&records2, ALLOWED).expect("asked for");
    assert_eq!(allowed2["allowed"], true, "{records2:?}");
    let denied2 = record_for(&records2, DENIED).expect("asked for");
    assert_eq!(denied2["allowed"], false, "{records2:?}");
    assert_eq!(denied2["reason"], "ORG_ALLOW_LIST", "{denied2}");
    assert!(
        denied2["detail"]
            .as_str()
            .unwrap()
            .contains("organisation policy bundle generation 1"),
        "{denied2}"
    );
    // The host nobody approved is still refused for want of a rule.
    assert_eq!(
        record_for(&records2, OTHER).unwrap()["reason"],
        "NO_EGRESS_RULE"
    );
    // What the list names and the rules admit still passes - for real: the
    // forge (127.0.0.1) is relayed, with the token the guest never held.
    assert!(
        records2
            .iter()
            .any(|r| r["kind"] == "credentialed" && r["allowed"] == true),
        "{records2:?}"
    );
    assert!(
        forge.seen.lock().unwrap().len() >= 2,
        "the forge answered the second run too"
    );
    // What the guest saw of the denied host is a refusal, not a page.
    let texts = tool_texts(&seen);
    assert!(
        texts.iter().any(|t| t.contains("authorized")),
        "the forge call succeeded in the run: {texts:?}"
    );
    // The denial is on the run, by code, beside the run's other facts.
    let (_, runs) = api.get(&a, &format!("/v1/automations/{id}/runs")).await;
    let latest = runs["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["session_id"] == session2.as_str())
        .expect("the second run");
    let denials = latest["egress_denials"].as_array().unwrap();
    let find = |host: &str| {
        denials.iter().find(|d| {
            d["destination"]
                .as_str()
                .is_some_and(|x| x.starts_with(host))
        })
    };
    let org_denial = find(DENIED).expect("the organisation's denial is on the run");
    assert_eq!(org_denial["reason"], "ORG_ALLOW_LIST", "{latest}");
    assert!(find(ALLOWED).is_none(), "the named host was not refused");
    assert_eq!(find(OTHER).unwrap()["reason"], "NO_EGRESS_RULE");

    worker.stop().await;
}
