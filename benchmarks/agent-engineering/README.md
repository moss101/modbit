# Eval Harness — competence baseline (PX-020)

The fixed competence baseline of `docs/63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md`,
measured through the real product: real Core, real tools, real verification
engine, real repair loop, driven only through the headless CLI. Nothing on the
Core's side knows it is being benchmarked — there is no benchmark prompt, tool
or policy relaxation — and the harness scores DI-3 test integrity again on top
of the engine's own enforcement.

## Frozen protocol

- **Suite:** `suites/internal/tasks.json` (id `internal-competence`, version
  `1.0.0`): six tasks over the fixture repositories of docs/50 — two per
  language (Rust, TypeScript, Python), one easy and one medium — each with an
  observable acceptance in repository state and tests, a seeded difficulty
  label and a language tier. The bundle's `task_list_digest` is SHA-256 over
  the suite file and every hidden acceptance file it names.
- **Configuration:** `direct` — one pinned endpoint and model, the single-model
  initial leg; no routing. The Core takes its endpoints from the environment
  exactly as in production (`OPENAI_API_KEY` / `ANTHROPIC_API_KEY`,
  `MODBIT_<P>_BASE_URL`, `_MODELS`, `_AUTH`, `_EXTRA_BODY`; docs/15
  "Endpoint configuration"). The harness never reads a credential.
- **Trials:** 3 independent trials per task (`--trials` may lower it for a
  smoke run; the bundle records what ran). Each trial is a fresh Core profile
  on a fresh LF-normalized copy of the fixture, committed to a fresh git
  repository; the fixture's installed modules are linked so a configured
  command runs as a developer's would. `FIXTURE_FLAKY_STATE` is fresh per
  trial, so the seeded flaky test fails exactly once per trial.
- **Interaction:** a typed question gets the suite's one fixed answer; a
  protected-effect approval is denied ("benchmark protocol"); a Waiting task is
  resumed at most four times. A trial is cancelled and scored `TIMEOUT` after
  `--trial-timeout-secs` (default 900).
- **Gold patches and hidden acceptance** are never in the workspace during a
  run; the hidden files are placed only after the task has ended. The bundle
  records `gold_patch_access: false` and validation refuses anything else.

## Scoring

Per trial, from the Core's own event log (`events tail --json`), the task
status and `task economics`:

| field | meaning |
|---|---|
| `core_verified` | the task ended `ReadyForReview`/`Completed` through the Core's own completion gating with no `RegressionAttributed` |
| `acceptance_passed` | every acceptance command exited 0 after the hidden files were placed |
| `test_integrity_ok` | every protected test file is byte-identical to before the run (DI-3, scored by the harness) |
| `verified_success` | all three |
| `first_pass` | verified with zero `RepairAttemptRecorded` (doc 63, literally) |
| `first_candidate` | verified with at most one `RepairAttemptRecorded` and no `RepairEscalated`: the first fix held, or one recorded repair after a failing planned verification sufficed. (Reproducing a reported failure with an ad-hoc command opens no verification signature, so a direct fix after it still counts as first pass.) |
| `counts` | repair attempts, `RepairEscalated`, `NoProgressDetected`, policy denies, plan v1 write set, plan revisions, questions, files changed / outside the original plan, edits without a `RetrievalRecorded`, regressions, flaky quarantines, `DiffInvariantViolated` by invariant, gate verdict, verification stage/status, self-review |
| `economics` | model calls, tokens, cost at catalog list prices, wall/model/tool time |

Metrics follow doc 63's table: verified and first-pass success with 95% Wilson
intervals, the repair-loop distribution, escalation counts and rates, blocked
wrong-effect attempts, evidence coverage, cost and time per verified success,
retrieval discipline (must be zero), scope discipline against the *original*
plan, regression attribution, flaky-check rate and DI-3 violations.
Test-selection quality is stated as not measured on these suites.

## Running

```bash
cargo build -p modbit-core -p modbit-cli -p modbit-execd -p modbit-bench-agent-engineering
target/debug/competence-baseline \
  --suite benchmarks/agent-engineering/suites/internal/tasks.json \
  --fixtures tests/fixtures/repos --out target/competence-baseline \
  --endpoint openai --model glm-5.3-flash            # or MODBIT_LIVE_MODEL
```

Output: `baseline.json` (the bundle), `baseline.sha256` (its digest),
`summary.md`, and `trials/<task>-<n>/{cli.log,events.jsonl,diff.patch,acceptance.log}`.
Exit 0 only when the bundle validates. `.github/workflows/live-competence.yml`
runs it on hosted CI with the repository secrets and gateway variables and
retains the output as an artifact; an accepted run is copied under
`evidence/m3/PX-020/`.

## What this does not do

The public benchmark half of PX-020 (SWE-bench Verified under the frozen
protocol with pinned container images) is a separate driver; its absence is
stated on the task card until it runs. `tests/harness.rs` proves the harness
itself offline — suite digest, bundle refusals, event scoring, and the binary
driving the real CLI and Core on one task with a scripted model — and never
produces a baseline: only a real model does (docs/82).
