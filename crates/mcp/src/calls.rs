//! The in-flight call table (docs/16: "Cancellation and unknown-outcome
//! reconciliation are mandatory for effectful calls"; docs/54 fault 25: an
//! external server disconnects during a call).
//!
//! An MCP call has two identities: the tool call the host issued
//! (`call_id`, which the model, the log and the evidence all name) and the
//! JSON-RPC request id on the wire. Cancellation needs the second, audit
//! needs the first, and reconciliation needs both plus one fact the table
//! keeps: whether the call could have had an effect.
//!
//! The rule the table encodes: **a read that ends without an answer failed;
//! an effectful call that ends without an answer has an unknown outcome**.
//! The host never guesses that an effect did not happen.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Where a call is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CallState {
    /// Sent; no answer yet.
    InFlight,
    /// A cancellation was sent; the server may still answer.
    Cancelling,
}

/// One call the host is waiting on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InFlight {
    /// The host's tool call id.
    pub call_id: String,
    /// Server name.
    pub server: String,
    /// Tool name on that server.
    pub tool: String,
    /// JSON-RPC request id on the wire.
    pub request_id: u64,
    /// Whether the call could have an effect outside the host.
    pub effectful: bool,
    /// State.
    pub state: CallState,
    /// Why it is being cancelled, when it is.
    pub cancel_reason: Option<String>,
}

/// How a call ended once no answer will come.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciled {
    /// The call.
    pub call_id: String,
    /// Stable code.
    pub code: String,
    /// What the host can say about the effect.
    pub message: String,
    /// True when the effect may have happened and the host cannot tell.
    pub unknown_outcome: bool,
}

/// What cancelling a call asks the host to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CancelPlan {
    /// Send `notifications/cancelled` for this request id, then wait for
    /// the server's answer (which may still arrive).
    Notify {
        /// Wire id to name in the notification.
        request_id: u64,
        /// Whether the call could have had an effect.
        effectful: bool,
    },
    /// The call is not in flight: it finished, or it was never issued.
    NotInFlight,
    /// A cancellation was already sent for it.
    AlreadyCancelling {
        /// Wire id.
        request_id: u64,
    },
}

/// The calls one transport is serving.
#[derive(Clone, Debug, Default)]
pub struct CallTable {
    by_request: BTreeMap<u64, InFlight>,
}

impl CallTable {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a call the host just sent.
    pub fn begin(
        &mut self,
        call_id: &str,
        server: &str,
        tool: &str,
        request_id: u64,
        effectful: bool,
    ) -> InFlight {
        let f = InFlight {
            call_id: call_id.to_owned(),
            server: server.to_owned(),
            tool: tool.to_owned(),
            request_id,
            effectful,
            state: CallState::InFlight,
            cancel_reason: None,
        };
        self.by_request.insert(request_id, f.clone());
        f
    }

    /// The server answered: the call leaves the table. Returns what it was,
    /// and whether the answer arrived after a cancellation was sent (which
    /// the host reports, because the person was told it was cancelled).
    pub fn complete(&mut self, request_id: u64) -> Option<(InFlight, bool)> {
        let f = self.by_request.remove(&request_id)?;
        let raced = f.state == CallState::Cancelling;
        Some((f, raced))
    }

    /// Ask to cancel the call the host issued as `call_id`.
    pub fn cancel(&mut self, call_id: &str, reason: &str) -> CancelPlan {
        let Some(f) = self.by_request.values_mut().find(|f| f.call_id == call_id) else {
            return CancelPlan::NotInFlight;
        };
        if f.state == CallState::Cancelling {
            return CancelPlan::AlreadyCancelling {
                request_id: f.request_id,
            };
        }
        f.state = CallState::Cancelling;
        f.cancel_reason = Some(reason.to_owned());
        CancelPlan::Notify {
            request_id: f.request_id,
            effectful: f.effectful,
        }
    }

    /// The transport died. Every call still in the table ends now: a read
    /// failed, an effectful call has an unknown outcome the host must
    /// reconcile rather than assume away.
    pub fn transport_lost(&mut self, detail: &str) -> Vec<Reconciled> {
        let lost: Vec<InFlight> = std::mem::take(&mut self.by_request).into_values().collect();
        lost.into_iter()
            .map(|f| {
                if f.effectful {
                    Reconciled {
                        code: "EXTERNAL_OUTCOME_UNKNOWN".into(),
                        message: format!(
                            "`{}.{}` was in flight when the server was lost ({detail}); the effect may have happened",
                            f.server, f.tool
                        ),
                        call_id: f.call_id,
                        unknown_outcome: true,
                    }
                } else {
                    Reconciled {
                        code: "EXTERNAL_TRANSPORT_LOST".into(),
                        message: format!(
                            "`{}.{}` was reading when the server was lost ({detail}); nothing was changed",
                            f.server, f.tool
                        ),
                        call_id: f.call_id,
                        unknown_outcome: false,
                    }
                }
            })
            .collect()
    }

