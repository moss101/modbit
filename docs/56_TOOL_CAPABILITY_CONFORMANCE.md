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
| Test | real project test runner | pass/fail/timeout/artifact output and verifier attribution; normalized `TestReport`/`CheckResult` per adapter (vitest/jest/mocha, pytest, cargo, go) with `STRUCTURED` vs `HEURISTIC` confidence and `UNKNOWN` never counted as pass (`64_VERIFICATION_EXECUTION_CONTRACTS.md`) |
| Verification execution | real fixture repositories with seeded flaky, acceptance-named and pre-existing failing tests | BASELINE/TARGETED/COMPLETION stages, `KNOWN_FAILING` labelling, isolated rerun and revision-scoped quarantine, regression attribution, diff invariants DI-1..DI-9 including DI-3 test integrity (PX-032..037) |
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
| Language tiers | real fixture repositories per language | Tier A/B/C suites of `76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`: recall, diagnostics parity, structural edit safety, text safety, configured-command evidence; a language enters a tier only through a recorded pass (PX-027) |
| Platform conformance | CI runners for macOS, Windows, Linux | PTY/process, language services, Git, path/symlink policy, secrets, packaging, browser host; results labeled CI_COMPATIBLE, never support (PX-030) |
| Review isolation | real disposable worktree + sandboxed process under `review_isolated` | permitted scratch write/build/test; denied canonical write, commit/push, secret read, egress, deploy; hidden-reasoning exclusion; cleanup on accept/cancel/kill (EPR-E2E-018, EPR-FI-018) |

## Language tier suites (PX-027)

The tier suites are `crates/verification/src/tiers.rs` and they run in
`qual_px_027_language_tier_suites_run_on_real_fixtures_and_a_tier_is_only_a_recorded_pass`
against the fixture repositories of `50_TEST_STRATEGY_REAL_SYSTEM_GATES.md`. A
tier is every check of that tier and of every tier below it, passing on a real
repository; a skipped check is not a pass.

| Check | Tier | What it proves |
|---|---|---|
| `c1_text_edit_preserves_encoding_and_line_endings` | C | edits never corrupt encoding or line endings |
| `c2_exact_and_lexical_retrieval` | C | exact and BM25 retrieval return revision-bound hits |
| `c3_configured_command_evidence` | C | evidence from the configured command is attributed to the task |
| `b1_symbol_extraction` | B | tree-sitter finds the definitions of a real file |
| `b2_build_output_diagnostics` | B | failures are parsed out of build or test output and located |
| `b3_revision_bound_structural_edit` | B | an edit against a stale revision is refused |
| `a1_language_service_symbols_and_references` | A | the headless service returns symbols and finds a known reference |
| `a2_diagnostics_parity` | A | the service reports a seeded defect at its line and stops once it is fixed |

Passes are recorded in `crates/verification/language-tiers.json`. The record is
the only thing a tier claim may rest on: `verify_record` refuses a record that
claims more than its checks show, the suite refuses a record that claims more
than the run earned, and every client's language label carries the recorded
line or says that no suite has run. Promoting a language into a tier in the
product's labels is PX-028 and may cite nothing but these records.

## Procedural runtime proof

At least one release-gate task must expose only `exec`, `wait`, and `request_user_input` to the model while the generated isolated program composes `tools.*`. The task must edit files through Change Engine, run tests, inspect Git diff and return evidence. Nested tool calls must be indistinguishable in policy/evidence rigor from direct model tool calls.

## Source parity assertion

The CI script reads the source reconciliation ledger and verifies every source capability with `ADOPT/ADAPT/ALREADY COVERED` has at least one canonical tool/domain operation or explicit non-tool owner. **Exact undocumented private tool names are never fabricated merely to claim parity.**
