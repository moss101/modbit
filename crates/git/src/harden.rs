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
//! PX-067 closes what FIX-01 documented as open:
//!
//! * `safe.bareRepository=explicit`: a bare repository embedded in a working
//!   tree (a planted `evil.git/`) is never entered by discovery, so its
//!   `config` and `hooks` are never read for a command run in or under it;
//! * a repository-scoped `core.sshCommand`, `core.gitProxy`,
//!   `credential.helper` (and every `credential.<url>.helper`),
//!   `diff.external`, `gpg.*.program`, `core.pager`, `core.editor`,
//!   `sequence.editor`, `core.alternateRefsCommand` and
//!   `uploadpack.packObjectsHook` is blanked exactly as a filter is: read
//!   from the repository's own configuration (an `include.path` or
//!   `includeIf` file of the repository has the repository's scope, so it is
//!   covered), never from the user's global file, so a user's own ssh
//!   command and credential helper keep working. A blanked `sshCommand`
//!   fails the network command closed instead of running the repository's;
//! * attributes files: `core.attributesFile` is neutral and the attribute
//!   *names* a repository's `.gitattributes` / `info/attributes` chooses
//!   (`filter=`, `diff=`, `merge=`) only ever resolve to drivers, and every
//!   repository-scoped driver program is blanked above. An attribute that
//!   names a driver the user's own configuration defines (git-lfs) stays;
//! * `safe.directory` is never widened: a repository owned by somebody else
//!   is refused with a typed [`crate::Error::UnsafeRepository`], because
//!   trusting it is the user's decision, not a side effect of an agent's call;
//! * [`neutralized`] reports what a repository asked git to run and was
//!   refused, so the Core can put a security event on the task (FIX-01
//!   neutralised silently).

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
        // A bare repository found by discovery (one planted inside a working
        // tree) is entered only when it is named; its config and hooks are
        // never consulted for a command run in or under it.
        "safe.bareRepository=explicit".into(),
    ]
}

/// What a repository-scoped configuration key asks git to run, if anything:
/// the kind of program and the `-c` overrides that blank it.
fn program_kind(key: &str) -> Option<(&'static str, Vec<String>)> {
    // Section and variable are lower-cased by `--name-only`; the subsection
    // (a driver's or a URL's name) keeps its case and may itself contain dots.
    let (section, rest) = key.split_once('.')?;
    let (name, var) = match rest.rsplit_once('.') {
        Some((n, v)) => (Some(n), v),
        None => (None, rest),
    };
    let blank = || -> String {
        match name {
            Some(n) => format!("{section}.{n}.{var}="),
            None => format!("{section}.{var}="),
        }
    };
    Some(match (section, name, var) {
        ("filter", Some(n), "clean" | "smudge" | "process") => (
            "FILTER",
            // A blank command is "no filter"; `required` would otherwise
            // turn the missing filter into an error.
            vec![format!("filter.{n}.required=false"), blank()],
        ),
        ("merge", Some(_), "driver") => ("MERGE_DRIVER", vec![blank()]),
        ("diff", Some(_), "textconv") => ("TEXTCONV", vec![blank()]),
        ("diff", Some(_), "command") | ("diff", None, "external") => {
            ("EXTERNAL_DIFF", vec![blank()])
        }
        ("core", None, "sshcommand") => ("SSH_COMMAND", vec![blank()]),
        ("core", None, "gitproxy") => ("GIT_PROXY", vec![blank()]),
        ("core", None, "pager") | ("pager", _, _) => ("PAGER", vec![blank()]),
        ("core", None, "editor") | ("sequence", None, "editor") => ("EDITOR", vec![blank()]),
        ("core", None, "alternaterefscommand") => ("ALTERNATE_REFS", vec![blank()]),
        ("uploadpack", None, "packobjectshook") => ("PACK_OBJECTS_HOOK", vec![blank()]),
        // An empty value resets every helper configured before it.
        ("credential", _, "helper") => ("CREDENTIAL_HELPER", vec![blank()]),
        ("gpg", _, "program") => ("SIGNING_PROGRAM", vec![blank()]),
        ("core", None, "fsmonitor") => ("FSMONITOR", vec![]),
        ("core", None, "hookspath") => ("HOOKS_PATH", vec![]),
        _ => return None,
    })
}

/// Config keys of a program-running setting, from the repository's own
/// configuration, as the `-c` overrides that blank them.
fn driver_overrides(keys: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for key in keys {
        if let Some((_, overrides)) = program_kind(key) {
            for o in overrides {
                if !out.contains(&o) {
                    out.push(o);
                }
            }
        }
    }
    out
}

