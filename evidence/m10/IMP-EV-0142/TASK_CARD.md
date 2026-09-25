# Task Card — IMP-EV-0142 Trace/status/export diagnostics

## Identity

- Task ID: IMP-EV-0142 (REQ-EV-0142, ADOPT; owner Operations, subsystem observability)
- Milestone: M10 (wave 1)
- Qualification: QUAL-EV-0142 — export can replay evidence metadata and contains no credential values.
- Evidence tier: release-critical (evidence semantics; a security boundary: what leaves the machine)
- Built on IMP-EV-0017: the package is redacted by the same one redactor. No second store or log reader: the package is a read-only projection of the canonical log (`modbit_event_store`), its verification the store's own chain recomputation.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL for status (`GetTaskStatus`, `GetRecoveryReport`, `ListModels` health, CLI `task status`/`events tail`); NOT-FOUND for doctor, trace and a diagnostics export. SQLite's integrity check ran only at startup; `ExportHandoff` (M8.7) existed in the Core but no client could call it, and it changes state (parks the run), so it is not a diagnostics export.
- first missing link: no read-only, redacted package of the Core's state and no way to check one against the log.
- production entry points: `crates/observability/src/diagnostics.rs` (`Package`, `sealed`, `computed_digest`, `verify`); `services/modbit-core/src/doctor.rs` (`export`: scope, ranges and chain heads, integrity now, trace, recent failures, provider health without credentials, leases, object refs, opt-in content, whole-package redaction, seal; `verify` over `read_aggregate`/`verify_aggregate`); `services/modbit-core/src/server.rs` (`ExportDiagnostics`, `VerifyDiagnostics`); `apps/cli/src/main.rs` (`doctor`, `trace`, `export diagnostics`, `diagnostics verify`, `export handoff`); `packages/ide-adapter-core/src/client.ts` (`exportDiagnostics`, `verifyDiagnostics`, `exportHandoff`).

## Verification

- `qual_ev_0142_the_diagnostics_export_replays_evidence_metadata_and_holds_no_credential` (real Core, scripted provider): a Core holding a provider key and a forge token runs one task to review and has another refused by a provider echoing its key; a task's goal quotes an access key the Core does not hold. The session's package names the build; `integrity_check` ok, every chain in scope verified, receipts valid; each aggregate's range with a 64-hex head; a trace naming the refusal `TaskNeedsAttention` by `AUTH_REJECTED`; the refusal among recent errors as `PROVIDER`; the provider's host, `credential_configured`, successes and failures — and none of the three secrets. With content, the goal is there as `deploy with access key [redacted]`. The package replays (`verified`, every aggregate checked); changed after sealing it fails its digest; forged and resealed it fails against the log (one mismatch, "the package says"); a task-narrowed package holds only that task; after the Core is killed and a new one opens the same store, the original package replays.
- `imp_ev_0142_doctor_trace_export_verify_and_handoff_through_the_cli` (real CLI binary and Core): `doctor` reports `database=ok`, `receipts=valid`, the provider's host and `credential=configured` without the key; `trace` lists the task's events by type with no payload; `export diagnostics --include-content` writes a package with no key; `diagnostics verify` prints `verified=true`, and after the file is changed exits non-zero with `verified=false digest_ok=false`; `export handoff` writes the bundle's `manifest.json`.
- Mutations: removing the whole-package redaction fails the test on the goal's access key; verifying without comparing the pinned head accepts the forged package and fails the test.
- Unit: `modbit_observability::diagnostics::tests::a_sealed_package_replays_against_its_log_and_a_changed_one_does_not`; `doctor::tests::{a_host_is_named_without_what_could_carry_a_secret, a_failure_reads_its_code_from_its_diagnosis_or_its_own_field}`.

## Limitations

- Browser and terminal lease states are not in the package yet (capacity tickets name terminal holders; sandbox leases are listed).
- The desktop has the client methods but no menu entry; the person-facing export in the app is M10.5 (support diagnostics).
- The trace keeps the latest 5 000 lines and the error list the latest 200; a longer session is marked `trace_truncated`.
- Verification checks the heads a package pinned and each chain's integrity; it does not prove that no event was appended *before* the exported range was read (the ranges are what the log held at export).

## Evidence

- `evidence.json` in this directory
- docs/71 "Desktop diagnostics package" and docs/30 as built; `apps/cli/README.md` "Diagnostics"
