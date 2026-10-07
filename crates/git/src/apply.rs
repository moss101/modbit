//! Apply-back of a worktree result to the user's checkout (PX-066, docs/65
//! AFW-J08).
//!
//! A task that ran in its own worktree leaves a *candidate*: a commit holding
//! the worktree's whole state (committed work and dirty files, from
//! [`crate::Repo::snapshot_dirty`]) beside the *base* commit the worktree was
//! made from. Applying it to the user's checkout is a file-level three-way
//! operation per changed path, in four separately testable steps:
//!
//! 1. [`plan`] reads the checkout (read-only) and classifies every path the
//!    task changed: `Clean` (the checkout still holds the base), `AlreadyApplied`
//!    (it already holds the candidate), `AutoMerge` (both sides changed
//!    disjoint regions of a text file; `git merge-file` merges them with no
//!    marker), `Conflict` (anything else the user changed) or `Unsupported`
//!    (symlink, submodule); a protected path is flagged and written only when
//!    the confirmation names it. The plan's
//!    [`ApplyPlan::digest`] binds the base, the candidate, the checkout's head
//!    and every per-path fact, so an approval of "apply this" is an approval of
//!    exactly this.
//! 2. [`prepare`] turns a plan and the user's choice into the writes, and
//!    captures the exact pre-apply bytes of every path it will touch into
//!    [`Blobs`] (the content-addressed store) as a [`Manifest`]: the
//!    pre-apply checkpoint. It writes nothing to the checkout.
//! 3. [`execute`] performs the writes, one atomic rename at a time.
//! 4. [`restore`] puts the manifest's pre-apply bytes back: for Undo (all or
//!    nothing, and a path the user edited since is a reported conflict that
//!    destroys nothing) and for crash recovery (best effort, per path).
//!
//! Nothing here ever chooses an overwrite on the caller's behalf: a plan with
//! conflicts and no explicit, confirmed option is a [`Refusal`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{EMPTY_TREE, Error, Repo, Result, run_bytes};

/// Content-addressed bytes (the Core's object store).
pub trait Blobs {
    /// Store `bytes`; returns the id [`Blobs::get`] takes.
    fn put(&self, bytes: &[u8]) -> std::result::Result<String, String>;
    /// The bytes stored under `id`.
    fn get(&self, id: &str) -> std::result::Result<Vec<u8>, String>;
}

/// Largest file the apply reads or writes.
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// What the task did to a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Change {
    /// The task created it.
    Add,
    /// The task changed it.
    Modify,
    /// The task deleted it.
    Delete,
}

/// Why a path conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictKind {
    /// Both changed the same text file in overlapping regions.
    BothModified,
    /// Both created it, differently.
    AddAdd,
    /// The task deleted it, the checkout changed it.
    DeleteModify,
    /// The task changed it, the checkout deleted it.
    ModifyDelete,
    /// Both changed it and it is not text a merge can mark.
    Binary,
    /// The checkout holds something else (a directory, a link) at the path.
    TypeChange,
}

/// How the checkout stands against the task's change of one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "SCREAMING_SNAKE_CASE",
    tag = "state",
    content = "conflict"
)]
pub enum EntryState {
    /// The checkout holds the base: the task's change applies as is.
    Clean,
    /// The checkout already holds the task's result.
    AlreadyApplied,
    /// Both sides changed disjoint regions; the merged result applies.
    AutoMerge,
    /// The user changed it too.
    Conflict(ConflictKind),
    /// A symlink or submodule change, which an apply does not carry.
    Unsupported,
}

impl EntryState {
    /// Stable label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Clean => "CLEAN",
            Self::AlreadyApplied => "ALREADY_APPLIED",
            Self::AutoMerge => "AUTO_MERGE",
            Self::Conflict(_) => "CONFLICT",
            Self::Unsupported => "UNSUPPORTED",
        }
    }
}

/// Which state of the checkout a plan reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum View {
    /// The files on disk, uncommitted edits included.
    Disk,
    /// `HEAD` as committed: what the checkout would hold if its dirty state
    /// were stashed first.
    Head,
}

/// One path of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    /// Root-relative, forward-slash path.
    pub path: String,
    /// What the task did.
    pub change: Change,
    /// How the checkout stands against it.
    pub state: EntryState,
    /// A protected path: written only when named in the confirmation.
    pub protected: bool,
    /// The base blob (absent for an add).
    pub base_oid: Option<String>,
    /// The candidate blob (absent for a delete).
    pub candidate_oid: Option<String>,
    /// The candidate's file mode (`100644`, `100755`).
    pub candidate_mode: Option<u32>,
    /// sha256 of what the checkout holds (absent: no file).
    pub checkout_sha256: Option<String>,
    /// A text three-way merge with conflict markers is available.
    pub markers_available: bool,
    #[serde(skip)]
    merged: Option<Vec<u8>>,
    #[serde(skip)]
    marked: Option<Vec<u8>>,
}

