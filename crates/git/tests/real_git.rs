//! Real-repository tests for typed Git operations (docs/20 "Git strategy"):
//! isolated worktrees, typed status/diff/commit, merge transactions with
//! injected conflicts, and provenance-bound dirty-state snapshots.

use std::path::Path;

use modbit_git::{MergeState, Repo};

/// Read text with line endings normalized (a user's global autocrlf must not matter).
fn read_lf(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap().replace("\r\n", "\n")
}

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

fn seed() -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::init(&dir.path().join("repo"), "main").unwrap();
    write(&repo.dir().join("src/lib.rs"), "pub fn a() {}\n");
    write(&repo.dir().join("README.md"), "# demo\n");
    repo.commit("base", &["."]).unwrap();
    (dir, repo)
}

#[test]
fn qual_ev_0125_0124_worktrees_isolate_parallel_writes_and_carry_lineage() {
    let (dir, repo) = seed();
    let base = repo.head().unwrap();
    repo.create_branch("task/a", &base).unwrap();
    repo.create_branch("task/b", &base).unwrap();
    let a = repo
        .worktree_add(&dir.path().join("wt-a"), "task/a")
        .unwrap();
    let b = repo
        .worktree_add(&dir.path().join("wt-b"), "task/b")
        .unwrap();
    assert_eq!(a.current_branch().unwrap().as_deref(), Some("task/a"));
    assert_eq!(b.current_branch().unwrap().as_deref(), Some("task/b"));
    // Parallel writes in both worktrees; neither sees the other's change.
    write(&a.dir().join("src/a_only.rs"), "// a\n");
    write(&b.dir().join("src/b_only.rs"), "// b\n");
    assert!(!a.dir().join("src/b_only.rs").exists());
    assert!(!b.dir().join("src/a_only.rs").exists());
    assert!(
        !repo.dir().join("src/a_only.rs").exists(),
        "the user's main worktree is untouched (docs/20)"
    );
    assert_eq!(
        a.status()
            .unwrap()
            .iter()
            .map(|e| (e.path.as_str(), e.code.as_str()))
            .collect::<Vec<_>>(),
        [("src/a_only.rs", "??")]
    );
    let ca = a.commit("a change", &["."]).unwrap();
    let cb = b.commit("b change", &["."]).unwrap();
    assert_ne!(ca, cb);
    // Lineage: both descend from the base.
    assert_eq!(repo.merge_base(&ca, &cb).unwrap(), base);
    let list = repo.worktree_list().unwrap();
    assert_eq!(list.len(), 3);
    assert!(
        list.iter()
            .any(|w| w.branch.as_deref() == Some("task/a") && w.head == ca)
    );
    // Typed diff of a's branch against base.
    let d = repo.diff(&base, &ca).unwrap();
    assert_eq!(d.files.len(), 1);
    assert_eq!(
        (
            d.files[0].path.as_str(),
            d.files[0].additions,
            d.files[0].deletions
        ),
        ("src/a_only.rs", Some(1), Some(0))
    );
    assert!(d.unified.contains("+// a"));
    repo.worktree_remove(a.dir()).unwrap();
    assert_eq!(repo.worktree_list().unwrap().len(), 2);
    assert!(!a.dir().exists());
}

#[test]
fn qual_ev_0067_merge_transaction_exposes_injected_conflict_and_is_recoverable() {
    let (dir, repo) = seed();
    let base = repo.head().unwrap();
    repo.create_branch("feature", &base).unwrap();
    let f = repo
        .worktree_add(&dir.path().join("wt-f"), "feature")
        .unwrap();
    write(&f.dir().join("README.md"), "# feature\n");
    let feature = f.commit("feature edit", &["."]).unwrap();
    // Conflicting change on main.
    write(&repo.dir().join("README.md"), "# main\n");
    let main_head = repo.commit("main edit", &["."]).unwrap();

    let mut tx = repo.merge_begin("tx-1", "feature").unwrap();
    assert_eq!(tx.state, MergeState::Conflicted);
    assert_eq!(tx.conflicts, vec!["README.md".to_owned()]);
    assert_eq!(
        (
            tx.source.as_str(),
            tx.target_before.as_str(),
            tx.base.as_str()
        ),
        (feature.as_str(), main_head.as_str(), base.as_str())
    );
    assert!(
        read_lf(&repo.dir().join("README.md")).contains("<<<<<<<"),
        "conflict markers are inspectable evidence"
    );
    // Committing a conflicted transaction is refused.
    assert!(matches!(
        repo.merge_commit(&mut tx, "no").unwrap_err(),
        modbit_git::Error::TransactionState { .. }
    ));
    // Recoverable: abort restores the exact pre-merge target.
    repo.merge_abort(&mut tx).unwrap();
    assert_eq!(tx.state, MergeState::Aborted);
    assert_eq!(repo.head().unwrap(), main_head);
    assert_eq!(read_lf(&repo.dir().join("README.md")), "# main\n");
    assert!(repo.status().unwrap().is_empty());
    // Resolve path: begin again, write a resolution, mark resolved, commit.
    let mut tx2 = repo.merge_begin("tx-2", "feature").unwrap();
    write(&repo.dir().join("README.md"), "# main + feature\n");
    repo.merge_resolve(&mut tx2, "README.md").unwrap();
    assert_eq!(tx2.state, MergeState::Staged);
    let merged = repo.merge_commit(&mut tx2, "merge feature").unwrap();
    assert_eq!(tx2.state, MergeState::Committed);
    assert_eq!(tx2.result.as_deref(), Some(merged.as_str()));
    assert_eq!(
        repo.merge_base(&merged, &feature).unwrap(),
        feature,
        "feature is an ancestor of the merge"
    );
    let json = serde_json::to_string(&tx2).unwrap();
    assert!(json.contains("\"resolutions\":[\"README.md\"]"));
}

