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
//! absent is no opinion. A file that is present and cannot be read or does
//! not parse is a [`ConfigError`] and fails closed (FIX-05): a corrupted
//! admin or device file would otherwise silently remove every deny it
//! carried. The Core still starts and says so on stderr, but no task starts
//! (`CONFIG_UNREADABLE`, naming the file and what is wrong with it) and a
//! running task stops at its next round boundary. The project layer is the
//! repository's: its broken file refuses the tasks in that workspace only.
//!
//! The repository layer is also read once per task. A task that writes
//! `.modbit/config.json` itself (a process can: nothing stops `shell.exec`)
//! does not thereby change its own hooks or permissions; the file is read
//! again by the next task, after the change has been reviewed (FIX-04).
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

/// A configuration layer that is present and cannot be used (FIX-05).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    /// The layer the file belongs to.
    pub authority: Authority,
    /// The file (or the environment variable) it was read from.
    pub source: String,
    /// What is wrong with it: the read failure or the parse problem.
    pub detail: String,
}

impl ConfigError {
    /// The refusal code a task start answers with.
    pub const CODE: &'static str = "CONFIG_UNREADABLE";
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the {} configuration `{}` {}; fix or remove it (a missing file is fine, a broken one is not)",
            format!("{:?}", self.authority).to_lowercase(),
            self.source,
            self.detail
        )
    }
}

impl std::error::Error for ConfigError {}

/// One layer read from disk: `Ok(None)` for a file that is not there (no
/// opinion), an error for one that is there and cannot be used.
fn read_layer(path: &Path, authority: Authority) -> Result<Option<Layer>, ConfigError> {
    let fail = |detail: String| ConfigError {
        authority,
        source: path.display().to_string(),
        detail,
    };
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        // Nothing there, or no directory for it to be in: no opinion.
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(fail(format!("cannot be read ({e})"))),
    };
    serde_json::from_str::<Layer>(&text)
        .map(Some)
        .map_err(|e| fail(format!("is not a configuration layer ({e})")))
}

