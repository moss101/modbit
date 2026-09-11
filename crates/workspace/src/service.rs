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

/// One text edit located by the deterministic match ladder (REQ-EV-0015).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    /// Text to find (must match exactly once at some ladder tier).
    pub old: String,
    /// Replacement text.
    pub new: String,
}

/// Ladder tier at which an edit target was located.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchTier {
    /// Byte-exact unique match.
    Exact,
    /// Unique match after trimming each line's leading/trailing whitespace.
    WhitespaceRemap,
}

/// One operation of a multi-file transaction (REQ-EV-0016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeOp {
    /// Root-relative path.
    pub path: String,
    /// Operation.
    pub kind: ChangeOpKind,
    /// Precondition.
    pub pre: WritePrecondition,
}

/// Transaction operation kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeOpKind {
    /// Create with content.
    Create(Vec<u8>),
    /// Replace whole content.
    Replace(Vec<u8>),
    /// Replace whole content with exactly these bytes: no line-ending
    /// preservation. For restores from content-addressed objects
    /// (checkpoints, forks, undo), where the bytes are the truth.
    ReplaceExact(Vec<u8>),
    /// Ordered text edits through the match ladder.
    Edit(Vec<TextEdit>),
    /// Delete.
    Delete,
}

/// Locate `needle` in `content` through the ladder: exact → whitespace remap
/// → ambiguity/no-match error. Returns the byte range and the tier.
pub fn locate(content: &str, needle: &str, path: &str) -> Result<(usize, usize, MatchTier)> {
    if needle.is_empty() {
        return Err(Error::Invalid {
            path: path.into(),
            detail: "empty edit target".into(),
        });
    }
    let exact: Vec<usize> = content.match_indices(needle).map(|(i, _)| i).collect();
    match exact.len() {
        1 => return Ok((exact[0], exact[0] + needle.len(), MatchTier::Exact)),
        n if n > 1 => {
            return Err(Error::AmbiguousTarget {
                path: path.into(),
                occurrences: n,
                tier: "exact".into(),
            });
        }
        _ => {}
    }
    // Whitespace remap: compare line windows with each line trimmed.
    let needle_lines: Vec<&str> = needle.lines().map(str::trim).collect();
    let mut offsets = Vec::new();
    let mut pos = 0usize;
    for line in content.split_inclusive('\n') {
        offsets.push((
            pos,
            pos + line.len(),
            line.trim_end_matches(['\n', '\r']).trim(),
        ));
        pos += line.len();
    }
    let mut hits = Vec::new();
    if !needle_lines.is_empty() && needle_lines.len() <= offsets.len() {
        for i in 0..=offsets.len() - needle_lines.len() {
            if (0..needle_lines.len()).all(|k| offsets[i + k].2 == needle_lines[k]) {
                let start = offsets[i].0;
                let last = offsets[i + needle_lines.len() - 1];
                // Keep the trailing newline out of the range unless the needle ends with one.
                let end = if needle.ends_with('\n') {
                    last.1
                } else {
                    last.0 + content[last.0..last.1].trim_end_matches(['\n', '\r']).len()
                };
                hits.push((start, end));
            }
        }
    }
    match hits.len() {
        1 => Ok((hits[0].0, hits[0].1, MatchTier::WhitespaceRemap)),
        n if n > 1 => Err(Error::AmbiguousTarget {
            path: path.into(),
            occurrences: n,
            tier: "whitespace_remap".into(),
        }),
        _ => {
            // Contextual suggestion: the existing line sharing the longest common prefix
            // with the first non-empty needle line. Never applied, only reported.
            let first = needle_lines
                .iter()
                .find(|l| !l.is_empty())
                .copied()
                .unwrap_or("");
            let suggestion = offsets
                .iter()
                .map(|o| o.2)
                .filter(|l| !l.is_empty())
                .max_by_key(|l| {
                    l.chars()
                        .zip(first.chars())
                        .take_while(|(a, b)| a == b)
                        .count()
                })
                .filter(|l| {
                    l.chars()
                        .zip(first.chars())
                        .take_while(|(a, b)| a == b)
                        .count()
                        >= 4
                })
                .map(str::to_owned);
            Err(Error::NoMatch {
                path: path.into(),
                suggestion,
            })
        }
    }
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

    /// Apply the path policy to `path` without opening it (for callers that
    /// hand the resolved location to another effector, e.g. a process cwd).
    pub fn resolve(&self, path: &str) -> Result<ResolvedPath> {
        self.policy.check(path)
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
        // Tier C conformance (docs/76): replacing a text file keeps its
        // consistent line-ending style when the new text is plain UTF-8 text
        // in the other style (a whole-file flip would corrupt the diff).
        let preserved: Option<Vec<u8>> = match (
            std::fs::read(&r.absolute)
                .ok()
                .and_then(|b| String::from_utf8(b).ok()),
            std::str::from_utf8(bytes).ok(),
        ) {
            (Some(old), Some(new)) => {
                let style = LineEnding::of(&old);
                let fixed = style.apply(new);
                (fixed.as_bytes() != bytes).then(|| fixed.into_bytes())
            }
            _ => None,
        };
        let bytes: &[u8] = preserved.as_deref().unwrap_or(bytes);
        write_atomic(&self.tmp_for(&r.absolute), &r.absolute, bytes)?;
        self.commit(&r, "atomic_replace", before, Some(content_hash(bytes)))
    }

    /// `atomic_replace` without line-ending preservation: the bytes land
    /// exactly as given (a checkpoint's or an undo's recorded content).
    pub fn atomic_replace_exact(
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

    /// Ordered text edits through the match ladder (REQ-EV-0015/0016): each
    /// edit is located against the content as left by the previous one; a
    /// failure at step N writes nothing (the file is written once, atomically).
    pub fn edit_by_match(
        &mut self,
        path: &str,
        edits: &[TextEdit],
        pre: WritePrecondition,
    ) -> Result<(WorkspaceChange, Vec<MatchTier>)> {
        let r = self.policy.check(path)?;
        let before = self.check_precondition(&r, path, &pre)?;
        let Some(before_hash) = before else {
            return Err(Error::Precondition {
                path: path.into(),
                detail: "edit needs an existing file".into(),
            });
        };
        let original = std::fs::read(&r.absolute).map_err(|e| Self::io(&r.absolute, e))?;
        let mut content = String::from_utf8(original).map_err(|_| Error::Invalid {
            path: path.into(),
            detail: "edit needs UTF-8 text".into(),
        })?;
        let mut tiers = Vec::with_capacity(edits.len());
        let style = LineEnding::of(&content);
        for (i, e) in edits.iter().enumerate() {
            let (start, end, tier) =
                locate(&content, &e.old, path).map_err(|cause| Error::StepFailed {
                    step: i,
                    cause: Box::new(cause),
                    rolled_back: true,
                    restored: vec![],
                    unrestored: vec![],
                })?;
            // Tier C conformance (docs/76): an edit never corrupts the file's
            // line endings — inserted text takes the file's consistent style.
            content.replace_range(start..end, &style.apply(&e.new));
            tiers.push(tier);
        }
        write_atomic(&self.tmp_for(&r.absolute), &r.absolute, content.as_bytes())?;
        let change = self.commit(
            &r,
            "edit",
            Some(before_hash),
            Some(content_hash(content.as_bytes())),
        )?;
        Ok((change, tiers))
    }

    /// Multi-file transaction (REQ-EV-0016): operations run in order with
    /// per-step validation; a failure at step N restores every earlier path
    /// to its pre-transaction bytes and reports the step, or reports the
    /// explicit partial state when a restore itself fails.
    pub fn apply_transaction(&mut self, ops: &[ChangeOp]) -> Result<Vec<WorkspaceChange>> {
        let mut done: Vec<(String, Option<Vec<u8>>)> = Vec::new();
        let mut changes = Vec::new();
        for (i, op) in ops.iter().enumerate() {
            // Pre-step snapshot for rollback; policy failures surface from the step itself.
            let snapshot = match self.policy.check(&op.path) {
                Ok(r) => match std::fs::read(&r.absolute) {
                    Ok(b) => Some(b),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => {
                        return Err(Error::StepFailed {
                            step: i,
                            cause: Box::new(Self::io(&r.absolute, e)),
                            rolled_back: done.is_empty(),
                            restored: vec![],
                            unrestored: done.iter().map(|(p, _)| p.clone()).collect(),
                        });
                    }
                },
                Err(_) => None,
            };
            let result = match &op.kind {
                ChangeOpKind::Create(b) => self.create(&op.path, b, op.pre.clone()),
                ChangeOpKind::Replace(b) => self.atomic_replace(&op.path, b, op.pre.clone()),
                ChangeOpKind::ReplaceExact(b) => {
                    self.atomic_replace_exact(&op.path, b, op.pre.clone())
                }
                ChangeOpKind::Edit(edits) => self
                    .edit_by_match(&op.path, edits, op.pre.clone())
                    .map(|(c, _)| c),
                ChangeOpKind::Delete => self.delete(&op.path, op.pre.clone()),
            };
            match result {
                Ok(c) => {
                    done.push((op.path.clone(), snapshot));
                    changes.push(c);
                }
                Err(cause) => {
                    let (mut restored, mut unrestored) = (Vec::new(), Vec::new());
                    for (path, bytes) in done.iter().rev() {
                        let ok = match bytes {
                            Some(b) => self
                                .atomic_replace(path, b, WritePrecondition::default())
                                .is_ok(),
                            None => self.delete(path, WritePrecondition::default()).is_ok(),
                        };
                        if ok {
                            restored.push(path.clone());
                        } else {
                            unrestored.push(path.clone());
                        }
                    }
                    return Err(Error::StepFailed {
                        step: i,
                        rolled_back: unrestored.is_empty(),
                        cause: Box::new(cause),
                        restored,
                        unrestored,
                    });
                }
            }
        }
        Ok(changes)
    }
}

/// The consistent line-ending style of a text, if it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    /// Every newline is `\r\n`.
    Crlf,
    /// Every newline is a bare `\n`.
    Lf,
    /// No newline, or a mix: nothing is normalised.
    Undecided,
}

impl LineEnding {
    /// Classify a text.
    #[must_use]
    pub fn of(text: &str) -> Self {
        let crlf = text.matches("\r\n").count();
        let lf = text.matches('\n').count() - crlf;
        match (crlf, lf) {
            (0, 0) => Self::Undecided,
            (_, 0) => Self::Crlf,
            (0, _) => Self::Lf,
            _ => Self::Undecided,
        }
    }

    /// Rewrite `text`'s newlines into this style (no-op when undecided).
    #[must_use]
    pub fn apply(self, text: &str) -> String {
        match self {
            Self::Crlf => text.replace("\r\n", "\n").replace('\n', "\r\n"),
            Self::Lf => text.replace("\r\n", "\n"),
            Self::Undecided => text.to_owned(),
        }
    }
}
