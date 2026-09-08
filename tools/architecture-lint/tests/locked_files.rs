//! Real-effect tests for the locked-path guard (DR-M0-002): disposable Git
//! repositories with real commits and trailers.

use std::fs;
use std::path::Path;
use std::process::Command;

use architecture_lint::Rules;
use architecture_lint::locked::check_locked;

const RULES: &str = r#"
[locked_policy]
trailer = "Decision-Record"
decisions_dir = "docs/decisions"

[[locked]]
path = "docs/11_*.md"
reason = "test: architecture doc is locked"

[[locked]]
path = "docs/decisions/DR-*.md"
reason = "test: accepted records are immutable"
"#;

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn write(repo: &Path, rel: &str, text: &str) {
    let p = repo.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn commit(repo: &Path, msg: &str) -> String {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", msg]);
    git(repo, &["rev-parse", "HEAD"])
}

/// Repo with an accepted and a proposed record plus a locked doc; returns base sha.
fn seed() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    git(r, &["init", "-q", "-b", "main"]);
    write(r, "docs/11_SYSTEM_ARCHITECTURE.md", "v1\n");
    write(
        r,
        "docs/decisions/DR-1.md",
        "---\nid: DR-1\nstatus: accepted\n---\n",
    );
    write(
        r,
        "docs/decisions/DR-2.md",
        "---\nid: DR-2\nstatus: proposed\n---\n",
    );
    write(r, "README.md", "hello\n");
    let base = commit(r, "seed\n\nDecision-Record: docs/decisions/DR-1.md");
    (dir, base)
}

fn run(repo: &Path, base: &str) -> Vec<String> {
    let rules = Rules::parse(RULES).unwrap();
    check_locked(repo, Some(base), "HEAD", &rules)
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn locked_change_without_trailer_is_rejected() {
    let (dir, base) = seed();
    write(dir.path(), "docs/11_SYSTEM_ARCHITECTURE.md", "v2\n");
    commit(dir.path(), "silently change architecture");
    let v = run(dir.path(), &base);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].contains("docs/11_SYSTEM_ARCHITECTURE.md"));
    assert!(v[0].contains("no `Decision-Record:` trailer"));
}

#[test]
fn locked_change_with_accepted_record_passes() {
    let (dir, base) = seed();
    write(dir.path(), "docs/11_SYSTEM_ARCHITECTURE.md", "v2\n");
    commit(
        dir.path(),
        "change architecture\n\nDecision-Record: docs/decisions/DR-1.md",
    );
    assert!(run(dir.path(), &base).is_empty());
}

#[test]
fn record_added_in_the_same_commit_counts() {
    let (dir, base) = seed();
    write(dir.path(), "docs/11_SYSTEM_ARCHITECTURE.md", "v3\n");
    write(
        dir.path(),
        "docs/decisions/DR-3.md",
        "---\nstatus: accepted\n---\n",
    );
    commit(
        dir.path(),
        "change with new record\n\nDecision-Record: docs/decisions/DR-3.md",
    );
    assert!(run(dir.path(), &base).is_empty());
}

#[test]
fn missing_or_unaccepted_record_is_rejected() {
    let (dir, base) = seed();
    write(dir.path(), "docs/11_SYSTEM_ARCHITECTURE.md", "v2\n");
    commit(dir.path(), "a\n\nDecision-Record: docs/decisions/DR-404.md");
    write(dir.path(), "docs/11_SYSTEM_ARCHITECTURE.md", "v3\n");
    commit(dir.path(), "b\n\nDecision-Record: docs/decisions/DR-2.md");
    write(dir.path(), "docs/11_SYSTEM_ARCHITECTURE.md", "v4\n");
    commit(dir.path(), "c\n\nDecision-Record: README.md");
    let v = run(dir.path(), &base);
    assert_eq!(v.len(), 3, "{v:?}");
    assert!(v[0].contains("does not exist"));
    assert!(v[1].contains("not an accepted Decision Record"));
    assert!(v[2].contains("does not point under docs/decisions"));
}

#[test]
fn unlocked_change_needs_no_trailer_and_range_is_respected() {
    let (dir, base) = seed();
    write(dir.path(), "README.md", "changed\n");
    commit(dir.path(), "docs tweak");
    assert!(run(dir.path(), &base).is_empty());
    // A bad commit before `base` is outside the checked range.
    write(dir.path(), "docs/11_SYSTEM_ARCHITECTURE.md", "v2\n");
    let bad = commit(dir.path(), "bad");
    write(dir.path(), "README.md", "again\n");
    commit(dir.path(), "after bad");
    assert_eq!(run(dir.path(), &base).len(), 1);
    assert!(run(dir.path(), &bad).is_empty());
}

#[test]
fn cli_exit_codes() {
    let bin = env!("CARGO_BIN_EXE_architecture-lint");
    let (dir, base) = seed();
    let rules = dir.path().join("rules.toml");
    fs::write(&rules, RULES).unwrap();
    write(dir.path(), "docs/11_SYSTEM_ARCHITECTURE.md", "v2\n");
    commit(dir.path(), "silent");
    let out = Command::new(bin)
        .args(["locked", "--repo"])
        .arg(dir.path())
        .args(["--base", &base, "--rules"])
        .arg(&rules)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("LOCKED"));
    let out = Command::new(bin)
        .args(["locked", "--repo"])
        .arg(dir.path())
        .args(["--base", &base, "--head", &base, "--rules"])
        .arg(&rules)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let out = Command::new(bin)
        .args(["locked", "--repo", "/nonexistent-repo", "--rules"])
        .arg(&rules)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
