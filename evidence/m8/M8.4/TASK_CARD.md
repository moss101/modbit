# Task Card — M8.4 signed/versioned `modbit-guest`

## Identity

- Task ID: M8.4
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: docs/21 "Sandbox substrate boundary" (the guest image is immutable/versioned and contains no tenant secrets), "`modbit-guest`"; docs/30 "Version compatibility" (guest/gateway negotiation); docs/02 MOD-SBX-001 (no secrets in the guest image); REQ-EV-0285 (a hardened guest behind the Modbit-owned gateway), REQ-EV-0289 (versioned guest RPC: an unknown or stale version is rejected).
- Qualification: the hosted `cloud` job mints a publisher key, signs the root image and the reference guest it built, and boots the signed image on a real Firecracker MicroVM; a manifest under another key, a tampered image and a manifest naming another guest version are refused; the same refusals on the reference backend on every OS.
- Evidence tier: real-system (a real MicroVM boots the signed image; Ed25519 signatures verified by the gateway).

## Goal

A gateway boots only a guest image its publisher signed: the manifest verifies under a trusted key, the image on disk is the one it names (checked again at every boot), the kernel is the one it was built for, and the guest that comes up reports the version and protocol the manifest names.

## Existing-code audit

- classification: MISSING before this task: the guest image was whatever the configured path held; a gateway had no way to tell a tampered or stale image from its own.
- production entry points: `crates/sandbox/src/image.rs` (`ImageManifest`, `SignedManifest`, `sign`, `verify`, `check_image`, `check_guest`, `trusted_keys_from_env`), `crates/sandbox/src/backend/microvm.rs` (`MicrovmConfig.manifest`/`trusted_keys`; `MicrovmBackend::new` verifies, `provision` re-checks the hash; `image()`), `crates/sandbox/src/backend/reference.rs` (`ReferenceBackend::verified`), `crates/sandbox/src/backend/mod.rs` (`SandboxBackend::image`), `crates/sandbox/src/link.rs` (`GuestLink::admit_image`: `IMAGE_VERSION_MISMATCH` before the credential), `apps/sandbox-gateway/src/{lib.rs,routes.rs}` (configuration, the image on `/v1/health` and on every provisioned sandbox), `tools/guest-image/src/main.rs` (`guest-image-tool keygen | sign | verify`), `.github/workflows/ci.yml` (the per-run publisher key, the signed manifests, the verification).
- proof: `apps/sandbox-gateway/tests/gateway.rs` — `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` (the job-signed image boots; the guest's version is the manifest's; another key `IMAGE_UNVERIFIED`; a tampered copy `IMAGE_UNVERIFIED`; a manifest naming `0.0.0-stale` boots and is refused `IMAGE_VERSION_MISMATCH` at admission), `qual_m8_3_the_reference_backend_passes_the_backend_contract` (the same three refusals on the reference guest, every OS), `qual_m8_3_the_gateway_binds_sandboxes_to_the_tenant_and_the_workers_session_lease` (the gateway serves a verified reference guest; `/v1/health` and the provisioned sandbox name the image). `crates/sandbox` unit: `a_manifest_verifies_under_its_publisher_key_only_and_binds_the_image_bytes`, `the_guest_that_comes_up_must_be_the_one_the_manifest_names`.

## Limitations

- The publisher key in CI is minted per run; a release publisher key and its rotation are deployment configuration.
- The manifest binds the image, kernel and guest version; a transparency log or countersignature is not part of this build.

## Verification

Named tests (the hosted `cloud` job; the three-OS `rust` job runs the reference cases):

- `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` (M8.4 assertions inside)
- `qual_m8_3_the_reference_backend_passes_the_backend_contract` (M8.4 assertions inside)
- `qual_m8_3_the_gateway_binds_sandboxes_to_the_tenant_and_the_workers_session_lease`
- Unit: `a_manifest_verifies_under_its_publisher_key_only_and_binds_the_image_bytes`, `the_guest_that_comes_up_must_be_the_one_the_manifest_names`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-35218388282.json` (hosted run 35218388282 at 69b80b6, all 14 jobs green on attempt 2 — the first attempt lost `qual_m4_2_e2e_006` to a slow Windows runner, unrelated to this work, and passed on re-run)
