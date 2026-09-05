# Database and Storage Schema

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Local storage

Use SQLite in WAL mode with `foreign_keys=ON`, `synchronous=FULL` for authoritative event/protocol/effect commits, explicit schema migrations and periodic integrity check. Large blobs use a content-addressed object directory keyed by SHA-256.

Recommended logical database split is **not** one SQLite file per feature. Use two durable DBs to reduce cross-file transaction complexity:
- `core.db` — identity, sessions, tasks, events, projections, protocol state, checkpoints, compaction metadata, memory metadata, policy/effects.
- `index.db` — repository/index metadata, symbol/dependency graph, embedding generation references, diagnostics/test mapping.

Actual content indexes (Tantivy/USearch) are external versioned files referenced by `index.db`.

## Core tables

### `sessions`
`session_id PK, tenant_id, user_id, space_id, state, generation, created_at, updated_at, current_task_id, last_event_sequence`.

### `tasks`
`task_id PK, session_id FK, goal_text, workspace_id, base_revision, execution_profile, policy_profile_id, state, generation, created_at, started_at, completed_at, failure_code`.

### `runs`
`run_id PK, task_id FK, attempt, owner_location, kernel_lease_generation, state, started_at, ended_at`.

### `turns`
`turn_id PK, run_id FK, ordinal, state, model_route_json, tool_projection_hash, context_pack_id, started_at, ended_at`.

### `events`
`event_id PK, session_id, aggregate_type, aggregate_id, sequence, event_type, schema_version, occurred_at, actor_type, actor_id, causation_id, correlation_id, payload_inline, payload_object_hash, integrity_hash`.
Unique `(aggregate_id, sequence)`.

### `run_steps`
`step_id PK, turn_id, step_type, state, ordinal, started_at, ended_at, input_ref, output_ref, failure_code`.

### `tool_calls`
`tool_call_id PK, step_id, tool_name, tool_version, effect_class, capability_lease_id, status, arguments_hash, dispatched_at, completed_at, result_ref, unknown_outcome_reason`.

### `approvals`
`approval_id PK, task_id, tool_call_id, intent_hash, scope_json, status, requested_at, resolved_at, resolver_user_id, expires_at`.

### `capability_leases`
`lease_id PK, tenant_id, task_id, agent_id, resource_json, operations_json, effect_ceiling, execution_profile, generation, expires_at, revoked_at`.

### `effect_receipts`
`effect_id PK, previous_receipt_hash, task_id, turn_id, step_id, tool_call_id, capability_lease_id, intent_hash, policy_decision, approval_id, execution_target, evidence_ref, status, occurred_at, receipt_hash`.

### `protocol_state`
Keyed by `session_id + protocol_key`; stores typed JSON/protobuf payload and generation for pending tool/approval/question/subagent/terminal/browser/sandbox lifecycle.

### `checkpoints`
`checkpoint_id PK, task_id, epoch, base_checkpoint_id, workspace_revision, manifest_object_hash, git_state_json, runtime_state_ref, index_generation, created_at, status, integrity_hash`.
Unique `(task_id, epoch)`.

### `compaction_epochs`
`epoch_id PK, session_id, branch_generation, source_event_start, source_event_end, previous_epoch_id, compiler_version, target_tokens, status, result_object_hash, created_at, committed_at`.

### `memory_items`
`memory_id PK, scope_type, scope_id, type, content_object_hash, source_ref, confidence, sensitivity, ttl_at, revision_binding, supersedes_id, state, created_at, validated_at`.

### `repair_attempts`
`attempt_id PK, run_id FK, turn_id, attempt_ordinal, failure_signature_hash, failure_signature_ref, hypothesis_hash, hypothesis_ref, evidence_refs_json, intended_fix_ref, change_ref, verification_result_ref, outcome, created_at`.
Unique `(run_id, failure_signature_hash, hypothesis_hash)` makes an equivalent repeated hypothesis unrepresentable as a new attempt; the runtime escalates instead (`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md`). A `change_fingerprint` column with unique `(run_id, change_fingerprint)` makes an identical repeated change unrepresentable as a new attempt. Plans and self-reviews persist as WorkGraph state and `run_steps` rows with object refs.

### `verification_runs`
`verification_run_id PK, run_id FK, plan_ref, stage, candidate_revision, environment_digest, started_at, ended_at, status, report_ref`.
`stage` is `BASELINE`, `TARGETED`, `COMPLETION` or `RERUN` (`64_VERIFICATION_EXECUTION_CONTRACTS.md`). Reports and raw runner output are content-addressed artifacts.

### `check_results`
`verification_run_id FK, check_id, kind, status, duration_ms, location_ref, error_class, message_fingerprint, output_ref`.
Unique `(verification_run_id, check_id)`. Regression attribution joins BASELINE and COMPLETION rows of the same `run_id` on `check_id`.

