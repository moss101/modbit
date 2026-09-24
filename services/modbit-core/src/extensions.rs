//! The Extension System in the Core (REQ-EV-0138, REQ-EV-0225, REQ-EV-0240;
//! docs/16 "Extension System"): inspecting, loading, trusting and unloading
//! an extension for a session, and handing each part of it to the owner that
//! already governs that kind of thing — hooks to the Hook Bus, tool servers
//! to the External Tool Hub, commands to the task's input queue, providers
//! to the Provider Gateway.
//!
//! Only an extension whose signature verifies against a publisher key this
//! Core trusts (`MODBIT_EXTENSION_KEYS`) is active on load. Any other is
//! quarantined: on the session's log with why, and inert — none of its
//! hooks run, its tool servers are not served, its commands are refused and
//! its providers are not registered — until the person trusts that exact
//! manifest. A manifest whose signature does not verify was changed after
//! it was signed and is never trusted.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use modbit_domain::SessionId;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::session::SessionEvent;
use modbit_domain::task::{InputMode, Task, TaskEvent};
use modbit_protocol::v1 as wire;
use modbit_tools::extensions::{
    EXTENSION_MANIFEST, EXTENSION_SIGNATURE, ExtensionManifest, SignatureStatus,
};
use sha2::Digest;

use crate::hooks::LoadedExtension;
use crate::server::Core;

fn refuse(code: &str, detail: impl Into<String>) -> (String, String) {
    (code.to_owned(), detail.into())
}

/// The publisher keys this Core trusts: `MODBIT_EXTENSION_KEYS="<key id>:<64
/// hex chars>,..."`. None trusted means every extension is quarantined.
fn trusted_keys() -> std::collections::BTreeMap<String, [u8; 32]> {
    modbit_tools::extensions::trusted_keys(
        &std::env::var("MODBIT_EXTENSION_KEYS").unwrap_or_default(),
    )
}

/// An extension read from its directory, before anything is registered.
struct Read {
    dir: String,
    json: String,
    digest: String,
    manifest: ExtensionManifest,
    signature: SignatureStatus,
}

fn read(path: &str) -> Result<Read, (String, String)> {
    let dir = std::path::Path::new(path)
        .canonicalize()
        .map_err(|e| refuse("EXTENSION_NOT_FOUND", format!("{path}: {e}")))?;
    let manifest_path = dir.join(EXTENSION_MANIFEST);
    let bytes = std::fs::read(&manifest_path).map_err(|e| {
        refuse(
            "EXTENSION_MANIFEST_MISSING",
            format!("{}: {e}", manifest_path.display()),
        )
    })?;
    let json = String::from_utf8(bytes)
        .map_err(|_| refuse("EXTENSION_INVALID", "the manifest is not UTF-8"))?;
    let manifest = ExtensionManifest::parse(&json).map_err(|e| refuse("EXTENSION_INVALID", e))?;
    check_files(&dir, &manifest)?;
    let signature_text = std::fs::read_to_string(dir.join(EXTENSION_SIGNATURE)).ok();
    let signature = modbit_tools::extensions::verify(
        json.as_bytes(),
        signature_text.as_deref(),
        &trusted_keys(),
    );
    Ok(Read {
        dir: dir.to_string_lossy().into_owned(),
        digest: hex::encode(sha2::Sha256::digest(json.as_bytes())),
        json,
        manifest,
        signature,
    })
}

