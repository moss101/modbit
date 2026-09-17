//! A task's sandbox under `cloud_isolated` (M8.5; docs/21 "Sandbox
//! substrate boundary", docs/24 "Cloud Core Worker": the worker's Core
//! calls the Sandbox Gateway). The Core provisions one sandbox per task when
//! its run starts — for the tenant, session and task it serves, under the
//! worker's cloud session lease generation the gateway checks — journals
//! `SandboxLeaseAcquired` with the sandbox's identity (never a credential),
//! hands the handle to the tool host so the tools with a sandbox path act
//! inside it, and destroys it when the task ends (`SandboxReleased`).

use std::sync::Arc;

use modbit_domain::TaskId;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent, TaskState};
use modbit_event_store::AppendRequest;
use modbit_sandbox::client::{ProvisionRequest, SandboxHandle};
use modbit_sandbox::port::SandboxPort;

use crate::runtime::typed;
use crate::server::Core;

/// Provision the task's sandbox unless it has one; refuse when no gateway
/// is configured or the gateway refuses.
pub async fn ensure_for_task(
    core: &Arc<Core>,
    task: &Task,
    actor: &Actor,
) -> Result<Arc<SandboxHandle>, (String, String)> {
    if let Some(h) = core.tools.sandboxes.lock().await.get(&task.task_id) {
        return Ok(Arc::clone(h));
    }
    let Some(custody) = core.tools.sandbox_gateway.lock().await.clone() else {
        return Err((
            "NO_SANDBOX_GATEWAY".into(),
            "a cloud_isolated task needs a Sandbox Gateway; none is configured in this Core (ConfigureSandboxGateway)".into(),
        ));
    };
    let Some(root) = task.workspace_root.as_deref() else {
        return Err((
            "NO_WORKSPACE".into(),
            "a cloud_isolated task needs a workspace root to seed its sandbox".into(),
        ));
    };
    // M8.6: what the task's lease lets leave the sandbox — the configured
    // forge's API host when the lease carries `network.egress`, and the
    // forge token as a credentialed virtual host when it carries
    // `secret.use`; the token crosses to the gateway's broker once and
    // never enters the guest.
    let ops: Vec<String> = core
        .store
        .lock()
        .await
        .leases_for_task(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|l| l.is_valid(modbit_domain::Timestamp::now()))
        .flat_map(|l| l.operations)
        .collect();
    let mut egress = Vec::new();
    let mut credentials = Vec::new();
    if let Some(forge) = core.tools.forge.get() {
        let target = forge.egress_target();
        let (host, port) = target
            .rsplit_once(':')
            .map(|(h, p)| (h.to_owned(), p.parse::<u16>().unwrap_or(443)))
            .unwrap_or((target.clone(), 443));
        if ops.iter().any(|o| o == "network.egress") {
            egress.push(modbit_sandbox::policy::EgressRule {
                host,
                port,
                capability: "network.egress".into(),
            });
        }
        if ops.iter().any(|o| o == "secret.use")
            && let Some(token) = &forge.token
        {
            credentials.push((
                modbit_sandbox::policy::CredentialGrant {
                    handle: "forge-token".into(),
                    virtual_host: "forge.modbit.internal".into(),
                    target_url: forge.api_base.trim_end_matches('/').to_owned(),
                    header: "Authorization".into(),
                    value_prefix: "Bearer ".into(),
                    capability: "secret.use".into(),
                },
                token.clone(),
            ));
        }
    }
    let req = ProvisionRequest {
        tenant_id: custody.tenant_id.clone(),
        session_id: task.session_id.to_string(),
        task_id: task.task_id.to_string(),
        lease_generation: custody.lease_generation,
        workspace_source: root.to_owned(),
        protected_paths: vec![".git/hooks".into()],
        resources: serde_json::json!({}),
        egress,
        credentials,
    };
    let handle = match custody.client.provision(&req).await {
        Ok(h) => h,
        Err(e) => {
            return Err((
                "SANDBOX_UNAVAILABLE".into(),
                format!("the gateway did not provision a sandbox: {e}"),
            ));
        }
    };
    let id = handle.identity().clone();
    core.tools
        .sandboxes
        .lock()
        .await
        .insert(task.task_id, Arc::clone(&handle));
    let ev = typed(
        "SandboxLeaseAcquired",
        &TaskEvent::SandboxLeaseAcquired {
            sandbox_id: id.sandbox_id.clone(),
            backend: id.backend.clone(),
            isolated: id.isolated,
            image_version: id.image_version.clone().unwrap_or_default(),
            boot_id: id.boot_id.clone(),
            workspace_root: id.workspace_root.clone(),
            lease_generation: custody.lease_generation,
            egress: id.egress.clone(),
            credentials: id.credentials.clone(),
        },
        actor.clone(),
    );
    let mut store = core.store.lock().await;
    if let Ok(stored) = store.append(AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task.task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *task.task_id.as_bytes(),
        expected_sequence: None,
        events: vec![ev],
    }) && let Some(last) = stored.last()
    {
        core.last_offset.send_replace(last.offset);
    }
    eprintln!(
        "modbit-core: task {} runs in sandbox {} ({}, isolated: {}) issued to worker {} at cloud lease generation {}",
        task.task_id,
        id.sandbox_id,
        id.backend,
        id.isolated,
        custody.worker_id,
        custody.lease_generation
    );
    Ok(handle)
}

/// Destroy the task's sandbox once the task has ended; a no-op otherwise.
pub async fn release_if_ended(core: &Arc<Core>, task_id: TaskId, actor: &Actor) {
    let task = match core.store.lock().await.task(&task_id) {
        Ok(Some(t)) => t,
        _ => return,
    };
    let reason = match task.state {
        TaskState::Completed => "task_completed",
        TaskState::Cancelled => "task_cancelled",
        TaskState::Failed => "task_failed",
        _ => return,
    };
    let handle = core.tools.sandboxes.lock().await.remove(&task_id);
    let Some(handle) = handle else {
        return;
    };
    eprintln!(
        "modbit-core: task {task_id} ended ({reason}); releasing sandbox {}",
        handle.identity().sandbox_id
    );
    let sandbox_id = handle.identity().sandbox_id.clone();
    match tokio::time::timeout(std::time::Duration::from_secs(60), handle.destroy()).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("modbit-core: destroying sandbox {sandbox_id}: {e}"),
        Err(_) => eprintln!(
            "modbit-core: destroying sandbox {sandbox_id}: the gateway did not answer within 60 s"
        ),
    }
    let ev = typed(
        "SandboxReleased",
        &TaskEvent::SandboxReleased {
            sandbox_id,
            reason: reason.into(),
        },
        actor.clone(),
    );
    let mut store = core.store.lock().await;
    match store.append(AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *task_id.as_bytes(),
        expected_sequence: None,
        events: vec![ev],
    }) {
        Ok(stored) => {
            if let Some(last) = stored.last() {
                core.last_offset.send_replace(last.offset);
            }
        }
        Err(e) => eprintln!("modbit-core: recording SandboxReleased for {task_id}: {e}"),
    }
}
