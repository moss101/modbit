//! `modbit-cli` forge verbs and the forge evidence in Review (PX-127;
//! docs/29 "CI evidence", "Review-comment steering"): the headless client
//! over the commands the Core already has — `IngestCiResults`,
//! `IngestReviewComments`, `GetReviewBundle`. The CLI holds no forge logic
//! and makes no GitHub call: it asks the Core to read the forge (with the
//! Core's token, under the task's lease) and prints what the Core recorded.
//!
//! ```text
//! ci ingest        --session <id> --task <id>
//! comments ingest  --session <id> --task <id>
//! task evidence    --task <id>
//! ```
//!
//! What a forge author wrote is untrusted text: it is printed as data,
//! with every control character (terminal escapes included) replaced, and
//! labelled. A green CI run is printed as external evidence beside the
//! verification runs and is never printed as acceptance.

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    CiResultsIngested, GetReviewBundle, IngestCiResults, IngestReviewComments, ReviewBundle,
    ReviewCommentsIngestedView,
};
use prost::Message;

use crate::{USAGE, envelope, envelope_fenced, join_lease, parse_id};

/// Text from the forge, safe to put on a terminal: control characters
/// (ESC, C1 controls, DEL, a carriage return) become `?`; a newline or a
/// tab survives.
pub(crate) fn plain(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' | '\t' => c,
            c if c.is_control() => '?',
            c => c,
        })
        .collect()
}

fn dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

/// The forge evidence of a review bundle, one fact per line.
pub(crate) fn print_forge_evidence(b: &ReviewBundle) {
    if !b.ci_evidence.is_empty() || !b.ci_rejected.is_empty() {
        println!(
            "ci_note external evidence from the forge; it informs the review and is never a verification result or an acceptance"
        );
    }
    for c in &b.ci_evidence {
        println!(
            "ci {} run={} status={} conclusion={} commit={} provenance={} completed_at={} url={} log={}{}",
            plain(&c.name),
            c.run_id,
            plain(&c.status),
            dash(&plain(&c.conclusion)),
            c.commit.chars().take(12).collect::<String>(),
            plain(&c.provenance),
            dash(&plain(&c.completed_at)),
            dash(&plain(&c.url)),
            dash(&c.log_ref),
            if c.log_truncated { " (log cut)" } else { "" }
        );
    }
    for r in &b.ci_rejected {
        println!(
            "ci_refused {} head={} reason={}",
            plain(&r.name),
            r.head_sha.chars().take(12).collect::<String>(),
            plain(&r.reason)
        );
    }
    for t in &b.review_comments {
        let place = match (t.path.is_empty(), t.line) {
            (true, _) => String::new(),
            (false, 0) => format!(" on {}", plain(&t.path)),
            (false, l) => format!(" on {}:{l}", plain(&t.path)),
        };
        println!(
            "comment #{} {} by @{} [{}]{place} {}{} answered={} reported={} url={}",
            t.comment_id,
            plain(&t.kind),
            plain(&t.author),
            plain(&t.trust),
            t.disposition,
            if t.reason.is_empty() {
                String::new()
            } else {
                format!(":{}", plain(&t.reason))
            },
            t.answered,
            t.reported_back,
            dash(&plain(&t.url))
        );
        for line in plain(&t.body).lines() {
            println!("  | {line}");
        }
    }
}

/// The verbs; `Ok(true)` when `words` was one of them.
pub(crate) async fn run(client: &mut Client, words: &[&str]) -> Result<bool, String> {
    let opt = |flag: &str| {
        words
            .iter()
            .position(|w| *w == flag)
            .and_then(|i| words.get(i + 1).copied())
    };
    match words {
        ["ci", "ingest", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let task = parse_id(opt("--task").ok_or(USAGE)?)?;
            let lease = join_lease(client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "IngestCiResults",
                    IngestCiResults {
                        task_id: Some(task),
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: CiResultsIngested = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "ci-ingested provider={} repo={}/{} pull={} commit={} checks={} refused={} class={} offset={}",
                r.provider,
                r.owner,
                r.repo,
                r.pull_number,
                r.commit.chars().take(12).collect::<String>(),
                r.checks.len(),
                r.rejected.len(),
                r.evidence_class,
                r.offset
            );
            for c in &r.checks {
                println!(
                    "ci {} run={} status={} conclusion={} provenance={}",
                    plain(&c.name),
                    c.run_id,
                    plain(&c.status),
                    dash(&plain(&c.conclusion)),
                    plain(&c.provenance)
                );
            }
            for x in &r.rejected {
                println!("ci_refused {} {}", plain(&x.name), plain(&x.reason));
            }
            Ok(true)
        }
        ["comments", "ingest", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let task = parse_id(opt("--task").ok_or(USAGE)?)?;
            let lease = join_lease(client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "IngestReviewComments",
                    IngestReviewComments {
                        task_id: Some(task),
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: ReviewCommentsIngestedView = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "comments-ingested repo={}/{} pull={} steered={} ignored={} already_taken={} offset={}",
                r.owner,
                r.repo,
                r.pull_number,
                r.steered.len(),
                r.ignored.len(),
                r.already_taken,
                r.offset
            );
            for c in &r.steered {
                println!(
                    "comment #{} {} by @{} [UNTRUSTED_EXTERNAL_CONTENT] STEERED input={}",
                    c.comment_id,
                    plain(&c.kind),
                    plain(&c.author),
                    plain(&c.input_id)
                );
            }
            for c in &r.ignored {
                println!(
                    "comment #{} {} by @{} IGNORED:{}",
                    c.comment_id,
                    plain(&c.kind),
                    plain(&c.author),
                    plain(&c.reason)
                );
            }
            Ok(true)
        }
        ["task", "evidence", ..] => {
            let task = parse_id(opt("--task").ok_or(USAGE)?)?;
            let ack = client
                .command(envelope(
                    "GetReviewBundle",
                    GetReviewBundle {
                        task_id: Some(task),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let b: ReviewBundle = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "evidence state={} workspace_revision={} receipts={}",
                b.task_state, b.workspace_revision, b.receipts
            );
            for v in &b.verification_runs {
                println!(
                    "verification {} stage={} status={} checks={}",
                    v.verification_run_id,
                    v.stage,
                    v.status,
                    v.check_ids.len()
                );
            }
            for (id, status) in b.verification_runs.iter().flat_map(|v| {
                v.check_ids
                    .iter()
                    .zip(v.check_statuses.iter())
                    .map(|(i, s)| (i.clone(), s.clone()))
            }) {
                println!("check {id} {status}");
            }
            print_forge_evidence(&b);
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn forge_text_cannot_drive_a_terminal() {
        let hostile = "ok\u{1b}[2J\u{1b}]0;owned\u{7}\r\nnext\u{9b}31m\ttab\u{7f}";
        let shown = plain(hostile);
        assert!(!shown.contains('\u{1b}') && !shown.contains('\u{7}') && !shown.contains('\r'));
        assert!(!shown.contains('\u{9b}') && !shown.contains('\u{7f}'));
        assert!(shown.contains("\nnext") && shown.contains('\t'));
    }
}
