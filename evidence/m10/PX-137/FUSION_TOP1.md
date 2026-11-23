# PX-137 finding: fusion lost first place to a token embedder — fixed

- Task: PX-137 (REAL_TESTING; unchanged by this fix). Owner: context-engine (`crates/retrieval`, M3.7 planner fusion, docs/18 "Fusion").
- Evidence tier: release-critical (ranking that reaches every Context Pack).
- Branch: `fix/retrieval-fusion-top1`, from `main` at c5294eb0.

## Finding

The PX-137 retrieval-only report on `main` showed the full planner (profiles B and C) putting the labelled file first far less often than BM25 alone: recall@1 0.188 against 0.449 on this repository, 0.233 against 0.967 on the 3,000-file synthetic corpus. Per-case diagnosis over both corpora found three causes, all in reciprocal-rank fusion (k = 60):

1. A hit only the semantic index found scored exactly what the BM25 first place scored (1/61); the tie fell to path order.
2. Semantic rank 0 plus BM25 rank 1 outscored BM25 rank 0 alone, which on the synthetic corpus promoted the `*_test.rs` twin of the answer.
3. Fusion is per region, so semantic agreement on another region of the right file added nothing to it.

The shipped `HashingEmbedder` (`hashing-v1`) hashes the query's own tokens. Its nearness is the overlap BM25 already scores, more coarsely, so counting it as a second vote double-counts that overlap with noise. A sweep of semantic weights and k values confirmed it: any weight large enough to change an order lowered top-1 on both corpora; whole-file fusion lowered it on the repository.

## Change

- `Embedder::independent_of_lexical()` (default `true`): whether nearness in the space is evidence the lexical index lacks. `HashingEmbedder` returns `false`; a learned embedder (PX-112) keeps the default.
- In the planner, a candidate from a non-independent embedder is *corroborating*. It still adds the candidate and keeps its rank as provenance (`MethodEvidence.rank`). It contributes `rrf = 0` to the score, and its reciprocal rank breaks ties below the score and above path order.

## Measured (retained in `benchmarks/retrieval/results/px-137-retrieval-*.json`)

| Corpus | Profile | recall@1 before → after | @5 before → after | @10 before → after |
|---|---|---|---|---|
| modbit-repository (64 cases) | B / C | 0.188 → 0.449 | 0.754 → 0.870 | 0.928 → 0.986 |
| modbit-repository | LexicalOnly | 0.449 → 0.449 | 0.884 → 0.870 | 0.971 → 0.971 |
| synthetic-3000 (60 cases) | B / C | 0.233 → 0.967 | 0.967 → 1.000 | 1.000 → 1.000 |
| synthetic-3000 | LexicalOnly | 0.967 → 0.967 | 1.000 → 1.000 | 1.000 → 1.000 |

The repository corpus is the live tree, so every column moved slightly with the code under test. B/C recall@10 against ripgrep: mean delta +0.039, 95% CI [0.000, 0.094]. This is not a significant win. The fusion is now level with BM25 at top-1 and top-5, not ahead of it.

## Tests

- `crates/retrieval/tests/nl_retrieval.rs`:
  - `methods_that_found_one_region_reinforce_one_hit` now runs with an embedder declared independent; the N5 assertions are unchanged.
  - New `a_token_embedder_corroborates_without_voting`.
  - New `a_corroborating_only_hit_never_outranks_scored_evidence`.
- Mutation: forcing `corroborating = false` fails the two new tests (8 passed, 2 failed); restored.
- Passing: `cargo test -p modbit-retrieval`, `cargo test -p modbit-bench-retrieval` with `MODBIT_BENCH_REQUIRE_EXTERNAL=1` (real ripgrep), and `cargo clippy -p modbit-retrieval --all-targets -D warnings`. Core retrieval and context tests: see `core-tests.log`.

## Not done

- The PX-137 live same-model run (`benchmarks/live-results/px137/`) predates this fix. Its planner-profile numbers must be re-measured on the gateway path; that is the task's open REAL_TESTING leg.
- Semantic retrieval adds no independent signal until a learned embedder lands (PX-112, NOT_STARTED).
- The structural levels (L2/L3) tie the hybrid profile on these cases.
