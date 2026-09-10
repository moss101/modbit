//! Repository Knowledge Artifact (REQ-EV-0060, docs/18 "Repository
//! knowledge"): a generated map of what lives where, built from the same
//! indexes retrieval uses.
//!
//! It is a discovery aid and a cache, never authority. Every claim names the
//! files it was derived from and the content hash each had at build time, so a
//! reader can always check it against the source — and when the source moves,
//! the claim says so instead of quietly aging. Nothing here decides anything:
//! the artifact points at code, and the code answers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One source a claim was derived from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimSource {
    /// Root-relative path.
    pub path: String,
    /// sha256 of the file's text when the claim was written.
    pub content_hash: String,
}

/// What a claim is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    /// What a module contains.
    Contents,
    /// What a module exports.
    Exports,
    /// What a module depends on.
    Dependencies,
    /// What depends on a module.
    Dependents,
    /// Which tests exercise it.
    Tests,
}

/// One statement about the repository, with the sources it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// Module the claim is about (a directory path, or `.` for the root).
    pub module: String,
    /// Kind.
    pub kind: ClaimKind,
    /// The statement, in words.
    pub text: String,
    /// The files it was derived from, with their hashes at build time.
    pub sources: Vec<ClaimSource>,
}

/// One module of the repository, as the artifact sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    /// Directory path (`.` for the root).
    pub path: String,
    /// Files directly in it.
    pub files: Vec<String>,
    /// Top-level symbols it exports, as `kind name`.
    pub exports: Vec<String>,
    /// Modules it imports from.
    pub depends_on: Vec<String>,
    /// Modules that import it.
    pub depended_on_by: Vec<String>,
    /// Test files that reach it.
    pub tests: Vec<String>,
}

/// The generated artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeArtifact {
    /// Workspace revision it was built at.
    pub revision: u64,
    /// Files it covers.
    pub files: usize,
    /// Modules, deepest paths last.
    pub modules: Vec<Module>,
    /// Claims, each with its sources.
    pub claims: Vec<Claim>,
    /// What this artifact is and is not, carried with the data so no reader
    /// has to remember it.
    pub authority: String,
    /// sha256 over the claims, so a client can tell two builds apart.
    pub artifact_hash: String,
}

/// A claim checked against the repository as it is now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckedClaim {
    /// The claim.
    pub claim: Claim,
    /// `fresh` | `stale` | `missing`.
    pub status: String,
    /// Sources that changed or vanished, with what changed.
    pub changed: Vec<String>,
}

/// The sentence every reader gets with the artifact.
pub const AUTHORITY: &str = "This is a generated map of the repository: a discovery aid and a cache, never authority. Every claim names the files it came from; read those files before acting on a claim. A claim whose sources changed is reported as stale and must not be used as evidence.";

fn sha(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

/// The module a path belongs to: its directory, or `.` at the root.
#[must_use]
pub fn module_of(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((dir, _)) if !dir.is_empty() => dir.to_owned(),
        _ => ".".to_owned(),
    }
}

/// Whether a path looks like a test file (the same rule the evidence graph
/// uses, kept here so the artifact does not need the graph to say it).
fn is_test(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("/tests/")
        || p.starts_with("tests/")
        || p.contains("/test/")
        || p.starts_with("test/")
        || p.contains("_test.")
        || p.contains(".test.")
        || p.contains("test_")
        || p.contains(".spec.")
}

/// What `build` needs to know about one file.
pub struct FileFacts<'a> {
    /// Root-relative path.
    pub path: &'a str,
    /// sha256 of its text.
    pub content_hash: &'a str,
    /// Top-level symbols, as `(kind, name)`.
    pub exports: Vec<(String, String)>,
    /// Paths it imports.
    pub imports: Vec<String>,
    /// Test files that reach it.
    pub tests: Vec<String>,
}

