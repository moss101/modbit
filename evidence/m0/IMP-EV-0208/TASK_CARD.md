# Task Card — IMP-EV-0208 No second general memory system

## Identity

- Task ID: IMP-EV-0208
- Milestone: M0
- Canonical owner: Architecture Governance (subsystem `governance`); enforced by `tools/architecture-lint`
- Requirement IDs: REQ-EV-0208 (ADOPT); qualification QUAL-EV-0208
- Risk class: medium (architecture guardrail)
- Evidence tier: release-critical (canonical persistence boundary)
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

The production runtime has exactly one Engineering Memory interface (`modbit-memory`); the Skill Evolution Lab wiki (`modbit-skills`) can never become a parallel memory or recovery system, and the dependency test proving it runs in CI on every push.

## Non-goals

- Implementing memory promotion, retrieval or storage (M9 tasks under the `memory` owner).

## Required reading

`AGENTS.md`, `docs/19` (memory separation), `docs/26` (Skill Lab scoping), `docs/40` row REQ-EV-0208, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL (M0.4 made `engineering-memory` a canonical system claimed only by `modbit-memory`; no dependency rule kept `modbit-skills` off memory persistence or recovery stores; no named qualification test)
- production entry point: `architecture-lint deps` and `architecture-lint modules` in the rust CI matrix
- real effector/storage boundary: real `cargo metadata` over the workspace
- tests found: generic lint tests
- first missing/broken link: no REQ-EV-0208-specific rules or test
- duplicate/drift risks: a later crate claiming `engineering-memory` or `modbit-skills` growing a dependency on the stores; both now fail CI

## Invariants

- Exactly one claimant of `engineering-memory`.
- `modbit-skills` has no dependency path to `modbit-protocol-state`, `modbit-checkpoint`, `modbit-compaction` or `modbit-event-store`.
- `modbit-skills`, `modbit-procedural-runtime`, `modbit-prompt-compiler`, `modbit-context`, `modbit-retrieval` have no path to `modbit-memory`; memory items reach them through core-runtime ports.

## Implementation slice

- policy/capabilities: two `[[forbid]]` rules tagged REQ-EV-0208 in `tools/architecture-lint/rules.toml`
- service/effector: existing lint subcommands; `tools/architecture-lint/tests/qual_ev_0208.rs`
- failure/recovery: CI fails on violation

## Verification

- qualification test IDs: QUAL-EV-0208 → `qual_ev_0208_exactly_one_engineering_memory_owner_and_no_parallel_paths` (real workspace) and `qual_ev_0208_negative_second_memory_system_is_rejected` (disposable workspace with a second claimant and a skills→checkpoint edge)
- E2E: rust CI matrix on macOS, Linux, Windows

## Completion evidence

See `evidence.json`.
