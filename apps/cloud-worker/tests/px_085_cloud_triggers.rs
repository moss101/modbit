//! PX-085 (QUAL-PX-085, docs/68 AUT-B01, AUT-B03, AUT-B05, AUT-C01, AUT-D07),
//! the worker half: the cloud time source and the budgets of a started task.
//!
//! * The schedule is evaluated by the worker's claim loop against the
//!   database clock (an injectable seam for tests). Workers racing on a slot
//!   produce one firing; a missed window follows the definition's policy
//!   once and is never replayed in bulk; the pauses stop it; a process that
//!   dies between recording a firing and creating its task loses nothing and
//!   duplicates nothing.
//! * A signed webhook becomes a task a real worker starts with a real
//!   `modbit-core` under the definition's limits, not the defaults.
//!
//! Real: Postgres (`MODBIT_CLOUD_TEST_DATABASE_URL`), the Cloud API, the
//! worker and its claim loop, `modbit-core`, git. Stand-in: the model (a
//! scripted OpenAI-compatible server). The loop test needs
//! `MODBIT_CLOUD_AUTOMATION_TEST_CLOCK=1` (the offset seam is ignored
//! otherwise) and says so when it is unset.

mod px_cloud_common;

use std::time::Duration;

use modbit_automation::webhook;
use modbit_cloud_api::Extras;
use modbit_cloud_worker::{automation_tick, start};
use modbit_event_store::cloud::automation::{ScheduleDecision, ScheduleFire};
use modbit_event_store::cloud::{CloudStore, CloudStoreConfig};
use px_cloud_common::*;
use serde_json::{Value, json};

const MASTER: &[u8] = b"px085-worker-master-key-0123456789abcdef";
const HOUR: i64 = 3_600_000;

async fn api_with_admin(
    store_cfg: &CloudStoreConfig,
    name: &str,
) -> (Api, modbit_domain::TenantId, String) {
    let api = Api::start_with(
        store_cfg,
        None,
        Extras::default().with_webhook_master_key(MASTER),
    )
    .await;
    let store = &api.served.state.store;
    let t = store.create_tenant(name).await.unwrap();
    let (_p, secret) = store
        .create_principal_with(t, "user", name, "admin", None)
        .await
        .unwrap();
    let (_, tok) = api
        .post("", "/v1/auth/token", json!({"secret": secret}))
        .await;
    (api, t, tok["access_token"].as_str().unwrap().to_owned())
}

fn sched_def(name: &str, tweak: impl FnOnce(&mut Value)) -> Value {
    let mut d = json!({
        "schema": "modbit.automation/1", "name": name, "prompt": "Report whether the default branch moved.",
        "triggers": [{"kind": "schedule", "id": "hourly", "cron": "0 * * * *"}],
        "concurrency": {"policy": "replace"},
        "budget": {"max_runs_per_hour": 60},
        "limits": {"max_turns": 5, "max_tool_calls": 9, "deadline_minutes": 2, "max_cost_minor": 100},
    });
    tweak(&mut d);
    d
}

/// Create and approve a (read-only) definition; its id.
async fn define(api: &Api, token: &str, def: Value, root: Option<&str>) -> String {
    let mut body = json!({"definition": def});
    if let Some(r) = root {
        body["workspace_root"] = json!(r);
    }
    let (s, created) = api.post(token, "/v1/automations", body).await;
    assert_eq!(s, 201, "{created}");
    let id = created["automation_id"].as_str().unwrap().to_owned();
    let (s, r) = api
        .post(
            token,
            &format!("/v1/automations/{id}:enable"),
            json!({"version": 1, "definition_hash": created["definition_hash"], "effects": "read_only", "capabilities": [], "paths": [], "hosts": []}),
        )
        .await;
    assert_eq!(s, 200, "{r}");
    id
}

