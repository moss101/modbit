//! Environment definitions and revisions (REQ-EV-0021 / 0062 / 0146; docs/21,
//! docs/64 §1): what a task's processes run in — the toolchain, extra
//! `PATH` entries, declared variables, the tools that must be there —
//! declared in layers with an explicit precedence, reusable through
//! blueprints, and pinned to a run as one revision with a content digest.
//!
//! Layers, lowest precedence first: the **user** (`<data>/environments/user.json`),
//! the **team** (`<data>/environments/team.json`), the **repository**
//! (`<root>/.modbit/environment.json`). A later layer's variable overrides
//! an earlier one's; paths, tools and toolchain entries accumulate (a
//! toolchain entry of the same name is replaced). A layer may `extends`
//! blueprints by name — reusable definitions under
//! `<root>/.modbit/environments/<name>.json` (the repository's, first) or
//! `<data>/environments/blueprints/<name>.json` — applied before the layer's
//! own content. Every file read is hashed into the revision: the same
//! files and the same tools give the same digest; any change gives another.
//!
//! A revision never changes on its own: a run pins the one it started with
//! and keeps it until an explicit rebuild (docs/21 "Sandbox recovery" is
//! the sandbox's story; this is the environment's).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Where a layer comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// The user's own (`<data>/environments/user.json`).
    User,
    /// The team's (`<data>/environments/team.json`).
    Team,
    /// The repository's (`<root>/.modbit/environment.json`).
    Repository,
}

impl SourceKind {
    /// Lowest precedence first.
    pub const ORDER: [SourceKind; 3] = [SourceKind::User, SourceKind::Team, SourceKind::Repository];

    /// The label on the log.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SourceKind::User => "user",
            SourceKind::Team => "team",
            SourceKind::Repository => "repository",
        }
    }
}

/// A tool the environment names, with how its version is read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRequirement {
    /// The executable (`cargo`, `node`, …).
    pub name: String,
    /// The argument that prints the version (`--version` by default).
    #[serde(default = "default_version_arg")]
    pub version_arg: String,
    /// Whether the run may start without it (`false`: it must be there).
    #[serde(default)]
    pub optional: bool,
}

fn default_version_arg() -> String {
    "--version".into()
}

/// One definition file's content (a layer or a blueprint).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentDefinition {
    /// Blueprints applied before this content, in order.
    #[serde(default)]
    pub extends: Vec<String>,
    /// Tools whose versions are part of the revision.
    #[serde(default)]
    pub toolchain: Vec<ToolRequirement>,
    /// Variables a process gets (a later layer overrides an earlier one).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Directories put in front of `PATH` (relative to the workspace root
    /// unless absolute).
    #[serde(default)]
    pub path: Vec<String>,
    /// A note for people.
    #[serde(default)]
    pub description: String,
}

/// A file that went into the revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRecord {
    /// Layer or blueprint.
    pub kind: String,
    /// The path read.
    pub path: String,
    /// Its content's sha256.
    pub sha256: String,
}

/// The merged definition, with what it came from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compiled {
    /// Layers and blueprints in the order applied.
    pub sources: Vec<SourceRecord>,
    /// The merged toolchain (by name; a later layer replaces).
    pub toolchain: Vec<ToolRequirement>,
    /// The merged variables.
    pub env: BTreeMap<String, String>,
    /// The merged path entries (absolute, in order, deduplicated).
    pub path: Vec<String>,
    /// Problems reading a source that was named (a blueprint not found, a
    /// file that did not parse): part of the revision, never silent.
    pub problems: Vec<String>,
}

/// A tool's observed state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolState {
    /// The tool.
    pub name: String,
    /// The version it printed; `None` when absent.
    pub version: Option<String>,
    /// Whether the definition tolerates its absence.
    pub optional: bool,
}

/// One revision of the environment: what a run pins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentRevision {
    /// Content digest (sha256 over everything below but `captured_at_ms`).
    pub digest: String,
    /// The workspace root the paths are relative to.
    pub workspace_root: String,
    /// The files that went in.
    pub sources: Vec<SourceRecord>,
    /// The tools and their versions as observed.
    pub toolchain: Vec<ToolState>,
    /// The `PATH` entries in front.
    pub path: Vec<String>,
    /// The variables' names (values are part of the digest, never listed).
    pub env_names: Vec<String>,
    /// The variables, for the processes of the run (never on the log).
    #[serde(skip)]
    pub env: BTreeMap<String, String>,
    /// The platform.
    pub platform: String,
    /// Problems of the compilation.
    pub problems: Vec<String>,
    /// When it was captured (not part of the digest).
    pub captured_at_ms: i64,
}

