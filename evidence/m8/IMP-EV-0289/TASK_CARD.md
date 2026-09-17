# Task Card — IMP-EV-0289 Typed capability/effect-bound guest RPC

## Identity

- Task ID: IMP-EV-0289
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0289 (docs/40); acceptance QUAL-EV-0289 — Unknown/stale RPC version/capability is rejected.
- Qualification: the guest protocol's negotiation and calls in the hosted `cloud` job (a real Firecracker MicroVM) and on the three-OS `rust` job (the reference backend).
- Evidence tier: real-system

## Goal

Guest operations use versioned typed RPC carrying task/effect/capability identity; an unknown or stale version or capability is rejected.

## Existing-code audit

- classification: MISSING before M8.3 (no guest protocol).
- production entry points: `crates/protocol/proto/modbit/v1/guest.proto` (protocol 1.1: `GuestHello`/`GuestAdmit`/`GuestRefused`, `GuestCall{call_id, task_id, effect_id, capability, auth, body}`, `GuestReply`), `crates/sandbox/src/link.rs` (negotiation: another major or a missing required method refused before admission; a guest of another image version refused `IMAGE_VERSION_MISMATCH`; every call signed, every reply verified), `services/modbit-guest/src/serve.rs` (a call whose named capability does not cover its body is `BAD_CALL`; an unauthenticated or replayed call refused).
- proof: `crates/sandbox` unit `a_guest_of_another_protocol_major_or_missing_a_method_is_refused_before_admission`; conformance steps `negotiated`, `unauthenticated_call_refused`, `replayed_call_refused` and the typed process/fs/PTY steps (`proc_*`, `pty`, `fs_dir_ops`) on the MicroVM and the reference backend; `the_guest_that_comes_up_must_be_the_one_the_manifest_names`.

## Limitations

- Git helpers, artifact transfer and browser endpoint discovery follow with M8.7 and M8.8.

## Verification

- Sealed with M8.5 on hosted CI run 35249710305 (`evidence/m8/M8.5/ci-run-35249710305.json`).

## Evidence

- `evidence.json` in this directory
- CI run json: `evidence/m8/M8.5/ci-run-35249710305.json`
