# Task Card — IMP-EV-0168 Retrieve before edit

## Identity

- Task ID: IMP-EV-0168
- Milestone: M3
- Requirement: REQ-EV-0168; owner: Context + Change Engine; docs/28 §2
- Qualification: QUAL-EV-0168 — `QUAL-EV-0168` — Blind edit attempt with missing context is blocked/surfaced.
- Evidence tier: real-system or production-equivalent

## Goal

Material mutation requires adequate repository understanding: no edit to a file the task has not retrieved.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was NOT-FOUND)
- production entry point: `services/modbit-core/src/runtime.rs` `unretrieved_targets` and the write gate (`HarnessRefusal::RetrievalRequired`); `services/modbit-core/src/tools.rs` records every fs.read / lsp.* retrieval and the task's own writes in the Context Ledger and as `RetrievalRecorded` task events; `crates/context` `ContextLedger::record_read` / `has_current_record`
- proof: the runtime refuses HARNESS_RETRIEVAL_REQUIRED before any effector when a write targets an existing file the task has no retrieval record for: a record is an fs.read, an lsp.* query, a Context Pack entry or the task's own write, and it binds to the bytes it was taken at (content hash), so a record whose file was changed behind the task is stale and refused; records are durable TaskEvents (RetrievalRecorded), so the gate still holds after a Core restart when the in-memory ledger is gone

## Interpretation recorded with this task

Interpretation of docs/28 §2 recorded here: the rule is 'no edit to bytes the task has not retrieved'. A record is checked against the file's current content hash rather than against the workspace revision number alone, so a record taken before an unrelated write elsewhere is still valid while a record whose own file changed is refused. The revision is still recorded on every record and reported in the refusal.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_015_retrieval_before_edit_is_enforced_and_a_stale_record_is_refused`
- `qual_px_016_change_strategy_tests_first_one_concern_per_transaction_and_no_silent_scope_widening`
- `qual_px_018_repair_attempts_are_recorded_bounded_reverted_when_worsened_and_escalate_on_equivalence`

The fixture test drives a real scripted run: a blind edit is refused, a read
makes the next edit legal, the Core is killed and a third party rewrites the
file, and after the restart the edit is refused as stale until the task reads
again; a created file needs no record and the task's own write is the record
for its next edit.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
