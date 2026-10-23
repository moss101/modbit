//! PX-066 (docs/65 AFW-J08): apply-back of a worktree result to a real
//! checkout, against real repositories and real files. Every claim is made on
//! file hashes: the checkout after an apply, after each conflict option, and
//! after Undo or a simulated crash.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use modbit_git::apply::{
    Blobs, Choice, ConflictKind, EntryState, Request, RestoreMode, View, execute, in_force, plan,
    prepare, restore,
};
use modbit_git::{EMPTY_TREE, Repo};
use sha2::{Digest, Sha256};

/// A content-addressed store in a directory.
struct Store(PathBuf);

impl Blobs for Store {
    fn put(&self, bytes: &[u8]) -> Result<String, String> {
        let id = hex::encode(Sha256::digest(bytes));
        std::fs::write(self.0.join(&id), bytes).map_err(|e| e.to_string())?;
        Ok(id)
    }

    fn get(&self, id: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.0.join(id)).map_err(|e| e.to_string())
    }
}

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

/// Every file under `root` (not `.git`) by path, with the sha256 of its bytes.
fn tree_hashes(root: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if e.file_name() == ".git" {
                continue;
            }
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, hex::encode(Sha256::digest(std::fs::read(&p).unwrap())));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn text(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap().replace("\r\n", "\n")
}

struct World {
    _dir: tempfile::TempDir,
    store: Store,
    checkout: Repo,
    task: Repo,
    base: String,
}

/// A checkout with five committed files and a worktree of it on its own branch.
fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let checkout = Repo::init(&dir.path().join("checkout"), "main").unwrap();
    write(
        &checkout.dir().join("a.txt"),
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
    );
    write(&checkout.dir().join("del.txt"), "to be deleted\n");
    write(&checkout.dir().join("keep.txt"), "untouched\n");
    write(&checkout.dir().join("src/lib.rs"), "pub fn a() {}\n");
    write(&checkout.dir().join("bin.dat"), "bin\0ary\n");
    checkout.commit("base", &["."]).unwrap();
    let base = checkout.head().unwrap();
    checkout.create_branch("task/x", &base).unwrap();
    let task = checkout
        .worktree_add(&dir.path().join("task-wt"), "task/x")
        .unwrap();
    // The task: edits the top of a.txt, deletes del.txt, adds a new file in a
    // new directory.
    write(
        &task.dir().join("a.txt"),
        "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
    );
    std::fs::remove_file(task.dir().join("del.txt")).unwrap();
    write(&task.dir().join("new/dir/added.txt"), "added by the task\n");
    let store = Store({
        let p = dir.path().join("blobs");
        std::fs::create_dir_all(&p).unwrap();
        p
    });
    World {
        _dir: dir,
        store,
        checkout,
        task,
        base,
    }
}

fn not_protected(_: &str) -> bool {
    false
}

fn req(choice: Choice, confirm: &[&str]) -> Request {
    Request {
        choice,
        confirm_paths: confirm.iter().map(|s| (*s).to_owned()).collect(),
        apply_id: "apply-1".into(),
    }
}

fn the_plan(w: &World, view: View) -> modbit_git::apply::ApplyPlan {
    let candidate = w.task.snapshot_dirty("cand").unwrap().commit;
    plan(&w.checkout, Some(&w.base), &candidate, view, &not_protected).unwrap()
}

fn state_of<'a>(p: &'a modbit_git::apply::ApplyPlan, path: &str) -> &'a EntryState {
    &p.entries.iter().find(|e| e.path == path).unwrap().state
}

