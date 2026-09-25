# Test Strategy — Real-System Completion Gates

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Interpretation of “no mocks/placeholders”

Production features must be real. Unit tests may use fakes for pure logic, clocks or induced errors, but **no feature can reach COMPLETE from mocked-only tests**. The release gate uses actual compiled binaries/services, real disk, real Git, real Chromium, real databases, real sandbox substrate where applicable and live model-provider calls.

## Evidence tier selection

Before choosing test levels, classify the change by behavioral risk (`83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md`). Release-critical changes run the full pyramid below through level 4 or 5. Iteration-tier changes, which modify none of effect-bearing behavior, canonical persistence, permissions or policy, execution, recovery, protocol or schema, security boundaries or evidence semantics, run levels 1 and 2 plus the packaged UI smoke subset of level 4 against a real local Core. Mixed or uncertain changes are release-critical. In neither tier does a mocked Core, provider, store or effector count as proof.

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

## Multi-level integration qualification (REQ-EV-0211)

An integration is any place production code opens a connection outside its own process. Each one is declared in `tools/integrations.json` with its boundary and the tests that qualify it at each level:

- **component** — the adapter called directly, over a real socket, against a counterpart that speaks the real protocol;
- **integration** — through the Core, registry or service the product actually routes through, against the real counterpart;
- **external** — only for an `external-api` boundary: a `live_` test against the real API with the real test credential, run by a workflow, plus a **recorded safe fixture**. The fixture is the exchange a live run captured, holds no credential, and is replayed offline through the real adapter on every platform.

`tools/integration_gate.py` holds the declaration to the code in the `dossier integrity` job:

- every workspace member or TypeScript package whose production source opens an outbound connection is declared, and no declaration claims one that opens none;
- every named test exists as a test, not as a helper function;
- an `external-api` integration has all three levels, and its live tests are ones a workflow's `cargo test -p <crate> live_` actually selects;
- every recorded fixture names the CI run that made it, carries no credential header, and has its sha256 in that run's retained result record under `evidence/`;
- a `real-service` or `in-tree` integration has its integration level and no external level.

The live level may be deferred only by an accepted Decision Record naming the missing input.

`--live DIR` is the live workflow's last step. It reads the run's result records and fails when any live test of a non-deferred external integration skipped, failed or left no record. A skipped live test exits 0, so without this step it is indistinguishable from a pass.

`--release` also fails on every deferral: a release cannot stand on mock-only proof of an external integration.

As built (IMP-EV-0211), eight integrations:

| Integration | Boundary | Live level |
|---|---|---|
| OpenAI wire | external API | live on the compatible gateway (DR-M9-002), recorded |
| Anthropic wire | external API | live on the compatible gateway (DR-M9-002), recorded |
| GitHub forge | external API | deferred by DR-M6-002 until `MODBIT_GITHUB_TOKEN` and a test repository exist, so `--release` fails today |
| S3-compatible object store | real service (SeaweedFS in the `cloud` job) | none |
| Cloud worker link | in-tree | none |
| Sandbox gateway link | in-tree | none |
| Sandbox egress relay | in-tree | none |
| Sandbox browser relay | in-tree | none |

## Fixture repositories

Maintain small but real Git repositories committed under `tests/fixtures/repos`:
- `ts-webapp` with TypeScript tests and intentionally seeded bugs;
- `rust-cli` with Cargo tests;
- `python-service` with pytest;
- `multi-package` monorepo for cross-package references;
- `conflict-repo` for concurrent worktree conflicts;
- `large-context-repo` generated once and checked by manifest for retrieval/perf.

Fixtures contain no fake Modbit implementations; they are target software for agent tests. Each Alpha fixture (`ts-webapp`, `python-service`, `rust-cli`) additionally carries, documented in its own README: one intentionally flaky test, one seeded failure whose obvious fix is wrong, one acceptance-named test that a tempted agent could weaken, and one pre-existing failing test unrelated to any task, so that the flake protocol, the repair bounds, the test-integrity invariant and regression attribution of `64_VERIFICATION_EXECUTION_CONTRACTS.md` have real targets (PX-E2E-018, PX-E2E-032..037).

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
