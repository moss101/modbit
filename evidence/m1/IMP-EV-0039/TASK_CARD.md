# Task Card — IMP-EV-0039 Typed ConfigurationResolver

## Identity

- Task ID: IMP-EV-0039
- Milestone: M1
- Canonical owner: Configuration Service / effects-security (modbit-policy::config)
- Requirement IDs: REQ-EV-0039; qualification QUAL-EV-0039
- Risk class: high (protocol / persistence / security boundary as applicable)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Admin, project and user layers merge with domain-specific laws (permissions tighten-only, network and model allow-lists intersect, admin MCP deny wins, hooks/rules ordered by authority) and every resolved value carries provenance; attempted widenings are recorded and rejected.

## Non-goals

Behavior owned by later milestones (agent runtime, providers, checkpoints, cloud plane).

## Required reading

`AGENTS.md`, the `docs/40` row for REQ-EV-0039, `docs/13`, `docs/19`, `docs/30`, `docs/32`, `docs/33`, `docs/39`, `docs/81`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL on the M1.1–M1.5 substrate before this task; this task adds the missing behavior and the named qualification test
- production entry point: `modbit-core` command handlers / desktop main IPC / domain reducers as applicable
- real effector/storage boundary: `core.db`, the real local socket, the real Electron app

## Verification

- qualification test IDs: QUAL-EV-0039 → modbit-policy config tests: lower_authority_narrows_but_never_widens_and_everything_has_provenance; resolution_is_deterministic_and_order_independent; missing_layers_and_empty_allow_lists_behave
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json` (written at seal time).
