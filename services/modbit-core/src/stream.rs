//! Assistant streaming (PX-041; docs/65 AFW-C02, AFW-K01): what the model
//! says reaches every client while it is being said, from the same canonical
//! log as everything else.
//!
//! For each model invocation the runtime opens a [`Sink`]. Text the provider
//! streams goes through it:
//!
//! * **coalesced** — a lane publishes at most one delta per
//!   [`MIN_DELTA_INTERVAL_MS`] and never more than [`MAX_DELTA_BYTES`] per
//!   event, so a token-per-frame provider does not flood the log;
//! * **redacted before it is appended** — what the Core holds and every
//!   credential shape is replaced, and a tail that may still grow into a
//!   credential is held back until it is known, so a secret split across two
//!   chunks never leaks in halves;
//! * **provenance-tagged** — it is model output, data, never an instruction;
//! * **closed exactly once** — `AssistantMessageCompleted` with the full
//!   redacted text as an object and its digest, or `AssistantMessageAborted`
//!   with a typed source. A stream that fails, is cancelled or loses its
//!   provider is never presented as a finished message, and a Core restart
//!   closes any stream it finds open as aborted by recovery
//!   ([`close_after_restart`]); nothing resumes mid-token.
//!
//! Memory is bounded: a lane holds at most one pending chunk and the redacted
//! text of a stream that may not exceed [`MAX_STREAM_BYTES`].

use std::time::Duration;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::stream::{
    self, AbortSource, MAX_DELTA_BYTES, MAX_STREAM_BYTES, MIN_DELTA_INTERVAL_MS, Retention,
    StreamEvent, StreamKind, TextRef,
};
use modbit_domain::{RunId, RunStepId, StreamId, TenantId, TurnId};
use modbit_event_store::{AppendRequest, EventStore};
use sha2::Digest;
use tokio::time::Instant;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// Provenance every delta carries: the model said it; it is data.
const PROVENANCE: &str = "MODEL_OUTPUT";

/// Why a stream cannot take more text.
pub(crate) const TOO_LARGE: &str = "STREAM_TOO_LARGE";

/// Whether reasoning summaries are published at all (policy, off unless the
/// operator turns it on: a provider's reasoning text is not ours to show by
/// default).
fn reasoning_allowed() -> bool {
    std::env::var("MODBIT_STREAM_REASONING")
        .is_ok_and(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "on"))
}

/// One stream of one kind inside an invocation.
struct Lane {
    kind: StreamKind,
    id: StreamId,
    /// Raw text received and not yet published.
    pending: String,
    /// Raw bytes received in all.
    received: u64,
    /// Redacted text published so far (the completed record's body).
    published: String,
    deltas: u64,
    last_flush: Option<Instant>,
    closed: bool,
    /// An append failed: the lane stops trying and its stream is never
    /// closed as completed.
    broken: bool,
}

impl Lane {
    fn new(kind: StreamKind) -> Self {
        Self {
            kind,
            id: StreamId::new(),
            pending: String::new(),
            received: 0,
            published: String::new(),
            deltas: 0,
            last_flush: None,
            closed: false,
            broken: false,
        }
    }

    fn deadline(&self) -> Option<Instant> {
        if self.pending.is_empty() || self.closed || self.broken {
            return None;
        }
        let now = Instant::now();
        // A burst past the bound goes out at once (the held-back tail never
        // makes this spin: it is far smaller than the margin).
        if self.pending.len() >= MAX_DELTA_BYTES + modbit_secrets::redact::TAIL_WINDOW {
            return Some(now);
        }
        match self.last_flush {
            None => Some(now),
            Some(t) => Some(t + Duration::from_millis(MIN_DELTA_INTERVAL_MS)),
        }
    }
}

/// What a stream ended as, once it completed.
pub(crate) struct Completed {
    /// The full redacted text.
    pub text: String,
}

/// The streams of one model invocation.
pub(crate) struct Sink {
    lineage: Lineage,
    actor: Actor,
    run: RunId,
    turn: TurnId,
    step: RunStepId,
    text: Lane,
    reasoning: Option<Lane>,
}

impl Sink {
    /// A sink for the invocation `step` of `turn`; `lineage` is the turn's
    /// (fenced) lineage.
    pub(crate) fn new(
        lineage: Lineage,
        actor: Actor,
        run: RunId,
        turn: TurnId,
        step: RunStepId,
    ) -> Self {
        Self {
            run,
            turn,
            lineage: lineage.with_step(step),
            actor,
            step,
            text: Lane::new(StreamKind::Text),
            reasoning: reasoning_allowed().then(|| Lane::new(StreamKind::Reasoning)),
        }
    }