/// Reads a tool's version.
pub trait ToolProbe {
    /// Run `name version_arg` with `path` in front and return the first
    /// line it printed; `None` when it is not there or fails.
    fn version(&self, name: &str, version_arg: &str, path: &[String]) -> Option<String>;
}

/// The host's tools.
pub struct SystemProbe;

impl ToolProbe for SystemProbe {
    fn version(&self, name: &str, version_arg: &str, path: &[String]) -> Option<String> {
        let mut full = path.to_vec();
        if let Ok(p) = std::env::var("PATH") {
            full.extend(std::env::split_paths(&p).map(|p| p.to_string_lossy().into_owned()));
        }
        let joined = std::env::join_paths(full.iter().map(PathBuf::from))
            .ok()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let out = std::process::Command::new(name)
            .arg(version_arg)
            .env("PATH", joined)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = if out.stdout.is_empty() {
            out.stderr
        } else {
            out.stdout
        };
        String::from_utf8_lossy(&text)
            .lines()
            .next()
            .map(|l| l.trim().chars().take(200).collect())
    }
}

fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn read_definition(path: &Path) -> Result<(EnvironmentDefinition, String), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let def: EnvironmentDefinition =
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((def, sha_hex(&bytes)))
}

/// The layer files for a workspace and a data directory.
#[must_use]
pub fn layer_paths(workspace_root: &Path, data_dir: &Path) -> Vec<(SourceKind, PathBuf)> {
    vec![
        (
            SourceKind::User,
            data_dir.join("environments").join("user.json"),
        ),
        (
            SourceKind::Team,
            data_dir.join("environments").join("team.json"),
        ),
        (
            SourceKind::Repository,
            workspace_root.join(".modbit").join("environment.json"),
        ),
    ]
}

fn blueprint_paths(name: &str, workspace_root: &Path, data_dir: &Path) -> Vec<PathBuf> {
    vec![
        workspace_root
            .join(".modbit")
            .join("environments")
            .join(format!("{name}.json")),
        data_dir
            .join("environments")
            .join("blueprints")
            .join(format!("{name}.json")),
    ]
}

/// Compile the layers (and the blueprints they extend) for a workspace.
#[must_use]
pub fn compile(workspace_root: &Path, data_dir: &Path) -> Compiled {
    let mut out = Compiled::default();
    let mut seen_blueprints: BTreeSet<String> = BTreeSet::new();
    for (kind, path) in layer_paths(workspace_root, data_dir) {
        if !path.is_file() {
            continue;
        }
        let (def, sha) = match read_definition(&path) {
            Ok(x) => x,
            Err(e) => {
                out.problems.push(format!("{} layer: {e}", kind.label()));
                continue;
            }
        };
        // Blueprints first, in the order named; a name is applied once.
        for name in &def.extends {
            if !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
                || name.is_empty()
            {
                out.problems.push(format!(
                    "{} layer: blueprint name `{name}` is not a name",
                    kind.label()
                ));
                continue;
            }
            if !seen_blueprints.insert(name.clone()) {
                continue;
            }
            let Some(bp_path) = blueprint_paths(name, workspace_root, data_dir)
                .into_iter()
                .find(|p| p.is_file())
            else {
                out.problems.push(format!(
                    "{} layer extends blueprint `{name}`, which is nowhere",
                    kind.label()
                ));
                continue;
            };
            match read_definition(&bp_path) {
                Ok((bp, bp_sha)) => {
                    out.sources.push(SourceRecord {
                        kind: format!("blueprint:{name}"),
                        path: bp_path.to_string_lossy().into_owned(),
                        sha256: bp_sha,
                    });
                    apply(&mut out, &bp, workspace_root);
                }
                Err(e) => out.problems.push(format!("blueprint `{name}`: {e}")),
            }
        }
        out.sources.push(SourceRecord {
            kind: kind.label().to_owned(),
            path: path.to_string_lossy().into_owned(),
            sha256: sha,
        });
        apply(&mut out, &def, workspace_root);
    }
    out
}

fn apply(out: &mut Compiled, def: &EnvironmentDefinition, root: &Path) {
    for t in &def.toolchain {
        out.toolchain.retain(|x| x.name != t.name);
        out.toolchain.push(t.clone());
    }
    for (k, v) in &def.env {
        out.env.insert(k.clone(), v.clone());
    }
    for p in &def.path {
        let abs = {
            let pb = PathBuf::from(p);
            if pb.is_absolute() { pb } else { root.join(pb) }
        };
        let s = abs.to_string_lossy().into_owned();
        if !out.path.contains(&s) {
            out.path.push(s);
        }
    }
}

