//! `WorkspaceRevision` (docs/20): monotonic, bound to Git HEAD, worktree
//! identity and a content fingerprint of files changed since the last Git
//! state. Persisted atomically outside the worktree so the worktree is never
//! polluted and the revision survives Core restarts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// The revision record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRevision {
    /// Monotonic number, starting at 1 for a freshly observed workspace.
    pub number: u64,
    /// Git HEAD commit at the time of the last observation (`None` when not a repo).
    pub git_head: Option<String>,
    /// Stable identity of the worktree (sha256 of the canonical root path).
    pub worktree_id: String,
    /// sha256 over the sorted (path, content-hash) pairs of files this service
    /// changed since `git_head` was last observed.
    pub fingerprint: String,
    /// Files changed by the service since `git_head` (path → content hash).
    #[serde(default)]
    pub changed: BTreeMap<String, String>,
}

impl WorkspaceRevision {
    fn compute_fingerprint(changed: &BTreeMap<String, String>) -> String {
        let mut h = Sha256::new();
        for (p, c) in changed {
            h.update(p.as_bytes());
            h.update([0]);
            h.update(c.as_bytes());
            h.update([0]);
        }
        hex::encode(h.finalize())
    }

    /// Identity for a root.
    #[must_use]
    pub fn worktree_id_for(root: &Path) -> String {
        hex::encode(Sha256::digest(root.to_string_lossy().as_bytes()))
    }
}

/// Reads the Git HEAD of `root` with the real `git` binary; `None` when not a
/// repository or git is unavailable.
#[must_use]
pub fn git_head(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    (s.len() == 40).then(|| s.to_owned())
}

/// Persistent revision store: one JSON file per worktree under `dir`.
#[derive(Debug, Clone)]
pub struct RevisionStore {
    path: PathBuf,
}

impl RevisionStore {
    /// Store for the worktree with `worktree_id` under `dir`.
    pub fn open(dir: &Path, worktree_id: &str) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|e| Error::Io {
            path: dir.display().to_string(),
            source: e,
        })?;
        Ok(Self {
            path: dir.join(format!("{worktree_id}.revision.json")),
        })
    }

    /// Load the current record or start at 1 for `root`.
    pub fn load_or_init(&self, root: &Path) -> Result<WorkspaceRevision> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| Error::Invalid {
                path: self.path.display().to_string(),
                detail: format!("corrupt revision record: {e}"),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let rev = WorkspaceRevision {
                    number: 1,
                    git_head: git_head(root),
                    worktree_id: WorkspaceRevision::worktree_id_for(root),
                    fingerprint: WorkspaceRevision::compute_fingerprint(&BTreeMap::new()),
                    changed: BTreeMap::new(),
                };
                self.save(&rev)?;
                Ok(rev)
            }
            Err(e) => Err(Error::Io {
                path: self.path.display().to_string(),
                source: e,
            }),
        }
    }

    /// Advance after a change to `relative` with `content_hash` (or removal).
    pub fn advance(
        &self,
        rev: &WorkspaceRevision,
        root: &Path,
        relative: &str,
        content_hash: Option<&str>,
    ) -> Result<WorkspaceRevision> {
        let head = git_head(root);
        let mut changed = if head == rev.git_head {
            rev.changed.clone()
        } else {
            BTreeMap::new()
        };
        match content_hash {
            Some(h) => {
                changed.insert(relative.to_owned(), h.to_owned());
            }
            None => {
                changed.insert(relative.to_owned(), "deleted".to_owned());
            }
        }
        let next = WorkspaceRevision {
            number: rev.number + 1,
            git_head: head,
            worktree_id: rev.worktree_id.clone(),
            fingerprint: WorkspaceRevision::compute_fingerprint(&changed),
            changed,
        };
        self.save(&next)?;
        Ok(next)
    }

    fn save(&self, rev: &WorkspaceRevision) -> Result<()> {
        let tmp = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        let bytes = serde_json::to_vec_pretty(rev).map_err(|e| Error::Invalid {
            path: self.path.display().to_string(),
            detail: e.to_string(),
        })?;
        crate::service::write_atomic(&tmp, &self.path, &bytes)
    }
}
