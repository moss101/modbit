//! PX-065 / PX-119 library support, against real repositories: the typed
//! "branch is checked out elsewhere" failure, pruning, orphan worktrees for a
//! repository with no commit, ancestry, and the per-file evidence of a
//! conflicted merge.

use std::path::Path;

use modbit_git::{Error, MergeState, Repo};

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

fn seed() -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::init(&dir.path().join("repo"), "main").unwrap();
    write(&repo.dir().join("f.txt"), "base\n");
    repo.commit("base", &["."]).unwrap();
    (dir, repo)
}

#[test]
fn a_branch_checked_out_elsewhere_is_a_typed_error_naming_where() {
    let (d, repo) = seed();
    let head = repo.head().unwrap();
    repo.create_branch("task/a", &head).unwrap();
    let first = d.path().join("wt-1");
    repo.worktree_add(&first, "task/a").unwrap();
    let err = repo
        .worktree_add(&d.path().join("wt-2"), "task/a")
        .unwrap_err();
    match err {
        Error::BranchInUse { branch, path } => {
            assert_eq!(branch, "task/a");
            assert_eq!(
                Path::new(&path).canonicalize().unwrap(),
                first.canonicalize().unwrap()
            );
        }
        other => panic!("expected BranchInUse, got {other:?}"),
    }
    // The branch the user's own checkout is on is in use too.
    let err = repo
        .worktree_add(&d.path().join("wt-3"), "main")
        .unwrap_err();
    assert!(matches!(err, Error::BranchInUse { .. }), "{err:?}");
}

#[test]
fn pruning_frees_the_branch_a_deleted_worktree_held() {
    let (d, repo) = seed();
    let head = repo.head().unwrap();
    repo.create_branch("task/a", &head).unwrap();
    let wt = d.path().join("wt");
    repo.worktree_add(&wt, "task/a").unwrap();
    // The directory is removed behind git's back (a crash, a manual rm).
    std::fs::remove_dir_all(&wt).unwrap();
    assert!(matches!(
        repo.worktree_add(&d.path().join("wt-again"), "task/a")
            .unwrap_err(),
        Error::BranchInUse { .. }
    ));
    repo.worktree_prune().unwrap();
    repo.worktree_add(&d.path().join("wt-again"), "task/a")
        .unwrap();
    repo.worktree_remove(&d.path().join("wt-again")).unwrap();
    repo.branch_delete("task/a").unwrap();
    assert!(!repo.ref_exists("refs/heads/task/a"));
}

#[test]
fn ancestry_and_counts_answer_what_the_completion_gate_asks() {
    let (d, repo) = seed();
    let base = repo.head().unwrap();
    repo.create_branch("child", &base).unwrap();
    let c = repo.worktree_add(&d.path().join("c"), "child").unwrap();
    write(&c.dir().join("c.txt"), "c\n");
    let tip = c.commit("child work", &["."]).unwrap();
    assert_eq!(repo.commits_between(&base, &tip).unwrap(), 1);
    assert_eq!(repo.commits_between(&tip, &base).unwrap(), 0);
    assert!(!repo.is_ancestor(&tip, "main").unwrap());
    let mut tx = repo.merge_begin("m", &tip).unwrap();
    assert_eq!(tx.state, MergeState::Staged);
    assert!(repo.merge_in_progress());
    repo.merge_commit(&mut tx, "merge child").unwrap();
    assert!(!repo.merge_in_progress());
    assert!(repo.is_ancestor(&tip, "main").unwrap());
    assert_eq!(
        repo.tree_of("main").unwrap(),
        c.tree_of(&tip).unwrap(),
        "the merge result carries the child's tree here (a fast-forward-shaped merge)"
    );
}

#[test]
fn a_conflicted_merge_reports_the_three_sides_and_the_markers_per_file() {
    let (d, repo) = seed();
    let base = repo.head().unwrap();
    repo.create_branch("child", &base).unwrap();
    let c = repo.worktree_add(&d.path().join("c"), "child").unwrap();
    write(&c.dir().join("f.txt"), "child side\n");
    let tip = c.commit("child", &["."]).unwrap();
    write(&repo.dir().join("f.txt"), "main side\n");
    repo.commit("main", &["."]).unwrap();
    let mut tx = repo.merge_begin("m", &tip).unwrap();
    assert_eq!(tx.state, MergeState::Conflicted);
    assert!(repo.merge_in_progress());
    let ev = repo.conflict_evidence("f.txt").unwrap();
    assert_eq!(ev.base.as_deref().map(str::trim), Some("base"));
    assert_eq!(ev.ours.as_deref().map(str::trim), Some("main side"));
    assert_eq!(ev.theirs.as_deref().map(str::trim), Some("child side"));
    assert!(ev.has_markers && ev.current.contains("<<<<<<<"), "{ev:?}");
    // A resolution without markers clears the flag.
    write(&repo.dir().join("f.txt"), "main side + child side\n");
    assert!(!repo.conflict_evidence("f.txt").unwrap().has_markers);
    repo.merge_abort(&mut tx).unwrap();
    assert!(!repo.merge_in_progress());
    assert_eq!(repo.status().unwrap(), vec![]);
}
