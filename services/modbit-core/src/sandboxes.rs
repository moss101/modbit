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
    // M8.8: a browser inside the sandbox when the lease grants the task
    // one; its traffic goes through the same broker.
    let browser = ops.iter().any(|o| o == "browser.control");
    let req = ProvisionRequest {
        tenant_id: custody.tenant_id.clone(),
        session_id: task.session_id.to_string(),
        task_id: task.task_id.to_string(),
        lease_generation: custody.lease_generation,
        workspace_source: root.to_owned(),
        protected_paths: vec![".git/hooks".into()],
        // A browser needs room: a MicroVM with one gets more memory.
        resources: if browser {
            serde_json::json!({"memory_mib": 1024})
        } else {
            serde_json::json!({})
        },
        egress,
        credentials,
        browser,
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
            browser: id.browser,
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

/// M8.9 (docs/21 "Sandbox recovery", E2E-018): the task's sandbox did not
/// answer a call. Try the link once (a lost link over a live guest is
/// re-admitted and the guest's processes are as they were — M8.5); when
/// the guest itself is gone, journal `SandboxLost`, provision a fresh
/// sandbox from the task's seed, write the latest checkpoint's worktree
/// into it (`SandboxRestored`) and hand the tools the new one. Returns what
/// the model is told: the call that did not answer keeps its unknown
/// outcome (never replayed here); what came after the checkpoint is its to
/// redo.
pub async fn recover_if_lost(core: &Arc<Core>, task: &Task, actor: &Actor) -> Option<String> {
    let handle = core
        .tools
        .sandboxes
        .lock()
        .await
        .get(&task.task_id)
        .cloned()?;
    let old_id = handle.identity().sandbox_id.clone();
    // 1. the link, once.
    match tokio::time::timeout(std::time::Duration::from_secs(30), handle.relink()).await {
        Ok(Ok(same_boot)) if same_boot => {
            eprintln!(
                "modbit-core: sandbox {old_id}: the link was re-admitted over the same boot; nothing lost"
            );
            return Some(format!(
                "the sandbox's link was lost and re-admitted (sandbox {old_id}, same boot); the guest and its processes are as they were"
            ));
        }
        Ok(Ok(_)) => eprintln!(
            "modbit-core: sandbox {old_id}: re-linked to a guest of another boot; treating the sandbox as lost"
        ),
        Ok(Err(e)) => eprintln!("modbit-core: sandbox {old_id}: relink failed: {e}; lost"),
        Err(_) => eprintln!("modbit-core: sandbox {old_id}: relink did not answer; lost"),
    }
    // 2. lost: on the log, the handle gone, the remains destroyed.
    core.tools.sandboxes.lock().await.remove(&task.task_id);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(30), handle.destroy()).await;
    append_task_event(
        core,
        task,
        typed(
            "SandboxLost",
            &TaskEvent::SandboxLost {
                sandbox_id: old_id.clone(),
                detail: "the guest stopped answering and could not be re-linked".into(),
            },
            actor.clone(),
        ),
    )
    .await;
    // 3. a fresh sandbox from the seed, the latest checkpoint over it.
    let fresh = match ensure_for_task(core, task, actor).await {
        Ok(h) => h,
        Err((code, why)) => {
            return Some(format!(
                "the sandbox {old_id} was lost and no replacement could be provisioned ({code}: {why}); the task cannot continue in a sandbox until the gateway serves one"
            ));
        }
    };
    let new_id = fresh.identity().sandbox_id.clone();
    let (checkpoint, files) = {
        let store = core.store.lock().await;
        let all = crate::checkpoint::manifests(&store, task);
        match crate::checkpoint::current(&store, task) {
            Some(cur) => {
                let chain = modbit_checkpoint::chain_to(&all, cur.checkpoint_id).ok();
                let files = chain
                    .as_ref()
                    .and_then(|c| modbit_checkpoint::materialize(c).ok())
                    .map(|m| m.files)
                    .unwrap_or_default();
                (Some(cur), files)
            }
            None => (None, Default::default()),
        }
    };
    let root = fresh
        .identity()
        .workspace_root
        .trim_end_matches('/')
        .to_owned();
    let tid = task.task_id.to_string();
    let mut written = 0u32;
    let mut removed = 0u32;
    let mut failures = Vec::new();
    for (path, hash) in &files {
        let guest_path = format!("{root}/{path}");
        if hash == modbit_checkpoint::DELETED {
            match fresh.remove(&tid, "", &guest_path).await {
                Ok(()) => removed += 1,
                Err(e) => failures.push(format!("remove {path}: {e}")),
            }
            continue;
        }
        let bytes = match core.store.lock().await.objects().get(hash) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("object of {path}: {e}"));
                continue;
            }
        };
        match fresh.write_file(&tid, "", &guest_path, bytes).await {
            Ok(_) => written += 1,
            Err(e) => failures.push(format!("write {path}: {e}")),
        }
    }
    // The task's browser, when it has one, is the fresh sandbox's now (M8.8).
    if fresh.identity().browser
        && let Err(e) =
            crate::browser_cloud::CloudHost::attach(core, task, Arc::clone(&fresh), actor).await
    {
        failures.push(format!("browser host: {e}"));
    }
    append_task_event(
        core,
        task,
        typed(
            "SandboxRestored",
            &TaskEvent::SandboxRestored {
                sandbox_id: new_id.clone(),
                replaced: old_id.clone(),
                checkpoint_id: checkpoint
                    .as_ref()
                    .map(|c| c.checkpoint_id.to_string())
                    .unwrap_or_default(),
                epoch: checkpoint.as_ref().map(|c| c.epoch).unwrap_or(0),
                files_written: written,
                files_removed: removed,
            },
            actor.clone(),
        ),
    )
    .await;
    eprintln!(
        "modbit-core: sandbox {old_id} lost; task {} continues in sandbox {new_id} restored from {} ({written} written, {removed} removed{})",
        task.task_id,
        checkpoint
            .as_ref()
            .map(|c| format!("checkpoint {} (epoch {})", c.checkpoint_id, c.epoch))
            .unwrap_or_else(|| "the seed alone".into()),
        if failures.is_empty() {
            String::new()
        } else {
            format!("; {} failed", failures.len())
        }
    );
    let mut note = match &checkpoint {
        Some(c) => format!(
            "the sandbox {old_id} was lost; a fresh sandbox {new_id} was provisioned and the worktree restored from checkpoint {} (epoch {}, {written} files written, {removed} removed) — everything done in the sandbox after that checkpoint is gone and must be redone; this call's outcome is unknown and was not replayed",
            c.checkpoint_id, c.epoch
        ),
        None => format!(
            "the sandbox {old_id} was lost; a fresh sandbox {new_id} was provisioned from the task's seed — no checkpoint existed, so everything done in the sandbox is gone and must be redone; this call's outcome is unknown and was not replayed"
        ),
    };
    if !failures.is_empty() {
        note.push_str(&format!("; restore failures: {}", failures.join("; ")));
    }
    Some(note)
}

async fn append_task_event(core: &Arc<Core>, task: &Task, ev: modbit_event_store::NewEvent) {
    let mut store = core.store.lock().await;
    match store.append(AppendRequest {
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
    }) {
        Ok(stored) => {
            if let Some(last) = stored.last() {
                core.last_offset.send_replace(last.offset);
            }
        }
        Err(e) => eprintln!(
            "modbit-core: recording a sandbox event for {}: {e}",
            task.task_id
        ),
    }
}
