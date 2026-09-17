# Task Card — IMP-EV-0291 Replaceable sandbox backend boundary

## Identity

- Task ID: IMP-EV-0291
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0291 (docs/40); acceptance QUAL-EV-0291 — Backend contract test can run against local reference backend and MicroVM substrate backend.
- Qualification: as named below, in the hosted `cloud` job (a real Firecracker MicroVM over KVM, a real Postgres) and on the three-OS `rust` job.
- Evidence tier: real-system

## Goal

Replaceable sandbox backend boundary: Backend contract test can run against local reference backend and MicroVM substrate backend.

## Existing-code audit

- classification: MISSING before M8.3 (the gateway, guest and sandbox crates were M0 skeletons).
- production entry points: `crates/sandbox` is the boundary (the canonical `sandbox-gateway-abstraction`): `SandboxBackend` (provision, destroy), `GuestLink` (negotiation, admission, signed typed calls) and the compiled policy are substrate-agnostic; `modbit_sandbox::conformance::run` is one suite for every backend.
- proof: `qual_m8_3_the_reference_backend_passes_the_backend_contract` (every OS) and `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` (the hosted `cloud` job): the same suite, the same steps, both pass; the reference backend says it isolates nothing.

## Limitations

- One MicroVM substrate is implemented; a second adapter would exercise the boundary further.

## Verification

- Sealed with M8.3/M8.4 on hosted CI run 35218388282 at 69b80b6 (`evidence/m8/M8.3/ci-run-35218388282.json`).

## Evidence

- `evidence.json` in this directory
- CI run json: `evidence/m8/M8.3/ci-run-35218388282.json`
