//! The environment a task's run is pinned to (REQ-EV-0021 / 0062 / 0146;
//! docs/21 "Environment revisions"). The definition — user, team and
//! repository layers with the blueprints they extend (`modbit_workspace::environment`)
//! — is captured as a revision when a run starts: the files by hash, the
//! toolchain as observed, the `PATH` entries and the variables. The run
//! pins that revision (`EnvironmentPinned`), its processes run with it,
//! and it keeps it whatever the files do afterwards: a resumed run whose
//! environment no longer matches finds `EnvironmentStale` and waits for an
//! explicit `RebuildEnvironment` (`EnvironmentRebuilt`) rather than running
//! in an environment nobody chose. A rebuild re-captures and pins the exact
//! digest of what is there now.
//!
//! A `cloud_isolated` task's environment is its sandbox's image: the
//! revision is the image the gateway verified (its version and hash) and
//! the features it declared; no host tool is probed for it.

use std::collections::HashMap;
use std::sync::Arc;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_domain::{RunId, TaskId, Timestamp};
use modbit_workspace::environment::{self as envdef, EnvironmentRevision, SystemProbe};

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// What a run's processes get from the pinned revision.
#[derive(Clone, Debug)]
pub struct PinnedEnvironment {
    /// The revision.
    pub revision: EnvironmentRevision,
    /// The object holding it.
    pub revision_ref: String,
    /// The execution profile the revision was captured under. A resumed
    /// run whose profile has since changed (a local→cloud handoff, a
    /// `RebindTaskWorkspace`) does not read this pin as stale — the
    /// environment legitimately changed with the profile, so it re-pins.
    pub profile: String,
}

/// The revision as stored in the object store (with the values, which the
/// log never carries).
#[derive(serde::Serialize, serde::Deserialize)]
struct Stored {
    revision: EnvironmentRevision,
    env: std::collections::BTreeMap<String, String>,
    /// The execution profile the revision was captured under (absent on
    /// revisions pinned before this field existed).
    #[serde(default)]
    profile: String,
}

/// The pinned environments by task, for the tool host.
#[derive(Default)]
pub struct Environments {
    pinned: std::sync::Mutex<HashMap<TaskId, Arc<PinnedEnvironment>>>,
}

impl Environments {
    /// The task's pinned environment, if any is in memory.
    pub fn get(&self, task: &TaskId) -> Option<Arc<PinnedEnvironment>> {
        self.pinned.lock().ok().and_then(|m| m.get(task).cloned())
    }

    fn set(&self, task: TaskId, p: Arc<PinnedEnvironment>) {
        if let Ok(mut m) = self.pinned.lock() {
            m.insert(task, p);
        }
    }

    /// Forget a task's environment (the task ended).
    pub fn forget(&self, task: &TaskId) {
        if let Ok(mut m) = self.pinned.lock() {
            m.remove(task);
        }
    }
}

/// The environment as it is now for `task`.
pub async fn current(core: &Core, task: &Task) -> EnvironmentRevision {
    let root = std::path::PathBuf::from(task.workspace_root.as_deref().unwrap_or("."));
    if task.execution_profile == modbit_policy::kernel::PROFILE_CLOUD_ISOLATED {
        return cloud_revision(core, task).await;
    }
    let data_dir = core.data_dir.clone();
    let probe_root = root.clone();
    match tokio::task::spawn_blocking(move || {
        envdef::snapshot(&probe_root, &data_dir, &SystemProbe, Timestamp::now().0)
    })
    .await
    {
        Ok(rev) => rev,
        Err(_) => envdef::snapshot(&root, &core.data_dir, &NoProbe, Timestamp::now().0),
    }
}

struct NoProbe;

impl envdef::ToolProbe for NoProbe {
    fn version(&self, _name: &str, _arg: &str, _path: &[String]) -> Option<String> {
        None
    }
}

/// A cloud task's environment: the sandbox's verified image and features.
async fn cloud_revision(core: &Core, task: &Task) -> EnvironmentRevision {
    let identity = core
        .tools
        .sandboxes
        .lock()
        .await
        .get(&task.task_id)
        .map(|h| modbit_sandbox::port::SandboxPort::identity(&**h).clone());
    let features = core
        .tools
        .sandbox_gateway
        .lock()
        .await
        .as_ref()
        .map(|c| c.features.clone())
        .unwrap_or_default();
    let mut toolchain = Vec::new();
    let mut sources = Vec::new();
    if let Some(id) = &identity {
        sources.push(envdef::SourceRecord {
            kind: "image".into(),
            path: format!(
                "{}:{}",
                id.backend,
                id.image_version.clone().unwrap_or_default()
            ),
            sha256: String::new(),
        });
        toolchain.push(envdef::ToolState {
            name: "modbit-guest".into(),
            version: id.image_version.clone(),
            optional: false,
        });
    }
    for f in features {
        toolchain.push(envdef::ToolState {
            name: format!("feature:{f}"),
            version: Some("present".into()),
            optional: true,
        });
    }
    let mut rev = EnvironmentRevision {
        digest: String::new(),
        workspace_root: identity
            .as_ref()
            .map(|i| i.workspace_root.clone())
            .unwrap_or_else(|| "/workspace".into()),
        sources,
        toolchain,
        path: vec![],
        env_names: vec![],
        env: Default::default(),
        platform: identity
            .as_ref()
            .map(|i| {
                format!(
                    "{}/{}",
                    i.backend,
                    if i.isolated { "isolated" } else { "unisolated" }
                )
            })
            .unwrap_or_else(|| "cloud".into()),
        problems: vec![],
        captured_at_ms: Timestamp::now().0,
    };
    rev.digest = envdef::digest_of(&rev);
    rev
}

