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

## Health and failover

Provider health includes rolling success rate, first-token latency, stream interruption, tool-call validity, rate-limit state and cost. Retry is bounded with jitter and charged to the same request. A replacement must be an already validated slot binding and satisfy current policy. Otherwise stop/reconcile; a different topology requires separately admitted transaction/epoch with remaining budget. Ambiguous effectful tool outcomes must be reconciled through protocol state and Effect Ledger before a new attempt. Missing mandatory gate/reviewer, exhausted budget or no eligible fallback yields an explicit error/attention state, never unverified completion.

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

## Live provider proof

Provider adapters are not considered complete until nightly/RC CI successfully performs a real streaming model call, a real typed tool-call round trip and cancellation/timeout against the production provider endpoint using dedicated test credentials.


## V2 multimodal/provider capability fields

`ModelCapability` must include agent-loop support, vision, supported media modalities, context/output limits, parallel-tool support, structured-output support, tool-message media constraints and schema-compatibility mode. Provider adapters may transform canonical media placement (for example splitting media from a tool-role message) but may not alter canonical call/result identity. Routing rejects an endpoint that cannot satisfy required modality/policy/data-residency constraints.

## Cache epochs and full accounting

Maintain conversation-level routing stickiness; re-route only at new request, compaction/epoch, explicit mode, provider degradation, complexity/risk or decomposition boundaries. Compare retained cached prefix/TTL/read-write pricing against lost reuse, uncached re-prefill and reconstruction latency. Record switch reasons and route epoch; nominal token price alone cannot justify switching.

ModelRequest metadata additionally binds plan, leg, invocation attempt, route epoch, policy/model/Skill/registry versions and budget reservation. Normalize every attempt's usage, cached subset, failed/cancelled charges and uncertainty into RoutingTrace and OutcomeRecord; estimates are not observed counterfactual savings. The primary metric is total inference cost plus human intervention and wall-clock per verified success under a quality floor. EPR-000 establishes baseline, EPR-005 proves DIRECT non-regression, and doc 61 gates any floor/compound policy promotion.