/// The read-only classification of an apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPlan {
    /// The checkout the plan is for.
    pub checkout: PathBuf,
    /// The base tree.
    pub base_tree: String,
    /// The candidate tree.
    pub candidate_tree: String,
    /// The candidate commit.
    pub candidate_commit: String,
    /// The checkout's `HEAD` (`None`: no commit yet).
    pub checkout_head: Option<String>,
    /// What was read.
    pub view: View,
    /// One entry per path the task changed, sorted by path.
    pub entries: Vec<PlanEntry>,
    /// The checkout's dirty paths (tracked changes and untracked files).
    pub dirty: Vec<String>,
    /// Binds every fact above.
    pub digest: String,
}

impl ApplyPlan {
    /// The paths in conflict.
    #[must_use]
    pub fn conflicts(&self) -> Vec<&PlanEntry> {
        self.entries
            .iter()
            .filter(|e| matches!(e.state, EntryState::Conflict(_)))
            .collect()
    }

    /// The protected paths the apply would write.
    #[must_use]
    pub fn protected(&self) -> Vec<&PlanEntry> {
        self.entries
            .iter()
            .filter(|e| e.protected && e.state != EntryState::AlreadyApplied)
            .collect()
    }
}

/// What the person chose for a plan with conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Choice {
    /// Do nothing (the default).
    Cancel,
    /// Apply what is clean and leave conflict markers in the conflicting text
    /// files for the person to resolve.
    MergeManually,
    /// Save the checkout's dirty state (tracked and untracked) as a stash
    /// snapshot, reset it to `HEAD`, then apply on the clean checkout.
    Stash,
    /// Replace the conflicting files with the task's version.
    Overwrite,
    /// Make every path the task changed or the checkout has dirty equal the
    /// task's tree.
    FullOverwrite,
}

impl Choice {
    /// Whether the choice can destroy bytes the pre-apply checkpoint keeps
    /// only for Undo: an overwrite.
    #[must_use]
    pub fn destructive(self) -> bool {
        matches!(self, Self::Overwrite | Self::FullOverwrite)
    }

    /// Parse the wire spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "" | "CANCEL" => Self::Cancel,
            "MERGE_MANUALLY" => Self::MergeManually,
            "STASH" => Self::Stash,
            "OVERWRITE" => Self::Overwrite,
            "FULL_OVERWRITE" => Self::FullOverwrite,
            _ => return None,
        })
    }

    /// The wire spelling.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Cancel => "CANCEL",
            Self::MergeManually => "MERGE_MANUALLY",
            Self::Stash => "STASH",
            Self::Overwrite => "OVERWRITE",
            Self::FullOverwrite => "FULL_OVERWRITE",
        }
    }
}

/// A request to turn a plan into writes.
#[derive(Debug, Clone)]
pub struct Request {
    /// The choice for conflicts.
    pub choice: Choice,
    /// The paths the person typed to confirm an overwrite or a protected
    /// write.
    pub confirm_paths: Vec<String>,
    /// This apply's id (names the stash snapshot and the temp files).
    pub apply_id: String,
}

/// Why a plan cannot be turned into writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// Stable code: `CONFLICTS`, `OVERWRITE_NOT_CONFIRMED`, `PROTECTED_PATHS`,
    /// `UNSUPPORTED_CHANGE`, `NO_MANUAL_MERGE`, `STASH_CONFLICTS`.
    pub code: &'static str,
    /// What to do about it.
    pub detail: String,
    /// The paths concerned.
    pub paths: Vec<String>,
}

/// A stored blob and the hash of its content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    /// The id in [`Blobs`].
    pub blob: String,
    /// sha256 of the content.
    pub sha256: String,
    /// Length.
    pub size: u64,
    /// The executable bit.
    pub exec: bool,
}

/// One path of the pre-apply checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Root-relative path.
    pub path: String,
    /// What the checkout held (`None`: no file).
    pub before: Option<BlobRef>,
    /// What the apply writes (`None`: the file is removed).
    pub after: Option<BlobRef>,
    /// Directories the apply creates for it (root-relative), outermost first.
    pub created_dirs: Vec<String>,
    /// Why the path is in the apply.
    pub reason: String,
}

/// The pre-apply checkpoint: exact bytes of every path an apply touches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// This apply.
    pub apply_id: String,
    /// The plan this realises.
    pub plan_digest: String,
    /// The choice.
    pub choice: Choice,
    /// The checkout's head before.
    pub checkout_head: Option<String>,
    /// The candidate tree.
    pub candidate_tree: String,
    /// The stash snapshot ref, when the choice stashed.
    pub stash_ref: Option<String>,
    /// The paths.
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone)]
enum Action {
    Put { bytes: Vec<u8>, exec: bool },
    Remove,
}

/// The writes of one apply, with their checkpoint.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// The pre-apply checkpoint (persist it before [`execute`]).
    pub manifest: Manifest,
    writes: Vec<(String, Action)>,
    /// Paths that end with conflict markers for the person to resolve.
    pub unresolved: Vec<String>,
}

