//! Bounded content snapshots of a workspace tree (FIX-06, docs/20, docs/23
//! "Protected paths").
//!
//! The file service is the write barrier for `change.*` tools, but a process
//! started by `shell.exec`, `shell.start` or `test.run` writes the tree
//! directly. A snapshot taken before and after such a process turns "it ran
//! in the workspace" into a typed list of what it changed
//! ([`FileDelta`]), attributable to the call, and lets the path policy
//! judge the *result*: a change to a protected path can be reverted from the
//! pre-run content ([`restore`]).
//!
//! Bounded by construction: generated and vendored directories (the same set
//! the search index skips) are not walked, the entry count, the bytes hashed
//! and the size of a file that is hashed are capped (a file over the cap is
//! fingerprinted by size and mtime, an incomplete walk is declared, never
//! hidden), and only protected files keep their pre-run bytes (up to a cap).
//! `.git` is not walked either, except the two surfaces that make git run
//! programs — `hooks/` and `config` — which are watched under the virtual
//! names `.git/hooks/…` and `.git/config` (resolved through the `.git` file
//! of a linked worktree to the common git directory).
//!
//! Symbolic links are recorded by target and never followed: a link that
//! points outside the root is a change of the link, not a read of what it
//! names.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::PathPolicy;
use crate::service::{content_hash, write_atomic};

/// Directories never walked (aligned with the search index's skip list;
/// `.git` is handled separately).
pub const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".modbit",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".cache",
    "vendor",
];

/// Bounds of one snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Entries (files and links) scanned before the walk stops.
    pub max_entries: usize,
    /// A file larger than this is fingerprinted by size and mtime.
    pub max_hashed_file_bytes: u64,
    /// Bytes hashed in total (protected files are always hashed).
    pub max_total_hashed_bytes: u64,
    /// A protected file larger than this is detected but cannot be restored.
    pub max_retained_file_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 50_000,
            max_hashed_file_bytes: 8 * 1024 * 1024,
            max_total_hashed_bytes: 512 * 1024 * 1024,
            max_retained_file_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Entry {
    File {
        /// sha256 hex, or `size:<n>:mtime:<ns>` when not hashed.
        fingerprint: String,
        exact: bool,
        exec: bool,
        size: u64,
        mtime_ns: u128,
    },
    Symlink {
        target: String,
    },
}

/// A file is "racy" when it was modified within this long of the moment its
/// hash was taken: a rewrite that keeps size and mtime cannot be excluded, so
/// such a file is always re-read (the same rule git applies to its index).
const RACY_NS: u128 = 2_000_000_000;

fn now_ns() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// What an earlier snapshot learned about file contents: a file whose size
/// and mtime are unchanged, and that was not racy when hashed, is not read
/// again. A pure cache: it can only skip a read of a file that provably has
/// the content it had, never change what a snapshot reports.
#[derive(Clone, Debug, Default)]
pub struct HashCache {
    taken_at_ns: u128,
    files: std::collections::HashMap<String, (u64, u128, String)>,
}

impl HashCache {
    /// Number of files remembered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether nothing is remembered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn lookup(&self, rel: &str, size: u64, mtime_ns: u128) -> Option<&str> {
        let (s, m, fp) = self.files.get(rel)?;
        (*s == size && *m == mtime_ns && m.saturating_add(RACY_NS) < self.taken_at_ns)
            .then_some(fp.as_str())
    }
}

impl Entry {
    fn label(&self) -> String {
        match self {
            Self::File {
                fingerprint, exec, ..
            } => {
                if *exec {
                    format!("{fingerprint}+x")
                } else {
                    fingerprint.clone()
                }
            }
            Self::Symlink { target } => format!("symlink:{target}"),
        }
    }
}

#[derive(Clone, Debug)]
struct Retained {
    bytes: Vec<u8>,
    /// Permission bits to put back; restored on Unix only.
    #[cfg_attr(not(unix), allow(dead_code))]
    mode: Option<u32>,
}

