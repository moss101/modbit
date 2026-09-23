//! The configuration in force for a task (REQ-EV-0039, docs/23): the admin,
//! project and user layers resolved by `modbit_policy::config` into one
//! answer with provenance, and pinned for the task's life.
//!
//! Where the layers come from:
//!
//! | layer | source |
//! |---|---|
//! | device | `MODBIT_DEVICE_POLICY` (a path the device manager sets), else the machine's managed file (`device_policy_path`) |
//! | admin | `MODBIT_ADMIN_CONFIG` (a path), else `<data-dir>/admin-config.json` |
//! | project | `<workspace root>/.modbit/config.json` |
//! | user | `<data-dir>/config.json` |
//!
//! Each file is one `modbit_policy::config::Layer` as JSON. A file that is
//! absent is no opinion; a file that does not parse is reported on stderr
//! and treated as absent — a broken configuration file narrows nothing and
//! must never stop the Core from starting or a task from running.
//!
//! The merge laws are the resolver's (docs/23): a lower authority may
//! tighten a permission but never widen it, egress and model allow-lists
//! intersect, and for MCP servers a higher layer's deny always wins while a
//! lower layer may add a server the higher layers did not deny. A server
//! two layers both define is decided by the higher one, and the lower
//! layer's definition is kept in the provenance rather than dropped in
//! silence.
//!
//! The configuration is resolved once per task and pinned: a task's policy
//! does not change under it while it runs, for the same reason its
//! environment revision does not (docs/23 "Capability kernel").

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use modbit_domain::TaskId;
use modbit_policy::config::{Authority, Layer, ResolvedConfig, resolve};

/// One layer read from disk, or no opinion.
fn read_layer(path: &Path) -> Option<Layer> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<Layer>(&text) {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!(
                "modbit-core: configuration `{}` is not a configuration layer ({e}); ignored",
                path.display()
            );
            None
        }
    }
}

/// Where the machine's managed device policy lives (REQ-EV-0040): a path
/// the device manager provisions, outside anything a user or a repository
/// writes to — `MODBIT_DEVICE_POLICY` when the manager sets it, else the
/// platform's system-wide location.
#[must_use]
pub fn device_policy_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("MODBIT_DEVICE_POLICY") {
        return std::path::PathBuf::from(p);
    }
    if cfg!(target_os = "macos") {
        std::path::PathBuf::from("/Library/Application Support/Modbit/device-policy.json")
    } else if cfg!(windows) {
        std::path::PathBuf::from(r"C:\ProgramData\Modbit\device-policy.json")
    } else {
        std::path::PathBuf::from("/etc/modbit/device-policy.json")
    }
}

/// The layers for a task, in authority order: device, admin, project, user.
#[must_use]
pub fn layers_for(data_dir: &Path, workspace_root: Option<&str>) -> BTreeMap<Authority, Layer> {
    let mut layers = BTreeMap::new();
    if let Some(l) = read_layer(&device_policy_path()) {
        layers.insert(Authority::Device, l);
    }
    let admin = std::env::var("MODBIT_ADMIN_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| data_dir.join("admin-config.json"));
    if let Some(l) = read_layer(&admin) {
        layers.insert(Authority::Admin, l);
    }
    if let Some(root) = workspace_root
        && let Some(l) = read_layer(&Path::new(root).join(".modbit").join("config.json"))
    {
        layers.insert(Authority::Project, l);
    }
    if let Some(l) = read_layer(&data_dir.join("config.json")) {
        layers.insert(Authority::User, l);
    }
    // A server handed to the Core at boot (`MODBIT_MCP_SERVERS`, a JSON
    // array of server definitions) is an admin-layer declaration: the
    // operator who started the process is the highest authority there is,
    // and folding it in here keeps one resolution path for every server.
    if let Ok(raw) = std::env::var("MODBIT_MCP_SERVERS")
        && let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(&raw)
    {
        let admin = layers.entry(Authority::Admin).or_default();
        for def in list {
            if let Some(name) = def.get("name").and_then(serde_json::Value::as_str) {
                admin.mcp_servers.insert(name.to_owned(), def.to_string());
            }
        }
    }
    layers
}

/// Resolved configurations, pinned per task: the snapshot a model round
/// runs under (REQ-EV-0041). The loop refreshes it at each round boundary; a
/// call already in flight keeps the snapshot it was decided under.
#[derive(Default)]
pub struct Configurations {
    pinned: Mutex<HashMap<TaskId, Arc<ResolvedConfig>>>,
}

/// One resolution of a task's configuration, with its generation.
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// The configuration.
    pub config: Arc<ResolvedConfig>,
    /// Its generation (`modbit_policy::config::generation`).
    pub generation: String,
}

