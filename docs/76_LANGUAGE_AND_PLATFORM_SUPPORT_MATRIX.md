# Language and platform support matrix

> **Authority date:** 2026-09-05  
> **Decision:** DR-PX-2026-09-05 items 8 and 9 (`07_PRODUCT_EXTENSION_DECISION_RECORD.md`). Owners: context-engine (tree-sitter, headless language services, tier conformance for structural intelligence), verification (compile and test evidence, tier suites), governance (platform CI matrix), desktop (platform release promotion). No new canonical subsystem.  
> **Rule:** no language and no platform is supported by default. A language enters a tier, and a platform becomes release-grade, only after the corresponding conformance suite passes and the promotion is recorded.

## Language tiers

| Tier | Name | What the user gets | Conformance suite must prove |
|---|---|---|---|
| A | Full engineering intelligence | tree-sitter symbols and AST anchors; headless language-service symbols, references and diagnostics; compile, typecheck and test evidence; dependency graph; language-specific change strategies and skills; L0–L3 retrieval | symbol/reference recall on fixtures; diagnostics parity with the language service; compile/test evidence attribution; incremental index latency within budget; the competence suite tasks for the language pass at baseline |
| B | Validated structural support | tree-sitter symbols and AST anchors; compile and test evidence where a command is configured; generic diagnostics from build output; L0–L2 retrieval | symbol extraction accuracy on fixtures; safe structural edits with revision preconditions; build-output diagnostics parsed; no language-service claims |
| C | Text-safe support | exact and BM25 retrieval; safe text edits with revision preconditions; test or build evidence only through explicitly configured commands; no structural claims | edits never corrupt encoding or line endings; configured command evidence attributed; retrieval and change transactions behave identically to Tier B for text |
| — | Unsupported | read-only analysis; edits require explicit per-task user opt-in and carry `unsupported_language` provenance; no verification claim beyond configured commands | the explicit unsupported state is shown in every client; no intelligence is faked |

A tier is a **claim the product makes**, so it is earned by evidence: the tier conformance suite (PX-027) runs on real fixture repositories for that language, and promotion into a tier is a recorded decision with the suite's evidence bundle. Any language not in the table is Unsupported. Nothing is classified as structural or tree-sitter supported merely because a grammar exists.

## Current classification

| Language | Alpha | Beta target | Notes |
|---|---|---|---|
| TypeScript / JavaScript | Tier A candidate at the Alpha baseline | Tier A (PX-028) | `ts-webapp` fixture |
| Python | Tier A candidate at the Alpha baseline | Tier A (PX-028) | `python-service` fixture |
| Rust | Tier A candidate at the Alpha baseline | Tier A (PX-028) | `rust-cli` fixture |
| Any other | Unsupported | classified only after its suite passes | proposals via Decision Record |

**Alpha baseline (PX-026).** In Alpha the three candidates are proven at Tier C plus compile and test evidence: exact and BM25 retrieval, safe revision-bound edits, and real compiler and test-runner evidence for the fixture stacks. The full Tier A suite, including headless language services, lands with the context engine in Beta. Alpha therefore ships with honest labels: structural intelligence is not claimed until PX-028 passes.

## Degradation path (PX-029)

When a file's language is below the tier the task needs, the product degrades explicitly and visibly: retrieval falls back to text, the verification plan uses only configured commands, the agent's plan states the limitation, and every client shows the language state on the task and in Review. Tier C and Unsupported never present structural or semantic claims. There is no silent fallback to a lower tier.

## Platform states

| State | Meaning | How it is earned |
|---|---|---|
| RELEASE_GRADE | packaged desktop and CLI supported for users on that platform | packaged desktop E2E catalog and the applicable Release Zero subset pass on the platform; promotion recorded (PX-031) |
| CI_COMPATIBLE | Core, CLI and runtime build and pass unit, component and platform conformance suites in CI on that platform | platform CI matrix from M0 (PX-030) |
| UNSUPPORTED | none of the above | default |

CI compatibility is a development guarantee, never a user-facing support claim. Documentation, the app and the CLI describe a CI-compatible platform as exactly that.

| Platform | Alpha | Later |
|---|---|---|
| macOS (arm64, x86_64) | RELEASE_GRADE target for Alpha | maintained |
| Windows | CI_COMPATIBLE from M0 | RELEASE_GRADE only by PX-031 evidence and a Decision Record |
| Linux | CI_COMPATIBLE from M0 | RELEASE_GRADE only by PX-031 evidence and a Decision Record |

The platform conformance suites cover PTY and process control, headless language services, Git behavior, path and symlink policy, keychain and secret storage, packaging and updater, and the browser session host. The high-likelihood cross-platform drift risk in `72_RISK_REGISTER_AND_OPEN_DECISIONS.md` is mitigated by running these suites on every platform from M0, not by claiming support early.

## Relationship to other documents

Retrieval and diagnostics mechanics: `18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md`, `20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md`. Tier suites join `56_TOOL_CAPABILITY_CONFORMANCE.md`. Competence baselines per language: `63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md`. Release projections: `75_PHASED_RELEASE_PLAN_AND_READINESS.md`.
