# Agent Task Execution Protocol

## Phase A — reconcile

- load task-scoped authority;
- enumerate requirement IDs;
- run existing-code feature audit;
- identify architecture owner and forbidden boundaries;
- identify tests that will prove completion before writing code.

## Phase B — plan

Plan the smallest **complete vertical slice**, not the smallest diff. State domain/API change, persistence, policy, effect boundary, UI/model projection, recovery/failure handling and tests when applicable.

## Phase C — implement

Implement behind canonical interfaces. Keep migrations/versioning compatible. Add idempotency/cancellation/evidence at the same time as success behavior; do not postpone reliability to a vague later cleanup task.

## Phase D — verify locally

Run formatting/lint/type/unit/property tests. Then run integration tests through real registration/routing.

## Phase E — prove real effect

Execute the linked qualification/E2E path against a real production-equivalent boundary. Kill/restart processes where durability is claimed. Use actual Chromium for browser behavior and actual guest execution for cloud-sandbox behavior.

## Phase F — attack/fault

Run at least the task-relevant negative/fault case. Verify no data loss, duplicate protected effect, authority escalation or silent corruption.

## Phase G — evidence and handoff

Capture test IDs, run IDs, revision, artifact/event/effect refs, environment digest and results. Update build manifest. Only then move the task to COMPLETE.