impl Configurations {
    /// Resolve `task`'s configuration now and make it the snapshot in
    /// force; answers the new snapshot and, when the generation moved, the
    /// one it replaced (REQ-EV-0041). The first resolution of a task has no
    /// predecessor.
    pub fn refresh(
        &self,
        task: TaskId,
        data_dir: &Path,
        workspace_root: Option<&str>,
    ) -> (Snapshot, Option<Snapshot>) {
        let fresh = Arc::new(resolve(&layers_for(data_dir, workspace_root)));
        let now = Snapshot {
            generation: modbit_policy::config::generation(&fresh),
            config: fresh,
        };
        let mut pinned = self.pinned.lock().unwrap_or_else(|e| e.into_inner());
        let previous = pinned
            .insert(task, Arc::clone(&now.config))
            .and_then(|old| {
                let g = modbit_policy::config::generation(&old);
                (g != now.generation).then_some(Snapshot {
                    config: old,
                    generation: g,
                })
            });
        (now, previous)
    }

    /// The configuration in force for `task`, resolving and pinning it the
    /// first time the task asks.
    pub fn for_task(
        &self,
        task: TaskId,
        data_dir: &Path,
        workspace_root: Option<&str>,
    ) -> Arc<ResolvedConfig> {
        let mut pinned = self.pinned.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(
            pinned
                .entry(task)
                .or_insert_with(|| Arc::new(resolve(&layers_for(data_dir, workspace_root)))),
        )
    }

    /// Forget a finished task's configuration.
    pub fn release(&self, task: TaskId) {
        self.pinned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&task);
    }
}