/// Every file in the extension's directory is the manifest, its signature,
/// or listed in the manifest with its exact digest: what was signed or
/// trusted is everything that is there.
fn check_files(dir: &std::path::Path, m: &ExtensionManifest) -> Result<(), (String, String)> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if let Ok(r) = p.strip_prefix(root) {
                out.push(r.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut present = Vec::new();
    walk(dir, dir, &mut present);
    for f in &present {
        if f == EXTENSION_MANIFEST || f == EXTENSION_SIGNATURE {
            continue;
        }
        let Some(want) = m.files.get(f) else {
            return Err(refuse(
                "EXTENSION_INVALID",
                format!("`{f}` is in the extension but not listed in its manifest"),
            ));
        };
        let got = std::fs::read(dir.join(f))
            .map(|b| hex::encode(sha2::Sha256::digest(&b)))
            .unwrap_or_default();
        if &got != want {
            return Err(refuse(
                "EXTENSION_INVALID",
                format!("`{f}` is not the file the manifest lists (sha256 {got}, listed {want})"),
            ));
        }
    }
    if let Some(missing) = m.files.keys().find(|f| !present.contains(f)) {
        return Err(refuse(
            "EXTENSION_INVALID",
            format!("the manifest lists `{missing}`, which is not in the extension"),
        ));
    }
    Ok(())
}

/// The directories of a session's active extensions, for the owners that
/// read `rules/`, `agents/` and `skills/` from them.
pub(crate) fn active_dirs(core: &Core, session_id: SessionId, sub: &str) -> Vec<std::path::PathBuf> {
    core.tools
        .hooks
        .loaded_now(session_id)
        .iter()
        .filter(|e| e.active())
        .map(|e| std::path::PathBuf::from(&e.path).join(sub))
        .filter(|p| p.is_dir())
        .collect()
}

fn names(m: &ExtensionManifest) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    (
        m.hooks
            .iter()
            .map(|h| format!("{}@{}", h.name, h.point.label()))
            .collect(),
        m.tools.iter().map(|t| t.name.clone()).collect(),
        m.commands.iter().map(|c| c.name.clone()).collect(),
        m.providers.iter().map(|p| p.endpoint(&m.name)).collect(),
    )
}

/// `InspectExtension`: everything an extension would do, and who vouches for
/// it, before it is loaded (REQ-EV-0225). Reads the directory; changes
/// nothing.
pub(crate) fn inspect(path: &str) -> Result<wire::ExtensionInspectionView, (String, String)> {
    let r = read(path)?;
    Ok(wire::ExtensionInspectionView {
        name: r.manifest.name.clone(),
        version: r.manifest.version.clone(),
        publisher: r.manifest.publisher.clone(),
        source: r.manifest.source.clone(),
        manifest_digest: r.digest,
        signature: r.signature.label(),
        quarantine_reason: r.signature.quarantine_reason().unwrap_or_default(),
        capabilities: r.manifest.capabilities(),
        path: r.dir,
    })
}

fn view(ext: &LoadedExtension, providers: Vec<String>) -> wire::ExtensionLoadedView {
    let (hooks, tools, commands, _) = names(&ext.manifest);
    wire::ExtensionLoadedView {
        extension_id: ext.extension_id.clone(),
        name: ext.manifest.name.clone(),
        version: ext.manifest.version.clone(),
        manifest_digest: ext.digest.clone(),
        hooks,
        signature: ext.signature.clone(),
        quarantined: ext.quarantine.clone().unwrap_or_default(),
        tools,
        commands,
        providers,
        capabilities: ext.manifest.capabilities(),
    }
}

