# Task Card — IMP-EV-0270 Protected-effect receipt chain (M9.2)

## Identity

- Task ID: IMP-EV-0270 (the substance of milestone task M9.2 "protected-effect receipt hash chain")
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0270 (high-risk effects produce an immutable hash-linked receipt chain bound to approval/capability/call/result); QUAL-EV-0270 (tamper/delete/reorder receipt causes chain verification failure); docs/23 "Protected-effect receipt chain".
- Qualification: a real Core producing protected-effect receipts, with the stored chain tampered/reordered/deleted directly in the SQLite store.
- Evidence tier: real-system.

## Goal

Every high-risk effect leaves an immutable, hash-linked receipt bound to its approval, capability lease, tool call and result; the chain is independently verifiable from the stored projection, so any tamper, deletion or reorder is detected.

## Existing-code audit

- classification: PARTIAL before this task: the Effect Ledger (M2.5/M2.8) already implemented the chain — `crates/policy/src/ledger.rs` (`receipt_hash`, `seal`, `verify_chain`), the `EffectReceipt` domain type with `previous_receipt_hash`/`receipt_hash`, the `effect_receipts` projection (event-store V5) from `EffectReceiptAppended`, `services/modbit-core/src/tools.rs` sealing a receipt for every effect ≥ `ProtectedWrite` chaining `last_receipt_hash`, and the `GetEffectReceipts` surface command returning `chain_valid`/`detail` by recomputing `verify_chain` over the stored rows on every read. What was missing for M9.2 was the real-system qualification of the failure behavior (tamper/delete/reorder against the store) required by QUAL-EV-0270.
- production entry points: `crates/policy/src/ledger.rs` (`verify_chain` — order + link + hash recompute), `services/modbit-core/src/server.rs` (`GetEffectReceipts` recomputes over `store.receipts(None)`), `services/modbit-core/src/tools.rs` (receipt sealed and linked as each protected effect completes), `crates/event-store` (`effect_receipts`, `last_receipt_hash`, `receipts`).
- proof: `services/modbit-core/tests/surface_protocol.rs::qual_ev_0270_the_protected_effect_receipt_chain_detects_tamper_delete_and_reorder` — two approved `git.worktree.close` effects produce a linked two-receipt chain (`chain_valid`, receipt[1].previous == receipt[0].hash, each hash 64 hex, each bound to its approval/lease/call/result); then, tampering a stored field makes verification fail with "does not recompute", swapping the two receipts' `seq` (reorder) fails, and deleting the first (a missing predecessor) fails — and restoring each makes `chain_valid` true again. The domain unit test in `ledger.rs` covers tamper and missing-predecessor directly.

## Limitations

- The chain is per tenant (this Core's store); a cross-tenant/cloud aggregated chain is out of scope.
- Tampering the projection is detected on read and self-heals on a projection rebuild from the immutable event log (the log is truth); tampering the log itself is caught by the store's own per-aggregate hash chain (M1.1), not by this receipt chain.
- Verification is whole-chain on each `GetEffectReceipts`; an incremental verifier is not needed at current volumes.

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0270_the_protected_effect_receipt_chain_detects_tamper_delete_and_reorder`
- `crates/policy` `ledger::tests::chain_verifies_and_tampering_is_detected`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
