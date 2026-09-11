# Task Card — EPR-002 Extend the Model Registry with current role bindings

## Identity

- Task ID: EPR-002
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-EPR-002; owner label: model-gateway; subsystem: model-gateway
- Qualification: `QUAL-EPR-002` / `EPR-E2E-002` — activate signed configuration through the real Gateway; assert the registry has no empirical workflow outcome fields, that current capabilities and data rules filter bindings, and that ingestion rejects embedded empirical fields; keep an external `stats_version` reference without producing estimates; retain live provider and generation proof. Negative: `EPR-FI-002` — tamper the signature, expire freshness, remove the floor or the reviewer, and revoke a model mid-Run; a prevalidated eligible fallback or an explicit failure, and no client secret exposure.
- Evidence tier: real-system or production-equivalent
- Decision Record: `docs/decisions/DR-M3-002-epr-baseline-live-provider-proof.md` (the live-provider half)

## Goal

Hold model, provider, family and role bindings, modality/context/tool compatibility, current economics, latency and governance in a signed, versioned Model Registry; keep empirical workflow statistics out of it behind a `stats_version` reference; and make a configuration change need no desktop release.

## Existing-code audit

- classification: ABSENT before this task. The gateway's catalog was hardcoded in `default_openai_models` and `default_anthropic_models`, so a new binding, a price change or a revocation required a new build. Nothing was signed, nothing carried a generation, and there was no role binding at all — `Requirements` described what one request needed, not what the deployment allowed.
- production entry points:
  - `crates/providers/src/registry.rs` — the document, its verification and the bindings. `activate` checks an Ed25519 signature over the document's exact bytes against the Core's trusted keys, refuses a document outside its freshness window or written for another schema, refuses one that carries empirical outcome fields, refuses one whose floors are out of range, and refuses one that leaves `solver` or `reviewer` unbound. `bindings_for` filters by capability and governance; `check_dispatch` answers `MODEL_REVOKED`, `MODEL_NOT_IN_REGISTRY` or `MODEL_NOT_ELIGIBLE`.
  - `crates/providers/src/gateway.rs` — `activate_registry`, and a per-dispatch check inside `route`, so a generation that withdraws a binding takes effect between one dispatch and the next rather than at startup.
  - `services/modbit-core/src/model_registry.rs`, `ActivateModelRegistry` and `GetModelRegistry` — the Core entry points. Only the Core reads `MODBIT_REGISTRY_KEYS`, the way only the Core reads provider credentials.
  - `services/modbit-core/src/runtime.rs` — a registry refusal keeps its own code, so a withdrawn model reads as `MODEL_REVOKED` rather than as a generic routing problem.
- proof: a Core running on its build defaults activates a signed document over the surface protocol and reports the generation, the signing key, the document digest and the bindings — with no credential, no endpoint URL and no signing key in what the client sees. A tampered document, an expired one, one carrying `success_rate`, and one that leaves no reviewer are each refused by code, and none of them disturbs the generation already active. A task then runs under the registry; a new generation revokes the model it used, and the next run stops explicitly with `MODEL_REVOKED` naming the generation that withdrew it, with nothing dispatched on another model.

## Limitations

- The live-provider half of `EPR-E2E-002` — the same activation against a production provider endpoint, retaining the provider's own generation proof — has not run: this repository has no provider credentials. DR-M3-002 records the boundary and defers that run to `.github/workflows/live-providers.yml`.
- The registry filters and refuses; it does not yet choose. Selecting the cheapest feasible binding for a role is EPR-016, and compiling a plan from the bindings is EPR-004.
- A revoked model produces an explicit stop rather than an automatic fallback, because on the direct path there is no prevalidated alternative to fall back to. Falling back to another prevalidated slot arrives with EPR-005.
- `stats_version` is a reference only. Materializing the statistics dataset is EPR-015, and the registry deliberately produces no estimate of its own.
- Signing uses Ed25519 through `ed25519-dalek`, a new dependency chosen under the docs/36 framework (dependency before build, for a cryptographic primitive nobody should hand-roll).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_002_a_signed_registry_activates_at_runtime_and_a_revocation_stops_dispatch` — the end-to-end proof through Core, including every refusal and the mid-run revocation.
- `a_signed_document_activates_without_a_new_build`
- `ingestion_refuses_empirical_workflow_outcomes` (EPR-FI-002)
- `a_document_that_is_not_authentic_or_not_fresh_never_activates` (EPR-FI-002)
- `a_registry_without_a_floor_or_a_reviewer_is_refused_whole` (EPR-FI-002)
- `a_revoked_model_is_refused_at_dispatch_and_says_so` (EPR-FI-002)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
