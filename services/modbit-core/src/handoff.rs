//! Local → cloud handoff (M8.7; docs/21 "Handoff local → cloud", docs/24
//! "Sync model": remote coding operates on explicit Git/checkpoint handoff
//! bundles). `export` parks the task's run at its next boundary, takes a
//! checkpoint of the worktree, and writes a bundle directory: the session's
//! log verbatim (`events.jsonl`), every object those events reference
//! (`objects/<hash>`: transcripts, outputs, the checkpoint manifest and its
//! file blobs), the repository as a Git bundle (`repo.bundle`), and
//! `manifest.json` naming what the continuation needs — the capabilities
//! the run has used and the secret handles it names. No raw secret is in
//! the bundle: the forge token stays in this Core's custody, the cloud's
//! broker holds its own. `TaskHandedOff` records the export on the task.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use modbit_domain::TaskId;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::state::StateMachine;
use modbit_domain::task::{TaskEvent, TaskState};
use modbit_event_store::AppendRequest;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::runtime::{Lineage, typed};
use crate::server::Core;

/// The manifest schema.
pub const HANDOFF_SCHEMA_VERSION: u32 = 1;

/// What `export` wrote.
pub struct Exported {
    /// The manifest as JSON.
    pub manifest: Value,
    /// sha256 of `manifest.json`.
    pub manifest_hash: String,
    /// The files in the bundle directory the manifest names.
    pub parts: Vec<String>,
}

fn hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Every 64-hex token in a JSON value.
fn hashes_in(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::String(s) => {
            if hex64(s) {
                out.insert(s.clone());
            }
        }
        Value::Array(a) => a.iter().for_each(|x| hashes_in(x, out)),
        Value::Object(m) => m.values().for_each(|x| hashes_in(x, out)),
        _ => {}
    }
}

