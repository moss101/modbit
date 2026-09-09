---
id: DR-M2-001
title: Provider Gateway seals on the wire-faithful conformance suite; the live production-endpoint proof runs in a dedicated workflow once test credentials exist
status: accepted
date: 2026-09-09
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks" on 2026-09-09; evidence-scope decision under docs/15 "Live provider proof" and docs/82 (no simulated substrate is claimed as the live proof)
---

# DR-M2-001 — Live provider proof pending credentials

## Trigger / evidence

docs/15 "Live provider proof" says adapters are complete only when nightly/RC
CI performs a real streaming call, a real typed tool-call round trip and
cancellation/timeout against the production provider endpoint with dedicated
test credentials. The repository has no provider secrets (`gh secret list` is
empty on 2026-09-09) and the build host holds no `OPENAI_API_KEY` /
`ANTHROPIC_API_KEY`. M2.6 cannot present that evidence today without inventing
it, which `docs/82` forbids.

## Current behavior

No gateway existed; nothing streamed.

## Proposed replacement

1. M2.6 ships `modbit-providers` (OpenAI-compatible Chat Completions and
   Anthropic Messages streaming adapters, capability catalog, requested-vs-
   resolved route records, bounded retry, timeout, cancellation, health) and
   the Core commands `ListModels` / `ProbeModel`.
2. Its retained evidence is the conformance suite
   (`crates/providers/tests/conformance.rs`): real HTTP over real sockets
   against wire-faithful local OpenAI and Anthropic servers with fault
   injection (rate limit, stall, disconnect after first token, malformed
   frames, auth rejection), plus the Core-level socket proof
   (`m2_6_provider_gateway_streams_through_the_core_over_real_http`).
3. The live proof is the same suite's
   `live_streaming_tool_round_trip_and_cancellation_against_production_endpoints`
   test, executed by `.github/workflows/live-providers.yml` (nightly and on
   demand) with `MODBIT_LIVE_PROVIDERS=1` and the repository secrets
   `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`. Until the owner adds those secrets
   the workflow skips with an explicit notice; the first green run is appended
   to `evidence/m2/M2.6/evidence.json` as run evidence. M2.6's graph node is
   sealed on items 1–2; the M2 milestone proof "E2E-001/002/003 with live
   model" (docs/43) stays open until item 3 has run green.

## Migration

None.

## Compatibility

No interface changes beyond the additive `ListModels` / `ProbeModel` messages.

## Security impact

Credentials are read only by the Core process from its environment
(docs/15 "Credentials"); `SecretHandle` redacts in `Debug`; gateway errors are
scrubbed of bearer-like tokens; the live workflow receives secrets only through
GitHub's secret store.

## Test impact

Conformance suite (7 tests) and Core socket proof run in the regular CI matrix;
the live test self-skips without `MODBIT_LIVE_PROVIDERS=1`.

## Rollback

Delete the workflow and this record's reference from the M2.6 evidence; the
conformance suite stands on its own.
