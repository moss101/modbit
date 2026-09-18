# Task Card — IMP-EV-0233 Broad terminal backends

## Identity

- Task ID: IMP-EV-0233
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0233 (replaceable local/cloud/private backends under one contract; no consumer/serverless vendor by default); QUAL-EV-0233 (the backend conformance suite runs the same fixture on the supported backends); docs/21 "Sandbox substrate boundary"; REQ-EV-0291 (replaceable sandbox backend boundary).
- Qualification: the one conformance suite on the Firecracker MicroVM backend (the cloud substrate) and on the reference backend (a host process), on hosted CI and on every developer OS.
- Evidence tier: real-system.

## Goal

Every backend the product runs a task's processes on answers one contract, and the same suite proves each.

## Existing-code audit

- classification: PRESENT since M8.3, extended since: `crates/sandbox/src/backend/mod.rs` (`SandboxBackend`: provision, reconnect, destroy, the verified image), `backend/microvm.rs` (Firecracker), `backend/reference.rs` (a host process, unisolated and saying so), `crates/sandbox/src/conformance.rs` (the fixture and its ~40 steps: provisioning, admission, the process/PTY/file/dir/egress/browser calls, protected paths, control-plane isolation, replay refusal, teardown); `apps/sandbox-gateway/src/lib.rs` (`BackendChoice` chosen by configuration, `MODBIT_SANDBOX_BACKEND`); no consumer or serverless vendor is a backend.
- proof: `apps/sandbox-gateway/tests/gateway.rs::qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` and `qual_m8_3_the_reference_backend_passes_the_backend_contract` — the same `conformance::run` over the same fixture, every step held on both, including the M8.6 egress steps and the M8.8 browser steps; the report names the backend and whether it isolates.

## Limitations

- Two backends exist (MicroVM, reference); a private-cloud or third substrate is a `SandboxBackend` implementation away and would run the same suite.
- The reference backend isolates nothing and is refused for tenants by the gateway's own warning; it exists for development and for the contract.

## Verification

- `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract`
- `qual_m8_3_the_reference_backend_passes_the_backend_contract`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