/// Capture a revision: compile the layers, read the tools' versions.
#[must_use]
pub fn snapshot(
    workspace_root: &Path,
    data_dir: &Path,
    probe: &dyn ToolProbe,
    now_ms: i64,
) -> EnvironmentRevision {
    let compiled = compile(workspace_root, data_dir);
    let toolchain: Vec<ToolState> = compiled
        .toolchain
        .iter()
        .map(|t| ToolState {
            name: t.name.clone(),
            version: probe.version(&t.name, &t.version_arg, &compiled.path),
            optional: t.optional,
        })
        .collect();
    let mut rev = EnvironmentRevision {
        digest: String::new(),
        workspace_root: workspace_root.to_string_lossy().into_owned(),
        sources: compiled.sources,
        toolchain,
        path: compiled.path,
        env_names: compiled.env.keys().cloned().collect(),
        env: compiled.env,
        platform: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        problems: compiled.problems,
        captured_at_ms: now_ms,
    };
    rev.digest = digest_of(&rev);
    rev
}

/// The content digest of a revision (everything but when it was captured).
#[must_use]
pub fn digest_of(rev: &EnvironmentRevision) -> String {
    let mut h = Sha256::new();
    h.update(b"modbit-environment-revision:1\n");
    h.update(rev.workspace_root.as_bytes());
    h.update(b"\n");
    for s in &rev.sources {
        h.update(format!("source {} {} {}\n", s.kind, s.path, s.sha256).as_bytes());
    }
    for t in &rev.toolchain {
        h.update(
            format!(
                "tool {} {} {}\n",
                t.name,
                t.version.as_deref().unwrap_or("<absent>"),
                t.optional
            )
            .as_bytes(),
        );
    }
    for p in &rev.path {
        h.update(format!("path {p}\n").as_bytes());
    }
    for (k, v) in &rev.env {
        // The value's hash, so a changed value changes the digest and the
        // value itself never appears anywhere.
        h.update(format!("env {k} {}\n", sha_hex(v.as_bytes())).as_bytes());
    }
    h.update(format!("platform {}\n", rev.platform).as_bytes());
    for p in &rev.problems {
        h.update(format!("problem {p}\n").as_bytes());
    }
    hex::encode(h.finalize())
}

/// Tools the definition requires that are absent.
#[must_use]
pub fn missing_tools(rev: &EnvironmentRevision) -> Vec<String> {
    rev.toolchain
        .iter()
        .filter(|t| t.version.is_none() && !t.optional)
        .map(|t| t.name.clone())
        .collect()
}