/// `LoadExtension`: read the directory, verify the signature and record the
/// load — active when verified, quarantined otherwise. `expected_digest`,
/// when given, is the manifest the person inspected: a different one is
/// refused (`EXTENSION_CHANGED`).
pub(crate) async fn load(
    core: &Core,
    session_id: SessionId,
    path: &str,
    expected_digest: &str,
    actor: Actor,
) -> Result<wire::ExtensionLoadedView, (String, String)> {
    let r = read(path)?;
    if !expected_digest.is_empty() && expected_digest != r.digest {
        return Err(refuse(
            "EXTENSION_CHANGED",
            format!(
                "the manifest is sha256:{}, not the sha256:{expected_digest} that was inspected",
                r.digest
            ),
        ));
    }
    let quarantine = r.signature.quarantine_reason();
    let (hooks, ..) = names(&r.manifest);
    let ext = {
        let mut store = core.store.lock().await;
        let loaded = core.tools.hooks.extensions_of(&store, session_id);
        if let Some(e) = loaded.iter().find(|e| e.manifest.name == r.manifest.name) {
            return Err(refuse(
                "EXTENSION_ALREADY_LOADED",
                format!(
                    "`{}` is loaded as {} (version {}); unload it first",
                    e.manifest.name, e.extension_id, e.manifest.version
                ),
            ));
        }
        let extension_id = modbit_domain::RunStepId::new().to_string();
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
                    name: r.manifest.name.clone(),
                    version: r.manifest.version.clone(),
                    path: r.dir.clone(),
                    manifest_digest: r.digest.clone(),
                    manifest_json: r.json.clone(),
                    hooks,
                    signature: r.signature.label(),
                    quarantined: quarantine.clone(),
                },
                actor,
            )],
        )
        .map_err(|e| refuse("STORE", e))?;
        let ext = LoadedExtension {
            extension_id,
            path: r.dir,
            digest: r.digest,
            signature: r.signature.label(),
            live: Arc::new(AtomicBool::new(quarantine.is_none())),
            quarantine,
            manifest: r.manifest,
        };
        core.tools.hooks.add(session_id, ext.clone());
        ext
    };
    let providers = register_providers(core, &ext);
    Ok(view(&ext, providers))
}

/// `TrustExtension`: the person releases a quarantined extension — this
/// exact manifest, which they have seen. An extension whose signature does
/// not verify is never released (`EXTENSION_TAMPERED`).
pub(crate) async fn trust(
    core: &Core,
    session_id: SessionId,
    extension_id: &str,
    manifest_digest: &str,
    actor: Actor,
) -> Result<wire::ExtensionLoadedView, (String, String)> {
    let ext = {
        let mut store = core.store.lock().await;
        let loaded = core.tools.hooks.extensions_of(&store, session_id);
        let Some(ext) = loaded.into_iter().find(|e| e.extension_id == extension_id) else {
            return Err(refuse(
                "UNKNOWN_EXTENSION",
                format!("no extension {extension_id} is loaded in this session"),
            ));
        };
        if ext.quarantine.is_none() {
            return Err(refuse(
                "EXTENSION_NOT_QUARANTINED",
                format!("`{}` is already active", ext.manifest.name),
            ));
        }
        if ext.signature.starts_with("INVALID") {
            return Err(refuse(
                "EXTENSION_TAMPERED",
                format!(
                    "`{}`: {}",
                    ext.manifest.name,
                    ext.quarantine.clone().unwrap_or_default()
                ),
            ));
        }
        if manifest_digest != ext.digest {
            return Err(refuse(
                "DIGEST_MISMATCH",
                format!(
                    "the loaded manifest is sha256:{}; trust names sha256:{manifest_digest}",
                    ext.digest
                ),
            ));
        }
        crate::runtime::append(
            &mut store,
            core,
            crate::runtime::Lineage::session(core.tenant_id, session_id),
            AggregateType::Session,
            *session_id.as_bytes(),
            vec![crate::runtime::typed(
                "ExtensionTrusted",
                &SessionEvent::ExtensionTrusted {
                    extension_id: extension_id.to_owned(),
                    name: ext.manifest.name.clone(),
                    manifest_digest: ext.digest.clone(),
                },
                actor,
            )],
        )
        .map_err(|e| refuse("STORE", e))?;
        core.tools.hooks.activate(session_id, extension_id);
        LoadedExtension {
            quarantine: None,
            ..ext
        }
    };
    let providers = register_providers(core, &ext);
    Ok(view(&ext, providers))
}

