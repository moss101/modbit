# Task Card — IMP-EV-0290 Fine-grained filesystem/protected-path enforcement

## Identity

- Task ID: IMP-EV-0290
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0290 (docs/40); acceptance QUAL-EV-0290 — Attempt protected host/control path write fails.
- Qualification: as named below, in the hosted `cloud` job (a real Firecracker MicroVM over KVM, a real Postgres) and on the three-OS `rust` job.
- Evidence tier: real-system

## Goal

Fine-grained filesystem/protected-path enforcement: Attempt protected host/control path write fails.

## Existing-code audit

- classification: MISSING before M8.3 (the gateway, guest and sandbox crates were M0 skeletons).
- production entry points: The guest (`services/modbit-guest/src/serve.rs`) writes only under the workspace and never on a protected path (the control paths `/init`, `/etc`, `/proc`, `/sys`, `/dev`, `/run/modbit` always, plus the spec's under the workspace), after resolving the symlinks it can; reads only under the workspace and the readable roots; the policy is compiled in `crates/sandbox/src/policy.rs` (`write_allowed`, `read_allowed`).
- proof: Conformance steps `fs_protected_path_refused`, `fs_outside_workspace_refused`, `fs_traversal_refused`, `fs_control_path_unreadable` on the MicroVM and the reference backend; the gateway test's protected write refused through the gateway; unit `a_policy_has_no_network_unless_granted_and_protects_control_paths`, `paths_normalize_lexically_and_map_onto_the_host`.

## Limitations

- Enforced by the guest's own file operations in this build; mount-level pinning of protected paths against processes lands with M8.5.

## Verification

- Sealed with M8.3/M8.4 on hosted CI run 35218388282 at 69b80b6 (`evidence/m8/M8.3/ci-run-35218388282.json`).

## Evidence

- `evidence.json` in this directory
- CI run json: `evidence/m8/M8.3/ci-run-35218388282.json`
