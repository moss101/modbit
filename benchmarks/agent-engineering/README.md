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
| `test_integrity_ok` | every protected test file is intact in doc 63's sense: each original line that declares a test or asserts something is still present, unchanged and in order (a named test modified, deleted, skipped or weakened fails; tests added beside them and an import gaining a symbol are allowed); scored by the harness on top of the engine's DI-3 |
| `verified_success` | all three |
| `first_pass` | verified with zero `RepairAttemptRecorded` (doc 63, literally) |
| `first_candidate` | verified with at most one `RepairAttemptRecorded` and no `RepairEscalated`: the first fix held, or one recorded repair after a failing planned verification sufficed. (Reproducing a reported failure with an ad-hoc command opens no verification signature, so a direct fix after it still counts as first pass.) |
| `counts` | repair attempts, `RepairEscalated`, `NoProgressDetected`, policy denies, plan v1 write set, plan revisions, questions, files changed / outside the original plan, edits without a `RetrievalRecorded`, regressions (only `RegressionAttributed` with the `REGRESSION` label; every label is tallied in `attributions`), flaky quarantines, `DiffInvariantViolated` by invariant, gate verdict, verification stage/status, self-review |
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
`summary.md`, and `trials/<task>-<n>/{cli.log,events.jsonl,diff.patch,acceptance.log,reports/}`
(`reports/` holds the Core's verification report objects for every stage of the trial).
Exit 0 only when the bundle validates. `competence-baseline merge --out <dir>
<part dir>...` combines per-task bundles of the same suite, protocol and
revision into one (trials concatenated, metrics recomputed, every part's
event logs kept under its own directory; a mismatch is refused).
`.github/workflows/live-competence.yml` runs one job per task on hosted CI with
the repository secrets and gateway variables — a lost runner loses one task's
trials, not the run — then merges the parts and retains
`competence-baseline-<endpoint>-<run>`; an accepted run is copied under
`evidence/m3/PX-020/`. After every trial the harness stops that profile's
Core and prints the processes holding the most memory, so a starved runner
can be read from the log.

## Four-axis agent benchmark (PX-184)

`suites/four-axis/tasks.json` (id `four-axis`) runs on the same harness,
bundle format and protocol, over its own fixture
(`suites/four-axis/fixtures/inventory`). Forty tasks, ten on each axis, each
tagged by `axis` and carrying its own `verifier`:

| axis | what the task exercises through the real Core | what its verifier checks |
|---|---|---|
| `tool_use` | `fs.read`, `search.exact`, `fs.glob`, `fs.list`, `shell.exec`, `change.apply` (create/edit), `change.batch` | workspace files; the named tool's result as the Core stored it (`ToolCallSucceeded.result_ref` in the profile's object store, copied to `trials/*/tool-results/`) |
| `memory` | curated memory seeded through `memory propose`/`promote` (one seed then `forget`), recalled by injection or `memory.query`; new items recorded with `memory.propose` | workspace files; `MemoryUsed` or a `memory.query` result naming the seeded item's id; a forgotten item reaching the task fails; `MemoryProposed` by an `agent:` author with the stated topic and type |
| `skill_use` | a skill written to the profile and trusted with `skill trust name@hash`, read with `skill.load` | workspace files following the skill's rule; a `skill.load` result (or `SkillSelected`) with that name and the trusted content hash |
| `subagent` | `agent.spawn` of one or two children with disjoint write scopes, `agent.wait`, `git.merge.prepare`/`commit` of each child | the children's files in the parent's workspace; `SubagentAdmitted`, `SubagentResultRecorded` and `MergeTransactionRecorded` (`COMMITTED`) counts on the parent |

The harness puts seeds in place through the CLI before the run and resolves
each seed key to the id or content hash the Core assigned; the suite sets
`run_mode: RUN_EVERYTHING` on every task (`run-mode set --ack`, as a person
would), which lets a parent integrate its children without an approval; the
always-ask classes still ask and no task needs one. A trial's
`acceptance_passed` includes its verifier's verdict, so `verified_success`
needs the Core's completion gating and the task's own check.

Validation refuses a task without an axis or without its own verifier (a
unique id and checks no other task shares), a verifier that checks nothing,
an axis with fewer than ten tasks (the suite cannot lower the floor), and a
gate that cannot fail. For such a suite the run also writes `axes.json`:
verified success per axis with its 95% Wilson interval, and the output-token
cost per thousand tasks (and USD at catalog list prices when priced),
computed only from recorded usage — per trial, `task economics` of the task
and every child it admitted, which must equal the `ModelUsageRecorded` events
of the trial's log; a trial without recorded usage, or whose two records
disagree, refuses the report.

```bash
target/debug/competence-baseline \
  --suite benchmarks/agent-engineering/suites/four-axis/tasks.json \
  --fixtures benchmarks/agent-engineering/suites/four-axis/fixtures \
  --out target/four-axis --endpoint openai --model <id> [--trials N] [--jobs 4]
target/debug/competence-baseline compare \
  --baseline benchmarks/agent-engineering/baselines/four-axis --current target/four-axis
```

