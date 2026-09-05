# Verification execution contracts: baseline, targeting, result normalization, flake handling and diff invariants

> **Authority date:** 2026-09-05  
> **Decision:** DR-PX-2026-09-05-006 (`97_DOSSIER_MAINTENANCE_LOG.md`), extending DR-PX-2026-09-05 item 6.  
> **Owners:** verification (verification runs and stages, normalized reports, flake protocol, completion run, regression attribution), workspace-git (diff invariants evaluated at ChangeTransaction time), context-engine (impact-based test selection from M3), core-runtime (repair-loop and harness consumers, `14_AGENT_RUNTIME_AND_ORCHESTRATION.md`, `28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md`), eval-bench (metrics, `63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md`). No new canonical subsystem.

## Why this document exists

`28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` says what must be verified and how repair is bounded. Before this document, three of its load-bearing terms were names without mechanics: "targeted tests" had no selection rule, "diff invariants" had no definition, and `failure_signature` had no source because no contract turned jest, pytest or cargo output into a structured failing check. A flaky test in the target repository could mint a phantom failure signature and consume repair attempts; a collateral edit could break a previously passing test without anyone attributing it. This document closes those seams. Every rule is a Core-enforced behavior with events and evidence; prompts describe it, the Verification Engine and Change Engine check it.

## 1. Verification runs and stages (PX-032, PX-034)

Every execution of the derived verification plan is a `VerificationRun` bound to one candidate revision and one environment digest:

```text
VerificationRun {
  verification_run_id
  run_id                  # the agent Run
  plan_ref                # derived verification plan (28 §4)
  stage                   # BASELINE | TARGETED | COMPLETION | RERUN
  candidate_revision      # workspace revision the checks ran against
  environment_digest      # toolchain, runner versions, lockfile digests, container/sandbox image
  started_at, ended_at
  status                  # PASSED | FAILED | ERROR | TIMEOUT | CANCELLED | UNKNOWN
  report_ref              # TestReport (section 2)
}
```

**BASELINE (PX-032).** Before the first write, the engine runs the plan's deterministic checks against the base revision: build and typecheck where configured, the diagnostics snapshot (`REQ-EV-0018`, `REQ-EV-0070`), the checks named by the task and the plan, and the configured test suite when its last known duration fits `baseline_budget`; otherwise the task-named and planned-file checks, with the narrowing stated in the plan. The result is a `VerificationBaseline` recorded as a `VerificationBaselineRecorded` event. Checks failing at baseline are labelled `KNOWN_FAILING`; they are never attributed to the agent and, when the goal is to fix them, they are the reproduction the repair loop requires (28 §5). A check that fails at baseline and passes on its isolated rerun is quarantined before the agent has written anything (section 3).

**TARGETED.** Inner runs of the repair loop execute a bounded subset in this order: (1) checks that failed in the previous run; (2) checks named by the task or the plan; (3) checks mapped to the changed files, using in M2 the M2 heuristics (test files in the same module or package, naming conventions of the configured runner, import and path adjacency from the exact index) and from M3 the impact selection of section 6; (4) build, typecheck and lint when files of those kinds changed. A TARGETED run has its own budget and never supports a completion proposal.

**COMPLETION (PX-034).** Mandatory before any completion proposal and after the SelfReview (28 §6). Scope: every mandatory check plus, by default, the full configured suite; policy `completion_scope` may narrow it for large repositories to the baseline-covered set, the impacted set and the task-named checks, and only with the limitation recorded in the plan and shown in Review. It runs at the final candidate revision; any later ChangeTransaction invalidates it and the harness refuses the completion proposal until it is rerun (`14_AGENT_RUNTIME_AND_ORCHESTRATION.md`, harness contracts).

**RERUN.** The isolated re-execution of specific checks used by the flake protocol (section 3) and by an explicit agent request. Reruns are charged to the task budget.

**Regression attribution.** After the flake protocol, a check that was `PASS` in BASELINE and is `FAIL`, `ERROR` or `TIMEOUT` in COMPLETION at the candidate revision is a `REGRESSION` and blocks acceptance. The only exception is a check the plan declared, before the run, as an expected behavior change with a reason; such a check appears in Review as a declared change, never silently. A `KNOWN_FAILING` check that still fails is not a regression; one that now passes is recorded as a collateral fix. Attribution is recorded as `RegressionAttributed` events and feeds the metrics of doc 63.

## 2. Normalized test reports and failing-check identity (PX-033)

`test.run` (`17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md`) returns a `TestReport`; a raw exit code and a log are never the model's only view of a test run.

