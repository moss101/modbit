//! The Hook Bus in the Core (REQ-EV-0042, REQ-EV-0139, REQ-EV-0240; docs/16
//! "Hook Bus"). `modbit_tools::hooks` owns the types, the handler runner and
//! the monotonic fold; this is where registrations come from, where each
//! invocation is journaled, and where an extension is loaded and unloaded.
//!
//! Registrations come from two places. The configuration layers (Device >
//! Admin > Project > User) declare hooks in their `hooks` list, each a typed
//! declaration; a repository's own (Project) hooks are in force only once
//! the session has trusted that repository, since they are code the
//! repository asks the Core to run. A session may load an extension — a
//! directory whose `modbit-extension.json` registers typed handlers — and
//! unload it; both are on the session's log, so a restarted Core rebuilds
//! exactly the extensions that are loaded.
//!
//! Every invocation is a `HookInvoked` on the task's log with what the
//! handler answered and whether it changed anything. A fail-closed hook that
//! fails after a step (the effect has happened) stops the run at its next
//! boundary.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::session::SessionEvent;
use modbit_domain::task::{HookRefusal, Task, TaskEvent};
use modbit_domain::{RunId, SessionId, TaskId, TenantId, TurnId};
use modbit_event_store::{AppendRequest, EventStore, NewEvent};
use modbit_policy::config::{Authority, ResolvedConfig};
use modbit_tools::hooks::{
    EXTENSION_MANIFEST, ExtensionManifest, HookEffect, HookPoint, HookRecord, HookRequest,
    HookSource, HookSpec, Registration,
};
use serde_json::Value;
use sha2::Digest;
use tokio::sync::Mutex;

/// One extension a session has loaded.
#[derive(Clone, Debug)]
pub(crate) struct LoadedExtension {
    /// The load's id.
    pub extension_id: String,
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Directory it was loaded from.
    pub path: String,
    /// sha256 of the manifest as loaded.
    pub digest: String,
    /// Its hooks.
    pub hooks: Vec<HookSpec>,
    /// Cleared the moment it is unloaded: an answer from one of its handlers
    /// that arrives afterwards is discarded.
    pub live: Arc<AtomicBool>,
}

/// The Core's hook state: loaded extensions per session and the runs a
/// fail-closed after-hook has stopped.
#[derive(Default)]
pub struct HookBus {
    extensions: std::sync::Mutex<HashMap<SessionId, Vec<LoadedExtension>>>,
    rebuilt: std::sync::Mutex<HashSet<SessionId>>,
    halted: std::sync::Mutex<HashMap<TaskId, (String, String)>>,
}

fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn authority_label(a: Authority) -> &'static str {
    match a {
        Authority::Device => "device",
        Authority::Admin => "admin",
        Authority::Project => "project",
        Authority::User => "user",
    }
}

impl HookBus {
    /// The session's loaded extensions, rebuilt from its log the first time
    /// this process asks.
    fn extensions_of(&self, store: &EventStore, session: SessionId) -> Vec<LoadedExtension> {
        if lock(&self.rebuilt).insert(session) {
            let mut loaded: Vec<LoadedExtension> = Vec::new();
            let mut after = 0u64;
            while let Ok(batch) = store.read_session(&session, after, 512) {
                if batch.is_empty() {
                    break;
                }
                for ev in &batch {
                    after = ev.offset;
                    let t = ev.envelope.event_type.as_str();
                    if t != "ExtensionLoaded" && t != "ExtensionUnloaded" {
                        continue;
                    }
                    let Ok(p) = store.payload(&ev.envelope) else {
                        continue;
                    };
                    let id = p["extension_id"].as_str().unwrap_or_default().to_owned();
                    if t == "ExtensionUnloaded" {
                        loaded.retain(|e| e.extension_id != id);
                        continue;
                    }
                    if let Ok(m) =
                        ExtensionManifest::parse(p["manifest_json"].as_str().unwrap_or_default())
                    {
                        loaded.push(LoadedExtension {
                            extension_id: id,
                            name: m.name,
                            version: m.version,
                            path: p["path"].as_str().unwrap_or_default().to_owned(),
                            digest: p["manifest_digest"].as_str().unwrap_or_default().to_owned(),
                            hooks: m.hooks,
                            live: Arc::new(AtomicBool::new(true)),
                        });
                    }
                }
            }
            let mut ext = lock(&self.extensions);
            let entry = ext.entry(session).or_default();
            // Anything loaded in this process meanwhile is already there.
            for l in loaded {
                if !entry.iter().any(|e| e.extension_id == l.extension_id) {
                    entry.push(l);
                }
            }
        }
        lock(&self.extensions)
            .get(&session)
            .cloned()
            .unwrap_or_default()
    }

