//! CI results as external evidence (PX-009; docs/29 "CI evidence",
//! REQ-EV-0010).
//!
//! A forge's check runs for the commit a task's pull request carries are
//! recorded as evidence with provenance `ci`: who ran them, which run, on
//! which commit, what they concluded and the log they left. They inform the
//! reviewer and the verification plan; they are never a verification result.
//! Nothing here produces a [`crate::CheckResult`] or a
//! [`crate::VerificationRun`], and the Acceptance Gate reads neither of the
//! types below, so a green run elsewhere cannot pass a check that Modbit has
//! not run itself.
//!
//! A check run that names another commit than the one asked about is
//! refused, never filed under the wrong revision.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Provenance label of CI evidence.
pub const CI_PROVENANCE: &str = "ci";

/// The evidence class CI results belong to: external and informational.
pub const CI_EVIDENCE_CLASS: &str = "external_ci";

/// Bytes of a check run's own output kept inline in its log text; the rest
/// is cut and the cut is said.
pub const MAX_LOG_BYTES: usize = 64 * 1024;

/// One check run accepted as evidence for the commit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiCheck {
    /// Check name.
    pub name: String,
    /// The forge's run id.
    pub run_id: u64,
    /// `queued` | `in_progress` | `completed`.
    pub status: String,
    /// `success` | `failure` | `neutral` | `cancelled` | `skipped` |
    /// `timed_out` | `action_required`; empty while not completed.
    pub conclusion: String,
    /// Where a person reads it.
    pub url: String,
    /// When it completed, as the forge said.
    pub completed_at: String,
    /// The run's own output (title, summary, text), as its log.
    pub log: String,
    /// Whether the log was cut at [`MAX_LOG_BYTES`].
    pub log_truncated: bool,
}

/// One check run refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiRejected {
    /// Check name, when it had one.
    pub name: String,
    /// The commit it named.
    pub head_sha: String,
    /// `MISMATCHED_COMMIT` | `MALFORMED`.
    pub reason: String,
}

/// What one ingestion found for one commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiIngestion {
    /// The commit asked about.
    pub commit: String,
    /// Check runs recorded as evidence.
    pub accepted: Vec<CiCheck>,
    /// Check runs refused.
    pub rejected: Vec<CiRejected>,
}

fn text(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_owned()
}

/// Cut `s` to at most `max` bytes on a character boundary.
fn cut(s: &str, max: usize) -> (String, bool) {
    if s.len() <= max {
        return (s.to_owned(), false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_owned(), true)
}

/// Normalize a forge's check runs (as `forge.ci.status` returns them) for
/// `commit`. A run that names another commit, or that has no name or no
/// commit, is refused with its reason.
#[must_use]
pub fn ingest(commit: &str, checks: &[Value]) -> CiIngestion {
    let mut out = CiIngestion {
        commit: commit.to_owned(),
        ..CiIngestion::default()
    };
    for c in checks {
        let name = text(c, "name");
        let head_sha = text(c, "head_sha");
        if name.is_empty() || head_sha.is_empty() {
            out.rejected.push(CiRejected {
                name,
                head_sha,
                reason: "MALFORMED".into(),
            });
            continue;
        }
        if !head_sha.eq_ignore_ascii_case(commit) {
            out.rejected.push(CiRejected {
                name,
                head_sha,
                reason: "MISMATCHED_COMMIT".into(),
            });
            continue;
        }
        let output = &c["output"];
        let raw = [
            text(output, "title"),
            text(output, "summary"),
            text(output, "text"),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
        let (log, log_truncated) = cut(&raw, MAX_LOG_BYTES);
        out.accepted.push(CiCheck {
            name,
            run_id: c["id"].as_u64().unwrap_or(0),
            status: text(c, "status"),
            conclusion: text(c, "conclusion"),
            url: text(c, "url"),
            completed_at: text(c, "completed_at"),
            log,
            log_truncated: log_truncated || c["output_truncated"].as_bool() == Some(true),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_run_on_another_commit_or_without_identity_is_refused() {
        let sha = "a".repeat(40);
        let r = ingest(
            &sha,
            &[
                json!({"name": "ci", "id": 7, "status": "completed", "conclusion": "success", "head_sha": sha.to_uppercase(), "output": {"title": "ok", "summary": "3 passed"}}),
                json!({"name": "stale", "id": 8, "status": "completed", "conclusion": "success", "head_sha": "b".repeat(40)}),
                json!({"id": 9, "head_sha": sha}),
            ],
        );
        assert_eq!(r.accepted.len(), 1);
        assert_eq!(r.accepted[0].run_id, 7);
        assert_eq!(r.accepted[0].log, "ok\n\n3 passed");
        assert_eq!(
            r.rejected
                .iter()
                .map(|x| x.reason.as_str())
                .collect::<Vec<_>>(),
            vec!["MISMATCHED_COMMIT", "MALFORMED"]
        );
    }

    #[test]
    fn a_long_log_is_cut_on_a_character_boundary_and_says_so() {
        let sha = "c".repeat(40);
        let long = "é".repeat(MAX_LOG_BYTES);
        let r = ingest(
            &sha,
            &[json!({"name": "ci", "id": 1, "head_sha": sha, "output": {"text": long}})],
        );
        assert!(r.accepted[0].log_truncated);
        assert!(r.accepted[0].log.len() <= MAX_LOG_BYTES);
        assert!(r.accepted[0].log.chars().all(|c| c == 'é'));
    }
}
