//! Typed file operations (docs/20 "Workspace File Service").

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::paths::{PathPolicy, ResolvedPath};
use crate::revision::{RevisionStore, WorkspaceRevision};
use crate::{Error, Result};

/// Kind of a directory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// Regular file.
    File,
    /// Directory.
    Dir,
    /// Symbolic link (target not followed in listings).
    Symlink,
    /// Anything else.
    Other,
}

/// `stat` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStat {
    /// Root-relative path.
    pub path: String,
    /// Kind.
    pub kind: EntryKind,
    /// Size in bytes (files).
    pub size: u64,
    /// sha256 of the content (files only).
    pub content_hash: Option<String>,
    /// Workspace revision at observation.
    pub workspace_revision: u64,
}

/// `read` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRead {
    /// Root-relative path.
    pub path: String,
    /// Content.
    pub bytes: Vec<u8>,
    /// sha256 of `bytes`: the file revision every CodeReference binds to.
    pub content_hash: String,
    /// Workspace revision at observation.
    pub workspace_revision: u64,
}

/// Optimistic precondition for a write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WritePrecondition {
    /// Required current content hash (`Some("")`-free: use `expect_absent` for creates).
    pub expected_content_hash: Option<String>,
    /// Required current workspace revision.
    pub expected_workspace_revision: Option<u64>,
    /// The path must not exist.
    pub expect_absent: bool,
}

/// One byte-range replacement inside `apply_patch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    /// Start byte (inclusive).
    pub start: usize,
    /// End byte (exclusive).
    pub end: usize,
    /// Replacement bytes.
    pub replacement: Vec<u8>,
}

/// `apply_patch` request: ordered, non-overlapping byte-range edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPatch {
    /// Edits applied against the original content; must be ascending and non-overlapping.
    pub edits: Vec<Edit>,
}

/// A typed change record produced by every successful write (basis of the
/// revision-bound change events of REQ-EV-0106).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceChange {
    /// Root-relative path.
    pub path: String,
    /// Operation name.
    pub op: String,
    /// Content hash before (files).
    pub before_hash: Option<String>,
    /// Content hash after (files).
    pub after_hash: Option<String>,
    /// Workspace revision after the change.
    pub workspace_revision: WorkspaceRevision,
}

/// The file service for one workspace root.
#[derive(Debug)]
pub struct WorkspaceService {
    policy: PathPolicy,
    store: RevisionStore,
    revision: WorkspaceRevision,
}

/// sha256 hex.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Write `bytes` to `final_path` atomically via `tmp` (write, fsync, rename).
pub fn write_atomic(tmp: &Path, final_path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let io = |e: std::io::Error| Error::Io {
        path: final_path.display().to_string(),
        source: e,
    };
    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    {
        let mut f = std::fs::File::create(tmp).map_err(io)?;
        f.write_all(bytes).map_err(io)?;
        f.sync_all().map_err(io)?;
    }
    if let Err(e) = std::fs::rename(tmp, final_path) {
        let _ = std::fs::remove_file(tmp);
        return Err(io(e));
    }
    if let Some(parent) = final_path.parent()
        && let Ok(d) = std::fs::File::open(parent)
    {
        let _ = d.sync_all();
    }
    Ok(())
}

impl WorkspaceService {
    /// Open the service for `root`, keeping revision state under `state_dir`
    /// (never inside the worktree). `extra_protected` adds policy patterns.
    pub fn open(root: &Path, state_dir: &Path, extra_protected: &[String]) -> Result<Self> {
        let policy = PathPolicy::new(root, extra_protected)?;
        let worktree_id = WorkspaceRevision::worktree_id_for(policy.root());
        let store = RevisionStore::open(state_dir, &worktree_id)?;
        let revision = store.load_or_init(policy.root())?;
        Ok(Self {
            policy,
            store,
            revision,
        })
    }

    /// Current revision.
    #[must_use]
    pub fn revision(&self) -> &WorkspaceRevision {
        &self.revision
    }