    /// Stop `task`'s run at its next boundary (a fail-closed after-hook
    /// failed).
    pub(crate) fn halt(&self, task: TaskId, code: String, reason: String) {
        lock(&self.halted).entry(task).or_insert((code, reason));
    }

    /// Why `task`'s run must stop, once.
    pub(crate) fn take_halt(&self, task: TaskId) -> Option<(String, String)> {
        lock(&self.halted).remove(&task)
    }
}

/// The registrations in force for a task, and the declarations refused.
pub(crate) struct Resolution {
    /// In force, in order: configuration by authority, then extensions in
    /// load order.
    pub active: Vec<Registration>,
    /// Refused, with why.
    pub refused: Vec<HookRefusal>,
    /// Extension id → whether it is still loaded.
    pub flags: HashMap<String, Arc<AtomicBool>>,
}

/// Resolve the task's hooks from its configuration and its session's
/// extensions. `trusted` is asked only when a repository declares hooks.
pub(crate) fn resolve(
    bus: &HookBus,
    store: &EventStore,
    session_id: SessionId,
    workspace_root: Option<&str>,
    cfg: &ResolvedConfig,
) -> Resolution {
    let mut active: Vec<Registration> = Vec::new();
    let mut refused = Vec::new();
    let mut ids = HashSet::new();
    let mut trusted: Option<bool> = None;
    for r in &cfg.hooks {
        let authority = r.provenance.decided_by;
        let source = HookSource::Config {
            authority: authority_label(authority).to_owned(),
        };
        let spec = match HookSpec::parse(&r.value) {
            Ok(s) => s,
            Err(reason) => {
                refused.push(HookRefusal {
                    hook: r.value.chars().take(120).collect(),
                    source: source.label(),
                    reason,
                });
                continue;
            }
        };
        if authority == Authority::Project {
            let root = workspace_root.unwrap_or_default();
            let ok = *trusted.get_or_insert_with(|| {
                !root.is_empty() && crate::onboarding::is_trusted(store, session_id, root)
            });
            if !ok {
                refused.push(HookRefusal {
                    hook: spec.name.clone(),
                    source: source.label(),
                    reason: "a repository's hooks are code it asks the Core to run: they are in force only once the session trusts the repository (TrustRepository)".into(),
                });
                continue;
            }
        }
        let reg = Registration::new(source, spec);
        if !ids.insert(reg.id.clone()) {
            refused.push(HookRefusal {
                hook: reg.spec.name.clone(),
                source: reg.source.label(),
                reason: "declared twice in the same layer".into(),
            });
            continue;
        }
        active.push(reg);
    }
    let mut flags = HashMap::new();
    for ext in bus.extensions_of(store, session_id) {
        flags.insert(ext.extension_id.clone(), Arc::clone(&ext.live));
        for spec in &ext.hooks {
            active.push(Registration::new(
                HookSource::Extension {
                    extension_id: ext.extension_id.clone(),
                    name: ext.name.clone(),
                },
                spec.clone(),
            ));
        }
    }
    Resolution {
        active,
        refused,
        flags,
    }
}

/// A call a hook rewrote: its final arguments, and the workspace as it was
/// at the rewrite for the files the rewritten change targets (the change
/// record diffs against it).
pub(crate) struct Rewrite {
    pub arguments_json: String,
    pub pre_bytes: HashMap<String, Option<Vec<u8>>>,
    pub pre_revision: u64,
}

/// Where one task's hooks fire from, and where their records go.
#[derive(Clone)]
pub(crate) struct HookScope {
    pub store: Arc<Mutex<EventStore>>,
    pub bus: Arc<HookBus>,
    pub tenant_id: TenantId,
    pub session_id: SessionId,
    pub task_id: TaskId,
    pub run_id: Option<RunId>,
    pub turn_id: Option<TurnId>,
    pub lease_generation: Option<u64>,
    pub actor: Actor,
    pub cwd: Option<PathBuf>,
    pub registrations: Arc<Vec<Registration>>,
    pub flags: Arc<HashMap<String, Arc<AtomicBool>>>,
    /// The call's workspace, for the snapshot a rewritten change needs.
    pub workspace: Option<Arc<Mutex<modbit_workspace::WorkspaceService>>>,
    /// The last rewrite a hook made to the call, taken by the caller.
    pub rewritten: Arc<std::sync::Mutex<Option<Rewrite>>>,
}

