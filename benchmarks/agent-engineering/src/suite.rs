//! The frozen task list of a suite (docs/63 "Internal competence regression
//! suite"): every task has an observable acceptance in repository state and
//! tests, a seeded difficulty label and a language tier. The suite file and
//! every hidden acceptance file it names are digested together, so the
//! bundle's `task list digest` pins exactly what was run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One suite: id, version, protocol defaults and tasks.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Suite {
    /// Stable suite id (`internal-competence`).
    pub id: String,
    /// Suite version; bumped whenever a task changes.
    pub version: String,
    /// Frozen protocol defaults.
    pub protocol: SuiteProtocol,
    /// The tasks.
    pub tasks: Vec<TaskSpec>,
}

/// Frozen protocol defaults a run must state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuiteProtocol {
    /// Independent trials per task.
    pub trials: u32,
    /// Turn budget per trial.
    pub max_turns: u32,
    /// The one answer the harness gives to any typed question the agent asks.
    pub question_answer: String,
    /// What the harness does with a protected-effect approval (`deny`).
    pub approvals: String,
}

/// One task.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Stable task id (`rust-cli/reject-negative`).
    pub id: String,
    /// Fixture repository under `tests/fixtures/repos`.
    pub fixture: String,
    /// Language (docs/76).
    pub language: String,
    /// Language tier the fixture holds (docs/76).
    pub tier: String,
    /// Seeded difficulty label.
    pub difficulty: String,
    /// The goal text the task is created with; nothing else is said to the agent.
    pub goal: String,
    /// Edits applied to the fixture copy and committed before the run.
    #[serde(default)]
    pub setup: Vec<FileOp>,
    /// How the harness scores repository state after the run.
    pub acceptance: Acceptance,
}

/// A file edit.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FileOp {
    /// Write `content` to `path` (root-relative).
    Write {
        /// Root-relative path.
        path: String,
        /// Full content.
        content: String,
    },
    /// Delete `path`.
    Delete {
        /// Root-relative path.
        path: String,
    },
}

/// Observable acceptance: hidden files placed after the run, commands that
/// must all exit 0, and test files that must be byte-identical (DI-3).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Acceptance {
    /// Files copied into the workspace after the run (the agent never saw them).
    #[serde(default)]
    pub hidden_files: Vec<HiddenFile>,
    /// Commands run in the workspace root; every one must exit 0.
    pub commands: Vec<Vec<String>>,
    /// Root-relative test files whose content must be unchanged by the run.
    #[serde(default)]
    pub protected_tests: Vec<String>,
    /// Environment for the commands.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// A hidden acceptance file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HiddenFile {
    /// Root-relative destination in the workspace.
    pub path: String,
    /// Source, relative to the suite file's directory.
    pub from: String,
}

impl Suite {
    /// A hidden file's bytes as they are placed in a workspace: LF-normalized,
    /// so a CRLF checkout digests and behaves like an LF one.
    ///
    /// # Errors
    /// The file cannot be read.
    pub fn hidden_bytes(path: &Path) -> Result<Vec<u8>, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(String::from_utf8_lossy(&bytes)
            .replace("\r\n", "\n")
            .into_bytes())
    }

    /// Load a suite file and compute its digest: SHA-256 over the canonical
    /// suite JSON followed by each hidden file's LF-normalized bytes, in task
    /// order. Checkout line endings do not change the digest.
    ///
    /// # Errors
    /// The file or a hidden file cannot be read or parsed, or the suite is
    /// structurally invalid (no tasks, duplicate ids, missing commands).
    pub fn load(path: &Path) -> Result<(Self, String), String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let suite: Self =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        suite.check()?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        let canonical = serde_json::to_vec(&suite).map_err(|e| e.to_string())?;
        h.update(&canonical);
        h.update([0]);
        for t in &suite.tasks {
            for f in &t.acceptance.hidden_files {
                let src = base.join(&f.from);
                let bytes =
                    Self::hidden_bytes(&src).map_err(|e| format!("{}: hidden file {e}", t.id))?;
                h.update(t.id.as_bytes());
                h.update([0]);
                h.update(f.path.as_bytes());
                h.update([0]);
                h.update(&bytes);
                h.update([0]);
            }
        }
        Ok((suite, hex::encode(h.finalize())))
    }

    /// Structural checks.
    ///
    /// # Errors
    /// No tasks, a duplicate id, a task without acceptance commands, or a
    /// protocol without trials.
    pub fn check(&self) -> Result<(), String> {
        if self.tasks.is_empty() {
            return Err("suite has no tasks".into());
        }
        if self.protocol.trials == 0 {
            return Err("suite protocol must state a positive trial count".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for t in &self.tasks {
            if !seen.insert(t.id.as_str()) {
                return Err(format!("duplicate task id {}", t.id));
            }
            if t.acceptance.commands.is_empty() {
                return Err(format!("{}: acceptance names no command", t.id));
            }
            if t.goal.trim().is_empty() {
                return Err(format!("{}: empty goal", t.id));
            }
        }
        Ok(())
    }

    /// Resolve a hidden file's source path against the suite file's directory.
    #[must_use]
    pub fn hidden_source(suite_path: &Path, f: &HiddenFile) -> PathBuf {
        suite_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&f.from)
    }
}
