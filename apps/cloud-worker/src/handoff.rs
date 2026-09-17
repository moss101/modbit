//! Materializing a handoff on the worker (M8.7; docs/21 "Handoff local →
//! cloud", docs/24 "Sync model"): when the imported log carries a
//! `TaskHandoffAdmitted` the worker has not yet materialized, it fetches
//! the bundle's parts from the tenant's object store — the manifest, the
//! repository bundle, the checkpoint's file objects and every other object
//! the log references — clones the repository from the bundle into a
//! directory of the session's own, writes the checkpoint's files over it
//! (the worktree exactly as it was), hands the objects to the Core
//! (`ImportObjects`, so transcripts and outputs resolve), rebinds the
//! task's workspace to the directory (`RebindTaskWorkspace`), and trusts
//! it. The task then resumes as any waiting task the worker starts.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_event_store::cloud::CloudStore;
use modbit_protocol::client::Client;
use modbit_protocol::v1 as wire;
use prost::Message;
use serde_json::Value;

/// A handoff admitted on the log that this worker has not materialized.
#[derive(Clone, Debug)]
pub struct Pending {
    /// The task.
    pub task_id: TaskId,
    /// The bundle's manifest hash (an object in the tenant's store).
    pub bundle_hash: String,
}

/// The handoffs on the session's log, minus the ones a later
/// `TaskWorkspaceRebound` already served.
pub async fn pending(
    store: &CloudStore,
    tenant: TenantId,
    sid: SessionId,
) -> anyhow::Result<Vec<Pending>> {
    let mut admitted: Vec<Pending> = Vec::new();
    let mut rebound: BTreeSet<[u8; 16]> = BTreeSet::new();
    let mut after = 0;
    loop {
        let batch = store.events_after(tenant, sid, after, 500).await?;
        if batch.is_empty() {
            break;
        }
        for e in &batch {
            after = e.session_offset;
            match e.envelope.event_type.as_str() {
                "TaskHandoffAdmitted" => {
                    if let Some(h) = e.payload["bundle_hash"].as_str() {
                        admitted.push(Pending {
                            task_id: TaskId::from_bytes(e.envelope.aggregate_id),
                            bundle_hash: h.to_owned(),
                        });
                    }
                }
                "TaskWorkspaceRebound" => {
                    rebound.insert(e.envelope.aggregate_id);
                }
                _ => {}
            }
        }
    }
    Ok(admitted
        .into_iter()
        .filter(|p| !rebound.contains(p.task_id.as_bytes()))
        .collect())
}

fn git(root: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Materialize one handoff into `dir` and bind the task to it on the Core.
pub async fn materialize(
    store: &CloudStore,
    c: &mut Client,
    tenant: TenantId,
    sid: SessionId,
    generation: u64,
    p: &Pending,
    dir: &Path,
) -> anyhow::Result<PathBuf> {
    let manifest_bytes = store.get_object(tenant, &p.bundle_hash).await?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
    if manifest["kind"] != "modbit-handoff" {
        anyhow::bail!("bundle {} is not a handoff manifest", p.bundle_hash);
    }
    let root = dir.join("workspace");
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(dir)?;
    // 1. The repository: cloned from the bundle, on the branch it was on.
    let has_bundle = manifest["git"]["bundle"].as_bool().unwrap_or(false);
    let bundle_hash = manifest["parts"]["repo.bundle"].as_str();
    if has_bundle && let Some(h) = bundle_hash {
        let bytes = store.get_object(tenant, h).await?;
        let bundle_path = dir.join("repo.bundle");
        std::fs::write(&bundle_path, &bytes)?;
        let out = std::process::Command::new("git")
            .arg("clone")
            .arg("-q")
            .arg(&bundle_path)
            .arg(&root)
            .output()?;
        if !out.status.success() {
            anyhow::bail!(
                "git clone of the handoff bundle: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let branch = manifest["git"]["branch"].as_str().unwrap_or("");
        if !branch.is_empty() && branch != "HEAD" {
            let _ = git(&root, &["checkout", "-q", branch]);
        }
        let _ = git(&root, &["config", "core.autocrlf", "false"]);
    } else {
        std::fs::create_dir_all(&root)?;
    }
    // 2. The checkpoint's files over the clone, and every object the log
    // references into the Core.
    let object_hashes: Vec<String> = manifest["objects"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut objects: Vec<(String, Vec<u8>)> = Vec::new();
    for h in &object_hashes {
        match store.get_object(tenant, h).await {
            Ok(b) => objects.push((h.clone(), b)),
            Err(e) => eprintln!("modbit-cloud-worker: handoff object {h}: {e}"),
        }
    }
    // The checkpoint manifest names path → object hash.
    let checkpoint_manifest_hash = manifest["checkpoint"]["manifest_ref"]
        .as_str()
        .unwrap_or_default();
    let mut applied = 0usize;
    if let Some((_, bytes)) = objects.iter().find(|(h, _)| h == checkpoint_manifest_hash)
        && let Ok(cm) = serde_json::from_slice::<Value>(bytes)
        && let Some(files) = cm["files"].as_object()
    {
        for (path, hash) in files {
            let target = root.join(path);
            let hash = hash.as_str().unwrap_or_default();
            if hash.is_empty() {
                let _ = std::fs::remove_file(&target);
                continue;
            }
            if let Some((_, content)) = objects.iter().find(|(h, _)| h == hash) {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, content)?;
                applied += 1;
            }
        }
    }
    for chunk in objects.chunks(64) {
        let ack = c
            .command(super::session::envelope(
                "ImportObjects",
                wire::ImportObjects {
                    objects: chunk.iter().map(|(_, b)| b.clone()).collect(),
                }
                .encode_to_vec(),
                None,
            ))
            .await?;
        let _: wire::ObjectsImported = Client::result(&ack)?;
    }
    // 3. The task's workspace is here now; trusted for the session.
    let root_text = root
        .canonicalize()
        .unwrap_or(root.clone())
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let ack = c
        .command(super::session::envelope(
            "RebindTaskWorkspace",
            wire::RebindTaskWorkspace {
                task_id: Some(super::session::id_of(p.task_id.as_bytes())),
                workspace_root: root_text.clone(),
                reason: "handoff".into(),
                // The continuation runs where the cloud runs tasks: inside a
                // sandbox, whatever profile the laptop ran it under.
                execution_profile: "cloud_isolated".into(),
            }
            .encode_to_vec(),
            Some(generation),
        ))
        .await?;
    let _: wire::TaskWorkspaceRebound = Client::result(&ack)?;
    let _ = c
        .command(super::session::envelope(
            "TrustRepository",
            wire::TrustRepository {
                session_id: Some(super::session::id_of(sid.as_bytes())),
                workspace_root: root_text.clone(),
                scope: "repository".into(),
            }
            .encode_to_vec(),
            Some(generation),
        ))
        .await;
    eprintln!(
        "modbit-cloud-worker: handoff of task {} materialized at {} ({} objects, {} checkpoint files)",
        p.task_id,
        root_text,
        objects.len(),
        applied
    );
    Ok(PathBuf::from(root_text))
}
