//! `AuthorizationEpoch` and `CapabilitySnapshot` (REQ-PX-131, docs/23
//! "Policy generations"): the digest of a frozen capability view, and the
//! checks that make the epoch on a decision, a receipt and a recorded
//! snapshot agree.
//!
//! The Core freezes a snapshot at each model-round boundary and records it
//! (`TaskEvent::CapabilitySnapshotRecorded`); every kernel decision and
//! receipt of the round carries `{epoch, snapshot_hash}`. A receipt is
//! covered by the receipt chain's hash, so editing its epoch breaks the
//! chain; a receipt re-sealed with another epoch is caught here, because the
//! decision it executes under, written write-ahead at dispatch, names the
//! epoch it was made in.

use modbit_domain::epoch::{AuthorizationStamp, CapabilitySnapshot};
use modbit_domain::toolcall::EffectReceipt;
use modbit_domain::{TaskId, ToolCallId};
use sha2::{Digest, Sha256};

/// SHA-256 (hex) over the canonical JSON of a snapshot.
#[must_use]
pub fn snapshot_hash(s: &CapabilitySnapshot) -> String {
    let v = serde_json::to_value(s).expect("snapshot serializes");
    hex::encode(Sha256::digest(crate::ledger::canonical_json(&v).as_bytes()))
}

/// The stamp a round's decisions and receipts carry.
#[must_use]
pub fn stamp(s: &CapabilitySnapshot) -> AuthorizationStamp {
    AuthorizationStamp {
        epoch: s.epoch,
        snapshot_hash: snapshot_hash(s),
    }
}

/// Verify one task's recorded snapshots, in log order: each hash recomputes
/// from the snapshot and the epochs are 1, 2, 3 ... without a gap, a repeat
/// or a step back. A restarted Core that continued the sequence from the log
/// passes; one that re-numbered, or a snapshot edited after it was recorded,
/// does not.
///
/// # Errors
/// The first record that breaks either rule.
pub fn verify_snapshots(records: &[(CapabilitySnapshot, String)]) -> Result<(), String> {
    for (expected, (snap, recorded)) in (1u64..).zip(records) {
        let h = snapshot_hash(snap);
        if &h != recorded {
            return Err(format!(
                "snapshot of epoch {} records hash {recorded}, which does not recompute ({h})",
                snap.epoch
            ));
        }
        if snap.epoch != expected {
            return Err(format!(
                "epoch {} follows epoch {}: the sequence must be monotonic without gaps",
                snap.epoch,
                expected - 1
            ));
        }
    }
    Ok(())
}

