//! `modbit-git` — branches, worktrees, diff and commit operations (docs/20
//! "Git strategy"): typed operations over the real `git` binary, never hidden
//! shell magic. Every operation returns structured results; merge is a typed
//! transaction with conflict evidence; dirty local state can be captured as a
//! provenance-bound snapshot commit under `refs/modbit/snapshots/` that never
//! moves HEAD or the index.
//!
//! Every `git` process is built by one hardened command builder (`harden`):
//! repository hooks, fsmonitor, filters, merge drivers, textconv and signing
//! programs never run, caller-supplied refs are validated and follow
//! `--end-of-options`, and URL credentials are redacted from errors (FIX-01).
//!
//! Canonical owner: workspace-git (`docs/12`).

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

pub mod apply;
pub mod diff;
mod harden;
pub use diff::{FileDiff, Hunk, apply_selected, parse_unified};
pub use harden::{
    Neutralized, redact_url_credentials, validate_branch_syntax, validate_remote, validate_revision,
};

/// The id of git's empty tree: the "base" of a repository with no commit.
pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

use serde::{Deserialize, Serialize};

/// Errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `git` could not be run.
    #[error("git could not be executed: {0}")]
    Spawn(std::io::Error),
    /// `git` exited non-zero.
    #[error("git {args:?} failed ({code:?}): {stderr}")]
    Git {
        /// Arguments.
        args: Vec<String>,
        /// Exit code.
        code: Option<i32>,
        /// Stderr.
        stderr: String,
    },
    /// A revision, branch or remote supplied by a caller was refused before it
    /// reached a command line (option-shaped, ambiguous, or not a valid name).
    #[error("invalid {kind} `{value}`: {reason}")]
    InvalidRef {
        /// What was being named (`revision`, `branch`, `remote`).
        kind: String,
        /// The offending text (escaped, credentials redacted).
        value: String,
        /// Why it was refused.
        reason: String,
    },
    /// The path is not inside a Git repository.
    #[error("`{0}` is not a git repository")]
    NotARepository(String),
    /// Git refused the repository because somebody else owns it
    /// (`safe.directory`). Modbit never widens `safe.directory`: trusting a
    /// repository another user owns is the user's decision, made with their
    /// own `git config --global --add safe.directory`.
    #[error(
        "`{0}` is owned by another user, so git refuses it (safe.directory); \
         Modbit does not override that: mark it safe yourself if you trust it"
    )]
    UnsafeRepository(String),
    /// The branch is already checked out in another worktree.
    #[error("branch `{branch}` is already checked out at `{path}`")]
    BranchInUse {
        /// The branch.
        branch: String,
        /// Where it is checked out.
        path: String,
    },
    /// The repository has no commit, which the operation needs.
    #[error("the repository has no commit yet")]
    EmptyRepository,
    /// Output could not be parsed.
    #[error("unexpected git output: {0}")]
    Parse(String),
    /// A transaction is not in the state the operation needs.
    #[error("merge transaction {id} is {state:?}; {detail}")]
    TransactionState {
        /// Id.
        id: String,
        /// State.
        state: MergeState,
        /// Detail.
        detail: String,
    },
}

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// A repository or worktree checkout.
#[derive(Debug, Clone)]
pub struct Repo {
    dir: PathBuf,
}

/// Run git in `dir` (through the one hardened command builder, [`harden`]);
/// returns stdout bytes. `envs` are extra variables for this call only.
fn run_bytes(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Result<Vec<u8>> {
    let mut cmd = harden::command(dir, args)?;
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(Error::Spawn)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if harden::is_dubious_ownership(&stderr) {
            return Err(Error::UnsafeRepository(dir.display().to_string()));
        }
        return Err(Error::Git {
            args: args.iter().map(|s| redact_url_credentials(s)).collect(),
            code: out.status.code(),
            stderr: redact_url_credentials(stderr.trim()),
        });
    }
    Ok(out.stdout)
}

/// A hardened `git -C <dir>` process, not yet started: the one way code
/// outside this crate may build a Git process (PX-067; the architecture lint
/// refuses a Git spawn anywhere else). `args` are inspected only to decide
/// which of the repository's programs must be neutralised for this
/// subcommand; the caller adds them with `.args(args)` and any further
/// arguments or stdio.
pub fn command(dir: &Path, args: &[&str]) -> Result<std::process::Command> {
    harden::command(dir, args)
}