/// The revision the task pinned last, from the log (the newest
/// `EnvironmentRebuilt` or `EnvironmentPinned`), with its values from the
/// object store.
pub async fn pinned(core: &Core, task: &Task) -> Option<Arc<PinnedEnvironment>> {
    if let Some(p) = core.tools.environments.get(&task.task_id) {
        return Some(p);
    }
    let store = core.store.lock().await;
    let events = store
        .read_aggregate(task.task_id.as_bytes(), 0, usize::MAX)
        .ok()?;
    let revision_ref = events
        .iter()
        .rev()
        .find_map(|e| match e.envelope.event_type.as_str() {
            "EnvironmentPinned" | "EnvironmentRebuilt" => {
                let p = store.payload(&e.envelope).ok()?;
                p["revision_ref"].as_str().map(str::to_owned)
            }
            _ => None,
        })?;
    let bytes = store.objects().get(&revision_ref).ok()?;
    let stored: Stored = serde_json::from_slice(&bytes).ok()?;
    let mut revision = stored.revision;
    revision.env = stored.env;
    let p = Arc::new(PinnedEnvironment {
        revision,
        revision_ref,
        profile: stored.profile,
    });
    core.tools.environments.set(task.task_id, Arc::clone(&p));
    Some(p)
}

fn store_revision(
    core_store: &modbit_event_store::EventStore,
    rev: &EnvironmentRevision,
    profile: &str,
) -> Option<String> {
    let stored = Stored {
        revision: rev.clone(),
        env: rev.env.clone(),
        profile: profile.to_owned(),
    };
    core_store
        .objects()
        .put(&serde_json::to_vec(&stored).ok()?)
        .ok()
}

fn pinned_event(run_id: RunId, rev: &EnvironmentRevision, revision_ref: &str) -> TaskEvent {
    TaskEvent::EnvironmentPinned {
        run_id,
        digest: rev.digest.clone(),
        revision_ref: revision_ref.to_owned(),
        sources: rev
            .sources
            .iter()
            .map(|s| serde_json::json!({"kind": s.kind, "path": s.path, "sha256": s.sha256}))
            .collect(),
        toolchain: rev
            .toolchain
            .iter()
            .map(|t| serde_json::json!({"name": t.name, "version": t.version, "optional": t.optional}))
            .collect(),
        path: rev.path.clone(),
        env_names: rev.env_names.clone(),
        problems: rev.problems.clone(),
    }
}

