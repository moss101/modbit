//! Path normalization and policy (docs/20, docs/23).

use std::path::{Component, Path, PathBuf};

use globset::{GlobSet, GlobSetBuilder};

use crate::{Error, Result};

/// Protected patterns that are always denied without an explicit approval
/// (docs/23 "Protected paths"): Git internals, SSH keys, credential stores,
/// CI secrets. Users and policy add more.
pub const DEFAULT_PROTECTED: &[&str] = &[
    ".git/**",
    "**/.git/**",
    ".ssh/**",
    "**/.ssh/**",
    "**/id_rsa*",
    "**/id_ed25519*",
    "**/*.pem",
    "**/.netrc",
    "**/.npmrc",
    "**/.pypirc",
    "**/.aws/credentials",
    "**/.env",
    "**/.env.*",
    "**/.github/workflows/*.yml",
];

/// A workspace root with its policy.
#[derive(Debug, Clone)]
pub struct PathPolicy {
    root: PathBuf,
    protected: GlobSet,
    protected_patterns: Vec<String>,
}

/// A path that passed normalization, symlink resolution and policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    /// Root-relative, normalized, forward-slash form as given (for records).
    pub relative: String,
    /// Absolute filesystem path with symlinks resolved as far as they exist.
    pub absolute: PathBuf,
}

impl PathPolicy {
    /// Policy for `root` (canonicalized) with the default protected patterns
    /// plus `extra` user/policy patterns.
    pub fn new(root: &Path, extra: &[String]) -> Result<Self> {
        let root = root.canonicalize().map_err(|e| Error::Io {
            path: root.display().to_string(),
            source: e,
        })?;
        let mut b = GlobSetBuilder::new();
        let mut patterns = Vec::new();
        for p in DEFAULT_PROTECTED
            .iter()
            .map(|s| (*s).to_owned())
            .chain(extra.iter().cloned())
        {
            // Windows filesystems match names case-insensitively: a protected
            // pattern must too, or `.GIT/config` walks past `.git/**`
            // (WIN-03, PX-030). Unix keeps the exact case.
            let glob = globset::GlobBuilder::new(&p)
                .case_insensitive(cfg!(windows))
                .build()
                .map_err(|e| Error::Invalid {
                    path: p.clone(),
                    detail: format!("bad protected pattern: {e}"),
                })?;
            b.add(glob);
            patterns.push(p);
        }
        Ok(Self {
            root,
            protected: b.build().map_err(|e| Error::Invalid {
                path: String::new(),
                detail: e.to_string(),
            })?,
            protected_patterns: patterns,
        })
    }

    /// Canonical root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The protected pattern a root-relative, forward-slash path matches, if
    /// any. Purely lexical (no filesystem access): the check the snapshot
    /// diff applies to paths a process changed.
    #[must_use]
    pub fn protected_match(&self, rel: &str) -> Option<&str> {
        self.protected
            .matches(rel)
            .first()
            .map(|&i| self.protected_patterns[i].as_str())
    }

    /// Lexical normalization: reject absolute paths outside the root, `..`
    /// that climbs above the root, and empty components.
    fn normalize(&self, given: &str) -> Result<PathBuf> {
        // On Unix a backslash is a filename character, not a separator: a
        // path such as `src\x.rs` would name one file at the root while the
        // root-relative record (forward-slash form) would say `src/x.rs` — a
        // write-set and diff-invariant match for a file that was not
        // written. Found by the M9.6 path fuzzer; refused here (docs/52).
        if !cfg!(windows) && given.contains('\\') {
            return Err(Error::OutsideRoot {
                path: given.into(),
                detail: "backslash is not a path separator on this platform; use `/`".into(),
            });
        }
        let p = Path::new(given);
        let mut rel = PathBuf::new();
        let mut depth: i32 = 0;
        let comps: Vec<Component> = if p.is_absolute() {
            // On Windows the canonical root is the verbatim `\\?\C:\...` form, so
            // a path given in the ordinary `C:/...` form is compared against the
            // root with that prefix removed too.
            let plain_root = without_verbatim_prefix(&self.root);
            match p
                .strip_prefix(&self.root)
                .or_else(|_| p.strip_prefix(&plain_root))
            {
                Ok(r) => r.components().collect(),
                Err(_) => {
                    return Err(Error::OutsideRoot {
                        path: given.into(),
                        detail: "absolute path not under the root".into(),
                    });
                }
            }
        } else {
            p.components().collect()
        };
        for c in comps {
            match c {
                Component::Normal(s) => {
                    #[cfg(windows)]
                    validate_windows_component(given, s)?;
                    rel.push(s);
                    depth += 1;
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    depth -= 1;
                    if depth < 0 {
                        return Err(Error::OutsideRoot {
                            path: given.into(),
                            detail: "`..` climbs above the root".into(),
                        });
                    }
                    rel.pop();
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(Error::OutsideRoot {
                        path: given.into(),
                        detail: "unexpected root component".into(),
                    });
                }
            }
        }
        Ok(rel)
    }

