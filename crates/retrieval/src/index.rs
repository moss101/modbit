//! Exact / regex / path index (docs/18 "Index inputs", "Concrete local
//! indexing stack": ripgrep-compatible search path; "Index freshness": small
//! edits update the exact index incrementally).
//!
//! The index holds the text of every indexable file of a workspace at one
//! revision: not generated, vendored, ignored or binary paths, and nothing
//! above the per-file size ceiling. Searches are bounded (hits per file and in
//! total) and return byte spans with line/column so callers can build typed
//! CodeReferences at the revision the hit was found.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Per-file ceiling (bytes); larger files are recorded but not searchable.
pub const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
/// Regex size ceiling (compiled program), so a hostile pattern cannot exhaust memory.
const REGEX_SIZE_LIMIT: usize = 4 * 1024 * 1024;

/// Directories never indexed (docs/18: generated/vendor paths are policy-tagged and skipped).
const SKIPPED_DIRS: &[&str] = &[
    ".git",
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

/// Index errors.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    /// I/O failure.
    #[error("io on {path}: {source}")]
    Io {
        /// Path.
        path: String,
        /// Cause.
        #[source]
        source: std::io::Error,
    },
    /// Invalid regex.
    #[error("invalid regex: {0}")]
    Regex(String),
    /// Invalid glob.
    #[error("invalid glob: {0}")]
    Glob(String),
}

/// One indexed file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Root-relative path with `/` separators.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// sha256 of the bytes (the file revision CodeReferences bind to).
    pub content_hash: String,
    /// Language label by extension (`rust`, `typescript`, `python`, ...), if known.
    pub language: Option<String>,
    /// Whether the text is searchable (UTF-8 text within the size ceiling).
    pub searchable: bool,
    /// Index revision at which this entry was (re)built.
    pub indexed_at_revision: u64,
    #[serde(skip)]
    text: Option<String>,
}

/// One text hit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    /// Root-relative path.
    pub path: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based column (bytes).
    pub column: u32,
    /// Byte span of the match within the file.
    pub span: (u64, u64),
    /// The matched line (bounded).
    pub line_text: String,
    /// Content hash of the file the hit was found in.
    pub content_hash: String,
    /// Index revision the hit is bound to.
    pub index_revision: u64,
}

/// One path hit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathHit {
    /// Root-relative path.
    pub path: String,
    /// Language label, if known.
    pub language: Option<String>,
    /// Content hash.
    pub content_hash: String,
}

/// Search bounds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchOptions {
    /// Case-insensitive matching.
    pub case_insensitive: bool,
    /// Restrict to paths matching this glob.
    pub path_glob: Option<String>,
    /// Maximum hits in total.
    pub max_hits: usize,
    /// Maximum hits per file.
    pub max_hits_per_file: usize,
    /// Maximum bytes of `line_text` per hit.
    pub max_line_bytes: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_insensitive: false,
            path_glob: None,
            max_hits: 200,
            max_hits_per_file: 20,
            max_line_bytes: 400,
        }
    }
}

/// Index statistics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexStats {
    /// Index revision.
    pub revision: u64,
    /// Files recorded.
    pub files: usize,
    /// Files searchable.
    pub searchable: usize,
    /// Bytes of searchable text.
    pub bytes: u64,
}

/// The exact / regex / path index of one workspace root.
#[derive(Debug)]
pub struct RepositoryIndex {
    root: PathBuf,
    revision: u64,
    files: BTreeMap<String, FileEntry>,
}

impl RepositoryIndex {
    /// Build the index of `root` at `revision`. Git-aware: in a repository the
    /// file set is `git ls-files -co --exclude-standard` (tracked plus untracked
    /// but not ignored); elsewhere a walk that skips the generated/vendor
    /// directories.
    pub fn build(root: &Path, revision: u64) -> Result<Self, IndexError> {
        let mut idx = Self {
            root: root.to_path_buf(),
            revision,
            files: BTreeMap::new(),
        };
        for rel in list_files(root)? {
            idx.index_file(&rel, revision);
        }
        Ok(idx)
    }