#[test]
fn a_clean_apply_makes_the_checkout_equal_the_candidate_and_undo_is_exact() {
    let w = world();
    // The user's own, unrelated edits stay.
    write(&w.checkout.dir().join("keep.txt"), "user edit in keep\n");
    write(
        &w.checkout.dir().join("scratch.txt"),
        "untracked user file\n",
    );
    let before = tree_hashes(w.checkout.dir());
    let p = the_plan(&w, View::Disk);
    assert!(p.conflicts().is_empty());
    assert_eq!(*state_of(&p, "a.txt"), EntryState::Clean);
    assert_eq!(*state_of(&p, "del.txt"), EntryState::Clean);
    assert_eq!(*state_of(&p, "new/dir/added.txt"), EntryState::Clean);
    assert_eq!(
        p.entries.len(),
        3,
        "only the task's own changes are in the plan: {:?}",
        p.entries.iter().map(|e| &e.path).collect::<Vec<_>>()
    );

    let prepared = prepare(&w.checkout, &p, &req(Choice::Overwrite, &[]), &w.store).unwrap();
    // Nothing is written by `prepare`.
    assert_eq!(tree_hashes(w.checkout.dir()), before);
    execute(w.checkout.dir(), &prepared, &mut |_, _| {}).unwrap();
    let after = tree_hashes(w.checkout.dir());
    // The checkout equals the candidate on every path the task changed, and
    // the user's edits are exactly as they were.
    let cand = tree_hashes(w.task.dir());
    for path in ["a.txt", "new/dir/added.txt"] {
        assert_eq!(after.get(path), cand.get(path), "{path}");
    }
    assert!(!after.contains_key("del.txt"));
    assert_eq!(after["keep.txt"], before["keep.txt"]);
    assert_eq!(after["scratch.txt"], before["scratch.txt"]);
    assert!(in_force(w.checkout.dir(), &prepared.manifest).unwrap());

    // Undo restores every byte, removes the created directories.
    let report = restore(
        w.checkout.dir(),
        &prepared.manifest,
        &w.store,
        RestoreMode::AllOrNothing,
    )
    .unwrap();
    assert!(report.divergent.is_empty(), "{report:?}");
    assert_eq!(tree_hashes(w.checkout.dir()), before);
    assert!(!w.checkout.dir().join("new").exists());
    assert!(!in_force(w.checkout.dir(), &prepared.manifest).unwrap());
}

#[test]
fn a_user_edit_in_another_region_merges_and_a_user_edit_in_the_same_region_conflicts() {
    let w = world();
    // The user edited the bottom of a.txt; the task edited the top.
    write(
        &w.checkout.dir().join("a.txt"),
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nTEN by the user\n",
    );
    let p = the_plan(&w, View::Disk);
    assert_eq!(*state_of(&p, "a.txt"), EntryState::AutoMerge);
    let prepared = prepare(&w.checkout, &p, &req(Choice::MergeManually, &[]), &w.store).unwrap();
    assert!(prepared.unresolved.is_empty());
    execute(w.checkout.dir(), &prepared, &mut |_, _| {}).unwrap();
    assert_eq!(
        text(&w.checkout.dir().join("a.txt")),
        "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nTEN by the user\n"
    );
}