/// Human-readable provenance lines for one resolved MCP server: which layer
/// decided and whose contrary opinion was overridden, plus any widening the
/// resolver refused for that name.
#[must_use]
pub fn server_provenance(cfg: &ResolvedConfig, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(r) = cfg.mcp_servers.get(name) {
        out.push(format!("decided by {:?}", r.provenance.decided_by));
        for (level, why) in &r.provenance.overridden {
            out.push(format!("{level:?}: {why}"));
        }
    }
    for w in &cfg.rejected_widenings {
        if w.contains(&format!("`{name}`")) {
            out.push(w.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_broken_layer_is_no_opinion_and_never_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(layers_for(dir.path(), None).is_empty(), "nothing on disk");
        std::fs::write(dir.path().join("config.json"), "{ not json").expect("write");
        assert!(
            layers_for(dir.path(), None).is_empty(),
            "a broken configuration file is ignored, not fatal"
        );
        std::fs::write(dir.path().join("config.json"), r#"{"hooks":["lint"]}"#).expect("write");
        let layers = layers_for(dir.path(), None);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[&Authority::User].hooks, vec!["lint".to_owned()]);
    }

    #[test]
    fn the_three_layers_are_read_from_their_places_and_a_task_pins_the_answer() {
        let data = tempfile::tempdir().expect("tempdir");
        let repo = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(repo.path().join(".modbit")).expect("mkdir");
        std::fs::write(
            data.path().join("admin-config.json"),
            r#"{"mcp_deny":["shadow"]}"#,
        )
        .expect("write");
        std::fs::write(
            repo.path().join(".modbit").join("config.json"),
            r#"{"mcp_servers":{"docs":"project-definition"}}"#,
        )
        .expect("write");
        std::fs::write(
            data.path().join("config.json"),
            r#"{"mcp_servers":{"docs":"user-definition","shadow":"x"}}"#,
        )
        .expect("write");
        let root = repo.path().to_string_lossy().into_owned();
        let layers = layers_for(data.path(), Some(&root));
        assert_eq!(layers.len(), 3);

        let resolved = resolve(&layers);
        assert_eq!(
            resolved.mcp_servers["docs"].value, "project-definition",
            "the project layer decides over the user's"
        );
        assert!(
            !resolved.mcp_servers.contains_key("shadow"),
            "an admin deny wins over a user addition"
        );
        let p = server_provenance(&resolved, "docs");
        assert!(p[0].contains("Project"), "{p:?}");
        assert!(p.iter().any(|l| l.contains("User")), "{p:?}");
        assert!(
            server_provenance(&resolved, "shadow")
                .iter()
                .any(|l| l.contains("denied by Admin")),
            "the refusal is on the record"
        );

        // Pinned: a task keeps the answer it started with.
        let configs = Configurations::default();
        let task = TaskId::new();
        let first = configs.for_task(task, data.path(), Some(&root));
        std::fs::write(
            data.path().join("config.json"),
            r#"{"mcp_servers":{"later":"x"}}"#,
        )
        .expect("write");
        let again = configs.for_task(task, data.path(), Some(&root));
        assert!(
            Arc::ptr_eq(&first, &again),
            "the task's configuration is pinned"
        );
        configs.release(task);
        let after = configs.for_task(task, data.path(), Some(&root));
        assert!(
            after.mcp_servers.contains_key("later"),
            "a new task reads the new file"
        );
    }
}

/// The user layer's file. Proposals are written here because it is the same
/// file the resolver reads and a person edits: there is no second store of
/// external servers to disagree with the configuration.
#[must_use]
pub fn user_layer_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("config.json")
}

/// Write `definition` under `mcp_servers[name]` in the user layer, keeping
/// every other key of the file exactly as it was — including keys this
/// build does not know, which belong to whoever wrote them.
///
/// # Errors
/// A message naming what went wrong when the file cannot be read as JSON,
/// is not an object, or cannot be written.
pub fn put_user_server(data_dir: &Path, name: &str, definition: &str) -> Result<(), String> {
    let path = user_layer_path(data_dir);
    let mut doc: serde_json::Value = match std::fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str(&text).map_err(|e| {
            format!(
                "`{}` is not JSON ({e}); refusing to overwrite it",
                path.display()
            )
        })?,
        _ => serde_json::json!({}),
    };
    let Some(obj) = doc.as_object_mut() else {
        return Err(format!(
            "`{}` is not a configuration object; refusing to overwrite it",
            path.display()
        ));
    };
    let servers = obj
        .entry("mcp_servers")
        .or_insert_with(|| serde_json::json!({}));
    let Some(servers) = servers.as_object_mut() else {
        return Err("`mcp_servers` is not an object".into());
    };
    servers.insert(
        name.to_owned(),
        serde_json::Value::String(definition.to_owned()),
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    // Written whole through a temporary file: a half-written configuration
    // would be read as "no opinion" by the next task.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text.as_bytes()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

/// The definition the user layer holds for `name`, if any.
#[must_use]
pub fn user_server(data_dir: &Path, name: &str) -> Option<String> {
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(user_layer_path(data_dir)).ok()?).ok()?;
    doc.get("mcp_servers")?
        .get(name)?
        .as_str()
        .map(str::to_owned)
}

/// Whether a layer above the user's denied `name` — the reason a proposal
/// for it can never become a running server, answered at propose time
/// rather than silently at resolve time.
#[must_use]
pub fn denied_above_user(
    data_dir: &Path,
    workspace_root: Option<&str>,
    name: &str,
) -> Option<Authority> {
    let layers = layers_for(data_dir, workspace_root);
    [Authority::Device, Authority::Admin, Authority::Project]
        .into_iter()
        .find(|l| {
            layers
                .get(l)
                .is_some_and(|layer| layer.mcp_deny.contains(name))
        })
}

#[cfg(test)]
mod proposal_tests {
    use super::*;

    #[test]
    fn a_proposal_is_written_into_the_user_layer_without_disturbing_the_rest_of_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            user_layer_path(dir.path()),
            r#"{"hooks":["lint"],"something_this_build_does_not_know":{"keep":1}}"#,
        )
        .expect("write");
        put_user_server(dir.path(), "docs", r#"{"trust":"PROPOSED"}"#).expect("written");
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(user_layer_path(dir.path())).unwrap())
                .unwrap();
        assert_eq!(doc["hooks"][0], "lint");
        assert_eq!(doc["something_this_build_does_not_know"]["keep"], 1);
        assert_eq!(
            user_server(dir.path(), "docs").as_deref(),
            Some(r#"{"trust":"PROPOSED"}"#)
        );
        // And it is a layer the resolver reads.
        let layers = layers_for(dir.path(), None);
        assert!(layers[&Authority::User].mcp_servers.contains_key("docs"));
    }

    #[test]
    fn a_file_that_is_not_json_is_never_overwritten() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(user_layer_path(dir.path()), "{ not json").expect("write");
        let e = put_user_server(dir.path(), "docs", "{}").expect_err("refused");
        assert!(e.contains("refusing to overwrite"), "{e}");
        assert_eq!(
            std::fs::read_to_string(user_layer_path(dir.path())).unwrap(),
            "{ not json"
        );
    }

    #[test]
    fn a_name_a_higher_layer_denied_is_known_before_it_is_proposed() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("admin-config.json"),
            r#"{"mcp_deny":["shadow"]}"#,
        )
        .expect("write");
        assert_eq!(
            denied_above_user(dir.path(), None, "shadow"),
            Some(Authority::Admin)
        );
        assert_eq!(denied_above_user(dir.path(), None, "docs"), None);
    }
}