/// Every 64-hex token in text (an object that is JSON or text).
fn hashes_in_bytes(bytes: &[u8], out: &mut BTreeSet<String>) {
    if let Ok(v) = serde_json::from_slice::<Value>(bytes) {
        hashes_in(&v, out);
        return;
    }
    let text = String::from_utf8_lossy(bytes);
    for token in text.split(|c: char| !c.is_ascii_hexdigit()) {
        if hex64(token) {
            out.insert(token.to_owned());
        }
    }
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Export `task_id` into `out_dir`.
pub async fn export(
    core: &Arc<Core>,
    task_id: TaskId,
    actor: &Actor,
    out_dir: &Path,
) -> Result<Exported, (String, String)> {
    let task = match core.store.lock().await.task(&task_id) {
        Ok(Some(t)) => t,
        Ok(None) => return Err(("UNKNOWN_TASK".into(), task_id.to_string())),
        Err(e) => return Err(("STORE".into(), e.to_string())),
    };
    let Some(root) = task.workspace_root.clone() else {
        return Err((
            "NO_WORKSPACE".into(),
            "a handoff carries a workspace; this task has none".into(),
        ));
    };
    // 1. Park the run at its next boundary; wait for the loop to end.
    if core.runtime.park(&task_id).await {
        let deadline = Instant::now() + Duration::from_secs(120);
        while core.runtime.is_running(&task_id).await {
            if Instant::now() > deadline {
                return Err((
                    "HANDOFF_PARK_TIMEOUT".into(),
                    "the run did not reach a boundary within two minutes".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    let task = match core.store.lock().await.task(&task_id) {
        Ok(Some(t)) => t,
        _ => return Err(("UNKNOWN_TASK".into(), task_id.to_string())),
    };
    if task.state.is_terminal() || matches!(task.state, TaskState::Running) {
        return Err((
            "HANDOFF_STATE".into(),
            format!("a task in state {:?} is not handed off", task.state),
        ));
    }
    // 2. The worktree, as an immutable checkpoint (its blobs are objects).
    let lt = Lineage::task(core.tenant_id, task.session_id, task_id);
    let captured = crate::checkpoint::capture(core, &task, lt, actor, None, "before_handoff")
        .await
        .map_err(|e| ("CHECKPOINT".into(), e.to_string()))?;
    let checkpoint_id = captured.manifest.checkpoint_id.to_string();
    // The manifest as the log references it: the object the Core stored.
    let manifest_ref = core
        .store
        .lock()
        .await
        .objects()
        .put(&serde_json::to_vec(&captured.manifest).map_err(|e| ("IO".into(), e.to_string()))?)
        .map_err(|e| ("STORE".into(), e.to_string()))?;
    let git_head = captured
        .manifest
        .git_head
        .clone()
        .or_else(|| git(Path::new(&root), &["rev-parse", "HEAD"]))
        .unwrap_or_default();
    let branch = git(Path::new(&root), &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    std::fs::create_dir_all(out_dir.join("objects")).map_err(|e| ("IO".into(), e.to_string()))?;
    // 3. The log, verbatim, and every object it reaches.
    let (events, objects, tools_used) = {
        let store = core.store.lock().await;
        let events = store
            .read_session(&task.session_id, 0, usize::MAX)
            .map_err(|e| ("STORE".into(), e.to_string()))?;
        let mut lines = Vec::with_capacity(events.len());
        let mut hashes = BTreeSet::new();
        let mut tools_used = BTreeSet::new();
        for e in &events {
            let payload = store
                .payload(&e.envelope)
                .map_err(|x| ("STORE".into(), x.to_string()))?;
            hashes_in(&payload, &mut hashes);
            if e.envelope.event_type == "ToolCallProposed"
                && let Some(name) = payload["tool_name"].as_str()
            {
                tools_used.insert(name.to_owned());
            }
            lines.push(json!({"envelope": e.envelope, "payload": payload}));
        }
        // One level deeper: what the objects themselves reference (a
        // checkpoint manifest's file blobs, a transcript's outputs).
        let mut all = BTreeSet::new();
        let mut frontier: Vec<String> = hashes.into_iter().collect();
        let mut depth = 0;
        while !frontier.is_empty() && depth < 3 {
            let mut next = BTreeSet::new();
            for h in frontier {
                if !all.insert(h.clone()) {
                    continue;
                }
                if let Ok(bytes) = store.objects().get(&h) {
                    hashes_in_bytes(&bytes, &mut next);
                }
            }
            frontier = next.into_iter().filter(|h| !all.contains(h)).collect();
            depth += 1;
        }
        let mut objects = Vec::new();
        for h in &all {
            if let Ok(bytes) = store.objects().get(h) {
                objects.push((h.clone(), bytes));
            }
        }
        (lines, objects, tools_used)
    };
    let mut events_file = String::new();
    for l in &events {
        events_file.push_str(&l.to_string());
        events_file.push('\n');
    }
    std::fs::write(out_dir.join("events.jsonl"), &events_file)
        .map_err(|e| ("IO".into(), e.to_string()))?;
    let mut object_hashes = Vec::new();
    for (h, bytes) in &objects {
        std::fs::write(out_dir.join("objects").join(h), bytes)
            .map_err(|e| ("IO".into(), e.to_string()))?;
        object_hashes.push(h.clone());
    }
    // 4. The repository, as a bundle (every ref).
    let mut parts = vec!["events.jsonl".to_owned()];
    let bundle_path = out_dir.join("repo.bundle");
    let has_repo = git(Path::new(&root), &["rev-parse", "--is-inside-work-tree"]).is_some();
    if has_repo {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["bundle", "create"])
            .arg(&bundle_path)
            .arg("--all")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            return Err(("GIT_BUNDLE".into(), "git bundle create failed".into()));
        }
        parts.push("repo.bundle".to_owned());
    }
    // 5. What the continuation needs: the capabilities of every tool the
    // run has used; the secret handles the task may use — named by the
    // lease's `secret.use` against the custody this Core holds — as
    // handles, never values.
    let mut capabilities = BTreeSet::new();
    let mut secret_handles = BTreeSet::new();
    for name in &tools_used {
        if let Some(t) = core.tools.runtime.registry().get(name) {
            for c in &t.spec().required_capabilities {
                capabilities.insert(c.clone());
            }
        }
    }
    let lease_ops: Vec<String> = core
        .store
        .lock()
        .await
        .leases_for_task(&task_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|l| l.is_valid(modbit_domain::Timestamp::now()))
        .flat_map(|l| l.operations)
        .collect();
    if (lease_ops.iter().any(|o| o == "secret.use") || capabilities.contains("secret.use"))
        && core.tools.forge.get().is_some_and(|f| f.token.is_some())
    {
        secret_handles.insert("forge-token".to_owned());
    }
    let manifest = json!({
        "schema_version": HANDOFF_SCHEMA_VERSION,
        "kind": "modbit-handoff",
        "from_tenant": core.tenant_id.to_string(),
        "session_id": task.session_id.to_string(),
        "task_id": task_id.to_string(),
        "goal_text": task.goal_text,
        "execution_profile": task.execution_profile,
        "workspace_root": root,
        "git": {"head": git_head, "branch": branch, "bundle": has_repo},
        "checkpoint": {"checkpoint_id": checkpoint_id, "manifest_ref": manifest_ref, "integrity_hash": modbit_checkpoint::integrity_hash(&captured.manifest), "files": captured.manifest.files.len()},
        "events": events.len(),
        "objects": object_hashes,
        "capabilities": capabilities.iter().cloned().collect::<Vec<_>>(),
        "secret_handles": secret_handles.iter().cloned().collect::<Vec<_>>(),
        "tools_used": tools_used.iter().cloned().collect::<Vec<_>>(),
        "exported_at_ms": modbit_domain::Timestamp::now().millis(),
    });
    let manifest_text = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    std::fs::write(out_dir.join("manifest.json"), &manifest_text)
        .map_err(|e| ("IO".into(), e.to_string()))?;
    parts.push("manifest.json".to_owned());
    let manifest_hash = hex::encode(Sha256::digest(manifest_text.as_bytes()));
    // 6. On the task: handed off.
    let ev = typed(
        "TaskHandedOff",
        &TaskEvent::TaskHandedOff {
            bundle_hash: manifest_hash.clone(),
            checkpoint_id,
            git_head,
            capabilities: capabilities.into_iter().collect(),
            secret_handles: secret_handles.into_iter().collect(),
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
        Err(e) => return Err(("STORE".into(), e.to_string())),
    }
    Ok(Exported {
        manifest,
        manifest_hash,
        parts,
    })
}
