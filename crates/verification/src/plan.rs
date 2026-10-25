//! Derived verification plan (docs/28 §4): from repository configuration,
//! the task's named checks and the agent's proposals; recorded before the
//! first run. The agent may add checks; it cannot remove mandatory ones.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::report::RunnerFamily;

/// Id prefix of a check the repository declares for itself in
/// `.modbit/verification.json`: the argv comes from repository content, so it
/// is an effect the capability kernel decides before it runs (FIX-03).
pub const CONFIGURED_ID_PREFIX: &str = "configured:";

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

impl CheckCommand {
    /// Whether the argv was defined by repository content rather than derived
    /// by the engine from a detected stack.
    #[must_use]
    pub fn is_repo_defined(&self) -> bool {
        self.id.starts_with(CONFIGURED_ID_PREFIX)
    }
}

/// An adapter's diagnostics batch the plan was derived with (PX-004): an
/// input that says where an editor's language service saw problems at this
/// revision. It targets nothing by itself and passes nothing: every
/// mandatory command runs regardless of what the batch reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalDiagnosticsInput {
    /// Language service / adapter identity.
    pub source: String,
    /// Its version.
    pub source_version: String,
    /// Workspace revision the batch is bound to (the plan's revision).
    pub workspace_revision: u64,
    /// Object hash of the normalized batch.
    pub batch_ref: String,
    /// Diagnostics in the batch.
    pub count: u32,
    /// Root-relative paths named.
    pub paths: Vec<String>,
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
    /// External diagnostics batches at the plan's revision (PX-004), an
    /// input the plan was derived with; empty on plans made without any.
    #[serde(default)]
    pub external_diagnostics: Vec<ExternalDiagnosticsInput>,
}

/// The checks a repository declares for itself in `.modbit/verification.json`
/// (PX-029): `{"commands": [{"id", "argv", "mandatory"?, "reporter_file"?}]}`.
/// Ids are namespaced so a declaration cannot impersonate a detected runner.
#[must_use]
pub fn configured_commands(root: &Path) -> Vec<CheckCommand> {
    let Ok(text) = std::fs::read_to_string(root.join(".modbit/verification.json")) else {
        return vec![];
    };
    configured_commands_from(&text)
}

/// [`configured_commands`] over the text of `.modbit/verification.json` as
/// the caller read it: a run reads the file once, when it starts, so a
/// process that rewrites it during the run does not choose what the run
/// executes (FIX-04).
#[must_use]
pub fn configured_commands_from(text: &str) -> Vec<CheckCommand> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
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
                id: format!("{CONFIGURED_ID_PREFIX}{id}"),
                family: RunnerFamily::ConfiguredCommand,
                argv,
                mandatory: c["mandatory"].as_bool().unwrap_or(true),
                reporter_file: c["reporter_file"].as_str().map(str::to_owned),
            })
        })
        .take(8)
        .collect()
}

/// The package manager a Node project uses: the `packageManager` field of its
/// `package.json`, else the lockfile it keeps, else `npm`.
fn node_package_manager(root: &Path, package_json: &str) -> &'static str {
    let declared = serde_json::from_str::<serde_json::Value>(package_json)
        .ok()
        .and_then(|v| v["packageManager"].as_str().map(str::to_owned));
    if let Some(d) = declared {
        for m in ["pnpm", "yarn", "bun", "npm"] {
            if d.starts_with(m) {
                return m;
            }
        }
    }
    if root.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if root.join("yarn.lock").exists() {
        "yarn"
    } else if root.join("bun.lockb").exists() || root.join("bun.lock").exists() {
        "bun"
    } else {
        "npm"
    }
}

/// The `scripts` of a `package.json`, in file order.
fn node_scripts(package_json: &str) -> Vec<(String, String)> {
    serde_json::from_str::<serde_json::Value>(package_json)
        .ok()
        .and_then(|v| {
            v["scripts"].as_object().map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
                    .collect()
            })
        })
        .unwrap_or_default()
}

/// A binary the project installed under `node_modules/.bin`, as a path
/// relative to the root.
fn local_bin(name: &str) -> String {
    if cfg!(windows) {
        format!("node_modules/.bin/{name}.cmd")
    } else {
        format!("node_modules/.bin/{name}")
    }
}

/// Derive the plan from a workspace root.
#[must_use]
pub fn derive(
    root: &Path,
    task_named: &[String],
    extra_commands: &[CheckCommand],
) -> VerificationPlan {
    let configured = std::fs::read_to_string(root.join(".modbit/verification.json")).ok();
    derive_with_configured(root, task_named, extra_commands, configured.as_deref())
}

