//! WIN-05: the remaining git-on-Windows conformance the handoff names —
//! `core.autocrlf` content fidelity, case-colliding names, and worktree
//! cleanup with a file another process holds. The hook/fsmonitor/credential
//! surface is WIN-02's git group (hardening.rs, px_067_remaining.rs).

use std::path::Path;

use modbit_git::Repo;

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

fn plain_ok(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The repository's own `core.autocrlf` never rewrites what the model
/// writes: a CRLF file committed through the typed runner must be stored
/// byte-exact. With a live repo-controlled autocrlf the clean filter would
/// store LF while the worktree holds CRLF, and every digest, diff and patch
/// round-trip would quietly disagree with the bytes on disk (WIN-05).
#[test]
fn a_repository_autocrlf_never_rewrites_what_the_runner_commits() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::init(&dir.path().join("r"), "main").unwrap();
    write(&repo.dir().join("base.txt"), "base\n");
    repo.commit("base", &["."]).unwrap();
    // The hostile repository turns line-ending rewriting on in its own
    // config (this is repo-controlled state, not the user's).
    plain_ok(repo.dir(), &["config", "core.autocrlf", "true"]);
    plain_ok(repo.dir(), &["config", "core.eol", "crlf"]);
    // A task worktree writes CRLF and the typed runner commits it.
    let base = repo.head().unwrap();
    repo.create_branch("task/crlf", &base).unwrap();
    let wt = repo
        .worktree_add(&dir.path().join("wt"), "task/crlf")
        .unwrap();
    write(&wt.dir().join("written.txt"), "line one\r\nline two\r\n");
    wt.commit("crlf", &["."]).unwrap();
    // The stored blob is byte-exact what was written.
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo.dir())
        .args(["cat-file", "-p", "task/crlf:written.txt"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "line one\r\nline two\r\n",
        "the repository's line-ending config rewrote the stored bytes"
    );
}

/// On a case-insensitive filesystem a branch whose name collides with an
/// existing one by case is refused by git; the refusal must surface typed,
/// not as a silent second branch or a corrupted ref (WIN-05).
#[cfg(windows)]
#[test]
fn case_colliding_branch_names_are_refused_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::init(&dir.path().join("r"), "main").unwrap();
    write(&repo.dir().join("a.txt"), "a\n");
    repo.commit("base", &["."]).unwrap();
    let base = repo.head().unwrap();
    repo.create_branch("Feature", &base).unwrap();
    let err = repo.create_branch("feature", &base).unwrap_err();
    assert!(
        matches!(
            err,
            modbit_git::Error::Git { .. } | modbit_git::Error::InvalidRef { .. }
        ),
        "typed refusal, got {err:?}"
    );
    // The existing branch is untouched.
    assert!(repo.rev_parse("Feature").is_ok());
}

/// A candidate that adds a file whose name collides by case with a file in
/// the checkout must be refused by the apply plan, never silently applied
/// over the existing file (Windows filesystems treat the two names as one
/// file; WIN-05).
#[cfg(windows)]
#[test]
fn a_case_colliding_add_is_refused_and_nothing_is_half_applied() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = Repo::init(&dir.path().join("checkout"), "main").unwrap();
    write(&checkout.dir().join("file.txt"), "checkout content\n");
    checkout.commit("base", &["."]).unwrap();
    let base = checkout.head().unwrap();
    checkout.create_branch("task/case", &base).unwrap();
    let wt = checkout
        .worktree_add(&dir.path().join("wt"), "task/case")
        .unwrap();
    // The candidate TREE must carry FILE.TXT without any on-disk case
    // collision in the task worktree (writing FILE.TXT there would simply
    // overwrite file.txt: one file, two spellings), so the differently-cased
    // name is staged straight into the index from a hashed blob.
    let blob = {
        let mut child = std::process::Command::new("git")
            .arg("-C")
            .arg(checkout.dir())
            .args(["hash-object", "-w", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"case-colliding content\n")
            .unwrap();
        String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout)
            .trim()
            .to_owned()
    };
    plain_ok(
        wt.dir(),
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},FILE.TXT"),
        ],
    );
    plain_ok(wt.dir(), &["write-tree"]);
    // The plan over the candidate tree must refuse the colliding add.
    let tree = {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(wt.dir())
            .args(["write-tree"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    let plan = modbit_git::apply::plan(
        &checkout,
        Some(&base),
        &tree,
        modbit_git::apply::View::Disk,
        &|_| false,
    )
    .unwrap();
    let colliding = plan
        .entries
        .iter()
        .find(|e| e.path.eq_ignore_ascii_case("file.txt") && e.path != "file.txt")
        .expect("the plan sees the case-colliding add");
    assert!(
        matches!(
            colliding.state,
            modbit_git::apply::EntryState::Unsupported
                | modbit_git::apply::EntryState::Conflict { .. }
        ),
        "a case-colliding add must be refused by state, got {:?}",
        colliding.state
    );
}

/// Cleanup of a worktree with a file another process holds open must fail
/// typed, keep the worktree registered (retained evidence), and succeed once
/// the lock is gone (WIN-05: no half-deleted worktree).
#[cfg(windows)]
#[test]
fn a_locked_worktree_fails_cleanup_typed_and_retains_the_worktree() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::init(&dir.path().join("r"), "main").unwrap();
    write(
        &repo.dir().join("a.txt"),
        "a
",
    );
    repo.commit("base", &["."]).unwrap();
    let base = repo.head().unwrap();
    repo.create_branch("task/lock", &base).unwrap();
    let wt = repo
        .worktree_add(&dir.path().join("wt"), "task/lock")
        .unwrap();
    // An exclusive handle (no share-delete): nothing can remove the file.
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(wt.dir().join("a.txt"))
        .unwrap();
    let wt_path = dir.path().join("wt");
    let err = repo.worktree_remove(&wt_path).unwrap_err();
    assert!(
        matches!(err, modbit_git::Error::WorktreeHeld(_)),
        "typed refusal naming the held worktree, got {err:?}"
    );
    // The residue itself is the retained evidence: the locked file (and the
    // directory around it) survives, and a best-effort re-registration keeps
    // the worktree listed for the retry.
    assert!(
        wt_path.join("a.txt").exists(),
        "the locked file is retained"
    );
    // git's porcelain output may spell the same directory with an 8.3 name
    // (a CI user like `runneradmin` becomes `RUNNER~1`), so the comparison
    // goes through canonicalization, not exact PathBuf equality.
    let listed = repo.worktree_list().unwrap().into_iter().any(|w| {
        let listed = std::fs::canonicalize(&w.path).unwrap_or_else(|_| w.path.clone());
        let wanted = std::fs::canonicalize(&wt_path).unwrap_or_else(|_| wt_path.clone());
        listed == wanted
    });
    assert!(listed, "the residue stays registered for the retry");
    drop(held);
    repo.worktree_remove(&wt_path)
        .expect("cleanup succeeds once the lock is gone");
    assert!(!wt_path.exists(), "the retry removed everything");
}
