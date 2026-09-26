//! M9.6 (docs/52 "Security gates": fuzzer for the path normalizer; "Path
//! traversal/symlink escape" and "TOCTOU capability bypass" attack tests).
//! For paths built from hostile segments — parent references, dot variants,
//! Unicode confusables, NUL, absolute and drive prefixes, both separators,
//! very long names — the policy either refuses or answers a path inside the
//! root; a real symbolic link to the outside is refused wherever it sits in
//! the path; protected names are refused under any prefix; and a target
//! swapped for an escaping link between two operations is refused at the
//! second, because every operation resolves anew.

use modbit_workspace::{Error, PathPolicy};
use proptest::prelude::*;
use std::path::PathBuf;

fn segment() -> impl Strategy<Value = String> {
    prop_oneof![
        8 => prop::sample::select(vec![
            "..", ".", "...", "src", "a b", "é", "e\u{301}", "\u{FF0E}\u{FF0E}", "..\u{2215}", "x\u{0}y",
            "src", "main.rs", "README.md", "link", "linkdir", ".git", ".env", ".ssh", "id_rsa", "key.pem",
            "node_modules", "~", "C:", "\\\\?\\", "", "....", "..;", "%2e%2e", "\u{2024}\u{2024}",
        ])
        .prop_map(str::to_owned),
        2 => "[a-zA-Z0-9._-]{1,12}",
        1 => Just("z".repeat(300)),
    ]
}

fn hostile_path() -> impl Strategy<Value = String> {
    (
        prop::sample::select(vec!["", "/", "//", "\\", "./", "../", "~/"]),
        prop::collection::vec(segment(), 0..6),
        prop::sample::select(vec!["/", "\\", "//"]),
        prop::sample::select(vec!["", "/", "/."]),
    )
        .prop_map(|(lead, segs, sep, tail)| format!("{lead}{}{tail}", segs.join(sep)))
}

fn root_with_links() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.path().join("README.md"), "# demo\n").unwrap();
    std::fs::write(outside.path().join("secret.txt"), "top secret").unwrap();
    link(
        &outside.path().join("secret.txt"),
        &root.path().join("link"),
    );
    link(outside.path(), &root.path().join("linkdir"));
    let canonical = root.path().canonicalize().unwrap();
    (root, outside, canonical)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn a_hostile_path_is_refused_or_resolves_inside_the_root(p in hostile_path()) {
        let (root, _outside, canonical) = root_with_links();
        let policy = PathPolicy::new(root.path(), &[]).unwrap();
        match policy.check(&p) {
            Ok(resolved) => {
                // Found by this fuzzer: on Unix a backslash used to resolve to a
                // literal file name while the record said a forward-slash path.
                prop_assert!(cfg!(windows) || !p.contains('\\'), "{p:?} accepted on a platform where a backslash is not a separator");
                prop_assert!(resolved.absolute.starts_with(&canonical), "{p:?} -> {}", resolved.absolute.display());
                prop_assert!(!resolved.relative.split('/').any(|s| s == ".."), "{p:?} -> {}", resolved.relative);
                prop_assert!(!resolved.relative.starts_with('/'), "{p:?} -> {}", resolved.relative);
                // The planted links live at the root: a normalized path whose
                // first component is one of them escapes, wherever it goes next.
                let first = resolved.relative.split('/').next().unwrap_or("");
                let via_link = first == "link" || first == "linkdir";
                prop_assert!(!via_link, "an escaping link resolved: {p:?} -> {}", resolved.absolute.display());
            }
            Err(Error::OutsideRoot { .. } | Error::Protected { .. } | Error::Io { .. }) => {}
            Err(other) => prop_assert!(false, "{p:?}: unexpected {other}"),
        }
    }

    #[test]
    fn a_protected_name_is_refused_under_any_prefix(
        prefix in prop::collection::vec("[a-z]{1,8}", 0..4),
        name in prop::sample::select(vec![".env", ".env.local", ".git/config", ".ssh/id_rsa", "cert.pem", ".github/workflows/ci.yml"]),
    ) {
        let (root, _outside, _canonical) = root_with_links();
        let policy = PathPolicy::new(root.path(), &[]).unwrap();
        let mut parts = prefix.clone();
        parts.push(name.to_owned());
        let p = parts.join("/");
        let r = policy.check(&p);
        prop_assert!(matches!(r, Err(Error::Protected { .. })), "{p:?}: {r:?}");
    }
}

/// docs/52 "TOCTOU capability bypass": the target is a regular file when
/// first read and becomes a link to the outside before the write. Every
/// operation resolves the path anew, so the write is refused and nothing
/// outside the root changes.
#[test]
fn a_target_swapped_for_an_escaping_link_between_operations_is_refused() {
    use modbit_workspace::{WorkspaceService, WritePrecondition};
    let (root, outside, _canonical) = root_with_links();
    let state = tempfile::tempdir().unwrap();
    let mut ws = WorkspaceService::open(root.path(), state.path(), &[]).unwrap();
    std::fs::write(root.path().join("notes.txt"), "plain").unwrap();
    let first = ws.read("notes.txt").unwrap();
    assert_eq!(first.path, "notes.txt");
    // The race: the checked file is replaced by a link to a secret outside.
    std::fs::remove_file(root.path().join("notes.txt")).unwrap();
    link(
        &outside.path().join("secret.txt"),
        &root.path().join("notes.txt"),
    );
    let err = ws
        .atomic_replace("notes.txt", b"overwritten", WritePrecondition::default())
        .unwrap_err();
    assert!(matches!(err, Error::OutsideRoot { .. }), "{err}");
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
        "top secret",
        "the write did not follow the swapped link"
    );
    let err = ws.read("notes.txt").unwrap_err();
    assert!(matches!(err, Error::OutsideRoot { .. }), "{err}");
    // And a directory swapped for a link: the same.
    std::fs::create_dir_all(root.path().join("docs")).unwrap();
    std::fs::write(root.path().join("docs/a.md"), "a").unwrap();
    assert!(ws.read("docs/a.md").is_ok());
    std::fs::remove_dir_all(root.path().join("docs")).unwrap();
    link(outside.path(), &root.path().join("docs"));
    let err = ws
        .atomic_replace("docs/secret.txt", b"x", WritePrecondition::default())
        .unwrap_err();
    assert!(matches!(err, Error::OutsideRoot { .. }), "{err}");
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
        "top secret"
    );
}

/// A symbolic link on any platform: Windows needs to know a file link from a
/// directory link (PX-030: the path policy is conformance-tested on every OS).
fn link(target: &std::path::Path, at: &std::path::Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, at).unwrap();
    #[cfg(windows)]
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, at).unwrap();
    } else {
        std::os::windows::fs::symlink_file(target, at).unwrap();
    }
}
