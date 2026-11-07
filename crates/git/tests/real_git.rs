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

/// What the real index holds, byte for byte, and what git says about it. The
/// status is asked with `--no-optional-locks`: a plain `git status` would
/// refresh the very index under test.
fn index_state(repo: &Repo) -> (Vec<u8>, String, String) {
    let git_dir = String::from_utf8_lossy(
        &modbit_git::output(repo.dir(), &["rev-parse", "--absolute-git-dir"])
            .unwrap()
            .stdout,
    )
    .trim()
    .to_owned();
    let text = |args: &[&str]| {
        let out = modbit_git::output(repo.dir(), args).unwrap();
        assert!(out.status.success(), "{args:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    (
        std::fs::read(Path::new(&git_dir).join("index")).unwrap(),
        text(&["--no-optional-locks", "status", "--porcelain"]),
        text(&["ls-files", "--stage"]),
    )
}

fn leftovers(repo: &Repo) -> Vec<String> {
    let git_dir = String::from_utf8_lossy(
        &modbit_git::output(repo.dir(), &["rev-parse", "--absolute-git-dir"])
            .unwrap()
            .stdout,
    )
    .trim()
    .to_owned();
    std::fs::read_dir(git_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("modbit-") || n.ends_with(".lock"))
        .collect()
}

#[test]
fn a_diff_with_untracked_files_equals_add_n_and_never_touches_the_real_index() {
    let (_dir, repo) = seed();
    // A tracked edit, a staged new file, new files with names a pathspec would
    // misread, and a tracked file whose stat changed but whose content did not.
    write(&repo.dir().join("src/lib.rs"), "pub fn a() { 1; }\n");
    write(&repo.dir().join("staged.txt"), "staged\n");
    repo.stage_paths(&["staged.txt"]).unwrap();
    write(&repo.dir().join("fresh.txt"), "one\ntwo\n");
    write(&repo.dir().join("a b[1].txt"), "bracket\n");
    write(&repo.dir().join("deep/er/new.rs"), "// new\n");
    write(&repo.dir().join("empty.txt"), "");
    std::fs::remove_file(repo.dir().join("README.md")).unwrap();
    write(&repo.dir().join("README.md"), "# demo\n");
    let untracked = ["a b[1].txt", "deep/er/new.rs", "empty.txt", "fresh.txt"];

    let before = index_state(&repo);
    let read = repo.diff_worktree_with_untracked(&untracked).unwrap();
    assert_eq!(
        index_state(&repo),
        before,
        "the read changed the real index"
    );
    assert!(leftovers(&repo).is_empty(), "{:?}", leftovers(&repo));
    for path in untracked {
        assert!(
            read.files.iter().any(|f| f.path == path),
            "{path} is not in the diff: {:?}",
            read.files
        );
    }

    // Byte for byte what the real `add -N` then `diff HEAD` answers.
    let mut add = vec!["add", "-N", "--"];
    add.extend(untracked);
    assert!(
        modbit_git::output(repo.dir(), &add)
            .unwrap()
            .status
            .success()
    );
    let real = repo.diff_worktree().unwrap();
    assert_eq!(read.unified, real.unified);
    assert_eq!(read.files, real.files);
}

#[test]
fn a_diff_with_no_untracked_files_is_the_plain_worktree_diff() {
    let (_dir, repo) = seed();
    write(&repo.dir().join("src/lib.rs"), "pub fn a() { 1; }\n");
    let before = index_state(&repo);
    let read = repo.diff_worktree_with_untracked(&[]).unwrap();
    assert_eq!(index_state(&repo), before);
    assert_eq!(read.unified, repo.diff_worktree().unwrap().unified);
}

#[test]
fn git_clean_removes_a_file_a_diff_read_has_shown() {
    let (_dir, repo) = seed();
    write(&repo.dir().join("junk/new.txt"), "scratch\n");
    write(&repo.dir().join("top.txt"), "scratch\n");
    repo.diff_worktree_with_untracked(&["junk/new.txt", "top.txt"])
        .unwrap();
    let out = modbit_git::output(repo.dir(), &["clean", "-fd"]).unwrap();
    assert!(out.status.success());
    assert!(!repo.dir().join("top.txt").exists());
    assert!(!repo.dir().join("junk").exists());
    assert!(repo.status().unwrap().is_empty());
}

#[test]
fn a_diff_read_in_a_linked_worktree_leaves_both_indexes_alone() {
    let (dir, repo) = seed();
    let head = repo.head().unwrap();
    repo.create_branch("task/w", &head).unwrap();
    let wt = repo.worktree_add(&dir.path().join("wt"), "task/w").unwrap();
    write(&wt.dir().join("fresh.txt"), "new\n");
    let (main_before, wt_before) = (index_state(&repo), index_state(&wt));
    let read = wt.diff_worktree_with_untracked(&["fresh.txt"]).unwrap();
    assert!(read.files.iter().any(|f| f.path == "fresh.txt"));
    assert_eq!(index_state(&repo), main_before);
    assert_eq!(index_state(&wt), wt_before);
    assert!(leftovers(&wt).is_empty(), "{:?}", leftovers(&wt));
}

#[test]
fn a_diff_read_takes_more_untracked_files_than_a_command_line_holds() {
    let (_dir, repo) = seed();
    let names: Vec<String> = (0..4000)
        .map(|i| format!("generated/{:0>60}.txt", i))
        .collect();
    for n in &names {
        write(&repo.dir().join(n), "x\n");
    }
    let paths: Vec<&str> = names.iter().map(String::as_str).collect();
    let read = repo.diff_worktree_with_untracked(&paths).unwrap();
    assert_eq!(read.files.len(), names.len());
}

#[test]
fn a_file_gone_since_the_status_is_left_out_and_the_rest_still_shows() {
    let (_dir, repo) = seed();
    write(&repo.dir().join("here.txt"), "here\n");
    let read = repo
        .diff_worktree_with_untracked(&["vanished.txt", "here.txt"])
        .unwrap();
    let paths: Vec<&str> = read.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["here.txt"]);
}

#[test]
fn a_diff_read_on_a_corrupt_index_fails_and_leaves_it_as_it_was() {
    let (_dir, repo) = seed();
    write(&repo.dir().join("fresh.txt"), "new\n");
    let index = repo.dir().join(".git/index");
    std::fs::write(&index, b"not an index").unwrap();
    let err = repo
        .diff_worktree_with_untracked(&["fresh.txt"])
        .unwrap_err();
    assert!(matches!(err, modbit_git::Error::Git { .. }), "{err:?}");
    assert_eq!(std::fs::read(&index).unwrap(), b"not an index");
    assert!(leftovers(&repo).is_empty(), "{:?}", leftovers(&repo));
}

#[test]
fn status_leaves_a_stat_dirty_index_alone() {
    let (_dir, repo) = seed();
    // Same content, new inode: a plain `git status` rewrites the index to
    // record the new stat data.
    std::fs::remove_file(repo.dir().join("README.md")).unwrap();
    write(&repo.dir().join("README.md"), "# demo\n");
    let before = std::fs::read(repo.dir().join(".git/index")).unwrap();
    assert!(repo.status().unwrap().is_empty());
    assert_eq!(
        std::fs::read(repo.dir().join(".git/index")).unwrap(),
        before
    );
}
