# Execution Policy Router and Provider Gateway

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Objective

Expose one normalized streaming inference boundary and compile the cheapest eligible ConditionalExecutionPlan whose conservative verified-quality bound meets the selected mode floor and whose latency satisfies hard constraints; otherwise explicitly mark QUALITY_FLOOR_INFEASIBLE on the best hard-eligible fallback. Frontier remains the quality backstop. One ConditionalExecutionPlan binds provider-neutral roles and prevalidated slots, while the canonical Core executes it; DIRECT/CASCADE/CRITIQUE are derived path labels only. Provider-specific semantics never leak into Agent Runtime state machines. Detailed routing authority: `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md`; algorithms/contracts: `38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md`.

## Provider contract

```text
ModelRequest
  request_id
  model_policy
  messages/context segments
  tool_projection
  response_format?
  reasoning_effort?
  cache_key/prefix metadata
  max_output_tokens
  timeout
  tenant/user policy tags

ModelEvent
  MessageDelta
  ReasoningDelta (when provider exposes it)
  ToolCallStart
  ToolCallDelta
  ToolCallComplete
  Usage
  ProviderMetadata
  Completed
  Error
```

Adapters translate OpenAI-compatible and Anthropic-style APIs first. Additional providers implement the same conformance suite before exposure.

## Execution policy routing

PolicyEnvelope is deterministic and owned by the existing Policy Kernel. RequestProfile predicts task type/domain/modifiers, capability demand, calibrated floor success, confidence and OOD independently of provider/model names, prices, cache, health, allowlists and objective preference. The hot path uses bounded deterministic/small learned profiling with conservative fallback; no general LLM classifier or full repository scan.

The deterministic compiler joins intrinsic profile, policy, model/Skill registry versions, Outcome Statistics, price/health/latency/cache snapshots, budgets and verification availability. Hard eligibility and confidence-adjusted verified-quality feasibility precede minimum expected complete cost selection. High mean/weak LCB cannot qualify cheap plans; when no feasible plan exists, best hard-eligible fallback is labeled QUALITY_FLOOR_INFEASIBLE without waiving assurance or claiming target success. Validate finite attempts/retries/timeouts/revisions, acyclic fallbacks, context/data policy, reviewer isolation, worktree availability and worst-case request plus verification reservation before dispatch. Registry and policy updates are signed, auditable and compatible; a last-known-good bundle is usable only within freshness and current entitlement constraints.

Auto exposes Cost, Balance (default), Intelligence or organization objective profiles. Manual model pins are allowed only by policy and cannot silently weaken checks or switch after failure. Effective model/workflow and reasons are always logged internally; label visibility is policy-controlled and available for authorized development/admin diagnostics. Surface clients do not embed routing weights or classifier authority.

Model Registry holds current identity/revision/provider/family, ECONOMICAL_FLOOR/FRONTIER_SOLVER/REVIEWER/SPECIALIST roles, capabilities/context/tool/media/economics/health/governance. Versioned Outcome Statistics separately holds solver success, joint escalation, relational reviewer value and revision success with sample counts/confidence. Skills influence qualified statistics, never permissions. Registry or benchmark means alone cannot qualify a cheap plan.

As built (IMP-EV-0029, REQ-EV-0029; `crates/providers/src/compiler.rs`, `services/modbit-core/src/routing.rs` and `runtime.rs`): the hard filters run before anything is weighed, in one place — `binding_exclusion` removes a binding that is revoked, not allowed by the model policy in force, not bound to the role, outside the request's residencies, kept from the execution profile by governance, priced in another currency, without tools, vision or structured output the request needs, or whose context is below what the request is priced at. The model policy is the configuration's `models_allow` (Device > Admin > Project > User, intersected; an entry names `endpoint/model` or a bare model) from the task's pinned snapshot, part of the compile's input digest only when it restricts (an unrestricted compile digests as before). Every Core compile now demands tools and a context of at least the expected input it prices (40 000 tokens); images are not a hard need, since a text-only binding gets them described or refused by name (docs/25). `Compiled` carries `hard_exclusions`, the request's `demands` in words and their `demands_digest`; `RoutingDecisionRecorded` records all three with the `input_digest`, so the decision is auditable from the log and replayable: `CompileRoutingPlan` on the same task reproduces the digest, the demands, the exclusions and every candidate's scores (on a run that has ended it is a replay, recorded nowhere). The same policy holds everywhere a model is chosen: an operator plan naming a refused model is refused `MODEL_NOT_ALLOWED`, a pin on one is `PIN_NOT_ELIGIBLE` with the policy's reason, the direct path refuses it `MODEL_NOT_ALLOWED`, and `StartTask` decides the route before it records anything, so a refused start leaves the task as it was. The policy is re-read at every model round (REQ-EV-0041); when it refuses the model in force the run re-routes at boundary `POLICY` (a `SWITCH` under the policy that now holds), and when nothing it allows can take over (no registry, a pin, nothing eligible, a review task) the run ends `RunFailed { MODEL_NOT_ALLOWED }` with a `Policy` diagnosis and the next start routes a new run. The task's intrinsic fingerprint stays the Request Profiler's (`RequestProfiled`, shadow, REQ-EPR-003): recorded beside the decision, never an input to it (tests `qual_ev_0029_hard_filters_decide_first_and_the_corpus_replays_exactly`, `qual_ev_0029_hard_filters_decide_first_and_the_decision_replays`).

