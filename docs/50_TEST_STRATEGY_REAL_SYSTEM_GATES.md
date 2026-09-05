# Test Strategy — Real-System Completion Gates

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Interpretation of “no mocks/placeholders”

Production features must be real. Unit tests may use fakes for pure logic, clocks or induced errors, but **no feature can reach COMPLETE from mocked-only tests**. The release gate uses actual compiled binaries/services, real disk, real Git, real Chromium, real databases, real sandbox substrate where applicable and live model-provider calls.

## Test pyramid

### 1. Pure unit tests
State transitions, policy evaluation, rank fusion, path normalization, hash chains, token packing, schema validation. Fast and deterministic.

### 2. Component tests with real local dependencies
SQLite WAL, content-addressed store, Tantivy/USearch/tree-sitter, actual Git repos, actual PTYs/processes, actual LSP servers packaged in CI images where supported.

### 3. Cross-process integration
Launch `modbit-core`, `modbit-execd`, Electron main protocol client, Sandbox Gateway/guest test environment and browser bridge as real processes. Assert event ordering, auth, cancellation, replay and persistence.

### 4. Local E2E
Launch packaged desktop app against real fixture repository. Use Playwright Electron automation only to click/type like a user; do not inject internal state. Agent uses live provider test model. Verify resulting filesystem/Git/tests/events after app restart.

### 5. Cloud E2E
Use staging cloud API, Postgres, object storage and actual MicroVM-substrate-backed MicroVM. Run real task, disconnect desktop, reconnect, verify remote continuation and artifact/effect chain.

### 6. Live provider conformance
At least OpenAI and Anthropic adapters perform real streaming, tool call, cancellation and rate-limit handling with dedicated credentials on nightly/RC pipeline.

### 7. Security/chaos
Kill processes, sever streams, expire leases, inject malformed MCP/browser content, attack paths/secrets/tenancy and verify fail-closed behavior.

## Fixture repositories

Maintain small but real Git repositories committed under `tests/fixtures/repos`:
- `ts-webapp` with TypeScript tests and intentionally seeded bugs;
- `rust-cli` with Cargo tests;
- `python-service` with pytest;
- `multi-package` monorepo for cross-package references;
- `conflict-repo` for concurrent worktree conflicts;
- `large-context-repo` generated once and checked by manifest for retrieval/perf.

Fixtures contain no fake Modbit implementations; they are target software for agent tests.

## Live model test control

To avoid nondeterministic completion claims, every live agent E2E defines observable acceptance in repository state/tests rather than exact prose. Run multiple trials for agent behavior. CI stores model/provider/version, prompt/compiler versions, seed where available and all evidence refs.

## Release evidence bundle

Each RC test run emits immutable bundle:
- build digest;
- test scenario version;
- environment versions;
- model/provider metadata;
- input repo commit;
- event log checksum;
- effect receipt chain head;
- final Git diff/commit;
- test/build outputs;
- checkpoint restore proof;
- result status.

A release cannot be signed if required evidence bundle is missing.


## V2 source-qualification rule

`42_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md` is now part of release authority. Unit mocks remain permitted for deterministic fault/edge testing, but a feature inspired/adopted from source research cannot become COMPLETE until its requirement qualification uses the real execution class appropriate to the claim. Multimodal and skill-evolution paths have dedicated real suites in `57_SKILL_EVOLUTION_REAL_TESTS.md` and `58_MULTIMODAL_MEDIA_REAL_TESTS.md`.

## Execution policy test extension

Doc 61 defines QUAL-EPR-000..019, EPR-E2E-000..013, EPR-FI-000..013 and EPR-GATE-A..G. Use production command routing into the real Core/Gateway/workspace/policy/verification stores; deterministic compiler unit tests supplement live provider and killed-process proofs. Preserve direct baseline before new workflows. Holdout calibration, complete all-leg costs, gate errors, relational critic recall, cache economics and rollback evidence gate activation; seeded defects and mutation tests must detect broken controls. Dossier CLI validation cannot satisfy product qualifications.
