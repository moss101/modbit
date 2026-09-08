//! Real-effect tests for the module registration check (M0.4): disposable
//! Cargo workspaces with real `cargo metadata`, a small graph file and a
//! Decision Record on disk.

use std::fs;
use std::path::{Path, PathBuf};

use architecture_lint::Rules;
use architecture_lint::modules::{GraphFacts, check_modules, collect};

const RULES: &str = r#"
[locked_policy]
trailer = "Decision-Record"
decisions_dir = "docs/decisions"
[[canonical]]
id = "event-state"
description = "canonical event state"
"#;

const GRAPH: &str = r#"{"nodes":[
  {"id":"domain-events","type":"subsystem"},
  {"id":"REQ-EV-0001","type":"requirement"},
  {"id":"IMP-EV-0001","type":"imp_task"}
]}"#;

fn write(p: &Path, text: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

/// Workspace with crates `a` and `b`; each gets the given metadata block.
fn workspace(a_meta: &str, b_meta: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(
        &r.join("Cargo.toml"),
        "[workspace]\nresolver = \"3\"\nmembers = [\"a\", \"b\"]\n",
    );
    for (name, meta) in [("a", a_meta), ("b", b_meta)] {
        write(
            &r.join(name).join("Cargo.toml"),
            &format!(
                "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n{meta}"
            ),
        );
        write(&r.join(name).join("src/lib.rs"), "");
    }
    write(&r.join("graph/project-graph.json"), GRAPH);
    write(
        &r.join("docs/decisions/DR-1.md"),
        "---\nstatus: accepted\n---\n",
    );
    write(
        &r.join("docs/decisions/DR-2.md"),
        "---\nstatus: proposed\n---\n",
    );
    let manifest = r.join("Cargo.toml");
    (dir, manifest)
}

fn run(root: &Path, manifest: &Path, ts: &[PathBuf]) -> Vec<String> {
    let rules = Rules::parse(RULES).unwrap();
    let facts = GraphFacts::load(&root.join("graph/project-graph.json")).unwrap();
    let modules = collect(manifest, ts).unwrap();
    check_modules(root, &modules, &facts, &rules)
        .iter()
        .map(ToString::to_string)
        .collect()
}

const OK_A: &str = "[package.metadata.modbit]\nowner = \"domain-events\"\ncanonical = [\"event-state\"]\ncapabilities = [{ id = \"events.append\", requirements = [\"REQ-EV-0001\"] }]\n";
const OK_B: &str = "[package.metadata.modbit]\nowner = \"domain-events\"\n";

#[test]
fn valid_registrations_pass() {
    let (dir, manifest) = workspace(OK_A, OK_B);
    assert!(run(dir.path(), &manifest, &[]).is_empty());
}

#[test]
fn missing_registration_unknown_owner_and_bad_capability_are_rejected() {
    let bad_b = "[package.metadata.modbit]\nowner = \"nope\"\ncapabilities = [{ id = \"x\" }, { id = \"y\", requirements = [\"REQ-EV-9999\"] }]\n";
    let (dir, manifest) = workspace("", bad_b);
    let v = run(dir.path(), &manifest, &[]);
    assert_eq!(v.len(), 4, "{v:?}");
    assert!(v[0].contains("MODULE a : no modbit registration"));
    assert!(v[1].contains("owner `nope` is not a subsystem"));
    assert!(v[2].contains("capability `x` registers no requirement id"));
    assert!(v[3].contains("unknown requirement `REQ-EV-9999`"));
}

#[test]
fn duplicate_canonical_owner_is_rejected_unless_migration_adr_is_accepted() {
    let claim_b =
        "[package.metadata.modbit]\nowner = \"domain-events\"\ncanonical = [\"event-state\"]\n";
    let (dir, manifest) = workspace(OK_A, claim_b);
    let v = run(dir.path(), &manifest, &[]);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(
        v[0].contains(
            "`event-state` has 2 active implementations without an accepted migration ADR"
        )
    );

    let migrating = "[package.metadata.modbit]\nowner = \"domain-events\"\ncanonical = [\"event-state\"]\nmigration_adr = \"docs/decisions/DR-1.md\"\n";
    let (dir, manifest) = workspace(OK_A, migrating);
    assert!(run(dir.path(), &manifest, &[]).is_empty());

    let proposed = "[package.metadata.modbit]\nowner = \"domain-events\"\ncanonical = [\"event-state\"]\nmigration_adr = \"docs/decisions/DR-2.md\"\n";
    let (dir, manifest) = workspace(OK_A, proposed);
    let v = run(dir.path(), &manifest, &[]);
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(v[0].contains("not an accepted Decision Record"));

    let unknown =
        "[package.metadata.modbit]\nowner = \"domain-events\"\ncanonical = [\"not-a-system\"]\n";
    let (dir, manifest) = workspace(OK_A, unknown);
    let v = run(dir.path(), &manifest, &[]);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("not declared in rules.toml"));
}

#[test]
fn typescript_packages_are_checked_too() {
    let (dir, manifest) = workspace(OK_A, OK_B);
    let pkg = dir.path().join("packages/ui/package.json");
    write(
        &pkg,
        r#"{"name":"@x/ui","modbit":{"owner":"desktop-typo","canonical":["event-state"]}}"#,
    );
    let v = run(dir.path(), &manifest, std::slice::from_ref(&pkg));
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(v[0].contains("MODULE @x/ui : owner `desktop-typo`"));
    assert!(
        v[1].contains("@x/ui") && v[1].contains("MODULE") && v[1].contains("`event-state` has 2"),
        "{}",
        v[1]
    );
    write(
        &dir.path().join("packages/ok/package.json"),
        r#"{"name":"@x/ok","modbit":{"owner":"domain-events"}}"#,
    );
    assert!(
        run(
            dir.path(),
            &manifest,
            &[dir.path().join("packages/ok/package.json")]
        )
        .is_empty()
    );
}

#[test]
fn modbit_workspace_registrations_are_valid() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let rules = Rules::load(&root.join("tools/architecture-lint/rules.toml")).unwrap();
    let facts = GraphFacts::load(&root.join("graph/project-graph.json")).unwrap();
    let mut ts: Vec<PathBuf> = fs::read_dir(root.join("packages"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("package.json"))
        .filter(|p| p.exists())
        .collect();
    ts.push(root.join("apps/desktop/package.json"));
    let modules = collect(&root.join("Cargo.toml"), &ts).unwrap();
    assert!(modules.len() >= 40, "{}", modules.len());
    let v = check_modules(&root, &modules, &facts, &rules);
    assert!(
        v.is_empty(),
        "{}",
        v.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