/// Run `git -C <dir> <args>` hardened and return its raw output (status,
/// stdout, stderr). An unsuccessful exit is the caller's to read, as with
/// [`std::process::Command::output`].
pub fn output(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    let mut cmd = harden::command(dir, args)?;
    cmd.args(args);
    cmd.output().map_err(Error::Spawn)
}

/// Run git in `dir`; returns stdout.
fn run(dir: &Path, args: &[&str]) -> Result<String> {
    run_bytes(dir, args, &[]).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Status of one path (porcelain v2 simplified).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusEntry {
    /// Path.
    pub path: String,
    /// Two-letter XY code (`??` untracked, `M.`, `.M`, `A.`, `UU` conflict, …).
    pub code: String,
}

/// A worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worktree {
    /// Absolute path.
    pub path: PathBuf,
    /// HEAD commit.
    pub head: String,
    /// Branch (`None` when detached).
    pub branch: Option<String>,
}

/// One file in a diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFile {
    /// Path (new path for renames).
    pub path: String,
    /// Added lines (`None` for binary).
    pub additions: Option<u64>,
    /// Deleted lines.
    pub deletions: Option<u64>,
}

/// One commit of the recent history with the paths it touched (M3.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitInfo {
    /// Full sha.
    pub sha: String,
    /// Author name.
    pub author: String,
    /// Author date (ISO 8601).
    pub date: String,
    /// Subject line.
    pub subject: String,
    /// Root-relative paths touched.
    pub files: Vec<String>,
}

/// A typed diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    /// Base revision.
    pub base: String,
    /// Target revision (`"WORKTREE"` for uncommitted changes).
    pub target: String,
    /// Files.
    pub files: Vec<DiffFile>,
    /// Unified diff text.
    pub unified: String,
}

/// Merge transaction state (REQ-EV-0067).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MergeState {
    /// Merge applied cleanly in the target worktree but not committed.
    Staged,
    /// Conflicts present; resolutions pending.
    Conflicted,
    /// Committed.
    Committed,
    /// Aborted; target restored.
    Aborted,
}

/// A merge transaction record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeTransaction {
    /// Id.
    pub id: String,
    /// Source revision.
    pub source: String,
    /// Target branch.
    pub target_branch: String,
    /// Target HEAD before the merge.
    pub target_before: String,
    /// Merge base.
    pub base: String,
    /// Worktree where the merge is staged.
    pub worktree: PathBuf,
    /// Conflicted paths.
    pub conflicts: Vec<String>,
    /// Paths marked resolved.
    pub resolutions: Vec<String>,
    /// State.
    pub state: MergeState,
    /// Resulting commit when committed.
    pub result: Option<String>,
}

/// The evidence of one conflicted path (PX-119): the three sides and the
/// worktree file as it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictEvidence {
    /// Root-relative path.
    pub path: String,
    /// The merge base's text (`None`: absent there, or binary).
    pub base: Option<String>,
    /// Our side's text.
    pub ours: Option<String>,
    /// The incoming side's text.
    pub theirs: Option<String>,
    /// The worktree file now (with markers while unresolved).
    pub current: String,
    /// The worktree file still carries a conflict marker line.
    pub has_markers: bool,
}

/// UTF-8 text of `bytes`, cut at `cap` bytes on a character boundary.
fn bounded_text(bytes: &[u8], cap: usize) -> String {
    let mut s = String::from_utf8_lossy(bytes).into_owned();
    if s.len() > cap {
        let mut cut = cap;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("\n[truncated]\n");
    }
    s
}

/// A dirty-state snapshot (REQ-EV-0022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot id.
    pub id: String,
    /// Ref holding the snapshot commit.
    pub reference: String,
    /// Snapshot commit.
    pub commit: String,
    /// Tree of the snapshot.
    pub tree: String,
    /// HEAD the snapshot was taken on (its parent).
    pub parent: String,
    /// Paths captured (tracked changes and untracked files).
    pub paths: Vec<String>,
}

impl Repo {
    /// Open the repository containing `dir`.
    pub fn open(dir: &Path) -> Result<Self> {
        let top = run(dir, &["rev-parse", "--show-toplevel"]).map_err(|e| match e {
            Error::UnsafeRepository(p) => Error::UnsafeRepository(p),
            _ => Error::NotARepository(dir.display().to_string()),
        })?;
        Ok(Self {
            dir: PathBuf::from(top.trim()),
        })
    }