/// Build the artifact from per-file facts.
#[must_use]
pub fn build(revision: u64, files: &[FileFacts<'_>]) -> KnowledgeArtifact {
    let mut modules: BTreeMap<String, Module> = BTreeMap::new();
    let mut sources: BTreeMap<String, Vec<ClaimSource>> = BTreeMap::new();
    for f in files {
        let m = module_of(f.path);
        let entry = modules.entry(m.clone()).or_insert_with(|| Module {
            path: m.clone(),
            files: vec![],
            exports: vec![],
            depends_on: vec![],
            depended_on_by: vec![],
            tests: vec![],
        });
        entry.files.push(f.path.to_owned());
        for (kind, name) in &f.exports {
            let e = format!("{kind} {name}");
            if !entry.exports.contains(&e) {
                entry.exports.push(e);
            }
        }
        for i in &f.imports {
            let dep = module_of(i);
            if dep != m && !entry.depends_on.contains(&dep) {
                entry.depends_on.push(dep);
            }
        }
        for t in &f.tests {
            if !entry.tests.contains(t) {
                entry.tests.push(t.clone());
            }
        }
        sources.entry(m).or_default().push(ClaimSource {
            path: f.path.to_owned(),
            content_hash: f.content_hash.to_owned(),
        });
    }
    // Dependents are the mirror of dependencies, so they cannot disagree.
    let edges: Vec<(String, String)> = modules
        .values()
        .flat_map(|m| m.depends_on.iter().map(|d| (m.path.clone(), d.clone())))
        .collect();
    for (from, to) in edges {
        if let Some(t) = modules.get_mut(&to)
            && !t.depended_on_by.contains(&from)
        {
            t.depended_on_by.push(from);
        }
    }
    let mut claims = Vec::new();
    for m in modules.values() {
        let src = sources.get(&m.path).cloned().unwrap_or_default();
        let code: Vec<&String> = m.files.iter().filter(|f| !is_test(f)).collect();
        claims.push(Claim {
            module: m.path.clone(),
            kind: ClaimKind::Contents,
            text: format!(
                "{} holds {} file(s) ({} of them test files): {}",
                m.path,
                m.files.len(),
                m.files.len() - code.len(),
                m.files.join(", ")
            ),
            sources: src.clone(),
        });
        if !m.exports.is_empty() {
            claims.push(Claim {
                module: m.path.clone(),
                kind: ClaimKind::Exports,
                text: format!("{} defines {}", m.path, m.exports.join(", ")),
                sources: src.clone(),
            });
        }
        if !m.depends_on.is_empty() {
            claims.push(Claim {
                module: m.path.clone(),
                kind: ClaimKind::Dependencies,
                text: format!("{} imports from {}", m.path, m.depends_on.join(", ")),
                sources: src.clone(),
            });
        }
        if !m.depended_on_by.is_empty() {
            claims.push(Claim {
                module: m.path.clone(),
                kind: ClaimKind::Dependents,
                text: format!("{} is imported by {}", m.path, m.depended_on_by.join(", ")),
                sources: src.clone(),
            });
        }
        if !m.tests.is_empty() {
            claims.push(Claim {
                module: m.path.clone(),
                kind: ClaimKind::Tests,
                text: format!("{} is exercised by {}", m.path, m.tests.join(", ")),
                sources: src,
            });
        }
    }
    let artifact_hash = sha(&claims.iter().map(|c| c.text.as_str()).collect::<Vec<_>>());
    KnowledgeArtifact {
        revision,
        files: files.len(),
        modules: modules.into_values().collect(),
        claims,
        authority: AUTHORITY.to_owned(),
        artifact_hash,
    }
}

/// Check claims against the repository as it is now: `current` maps path to
/// the file's hash today (a path that is gone is simply absent).
///
/// A claim is fresh only when every source it names still has the hash it had.
/// This is what keeps the artifact a cache rather than a second truth.
#[must_use]
pub fn check(
    artifact: &KnowledgeArtifact,
    current: &BTreeMap<String, String>,
    module: Option<&str>,
) -> Vec<CheckedClaim> {
    artifact
        .claims
        .iter()
        .filter(|c| module.is_none_or(|m| c.module == m || c.module.starts_with(&format!("{m}/"))))
        .map(|claim| {
            let mut changed = Vec::new();
            let mut missing = false;
            for s in &claim.sources {
                match current.get(&s.path) {
                    Some(now) if now == &s.content_hash => {}
                    Some(now) => changed.push(format!(
                        "{} changed ({} -> {})",
                        s.path,
                        &s.content_hash[..8.min(s.content_hash.len())],
                        &now[..8.min(now.len())]
                    )),
                    None => {
                        missing = true;
                        changed.push(format!("{} is gone", s.path));
                    }
                }
            }
            let status = if missing {
                "missing"
            } else if changed.is_empty() {
                "fresh"
            } else {
                "stale"
            };
            CheckedClaim {
                claim: claim.clone(),
                status: status.to_owned(),
                changed,
            }
        })
        .collect()
}
