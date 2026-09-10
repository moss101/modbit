# Task Card — M3.5 USearch embeddings + changed-chunk incremental update

## Identity

- Task ID: M3.5
- Milestone: M3 (Context intelligence)
- Canonical owner: context-engine (`crates/retrieval` `semantic` module; `search.semantic` tool in `crates/tools`; per-workspace semantic index and write-time flush in `services/modbit-core/src/tools.rs`)
- Requirement IDs: docs/18 "Index inputs" (semantic chunk embeddings), "Concrete local indexing stack" (Semantic ANN: USearch HNSW index with versioned embeddings), "Index freshness" (semantic embedding is queued only for changed chunks; an older generation is declared so changed files are covered by the exact/lexical overlay), "Embedding is an infrastructure feature, not reasoning"; docs/36 DEPEND: USearch (PROVISIONAL); docs/43 M3.5; REQ-EV-0153 / REQ-EV-0159 groundwork
- Risk class: medium (native HNSW library in-process; provisional dependency per docs/36)
- Evidence tier: release-critical
- Release membership: BETA, RELEASE_ZERO (M3)

## Goal

A USearch HNSW index (cosine, f32) over chunks of every searchable file — symbol-bounded chunks where the tree-sitter index has definitions, fixed 40-line windows otherwise — with vectors from a versioned `Embedder` port. The shipped embedder is deterministic feature hashing of code tokens (`hashing-v1`: camelCase/snake_case-split tokens sign-hashed into 256 buckets, L2-normalized): real vectors and real nearest-neighbour search, named in every result and claiming no learned semantics; a provider embedding model plugs into the same port once credentials exist (DR-M2-001). Changed paths are queued and re-embedded — only they — at the new generation on every write; queries carry the embedder id, the embedding generation and the paths still queued so the planner can overlay exact/lexical results for them. Exposed as `search.semantic`.

## Non-goals

A learned embedding model (needs a provider or a local model; the port is ready), persistence of the index across Core restarts (rebuilt at first use), fusion with lexical/exact ranking (M3.7), the USearch licensing/benchmark lock (docs/36 keeps it PROVISIONAL).

## Existing-code audit

- classification: NOT-FOUND (no vector index; USearch was not a dependency)
- production entry point: `modbit_retrieval::SemanticIndex` (build / mark_changed / flush / search), `HashingEmbedder`, `chunk_file`; `ToolHost::semantic`; `IndexPort` serves kind `semantic`
- real effector/storage boundary: real USearch index in memory over the real files of the exact index

## Invariants

- Every generation records its embedder id; a different embedder is a different vector space and forces a rebuild.
- Unchanged chunks keep their vectors and revision; only queued paths are re-embedded, and the queue is visible to queries until flushed.
- Results are bounded and carry chunk hashes, byte spans and the revision each chunk was embedded at.

## Verification

- `modbit-retrieval/index` :: `semantic_index_finds_nearest_chunks_and_reembeds_only_changed_files`
- `modbit-core/surface_protocol` :: `m3_5_semantic_chunk_index_serves_nearest_chunks_and_reembeds_only_changed_files`
- `modbit-tools/pipeline_and_direct` :: `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