impl HookScope {
    /// Whether the scope has any hook at `point` at all (so a caller builds
    /// no payload for nothing).
    pub(crate) fn has(&self, point: HookPoint) -> bool {
        self.registrations.iter().any(|r| r.spec.point == point)
    }

    /// Run the hooks at `point` and journal every invocation.
    pub(crate) async fn fire(
        &self,
        point: HookPoint,
        tool: Option<&str>,
        payload: Value,
    ) -> HookEffect {
        if !self.has(point) {
            return HookEffect::default();
        }
        let flags = Arc::clone(&self.flags);
        let live = move |reg: &Registration| match &reg.source {
            HookSource::Config { .. } => true,
            HookSource::Extension { extension_id, .. } => flags
                .get(extension_id)
                .is_some_and(|f| f.load(Ordering::SeqCst)),
        };
        let req = HookRequest {
            point,
            task_id: self.task_id.to_string(),
            run_id: self.run_id.map(|r| r.to_string()),
            tool: tool.map(str::to_owned),
            payload,
        };
        let effect =
            modbit_tools::hooks::fire(&self.registrations, &req, self.cwd.as_deref(), &live).await;
        self.journal(&effect.records).await;
        effect
    }

    async fn journal(&self, records: &[HookRecord]) {
        if records.is_empty() {
            return;
        }
        let mut store = self.store.lock().await;
        let events: Vec<NewEvent> = records
            .iter()
            .map(|r| {
                let (arguments_hash, arguments_ref) = match &r.arguments {
                    Some(a) => {
                        let json = a.to_string();
                        (
                            modbit_tools::arguments_hash(&json),
                            store.objects().put(json.as_bytes()).ok(),
                        )
                    }
                    None => (None, None),
                };
                crate::runtime::typed(
                    "HookInvoked",
                    &TaskEvent::HookInvoked {
                        hook: r.hook.clone(),
                        source: r.source.clone(),
                        point: r.point.label().to_owned(),
                        mode: r.mode.label().to_owned(),
                        fail_policy: r.fail_policy.label().to_owned(),
                        outcome: r.outcome.label().to_owned(),
                        applied: r.applied,
                        duration_ms: r.duration_ms,
                        detail: r.detail.chars().take(2_000).collect(),
                        tool: r.tool.clone(),
                        arguments_hash,
                        arguments_ref,
                    },
                    self.actor.clone(),
                )
            })
            .collect();
        let req = AppendRequest {
            tenant_id: self.tenant_id,
            session_id: self.session_id,
            task_id: Some(self.task_id),
            run_id: self.run_id,
            turn_id: self.turn_id,
            step_id: None,
            aggregate_type: AggregateType::Task,
            aggregate_id: *self.task_id.as_bytes(),
            expected_sequence: None,
            events,
        };
        // The record of what a hook did is not optional: a failure to write
        // it is the Core's own, and the next append will say so.
        let _ = match self.lease_generation {
            Some(g) => store.append_fenced(req, g),
            None => store.append(req),
        };
    }
}

impl modbit_tools::hooks::HookPort for HookScope {
    fn before<'a>(
        &'a self,
        point: HookPoint,
        tool: &'a str,
        effect_class: modbit_tools::EffectClass,
        arguments: &'a Value,
    ) -> modbit_tools::registry::BoxFuture<'a, HookEffect> {
        Box::pin(async move {
            let effect = self
                .fire(
                    point,
                    Some(tool),
                    serde_json::json!({
                        "tool": tool,
                        "effect_class": effect_class,
                        "arguments": arguments,
                    }),
                )
                .await;
            // A rewritten change targets what it now names: the workspace
            // is read for those files here, before the kernel decides and
            // before anything is written.
            if let Some(a) = &effect.arguments {
                let arguments_json = a.to_string();
                let targets = crate::tools::change_targets(tool, &arguments_json);
                let (pre_bytes, pre_revision) = match (&self.workspace, targets.is_empty()) {
                    (Some(ws), false) => {
                        let ws = ws.lock().await;
                        (
                            targets
                                .iter()
                                .map(|p| (p.clone(), crate::tools::read_workspace_file(&ws, p)))
                                .collect(),
                            ws.revision().number,
                        )
                    }
                    _ => (HashMap::new(), 0),
                };
                *lock(&self.rewritten) = Some(Rewrite {
                    arguments_json,
                    pre_bytes,
                    pre_revision,
                });
            }
            effect
        })
    }

    fn after<'a>(
        &'a self,
        point: HookPoint,
        tool: &'a str,
        effect_class: modbit_tools::EffectClass,
        result: &'a Value,
    ) -> modbit_tools::registry::BoxFuture<'a, HookEffect> {
        Box::pin(async move {
            let effect = self
                .fire(
                    point,
                    Some(tool),
                    serde_json::json!({
                        "tool": tool,
                        "effect_class": effect_class,
                        "result": result,
                    }),
                )
                .await;
            if let Some((code, reason)) = &effect.denied {
                self.bus.halt(self.task_id, code.clone(), reason.clone());
            }
            effect
        })
    }
}

