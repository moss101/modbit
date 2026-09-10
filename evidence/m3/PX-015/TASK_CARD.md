# Task Card — PX-015 Retrieval-before-edit contract

## Identity

- Task ID: PX-015
- Milestone: M3
- Requirement: REQ-PX-015; owner: context-engine; docs/28 §2
- Qualification: QUAL-PX-015 — real fixture task: every edited file has a retrieval record at the current workspace revision and the Context Ledger records use; symbol edits show definition and reference retrieval; an edit to a file without a retrieval record is rejected, and a stale-revision retrieval does not satisfy the rule
- Evidence tier: real-system or production-equivalent

## Goal

Material mutation requires adequate repository understanding: no edit to a file the task has not retrieved.

## Existing-code audit

- classification: IMPLEMENTED in this batch for file-level records (was PARTIAL: the Context Ledger recorded pack entries and their use, but nothing gated an edit). Symbol-level definition/reference retrieval is recorded when the task uses the lsp.* tools on the path; it is not yet required per symbol, and this card does not claim it.
- production entry point: `services/modbit-core/src/runtime.rs` `unretrieved_targets` and the write gate (`HarnessRefusal::RetrievalRequired`); `services/modbit-core/src/tools.rs` records every fs.read / lsp.* retrieval and the task's own writes in the Context Ledger and as `RetrievalRecorded` task events; `crates/context` `ContextLedger::record_read` / `has_current_record`
- proof: the runtime refuses HARNESS_RETRIEVAL_REQUIRED before any effector when a write targets an existing file the task has no retrieval record for: a record is an fs.read, an lsp.* query, a Context Pack entry or the task's own write, and it binds to the bytes it was taken at (content hash), so a record whose file was changed behind the task is stale and refused; records are durable TaskEvents (RetrievalRecorded), so the gate still holds after a Core restart when the in-memory ledger is gone

## Interpretation recorded with this task

Interpretation of docs/28 §2 recorded here: the rule is 'no edit to bytes the task has not retrieved'. A record is checked against the file's current content hash rather than against the workspace revision number alone, so a record taken before an unrelated write elsewhere is still valid while a record whose own file changed is refused. The revision is still recorded on every record and reported in the refusal.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_015_retrieval_before_edit_is_enforced_and_a_stale_record_is_refused`
- `ledger_records_injections_and_marks_use_only_at_the_retrieved_revision`
- `left_out_candidates_become_signature_stubs_inside_the_budget_with_hydration_handles`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

The fixture test drives a real scripted run: a blind edit is refused, a read
makes the next edit legal, the Core is killed and a third party rewrites the
file, and after the restart the edit is refused as stale until the task reads
again; a created file needs no record and the task's own write is the record
for its next edit.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
