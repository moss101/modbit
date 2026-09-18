//! The configuration in force for a task (REQ-EV-0039, docs/23): the admin,
//! project and user layers resolved by `modbit_policy::config` into one
//! answer with provenance, and pinned for the task's life.
//!
//! Where the layers come from:
//!
//! | layer | source |
//! |---|---|
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

/// The three layers for a task, in authority order.
#[must_use]
pub fn layers_for(data_dir: &Path, workspace_root: Option<&str>) -> BTreeMap<Authority, Layer> {
    let mut layers = BTreeMap::new();
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

/// Resolved configurations, pinned per task.
#[derive(Default)]
pub struct Configurations {
    pinned: Mutex<HashMap<TaskId, Arc<ResolvedConfig>>>,
}

impl Configurations {
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
