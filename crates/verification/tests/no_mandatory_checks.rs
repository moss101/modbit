//! FIX-03 (audit G section 8): an empty mandatory check set is INDETERMINATE,
//! never Passed, and an argv defined by repository content is authorized
//! before it runs. Real commands through a real process runner.

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use modbit_verification::engine::BoxFuture;
use modbit_verification::*;

/// Runs commands for real; counts what it was asked to run and can refuse
/// repository-defined commands the way the Core's kernel binding does.
struct GatedRunner {
    refuse_repo_defined: bool,
    ran: AtomicUsize,
    asked: Mutex<Vec<String>>,
}

impl GatedRunner {
    fn new(refuse_repo_defined: bool) -> Self {
        Self {
            refuse_repo_defined,
            ran: AtomicUsize::new(0),
            asked: Mutex::new(Vec::new()),
        }
    }
}

impl CommandRunner for GatedRunner {
    fn authorize<'a>(
        &'a self,
        check_id: &'a str,
        _argv: &'a [String],
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(check_id.to_owned());
            if self.refuse_repo_defined {
                Err("LEASE_REQUIRED: no lease".into())
            } else {
                Ok(())
            }
        })
    }

    fn run<'a>(
        &'a self,
        argv: &'a [String],
        cwd: &'a Path,
        env: &'a [(String, String)],
        _timeout_ms: u64,
    ) -> BoxFuture<'a, RawRun> {
        Box::pin(async move {
            self.ran.fetch_add(1, Ordering::SeqCst);
            let argv = argv.to_vec();
            let cwd = cwd.to_path_buf();
            let env = env.to_vec();
            tokio::task::spawn_blocking(move || {
                match Command::new(&argv[0])
                    .args(&argv[1..])
                    .current_dir(&cwd)
                    .envs(env)
                    .output()
                {
                    Ok(o) => RawRun {
                        exit_code: o.status.code(),
                        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                        ..Default::default()
                    },
                    Err(e) => RawRun {
                        exit_code: None,
                        stderr: e.to_string(),
                        ..Default::default()
                    },
                }
            })
            .await
            .unwrap()
        })
    }
}

#[derive(Default)]
struct MemSink;

impl ArtifactSink for MemSink {
    fn put(&self, bytes: &[u8]) -> String {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(bytes))
    }
}

async fn completion(runner: &GatedRunner, plan: &VerificationPlan, root: &Path) -> VerificationRun {
    let engine = VerificationEngine::new(runner, &MemSink, VerificationPolicy::default());
    engine
        .run_stage(
            plan,
            "plan",
            "run",
            Stage::Completion,
            root,
            "rev-1",
            &[],
            &[],
        )
        .await
        .0
}

fn command(id: &str, argv: &[&str], mandatory: bool) -> CheckCommand {
    CheckCommand {
        id: id.into(),
        family: RunnerFamily::ConfiguredCommand,
        argv: argv.iter().map(|a| (*a).to_owned()).collect(),
        mandatory,
        reporter_file: None,
    }
}

fn passing_argv() -> Vec<&'static str> {
    if cfg!(windows) {
        vec!["cmd", "/C", "exit 0"]
    } else {
        vec!["sh", "-c", "exit 0"]
    }
}

#[tokio::test]
async fn an_empty_plan_is_indeterminate_and_blocks_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    // A repository with no stack and no `.modbit/verification.json`.
    std::fs::write(dir.path().join("notes.txt"), "hello\n").unwrap();
    let plan = derive(dir.path(), &[], &[]);
    assert!(plan.commands.is_empty(), "{plan:?}");
    let runner = GatedRunner::new(false);
    let run = completion(&runner, &plan, dir.path()).await;
    assert_eq!(run.status, ReportStatus::Unknown, "{run:?}");
    assert_eq!(
        run.indeterminate_reason,
        Some(IndeterminateReason::NoMandatoryChecks)
    );
    assert_eq!(runner.ran.load(Ordering::SeqCst), 0);
    let baseline = VerificationRun {
        stage: Stage::Baseline,
        status: ReportStatus::Unknown,
        ..run.clone()
    };
    let a = attribute(&baseline, &run, &plan);
    assert!(a.blocks_acceptance && a.inconclusive, "{a:?}");
    assert!(
        a.reasons
            .iter()
            .any(|r| r.starts_with("NO_MANDATORY_CHECKS")),
        "{a:?}"
    );
}

#[tokio::test]
async fn a_plan_with_only_optional_commands_is_indeterminate_when_they_pass() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = derive(dir.path(), &[], &[]);
    plan.commands = vec![command("suite:optional", &passing_argv(), false)];
    let runner = GatedRunner::new(false);
    let run = completion(&runner, &plan, dir.path()).await;
    // The optional command ran and passed, but nothing mandatory stands
    // behind the pass.
    assert_eq!(runner.ran.load(Ordering::SeqCst), 1);
    assert_eq!(run.status, ReportStatus::Unknown, "{run:?}");
    assert_eq!(
        run.indeterminate_reason,
        Some(IndeterminateReason::NoMandatoryChecks)
    );
}

#[tokio::test]
async fn a_mandatory_passing_command_still_passes() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = derive(dir.path(), &[], &[]);
    plan.commands = vec![command("suite:ok", &passing_argv(), true)];
    let runner = GatedRunner::new(false);
    let run = completion(&runner, &plan, dir.path()).await;
    assert_eq!(run.status, ReportStatus::Passed, "{run:?}");
    assert_eq!(run.indeterminate_reason, None);
}

#[tokio::test]
async fn repository_defined_argv_is_authorized_before_it_runs_and_refusal_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let argv: Vec<String> = passing_argv().into_iter().map(str::to_owned).collect();
    std::fs::create_dir_all(dir.path().join(".modbit")).unwrap();
    std::fs::write(
        dir.path().join(".modbit/verification.json"),
        serde_json::json!({"commands": [{"id": "gate", "argv": argv}]}).to_string(),
    )
    .unwrap();
    let plan = derive(dir.path(), &[], &[]);
    assert_eq!(plan.commands.len(), 1, "{plan:?}");
    assert!(plan.commands[0].is_repo_defined());

    // Refused by the authorizer: not run, UNKNOWN, never a pass.
    let refusing = GatedRunner::new(true);
    let run = completion(&refusing, &plan, dir.path()).await;
    assert_eq!(refusing.ran.load(Ordering::SeqCst), 0, "{run:?}");
    assert_eq!(*refusing.asked.lock().unwrap(), vec!["configured:gate"]);
    assert_eq!(run.status, ReportStatus::Unknown, "{run:?}");
    assert!(
        run.checks()
            .iter()
            .all(|c| c.status == CheckStatus::Unknown),
        "{run:?}"
    );

    // Allowed: it runs and passes.
    let allowing = GatedRunner::new(false);
    let run = completion(&allowing, &plan, dir.path()).await;
    assert_eq!(allowing.ran.load(Ordering::SeqCst), 1);
    assert_eq!(run.status, ReportStatus::Passed, "{run:?}");
}

#[tokio::test]
async fn engine_derived_commands_are_not_asked_to_the_authorizer() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = derive(dir.path(), &[], &[]);
    plan.commands = vec![command("suite:derived", &passing_argv(), true)];
    let runner = GatedRunner::new(true);
    let run = completion(&runner, &plan, dir.path()).await;
    assert!(runner.asked.lock().unwrap().is_empty());
    assert_eq!(run.status, ReportStatus::Passed, "{run:?}");
}