/// Verify that every receipt names the epoch of the decision it was made
/// under, and that the epoch's snapshot is the one recorded.
///
/// `decision_of` answers the stamp of a call's policy decision (`None` = the
/// decision carries no stamp, written before epochs); `snapshot_hash_of`
/// answers the recorded hash of a task's snapshot at an epoch.
///
/// A receipt with no stamp is accepted only when its decision has none
/// either; a stamp stripped from a receipt whose decision has one is
/// refused.
///
/// # Errors
/// The first receipt whose epoch differs from its decision's, whose
/// snapshot hash does not match the record, or whose epoch has no record.
pub fn verify_receipt_epochs(
    chain: &[EffectReceipt],
    decision_of: impl Fn(&ToolCallId) -> Option<AuthorizationStamp>,
    snapshot_hash_of: impl Fn(&TaskId, u64) -> Option<String>,
) -> Result<(), String> {
    for r in chain {
        let decided = decision_of(&r.tool_call_id);
        match (&r.authorization, &decided) {
            (None, None) => {}
            (None, Some(d)) => {
                return Err(format!(
                    "receipt {} ({}) carries no epoch, but its decision was made in epoch {}",
                    r.effect_id, r.status, d.epoch
                ));
            }
            (Some(a), None) => {
                return Err(format!(
                    "receipt {} ({}) claims epoch {}, but its decision carries none",
                    r.effect_id, r.status, a.epoch
                ));
            }
            (Some(a), Some(d)) => {
                if a != d {
                    return Err(format!(
                        "receipt {} ({}) names epoch {} ({}), its decision was made in epoch {} ({})",
                        r.effect_id,
                        r.status,
                        a.epoch,
                        short(&a.snapshot_hash),
                        d.epoch,
                        short(&d.snapshot_hash)
                    ));
                }
                match snapshot_hash_of(&r.task_id, a.epoch) {
                    Some(h) if h == a.snapshot_hash => {}
                    Some(h) => {
                        return Err(format!(
                            "receipt {} names snapshot {} for epoch {}, the recorded snapshot is {}",
                            r.effect_id,
                            short(&a.snapshot_hash),
                            a.epoch,
                            short(&h)
                        ));
                    }
                    None => {
                        return Err(format!(
                            "receipt {} names epoch {}, which has no recorded snapshot",
                            r.effect_id, a.epoch
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn short(h: &str) -> &str {
    &h[..h.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::seal;
    use modbit_domain::epoch::LeaseSnapshot;
    use modbit_domain::{EffectId, Timestamp};
    use std::collections::HashMap;

    fn snap(epoch: u64) -> CapabilitySnapshot {
        CapabilitySnapshot {
            epoch,
            run_id: "run".into(),
            turn_id: format!("turn-{epoch}"),
            round: u32::try_from(epoch).unwrap(),
            config_generation: "g".into(),
            config_ref: "c".into(),
            mode: "AGENT".into(),
            execution_profile: "local_trusted".into(),
            lease: Some(LeaseSnapshot {
                lease_id: "l".into(),
                generation: 1,
                effect_ceiling: "ProtectedWrite".into(),
                operations: vec!["fs.read".into()],
                resources_digest: "r".into(),
            }),
            projected_tools: vec!["fs.read".into()],
            projection_hash: "p".into(),
            skills: vec![],
        }
    }

    fn receipt(task: TaskId, call: ToolCallId, a: Option<AuthorizationStamp>) -> EffectReceipt {
        seal(EffectReceipt {
            effect_id: EffectId::new(),
            previous_receipt_hash: None,
            task_id: task,
            tool_call_id: call,
            capability_lease_id: None,
            intent_hash: "h".into(),
            policy_decision: "allow".into(),
            approval_id: None,
            execution_target: "local".into(),
            evidence_ref: None,
            status: "SUCCESS".into(),
            occurred_at: Timestamp(1),
            reversibility: None,
            compensates: None,
            authorization: a,
            receipt_hash: String::new(),
        })
    }

    #[test]
    fn the_hash_covers_every_field_of_the_snapshot() {
        let a = snap(1);
        let base = snapshot_hash(&a);
        assert_eq!(base, snapshot_hash(&a.clone()));
        let mut b = a.clone();
        b.projected_tools.push("shell.exec".into());
        assert_ne!(base, snapshot_hash(&b));
        let mut c = a.clone();
        c.config_generation = "other".into();
        assert_ne!(base, snapshot_hash(&c));
        let mut d = a.clone();
        d.lease
            .as_mut()
            .unwrap()
            .operations
            .push("shell.exec".into());
        assert_ne!(base, snapshot_hash(&d));
        let mut e = a;
        e.mode = "READ_ONLY".into();
        assert_ne!(base, snapshot_hash(&e));
    }

    #[test]
    fn snapshots_verify_only_as_a_gapless_monotonic_sequence_with_true_hashes() {
        let rec = |s: CapabilitySnapshot| {
            let h = snapshot_hash(&s);
            (s, h)
        };
        assert!(verify_snapshots(&[rec(snap(1)), rec(snap(2)), rec(snap(3))]).is_ok());
        assert!(
            verify_snapshots(&[rec(snap(1)), rec(snap(3))]).is_err(),
            "gap"
        );
        assert!(
            verify_snapshots(&[rec(snap(1)), rec(snap(1))]).is_err(),
            "repeat"
        );
        assert!(verify_snapshots(&[rec(snap(2))]).is_err(), "not from 1");
        let (mut s, h) = rec(snap(1));
        s.mode = "AGENT2".into();
        assert!(
            verify_snapshots(&[(s, h)]).is_err(),
            "edited after recorded"
        );
    }

    #[test]
    fn a_receipt_whose_epoch_differs_from_its_decision_is_refused_even_when_resealed() {
        let task = TaskId::new();
        let (s1, s2) = (snap(1), snap(2));
        let (a1, a2) = (stamp(&s1), stamp(&s2));
        let recorded: HashMap<(TaskId, u64), String> = [
            ((task, 1), a1.snapshot_hash.clone()),
            ((task, 2), a2.snapshot_hash.clone()),
        ]
        .into();
        let call = ToolCallId::new();
        let decisions: HashMap<ToolCallId, AuthorizationStamp> = [(call, a1.clone())].into();
        let decision_of = |c: &ToolCallId| decisions.get(c).cloned();
        let snapshot_of = |t: &TaskId, e: u64| recorded.get(&(*t, e)).cloned();
        // The honest receipt.
        let honest = receipt(task, call, Some(a1.clone()));
        assert!(verify_receipt_epochs(&[honest], decision_of, snapshot_of).is_ok());
        // A receipt forged into another epoch, validly sealed: the chain hash
        // verifies, the epoch check does not.
        let forged = receipt(task, call, Some(a2));
        assert!(crate::ledger::verify_chain(std::slice::from_ref(&forged)).is_ok());
        let e = verify_receipt_epochs(&[forged], decision_of, snapshot_of).unwrap_err();
        assert!(e.contains("epoch 2") && e.contains("epoch 1"), "{e}");
        // A stripped stamp.
        let stripped = receipt(task, call, None);
        assert!(verify_receipt_epochs(&[stripped], decision_of, snapshot_of).is_err());
        // A right epoch with another snapshot's hash.
        let wrong = AuthorizationStamp {
            epoch: 1,
            snapshot_hash: "0".repeat(64),
        };
        let wrong_hash = receipt(task, call, Some(wrong.clone()));
        let decisions2: HashMap<ToolCallId, AuthorizationStamp> = [(call, wrong)].into();
        let e = verify_receipt_epochs(
            &[wrong_hash],
            |c: &ToolCallId| decisions2.get(c).cloned(),
            snapshot_of,
        )
        .unwrap_err();
        assert!(e.contains("recorded snapshot"), "{e}");
        // A receipt claiming an epoch its decision lacks.
        let claims = receipt(task, ToolCallId::new(), Some(a1));
        assert!(verify_receipt_epochs(&[claims], decision_of, snapshot_of).is_err());
        // Legacy: neither carries a stamp.
        let legacy = receipt(task, ToolCallId::new(), None);
        assert!(verify_receipt_epochs(&[legacy], decision_of, snapshot_of).is_ok());
    }
}
