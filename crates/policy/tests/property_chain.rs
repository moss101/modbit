//! M9.6 (docs/52 "Security gates": property tests for the effect receipt
//! chain). For arbitrary chains the verifier accepts the untampered chain
//! and rejects every single tamper, reorder and interior deletion; and, per
//! docs/55, the two checks the verifier makes are shown to be load-bearing
//! by verifiers with one check removed accepting what the real one rejects.

use modbit_domain::toolcall::EffectReceipt;
use modbit_domain::{ApprovalId, CapabilityLeaseId, EffectId, TaskId, Timestamp, ToolCallId};
use modbit_policy::ledger::{receipt_hash, seal, verify_chain};
use proptest::prelude::*;

#[derive(Clone, Debug)]
struct Fields {
    intent: String,
    decision: String,
    target: String,
    status: String,
    evidence: Option<String>,
    lease: bool,
    approval: bool,
    at: i64,
}

fn fields() -> impl Strategy<Value = Fields> {
    (
        "[a-f0-9]{0,64}",
        prop::sample::select(vec!["approval:x", "allow", "deny", "lease:1"])
            .prop_map(str::to_owned),
        prop::sample::select(vec!["local", "sandbox:1", "gateway", ""]).prop_map(str::to_owned),
        prop::sample::select(vec!["committed", "failed", "unknown", "cancelled"])
            .prop_map(str::to_owned),
        prop::option::of("[a-z0-9:/._-]{0,40}"),
        any::<bool>(),
        any::<bool>(),
        0i64..1_000_000,
    )
        .prop_map(
            |(intent, decision, target, status, evidence, lease, approval, at)| Fields {
                intent,
                decision,
                target,
                status,
                evidence,
                lease,
                approval,
                at,
            },
        )
}

fn chain_of(fs: &[Fields]) -> Vec<EffectReceipt> {
    let mut prev: Option<String> = None;
    let mut out = Vec::new();
    for f in fs {
        let r = seal(EffectReceipt {
            effect_id: EffectId::new(),
            previous_receipt_hash: prev.clone(),
            task_id: TaskId::new(),
            tool_call_id: ToolCallId::new(),
            capability_lease_id: f.lease.then(CapabilityLeaseId::new),
            intent_hash: f.intent.clone(),
            policy_decision: f.decision.clone(),
            approval_id: f.approval.then(ApprovalId::new),
            execution_target: f.target.clone(),
            evidence_ref: f.evidence.clone(),
            status: f.status.clone(),
            occurred_at: Timestamp(f.at),
            receipt_hash: String::new(),
        });
        prev = Some(r.receipt_hash.clone());
        out.push(r);
    }
    out
}

/// A verifier that only checks the links (docs/55: the hash check removed).
fn verify_links_only(chain: &[EffectReceipt]) -> bool {
    let mut prev: Option<&str> = None;
    for r in chain {
        if r.previous_receipt_hash.as_deref() != prev {
            return false;
        }
        prev = Some(r.receipt_hash.as_str());
    }
    true
}

/// A verifier that only recomputes each hash (docs/55: the link check removed).
fn verify_hashes_only(chain: &[EffectReceipt]) -> bool {
    chain.iter().all(|r| receipt_hash(r) == r.receipt_hash)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn an_untampered_chain_verifies_and_every_tamper_is_detected(
        fs in prop::collection::vec(fields(), 1..10),
        which in 0usize..10,
        field in 0usize..7,
    ) {
        let chain = chain_of(&fs);
        prop_assert!(verify_chain(&chain).is_ok());
        let i = which % chain.len();
        // One field of one receipt changed after sealing.
        let mut tampered = chain.clone();
        let r = &mut tampered[i];
        match field {
            0 => r.status.push('!'),
            1 => r.intent_hash.push('0'),
            2 => r.policy_decision = format!("{}?", r.policy_decision),
            3 => r.execution_target.push('x'),
            4 => r.occurred_at = Timestamp(r.occurred_at.0.wrapping_add(1)),
            5 => r.evidence_ref = Some(format!("{}+", r.evidence_ref.clone().unwrap_or_default())),
            _ => r.approval_id = Some(ApprovalId::new()),
        }
        prop_assert!(verify_chain(&tampered).is_err(), "tamper of field {field} on receipt {i} not detected");
        // The hash check is what catches it: a verifier without it accepts the tamper.
        prop_assert!(verify_links_only(&tampered), "docs/55: the link-only verifier must accept a field tamper");
    }

    #[test]
    fn reorder_and_interior_deletion_break_the_links(
        fs in prop::collection::vec(fields(), 2..10),
        a in 0usize..10,
        b in 0usize..10,
    ) {
        let chain = chain_of(&fs);
        let (i, j) = (a % chain.len(), b % chain.len());
        if i != j {
            let mut swapped = chain.clone();
            swapped.swap(i, j);
            prop_assert!(verify_chain(&swapped).is_err(), "swap {i}<->{j} not detected");
            // Each receipt still recomputes; only the link check sees the reorder.
            prop_assert!(verify_hashes_only(&swapped), "docs/55: the hash-only verifier must accept a reorder");
        }
        if chain.len() >= 2 && i < chain.len() - 1 {
            let mut cut = chain.clone();
            cut.remove(i);
            prop_assert!(verify_chain(&cut).is_err(), "interior deletion at {i} not detected");
        }
        // Truncating the tail is invisible to the chain alone: that is what
        // the store's `last_receipt_hash` is for (docs/23), stated here.
        let mut truncated = chain.clone();
        truncated.pop();
        prop_assert!(verify_chain(&truncated).is_ok());
    }

    #[test]
    fn the_receipt_hash_is_a_function_of_every_field_but_itself(f in fields(), f2 in fields()) {
        let one = chain_of(std::slice::from_ref(&f));
        let mut same = one[0].clone();
        same.receipt_hash = "anything".into();
        prop_assert_eq!(receipt_hash(&same), one[0].receipt_hash.clone(), "the stored hash never feeds its own computation");
        let two = chain_of(&[f.clone(), f2.clone()]);
        prop_assert_eq!(two[1].previous_receipt_hash.as_deref(), Some(two[0].receipt_hash.as_str()));
    }
}
