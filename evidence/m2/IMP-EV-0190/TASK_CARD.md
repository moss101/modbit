# Task Card — IMP-EV-0190 Channel image/file ingestion

## Identity

- Task ID: IMP-EV-0190
- Milestone: M2 (backlog batch 2d)
- Requirement: REQ-EV-0190 (ADAPT); owner label: Input Gateway; subsystem: core-runtime
- Qualification: QUAL-EV-0190 — Upload image/file through desktop/API and verify same canonical envelope.
- Evidence tier: real-system or production-equivalent

## Goal

Upload image/file through desktop/API and verify same canonical envelope. Normalize attachments to MediaEnvelope with tenant/task/source provenance; no consumer chat-product scope.

## Existing-code audit

- classification: IMPLEMENTED in this batch (the media pipeline existed since M2.10; no ingestion path)
- production entry point: IngestAttachment command in services/modbit-core/src/server.rs (media pipeline of crates/tools/src/media.rs, AttachmentIngested on the task); desktop main task:attach + preload attachFile; CLI task attach
- proof: an attachment ingested through the API, the desktop (task:attach IPC over the same command) and the CLI (task attach) is normalized by the media pipeline to the same MediaEnvelope a workspace read of the same bytes yields (kind, MIME, digests, dimensions, stripped metadata, trust); only the provenance source differs; the log carries digests, the original is retrievable by digest, the same bytes replay, malformed media is a typed refusal

## Verification

Named tests (crate/test-binary :: test name), run by `.github/workflows/ci.yml`:

- `modbit-core/surface_protocol` :: `qual_ev_0190_attachments_normalize_to_the_same_canonical_envelope_as_workspace_reads`
- `desktop-e2e/fleet.spec.ts` :: `new task renders from Core ids, fleet reduces Core events, and survives Core kill and app restart`
- `modbit-cli/smoke` :: `cli_drives_a_real_core_end_to_end`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
