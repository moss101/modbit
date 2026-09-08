//! Locked-path guard (DR-M0-002): every commit in a range that changes a locked
//! path must carry a trailer naming an accepted Decision Record that exists in
//! that commit's tree. Uses the real `git` binary; nothing is simulated.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use globset::Glob;

use crate::Rules;

/// One locked-path violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedViolation {
    /// Abbreviated commit id.
    pub commit: String,
    /// Locked paths the commit changed.
    pub paths: Vec<String>,
    /// Why the commit was rejected.
    pub problem: String,
}

impl std::fmt::Display for LockedViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LOCKED {} changes {} : {}",
            self.commit,
            self.paths.join(", "),
            self.problem
        )
    }
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    String::from_utf8(out.stdout).context("git output is not UTF-8")
}

/// Does `blob` (a file's bytes) declare `status: accepted` in its front matter?
pub fn is_accepted_record(text: &str) -> bool {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return false;
    }
    for line in lines {
        let line = line.trim();
        if line == "---" {
            return false;
        }
        if let Some(v) = line.strip_prefix("status:") {
            return v.trim() == "accepted";
        }
    }
    false
}

/// Commits reachable from `head` but not from `base`, oldest first.
pub fn commits_in_range(repo: &Path, base: Option<&str>, head: &str) -> Result<Vec<String>> {
    let range = match base {
        Some(b) if !b.is_empty() && !b.chars().all(|c| c == '0') => format!("{b}..{head}"),
        _ => format!("{head}^..{head}"),
    };
    let out = match git(repo, &["rev-list", "--reverse", "--no-merges", &range]) {
        Ok(o) => o,
        // A root commit has no parent: check it alone.
        Err(_) if base.is_none() => git(repo, &["rev-list", "--max-count=1", head])?,
        Err(e) => return Err(e),
    };
    Ok(out.lines().map(str::to_owned).collect())
}

/// Check every commit in `base..head` of `repo` against the locked rules.
pub fn check_locked(
    repo: &Path,
    base: Option<&str>,
    head: &str,
    rules: &Rules,
) -> Result<Vec<LockedViolation>> {
    let matchers: Vec<_> = rules
        .locked
        .iter()
        .map(|r| {
            Glob::new(&r.path)
                .expect("validated glob")
                .compile_matcher()
        })
        .collect();
    let policy = &rules.locked_policy;
    let mut violations = Vec::new();
    for sha in commits_in_range(repo, base, head)? {
        let changed = git(
            repo,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", &sha],
        )?;
        let locked: Vec<String> = changed
            .lines()
            .filter(|p| matchers.iter().any(|m| m.is_match(p)))
            .map(str::to_owned)
            .collect();
        if locked.is_empty() {
            continue;
        }
        let short = git(repo, &["rev-parse", "--short", &sha])?
            .trim()
            .to_owned();
        let trailer_fmt = format!("--format=%(trailers:key={},valueonly)", policy.trailer);
        let value = git(repo, &["show", "-s", &trailer_fmt, &sha])?;
        let value = value.trim();
        let problem = if value.is_empty() {
            Some(format!("no `{}:` trailer", policy.trailer))
        } else if !value.starts_with(&format!("{}/", policy.decisions_dir)) {
            Some(format!(
                "`{}: {}` does not point under {}",
                policy.trailer, value, policy.decisions_dir
            ))
        } else {
            match git(repo, &["show", &format!("{sha}:{value}")]) {
                Err(_) => Some(format!("`{value}` does not exist in commit {short}")),
                Ok(text) if !is_accepted_record(&text) => Some(format!(
                    "`{value}` is not an accepted Decision Record (status: accepted)"
                )),
                Ok(_) => None,
            }
        };
        if let Some(problem) = problem {
            violations.push(LockedViolation {
                commit: short,
                paths: locked,
                problem,
            });
        }
    }
    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::is_accepted_record;

    #[test]
    fn front_matter_status_detection() {
        assert!(is_accepted_record(
            "---\nid: X\nstatus: accepted\n---\nbody"
        ));
        assert!(!is_accepted_record("---\nstatus: proposed\n---\n"));
        assert!(!is_accepted_record("status: accepted\n"));
        assert!(!is_accepted_record("---\nid: X\n---\nstatus: accepted\n"));
    }
}
