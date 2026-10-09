//! Protected-effect receipt chain (docs/23 "Protected-effect receipt chain"):
//! append-only, each receipt hashes its own fields plus the previous receipt
//! hash, so the chain is independently verifiable from the projection alone.

use modbit_domain::toolcall::EffectReceipt;
use sha2::{Digest, Sha256};

/// `status` of the receipt appended at dispatch, before the effector runs
/// (FIX-08): the call was authorized — intent hash, decision, approval and
/// lease are chained — and its effect has not been reported yet. The receipt
/// appended after execution carries the outcome (`SUCCESS`, `INFRAFAILURE`,
/// ...) and chains after it.
pub const STATUS_AUTHORIZED: &str = "AUTHORIZED";

/// Whether `r` is an authorization receipt (written at dispatch).
#[must_use]
pub fn is_authorization(r: &EffectReceipt) -> bool {
    r.status == STATUS_AUTHORIZED
}

/// The receipt that reports the outcome of `call` (not its authorization).
#[must_use]
pub fn result_of<'a>(
    chain: &'a [EffectReceipt],
    call: &modbit_domain::ToolCallId,
) -> Option<&'a EffectReceipt> {
    chain
        .iter()
        .find(|r| r.tool_call_id == *call && !is_authorization(r))
}

/// The authorization receipt of `call`, if one was written.
#[must_use]
pub fn authorization_of<'a>(
    chain: &'a [EffectReceipt],
    call: &modbit_domain::ToolCallId,
) -> Option<&'a EffectReceipt> {
    chain
        .iter()
        .find(|r| r.tool_call_id == *call && is_authorization(r))
}

/// Effects in doubt: authorized and dispatched, with no result receipt. The
/// process that dispatched them stopped before it could say how they ended;
/// whether the effect happened is unknown, so it is neither assumed done nor
/// retried — it is reconciled against its target (docs/19, docs/23).
#[must_use]
pub fn in_doubt(chain: &[EffectReceipt]) -> Vec<&EffectReceipt> {
    chain
        .iter()
        .filter(|r| is_authorization(r) && result_of(chain, &r.tool_call_id).is_none())
        .collect()
}

/// Canonical hash of a receipt: sha256 over the canonical JSON (sorted keys)
/// of every field except `receipt_hash`.
#[must_use]
pub fn receipt_hash(r: &EffectReceipt) -> String {
    let mut v = serde_json::to_value(r).expect("receipt serializes");
    if let Some(m) = v.as_object_mut() {
        m.remove("receipt_hash");
    }
    let canonical = canonical_json(&v);
    let mut h = Sha256::new();
    h.update(canonical.as_bytes());
    hex::encode(h.finalize())
}

/// Seal a receipt: fill `receipt_hash` from the other fields.
#[must_use]
pub fn seal(mut r: EffectReceipt) -> EffectReceipt {
    r.receipt_hash = receipt_hash(&r);
    r
}

/// Verify a chain in order: every hash recomputes and links to its predecessor.
pub fn verify_chain(chain: &[EffectReceipt]) -> Result<(), String> {
    let mut prev: Option<&str> = None;
    for (i, r) in chain.iter().enumerate() {
        if r.previous_receipt_hash.as_deref() != prev {
            return Err(format!(
                "receipt {i} ({}) links to {:?}, expected {:?}",
                r.effect_id, r.previous_receipt_hash, prev
            ));
        }
        let h = receipt_hash(r);
        if h != r.receipt_hash {
            return Err(format!(
                "receipt {i} ({}) hash {} does not recompute ({h})",
                r.effect_id, r.receipt_hash
            ));
        }
        prev = Some(r.receipt_hash.as_str());
    }
    Ok(())
}

pub(crate) fn canonical_json(v: &serde_json::Value) -> String {
    fn sort(v: &serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Object(m) => serde_json::Value::Object(
                m.iter()
                    .map(|(k, v)| (k.clone(), sort(v)))
                    .collect::<serde_json::Map<_, _>>(),
            ),
            serde_json::Value::Array(a) => serde_json::Value::Array(a.iter().map(sort).collect()),
            other => other.clone(),
        }
    }
    sort(v).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use modbit_domain::{EffectId, TaskId, Timestamp, ToolCallId};

    fn receipt(prev: Option<String>, status: &str) -> EffectReceipt {
        seal(EffectReceipt {
            effect_id: EffectId::new(),
            previous_receipt_hash: prev,
            task_id: TaskId::new(),
            tool_call_id: ToolCallId::new(),
            capability_lease_id: None,
            intent_hash: "h".into(),
            policy_decision: "approval:x".into(),
            approval_id: None,
            execution_target: "local".into(),
            evidence_ref: None,
            status: status.into(),
            occurred_at: Timestamp(1),
            reversibility: None,
            compensates: None,
            authorization: None,
            receipt_hash: String::new(),
        })
    }

    /// REQ-EV-0066: a receipt written before reversibility existed keeps its
    /// hash (the absent fields are not serialized), and on a new receipt the
    /// class and the compensated effect are covered by the hash.
    #[test]
    fn reversibility_and_compensation_are_hashed_and_legacy_receipts_keep_their_hash() {
        use modbit_domain::toolcall::Reversibility;
        let legacy = receipt(None, "SUCCESS");
        let json = serde_json::to_value(&legacy).unwrap();
        assert!(json.get("reversibility").is_none() && json.get("compensates").is_none());
        assert_eq!(receipt_hash(&legacy), legacy.receipt_hash);
        let mut new = seal(EffectReceipt {
            reversibility: Some(Reversibility::Compensatable),
            compensates: Some(EffectId::new()),
            ..receipt(None, "SUCCESS")
        });
        assert!(verify_chain(std::slice::from_ref(&new)).is_ok());
        new.reversibility = Some(Reversibility::Reversible);
        assert!(
            verify_chain(std::slice::from_ref(&new)).is_err(),
            "relabelling an external effect as reversible breaks the chain"
        );
    }

    #[test]
    fn chain_verifies_and_tampering_is_detected() {
        let a = receipt(None, "SUCCESS");
        let b = receipt(Some(a.receipt_hash.clone()), "SUCCESS");
        verify_chain(&[a.clone(), b.clone()]).unwrap();
        let mut forged = b.clone();
        forged.status = "FAILED".into();
        assert!(verify_chain(&[a.clone(), forged]).is_err());
        assert!(verify_chain(&[b]).is_err(), "missing predecessor");
    }

    #[test]
    fn an_authorization_without_a_result_is_in_doubt_until_the_result_chains_after_it() {
        let auth = receipt(None, STATUS_AUTHORIZED);
        let call = auth.tool_call_id;
        let chain = vec![auth.clone()];
        assert!(verify_chain(&chain).is_ok());
        assert_eq!(in_doubt(&chain), vec![&auth]);
        assert!(result_of(&chain, &call).is_none());
        // A different call's result does not settle it.
        let other = receipt(Some(auth.receipt_hash.clone()), "SUCCESS");
        let chain = vec![auth.clone(), other];
        assert_eq!(in_doubt(&chain), vec![&auth]);
        // Its own result chains after it and settles it.
        let result = seal(EffectReceipt {
            tool_call_id: call,
            ..receipt(Some(auth.receipt_hash.clone()), "SUCCESS")
        });
        let chain = vec![auth.clone(), result.clone()];
        verify_chain(&chain).unwrap();
        assert!(in_doubt(&chain).is_empty());
        assert_eq!(result_of(&chain, &call), Some(&result));
        assert_eq!(authorization_of(&chain, &call), Some(&auth));
    }
}
