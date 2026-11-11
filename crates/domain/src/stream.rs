//! Assistant streams (PX-041, docs/65 AFW-C02): the events of one streamed
//! model output.
//!
//! One model invocation that produces text opens one stream. The stream is
//! an aggregate of its own (`AggregateType::AssistantStream`, aggregate id =
//! the stream id), so the store's per-aggregate sequence *is* the delta
//! sequence and its hash chain covers the whole stream. A stream is
//! `AssistantTextDelta*` followed by exactly one closing record:
//!
//! * `AssistantMessageCompleted` carries the full text as a content-addressed
//!   object (the OutputRef digest) and its content hash; it is the only thing
//!   a client may present as a finished assistant message;
//! * `AssistantMessageAborted` carries a typed source (user interrupt,
//!   runtime, provider, recovery). Whatever text was streamed before it is
//!   partial by definition and is never presented as complete.
//!
//! Deltas are ordinary events (offset, cursor replay, any number of
//! clients). Their retention class is [`Retention::CollapsibleOnComplete`]:
//! the completed record holds the whole text, so a reader that has seen it
//! needs no delta. Nothing here removes a delta from the log, because the
//! hash chain of the aggregate covers it.

use serde::{Deserialize, Serialize};

use crate::ids::{RunId, RunStepId, StreamId, TurnId};

/// Largest text one delta event may carry (docs/65 AFW-C02: coalesced to at
/// most 2 KiB per event).
pub const MAX_DELTA_BYTES: usize = 2048;

/// Shortest interval between two coalesced deltas of one stream, in
/// milliseconds (docs/65: at most one event per 50 ms), except that a delta
/// that reaches [`MAX_DELTA_BYTES`] is not held back.
pub const MIN_DELTA_INTERVAL_MS: u64 = 50;

/// Most text one stream may carry. A response beyond it is a runaway, not a
/// message: the stream aborts as a runtime failure and the model call is
/// cancelled, so neither the Core nor the log grows without bound.
pub const MAX_STREAM_BYTES: u64 = 4 * 1024 * 1024;

/// Most characters the completion record's `last_line` carries (PX-218): a
/// preview of how the message ended, never the message.
pub const LAST_LINE_MAX_CHARS: usize = 200;

/// The last non-empty line of `text`, trimmed, for a task header's preview
/// (PX-218, DR-PX-2026-11-11-021 MWS-20). A line longer than
/// [`LAST_LINE_MAX_CHARS`] characters is cut at a character boundary and ends
/// in an ellipsis, so the result never exceeds the bound and a reader can
/// tell a cut line from a whole one. Empty when the text has no line with
/// anything on it. The caller passes text that is already redacted.
#[must_use]
pub fn last_line(text: &str) -> String {
    let Some(line) = text.lines().rev().map(str::trim).find(|l| !l.is_empty()) else {
        return String::new();
    };
    if line.chars().count() <= LAST_LINE_MAX_CHARS {
        return line.to_owned();
    }
    let mut cut: String = line.chars().take(LAST_LINE_MAX_CHARS - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('\u{2026}');
    cut
}

/// Event type names of this aggregate.
pub const DELTA: &str = "AssistantTextDelta";
/// The closing record of a stream that finished.
pub const COMPLETED: &str = "AssistantMessageCompleted";
/// The closing record of a stream that did not.
pub const ABORTED: &str = "AssistantMessageAborted";

/// What a stream carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StreamKind {
    /// Assistant text.
    Text,
    /// A reasoning summary, only where the provider returns one and policy
    /// allows it.
    Reasoning,
}

/// Why a stream ended without completing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AbortSource {
    /// The person interrupted: a steer, a cancel, an emergency stop.
    UserInterrupt,
    /// The Core stopped it: a fence, a size bound, a refusal.
    Runtime,
    /// The provider connection failed or the provider reported an error.
    Provider,
    /// A Core restart found the stream open; no stream resumes mid-token.
    Recovery,
}