    /// How a cancelled effectful call is reported when the server never
    /// answers: the host says plainly that it cannot tell.
    #[must_use]
    pub fn cancelled_outcome(call: &InFlight) -> Reconciled {
        if call.effectful {
            Reconciled {
                code: "EXTERNAL_OUTCOME_UNKNOWN".into(),
                message: format!(
                    "`{}.{}` was cancelled after it was sent; the effect may have happened",
                    call.server, call.tool
                ),
                call_id: call.call_id.clone(),
                unknown_outcome: true,
            }
        } else {
            Reconciled {
                code: "EXTERNAL_CALL_CANCELLED".into(),
                message: format!("`{}.{}` was cancelled", call.server, call.tool),
                call_id: call.call_id.clone(),
                unknown_outcome: false,
            }
        }
    }

    /// Calls still waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_request.len()
    }

    /// Whether nothing is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_request.is_empty()
    }

    /// The call the host issued as `call_id`, when it is still waiting.
    #[must_use]
    pub fn get(&self, call_id: &str) -> Option<&InFlight> {
        self.by_request.values().find(|f| f.call_id == call_id)
    }

    /// Every call still waiting, in wire-id order — what a host resolving a
    /// dead transport's waiters walks.
    #[must_use]
    pub fn in_flight(&self) -> Vec<InFlight> {
        self.by_request.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_is_tracked_by_both_identities_and_leaves_on_an_answer() {
        let mut t = CallTable::new();
        t.begin("call-1", "docs", "search", 11, false);
        assert_eq!(t.len(), 1);
        assert_eq!(t.get("call-1").expect("tracked").request_id, 11);
        let (f, raced) = t.complete(11).expect("completed");
        assert_eq!(f.call_id, "call-1");
        assert!(!raced);
        assert!(t.is_empty());
        assert!(t.complete(11).is_none(), "an answer to nothing is nothing");
    }

    #[test]
    fn cancelling_names_the_wire_id_and_is_idempotent() {
        let mut t = CallTable::new();
        t.begin("call-1", "docs", "deploy", 7, true);
        assert_eq!(
            t.cancel("call-1", "user cancelled"),
            CancelPlan::Notify {
                request_id: 7,
                effectful: true
            }
        );
        assert_eq!(
            t.cancel("call-1", "again"),
            CancelPlan::AlreadyCancelling { request_id: 7 }
        );
        assert_eq!(t.cancel("call-unknown", "x"), CancelPlan::NotInFlight);
        assert_eq!(
            t.get("call-1")
                .expect("still tracked")
                .cancel_reason
                .as_deref(),
            Some("user cancelled")
        );
    }

    #[test]
    fn an_answer_that_races_a_cancellation_is_reported_as_a_race() {
        let mut t = CallTable::new();
        t.begin("call-1", "docs", "deploy", 7, true);
        t.cancel("call-1", "user cancelled");
        let (_, raced) = t.complete(7).expect("the server answered anyway");
        assert!(
            raced,
            "the host must say the answer arrived after the cancellation"
        );
    }

    #[test]
    fn a_lost_transport_never_claims_an_effect_did_not_happen() {
        let mut t = CallTable::new();
        t.begin("read", "docs", "search", 1, false);
        t.begin("write", "docs", "deploy", 2, true);
        let mut out = t.transport_lost("server exited");
        out.sort_by(|a, b| a.call_id.cmp(&b.call_id));
        assert_eq!(out.len(), 2);
        assert!(t.is_empty());

        let read = out.iter().find(|r| r.call_id == "read").expect("read");
        assert_eq!(read.code, "EXTERNAL_TRANSPORT_LOST");
        assert!(!read.unknown_outcome);

        let write = out.iter().find(|r| r.call_id == "write").expect("write");
        assert_eq!(write.code, "EXTERNAL_OUTCOME_UNKNOWN");
        assert!(
            write.unknown_outcome,
            "an effect in flight is never assumed away"
        );
        assert!(write.message.contains("may have happened"));
    }

    #[test]
    fn a_cancelled_effectful_call_has_an_unknown_outcome_and_a_read_does_not() {
        let mut t = CallTable::new();
        let write = t.begin("w", "docs", "deploy", 1, true);
        let read = t.begin("r", "docs", "search", 2, false);
        assert!(CallTable::cancelled_outcome(&write).unknown_outcome);
        assert_eq!(
            CallTable::cancelled_outcome(&read).code,
            "EXTERNAL_CALL_CANCELLED"
        );
    }
}