/// The scope for `task`: its pinned configuration's hooks and its
/// session's extensions.
pub(crate) async fn scope(
    core: &crate::server::Core,
    task: &Task,
    run_id: Option<RunId>,
    turn_id: Option<TurnId>,
    lease_generation: Option<u64>,
    actor: &Actor,
) -> (HookScope, Vec<HookRefusal>) {
    let cfg = core.tools.configurations.for_task(
        task.task_id,
        &core.data_dir,
        task.workspace_root.as_deref(),
    );
    let resolution = {
        let store = core.store.lock().await;
        resolve(
            &core.tools.hooks,
            &store,
            task.session_id,
            task.workspace_root.as_deref(),
            &cfg,
        )
    };
    (
        HookScope {
            store: Arc::clone(&core.store),
            bus: Arc::clone(&core.tools.hooks),
            tenant_id: core.tenant_id,
            session_id: task.session_id,
            task_id: task.task_id,
            run_id,
            turn_id,
            lease_generation,
            actor: actor.clone(),
            cwd: task.workspace_root.as_ref().map(PathBuf::from),
            registrations: Arc::new(resolution.active),
            flags: Arc::new(resolution.flags),
            workspace: None,
            rewritten: Arc::new(std::sync::Mutex::new(None)),
        },
        resolution.refused,
    )
}

/// `LoadExtension`: read the directory's manifest, register its handlers
/// for the session and record it.
pub(crate) async fn load(
    core: &crate::server::Core,
    session_id: SessionId,
    path: &str,
    actor: Actor,
) -> Result<modbit_protocol::v1::ExtensionLoadedView, (String, String)> {
    let dir = std::path::Path::new(path);
    let dir = dir
        .canonicalize()
        .map_err(|e| ("EXTENSION_NOT_FOUND".to_owned(), format!("{path}: {e}")))?;
    let manifest_path = dir.join(EXTENSION_MANIFEST);
    let bytes = std::fs::read(&manifest_path).map_err(|e| {
        (
            "EXTENSION_MANIFEST_MISSING".to_owned(),
            format!("{}: {e}", manifest_path.display()),
        )
    })?;
    let json = String::from_utf8(bytes).map_err(|_| {
        (
            "EXTENSION_INVALID".to_owned(),
            "the manifest is not UTF-8".to_owned(),
        )
    })?;
    let manifest =
        ExtensionManifest::parse(&json).map_err(|e| ("EXTENSION_INVALID".to_owned(), e))?;
    let digest = hex::encode(sha2::Sha256::digest(json.as_bytes()));
    let mut store = core.store.lock().await;
    let loaded = core.tools.hooks.extensions_of(&store, session_id);
    if let Some(e) = loaded.iter().find(|e| e.name == manifest.name) {
        return Err((
            "EXTENSION_ALREADY_LOADED".to_owned(),
            format!(
                "`{}` is loaded as {} (version {}); unload it first",
                e.name, e.extension_id, e.version
            ),
        ));
    }
    let extension_id = modbit_domain::RunStepId::new().to_string();
    let hooks: Vec<String> = manifest
        .hooks
        .iter()
        .map(|h| format!("{}@{}", h.name, h.point.label()))
        .collect();
    let dir_s = dir.to_string_lossy().into_owned();
    crate::runtime::append(
        &mut store,
        core,
        crate::runtime::Lineage::session(core.tenant_id, session_id),
        AggregateType::Session,
        *session_id.as_bytes(),
        vec![crate::runtime::typed(
            "ExtensionLoaded",
            &SessionEvent::ExtensionLoaded {
                extension_id: extension_id.clone(),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                path: dir_s.clone(),
                manifest_digest: digest.clone(),
                manifest_json: json,
                hooks: hooks.clone(),
            },
            actor,
        )],
    )
    .map_err(|e| ("STORE".to_owned(), e))?;
    lock(&core.tools.hooks.extensions)
        .entry(session_id)
        .or_default()
        .push(LoadedExtension {
            extension_id: extension_id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            path: dir_s,
            digest: digest.clone(),
            hooks: manifest.hooks,
            live: Arc::new(AtomicBool::new(true)),
        });
    Ok(modbit_protocol::v1::ExtensionLoadedView {
        extension_id,
        name: manifest.name,
        version: manifest.version,
        manifest_digest: digest,
        hooks,
    })
}

