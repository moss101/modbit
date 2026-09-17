# Task Card — IMP-EV-0287 Deny-by-default sandbox/network isolation

## Identity

- Task ID: IMP-EV-0287
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0287 (docs/40); acceptance QUAL-EV-0287 — Guest cannot reach internal/control-plane endpoints by default.
- Qualification: as named below, in the hosted `cloud` job (a real Firecracker MicroVM over KVM, a real Postgres) and on the three-OS `rust` job.
- Evidence tier: real-system

## Goal

Deny-by-default sandbox/network isolation: Guest cannot reach internal/control-plane endpoints by default.

## Existing-code audit

- classification: MISSING before M8.3 (the gateway, guest and sandbox crates were M0 skeletons).
- production entry points: `crates/sandbox/src/policy.rs`: a `SandboxSpec` compiles to a policy with no network interface unless egress is granted; the MicroVM backend attaches no interface (and refuses an egress grant rather than serving it open until M8.6); the guest's filesystem, process count, output and timeout are bounded by the admitted policy.
- proof: `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` step `net_control_plane_unreachable`: a connect to the cloud metadata endpoint from inside the MicroVM fails (no route) — asserted of the isolating backend; unit `a_policy_has_no_network_unless_granted_and_protects_control_paths`.

## Limitations

- Explicit egress grants (tap device, host firewall) arrive with M8.6.

## Verification

- Sealed with M8.3/M8.4 on hosted CI run 35218388282 at 69b80b6 (`evidence/m8/M8.3/ci-run-35218388282.json`).

## Evidence

- `evidence.json` in this directory
- CI run json: `evidence/m8/M8.3/ci-run-35218388282.json`