async fn sql(cfg: &CloudStoreConfig, query: &str) -> Vec<tokio_postgres::Row> {
    let c: tokio_postgres::Config = cfg.database_url.parse().unwrap();
    let (client, conn) = c.connect(tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client.query(query, &[]).await.unwrap()
}

async fn count(cfg: &CloudStoreConfig, query: &str) -> i64 {
    sql(cfg, query).await[0].get::<_, i64>(0)
}

async fn next_due(cfg: &CloudStoreConfig, id: &str) -> i64 {
    sql(
        cfg,
        &format!("SELECT next_due_ms FROM automation_schedule WHERE automation_id = '{id}'"),
    )
    .await[0]
        .get::<_, i64>(0)
}

#[tokio::test]
async fn qual_px_085_a_schedule_fires_exactly_once_per_slot_with_workers_racing_on_a_controlled_clock()
 {
    let Some(cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    let (api, _t, a) = api_with_admin(&cfg, "px085-race").await;
    let id = define(&api, &a, sched_def("nightly", |_| {}), None).await;
    let due = next_due(&cfg, &id).await;
    assert_eq!(due % HOUR, 0, "an hourly cron's slot is on the hour");

    // Three "workers" (their own pools) and many racers each: one firing.
    let stores = [
        CloudStore::connect(&cfg).await.unwrap(),
        CloudStore::connect(&cfg).await.unwrap(),
        CloudStore::connect(&cfg).await.unwrap(),
    ];
    let now = due + 2_000;
    let ticks: Vec<_> = (0..12)
        .map(|i| automation_tick(&stores[i % 3], now))
        .collect();
    for r in futures_util::future::join_all(ticks).await {
        r.unwrap();
    }
    let slot = format!("slot:hourly:{due}");
    assert_eq!(
        count(
            &cfg,
            &format!("SELECT count(*) FROM automation_firings WHERE event_id = '{slot}'")
        )
        .await,
        1,
        "one firing for the slot"
    );
    assert_eq!(
        count(&cfg, "SELECT count(*) FROM tasks").await,
        1,
        "one task"
    );
    assert_eq!(count(&cfg, "SELECT count(*) FROM sessions").await, 1);
    // The same instant again, from every worker: nothing more.
    for s in &stores {
        automation_tick(s, now).await.unwrap();
    }
    assert_eq!(count(&cfg, "SELECT count(*) FROM tasks").await, 1);
    // The cursor moved and the next slot is persisted.
    assert_eq!(next_due(&cfg, &id).await, due + HOUR);

    // The next slot fires once more (the replace policy cancels the first run).
    let now2 = due + HOUR + 3_000;
    let ticks: Vec<_> = (0..12)
        .map(|i| automation_tick(&stores[i % 3], now2))
        .collect();
    for r in futures_util::future::join_all(ticks).await {
        r.unwrap();
    }
    assert_eq!(
        count(
            &cfg,
            "SELECT count(*) FROM automation_firings WHERE trigger_kind = 'schedule'"
        )
        .await,
        2
    );
    assert_eq!(count(&cfg, "SELECT count(*) FROM tasks").await, 2);
    // The history says so, with the schedule's slot ids.
    let (_, runs) = api.get(&a, &format!("/v1/automations/{id}/runs")).await;
    let ids: Vec<&str> = runs["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["event_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&slot.as_str()), "{ids:?}");
    assert!(runs["runs"][0]["slot_ms"].is_i64());
    // An edit disables the definition and its schedule: later slots fire nothing.
    let (st, _) = api
        .post(
            &a,
            &format!("/v1/automations/{id}:update"),
            json!({"definition": sched_def("nightly", |d| d["prompt"] = json!("Changed."))}),
        )
        .await;
    assert_eq!(st, 200);
    automation_tick(&stores[0], due + 10 * HOUR).await.unwrap();
    assert_eq!(
        count(&cfg, "SELECT count(*) FROM tasks").await,
        2,
        "an unapproved version never fires"
    );
}

#[tokio::test]
async fn qual_px_085_a_missed_window_is_skipped_or_caught_up_once_and_never_replayed_in_bulk() {
    let Some(cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    let (api, _t, a) = api_with_admin(&cfg, "px085-missed").await;
    let skip = define(&api, &a, sched_def("skipper", |_| {}), None).await;
    let once = define(
        &api,
        &a,
        sched_def("catcher", |d| {
            d["missed"] = json!({"policy": "run_once", "catch_up_window_minutes": 600})
        }),
        None,
    )
    .await;
    let store = CloudStore::connect(&cfg).await.unwrap();
    let due = next_due(&cfg, &skip).await;
    // Six hourly slots pass while "the worker was down"; it comes back mid-hour.
    let back = due + 5 * HOUR + 30 * 60_000;
    automation_tick(&store, back).await.unwrap();
    // skip: nothing runs; the window is recorded once.
    let skipped = sql(
        &cfg,
        &format!("SELECT reason, missed FROM automation_firings WHERE automation_id = '{skip}'"),
    )
    .await;
    assert_eq!(skipped.len(), 1, "one record, not six");
    assert_eq!(skipped[0].get::<_, String>(0), "MISSED");
    assert_eq!(skipped[0].get::<_, i64>(1), 6);
    assert_eq!(count(&cfg, &format!("SELECT count(*) FROM tasks t JOIN automation_firings f ON f.task_id = t.task_id WHERE f.automation_id = '{skip}'")).await, 0);
    // run_once: exactly one catch-up run for the whole window, plus the record.
    let fired = sql(
        &cfg,
        &format!("SELECT catch_up, status, missed FROM automation_firings WHERE automation_id = '{once}' AND reason = '' ORDER BY fired_ms"),
    )
    .await;
    assert_eq!(fired.len(), 1, "one catch-up, never six");
    assert!(fired[0].get::<_, bool>(0));
    assert_eq!(fired[0].get::<_, String>(1), "RUNNING");
    assert_eq!(fired[0].get::<_, i64>(2), 5);
    assert_eq!(count(&cfg, "SELECT count(*) FROM tasks").await, 1);
    let missed = count(&cfg, &format!("SELECT count(*) FROM automation_firings WHERE automation_id = '{once}' AND reason = 'MISSED'")).await;
    assert_eq!(missed, 1);
    // Evaluating again at the same instant changes nothing; the next on-time
    // slot after the return fires once.
    automation_tick(&store, back).await.unwrap();
    assert_eq!(
        count(&cfg, "SELECT count(*) FROM automation_firings").await,
        3
    );
    assert_eq!(next_due(&cfg, &skip).await, due + 6 * HOUR);
    automation_tick(&store, due + 6 * HOUR + 1_000)
        .await
        .unwrap();
    assert_eq!(count(&cfg, &format!("SELECT count(*) FROM automation_firings WHERE automation_id = '{skip}' AND reason = ''")).await, 1);
    // Past the catch-up bound a window is skipped, not caught up: a daily
    // schedule down for two days, back five hours after its last slot, with a
    // one-hour bound.
    let daily = define(
        &api,
        &a,
        sched_def("daily", |d| {
            d["triggers"] = json!([{"kind": "schedule", "id": "hourly", "cron": "0 3 * * *"}]);
            d["missed"] = json!({"policy": "run_once", "catch_up_window_minutes": 60});
        }),
        None,
    )
    .await;
    let d0 = next_due(&cfg, &daily).await;
    let tasks_before = count(&cfg, "SELECT count(*) FROM tasks").await;
    automation_tick(&store, d0 + 2 * 24 * HOUR + 5 * HOUR)
        .await
        .unwrap();
    let rows = sql(
        &cfg,
        &format!("SELECT reason, catch_up FROM automation_firings WHERE automation_id = '{daily}'"),
    )
    .await;
    assert_eq!(rows.len(), 1, "one record for the window");
    assert_eq!(rows[0].get::<_, String>(0), "MISSED");
    assert!(!rows[0].get::<_, bool>(1));
    // (The hourly definitions kept firing their own slots; the daily one made no task.)
    assert!(count(&cfg, "SELECT count(*) FROM tasks").await >= tasks_before);
    assert_eq!(
        count(&cfg, &format!("SELECT count(*) FROM tasks t JOIN automation_firings f ON f.task_id = t.task_id WHERE f.automation_id = '{daily}'")).await,
        0
    );
}

#[tokio::test]
async fn qual_px_085_a_paused_automation_tenant_or_platform_fires_nothing_on_schedule() {
    let Some(cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    let (api, _t, a) = api_with_admin(&cfg, "px085-paused").await;
    let id = define(&api, &a, sched_def("pausable", |_| {}), None).await;
    let store = CloudStore::connect(&cfg).await.unwrap();
    let due = next_due(&cfg, &id).await;
    // Per-automation pause.
    let (st, _) = api
        .post(
            &a,
            &format!("/v1/automations/{id}:pause"),
            json!({"paused": true}),
        )
        .await;
    assert_eq!(st, 200);
    automation_tick(&store, due + 1_000).await.unwrap();
    assert_eq!(count(&cfg, "SELECT count(*) FROM tasks").await, 0);
    let r = sql(&cfg, "SELECT status, reason FROM automation_firings").await;
    assert_eq!(
        (r[0].get::<_, String>(0), r[0].get::<_, String>(1)),
        ("SKIPPED".into(), "PAUSED".into())
    );
    // Tenant pause.
    api.post(
        &a,
        &format!("/v1/automations/{id}:pause"),
        json!({"paused": false}),
    )
    .await;
    api.post(&a, "/v1/automations/pause", json!({"paused": true}))
        .await;
    automation_tick(&store, due + HOUR + 1_000).await.unwrap();
    assert_eq!(count(&cfg, "SELECT count(*) FROM tasks").await, 0);
    // Platform pause.
    api.post(&a, "/v1/automations/pause", json!({"paused": false}))
        .await;
    store
        .set_automation_switch("global", true, "maintenance", "test")
        .await
        .unwrap();
    automation_tick(&store, due + 2 * HOUR + 1_000)
        .await
        .unwrap();
    assert_eq!(count(&cfg, "SELECT count(*) FROM tasks").await, 0);
    assert_eq!(
        count(
            &cfg,
            "SELECT count(*) FROM automation_firings WHERE reason = 'PAUSED'"
        )
        .await,
        3
    );
    // Lifted, the next slot fires.
    store
        .set_automation_switch("global", false, "", "test")
        .await
        .unwrap();
    automation_tick(&store, due + 3 * HOUR + 1_000)
        .await
        .unwrap();
    assert_eq!(count(&cfg, "SELECT count(*) FROM tasks").await, 1);
}

#[tokio::test]
async fn qual_px_085_a_process_that_dies_between_recording_a_firing_and_creating_its_task_loses_nothing_and_duplicates_nothing()
 {
    let Some(cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    let (api, _t, a) = api_with_admin(&cfg, "px085-crash").await;
    let id = define(&api, &a, sched_def("crashy", |_| {}), None).await;
    let doomed = CloudStore::connect(&cfg).await.unwrap();
    let due = next_due(&cfg, &id).await;
    // The doomed worker records the slot (the firing and the cursor commit
    // together) and dies before it creates the task.
    let report = doomed
        .automation_schedule_tick(due + 1_000, |_| ScheduleDecision {
            fires: vec![ScheduleFire {
                slot_ms: due,
                catch_up: false,
                missed_before: 0,
            }],
            skipped: None,
            through_ms: due,
            next_due_ms: Some(due + HOUR),
        })
        .await
        .unwrap();
    assert_eq!(report.pending.len(), 1);
    drop(doomed);
    assert_eq!(
        count(&cfg, "SELECT count(*) FROM tasks").await,
        0,
        "died before the task"
    );
    assert_eq!(
        sql(&cfg, "SELECT status FROM automation_firings").await[0].get::<_, String>(0),
        "PENDING"
    );
    // Its slot is spent: the cursor moved, a later pass does not fire it again.
    assert_eq!(next_due(&cfg, &id).await, due + HOUR);
    // Survivors finish it, racing, exactly once - the pass before the grace
    // period leaves it alone (the first owner may still be finishing).
    let survivors = [
        CloudStore::connect(&cfg).await.unwrap(),
        CloudStore::connect(&cfg).await.unwrap(),
    ];
    automation_tick(&survivors[0], due + 2_000).await.unwrap();
    assert_eq!(
        count(&cfg, "SELECT count(*) FROM tasks").await,
        0,
        "inside the grace period"
    );
    let later = due + 30_000;
    let ticks: Vec<_> = (0..8)
        .map(|i| automation_tick(&survivors[i % 2], later))
        .collect();
    for r in futures_util::future::join_all(ticks).await {
        r.unwrap();
    }
    assert_eq!(
        count(&cfg, "SELECT count(*) FROM tasks").await,
        1,
        "no loss"
    );
    assert_eq!(
        count(&cfg, "SELECT count(*) FROM sessions").await,
        1,
        "no duplicate"
    );
    assert_eq!(
        count(
            &cfg,
            "SELECT count(*) FROM automation_firings WHERE status = 'RUNNING'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &cfg,
            "SELECT count(*) FROM events WHERE event_type = 'TaskCreated'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &cfg,
            "SELECT count(*) FROM events WHERE event_type = 'TaskTriggeredByAutomation'"
        )
        .await,
        1
    );
    // Direct racing dispatches of the same key are one task too.
    let key = sql(&cfg, "SELECT dispatch_key FROM automation_firings").await[0].get::<_, String>(0);
    let races: Vec<_> = (0..6)
        .map(|i| survivors[i % 2].dispatch_firing(&key))
        .collect();
    for r in futures_util::future::join_all(races).await {
        r.unwrap();
    }
    assert_eq!(
        count(
            &cfg,
            "SELECT count(*) FROM events WHERE event_type = 'TaskCreated'"
        )
        .await,
        1
    );
}

/// A signed webhook for `endpoint`.
async fn deliver(
    api: &Api,
    endpoint: &str,
    secret: &str,
    body: &Value,
    nonce: &str,
) -> (u16, Value) {
    let bytes = serde_json::to_vec(body).unwrap();
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

#[tokio::test]
async fn qual_px_085_a_webhook_task_starts_on_a_real_worker_and_core_under_the_definitions_limits()
{
    let Some(cfg) = store_config().await else {
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
    let root = repo(&data.join("repo"));
    let (api, _t, a) = api_with_admin(&cfg, "px085-budgets").await;
    // One turn is all the definition allows; the model would take many.
    let def = json!({
        "schema": "modbit.automation/1", "name": "budgeted", "prompt": "Look around and keep working.",
        "triggers": [{"kind": "webhook", "id": "ping", "name": "ping"}],
        "limits": {"max_turns": 1, "max_tool_calls": 9, "deadline_minutes": 3, "max_cost_minor": 250},
    });
    let id = define(&api, &a, def, Some(&root)).await;
    let (st, ep) = api
        .post(
            &a,
            &format!("/v1/automations/{id}/endpoints"),
            json!({"trigger_id": "ping"}),
        )
        .await;
    assert_eq!(st, 201, "{ep}");
    let (endpoint, secret) = (
        ep["endpoint_id"].as_str().unwrap().to_owned(),
        ep["secret"].as_str().unwrap().to_owned(),
    );
    let script: Vec<Value> = (0..8)
        .map(|_| json!({"calls": [{"name": "plan.update", "args": {"outcome": "still working", "expected_files": []}}]}))
        .collect();
    let (model, _seen) = scripted_model(script).await;
    let (st, fired) = deliver(&api, &endpoint, &secret, &json!({"hello": "world"}), "b-1").await;
    assert_eq!(st, 201, "{fired}");
    let (session, task) = (
        fired["session_id"].as_str().unwrap().to_owned(),
        fired["task_id"].as_str().unwrap().to_owned(),
    );

    let worker = start(worker_config(
        &cfg,
        "worker-px085",
        &data.join("w"),
        &model,
        None,
    ))
    .await
    .expect("worker");
    // The worker claims the run's own session and starts the task; the
    // definition's limits are on its log and the run stops at its one turn.
    let exhausted = until("the one-turn budget to be exhausted", 120, async || {
        let evs = api.events(&a, &session).await;
        evs.iter()
            .find(|(t, _)| t == "HarnessBudgetExhausted")
            .map(|(_, p)| p.clone())
    })
    .await;
    assert_eq!(exhausted["budget"], "max_turns", "{exhausted}");
    assert_eq!(
        exhausted["limit"], 1,
        "the definition's max_turns reached the run: {exhausted}"
    );
    let evs = api.events(&a, &session).await;
    let budgets = evs
        .iter()
        .find(|(t, _)| t == "TaskBudgetsSet")
        .expect("budgets on the task's log")
        .1
        .clone();
    assert_eq!(budgets["max_cost_minor"], 250, "{budgets}");
    assert_eq!(budgets["max_wall_ms"], 180_000, "{budgets}");
    assert_eq!(budgets["forbid_spawn"], true);
    // The provenance and the payload document came through the worker's Core too.
    let types: Vec<&str> = evs.iter().map(|(t, _)| t.as_str()).collect();
    assert!(types.contains(&"TaskTriggeredByAutomation"), "{types:?}");
    assert!(types.contains(&"ContextDocumentAttached"), "{types:?}");
    let created = evs.iter().find(|(t, _)| t == "TaskCreated").unwrap();
    assert_eq!(created.1["origin"], "automation");
    assert_eq!(created.1["execution_profile"], "plan");
    let _ = task;
    // The run history reads the task's outcome.
    let (_, runs) = api.get(&a, &format!("/v1/automations/{id}/runs")).await;
    assert_eq!(runs["runs"][0]["budgets"]["max_turns"], 1);
    worker.stop().await;
}

#[tokio::test]
async fn qual_px_085_the_workers_claim_loop_is_the_time_evaluator_two_workers_one_firing_per_slot_and_the_task_runs()
 {
    let Some(cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    if std::env::var(modbit_event_store::cloud::automation::TEST_CLOCK_ENV).is_ok_and(|v| v == "1")
    {
        // The offset seam is honoured.
    } else {
        // Unset, the seam is (rightly) ignored and the loop cannot be driven
        // to an hourly slot: the CI cloud job sets the variable.
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_AUTOMATION_TEST_CLOCK=1 is required to drive the worker loop's clock"
        );
        return;
    }
    assert!(
        core_bin().exists(),
        "modbit-core at {}",
        core_bin().display()
    );
    let keep = tempfile::tempdir().unwrap();
    let data = keep.path().to_path_buf();
    let root = repo(&data.join("repo"));
    let (api, _t, a) = api_with_admin(&cfg, "px085-loop").await;
    let id = define(
        &api,
        &a,
        sched_def("looped", |d| {
            d["limits"]["max_turns"] = json!(8);
            // The scripted model has no price: a cost cap would stop the run (PX-116).
            d["limits"]
                .as_object_mut()
                .unwrap()
                .remove("max_cost_minor");
        }),
        Some(&root),
    )
    .await;
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "nothing to do", "expected_files": []}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "nothing to report", "self_review": {"findings": []}}}]}),
    ];
    let (model, _seen) = scripted_model(script).await;
    let due = next_due(&cfg, &id).await;
    let store = CloudStore::connect(&cfg).await.unwrap();
    let real = store.automation_now_ms().await.unwrap();
    // Two workers, each running the loop; the clock jumps to just past the slot.
    let w1 = start(worker_config(
        &cfg,
        "worker-px085-a",
        &data.join("wa"),
        &model,
        None,
    ))
    .await
    .unwrap();
    let w2 = start(worker_config(
        &cfg,
        "worker-px085-b",
        &data.join("wb"),
        &model,
        None,
    ))
    .await
    .unwrap();
    store
        .automation_set_clock_offset(due - real + 2_000)
        .await
        .unwrap();
    let run = until("the schedule's run to appear", 60, async || {
        let (_, r) = api.get(&a, &format!("/v1/automations/{id}/runs")).await;
        r["runs"].as_array().and_then(|x| x.first().cloned())
    })
    .await;
    assert_eq!(run["trigger_kind"], "schedule");
    assert_eq!(run["event_id"], format!("slot:hourly:{due}"));
    let session = run["session_id"].as_str().unwrap().to_owned();
    // The task runs to review on a worker, after a settle period no second firing appears.
    let mut last = String::new();
    for _ in 0..80 {
        let (_, v) = api.get(&a, &format!("/v1/sessions/{session}")).await;
        last = v["tasks"][0]["state"].to_string();
        if last.contains("READY_FOR_REVIEW") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let evs = api.events(&a, &session).await;
    let dump: Vec<String> = evs
        .iter()
        .map(|(t, p)| {
            format!(
                "{t} {}",
                p.to_string().chars().take(300).collect::<String>()
            )
        })
        .collect();
    assert!(
        last.contains("READY_FOR_REVIEW"),
        "task state {last}; events:\n{}",
        dump.join("\n")
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        count(
            &cfg,
            "SELECT count(*) FROM automation_firings WHERE trigger_kind = 'schedule'"
        )
        .await,
        1,
        "two workers polling every 300ms made one firing"
    );
    assert_eq!(count(&cfg, "SELECT count(*) FROM tasks").await, 1);
    w1.stop().await;
    w2.stop().await;
}