```text
TestReport {
  report_id, verification_run_id, stage, candidate_revision, environment_digest
  runner { family: vitest | jest | mocha | pytest | cargo | go | configured_command, version, argv, cwd }
  parser { adapter, adapter_version, confidence: STRUCTURED | HEURISTIC }
  status                  # PASSED | FAILED | ERROR | TIMEOUT | CANCELLED | UNKNOWN
  counts { pass, fail, error, skip, timeout, flaky }
  checks[]: CheckResult
  raw_output_ref          # full stdout/stderr as OutputRef; never dropped
}

CheckResult {
  check_id                # stable, runner-normalized: file path + suite path + case name (parameters canonicalized)
  kind                    # test | build | typecheck | lint | diagnostic | invariant | command
  status                  # PASS | FAIL | ERROR | SKIP | TIMEOUT | FLAKY | UNKNOWN
  duration_ms
  location { path, line?, symbol? }
  error_class             # runner or language error type when available
  message_fingerprint     # normalized message with volatile tokens removed
  message_excerpt         # bounded, provenance-tagged
  output_ref              # range into raw_output_ref for this check
}
```

**Adapters.** Structured reporters first: the JSON reporters of vitest and jest, mocha's JSON reporter, pytest's JUnit XML or report log, `cargo test` in libtest human output as the stable path and its JSON format where the toolchain permits, `go test -json`. When no structured reporter is available, a `configured_command` adapter parses exit code and output with conservative rules and labels the report `HEURISTIC`. A mandatory check whose outcome a `HEURISTIC` parse cannot establish is `UNKNOWN`, and `UNKNOWN` is `INDETERMINATE` for acceptance, never a pass (`REQ-EV-0068`). Adapters for the Alpha Tier A candidates ship with PX-026; Tier C languages use `configured_command` evidence and the plan says so (`76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md`, PX-029).

**Failing-check identity and `failure_signature`.** The repair loop's `failure_signature` (28 §5) is derived only from normalized `CheckResult`s:

```text
failure_signature = normalize(check_id, kind, error_class, location.path, location.symbol?, message_fingerprint)
```

Normalization removes addresses, timestamps, durations, temporary paths, random identifiers, line and column numbers and process ids; it keeps the file path, symbol and error class. `FLAKY` and `UNKNOWN` results never form a signature. Two failures with identical signatures at identical candidate diffs are the same failure, which is what makes the equivalent-hypothesis rule of doc 28 enforceable.

**Bounded failure evidence to the model (`REQ-EV-0107`).** The next-round context receives, in this order and within the output budget: the failing `CheckResult`s, then a bounded excerpt of each failure's output range, then the `raw_output_ref` with the truncated ranges declared. The raw log is retained in full as an OutputRef; nothing the model did not see is lost.

## 3. Flaky-check protocol (PX-036)

A check is flaky when its status differs between two runs at the same candidate revision and environment digest with no ChangeTransaction between them. The protocol:

1. When a check fails in a TARGETED or COMPLETION run and policy `flake_rerun` is enabled (Alpha default: one rerun), the engine reruns exactly the failed checks, isolated, at the same revision, as a RERUN run.
2. If the rerun passes, the check is labelled `FLAKY`, a `FlakyCheckQuarantined` event records both run references, and the check is excluded from `failure_signature` derivation. If the rerun fails, the failure stands.
3. A `FLAKY` check counts as neither `PASS` nor `FAIL` for acceptance. A mandatory check that is `FLAKY` makes the acceptance `INCONCLUSIVE` and the task `Needs Attention` with the quarantine evidence, unless policy `mandatory_flaky_passes` (Alpha default: three) consecutive isolated passes are obtained within budget, in which case it is recorded as `PASS` with the flake history attached.
4. Only the Verification Engine's rerun protocol can label a check `FLAKY`. The agent may request a rerun, which is charged to its budget; it cannot mark, skip or quarantine a check itself. A change that adds skip or retry markers to a test is a diff-invariant violation (DI-3, section 4).
5. Quarantine is scoped to the task and to the revision range in which the check's file and its dependencies are unchanged; it expires when either changes. There is no global silent skip list. Review and the SelfReview show every quarantine.
6. A check that is flaky at BASELINE is pre-quarantined so the agent never repairs a failure that was never caused by its change.

Flake rate per run and quarantines per task are metrics in doc 63; a repair that "succeeds" only because a check went `FLAKY` is not `RESOLVED`.

## 4. Diff invariants (PX-037)

Diff invariants are evaluated twice: by the Change Engine when a ChangeTransaction is proposed (per-transaction, against the current plan) and by the Verification Engine in the COMPLETION run (whole diff, base revision to candidate revision). `DENY` invariants reject the transaction as a `ToolCallPolicyDecision`; `FLAG` invariants block the SelfReview and the completion proposal until the finding is resolved or justified by an explicit plan entry. Every violation emits `DiffInvariantViolated` with the invariant id, the paths and the evidence range.

