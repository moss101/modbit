//! WIN-03: NTFS path-spelling attacks against the workspace path policy
//! (docs/20, docs/23, docs/52). Every case names something real on real NTFS
//! (Developer Mode on for unprivileged link creation) and then asks the
//! policy what it thinks. The expectation is the security property; the
//! first run of each case that fails records an escape (PX-030).

#![cfg(windows)]

use modbit_workspace::{Error, PathPolicy};

fn root() -> (tempfile::TempDir, PathPolicy) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn a() {}\n").unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::write(dir.path().join(".git/config"), "[core]\n").unwrap();
    let policy = PathPolicy::new(dir.path(), &[]).unwrap();
    (dir, policy)
}

/// The short (8.3) form of a path, if this volume mints one.
fn short_form(path: &std::path::Path) -> Option<String> {
    let out = std::process::Command::new("cmd")
        .args(["/c", "for %I in (\"path\") do @echo %~sI"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?.trim().to_owned();
    let long = path.to_string_lossy().to_ascii_lowercase();
    if line.to_ascii_lowercase().replace('\\', "/") == long.replace('\\', "/") {
        return None; // no short form minted
    }
    Some(line)
}

fn junction(target: &std::path::Path, at: &std::path::Path) {
    let out = std::process::Command::new("cmd")
        .args([
            "/c",
            "mklink",
            "/J",
            &at.display().to_string(),
            &target.display().to_string(),
        ])
        .output()
        .expect("mklink runs");
    assert!(
        out.status.success(),
        "junction {} -> {}: {}",
        at.display(),
        target.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(at.exists(), "the junction exists");
}

#[test]
fn a_protected_path_is_denied_in_every_casing() {
    let (d, p) = root();
    for spelling in [
        ".git/config",
        ".GIT/config",
        ".Git/config",
        "src/../.GIT/config",
    ] {
        match p.check(spelling) {
            Err(Error::Protected { .. }) => {}
            other => panic!("{spelling}: expected Protected, got {other:?}"),
        }
    }
    drop(d);
}

#[test]
fn a_junction_out_of_the_root_is_never_followed() {
    let (d, p) = root();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("evil.txt"), "OUTSIDE\n").unwrap();
    junction(outside.path(), &d.path().join("junc"));
    assert!(
        d.path().join("junc/evil.txt").exists(),
        "control: the junction really reaches out"
    );
    match p.check("junc/evil.txt") {
        Err(Error::OutsideRoot { .. }) => {}
        other => panic!("junction escape: expected OutsideRoot, got {other:?}"),
    }
    // A junction nested deeper must be caught on the hop too.
    std::fs::create_dir_all(d.path().join("src").join("deep")).unwrap();
    junction(
        outside.path(),
        &d.path().join("src").join("deep").join("junc"),
    );
    match p.check("src/deep/junc/evil.txt") {
        Err(Error::OutsideRoot { .. }) => {}
        other => panic!("nested junction escape: expected OutsideRoot, got {other:?}"),
    }
}

#[test]
fn an_8_3_short_name_of_the_root_still_reaches_only_the_root() {
    let (d, p) = root();
    let Some(short_root) = short_form(d.path()) else {
        return; // this volume mints no short names; nothing to prove here
    };
    // The protected file reached through the short spelling of the root:
    // any refusal (the spelling may also simply not match the canonical
    // root) is the property; an Ok would be an escape.
    let spelling = format!("{short_root}\\.git\\config");
    match p.check(&spelling) {
        Err(Error::Protected { .. }) | Err(Error::OutsideRoot { .. }) => {}
        other => panic!("short-root spelling {spelling}: expected refusal, got {other:?}"),
    }
    // A short name inside the root is still inside the root.
    drop(d);
}

#[test]
fn reserved_device_names_are_refused() {
    let (d, p) = root();
    for spelling in [
        "NUL", "CON", "PRN", "AUX", "COM1", "LPT1", "nul.txt", "con.txt",
    ] {
        match p.check(spelling) {
            Err(Error::Invalid { .. }) | Err(Error::OutsideRoot { .. }) => {}
            other => panic!("{spelling}: expected refusal (device name), got {other:?}"),
        }
    }
    drop(d);
}

#[test]
fn trailing_dots_and_spaces_are_refused() {
    let (d, p) = root();
    for spelling in ["file.", "file ", "src/lib.rs.", "src/lib.rs "] {
        match p.check(spelling) {
            Err(Error::Invalid { .. }) | Err(Error::OutsideRoot { .. }) => {}
            other => panic!("{spelling:?}: expected refusal (trailing dot/space), got {other:?}"),
        }
    }
    drop(d);
}

#[test]
fn alternate_data_streams_are_refused() {
    let (d, p) = root();
    std::fs::write(d.path().join("data.txt"), "DATA\n").unwrap();
    for spelling in ["data.txt:stream", "data.txt::$DATA", "src/lib.rs:hidden"] {
        match p.check(spelling) {
            Err(Error::Invalid { .. }) | Err(Error::OutsideRoot { .. }) => {}
            other => panic!("{spelling:?}: expected refusal (ADS), got {other:?}"),
        }
    }
    drop(d);
}

#[test]
fn absolute_forms_outside_the_root_are_refused_whatever_their_spelling() {
    let (d, p) = root();
    let outside = tempfile::tempdir().unwrap();
    let host_root = std::path::Path::new(r"C:\");
    for spelling in [
        format!(r"\\?\{}\x.txt", outside.path().display()),
        format!(r"{}\x.txt", outside.path().display()),
        r"\\localhost\c$\windows\x.txt".to_owned(),
        format!(
            r"\\?\UNC\localhost\c$\{}\x.txt",
            outside
                .path()
                .strip_prefix(host_root)
                .unwrap_or(outside.path())
                .display()
        ),
    ] {
        match p.check(&spelling) {
            Err(Error::OutsideRoot { .. }) => {}
            other => panic!("{spelling}: expected OutsideRoot, got {other:?}"),
        }
    }
    drop(d);
}

#[test]
fn a_drive_relative_path_is_refused() {
    let (d, p) = root();
    for spelling in ["C:src/lib.rs", "c:../outside"] {
        match p.check(spelling) {
            Err(Error::OutsideRoot { .. }) | Err(Error::Invalid { .. }) => {}
            other => panic!("{spelling}: expected refusal (drive-relative), got {other:?}"),
        }
    }
    drop(d);
}

#[test]
fn ordinary_spellings_still_work() {
    let (d, p) = root();
    for spelling in [
        "src/lib.rs",
        "SRC/LIB.RS",
        "src\\lib.rs",
        "src/./lib.rs",
        "src/../src/lib.rs",
        "notes/today.txt",
    ] {
        p.check(spelling)
            .unwrap_or_else(|e| panic!("{spelling}: {e:?}"));
    }
    // Absolute inside the root, plain and verbatim, is allowed. The root is
    // canonicalized first: std's tempdir can hand back an 8.3 spelling
    // (`RUNNER~1` on a CI runner) which is a *different* string than the
    // canonical root the policy holds, and an absolute spelling must name
    // the root as it canonically is.
    let root = d.path().canonicalize().unwrap();
    // canonicalize hands back the verbatim \\\\?\\ form; both spellings are built from the plain root.
    let root_text = root.display().to_string();
    let plain_root = root_text
        .strip_prefix("\\\\?\\")
        .unwrap_or(root_text.as_str());
    let plain = format!(r"{}\src\lib.rs", plain_root);
    p.check(&plain).expect("plain absolute inside root");
    let verbatim = format!(r"\\?\{}\src\lib.rs", plain_root);
    p.check(&verbatim).expect("verbatim absolute inside root");
}

/// A hard link to a protected file is invisible to a lexical policy (the
/// link's name is an ordinary in-root file; only its file ID betrays it, and
/// std has no API for that). The compensating control is the snapshot: the
/// protected file's CONTENT changes through the link, and the diff catches
/// and restores it. This test proves that control (WIN-03: documented gap at
/// check time, caught post-hoc).
#[test]
fn a_write_through_a_hard_link_to_a_protected_file_is_caught_by_the_snapshot() {
    use modbit_workspace::snapshot::{Limits, capture, diff, restore};
    let (d, p) = root();
    let link = d.path().join("notes.txt");
    let out = std::process::Command::new("cmd")
        .args([
            "/c",
            "mklink",
            "/H",
            &link.display().to_string(),
            &d.path().join(".git").join("config").display().to_string(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "hard link: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let before = capture(&p, Limits::default());
    // A process with an allowed path writes through the hard link.
    p.check("notes.txt").expect("an ordinary in-root name");
    std::fs::write(&link, "[core]\n\talien = true\n").unwrap();
    let after = capture(&p, Limits::default());
    let deltas = diff(&before, &after, &p);
    let hit = deltas
        .iter()
        .find(|x| x.path == ".git/config")
        .expect("the protected file's change is seen through the link");
    assert!(hit.protected_by.is_some(), "{hit:?}");
    let restored = restore(&before, &[hit]);
    assert!(restored.unrestored.is_empty(), "{restored:?}");
    assert_eq!(
        std::fs::read_to_string(d.path().join(".git").join("config")).unwrap(),
        "[core]\n"
    );
}
