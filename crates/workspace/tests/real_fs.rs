//! Real-filesystem tests for the Workspace File Service (docs/20, docs/23):
//! symlink escapes, `..` traversal, protected paths, optimistic preconditions,
//! atomic replacement, monotonic persisted revisions bound to a real Git HEAD.

use std::path::Path;
use std::process::Command;

use modbit_workspace::{
    ApplyPatch, ChangeOp, ChangeOpKind, Edit, EntryKind, Error, MatchTier, TextEdit,
    WorkspaceService, WritePrecondition, content_hash,
};

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
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn setup() -> (tempfile::TempDir, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.path().join("README.md"), "# demo\n").unwrap();
    (root, state)
}

#[test]
fn typed_operations_advance_a_persisted_monotonic_revision_with_change_records() {
    let (root, state) = setup();
    let mut ws = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    assert_eq!(ws.revision().number, 1);
    assert!(ws.revision().git_head.is_none(), "not a git repo yet");
    let r = ws.read("src/main.rs").unwrap();
    assert_eq!(r.bytes, b"fn main() {}\n");
    let c1 = ws
        .create(
            "src/lib.rs",
            b"pub fn f() {}\n",
            WritePrecondition::default(),
        )
        .unwrap();
    assert_eq!(
        (
            c1.op.as_str(),
            c1.before_hash.is_none(),
            c1.workspace_revision.number
        ),
        ("create", true, 2)
    );
    let c2 = ws
        .apply_patch(
            "src/main.rs",
            &ApplyPatch {
                edits: vec![Edit {
                    start: 3,
                    end: 7,
                    replacement: b"run".to_vec(),
                }],
            },
            WritePrecondition {
                expected_content_hash: Some(r.content_hash.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(ws.read("src/main.rs").unwrap().bytes, b"fn run() {}\n");
    assert_eq!(c2.workspace_revision.number, 3);
    assert_ne!(c2.before_hash, c2.after_hash);
    let listed = ws.list("src").unwrap();
    assert_eq!(
        listed.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
        ["src/lib.rs", "src/main.rs"]
    );
    assert!(
        listed
            .iter()
            .all(|s| s.kind == EntryKind::File && s.content_hash.is_some())
    );
    ws.mkdir("docs/notes").unwrap();
    ws.r#move(
        "README.md",
        "docs/notes/README.md",
        WritePrecondition::default(),
    )
    .unwrap();
    assert!(ws.stat("docs/notes/README.md").is_ok());
    assert!(matches!(ws.read("README.md"), Err(Error::Io { .. })));
    let c = ws
        .delete("src/lib.rs", WritePrecondition::default())
        .unwrap();
    assert_eq!(c.after_hash, None);
    let n = ws.revision().number;
    assert_eq!(n, 7, "create, patch, mkdir, move(2), delete");
    let fp = ws.revision().fingerprint.clone();
    assert_eq!(
        ws.revision().changed.len(),
        5,
        "{:?}",
        ws.revision().changed
    );
    // Persisted: a fresh service on the same root and state continues the sequence.
    drop(ws);
    let ws2 = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    assert_eq!(
        (ws2.revision().number, &ws2.revision().fingerprint),
        (n, &fp)
    );
    // The worktree itself holds no service state.
    assert!(!root.path().join(".modbit").exists());
    assert!(std::fs::read_dir(root.path()).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".modbit-tmp")
    }));
}

#[test]
fn revision_binds_to_the_real_git_head_and_resets_changed_set_on_new_commit() {
    let (root, state) = setup();
    git(root.path(), &["init", "-q", "-b", "main"]);
    git(root.path(), &["add", "-A"]);
    git(root.path(), &["commit", "-q", "-m", "base"]);
    let mut ws = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    let head1 = ws.revision().git_head.clone().expect("HEAD");
    assert_eq!(head1.len(), 40);
    ws.atomic_replace("README.md", b"# changed\n", WritePrecondition::default())
        .unwrap();
    assert_eq!(ws.revision().changed.len(), 1);
    git(root.path(), &["commit", "-q", "-am", "next"]);
    ws.atomic_replace(
        "README.md",
        b"# changed again\n",
        WritePrecondition::default(),
    )
    .unwrap();
    let head2 = ws.revision().git_head.clone().unwrap();
    assert_ne!(head1, head2);
    assert_eq!(
        ws.revision().changed.len(),
        1,
        "changed set restarts at the new HEAD"
    );
    assert_eq!(ws.revision().number, 3);
}