/// `UnloadExtension`: everything the extension added stops at once — its
/// registrations are no longer live, so an answer already on its way is
/// discarded — then it leaves the session, and its providers leave the
/// gateway unless another session has the same extension active.
pub(crate) async fn unload(
    core: &Core,
    session_id: SessionId,
    extension_id: &str,
    actor: Actor,
) -> Result<wire::ExtensionUnloadedView, (String, String)> {
    let mut store = core.store.lock().await;
    let loaded = core.tools.hooks.extensions_of(&store, session_id);
    let Some(ext) = loaded.iter().find(|e| e.extension_id == extension_id) else {
        return Err(refuse(
            "UNKNOWN_EXTENSION",
            format!("no extension {extension_id} is loaded in this session"),
        ));
    };
    ext.live.store(false, Ordering::SeqCst);
    let (hooks, tools, commands, providers) = names(&ext.manifest);
    let removed: Vec<String> = hooks
        .into_iter()
        .chain(tools.into_iter().map(|t| format!("tool server {t}")))
        .chain(commands.into_iter().map(|c| format!("command {c}")))
        .chain(providers.iter().map(|p| format!("provider {p}")))
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
                name: ext.manifest.name.clone(),
                removed: removed.clone(),
            },
            actor,
        )],
    )
    .map_err(|e| refuse("STORE", e))?;
    core.tools.hooks.remove(session_id, extension_id);
    let still_active = core.tools.hooks.active_anywhere(&ext.manifest.name);
    if !still_active {
        for p in &providers {
            core.gateway.remove_endpoint(p);
        }
    }
    Ok(wire::ExtensionUnloadedView {
        extension_id: extension_id.to_owned(),
        name: ext.manifest.name.clone(),
        removed,
    })
}

/// Register an active extension's providers with the gateway, their
/// credentials resolved from the Core's custody by handle. Returns what was
/// registered, and why any was not.
fn register_providers(core: &Core, ext: &LoadedExtension) -> Vec<String> {
    if !ext.active() {
        return vec![];
    }
    let mut out = Vec::new();
    for p in &ext.manifest.providers {
        let name = p.endpoint(&ext.manifest.name);
        let credential = match &p.credential {
            None => modbit_providers::SecretHandle::None,
            Some(handle) => match core.tools.mcp.credential(handle) {
                Some(v) => modbit_providers::SecretHandle::Inline(v),
                None => {
                    out.push(format!(
                        "{name}: not registered: the credential `{handle}` is not in this Core's custody"
                    ));
                    continue;
                }
            },
        };
        core.gateway.configure_endpoint(modbit_providers::Endpoint {
            name: name.clone(),
            kind: if p.kind == "anthropic" {
                modbit_providers::ProviderKind::Anthropic
            } else {
                modbit_providers::ProviderKind::OpenAi
            },
            base_url: p.base_url.trim().trim_end_matches('/').to_owned(),
            credential,
            models: p
                .models
                .iter()
                .map(|m| modbit_providers::ModelCapability {
                    model: m.model.clone(),
                    context_tokens: m.context_tokens,
                    max_output_tokens: m.max_output_tokens,
                    tools: m.tools,
                    parallel_tools: false,
                    vision: m.vision,
                    input_modalities: if m.vision {
                        vec!["text".into(), "image".into()]
                    } else {
                        vec!["text".into()]
                    },
                    reasoning: false,
                    structured_output: false,
                    agent_loop: m.tools,
                    input_price_per_mtok: 0.0,
                    output_price_per_mtok: 0.0,
                })
                .collect(),
            max_retries: 2,
            auth: Default::default(),
            extra_body: Default::default(),
        });
        out.push(name);
    }
    out
}

/// Make sure a session's active extensions' providers are registered — the
/// gateway is rebuilt empty when the Core restarts, the session log is not.
pub(crate) async fn ensure_session(core: &Core, session_id: SessionId) {
    let loaded = {
        let store = core.store.lock().await;
        core.tools.hooks.extensions_of(&store, session_id)
    };
    for ext in loaded.iter().filter(|e| e.active()) {
        let _ = register_providers(core, ext);
    }
}

