//! Path normalization and policy (docs/20, docs/23).

use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

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
            b.add(Glob::new(&p).map_err(|e| Error::Invalid {
                path: p.clone(),
                detail: format!("bad protected pattern: {e}"),
            })?);
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

    /// Lexical normalization: reject absolute paths outside the root, `..`
    /// that climbs above the root, and empty components.
    fn normalize(&self, given: &str) -> Result<PathBuf> {
        let p = Path::new(given);
        let mut rel = PathBuf::new();
        let mut depth: i32 = 0;
        let comps: Vec<Component> = if p.is_absolute() {
            match p.strip_prefix(&self.root) {
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

    /// Resolve symlinks component by component. Existing prefixes are
    /// canonicalized (following links); the non-existent tail is appended
    /// lexically. Any hop that leaves the root fails.
    fn resolve(&self, given: &str, rel: &Path) -> Result<PathBuf> {
        let mut cur = self.root.clone();
        let comps: Vec<_> = rel.components().collect();
        for (i, c) in comps.iter().enumerate() {
            let candidate = cur.join(c);
            match std::fs::symlink_metadata(&candidate) {
                Ok(meta) => {
                    if meta.file_type().is_symlink() {
                        let target = candidate.canonicalize().map_err(|e| Error::Io {
                            path: candidate.display().to_string(),
                            source: e,
                        })?;
                        if !target.starts_with(&self.root) {
                            return Err(Error::OutsideRoot {
                                path: given.into(),
                                detail: format!(
                                    "symlink `{}` points to `{}`",
                                    candidate.display(),
                                    target.display()
                                ),
                            });
                        }
                        cur = target;
                    } else {
                        cur = candidate;
                    }
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