### `flaky_checks`
`flaky_id PK, run_id FK, check_id, first_run_id, rerun_id, quarantined_at, scope_revision_range, expires_reason, resolved_at`.
Only the Verification Engine's rerun protocol writes this table; there is no global skip list.

### `output_refs`
`output_ref_id PK, object_hash, content_type, byte_length, checksum, preview_text, created_at, retention_class`.

### `artifacts`
`artifact_id PK, task_id, kind, object_hash, path_hint, mime_type, provenance_event_id, created_at`.

## Index metadata tables

`repositories, workspace_revisions, files, chunks, symbols, symbol_refs, dependency_edges, git_changes, diagnostics, test_links, index_generations, embedding_generations`.

Each chunk stores path, byte/line span, content hash, language, AST anchor and embedding generation. No chunk content duplication is required when recoverable from immutable workspace snapshot/object hash.

## Cloud schema

Postgres mirrors canonical session/task/event/protocol/effect structures with `tenant_id` present on every tenant resource and row-level application authorization. Postgres event append and projection update occur in one transaction. Object payloads are stored in encrypted S3-compatible storage.

## Retention

- Event/protocol/effect data: durable until explicit account/enterprise retention policy deletion.
- Terminal/browser raw output: configurable, default bounded retention with evidence-critical refs retained.
- Checkpoint blobs: keep rolling recent + task-final checkpoint; deduplicate by content hash.
- Memory: policy/TTL controlled and user inspectable.

## Migration safety

Every migration has forward and rollback/read-compatibility plan. Migrations run against a copied production-like fixture DB in CI. Core never auto-drops unknown columns/tables. Startup refuses write mode if a newer incompatible DB schema is detected.

## Execution policy persistence in existing stores

Extend `runs` additively with profile/envelope/active_plan refs, active_leg_id, routing_epoch, escalation/revision state, gate/reviewer/risk refs and accounting state. Add tenant-scoped logical tables to existing core.db (mirrored in cloud Postgres): `routing_plans(plan_id, run_id, version_refs, input_digest, base_revision, state, object_ref)`, `routing_attempts(attempt_id, plan_id, leg_id, provider_request_id, state, usage_ref)`, `routing_budget_reservations(reservation_id, request_id, plan_id, currency, scale, reserved, spent, uncertain, generation)`, `routing_outcomes(outcome_id, request_id, plan_id, raw_signal_refs, reward_version, object_ref)` and `routing_config_versions(kind, version, digest, signature_ref, compatibility_ref, activation_event_id)`.

Primary keys are immutable; foreign keys include tenant scope and parent identity. Unique provider-usage source/event identity prevents duplicate accounting; keep cumulative usage semantics explicit. Plan activation, request/daily reservation, event append and Run projection commit transactionally. Concurrent compilers cannot spend the same remaining quota. Attempts persist before provider dispatch; late/unknown charges keep conservative reservations until settled. Signed configuration blobs and large traces use the existing object store, never a new routing database.

Extend protocol/checkpoint payloads with pending plan/leg/attempt, route epoch, candidate revision and reservation refs. Migration preserves existing event IDs and null legacy provenance rather than inventing past costs/versions. Migration failure is recoverable and qualification uses actual pre-plan SQLite data plus killed Core processes. Retention applies to raw signals and profiles; deletion redacts/removes permitted payloads without converting missing observations into success or corrupting required audit/effect references. See EPR-001/010 in docs 49/61.

## v1.1 statistics, slots and independent outcomes

Extend existing routing plan/attempt/protocol projection tables with immutable transaction/slot table, trigger/predecessor/activation ordinal/maximum count, validation digest, required assurance, acceptance and risk refs, and independently pinned stats_version. Unique `(plan_id, slot_id, activation_ordinal)` makes slot dispatch idempotent; activation/event/projection/reservation commit atomically. Reviewer environment lifecycle metadata belongs to existing execution/protocol owners, never another scheduler.

Outcome Statistics is a derived versioned dataset under eval-bench backed by existing content-addressed artifacts and storage metadata: `routing_statistics_versions(stats_version, tenant_scope, source_digest, partition_digest, calibration_version, artifact_ref, created_at, retention_class)`. It is not a separate database/event/memory authority. Aggregates have complete joint keys and samples/confidence; schema migration moves empirical records out of model configuration without changing original sources or inventing missing intervals. Invalid/absent data remains a low-confidence prior or explicit infeasible outcome.

Request/leg/gate observations retain separate foreign keys and outcome semantics. Store acceptance/risk ground-truth corrections separately; successful escalation must not overwrite failed initial outcome. Version compatibility and retention scope constrain derived statistics. Migration/restart must preserve slot counts, pending evidence, actual spend and unknown charges; reviewer scratch is disposable, while trusted bounded evidence exports remain ordinary artifacts.
