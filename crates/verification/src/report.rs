//! Normalized test reports and failing-check identity (docs/64 §2).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Verification stage (docs/64 §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage {
    /// Before the first write.
    Baseline,
    /// Bounded inner-loop run.
    Targeted,
    /// Final candidate revision.
    Completion,
    /// Isolated rerun of specific checks.
    Rerun,
}

/// Parser confidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    /// A structured reporter was parsed.
    Structured,
    /// Exit code and output heuristics only.
    Heuristic,
}

/// Runner family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerFamily {
    /// vitest JSON reporter.
    Vitest,
    /// jest JSON reporter (same shape as vitest).
    Jest,
    /// pytest JUnit XML.
    Pytest,
    /// cargo test libtest human output.
    Cargo,
    /// Explicitly configured command; exit code only.
    ConfiguredCommand,
}

/// Overall report status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportStatus {
    /// All checks passed.
    Passed,
    /// At least one failed.
    Failed,
    /// Runner error.
    Error,
    /// Timed out.
    Timeout,
    /// Cancelled.
    Cancelled,
    /// Outcome not establishable (INDETERMINATE for acceptance, REQ-EV-0068).
    Unknown,
}

/// Check kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    /// Test case.
    Test,
    /// Build.
    Build,
    /// Typecheck.
    Typecheck,
    /// Lint.
    Lint,
    /// Diagnostic.
    Diagnostic,
    /// Diff invariant.
    Invariant,
    /// Configured command.
    Command,
}

/// Check status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckStatus {
    /// Pass.
    Pass,
    /// Fail.
    Fail,
    /// Runner/infrastructure error.
    Error,
    /// Skipped.
    Skip,
    /// Timed out.
    Timeout,
    /// Flaky (only the rerun protocol assigns this).
    Flaky,
    /// Unknown.
    Unknown,
}

/// Location of a check.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// Path (repository-relative when known).
    pub path: Option<String>,
    /// Line.
    pub line: Option<u32>,
    /// Symbol / case name.
    pub symbol: Option<String>,
}

/// docs/64 `CheckResult`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    /// Stable, runner-normalized identity.
    pub check_id: String,
    /// Kind.
    pub kind: CheckKind,
    /// Status.
    pub status: CheckStatus,
    /// Duration.
    pub duration_ms: u64,
    /// Location.
    pub location: Location,
    /// Runner/language error class.
    pub error_class: Option<String>,
    /// Normalized message (volatile tokens removed).
    pub message_fingerprint: Option<String>,
    /// Bounded excerpt.
    pub message_excerpt: Option<String>,
    /// Byte range into the raw output for this check.
    pub output_range: Option<(usize, usize)>,
}

/// Counts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    /// Pass.
    pub pass: u32,
    /// Fail.
    pub fail: u32,
    /// Error.
    pub error: u32,
    /// Skip.
    pub skip: u32,
    /// Timeout.
    pub timeout: u32,
    /// Flaky.
    pub flaky: u32,
}

/// Runner description.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerInfo {
    /// Family.
    pub family: RunnerFamily,
    /// Version text when known.
    pub version: String,
    /// argv.
    pub argv: Vec<String>,
    /// cwd.
    pub cwd: String,
}

/// Parser description.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParserInfo {
    /// Adapter name.
    pub adapter: String,
    /// Adapter version.
    pub adapter_version: String,
    /// Confidence.
    pub confidence: Confidence,
}

/// docs/64 `TestReport`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestReport {
    /// Report id.
    pub report_id: String,
    /// Verification run.
    pub verification_run_id: String,
    /// Stage.
    pub stage: Stage,
    /// Candidate revision.
    pub candidate_revision: String,
    /// Environment digest.
    pub environment_digest: String,
    /// Runner.
    pub runner: RunnerInfo,
    /// Parser.
    pub parser: ParserInfo,
    /// Status.
    pub status: ReportStatus,
    /// Counts.
    pub counts: Counts,
    /// Checks.
    pub checks: Vec<CheckResult>,
    /// Object hash of the full raw stdout+stderr (never dropped).
    pub raw_output_ref: String,
    /// Exit code when known.
    pub exit_code: Option<i32>,
}

