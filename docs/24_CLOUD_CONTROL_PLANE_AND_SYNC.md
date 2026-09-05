# Cloud Control Plane, Remote Execution, and Sync

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Purpose

Cloud exists for isolated MicroVM execution, remote continuation, durable cross-device task access and team policy—not as a separate “brain.” The same Core runtime/domain code executes in cloud workers.

## Services

### Cloud API
Rust service exposing authenticated HTTPS control endpoints and WSS/SSE-style event stream. Responsibilities: account/tenant auth, session directory, remote run create/stop/steer, artifact access grants, signed execution-policy/registry/statistics bundle distribution, provider policy lookup and worker lease coordination.

### Cloud Core Worker
Hosts one or more session kernels subject to capacity. Acquires fenced session lease from Postgres before processing events. Executes Agent Runtime and calls Sandbox Gateway.

### Postgres
Authoritative cloud metadata/event/projection store. Event rows are append-only; projections are transactionally updated. Worker queue initially uses durable Postgres lease/`SKIP LOCKED` patterns to avoid introducing a separate queue before needed.

### Object storage
S3-compatible encrypted bucket for checkpoints, OutputRefs, browser evidence, artifacts and large event payloads. Keys are tenant/session scoped and content-hashed.

### Sandbox Gateway
Owns mapping from authenticated tenant/session/task to sandbox substrate lease and guest capability tokens.

## Identity

Desktop cloud sign-in uses OIDC authorization-code + PKCE in the system browser/deep link flow. Cloud API issues short-lived access token + rotating refresh token. Enterprise SSO maps to same User/Tenant model.

## Sync model

Core event streams use cursor/sequence. Desktop caches cloud projections but never writes them as authority. On reconnect:
1. send last acknowledged cursor;
2. replay missing events;
3. if cursor expired or projection schema changed, fetch full snapshot + new cursor;
4. resolve local pending commands by idempotency keys.

Workspace files are not continuously CRDT-synced. Remote coding operates on explicit Git/checkpoint handoff bundles; this avoids a second source of truth.

## Multi-tenancy

Every DB table/object/sandbox lease carries TenantId. API authorization verifies tenant ownership before dereference. Object store uses per-tenant prefixes plus signed short-lived URLs. Cross-tenant tests are mandatory.

## Offline behavior

Local trusted tasks can run without the Modbit cloud account plane when provider configuration and required assets are available. Cloud-specific fleet sync/remote continuation is unavailable offline but does not break local Core persistence. Routing continues offline from the last verified, fresh, compatible policy/registry/statistics bundle; when that bundle is outside its freshness window Core reports `REGISTRY_STALE` and uses the configured conservative fallback rather than inventing eligibility (docs 38/71).

## Execution policy distribution and routing telemetry

The control plane is the publication authority for execution policy; Core is the enforcement point. The Cloud API distributes **signed, versioned, tenant-scoped bundles** containing the PolicyEnvelope rules and organization controls (permitted objective modes, model/provider families, residency, budgets, reasoning-effort ceilings, mandatory-review surfaces, escalation and pin permissions, telemetry retention, label visibility), the mode quality floors and confidence requirements (tau/delta per objective mode), the Model Registry generation, the Outcome Statistics `stats_version`, and the compiler, gate and realized-risk versions. Each bundle carries a freshness window and compatibility range; Core verifies signature, freshness and schema compatibility before compiling or activating any plan, and a Cloud Core Worker runs exactly the same conditional transaction as a local Core.

Routing telemetry flows the other way. `RoutingDecisionRecord`, `OutcomeRecord`, `AcceptanceGateResult`, `RealizedRisk` and `ReviewerResult` references are stored as ordinary tenant-scoped events and artifacts in Postgres and the object store, redacted of secrets and of solver hidden reasoning, and retained per organization policy. The Offline Policy Lab reads only tenant-permitted, sanitized aggregates; statistics and registry data never grant permissions, and clients never receive routing weights, statistics or provider credentials. Reference: `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md`, `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md`; proof EPR-001/009/010/011/012 in `49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md`.