    /// Initialize a new repository with an initial branch.
    pub fn init(dir: &Path, initial_branch: &str) -> Result<Self> {
        validate_branch_syntax(initial_branch)?;
        std::fs::create_dir_all(dir).map_err(Error::Spawn)?;
        check_branch_name(dir, initial_branch)?;
        run(dir, &["init", "-q", "-b", initial_branch])?;
        // Repositories Modbit creates store bytes as given: no line-ending
        // rewriting on checkout (Git for Windows defaults autocrlf=true).
        run(dir, &["config", "core.autocrlf", "false"])?;
        Self::open(dir)
    }

    /// Working directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// HEAD commit.
    pub fn head(&self) -> Result<String> {
        Ok(run(&self.dir, &["rev-parse", "--verify", "HEAD"])?
            .trim()
            .to_owned())
    }

    /// Current branch (`None` when detached).
    pub fn current_branch(&self) -> Result<Option<String>> {
        let s = run(&self.dir, &["symbolic-ref", "-q", "--short", "HEAD"]).unwrap_or_default();
        Ok(if s.trim().is_empty() {
            None
        } else {
            Some(s.trim().to_owned())
        })
    }

    /// Typed status, from `status --porcelain=v1 -z` (NUL-terminated records,
    /// never quoted, so names with spaces, quotes or non-ASCII bytes survive).
    ///
    /// Renames are not detected (`--no-renames`): a rename is reported as the
    /// deleted old path and the added new path, which is what a consumer that
    /// captures or restores per path needs. The two-letter code keeps the
    /// porcelain-v2 spelling: `.` for "unchanged in this column".
    pub fn status(&self) -> Result<Vec<StatusEntry>> {
        let out = run_bytes(
            &self.dir,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--no-renames",
                "--untracked-files=all",
            ],
            &[],
        )?;
        let mut entries = Vec::new();
        let mut records = out.split(|b| *b == 0).filter(|r| !r.is_empty());
        while let Some(rec) = records.next() {
            if rec.len() < 4 || rec[2] != b' ' {
                return Err(Error::Parse(format!(
                    "status record `{}`",
                    String::from_utf8_lossy(rec)
                )));
            }
            let (xy, path) = (&rec[..2], &rec[3..]);
            // With renames/copies enabled in config the record is followed by
            // the origin path; `--no-renames` makes that unreachable, but a
            // record we do not understand must not shift the stream.
            if matches!(xy[0], b'R' | b'C') || matches!(xy[1], b'R' | b'C') {
                records.next();
            }
            let code: String = xy
                .iter()
                .map(|b| if *b == b' ' { '.' } else { char::from(*b) })
                .collect();
            entries.push(StatusEntry {
                path: String::from_utf8_lossy(path).into_owned(),
                code,
            });
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    /// Stage paths (or everything with `["."]`) and commit; returns the commit.
    pub fn commit(&self, message: &str, paths: &[&str]) -> Result<String> {
        let mut add = vec!["add", "-A", "--"];
        add.extend_from_slice(paths);
        run(&self.dir, &add)?;
        run(
            &self.dir,
            &[
                "-c",
                "user.name=Modbit",
                "-c",
                "user.email=modbit@localhost",
                "commit",
                "-q",
                "--no-verify",
                "-m",
                message,
            ],
        )?;
        self.head()
    }

    /// Resolve `revision` to the object it names. The text is validated
    /// ([`validate_revision`]: no option shape, whitespace, control characters
    /// or range) and passed after `--end-of-options`; a revision git cannot
    /// resolve is refused. Every public operation that takes a caller-supplied
    /// revision goes through this (or [`validate_revision`] alone when a lookup
    /// per call would be wasteful).
    pub fn resolve(&self, revision: &str) -> Result<String> {
        validate_revision(revision)?;
        run(
            &self.dir,
            &["rev-parse", "--verify", "--end-of-options", revision],
        )
        .map(|s| s.trim().to_owned())
    }

    /// Create `branch` at `base` (a revision) without checking it out.
    pub fn create_branch(&self, branch: &str, base: &str) -> Result<()> {
        check_branch_name(&self.dir, branch)?;
        let base = self.resolve(&format!("{base}^{{commit}}"))?;
        run(
            &self.dir,
            &["branch", "--no-track", "--end-of-options", branch, &base],
        )
        .map(|_| ())
    }

    /// Add a worktree at `path` checked out on `branch` (which must exist).
    /// A branch that is already checked out in another worktree is the typed
    /// [`Error::BranchInUse`], naming where.
    pub fn worktree_add(&self, path: &Path, branch: &str) -> Result<Repo> {
        self.resolve(branch)?;
        run(
            &self.dir,
            &[
                "worktree",
                "add",
                "-q",
                "--end-of-options",
                &git_path(path)?,
                branch,
            ],
        )
        .map_err(|e| in_use(e, branch))?;
        Repo::open(path)
    }

    /// Add a worktree at `path` on a new orphan branch `branch` with nothing
    /// checked out: the worktree of a repository that has no commit yet (its
    /// "base" is [`EMPTY_TREE`]).
    pub fn worktree_add_orphan(&self, path: &Path, branch: &str) -> Result<Repo> {
        check_branch_name(&self.dir, branch)?;
        run(
            &self.dir,
            &[
                "worktree",
                "add",
                "-q",
                "--orphan",
                "-b",
                branch,
                "--end-of-options",
                &git_path(path)?,
            ],
        )
        .map_err(|e| in_use(e, branch))?;
        Repo::open(path)
    }

    /// Forget worktree records whose directory is gone (`git worktree
    /// prune`), so a path or branch a deleted worktree held is free again.
    pub fn worktree_prune(&self) -> Result<()> {
        run(&self.dir, &["worktree", "prune"]).map(|_| ())
    }

    /// Delete a branch that is not checked out (forced: the caller decided the
    /// branch's work is applied, merged or discarded).
    pub fn branch_delete(&self, branch: &str) -> Result<()> {
        check_branch_name(&self.dir, branch)?;
        run(&self.dir, &["branch", "-D", "--end-of-options", branch]).map(|_| ())
    }

    /// For a linked worktree, the main working tree it was added from
    /// (`None` for the main working tree itself, or a bare layout).
    #[must_use]
    pub fn common_checkout(&self) -> Option<PathBuf> {
        let out = run(
            &self.dir,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .ok()?;
        let common = PathBuf::from(out.trim());
        if common.file_name()? != ".git" {
            return None;
        }
        let main = common.parent()?.to_path_buf();
        let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        };
        (!same(&main, &self.dir)).then_some(main)
    }

    /// Whether `HEAD` names no commit yet (a repository or orphan branch with
    /// no history).
    #[must_use]
    pub fn is_unborn(&self) -> bool {
        run(&self.dir, &["rev-parse", "--verify", "-q", "HEAD"]).is_err()
    }

    /// Everything this repository's configuration and hooks asked git to run
    /// that the hardened runner refuses (hooks, fsmonitor, filters, drivers,
    /// ssh commands, credential helpers…). Reading it runs none of them.
    pub fn neutralized(&self) -> Result<Vec<Neutralized>> {
        harden::neutralized(&self.dir)
    }

    /// Whether `ancestor` is reachable from `descendant`.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool> {
        validate_revision(ancestor)?;
        validate_revision(descendant)?;
        match run(
            &self.dir,
            &[
                "merge-base",
                "--is-ancestor",
                "--end-of-options",
                ancestor,
                descendant,
            ],
        ) {
            Ok(_) => Ok(true),
            Err(Error::Git { code: Some(1), .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Commits reachable from `head` and not from `base`.
    pub fn commits_between(&self, base: &str, head: &str) -> Result<u64> {
        validate_revision(base)?;
        validate_revision(head)?;
        let range = format!("{base}..{head}");
        run(
            &self.dir,
            &["rev-list", "--count", "--end-of-options", &range],
        )?
        .trim()
        .parse()
        .map_err(|_| Error::Parse("rev-list count".into()))
    }

    /// The tree object a revision names.
    pub fn tree_of(&self, revision: &str) -> Result<String> {
        self.resolve(&format!("{revision}^{{tree}}"))
    }

    /// A merge is in progress in this worktree (`MERGE_HEAD` exists).
    #[must_use]
    pub fn merge_in_progress(&self) -> bool {
        run(&self.dir, &["rev-parse", "--verify", "-q", "MERGE_HEAD"]).is_ok()
    }

    /// The per-file evidence of a conflicted path: what the merge base, this
    /// side ("ours") and the other side ("theirs") hold, and what the
    /// worktree file says now (conflict markers included). Texts are bounded.
    pub fn conflict_evidence(&self, path: &str) -> Result<ConflictEvidence> {
        const CAP: usize = 64 * 1024;
        let stage = |n: u8| -> Option<String> {
            run_bytes(
                &self.dir,
                &["show", "--no-textconv", &format!(":{n}:{path}")],
                &[],
            )
            .ok()
            .map(|b| bounded_text(&b, CAP))
        };
        let current = std::fs::read(self.dir.join(path))
            .map(|b| bounded_text(&b, CAP))
            .unwrap_or_default();
        let markers = current.lines().any(|l| {
            l.starts_with("<<<<<<<") || l.starts_with(">>>>>>>") || l.starts_with("=======")
        });
        Ok(ConflictEvidence {
            path: path.to_owned(),
            base: stage(1),
            ours: stage(2),
            theirs: stage(3),
            current,
            has_markers: markers,
        })
    }

    /// Remove a worktree (forced: the task owns it).
    pub fn worktree_remove(&self, path: &Path) -> Result<()> {
        run(
            &self.dir,
            &[
                "worktree",
                "remove",
                "--force",
                "--end-of-options",
                &git_path(path)?,
            ],
        )
        .map(|_| ())
    }

    /// List worktrees.
    pub fn worktree_list(&self) -> Result<Vec<Worktree>> {
        let out = run(&self.dir, &["worktree", "list", "--porcelain"])?;
        let mut list: Vec<Worktree> = Vec::new();
        for line in out.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                list.push(Worktree {
                    path: PathBuf::from(p),
                    head: String::new(),
                    branch: None,
                });
            } else if let Some(h) = line.strip_prefix("HEAD ")
                && let Some(w) = list.last_mut()
            {
                w.head = h.to_owned();
            } else if let Some(b) = line.strip_prefix("branch ")
                && let Some(w) = list.last_mut()
            {
                w.branch = Some(b.trim_start_matches("refs/heads/").to_owned());
            }
        }
        Ok(list)
    }

    /// `git diff` over `revs` (already-resolved object ids or `HEAD`): the
    /// flags that keep repository programs out (`--no-ext-diff`,
    /// `--no-textconv`) and `--end-of-options` before the operands.
    fn diff_args<'a>(flags: &[&'a str], revs: &[&'a str]) -> Vec<&'a str> {
        let mut a = vec!["diff", "--no-ext-diff", "--no-textconv", "-M"];
        a.extend_from_slice(flags);
        a.push("--end-of-options");
        a.extend_from_slice(revs);
        a.push("--");
        a
    }

    fn numstat(&self, revs: &[&str]) -> Result<Vec<DiffFile>> {
        // `-z`: `add\tdel\tpath\0`, and for a rename `add\tdel\t\0old\0new\0`.
        let out = run_bytes(&self.dir, &Self::diff_args(&["--numstat", "-z"], revs), &[])?;
        let mut files = Vec::new();
        let mut records = out.split(|b| *b == 0).filter(|r| !r.is_empty());
        while let Some(rec) = records.next() {
            let rec = String::from_utf8_lossy(rec);
            let mut it = rec.splitn(3, '\t');
            let (add, del, path) = (
                it.next().unwrap_or(""),
                it.next().unwrap_or(""),
                it.next().unwrap_or(""),
            );
            let path = if path.is_empty() {
                // Rename or copy: the next two records are the old and new path.
                let _old = records.next();
                records
                    .next()
                    .map(|p| String::from_utf8_lossy(p).into_owned())
                    .unwrap_or_default()
            } else {
                path.to_owned()
            };
            files.push(DiffFile {
                path,
                additions: add.parse().ok(),
                deletions: del.parse().ok(),
            });
        }
        Ok(files)
    }

    /// Typed diff between two revisions. Both are validated and resolved to
    /// object ids before they reach the command line, so a model-supplied
    /// `--output=…`, a range, or whitespace is refused, not interpreted.
    pub fn diff(&self, base: &str, target: &str) -> Result<Diff> {
        let (b, t) = (self.resolve(base)?, self.resolve(target)?);
        let files = self.numstat(&[&b, &t])?;
        let unified = run(&self.dir, &Self::diff_args(&[], &[&b, &t]))?;
        Ok(Diff {
            base: base.into(),
            target: target.into(),
            files,
            unified,
        })
    }

    /// Typed diff of the worktree (staged + unstaged) against HEAD.
    pub fn diff_worktree(&self) -> Result<Diff> {
        let files = self.numstat(&["HEAD"])?;
        let unified = run(&self.dir, &Self::diff_args(&[], &["HEAD"]))?;
        Ok(Diff {
            base: "HEAD".into(),
            target: "WORKTREE".into(),
            files,
            unified,
        })
    }

    /// Recent commits (newest first) with the paths each touched, for the
    /// evidence graph's change history and co-change edges (M3.6).
    pub fn log_recent(&self, max: usize) -> Result<Vec<CommitInfo>> {
        let text = run(
            &self.dir,
            &[
                "log",
                "--no-ext-diff",
                "--no-textconv",
                &format!("--max-count={}", max.max(1)),
                "--name-only",
                "--format=%x1e%H%x1f%an%x1f%aI%x1f%s",
            ],
        )?;
        let mut out = Vec::new();
        for rec in text.split('\u{1e}').filter(|r| !r.trim().is_empty()) {
            let mut lines = rec.lines();
            let Some(head) = lines.next() else { continue };
            let mut f = head.split('\u{1f}');
            let sha = f.next().unwrap_or_default().trim().to_owned();
            let author = f.next().unwrap_or_default().to_owned();
            let date = f.next().unwrap_or_default().to_owned();
            let subject = f.next().unwrap_or_default().to_owned();
            let files: Vec<String> = lines
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(|l| l.replace('\\', "/"))
                .collect();
            if !sha.is_empty() {
                out.push(CommitInfo {
                    sha,
                    author,
                    date,
                    subject,
                    files,
                });
            }
        }
        Ok(out)
    }

    /// Merge base.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<String> {
        validate_revision(a)?;
        validate_revision(b)?;
        Ok(run(&self.dir, &["merge-base", "--end-of-options", a, b])?
            .trim()
            .to_owned())
    }

    /// Start a merge transaction: merge `source` into this worktree's branch
    /// without committing; conflicts are evidence, not an error.
    pub fn merge_begin(&self, id: &str, source: &str) -> Result<MergeTransaction> {
        let target_branch = self
            .current_branch()?
            .ok_or_else(|| Error::Parse("merge target must be on a branch".into()))?;
        let target_before = self.head()?;
        let source_sha = self.resolve(source)?;
        let base = self.merge_base(&target_before, &source_sha)?;
        let result = run(
            &self.dir,
            &[
                "-c",
                "user.name=Modbit",
                "-c",
                "user.email=modbit@localhost",
                "merge",
                "--no-commit",
                "--no-ff",
                "-q",
                "--end-of-options",
                &source_sha,
            ],
        );
        let conflicts = self.unmerged_paths()?;
        let state = if !conflicts.is_empty() {
            MergeState::Conflicted
        } else if result.is_ok() {
            MergeState::Staged
        } else {
            return Err(result.unwrap_err());
        };
        Ok(MergeTransaction {
            id: id.into(),
            source: source_sha,
            target_branch,
            target_before,
            base,
            worktree: self.dir.clone(),
            conflicts,
            resolutions: vec![],
            state,
            result: None,
        })
    }

    /// The paths the index still holds as unmerged (conflicted, not yet
    /// marked resolved).
    pub fn unmerged_paths(&self) -> Result<Vec<String>> {
        Ok(run_bytes(
            &self.dir,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--name-only",
                "-z",
                "--diff-filter=U",
            ],
            &[],
        )?
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect())
    }

