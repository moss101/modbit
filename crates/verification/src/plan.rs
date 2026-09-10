//! Derived verification plan (docs/28 §4): from repository configuration,
//! the task's named checks and the agent's proposals; recorded before the
//! first run. The agent may add checks; it cannot remove mandatory ones.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::report::RunnerFamily;

/// One configured check command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckCommand {
    /// Stable id (`suite:cargo`, `build:cargo`, ...).
    pub id: String,
    /// Runner family for parsing.
    pub family: RunnerFamily,
    /// argv.
    pub argv: Vec<String>,
    /// Mandatory for acceptance.
    pub mandatory: bool,
    /// Structured reporter file the runner writes (relative to cwd), if any.
    pub reporter_file: Option<String>,
}

/// The derived plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationPlan {
    /// Plan version.
    pub version: u32,
    /// Detected stacks.
    pub stacks: Vec<String>,
    /// Commands in execution order.
    pub commands: Vec<CheckCommand>,
    /// Check ids named by the task's acceptance (DI-3 protected).
    pub acceptance_named: Vec<String>,
    /// Expected behavior changes declared before the run (check id → reason).
    pub declared_changes: Vec<(String, String)>,
    /// Environment digest inputs (toolchain/lockfile names).
    pub environment_inputs: Vec<String>,
    /// Limitations stated in the plan (missing runners, narrowed scope).
    pub limitations: Vec<String>,
}

/// The checks a repository declares for itself in `.modbit/verification.json`
/// (PX-029): `{"commands": [{"id", "argv", "mandatory"?, "reporter_file"?}]}`.
/// Ids are namespaced so a declaration cannot impersonate a detected runner.
#[must_use]
pub fn configured_commands(root: &Path) -> Vec<CheckCommand> {
    let Ok(text) = std::fs::read_to_string(root.join(".modbit/verification.json")) else {
        return vec![];
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return vec![];
    };
    v["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let argv: Vec<String> = c["argv"]
                .as_array()?
                .iter()
                .filter_map(|a| a.as_str().map(str::to_owned))
                .collect();
            if argv.is_empty() {
                return None;
            }
            let id = c["id"].as_str().filter(|i| !i.is_empty())?;
            Some(CheckCommand {
                id: format!("configured:{id}"),
                family: RunnerFamily::ConfiguredCommand,
                argv,
                mandatory: c["mandatory"].as_bool().unwrap_or(true),
                reporter_file: c["reporter_file"].as_str().map(str::to_owned),
            })
        })
        .take(8)
        .collect()
}

/// Derive the plan from a workspace root.
#[must_use]
pub fn derive(
    root: &Path,
    task_named: &[String],
    extra_commands: &[CheckCommand],
) -> VerificationPlan {
    let mut stacks = Vec::new();
    let mut commands = Vec::new();
    let mut env_inputs = Vec::new();
    let mut limitations = Vec::new();
    if root.join("Cargo.toml").exists() {
        stacks.push("rust".into());
        env_inputs.push("Cargo.lock".into());
        commands.push(CheckCommand {
            id: "build:cargo".into(),
            family: RunnerFamily::ConfiguredCommand,
            argv: vec!["cargo".into(), "build".into(), "--offline".into()],
            mandatory: true,
            reporter_file: None,
        });
        commands.push(CheckCommand {
            id: "suite:cargo".into(),
            family: RunnerFamily::Cargo,
            argv: vec![
                "cargo".into(),
                "test".into(),
                "--offline".into(),
                "--".into(),
                "--test-threads=1".into(),
            ],
            mandatory: true,
            reporter_file: None,
        });
    }
    if root.join("package.json").exists() {
        stacks.push("node".into());
        env_inputs.push("pnpm-lock.yaml".into());
        let pkg = std::fs::read_to_string(root.join("package.json")).unwrap_or_default();
        if pkg.contains("vitest") {
            commands.push(CheckCommand {
                id: "suite:vitest".into(),
                family: RunnerFamily::Vitest,
                argv: vec![
                    "node".into(),
                    "node_modules/vitest/vitest.mjs".into(),
                    "run".into(),
                    "--reporter=json".into(),
                    "--outputFile=.modbit-vitest.json".into(),
                ],
                mandatory: true,
                reporter_file: Some(".modbit-vitest.json".into()),
            });
        } else if pkg.contains("\"test\"") {
            commands.push(CheckCommand {
                id: "suite:npm-test".into(),
                family: RunnerFamily::ConfiguredCommand,
                argv: vec!["pnpm".into(), "test".into()],
                mandatory: true,
                reporter_file: None,
            });
            limitations
                .push("node test script has no structured reporter; HEURISTIC evidence".into());
        }
    }
    if root.join("pyproject.toml").exists() || root.join("pytest.ini").exists() {
        stacks.push("python".into());
        commands.push(CheckCommand {
            id: "suite:pytest".into(),
            family: RunnerFamily::Pytest,
            argv: vec![
                // Hosted Windows runners and most Windows installs expose
                // `python`, not `python3`.
                if cfg!(windows) { "python" } else { "python3" }.into(),
                "-m".into(),
                "pytest".into(),
                "-q".into(),
                "--junitxml=.modbit-pytest.xml".into(),
            ],
            mandatory: true,
            reporter_file: Some(".modbit-pytest.xml".into()),
        });
    }
    // A repository can declare its own checks (docs/64 "Adapters", PX-029):
    // this is how a language with no runner of ours gets evidence at all, and
    // the plan says the evidence is heuristic because it is exit-code based.
    for c in configured_commands(root) {
        if !commands.iter().any(|x| x.id == c.id) {
            limitations.push(format!(
                "`{}` is a repository-configured command; its evidence is HEURISTIC (exit code and output)",
                c.id
            ));
            commands.push(c);
        }
    }
    for c in extra_commands {
        if !commands.iter().any(|x| x.id == c.id) {
            commands.push(c.clone());
        }
    }
    if commands.is_empty() {
        limitations
            .push("no configured runner detected; verification needs a configured command".into());
    }
    VerificationPlan {
        version: 1,
        stacks,
        commands,
        acceptance_named: task_named.to_vec(),
        declared_changes: vec![],
        environment_inputs: env_inputs,
        limitations,
    }
}
