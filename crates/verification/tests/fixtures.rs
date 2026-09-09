//! Verification engine against the real Alpha fixtures (docs/50): real cargo
//! and vitest runs, structured parsing, the flake rerun protocol, regression
//! attribution and the diff invariants.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use modbit_verification::engine::BoxFuture;
use modbit_verification::*;

struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run<'a>(
        &'a self,
        argv: &'a [String],
        cwd: &'a Path,
        env: &'a [(String, String)],
        timeout_ms: u64,
    ) -> BoxFuture<'a, RawRun> {
        Box::pin(async move {
            let argv = argv.to_vec();
            let cwd = cwd.to_path_buf();
            let env = env.to_vec();
            tokio::task::spawn_blocking(move || {
                let mut cmd = Command::new(&argv[0]);
                cmd.args(&argv[1..])
                    .current_dir(&cwd)
                    .envs(env.iter().cloned());
                let started = std::time::Instant::now();
                let out = cmd.output();
                let _ = timeout_ms;
                match out {
                    Ok(o) => RawRun {
                        exit_code: o.status.code(),
                        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                        reporter_file: None,
                        timed_out: started.elapsed().as_millis() as u64 > timeout_ms,
                        cancelled: false,
                    },
                    Err(e) => RawRun {
                        exit_code: None,
                        stdout: String::new(),
                        stderr: e.to_string(),
                        reporter_file: None,
                        timed_out: false,
                        cancelled: false,
                    },
                }
            })
            .await
            .unwrap()
        })
    }
}

#[derive(Default)]
struct MemSink(Mutex<Vec<Vec<u8>>>);

impl ArtifactSink for MemSink {
    fn put(&self, bytes: &[u8]) -> String {
        use sha2::Digest;
        self.0.lock().unwrap().push(bytes.to_vec());
        hex::encode(sha2::Sha256::digest(bytes))
    }
}

fn fixture_copy(name: &str) -> (tempfile::TempDir, PathBuf) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/repos")
        .join(name);
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join(name);
    copy_dir(&src, &dst);
    (dir, dst)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let name = e.file_name();
        if name == "target" || name == "node_modules" || name == ".vitest" {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &dst.join(&name));
        } else {
            // Checkouts on Windows may carry CRLF; fixtures are LF text.
            let bytes = std::fs::read(&p).unwrap();
            let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
            std::fs::write(dst.join(&name), text).unwrap();
        }
    }
}

fn flaky_env(dir: &Path) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = std::env::vars().collect();
    env.retain(|(k, _)| k != "FIXTURE_FLAKY_STATE");
    env.push((
        "FIXTURE_FLAKY_STATE".into(),
        dir.join("flaky-state").to_string_lossy().into_owned(),
    ));
    env.push(("CARGO_TERM_COLOR".into(), "always".into()));
    env
}

