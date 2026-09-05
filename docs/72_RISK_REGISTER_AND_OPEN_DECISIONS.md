# Risk Register and Open Technical Decisions

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Rule:** product locks stay locked; provisional implementation choices must earn permanence through measured validation.

## Highest risks

| Risk | Likelihood | Impact | Mitigation / validation | Owner |
|---|---:|---:|---|---|
| Clean-slate build expands into too many subsystems before a working vertical slice | Medium | Critical | Enforce M0→M1→M2→M4 critical path and Release Zero before broad feature work | Core/Product |
| Procedural runtime creates a second tool/security system | Medium | Critical | All `tools.*` calls route through the same Tool Registry, Capability Kernel, events and Effect Ledger | Runtime/Security |
| Retrieval becomes sophisticated but slower/worse than simple search | Medium | High | L0-L3 escalation, A/B/C benchmarks, latency gates, exact/BM25 fallback | Context |
| Multi-agent parallelism causes merge/conflict overhead greater than benefit | Medium | High | Transactional admission, semantic write conflict checks, capacity budgets; single agent is default when decomposition has low value | Core |
| Browser semantic IDs are unstable on modern reactive apps | Medium | High | State fingerprint + entity remapping + DOM/AX/layout identity + targeted visual fallback; benchmark survival rate | Browser |
| Screenshot fallback grows into screenshot-first behavior | Medium | High | Track structural-action percentage and visual fallback reasons; regression gate | Browser/Context |
| Cloud sandbox dependency leaks product policy into substrate | Low-Med | Critical | Sandbox Gateway owns tenancy/capability/secrets/effects; sandbox substrate adapter has narrow interface and conformance suite | Platform/Security |
| Durable recovery duplicates or replays external effects after crashes | Medium | Critical | stable call IDs, protocol state, UnknownOutcome, effect receipts, reconciliation E2Es | Core/Security |
| Engineering Memory pollutes future context with stale/untrusted facts | Medium | High | explicit promotion, provenance/confidence/TTL/revision binding, conflict/supersession | Context |
| Electron renderer/browser increases desktop attack surface | Medium | High | strict main/preload/renderer boundary, sandbox, CSP, untrusted browser partition, Core authority outside renderer | Desktop/Security |
| Live-provider E2E becomes flaky/costly | Medium | Medium | objective repo-state assertions, multi-trial nightly runs, frozen low-cost test tasks, provider health labeling; mocked tests never substitute for release proof | QA/AI |
| Cross-platform PTY/LSP/Git differences create drift | High | Medium | platform conformance suites, packaged CI runners, capability reporting, explicit unsupported states | Execution |
| Context/model costs become opaque | Medium | High | per-turn token/cache/tool/sandbox accounting; task budgets and cost evidence | Runtime/Platform |

## Provisional implementation choices

### Electron desktop shell — **PROVISIONAL**
Why selected: strongest fit for React/TypeScript and visible local Chromium session using a hardened `WebContentsView`; aligns with the desktop patterns already studied.  
Validation before locking: packaged macOS/Windows performance/security, browser same-session control, updater reliability, idle memory budget and preload attack tests.  
Exit: SurfaceProtocol/Core stays independent, allowing another native shell without runtime rewrite.

### USearch HNSW — **PROVISIONAL**
Why selected: embedded ANN avoids a separate vector service.  
Validation: recall/latency/memory on large repository classes, incremental rebuild behavior, license/maintenance review, crash/corruption behavior.  
Exit: `SemanticIndex` trait plus generation format migration.

### QuickJS procedural isolate — **PROVISIONAL**
Why selected: small embeddable JavaScript VM with no ambient authority when host bindings are explicit.  
Validation: async tool ergonomics, deterministic cancellation, CPU/memory limits, fuzzing, malicious program isolation and cross-platform build.  
Exit: `ProcedureEngine` interface can be backed by WASM/Starlark-like runtime without changing Tool Registry.

### Hosted OIDC implementation — **PROVISIONAL**
The identity protocol is fixed (OIDC authorization-code + PKCE); the vendor/hosted implementation is not. Choose via security, enterprise SSO, desktop deep-link support, regional/data requirements, cost and exportability. Keep internal User/Tenant IDs independent of provider IDs.

### Local embedding model — **PROVISIONAL**
Use a versioned non-generative embedding model for semantic retrieval when available. Validate licensing, CPU/GPU latency, package size, multilingual/code retrieval quality and update compatibility. Semantic embeddings must never become a correctness dependency; deterministic retrieval remains available.

### Remote browser display transport — **PROVISIONAL**
CDP semantic control and BrowserSession/control-lease contracts are fixed; WebRTC/VNC-style display implementation is replaceable. Validate latency, bandwidth, clipboard/input security and resume behavior before lock.

## Explicitly deferred product decisions

- pricing/plan packaging and exact cloud quotas;
- marketplace/ecosystem economics;
- mobile/simulator product surface;
- broad recurring consumer automations;
- general-purpose embedded editor;
- Code-OSS compatibility adapter.

Deferral means these do not block the core product and must not create placeholder UI in P0.

## Go / no-go checkpoints

### After M2
**Go** only if Release Zero's local coding subset works with real model/Git/terminal/tests and restart. Otherwise stop context/multi-agent expansion and repair the core vertical slice.

### After M3/M5
**Go** only if Context Engine and procedural runtime measurably reduce tool/token cost or improve success without security/regression. If not, retain simpler direct tools/retrieval and reconsider the sophistication.

### After M7
**Go** only if structural browser operation solves the standard app suite with low screenshot fallback and same-session takeover is reliable.

### Before broad cloud rollout
**Go** only after real MicroVM isolation, cross-tenant, secret-leak and sandbox-loss tests pass. No “beta exception” for tenancy or secret isolation.


## V2 added risks / decisions

- **PROVISIONAL:** exact multimodal vision-bridge provider selection; architecture requires a replaceable explicit bridge, not a vendor.
- **EXPERIMENT:** evolution-lab-style automatic evolution; promotion remains off by default until Modbit benchmark evidence justifies it.
- **RISK:** source-feature completeness can increase scope; mitigation is deduplication by canonical owner and disposition, not automatic adoption.
- **RISK:** tool-schema proliferation; mitigation is procedural runtime + dynamic projection + deferred tool tail.
- **RISK:** media cost/context explosion; mitigation is byte/page/region/duration budgets and ArtifactRefs.
- **RISK:** compatibility import can smuggle executable behavior; mitigation is quarantine/trust/signature/policy review.

## EPR development and rollout risks

| Risk | Owner and required resolution |
|---|---|
| Uncalibrated floor success / catalog drift | model-gateway + eval-bench; versioned cohort, slice holdout and drift gates |
| Critic false reassurance / correlated failures | verification + eval-bench; relational recall, evidence validation and non-committing disposable review isolation |
| Failed floor makes compound execution more expensive | observability + eval-bench; complete expected/worst-case cost, human intervention and latency |
| Multi-turn switching loses cache economics | core-runtime; sticky epochs and measured refill/TTL cost |
| Selection bias and unsafe replay | eval-bench + effects-security; propensity, isolated snapshot replay and no duplicated effects |
| Local operation depends on unavailable control plane | model-gateway; signed compatible fresh cached bundles and explicit conservative fallback |
| Missing quality/rollout thresholds | eval-bench; publish approved immutable profile with sample-size/statistical method before promotion; absence blocks activation |

These are unmeasured risks/acceptance obligations, not claimed empirical results. ADR-R-039..056 lock architecture; numerical qualification thresholds remain evidence-driven under doc 61.