    /// Text the model streamed. `Err` when the stream has outgrown its bound
    /// (the caller stops the provider and fails the invocation).
    pub(crate) fn text(&mut self, chunk: &str) -> Result<(), &'static str> {
        push(&mut self.text, chunk)
    }

    /// A reasoning summary the provider streamed; dropped unless policy
    /// allows it.
    pub(crate) fn reasoning(&mut self, chunk: &str) -> Result<(), &'static str> {
        match &mut self.reasoning {
            Some(lane) => push(lane, chunk),
            None => Ok(()),
        }
    }

    /// When the next coalesced delta is due, if any text is waiting.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        [Some(&self.text), self.reasoning.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(Lane::deadline)
            .min()
    }

    /// Publish what is due now.
    pub(crate) async fn flush(&mut self, core: &Core) {
        let now = Instant::now();
        for lane in lanes(&mut self.text, &mut self.reasoning) {
            if lane.deadline().is_some_and(|d| d <= now) {
                publish(
                    &self.lineage,
                    &self.actor,
                    self.run,
                    self.turn,
                    self.step,
                    core,
                    lane,
                    false,
                )
                .await;
            }
        }
    }

    /// The invocation finished: publish the rest and close the text stream
    /// as completed. `None` when there was no text (nothing was opened) or
    /// the stream could not be recorded intact; a reasoning stream, if any,
    /// closes alongside.
    pub(crate) async fn complete(&mut self, core: &Core) -> Option<Completed> {
        let mut out = None;
        for lane in lanes(&mut self.text, &mut self.reasoning) {
            publish(
                &self.lineage,
                &self.actor,
                self.run,
                self.turn,
                self.step,
                core,
                lane,
                true,
            )
            .await;
            if lane.deltas == 0 || lane.closed || lane.broken {
                continue;
            }
            let bytes = lane.published.as_bytes();
            let hash = {
                let store = core.store.lock().await;
                store.objects().put(bytes).ok()
            };
            let Some(object_hash) = hash else {
                lane.broken = true;
                continue;
            };
            let event = StreamEvent::AssistantMessageCompleted {
                stream_id: lane.id,
                kind: lane.kind,
                delta_count: lane.deltas,
                text_ref: TextRef {
                    object_hash,
                    byte_length: bytes.len() as u64,
                },
                content_hash: hex::encode(sha2::Sha256::digest(bytes)),
            };
            if close(&self.lineage, &self.actor, core, lane, &event).await
                && lane.kind == StreamKind::Text
            {
                out = Some(Completed {
                    text: std::mem::take(&mut lane.published),
                });
            }
        }
        out
    }

    /// The invocation ended without a message: publish what was safely
    /// received and close each open stream as aborted. Partial text stays
    /// partial; nothing here marks it complete.
    pub(crate) async fn abort(
        &mut self,
        core: &Core,
        source: AbortSource,
        code: &str,
        reason: &str,
    ) {
        for lane in lanes(&mut self.text, &mut self.reasoning) {
            publish(
                &self.lineage,
                &self.actor,
                self.run,
                self.turn,
                self.step,
                core,
                lane,
                true,
            )
            .await;
            if lane.deltas == 0 || lane.closed {
                continue;
            }
            let event = StreamEvent::AssistantMessageAborted {
                stream_id: lane.id,
                kind: lane.kind,
                source,
                code: code.to_owned(),
                reason: reason.to_owned(),
                delta_count: lane.deltas,
                bytes_streamed: lane.published.len() as u64,
            };
            close(&self.lineage, &self.actor, core, lane, &event).await;
        }
    }
}

fn lanes<'a>(
    text: &'a mut Lane,
    reasoning: &'a mut Option<Lane>,
) -> impl Iterator<Item = &'a mut Lane> {
    std::iter::once(text).chain(reasoning.as_mut())
}

fn push(lane: &mut Lane, chunk: &str) -> Result<(), &'static str> {
    lane.received += chunk.len() as u64;
    if lane.received > MAX_STREAM_BYTES {
        return Err(TOO_LARGE);
    }
    lane.pending.push_str(chunk);
    Ok(())
}