/// What a run starting now does with its environment: a fresh run pins
/// what is there; a resumed run checks what it pinned against what is
/// there and answers `Err((code, reason))` — `ENVIRONMENT_STALE` when they
/// differ, `ENVIRONMENT_UNAVAILABLE` when a required tool is gone (the run
/// waits either way).
pub async fn attach_to_run(
    core: &Arc<Core>,
    task: &Task,
    run_id: RunId,
    resumed: bool,
    lt: Lineage,
    actor: &Actor,
) -> Result<Arc<PinnedEnvironment>, (&'static str, String)> {
    let now = current(core, task).await;
    let pinned = if resumed {
        pinned(core, task).await
    } else {
        None
    };
    // A pin from a different execution profile is not this run's to keep:
    // the environment legitimately changed with the profile (a local→cloud
    // handoff, a `RebindTaskWorkspace`), so the run re-pins what is there
    // rather than reading the old profile's revision as stale.
    let comparable = pinned.filter(|p| keeps_pin(&p.profile, &task.execution_profile));
    match comparable {
        Some(p) => {
            if p.revision.digest != now.digest {
                let changes = envdef::changes(&p.revision, &now);
                let mut store = core.store.lock().await;
                let _ = append(
                    &mut store,
                    core,
                    lt,
                    AggregateType::Task,
                    *task.task_id.as_bytes(),
                    vec![typed(
                        "EnvironmentStale",
                        &TaskEvent::EnvironmentStale {
                            run_id,
                            pinned_digest: p.revision.digest.clone(),
                            current_digest: now.digest.clone(),
                            changes: changes.clone(),
                        },
                        actor.clone(),
                    )],
                );
                return Err((
                    "ENVIRONMENT_STALE",
                    format!(
                        "the environment is not the revision this run pinned ({} → {}): {}; rebuild it (RebuildEnvironment) to continue in what is there now, or restore the definition",
                        &p.revision.digest[..12.min(p.revision.digest.len())],
                        &now.digest[..12.min(now.digest.len())],
                        changes.join("; ")
                    ),
                ));
            }
            Ok(p)
        }
        None => {
            let missing = envdef::missing_tools(&now);
            if !missing.is_empty() {
                return Err((
                    "ENVIRONMENT_UNAVAILABLE",
                    format!(
                        "the environment names tools that are not there: {}",
                        missing.join(", ")
                    ),
                ));
            }
            let mut store = core.store.lock().await;
            let revision_ref =
                store_revision(&store, &now, &task.execution_profile).unwrap_or_default();
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *task.task_id.as_bytes(),
                vec![typed(
                    "EnvironmentPinned",
                    &pinned_event(run_id, &now, &revision_ref),
                    actor.clone(),
                )],
            );
            let p = Arc::new(PinnedEnvironment {
                revision: now,
                revision_ref,
                profile: task.execution_profile.clone(),
            });
            core.tools.environments.set(task.task_id, Arc::clone(&p));
            Ok(p)
        }
    }
}

/// Whether a resumed run keeps the pin it read to compare against what is
/// there now. A pin from an earlier build carries no profile and is kept
/// (its digest still describes the same execution); a pin made under a
/// different execution profile is not kept — the profile changed (a
/// local→cloud handoff, a `RebindTaskWorkspace`) and its revision describes
/// an environment this run no longer runs in, so the run re-pins instead of
/// reading it as stale.
fn keeps_pin(pinned_profile: &str, task_profile: &str) -> bool {
    pinned_profile.is_empty() || pinned_profile == task_profile
}

/// Re-capture and pin the environment as it is now (an explicit rebuild).
/// Returns `(from_digest, to_digest, changes, offset)`.
pub async fn rebuild(
    core: &Arc<Core>,
    task: &Task,
    actor: &Actor,
) -> Result<(String, String, Vec<String>, u64), String> {
    let before = pinned(core, task).await;
    let now = current(core, task).await;
    let missing = envdef::missing_tools(&now);
    if !missing.is_empty() {
        return Err(format!(
            "the environment names tools that are not there: {}",
            missing.join(", ")
        ));
    }
    let changes = before
        .as_ref()
        .map(|p| envdef::changes(&p.revision, &now))
        .unwrap_or_default();
    let mut store = core.store.lock().await;
    let revision_ref = store_revision(&store, &now, &task.execution_profile).unwrap_or_default();
    let offset = append(
        &mut store,
        core,
        Lineage::task(core.tenant_id, task.session_id, task.task_id),
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "EnvironmentRebuilt",
            &TaskEvent::EnvironmentRebuilt {
                from_digest: before
                    .as_ref()
                    .map(|p| p.revision.digest.clone())
                    .unwrap_or_default(),
                to_digest: now.digest.clone(),
                revision_ref: revision_ref.clone(),
                changes: changes.clone(),
            },
            actor.clone(),
        )],
    )
    .map_err(|e| e.to_string())?;
    drop(store);
    let to = now.digest.clone();
    core.tools.environments.set(
        task.task_id,
        Arc::new(PinnedEnvironment {
            revision: now,
            revision_ref,
            profile: task.execution_profile.clone(),
        }),
    );
    Ok((
        before
            .map(|p| p.revision.digest.clone())
            .unwrap_or_default(),
        to,
        changes,
        offset,
    ))
}

#[cfg(test)]
mod tests {
    use super::keeps_pin;

    #[test]
    fn a_pin_is_kept_within_its_profile_and_dropped_when_the_profile_changed() {
        // Same profile: the pin is this run's to compare against (staleness
        // is meaningful).
        assert!(keeps_pin("local_trusted", "local_trusted"));
        assert!(keeps_pin("cloud_isolated", "cloud_isolated"));
        // A local→cloud handoff (or any RebindTaskWorkspace) changes the
        // profile: the old pin is not kept, so the resumed run re-pins the
        // sandbox's environment rather than parking ENVIRONMENT_STALE.
        assert!(!keeps_pin("local_trusted", "cloud_isolated"));
        assert!(!keeps_pin("cloud_isolated", "local_trusted"));
        // A pin from before this field existed carries no profile and is
        // kept (its digest still describes the same execution).
        assert!(keeps_pin("", "local_trusted"));
        assert!(keeps_pin("", "cloud_isolated"));
    }
}