#[test]
fn every_conflict_option_is_tested_on_hashes_and_undo_restores_the_users_bytes() {
    let w = world();
    // Same region: the user edited the line the task edited.
    let user_a = "uno\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
    write(&w.checkout.dir().join("a.txt"), user_a);
    write(&w.checkout.dir().join("keep.txt"), "user edit in keep\n");
    write(
        &w.checkout.dir().join("scratch.txt"),
        "untracked user file\n",
    );
    let before = tree_hashes(w.checkout.dir());
    let p = the_plan(&w, View::Disk);
    assert_eq!(
        *state_of(&p, "a.txt"),
        EntryState::Conflict(ConflictKind::BothModified)
    );
    let candidate_a = std::fs::read(w.task.dir().join("a.txt")).unwrap();

    // CANCEL (the default): nothing happens.
    let refusal = prepare(&w.checkout, &p, &req(Choice::Cancel, &[]), &w.store).unwrap_err();
    assert_eq!(refusal.code, "CANCELLED");
    assert_eq!(refusal.paths, vec!["a.txt".to_owned()]);
    assert_eq!(tree_hashes(w.checkout.dir()), before);

    // OVERWRITE without the typed confirmation is refused, naming the file.
    let refusal = prepare(&w.checkout, &p, &req(Choice::Overwrite, &[]), &w.store).unwrap_err();
    assert_eq!(refusal.code, "OVERWRITE_NOT_CONFIRMED");
    assert_eq!(refusal.paths, vec!["a.txt".to_owned()]);
    // ... and a confirmation that names another file does not count.
    let refusal = prepare(
        &w.checkout,
        &p,
        &req(Choice::Overwrite, &["keep.txt"]),
        &w.store,
    )
    .unwrap_err();
    assert_eq!(refusal.code, "OVERWRITE_NOT_CONFIRMED");
    assert_eq!(tree_hashes(w.checkout.dir()), before);

    // MERGE_MANUALLY: clean paths apply, the conflicting file carries markers.
    let prepared = prepare(&w.checkout, &p, &req(Choice::MergeManually, &[]), &w.store).unwrap();
    assert_eq!(prepared.unresolved, vec!["a.txt".to_owned()]);
    execute(w.checkout.dir(), &prepared, &mut |_, _| {}).unwrap();
    let merged = text(&w.checkout.dir().join("a.txt"));
    assert!(
        merged.contains("<<<<<<< checkout") && merged.contains("uno") && merged.contains("ONE"),
        "{merged}"
    );
    assert!(w.checkout.dir().join("new/dir/added.txt").exists());
    assert!(!w.checkout.dir().join("del.txt").exists());
    // Undo is exact.
    let r = restore(
        w.checkout.dir(),
        &prepared.manifest,
        &w.store,
        RestoreMode::AllOrNothing,
    )
    .unwrap();
    assert!(r.divergent.is_empty());
    assert_eq!(tree_hashes(w.checkout.dir()), before);

    // OVERWRITE with the confirmation: conflicting file only.
    let prepared = prepare(
        &w.checkout,
        &p,
        &req(Choice::Overwrite, &["a.txt"]),
        &w.store,
    )
    .unwrap();
    execute(w.checkout.dir(), &prepared, &mut |_, _| {}).unwrap();
    assert_eq!(
        std::fs::read(w.checkout.dir().join("a.txt")).unwrap(),
        candidate_a
    );
    let after = tree_hashes(w.checkout.dir());
    assert_eq!(
        after["keep.txt"], before["keep.txt"],
        "unrelated edit stays"
    );
    assert_eq!(after["scratch.txt"], before["scratch.txt"]);
    restore(
        w.checkout.dir(),
        &prepared.manifest,
        &w.store,
        RestoreMode::AllOrNothing,
    )
    .unwrap();
    assert_eq!(tree_hashes(w.checkout.dir()), before);

    // FULL_OVERWRITE: also the user's other dirty paths become the task's
    // tree; the confirmation must name every one of them.
    let refusal = prepare(
        &w.checkout,
        &p,
        &req(Choice::FullOverwrite, &["a.txt"]),
        &w.store,
    )
    .unwrap_err();
    assert_eq!(refusal.code, "OVERWRITE_NOT_CONFIRMED");
    assert_eq!(
        refusal.paths,
        vec!["keep.txt".to_owned(), "scratch.txt".to_owned()]
    );
    let prepared = prepare(
        &w.checkout,
        &p,
        &req(Choice::FullOverwrite, &["a.txt", "keep.txt", "scratch.txt"]),
        &w.store,
    )
    .unwrap();
    execute(w.checkout.dir(), &prepared, &mut |_, _| {}).unwrap();
    let after = tree_hashes(w.checkout.dir());
    let cand = tree_hashes(w.task.dir());
    assert_eq!(after["a.txt"], cand["a.txt"]);
    assert_eq!(
        after["keep.txt"], cand["keep.txt"],
        "reverted to the task's tree"
    );
    assert!(!after.contains_key("scratch.txt"), "untracked file removed");
    restore(
        w.checkout.dir(),
        &prepared.manifest,
        &w.store,
        RestoreMode::AllOrNothing,
    )
    .unwrap();
    assert_eq!(tree_hashes(w.checkout.dir()), before);

    // STASH: the user's dirty state is saved as a snapshot, the checkout is
    // reset to HEAD, the task's change applies on the clean tree.
    let head_plan = the_plan(&w, View::Head);
    assert!(head_plan.conflicts().is_empty(), "no conflict against HEAD");
    let prepared = prepare(&w.checkout, &head_plan, &req(Choice::Stash, &[]), &w.store).unwrap();
    let stash_ref = prepared.manifest.stash_ref.clone().expect("stash ref");
    execute(w.checkout.dir(), &prepared, &mut |_, _| {}).unwrap();
    let after = tree_hashes(w.checkout.dir());
    assert_eq!(after["a.txt"], cand["a.txt"]);
    assert_eq!(text(&w.checkout.dir().join("keep.txt")), "untouched\n");
    assert!(!after.contains_key("scratch.txt"));
    // The user's work is retrievable from the stash snapshot.
    assert_eq!(
        w.checkout.show(&stash_ref, "keep.txt").unwrap(),
        b"user edit in keep\n"
    );
    assert_eq!(
        w.checkout.show(&stash_ref, "a.txt").unwrap(),
        user_a.as_bytes()
    );
    restore(
        w.checkout.dir(),
        &prepared.manifest,
        &w.store,
        RestoreMode::AllOrNothing,
    )
    .unwrap();
    assert_eq!(tree_hashes(w.checkout.dir()), before);
}

