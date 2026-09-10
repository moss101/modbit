# Task Card — IMP-EV-0131 Context window breakdown

## Identity

- Task ID: IMP-EV-0131
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0131; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0131` — Inspector totals match actual provider request envelope.
- Evidence tier: real-system or production-equivalent

## Goal

Expose composition, token cost, source, freshness and reasons.

## Existing-code audit

- classification: IMPLEMENTED in this batch (M3.8 built the pack, IMP-EV-0169 the envelope validation; nothing exposed either to a client)
- production entry point: `crates/protocol` `GetContextInspector` → `ContextInspectorView`; `services/modbit-core/src/inspector.rs` builds it from the task's Context Ledger and the last ContextCompile step object; CLI `context show <task-id>`; desktop `context:inspector` IPC and the inspector panel (`context-inspector`, `context-entry`, `context-omitted`)
- proof: GetContextInspector serves the task's pack from the Context Ledger and the last ContextCompile step: every entry carries its source ref, path, line range, reason, retrieval reasons, sources, freshness, token cost, content hash and revision, whether the prompt envelope injected it and whether a later tool call used it; stubs and omitted paths are listed with their token cost; the view's injected refs and token total are the envelope's own (the test also checks them against the request the provider received), and a fragment the envelope refused for missing provenance is reported as refused

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0035_0131_0175_context_inspector_matches_the_prompt_envelope`
- `qual_ev_0169_context_pack_reaches_the_prompt_with_provenance_or_not_at_all`
- `m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use`

The desktop and CLI surfaces are built and typechecked in the same CI run
(`node CI_COMPATIBLE`, `desktop E2E`, `rust CI_COMPATIBLE`).

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