#[cfg(unix)]
#[test]
fn symlink_escapes_and_parent_traversal_are_rejected_for_reads_and_writes() {
    let (root, state) = setup();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "top secret").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        root.path().join("link.txt"),
    )
    .unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("linkdir")).unwrap();
    let mut ws = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    for p in [
        "link.txt",
        "linkdir/secret.txt",
        "../secret.txt",
        "src/../../secret.txt",
        outside.path().join("secret.txt").to_str().unwrap(),
    ] {
        let err = ws.read(p).unwrap_err();
        assert!(matches!(err, Error::OutsideRoot { .. }), "{p}: {err}");
        let err = ws
            .atomic_replace(p, b"x", WritePrecondition::default())
            .unwrap_err();
        assert!(matches!(err, Error::OutsideRoot { .. }), "{p}: {err}");
    }
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
        "top secret",
        "nothing outside was touched"
    );
    // A symlink that stays inside the root is fine and resolves to the real file.
    std::os::unix::fs::symlink(
        root.path().join("src/main.rs"),
        root.path().join("inner-link.rs"),
    )
    .unwrap();
    assert_eq!(ws.read("inner-link.rs").unwrap().path, "inner-link.rs");
    ws.atomic_replace(
        "inner-link.rs",
        b"fn via_link() {}\n",
        WritePrecondition::default(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/main.rs")).unwrap(),
        "fn via_link() {}\n"
    );
}

#[test]
fn protected_paths_are_denied_after_resolution_and_listed_as_opaque() {
    let (root, state) = setup();
    git(root.path(), &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(root.path().join(".ssh")).unwrap();
    std::fs::write(root.path().join(".ssh/id_ed25519"), "key").unwrap();
    std::fs::write(root.path().join(".env"), "TOKEN=1").unwrap();
    let mut ws =
        WorkspaceService::open(root.path(), state.path(), &["**/*.secret".into()]).unwrap();
    std::fs::write(root.path().join("db.secret"), "x").unwrap();
    for p in [
        ".git/config",
        ".git/HEAD",
        ".ssh/id_ed25519",
        ".env",
        "db.secret",
        "src/../.env",
    ] {
        let err = ws.read(p).unwrap_err();
        assert!(matches!(err, Error::Protected { .. }), "{p}: {err}");
        let err = ws
            .atomic_replace(p, b"pwned", WritePrecondition::default())
            .unwrap_err();
        assert!(matches!(err, Error::Protected { .. }), "{p}: {err}");
        let err = ws.delete(p, WritePrecondition::default()).unwrap_err();
        assert!(matches!(err, Error::Protected { .. }), "{p}: {err}");
    }
    assert_eq!(
        std::fs::read_to_string(root.path().join(".env")).unwrap(),
        "TOKEN=1"
    );
    let top = ws.list("").unwrap();
    let env = top.iter().find(|s| s.path == ".env").unwrap();
    assert_eq!(
        (env.kind, env.content_hash.is_none()),
        (EntryKind::Other, true),
        "protected entries are opaque"
    );
    assert_eq!(
        ws.revision().number,
        1,
        "denied operations never advance the revision"
    );
}

#[test]
fn stale_preconditions_are_rejected_and_nothing_is_written() {
    let (root, state) = setup();
    let mut ws = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    let r = ws.read("src/main.rs").unwrap();
    // Someone else edits the file behind our back (blind-overwrite hazard).
    std::fs::write(root.path().join("src/main.rs"), "fn other() {}\n").unwrap();
    let err = ws
        .atomic_replace(
            "src/main.rs",
            b"fn mine() {}\n",
            WritePrecondition {
                expected_content_hash: Some(r.content_hash.clone()),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, Error::Precondition { .. }), "{err}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/main.rs")).unwrap(),
        "fn other() {}\n"
    );
    let err = ws
        .atomic_replace(
            "src/main.rs",
            b"x",
            WritePrecondition {
                expected_workspace_revision: Some(42),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, Error::Precondition { .. }));
    let err = ws
        .create("src/main.rs", b"x", WritePrecondition::default())
        .unwrap_err();
    assert!(
        matches!(err, Error::Precondition { .. }),
        "create refuses to clobber"
    );
    let err = ws
        .apply_patch(
            "src/main.rs",
            &ApplyPatch {
                edits: vec![Edit {
                    start: 5,
                    end: 3,
                    replacement: vec![],
                }],
            },
            WritePrecondition::default(),
        )
        .unwrap_err();
    assert!(matches!(err, Error::EditOutOfBounds { .. }));
    let err = ws
        .apply_patch(
            "src/main.rs",
            &ApplyPatch {
                edits: vec![
                    Edit {
                        start: 0,
                        end: 2,
                        replacement: vec![],
                    },
                    Edit {
                        start: 1,
                        end: 3,
                        replacement: vec![],
                    },
                ],
            },
            WritePrecondition::default(),
        )
        .unwrap_err();
    assert!(
        matches!(err, Error::EditOutOfBounds { .. }),
        "overlapping edits"
    );
    assert_eq!(ws.revision().number, 1);
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/main.rs")).unwrap(),
        "fn other() {}\n"
    );
    // No temp files leak after failures.
    assert!(
        std::fs::read_dir(root.path().join("src"))
            .unwrap()
            .all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".modbit-tmp"))
    );
}