/// The tool servers of a session's active extensions, as External Tool Hub
/// servers (their layer names the extension).
pub(crate) fn servers_of(loaded: &[LoadedExtension]) -> Vec<crate::mcp::ConfiguredServer> {
    loaded
        .iter()
        .filter(|e| e.active())
        .flat_map(|e| {
            e.manifest.tools.iter().map(move |t| {
                let mut config = t.clone();
                config.layer = format!("extension:{}", e.manifest.name);
                // Active means its publisher's signature verified or the
                // person trusted this manifest: that is the host's trust.
                config.trust = modbit_mcp::Trust::Trusted;
                crate::mcp::ConfiguredServer {
                    config,
                    provenance: vec![format!(
                        "declared by extension {}@{} (sha256:{}, {})",
                        e.manifest.name, e.manifest.version, e.digest, e.signature
                    )],
                }
            })
        })
        .collect()
}

/// `RunExtensionCommand`: an active extension's instruction template,
/// expanded with the person's arguments, queued on the task as the person's
/// input with the command as its provenance.
pub(crate) async fn run_command(
    core: &Core,
    task: &Task,
    command: &str,
    arguments: &str,
    input_id: &str,
    actor: Actor,
) -> Result<wire::InputQueued, (String, String)> {
    let Some((ext_name, cmd_name)) = command.split_once('/') else {
        return Err(refuse("BAD_PAYLOAD", "command is `<extension>/<command>`"));
    };
    let mut store = core.store.lock().await;
    let loaded = core.tools.hooks.extensions_of(&store, task.session_id);
    let Some(ext) = loaded.iter().find(|e| e.manifest.name == ext_name) else {
        return Err(refuse(
            "UNKNOWN_COMMAND",
            format!("no extension `{ext_name}` is loaded in this session"),
        ));
    };
    if let Some(q) = &ext.quarantine {
        return Err(refuse(
            "EXTENSION_QUARANTINED",
            format!("`{ext_name}` is quarantined: {q}"),
        ));
    }
    let Some(spec) = ext.manifest.commands.iter().find(|c| c.name == cmd_name) else {
        return Err(refuse(
            "UNKNOWN_COMMAND",
            format!("`{ext_name}` declares no command `{cmd_name}`"),
        ));
    };
    let offset = crate::runtime::append(
        &mut store,
        core,
        crate::runtime::Lineage::task(core.tenant_id, task.session_id, task.task_id),
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![crate::runtime::typed(
            "TaskInputQueued",
            &TaskEvent::TaskInputQueued {
                input_id: input_id.to_owned(),
                mode: InputMode::FollowUp,
                text: spec.expand(arguments),
                provenance: format!("extension:{ext_name}/{cmd_name}"),
                untrusted: false,
            },
            actor,
        )],
    )
    .map_err(|e| refuse("STORE", e))?;
    core.last_offset.send_replace(offset);
    Ok(wire::InputQueued {
        task_id: Some(crate::server::wire_id(task.task_id.as_bytes())),
        sequence: 0,
        offset,
    })
}

