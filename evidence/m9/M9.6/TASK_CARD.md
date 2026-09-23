# Task Card — M9.6 security fuzz/property/attack suites

## Identity

- Task ID: M9.6 (milestone task, docs/43 "M9 — Engineering memory/effects/security hardening")
- Milestone: M9; release: RELEASE_ZERO
- Specification: `docs/52_SECURITY_THREAT_MODEL_AND_TESTS.md` "Security gates" (fuzzers for the protocol decoder, path normalizer, tool argument validation and event migration; property tests for the effect receipt chain and lease fencing) and "Major threats and controls" (the attack tests per threat); `docs/55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md` (prove the test detects broken behaviour).
- Evidence tier: release-critical (security boundaries)

## Goal

Randomized and adversarial proof over the real components that the security boundaries hold for inputs nobody wrote a case for, with each suite carrying the mutation that shows its control is load-bearing.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. Every threat of docs/52 had at least one hand-written qualification (path traversal and symlink escape, argv-explicit execution, secret custody, cross-tenant refusal, at-most-once effects, hostile MCP declarations, the receipt chain's tamper cases, lease fencing's stale-writer case), but no randomized suite existed anywhere, no TOCTOU race was tested, and no hostile-metacharacter argument had been sent through the real broker.
- production entry points exercised: `crates/protocol/src/framing.rs`, `crates/workspace/src/paths.rs` and `service.rs`, `crates/tools` registry schemas, `crates/policy/src/ledger.rs`, `crates/event-store/src/store.rs` (`append_fenced`, migrations, `rebuild_projections`), `services/modbit-execd` (the real broker process).
- found by the suites: on Unix the path normalizer accepted a backslash as a filename character but recorded the root-relative path with `\` rewritten to `/`, so `src\x.rs` named one file while the record — the write-set match, the diff invariants, the events — said `src/x.rs`. The normalizer now refuses a backslash on non-Windows platforms (`crates/workspace/src/paths.rs`); the fuzzer's own oracle was corrected once (a nonexistent directory named `...` is not the root-level link).

## Verification

- `crates/protocol/tests/property_framing.rs` (3 properties, 256 cases each)
- `crates/workspace/tests/property_paths.rs` (2 properties at 512 cases, the TOCTOU attack test)
- `crates/tools/tests/property_arguments.rs` (2 properties at 128 cases; every direct tool closes its schema)
- `crates/policy/tests/property_chain.rs` (3 properties at 96 cases, with the two verifier mutations)
- `crates/event-store/tests/property_fencing.rs` (2 properties at 24 cases on real SQLite)
- `services/modbit-execd/tests/broker.rs::hostile_arguments_reach_the_process_as_literal_tokens_and_nothing_inside_them_runs`
- unchanged and green: the workspace crate's suites, the Core's workspace/change/review qualifications, clippy across the six crates.

## Failure and negative proof

- docs/55 mutations inside the suites: a verifier without the hash check accepts a field tamper and one without the link check accepts a reorder (the real verifier rejects both); a corrupted length prefix never yields a frame; a stale lease generation writes nothing while the current one advances.
- The path fuzzer's finding is itself the negative proof of the previous normalizer: `\..\` was accepted and recorded as `/../`.

## Limitations

- These are property suites, not coverage-guided fuzzing: `cargo-fuzz`/libFuzzer needs a nightly toolchain the pinned build does not use; the case counts are bounded so `cargo test --workspace` stays within CI time. A coverage-guided campaign is a release-procedure item beside the external penetration test (docs/52 "Security gates").
- SAST, dependency and secret scanning and the signed SBOM are M10.2's.
- The sandbox/SSRF and cross-tenant attack matrices remain the sealed M8 qualifications; they are not re-randomized here.

## Evidence

- `evidence.json` in this directory; PR #22, hosted CI run recorded there
- docs/52 and docs/55 carry the as-built paragraphs
