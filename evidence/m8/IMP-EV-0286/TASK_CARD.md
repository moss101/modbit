# Task Card — IMP-EV-0286 Authenticated tenant-bound Sandbox Gateway

## Identity

- Task ID: IMP-EV-0286
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0286 (docs/40); acceptance QUAL-EV-0286 — Cross-tenant sandbox handle use is denied and audited.
- Qualification: as named below, in the hosted `cloud` job (a real Firecracker MicroVM over KVM, a real Postgres) and on the three-OS `rust` job.
- Evidence tier: real-system

## Goal

Authenticated tenant-bound Sandbox Gateway: Cross-tenant sandbox handle use is denied and audited.

## Existing-code audit

- classification: MISSING before M8.3 (the gateway, guest and sandbox crates were M0 skeletons).
- production entry points: `apps/sandbox-gateway`: every request carries a worker bearer token (HMAC under the gateway's key) and names the tenant, session and task it acts for; a sandbox is provisioned only for the worker holding that session's lease at the presented generation (checked in the cloud store), recorded in `sandboxes` bound to its tenant; another tenant's use of a handle is `404` and audited on that tenant, another worker's `403`.
- proof: `qual_m8_3_the_gateway_binds_sandboxes_to_the_tenant_and_the_workers_session_lease` (no token and a bad token 401; a non-holder 403; a stale generation 409; another tenant naming the session 403 and audited; another tenant's call, read and destroy on the sandbox 404 and audited on that tenant; another worker of the tenant 403; a destroyed handle 410).

## Limitations

- Worker identity is a bearer token; mTLS is deployment configuration.

## Verification

- Sealed with M8.3/M8.4 on hosted CI run 35218388282 at 69b80b6 (`evidence/m8/M8.3/ci-run-35218388282.json`).

## Evidence

- `evidence.json` in this directory
- CI run json: `evidence/m8/M8.3/ci-run-35218388282.json`