impl AbortSource {
    /// Stable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UserInterrupt => "USER_INTERRUPT",
            Self::Runtime => "RUNTIME",
            Self::Provider => "PROVIDER",
            Self::Recovery => "RECOVERY",
        }
    }
}

/// How long the store keeps a delta once its stream has completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Retention {
    /// The completed record supersedes the delta.
    CollapsibleOnComplete,
}

/// A reference to an object in the content-addressed store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRef {
    /// SHA-256 hex of the bytes.
    pub object_hash: String,
    /// Byte length.
    pub byte_length: u64,
}

/// The events of an assistant stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum StreamEvent {
    /// `AssistantTextDelta`: the next piece of a stream. Already redacted and
    /// provenance-tagged when it is appended.
    AssistantTextDelta {
        /// The stream (also the aggregate id).
        stream_id: StreamId,
        /// What the stream carries.
        kind: StreamKind,
        /// Monotonic position in the stream, from 1; equals the aggregate
        /// sequence.
        sequence: u64,
        /// Run.
        run_id: RunId,
        /// Turn.
        turn_id: TurnId,
        /// The model-invocation step.
        step_id: RunStepId,
        /// The text (at most [`MAX_DELTA_BYTES`]).
        text: String,
        /// Where this text came from: model output, never an instruction.
        provenance: String,
        /// How the store may retain it.
        retention: Retention,
    },
    /// `AssistantMessageCompleted`: the stream finished.
    AssistantMessageCompleted {
        /// The stream.
        stream_id: StreamId,
        /// What it carried.
        kind: StreamKind,
        /// Deltas appended before this record.
        delta_count: u64,
        /// The full (redacted) text as an object.
        text_ref: TextRef,
        /// SHA-256 hex of the full text.
        content_hash: String,
        /// [`last_line`] of that text (PX-218): what a task header shows of
        /// how the message ended, without reading the object. `None` only on
        /// a record written before the field existed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_line: Option<String>,
    },
    /// `AssistantMessageAborted`: the stream ended without completing.
    AssistantMessageAborted {
        /// The stream.
        stream_id: StreamId,
        /// What it carried.
        kind: StreamKind,
        /// Who ended it.
        source: AbortSource,
        /// Stable code.
        code: String,
        /// Why, in words (redacted).
        reason: String,
        /// Deltas appended before this record.
        delta_count: u64,
        /// Bytes of text those deltas carried.
        bytes_streamed: u64,
    },
}

impl StreamEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::AssistantTextDelta { .. } => DELTA,
            Self::AssistantMessageCompleted { .. } => COMPLETED,
            Self::AssistantMessageAborted { .. } => ABORTED,
        }
    }

    /// The stream the event belongs to.
    #[must_use]
    pub const fn stream_id(&self) -> StreamId {
        match self {
            Self::AssistantTextDelta { stream_id, .. }
            | Self::AssistantMessageCompleted { stream_id, .. }
            | Self::AssistantMessageAborted { stream_id, .. } => *stream_id,
        }
    }
}

/// Whether an event type closes a stream.
#[must_use]
pub fn closes(event_type: &str) -> bool {
    event_type == COMPLETED || event_type == ABORTED
}

