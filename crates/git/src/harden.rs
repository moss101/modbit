//! The one place a `git` process is built (FIX-01, research/audit).
//!
//! Every Modbit git invocation goes through [`command`], so no call site can
//! forget a defence. A repository is *data*: its `.git/config`, hooks and
//! `.gitattributes` can name programs, and several of the commands Modbit runs
//! (status, diff, add, checkout, merge) would run them. This module:
//!
//! * points `core.hooksPath` at an empty location, so **no** hook runs for any
//!   Modbit-driven command (`pre-commit`/`commit-msg` were already skipped by
//!   `--no-verify`; `post-commit`, `post-checkout` from `worktree add`,
//!   `post-merge`, `reference-transaction` and `pre-push` are skipped now too).
//!   Decision: Modbit's commits are machine-made checkpoints, snapshots and
//!   review accepts; repository policy hooks are the user's own workflow and
//!   run when the user runs git themselves, never as a side effect of an
//!   agent's typed tool call;
//! * forces `core.fsmonitor=false` and a neutral `core.attributesFile`;
//! * blanks every repository-scoped `filter.*.{clean,smudge,process}`,
//!   `merge.*.driver` and `diff.*.{textconv,command}` program (read from the
//!   repository's own configuration, never from the user's global config, so a
//!   user-installed git-lfs filter keeps working);
//! * disables commit/tag/push signing and signature display (`gpg.program` is
//!   repository-settable), and external credential prompts;
//! * ignores the system gitconfig and inherited `GIT_DIR`-style overrides;
//! * validates every model- or user-supplied ref, branch and remote before it
//!   is placed on a command line, and redacts URL credentials from the text of
//!   any error.
//!
//! Remaining gap, by design and documented: a repository-local
//! `core.sshCommand` or `credential.helper` still runs on network commands
//! (push, ls-remote). Those are protected effects behind an approval.

use std::path::Path;
use std::process::Command;

use crate::{Error, Result};

/// A location git sees as "nothing here": `/dev/null` on Unix, a per-process
/// empty directory (and empty file inside it) on Windows, where `NUL` is not
/// accepted for `core.hooksPath`.
struct Neutral {
    hooks: String,
    attributes: String,
}

#[cfg(unix)]
fn neutral() -> Result<&'static Neutral> {
    static N: std::sync::OnceLock<Neutral> = std::sync::OnceLock::new();
    Ok(N.get_or_init(|| Neutral {
        hooks: "/dev/null".into(),
        attributes: "/dev/null".into(),
    }))
}

#[cfg(not(unix))]
fn neutral() -> Result<&'static Neutral> {
    static N: std::sync::OnceLock<Option<Neutral>> = std::sync::OnceLock::new();
    N.get_or_init(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("modbit-git-neutral-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir).ok()?;
        let attrs = dir.join("attributes");
        std::fs::write(&attrs, b"").ok()?;
        Some(Neutral {
            hooks: dir.join("hooks").to_string_lossy().replace('\\', "/"),
            attributes: attrs.to_string_lossy().replace('\\', "/"),
        })
    })
    .as_ref()
    .ok_or_else(|| {
        Error::Spawn(std::io::Error::other(
            "cannot create the neutral git config location",
        ))
    })
}

/// Subcommands that never touch the working tree or the index's file
/// content, so they cannot run a filter, merge driver or textconv; the
/// repository-config scan is skipped for them (it costs one process).
/// `write-tree`/`read-tree` are deliberately absent: a racily-clean index
/// entry makes `write-tree` re-hash the file through its clean filter.
fn is_plumbing(sub: &str) -> bool {
    matches!(
        sub,
        "rev-parse"
            | "config"
            | "symbolic-ref"
            | "update-ref"
            | "merge-base"
            | "ls-remote"
            | "check-ref-format"
            | "init"
            | "branch"
            | "remote"
            | "commit-tree"
            | "push"
    )
}

/// The first non-option word of `args` (the subcommand).
fn subcommand<'a>(args: &[&'a str]) -> &'a str {
    args.iter()
        .copied()
        .find(|a| !a.starts_with('-') && !a.contains('='))
        .unwrap_or("")
}