    /// Mark a conflicted path resolved (the caller wrote the resolution to the file).
    pub fn merge_resolve(&self, tx: &mut MergeTransaction, path: &str) -> Result<()> {
        if tx.state != MergeState::Conflicted {
            return Err(Error::TransactionState {
                id: tx.id.clone(),
                state: tx.state,
                detail: "only a conflicted merge takes resolutions".into(),
            });
        }
        run(&self.dir, &["add", "--", path])?;
        tx.conflicts.retain(|c| c != path);
        tx.resolutions.push(path.to_owned());
        if tx.conflicts.is_empty() {
            tx.state = MergeState::Staged;
        }
        Ok(())
    }

    /// Commit a staged merge.
    pub fn merge_commit(&self, tx: &mut MergeTransaction, message: &str) -> Result<String> {
        if tx.state != MergeState::Staged {
            return Err(Error::TransactionState {
                id: tx.id.clone(),
                state: tx.state,
                detail: "conflicts must be resolved before commit".into(),
            });
        }
        run(
            &self.dir,
            &[
                "-c",
                "user.name=Modbit",
                "-c",
                "user.email=modbit@localhost",
                "commit",
                "-q",
                "--no-verify",
                "-m",
                message,
            ],
        )?;
        let sha = self.head()?;
        tx.state = MergeState::Committed;
        tx.result = Some(sha.clone());
        Ok(sha)
    }

