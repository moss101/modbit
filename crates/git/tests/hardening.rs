//! FIX-01 (research/audit): git hardening against a real repository.
//!
//! A repository is data. These tests build repositories that carry hooks, an
//! fsmonitor command, clean/smudge filters, a merge driver, a textconv and a
//! signing program that would each write a sentinel file if git ran them, plus
//! option-shaped revisions that would write a file through `--output=`. Each
//! attack is first run through plain `git` (the control: the sentinel DOES
//! appear, so the attack is live on this machine), then through `modbit-git`
//! (nothing may run, nothing may be written).

// The hook/filter/merge-driver traps are shell scripts: Unix only.
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::path::{Path, PathBuf};
use std::process::Command;

use modbit_git::{Error, MergeState, Repo};

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

/// Plain `git`, with none of the crate's defences: the attacker's view.
fn plain(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
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
    write(&repo.dir().join("src/lib.rs"), "pub fn a() {}\n");
    write(&repo.dir().join("README.md"), "# demo\n");
    repo.commit("base", &["."]).unwrap();
    (dir, repo)
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Status parses names git would quote or that contain separators: spaces, a
/// rename (both names with spaces), non-ASCII and (on Unix) a double quote.
#[test]
fn status_z_round_trips_spaces_renames_quotes_and_unicode() {
    let (_d, repo) = seed();
    write(&repo.dir().join("a b.txt"), "one\n");
    write(&repo.dir().join("old name.txt"), "stay\n");
    write(&repo.dir().join("dir with space/f g.txt"), "x\n");
    repo.commit("more", &["."]).unwrap();
    // Dirty: modify the spaced file, rename another (staged), add unicode and
    // quote-bearing untracked files.
    write(&repo.dir().join("a b.txt"), "two\n");
    plain_ok(repo.dir(), &["mv", "old name.txt", "new name.txt"]);
    write(&repo.dir().join("dir with space/f g.txt"), "y\n");
    write(&repo.dir().join("caf\u{e9}.txt"), "u\n");
    #[cfg(unix)]
    write(&repo.dir().join("q\"uote.txt"), "q\n");
    let got: Vec<(String, String)> = repo
        .status()
        .unwrap()
        .into_iter()
        .map(|e| (e.path, e.code))
        .collect();
    let mut want: Vec<(String, String)> = vec![
        ("a b.txt".into(), ".M".into()),
        ("caf\u{e9}.txt".into(), "??".into()),
        ("dir with space/f g.txt".into(), ".M".into()),
        // A rename is reported as the two paths whose state changed, so a
        // consumer that restores per path sees the deletion too.
        ("new name.txt".into(), "A.".into()),
        ("old name.txt".into(), "D.".into()),
    ];
    #[cfg(unix)]
    want.push(("q\"uote.txt".into(), "??".into()));
    want.sort();
    let mut got = got;
    got.sort();
    assert_eq!(got, want);
}

#[test]
fn option_shaped_revisions_are_refused_before_git_runs_and_write_nothing() {
    let (dir, repo) = seed();
    let evidence = dir.path().join("evidence");
    std::fs::create_dir_all(&evidence).unwrap();
    let victim = evidence.join("pwned");
    let opt = format!("--output={}", victim.display());
    for (base, target) in [
        (opt.as_str(), "HEAD"),
        (opt.as_str(), ""),
        ("HEAD", opt.as_str()),
        ("-p", "HEAD"),
        ("HEAD", "a b"),
        ("main..HEAD", "HEAD"),
        ("HEAD", "main...HEAD"),
        ("HEAD\n--output=x", "HEAD"),
    ] {
        let err = repo.diff(base, target).unwrap_err();
        assert!(
            matches!(err, Error::InvalidRef { .. }),
            "{base:?} {target:?}: {err}"
        );
    }
    assert!(repo.show("--output=x", "README.md").is_err());
    assert!(repo.rev_parse(&opt).is_err());
    assert!(!repo.ref_exists(&opt));
    assert!(repo.merge_base("HEAD", &opt).is_err());
    assert!(repo.merge_begin("t", &opt).is_err());
    assert!(repo.create_branch("-f", "HEAD").is_err());
    assert!(repo.create_branch("ok", &opt).is_err());
    assert!(repo.set_branch("--output=x", "HEAD").is_err());
    assert!(repo.worktree_add(&dir.path().join("w"), &opt).is_err());
    assert!(
        repo.remote_branch_head("--upload-pack=touch x", "main")
            .is_err()
    );
    assert!(
        repo.push_branch("--receive-pack=touch x", "main", false)
            .is_err()
    );
    assert_eq!(names(&evidence), Vec::<String>::new());
    // A valid but non-existent revision is a git error, not an injection.
    assert!(matches!(
        repo.diff("no-such-rev", "HEAD").unwrap_err(),
        Error::Git { .. }
    ));
    // Valid inputs still work, including ancestry syntax.
    write(&repo.dir().join("README.md"), "# changed\n");
    let c2 = repo.commit("two", &["."]).unwrap();
    let d = repo.diff("HEAD~1", &c2).unwrap();
    assert_eq!(d.files.len(), 1);
    assert_eq!(d.files[0].path, "README.md");
    assert_eq!(repo.show("HEAD~1", "README.md").unwrap(), b"# demo\n");
}

#[test]
fn branch_names_must_be_valid_refs() {
    let (_d, repo) = seed();
    let head = repo.head().unwrap();
    for bad in [
        "", "-x", "a b", "a..b", "x.lock", "a~b", "a:b", "a\\b", "@{", "a//b",
    ] {
        assert!(
            repo.create_branch(bad, &head).is_err(),
            "branch {bad:?} must be refused"
        );
    }
    repo.create_branch("feature/ok-1", &head).unwrap();
    assert!(Repo::init(&_d.path().join("other"), "--bare").is_err());
}

/// Credentials embedded in a remote URL never appear in an error.
#[test]
fn remote_url_credentials_are_redacted_from_errors() {
    let (_d, repo) = seed();
    let url = "https://bob:ghp_TOPSECRET123@127.0.0.1:1/owner/repo.git";
    plain_ok(repo.dir(), &["remote", "add", "origin", url]);
    // The raw spelling stays available to the code that parses it.
    assert!(
        repo.remote_url("origin")
            .unwrap()
            .contains("ghp_TOPSECRET123")
    );
    let err = repo.remote_branch_head("origin", "main").unwrap_err();
    let text = format!("{err} {err:?}");
    assert!(!text.contains("ghp_TOPSECRET123"), "{text}");
    assert!(!text.contains("bob:"), "{text}");
    let err = repo.push_branch("origin", "main", false).unwrap_err();
    let text = format!("{err} {err:?}");
    assert!(!text.contains("ghp_TOPSECRET123"), "{text}");
    // A remote that is not configured is refused rather than treated as a URL.
    let err = repo
        .remote_branch_head(url, "main")
        .expect_err("a URL is not a configured remote");
    let text = format!("{err} {err:?}");
    assert!(!text.contains("ghp_TOPSECRET123"), "{text}");
}

struct Trap {
    dir: PathBuf,
}

impl Trap {
    fn sentinels(&self) -> Vec<String> {
        names(&self.dir)
            .into_iter()
            .filter(|n| n.starts_with("RAN_"))
            .collect()
    }
}

/// Install every repository-owned executable the audit names, each writing
/// `RAN_<what>` into `trap`.
#[cfg(unix)]
fn arm(repo: &Repo, trap: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let t = trap.display();
    let script = |name: &str, body: &str| -> PathBuf {
        let p = trap.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    };
    let hooks = repo.dir().join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for h in [
        "pre-commit",
        "commit-msg",
        "post-commit",
        "post-checkout",
        "post-merge",
        "pre-merge-commit",
        "reference-transaction",
        "pre-push",
    ] {
        let p = hooks.join(h);
        std::fs::write(&p, format!("#!/bin/sh\ntouch '{t}/RAN_hook_{h}'\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let fsmon = script(
        "fsmon.sh",
        &format!("touch '{t}/RAN_fsmonitor'\nprintf '\\0'"),
    );
    let gpg = script("gpg.sh", &format!("touch '{t}/RAN_gpg'\nexit 1"));
    let cfg = |k: &str, v: &str| plain_ok(repo.dir(), &["config", k, v]);
    cfg("core.fsmonitor", &fsmon.display().to_string());
    cfg("commit.gpgsign", "true");
    cfg("gpg.program", &gpg.display().to_string());
    cfg(
        "filter.evil.clean",
        &format!("touch '{t}/RAN_filter_clean'; cat"),
    );
    cfg(
        "filter.evil.smudge",
        &format!("touch '{t}/RAN_filter_smudge'; cat"),
    );
    cfg(
        "diff.evil.textconv",
        &format!("touch '{t}/RAN_textconv'; cat"),
    );
    cfg(
        "diff.evil.command",
        &format!("touch '{t}/RAN_extdiff'; true"),
    );
    cfg(
        "merge.evil.driver",
        &format!("touch '{t}/RAN_mergedriver'; cp %A %A.merged"),
    );
    write(
        &repo.dir().join(".git/info/attributes"),
        "*.evil filter=evil diff=evil merge=evil\n",
    );
}

/// The control: with plain git the armed repository really does run its
/// programs on this machine, so the negative results below mean something.
#[cfg(unix)]
#[test]
fn control_plain_git_runs_the_repository_programs() {
    let (d, repo) = seed();
    let trap = Trap {
        dir: d.path().join("trap"),
    };
    std::fs::create_dir_all(&trap.dir).unwrap();
    arm(&repo, &trap.dir);
    write(&repo.dir().join("x.evil"), "data\n");
    plain(repo.dir(), &["status", "--porcelain"]);
    plain(repo.dir(), &["add", "-A"]);
    plain(
        repo.dir(),
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "x"],
    );
    // Signing (the program fails, the sentinel stays).
    plain(
        repo.dir(),
        &["commit", "-q", "--allow-empty", "-m", "signed"],
    );
    // Worktree add (post-checkout, smudge) on a branch, then a conflicting
    // add/add of an `*.evil` file merged (merge driver), and a diff
    // (textconv / external diff).
    plain_ok(repo.dir(), &["branch", "feature", "HEAD~1"]);
    let wt = d.path().join("wt");
    plain_ok(
        repo.dir(),
        &[
            "worktree",
            "add",
            "-q",
            &wt.display().to_string(),
            "feature",
        ],
    );
    write(&wt.join("x.evil"), "feature side\n");
    plain_ok(&wt, &["add", "-A"]);
    plain_ok(
        &wt,
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "f"],
    );
    write(&repo.dir().join("x.evil"), "main side\n");
    plain_ok(repo.dir(), &["add", "-A"]);
    plain_ok(
        repo.dir(),
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "m"],
    );
    plain(repo.dir(), &["merge", "--no-commit", "--no-ff", "feature"]);
    plain(repo.dir(), &["merge", "--abort"]);
    write(&repo.dir().join("x.evil"), "dirty\n");
    plain(repo.dir(), &["diff", "--ext-diff", "--textconv", "HEAD"]);
    let ran = trap.sentinels();
    for expect in [
        "RAN_fsmonitor",
        "RAN_filter_clean",
        "RAN_filter_smudge",
        "RAN_hook_pre-commit",
        "RAN_hook_post-commit",
        "RAN_hook_post-checkout",
        "RAN_gpg",
        "RAN_mergedriver",
        "RAN_extdiff",
    ] {
        assert!(
            ran.iter().any(|n| n == expect),
            "control: plain git must run {expect}; ran {ran:?}"
        );
    }
}