/// One tree at one moment.
#[derive(Clone, Debug)]
pub struct Snapshot {
    root: PathBuf,
    entries: BTreeMap<String, Entry>,
    retained: BTreeMap<String, Retained>,
    hooks_dir: Option<PathBuf>,
    git_config: Option<PathBuf>,
    /// The walk hit a bound: the snapshot does not describe the whole tree.
    pub incomplete: bool,
    /// Entries scanned.
    pub scanned: usize,
    taken_at_ns: u128,
}

/// What happened to one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Did not exist before.
    Added,
    /// Content (or link target, or executable bit) differs.
    Modified,
    /// Existed before, is gone.
    Deleted,
}

impl ChangeKind {
    /// Wire label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }
}

/// A path a process changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDelta {
    /// Root-relative, forward-slash path (`.git/hooks/…`, `.git/config` for
    /// the watched git surfaces).
    pub path: String,
    /// What happened.
    pub kind: ChangeKind,
    /// Content hash (or fingerprint) before.
    pub before: Option<String>,
    /// Content hash (or fingerprint) after.
    pub after: Option<String>,
    /// The protected pattern the path matches, when it does.
    pub protected_by: Option<String>,
}

/// What [`restore`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restored {
    /// Paths put back exactly as they were (verified by hash).
    pub restored: Vec<String>,
    /// Paths that could not be restored, with why.
    pub unrestored: Vec<(String, String)>,
}

fn rel_name(parent_rel: &str, name: &str) -> String {
    if parent_rel.is_empty() {
        name.to_owned()
    } else {
        format!("{parent_rel}/{name}")
    }
}

#[cfg(unix)]
fn mode_of(meta: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(meta.permissions().mode())
}

#[cfg(not(unix))]
fn mode_of(_meta: &std::fs::Metadata) -> Option<u32> {
    None
}

fn is_exec(meta: &std::fs::Metadata) -> bool {
    mode_of(meta).is_some_and(|m| m & 0o111 != 0)
}

/// Where the hooks directory and config of the repository at `root` live,
/// following the `.git` file of a linked worktree to the common directory.
fn git_surfaces(root: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let dot = root.join(".git");
    let Ok(meta) = std::fs::symlink_metadata(&dot) else {
        return (None, None);
    };
    let common = if meta.is_dir() {
        dot
    } else if meta.is_file() {
        let Ok(text) = std::fs::read_to_string(&dot) else {
            return (None, None);
        };
        let Some(gitdir) = text.trim().strip_prefix("gitdir:").map(|s| s.trim()) else {
            return (None, None);
        };
        let gitdir = if Path::new(gitdir).is_absolute() {
            PathBuf::from(gitdir)
        } else {
            root.join(gitdir)
        };
        match std::fs::read_to_string(gitdir.join("commondir")) {
            Ok(c) => {
                let joined = gitdir.join(c.trim());
                joined.canonicalize().unwrap_or(joined)
            }
            Err(_) => gitdir,
        }
    } else {
        return (None, None);
    };
    (Some(common.join("hooks")), Some(common.join("config")))
}

/// Take a snapshot of the tree under `policy`'s root.
#[must_use]
pub fn capture(policy: &PathPolicy, limits: Limits) -> Snapshot {
    capture_cached(policy, limits, &HashCache::default())
}