impl Prepared {
    /// Number of writes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.writes.len()
    }

    /// No write at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
}

fn hex_sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn is_text(bytes: &[u8]) -> bool {
    !bytes[..bytes.len().min(8000)].contains(&0)
}

fn normalize_eol(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Same bytes, or the same text up to line endings (a checkout with
/// `core.autocrlf` holds CRLF where the blob holds LF).
fn same(a: &[u8], b: &[u8]) -> bool {
    a == b || (is_text(a) && is_text(b) && normalize_eol(a) == normalize_eol(b))
}

/// What a checkout path holds.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Disk {
    Absent,
    File { bytes: Vec<u8>, exec: bool },
    Other(String),
}

impl Disk {
    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::File { bytes, .. } => Some(bytes),
            _ => None,
        }
    }

    fn sha(&self) -> Option<String> {
        self.bytes().map(hex_sha)
    }
}

/// A root-relative path git reported: normal components only.
fn checked_rel(rel: &str) -> Result<()> {
    let bad = rel.is_empty()
        || Path::new(rel).is_absolute()
        || Path::new(rel)
            .components()
            .any(|c| !matches!(c, Component::Normal(n) if n != ".git" && n != ".GIT"));
    if bad {
        return Err(Error::InvalidRef {
            kind: "path".into(),
            value: rel.escape_debug().to_string(),
            reason: "is not a plain root-relative path".into(),
        });
    }
    Ok(())
}

/// Every ancestor of `root/rel` that exists is a real directory (a symlink
/// among them could lead a write out of the checkout).
fn no_link_in_path(root: &Path, rel: &str) -> Result<()> {
    let mut cur = root.to_path_buf();
    let comps: Vec<_> = Path::new(rel).components().collect();
    for (i, c) in comps.iter().enumerate() {
        cur.push(c);
        match std::fs::symlink_metadata(&cur) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(Error::InvalidRef {
                    kind: "path".into(),
                    value: rel.escape_debug().to_string(),
                    reason: format!("passes through the link `{}`", cur.display()),
                });
            }
            Ok(m) if i + 1 < comps.len() && !m.is_dir() => return Ok(()),
            Ok(_) => {}
            Err(_) => return Ok(()),
        }
    }
    Ok(())
}

fn read_disk(root: &Path, rel: &str) -> Result<Disk> {
    checked_rel(rel)?;
    no_link_in_path(root, rel)?;
    let full = root.join(rel);
    match std::fs::symlink_metadata(&full) {
        Ok(m) if m.is_file() => {
            if m.len() > MAX_FILE_BYTES {
                return Ok(Disk::Other(format!(
                    "{} bytes: too large to apply",
                    m.len()
                )));
            }
            let bytes = std::fs::read(&full).map_err(Error::Spawn)?;
            #[cfg(unix)]
            let exec = {
                use std::os::unix::fs::PermissionsExt;
                m.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let exec = false;
            Ok(Disk::File { bytes, exec })
        }
        Ok(m) if m.file_type().is_symlink() => Ok(Disk::Other("a symbolic link".into())),
        Ok(_) => Ok(Disk::Other("a directory".into())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Disk::Absent),
        // A file where a directory is expected, and the like.
        Err(e) => Ok(Disk::Other(e.to_string())),
    }
}

fn blob(repo: &Repo, oid: &str) -> Result<Vec<u8>> {
    if oid.is_empty() || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidRef {
            kind: "object".into(),
            value: oid.escape_debug().to_string(),
            reason: "is not an object id".into(),
        });
    }
    run_bytes(&repo.dir, &["cat-file", "blob", oid], &[])
}

/// `path` at `HEAD` (absent when HEAD has no such file).
fn head_file(repo: &Repo, path: &str) -> Result<Disk> {
    if repo.is_unborn() {
        return Ok(Disk::Absent);
    }
    // The blob and its mode, from the tree.
    let out = run_bytes(
        &repo.dir,
        &["ls-tree", "-z", "--end-of-options", "HEAD", "--", path],
        &[],
    )?;
    let rec = out.split(|b| *b == 0).find(|r| !r.is_empty());
    let Some(rec) = rec else {
        return Ok(Disk::Absent);
    };
    let text = String::from_utf8_lossy(rec);
    let (meta, _name) = text.split_once('\t').unwrap_or((&text, ""));
    let mut it = meta.split(' ');
    let (mode, kind, oid) = (
        it.next().unwrap_or(""),
        it.next().unwrap_or(""),
        it.next().unwrap_or(""),
    );
    if kind != "blob" || mode == "120000" {
        return Ok(Disk::Other("not a regular file at HEAD".into()));
    }
    Ok(Disk::File {
        bytes: blob(repo, oid)?,
        exec: mode == "100755",
    })
}

struct Merge {
    clean: bool,
    bytes: Vec<u8>,
}