## Health and failover

Provider health includes rolling success rate, first-token latency, stream interruption, tool-call validity, rate-limit state and cost. Retry is bounded with jitter and charged to the same request. A replacement must be an already validated slot binding and satisfy current policy. Otherwise stop/reconcile; a different topology requires separately admitted transaction/epoch with remaining budget. Ambiguous effectful tool outcomes must be reconciled through protocol state and Effect Ledger before a new attempt. Missing mandatory gate/reviewer, exhausted budget or no eligible fallback yields an explicit error/attention state, never unverified completion.

As built (IMP-EV-0030, REQ-EV-0030; `crates/providers` registry and compiler, `services/modbit-core/src/escalation.rs::at_provider_boundary`): a binding's approved fallback chain is part of the signed registry — `RegistryEntry.fallbacks`, in order, at most `MAX_FALLBACKS` (2), naming bindings of the same document, never itself and never twice (`REGISTRY_INVALID_FALLBACK` refuses the document whole). The compiler carries the opener's chain in every plan as prevalidated slots `fallback-1`, `fallback-2` (`Trigger::LegFailed`, each continuing the one before), worst-case budgeted like every reachable slot and left out of the expected cost (only an outage pays them); a fallback the request cannot use is excluded with its reason. When a leg's provider fails outright — its bounded retries spent, or the gateway refusing the route — the loop takes the plan's fallback continuing the failed binding through admission with the remaining budget, on the same run and transcript, and records the decision: `SlotActivated`, `RouteReevaluated` at boundary `PROVIDER` (`SWITCH`, the failure as the reason), `ContinuationActivated` (`LEG_FAILED`, cause `PROVIDER_FAILED:<code>`) with the note the model reads. Without an approved fallback, or one admission refuses, the `STAY` is on the log with why and the run stops as before — there is no hidden retry elsewhere. The routing view labels such a path `FALLBACK` (and now lists executed legs in the order they ran); the request record (EPR-010) keeps the failed leg `FAILED` (test `qual_ev_0030_a_primary_outage_continues_on_the_approved_fallback_and_records_the_decision`).

## Prompt cache economics

Prompt Compiler emits stable segments:
1. system/policy;
2. stable workspace rules/skill manifests;
3. compaction epoch;
4. task context pack;
5. recent events.

Cache keys include model/provider/prompt compiler version and stable segment hashes. Context economics records cache hit/miss, input tokens by segment and recomputation cause.

## Credentials

Raw provider secrets never enter renderer/model context. Local provider credentials use OS-protected storage accessed by Core/main boundary. Hosted service credentials live in cloud secret manager; sandbox receives only short-lived broker handles when explicitly required for a tool, never model API secrets.

## Endpoint configuration

