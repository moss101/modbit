//! Hosting one session under its lease (docs/33 "Cloud worker lifecycle"):
//! materialize the cloud log in the local Core, take the local session
//! lease, run queued tasks, execute relayed commands, mirror every local
//! event back to the cloud, renew the lease — and stop the moment the
//! lease cannot be renewed (another owner has it).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use modbit_domain::SessionId;
use modbit_event_store::cloud::{ClaimedLease, CloudError, CloudStore};
use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{self as wire, CommandEnvelope};
use prost::Message;
use serde_json::{Value, json};

use crate::core_process::CoreProcess;
use crate::{Config, Hosting, HostingMap};

/// Per-session progress the worker persists beside the Core's data.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct MirrorState {
    /// The cloud session offset imported into the local Core so far.
    imported_through: u64,
    /// The local store offset mirrored to the cloud so far.
    exported_through: u64,
}

fn state_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("mirror.json")
}

fn load_state(dir: &std::path::Path) -> MirrorState {
    std::fs::read(state_path(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_state(dir: &std::path::Path, st: &MirrorState) {
    if let Ok(bytes) = serde_json::to_vec(st) {
        let tmp = dir.join("mirror.json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, state_path(dir));
        }
    }
}

fn fresh_id() -> wire::Id {
    wire::Id {
        value: uuid::Uuid::now_v7().as_bytes().to_vec(),
    }
}

fn id_of(bytes: &[u8; 16]) -> wire::Id {
    wire::Id {
        value: bytes.to_vec(),
    }
}

fn envelope(command_type: &str, payload: Vec<u8>, generation: Option<u64>) -> CommandEnvelope {
    envelope_with_id(fresh_id(), command_type, payload, generation)
}

fn envelope_with_id(
    id: wire::Id,
    command_type: &str,
    payload: Vec<u8>,
    generation: Option<u64>,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(id),
        tenant_id: None,
        user_id: None,
        session_id: None,
        aggregate_id: None,
        expected_generation: generation,
        command_type: command_type.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

/// Why hosting ended.
#[derive(Debug)]
enum End {
    /// The worker was asked to stop: the lease is released, the session stays ready.
    Stopped,
    /// The lease could not be renewed or a mirror was refused as stale: fenced.
    Fenced,
    /// The Core or the store failed in a way this worker cannot mend.
    Failed(String),
}

/// Host `lease.session_id` until stopped or fenced.
pub(crate) async fn host(
    cfg: Arc<Config>,
    store: Arc<CloudStore>,
    lease: ClaimedLease,
    stop: tokio::sync::watch::Receiver<bool>,
    hosting: HostingMap,
) {
    let sid = lease.session_id;
    let generation = lease.generation;
    let record = |h: Hosting| {
        if let Ok(mut m) = hosting.lock() {
            m.insert(sid, h);
        }
    };
    let dir = cfg.data_dir.join("sessions").join(sid.to_string());
    // The heartbeat runs from the claim on, apart from the hosting itself:
    // spawning and materializing a Core must not cost the lease. A renewal
    // that is refused (the lease expired or another worker holds it) fences
    // this owner.
    let (fenced_tx, fenced) = tokio::sync::watch::channel(false);
    let heartbeat = tokio::spawn(heartbeat(
        Arc::clone(&cfg),
        Arc::clone(&store),
        lease.clone(),
        fenced_tx,
    ));
    let end = host_inner(&cfg, &store, &lease, &dir, stop, fenced).await;
    heartbeat.abort();
    match end {
        End::Stopped => {
            let _ = store
                .release_lease(sid, &cfg.worker_id, generation, true)
                .await;
            eprintln!(
                "modbit-cloud-worker[{}]: released session {sid}",
                cfg.worker_id
            );
            record(Hosting::Released { generation });
        }
        End::Fenced => {
            eprintln!(
                "modbit-cloud-worker[{}]: fenced on session {sid} (generation {generation}); stopped",
                cfg.worker_id
            );
            record(Hosting::Fenced { generation });
        }
        End::Failed(why) => {
            eprintln!(
                "modbit-cloud-worker[{}]: session {sid}: {why}; released",
                cfg.worker_id
            );
            let _ = store
                .release_lease(sid, &cfg.worker_id, generation, true)
                .await;
            record(Hosting::Released { generation });
        }
    }
}

async fn heartbeat(
    cfg: Arc<Config>,
    store: Arc<CloudStore>,
    lease: ClaimedLease,
    fenced: tokio::sync::watch::Sender<bool>,
) {
    let every = cfg.lease_ttl / 3;
    loop {
        tokio::time::sleep(every).await;
        match store
            .renew_lease(
                lease.session_id,
                &cfg.worker_id,
                lease.generation,
                cfg.lease_ttl.as_millis() as i64,
            )
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                let _ = fenced.send(true);
                return;
            }
            Err(e) => eprintln!("modbit-cloud-worker[{}]: renew: {e}", cfg.worker_id),
        }
    }
}

async fn host_inner(
    cfg: &Config,
    store: &CloudStore,
    lease: &ClaimedLease,
    dir: &std::path::Path,
    mut stop: tokio::sync::watch::Receiver<bool>,
    mut fenced: tokio::sync::watch::Receiver<bool>,
) -> End {
    let sid = lease.session_id;
    let tenant = lease.tenant_id;
    let core = match CoreProcess::spawn(&cfg.core_bin, dir, tenant) {
        Ok(c) => c,
        Err(e) => return End::Failed(format!("core: {e}")),
    };
    let mut c = match core.client().await {
        Ok(c) => c,
        Err(e) => return End::Failed(format!("connect: {e}")),
    };
    // 0. The provider, into the Core's memory only (not journaled there).
    if let Some(p) = &cfg.provider {
        let ack = c
            .command(envelope(
                "ConfigureProvider",
                wire::ConfigureProvider {
                    provider: p.provider.clone(),
                    api_key: p.api_key.clone(),
                    base_url: p.base_url.clone(),
                }
                .encode_to_vec(),
                None,
            ))
            .await;
        if let Err(e) = ack {
            return End::Failed(format!("provider: {e}"));
        }
    }
    // 0b. The Sandbox Gateway (M8.5), with the cloud lease generation the
    // gateway checks against the store; the token stays in memory.
    if let Some(g) = &cfg.sandbox_gateway {
        let ack = c
            .command(envelope(
                "ConfigureSandboxGateway",
                wire::ConfigureSandboxGateway {
                    base_url: g.base_url.clone(),
                    worker_token: g.worker_token.clone(),
                    worker_id: cfg.worker_id.clone(),
                    tenant_id: tenant.to_string(),
                    lease_generation: lease.generation,
                }
                .encode_to_vec(),
                None,
            ))
            .await;
        if let Err(e) = ack {
            return End::Failed(format!("sandbox gateway: {e}"));
        }
    }
    let mut st = load_state(dir);
    // 1. The cloud log into the local Core (verbatim, chain-checked there).
    if let Err(e) = import_cloud_events(store, &mut c, tenant, sid, &mut st, dir).await {
        return End::Failed(format!("import: {e}"));
    }
    // 2. The local session lease: this worker is the session's owner here too.
    let ack = c
        .command(envelope(
            "AcquireSessionLease",
            wire::AcquireSessionLease {
                session_id: Some(id_of(sid.as_bytes())),
                owner: format!(
                    "modbit-cloud-worker {} (cloud lease {})",
                    cfg.worker_id, lease.generation
                ),
            }
            .encode_to_vec(),
            None,
        ))
        .await;
    let local_generation = match ack.map(|a| Client::result::<wire::SessionLeaseAcquired>(&a)) {
        Ok(Ok(l)) => l.lease_generation,
        Ok(Err(e)) | Err(e) => return End::Failed(format!("local lease: {e}")),
    };
    // 3. The loop.
    let mut started: HashSet<[u8; 16]> = HashSet::new();
    loop {
        if *fenced.borrow() {
            core.stop();
            return End::Fenced;
        }
        // Mirror what the Core recorded since the last export.
        match mirror_local_events(store, &mut c, cfg, lease, &mut st, dir).await {
            Ok(()) => {}
            Err(CloudError::StaleLease { .. }) => {
                core.stop();
                return End::Fenced;
            }
            Err(e) => eprintln!("modbit-cloud-worker[{}]: mirror: {e}", cfg.worker_id),
        }
        // Relayed commands: executed here, mirrored, then completed — a
        // caller that sees the command's outcome sees its events on the
        // cloud log too.
        let mut completions = Vec::new();
        match store.pending_commands(tenant, sid, 50).await {
            Ok(pending) => {
                for (command_id, kind, body) in pending {
                    let (status, code, result) =
                        execute_relayed(&mut c, sid, local_generation, command_id, &kind, &body)
                            .await;
                    completions.push((command_id, status, code, result));
                }
            }
            Err(e) => eprintln!("modbit-cloud-worker[{}]: pending: {e}", cfg.worker_id),
        }
        if !completions.is_empty() {
            match mirror_local_events(store, &mut c, cfg, lease, &mut st, dir).await {
                Ok(()) => {}
                Err(CloudError::StaleLease { .. }) => {
                    core.stop();
                    return End::Fenced;
                }
                Err(e) => eprintln!("modbit-cloud-worker[{}]: mirror: {e}", cfg.worker_id),
            }
            for (command_id, status, code, result) in completions {
                if let Err(e) = store
                    .complete_command(tenant, command_id, status, &code, result)
                    .await
                {
                    eprintln!(
                        "modbit-cloud-worker[{}]: complete {command_id}: {e}",
                        cfg.worker_id
                    );
                }
            }
        }
        // Queued tasks run.
        if let Err(e) = start_queued(&mut c, cfg, sid, local_generation, &mut started).await {
            eprintln!("modbit-cloud-worker[{}]: start: {e}", cfg.worker_id);
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(400)) => {}
            _ = fenced.changed() => {}
            _ = stop.changed() => {
                if *stop.borrow() {
                    // A last mirror so nothing recorded here is lost to the next owner.
                    let _ = mirror_local_events(store, &mut c, cfg, lease, &mut st, dir).await;
                    core.stop();
                    return End::Stopped;
                }
            }
        }
    }
}

async fn import_cloud_events(
    store: &CloudStore,
    c: &mut Client,
    tenant: modbit_domain::TenantId,
    sid: SessionId,
    st: &mut MirrorState,
    dir: &std::path::Path,
) -> anyhow::Result<()> {
    loop {
        let batch = store
            .events_after(tenant, sid, st.imported_through, 500)
            .await?;
        if batch.is_empty() {
            return Ok(());
        }
        let last = batch
            .last()
            .map(|e| e.session_offset)
            .unwrap_or(st.imported_through);
        let events: Vec<wire::MirroredEvent> = batch
            .iter()
            .map(|e| wire::MirroredEvent {
                envelope_json: serde_json::to_string(&e.envelope).unwrap_or_default(),
                payload_json: e.payload.to_string(),
            })
            .collect();
        let ack = c
            .command(envelope(
                "ImportMirroredEvents",
                wire::ImportMirroredEvents { events }.encode_to_vec(),
                None,
            ))
            .await?;
        let imported: wire::MirroredEventsImported = Client::result(&ack)?;
        // Everything the Core already had counts as imported; the export
        // cursor moves past it so the cloud's own events are not sent back.
        st.imported_through = last;
        if imported.last_offset > st.exported_through && imported.imported > 0 {
            st.exported_through = imported.last_offset;
        }
        save_state(dir, st);
    }
}

async fn mirror_local_events(
    store: &CloudStore,
    c: &mut Client,
    cfg: &Config,
    lease: &ClaimedLease,
    st: &mut MirrorState,
    dir: &std::path::Path,
) -> Result<(), CloudError> {
    loop {
        let ack = c
            .command(envelope(
                "ReadMirrorEvents",
                wire::ReadMirrorEvents {
                    session_id: Some(id_of(lease.session_id.as_bytes())),
                    after_offset: st.exported_through,
                    limit: 500,
                }
                .encode_to_vec(),
                None,
            ))
            .await
            .map_err(|e| CloudError::Object(format!("read mirror events: {e}")))?;
        let batch: wire::MirrorEvents = Client::result(&ack)
            .map_err(|e| CloudError::Object(format!("read mirror events: {e}")))?;
        if batch.events.is_empty() {
            return Ok(());
        }
        let mut events = Vec::with_capacity(batch.events.len());
        for e in &batch.events {
            let env: modbit_domain::event::EventEnvelope = serde_json::from_str(&e.envelope_json)?;
            let payload: Value = serde_json::from_str(&e.payload_json)?;
            events.push((env, payload));
        }
        let last = batch.offsets.last().copied().unwrap_or(st.exported_through);
        store
            .mirror(
                lease.tenant_id,
                lease.session_id,
                &cfg.worker_id,
                lease.generation,
                events,
            )
            .await?;
        st.exported_through = last;
        save_state(dir, st);
    }
}

/// Execute one relayed command on the local Core; the outcome for the ledger.
async fn execute_relayed(
    c: &mut Client,
    sid: SessionId,
    generation: u64,
    command_id: uuid::Uuid,
    kind: &str,
    body: &Value,
) -> (&'static str, String, Value) {
    let rejected = |code: &str, message: String| {
        (
            "REJECTED",
            code.to_owned(),
            json!({"code": code, "message": message, "status": 409}),
        )
    };
    let outcome = match kind {
        "CreateTask" => {
            // The relayed command's id names the task, as the Core's rule does.
            let env = envelope_with_id(
                id_of(command_id.as_bytes()),
                "CreateTask",
                wire::CreateTask {
                    session_id: Some(id_of(sid.as_bytes())),
                    goal_text: body["goal_text"].as_str().unwrap_or_default().to_owned(),
                    workspace_id: None,
                    execution_profile: body["execution_profile"]
                        .as_str()
                        .unwrap_or("cloud_isolated")
                        .to_owned(),
                    origin: "desktop".into(),
                    workspace_root: body["workspace_root"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    issue_url: String::new(),
                }
                .encode_to_vec(),
                Some(generation),
            );
            c.command(env).await.and_then(|a| Client::result::<wire::TaskCreated>(&a).map(|t| json!({"task_id": t.task_id.map(|i| hex::encode(i.value)), "offset": t.offset, "state": "QUEUED"})))
        }
        "Task:steer" => {
            let task_id = body["task_id"]
                .as_str()
                .and_then(|s| modbit_domain::TaskId::parse(s).ok());
            let Some(task_id) = task_id else {
                return rejected("BAD_PAYLOAD", "task_id".into());
            };
            c.command(envelope(
                "QueueInput",
                wire::QueueInput {
                    task_id: Some(id_of(task_id.as_bytes())),
                    input_id: command_id.to_string(),
                    mode: "STEER".into(),
                    text: body["text"].as_str().unwrap_or_default().to_owned(),
                }
                .encode_to_vec(),
                Some(generation),
            ))
            .await
            .and_then(|a| Client::result::<wire::InputQueued>(&a).map(|q| json!({"task_id": task_id.to_string(), "sequence": q.sequence, "outcome": "queued", "event_type": "TaskInputQueued"})))
        }
        "Task:cancel" => {
            let task_id = body["task_id"]
                .as_str()
                .and_then(|s| modbit_domain::TaskId::parse(s).ok());
            let Some(task_id) = task_id else {
                return rejected("BAD_PAYLOAD", "task_id".into());
            };
            c.command(envelope(
                "CancelTask",
                wire::CancelTask {
                    task_id: Some(id_of(task_id.as_bytes())),
                }
                .encode_to_vec(),
                Some(generation),
            ))
            .await
            .and_then(|a| Client::result::<wire::TaskCancelRequested>(&a).map(|r| json!({"task_id": task_id.to_string(), "outcome": if r.was_running { "cancel requested at the next safe boundary" } else { "cancelled" }, "was_running": r.was_running})))
        }
        "Task:pause" | "Task:resume" => {
            // The Core has no pause (PX-024): the request is honestly refused.
            return rejected(
                "PAUSE_UNSUPPORTED",
                "the execution owner has no pause; cancel (confirmed) or steer".into(),
            );
        }
        k if k.starts_with("Approval:") => {
            let approve = k == "Approval:approve";
            let approval_id = body["approval_id"]
                .as_str()
                .and_then(|s| modbit_domain::ApprovalId::parse(s).ok());
            let Some(approval_id) = approval_id else {
                return rejected("BAD_PAYLOAD", "approval_id".into());
            };
            c.command(envelope(
                "ResolveApproval",
                wire::ResolveApproval {
                    approval_id: Some(id_of(approval_id.as_bytes())),
                    approve,
                    reason: body["reason"].as_str().unwrap_or_default().to_owned(),
                    intent_hash: body["intent_hash"].as_str().unwrap_or_default().to_owned(),
                }
                .encode_to_vec(),
                Some(generation),
            ))
            .await
            .and_then(|a| {
                Client::result::<wire::ApprovalResolvedAck>(&a)
                    .map(|r| json!({"approval_id": approval_id.to_string(), "status": r.status}))
            })
        }
        other => {
            return rejected(
                "UNKNOWN_COMMAND",
                format!("`{other}` is not a command this owner runs"),
            );
        }
    };
    match outcome {
        Ok(v) => ("ACCEPTED", "OK".to_owned(), v),
        Err(ClientError::Rejected { code, message, .. }) => {
            let result = json!({"code": code, "message": message, "status": 409});
            ("REJECTED", code, result)
        }
        Err(e) => (
            "REJECTED",
            "OWNER_ERROR".to_owned(),
            json!({"code": "OWNER_ERROR", "message": e.to_string(), "status": 500}),
        ),
    }
}

/// Every queued task of the session is started once (a refusal is retried
/// on the next pass; a task the Core refuses for good is logged).
async fn start_queued(
    c: &mut Client,
    cfg: &Config,
    sid: SessionId,
    generation: u64,
    started: &mut HashSet<[u8; 16]>,
) -> anyhow::Result<()> {
    let ack = c
        .command(envelope(
            "GetSessionSnapshot",
            wire::GetSessionSnapshot {
                session_id: Some(id_of(sid.as_bytes())),
            }
            .encode_to_vec(),
            None,
        ))
        .await?;
    let snap: wire::SessionSnapshot = Client::result(&ack)?;
    for t in snap.tasks {
        let Some(id) = t
            .task_id
            .as_ref()
            .and_then(|i| <[u8; 16]>::try_from(i.value.as_slice()).ok())
        else {
            continue;
        };
        if t.state != "Queued" || started.contains(&id) {
            continue;
        }
        let mut trusted_once = false;
        loop {
            let ack = c
                .command(envelope(
                    "StartTask",
                    wire::StartTask {
                        task_id: Some(id_of(&id)),
                        endpoint: cfg.endpoint.clone().unwrap_or_default(),
                        model: cfg.model.clone().unwrap_or_default(),
                        max_turns: 0,
                        max_tool_calls: 0,
                        max_no_progress_turns: 0,
                        skills: vec![],
                    }
                    .encode_to_vec(),
                    Some(generation),
                ))
                .await;
            match ack.map(|a| Client::result::<wire::TaskRunStarted>(&a)) {
                Ok(Ok(_)) => {
                    started.insert(id);
                }
                Ok(Err(ClientError::Rejected { code, message, .. }))
                | Err(ClientError::Rejected { code, message, .. }) => {
                    if code == "CAPACITY_EXHAUSTED" {
                        break;
                    }
                    // The workspace a principal named for the task through
                    // the API is the tenant's own choice: this Core trusts
                    // it for the session once (journaled, so the successor
                    // inherits the trust from the mirrored log).
                    if code == "REPOSITORY_UNTRUSTED"
                        && !trusted_once
                        && !t.workspace_root.is_empty()
                    {
                        trusted_once = true;
                        c.command(envelope(
                            "TrustRepository",
                            wire::TrustRepository {
                                session_id: Some(id_of(sid.as_bytes())),
                                workspace_root: t.workspace_root.clone(),
                                scope: "repository".into(),
                            }
                            .encode_to_vec(),
                            Some(generation),
                        ))
                        .await?;
                        continue;
                    }
                    started.insert(id);
                    eprintln!(
                        "modbit-cloud-worker[{}]: task {} not started: {code}: {message}",
                        cfg.worker_id,
                        hex::encode(id)
                    );
                }
                Ok(Err(e)) | Err(e) => return Err(e.into()),
            }
            break;
        }
    }
    Ok(())
}
