# Task Card — M2.6 Provider Gateway OpenAI + Anthropic streaming

## Identity

- Task ID: M2.6
- Milestone: M2
- Canonical owner: model-gateway (`crates/providers` is THE doc 81 model/provider gateway; the Core owns the instance and its credentials)
- Requirement IDs: REQ-EV-0028, REQ-EV-0189 (capability catalog; conformance probes detect mismatch and route away), REQ-EV-0112 (provider-neutral requested/resolved model, effort, tier); docs/15 "Provider contract", "Health and failover", "Credentials", "Live provider proof"; docs/43 M2.6.
- Risk class: high (external network, credentials)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

One normalized streaming boundary: `ModelRequest` in, `ModelEvent`s out (`MessageDelta`, `ReasoningDelta`, `ToolCallStart/Delta/Complete`, `Usage`, `ProviderMetadata`, `Completed`, `Error`) for OpenAI-compatible Chat Completions and the Anthropic Messages API, with a capability catalog per endpoint, requested-vs-resolved route records, bounded jittered retry before the first token, whole-stream timeout, cancellation, and rolling health. The Core exposes `ListModels` and `ProbeModel`.

## Non-goals

- The Execution Policy Router, RequestProfile, plan compiler and Outcome Statistics (docs/27, EPR tasks).
- OS keychain credential storage (the Core reads its environment; `SecretHandle` is the seam).
- Prompt compilation and the agent loop that consumes the stream (M2.7).
- Media placement transforms (REQ-EV-0188, M2.10).

## Required reading

`AGENTS.md`, `docs/15`, `docs/25`, `docs/30`, `docs/52`, `docs/decisions/DR-M2-001`.

## Existing-code audit

- classification: NOT-FOUND (`crates/providers` was an M0.1 shell)
- production entry point: `modbit_providers::ProviderGateway::{route, stream}`, `endpoints_from_env`; Core `ListModels`, `ProbeModel`; CLI `model list`, `model probe`
- real effector/storage boundary: HTTPS to provider endpoints (reqwest + rustls); no persistence (the runtime records turns in M2.7)

## Invariants

- Routing checks the catalog (tools, vision, modalities, reasoning, structured output) and credential availability before any network call; mismatch is a typed `RouteError`.
- Retries happen only before the first token; after it, an interruption is reported, never replayed.
- Cancellation ends the stream with `Completed{cancelled}` and drops the connection; timeout is a typed error.
- Raw credentials never appear in events, route records, errors or `Debug` output.
- Tool call identity (`call_id`) round-trips unchanged into the next request on both wires.

## Verification

- `crates/providers/tests/conformance.rs` against wire-faithful local servers over real HTTP: OpenAI and Anthropic streaming tool round trips (request bodies, headers, usage, metadata, route record), rate-limit retry bounds and exhaustion, timeout, cancellation observed by the server, interruption after first token (no replay), malformed frames, auth rejection, capability mismatch before dispatch, credential redaction
- Core: `m2_6_provider_gateway_streams_through_the_core_over_real_http` (catalog, text and tool probes, route refusal, health)
- CLI smoke: `model list`
- Live production-endpoint proof: `live-providers.yml` per DR-M2-001 (pending repository secrets)
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json`.