#[tokio::test]
async fn cargo_fixture_baseline_labels_known_failing_quarantines_flaky_and_attributes_regressions()
{
    let (tmp, root) = fixture_copy("rust-cli");
    let plan = derive(&root, &["acceptance_rejects_negative_quantity".into()], &[]);
    assert_eq!(plan.stacks, vec!["rust".to_owned()]);
    assert!(
        plan.commands
            .iter()
            .any(|c| c.id == "suite:cargo" && c.mandatory)
    );
    let sink = MemSink::default();
    let engine = VerificationEngine::new(&ProcessRunner, &sink, VerificationPolicy::default());
    let env = flaky_env(tmp.path());
    let (baseline, quarantines) = engine
        .run_stage(
            &plan,
            "plan-1",
            "run-1",
            Stage::Baseline,
            &root,
            "rev-0",
            &env,
            &["rustc-test".into()],
        )
        .await;
    assert_eq!(baseline.stage, Stage::Baseline);
    let suite = baseline
        .reports
        .iter()
        .find(|r| r.runner.family == RunnerFamily::Cargo)
        .unwrap();
    assert_eq!(suite.parser.confidence, Confidence::Structured, "{suite:?}");
    let status_of = |r: &VerificationRun, name: &str| {
        r.checks()
            .into_iter()
            .find(|c| c.location.symbol.as_deref() == Some(name))
            .map(|c| c.status)
    };
    assert_eq!(
        status_of(&baseline, "formats_totals"),
        Some(CheckStatus::Pass)
    );
    assert_eq!(
        status_of(&baseline, "acceptance_rejects_negative_quantity"),
        Some(CheckStatus::Fail)
    );
    assert_eq!(
        status_of(&baseline, "preexisting_failing_unrelated"),
        Some(CheckStatus::Fail)
    );
    // The seeded flaky test failed once and passed on the isolated rerun: FLAKY, quarantined, no signature.
    assert_eq!(
        status_of(&baseline, "flaky_first_run_fails"),
        Some(CheckStatus::Flaky),
        "{:?}",
        baseline.checks()
    );
    assert_eq!(quarantines.len(), 1);
    assert_eq!(
        quarantines[0].check_id,
        "cargo:tests/quantities.rs::flaky_first_run_fails"
    );
    assert!(
        baseline.reports.iter().any(|r| r.stage == Stage::Rerun),
        "rerun report retained"
    );
    let sigs = baseline.failure_signatures();
    assert_eq!(sigs.len(), 2, "{sigs:?}");
    assert!(sigs.iter().all(|s| !s.contains("flaky")));
    // Failing checks carry location, error class and a stable fingerprint; the raw log is retained.
    let acc = baseline
        .checks()
        .into_iter()
        .find(|c| c.location.symbol.as_deref() == Some("acceptance_rejects_negative_quantity"))
        .unwrap();
    assert_eq!(
        (acc.location.path.as_deref(), acc.error_class.as_deref()),
        (Some("tests/quantities.rs"), Some("panic")),
        "a bare assert! with a custom message panics without the word assertion"
    );
    let pre = baseline
        .checks()
        .into_iter()
        .find(|c| c.location.symbol.as_deref() == Some("preexisting_failing_unrelated"))
        .unwrap();
    assert_eq!(pre.error_class.as_deref(), Some("assertion_eq"));
    assert!(pre.location.line.is_some(), "{pre:?}");
    assert!(
        acc.message_fingerprint
            .as_deref()
            .unwrap()
            .contains("negative quantities must be rejected")
    );
    assert!(acc.output_range.is_some());
    assert!(
        sink.0
            .lock()
            .unwrap()
            .iter()
            .any(|b| String::from_utf8_lossy(b).contains("negative quantities must be rejected"))
    );
    assert_eq!(suite.raw_output_ref.len(), 64);
    // Signature stability: a second baseline at the same revision yields the same signatures.
    let (again, _) = engine
        .run_stage(
            &plan,
            "plan-1",
            "run-1",
            Stage::Baseline,
            &root,
            "rev-0",
            &env,
            &["rustc-test".into()],
        )
        .await;
    assert_eq!(again.failure_signatures(), sigs);
    assert_eq!(again.environment_digest, baseline.environment_digest);

    // Candidate change: fix the acceptance test, break formats_totals.
    let lib = root.join("src/lib.rs");
    let src = std::fs::read_to_string(&lib).unwrap();
    let fixed = src
        .replace(
            "    Ok(n)\n",
            "    if n < 0 {\n        return Err(\"negative quantity\".into());\n    }\n    Ok(n)\n",
        )
        .replace(
            "format!(\"{}.{:02}\", cents / 100, cents % 100)",
            "format!(\"{}.{:03}\", cents / 100, cents % 100)",
        );
    std::fs::write(&lib, fixed).unwrap();
    let (completion, _) = engine
        .run_stage(
            &plan,
            "plan-1",
            "run-1",
            Stage::Completion,
            &root,
            "rev-1",
            &env,
            &["rustc-test".into()],
        )
        .await;
    let a = attribute(&baseline, &completion, &plan);
    let of = |id: &str| {
        a.checks
            .iter()
            .find(|(c, _)| c.ends_with(id))
            .map(|(_, x)| *x)
    };
    assert_eq!(of("::formats_totals"), Some(Attribution::Regression));
    assert_eq!(
        of("::acceptance_rejects_negative_quantity"),
        Some(Attribution::CollateralFix)
    );
    assert_eq!(
        of("::preexisting_failing_unrelated"),
        Some(Attribution::KnownFailing)
    );
    assert_eq!(
        of("::flaky_first_run_fails"),
        Some(Attribution::Pass),
        "quarantined state file already primed"
    );
    assert!(a.blocks_acceptance, "{a:?}");
    assert!(a.reasons.iter().any(|r| r.contains("REGRESSION")));
    // A change declared before the run is not a regression.
    let mut declared = plan.clone();
    declared.declared_changes.push((
        "cargo:tests/quantities.rs::formats_totals".into(),
        "formatting changes with the task".into(),
    ));
    let b = attribute(&baseline, &completion, &declared);
    assert_eq!(
        b.checks
            .iter()
            .find(|(c, _)| c.ends_with("::formats_totals"))
            .map(|(_, x)| *x),
        Some(Attribution::DeclaredChange)
    );
    assert!(!b.blocks_acceptance, "{b:?}");
}

