//! The git operations of the desktop Changes app (PX-048): what is staged,
//! staging and unstaging by path, committing exactly the staged set, how many
//! commits a remote lacks, and the push of the current branch with
//! caller-supplied environment (the credential broker's header travels there,
//! never on a command line). Every process is built by the one hardened
//! runner, like the rest of this crate.

use sha2::{Digest, Sha256};

use crate::{Error, Repo, Result, harden, run, run_bytes};

/// One path the index holds a change for, relative to `HEAD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedEntry {
    /// Repository-relative path.
    pub path: String,
    /// `A` added, `M` modified or `D` deleted in the index.
    pub status: char,
    /// The blob the index holds for the path (40 zeros when deleted).
    pub blob: String,
}

/// The staged set and a digest that binds it: two reads answer the same digest
/// exactly when the same paths hold the same index content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedSet {
    /// Entries sorted by path.
    pub entries: Vec<StagedEntry>,
    /// sha256 over `status path blob` of every entry.
    pub digest: String,
}

impl Repo {
    /// The index's changes against `HEAD` (renames are not detected: a rename
    /// is a delete and an add, as in [`Repo::status`]).
    pub fn staged(&self) -> Result<StagedSet> {
        let args: &[&str] = if self.is_unborn() {
            // No HEAD: the index against the empty tree.
            &[
                "diff-index",
                "--cached",
                "--raw",
                "-z",
                "--no-renames",
                "--abbrev=40",
                crate::EMPTY_TREE,
            ]
        } else {
            &[
                "diff-index",
                "--cached",
                "--raw",
                "-z",
                "--no-renames",
                "--abbrev=40",
                "HEAD",
            ]
        };
        let out = run_bytes(&self.dir, args, &[])?;
        let mut fields = out.split(|b| *b == 0).filter(|f| !f.is_empty());
        let mut entries = Vec::new();
        while let Some(meta) = fields.next() {
            let meta = String::from_utf8_lossy(meta);
            let Some(path) = fields.next() else {
                return Err(Error::Parse(format!("diff-index record `{meta}`")));
            };
            // `:<old mode> <new mode> <old sha> <new sha> <status>`
            let parts: Vec<&str> = meta.trim_start_matches(':').split(' ').collect();
            if parts.len() < 5 {
                return Err(Error::Parse(format!("diff-index record `{meta}`")));
            }
            let status = parts[4].chars().next().unwrap_or('M');
            entries.push(StagedEntry {
                path: String::from_utf8_lossy(path).into_owned(),
                status: if matches!(status, 'A' | 'D') {
                    status
                } else {
                    'M'
                },
                blob: parts[3].to_owned(),
            });
        }
        // An intent-to-add entry (`add -N`, which the review does for new files
        // so they appear in its diff) is an addition in the raw diff but is not
        // staged: only the status's index column says which paths are.
        let staged: std::collections::HashSet<String> = self
            .status()?
            .into_iter()
            .filter(|e| !e.code.starts_with(['.', '?']))
            .map(|e| e.path)
            .collect();
        entries.retain(|e| staged.contains(&e.path));
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let mut h = Sha256::new();
        for e in &entries {
            h.update(format!("{} {} {}\n", e.status, e.path, e.blob).as_bytes());
        }
        Ok(StagedSet {
            entries,
            digest: hex::encode(h.finalize()),
        })
    }

    /// Stage the given paths exactly as the working tree holds them now
    /// (additions, modifications and deletions); each is a literal path.
    pub fn stage_paths(&self, paths: &[&str]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["add", "-A", "--"];
        args.extend_from_slice(paths);
        run(&self.dir, &args).map(|_| ())
    }

    /// Take the given paths out of the index, leaving the working tree alone.
    pub fn unstage_paths(&self, paths: &[&str]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = if self.is_unborn() {
            vec!["rm", "--cached", "-r", "-q", "--ignore-unmatch", "--"]
        } else {
            vec!["reset", "-q", "HEAD", "--"]
        };
        args.extend_from_slice(paths);
        run(&self.dir, &args).map(|_| ())
    }

    /// Commit exactly what the index holds (no `-a`, no pathspec, no hooks),
    /// and return the new commit. The configured identity is used; where the
    /// repository has none the commit is authored as Modbit.
    pub fn commit_staged(&self, message: &str) -> Result<String> {
        let has_identity = run(&self.dir, &["config", "--get", "user.email"])
            .is_ok_and(|s| !s.trim().is_empty())
            && run(&self.dir, &["config", "--get", "user.name"])
                .is_ok_and(|s| !s.trim().is_empty());
        let mut args = Vec::new();
        if !has_identity {
            args.extend([
                "-c",
                "user.name=Modbit",
                "-c",
                "user.email=modbit@localhost",
            ]);
        }
        args.extend(["commit", "-q", "--no-verify", "-m", message]);
        run(&self.dir, &args)?;
        self.head()
    }

    /// The names of the configured remotes.
    pub fn remote_names(&self) -> Result<Vec<String>> {
        Ok(run(&self.dir, &["remote"])?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// How many commits `HEAD` has that no remote-tracking branch of `remote`
    /// holds (what a push would send, as far as this repository last saw).
    pub fn unpushed_count(&self, remote: &str) -> Result<u32> {
        harden::validate_remote(remote)?;
        if self.is_unborn() {
            return Ok(0);
        }
        let pattern = format!("--remotes={remote}");
        let out = run(
            &self.dir,
            &["rev-list", "--count", "HEAD", "--not", &pattern],
        )?;
        out.trim()
            .parse()
            .map_err(|_| Error::Parse(format!("rev-list count `{}`", out.trim())))
    }

    /// The subject line of the commit at `revision`.
    pub fn subject_of(&self, revision: &str) -> Result<String> {
        let rev = self.resolve(revision)?;
        Ok(run(
            &self.dir,
            &["log", "-1", "--format=%s", "--end-of-options", &rev],
        )?
        .trim()
        .to_owned())
    }

    /// The paths the commits `HEAD` has beyond the remote-tracking branches of
    /// `remote` touched.
    pub fn unpushed_paths(&self, remote: &str) -> Result<Vec<String>> {
        harden::validate_remote(remote)?;
        let pattern = format!("--remotes={remote}");
        let out = run_bytes(
            &self.dir,
            &[
                "log",
                "--format=",
                "--name-only",
                "-z",
                "--no-renames",
                "HEAD",
                "--not",
                &pattern,
            ],
            &[],
        )?;
        let mut paths: Vec<String> = out
            .split(|b| *b == 0)
            .filter(|f| !f.is_empty())
            .map(|f| String::from_utf8_lossy(f).trim_matches('\n').to_owned())
            .filter(|p| !p.is_empty())
            .collect();
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// Push `branch` to the same-named branch of `remote` (never forced),
    /// with `envs` set for this one process.
    pub fn push_with_env(
        &self,
        remote: &str,
        branch: &str,
        envs: &[(&str, &str)],
    ) -> Result<String> {
        crate::check_branch_name(&self.dir, branch)?;
        self.configured_remote(remote)?;
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        let args = [
            "push",
            "--porcelain",
            "--no-verify",
            "--end-of-options",
            remote,
            &refspec,
        ];
        run_bytes(&self.dir, &args, envs).map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}