    /// Abort: restore the target to `target_before`.
    pub fn merge_abort(&self, tx: &mut MergeTransaction) -> Result<()> {
        if matches!(tx.state, MergeState::Committed | MergeState::Aborted) {
            return Err(Error::TransactionState {
                id: tx.id.clone(),
                state: tx.state,
                detail: "nothing to abort".into(),
            });
        }
        validate_revision(&tx.target_before)?;
        let _ = run(&self.dir, &["merge", "--abort"]);
        run(
            &self.dir,
            &[
                "reset",
                "-q",
                "--hard",
                "--end-of-options",
                &tx.target_before,
            ],
        )?;
        tx.state = MergeState::Aborted;
        Ok(())
    }

    /// The tree object of the worktree's whole state: `HEAD` plus every
    /// tracked change and every untracked, non-ignored file, built in a
    /// temporary index (a fresh one per call). No commit, no ref; `HEAD`, the
    /// index and the worktree are untouched. Two calls on an unchanged
    /// worktree answer the same tree.
    pub fn dirty_tree(&self) -> Result<String> {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unborn = self.is_unborn();
        let git_dir = run(&self.dir, &["rev-parse", "--git-dir"])?
            .trim()
            .to_owned();
        let name = format!(
            "modbit-snapshot-index-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let tmp_index = if Path::new(&git_dir).is_absolute() {
            PathBuf::from(&git_dir).join(name)
        } else {
            self.dir.join(&git_dir).join(name)
        }
        .to_string_lossy()
        .into_owned();
        let with_index = |args: &[&str]| -> Result<String> {
            run_bytes(&self.dir, args, &[("GIT_INDEX_FILE", &tmp_index)])
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        };
        let built = (|| -> Result<String> {
            if !unborn {
                with_index(&["read-tree", "HEAD"])?;
            }
            with_index(&["add", "-A", "."])?;
            Ok(with_index(&["write-tree"])?.trim().to_owned())
        })();
        let _ = std::fs::remove_file(&tmp_index);
        built
    }

