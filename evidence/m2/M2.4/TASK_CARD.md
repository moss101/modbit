# Task Card — M2.4 Tool Registry + direct `fs/git/shell/test` tools

## Identity

- Task ID: M2.4
- Milestone: M2
- Canonical owner: tool-runtime (`crates/tools` is THE doc 81 tool registry; direct tools call the canonical workspace, git and execd substrates)
- Requirement IDs: REQ-EV-0079, REQ-EV-0217 (registry, typed specs, versioned schemas), REQ-EV-0239, REQ-EV-0080 (pipeline normalize→validate→policy→execute; monotonic deny; argument text cannot bypass policy), REQ-EV-0269 (OutputRef paging); docs/16 "Tool completion proof" (reachable through registry, kernel port and the event loop); docs/43 M2.4.
- Risk class: high (real filesystem, git and process effects)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

Tools exist only as typed specs in a registry and are reachable only through one pipeline: normalize → JSON-schema validate → capability/profile policy → execute → OutputRef spill → evidence. The Core exposes `ListTools` and `InvokeTool` on the SurfaceProtocol; every invocation is recorded as `ToolCall*` events on the canonical log. Direct tools: `fs.list/read/stat/glob`, `change.apply`, `git.status/diff/worktree.create/worktree.close`, `shell.exec`, `test.run` (through the real `modbit-execd`).

## Non-goals

- Approval-gated execution and capability grants beyond the profile policy (M2.5 Capability Kernel).
- Provider-driven tool calls (M2.6/M2.7), MCP tools, model-facing bounded views beyond the output budget.
- Test report parsers beyond the HEURISTIC exit-code parser (M2.8 Verification engine).

## Required reading

`AGENTS.md`, `docs/16`, `docs/21`, `docs/24`, `docs/30`, `docs/43`.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL (workspace, git, execd and terminal client existed; `crates/tools` was an M0.1 shell; no registry, envelopes or Core wiring)
- production entry point: `ToolRegistry`/`ToolPipeline` (`crates/tools`), `modbit_core::tools::ToolHost`, Core commands `ListTools`, `InvokeTool`; CLI `tool list`, `tool invoke`
- real effector/storage boundary: `modbit-workspace` (path policy, atomic writes, revision store), `modbit-git`, `modbit-execd` (spawned by the Core, stdin-tethered), event store objects for results and OutputRefs

## Invariants

- No tool runs without a registry entry, a passing schema check and a policy decision; a denial anywhere is final (monotonic deny).
- Protected paths and paths outside the task workspace are typed application failures; nothing is written.
- Every invocation appends `ToolCallProposed` and a terminal `ToolCallSucceeded/Failed/UnknownOutcome/Cancelled`; validated+dispatched calls also carry `ToolCallValidated`, `ToolCallPolicyDecision`, `ToolCallDispatched`.
- `InvokeTool` needs the session lease; the same `tool_call_id` replays the recorded result.
- Results above the output budget are spilled to a content-addressed OutputRef whose digest equals the raw bytes.
- The broker exits when the Core dies (stdin tether): no orphaned process brokers.

## Verification

- QUAL-EV-0079: invalid arguments rejected before any effector
- QUAL-EV-0239/0080: denial is monotonic; argument text cannot bypass policy
- fs/change/git tools against a real repository with revision binding (precondition hash)
- QUAL-EV-0269: large results paged by OutputRef with matching digest
- `shell.exec` and `test.run` through the real broker
- Core: `m2_4_invoke_tool_runs_direct_tools_through_registry_policy_and_event_log` over the real socket (fs.read, change.apply, git.status, protected path, schema violation, unknown tool, shell.exec, test.run, LEASE_REQUIRED, replay, ToolCall event trail)
- CLI smoke: `tool list`, `tool invoke` succeed/fail typed, events tail shows ToolCall events
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json`.
