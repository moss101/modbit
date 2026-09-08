//! Real-effect tests: the lint runs actual `cargo metadata` on (a) the Modbit
//! workspace with the shipped rules and (b) a disposable workspace that
//! contains an injected forbidden edge (M0.1 acceptance: "forbidden dependency
//! test works").

use std::fs;
use std::path::{Path, PathBuf};

use architecture_lint::{Graph, Rules, check};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn scratch_workspace(edge_a_to_b: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join("Cargo.toml"),
        "[workspace]\nresolver = \"3\"\nmembers = [\"a\", \"b\"]\n",
    );
    let a_deps = if edge_a_to_b {
        "b = { path = \"../b\" }\n"
    } else {
        ""
    };
    write(
        &root.join("a/Cargo.toml"),
        &format!(
            "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n{a_deps}"
        ),
    );
    write(&root.join("a/src/lib.rs"), "");
    write(
        &root.join("b/Cargo.toml"),
        "[package]\nname = \"b\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    write(&root.join("b/src/lib.rs"), "");
    dir
}

const SCRATCH_RULES: &str =
    "[[forbid]]\nfrom = \"a\"\nto = \"b\"\nreason = \"test: a -> b forbidden\"\n";

#[test]
fn modbit_workspace_respects_shipped_rules() {
    let root = repo_root();
    let rules = Rules::load(&root.join("tools/architecture-lint/rules.toml")).unwrap();
    assert!(!rules.forbid.is_empty(), "shipped rules must not be empty");
    let graph = Graph::from_cargo_metadata(&root.join("Cargo.toml"), false).unwrap();
    assert!(graph.members.contains("modbit-domain"));
    assert!(graph.members.contains("architecture-lint"));
    let violations = check(&graph, &rules);
    assert!(
        violations.is_empty(),
        "workspace violates architecture rules:\n{}",
        violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn injected_forbidden_edge_is_rejected_by_real_cargo_metadata() {
    let ws = scratch_workspace(true);
    let rules = Rules::parse(SCRATCH_RULES).unwrap();
    let graph = Graph::from_cargo_metadata(&ws.path().join("Cargo.toml"), false).unwrap();
    let violations = check(&graph, &rules);
    assert_eq!(
        violations.len(),
        1,
        "expected exactly one violation: {violations:?}"
    );
    assert_eq!(violations[0].path, vec!["a", "b"]);
    assert_eq!(violations[0].reason, "test: a -> b forbidden");
}

#[test]
fn workspace_without_the_edge_passes_the_same_rules() {
    let ws = scratch_workspace(false);
    let rules = Rules::parse(SCRATCH_RULES).unwrap();
    let graph = Graph::from_cargo_metadata(&ws.path().join("Cargo.toml"), false).unwrap();
    assert!(check(&graph, &rules).is_empty());
}

#[test]
fn cli_binary_exits_nonzero_on_violation_and_zero_on_pass() {
    let bin = env!("CARGO_BIN_EXE_architecture-lint");
    let ws = scratch_workspace(true);
    let rules_path = ws.path().join("rules.toml");
    fs::write(&rules_path, SCRATCH_RULES).unwrap();
    let out = std::process::Command::new(bin)
        .args(["--manifest-path"])
        .arg(ws.path().join("Cargo.toml"))
        .arg("--rules")
        .arg(&rules_path)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("FORBIDDEN a -> b via a -> b"));

    let ws_ok = scratch_workspace(false);
    let out = std::process::Command::new(bin)
        .args(["--manifest-path"])
        .arg(ws_ok.path().join("Cargo.toml"))
        .arg("--rules")
        .arg(&rules_path)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));

    let out = std::process::Command::new(bin)
        .args(["--rules", "/nonexistent/rules.toml"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