#[test]
fn undo_after_a_second_user_edit_reports_the_conflict_and_destroys_nothing() {
    let w = world();
    let p = the_plan(&w, View::Disk);
    let prepared = prepare(&w.checkout, &p, &req(Choice::MergeManually, &[]), &w.store).unwrap();
    execute(w.checkout.dir(), &prepared, &mut |_, _| {}).unwrap();
    // The user keeps working on a file the apply wrote.
    write(
        &w.checkout.dir().join("a.txt"),
        "ONE then the user's second edit\n",
    );
    let held = tree_hashes(w.checkout.dir());
    let r = restore(
        w.checkout.dir(),
        &prepared.manifest,
        &w.store,
        RestoreMode::AllOrNothing,
    )
    .unwrap();
    assert_eq!(r.divergent, vec!["a.txt".to_owned()]);
    assert!(r.restored.is_empty());
    assert_eq!(
        tree_hashes(w.checkout.dir()),
        held,
        "an undo that finds a second edit changes nothing"
    );
    assert!(!in_force(w.checkout.dir(), &prepared.manifest).unwrap());
}

#[test]
fn a_crash_in_the_middle_of_an_apply_rolls_back_to_the_exact_pre_apply_state() {
    let w = world();
    let before = tree_hashes(w.checkout.dir());
    let p = the_plan(&w, View::Disk);
    let prepared = prepare(&w.checkout, &p, &req(Choice::Overwrite, &[]), &w.store).unwrap();
    assert!(prepared.len() >= 3);
    // The process dies after the first write: simulated by a panic in the
    // step hook, which unwinds out of `execute` with the writes so far done.
    let dir = w.checkout.dir().to_path_buf();
    let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut n = 0;
        let _ = execute(&dir, &prepared, &mut |_, _| {
            n += 1;
            assert!(n <= 1, "killed");
        });
    }));
    assert!(died.is_err());
    let mid = tree_hashes(w.checkout.dir());
    assert_ne!(mid, before, "one write landed: a half-applied tree");
    // Recovery: every path is back to its pre-apply bytes.
    let r = restore(
        w.checkout.dir(),
        &prepared.manifest,
        &w.store,
        RestoreMode::BestEffort,
    )
    .unwrap();
    assert!(r.divergent.is_empty(), "{r:?}");
    assert_eq!(tree_hashes(w.checkout.dir()), before);
}

