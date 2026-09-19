---
id: DR-M9-002
title: The live-provider proofs may run against an OpenAI- or Anthropic-protocol compatible gateway (z.ai, glm-5.3-flash), recorded as such; the providers' own production endpoints stay open
status: accepted
date: 2026-09-19
supersedes: none
approved_by: repository owner (moss101), instruction in chat on 2026-09-19 to use z.ai credentials and endpoints ("use following credentials for anthropic and openai keys … end point https://api.z.ai/api/anthropic … https://api.z.ai/api/paas/v4 … model GLM-5.3-Flash") for the two live adapters; evidence-scope decision refining DR-M2-001, DR-M3-002, DR-M3-003 and DR-M6-001 under docs/15 "Live provider proof" and docs/82 (no simulated substrate is claimed as the live proof)
---

# DR-M9-002 — Live proofs on a compatible gateway, named as such

## Trigger / evidence

DR-M2-001, DR-M3-002, DR-M3-003 and DR-M6-001 deferred the live halves of
the provider gateway, the EPR direct baseline, the benchmark and
conformance tasks and the VS Code adapter until "dedicated test
credentials" for the production endpoints exist; `PX-020` is `BLOCKED` on
the same condition and holds M3 and, through it, every M10 step
(`docs/77_RELEASE_ZERO_EXECUTION_GOAL.md` §3). On 2026-09-19 the owner
supplied credentials for z.ai, which serves `glm-5.3-flash` over both wire
protocols the gateway speaks: the Anthropic protocol at
`https://api.z.ai/api/anthropic` (`/v1/messages`, per z.ai's Claude Code
guidance) and the OpenAI protocol at `https://api.z.ai/api/paas/v4`
(`/chat/completions`, z.ai's general API; a separate
`…/api/coding/paas/v4` endpoint serves the GLM Coding Plan). z.ai's
published list price for `glm-5.3-flash` is USD 0.15 per million input
tokens and 0.50 per million output tokens (docs.z.ai, pricing, read
2026-09-19).

The gateway could not use them: `endpoints_from_env` appended `/v1` to
every base URL, chose the family's own auth header only, offered no way
to name a gateway's catalog (so the live test's "small model" heuristic
selected an OpenAI or Anthropic model the gateway does not serve), and
registered the Anthropic family only when an OpenAI credential was also
present.

## Current behavior

The live proof (`live_streaming_tool_round_trip_and_cancellation_against_production_endpoints`)
and `.github/workflows/live-providers.yml` run only against
`api.openai.com` / `api.anthropic.com` with the built-in catalogs, and skip
while the secrets are absent.

## Proposed replacement

1. `Endpoint` gains `auth: AuthScheme` (`native` — the family's own
   header — or `bearer`) and `extra_body` (a JSON object of
   provider-specific request fields added beside the canonical body; a
   canonical key is never overridden). The request URL comes from
   `wire_url`: a base whose last segment names an API version (`v1`, `v4`)
   is used as is, otherwise the family's `/v1` is appended.
2. `endpoints_from_env` becomes `endpoints_from(lookup)`: per family,
   `MODBIT_<P>_BASE_URL`, `MODBIT_<P>_MODELS` (the gateway's catalog as
   `model=input/output[;ctx=…;out=…;vision=…;reasoning=…]`, prices in USD
   per million tokens and mandatory — an unknown provider cost is not free,
   docs/73), `MODBIT_<P>_AUTH`, `MODBIT_<P>_EXTRA_BODY`. Each family
   registers on its own; a malformed value leaves that family unregistered
   with the reason on stderr.
3. The live test takes its model from `MODBIT_<P>_LIVE_MODEL`, else
   `MODBIT_LIVE_MODEL`, else the catalog's small model, refuses a model
   the catalog does not list, and prints the endpoint, URL, model, auth
   scheme and extra fields it used — never a credential. The workflow
   passes these as repository variables (not secrets), writes the
   non-secret configuration to the job summary and retains the log as an
   artifact.
4. A green run against a compatible gateway is appended to each affected
   task's `evidence.json` as `run:<id>` with the gateway host and model
   named in the card ("live proof: z.ai glm-5.3-flash, OpenAI and Anthropic
   protocols"). It discharges the parts of the deferred halves that need a
   real model: streaming, the typed tool-call round trip, cancellation, a
   model choosing its own tools, and the competence and onboarding
   measurements. It does not discharge "the production provider endpoint"
   of docs/15: that clause stays an open item on the same cards until a
   run against `api.openai.com` / `api.anthropic.com` exists, and the
   Release Zero scenario's step 1 ("the configured real staging model
   gateway") is satisfied by whichever real gateway the release names.
5. Condition on the credentials: the keys used must be licensed for a
   self-built client. z.ai limits the GLM Coding Plan "to use within
   officially supported tools and products"; the owner supplies general
   API keys, or accepts that restriction, before the workflow runs. The
   keys were pasted into a chat transcript on 2026-09-19 and are to be
   rotated after the proof runs.

## Migration

None. Two additive `Endpoint` fields with serde defaults; existing
environments (a key, no variables) behave exactly as before, except that an
Anthropic-only environment now registers its endpoint.

## Compatibility

`ConfigureProvider` still builds endpoints with the default auth scheme and
no extra fields; a gateway catalog through the desktop onboarding is a
later change. The default catalogs and prices are unchanged.

## Security impact

No new secret path: credentials stay in `SecretHandle::Env`, read at
request time, redacted from errors, absent from the printed
configuration, the job summary and the retained log. `extra_body` is
configuration, not a secret, and cannot override the canonical request.
A bearer scheme on the Anthropic family sends the same credential in a
different header, to the configured host only.

## Test impact

Two tests in `crates/providers/tests/conformance.rs`:
`compatible_gateway_versioned_base_url_bearer_auth_and_extra_body` (URL
derivation for four bases; a versioned base reaches
`/api/paas/v4/chat/completions` with the extra field beside an untouched
canonical `model`; bearer auth on the Anthropic family with no
`x-api-key`) and
`endpoints_from_configuration_register_providers_independently_with_priced_catalogs`
(Anthropic alone registers; a two-family gateway configuration with
catalog, prices, limits, flags, extra body and auth; seven malformed values
each leave only the other family registered; the spec parser's edge
cases). The live test itself is unchanged in what it asserts.

## Rollback

Revert the commit; no data or schema is affected. A later record may
retire the gateway path once the production-endpoint runs exist.

## Explicit user approval

Given in chat on 2026-09-19 by supplying the z.ai credentials, endpoints
and model for the two live adapters.