/// Publish the redacted, safe-to-release part of a lane's pending text as
/// deltas of at most [`MAX_DELTA_BYTES`] each.
#[allow(clippy::too_many_arguments)]
async fn publish(
    lineage: &Lineage,
    actor: &Actor,
    run: RunId,
    turn: TurnId,
    step: RunStepId,
    core: &Core,
    lane: &mut Lane,
    last: bool,
) {
    if lane.closed || lane.broken {
        return;
    }
    let redactor = core.tools.redactor();
    let held = if last {
        0
    } else {
        redactor.pending_tail(&lane.pending)
    };
    let cut = lane.pending.len() - held;
    lane.last_flush = Some(Instant::now());
    if cut == 0 {
        return;
    }
    let raw: String = lane.pending.drain(..cut).collect();
    // Redacted before anything is appended: held values and credential
    // shapes alike (model text is not data a tool read; a credential in it
    // does not belong on the log).
    let safe = redactor.error(&raw).text;
    let mut events = Vec::new();
    let mut sequence = lane.deltas;
    let mut rest = safe.as_str();
    while !rest.is_empty() {
        let mut end = rest.len().min(MAX_DELTA_BYTES);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (piece, tail) = rest.split_at(end);
        sequence += 1;
        events.push(typed(
            stream::DELTA,
            &StreamEvent::AssistantTextDelta {
                stream_id: lane.id,
                kind: lane.kind,
                sequence,
                run_id: run,
                turn_id: turn,
                step_id: step,
                text: piece.to_owned(),
                provenance: PROVENANCE.to_owned(),
                retention: Retention::CollapsibleOnComplete,
            },
            actor.clone(),
        ));
        rest = tail;
    }
    if events.is_empty() {
        return;
    }
    let appended = {
        let mut store = core.store.lock().await;
        append(
            &mut store,
            core,
            *lineage,
            AggregateType::AssistantStream,
            *lane.id.as_bytes(),
            events,
        )
    };
    match appended {
        Ok(_) => {
            lane.deltas = sequence;
            lane.published.push_str(&safe);
        }
        Err(_) => lane.broken = true,
    }
}

async fn close(
    lineage: &Lineage,
    actor: &Actor,
    core: &Core,
    lane: &mut Lane,
    event: &StreamEvent,
) -> bool {
    let appended = {
        let mut store = core.store.lock().await;
        append(
            &mut store,
            core,
            *lineage,
            AggregateType::AssistantStream,
            *lane.id.as_bytes(),
            vec![typed(event.event_type(), event, actor.clone())],
        )
    };
    lane.closed = appended.is_ok();
    appended.is_ok()
}

/// A Core start found these streams open: their process died mid-stream. Each
/// closes as aborted by recovery; the loop re-requests under the existing
/// turn semantics and no stream resumes mid-token (PX-041). Returns how many
/// were closed.
pub(crate) fn close_after_restart(store: &mut EventStore, tenant: TenantId) -> usize {
    let actor = Actor::Core("recovery".into());
    let Ok(open) = store.open_streams() else {
        return 0;
    };
    let mut closed = 0;
    for aggregate in open {
        let Ok(events) = store.read_aggregate(&aggregate, 0, usize::MAX) else {
            continue;
        };
        let Some(last) = events.last() else {
            continue;
        };
        let mut bytes = 0u64;
        let mut kind = StreamKind::Text;
        for e in &events {
            if let Ok(p) = store.payload(&e.envelope)
                && let Ok(StreamEvent::AssistantTextDelta { text, kind: k, .. }) =
                    serde_json::from_value::<StreamEvent>(p)
            {
                bytes += text.len() as u64;
                kind = k;
            }
        }
        let env = &last.envelope;
        let event = StreamEvent::AssistantMessageAborted {
            stream_id: StreamId::from_bytes(aggregate),
            kind,
            source: AbortSource::Recovery,
            code: "ABORTED_BY_RECOVERY".into(),
            reason: "the Core stopped while the message was streaming; the partial text is not a message"
                .into(),
            delta_count: env.sequence,
            bytes_streamed: bytes,
        };
        let appended = store.append(AppendRequest {
            tenant_id: tenant,
            session_id: env.session_id,
            task_id: env.task_id,
            run_id: env.run_id,
            turn_id: env.turn_id,
            step_id: env.step_id,
            aggregate_type: AggregateType::AssistantStream,
            aggregate_id: aggregate,
            expected_sequence: Some(env.sequence),
            events: vec![typed(event.event_type(), &event, actor.clone())],
        });
        if appended.is_ok() {
            closed += 1;
        }
    }
    closed
}
