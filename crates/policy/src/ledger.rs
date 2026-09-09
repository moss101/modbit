//! Protected-effect receipt chain (docs/23 "Protected-effect receipt chain"):
//! append-only, each receipt hashes its own fields plus the previous receipt
//! hash, so the chain is independently verifiable from the projection alone.

use modbit_domain::toolcall::EffectReceipt;
use sha2::{Digest, Sha256};

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

fn canonical_json(v: &serde_json::Value) -> String {
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
            receipt_hash: String::new(),
        })
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
}
