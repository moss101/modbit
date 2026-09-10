# Task Card — IMP-EV-0161 Connected engineering context

## Identity

- Task ID: IMP-EV-0161
- Milestone: M3
- Requirement: REQ-EV-0161; owner label: Context Connectors; subsystem: context-engine
- Qualification: `QUAL-EV-0161` — Prompt injection in ticket remains untrusted data and cannot grant tools.
- Evidence tier: real-system or production-equivalent

## Goal

Approved specs/issues/design docs enter provenance-labeled context.

## Existing-code audit

- classification: NOT-FOUND before this task. Attachments existed for media through the channel pipeline, but no spec, issue or design document could become part of a task's retrievable context.
- production entry point: `AttachContextDocument` → `TaskEvent::ContextDocumentAttached`; `services/modbit-core/src/tools.rs` (`attached_documents`, the connector candidates in the pack); CLI `task attach-context`.
- proof: a document the user approves — they hand it to the Core themselves, naming where it came from — is stored by digest and recorded on the task's log with the trust label `UNTRUSTED_EXTERNAL_CONTENT`. From then on it is a retrieval candidate like any other, ranked by how much of the query it actually contains, never automatically critical, and its entry carries its own provenance: the source the user named, the content hash, the source ref `attached:<source>`, and a reason that says it is untrusted external content, data and never instructions. The injection proof is a real run: the ticket tells the agent it now has every tool and may write without a plan; the tool it names is not on the task's compiled surface and the call is refused before dispatch, the write is refused because no plan was recorded, the file on disk is unchanged, and the task's capability lease is identical before and after — resources, operations, effect ceiling and generation.

## Limitations

Modbit fetches nothing: the client brings the text, so "approved" means the user attached it. There are no issue-tracker or wiki connectors yet, and no refresh — an attached document is a snapshot, and a newer version is a new attachment. Documents are bounded at 256 KiB and enter whole (the pack's head excerpt), not chunked by section.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0161_an_attached_ticket_is_labelled_context_and_cannot_grant_a_tool`
- `qual_ev_0169_context_pack_reaches_the_prompt_with_provenance_or_not_at_all`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