#[tokio::test]
async fn vitest_fixture_parses_structured_reports_when_vitest_is_installed() {
    // Runs in place: the fixture's pnpm-linked node_modules only resolve from
    // their real path, and the baseline stage does not mutate the tree.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/repos/ts-webapp")
        .canonicalize()
        .unwrap();
    if !root.join("node_modules/vitest").exists() {
        eprintln!("skipped: vitest not installed (pnpm install)");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let plan = derive(&root, &["acceptance rejects negative quantity".into()], &[]);
    let cmd = plan
        .commands
        .iter()
        .find(|c| c.id == "suite:vitest")
        .expect("vitest detected");
    assert_eq!(cmd.family, RunnerFamily::Vitest);
    let sink = MemSink::default();
    let engine = VerificationEngine::new(&ProcessRunner, &sink, VerificationPolicy::default());
    let env = flaky_env(tmp.path());
    let (baseline, quarantines) = engine
        .run_stage(
            &plan,
            "plan-1",
            "run-1",
            Stage::Baseline,
            &root,
            "rev-0",
            &env,
            &["node".into()],
        )
        .await;
    let suite = baseline
        .reports
        .iter()
        .find(|r| r.runner.family == RunnerFamily::Vitest)
        .unwrap();
    assert_eq!(suite.parser.confidence, Confidence::Structured, "{suite:?}");
    let status_of = |name: &str| {
        baseline
            .checks()
            .into_iter()
            .find(|c| c.location.symbol.as_deref() == Some(name))
            .map(|c| c.status)
    };
    assert_eq!(status_of("cart > formats totals"), Some(CheckStatus::Pass));
    assert_eq!(
        status_of("cart > acceptance rejects negative quantity"),
        Some(CheckStatus::Fail)
    );
    assert_eq!(
        status_of("cart > preexisting failing unrelated"),
        Some(CheckStatus::Fail)
    );
    let diagnostics = baseline
        .reports
        .iter()
        .map(|r| {
            format!(
                "{:?} exit={:?} adapter={} argv={:?} checks={:?}",
                r.stage,
                r.exit_code,
                r.parser.adapter,
                r.runner.argv,
                r.checks
                    .iter()
                    .map(|c| (c.check_id.clone(), c.status))
                    .collect::<Vec<_>>()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let raw_tails = sink
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|b| {
            let t = String::from_utf8_lossy(b);
            t.chars()
                .rev()
                .take(700)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n---\n");
    assert_eq!(
        status_of("cart > flaky first run fails"),
        Some(CheckStatus::Flaky),
        "{diagnostics}\nraw tails:\n{raw_tails}"
    );
    assert_eq!(quarantines.len(), 1);
    let acc = baseline
        .checks()
        .into_iter()
        .find(|c| {
            c.location.symbol.as_deref() == Some("cart > acceptance rejects negative quantity")
        })
        .unwrap();
    assert_eq!(acc.location.path.as_deref(), Some("test/cart.test.ts"));
    assert_eq!(acc.error_class.as_deref(), Some("AssertionError"));
    assert_eq!(baseline.failure_signatures().len(), 2);
}

#[test]
fn pytest_junit_and_configured_command_adapters() {
    let xml = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reports/pytest-junit.xml"),
    )
    .unwrap();
    let checks = parse_junit_xml(&xml);
    assert_eq!(checks.len(), 5, "{checks:?}");
    let by = |n: &str| {
        checks
            .iter()
            .find(|c| c.location.symbol.as_deref() == Some(n))
            .unwrap()
    };
    assert_eq!(
        by("test_acceptance_rejects_negative_quantity").status,
        CheckStatus::Fail
    );
    assert_eq!(
        by("test_acceptance_rejects_negative_quantity")
            .location
            .path
            .as_deref(),
        Some("tests/test_orders.py")
    );
    assert_eq!(
        by("test_acceptance_rejects_negative_quantity")
            .location
            .line,
        Some(12)
    );
    assert_eq!(by("test_formats_totals").status, CheckStatus::Pass);
    assert_eq!(by("test_flaky_first_run_fails").status, CheckStatus::Error);
    assert_eq!(
        by("test_parametrized[3-250-7.50]").status,
        CheckStatus::Skip
    );
    assert!(
        by("test_parametrized[3-250-7.50]")
            .check_id
            .ends_with("test_parametrized[param]"),
        "parameters canonicalized"
    );
    // Volatile tokens are removed from fingerprints.
    let flaky = by("test_flaky_first_run_fails");
    assert!(
        !flaky
            .message_fingerprint
            .as_deref()
            .unwrap()
            .contains("0x7f3a1c"),
        "{flaky:?}"
    );
    // Configured command: exit code only, HEURISTIC; a killed process is UNKNOWN (never a pass).
    let mut report = TestReport {
        report_id: "r".into(),
        verification_run_id: "v".into(),
        stage: Stage::Targeted,
        candidate_revision: "c".into(),
        environment_digest: "e".into(),
        runner: RunnerInfo {
            family: RunnerFamily::ConfiguredCommand,
            version: String::new(),
            argv: vec!["make".into(), "check".into()],
            cwd: ".".into(),
        },
        parser: ParserInfo {
            adapter: String::new(),
            adapter_version: "1".into(),
            confidence: Confidence::Heuristic,
        },
        status: ReportStatus::Unknown,
        counts: Counts::default(),
        checks: vec![],
        raw_output_ref: "raw".into(),
        exit_code: None,
    };
    parse(
        RunnerFamily::ConfiguredCommand,
        &RawRun {
            exit_code: Some(2),
            stdout: "boom\n".into(),
            ..Default::default()
        },
        &mut report,
    );
    assert_eq!(
        (
            report.status,
            report.parser.confidence,
            report.checks[0].status
        ),
        (
            ReportStatus::Failed,
            Confidence::Heuristic,
            CheckStatus::Fail
        )
    );
    parse(
        RunnerFamily::ConfiguredCommand,
        &RawRun {
            exit_code: None,
            ..Default::default()
        },
        &mut report,
    );
    assert_eq!(
        (report.status, report.checks[0].status),
        (ReportStatus::Unknown, CheckStatus::Unknown)
    );
    assert!(
        failure_signature(&report.checks[0]).is_none(),
        "UNKNOWN never forms a signature"
    );
    let base = VerificationRun {
        verification_run_id: "b".into(),
        run_id: "r".into(),
        plan_ref: "p".into(),
        stage: Stage::Baseline,
        candidate_revision: "0".into(),
        environment_digest: "e".into(),
        status: ReportStatus::Passed,
        reports: vec![],
        report_refs: vec![],
        duration_ms: 0,
    };
    let comp = VerificationRun {
        reports: vec![report.clone()],
        status: ReportStatus::Unknown,
        stage: Stage::Completion,
        ..base.clone()
    };
    let a = attribute(&base, &comp, &derive(Path::new("/nonexistent"), &[], &[]));
    assert!(
        a.blocks_acceptance && a.inconclusive,
        "UNKNOWN is INDETERMINATE: {a:?}"
    );
}

#[test]
fn diff_invariants_deny_test_weakening_and_flag_the_rest() {
    let ctx = InvariantContext {
        write_set: Some(vec!["src/lib.rs".into()]),
        plan_entries: vec![],
        acceptance_named: vec!["acceptance_rejects_negative_quantity".into()],
        baseline_failing: vec!["preexisting_failing_unrelated".into()],
        protected_paths: vec![".github/".into()],
        formatting_churn_lines: 50,
        expected_revision: Some(3),
    };
    let test_old = "#[test]\nfn acceptance_rejects_negative_quantity() {\n    assert!(parse_quantity(\"-5\").is_err());\n}\n";
    // DI-3 DENY: skip marker on an acceptance-named test.
    let v = evaluate_file(
        &ctx,
        &ChangedFile {
            path: "tests/quantities.rs".into(),
            old: Some(test_old.into()),
            new: Some(format!("#[ignore]\n{test_old}")),
        },
        Some(3),
    );
    assert!(
        v.iter().any(|x| x.id == "DI-3" && x.class == Class::Deny),
        "{v:?}"
    );
    // DI-3 DENY: assertion removed; DI-1 as well since tests are outside the write set.
    let v = evaluate_file(
        &ctx,
        &ChangedFile {
            path: "tests/quantities.rs".into(),
            old: Some(test_old.into()),
            new: Some("#[test]\nfn acceptance_rejects_negative_quantity() {\n}\n".into()),
        },
        Some(3),
    );
    assert!(
        v.iter().any(|x| x.id == "DI-3" && x.class == Class::Deny)
            && v.iter().any(|x| x.id == "DI-1")
    );
    // DI-3 FLAG: an unrelated test file change without a plan entry.
    let ctx_tests = InvariantContext {
        write_set: Some(vec!["tests/**".into()]),
        ..ctx.clone()
    };
    let v = evaluate_file(
        &ctx_tests,
        &ChangedFile {
            path: "tests/other.rs".into(),
            old: Some("fn a() {}\n".into()),
            new: Some("fn a() {}\nfn b() {}\n".into()),
        },
        Some(3),
    );
    assert_eq!(
        v.iter()
            .map(|x| (x.id.as_str(), x.class))
            .collect::<Vec<_>>(),
        vec![("DI-3", Class::Flag)]
    );
    // DI-2 / DI-7 DENY without plan entries; allowed with them.
    let v = evaluate_diff(
        &ctx,
        &[
            ChangedFile {
                path: "Cargo.lock".into(),
                old: Some("a".into()),
                new: Some("b".into()),
            },
            ChangedFile {
                path: "Cargo.toml".into(),
                old: Some("a".into()),
                new: Some("b".into()),
            },
        ],
        Some(3),
    );
    assert!(v.iter().any(|x| x.id == "DI-2") && v.iter().any(|x| x.id == "DI-7"));
    let planned = InvariantContext {
        plan_entries: vec!["Cargo.lock".into(), "Cargo.toml".into()],
        write_set: Some(vec!["Cargo.toml".into(), "Cargo.lock".into()]),
        ..ctx.clone()
    };
    assert!(
        evaluate_diff(
            &planned,
            &[ChangedFile {
                path: "Cargo.lock".into(),
                old: Some("a".into()),
                new: Some("b".into())
            }],
            Some(3)
        )
        .is_empty()
    );
    // DI-4 FLAG, DI-5 DENY, DI-8 DENY, DI-9 DENY.
    let v = evaluate_file(
        &ctx,
        &ChangedFile {
            path: "src/lib.rs".into(),
            old: Some("fn f() {}\n".into()),
            new: Some(
                "fn f() { dbg!(1); }\nconst K: &str = \"sk-live-abcdefghijklmnopqrstuvwxyz\";\n"
                    .into(),
            ),
        },
        Some(3),
    );
    assert!(
        v.iter().any(|x| x.id == "DI-4" && x.class == Class::Flag)
            && v.iter().any(|x| x.id == "DI-5" && x.class == Class::Deny),
        "{v:?}"
    );
    let v = evaluate_file(
        &ctx,
        &ChangedFile {
            path: "src/lib.rs".into(),
            old: Some("a".into()),
            new: Some("b".into()),
        },
        Some(4),
    );
    assert!(v.iter().any(|x| x.id == "DI-8"));
    let v = evaluate_file(
        &ctx,
        &ChangedFile {
            path: ".github/workflows/ci.yml".into(),
            old: Some("a".into()),
            new: Some("b".into()),
        },
        Some(3),
    );
    assert!(v.iter().any(|x| x.id == "DI-9" && x.class == Class::Deny));
    assert!(denies(&v));
}

/// QUAL-EV-0069 (EXPERIMENT, off by default): the verification engine has no
/// model port at all, so a disabled semantic verifier emits zero model calls
/// by construction; deterministic gates decide.
#[test]
fn qual_ev_0069_disabled_semantic_verification_makes_zero_model_calls() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("modbit-providers"),
        "the verification crate must not depend on the provider gateway"
    );
    let src = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect::<String>();
    assert!(
        !src.contains("ModelRequest") && !src.contains("ProviderGateway"),
        "no semantic verifier path exists"
    );
}

/// QUAL-EV-0068: a verifier that crashes (killed by a signal, or never
/// starts) yields an INDETERMINATE completion that blocks acceptance; it is
/// never mapped to a pass.
#[tokio::test]
async fn qual_ev_0068_verifier_crash_is_indeterminate_never_success() {
    let (tmp, root) = fixture_copy("rust-cli");
    let sink = MemSink::default();
    let engine = VerificationEngine::new(&ProcessRunner, &sink, VerificationPolicy::default());
    let env = flaky_env(tmp.path());
    let crash = if cfg!(unix) {
        vec!["sh".to_owned(), "-c".to_owned(), "kill -9 $$".to_owned()]
    } else {
        vec![
            root.join("no-such-verifier.exe")
                .to_string_lossy()
                .into_owned(),
        ]
    };
    let mut plan = derive(&root, &[], &[]);
    plan.commands = vec![CheckCommand {
        id: "suite:crashing".into(),
        family: RunnerFamily::ConfiguredCommand,
        argv: crash,
        mandatory: true,
        reporter_file: None,
    }];
    let (baseline, _) = engine
        .run_stage(
            &plan,
            "plan-c",
            "run-c",
            Stage::Baseline,
            &root,
            "rev-0",
            &env,
            &[],
        )
        .await;
    let (completion, _) = engine
        .run_stage(
            &plan,
            "plan-c",
            "run-c",
            Stage::Completion,
            &root,
            "rev-1",
            &env,
            &[],
        )
        .await;
    assert_ne!(completion.status, ReportStatus::Passed, "{completion:?}");
    assert!(
        completion
            .reports
            .iter()
            .all(|r| r.status != ReportStatus::Passed
                && r.checks.iter().all(|c| c.status != CheckStatus::Pass)),
        "{completion:?}"
    );
    let a = attribute(&baseline, &completion, &derive(&root, &[], &[]));
    assert!(
        a.blocks_acceptance && a.inconclusive,
        "crash is INDETERMINATE and blocks acceptance: {a:?}"
    );
}

/// QUAL-EV-0071: a credential seeded into a patch is denied (DI-5) with
/// evidence naming the file, whatever the plan says; the runtime refuses the
/// transaction on any DENY (see the M2.8 Core test for the TRANSACTION stage).
#[test]
fn qual_ev_0071_secret_in_patch_is_denied_with_evidence() {
    let ctx = InvariantContext {
        write_set: Some(vec!["src/config.rs".into()]),
        plan_entries: vec!["src/config.rs".into()],
        acceptance_named: vec![],
        baseline_failing: vec![],
        protected_paths: vec![],
        formatting_churn_lines: 50,
        expected_revision: None,
    };
    let v = evaluate_diff(
        &ctx,
        &[ChangedFile {
            path: "src/config.rs".into(),
            old: Some("pub const KEY: &str = \"\";\n".into()),
            new: Some("pub const KEY: &str = \"sk-live-0123456789abcdefghijklmnop\";\n".into()),
        }],
        None,
    );
    let secret = v
        .iter()
        .find(|x| x.id == "DI-5")
        .unwrap_or_else(|| panic!("{v:?}"));
    assert_eq!(secret.class, Class::Deny);
    assert_eq!(secret.paths, vec!["src/config.rs".to_owned()]);
    assert!(
        secret.evidence.starts_with("credential-like token"),
        "{}",
        secret.evidence
    );
    assert!(
        !secret.evidence.contains("0123456789abcdefghijklmnop"),
        "evidence never echoes the secret"
    );
    // The same edit without the credential passes every invariant.
    assert!(
        evaluate_diff(
            &ctx,
            &[ChangedFile {
                path: "src/config.rs".into(),
                old: Some("pub const KEY: &str = \"\";\n".into()),
                new: Some("pub const KEY: &str = \"from-env\";\n".into()),
            }],
            None,
        )
        .is_empty()
    );
}