/// `UnloadExtension`: the extension's handlers stop being live at once — an
/// answer already on its way is discarded — then leave the session.
pub(crate) async fn unload(
    core: &crate::server::Core,
    session_id: SessionId,
    extension_id: &str,
    actor: Actor,
) -> Result<modbit_protocol::v1::ExtensionUnloadedView, (String, String)> {
    let mut store = core.store.lock().await;
    let loaded = core.tools.hooks.extensions_of(&store, session_id);
    let Some(ext) = loaded.iter().find(|e| e.extension_id == extension_id) else {
        return Err((
            "UNKNOWN_EXTENSION".to_owned(),
            format!("no extension {extension_id} is loaded in this session"),
        ));
    };
    ext.live.store(false, Ordering::SeqCst);
    let removed: Vec<String> = ext
        .hooks
        .iter()
        .map(|h| format!("{}@{}", h.name, h.point.label()))
        .collect();
    crate::runtime::append(
        &mut store,
        core,
        crate::runtime::Lineage::session(core.tenant_id, session_id),
        AggregateType::Session,
        *session_id.as_bytes(),
        vec![crate::runtime::typed(
            "ExtensionUnloaded",
            &SessionEvent::ExtensionUnloaded {
                extension_id: extension_id.to_owned(),
                name: ext.name.clone(),
                removed: removed.clone(),
            },
            actor,
        )],
    )
    .map_err(|e| ("STORE".to_owned(), e))?;
    if let Some(list) = lock(&core.tools.hooks.extensions).get_mut(&session_id) {
        list.retain(|e| e.extension_id != extension_id);
    }
    Ok(modbit_protocol::v1::ExtensionUnloadedView {
        extension_id: extension_id.to_owned(),
        name: ext.name.clone(),
        removed,
    })
}

/// `ListHooks`: what is in force for a task, and what was refused.
pub(crate) async fn list(
    core: &crate::server::Core,
    task: &Task,
) -> modbit_protocol::v1::HookListView {
    let (scope, refused) = scope(core, task, None, None, None, &Actor::Core("hooks".into())).await;
    let extensions = {
        let store = core.store.lock().await;
        core.tools.hooks.extensions_of(&store, task.session_id)
    };
    modbit_protocol::v1::HookListView {
        extensions: extensions
            .iter()
            .map(|e| {
                format!(
                    "{} {}@{} sha256:{} {}",
                    e.extension_id, e.name, e.version, e.digest, e.path
                )
            })
            .collect(),
        hooks: scope
            .registrations
            .iter()
            .map(|r| modbit_protocol::v1::HookView {
                id: r.id.clone(),
                source: r.source.label(),
                point: r.spec.point.label().to_owned(),
                mode: r.spec.mode.label().to_owned(),
                fail_policy: r.spec.fail_policy.label().to_owned(),
                timeout_ms: r.spec.timeout_ms,
                tools: r.spec.tools.clone(),
                command: r.spec.command.clone(),
            })
            .collect(),
        refused: refused
            .into_iter()
            .map(|r| format!("{} ({}): {}", r.hook, r.source, r.reason))
            .collect(),
    }
}

/// Record the hooks in force as a run starts.
pub(crate) fn resolved_event(
    scope: &HookScope,
    refused: Vec<HookRefusal>,
    actor: &Actor,
) -> NewEvent {
    crate::runtime::typed(
        "HooksResolved",
        &TaskEvent::HooksResolved {
            active: scope
                .registrations
                .iter()
                .map(|r| format!("{}@{}", r.id, r.spec.point.label()))
                .collect(),
            refused,
        },
        actor.clone(),
    )
}