/// [`capture`] that reads only the files an earlier snapshot's
/// [`HashCache`] cannot vouch for.
#[must_use]
pub fn capture_cached(policy: &PathPolicy, limits: Limits, cache: &HashCache) -> Snapshot {
    let root = policy.root().to_path_buf();
    let (hooks_dir, git_config) = git_surfaces(&root);
    let mut snap = Snapshot {
        root: root.clone(),
        entries: BTreeMap::new(),
        retained: BTreeMap::new(),
        hooks_dir: hooks_dir.clone(),
        git_config: git_config.clone(),
        incomplete: false,
        scanned: 0,
        taken_at_ns: now_ns(),
    };
    let mut hashed_total: u64 = 0;
    // (absolute directory, root-relative name, real directory is a watched git surface)
    let mut stack: Vec<(PathBuf, String)> = vec![(root.clone(), String::new())];
    if let Some(h) = &hooks_dir {
        stack.push((h.clone(), ".git/hooks".to_owned()));
    }
    if let Some(c) = &git_config {
        record_file(
            &mut snap,
            policy,
            limits,
            cache,
            &mut hashed_total,
            c,
            ".git/config".to_owned(),
        );
    }
    while let Some((dir, rel)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(ft) = entry.file_type() else { continue };
            let child_rel = rel_name(&rel, &name);
            if ft.is_dir() {
                // the hooks tree is walked in full; elsewhere generated and
                // vendored trees are skipped
                if !rel.starts_with(".git/hooks") && SKIPPED_DIRS.contains(&name.as_str()) {
                    continue;
                }
                stack.push((entry.path(), child_rel));
            } else if ft.is_symlink() || ft.is_file() {
                if snap.scanned >= limits.max_entries {
                    snap.incomplete = true;
                    stack.clear();
                    break;
                }
                snap.scanned += 1;
                if ft.is_symlink() {
                    let target = std::fs::read_link(entry.path())
                        .map(|t| t.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    snap.entries.insert(child_rel, Entry::Symlink { target });
                } else {
                    record_file(
                        &mut snap,
                        policy,
                        limits,
                        cache,
                        &mut hashed_total,
                        &entry.path(),
                        child_rel,
                    );
                }
            }
        }
    }
    snap
}

fn record_file(
    snap: &mut Snapshot,
    policy: &PathPolicy,
    limits: Limits,
    cache: &HashCache,
    hashed_total: &mut u64,
    path: &Path,
    rel: String,
) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(path)
            .map(|t| t.to_string_lossy().into_owned())
            .unwrap_or_default();
        snap.entries.insert(rel, Entry::Symlink { target });
        return;
    }
    if !meta.is_file() {
        return;
    }
    let protected = policy.protected_match(&rel).is_some();
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    // Protected files are always read: their bytes are retained for a restore.
    if !protected && let Some(fp) = cache.lookup(&rel, size, mtime) {
        snap.entries.insert(
            rel,
            Entry::File {
                fingerprint: fp.to_owned(),
                exact: true,
                exec: is_exec(&meta),
                size,
                mtime_ns: mtime,
            },
        );
        return;
    }
    let within_budget = size <= limits.max_hashed_file_bytes
        && hashed_total.saturating_add(size) <= limits.max_total_hashed_bytes;
    let retainable = protected && size <= limits.max_retained_file_bytes;
    if within_budget || retainable {
        match std::fs::read(path) {
            Ok(bytes) => {
                *hashed_total = hashed_total.saturating_add(size);
                let fingerprint = content_hash(&bytes);
                snap.entries.insert(
                    rel.clone(),
                    Entry::File {
                        fingerprint,
                        exact: true,
                        exec: is_exec(&meta),
                        size,
                        mtime_ns: mtime,
                    },
                );
                if retainable {
                    snap.retained.insert(
                        rel,
                        Retained {
                            bytes,
                            mode: mode_of(&meta),
                        },
                    );
                }
                return;
            }
            Err(_) => return,
        }
    }
    snap.entries.insert(
        rel,
        Entry::File {
            fingerprint: format!("size:{size}:mtime:{mtime}"),
            exact: false,
            exec: is_exec(&meta),
            size,
            mtime_ns: mtime,
        },
    );
}

impl Snapshot {
    /// The real location of a snapshot path.
    fn real_path(&self, rel: &str) -> PathBuf {
        if rel == ".git/config" {
            if let Some(c) = &self.git_config {
                return c.clone();
            }
        } else if let Some(rest) = rel.strip_prefix(".git/hooks/")
            && let Some(h) = &self.hooks_dir
        {
            return h.join(rest);
        }
        self.root.join(rel)
    }