#[cfg(test)]
thread_local! {
    /// A test's own device policy path, for the thread that set it.
    static DEVICE_POLICY_OVERRIDE: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Where the machine's managed device policy lives (REQ-EV-0040): a path
/// the device manager provisions, outside anything a user or a repository
/// writes to — `MODBIT_DEVICE_POLICY` when the manager sets it, else the
/// platform's system-wide location.
#[must_use]
pub fn device_policy_path() -> std::path::PathBuf {
    #[cfg(test)]
    if let Some(p) = DEVICE_POLICY_OVERRIDE.with(|o| o.borrow().clone()) {
        return p;
    }
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

/// Where the admin layer lives.
fn admin_path(data_dir: &Path) -> std::path::PathBuf {
    std::env::var("MODBIT_ADMIN_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| data_dir.join("admin-config.json"))
}

/// Where the repository layer lives.
fn project_path(root: &str) -> std::path::PathBuf {
    Path::new(root).join(".modbit").join("config.json")
}

/// Every layer that can be read, and every one that cannot, in authority
/// order. `project` supplies the repository layer when the caller already
/// holds it (the task's start-of-run read); `None` reads the working tree.
fn gather(
    data_dir: &Path,
    workspace_root: Option<&str>,
    project: Option<&Option<Layer>>,
) -> (BTreeMap<Authority, Layer>, Vec<ConfigError>) {
    let mut layers = BTreeMap::new();
    let mut problems = Vec::new();
    let mut take = |authority: Authority, read: Result<Option<Layer>, ConfigError>| match read {
        Ok(Some(l)) => {
            layers.insert(authority, l);
        }
        Ok(None) => {}
        Err(e) => problems.push(e),
    };
    take(
        Authority::Device,
        read_layer(&device_policy_path(), Authority::Device),
    );
    take(
        Authority::Admin,
        read_layer(&admin_path(data_dir), Authority::Admin),
    );
    if let Some(root) = workspace_root {
        take(
            Authority::Project,
            match project {
                Some(held) => Ok(held.clone()),
                None => read_layer(&project_path(root), Authority::Project),
            },
        );
    }
    take(
        Authority::User,
        read_layer(&data_dir.join("config.json"), Authority::User),
    );
    // A server handed to the Core at boot (`MODBIT_MCP_SERVERS`, a JSON
    // array of server definitions) is an admin-layer declaration: the
    // operator who started the process is the highest authority there is,
    // and folding it in here keeps one resolution path for every server.
    if let Ok(raw) = std::env::var("MODBIT_MCP_SERVERS") {
        match serde_json::from_str::<Vec<serde_json::Value>>(&raw) {
            Ok(list) => {
                let admin = layers.entry(Authority::Admin).or_default();
                for def in list {
                    if let Some(name) = def.get("name").and_then(serde_json::Value::as_str) {
                        admin.mcp_servers.insert(name.to_owned(), def.to_string());
                    }
                }
            }
            Err(e) => problems.push(ConfigError {
                authority: Authority::Admin,
                source: "MODBIT_MCP_SERVERS".into(),
                detail: format!("is not a JSON array of server definitions ({e})"),
            }),
        }
    }
    (layers, problems)
}

/// Every layer problem in force for `workspace_root` (the Core's own
/// layers when `None`), for diagnostics.
#[must_use]
pub fn problems(data_dir: &Path, workspace_root: Option<&str>) -> Vec<ConfigError> {
    gather(data_dir, workspace_root, None).1
}

/// The layers that can be read, for the paths that only inform (a view, a
/// proposal's pre-check, an extension's manifest) and have no refusal to
/// give. A broken layer is reported on stderr and absent from the answer:
/// nothing that decides a task's policy reads this; those paths use
/// [`try_layers_for`] or a task's pinned snapshot.
#[must_use]
pub fn layers_for(data_dir: &Path, workspace_root: Option<&str>) -> BTreeMap<Authority, Layer> {
    let (layers, problems) = gather(data_dir, workspace_root, None);
    for p in &problems {
        eprintln!("modbit-core: {p}");
    }
    layers
}

/// Resolved configurations, pinned per task: the snapshot a model round
/// runs under (REQ-EV-0041). The loop refreshes it at each round boundary; a
/// call already in flight keeps the snapshot it was decided under.
#[derive(Default)]
pub struct Configurations {
    pinned: Mutex<HashMap<TaskId, Arc<ResolvedConfig>>>,
    /// What a task read of the repository when it first resolved: the
    /// repository's own files are the task's start-of-run copy from then on.
    frozen: Mutex<HashMap<TaskId, RepositoryFiles>>,
}

/// The repository-controlled files a task reads once (FIX-04).
#[derive(Clone, Debug, Default)]
struct RepositoryFiles {
    /// `.modbit/config.json`, parsed; `None` when the repository has none.
    project: Option<Layer>,
    /// `.modbit/verification.json`'s text; `None` when the repository has none.
    verification: Option<String>,
    /// Every file under `.modbit/` as the task found it: repository-relative
    /// path (`/` separators) to the SHA-256 of its content. What a task
    /// changed there is what differs from this, not what was uncommitted
    /// before it began.
    tree: BTreeMap<String, String>,
}

/// The most files of `.modbit/` a task's start-of-run record holds; a
/// directory with more is not a configuration directory, and the files past
/// the cap are treated as changed (shown, never hidden).
const TREE_FILE_CAP: usize = 4096;

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// Every regular file under `<root>/.modbit` with its content digest.
fn modbit_tree(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.join(".modbit")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                if out.len() >= TREE_FILE_CAP {
                    return out;
                }
                if let (Ok(rel), Ok(bytes)) = (path.strip_prefix(root), std::fs::read(&path)) {
                    out.insert(rel.to_string_lossy().replace('\\', "/"), digest(&bytes));
                }
            }
        }
    }
    out
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
    /// The repository layer `task` runs under: read from `workspace_root`
    /// the first time the task asks, the same copy every time after.
    fn repository_layer(
        &self,
        task: TaskId,
        workspace_root: Option<&str>,
    ) -> Result<Option<Layer>, ConfigError> {
        let Some(root) = workspace_root else {
            return Ok(None);
        };
        let mut frozen = self.frozen.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(files) = frozen.get(&task) {
            return Ok(files.project.clone());
        }
        let project = read_layer(&project_path(root), Authority::Project)?;
        frozen.insert(
            task,
            RepositoryFiles {
                project: project.clone(),
                verification: std::fs::read_to_string(
                    Path::new(root).join(".modbit").join("verification.json"),
                )
                .ok(),
                tree: modbit_tree(Path::new(root)),
            },
        );
        Ok(project)
    }

    /// Of `candidates` (repository-relative paths under `.modbit/` that git
    /// reports as different from `HEAD`), those whose content is exactly what
    /// `task` found there when it started: uncommitted work that was already
    /// there, which the task did not do and the diff invariants and the review
    /// do not charge to it. A task that has not resolved its configuration has
    /// no start-of-run record, and every path counts as changed.
    #[must_use]
    pub fn unchanged_since_start(
        &self,
        task: TaskId,
        workspace_root: &str,
        candidates: &[String],
    ) -> std::collections::BTreeSet<String> {
        let frozen = self.frozen.lock().unwrap_or_else(|e| e.into_inner());
        let Some(files) = frozen.get(&task) else {
            return std::collections::BTreeSet::new();
        };
        candidates
            .iter()
            .filter(|p| {
                let now = std::fs::read(Path::new(workspace_root).join(p))
                    .ok()
                    .map(|b| digest(&b));
                now == files.tree.get(p.as_str()).cloned()
            })
            .cloned()
            .collect()
    }

    /// The text of the repository's `.modbit/verification.json` as `task`
    /// first found it: `Some(None)` is "the repository had none", and `None`
    /// is "this task has not resolved its configuration" (read the file).
    #[must_use]
    pub fn verification_json(&self, task: TaskId) -> Option<Option<String>> {
        self.frozen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&task)
            .map(|f| f.verification.clone())
    }

    /// Resolve `task`'s layers now, refusing a layer that cannot be used.
    fn resolve_now(
        &self,
        task: TaskId,
        data_dir: &Path,
        workspace_root: Option<&str>,
    ) -> Result<Arc<ResolvedConfig>, ConfigError> {
        let project = self.repository_layer(task, workspace_root)?;
        let (layers, problems) = gather(data_dir, workspace_root, Some(&project));
        match problems.into_iter().next() {
            Some(e) => Err(e),
            None => Ok(Arc::new(resolve(&layers))),
        }
    }

    /// Resolve `task`'s configuration now and make it the snapshot in
    /// force; answers the new snapshot and, when the generation moved, the
    /// one it replaced (REQ-EV-0041). The first resolution of a task has no
    /// predecessor.
    ///
    /// # Errors
    /// A layer that is present and cannot be used. The snapshot in force is
    /// left as it was: a task never runs under less policy than it had.
    pub fn try_refresh(
        &self,
        task: TaskId,
        data_dir: &Path,
        workspace_root: Option<&str>,
    ) -> Result<(Snapshot, Option<Snapshot>), ConfigError> {
        let fresh = self.resolve_now(task, data_dir, workspace_root)?;
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
        Ok((now, previous))
    }

    /// The configuration in force for `task`, resolving and pinning it the
    /// first time the task asks.
    ///
    /// # Errors
    /// A layer that is present and cannot be used, when the task has no
    /// snapshot yet.
    pub fn try_for_task(
        &self,
        task: TaskId,
        data_dir: &Path,
        workspace_root: Option<&str>,
    ) -> Result<Arc<ResolvedConfig>, ConfigError> {
        if let Some(c) = self
            .pinned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&task)
        {
            return Ok(Arc::clone(c));
        }
        let fresh = self.resolve_now(task, data_dir, workspace_root)?;
        Ok(Arc::clone(
            self.pinned
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(task)
                .or_insert(fresh),
        ))
    }

    /// The configuration in force for `task` where there is no refusal to
    /// give (the task's snapshot was admitted when it started). A task with
    /// no snapshot and a layer that cannot be used gets what the readable
    /// layers say, reported and not pinned; every path that decides a task's
    /// policy asks [`Self::try_for_task`] first.
    pub fn for_task(
        &self,
        task: TaskId,
        data_dir: &Path,
        workspace_root: Option<&str>,
    ) -> Arc<ResolvedConfig> {
        match self.try_for_task(task, data_dir, workspace_root) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("modbit-core: {e}");
                Arc::new(resolve(&layers_for(data_dir, workspace_root)))
            }
        }
    }

    /// Pin `config` as the snapshot in force for `task`. A run that resumes
    /// inside a model round it froze before the Core stopped decides the rest
    /// of that round under the configuration the round began with
    /// (REQ-PX-131); the next round boundary resolves afresh.
    pub fn install(&self, task: TaskId, config: Arc<ResolvedConfig>) {
        self.pinned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(task, config);
    }

    /// Forget a finished task's configuration.
    pub fn release(&self, task: TaskId) {
        self.pinned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&task);
        self.frozen
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

    /// The layers for a task, in authority order, or the first layer (in
    /// that order) that is present and cannot be used.
    fn try_layers_for(
        data_dir: &Path,
        workspace_root: Option<&str>,
    ) -> Result<BTreeMap<Authority, Layer>, ConfigError> {
        let (layers, problems) = gather(data_dir, workspace_root, None);
        problems.into_iter().next().map_or(Ok(layers), Err)
    }

    /// Run `f` with this thread's device policy at `device` (the machine's
    /// own file is not consulted), leaving every other test alone.
    fn with_device<T>(device: &Path, f: impl FnOnce() -> T) -> T {
        DEVICE_POLICY_OVERRIDE.with(|o| *o.borrow_mut() = Some(device.to_path_buf()));
        let out = f();
        DEVICE_POLICY_OVERRIDE.with(|o| *o.borrow_mut() = None);
        out
    }

    fn denies(cfg: &ResolvedConfig, capability: &str) -> bool {
        modbit_policy::config::denied(cfg).contains(capability)
    }

    /// FIX-05 (audit G "Fail-closed config"), inverting the test that used
    /// to be `a_missing_or_broken_layer_is_no_opinion_and_never_an_error`:
    /// that test asserted a broken file is ignored, which let a corrupted
    /// admin or device file silently drop every deny it carried. A missing
    /// file is still no opinion; a broken one is an error naming the file
    /// and the parse problem, for every layer.
    #[test]
    fn a_missing_layer_is_no_opinion_and_a_broken_one_is_an_error_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let device = dir.path().join("device-policy.json");
        let admin = dir.path().join("admin-config.json");
        with_device(&device, || {
            assert!(
                try_layers_for(dir.path(), None)
                    .expect("absent is fine")
                    .is_empty(),
                "nothing on disk"
            );
            // The user layer.
            let user = dir.path().join("config.json");
            std::fs::write(&user, "{ not json").expect("write");
            let e = try_layers_for(dir.path(), None).expect_err("a broken file is refused");
            assert_eq!(e.authority, Authority::User);
            assert_eq!(e.source, user.display().to_string());
            assert!(e.detail.contains("not a configuration layer"), "{e}");
            assert!(e.to_string().contains(&user.display().to_string()), "{e}");
            std::fs::write(&user, r#"{"hooks":["lint"]}"#).expect("write");
            let layers = try_layers_for(dir.path(), None).expect("fixed");
            assert_eq!(layers.len(), 1);
            assert_eq!(layers[&Authority::User].hooks, vec!["lint".to_owned()]);

            // The admin layer: a truncated write and a layer of the wrong
            // shape are both refused, and the admin's deny is not dropped
            // in silence.
            std::fs::write(&admin, r#"{"mcp_deny":["shadow""#).expect("write");
            let e = try_layers_for(dir.path(), None).expect_err("truncated admin");
            assert_eq!(e.authority, Authority::Admin);
            assert_eq!(e.source, admin.display().to_string());
            std::fs::write(&admin, r#"{"permissions":{"shell.exec":"MAYBE"}}"#).expect("write");
            let e = try_layers_for(dir.path(), None).expect_err("not a permission");
            assert_eq!(e.authority, Authority::Admin);
            assert!(
                e.detail.contains("MAYBE") || e.detail.contains("variant"),
                "{e}"
            );
            std::fs::write(&admin, r#"{"mcp_deny":["shadow"]}"#).expect("write");
            assert_eq!(
                try_layers_for(dir.path(), None).expect("fixed").len(),
                2,
                "user and admin"
            );

            // The device layer, and one that exists but cannot be read (a
            // directory where the file should be).
            std::fs::write(&device, "").expect("write");
            let e = try_layers_for(dir.path(), None).expect_err("empty device policy");
            assert_eq!(e.authority, Authority::Device);
            std::fs::remove_file(&device).expect("rm");
            std::fs::create_dir(&device).expect("mkdir");
            let e = try_layers_for(dir.path(), None).expect_err("unreadable device policy");
            assert_eq!(e.authority, Authority::Device);
            assert!(e.detail.contains("cannot be read"), "{e}");
            std::fs::remove_dir(&device).expect("rmdir");

            // Every problem is listed for diagnostics, not just the first.
            std::fs::write(&device, "{").expect("write");
            std::fs::write(&admin, "{").expect("write");
            let all = problems(dir.path(), None);
            assert_eq!(
                all.iter().map(|p| p.authority).collect::<Vec<_>>(),
                vec![Authority::Device, Authority::Admin]
            );
            // The reporting reader still answers (the paths that only
            // inform have no refusal to give).
            assert_eq!(layers_for(dir.path(), None).len(), 1, "only the user's");
        });
    }

    /// FIX-04: what a task changed under `.modbit/` is what differs from
    /// what it found; the user's uncommitted work that was already there is
    /// not the task's. A task with no start-of-run record is charged with
    /// every dirty path.
    #[test]
    fn what_a_task_changed_under_dot_modbit_is_what_differs_from_what_it_found() {
        let data = tempfile::tempdir().expect("tempdir");
        let repo = tempfile::tempdir().expect("tempdir");
        let none = data.path().join("none.json");
        let rules = repo.path().join(".modbit").join("rules");
        std::fs::create_dir_all(&rules).expect("mkdir");
        std::fs::write(rules.join("mine.md"), "mine").expect("write");
        std::fs::write(rules.join("edited.md"), "before").expect("write");
        std::fs::write(rules.join("removed.md"), "bye").expect("write");
        with_device(&none, || {
            let root = repo.path().to_string_lossy().into_owned();
            let configs = Configurations::default();
            let task = TaskId::new();
            let all: Vec<String> = [
                ".modbit/rules/mine.md",
                ".modbit/rules/edited.md",
                ".modbit/rules/removed.md",
                ".modbit/rules/planted.md",
                ".modbit/rules/gone-before.md",
            ]
            .map(str::to_owned)
            .to_vec();
            assert!(
                configs.unchanged_since_start(task, &root, &all).is_empty(),
                "no record: everything counts as changed"
            );
            configs
                .try_refresh(task, data.path(), Some(&root))
                .expect("resolved");
            std::fs::write(rules.join("edited.md"), "after").expect("write");
            std::fs::remove_file(rules.join("removed.md")).expect("rm");
            std::fs::write(rules.join("planted.md"), "x").expect("write");
            let unchanged = configs.unchanged_since_start(task, &root, &all);
            assert_eq!(
                unchanged.into_iter().collect::<Vec<_>>(),
                vec![
                    ".modbit/rules/gone-before.md".to_owned(),
                    ".modbit/rules/mine.md".to_owned()
                ],
                "only what was already there, byte for byte, or already absent"
            );
        });
    }

    /// A repository's broken file refuses that workspace's tasks and no
    /// other; a repository with no `.modbit` (or a `.modbit` that is a
    /// file) has no opinion.
    #[test]
    fn a_broken_repository_layer_refuses_that_workspace_only() {
        let data = tempfile::tempdir().expect("tempdir");
        let bad = tempfile::tempdir().expect("tempdir");
        let good = tempfile::tempdir().expect("tempdir");
        let file = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(bad.path().join(".modbit")).expect("mkdir");
        std::fs::write(bad.path().join(".modbit").join("config.json"), "{ nope").expect("write");
        std::fs::write(file.path().join(".modbit"), "not a directory").expect("write");
        let none = data.path().join("none.json");
        with_device(&none, || {
            let root = |d: &tempfile::TempDir| d.path().to_string_lossy().into_owned();
            let e = try_layers_for(data.path(), Some(&root(&bad))).expect_err("refused");
            assert_eq!(e.authority, Authority::Project);
            assert!(e.source.ends_with("config.json"), "{e}");
            assert!(
                try_layers_for(data.path(), Some(&root(&good))).is_ok(),
                "another workspace is unaffected"
            );
            assert!(
                try_layers_for(data.path(), Some(&root(&file))).is_ok(),
                "`.modbit` that is a file has no opinion"
            );
            let configs = Configurations::default();
            let task = TaskId::new();
            assert!(
                configs
                    .try_for_task(task, data.path(), Some(&root(&bad)))
                    .is_err()
            );
            assert!(
                configs
                    .try_refresh(task, data.path(), Some(&root(&bad)))
                    .is_err()
            );
            assert!(
                configs
                    .try_for_task(TaskId::new(), data.path(), Some(&root(&good)))
                    .is_ok()
            );
        });
    }

    /// FIX-04: the repository layer and the verification commands are what
    /// the task found when it first resolved. A file written during the run
    /// is read by the next task; a refresh still sees the other layers move
    /// (REQ-EV-0041).
    #[test]
    fn a_task_keeps_the_repository_files_it_started_with() {
        let data = tempfile::tempdir().expect("tempdir");
        let repo = tempfile::tempdir().expect("tempdir");
        let none = data.path().join("none.json");
        let modbit = repo.path().join(".modbit");
        std::fs::create_dir_all(&modbit).expect("mkdir");
        std::fs::write(
            modbit.join("config.json"),
            r#"{"permissions":{"one":"DENY"}}"#,
        )
        .expect("write");
        std::fs::write(modbit.join("verification.json"), "first").expect("write");
        with_device(&none, || {
            let root = repo.path().to_string_lossy().into_owned();
            let configs = Configurations::default();
            let task = TaskId::new();
            assert_eq!(configs.verification_json(task), None, "not resolved yet");
            let (first, _) = configs
                .try_refresh(task, data.path(), Some(&root))
                .expect("resolved");
            assert!(denies(&first.config, "one"));
            assert_eq!(configs.verification_json(task), Some(Some("first".into())));

            // The task writes both files and the user layer moves too.
            std::fs::write(
                modbit.join("config.json"),
                r#"{"permissions":{"two":"DENY"}}"#,
            )
            .expect("write");
            std::fs::write(modbit.join("verification.json"), "planted").expect("write");
            std::fs::write(
                data.path().join("config.json"),
                r#"{"permissions":{"user":"DENY"}}"#,
            )
            .expect("write");
            let (now, previous) = configs
                .try_refresh(task, data.path(), Some(&root))
                .expect("refreshed");
            assert!(denies(&now.config, "one"), "the repository's copy");
            assert!(
                !denies(&now.config, "two"),
                "the file the task wrote is not read"
            );
            assert!(denies(&now.config, "user"), "other layers move");
            assert!(previous.is_some(), "and the generation moved");
            assert_eq!(configs.verification_json(task), Some(Some("first".into())));

            // The next task reads what is on disk now.
            let next = TaskId::new();
            let (later, _) = configs
                .try_refresh(next, data.path(), Some(&root))
                .expect("resolved");
            assert!(denies(&later.config, "two"));
            assert_eq!(
                configs.verification_json(next),
                Some(Some("planted".into()))
            );
            configs.release(task);
            assert_eq!(configs.verification_json(task), None);
        });
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