    /// Resolve every existing hop by canonicalizing it. Canonicalization
    /// follows symlinks **and junctions** (std's `is_symlink` does not see
    /// mount points, which the earlier symlink-only walk let through) and
    /// expands 8.3 short names to the real spelling, so a hop that leaves the
    /// root by any of those spellings is caught here. The non-existent tail
    /// is appended lexically. A link whose target is missing fails closed
    /// (canonicalize errors). (WIN-03, PX-030.)
    fn resolve(&self, given: &str, rel: &Path) -> Result<PathBuf> {
        let mut cur = self.root.clone();
        let comps: Vec<_> = rel.components().collect();
        for (i, c) in comps.iter().enumerate() {
            let candidate = cur.join(c);
            match std::fs::symlink_metadata(&candidate) {
                Ok(_) => {
                    let target = candidate.canonicalize().map_err(|e| Error::Io {
                        path: candidate.display().to_string(),
                        source: e,
                    })?;
                    if !target.starts_with(&self.root) {
                        return Err(Error::OutsideRoot {
                            path: given.into(),
                            detail: format!(
                                "`{}` resolves to `{}`",
                                candidate.display(),
                                target.display()
                            ),
                        });
                    }
                    cur = target;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // Rest does not exist yet: append lexically.
                    for rest in &comps[i..] {
                        cur.push(rest);
                    }
                    break;
                }
                Err(e) => {
                    return Err(Error::Io {
                        path: candidate.display().to_string(),
                        source: e,
                    });
                }
            }
        }
        if !cur.starts_with(&self.root) {
            return Err(Error::OutsideRoot {
                path: given.into(),
                detail: format!("resolved to `{}`", cur.display()),
            });
        }
        Ok(cur)
    }

    /// Full check: normalize → resolve symlinks → protected patterns (checked
    /// on both the given relative form and the resolved relative form).
    pub fn check(&self, given: &str) -> Result<ResolvedPath> {
        let rel = self.normalize(given)?;
        let absolute = self.resolve(given, &rel)?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let resolved_rel = absolute
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        for candidate in [&rel_str, &resolved_rel] {
            if candidate.is_empty() {
                continue;
            }
            let matches = self.protected.matches(candidate);
            if let Some(&idx) = matches.first() {
                return Err(Error::Protected {
                    path: given.into(),
                    pattern: self.protected_patterns[idx].clone(),
                });
            }
        }
        Ok(ResolvedPath {
            relative: rel_str,
            absolute,
        })
    }
}

/// Windows-only component rules (WIN-03): a name that Win32 would silently
/// reinterpret must be refused, because the file the record names would not
/// be the file the write touches. Three families:
///
/// * a colon outside the drive prefix names an **alternate data stream**
///   (`data.txt:hidden`, `data.txt::$DATA`): bytes land in a stream of an
///   existing file, invisible to ordinary reads and to content hashing;
/// * a trailing dot or space is **dropped by Win32** on create
///   (`file.` and `file ` both name `file`);
/// * the reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1..9`,
///   `LPT1..9`, `CONIN$`, `CONOUT$`), with or without an extension
///   (`nul.txt`), are devices, never files: a write "succeeds" and no file
///   appears.
#[cfg(windows)]
fn validate_windows_component(given: &str, s: &std::ffi::OsStr) -> Result<()> {
    let name = s.to_string_lossy();
    let refuse = |detail: String| {
        Err(Error::Invalid {
            path: given.into(),
            detail,
        })
    };
    if name.contains(':') {
        return refuse(format!(
            "`{name}`: a colon names an alternate data stream, not a file"
        ));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return refuse(format!(
            "`{name}`: Windows drops a trailing dot or space, so the file on disk would not have this name"
        ));
    }
    let stem = name.split('.').next().unwrap_or("");
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM1", "COM2", "COM3", "COM4", "COM5",
        "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
        "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.to_ascii_uppercase().as_str()) {
        return refuse(format!(
            "`{name}`: a reserved device name, not a file (writes would reach a device or vanish)"
        ));
    }
    Ok(())
}

/// `path` without a Windows verbatim prefix (`\\?\C:\x` becomes `C:\x`,
/// `\\?\UNC\h\s` becomes `\\h\s`); any other path is returned unchanged.
fn without_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod verbatim_prefix_tests {
    use super::without_verbatim_prefix;
    use std::path::Path;

    #[test]
    fn a_verbatim_prefix_is_removed_and_other_paths_are_untouched() {
        assert_eq!(
            without_verbatim_prefix(Path::new(r"\\?\C:\work\repo")),
            Path::new(r"C:\work\repo")
        );
        assert_eq!(
            without_verbatim_prefix(Path::new(r"\\?\UNC\host\share\repo")),
            Path::new(r"\\host\share\repo")
        );
        assert_eq!(
            without_verbatim_prefix(Path::new("/home/me/repo")),
            Path::new("/home/me/repo")
        );
    }
}
