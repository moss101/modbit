# Task Card — PX-004 Provenance-bound external diagnostics intake

## Identity

- Task ID: PX-004
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-PX-004; owner label: context-engine; subsystem: core (`external_diagnostics.rs`), retrieval planner, verification plan, protocol, shared thin client
- Qualification: QUAL-PX-004 — an adapter submits language-service diagnostics for a fixture revision; the Core normalizes them with provenance `external_ide`; they appear in Context Pack provenance and verification-plan inputs; a mandatory verification step still executes Modbit's own check; a stale submission is discarded, a batch cannot mark any verification step passed, a malformed batch is rejected before persistence.
- Evidence tier: real-system (the real Core on a real git worktree with a repository-configured mandatory check, a scripted OpenAI-compatible server, the real verification engine through the terminal broker)

## Goal

`SubmitExternalDiagnostics` normalizes IDE or linter diagnostics into the canonical diagnostics records with provenance external_ide, bound to workspace and file revision; used as context and verification-plan input, never as a substitute for a mandatory verification step (docs/29).

## Existing-code audit

- classification: DOCUMENTED-ONLY before: the headless language-service bridge (M3.4) produced canonical diagnostic records from servers the Core runs; nothing let an editor hand its own in; the planner linked only Modbit's failing checks; the verification plan had no notion of external inputs.
- production entry points: `services/modbit-core/src/external_diagnostics.rs` (`submit`: validation, per-file revision check, normalization into `modbit_diagnostics::Diagnostic` with `external_ide:<adapter>` as source, batch object by hash, `ExternalDiagnosticsRecorded` / `ExternalDiagnosticsRejected` under the command record, replay; `locations` for retrieval at the current revision; `plan_inputs` for the verification plan), `server.rs` (`SubmitExternalDiagnostics`), `crates/domain` (the two task events), `crates/retrieval/src/planner.rs` (`PlanRequest.external_diagnostics`; hit reasons `diagnostic` + `external_ide`), `services/modbit-core/src/tools.rs` (`IndexPort.external`), `crates/verification/src/plan.rs` (`VerificationPlan.external_diagnostics: ExternalDiagnosticsInput`), `runtime.rs::verification_plan` (records the inputs at derivation), `surface.proto` (`ExternalDiagnostic`, `SubmitExternalDiagnostics`, `ExternalDiagnosticsAck`), `packages/ide-adapter-core` (`submitExternalDiagnostics`).
- proof: on a TypeScript fixture with `.modbit/verification.json` naming a mandatory `typecheck`, a batch from `vscode:typescript-language-features` at the current workspace revision records one diagnostic on `src/app.ts` (file revision matching) and drops one on `src/other.ts` computed on other bytes (`recorded=1`, `discarded=1`); the batch object carries `provenance: external_ide`, the revision, the per-file revisions and the normalized record; a retry replays; a batch at another workspace revision is refused `STALE_REVISION`, a `fatal` severity and a `../` path are refused `MALFORMED`, and the log holds one `ExternalDiagnosticsRecorded` and `ExternalDiagnosticsRejected` × 3 with those codes; `context.pack` packs `src/app.ts` as `critical:diagnostic` with `retrieval_reasons` `diagnostic` and `external_ide`; the run's plan (derived at the first write, before the edit) records the batch under `external_diagnostics` and the COMPLETION run executes `configured:typecheck` (`PASS`) with no check derived from the batch; after the edit the file moved on and the batch links nothing.

## Limitations

- Diagnostics link retrieval and inform the plan; the harness does not yet raise them to the model as a distinct observation (they reach it through packed context).
- Batches are per task and per source (latest wins at a revision); there is no session-wide store.
- The VS Code adapter that forwards its language services' diagnostics is PX-002; the intake is exercised here through the shared client and the surface protocol.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_004_external_diagnostics_are_provenance_bound_context_and_never_verification`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
