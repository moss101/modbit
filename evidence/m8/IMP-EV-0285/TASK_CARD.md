# Task Card — IMP-EV-0285 Common cloud Linux MicroVM substrate

## Identity

- Task ID: IMP-EV-0285
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0285 (docs/40); acceptance QUAL-EV-0285 — Real cloud sandbox boots fixture and passes backend conformance.
- Qualification: as named below, in the hosted `cloud` job (a real Firecracker MicroVM over KVM, a real Postgres) and on the three-OS `rust` job.
- Evidence tier: real-system

## Goal

Common cloud Linux MicroVM substrate: Real cloud sandbox boots fixture and passes backend conformance.

## Existing-code audit

- classification: MISSING before M8.3 (the gateway, guest and sandbox crates were M0 skeletons).
- production entry points: The MicroVM backend (`crates/sandbox/src/backend/microvm.rs`) drives Firecracker over KVM through its API socket: the gateway's kernel and immutable, signed root image (`modbit-guest` as `/init` over a static BusyBox userland, `tools/guest-image/build.sh`), a per-sandbox workspace ext4 image, vsock. Behind the Modbit-owned gateway and the `SandboxBackend` boundary; nothing of Firecracker leaks past the adapter (docs/35 as built).
- proof: `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` (the hosted `cloud` job: a real MicroVM boots the real guest image and passes every step of the backend contract suite over vsock; the guest reports the MicroVM's kernel).

## Limitations

- Egress grants are refused until M8.6; no jailer; the CI kernel and Firecracker release are pinned by checksum.

## Verification

- Sealed with M8.3/M8.4 on hosted CI run 35218388282 at 69b80b6 (`evidence/m8/M8.3/ci-run-35218388282.json`).

## Evidence

- `evidence.json` in this directory
- CI run json: `evidence/m8/M8.3/ci-run-35218388282.json`