/// What differs between a pinned revision and the current one, as lines a
/// person can act on; empty when they are the same.
#[must_use]
pub fn changes(pinned: &EnvironmentRevision, current: &EnvironmentRevision) -> Vec<String> {
    let mut out = Vec::new();
    if pinned.digest == current.digest {
        return out;
    }
    let by_path = |r: &EnvironmentRevision| -> BTreeMap<String, (String, String)> {
        r.sources
            .iter()
            .map(|s| (s.path.clone(), (s.kind.clone(), s.sha256.clone())))
            .collect()
    };
    let (a, b) = (by_path(pinned), by_path(current));
    for (path, (kind, sha)) in &b {
        match a.get(path) {
            None => out.push(format!("{kind} source added: {path}")),
            Some((_, old)) if old != sha => out.push(format!("{kind} source changed: {path}")),
            _ => {}
        }
    }
    for (path, (kind, _)) in &a {
        if !b.contains_key(path) {
            out.push(format!("{kind} source removed: {path}"));
        }
    }
    let tools = |r: &EnvironmentRevision| -> BTreeMap<String, Option<String>> {
        r.toolchain
            .iter()
            .map(|t| (t.name.clone(), t.version.clone()))
            .collect()
    };
    let (ta, tb) = (tools(pinned), tools(current));
    for (name, v) in &tb {
        match ta.get(name) {
            None => out.push(format!("tool added: {name}")),
            Some(old) if old != v => out.push(format!(
                "tool {name}: {} -> {}",
                old.as_deref().unwrap_or("<absent>"),
                v.as_deref().unwrap_or("<absent>")
            )),
            _ => {}
        }
    }
    for name in ta.keys() {
        if !tb.contains_key(name) {
            out.push(format!("tool removed: {name}"));
        }
    }
    if pinned.path != current.path {
        out.push("PATH entries changed".into());
    }
    let ea: BTreeSet<&String> = pinned.env_names.iter().collect();
    let eb: BTreeSet<&String> = current.env_names.iter().collect();
    for k in eb.difference(&ea) {
        out.push(format!("variable added: {k}"));
    }
    for k in ea.difference(&eb) {
        out.push(format!("variable removed: {k}"));
    }
    for k in ea.intersection(&eb) {
        if pinned.env.get(*k) != current.env.get(*k) && !pinned.env.is_empty() {
            out.push(format!("variable changed: {k}"));
        }
    }
    if pinned.platform != current.platform {
        out.push(format!(
            "platform: {} -> {}",
            pinned.platform, current.platform
        ));
    }
    if out.is_empty() {
        out.push("the revision's content differs".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProbe(BTreeMap<&'static str, &'static str>);

    impl ToolProbe for FakeProbe {
        fn version(&self, name: &str, _arg: &str, _path: &[String]) -> Option<String> {
            self.0.get(name).map(|v| (*v).to_owned())
        }
    }

    fn write(path: &Path, json: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, json).unwrap();
    }

    #[test]
    fn layers_apply_in_order_with_blueprints_first_and_the_digest_follows_every_input() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let data = dir.path().join("data");
        write(
            &data.join("environments/user.json"),
            r#"{"env": {"A": "user", "B": "user"}, "path": ["/opt/user/bin"], "toolchain": [{"name": "git"}]}"#,
        );
        write(
            &data.join("environments/team.json"),
            r#"{"env": {"B": "team"}, "toolchain": [{"name": "node", "optional": true}]}"#,
        );
        write(
            &data.join("environments/blueprints/base.json"),
            r#"{"env": {"C": "blueprint", "A": "blueprint"}, "path": ["tools/bin"]}"#,
        );
        write(
            &root.join(".modbit/environment.json"),
            r#"{"extends": ["base"], "env": {"A": "repo"}, "toolchain": [{"name": "cargo"}]}"#,
        );
        let probe = FakeProbe(
            [("git", "git 2.0"), ("cargo", "cargo 1.97")]
                .into_iter()
                .collect(),
        );
        let rev = snapshot(&root, &data, &probe, 1);
        // Precedence: user < team < blueprint (of the repo layer) < repo.
        assert_eq!(rev.env["A"], "repo");
        assert_eq!(rev.env["B"], "team");
        assert_eq!(rev.env["C"], "blueprint");
        // The user layer's absolute dir passes through; the blueprint's
        // relative dir is under the workspace root (compared with the same
        // `join`, so the platform's separator matches on both sides).
        assert_eq!(rev.path.len(), 2, "{:?}", rev.path);
        assert!(
            rev.path[0].replace('\\', "/").ends_with("opt/user/bin"),
            "{:?}",
            rev.path[0]
        );
        assert_eq!(
            rev.path[1],
            root.join("tools/bin").to_string_lossy().into_owned()
        );
        let kinds: Vec<&str> = rev.sources.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, vec!["user", "team", "blueprint:base", "repository"]);
        assert_eq!(
            missing_tools(&rev),
            Vec::<String>::new(),
            "node is optional: {rev:?}"
        );
        assert!(rev.problems.is_empty(), "{:?}", rev.problems);
        // Deterministic; every input moves it.
        let again = snapshot(&root, &data, &probe, 2);
        assert_eq!(again.digest, rev.digest);
        write(
            &root.join(".modbit/environment.json"),
            r#"{"extends": ["base"], "env": {"A": "repo2"}, "toolchain": [{"name": "cargo"}]}"#,
        );
        let changed = snapshot(&root, &data, &probe, 3);
        assert_ne!(changed.digest, rev.digest);
        let diff = changes(&rev, &changed);
        assert!(
            diff.iter()
                .any(|l| l.starts_with("repository source changed")),
            "{diff:?}"
        );
        assert!(diff.iter().any(|l| l == "variable changed: A"), "{diff:?}");
        let other_tool = FakeProbe(
            [("git", "git 2.1"), ("cargo", "cargo 1.97")]
                .into_iter()
                .collect(),
        );
        let tool_changed = snapshot(&root, &data, &other_tool, 4);
        assert!(
            changes(&changed, &tool_changed)
                .iter()
                .any(|l| l == "tool git: git 2.0 -> git 2.1")
        );
    }

    #[test]
    fn an_absent_required_tool_and_a_missing_blueprint_are_part_of_the_revision() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let data = dir.path().join("data");
        write(
            &root.join(".modbit/environment.json"),
            r#"{"extends": ["nowhere"], "toolchain": [{"name": "zig"}]}"#,
        );
        let rev = snapshot(&root, &data, &FakeProbe(BTreeMap::new()), 1);
        assert_eq!(missing_tools(&rev), vec!["zig".to_owned()]);
        assert!(
            rev.problems
                .iter()
                .any(|p| p.contains("blueprint `nowhere`")),
            "{:?}",
            rev.problems
        );
        // No layer at all is a revision too (the platform alone).
        let empty = snapshot(
            &dir.path().join("none"),
            &data,
            &FakeProbe(BTreeMap::new()),
            1,
        );
        assert!(empty.sources.is_empty());
        assert!(!empty.digest.is_empty());
    }
}