    /// Capture the dirty worktree (tracked changes and untracked files) as a
    /// commit under `refs/modbit/snapshots/<id>` using a temporary index;
    /// HEAD, the index and the worktree are untouched.
    pub fn snapshot_dirty(&self, id: &str) -> Result<Snapshot> {
        harden::plain_token("snapshot id", id)?;
        // A repository with no commit yet (an orphan worktree) snapshots on
        // the empty tree and has no parent.
        let unborn = self.is_unborn();
        let parent = if unborn { String::new() } else { self.head()? };
        let paths: Vec<String> = self.status()?.into_iter().map(|e| e.path).collect();
        let tree = self.dirty_tree()?;
        let message = format!("modbit snapshot {id}");
        let mut args = vec![
            "-c",
            "user.name=Modbit",
            "-c",
            "user.email=modbit@localhost",
            "commit-tree",
            tree.as_str(),
        ];
        if !unborn {
            args.extend(["-p", parent.as_str()]);
        }
        args.extend(["-m", message.as_str()]);
        let commit = run(&self.dir, &args)?.trim().to_owned();
        let reference = format!("refs/modbit/snapshots/{id}");
        run(&self.dir, &["update-ref", &reference, &commit])?;
        Ok(Snapshot {
            id: id.into(),
            reference,
            commit,
            tree,
            parent,
            paths,
        })
    }