    /// Root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Index revision.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Statistics.
    #[must_use]
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            revision: self.revision,
            files: self.files.len(),
            searchable: self.files.values().filter(|f| f.searchable).count(),
            bytes: self
                .files
                .values()
                .filter_map(|f| f.text.as_ref().map(|t| t.len() as u64))
                .sum(),
        }
    }

    /// Incremental refresh (docs/18 "Index freshness"): re-read exactly the
    /// paths a write changed and move the index to `revision`. A path that no
    /// longer exists leaves the index; a new path joins it.
    pub fn refresh(&mut self, changed: &[String], revision: u64) {
        for p in changed {
            let rel = p.replace('\\', "/");
            if is_skipped(&rel) {
                continue;
            }
            self.index_file(&rel, revision);
        }
        self.revision = revision;
    }

    /// Full rebuild at `revision` (used when the changed set is unknown).
    pub fn rebuild(&mut self, revision: u64) -> Result<(), IndexError> {
        *self = Self::build(&self.root, revision)?;
        Ok(())
    }

    /// The entry of a path.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&FileEntry> {
        self.files.get(path)
    }

    /// Searchable files as (path, text, language) for downstream indexes.
    pub fn texts(&self) -> impl Iterator<Item = (&str, &str, Option<&str>)> {
        self.files.values().filter_map(|f| {
            f.text
                .as_deref()
                .map(|t| (f.path.as_str(), t, f.language.as_deref()))
        })
    }

    /// Exact substring search (ripgrep `-F` semantics).
    pub fn search_exact(&self, needle: &str, opts: &SearchOptions) -> Vec<Hit> {
        if needle.is_empty() {
            return vec![];
        }
        let pattern = regex::escape(needle);
        self.search_regex(&pattern, opts).unwrap_or_default()
    }

    /// Regex search (Rust `regex` syntax, the ripgrep default engine).
    pub fn search_regex(
        &self,
        pattern: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<Hit>, IndexError> {
        let re = RegexBuilder::new(pattern)
            .case_insensitive(opts.case_insensitive)
            .size_limit(REGEX_SIZE_LIMIT)
            .build()
            .map_err(|e| IndexError::Regex(e.to_string()))?;
        let glob = match &opts.path_glob {
            Some(g) => Some(
                globset::Glob::new(g)
                    .map_err(|e| IndexError::Glob(e.to_string()))?
                    .compile_matcher(),
            ),
            None => None,
        };
        let mut hits = Vec::new();
        for f in self.files.values() {
            if hits.len() >= opts.max_hits {
                break;
            }
            let Some(text) = &f.text else { continue };
            if let Some(g) = &glob
                && !g.is_match(&f.path)
            {
                continue;
            }
            for (per_file, m) in re.find_iter(text).enumerate() {
                if per_file >= opts.max_hits_per_file || hits.len() >= opts.max_hits {
                    break;
                }
                let start = m.start();
                let line_start =
                    memchr::memrchr(b'\n', &text.as_bytes()[..start]).map_or(0, |i| i + 1);
                let line_end = memchr::memchr(b'\n', &text.as_bytes()[start..])
                    .map_or(text.len(), |i| start + i);
                let line_no =
                    1 + memchr::memchr_iter(b'\n', &text.as_bytes()[..line_start]).count() as u32;
                let mut line_text = text[line_start..line_end].to_owned();
                if line_text.len() > opts.max_line_bytes {
                    let mut cut = opts.max_line_bytes;
                    while !line_text.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    line_text.truncate(cut);
                    line_text.push('…');
                }
                hits.push(Hit {
                    path: f.path.clone(),
                    line: line_no,
                    column: (start - line_start) as u32 + 1,
                    span: (start as u64, m.end() as u64),
                    line_text,
                    content_hash: f.content_hash.clone(),
                    index_revision: f.indexed_at_revision,
                });
            }
        }
        Ok(hits)
    }

    /// Path search by glob (`**` aware), bounded.
    pub fn find_paths(&self, glob: &str, max: usize) -> Result<Vec<PathHit>, IndexError> {
        let m = globset::Glob::new(glob)
            .map_err(|e| IndexError::Glob(e.to_string()))?
            .compile_matcher();
        Ok(self
            .files
            .values()
            .filter(|f| m.is_match(&f.path))
            .take(max)
            .map(|f| PathHit {
                path: f.path.clone(),
                language: f.language.clone(),
                content_hash: f.content_hash.clone(),
            })
            .collect())
    }

    fn index_file(&mut self, rel: &str, revision: u64) {
        let abs = self.root.join(rel);
        let Ok(meta) = std::fs::metadata(&abs) else {
            self.files.remove(rel);
            return;
        };
        if !meta.is_file() {
            self.files.remove(rel);
            return;
        }
        let size = meta.len();
        let (content_hash, text, searchable) = if size as usize > MAX_FILE_BYTES {
            (hash_file(&abs), None, false)
        } else {
            match std::fs::read(&abs) {
                Ok(bytes) => {
                    let h = hex::encode(Sha256::digest(&bytes));
                    match String::from_utf8(bytes) {
                        Ok(t) if !t.contains('\0') => (h, Some(t), true),
                        _ => (h, None, false),
                    }
                }
                Err(_) => {
                    self.files.remove(rel);
                    return;
                }
            }
        };
        self.files.insert(
            rel.to_owned(),
            FileEntry {
                path: rel.to_owned(),
                size,
                content_hash,
                language: language_of(rel),
                searchable,
                indexed_at_revision: revision,
                text,
            },
        );
    }
}