The Core registers its provider endpoints from its own environment (`endpoints_from_env`), never from a renderer or a model. Per family `P` in `OPENAI`, `ANTHROPIC`: the credential `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` (read at request time, never logged); `MODBIT_P_BASE_URL` for a compatible gateway — a base whose last segment names an API version (`https://api.z.ai/api/paas/v4`) is used as is, otherwise the family's `/v1` path is appended; `MODBIT_P_MODELS`, the gateway's catalog as comma-separated `model=input/output[;ctx=…;out=…;vision=…;reasoning=…]` entries whose USD-per-million-token prices are mandatory (an unknown provider cost is not free, `73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md`); `MODBIT_P_AUTH` (`native`, the family's own header, or `bearer`); and `MODBIT_P_EXTRA_BODY`, a JSON object of provider-specific request fields that fill in beside the canonical body and never override a canonical key. Each family registers on its own; a malformed value leaves that family unregistered with the reason on the Core's stderr, and nothing is guessed in its place. The same fields exist on `Endpoint` for `ConfigureProvider`.

## Live provider proof

Provider adapters are not considered complete until nightly/RC CI successfully performs a real streaming model call, a real typed tool-call round trip and cancellation/timeout against the production provider endpoint using dedicated test credentials. `.github/workflows/live-providers.yml` is that run. It skipped explicitly while the secrets were absent (DR-M2-001); since IMP-EV-0211 it fails without them, because a skipped live proof exits 0 exactly like a passing one. DR-M9-002 allows the same proof to run against an OpenAI- or Anthropic-protocol compatible gateway configured through the variables above; such a run proves the adapters' wire behaviour, streaming, tool-call round trip and cancellation against a real model, and is recorded with the gateway and model it used, but it does not stand in for the provider's own production endpoint, which stays an open item on each affected task until it has run.

As built (IMP-EV-0211, REQ-EV-0211; `crates/providers/tests/recorded.rs`, `tests/support/mod.rs`): every live test leaves a result record in `MODBIT_LIVE_RESULTS_DIR` — `passed` with the integrations it proved, or `skipped` with why — and the workflow's last step, `tools/integration_gate.py --live`, refuses the run unless every live test proved every configured wire. `live_recorded_tool_round_trip_on_each_configured_wire` drives the gateway's tool round trip against each real endpoint through a recording proxy on 127.0.0.1. The proxy forwards the adapter's own request over TLS and keeps the request body and the response chunks as they arrived. It never keeps a credential header or our own request id. The recording is refused if the product's redactor finds a held key or a credential shape in it. A recording from a green run is committed as `crates/providers/tests/fixtures/recorded/<wire>.json` with that run's id, and its sha256 is kept in the run's retained result record. Offline, on every platform, `replay_recorded_<wire>_tool_round_trip` serves the recording from a server that answers only the recorded requests, byte for byte. The real adapter must reproduce the round trip from it: the same tool call, the same provider ids and models read from the stream. A request body the provider was never sent is refused with its first difference (`replay_refuses_a_request_the_provider_never_accepted`). So a change to what the adapter sends, or to how it reads a stream, fails until a live run re-records it. To re-record: dispatch `live-providers` on the branch, copy `live-qualification/recorded/<wire>.json` from the run's artifact into `crates/providers/tests/fixtures/recorded/`, and keep the run's `live-qualification/results/` under the task's `evidence/` directory, where the integration gate looks for the recording's digest. The cancellation step cancels on the first streamed token of either kind, answer or reasoning, and gives the counting request room to run. A model that reasons first had spent all 256 tokens counting in its reasoning, and no answer text ever arrived to trigger the cancellation (runs 35705593051 to 36115236181).


## V2 multimodal/provider capability fields

`ModelCapability` must include agent-loop support, vision, supported media modalities, context/output limits, parallel-tool support, structured-output support, tool-message media constraints and schema-compatibility mode. Provider adapters may transform canonical media placement (for example splitting media from a tool-role message) but may not alter canonical call/result identity. Routing rejects an endpoint that cannot satisfy required modality/policy/data-residency constraints.

## Cache epochs and full accounting

Maintain conversation-level routing stickiness; re-route only at new request, compaction/epoch, explicit mode, provider degradation, complexity/risk or decomposition boundaries. Compare retained cached prefix/TTL/read-write pricing against lost reuse, uncached re-prefill and reconstruction latency. Record switch reasons and route epoch; nominal token price alone cannot justify switching.

As built (EPR-010): `Usage` means one thing on every wire — `input_tokens` is every input token billed, `cached_input_tokens` and `cache_write_input_tokens` its subsets. The Messages adapter sums the plain input, cache reads and cache writes that wire reports as disjoint cumulative counts, and a repeated report replaces the held one rather than adding to it; Chat Completions reports the total with the cached subset inside it and no writes. The Core prices each attempt from that shape (doc 34 "As built (EPR-010)").

ModelRequest metadata additionally binds plan, leg, invocation attempt, route epoch, policy/model/Skill/registry versions and budget reservation. Normalize every attempt's usage, cached subset, failed/cancelled charges and uncertainty into RoutingTrace and OutcomeRecord; estimates are not observed counterfactual savings. The primary metric is total inference cost plus human intervention and wall-clock per verified success under a quality floor. EPR-000 establishes baseline, EPR-005 proves DIRECT non-regression, and doc 61 gates any floor/compound policy promotion.
