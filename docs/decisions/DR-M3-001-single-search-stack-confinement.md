---
id: DR-M3-001
title: Confine the search-stack dependencies (Tantivy, USearch, tree-sitter) to the retrieval crate so no production module can grow a second search stack
status: accepted
date: 2026-09-10
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks" on 2026-09-09/10; architecture guardrail under docs/81 and REQ-EV-0171
---

# DR-M3-001 — One search stack: confine Tantivy, USearch and tree-sitter to `modbit-retrieval`

## Trigger / evidence

M3.1–M3.9 delivered the exact/regex/path index, BM25 (Tantivy), AST symbols
(tree-sitter), semantic chunks (USearch), the evidence graph, the L0–L3
planner, the Context Pack compiler and the benchmark harness. REQ-EV-0171
("Context Engine as shared service/ports") requires that every agent,
reviewer, browser and testing path use the same context ports, and its
qualification QUAL-EV-0171 asks for an architecture test that prevents
duplicate search stacks in production modules. The architecture lint
already confines the cloud isolation dependency to one adapter; the search
stack had no such guard.

## Current behavior

`tools/architecture-lint/rules.toml` confines `cubesandbox*` to
`modbit-sandbox`. Any workspace member could add `tantivy`, `usearch` or a
`tree-sitter*` grammar and build a parallel index.

## Proposed replacement

Three `[[confine]]` rules: `tantivy*`, `usearch*` and `tree-sitter*` are
allowed only in `modbit-retrieval`. Other members reach search through
`modbit-tools`' `SearchPort` (served by the Core's `IndexPort`) and the
`search.*` / `context.*` tools; `modbit-context` packs what the port returns
and never indexes.

## Migration

None: at this record's date only `modbit-retrieval` depends on those
packages (verified by `architecture-lint deps`).

## Compatibility

Additive rule; no code changes.

## Security impact

None beyond a smaller surface for duplicated index code.

## Verification

- `architecture-lint deps` (CI) fails on a new direct dependency on the
  confined packages outside `modbit-retrieval`.
- `tools/architecture-lint/src/lib.rs` :: `confine_rejects_direct_use_outside_allowed_members`
  covers the rule mechanics.
