//! `modbit-git` — branches, worktrees, diff and commit operations (docs/20
//! "Git strategy"): typed operations over the real `git` binary, never hidden
//! shell magic. Every operation returns structured results; merge is a typed
//! transaction with conflict evidence; dirty local state can be captured as a
//! provenance-bound snapshot commit under `refs/modbit/snapshots/` that never
//! moves HEAD or the index.
//!
//! Canonical owner: workspace-git (`docs/12`).

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

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
    /// The path is not inside a Git repository.
    #[error("`{0}` is not a git repository")]
    NotARepository(String),
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

/// Run git in `dir`; returns stdout.
fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .output()
        .map_err(Error::Spawn)?;
    if !out.status.success() {
        return Err(Error::Git {
            args: args.iter().map(|s| (*s).to_owned()).collect(),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
        let top = run(dir, &["rev-parse", "--show-toplevel"])
            .map_err(|_| Error::NotARepository(dir.display().to_string()))?;
        Ok(Self {
            dir: PathBuf::from(top.trim()),
        })
    }

    /// Initialize a new repository with an initial branch.
    pub fn init(dir: &Path, initial_branch: &str) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(Error::Spawn)?;
        run(dir, &["init", "-q", "-b", initial_branch])?;
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

    /// Typed status.
    pub fn status(&self) -> Result<Vec<StatusEntry>> {
        let out = run(
            &self.dir,
            &["status", "--porcelain=v2", "--untracked-files=all"],
        )?;
        let mut entries = Vec::new();
        for line in out.lines() {
            let mut parts = line.splitn(9, ' ');
            match parts.next() {
                Some("1") | Some("2") => {
                    let code = parts.next().unwrap_or("").to_owned();
                    let path = line.rsplit(' ').next().unwrap_or("").to_owned();
                    entries.push(StatusEntry { path, code });
                }
                Some("u") => {
                    let path = line.rsplit(' ').next().unwrap_or("").to_owned();
                    entries.push(StatusEntry {
                        path,
                        code: parts.next().unwrap_or("UU").to_owned(),
                    });
                }
                Some("?") => entries.push(StatusEntry {
                    path: line[2..].to_owned(),
                    code: "??".into(),
                }),
                _ => {}
            }
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

    /// Create `branch` at `base` (a revision) without checking it out.
    pub fn create_branch(&self, branch: &str, base: &str) -> Result<()> {
        run(&self.dir, &["branch", "--no-track", branch, base]).map(|_| ())
    }

    /// Add a worktree at `path` checked out on `branch` (which must exist).
    pub fn worktree_add(&self, path: &Path, branch: &str) -> Result<Repo> {
        run(
            &self.dir,
            &[
                "worktree",
                "add",
                "-q",
                path.to_str()
                    .ok_or_else(|| Error::Parse("non-utf8 path".into()))?,
                branch,
            ],
        )?;
        Repo::open(path)
    }

    /// Remove a worktree (forced: the task owns it).
    pub fn worktree_remove(&self, path: &Path) -> Result<()> {
        run(
            &self.dir,
            &[
                "worktree",
                "remove",
                "--force",
                path.to_str()
                    .ok_or_else(|| Error::Parse("non-utf8 path".into()))?,
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

    fn numstat(&self, args: &[&str]) -> Result<Vec<DiffFile>> {
        let mut a = vec!["diff", "--numstat", "-M"];
        a.extend_from_slice(args);
        let out = run(&self.dir, &a)?;
        let mut files = Vec::new();
        for line in out.lines() {
            let mut it = line.split('\t');
            let (add, del, path) = (
                it.next().unwrap_or(""),
                it.next().unwrap_or(""),
                it.next().unwrap_or(""),
            );
            let path = path
                .rsplit(" => ")
                .next()
                .unwrap_or(path)
                .trim_end_matches('}')
                .to_owned();
            files.push(DiffFile {
                path,
                additions: add.parse().ok(),
                deletions: del.parse().ok(),
            });
        }
        Ok(files)
    }

    /// Typed diff between two revisions.
    pub fn diff(&self, base: &str, target: &str) -> Result<Diff> {
        let range = format!("{base}..{target}");
        let files = self.numstat(&[&range])?;
        let unified = run(&self.dir, &["diff", "-M", &range])?;
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
        let unified = run(&self.dir, &["diff", "-M", "HEAD"])?;
        Ok(Diff {
            base: "HEAD".into(),
            target: "WORKTREE".into(),
            files,
            unified,
        })
    }

    /// Merge base.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<String> {
        Ok(run(&self.dir, &["merge-base", a, b])?.trim().to_owned())
    }

    /// Start a merge transaction: merge `source` into this worktree's branch
    /// without committing; conflicts are evidence, not an error.
    pub fn merge_begin(&self, id: &str, source: &str) -> Result<MergeTransaction> {
        let target_branch = self
            .current_branch()?
            .ok_or_else(|| Error::Parse("merge target must be on a branch".into()))?;
        let target_before = self.head()?;
        let base = self.merge_base(&target_before, source)?;
        let source_sha = run(&self.dir, &["rev-parse", "--verify", source])?
            .trim()
            .to_owned();
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
                source,
            ],
        );
        let conflicts: Vec<String> = run(&self.dir, &["diff", "--name-only", "--diff-filter=U"])?
            .lines()
            .map(str::to_owned)
            .collect();
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
        let _ = run(&self.dir, &["merge", "--abort"]);
        run(&self.dir, &["reset", "-q", "--hard", &tx.target_before])?;
        tx.state = MergeState::Aborted;
        Ok(())
    }

    /// Capture the dirty worktree (tracked changes and untracked files) as a
    /// commit under `refs/modbit/snapshots/<id>` using a temporary index;
    /// HEAD, the index and the worktree are untouched.
    pub fn snapshot_dirty(&self, id: &str) -> Result<Snapshot> {
        let parent = self.head()?;
        let paths: Vec<String> = self.status()?.into_iter().map(|e| e.path).collect();
        let git_dir = run(&self.dir, &["rev-parse", "--git-dir"])?
            .trim()
            .to_owned();
        let tmp_index = if Path::new(&git_dir).is_absolute() {
            PathBuf::from(&git_dir).join(format!("modbit-snapshot-index-{}", std::process::id()))
        } else {
            self.dir
                .join(&git_dir)
                .join(format!("modbit-snapshot-index-{}", std::process::id()))
        }
        .to_string_lossy()
        .into_owned();
        let _ = &tmp_index;
        let with_index = |args: &[&str]| -> Result<String> {
            let out = Command::new("git")
                .arg("-C")
                .arg(&self.dir)
                .args(args)
                .env("GIT_INDEX_FILE", &tmp_index)
                .env("LC_ALL", "C")
                .output()
                .map_err(Error::Spawn)?;
            if !out.status.success() {
                return Err(Error::Git {
                    args: args.iter().map(|s| (*s).to_owned()).collect(),
                    code: out.status.code(),
                    stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
                });
            }
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        };
        with_index(&["read-tree", "HEAD"])?;
        with_index(&["add", "-A", "."])?;
        let tree = with_index(&["write-tree"])?.trim().to_owned();
        let _ = std::fs::remove_file(&tmp_index);
        let commit = run(
            &self.dir,
            &[
                "-c",
                "user.name=Modbit",
                "-c",
                "user.email=modbit@localhost",
                "commit-tree",
                &tree,
                "-p",
                &parent,
                "-m",
                &format!("modbit snapshot {id}"),
            ],
        )?
        .trim()
        .to_owned();
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

    /// Whether a ref exists.
    pub fn ref_exists(&self, reference: &str) -> bool {
        run(&self.dir, &["rev-parse", "--verify", "-q", reference]).is_ok()
    }

    /// Read a file at a revision.
    pub fn show(&self, revision: &str, path: &str) -> Result<Vec<u8>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(["show", &format!("{revision}:{path}")])
            .output()
            .map_err(Error::Spawn)?;
        if !out.status.success() {
            return Err(Error::Git {
                args: vec!["show".into()],
                code: out.status.code(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            });
        }
        Ok(out.stdout)
    }
}