| Id | Invariant | Class | Detection |
|---|---|---|---|
| DI-1 | Write set: every changed path is inside the plan's current write set, or the transaction is accompanied by a `PlanRevised` with a scope delta and reason (ScopePolicy, 28 §3) | DENY when silent | path set comparison |
| DI-2 | Generated artifacts: generated files, lockfiles and migrations change only through their generators or with an explicit plan entry | DENY | repository configuration plus path patterns |
| DI-3 | Test integrity: no deletion, skip/only/xfail/retry marker, assertion weakening, expected-value edit or snapshot rewrite in a test named by the task's acceptance or `KNOWN_FAILING` at baseline; any other test-file change requires a plan entry declaring it | DENY for acceptance-named and baseline-failing tests; FLAG otherwise | AST diff for Tier A languages, conservative text rules elsewhere; uncertain cases FLAG, never pass silently |
| DI-4 | No debug leftovers introduced by the diff in non-test code (print/console/dbg!/debugger statements, commented-out blocks, stray TODO markers) | FLAG | language-aware patterns |
| DI-5 | No secrets, credentials or tokens in the diff (`REQ-EV-0071` PatchPolicyGate) | DENY | existing secret scanners |
| DI-6 | Formatting churn: whitespace-only or formatter-only hunks in files outside the plan beyond `formatting_churn_lines` (Alpha default: fifty) require a plan entry | FLAG | hunk classification |
| DI-7 | Dependency manifests and lockfiles change only with a plan entry and, when policy requires, a typed question | DENY without plan entry | manifest path set |
| DI-8 | Revision binding: the diff applies to the declared candidate revision; stale transactions are rejected (`20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md`) | DENY | existing precondition |
| DI-9 | Protected paths from policy (CI configuration, security and policy files, deployment descriptors) change only after a typed question | DENY | PolicyEnvelope protected surfaces |

DI-3 is the anti-gaming invariant: an agent cannot "make the tests pass" by editing the tests. It applies unchanged during benchmark runs, and doc 63 scores any trial with a DI-3 violation as failed regardless of the runner's verdict. Detection is conservative by construction: when the engine cannot classify a test-file change it flags it and asks, it never lets it through.

## 5. Scope bounds

Scope is bounded in `28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` §3 (ScopePolicy, PX-038): the first recorded plan freezes the original write set, expansions are counted against a policy bound and beyond the bound a typed question is mandatory. DI-1 is the mechanical side of that rule, and the scope metric of doc 63 is measured against the original plan, not the last revision.

## 6. Impact-based test selection from the evidence graph (PX-035, M3)

From M3.6 the TARGETED and COMPLETION stages select impacted checks from the dependency, symbol-reference, test-link and Git co-change evidence: tests that transitively depend on changed symbols or files within a bounded depth, tests historically co-changed with the changed files, and the task-named checks. Selection precision and recall are measured against full-suite ground truth on the fixture repositories and reported with the retrieval benchmarks of doc 53. Until PX-035 is `COMPLETE`, the M2 heuristics of section 1 apply and the plan states that targeting is heuristic.

## 7. Events, persistence, tools and conformance

- Events (`30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`): `VerificationBaselineRecorded`, `VerificationRunRecorded`, `FlakyCheckQuarantined`, `RegressionAttributed`, `DiffInvariantViolated`.
- Persistence (`31_DATABASE_AND_STORAGE_SCHEMA.md`): `verification_runs`, `check_results`, `flaky_checks`; reports and raw output are content-addressed artifacts.
- Tools (`17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md`): `test.run` returns the `TestReport`; `diagnostics.pull` is unchanged; no new tool family.
- Conformance (`56_TOOL_CAPABILITY_CONFORMANCE.md`): the Test suite proves structured parsing per adapter, `HEURISTIC` labelling, timeout handling, the rerun protocol and regression attribution on real fixture repositories that carry a seeded flaky test (`50_TEST_STRATEGY_REAL_SYSTEM_GATES.md`).
- Metrics (`63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md`): regression attribution, flaky-check rate, test-integrity violations, scope against the original plan.

## 8. Alpha defaults

Policy-overridable defaults for the Alpha release; each is revisited by Decision Record after the PX-020 baseline exists. They are configuration, not claimed results.

| Setting | Alpha default |
|---|---|
| `baseline_budget` | full configured suite when its last known duration is at most ten minutes; otherwise task-named and planned-file checks, stated in the plan |
| `flake_rerun` | one isolated rerun of the failed checks |
| `mandatory_flaky_passes` | three consecutive isolated passes |
| `completion_scope` | full configured suite plus every mandatory check |
| `targeted_run_budget` | five minutes wall clock per TARGETED run, then the run reports `TIMEOUT` for the remainder |
| `formatting_churn_lines` | fifty |

## Relationship to other documents

Doc 28 owns the competence contracts that consume these mechanics; doc 14 owns the harness that sequences them; doc 33 places them in the Verification Engine; doc 63 measures them; ledger rows REQ-PX-032..040 in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` schedule and qualify them.
