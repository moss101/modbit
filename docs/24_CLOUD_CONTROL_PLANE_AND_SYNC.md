# Cloud Control Plane, Remote Execution, and Sync

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Purpose

Cloud exists for isolated MicroVM execution, remote continuation, durable cross-device task access and team policy—not as a separate “brain.” The same Core runtime/domain code executes in cloud workers.

## Services

### Cloud API
Rust service exposing authenticated HTTPS control endpoints and WSS/SSE-style event stream. Responsibilities: account/tenant auth, session directory, remote run create/stop/steer, artifact access grants, signed execution-policy/registry/statistics bundle distribution, forge webhook intake (issue-to-task and PR events, PX-011), provider policy lookup and worker lease coordination.

### Cloud Core Worker
Hosts one or more session kernels subject to capacity. Acquires fenced session lease from Postgres before processing events. Executes Agent Runtime and calls Sandbox Gateway.

As built (M8.2): `apps/cloud-worker` (`modbit-cloud-worker`) claims ready sessions from the cloud store up to its capacity (`claim_ready_session`, `SKIP LOCKED`, the generation strictly increasing; never a session a host in the same process is still winding down) and hosts each in its own process of the real `modbit-core` (`--tenant-id <tenant> --tether-stdin`, its data directory per session), so the session runs under exactly the Core the local profile runs — the same runtime, tools, policy and journal. Hosting a session: the provider the worker is configured with goes to the Core's memory only (`ConfigureProvider`; the key is never journaled, logged or stored); the cloud log is materialized into the Core verbatim (`ImportMirroredEvents`: the cloud's envelopes with their sequences and integrity hashes, chain-checked on the way in; what the Core already holds is skipped, never duplicated); the Core's local session lease is taken naming the cloud lease's generation; then, until stopped or fenced, the worker mirrors every event the Core journals to the cloud log (`ReadMirrorEvents` → `CloudStore::mirror`, which admits an event only from the lease's holder at its generation and only when it continues its aggregate's chain), executes the commands the API relayed to it and completes them after the mirror (a caller that sees a command's outcome sees its events on the cloud log too), trusts the workspace a principal named for a task once (journaled, so the successor inherits it from the log) and starts queued tasks. A heartbeat task renews the lease every third of its lifetime from the moment of the claim; a refused renewal (expired, or taken by a successor) fences the owner: it stops its Core and writes nothing more. A stop releases the lease with the session ready for the next owner; a successor claims at the next generation, materializes the same log into a fresh Core and resumes from it, adding only its own lease event. As built (M8.5): the worker hands each Core it hosts the Sandbox Gateway it is configured with (`MODBIT_SANDBOX_GATEWAY_URL`, `MODBIT_SANDBOX_WORKER_TOKEN` → `ConfigureSandboxGateway`, with the cloud lease generation it holds), and the Core provisions one sandbox per `cloud_isolated` task when its run starts and destroys it when the task ends; the task's tools act inside it (docs/21 "Execution profiles" as built).

### Postgres
Authoritative cloud metadata/event/projection store. Event rows are append-only; projections are transactionally updated. Worker queue initially uses durable Postgres lease/`SKIP LOCKED` patterns to avoid introducing a separate queue before needed.

### Object storage
S3-compatible encrypted bucket for checkpoints, OutputRefs, browser evidence, artifacts and large event payloads. Keys are tenant/session scoped and content-hashed.

### Sandbox Gateway
Owns mapping from authenticated tenant/session/task to sandbox substrate lease and guest capability tokens.

As built (M8.3): `apps/sandbox-gateway` (`modbit-sandbox-gateway`, axum) — a worker authenticates with a bearer token signed under the gateway's key (`mbw_…`: worker id, expiry); every request names the tenant, session and task it acts for, and a sandbox is provisioned only when the cloud store's lease says this worker holds that session at the generation it presents (`LEASE_NOT_HELD`, `STALE_LEASE` otherwise, the attempt audited on the requesting tenant). The gateway compiles the spec, provisions through the configured backend (`microvm`, or `reference` when explicitly configured — and then every sandbox it issues says `isolated: false`), admits the guest, records the sandbox in the store (`sandboxes`: tenant, session, task, worker, lease generation, backend, isolation, state) and keeps the admitted link; calls (`health`, `exec`, `fs.read`, `fs.write`, `net.probe`) and destroy go through it. A sandbox addressed by another tenant is `404` and audited on that tenant; by another worker of the same tenant, `403 NOT_HOLDER`; after destroy, `410 SANDBOX_GONE`. The guest credential never leaves the gateway.

As built (M8.1): the Cloud API is `apps/cloud-api` (`modbit-cloud-api`, axum over the cloud store) and the cloud store is `crates/event-store::cloud` (feature `cloud`; the local Core never links it): Postgres holds the tenant-scoped canonical log (`events`: the same envelope, per-aggregate sequence and integrity-hash chain as the local store, plus a per-session `session_offset` that is the resume cursor), the projections the API serves (`sessions`, `tasks`, `approvals`, each the domain object as JSONB with its state indexed, updated by the domain state machines in the same transaction as the append), the command ledger (`commands`: a retried `command_id` answers with the recorded outcome and appends nothing — rejections too), generation-fenced `session_leases` a worker claims with `FOR UPDATE SKIP LOCKED` (docs/33), `principals` with hashed secrets, rotating `refresh_tokens` (a replay revokes the family), `objects` (content-hashed, keyed `tenants/<tenant>/objects/<hash>` in the S3-compatible bucket) and `denials` (every cross-tenant dereference is not found and audited). Committed appends `pg_notify` the API, which fans them out to the WSS streams. Identity in this build: a principal's secret buys a short-lived HMAC-signed access token (verified without the store) and a rotating refresh token; OIDC sign-in for the desktop is not yet wired. Proven on a real Postgres 16 and a real MinIO (`apps/cloud-api/tests/cloud_api.rs`, the hosted `cloud` job): tokens and rotation, idempotent commands, cursor replay, the stream's replay-then-live with a resume cursor, approvals bound to intent, ranged output reads and a signed artifact URL fetched without credentials, tenants never crossing (audited), leases fencing by generation, appends all-or-nothing, per-principal rate limits.

## Identity

Desktop cloud sign-in uses OIDC authorization-code + PKCE in the system browser/deep link flow. Cloud API issues short-lived access token + rotating refresh token. Enterprise SSO maps to same User/Tenant model.

## Sync model

Core event streams use cursor/sequence. Desktop caches cloud projections but never writes them as authority. On reconnect:
1. send last acknowledged cursor;
2. replay missing events;
3. if cursor expired or projection schema changed, fetch full snapshot + new cursor;
4. resolve local pending commands by idempotency keys.

Workspace files are not continuously CRDT-synced. Remote coding operates on explicit Git/checkpoint handoff bundles; this avoids a second source of truth.

As built (M8.2): the cloud log and the worker's local Core log are one log, mirrored in both directions by session offset: cloud → Core when a worker takes a session (`ImportMirroredEvents`, resuming from the worker's saved cursor; the Core's `import_envelopes` stores each envelope as it was, checks the chain and refuses a fork), Core → cloud while it hosts (`ReadMirrorEvents` after the export cursor → `mirror`). Cursors are the store's session offsets on both sides; nothing is renumbered, so every consumer of either log — the desktop, the API's streams, a successor worker — sees the same event ids, sequences and hashes. Commands a client sends to the API for a session a worker holds are relayed (`202 PENDING`, `relayed_to` the worker, `GET /v1/commands/{id}` for the outcome) rather than applied by the API, which keeps the owning Core the session's only writer; the API applies a command itself only when no worker holds the session (a task is created queued and the session marked ready).

As built (M8.7): a handoff is the same log crossing tenants once. The laptop's bundle carries its session's log verbatim; the API imports it under the cloud tenant with the origin tenant on every envelope and the chain checked (`import_handoff`), so the cloud log of the session begins with the laptop's events — same ids, sequences and hashes — and the worker's mirror (`ImportMirroredEvents` with `admitted_handoff`) carries them into its Core unchanged. Everything after the admission (`TaskHandoffAdmitted`, the rebind, the sandbox, the continuation) is the cloud tenant's; the laptop's `TaskHandedOff` stays on the laptop's log, after the bundle. Workspace files cross once, as the checkpoint and the git bundle the worker materializes; nothing is synced back — the continuation's results are on the cloud log and in the cloud workspace.

As built (M8.8): a worker keeps one outbound link to the Cloud API (`GET /v1/workers/link`, a WebSocket it opens with its worker token, reconnecting with backoff; a worker needs no inbound port). Over it the API reaches the worker's hosted sessions for what the log cannot carry — the live view of a cloud browser (frames worker → API, fanned out to the persons watching), the person's input and control hand-overs (API → worker, answered with the outcome from the session's own Core). Every request names a session the worker must hold; the link carries no secret and stores nothing.

## Multi-tenancy

Every DB table/object/sandbox lease carries TenantId. API authorization verifies tenant ownership before dereference. Object store uses per-tenant prefixes plus signed short-lived URLs. Cross-tenant tests are mandatory.

## Offline behavior

Local trusted tasks can run without the Modbit cloud account plane when provider configuration and required assets are available. Cloud-specific fleet sync/remote continuation is unavailable offline but does not break local Core persistence. Routing continues offline from the last verified, fresh, compatible policy/registry/statistics bundle; when that bundle is outside its freshness window Core reports `REGISTRY_STALE` and uses the configured conservative fallback rather than inventing eligibility (docs 38/71).

## Execution policy distribution and routing telemetry

The control plane is the publication authority for execution policy; Core is the enforcement point. The Cloud API distributes **signed, versioned, tenant-scoped bundles** containing the PolicyEnvelope rules and organization controls (permitted objective modes, model/provider families, residency, budgets, reasoning-effort ceilings, mandatory-review surfaces, escalation and pin permissions, telemetry retention, label visibility), the mode quality floors and confidence requirements (tau/delta per objective mode), the Model Registry generation, the Outcome Statistics `stats_version`, and the compiler, gate and realized-risk versions. Each bundle carries a freshness window and compatibility range; Core verifies signature, freshness and schema compatibility before compiling or activating any plan, and a Cloud Core Worker runs exactly the same conditional transaction as a local Core.

Routing telemetry flows the other way. `RoutingDecisionRecord`, `OutcomeRecord`, `AcceptanceGateResult`, `RealizedRisk` and `ReviewerResult` references are stored as ordinary tenant-scoped events and artifacts in Postgres and the object store, redacted of secrets and of solver hidden reasoning, and retained per organization policy. The Offline Policy Lab reads only tenant-permitted, sanitized aggregates; statistics and registry data never grant permissions, and clients never receive routing weights, statistics or provider credentials. Reference: `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md`, `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md`; proof EPR-001/009/010/011/012 in `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md`.
