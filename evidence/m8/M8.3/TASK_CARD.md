# Task Card — M8.3 Sandbox Gateway + sandbox substrate adapter

## Identity

- Task ID: M8.3
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: docs/21 "Sandbox substrate boundary", "`modbit-guest`"; docs/24 "Sandbox Gateway"; docs/33 "Sandbox Gateway"; docs/30 "Version compatibility" (guest/gateway negotiation); docs/11 trust boundaries; REQ-EV-0285 (MicroVM-class substrate behind a Modbit-owned gateway), REQ-EV-0286 (authenticated tenant-bound gateway), REQ-EV-0287 (deny-by-default network), REQ-EV-0290 (protected-path enforcement), REQ-EV-0291 (replaceable backend boundary); REQ-EV-0289 in part (versioned, capability-bound calls; the full typed process/fs/PTY set is M8.5).
- Qualification: the backend contract suite on a real Firecracker MicroVM over KVM (the hosted `cloud` job: pinned Firecracker v1.17.0, the Firecracker CI kernel 6.1.155, a guest image built in the job from the static musl `modbit-guest` over BusyBox) and on the reference backend on every OS; the gateway over a real Postgres binding sandboxes to the tenant and the worker's session lease.
- Evidence tier: real-system (a real MicroVM boots the real guest; vsock; a real Postgres).

## Goal

The tenant-authenticated boundary between Cloud Core Workers and the sandbox substrate: a worker holding a session's lease asks the gateway for a sandbox; the gateway compiles the task's entitlement into a policy, boots the guest through a replaceable backend, admits it with an ephemeral credential and relays typed, signed calls; the guest enforces the policy inside; a sandbox is bound to its tenant.

## Existing-code audit

- classification: MISSING before this task: `apps/sandbox-gateway` and `services/modbit-guest` refused to run (M0 skeletons); `crates/sandbox` carried no behavior; no guest protocol existed.
- production entry points: `crates/protocol/proto/modbit/v1/guest.proto` (the guest RPC), `crates/sandbox/src/{lib.rs,policy.rs,auth.rs,link.rs,conformance.rs,backend/{mod.rs,reference.rs,microvm.rs}}` (spec → compiled policy; the ephemeral credential and worker tokens; admission and signed calls; the backend contract suite; the Firecracker and reference backends), `services/modbit-guest/src/{main.rs,serve.rs,init.rs}` (the guest: init duties, hello/admit, authenticated calls, policy enforcement), `apps/sandbox-gateway/src/{lib.rs,routes.rs,main.rs}` (config, backend choice, the HTTP surface, lease and tenant checks, denial audit), `crates/event-store/src/cloud/{schema.rs,mod.rs}` (schema v3 `sandboxes`; `record_sandbox`, `sandbox`, `set_sandbox_state`), `tools/guest-image/build.sh` (the immutable guest root image), `.github/workflows/ci.yml` (the `cloud` job: Firecracker, kernel, BusyBox pinned and checksummed, KVM access, the musl guest, the image, the suite).
- proof: `apps/sandbox-gateway/tests/gateway.rs` — `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` (a real MicroVM: the guest answers on vsock, reports the MicroVM's kernel, runs BusyBox processes with exit codes, stdin, timeout, the output bound; the workspace image is there; a protected path, a path outside the workspace, a traversal and a control path are refused; the cloud metadata endpoint is unreachable — no interface; an unsigned and a replayed call are refused; destroy), `qual_m8_3_the_reference_backend_passes_the_backend_contract` (the same suite, `isolated: false`), `qual_m8_3_the_gateway_binds_sandboxes_to_the_tenant_and_the_workers_session_lease` (no token and a bad token `401`; a non-holder `403 LEASE_NOT_HELD`; the wrong generation `409 STALE_LEASE`; another tenant naming the session `403` and audited; the holder provisions `201` with the compiled policy; an exec and a read through the gateway; a protected write refused by the guest through the gateway; another tenant's call, read and destroy on the sandbox `404` and audited on that tenant; another worker of the tenant `403 NOT_HOLDER`; destroy; `410 SANDBOX_GONE` after). `crates/sandbox` unit: `a_guest_of_another_protocol_major_or_missing_a_method_is_refused_before_admission`, `a_call_verifies_under_its_credential_only_and_a_change_breaks_it`, `a_worker_token_verifies_until_it_expires`, `a_policy_has_no_network_unless_granted_and_protects_control_paths`, `paths_normalize_lexically_and_map_onto_the_host`.

## Limitations

- Egress grants are refused rather than served: the tap device, host NAT and per-destination firewall arrive with M8.6 (credential broker + egress policy); until then a guest has no network interface.
- Worker identity is a bearer token; mTLS between workers and the gateway is deployment configuration ahead of this build. Firecracker runs without the jailer.
- The guest image is built per CI run and pinned by the inputs' checksums (signed and versioned with M8.4). The Core's tools are not yet routed through the gateway (M8.5).

## Verification

Named tests (the hosted `cloud` job runs all three against Postgres and a KVM MicroVM; the three-OS `rust` job runs the reference suite and says `SKIPPED` for the other two):

- `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract`
- `qual_m8_3_the_reference_backend_passes_the_backend_contract`
- `qual_m8_3_the_gateway_binds_sandboxes_to_the_tenant_and_the_workers_session_lease`
- Unit (crates/sandbox): the five named above.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
