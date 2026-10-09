//! External retrieval baselines (REQ-PX-137): the tools a developer or an
//! agent would reach for without Modbit's indexes, run as real processes
//! over the same corpus and the same cases as the planner profiles.
//!
//! - **ripgrep** is the required baseline. A missing binary FAILS the run
//!   (`BaselineMissing`); a report is never produced with the column quietly
//!   left out.
//! - **zvec-grep** is optional: detected on `PATH` (or `MODBIT_ZVEC_GREP_BIN`)
//!   and never downloaded. When it is absent the report says
//!   `NOT_INSTALLED`; when it is present its output is parsed as file paths,
//!   one per line (an optional `:line:` suffix is cut), and only paths that
//!   exist in the corpus count. Its contract was not available when this was
//!   written, so a run of it is reported as parsed under that assumption.
//!
//! The ripgrep profile is a deterministic stand-in for what an agent does
//! with a search tool: it searches the query's content words one by one
//! (one invocation each, which is its step count), and ranks files by how
//! many distinct words they contain, then by total matches. It is not an
//! agent and not a tuned search; it is the honest lower-effort baseline the
//! planner profiles are asked to beat.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// A required baseline binary that is not there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineMissing {
    /// The tool.
    pub tool: String,
    /// Where it was looked for.
    pub looked: String,
}

impl std::fmt::Display for BaselineMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BASELINE MISSING: {} was not found ({}); the run fails instead of leaving the column out. Install it or set the *_BIN variable",
            self.tool, self.looked
        )
    }
}

fn exe_names(base: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![format!("{base}.exe"), base.to_owned()]
    } else {
        vec![base.to_owned()]
    }
}

/// Find an executable named `base` through an explicit override and then the
/// directories of `path_var`. Pure, so it can be tested without touching the
/// process environment.
#[must_use]
pub fn find_binary(
    base: &str,
    override_path: Option<&str>,
    path_var: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(p) = override_path.filter(|p| !p.trim().is_empty()) {
        let p = PathBuf::from(p.trim());
        return p.is_file().then_some(p);
    }
    let path_var = path_var?;
    for dir in std::env::split_paths(path_var) {
        for name in exe_names(base) {
            let candidate = dir.join(&name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Locate ripgrep (`MODBIT_RG_BIN`, then `PATH`) and check that it is
/// ripgrep.
///
/// # Errors
/// `BaselineMissing` when there is no working ripgrep.
pub fn detect_rg(
    override_path: Option<&str>,
    path_var: Option<&OsStr>,
) -> Result<PathBuf, BaselineMissing> {
    let missing = || BaselineMissing {
        tool: "ripgrep (rg)".into(),
        looked: format!("MODBIT_RG_BIN={}, PATH", override_path.unwrap_or("unset")),
    };
    let bin = find_binary("rg", override_path, path_var).ok_or_else(missing)?;
    let out = Command::new(&bin)
        .arg("--version")
        .output()
        .map_err(|_| missing())?;
    if out.status.success()
        && String::from_utf8_lossy(&out.stdout)
            .to_lowercase()
            .contains("ripgrep")
    {
        Ok(bin)
    } else {
        Err(missing())
    }
}

/// [`detect_rg`] on the process environment.
///
/// # Errors
/// `BaselineMissing`.
pub fn detect_rg_from_env() -> Result<PathBuf, BaselineMissing> {
    detect_rg(
        std::env::var("MODBIT_RG_BIN").ok().as_deref(),
        std::env::var_os("PATH").as_deref(),
    )
}

/// Locate zvec-grep (`MODBIT_ZVEC_GREP_BIN`, then `PATH`); never downloads.
#[must_use]
pub fn detect_zvec_from_env() -> Option<PathBuf> {
    find_binary(
        "zvec-grep",
        std::env::var("MODBIT_ZVEC_GREP_BIN").ok().as_deref(),
        std::env::var_os("PATH").as_deref(),
    )
}

const STOP: [&str; 24] = [
    "the", "for", "and", "with", "from", "into", "that", "this", "where", "which", "what", "how",
    "are", "was", "our", "its", "per", "all", "any", "not", "who", "why", "when", "does",
];

/// The content words of a query: lowercase, three letters or more, not a
/// stopword, in order and without repeats.
#[must_use]
pub fn query_terms(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in query.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let w = w.to_lowercase();
        if w.len() >= 3 && !STOP.contains(&w.as_str()) && !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

/// One baseline's answer to one case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExternalAnswer {
    /// Ranked unique paths, relative to the corpus root.
    pub paths: Vec<String>,
    /// Process invocations made.
    pub steps: usize,
    /// Wall time of those invocations.
    pub latency_ms: f64,
}

/// Run the ripgrep profile for one query.
///
/// # Errors
/// The process could not be started or failed for a reason other than "no
/// match" (exit code 1).
pub fn rg_answer(rg: &Path, root: &Path, query: &str, k: usize) -> Result<ExternalAnswer, String> {
    let terms = query_terms(query);
    let started = Instant::now();
    // path -> (distinct terms matched, total matches)
    let mut score: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for term in &terms {
        let out = Command::new(rg)
            .current_dir(root)
            .args([
                "--count-matches",
                "--ignore-case",
                "--fixed-strings",
                "--no-heading",
                "--with-filename",
                "--no-messages",
                "--no-ignore",
                "--word-regexp",
                "--glob",
                "!.git",
                "-e",
                term,
                "--",
                ".",
            ])
            .output()
            .map_err(|e| format!("starting {}: {e}", rg.display()))?;
        // Exit 1 is "no matches"; anything above is an error.
        if out.status.code().is_none_or(|c| c > 1) {
            return Err(format!(
                "{} failed on `{term}`: {}",
                rg.display(),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let Some((path, count)) = line.rsplit_once(':') else {
                continue;
            };
            let path = path.trim_start_matches("./").replace('\\', "/");
            let e = score.entry(path).or_default();
            e.0 += 1;
            e.1 += count.trim().parse::<usize>().unwrap_or(0);
        }
    }
    let mut ranked: Vec<(String, (usize, usize))> = score.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(ExternalAnswer {
        paths: ranked.into_iter().take(k).map(|(p, _)| p).collect(),
        steps: terms.len(),
        latency_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Run zvec-grep for one query (paths one per line, optional `:line:` suffix).
///
/// # Errors
/// The process could not be started or exited non-zero.
pub fn zvec_answer(
    zvec: &Path,
    root: &Path,
    query: &str,
    k: usize,
) -> Result<ExternalAnswer, String> {
    let started = Instant::now();
    let out = Command::new(zvec)
        .current_dir(root)
        .arg(query)
        .output()
        .map_err(|e| format!("starting {}: {e}", zvec.display()))?;
    if !out.status.success() {
        return Err(format!(
            "{} exited {:?}: {}",
            zvec.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut paths: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let candidate = line.split(':').next().unwrap_or_default().trim();
        let candidate = candidate.trim_start_matches("./").replace('\\', "/");
        if !candidate.is_empty() && root.join(&candidate).is_file() && !paths.contains(&candidate) {
            paths.push(candidate);
        }
        if paths.len() >= k {
            break;
        }
    }
    Ok(ExternalAnswer {
        paths,
        steps: 1,
        latency_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}
