# Task Card — IMP-EV-0132 Searchable transcript/tool evidence

## Identity

- Task ID: IMP-EV-0132
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0132; owner label: Session Index; subsystem: durability (event store) + tool runtime
- Qualification: `QUAL-EV-0132` — Search returns evidence by run/step and respects tenant scope.
- Evidence tier: real-system or production-equivalent

## Goal

Index messages/tools/commands/files/errors/checkpoints metadata.

## Existing-code audit

- classification: IMPLEMENTED in this batch (was NOT-FOUND: evidence was reachable only by offset/aggregate reads)
- production entry point: crates/event-store/src/store.rs `EventStore::search_evidence` (`EvidenceScope`, `EvidenceHit`); `evidence.search` tool in crates/tools; Core `IndexPort` kind `evidence` (tenant, session and task from the invoking task)
- proof: the search walks the tenant's events (type + inline or object payload, classified as message/tool/step/file/error/checkpoint/event), the tasks' tool-call rows and verification checks, within the session/task/run/step scope; hits carry run, step and tool-call ids and a bounded snippet; the tenant filter sits in the SQL, so another tenant's events with the same run and step ids are never returned

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `evidence_search_scopes_by_tenant_run_and_step` (real SQLite: two tenants, run and step scoping, kind filter)
- `qual_ev_0134_deferred_tool_search_activates_without_authorizing_and_hydrates_schemas_lazily` (after a real scripted run: `evidence.search` finds the activation event by run and a step-scoped hit; scoping by that step returns only it)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