#[test]
fn protected_paths_and_symlinks_are_not_applied_without_saying_so() {
    let w = world();
    write(&w.task.dir().join(".github/workflows/ci.yml"), "name: ci\n");
    let candidate = w.task.snapshot_dirty("cand").unwrap().commit;
    let is_protected = |p: &str| p.starts_with(".github/workflows/");
    let p = plan(
        &w.checkout,
        Some(&w.base),
        &candidate,
        View::Disk,
        &is_protected,
    )
    .unwrap();
    assert_eq!(p.protected().len(), 1);
    let refusal = prepare(&w.checkout, &p, &req(Choice::Overwrite, &[]), &w.store).unwrap_err();
    assert_eq!(refusal.code, "PROTECTED_PATHS");
    assert_eq!(refusal.paths, vec![".github/workflows/ci.yml".to_owned()]);
    let prepared = prepare(
        &w.checkout,
        &p,
        &req(Choice::Overwrite, &[".github/workflows/ci.yml"]),
        &w.store,
    )
    .unwrap();
    execute(w.checkout.dir(), &prepared, &mut |_, _| {}).unwrap();
    assert!(w.checkout.dir().join(".github/workflows/ci.yml").exists());
}

#[cfg(unix)]
#[test]
fn a_symlink_change_is_refused_not_dropped() {
    let w = world();
    std::os::unix::fs::symlink("a.txt", w.task.dir().join("link")).unwrap();
    let p = the_plan(&w, View::Disk);
    assert_eq!(*state_of(&p, "link"), EntryState::Unsupported);
    let refusal = prepare(&w.checkout, &p, &req(Choice::Overwrite, &[]), &w.store).unwrap_err();
    assert_eq!(refusal.code, "UNSUPPORTED_CHANGE");
}

#[test]
fn the_plan_digest_changes_when_the_checkout_or_the_candidate_does() {
    let w = world();
    let p1 = the_plan(&w, View::Disk);
    let p1b = the_plan(&w, View::Disk);
    assert_eq!(p1.digest, p1b.digest, "same state, same digest");
    write(
        &w.checkout.dir().join("a.txt"),
        "uno\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
    );
    let p2 = the_plan(&w, View::Disk);
    assert_ne!(p1.digest, p2.digest, "a user edit makes the approval stale");
    write(&w.task.dir().join("more.txt"), "more\n");
    let p3 = the_plan(&w, View::Disk);
    assert_ne!(p2.digest, p3.digest, "a new task change makes it stale");
}

#[test]
fn an_empty_repository_gets_an_orphan_worktree_and_applies_from_the_empty_tree() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = Repo::init(&dir.path().join("empty"), "main").unwrap();
    assert!(checkout.is_unborn());
    let wt = checkout
        .worktree_add_orphan(&dir.path().join("wt"), "modbit/task-1")
        .unwrap();
    assert!(wt.is_unborn());
    write(&wt.dir().join("hello.txt"), "hello\n");
    let snap = wt.snapshot_dirty("cand").unwrap();
    assert_eq!(
        snap.parent, "",
        "an unborn worktree snapshots without a parent"
    );
    let store_dir = dir.path().join("blobs");
    std::fs::create_dir_all(&store_dir).unwrap();
    let store = Store(store_dir);
    let p = plan(&checkout, None, &snap.commit, View::Disk, &not_protected).unwrap();
    assert_eq!(p.base_tree, EMPTY_TREE);
    assert_eq!(p.checkout_head, None);
    let prepared = prepare(&checkout, &p, &req(Choice::Overwrite, &[]), &store).unwrap();
    execute(checkout.dir(), &prepared, &mut |_, _| {}).unwrap();
    assert_eq!(text(&checkout.dir().join("hello.txt")), "hello\n");
    restore(
        checkout.dir(),
        &prepared.manifest,
        &store,
        RestoreMode::AllOrNothing,
    )
    .unwrap();
    assert!(!checkout.dir().join("hello.txt").exists());
}