/// Every typed operation on an armed repository: nothing runs.
#[cfg(unix)]
#[test]
fn armed_repository_runs_no_hook_fsmonitor_filter_driver_textconv_or_signer() {
    let (d, repo) = seed();
    let trap = Trap {
        dir: d.path().join("trap"),
    };
    std::fs::create_dir_all(&trap.dir).unwrap();
    arm(&repo, &trap.dir);

    // status / diff / commit / show / log / snapshot on a dirty armed tree.
    write(&repo.dir().join("x.evil"), "data\n");
    write(
        &repo.dir().join("src/lib.rs"),
        "pub fn a() {}\npub fn b() {}\n",
    );
    let clean = |step: &str| {
        assert_eq!(
            trap.sentinels(),
            Vec::<String>::new(),
            "a repository-owned program ran during `{step}`"
        );
    };
    repo.status().unwrap();
    clean("status");
    repo.diff_worktree().unwrap();
    clean("diff_worktree");
    let snap = repo.snapshot_dirty("armed").unwrap();
    repo.snapshot_cleanup(&snap).unwrap();
    clean("snapshot_dirty");
    let c1 = repo.commit("armed one", &["."]).unwrap();
    clean("commit");
    let base = repo.rev_parse("HEAD~1").unwrap();
    repo.diff(&base, &c1).unwrap();
    clean("diff");
    repo.show(&c1, "x.evil").unwrap();
    clean("show");
    repo.log_recent(5).unwrap();
    clean("log_recent");

    // Branch + worktree add (post-checkout, smudge), then a merge with a
    // conflicting change on both sides of an `*.evil` file (merge driver).
    repo.create_branch("feature", &base).unwrap();
    clean("create_branch");
    let wt = repo.worktree_add(&d.path().join("wt"), "feature").unwrap();
    clean("worktree_add");
    write(&wt.dir().join("x.evil"), "feature side\n");
    let feat = wt.commit("feature", &["."]).unwrap();
    clean("commit in worktree");
    write(&repo.dir().join("x.evil"), "main side\n");
    repo.commit("main side", &["."]).unwrap();
    clean("commit main side");
    let mut tx = repo.merge_begin("armed-merge", &feat).unwrap();
    clean("merge_begin");
    assert!(
        matches!(tx.state, MergeState::Conflicted | MergeState::Staged),
        "{:?}",
        tx.state
    );
    repo.merge_abort(&mut tx).unwrap();
    clean("merge_abort");
    repo.worktree_remove(wt.dir()).unwrap();
    clean("worktree_remove");
}