/// The fixed hardening configuration, as `-c key=value` pairs.
fn fixed_config(n: &Neutral) -> Vec<String> {
    vec![
        format!("core.hooksPath={}", n.hooks),
        "core.fsmonitor=false".into(),
        format!("core.attributesFile={}", n.attributes),
        "core.askPass=".into(),
        "core.quotePath=false".into(),
        "commit.gpgSign=false".into(),
        "tag.gpgSign=false".into(),
        "push.gpgSign=false".into(),
        "log.showSignature=false".into(),
        "merge.verifySignatures=false".into(),
        "protocol.ext.allow=never".into(),
        "color.ui=false".into(),
    ]
}

/// Config keys of a program-running driver, from the repository's own
/// configuration, as the `-c` overrides that blank them.
fn driver_overrides(keys: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for key in keys {
        // Section and variable are lower-cased by `--name-only`; the subsection
        // (the driver's name) keeps its case, and may itself contain dots.
        let Some((section, rest)) = key.split_once('.') else {
            continue;
        };
        let Some((name, var)) = rest.rsplit_once('.') else {
            continue;
        };
        let blank = match (section, var) {
            ("filter", "clean" | "smudge" | "process") => {
                // A blank command is "no filter"; `required` would otherwise
                // turn the missing filter into an error.
                out.push(format!("filter.{name}.required=false"));
                true
            }
            ("merge", "driver") | ("diff", "textconv" | "command") => true,
            _ => false,
        };
        if blank {
            out.push(format!("{section}.{name}.{var}="));
        }
    }
    out
}

/// A `git -C <dir>` command with every defence applied. `args` are only
/// inspected to decide whether the repository's drivers must be neutralized;
/// the caller adds them with `.args(args)`.
pub(crate) fn command(dir: &Path, args: &[&str]) -> Result<Command> {
    let n = neutral()?;
    let mut cfg = fixed_config(n);
    if !is_plumbing(subcommand(args)) {
        cfg.extend(driver_overrides(&repo_scoped_keys(dir)?));
    }
    Ok(build(dir, &cfg))
}

fn build(dir: &Path, cfg: &[String]) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(dir);
    for kv in cfg {
        c.arg("-c").arg(kv);
    }
    c.env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("SSH_ASKPASS_REQUIRE", "never")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        .env_remove("GIT_ASKPASS")
        .env_remove("SSH_ASKPASS")
        .env_remove("GIT_EXTERNAL_DIFF")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_NAMESPACE");
    // Every pathspec Modbit passes is a literal path: `a[1].txt` is a file
    // name, not a glob; `:(top)`-style magic is never meant.
    c.env("GIT_LITERAL_PATHSPECS", "1");
    c
}

/// The names of the configuration keys that do not come from the user's own
/// global or system files: the repository (local, worktree), `-c`/environment
/// and anything of unknown origin. Read with a command that cannot run a
/// repository program.
fn repo_scoped_keys(dir: &Path) -> Result<Vec<String>> {
    let n = neutral()?;
    let mut cmd = build(dir, &fixed_config(n));
    let args = ["config", "--list", "--show-scope", "--name-only", "--null"];
    cmd.args(args);
    let out = cmd.output().map_err(Error::Spawn)?;
    if !out.status.success() {
        // Not a repository (or unreadable config): nothing repository-owned.
        // The command that follows fails on its own terms when it must.
        return Ok(Vec::new());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut tokens = text.split('\0').filter(|t| !t.is_empty());
    let mut keys = Vec::new();
    while let (Some(scope), Some(key)) = (tokens.next(), tokens.next()) {
        if !matches!(scope, "global" | "system") {
            keys.push(key.to_owned());
        }
    }
    Ok(keys)
}

/// Replace the `user[:password]@` part of every `scheme://` URL in `text` with
/// `<redacted>@`, so a token embedded in a remote URL never reaches an error,
/// a log line or a refusal shown to the model.
#[must_use]
pub fn redact_url_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '/' | '?' | '#' | '\'' | '"' | '`'))
            .unwrap_or(tail.len());
        let (authority, after) = tail.split_at(end);
        match authority.rfind('@') {
            Some(at) => {
                out.push_str("<redacted>@");
                out.push_str(&authority[at + 1..]);
            }
            None => out.push_str(authority),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Text that is never a legal ref component start or carries bytes that make
/// an argument ambiguous: empty, a leading `-` (an option), whitespace or
/// control characters (including NUL, newline), or an over-long value.
pub(crate) fn plain_token(kind: &str, value: &str) -> Result<()> {
    let bad = |why: &str| {
        Err(Error::InvalidRef {
            kind: kind.to_owned(),
            value: redact_url_credentials(&value.escape_debug().to_string()),
            reason: why.to_owned(),
        })
    };
    if value.is_empty() {
        return bad("is empty");
    }
    if value.starts_with('-') {
        return bad("starts with `-` (it would be read as an option)");
    }
    if value.len() > 1024 {
        return bad("is longer than 1024 bytes");
    }
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return bad("contains whitespace or a control character");
    }
    Ok(())
}