`compare` checks each `axes.json` against its bundle (figures not derived from
the bundle's recorded usage are refused), reports the per-axis and cost
difference in `comparison.json`, and exits 3 when an axis fails its gate: its
verified-success rate is more than `max_drop` (0.05) below the baseline's —
with ten single-trial tasks, any one regressed task. `--jobs N` runs trials
at once, each on its own profile and Core. `merge` does not write `axes.json`.

`baselines/four-axis/` is the retained baseline: the bundle, its digest,
`axes.json` and `summary.md` of a run with the scripted model of
`suites/four-axis/scripted-model.json` (event logs and tool results are not
retained; the bundle keeps each log's digest and offsets). It measures the
harness and the Core's capability paths end to end, not a model's competence;
a live model's run is compared the same way. `tests/four_axis.rs` runs the
forty tasks, compares the re-run with that baseline, then injects a
regression on the memory axis (the model stops recalling and writes stale
values) and shows that only the memory gate fails;
`PX184_RETAIN_BASELINE=1` refreshes the baseline from its clean run after a
change to the suite or harness.

## GUI regression suite (PX-179)

`suites/gui/tasks.json` (id `gui-regression`) runs on the same harness,
bundle, report and compare. Twenty-four Modbit-owned GUI tasks over the
fixture desktop, twelve on each of two axes:

| axis | what the computer-use child does through the real Core |
|---|---|
| `gui_element` | accessibility-first element actions: `computer.set_value`, `computer.press`, a modal dialog read with `computer.state {window}`, two applications in one task |
| `gui_input` | raw input: `computer.type` (with `then: enter`), `computer.key` chords, `computer.click` at a screenshot coordinate on an element with no accessible action |

Each task names its desktop (`suites/gui/desktops/*.json`, digested with the
suite): the trial's Core gets `MODBIT_COMPUTER_ACTUATOR` set to the fixture
actuator (`--actuator`, default the sibling `modbit-actuator-fixture`) over
that desktop with `--state-out`, so the application's state after every input
is written by the actuator itself. The parent spawns a `computer-use` child
with the task's `completion_evidence`; the approvals protocol
`approve-computer-inputs` has the harness act as the person at the fixture
desktop — every `computer.*` input the Core asks about is approved with the
intent it was asked under, every other protected effect is denied — since a
computer input is never allowlistable.

A task's verifier (`gui`) is the GUI verifier row (PX-178), read from the
Core's own records: a `GuiEvidenceEvaluated` at completion, on a child the
task admitted, with status `PASS` for exactly the suite's predicate (a child
declared with an easier one does not count), and that child's
`GuiOutcomeJudged` `VERIFIED`. On the fixture desktop the application's state
after the run (`desktop-state.json` in the trial) must also hold the task's
element values. Validation refuses a GUI task without a `gui` verifier, a
`gui` verifier without a desktop, a desktop path outside the suite, a GUI
suite under another approvals protocol and an evidence value that is not a
completion-evidence predicate; `tests/gui_suite.rs` also checks that no
task's evidence holds on its desktop before the run.

The suite declares its axes (`axes.names`) and the per-task gate
(`gate.per_task`): `axes.json` keeps each task's verified trials, and
`compare` names every task that verified in the baseline and does not now
(`regressed_tasks`, which fails the gate whatever the axis rates) and every
task that recovered. `baselines/gui/` is the retained baseline of a
scripted-model run. `tests/gui_suite.rs` runs all twenty-four tasks through
the real CLI, Core and fixture actuator, compares the re-run with the
baseline (no difference), then regresses three named tasks (the child stops
before the input that has the effect and still claims it is done) and shows
`compare` exiting 3 with exactly those tasks named. The test runs in the
`rust` CI job on macOS, Linux and Windows, so a regression of the fixture set
fails the merge. `PX179_RETAIN_BASELINE=1` refreshes the baseline.

```bash
cargo build -p modbit-core -p modbit-cli -p modbit-execd -p modbit-actuator-fixture -p modbit-bench-agent-engineering
target/debug/competence-baseline \
  --suite benchmarks/agent-engineering/suites/gui/tasks.json \
  --fixtures benchmarks/agent-engineering/suites/gui/fixtures \
  --out target/gui --endpoint openai --model <id> --trials 1 --jobs 4
target/debug/competence-baseline compare \
  --baseline benchmarks/agent-engineering/baselines/gui --current target/gui
```

The real-application half of PX-179 (a small set of real macOS apps on a
prepared machine) is not part of this suite: it needs the macOS helper of
PX-071 attached to the Core with Accessibility and Screen Recording granted on
an owner-prepared machine, and a live model; see
`evidence/m10/PX-179/TASK_CARD.md`.

## What this does not do

The public benchmark half of PX-020 (SWE-bench Verified under the frozen
protocol with pinned container images) is a separate driver; its absence is
stated on the task card until it runs. `tests/harness.rs` proves the harness
itself offline — suite digest, bundle refusals, event scoring, and the binary
driving the real CLI and Core on one task with a scripted model — and never
produces a baseline: only a real model does (docs/82).
