# Task Card — IMP-EV-0066 Reversibility/compensation classes

## Identity

- Task ID: IMP-EV-0066 (REQ-EV-0066, ADOPT; owner Effect Ledger)
- Milestone: M9
- Qualification: QUAL-EV-0066 — an external API effect is never labeled fully undoable; a compensation receipt is distinct.
- Evidence tier: release-critical (effect ledger, canonical persistence, protocol and schema)

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. Effects had an effect class (`ReadOnly` … `Destructive`) and protected effects a hash-chained receipt (M9.2), and workspace writes a typed undo (REQ-EV-0064/0065); but no receipt said how far its effect could be taken back, `UndoToolCall` on an external effect answered `NOTHING_TO_UNDO: the call changed no files` — as if the pull request it opened were nothing to take back — and there was no compensation path or compensation receipt.
- production entry points: `crates/domain/src/toolcall.rs` (`Reversibility::of`, `EffectReceipt.reversibility` / `.compensates`, omitted when absent so earlier receipts keep their hash); `crates/tools` (`ToolSpec.compensation`, `ToolSpec::reversibility`, `forge.pr.create` compensated by `forge.pr.update`, `forge::compensation_call`); `crates/event-store` (V17 `effect_receipts.reversibility`, `.compensates`; projected and read back so `verify_chain` recomputes over them); `services/modbit-core/src/tools.rs` (every receipt stamped from the tool's declaration; `InvokeRequest.compensates`); `compensation.rs` (`not_undoable`, `CompensateEffect`); `server.rs` (the undo refusal, the command, `review.decide`, fenced).

## Verification

- `qual_ev_0066_an_external_effect_is_never_undoable_and_its_compensation_is_a_receipt_of_its_own` (services/modbit-core, real Core, GitHub-compatible forge, a real bare remote): the opened pull request's receipt is `COMPENSATABLE`; `UndoToolCall` refuses it `NOT_UNDOABLE` naming `forge.pr.update`; `CompensateEffect` first asks for its own approval (nothing reaches the forge), then closes the pull request once; the compensation receipt is a new effect naming the original, `IRREVERSIBLE` itself, beside the untouched original, and the chain verifies; a second request answers from that receipt with one close on the forge; the compensation can be neither undone nor compensated; after a kill and restart the chain with the new fields verifies from the projection and reads back identical.
- `crates/policy` unit `reversibility_and_compensation_are_hashed_and_legacy_receipts_keep_their_hash`: a pre-V17 receipt serializes without the fields and keeps its hash; relabelling a compensatable effect as reversible breaks the chain.
- Regression: event-store migration suite at V17, the receipt-chain property suite (M9.6), the full modbit-core suite including `qual_ev_0270_…`, `qual_px_006/007/008/009`.

## Limitations

- One declared compensation today (`forge.pr.create` → close); MCP and browser effects are `IRREVERSIBLE` until their tools declare one.
- `PARTIALLY_REVERSIBLE` effects keep the typed undo of their content; the refusal applies to `COMPENSATABLE` and `IRREVERSIBLE` effects.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- docs/23, docs/30 and docs/31 carry the as-built paragraphs