/// A revision expression as one argument: no option shape, no whitespace or
/// control characters, and not a range (`a..b`, `a...b`), which is two
/// revisions spliced into one.
pub fn validate_revision(rev: &str) -> Result<()> {
    plain_token("revision", rev)?;
    if rev.contains("..") {
        return Err(Error::InvalidRef {
            kind: "revision".into(),
            value: rev.escape_debug().to_string(),
            reason: "is a range, not a single revision".into(),
        });
    }
    Ok(())
}

/// A remote name or URL as one argument (no option shape, whitespace or
/// control characters).
pub fn validate_remote(remote: &str) -> Result<()> {
    plain_token("remote", remote)
}

/// The syntax part of a branch name (git's `check-ref-format` runs on top).
pub fn validate_branch_syntax(branch: &str) -> Result<()> {
    plain_token("branch", branch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_lose_their_credentials_and_nothing_else() {
        assert_eq!(
            redact_url_credentials(
                "fatal: unable to access 'https://bob:ghp_SECRET@github.com/o/r.git/': 403"
            ),
            "fatal: unable to access 'https://<redacted>@github.com/o/r.git/': 403"
        );
        assert_eq!(
            redact_url_credentials("ssh://git@host:2222/x and https://example.com/a@b"),
            "ssh://<redacted>@host:2222/x and https://example.com/a@b"
        );
        assert_eq!(redact_url_credentials("no url here"), "no url here");
        assert_eq!(redact_url_credentials("trailing ://"), "trailing ://");
    }

    #[test]
    fn option_shaped_and_ambiguous_names_are_refused() {
        for bad in [
            "",
            "--output=/tmp/x",
            "-x",
            "a b",
            "a\nb",
            "a\0b",
            "a\tb",
            "a..b",
            "a...b",
            "\u{7f}",
        ] {
            assert!(validate_revision(bad).is_err(), "{bad:?}");
        }
        for ok in [
            "HEAD",
            "HEAD~3",
            "main",
            "feature/x",
            "v1.0",
            "origin/main^{commit}",
            "@{u}",
            "abc123",
        ] {
            assert!(validate_revision(ok).is_ok(), "{ok:?}");
        }
        assert!(validate_remote("--upload-pack=x").is_err());
        assert!(validate_branch_syntax("-f").is_err());
    }

    #[test]
    fn driver_keys_become_blanking_overrides() {
        let keys: Vec<String> = [
            "filter.lfs.clean",
            "filter.a.b.smudge",
            "merge.m.driver",
            "diff.d.textconv",
            "diff.d.command",
            "diff.d.binary",
            "core.fsmonitor",
            "user.name",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let o = driver_overrides(&keys);
        assert!(o.contains(&"filter.lfs.clean=".to_owned()));
        assert!(o.contains(&"filter.lfs.required=false".to_owned()));
        assert!(o.contains(&"filter.a.b.smudge=".to_owned()));
        assert!(o.contains(&"merge.m.driver=".to_owned()));
        assert!(o.contains(&"diff.d.textconv=".to_owned()));
        assert!(o.contains(&"diff.d.command=".to_owned()));
        assert!(!o.iter().any(|k| k.contains("binary") || k.contains("user")));
    }
}
