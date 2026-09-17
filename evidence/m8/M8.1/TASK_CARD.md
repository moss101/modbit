# Task Card — M8.1 Cloud API/Postgres/object store

## Identity

- Task ID: M8.1
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: docs/24 "Cloud API", "Postgres", "Object storage", "Identity", "Sync model", "Multi-tenancy"; docs/30 "Cloud HTTP control API"; docs/31 "Cloud schema"; docs/33 "Cloud API implementation", "Idempotency"; docs/36 (DEPEND: Postgres; INTEGRATE: S3-compatible); REQ-EV-0076 (typed RPC; the Core stays the session owner), REQ-EV-0109 (one canonical execution interface with local and cloud adapters — the cloud log keeps the local store's envelope, sequence and hash chain).
- Qualification: the real Cloud API over a real Postgres 16 and a real S3-compatible store (MinIO) in the hosted `cloud` job — tokens and rotation, idempotent commands, cursor replay, the stream's replay-then-live, approvals bound to intent, ranged outputs and signed artifact URLs, tenants never crossing (audited), fenced leases, all-or-nothing appends, rate limits.
- Evidence tier: real-system (Postgres 16 and MinIO in containers; the API bound on a real socket; WebSocket over the real transport)

## Goal

The cloud's authoritative store and control surface: every tenant's canonical log, projections and commands in Postgres, blobs in an S3-compatible bucket, an authenticated HTTPS/WSS API that writes commands and events and streams what committed — never running model or tool code.

## Existing-code audit

- classification: MISSING before this task: `apps/cloud-api` refused to run (M0 skeleton); no cloud store, schema, principals or streaming existed.
- production entry points: `crates/event-store/src/cloud/{mod.rs,schema.rs}` (`CloudStore`: migrations under an advisory lock; tenants, principals, refresh tokens; `append` — envelopes, sequence and chain hash exactly as the local store, projections by the domain state machines, the command ledger row and `pg_notify` in one transaction; `events_after` by session cursor; `claim_ready_session` / `renew_lease` / `release_lease` with `SKIP LOCKED` and strictly increasing generations; `put_object` / `get_object_range` / `signed_get_url` per tenant by content hash; `record_denial`), `apps/cloud-api/src/{lib.rs,routes.rs,stream.rs,auth.rs,rate.rs,main.rs}` (the router, the bearer and request-id layers, the token bucket, every docs/30 route but `/v1/handoffs`, the WSS stream), `crates/domain/src/task.rs` (`TaskPauseRequested`, `TaskResumeRequested`, `TaskCancelRequested` — durable control requests for the execution owner), `.github/workflows/ci.yml` (the `cloud` job: Postgres 16 and MinIO containers, the bucket, the tests asserted not skipped).
- proof: `apps/cloud-api/tests/cloud_api.rs` — `qual_m8_1_tokens_are_issued_from_a_secret_and_refresh_rotates_once` (a spent refresh token is a replay that revokes the family; an unknown secret and a bad bearer are refused before any handler), `qual_m8_1_commands_are_idempotent_events_replay_by_cursor_and_tenants_never_cross` (a retried command replays its outcome with `x-modbit-replayed`, one session and one task exist; three events replay by cursor with a 64-hex chain hash; tenant B is refused `NOT_FOUND` on A's session, task and events and the denials are on B's audit; steer, pause and cancel are durable events; a terminal task refuses a second cancel; a bad payload records nothing), `qual_m8_1_the_stream_replays_after_the_cursor_then_delivers_live_commits` (after cursor 1 the two task events then `caught_up`, a live steer arrives at offset 4, a reconnect at 4 sees nothing twice), `qual_m8_1_approvals_bind_to_intent_and_outputs_read_by_range_within_the_tenant` (an approval opened by the owner: the wrong intent is `INTENT_MISMATCH` and its command replays the same, another tenant finds nothing, the right intent is `APPROVED`, a second decision `APPROVAL_NOT_OPEN`; an object read by `?start&len` and by `Range:` with `Content-Range`; another tenant is refused; the artifact grant's signed URL fetches the bytes without credentials), `qual_m8_1_leases_fence_by_generation_and_an_append_is_all_or_nothing` (one worker claims at generation 1, a second gets nothing, the holder renews, a non-holder cannot, a release lets the next claim take generation 2 and fences the old holder; a cancel while a worker holds the session is relayed `202 PENDING` to that worker and a retry replays it, the owner's completion is readable at `GET /v1/commands/{id}`; a fenced owner's mirror is `StaleLease`, a hash that does not continue the chain is refused; an invalid transition and a stale expectation write nothing), `qual_m8_1_a_principal_over_its_budget_is_refused_rate_limited`.

## Limitations

- Identity is a principal secret → token pair; the OIDC authorization-code + PKCE desktop sign-in of docs/24 is not wired (no IdP in this build).
- Postgres connections are plain (`NoTls`); TLS to the database is configuration work before any deployment.
- `/v1/handoffs` (M8.7) follows; a task created while no worker holds the session waits `QUEUED` with the session marked ready for a worker (M8.2), and a command for a held session is relayed to its worker rather than applied by the API.
- Objects without a bucket configured are kept in Postgres (development only, marked as such).

## Verification

Named tests (the hosted `cloud` job on ubuntu-latest runs them against Postgres 16 and MinIO; on the three-OS `rust` job they say `SKIPPED` and pass):

- `qual_m8_1_tokens_are_issued_from_a_secret_and_refresh_rotates_once`
- `qual_m8_1_commands_are_idempotent_events_replay_by_cursor_and_tenants_never_cross`
- `qual_m8_1_the_stream_replays_after_the_cursor_then_delivers_live_commits`
- `qual_m8_1_approvals_bind_to_intent_and_outputs_read_by_range_within_the_tenant`
- `qual_m8_1_leases_fence_by_generation_and_an_append_is_all_or_nothing`
- `qual_m8_1_a_principal_over_its_budget_is_refused_rate_limited`
- Unit: `a_token_verifies_under_its_key_only_and_expires`, `a_bucket_admits_its_capacity_then_refuses_until_refilled`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