impl TestReport {
    /// Recompute counts and overall status from the checks and exit code.
    pub fn finalize(&mut self) {
        let mut c = Counts::default();
        for ch in &self.checks {
            match ch.status {
                CheckStatus::Pass => c.pass += 1,
                CheckStatus::Fail => c.fail += 1,
                CheckStatus::Error => c.error += 1,
                CheckStatus::Skip => c.skip += 1,
                CheckStatus::Timeout => c.timeout += 1,
                CheckStatus::Flaky => c.flaky += 1,
                CheckStatus::Unknown => {}
            }
        }
        self.counts = c;
        self.status = if matches!(self.status, ReportStatus::Timeout | ReportStatus::Cancelled) {
            self.status
        } else if self.checks.iter().any(|c| c.status == CheckStatus::Timeout) {
            ReportStatus::Timeout
        } else if self.counts.fail > 0 {
            ReportStatus::Failed
        } else if self.counts.error > 0 {
            ReportStatus::Error
        } else if self.checks.iter().any(|c| c.status == CheckStatus::Unknown) {
            ReportStatus::Unknown
        } else if self.checks.is_empty() {
            match self.exit_code {
                Some(0) => ReportStatus::Passed,
                Some(_) => ReportStatus::Failed,
                None => ReportStatus::Unknown,
            }
        } else {
            ReportStatus::Passed
        };
    }

    /// Failing checks (FAIL/ERROR/TIMEOUT), the model's first view (REQ-EV-0107).
    #[must_use]
    pub fn failing(&self) -> Vec<&CheckResult> {
        self.checks
            .iter()
            .filter(|c| {
                matches!(
                    c.status,
                    CheckStatus::Fail | CheckStatus::Error | CheckStatus::Timeout
                )
            })
            .collect()
    }
}

/// Normalize a message: strip addresses, timestamps, durations, temp paths,
/// hex ids, line:col numbers and pids (docs/64 §2).
#[must_use]
pub fn normalize_message(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut prev_space = false;
    for token in msg.split_whitespace() {
        let t = token.trim_matches(|c: char| c == ',' || c == ';' || c == ':');
        let norm = if t.len() > 2
            && t.starts_with('(')
            && t.ends_with(')')
            && t[1..t.len() - 1].chars().all(|c| c.is_ascii_digit())
        {
            "<pid>".to_owned()
        } else if t.starts_with("0x")
            && t.len() > 4
            && t[2..].chars().all(|c| c.is_ascii_hexdigit())
        {
            "<addr>".to_owned()
        } else if t.chars().filter(|c| c.is_ascii_hexdigit()).count() >= 12
            && t.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        {
            "<hex>".to_owned()
        } else if t.contains("/tmp/") || t.contains("\\Temp\\") || t.contains("/var/folders/") {
            "<tmp>".to_owned()
        } else if t.ends_with("ms")
            || t.ends_with('s')
                && t[..t.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.')
                && t.len() > 1
        {
            "<duration>".to_owned()
        } else if t
            .chars()
            .all(|c| c.is_ascii_digit() || c == ':' || c == '-' || c == 'T' || c == '.' || c == 'Z')
            && t.chars().filter(|c| c.is_ascii_digit()).count() >= 6
        {
            "<ts>".to_owned()
        } else if let Some((path, _)) = t.rsplit_once(':')
            && path.contains('.')
            && t.rsplit(':')
                .next()
                .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
        {
            strip_line_cols(t)
        } else {
            t.to_owned()
        };
        if !norm.is_empty() {
            if prev_space {
                out.push(' ');
            }
            out.push_str(&norm);
            prev_space = true;
        }
    }
    out
}

fn strip_line_cols(t: &str) -> String {
    let mut parts: Vec<&str> = t.split(':').collect();
    while parts.len() > 1
        && parts
            .last()
            .is_some_and(|p| p.chars().all(|c| c.is_ascii_digit()))
    {
        parts.pop();
    }
    parts.join(":")
}

/// `failure_signature = normalize(check_id, kind, error_class, path, symbol, fingerprint)`;
/// `None` for FLAKY/UNKNOWN/passing checks (docs/64 §2).
#[must_use]
pub fn failure_signature(c: &CheckResult) -> Option<String> {
    if !matches!(
        c.status,
        CheckStatus::Fail | CheckStatus::Error | CheckStatus::Timeout
    ) {
        return None;
    }
    let material = format!(
        "{}|{:?}|{}|{}|{}|{}",
        c.check_id,
        c.kind,
        c.error_class.as_deref().unwrap_or(""),
        c.location.path.as_deref().unwrap_or(""),
        c.location.symbol.as_deref().unwrap_or(""),
        c.message_fingerprint.as_deref().unwrap_or("")
    );
    let h = hex::encode(Sha256::digest(material.as_bytes()));
    Some(format!("{}:{}", c.check_id, &h[..16]))
}