/// `git merge-file -p` of `ours`, `base`, `theirs` (labels name the sides).
fn merge3(repo: &Repo, ours: &[u8], base: &[u8], theirs: &[u8]) -> Result<Merge> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "modbit-merge3-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(Error::Spawn)?;
    let result = (|| -> Result<Merge> {
        let (a, b, c) = (dir.join("ours"), dir.join("base"), dir.join("theirs"));
        std::fs::write(&a, ours).map_err(Error::Spawn)?;
        std::fs::write(&b, base).map_err(Error::Spawn)?;
        std::fs::write(&c, theirs).map_err(Error::Spawn)?;
        let (sa, sb, sc) = (
            crate::git_path(&a)?,
            crate::git_path(&b)?,
            crate::git_path(&c)?,
        );
        let args = [
            "merge-file",
            "-p",
            "-L",
            "checkout",
            "-L",
            "base",
            "-L",
            "task",
            &sa,
            &sb,
            &sc,
        ];
        let mut cmd = crate::harden::command(&repo.dir, &args)?;
        cmd.args(args);
        let out = cmd.output().map_err(Error::Spawn)?;
        match out.status.code() {
            Some(0) => Ok(Merge {
                clean: true,
                bytes: out.stdout,
            }),
            Some(n) if (1..=127).contains(&n) => Ok(Merge {
                clean: false,
                bytes: out.stdout,
            }),
            _ => Err(Error::Git {
                args: vec!["merge-file".into()],
                code: out.status.code(),
                stderr: crate::redact_url_credentials(String::from_utf8_lossy(&out.stderr).trim()),
            }),
        }
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

struct RawChange {
    path: String,
    src_mode: u32,
    dst_mode: u32,
    src_oid: String,
    dst_oid: String,
    status: char,
}

fn changes(repo: &Repo, base_tree: &str, candidate_tree: &str) -> Result<Vec<RawChange>> {
    let out = run_bytes(
        &repo.dir,
        &[
            "diff-tree",
            "-r",
            "--raw",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            base_tree,
            candidate_tree,
        ],
        &[],
    )?;
    let mut tokens = out.split(|b| *b == 0).filter(|t| !t.is_empty());
    let mut v = Vec::new();
    while let Some(meta) = tokens.next() {
        let meta = String::from_utf8_lossy(meta);
        let Some(meta) = meta.strip_prefix(':') else {
            return Err(Error::Parse(format!("diff-tree record `{meta}`")));
        };
        let path = tokens
            .next()
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .ok_or_else(|| Error::Parse("diff-tree path".into()))?;
        let f: Vec<&str> = meta.split(' ').collect();
        if f.len() < 5 {
            return Err(Error::Parse(format!("diff-tree record `{meta}`")));
        }
        let mode = |s: &str| u32::from_str_radix(s, 8).unwrap_or(0);
        let zero = |s: &str| s.bytes().all(|b| b == b'0');
        v.push(RawChange {
            path,
            src_mode: mode(f[0]),
            dst_mode: mode(f[1]),
            src_oid: if zero(f[2]) {
                String::new()
            } else {
                f[2].into()
            },
            dst_oid: if zero(f[3]) {
                String::new()
            } else {
                f[3].into()
            },
            status: f[4].chars().next().unwrap_or('M'),
        });
    }
    v.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(v)
}

/// Classify what applying the task's change would do to `checkout`.
///
/// `base` is the commit the worktree was made from (`None` or empty: the
/// worktree was an orphan of a repository with no commit, and the base is the
/// empty tree); `candidate` is the commit holding the worktree's whole state.
/// Read-only.
pub fn plan(
    checkout: &Repo,
    base: Option<&str>,
    candidate: &str,
    view: View,
    is_protected: &dyn Fn(&str) -> bool,
) -> Result<ApplyPlan> {
    let base_tree = match base {
        Some(b) if !b.is_empty() => checkout.tree_of(b)?,
        _ => EMPTY_TREE.to_owned(),
    };
    let candidate_commit = checkout.resolve(&format!("{candidate}^{{commit}}"))?;
    let candidate_tree = checkout.tree_of(&candidate_commit)?;
    let checkout_head = if checkout.is_unborn() {
        None
    } else {
        Some(checkout.head()?)
    };
    let dirty: Vec<String> = checkout.status()?.into_iter().map(|e| e.path).collect();
    let mut entries = Vec::new();
    for c in changes(checkout, &base_tree, &candidate_tree)? {
        checked_rel(&c.path)?;
        let change = match c.status {
            'A' => Change::Add,
            'D' => Change::Delete,
            _ => Change::Modify,
        };
        let special = |m: u32| m == 0o120_000 || m == 0o160_000 || m == 0o040_000;
        let mut entry = PlanEntry {
            path: c.path.clone(),
            change,
            state: EntryState::Clean,
            protected: is_protected(&c.path),
            base_oid: (!c.src_oid.is_empty()).then(|| c.src_oid.clone()),
            candidate_oid: (!c.dst_oid.is_empty()).then(|| c.dst_oid.clone()),
            candidate_mode: (!c.dst_oid.is_empty()).then_some(c.dst_mode),
            checkout_sha256: None,
            markers_available: false,
            merged: None,
            marked: None,
        };
        if special(c.src_mode) || special(c.dst_mode) {
            entry.state = EntryState::Unsupported;
            entries.push(entry);
            continue;
        }
        let disk = match view {
            View::Disk => read_disk(&checkout.dir, &c.path)?,
            View::Head => head_file(checkout, &c.path)?,
        };
        entry.checkout_sha256 = disk.sha();
        let base_bytes = match &entry.base_oid {
            Some(o) => Some(blob(checkout, o)?),
            None => None,
        };
        let cand_bytes = match &entry.candidate_oid {
            Some(o) => Some(blob(checkout, o)?),
            None => None,
        };
        // A mode-only change leaves the content equal on both sides.
        entry.state = match (&disk, &base_bytes, &cand_bytes) {
            (Disk::Other(_), _, _) => EntryState::Conflict(ConflictKind::TypeChange),
            // Add.
            (Disk::Absent, None, Some(_)) => EntryState::Clean,
            (Disk::File { bytes, .. }, None, Some(cand)) => {
                if same(bytes, cand) {
                    EntryState::AlreadyApplied
                } else if !is_text(bytes) || !is_text(cand) {
                    EntryState::Conflict(ConflictKind::Binary)
                } else {
                    let m = merge3(checkout, bytes, b"", cand)?;
                    entry.markers_available = true;
                    entry.marked = Some(m.bytes);
                    EntryState::Conflict(ConflictKind::AddAdd)
                }
            }
            // Delete.
            (Disk::Absent, Some(_), None) => EntryState::AlreadyApplied,
            (Disk::File { bytes, .. }, Some(base), None) => {
                if same(bytes, base) {
                    EntryState::Clean
                } else {
                    EntryState::Conflict(ConflictKind::DeleteModify)
                }
            }
            // Modify.
            (Disk::Absent, Some(_), Some(_)) => EntryState::Conflict(ConflictKind::ModifyDelete),
            (Disk::File { bytes, .. }, Some(base), Some(cand)) => {
                if same(bytes, base) {
                    EntryState::Clean
                } else if same(bytes, cand) {
                    EntryState::AlreadyApplied
                } else if !is_text(bytes) || !is_text(base) || !is_text(cand) {
                    EntryState::Conflict(ConflictKind::Binary)
                } else {
                    let m = merge3(checkout, bytes, base, cand)?;
                    if m.clean {
                        entry.merged = Some(m.bytes);
                        EntryState::AutoMerge
                    } else {
                        entry.markers_available = true;
                        entry.marked = Some(m.bytes);
                        EntryState::Conflict(ConflictKind::BothModified)
                    }
                }
            }
            // Absent on both sides of the diff cannot be reported.
            (_, None, None) => EntryState::AlreadyApplied,
        };
        entries.push(entry);
    }
    let mut h = Sha256::new();
    let mut line = |s: String| {
        h.update(s.as_bytes());
        h.update(*b"\n");
    };
    line(format!("base:{base_tree}"));
    line(format!("candidate:{candidate_tree}"));
    line(format!("head:{}", checkout_head.as_deref().unwrap_or("-")));
    line(format!("view:{view:?}"));
    for e in &entries {
        line(format!(
            "{}|{:?}|{}|{}|{}|{}|{}|{}",
            e.path,
            e.change,
            e.protected,
            serde_json::to_string(&e.state).unwrap_or_default(),
            e.base_oid.as_deref().unwrap_or("-"),
            e.candidate_oid.as_deref().unwrap_or("-"),
            e.checkout_sha256.as_deref().unwrap_or("-"),
            e.merged.as_deref().map(hex_sha).unwrap_or_default(),
        ));
    }
    let digest = hex::encode(h.finalize());
    Ok(ApplyPlan {
        checkout: checkout.dir.clone(),
        base_tree,
        candidate_tree,
        candidate_commit,
        checkout_head,
        view,
        entries,
        dirty,
        digest,
    })
}

fn refuse(code: &'static str, detail: impl Into<String>, paths: Vec<String>) -> Refusal {
    Refusal {
        code,
        detail: detail.into(),
        paths,
    }
}

fn blob_ref(blobs: &dyn Blobs, bytes: &[u8], exec: bool) -> std::result::Result<BlobRef, Refusal> {
    let blob = blobs
        .put(bytes)
        .map_err(|e| refuse("STORE", format!("pre-apply checkpoint: {e}"), vec![]))?;
    Ok(BlobRef {
        blob,
        sha256: hex_sha(bytes),
        size: bytes.len() as u64,
        exec,
    })
}

/// Turn `plan` and the person's `request` into the writes and the pre-apply
/// checkpoint. Writes nothing to the checkout (a `Stash` choice does record
/// the dirty state as a snapshot ref in the repository first).
///
/// `checkout` must be the repository `plan` was made for. `plan` is re-read
/// by the caller (and its digest compared with the approved one) right before
/// this call, so the bytes captured as "before" are the bytes the approval
/// was given over.
#[allow(clippy::too_many_lines)]
pub fn prepare(
    checkout: &Repo,
    plan: &ApplyPlan,
    request: &Request,
    blobs: &dyn Blobs,
) -> std::result::Result<Prepared, Refusal> {
    let confirmed: BTreeSet<&str> = request.confirm_paths.iter().map(String::as_str).collect();
    let unsupported: Vec<String> = plan
        .entries
        .iter()
        .filter(|e| e.state == EntryState::Unsupported)
        .map(|e| e.path.clone())
        .collect();
    if !unsupported.is_empty() {
        return Err(refuse(
            "UNSUPPORTED_CHANGE",
            "the task changed a symbolic link or a submodule, which an apply does not carry; \
             apply those by hand",
            unsupported,
        ));
    }
    let conflicts: Vec<&PlanEntry> = plan.conflicts();
    let protected: Vec<&PlanEntry> = plan.protected();
    if request.choice == Choice::Cancel {
        return Err(refuse(
            "CANCELLED",
            "cancelled: nothing was changed",
            conflicts.iter().map(|e| e.path.clone()).collect(),
        ));
    }
    // Protected paths are written only when the confirmation names them.
    let unconfirmed_protected: Vec<String> = protected
        .iter()
        .filter(|e| !confirmed.contains(e.path.as_str()))
        .map(|e| e.path.clone())
        .collect();
    if !unconfirmed_protected.is_empty() {
        return Err(refuse(
            "PROTECTED_PATHS",
            "the task changed protected paths; name each one in the confirmation to apply it",
            unconfirmed_protected,
        ));
    }
    // What each choice does with the conflicting paths.
    let mut writes: BTreeMap<String, (Action, &'static str)> = BTreeMap::new();
    let mut unresolved: Vec<String> = Vec::new();
    let mut overwritten: BTreeSet<String> = BTreeSet::new();
    let mut stash_ref = None;
    let candidate_action = |e: &PlanEntry| -> std::result::Result<Action, Refusal> {
        match &e.candidate_oid {
            Some(oid) => Ok(Action::Put {
                bytes: blob(checkout, oid)
                    .map_err(|er| refuse("GIT", er.to_string(), vec![e.path.clone()]))?,
                exec: e.candidate_mode == Some(0o100_755),
            }),
            None => Ok(Action::Remove),
        }
    };
    for e in &plan.entries {
        match e.state {
            EntryState::Clean => {
                writes.insert(e.path.clone(), (candidate_action(e)?, "CLEAN"));
            }
            EntryState::AutoMerge => {
                let bytes = e.merged.clone().unwrap_or_default();
                writes.insert(
                    e.path.clone(),
                    (
                        Action::Put {
                            bytes,
                            exec: e.candidate_mode == Some(0o100_755),
                        },
                        "AUTO_MERGE",
                    ),
                );
            }
            EntryState::AlreadyApplied | EntryState::Unsupported | EntryState::Conflict(_) => {}
        }
    }
    match request.choice {
        Choice::Cancel => {}
        Choice::MergeManually => {
            let no_markers: Vec<String> = conflicts
                .iter()
                .filter(|e| e.marked.is_none())
                .map(|e| e.path.clone())
                .collect();
            if !no_markers.is_empty() {
                return Err(refuse(
                    "NO_MANUAL_MERGE",
                    "these conflicts are not text a merge can mark (binary, or deleted on one side); \
                     choose an overwrite for them",
                    no_markers,
                ));
            }
            for e in &conflicts {
                writes.insert(
                    e.path.clone(),
                    (
                        Action::Put {
                            bytes: e.marked.clone().unwrap_or_default(),
                            exec: e.candidate_mode == Some(0o100_755),
                        },
                        "CONFLICT_MARKERS",
                    ),
                );
                unresolved.push(e.path.clone());
            }
        }
        Choice::Overwrite | Choice::FullOverwrite => {
            for e in &conflicts {
                overwritten.insert(e.path.clone());
                writes.insert(e.path.clone(), (candidate_action(e)?, "OVERWRITE"));
            }
            if request.choice == Choice::FullOverwrite {
                // Every dirty path the task did not change becomes the
                // candidate tree's.
                let touched: BTreeSet<&str> =
                    plan.entries.iter().map(|e| e.path.as_str()).collect();
                for p in &plan.dirty {
                    if touched.contains(p.as_str()) {
                        continue;
                    }
                    let action = match tree_blob(checkout, &plan.candidate_tree, p)? {
                        Some((bytes, exec)) => Action::Put { bytes, exec },
                        None => Action::Remove,
                    };
                    overwritten.insert(p.clone());
                    writes.insert(p.clone(), (action, "FULL_OVERWRITE"));
                }
            }
            let missing: Vec<String> = overwritten
                .iter()
                .filter(|p| !confirmed.contains(p.as_str()))
                .cloned()
                .collect();
            if !missing.is_empty() {
                return Err(refuse(
                    "OVERWRITE_NOT_CONFIRMED",
                    "an overwrite replaces the checkout's version: name every file it replaces \
                     in the confirmation",
                    missing,
                ));
            }
        }
        Choice::Stash => {
            if !conflicts.is_empty() {
                return Err(refuse(
                    "STASH_CONFLICTS",
                    "the task's change still conflicts with the committed checkout after a stash; \
                     merge manually or overwrite instead",
                    conflicts.iter().map(|e| e.path.clone()).collect(),
                ));
            }
            if plan.view != View::Head {
                return Err(refuse(
                    "STASH_VIEW",
                    "a stash apply is planned against HEAD",
                    vec![],
                ));
            }
            // Reset every dirty path to HEAD (the plan's own writes override).
            for p in &plan.dirty {
                if writes.contains_key(p) {
                    continue;
                }
                let action = match head_file(checkout, p)
                    .map_err(|er| refuse("GIT", er.to_string(), vec![p.clone()]))?
                {
                    Disk::File { bytes, exec } => Action::Put { bytes, exec },
                    _ => Action::Remove,
                };
                writes.insert(p.clone(), (action, "STASH_RESET"));
            }
            if !plan.dirty.is_empty() {
                let snap = checkout
                    .snapshot_dirty(&format!("stash-{}", request.apply_id))
                    .map_err(|er| refuse("GIT", er.to_string(), vec![]))?;
                stash_ref = Some(snap.reference);
            }
        }
    }
    // The pre-apply checkpoint: the bytes every touched path holds on disk.
    let mut entries = Vec::new();
    let mut ordered = Vec::new();
    for (path, (action, reason)) in writes {
        let disk = read_disk(&checkout.dir, &path)
            .map_err(|er| refuse("PATH", er.to_string(), vec![path.clone()]))?;
        let before = match &disk {
            Disk::Absent => None,
            Disk::File { bytes, exec } => Some(blob_ref(blobs, bytes, *exec)?),
            Disk::Other(why) => {
                return Err(refuse(
                    "PATH",
                    format!("`{path}` holds {why}, which an apply does not replace"),
                    vec![path],
                ));
            }
        };
        let after = match &action {
            Action::Put { bytes, exec } => Some(blob_ref(blobs, bytes, *exec)?),
            Action::Remove => None,
        };
        if before.is_none() && after.is_none() {
            continue;
        }
        if let (Some(b), Some(a)) = (&before, &after)
            && b.sha256 == a.sha256
            && b.exec == a.exec
        {
            continue;
        }
        // Directories the write creates.
        let mut created_dirs = Vec::new();
        if after.is_some() {
            let mut cur = PathBuf::new();
            let comps: Vec<_> = Path::new(&path).components().collect();
            for c in &comps[..comps.len().saturating_sub(1)] {
                cur.push(c);
                if !checkout.dir.join(&cur).exists() {
                    created_dirs.push(cur.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        entries.push(ManifestEntry {
            path: path.clone(),
            before,
            after,
            created_dirs,
            reason: reason.to_owned(),
        });
        ordered.push((path, action));
    }
    Ok(Prepared {
        manifest: Manifest {
            apply_id: request.apply_id.clone(),
            plan_digest: plan.digest.clone(),
            choice: request.choice,
            checkout_head: plan.checkout_head.clone(),
            candidate_tree: plan.candidate_tree.clone(),
            stash_ref,
            entries,
        },
        writes: ordered,
        unresolved,
    })
}

/// A file of `tree` (`None`: not there).
fn tree_blob(
    repo: &Repo,
    tree: &str,
    path: &str,
) -> std::result::Result<Option<(Vec<u8>, bool)>, Refusal> {
    let out = run_bytes(
        &repo.dir,
        &["ls-tree", "-z", "--end-of-options", tree, "--", path],
        &[],
    )
    .map_err(|er| refuse("GIT", er.to_string(), vec![path.to_owned()]))?;
    let Some(rec) = out.split(|b| *b == 0).find(|r| !r.is_empty()) else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(rec);
    let meta = text.split('\t').next().unwrap_or_default();
    let mut it = meta.split(' ');
    let (mode, kind, oid) = (
        it.next().unwrap_or(""),
        it.next().unwrap_or(""),
        it.next().unwrap_or(""),
    );
    if kind != "blob" || mode == "120000" {
        return Err(refuse(
            "UNSUPPORTED_CHANGE",
            format!("`{path}` is not a regular file in the task's tree"),
            vec![path.to_owned()],
        ));
    }
    let bytes =
        blob(repo, oid).map_err(|er| refuse("GIT", er.to_string(), vec![path.to_owned()]))?;
    Ok(Some((bytes, mode == "100755")))
}

fn temp_name(full: &Path, id: &str) -> PathBuf {
    let mut name = full
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(format!(".modbit-apply-{id}.tmp"));
    full.with_file_name(name)
}

fn write_atomic(full: &Path, bytes: &[u8], exec: bool, id: &str) -> Result<()> {
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(Error::Spawn)?;
    }
    let tmp = temp_name(full, id);
    std::fs::write(&tmp, bytes).map_err(Error::Spawn)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if exec { 0o755 } else { 0o644 };
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))
            .map_err(Error::Spawn)?;
    }
    #[cfg(not(unix))]
    let _ = exec;
    std::fs::rename(&tmp, full).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::Spawn(e)
    })
}

/// Perform the writes of `prepared` in `checkout`, one atomic rename each.
/// `step(index, path)` runs before each write (a fault-injection seam).
pub fn execute(
    checkout: &Path,
    prepared: &Prepared,
    step: &mut dyn FnMut(usize, &str),
) -> Result<()> {
    let id = &prepared.manifest.apply_id;
    for (i, (path, action)) in prepared.writes.iter().enumerate() {
        step(i, path);
        checked_rel(path)?;
        no_link_in_path(checkout, path)?;
        let full = checkout.join(path);
        match action {
            Action::Put { bytes, exec } => write_atomic(&full, bytes, *exec, id)?,
            Action::Remove => match std::fs::remove_file(&full) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::Spawn(e)),
            },
        }
    }
    Ok(())
}

