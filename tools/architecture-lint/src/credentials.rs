//! Credential-read lint (PX-130, QUAL-PX-130 "the architecture lint fails a
//! fixture that reads a credential outside the interface").
//!
//! Every credential — a provider key, an MCP server credential, a browser
//! fill value, a forge token, a cloud token or a webhook secret — is resolved
//! through the one broker interface. This check reads the Rust sources of the
//! workspace members under the rule's `roots` and reports each
//! `env::var("…")` / `env::var_os("…")` whose literal name looks like a
//! credential (it contains one of the rule's `markers`) outside the rule's
//! `allowed` path prefixes. The allowed prefixes are the broker itself and the
//! process entry points that still load a secret from their own environment
//! at start-up; each is a debt to migrate, listed by file and variable so
//! that a new read cannot slip in beside them. A name that only contains a marker as part of a
//! non-secret word (a token *budget*) is excluded by `exempt`. Test code
//! (everything from the first `#[cfg(test)]` of a file, and each member's
//! `tests/` directory) is not scanned.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// A rule confining credential reads to listed source paths.
#[derive(Debug, Clone, Deserialize)]
pub struct CredentialRule {
    /// Directories (repository-relative) whose members' `src/` trees are scanned.
    pub roots: Vec<String>,
    /// Upper-case fragments that make an environment variable name a credential.
    pub markers: Vec<String>,
    /// Fragments that exempt a name that contains a marker for another reason.
    #[serde(default)]
    pub exempt: Vec<String>,
    /// Repository-relative path prefixes where a read is allowed.
    pub allowed: Vec<String>,
    /// Reads that exist today and are still to be migrated, each exactly
    /// `<repository-relative path>::<VARIABLE>`. A new variable in the same
    /// file is not covered.
    #[serde(default)]
    pub grandfathered: Vec<String>,
    /// Authority reference explaining the rule.
    pub reason: String,
}

/// One credential read the rule refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialViolation {
    /// Repository-relative path, `/`-separated.
    pub path: String,
    /// 1-based line.
    pub line: usize,
    /// The environment variable read.
    pub name: String,
    /// The rule's reason.
    pub reason: String,
}

impl std::fmt::Display for CredentialViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CREDENTIAL {}:{}: reads `{}` outside the broker ({})",
            self.path, self.line, self.name, self.reason
        )
    }
}

/// The literal names read through `env::var(` / `env::var_os(` in `text`,
/// with the 1-based line each starts on. A call split after the opening
/// parenthesis is joined with the next line.
fn env_reads(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("#[cfg(test)]") {
            break;
        }
        if t.starts_with("//") {
            continue;
        }
        for needle in ["env::var(", "env::var_os("] {
            let mut from = 0;
            while let Some(at) = line[from..].find(needle) {
                let after = from + at + needle.len();
                let mut rest = line[after..].trim_start().to_owned();
                if rest.is_empty() {
                    rest = lines
                        .get(i + 1)
                        .map(|l| l.trim().to_owned())
                        .unwrap_or_default();
                }
                if let Some(lit) = rest.strip_prefix('"')
                    && let Some(end) = lit.find('"')
                {
                    out.push((i + 1, lit[..end].to_owned()));
                }
                from = after;
            }
        }
    }
    out
}

/// The credential reads in the sources under `repo` that the rule does not allow.
pub fn check_credentials(repo: &Path, rule: &CredentialRule) -> Result<Vec<CredentialViolation>> {
    let mut out = Vec::new();
    for root in &rule.roots {
        let base = repo.join(root);
        let Ok(members) = std::fs::read_dir(&base) else {
            continue;
        };
        let mut members: Vec<_> = members.flatten().collect();
        members.sort_by_key(std::fs::DirEntry::file_name);
        for m in members {
            let mut files = Vec::new();
            crate::spawns::rust_files(&m.path().join("src"), &mut files);
            for f in files {
                let rel = f
                    .strip_prefix(repo)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .replace('\\', "/");
                if rule.allowed.iter().any(|a| rel.starts_with(a.as_str())) {
                    continue;
                }
                let text = std::fs::read_to_string(&f)
                    .with_context(|| format!("reading {}", f.display()))?;
                for (line, name) in env_reads(&text) {
                    let credential = rule.markers.iter().any(|m| name.contains(m.as_str()))
                        && !rule.exempt.iter().any(|e| name.contains(e.as_str()));
                    if credential && !rule.grandfathered.contains(&format!("{rel}::{name}")) {
                        out.push(CredentialViolation {
                            path: rel.clone(),
                            line,
                            name,
                            reason: rule.reason.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(out)
}
