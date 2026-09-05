# Task Card Template

## Identity

- Task ID:
- Milestone:
- Canonical owner:
- Requirement IDs:
- Risk class: low / medium / high / protected-effect / recovery-critical
- Evidence tier: release-critical / iteration. Iteration is allowed only if the change modifies none of: effect-bearing behavior, canonical persistence, permissions/policy, execution, recovery, protocol/schema, security boundary, evidence semantics. Mixed or uncertain → release-critical.
- Release membership: ALPHA / BETA / RELEASE_ZERO (from `python3 tools/graph.py show <id>`)

## Goal

Observable behavior the user/system must gain.

## Non-goals

Explicitly excluded behavior.

## Required reading

List exact dossier files and source modules.

## Existing-code audit

- classification:
- production entry point:
- current owner:
- real effector/storage boundary:
- tests found:
- first missing/broken link:
- duplicate/drift risks:

## Invariants

List architecture/security/recovery invariants that must remain true.

## Implementation slice

- domain/API:
- persistence/migrations:
- policy/capabilities:
- service/effector:
- UI/model projection:
- failure/recovery:
- evidence/observability:

## Verification

- unit/property:
- integration:
- qualification test IDs:
- E2E:
- security/fault:
- performance budget:

## Completion evidence

- commit/revision:
- test run IDs:
- artifacts/effect/event refs:
- environment/build digest:

## Status

NOT_STARTED / AUDITING / IMPLEMENTING / WIRED / REAL_TESTING / E2E_PROVEN / COMPLETE / BLOCKED