/// What already exists where an import would land — the workspace's and
/// the operator's rules, agent profiles and skills, and the servers the
/// resolved configuration names — so an imported item of the same name is a
/// conflict and what is there wins.
fn existing_for(core: &Core, root: &std::path::Path) -> modbit_skills::import::Existing {
    let mut ex = modbit_skills::import::Existing::default();
    let stems = |dir: std::path::PathBuf, out: &mut std::collections::BTreeSet<String>| {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if let Ok(text) = std::fs::read_to_string(&p)
                && let Some(id) = text
                    .lines()
                    .skip(1)
                    .take_while(|l| l.trim() != "---")
                    .find_map(|l| l.strip_prefix("id:").map(|v| v.trim().to_owned()))
            {
                out.insert(id);
            } else if let Some(stem) = p.file_stem() {
                out.insert(stem.to_string_lossy().into_owned());
            }
        }
    };
    stems(root.join(".modbit").join("rules"), &mut ex.rules);
    stems(core.data_dir.join("rules"), &mut ex.rules);
    let (profiles, _) = modbit_domain::agent_profile::list(&[
        root.join(".modbit").join("agents"),
        core.data_dir.join("agents"),
    ]);
    ex.agents = profiles.into_iter().map(|(p, _)| p.name).collect();
    for dir in [root.join(".modbit").join("skills"), core.data_dir.join("skills")] {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                ex.skills.insert(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    let resolved = modbit_policy::config::resolve(&crate::config::layers_for(
        &core.data_dir,
        Some(&root.to_string_lossy()),
    ));
    ex.servers = resolved.mcp_servers.keys().cloned().collect();
    ex
}

/// `ImportAgentConfig` (REQ-EV-0137, REQ-EV-0183): turn another agent's
/// configuration under `source_root` into an extension under this Core's
/// `imports/`, with the migration report. The extension is unsigned: loading
/// it quarantines it until the person trusts what it would do.
pub(crate) async fn import(
    core: &Core,
    session_id: SessionId,
    source_root: &str,
    name: &str,
    replace: bool,
) -> Result<wire::ImportReportView, (String, String)> {
    let root = std::path::Path::new(source_root)
        .canonicalize()
        .map_err(|e| refuse("IMPORT_SOURCE_MISSING", format!("{source_root}: {e}")))?;
    let out = core.data_dir.join("imports").join(name);
    // An import that is loaded is not rewritten under it.
    {
        let store = core.store.lock().await;
        let out_s = out.canonicalize().unwrap_or_else(|_| out.clone());
        if core
            .tools
            .hooks
            .extensions_of(&store, session_id)
            .iter()
            .any(|e| std::path::Path::new(&e.path) == out_s)
        {
            return Err(refuse(
                "EXTENSION_LOADED",
                format!("`{name}` is loaded in this session; unload it before importing again"),
            ));
        }
    }
    let existing = existing_for(core, &root);
    let report = modbit_skills::import::import(&root, name, &out, &existing, replace).map_err(
        |e| match e {
            modbit_skills::import::ImportError::Exists(_) => {
                refuse("IMPORT_EXISTS", format!("{e}; import again with replace"))
            }
            modbit_skills::import::ImportError::Nothing(_) => refuse("IMPORT_EMPTY", e.to_string()),
            modbit_skills::import::ImportError::BadName(_)
            | modbit_skills::import::ImportError::NoSource(_) => {
                refuse("BAD_PAYLOAD", e.to_string())
            }
            modbit_skills::import::ImportError::Io(_) => refuse("IMPORT_FAILED", e.to_string()),
        },
    )?;
    // What was written is read back exactly as a load would read it.
    let written = read(&out.to_string_lossy())?;
    let label = |s: modbit_skills::import::ItemStatus| {
        serde_json::to_value(s)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    };
    Ok(wire::ImportReportView {
        extension_path: written.dir,
        name: report.extension.clone(),
        formats: report.formats.clone(),
        mapped: report.count(modbit_skills::import::ItemStatus::Mapped) as u32,
        skipped: report.count(modbit_skills::import::ItemStatus::Skipped) as u32,
        conflicts: report.count(modbit_skills::import::ItemStatus::Conflict) as u32,
        items: report
            .items
            .iter()
            .map(|i| wire::ImportItemView {
                kind: serde_json::to_value(i.kind)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                source: i.source.clone(),
                target: i.target.clone(),
                status: label(i.status),
                reason: i.reason.clone(),
            })
            .collect(),
        manifest_digest: written.digest,
        signature: written.signature.label(),
        capabilities: written.manifest.capabilities(),
    })
}