    /// What this snapshot learned about contents, for the next capture.
    #[must_use]
    pub fn hash_cache(&self) -> HashCache {
        HashCache {
            taken_at_ns: self.taken_at_ns,
            files: self
                .entries
                .iter()
                .filter_map(|(k, e)| match e {
                    Entry::File {
                        fingerprint,
                        exact: true,
                        size,
                        mtime_ns,
                        ..
                    } => Some((k.clone(), (*size, *mtime_ns, fingerprint.clone()))),
                    _ => None,
                })
                .collect(),
        }
    }

    /// Number of entries recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// What changed between two snapshots of the same root, sorted by path.
///
/// When either snapshot is incomplete, additions and deletions cannot be
/// told from paths the walk did not reach: only paths present in both are
/// compared (a `Modified` is always real).
#[must_use]
pub fn diff(before: &Snapshot, after: &Snapshot, policy: &PathPolicy) -> Vec<FileDelta> {
    let partial = before.incomplete || after.incomplete;
    let keys: BTreeSet<&String> = before.entries.keys().chain(after.entries.keys()).collect();
    let mut out = vec![];
    for k in keys {
        let (b, a) = (before.entries.get(k), after.entries.get(k));
        let kind = match (b, a) {
            (Some(b), Some(a)) if b.label() != a.label() => ChangeKind::Modified,
            (None, Some(_)) if !partial => ChangeKind::Added,
            (Some(_), None) if !partial => ChangeKind::Deleted,
            _ => continue,
        };
        out.push(FileDelta {
            path: k.clone(),
            kind,
            before: b.map(Entry::label),
            after: a.map(Entry::label),
            protected_by: policy.protected_match(k).map(str::to_owned),
        });
    }
    out
}

/// Put `deltas` (changes to paths the policy protects) back as `before` had
/// them: added paths are removed, modified and deleted ones rewritten from
/// the retained pre-run bytes (or the link recreated). Every restore is
/// verified by hashing the result; whatever could not be put back is named.
#[must_use]
pub fn restore(before: &Snapshot, deltas: &[&FileDelta]) -> Restored {
    let mut out = Restored::default();
    for d in deltas {
        let real = before.real_path(&d.path);
        match restore_one(before, d, &real) {
            Ok(()) => out.restored.push(d.path.clone()),
            Err(why) => out.unrestored.push((d.path.clone(), why)),
        }
    }
    out
}

fn remove_any(real: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(real) {
        Ok(m) if m.is_dir() => Err("a directory now stands where a file was".into()),
        Ok(_) => std::fs::remove_file(real).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

fn restore_one(before: &Snapshot, d: &FileDelta, real: &Path) -> Result<(), String> {
    match before.entries.get(&d.path) {
        None => {
            // did not exist: remove what the process created
            remove_any(real)?;
            if std::fs::symlink_metadata(real).is_ok() {
                return Err("still present after removal".into());
            }
            Ok(())
        }
        Some(Entry::Symlink { target }) => {
            remove_any(real)?;
            #[cfg(unix)]
            {
                if let Some(parent) = real.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::os::unix::fs::symlink(target, real).map_err(|e| e.to_string())?;
                Ok(())
            }
            #[cfg(not(unix))]
            {
                let _ = target;
                Err("symbolic links cannot be recreated on this platform".into())
            }
        }
        Some(Entry::File {
            fingerprint, exact, ..
        }) => {
            let Some(kept) = before.retained.get(&d.path) else {
                return Err("its pre-run content was not retained (over the size bound)".into());
            };
            if !*exact {
                return Err("its pre-run content was not hashed".into());
            }
            remove_any(real)?;
            let tmp = real.with_file_name(format!(
                ".{}.modbit-restore",
                real.file_name()
                    .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned())
            ));
            write_atomic(&tmp, real, &kept.bytes).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            if let Some(mode) = kept.mode {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(real, std::fs::Permissions::from_mode(mode));
            }
            let now = std::fs::read(real).map_err(|e| e.to_string())?;
            if &content_hash(&now) != fingerprint {
                return Err("the restored content does not match the pre-run hash".into());
            }
            Ok(())
        }
    }
}