    /// Delete a snapshot ref (the objects stay until gc).
    pub fn snapshot_cleanup(&self, snapshot: &Snapshot) -> Result<()> {
        run(&self.dir, &["update-ref", "-d", &snapshot.reference]).map(|_| ())
    }

    /// The URL a remote is configured with (the spelling in the
    /// configuration; `insteadOf` rewrites apply when git connects, not here).
    ///
    /// The raw spelling may carry credentials (`https://user:token@host/…`):
    /// the caller that echoes it anywhere must pass it through
    /// [`redact_url_credentials`]. Errors from this crate are already redacted.
    pub fn remote_url(&self, remote: &str) -> Result<String> {
        validate_remote(remote)?;
        run(
            &self.dir,
            &["config", "--get", &format!("remote.{remote}.url")],
        )
        .map(|s| s.trim().to_owned())
    }

    /// `remote` names a remote configured in this repository (never a URL or
    /// path: a push or ls-remote target is chosen by configuration, not by
    /// whoever supplied the argument).
    fn configured_remote(&self, remote: &str) -> Result<()> {
        validate_remote(remote)?;
        self.remote_url(remote)
            .map(|_| ())
            .map_err(|_| Error::InvalidRef {
                kind: "remote".into(),
                value: redact_url_credentials(&remote.escape_debug().to_string()),
                reason: "is not a configured remote of this repository".into(),
            })
    }

