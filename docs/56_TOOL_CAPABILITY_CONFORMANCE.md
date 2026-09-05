# Tool Parity and Capability Conformance — Real Effect Tests

> **Goal:** prove Modbit covers required source capabilities through canonical tools without tool-name cloning or fake effectors.

## Conformance contract

For every executable canonical tool family, the harness discovers the registered tool and records:

`schema → normalizer → capability decision → real effector → typed result → durable event → evidence/effect receipt → cancellation/retry behavior`.

A tool marked production cannot pass by returning a canned success value.

## Required real suites

| Suite | Real system | Minimum proof |
|---|---|---|
| FS | temporary real Git repository on disk | list/glob/read/media read/path traversal deny |
| Change | real worktree | stage/apply/reject/ambiguous target/concurrent edit/rollback |
| Git | real Git binary/repository | status/diff/log/blame/worktree/merge conflict |
| Terminal | real OS process + PTY | argv/cwd/env/input/output/cancel/detach/replay/restart |
| Test | real project test runner | pass/fail/timeout/artifact output and verifier attribution |
| Diagnostics | real parser/LSP/compiler adapter where supported | baseline vs introduced diagnostic delta |
| Context | real repository indices | L0/L1/L2/L3 selection, freshness, handles, provenance |
| Browser | real Chromium | navigate/semantic snapshot/action/network/console/visual fallback/takeover |
| Computer | approved native test app where platform permits | target identity/controller lock/human preempt/emergency stop |
| Agent | real model + real child runtime | spawn/idempotency/background/park/resume/steer/result/restart |
| Skill | real registry/package | discover/load/path gate/non-invocable/capability ceiling |
| MCP | real MCP test server | list/call/media/cancel/auth failure/transport pool |
| Web | real allowlisted test endpoint | fetch/search/network policy/redirect/size limit |
| Artifact | real content store | OutputRef range/digest/restart/tenant isolation |
| Memory | real DB | query/propose/promotion/scope/TTL/conflict/no transcript auto-promotion |

## Procedural runtime proof

At least one release-gate task must expose only `exec`, `wait`, and `request_user_input` to the model while the generated isolated program composes `tools.*`. The task must edit files through Change Engine, run tests, inspect Git diff and return evidence. Nested tool calls must be indistinguishable in policy/evidence rigor from direct model tool calls.

## Source parity assertion

The CI script reads the source reconciliation ledger and verifies every source capability with `ADOPT/ADAPT/ALREADY COVERED` has at least one canonical tool/domain operation or explicit non-tool owner. **Exact undocumented private tool names are never fabricated merely to claim parity.**