#[cfg(unix)]
#[test]
fn replacement_is_atomic_when_the_rename_fails() {
    use std::os::unix::fs::PermissionsExt;
    let (root, state) = setup();
    let mut ws = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    // Make the directory read-only so the rename (and temp create) fails mid-operation.
    std::fs::set_permissions(
        root.path().join("src"),
        std::fs::Permissions::from_mode(0o555),
    )
    .unwrap();
    let err = ws
        .atomic_replace("src/main.rs", b"partial", WritePrecondition::default())
        .unwrap_err();
    assert!(matches!(err, Error::Io { .. }), "{err}");
    std::fs::set_permissions(
        root.path().join("src"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/main.rs")).unwrap(),
        "fn main() {}\n",
        "original intact"
    );
    assert_eq!(ws.revision().number, 1);
}

fn edit(old: &str, new: &str) -> TextEdit {
    TextEdit {
        old: old.into(),
        new: new.into(),
    }
}

/// QUAL-EV-0015: the match ladder is exact → whitespace remap → error; an
/// ambiguous (duplicated) target fails at either tier and the worktree,
/// revision and temp files are exactly as before; a miss carries a
/// contextual suggestion that is never applied.
#[test]
fn qual_ev_0015_ambiguous_target_fails_and_leaves_the_worktree_unchanged() {
    let (root, state) = setup();
    let src = "fn a() {\n    return 1;\n}\nfn b() {\n    return 1;\n}\n";
    std::fs::write(root.path().join("src/lib.rs"), src).unwrap();
    let mut ws = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    let rev0 = ws.revision().number;
    let pre = WritePrecondition::default();
    // Exact tier: two byte-identical occurrences.
    let e = ws
        .edit_by_match(
            "src/lib.rs",
            &[edit("    return 1;\n", "    return 2;\n")],
            pre.clone(),
        )
        .unwrap_err();
    assert!(
        matches!(&e, Error::StepFailed { step: 0, cause, .. }
            if matches!(**cause, Error::AmbiguousTarget { occurrences: 2, ref tier, .. } if tier == "exact")),
        "{e}"
    );
    // Whitespace tier: no exact match (tab vs spaces), two remapped matches.
    let e = ws
        .edit_by_match(
            "src/lib.rs",
            &[edit("\treturn 1;", "\treturn 2;")],
            pre.clone(),
        )
        .unwrap_err();
    assert!(
        matches!(&e, Error::StepFailed { cause, .. }
            if matches!(**cause, Error::AmbiguousTarget { occurrences: 2, ref tier, .. } if tier == "whitespace_remap")),
        "{e}"
    );
    // Miss: a contextual suggestion, never a guess.
    let e = ws
        .edit_by_match(
            "src/lib.rs",
            &[edit("    return 3;", "    return 4;")],
            pre.clone(),
        )
        .unwrap_err();
    assert!(
        matches!(&e, Error::StepFailed { cause, .. }
            if matches!(**cause, Error::NoMatch { suggestion: Some(ref s), .. } if s == "return 1;")),
        "{e}"
    );
    // Nothing moved: bytes, revision, no temp files.
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/lib.rs")).unwrap(),
        src
    );
    assert_eq!(ws.revision().number, rev0);
    assert!(
        std::fs::read_dir(root.path().join("src"))
            .unwrap()
            .all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".modbit-tmp"))
    );
    // A unique target through the whitespace tier applies, with the tier reported.
    let (change, tiers) = ws
        .edit_by_match(
            "src/lib.rs",
            &[edit("fn b() {\nreturn 1;\n}", "fn b() {\n    return 2;\n}")],
            pre,
        )
        .unwrap();
    assert_eq!(tiers, vec![MatchTier::WhitespaceRemap]);
    assert_eq!(change.op, "edit");
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/lib.rs")).unwrap(),
        "fn a() {\n    return 1;\n}\nfn b() {\n    return 2;\n}\n"
    );
    assert_eq!(ws.revision().number, rev0 + 1);
}

