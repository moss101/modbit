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
    /// The run mode the harness sets on every task before its run, as a
    /// person would with `run-mode set --ack` (PX-184: the sub-agent axis
    /// integrates its children, which `ASK` would park on an approval).
    /// Absent: the Core's default (`ASK`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_mode: Option<String>,
    /// The four-axis protocol (PX-184): when present every task is tagged by
    /// axis and carries its own verifier, and the report scores each axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<crate::axes::AxesProtocol>,
}

/// The approvals protocol of a GUI suite (PX-179): the harness, as the
/// person at the fixture desktop, approves each `computer.*` input the Core
/// asks about and denies every other protected effect (a handover included:
/// nobody is there to do the step).
pub const APPROVE_COMPUTER_INPUTS: &str = "approve-computer-inputs";

/// Run modes the Core accepts on `run-mode set` (docs/domain `RunMode`).
pub const RUN_MODES: [&str; 5] = [
    "ASK",
    "ALLOWLIST",
    "ALLOWLIST_SANDBOX",
    "RUN_EVERYTHING",
    "AUTONOMOUS",
];

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
    /// The capability the task exercises (PX-184): `tool_use`, `memory`,
    /// `skill_use` or `subagent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<String>,
    /// The task's own verifier (PX-184): observable outcomes in repository
    /// state and the Core's own records, never the model's claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier: Option<crate::verifier::Verifier>,
    /// What the harness puts in place through the product before the run:
    /// curated memory and owner-trusted skills (PX-184).
    #[serde(default, skip_serializing_if = "crate::verifier::Seed::is_empty")]
    pub seed: crate::verifier::Seed,
    /// A GUI task (PX-179): the desktop the Core's actuator serves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gui: Option<GuiTask>,
}

/// The desktop of a GUI task (PX-179).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuiTask {
    /// The fixture desktop (`modbit-actuator-fixture --desktop`), relative to
    /// the suite file's directory; digested with the suite.
    pub desktop: String,
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
            if let Some(g) = &t.gui {
                let bytes = Self::hidden_bytes(&base.join(&g.desktop))
                    .map_err(|e| format!("{}: desktop {e}", t.id))?;
                h.update(t.id.as_bytes());
                h.update([0]);
                h.update(b"desktop");
                h.update([0]);
                h.update(&bytes);
                h.update([0]);
            }
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
        if let Some(mode) = &self.protocol.run_mode
            && !RUN_MODES.contains(&mode.as_str())
        {
            return Err(format!("run mode {mode:?} is not one of {RUN_MODES:?}"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for t in &self.tasks {
            if !seen.insert(t.id.as_str()) {
                return Err(format!("duplicate task id {}", t.id));
            }
            // A task is scored by its acceptance commands, its own verifier,
            // or both; a task with neither scores nothing.
            if t.acceptance.commands.is_empty() && t.verifier.is_none() {
                return Err(format!("{}: acceptance names no command", t.id));
            }
            if let Some(v) = &t.verifier {
                v.check().map_err(|e| format!("{}: {e}", t.id))?;
            }
            t.seed.check().map_err(|e| format!("{}: {e}", t.id))?;
            if t.goal.trim().is_empty() {
                return Err(format!("{}: empty goal", t.id));
            }
            // PX-179: a GUI task names its desktop, is verified by the GUI
            // verifier row, and runs under the approvals protocol that has
            // the harness approve its inputs as the person.
            if let Some(g) = &t.gui {
                if g.desktop.trim().is_empty() || g.desktop.split(['/', '\\']).any(|p| p == "..") {
                    return Err(format!(
                        "{}: desktop path {:?} is not inside the suite",
                        t.id, g.desktop
                    ));
                }
                if t.verifier.as_ref().is_none_or(|v| v.gui.is_none()) {
                    return Err(format!("{}: a GUI task needs a gui verifier", t.id));
                }
                if self.protocol.approvals != APPROVE_COMPUTER_INPUTS {
                    return Err(format!(
                        "{}: a GUI task needs the {APPROVE_COMPUTER_INPUTS} approvals protocol",
                        t.id
                    ));
                }
            } else if t.verifier.as_ref().is_some_and(|v| v.gui.is_some()) {
                return Err(format!("{}: a gui verifier needs a GUI task", t.id));
            }
        }
        if self.protocol.axes.is_some() {
            crate::axes::check_suite(self)?;
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

/// Whether a protected test file is intact in the sense of docs/63: no test
/// named by the acceptance was modified, deleted, skipped or weakened.
/// Every original line that declares a test or asserts something must still
/// appear, unchanged and in order, in the current content; additions
/// anywhere (new tests, an import gaining a symbol) are allowed. A deleted
/// file is not intact.
#[must_use]
pub fn protected_intact(original: &str, current: Option<&str>) -> bool {
    let Some(current) = current else {
        return false;
    };
    let significant = |line: &str| {
        let t = line.trim();
        !t.is_empty()
            && (t.contains("assert")
                || t.contains("raise ")
                || t.contains("expect(")
                || t.starts_with("def test")
                || t.starts_with("async def test")
                || t.contains("#[test]")
                || t.starts_with("fn ")
                || t.starts_with("pub fn ")
                || t.contains("it(")
                || t.contains("test(")
                || t.contains("describe("))
    };
    let mut cur = current.lines().map(str::trim);
    for wanted in original.lines().map(str::trim).filter(|l| significant(l)) {
        if !cur.any(|l| l == wanted) {
            return false;
        }
    }
    true
}
