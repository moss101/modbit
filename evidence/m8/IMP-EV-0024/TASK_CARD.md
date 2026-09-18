# Task Card — IMP-EV-0024 Private worker reverse-connect

## Identity

- Task ID: IMP-EV-0024
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0024 (disposition EXPERIMENT: evaluate an outbound worker connection for NAT/private networks behind the same authenticated worker protocol); QUAL-EV-0024 (no inbound ports; identity, reconnect, revocation and tenant isolation proven); docs/24 (the worker's outbound link, as built for M8.8).
- Qualification: the worker's link to the Cloud API, exercised on real components: a worker under another key, a worker under the API's key with a short-lived token, the link dropped by the API, the token expired, a request over the link for a session the worker does not hold.
- Evidence tier: real-system.

## Goal

Evaluate whether a worker can serve the control plane with no inbound port — connecting outbound under the worker token the gateway already verifies — and whether that connection carries identity, survives drops, honours revocation and never crosses tenants.

## Existing-code audit

- classification: MISSING before M8.8; built there as the transport the cloud browser's view needed (`apps/cloud-worker/src/link.rs`, `apps/cloud-api/src/browser_view.rs`: `GET /v1/workers/link`), sealed here as the experiment's qualification.
- production entry points: `apps/cloud-worker/src/link.rs` (one outbound WebSocket per worker, `Authorization: Bearer <worker token>`, reconnect with backoff 0.5 s → 10 s, every request checked against the sessions this worker hosts — `NOT_HOSTED` otherwise), `apps/cloud-api/src/browser_view.rs` (`WorkerLinks`: one link per worker id, the token verified under `MODBIT_CLOUD_WORKER_KEY_HEX` with its expiry, `disconnect` — an operator's revocation of the live link; the API asks only the worker that holds a session's lease, `held_by`).
- proof: `apps/cloud-worker/tests/cloud_worker.rs::qual_ev_0024_the_workers_outbound_link_proves_identity_reconnect_revocation_and_tenant_isolation` — a worker whose token is under another key retries `401 Unauthorized` and never links; a worker under the API's key links from its outbound connection; over that link a request for a foreign session is `REJECTED NOT_HOSTED`; the API drops the link and the worker comes back on its own (a new link object); once the token has expired a dropped link does not come back (`401` on every retry) — identity, reconnect, revocation, tenant isolation. The measurable benefit is M8.8's: the person's view of a cloud browser streams from a worker with no inbound port (`qual_m8_8_…`).

## Limitations

- An experiment, not a promoted decision: the link carries the browser view, input and control hand-overs only; commands, the log and leases still travel through the store (docs/24). Promotion of the link to the worker's control channel is a decision-record question (docs/02), not taken here.
- Revocation is by token expiry and by dropping the live link; there is no revocation list — a long-lived token stays valid until it expires.
- The "VPC with no inbound ports" is asserted by construction (the worker binds no listener) and exercised on one host; a real private-network deployment is not part of this qualification.

## Verification

- `qual_ev_0024_the_workers_outbound_link_proves_identity_reconnect_revocation_and_tenant_isolation`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