/// What the store enforces on the next event of a stream (docs/62 QUAL-PX-041:
/// a delta without a stream id, out of sequence, after completion or over the
/// size bound is rejected by the store).
///
/// `aggregate` is the aggregate id the event is appended to, `sequence` the
/// sequence it would take, `previous` the event type at `sequence - 1`
/// (`None` for the first event).
///
/// # Errors
///
/// Returns the reason the event is refused.
pub fn check_next(
    aggregate: [u8; 16],
    sequence: u64,
    previous: Option<&str>,
    event: &StreamEvent,
) -> Result<(), String> {
    if event.stream_id().as_bytes() != &aggregate {
        return Err("the event names a stream other than the aggregate it is appended to".into());
    }
    if previous.is_some_and(closes) {
        return Err("the stream is closed; nothing follows its completion or abort record".into());
    }
    match event {
        StreamEvent::AssistantTextDelta {
            sequence: s, text, ..
        } => {
            if *s != sequence {
                return Err(format!(
                    "delta sequence {s} is out of order; the stream is at {sequence}"
                ));
            }
            if text.len() > MAX_DELTA_BYTES {
                return Err(format!(
                    "a delta of {} bytes exceeds the {MAX_DELTA_BYTES}-byte bound",
                    text.len()
                ));
            }
            if text.is_empty() {
                return Err("a delta carries text".into());
            }
        }
        StreamEvent::AssistantMessageCompleted {
            delta_count,
            last_line,
            ..
        } => {
            if sequence == 1 {
                return Err("a stream cannot complete before its first delta".into());
            }
            if *delta_count != sequence - 1 {
                return Err(format!(
                    "the completion record counts {delta_count} deltas; the stream holds {}",
                    sequence - 1
                ));
            }
            if let Some(line) = last_line {
                let chars = line.chars().count();
                if chars > LAST_LINE_MAX_CHARS {
                    return Err(format!(
                        "the completion record's last line is {chars} characters; the bound is {LAST_LINE_MAX_CHARS}"
                    ));
                }
                if line.contains(['\n', '\r']) {
                    return Err("the completion record's last line is more than one line".into());
                }
            }
        }
        StreamEvent::AssistantMessageAborted { delta_count, .. } => {
            if *delta_count != sequence - 1 {
                return Err(format!(
                    "the abort record counts {delta_count} deltas; the stream holds {}",
                    sequence - 1
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(id: StreamId, sequence: u64, text: &str) -> StreamEvent {
        StreamEvent::AssistantTextDelta {
            stream_id: id,
            kind: StreamKind::Text,
            sequence,
            run_id: RunId::new(),
            turn_id: TurnId::new(),
            step_id: RunStepId::new(),
            text: text.into(),
            provenance: "MODEL_OUTPUT".into(),
            retention: Retention::CollapsibleOnComplete,
        }
    }

    #[test]
    fn a_stream_is_in_order_bounded_and_closed_once() {
        let id = StreamId::new();
        let agg = *id.as_bytes();
        assert!(check_next(agg, 1, None, &delta(id, 1, "a")).is_ok());
        assert!(check_next(agg, 2, Some(DELTA), &delta(id, 2, "b")).is_ok());
        // out of order, wrong stream, empty and over the bound
        assert!(check_next(agg, 2, Some(DELTA), &delta(id, 3, "b")).is_err());
        assert!(check_next(agg, 1, None, &delta(StreamId::new(), 1, "a")).is_err());
        assert!(check_next(agg, 1, None, &delta(id, 1, "")).is_err());
        let big = "x".repeat(MAX_DELTA_BYTES + 1);
        assert!(check_next(agg, 1, None, &delta(id, 1, &big)).is_err());
        let done = StreamEvent::AssistantMessageCompleted {
            stream_id: id,
            kind: StreamKind::Text,
            delta_count: 2,
            text_ref: TextRef {
                object_hash: "h".into(),
                byte_length: 2,
            },
            content_hash: "h".into(),
            last_line: Some("b".into()),
        };
        assert!(check_next(agg, 3, Some(DELTA), &done).is_ok());
        // a miscounting completion, an early one, and anything after it
        assert!(check_next(agg, 4, Some(DELTA), &done).is_err());
        assert!(check_next(agg, 1, None, &done).is_err());
        assert!(check_next(agg, 4, Some(COMPLETED), &delta(id, 4, "c")).is_err());
        let aborted = StreamEvent::AssistantMessageAborted {
            stream_id: id,
            kind: StreamKind::Text,
            source: AbortSource::Provider,
            code: "STREAM_INTERRUPTED".into(),
            reason: "reset".into(),
            delta_count: 2,
            bytes_streamed: 2,
        };
        assert!(check_next(agg, 3, Some(DELTA), &aborted).is_ok());
        assert!(check_next(agg, 4, Some(ABORTED), &aborted).is_err());
    }

    #[test]
    fn wire_names_are_stable() {
        let id = StreamId::new();
        let v = serde_json::to_value(delta(id, 1, "hi")).unwrap();
        assert_eq!(v["event_type"], DELTA);
        assert_eq!(v["kind"], "TEXT");
        assert_eq!(v["retention"], "COLLAPSIBLE_ON_COMPLETE");
        assert_eq!(AbortSource::Recovery.label(), "RECOVERY");
    }

    /// PX-218: the header's line is the last line with anything on it, whole
    /// when it fits and cut at a character boundary when it does not.
    #[test]
    fn the_last_line_is_the_last_non_empty_one_and_is_cut_at_a_character_boundary() {
        assert_eq!(last_line(""), "");
        assert_eq!(last_line(" \n\t\r\n"), "");
        assert_eq!(last_line("one"), "one");
        assert_eq!(last_line("first\nsecond\n"), "second");
        assert_eq!(last_line("first\r\n  second  \r\n\r\n   \n"), "second");
        // Exactly at the bound: whole, no ellipsis.
        let exact = "é".repeat(LAST_LINE_MAX_CHARS);
        assert_eq!(last_line(&format!("intro\n{exact}")), exact);
        // Past it: every character is two or four bytes, so a cut by bytes
        // would land inside one. The result is a prefix of the line plus the
        // ellipsis and stays within the bound.
        let long = "é😀".repeat(LAST_LINE_MAX_CHARS);
        let cut = last_line(&format!("intro\n{long}\n"));
        assert_eq!(cut.chars().count(), LAST_LINE_MAX_CHARS);
        assert!(cut.ends_with('\u{2026}'));
        let kept = cut.trim_end_matches('\u{2026}');
        assert!(long.starts_with(kept) && kept.chars().count() == LAST_LINE_MAX_CHARS - 1);
        // A cut that lands on a space does not leave it before the ellipsis.
        let spaced = format!("{} {}", "a".repeat(LAST_LINE_MAX_CHARS - 2), "b".repeat(50));
        let cut = last_line(&spaced);
        assert_eq!(
            cut,
            format!("{}\u{2026}", "a".repeat(LAST_LINE_MAX_CHARS - 2))
        );
    }

    /// PX-218: the store refuses a completion record whose last line is over
    /// the bound or is not one line, and reads a record written before the
    /// field existed.
    #[test]
    fn a_completion_records_a_bounded_single_last_line_and_old_records_still_read() {
        let id = StreamId::new();
        let agg = *id.as_bytes();
        let done = |line: Option<String>| StreamEvent::AssistantMessageCompleted {
            stream_id: id,
            kind: StreamKind::Text,
            delta_count: 1,
            text_ref: TextRef {
                object_hash: "h".into(),
                byte_length: 1,
            },
            content_hash: "h".into(),
            last_line: line,
        };
        assert!(check_next(agg, 2, Some(DELTA), &done(None)).is_ok());
        assert!(check_next(agg, 2, Some(DELTA), &done(Some(String::new()))).is_ok());
        let at_bound = "x".repeat(LAST_LINE_MAX_CHARS);
        assert!(check_next(agg, 2, Some(DELTA), &done(Some(at_bound))).is_ok());
        let over = "x".repeat(LAST_LINE_MAX_CHARS + 1);
        assert!(check_next(agg, 2, Some(DELTA), &done(Some(over))).is_err());
        assert!(check_next(agg, 2, Some(DELTA), &done(Some("a\nb".into()))).is_err());
        // The field is absent from an old record and from one that has none.
        let old = serde_json::json!({
            "event_type": COMPLETED, "stream_id": id, "kind": "TEXT", "delta_count": 1,
            "text_ref": {"object_hash": "h", "byte_length": 1}, "content_hash": "h"
        });
        assert_eq!(
            serde_json::from_value::<StreamEvent>(old.clone()).unwrap(),
            done(None)
        );
        assert_eq!(serde_json::to_value(done(None)).unwrap(), old);
        assert_eq!(
            serde_json::to_value(done(Some("bye".into()))).unwrap()["last_line"],
            "bye"
        );
    }
}