/// [`derive`] with the text of `.modbit/verification.json` supplied by the
/// caller (`None`: the repository declared no commands) instead of read from
/// the working tree now.
#[must_use]
pub fn derive_with_configured(
    root: &Path,
    task_named: &[String],
    extra_commands: &[CheckCommand],
    configured: Option<&str>,
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
        let pkg = std::fs::read_to_string(root.join("package.json")).unwrap_or_default();
        let manager = node_package_manager(root, &pkg);
        env_inputs.push(
            match manager {
                "pnpm" => "pnpm-lock.yaml",
                "yarn" => "yarn.lock",
                "bun" => "bun.lockb",
                _ => "package-lock.json",
            }
            .into(),
        );
        let scripts = node_scripts(&pkg);
        let script = |names: &[&str]| -> Option<String> {
            names
                .iter()
                .find(|n| scripts.iter().any(|(k, v)| k == *n && !v.trim().is_empty()))
                .map(|n| (*n).to_owned())
        };
        let run = |name: &str| -> Vec<String> { vec![manager.into(), "run".into(), name.into()] };
        let script_text = |name: &str| {
            scripts
                .iter()
                .find(|(k, _)| k == name)
                .map_or("", |(_, v)| v.as_str())
        };
        // A type error or a failing build is the first thing a coding task
        // meets in a JavaScript or TypeScript project, so the plan names them
        // beside the tests (a plan with only a test script left a project
        // whose tests do not exist with nothing to verify at all).
        let build = script(&["build", "compile"]);
        let build_covers_tsc = build
            .as_deref()
            .is_some_and(|b| script_text(b).contains("tsc"));
        if let Some(name) = script(&[
            "typecheck",
            "type-check",
            "check-types",
            "types:check",
            "check:types",
        ]) {
            commands.push(CheckCommand {
                id: "typecheck:node".into(),
                family: RunnerFamily::ConfiguredCommand,
                argv: run(&name),
                mandatory: true,
                reporter_file: None,
            });
        } else if !build_covers_tsc
            && root.join("tsconfig.json").exists()
            && root.join(local_bin("tsc")).exists()
        {
            commands.push(CheckCommand {
                id: "typecheck:tsc".into(),
                family: RunnerFamily::ConfiguredCommand,
                argv: vec![local_bin("tsc"), "--noEmit".into()],
                mandatory: true,
                reporter_file: None,
            });
        }
        if let Some(name) = &build {
            commands.push(CheckCommand {
                id: "build:node".into(),
                family: RunnerFamily::ConfiguredCommand,
                argv: run(name),
                mandatory: true,
                reporter_file: None,
            });
        }
        if let Some(name) = script(&["lint"]) {
            // Advisory: a lint failure the repository already had is not the
            // task's to fix, so it never blocks acceptance on its own.
            commands.push(CheckCommand {
                id: "lint:node".into(),
                family: RunnerFamily::ConfiguredCommand,
                argv: run(&name),
                mandatory: false,
                reporter_file: None,
            });
        }
        let has_test_script = scripts
            .iter()
            .any(|(k, v)| k == "test" && !v.contains("no test specified") && !v.trim().is_empty());
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
        } else if has_test_script {
            commands.push(CheckCommand {
                id: "suite:npm-test".into(),
                family: RunnerFamily::ConfiguredCommand,
                argv: if manager == "bun" {
                    vec!["bun".into(), "run".into(), "test".into()]
                } else {
                    vec![manager.into(), "test".into()]
                },
                mandatory: true,
                reporter_file: None,
            });
            limitations
                .push("node test script has no structured reporter; HEURISTIC evidence".into());
        }
        if commands
            .iter()
            .any(|c| c.id.ends_with(":node") || c.id == "typecheck:tsc")
        {
            limitations.push(
                "node build, type-check and lint checks are exit-code based; their evidence is HEURISTIC"
                    .into(),
            );
        }
    }
    if root.join("go.mod").exists() {
        stacks.push("go".into());
        env_inputs.push("go.sum".into());
        commands.push(CheckCommand {
            id: "build:go".into(),
            family: RunnerFamily::ConfiguredCommand,
            argv: vec!["go".into(), "build".into(), "./...".into()],
            mandatory: true,
            reporter_file: None,
        });
        commands.push(CheckCommand {
            id: "vet:go".into(),
            family: RunnerFamily::ConfiguredCommand,
            argv: vec!["go".into(), "vet".into(), "./...".into()],
            mandatory: false,
            reporter_file: None,
        });
        commands.push(CheckCommand {
            id: "suite:go".into(),
            family: RunnerFamily::ConfiguredCommand,
            argv: vec!["go".into(), "test".into(), "./...".into()],
            mandatory: true,
            reporter_file: None,
        });
        limitations.push("go checks are exit-code based; their evidence is HEURISTIC".into());
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
    for c in configured.map(configured_commands_from).unwrap_or_default() {
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
        external_diagnostics: vec![],
    }
}
