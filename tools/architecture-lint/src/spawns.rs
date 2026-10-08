//! Process-spawn lint (PX-067, docs/65 AFW-J09, QUAL-PX-067 "a direct Git
//! spawn outside the runner fails architecture lint").
//!
//! A repository is data: its configuration, hooks and attributes can name
//! programs, and almost every Git command Modbit runs would start them. The
//! one place a `git` process is built is the hardened runner in `modbit-git`.
//! This check reads the Rust sources of every workspace member under the
//! rule's `roots` and reports each `Command::new("git")` outside the rule's
//! `allowed` path prefixes. Test code (everything from the first
//! `#[cfg(test)]` of a file, and each member's `tests/` directory) is not
//! scanned: a test may drive the real `git` as the attacker does.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// A rule confining a spawned program to listed source paths.
#[derive(Debug, Clone, Deserialize)]
pub struct SpawnRule {
    /// The program name (`git`).
    pub program: String,
    /// Directories (repository-relative) whose members' `src/` trees are scanned.
    pub roots: Vec<String>,
    /// Repository-relative path prefixes where the spawn is allowed.
    pub allowed: Vec<String>,
    /// Authority reference explaining the rule.
    pub reason: String,
}

/// One spawn the rule refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnViolation {
    /// Repository-relative path, `/`-separated.
    pub path: String,
    /// 1-based line.
    pub line: usize,
    /// The offending source line, trimmed.
    pub text: String,
    /// The rule's reason.
    pub reason: String,
}

impl std::fmt::Display for SpawnViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SPAWN {}:{}: `{}` ({})",
            self.path, self.line, self.text, self.reason
        )
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The spawns of `rule.program` in the sources under `repo` that the rule does
/// not allow.
pub fn check_spawns(repo: &Path, rule: &SpawnRule) -> Result<Vec<SpawnViolation>> {
    let needles = [
        format!("Command::new(\"{}\")", rule.program),
        format!("Command::new('{}')", rule.program),
    ];
    let mut out = Vec::new();
    for root in &rule.roots {
        let base = repo.join(root);
        let Ok(members) = std::fs::read_dir(&base) else {
            continue;
        };
        let mut members: Vec<_> = members.flatten().collect();
        members.sort_by_key(std::fs::DirEntry::file_name);
        for m in members {
            let src = m.path().join("src");
            let mut files = Vec::new();
            rust_files(&src, &mut files);
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
                for (i, line) in text.lines().enumerate() {
                    let t = line.trim();
                    if t.starts_with("#[cfg(test)]") {
                        break;
                    }
                    if t.starts_with("//") {
                        continue;
                    }
                    if needles.iter().any(|n| line.contains(n.as_str())) {
                        out.push(SpawnViolation {
                            path: rel.clone(),
                            line: i + 1,
                            text: t.to_owned(),
                            reason: rule.reason.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(out)
}