    /// Canonical root.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.policy.root()
    }

    fn io(path: &Path, e: std::io::Error) -> Error {
        Error::Io {
            path: path.display().to_string(),
            source: e,
        }
    }

    /// `read`.
    pub fn read(&self, path: &str) -> Result<FileRead> {
        let r = self.policy.check(path)?;
        let meta = std::fs::metadata(&r.absolute).map_err(|e| Self::io(&r.absolute, e))?;
        if !meta.is_file() {
            return Err(Error::Invalid {
                path: path.into(),
                detail: "not a regular file".into(),
            });
        }
        let bytes = std::fs::read(&r.absolute).map_err(|e| Self::io(&r.absolute, e))?;
        Ok(FileRead {
            path: r.relative,
            content_hash: content_hash(&bytes),
            bytes,
            workspace_revision: self.revision.number,
        })
    }

    /// `stat`.
    pub fn stat(&self, path: &str) -> Result<FileStat> {
        let r = self.policy.check(path)?;
        let meta = std::fs::symlink_metadata(&r.absolute).map_err(|e| Self::io(&r.absolute, e))?;
        let ft = meta.file_type();
        let kind = if ft.is_dir() {
            EntryKind::Dir
        } else if ft.is_file() {
            EntryKind::File
        } else if ft.is_symlink() {
            EntryKind::Symlink
        } else {
            EntryKind::Other
        };
        let content_hash = if kind == EntryKind::File {
            Some(content_hash(
                &std::fs::read(&r.absolute).map_err(|e| Self::io(&r.absolute, e))?,
            ))
        } else {
            None
        };
        Ok(FileStat {
            path: r.relative,
            kind,
            size: meta.len(),
            content_hash,
            workspace_revision: self.revision.number,
        })
    }

    /// `list` a directory (non-recursive, sorted); symlinks are reported, not followed.
    pub fn list(&self, path: &str) -> Result<Vec<FileStat>> {
        let r = self.policy.check(path)?;
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&r.absolute).map_err(|e| Self::io(&r.absolute, e))? {
            let entry = entry.map_err(|e| Self::io(&r.absolute, e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if r.relative.is_empty() {
                name.clone()
            } else {
                format!("{}/{name}", r.relative)
            };
            // Protected children are listed by name only (kind Other) so the
            // agent knows they exist but cannot open them.
            match self.stat(&rel) {
                Ok(s) => out.push(s),
                Err(Error::Protected { .. }) => out.push(FileStat {
                    path: rel,
                    kind: EntryKind::Other,
                    size: 0,
                    content_hash: None,
                    workspace_revision: self.revision.number,
                }),
                Err(e) => return Err(e),
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn check_precondition(
        &self,
        r: &ResolvedPath,
        given: &str,
        pre: &WritePrecondition,
    ) -> Result<Option<String>> {
        if let Some(expected) = pre.expected_workspace_revision
            && expected != self.revision.number
        {
            return Err(Error::Precondition {
                path: given.into(),
                detail: format!(
                    "expected workspace revision {expected}, current {}",
                    self.revision.number
                ),
            });
        }
        let current = match std::fs::metadata(&r.absolute) {
            Ok(m) if m.is_file() => Some(content_hash(
                &std::fs::read(&r.absolute).map_err(|e| Self::io(&r.absolute, e))?,
            )),
            Ok(_) => {
                return Err(Error::Invalid {
                    path: given.into(),
                    detail: "target exists and is not a regular file".into(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(Self::io(&r.absolute, e)),
        };
        if pre.expect_absent && current.is_some() {
            return Err(Error::Precondition {
                path: given.into(),
                detail: "expected the path to be absent".into(),
            });
        }
        if let Some(expected) = &pre.expected_content_hash {
            match &current {
                Some(c) if c == expected => {}
                Some(c) => {
                    return Err(Error::Precondition {
                        path: given.into(),
                        detail: format!("expected content hash {expected}, current {c}"),
                    });
                }
                None => {
                    return Err(Error::Precondition {
                        path: given.into(),
                        detail: "expected an existing file".into(),
                    });
                }
            }
        }
        Ok(current)
    }

    fn tmp_for(&self, absolute: &Path) -> PathBuf {
        let name = absolute
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        absolute.with_file_name(format!(".modbit-tmp-{}-{name}", std::process::id()))
    }

    fn commit(
        &mut self,
        r: &ResolvedPath,
        op: &str,
        before: Option<String>,
        after: Option<String>,
    ) -> Result<WorkspaceChange> {
        let next = self.store.advance(
            &self.revision,
            self.policy.root(),
            &r.relative,
            after.as_deref(),
        )?;
        self.revision = next.clone();
        Ok(WorkspaceChange {
            path: r.relative.clone(),
            op: op.into(),
            before_hash: before,
            after_hash: after,
            workspace_revision: next,
        })
    }

    /// `create`: the path must not exist (unless the precondition says otherwise).
    pub fn create(
        &mut self,
        path: &str,
        bytes: &[u8],
        mut pre: WritePrecondition,
    ) -> Result<WorkspaceChange> {
        pre.expect_absent = true;
        let r = self.policy.check(path)?;
        self.check_precondition(&r, path, &pre)?;
        write_atomic(&self.tmp_for(&r.absolute), &r.absolute, bytes)?;
        let after = content_hash(bytes);
        self.commit(&r, "create", None, Some(after))
    }

    /// `atomic_replace`: whole-file replacement under precondition.
    pub fn atomic_replace(
        &mut self,
        path: &str,
        bytes: &[u8],
        pre: WritePrecondition,
    ) -> Result<WorkspaceChange> {
        let r = self.policy.check(path)?;
        let before = self.check_precondition(&r, path, &pre)?;
        write_atomic(&self.tmp_for(&r.absolute), &r.absolute, bytes)?;
        self.commit(&r, "atomic_replace", before, Some(content_hash(bytes)))
    }

    /// `apply_patch`: ordered non-overlapping byte-range edits against the
    /// current content, validated as a whole before anything is written.
    pub fn apply_patch(
        &mut self,
        path: &str,
        patch: &ApplyPatch,
        pre: WritePrecondition,
    ) -> Result<WorkspaceChange> {
        let r = self.policy.check(path)?;
        let before = self.check_precondition(&r, path, &pre)?;
        let Some(before_hash) = before else {
            return Err(Error::Precondition {
                path: path.into(),
                detail: "apply_patch needs an existing file".into(),
            });
        };
        let original = std::fs::read(&r.absolute).map_err(|e| Self::io(&r.absolute, e))?;
        let mut out = Vec::with_capacity(original.len());
        let mut cursor = 0usize;
        for (i, e) in patch.edits.iter().enumerate() {
            if e.start > e.end || e.end > original.len() || e.start < cursor {
                return Err(Error::EditOutOfBounds {
                    path: path.into(),
                    detail: format!(
                        "edit {i} [{}..{}) against {} bytes (cursor {cursor})",
                        e.start,
                        e.end,
                        original.len()
                    ),
                });
            }
            out.extend_from_slice(&original[cursor..e.start]);
            out.extend_from_slice(&e.replacement);
            cursor = e.end;
        }
        out.extend_from_slice(&original[cursor..]);
        write_atomic(&self.tmp_for(&r.absolute), &r.absolute, &out)?;
        self.commit(
            &r,
            "apply_patch",
            Some(before_hash),
            Some(content_hash(&out)),
        )
    }

    /// `delete` a file under precondition.
    pub fn delete(&mut self, path: &str, pre: WritePrecondition) -> Result<WorkspaceChange> {
        let r = self.policy.check(path)?;
        let before = self.check_precondition(&r, path, &pre)?;
        if before.is_none() {
            return Err(Error::Precondition {
                path: path.into(),
                detail: "delete needs an existing file".into(),
            });
        }
        std::fs::remove_file(&r.absolute).map_err(|e| Self::io(&r.absolute, e))?;
        self.commit(&r, "delete", before, None)
    }

    /// `mkdir` (recursive).
    pub fn mkdir(&mut self, path: &str) -> Result<WorkspaceChange> {
        let r = self.policy.check(path)?;
        std::fs::create_dir_all(&r.absolute).map_err(|e| Self::io(&r.absolute, e))?;
        self.commit(&r, "mkdir", None, None)
    }

    /// `move` a file (both ends policy-checked; destination must be absent).
    pub fn r#move(
        &mut self,
        from: &str,
        to: &str,
        pre: WritePrecondition,
    ) -> Result<WorkspaceChange> {
        let src = self.policy.check(from)?;
        let dst = self.policy.check(to)?;
        let before = self.check_precondition(&src, from, &pre)?;
        if before.is_none() {
            return Err(Error::Precondition {
                path: from.into(),
                detail: "move needs an existing file".into(),
            });
        }
        if dst.absolute.exists() {
            return Err(Error::Precondition {
                path: to.into(),
                detail: "destination exists".into(),
            });
        }
        if let Some(parent) = dst.absolute.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Self::io(parent, e))?;
        }
        std::fs::rename(&src.absolute, &dst.absolute).map_err(|e| Self::io(&src.absolute, e))?;
        self.commit(&src, "move_from", before.clone(), None)?;
        self.commit(&dst, "move_to", None, before)
    }
}
