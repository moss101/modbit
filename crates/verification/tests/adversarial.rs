//! The boundary check of the generated adversarial checks (PX-135) against a
//! real process: a real `node --test` run in a real tree, a mutant written
//! to a copy, the candidate's own tree left byte for byte as it was.

use std::path::Path;

use modbit_verification::adversarial::{
    Class, MutationOptions, Status, boundary_check, boundary_mutants,
};
use modbit_verification::engine::BoxFuture;
use modbit_verification::{ChangedFile, CommandRunner, RawRun};

struct Local;

impl CommandRunner for Local {
    fn run<'a>(
        &'a self,
        argv: &'a [String],
        cwd: &'a Path,
        env: &'a [(String, String)],
        _timeout_ms: u64,
    ) -> BoxFuture<'a, RawRun> {
        Box::pin(async move {
            let (argv, cwd, env) = (argv.to_vec(), cwd.to_path_buf(), env.to_vec());
            tokio::task::spawn_blocking(move || {
                match std::process::Command::new(&argv[0])
                    .args(&argv[1..])
                    .current_dir(&cwd)
                    .envs(env)
                    .output()
                {
                    Ok(o) => RawRun {
                        exit_code: o.status.code(),
                        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                        reporter_file: None,
                        timed_out: false,
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

fn node_ok() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

const SRC_OLD: &str = "export function f(q) {\n  return q;\n}\n";
const SRC: &str =
    "export function f(q) {\n  if (q > 100) throw new RangeError(\"too big\");\n  return q;\n}\n";
const TEST_LOOSE: &str = "import test from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport { f } from \"../src/shop.mjs\";\n\ntest(\"rejects a lot\", () => {\n  assert.throws(() => f(500), RangeError);\n  assert.equal(f(10), 10);\n});\n";
const TEST_PINNED: &str = "import test from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport { f } from \"../src/shop.mjs\";\n\ntest(\"rejects a lot\", () => {\n  assert.throws(() => f(500), RangeError);\n  assert.equal(f(10), 10);\n});\n\ntest(\"pins the boundary\", () => {\n  assert.equal(f(100), 100);\n  assert.throws(() => f(101), RangeError);\n});\n";

fn tree(test: &str, src: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("src")).unwrap();
    std::fs::create_dir_all(d.path().join("test")).unwrap();
    std::fs::write(d.path().join("src/shop.mjs"), src).unwrap();
    std::fs::write(d.path().join("test/shop.test.mjs"), test).unwrap();
    d
}

fn changed() -> Vec<ChangedFile> {
    vec![ChangedFile {
        path: "src/shop.mjs".into(),
        old: Some(SRC_OLD.into()),
        new: Some(SRC.into()),
    }]
}

fn cmd() -> Vec<Vec<String>> {
    vec![vec![
        "node".into(),
        "--test".into(),
        "test/shop.test.mjs".into(),
    ]]
}

#[tokio::test]
async fn a_boundary_the_tests_pin_survives_no_mutant_and_one_they_do_not_is_found() {
    if !node_ok() {
        eprintln!("node is not installed: boundary_check not run");
        return;
    }
    assert_eq!(boundary_mutants(&changed(), 12).len(), 3);
    let scratch = tempfile::tempdir().unwrap();
    let opts = MutationOptions::default();

    let pinned = tree(TEST_PINNED, SRC);
    let before = std::fs::read(pinned.path().join("src/shop.mjs")).unwrap();
    let c = boundary_check(
        &Local,
        &cmd(),
        pinned.path(),
        &scratch.path().join("m"),
        &[],
        &changed(),
        &opts,
    )
    .await;
    assert_eq!(
        (c.class, c.status),
        (Class::BoundaryNotPinned, Status::Pass),
        "{c:?}"
    );
    // The candidate's tree is not the mutants' playground.
    assert_eq!(
        std::fs::read(pinned.path().join("src/shop.mjs")).unwrap(),
        before
    );

    let loose = tree(TEST_LOOSE, SRC);
    let c = boundary_check(
        &Local,
        &cmd(),
        loose.path(),
        &scratch.path().join("m"),
        &[],
        &changed(),
        &opts,
    )
    .await;
    assert_eq!(c.status, Status::Fail, "{c:?}");
    assert_eq!(c.findings.len(), 3, "{c:?}");
    assert!(
        c.findings
            .iter()
            .all(|f| f.starts_with("src/shop.mjs:2: mutant"))
    );
    assert_eq!(c.to_evidence().status, "FAIL");

    // A test that reaches the boundary on one side only (101 must throw)
    // kills one of its mutants: the boundary is exercised, not a finding.
    let one_side = tree(&TEST_LOOSE.replace("f(500)", "f(101)"), SRC);
    let c = boundary_check(
        &Local,
        &cmd(),
        one_side.path(),
        &scratch.path().join("m"),
        &[],
        &changed(),
        &opts,
    )
    .await;
    assert_eq!(c.status, Status::Pass, "{c:?}");

    // A candidate whose own checks are red cannot be told from its mutants.
    let red = tree(&TEST_LOOSE.replace("f(10), 10", "f(10), 11"), SRC);
    let c = boundary_check(
        &Local,
        &cmd(),
        red.path(),
        &scratch.path().join("m"),
        &[],
        &changed(),
        &opts,
    )
    .await;
    assert_eq!(c.status, Status::Skip, "{c:?}");

    // No changed comparison: nothing to mutate, nothing found.
    let none = vec![ChangedFile {
        path: "src/shop.mjs".into(),
        old: Some(SRC.into()),
        new: Some(SRC.into()),
    }];
    let c = boundary_check(
        &Local,
        &cmd(),
        loose.path(),
        &scratch.path().join("m"),
        &[],
        &none,
        &opts,
    )
    .await;
    assert_eq!(c.status, Status::Pass);
}