/// What a restore did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreReport {
    /// Paths put back to their pre-apply bytes.
    pub restored: Vec<String>,
    /// Paths already at their pre-apply bytes.
    pub already: Vec<String>,
    /// Paths the checkout holds something else at (edited since the apply):
    /// left untouched.
    pub divergent: Vec<String>,
}

/// How strict a restore is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    /// Undo: if any path diverged, change nothing and report them.
    AllOrNothing,
    /// Crash recovery: restore every path that can be, report the rest.
    BestEffort,
}

fn state_of(checkout: &Path, path: &str) -> Result<Option<String>> {
    match read_disk(checkout, path)? {
        Disk::Absent => Ok(None),
        Disk::File { bytes, .. } => Ok(Some(hex_sha(&bytes))),
        Disk::Other(why) => Ok(Some(format!("other:{why}"))),
    }
}

/// Put the manifest's pre-apply bytes back.
pub fn restore(
    checkout: &Path,
    manifest: &Manifest,
    blobs: &dyn Blobs,
    mode: RestoreMode,
) -> Result<RestoreReport> {
    let mut report = RestoreReport::default();
    let mut todo: Vec<&ManifestEntry> = Vec::new();
    for e in &manifest.entries {
        let now = state_of(checkout, &e.path)?;
        let before = e.before.as_ref().map(|b| b.sha256.clone());
        let after = e.after.as_ref().map(|a| a.sha256.clone());
        if now == before {
            report.already.push(e.path.clone());
        } else if now == after {
            todo.push(e);
        } else {
            report.divergent.push(e.path.clone());
        }
    }
    // A crash can leave a temp file beside a path it was writing.
    for e in &manifest.entries {
        let _ = std::fs::remove_file(temp_name(&checkout.join(&e.path), &manifest.apply_id));
    }
    if mode == RestoreMode::AllOrNothing && !report.divergent.is_empty() {
        return Ok(report);
    }
    for e in todo.into_iter().rev() {
        let full = checkout.join(&e.path);
        match &e.before {
            Some(b) => {
                let bytes = blobs
                    .get(&b.blob)
                    .map_err(|er| Error::Parse(format!("pre-apply bytes of `{}`: {er}", e.path)))?;
                if hex_sha(&bytes) != b.sha256 {
                    return Err(Error::Parse(format!(
                        "pre-apply bytes of `{}` do not match their hash",
                        e.path
                    )));
                }
                write_atomic(&full, &bytes, b.exec, &manifest.apply_id)?;
            }
            None => match std::fs::remove_file(&full) {
                Ok(()) => {}
                Err(er) if er.kind() == std::io::ErrorKind::NotFound => {}
                Err(er) => return Err(Error::Spawn(er)),
            },
        }
        report.restored.push(e.path.clone());
        // Directories the apply created are removed again once empty.
        for d in e.created_dirs.iter().rev() {
            let _ = std::fs::remove_dir(checkout.join(d));
        }
    }
    report.restored.reverse();
    Ok(report)
}

/// Whether the checkout still holds exactly what `manifest`'s apply wrote
/// (every `after`), i.e. the apply is in force and can be undone.
pub fn in_force(checkout: &Path, manifest: &Manifest) -> Result<bool> {
    for e in &manifest.entries {
        if state_of(checkout, &e.path)? != e.after.as_ref().map(|a| a.sha256.clone()) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_do_not_make_a_difference() {
        assert!(same(b"a\r\nb\r\n", b"a\nb\n"));
        assert!(!same(b"a\nb\n", b"a\nc\n"));
        assert!(!same(b"a\0\r\n", b"a\0\n"), "binary compares exactly");
    }

    #[test]
    fn only_plain_relative_paths_are_accepted() {
        assert!(checked_rel("src/a.rs").is_ok());
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../../x",
            ".git/config",
            "a/.git/x",
        ] {
            assert!(checked_rel(bad).is_err(), "{bad:?}");
        }
    }
}