#[test]
fn qual_ev_0022_dirty_snapshot_reconstructs_exactly_and_cleanup_removes_the_ref() {
    let (dir, repo) = seed();
    let head = repo.head().unwrap();
    write(
        &repo.dir().join("src/lib.rs"),
        "pub fn a() {}\npub fn dirty() {}\n",
    );
    write(&repo.dir().join("new.txt"), "untracked\n");
    std::fs::remove_file(repo.dir().join("README.md")).unwrap();
    let before_status = repo.status().unwrap();
    let snap = repo.snapshot_dirty("snap-1").unwrap();
    assert_eq!(snap.parent, head);
    assert_eq!(snap.reference, "refs/modbit/snapshots/snap-1");
    assert!(
        snap.paths.contains(&"new.txt".to_owned())
            && snap.paths.contains(&"src/lib.rs".to_owned())
            && snap.paths.contains(&"README.md".to_owned())
    );
    // HEAD, index and worktree untouched.
    assert_eq!(repo.head().unwrap(), head);
    assert_eq!(repo.status().unwrap(), before_status);
    assert!(repo.ref_exists(&snap.reference));
    // Reconstruct in a fresh worktree (as a cloud run would): exact content.
    repo.create_branch("cloud-run", &snap.commit).unwrap();
    let cloud = repo
        .worktree_add(&dir.path().join("wt-cloud"), "cloud-run")
        .unwrap();
    assert_eq!(
        read_lf(&cloud.dir().join("src/lib.rs")),
        "pub fn a() {}\npub fn dirty() {}\n"
    );
    assert_eq!(read_lf(&cloud.dir().join("new.txt")), "untracked\n");
    assert!(!cloud.dir().join("README.md").exists());
    assert_eq!(
        String::from_utf8(repo.show(&snap.commit, "new.txt").unwrap())
            .unwrap()
            .replace("\r\n", "\n"),
        "untracked\n"
    );
    // Cleanup removes the ref safely; the snapshot is no longer addressable by name.
    repo.snapshot_cleanup(&snap).unwrap();
    assert!(!repo.ref_exists(&snap.reference));
    assert!(!repo.dir().join(".git").read_dir().unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("modbit-snapshot-index")
    }));
}

#[test]
fn non_repository_and_bad_revisions_are_typed_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        Repo::open(dir.path()),
        Err(modbit_git::Error::NotARepository(_))
    ));
    let (_d, repo) = seed();
    assert!(matches!(
        repo.diff("nope", "HEAD"),
        Err(modbit_git::Error::Git { .. })
    ));
    assert!(matches!(
        repo.merge_begin("x", "no-such-branch"),
        Err(modbit_git::Error::Git { .. })
    ));
}

/// M3.6: recent history with touched paths, newest first.
#[test]
fn log_recent_lists_commits_with_their_paths_newest_first() {
    let (_dir, repo) = seed();
    write(&repo.dir().join("b.txt"), "b\n");
    write(&repo.dir().join("sub/c.txt"), "c\n");
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=ann",
            "-c",
            "user.email=a@e",
            "commit",
            "-q",
            "-m",
            "second",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(repo.dir())
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    let log = repo.log_recent(10).unwrap();
    assert!(log.len() >= 2, "{log:?}");
    assert_eq!(log[0].subject, "second");
    assert_eq!(log[0].author, "ann");
    assert_eq!(log[0].files, ["b.txt", "sub/c.txt"]);
    assert_eq!(log[0].sha.len(), 40);
    assert!(log[0].date.starts_with("20"));
    assert_eq!(repo.log_recent(1).unwrap().len(), 1);
}