    /// Point `branch` at `revision` (created or moved), without checking it out.
    pub fn set_branch(&self, branch: &str, revision: &str) -> Result<()> {
        check_branch_name(&self.dir, branch)?;
        let revision = self.resolve(&format!("{revision}^{{commit}}"))?;
        run(
            &self.dir,
            &[
                "branch",
                "-f",
                "--no-track",
                "--end-of-options",
                branch,
                &revision,
            ],
        )
        .map(|_| ())
    }

    /// Push `branch` to `remote` (PX-007: the typed branch push; no shell).
    /// `force` updates a remote branch that already exists at another commit.
    pub fn push_branch(&self, remote: &str, branch: &str, force: bool) -> Result<String> {
        check_branch_name(&self.dir, branch)?;
        self.configured_remote(remote)?;
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        let mut args = vec!["push", "--porcelain", "--no-verify"];
        if force {
            args.push("--force");
        }
        args.push("--end-of-options");
        args.push(remote);
        args.push(&refspec);
        run(&self.dir, &args)
    }

    /// The commit a remote branch is at, as this repository last saw it
    /// (`git ls-remote`; no fetch of objects).
    pub fn remote_branch_head(&self, remote: &str, branch: &str) -> Result<Option<String>> {
        check_branch_name(&self.dir, branch)?;
        self.configured_remote(remote)?;
        let out = run(
            &self.dir,
            &[
                "ls-remote",
                "--heads",
                "--end-of-options",
                remote,
                &format!("refs/heads/{branch}"),
            ],
        )?;
        Ok(out
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().next())
            .map(str::to_owned))
    }

    /// The commit a revision resolves to (see [`Repo::resolve`]).
    pub fn rev_parse(&self, revision: &str) -> Result<String> {
        self.resolve(revision)
    }

    /// Whether a ref exists.
    pub fn ref_exists(&self, reference: &str) -> bool {
        validate_revision(reference).is_ok()
            && run(
                &self.dir,
                &["rev-parse", "--verify", "-q", "--end-of-options", reference],
            )
            .is_ok()
    }

    /// Read a file at a revision (no textconv, no external programs).
    pub fn show(&self, revision: &str, path: &str) -> Result<Vec<u8>> {
        validate_revision(revision)?;
        run_bytes(
            &self.dir,
            &[
                "show",
                "--no-ext-diff",
                "--no-textconv",
                "--end-of-options",
                &format!("{revision}:{path}"),
            ],
            &[],
        )
    }
}

/// Turn git's "branch is already checked out" failure into the typed error.
fn in_use(e: Error, branch: &str) -> Error {
    if let Error::Git { stderr, .. } = &e {
        for marker in [
            "is already checked out at '",
            "is already used by worktree at '",
        ] {
            if let Some(i) = stderr.find(marker) {
                let rest = &stderr[i + marker.len()..];
                let path = rest.split('\'').next().unwrap_or_default();
                return Error::BranchInUse {
                    branch: branch.to_owned(),
                    path: path.to_owned(),
                };
            }
        }
    }
    e
}

/// `name` is a valid branch name (git's own `check-ref-format` rules) and not
/// option-shaped.
fn check_branch_name(dir: &Path, name: &str) -> Result<()> {
    validate_branch_syntax(name)?;
    run(dir, &["check-ref-format", &format!("refs/heads/{name}")])
        .map(|_| ())
        .map_err(|_| Error::InvalidRef {
            kind: "branch".into(),
            value: name.escape_debug().to_string(),
            reason: "is not a valid branch name".into(),
        })
}

/// A path as the git binary accepts it: canonicalized Windows paths carry the
/// `\\?\` verbatim prefix, which git does not understand.
fn git_path(path: &Path) -> Result<String> {
    let s = path
        .to_str()
        .ok_or_else(|| Error::Parse("non-utf8 path".into()))?;
    Ok(
        match s
            .strip_prefix(r"\\?\UNC\")
            .or_else(|| s.strip_prefix("//?/UNC/"))
        {
            Some(unc) => format!(r"\\{unc}"),
            None => s
                .strip_prefix(r"\\?\")
                .or_else(|| s.strip_prefix("//?/"))
                .unwrap_or(s)
                .to_owned(),
        },
    )
}