fn hash_file(abs: &Path) -> String {
    std::fs::read(abs)
        .map(|b| hex::encode(Sha256::digest(&b)))
        .unwrap_or_default()
}

fn is_skipped(rel: &str) -> bool {
    rel.split('/')
        .any(|seg| SKIPPED_DIRS.contains(&seg) || seg.starts_with(".modbit"))
}

/// Language label by extension (docs/76 Alpha baseline first).
#[must_use]
pub fn language_of(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?;
    Some(
        match ext {
            "rs" => "rust",
            "ts" | "tsx" | "mts" | "cts" => "typescript",
            "js" | "jsx" | "mjs" | "cjs" => "javascript",
            "py" | "pyi" => "python",
            "go" => "go",
            "java" => "java",
            "kt" => "kotlin",
            "c" | "h" => "c",
            "cc" | "cpp" | "hpp" | "cxx" => "cpp",
            "cs" => "csharp",
            "rb" => "ruby",
            "sh" | "bash" | "zsh" => "shell",
            "md" => "markdown",
            "json" => "json",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            "html" => "html",
            "css" => "css",
            "sql" => "sql",
            _ => return None,
        }
        .to_owned(),
    )
}

/// The indexable file set: git-aware in a repository, a filtered walk elsewhere.
fn list_files(root: &Path) -> Result<Vec<String>, IndexError> {
    let git = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "-co", "--exclude-standard"])
        .output();
    if let Ok(o) = git
        && o.status.success()
    {
        let mut out: Vec<String> = String::from_utf8_lossy(&o.stdout)
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(|p| p.replace('\\', "/"))
            .filter(|p| !is_skipped(p))
            .collect();
        out.sort();
        out.dedup();
        return Ok(out);
    }
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(dir) = stack.pop() {
        let abs = root.join(&dir);
        let entries = std::fs::read_dir(&abs).map_err(|e| IndexError::Io {
            path: abs.display().to_string(),
            source: e,
        })?;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = if dir.as_os_str().is_empty() {
                name.clone()
            } else {
                format!("{}/{name}", dir.to_string_lossy().replace('\\', "/"))
            };
            if is_skipped(&rel) {
                continue;
            }
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(PathBuf::from(&rel)),
                Ok(t) if t.is_file() => out.push(rel),
                _ => {}
            }
        }
    }
    out.sort();
    Ok(out)
}