/// Subcommands that talk to a remote: the repository's `core.sshCommand`,
/// `core.gitProxy` and `credential.helper` would run for them.
fn is_network(sub: &str) -> bool {
    matches!(sub, "push" | "ls-remote" | "fetch" | "pull" | "clone")
}

/// A `git -C <dir>` command with every defence applied. `args` are only
/// inspected to decide whether the repository's programs must be neutralized;
/// the caller adds them with `.args(args)`.
pub(crate) fn command(dir: &Path, args: &[&str]) -> Result<Command> {
    let n = neutral()?;
    let mut cfg = fixed_config(n);
    let sub = subcommand(args);
    if !is_plumbing(sub) || is_network(sub) {
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

/// The `(scope, key)` pairs of the configuration that do not come from the
/// user's own global or system files: the repository (local, worktree),
/// `-c`/environment and anything of unknown origin. Read with a command that
/// cannot run a repository program. A file the repository `include`s has the
/// repository's scope.
fn repo_scoped(dir: &Path) -> Result<Vec<(String, String)>> {
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
        // `command` scope is this runner's own `-c` flags (and the process
        // environment's `GIT_CONFIG_*`): not something a repository supplied.
        if !matches!(scope, "global" | "system" | "command") {
            keys.push((scope.to_owned(), key.to_owned()));
        }
    }
    Ok(keys)
}

fn repo_scoped_keys(dir: &Path) -> Result<Vec<String>> {
    Ok(repo_scoped(dir)?.into_iter().map(|(_, k)| k).collect())
}

/// One program a repository asked git to run that the hardened runner refused.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Neutralized {
    /// `HOOK`, `HOOKS_PATH`, `FSMONITOR`, `FILTER`, `MERGE_DRIVER`,
    /// `TEXTCONV`, `EXTERNAL_DIFF`, `SSH_COMMAND`, `GIT_PROXY`, `PAGER`,
    /// `EDITOR`, `ALTERNATE_REFS`, `PACK_OBJECTS_HOOK`, `CREDENTIAL_HELPER`
    /// or `SIGNING_PROGRAM`.
    pub kind: String,
    /// The configuration key, or the hook's file name.
    pub name: String,
    /// Where it was found: the configuration scope (`local`, `worktree`,
    /// `command`) or `hooks`.
    pub scope: String,
}

/// Everything in `dir`'s repository that would have run a program on Modbit's
/// behalf and was neutralised, sorted and de-duplicated. Reading it runs no
/// repository program.
pub(crate) fn neutralized(dir: &Path) -> Result<Vec<Neutralized>> {
    let mut out: Vec<Neutralized> = Vec::new();
    for (scope, key) in repo_scoped(dir)? {
        if let Some((kind, _)) = program_kind(&key) {
            out.push(Neutralized {
                kind: kind.to_owned(),
                name: key,
                scope,
            });
        }
    }
    // The hooks directory the repository would have used (a hook is any
    // executable that is not a `.sample`).
    let n = neutral()?;
    let mut cmd = build(dir, &fixed_config(n));
    // (`--git-path hooks` would answer with the neutral `core.hooksPath` this
    // runner sets; the repository's own hooks live in the common git dir.)
    cmd.args(["rev-parse", "--git-common-dir"]);
    if let Ok(o) = cmd.output()
        && o.status.success()
    {
        let rel = String::from_utf8_lossy(&o.stdout).trim().to_owned();
        let hooks = {
            let p = Path::new(&rel);
            if p.is_absolute() {
                p.join("hooks")
            } else {
                dir.join(p).join("hooks")
            }
        };
        if let Ok(rd) = std::fs::read_dir(&hooks) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.ends_with(".sample") || !e.path().is_file() {
                    continue;
                }
                #[cfg(unix)]
                let runnable = {
                    use std::os::unix::fs::PermissionsExt;
                    e.metadata()
                        .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
                };
                #[cfg(not(unix))]
                let runnable = true;
                if runnable {
                    out.push(Neutralized {
                        kind: "HOOK".into(),
                        name,
                        scope: "hooks".into(),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| (&a.kind, &a.name, &a.scope).cmp(&(&b.kind, &b.name, &b.scope)));
    out.dedup();
    Ok(out)
}

/// Whether git's stderr says it refused the repository for its ownership.
#[must_use]
pub(crate) fn is_dubious_ownership(stderr: &str) -> bool {
    stderr.contains("dubious ownership")
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
