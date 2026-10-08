//! PX-067, the scope FIX-01 left: the bare-repository safeguard, a
//! repository's own `core.sshCommand` and `credential.helper`, configuration
//! `include`s, attributes files, and the report of what was neutralised.
//!
//! Each attack is first run through plain git (the control: the sentinel DOES
//! appear on this machine), then through `modbit-git` (it may not).

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::path::{Path, PathBuf};
use std::process::Command;

use modbit_git::{Error, Repo};

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

fn plain(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e")
        .output()
        .unwrap()
}

fn plain_ok(dir: &Path, args: &[&str]) {
    let out = plain(dir, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn seed() -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repo::init(&dir.path().join("repo"), "main").unwrap();
    write(&repo.dir().join("README.md"), "# demo\n");
    repo.commit("base", &["."]).unwrap();
    (dir, repo)
}

#[cfg(unix)]
fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn ran(trap: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(trap)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("RAN_"))
        .collect();
    v.sort();
    v
}

/// A bare repository planted inside a working tree is not entered by
/// discovery. The control proves it is a perfectly good repository to plain
/// git (with git's default `safe.bareRepository=all`).
#[test]
fn an_embedded_bare_repository_is_never_entered() {
    let (d, repo) = seed();
    let evil = repo.dir().join("vendor/evil.git");
    std::fs::create_dir_all(&evil).unwrap();
    plain_ok(&evil, &["init", "-q", "--bare"]);
    let control = Command::new("git")
        .arg("-C")
        .arg(&evil)
        .args(["rev-parse", "--is-bare-repository"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", d.path().join("empty-global"))
        .output()
        .unwrap();
    assert!(
        control.status.success() && String::from_utf8_lossy(&control.stdout).trim() == "true",
        "control: plain git enters the embedded bare repository"
    );
    let err = Repo::open(&evil).unwrap_err();
    assert!(
        matches!(err, Error::NotARepository(_)),
        "an embedded bare repository must not open: {err:?}"
    );
    // The enclosing worktree is unaffected.
    assert!(Repo::open(repo.dir()).is_ok());
}

/// A repository-local `core.sshCommand` is the first thing git runs for an
/// ssh remote. Plain git runs it; the runner blanks it, so the command fails
/// closed and the sentinel never appears.
#[cfg(unix)]
#[test]
fn a_repository_ssh_command_never_runs() {
    let (d, repo) = seed();
    let trap = d.path().join("trap");
    std::fs::create_dir_all(&trap).unwrap();
    let ssh = d.path().join("evil-ssh.sh");
    script(&ssh, &format!("touch '{}/RAN_ssh'\nexit 1", trap.display()));
    plain_ok(
        repo.dir(),
        &[
            "remote",
            "add",
            "origin",
            "ssh://git@invalid.example/o/r.git",
        ],
    );
    plain_ok(
        repo.dir(),
        &["config", "core.sshCommand", &ssh.display().to_string()],
    );
    // Control.
    let _ = plain(repo.dir(), &["ls-remote", "origin"]);
    assert_eq!(
        ran(&trap),
        vec!["RAN_ssh".to_owned()],
        "control: plain git runs it"
    );
    std::fs::remove_file(trap.join("RAN_ssh")).unwrap();

    assert!(repo.remote_branch_head("origin", "main").is_err());
    assert!(repo.push_branch("origin", "main", false).is_err());
    assert_eq!(
        ran(&trap),
        Vec::<String>::new(),
        "the runner ran the ssh command"
    );
    let n = repo.neutralized().unwrap();
    assert!(
        n.iter()
            .any(|x| x.kind == "SSH_COMMAND" && x.name == "core.sshcommand" && x.scope == "local"),
        "{n:?}"
    );
}

/// A repository-local `credential.helper` runs when the server asks for
/// credentials. A tiny HTTP server asks (401); plain git calls the helper,
/// the runner does not.
#[cfg(unix)]
#[test]
fn a_repository_credential_helper_never_runs() {
    use std::io::{Read, Write};
    let (d, repo) = seed();
    let trap = d.path().join("trap");
    std::fs::create_dir_all(&trap).unwrap();
    let helper = d.path().join("evil-helper.sh");
    script(
        &helper,
        &format!(
            "touch '{}/RAN_credential_helper'\ncat >/dev/null\nexit 1",
            trap.display()
        ),
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(40) {
            let Ok(mut s) = stream else { return };
            let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(500)));
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let _ = s.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"x\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    let url = format!("http://127.0.0.1:{port}/o/r.git");
    plain_ok(repo.dir(), &["remote", "add", "origin", &url]);
    plain_ok(
        repo.dir(),
        &[
            "config",
            "credential.helper",
            &format!("!{}", helper.display()),
        ],
    );
    // A URL-scoped helper too.
    plain_ok(
        repo.dir(),
        &[
            "config",
            &format!("credential.http://127.0.0.1:{port}.helper"),
            &format!("!{}", helper.display()),
        ],
    );
    let _ = plain(repo.dir(), &["ls-remote", "origin"]);
    assert_eq!(
        ran(&trap),
        vec!["RAN_credential_helper".to_owned()],
        "control: plain git runs the helper"
    );
    std::fs::remove_file(trap.join("RAN_credential_helper")).unwrap();

    assert!(repo.remote_branch_head("origin", "main").is_err());
    assert_eq!(
        ran(&trap),
        Vec::<String>::new(),
        "the runner ran a repository credential helper"
    );
    let kinds: Vec<String> = repo
        .neutralized()
        .unwrap()
        .into_iter()
        .filter(|n| n.kind == "CREDENTIAL_HELPER")
        .map(|n| n.name)
        .collect();
    assert_eq!(kinds.len(), 2, "{kinds:?}");
}

/// A filter defined in a file the repository's config `include`s, selected by
/// a committed `.gitattributes` or by `core.attributesFile`: still the
/// repository's, still blanked.
#[cfg(unix)]
#[test]
fn filters_reached_through_includes_and_attributes_files_never_run() {
    let (d, repo) = seed();
    let trap = d.path().join("trap");
    std::fs::create_dir_all(&trap).unwrap();
    let t = trap.display();
    // The filter lives in an included file; the attribute selecting it in a
    // committed .gitattributes and, separately, in an attributes file the
    // repository's config names.
    write(
        &repo.dir().join(".git/evil.cfg"),
        &format!(
            "[filter \"evil\"]\n\tclean = touch '{t}/RAN_include_clean'; cat\n\tsmudge = touch '{t}/RAN_include_smudge'; cat\n[core]\n\tfsmonitor = false\n"
        ),
    );
    plain_ok(repo.dir(), &["config", "include.path", "evil.cfg"]);
    let attrs = d.path().join("evil-attributes");
    write(&attrs, "*.viaconfig filter=evil\n");
    plain_ok(
        repo.dir(),
        &[
            "config",
            "core.attributesFile",
            &attrs.display().to_string(),
        ],
    );
    write(&repo.dir().join(".gitattributes"), "*.evil filter=evil\n");
    plain_ok(repo.dir(), &["add", ".gitattributes"]);
    plain_ok(repo.dir(), &["commit", "-q", "-m", "attrs"]);
    write(&repo.dir().join("x.evil"), "data\n");
    write(&repo.dir().join("y.viaconfig"), "data\n");
    // Control.
    plain_ok(repo.dir(), &["add", "-A"]);
    assert!(
        ran(&trap).contains(&"RAN_include_clean".to_owned()),
        "control: plain git runs the included filter: {:?}",
        ran(&trap)
    );
    plain_ok(repo.dir(), &["reset", "-q"]);
    for e in std::fs::read_dir(&trap).unwrap().flatten() {
        std::fs::remove_file(e.path()).unwrap();
    }

    repo.status().unwrap();
    repo.diff_worktree().unwrap();
    let snap = repo.snapshot_dirty("inc").unwrap();
    repo.snapshot_cleanup(&snap).unwrap();
    repo.commit("with filters configured", &["."]).unwrap();
    assert_eq!(
        ran(&trap),
        Vec::<String>::new(),
        "a filter reached through include.path ran"
    );
    let n = repo.neutralized().unwrap();
    assert!(
        n.iter()
            .any(|x| x.kind == "FILTER" && x.name == "filter.evil.clean" && x.scope == "local"),
        "an included file has the repository's scope: {n:?}"
    );
}

/// What was neutralised is reported, so the Core can record it; a clean
/// repository reports nothing, and the user's own global configuration is not
/// reported or blanked.
#[cfg(unix)]
#[test]
fn neutralized_reports_exactly_what_the_repository_asked_to_run() {
    let (d, repo) = seed();
    assert_eq!(repo.neutralized().unwrap(), vec![]);
    let hooks = repo.dir().join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    script(&hooks.join("pre-commit"), "exit 0");
    script(&hooks.join("post-merge"), "exit 0");
    write(&hooks.join("pre-push.sample"), "#!/bin/sh\n");
    let cfg = |k: &str, v: &str| plain_ok(repo.dir(), &["config", k, v]);
    cfg("core.fsmonitor", "/bin/true");
    cfg("filter.a.clean", "cat");
    cfg("merge.m.driver", "true");
    cfg("diff.t.textconv", "cat");
    cfg("gpg.program", "/bin/false");
    cfg("user.name", "harmless");
    let mut got: Vec<(String, String)> = repo
        .neutralized()
        .unwrap()
        .into_iter()
        .map(|n| (n.kind, n.name))
        .collect();
    got.sort();
    let mut want: Vec<(String, String)> = [
        ("FILTER", "filter.a.clean"),
        ("FSMONITOR", "core.fsmonitor"),
        ("HOOK", "post-merge"),
        ("HOOK", "pre-commit"),
        ("MERGE_DRIVER", "merge.m.driver"),
        ("SIGNING_PROGRAM", "gpg.program"),
        ("TEXTCONV", "diff.t.textconv"),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
    .collect();
    want.sort();
    assert_eq!(got, want);
    drop(d);
}

/// Cheap property checks that need no repository.
#[test]
fn the_ownership_refusal_has_its_own_type_and_is_never_overridden() {
    let e = Error::UnsafeRepository("/srv/other-user/repo".into());
    let text = e.to_string();
    assert!(
        text.contains("safe.directory") && text.contains("does not override"),
        "{text}"
    );
    let _: Option<PathBuf> = None;
}
