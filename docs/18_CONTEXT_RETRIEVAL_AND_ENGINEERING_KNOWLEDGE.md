# Context, Retrieval, and Engineering Knowledge Engine

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Objective

Maximize task-relevant evidence per token. Modbit uses retrieval-before-edit and exposes provenance so users and agents can inspect why context was selected.

## Index inputs

For each immutable workspace revision:
- file paths and exact text;
- lexical token index (BM25);
- semantic chunk embeddings;
- tree-sitter AST and symbol definitions;
- headless LSP symbols/references where available;
- import/dependency edges;
- Git ownership/change history and changed-line context;
- diagnostics;
- test mappings and recent runtime evidence;
- repository engineering memory with scope/confidence.

Generated/vendor/binary paths are policy-tagged and not indexed by default.

## Concrete local indexing stack

- **Exact/regex:** ripgrep-compatible Rust search path.
- **BM25:** Tantivy index per repository snapshot family.
- **Semantic ANN:** USearch HNSW index with versioned embeddings.
- **Syntax:** tree-sitter parsers for supported languages.
- **Semantic language services:** headless LSP processes normalized into Modbit diagnostics/symbol records.
- **Graph:** compact adjacency store in SQLite metadata + memory-mapped edge segments for hot revisions.

Embedding is an infrastructure feature, not reasoning. If semantic embedding is unavailable, exact/BM25/AST/LSP remain fully functional.

## Retrieval planner

```text
L0 Exact       known path/identifier → exact/regex/symbol
L1 Hybrid      unknown wording       → BM25 + semantic + exact fusion
L2 Structural  cross-file relation   → AST/LSP/dependency graph expansion
L3 Engineering impact/debug          → L2 + Git + diagnostics + tests + runtime evidence
```

Planner starts at the cheapest level supported by query features and escalates only when confidence/coverage is insufficient. This prevents “use every index for every query” latency regressions.

## Fusion

Candidates carry source-specific scores but are combined using rank-based fusion plus deterministic boosts for exact symbol/path match, workspace freshness, changed lines, diagnostic linkage and dependency distance. Duplicates are collapsed by code span identity. Optional LLM reranking is not required for P0 correctness and cannot be the only ranking path.

## Context Pack

```text
ContextPack {
  pack_id
  workspace_revision
  query/task fingerprint
  entries[] {source_ref, span, provenance, freshness, reason, score, token_cost}
  omitted_summary
  token_budget
  compiler_version
}
```

Packing is budget-aware: exact task constraints and critical diagnostics first, then highest marginal evidence utility. Context Ledger records every injected entry and later whether the agent actually referenced/used it.

## Index freshness

Workspace service emits revision events. Small edits update exact/BM25/AST/Git metadata incrementally. Semantic embedding is queued only for changed chunks. A query may run against an older semantic index only if its generation is declared and changed files are covered by exact/lexical overlay; otherwise escalate to fresh deterministic retrieval.

## Benchmark gates inherited from project research

Use the same-model, same-task methodology when comparing retrieval profiles:
- A: baseline exact/search tools;
- B: hybrid exact + BM25 + vector;
- C: Modbit structural context engine.

Target on BrowseComp-Plus-equivalent protocol: preserve ~99% answer accuracy while aiming for at least ~40% input-token reduction, ~45% tool-call reduction and ~40% agent-time reduction versus baseline. These are **targets, not claimed results** until reproduced.

For repository engineering, additionally measure relevant-file recall@K, evidence precision, changed-code impact accuracy, cold-index time and incremental-index latency. SWE-QA-style multi-hop questions are a P1 comparative gate.


## V2 Skill Evolution knowledge boundary

The evolution-lab-inspired **Skill Evolution Knowledge Store is not Engineering Memory**. It is an evaluation artifact store for patterns distilled from sealed traces. Production context does not automatically read it. An approved skill may cite PURPOSE/evidence metadata, while detailed optimization traces remain outside normal PromptEnvelope. This preserves the locked invariant that transcript/optimization history cannot silently become durable runtime memory or authority.

## Role-specific conditional routing inputs

Supply bounded intrinsic task/repository features without requiring full index build; missing features use conservative fallback. Model/price/availability/allowlist/cache/objective facts remain outside the learned profile. Compiler joins separately versioned Outcome Statistics and cache/context estimates.

Solver/reviser progressive disclosure stays governed. Reviewer context binds original task, criteria, candidate diff, relevant code and static diagnostics/tests/API/security evidence to one revision and excludes solver hidden reasoning by default. Bounded extra evidence can be gathered through the disposable review environment of EPR-018; canonical state stays read-only. Stale reviewer evidence cannot approve a new candidate. Record the context/compiler/statistics/gate/risk versions needed for exact replay; docs 27/38/61 govern.