/// QUAL-EV-0016: ordered ops with per-step validation; an injected failure at
/// step N restores every earlier path byte-for-byte and reports the step.
#[test]
fn qual_ev_0016_failed_step_rolls_back_earlier_steps_and_reports_the_step() {
    let (root, state) = setup();
    let mut ws = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    let main0 = std::fs::read(root.path().join("src/main.rs")).unwrap();
    let readme0 = std::fs::read(root.path().join("README.md")).unwrap();
    let pre = WritePrecondition::default;
    let stale = WritePrecondition {
        expected_content_hash: Some(content_hash(b"not the current content")),
        ..WritePrecondition::default()
    };
    let ops = vec![
        ChangeOp {
            path: "src/new.rs".into(),
            kind: ChangeOpKind::Create(b"pub fn n() {}\n".to_vec()),
            pre: pre(),
        },
        ChangeOp {
            path: "src/main.rs".into(),
            kind: ChangeOpKind::Edit(vec![edit("fn main() {}", "fn main() { n(); }")]),
            pre: pre(),
        },
        ChangeOp {
            path: "README.md".into(),
            kind: ChangeOpKind::Delete,
            pre: pre(),
        },
        // Injected failure: a stale precondition at step 3.
        ChangeOp {
            path: "src/lib.rs".into(),
            kind: ChangeOpKind::Create(b"x".to_vec()),
            pre: stale,
        },
    ];
    let e = ws.apply_transaction(&ops).unwrap_err();
    let Error::StepFailed {
        step,
        rolled_back,
        restored,
        unrestored,
        cause,
    } = e
    else {
        panic!("{e}");
    };
    assert_eq!((step, rolled_back), (3, true), "{cause}");
    assert!(matches!(*cause, Error::Precondition { .. }), "{cause}");
    let mut r = restored.clone();
    r.sort();
    assert_eq!(r, vec!["README.md", "src/main.rs", "src/new.rs"]);
    assert!(unrestored.is_empty());
    assert!(!root.path().join("src/new.rs").exists());
    assert_eq!(
        std::fs::read(root.path().join("src/main.rs")).unwrap(),
        main0
    );
    assert_eq!(
        std::fs::read(root.path().join("README.md")).unwrap(),
        readme0
    );
    assert!(!root.path().join("src/lib.rs").exists());
    // The same ops without the poisoned step succeed and report every change in order.
    let changes = ws.apply_transaction(&ops[..3]).unwrap();
    assert_eq!(
        changes
            .iter()
            .map(|c| (c.path.as_str(), c.op.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("src/new.rs", "create"),
            ("src/main.rs", "edit"),
            ("README.md", "delete")
        ]
    );
    assert!(
        changes
            .windows(2)
            .all(|w| w[0].workspace_revision.number < w[1].workspace_revision.number)
    );
    assert!(!root.path().join("README.md").exists());
}

/// PX-026 Tier C conformance (docs/76): edits and replacements never corrupt
/// a file's line endings, and a non-UTF-8 file refuses a text edit.
#[test]
fn edits_preserve_crlf_and_refuse_non_utf8() {
    use modbit_workspace::{LineEnding, TextEdit};
    let (root, state) = setup();
    std::fs::write(root.path().join("win.txt"), "alpha\r\nbeta\r\ngamma\r\n").unwrap();
    std::fs::write(root.path().join("bin.dat"), [0xff, 0xfe, 0x00, b'x']).unwrap();
    let mut ws = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    // An LF snippet spliced into a CRLF file takes CRLF.
    ws.edit_by_match(
        "win.txt",
        &[TextEdit {
            old: "beta".into(),
            new: "beta\nbeta2".into(),
        }],
        WritePrecondition::default(),
    )
    .unwrap();
    let after = std::fs::read_to_string(root.path().join("win.txt")).unwrap();
    assert_eq!(after, "alpha\r\nbeta\r\nbeta2\r\ngamma\r\n");
    assert_eq!(LineEnding::of(&after), LineEnding::Crlf);
    // A whole-file replacement written with LF keeps the file's CRLF style.
    ws.atomic_replace("win.txt", b"one\ntwo\n", WritePrecondition::default())
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("win.txt")).unwrap(),
        "one\r\ntwo\r\n"
    );
    // An LF file stays LF even when the replacement carries CRLF.
    ws.atomic_replace(
        "README.md",
        b"# demo\r\nmore\r\n",
        WritePrecondition::default(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("README.md")).unwrap(),
        "# demo\nmore\n"
    );
    // Non-UTF-8 bytes are never edited as text.
    let err = ws
        .edit_by_match(
            "bin.dat",
            &[TextEdit {
                old: "x".into(),
                new: "y".into(),
            }],
            WritePrecondition::default(),
        )
        .unwrap_err();
    assert!(err.to_string().contains("UTF-8"), "{err}");
    assert_eq!(
        std::fs::read(root.path().join("bin.dat")).unwrap(),
        [0xff, 0xfe, 0x00, b'x']
    );
    assert_eq!(LineEnding::of("a\r\nb\n"), LineEnding::Undecided);
}
