---
id: DR-PX-2026-10-25-017
title: The coding task runs autonomously inside the Capability Kernel's envelope — attended tasks start Supervised, AUTONOMOUS is a durable preset, the Core verifies and concludes repairs itself, turn budgets are windows, and an Autonomous task resumes after a crash
status: accepted
date: 2026-10-25
supersedes: scoped — the default mode of AFW-F06, the acknowledgement list of AFW-F07 and the network clause of AFW-F08 in doc 65; the Alpha defaults of docs 14 (contract 5) and 28 (RepairPolicy); nothing else
approved_by: owner, chat instruction of 2026-10-25 ("Transform Modbit from a partially autonomous coding assistant into a reliable autonomous software engineer ... Implement, test and verify ... Follow existing decision-record requirements. Do not weaken locked architectural invariants ... Execute confirmed, authorized changes now")
---

# DR-PX-2026-10-25-017 — Autonomous coding inside the envelope

> **Accepted 2026-10-25** on the owner's chat instruction. The approval covers the seven decisions of section 5 and nothing outside them. It does **not** cover: any change to the always-ask classes for a secret, a deletion, a push, a write outside the workspace, a protected or configuration path, an escalation, or a network reach that is not the install of the project's own declared dependencies; any widening of the capability envelope, a lease, a profile ceiling or the emergency stop; spending on a live model provider (a live run needs its own instruction naming the provider, as DR-M9-002 records for z.ai; section 9 records the one the owner gave); or the release gates A to G and M10, which this record does not attest.

## 1. Trigger and evidence

The owner asked for Modbit to behave, for "Fix my broken application", as an autonomous software engineer: investigate, edit, build, detect errors, repair, retest and deliver verified results without unnecessary interruptions, on the existing Context Engine, Policy Kernel, Agent Runtime and Model Gateway, and listed ten findings of an earlier audit to verify before fixing anything. Each was traced from its production caller to its effector at `HEAD` `076c1b70` (branch `wip/zcode-design-language`) and, where the claim is about behaviour, reproduced by running the real classifier or the real Core.

| # | Claim | Verdict | Evidence (at 076c1b70) |
|---|---|---|---|
| 1 | ASK mode ignores saved permission rules | **Confirmed, by design, and fatal by default.** `RunPolicy::decide` returns `Ask` for `RunMode::Ask` before any rule is read, and the Core gave every task `Ask` when nobody had set a mode (`run_control.rs`, `mode_of`). | `crates/policy/src/runmode.rs` `decide`; before-fix integration test would have asked `MODE_ASK` with a covering rule |
| 2 | The "Always" button saves ineffective approvals | **Confirmed.** `approval:allowAlways` made a rule and approved the one effect; with the default `Ask` the rule never applied, and the card's own note said so ("It applies when the run mode is Allowlist or higher"). Every Always rule also had no expiry. | `apps/desktop/src/main/control-ipc.ts`, `approval-dock.tsx` |
| 3 | Repeated tool calls need unnecessary approval | **Partly confirmed.** Routine calls (read, edit, build, test, `npm run`, a script of the project) are `ReversibleWrite` and never ask; an approval is single-use by design. What asked again and again was a protected-effect command with no effective rule (2), a network install (4) and an unlisted toolchain (5). | `crates/tools/src/shell_class.rs`, probe below |
| 4 | Dependency installation always prompts | **Confirmed.** Probe over the real classifier: `npm ci`, `npm install`, `pnpm install`, `yarn install`, `bun install`, `cargo fetch`, `go mod download` and `pip install -r` are `ExternalSideEffect`, which the host maps to the `NETWORK` always-ask class; it asks in every mode, `RUN_EVERYTHING` included. | `ask_classes` in `crates/policy/src/runmode.rs` |
| 5 | Unknown project scripts get a restrictive classification | **Partly confirmed.** A project script (`./scripts/build.sh`, `node x.js`, `make`, `npm run build`) is already `ReversibleWrite`. A front-end tool the table did not list (`tsup`, `svelte-check`, `ng`, `stylelint`, `sass`, `tailwindcss`, `postcss`, `oxlint`, `knip`, `uvicorn`, `staticcheck`, `rubocop`) was "unclassified", which is `ProtectedWrite` under `local_trusted` and asks. `npx <tool>` is a network effect (it can download the package). | probe over `classify_argv` |
| 6 | The default 60-turn / 300-tool budgets interrupt tasks | **Confirmed, and worse than stated.** The desktop sends no limits, so every task got 60 and 300; the counts are cumulative over the task's log, so a task resumed after `BUDGET_EXHAUSTED` with the same limits met the used-up count again at once; the tool-call budget ended a run only by refusing each call with `BUDGET_EXHAUSTED`. | `harness.rs` `check_turn_budget`, `rebuild` |
| 7 | Verification and repair depend on the model choosing the tools | **Confirmed.** The Core ran a verification stage only inside `verify.run` and `task.complete`; a repair attempt was concluded only by `verify.run`. The derived plan for a Node project held only a test runner, so a TypeScript project with a type error and no test script had no check at all (and completion was INDETERMINATE until the user waived it); the package manager was assumed to be `pnpm`. | `run_loop`, `crates/verification/src/plan.rs` |
| 8 | Crash recovery requires a manual resume | **Confirmed.** Recovery suspends the run; only a task the person chose to resume when quitting (`EndSession`) was named for the client to resume. | `session_end::resume_requests` |
| 9 | Execution permissions reset unexpectedly | **Confirmed for `RUN_EVERYTHING`** (process-local by design, AFW-F07) and for the default after the fix of (1) there is nothing left to reset; the durable modes and rules survive. No preset that lets a task go on unattended survived a restart. | `run_control.rs` fold |
| 10 | No live-model desktop-to-completion proof exists | **Confirmed.** No retained run drives the real Electron renderer with a real model through to a verified completion. This record does not close it; section 9 states how it is closed. | `evidence/` |

## 2. Goal and exit criteria

**Goal AUTO.** "Fix my broken application" is carried to a verified result by Modbit itself, inside the envelope, with only the interruptions the always-ask classes require. Done when, on one commit, each item has a retained artifact:

1. **Approval friction (PX-144).** An attended task asks for what no rule covers and for nothing routine; a rule made by Always takes effect; the AUTONOMOUS preset runs builds, tests, unknown project tools and manifest installs unasked and still asks for every other always-ask class.
2. **Autonomous repair (PX-145).** A seeded TypeScript build failure and a two-step failure are repaired through the Core's own verifications with no `verify.run` call by the model, no approval, and every attempt on the log.
3. **Recovery and budgets (PX-146).** A task survives a killed Core with its permissions and its notice; a used-up window can be continued by a person and, in AUTONOMOUS while progressing, by the Core, a bounded number of times.
4. **Live proof.** One run of the real desktop against a real provider (section 9). Until it exists the feature depth of the whole is WIRED and E2E_PROVEN at the Core boundary, not COMPLETE.

## 3. Options

| Option | Verdict |
|---|---|
| A. A classifier that decides whether a command is "safe" and approves it | **Rejected** (AFW-F06, doc 81). The classifier only states a typed fact; a person-set, acknowledged mode approves. |
| B. Make `RUN_EVERYTHING` carry installs and survive a restart | **Rejected.** Its contract is "per process, acknowledged every time"; changing it would weaken a locked invariant. A new preset leaves it as it is. |
| C. Seed `covers_always_ask` rules for the install commands when Autonomous is chosen | **Rejected.** A rule is a literal prefix: `npm install` would cover `npm install <anything>`. A typed predicate over the whole command is stricter. |
| D. A second verification engine or repair controller in the loop | **Rejected** (doc 81). One stage runner, one attempt-conclusion path, one event log; the loop calls them. |
| E. Chosen: presets and facts over the existing owners (`policy`, `tools`, `verification`, `core-runtime`, the Core's runtime), seven decisions below. | |

## 4. Current behaviour

Section 1.

## 5. Decisions

**D1 — Attended tasks start Supervised (`ALLOWLIST`).** `ASK` stays a mode a person can choose, and it ignores rules, as AFW-F06 says. The Core's default for a task of origin `desktop`, `cli`, `ide_adapter` or `fork` becomes `ALLOWLIST`; every other origin (forge, review, replay, subagent, automation) stays `ASK`, so a rule a person made for their own sessions never approves an unattended run's effects. `ALLOWLIST` with no rule asks exactly as `ASK` does (the reason code is `NOT_IN_ALLOWLIST`), so the change widens nothing: it makes a rule the person made take effect. The desktop's "Always" is withheld on a card from a task in `ASK`, with the reason, and refused by main before a rule is made (`ALWAYS_INEFFECTIVE`); a repository-wide Always rule now expires in 30 days. User-facing names: `ALLOWLIST` is shown as **Supervised**; Read-Only is the existing task mode `ASK` (PX-051: no write, no execution), which is a different axis and unchanged.

**D2 — `AUTONOMOUS`, a fifth run mode.** It approves what `RUN_EVERYTHING` approves (an in-envelope effect outside the always-ask classes: an unknown project tool, a protected write that is not in a class) and, additionally, passes the `NETWORK` class for a call the tool's classifier found to be only the install of the project's declared dependencies (D3). It is durable: the `RunModeSet` record and its acknowledgement are on the log and survive a restart. Setting it from a mode that approves less needs the acknowledgement of AFW-F07, whose text now names the install. Everything the kernel decides before the run mode (emergency stop, lease and its resource selectors, profile ceiling, denied capabilities, task-mode posture, configuration `DENY`/`ASK`, computer control) is untouched: the mode is consulted at the one step it always was.

**D3 — A typed predicate, not a judgement: `dependency_install`.** The shell classifier (the single owner of argv → effect class) reports, beside the class, whether every network reach in the command is the install of the project's own manifest: `npm`/`pnpm`/`bun` `install`/`i`/`ci`, `yarn`/`yarn install`, `cargo fetch`, `go mod download`, with no package named, no global/prefix/registry/index/config flag, no environment override (a call's `env`, an `env` wrapper or a leading assignment), and no other network program, verb, push, deletion or secret in the command. `npm install left-pad`, `pnpm add`, `npx`, `pip install`, `curl` and a publish are unchanged. Only the AUTONOMOUS mode reads it. Residual risk, stated: an install runs the lifecycle scripts of the project's dependencies, as `cargo build` runs `build.rs`; the project's own manifest (which the model can edit, visibly, in the diff) decides what is fetched.

**D4 — The classifier knows the toolchains a project installs.** `tsup`, `tsc-alias`, `vue-tsc`, `svelte-check`, `ng`, `stylelint`, `sass`, `tailwindcss`, `postcss`, `swc`, `babel`, `parcel`, `oxlint`, `knip`, `depcheck`, `madge`, `vitepress`, `storybook`, `uvicorn`, `gunicorn`, `alembic`, `staticcheck`, `goimports`, `gotestsum`, `rubocop`, `rails` join the build table. Programs that can publish or deploy (`expo`, `gatsby`, `remix`) were considered and left out.

**D5 — The Core verifies the model's edits and concludes the repair.** At the end of any turn in which a write landed, when a baseline exists, the plan holds a mandatory check and the task is not a reviewer, replay or subagent, the Core runs a TARGETED stage by the one stage runner (`run_verification`), concludes the pending repair attempt by the one conclusion path (`conclude_repair`), and delivers a notice to the model: the engine's observation, the open failures, the attempts already made on them and their outcomes, the lines of source the diagnostics name (read from the workspace, never outside it or under `node_modules`, `target`, `.git` or `dist`, never through a link), and the next step. The notice is the event `AutoVerificationDelivered` on the task's records aggregate; the transcript rebuild replays it, so a resumed run reads what the live one read. It selects, skips and weakens no check, runs at most 60 times per task, and `MODBIT_AUTO_VERIFY=0` turns it off for a Core. The derived plan now names a Node/TypeScript project's type check (`typecheck`-style script, else the project's own `tsc --noEmit` when installed), `build`, an advisory `lint`, and its test script (placeholder `npm init` test scripts are not a check), with the package manager the project uses (`packageManager`, else the lockfile, else npm), and a Go module's `go build`, `go vet` (advisory) and `go test`. The repair policy's per-task bound is 12 attempts (was 6): a build reports one failure at a time, and an equivalent hypothesis is still refused, the per-signature bound is still 2, and a `WORSENED` attempt is still reverted.

**D6 — Budgets are windows.** Defaults: 200 turns and 1000 tool calls (were 60 and 300), settable per Core by `MODBIT_DEFAULT_MAX_TURNS` / `MODBIT_DEFAULT_MAX_TOOL_CALLS` and per request, never above 1000 and 5000. The counts bound a window, not the task's life: a run that ended on `max_turns` or `max_tool_calls` is continued by a person's `StartTask`, and an AUTONOMOUS run that is still making progress (no consecutive no-progress turn) is given the next window by the Core and says so, at most 4 windows per task in all. Cost and wall-clock caps are never windowed. The tool-call window is judged at the turn boundary like the others.

**D7 — An AUTONOMOUS task resumes after a crash.** The Core names, with the tasks a person asked to resume, the AUTONOMOUS tasks whose run a restart suspended, unless the session was ended on purpose after they last started or `MODBIT_AUTO_RESUME=0`. The client resumes them with its ordinary `StartTask`, which judges the lease, the provider, the envelope, the mode and every rule's scope and expiry afresh, and reconciles a call the dead Core had dispatched with its target instead of replaying it. Nothing resumes without a client.

**D8 — A subagent inherits neither the mode nor a window (follow-up, 2026-10-25).** A child runs inside its capsule's effect ceiling (REQ-EV-0046) and a slice of its parent's budget that is reserved against the parent, so what the parent's person chose for the task they attend does not widen it. Stated as three guards, each tested with a real child (`qual_px_146_a_subagent_does_not_inherit_autonomous_or_a_budget_window`): a child's mode is `ASK` whatever its parent's is (D1); `SetRunMode` to `AUTONOMOUS` on a task of origin `subagent` is refused (`UNATTENDED_SUBAGENT`); and a child never opens a budget window, neither by the Autonomous extension (D6, already gated on the capsule) nor by a replayed `HarnessBudgetExhausted` (new: a resumed child would otherwise spend again what its parent has already been charged). The Core's automatic verification (D5) stays off for a child, as before.

**D9 — The repair attempts are shown (follow-up, 2026-10-25).** `RepairAttemptView` and `ReviewBundle.repair_attempts = 362` are additive: the Core folds the task's own `RepairAttemptRecorded`, `RepairAttemptConcluded` and `RepairEscalated` events into the review bundle the Evidence app already reads, and the app lists each attempt with its hypothesis and the outcome the Core recorded (open, RESOLVED, PARTIAL, UNCHANGED, WORSENED, reverted) and a stopped loop as a stop. No new state, store or channel; the preload type gains one field and its pinned hash moves with this record's review.

## 6. Migration

No schema or protocol change. `RunMode` gains the string `AUTONOMOUS`; older logs hold none. The run-control proto comment is updated. Existing rules and modes keep their meaning; a task that never had a mode set is `ALLOWLIST` (attended origins) from this commit, which asks exactly as `ASK` did while no rule exists.

## 7. Compatibility

Additive: two event types (`AutoVerificationDelivered`, `BudgetWindowExtended`) on the records aggregate, read only by the transcript rebuild; a new mode string; `CallFacts.dependency_install` and `PolicyRequest.dependency_install`. A client that only knows four modes shows the fifth by name.

## 8. Security impact

The always-ask classes for a secret, a deletion, a push, a write outside the workspace, a protected or configuration path and an escalation ask in AUTONOMOUS as in every mode (tested, against the real Core). The one relaxation is D3, reachable only by a person's acknowledged choice of AUTONOMOUS, only for a command the classifier proves to be a manifest install, and recorded on the effect receipt as `run_mode:AUTONOMOUS:dependency_install`. Mutation checks: ignoring the classifier's flag fails the kernel test; disabling the automatic verification fails all three real-Core repair tests. Auto-resume cannot start a run without a client and its lease. Prompt-injection posture: a notice built from tool output is data in a Core-labelled user message, and the source quoted in it is read from the workspace under the same path rules as `fs.read`.

## 9. Test impact and the live proof

Changed: the default-mode assertions of `qual_px_057_*` in `services/modbit-core/tests/run_control.rs` (the default is now `ALLOWLIST`, `ASK` is chosen explicitly where the old text relied on it), the Supervised labels in the desktop specs, the policy-default assertion in `harness.rs`. Added: kernel, classifier, plan-derivation and window tests; real-Core tests with a real `tsc`, `npm` and execd; the desktop model and IPC tests. **The live proof is not part of the approval of this record**, and was authorized separately: on 2026-10-25 the owner answered the question "which credential from this environment may I use" with the `ZCODE_API_KEY` (z.ai) credential, for one live proof of PX-145 on `glm-5.3-flash` under DR-M9-002's rule that a compatible gateway is named as such. The owner named no cost cap. The run is bounded instead by 80 turns and 20 minutes of wall clock, because a cost cap cannot meter this binding (no active model registry prices it, and the Core stops a capped run on unmetered spend, fail closed; observed, run 2). The key was passed to the Core process by environment and appears in no retained file. Retained under `evidence/m10/PX-145/`: five runs of `apps/desktop/e2e/live-autonomy.spec.ts` (real Electron, real Core, real `tsc` and `npm`, the instruction "Fix my broken application.", Autonomous mode, a seeded application with a type error, a wrong-argument call and a logic error that only its test catches):

| Run | Endpoint | Result |
|---|---|---|
| 1 | `api.z.ai/api/paas/v4` | provider refused: HTTP 429, code 1113, no balance on that resource package (the key is not for the general API); nothing ran |
| 2 | `api.z.ai/api/coding/paas/v4` with a $1 cost cap | Core stopped the run after one call: `cost_unmetered` (fail closed, as designed); nothing edited |
| 3 | coding endpoint, wall and turn bound | ReadyForReview in 512 s; 12 model calls; 2 Core verifications; 1 repair attempt (RESOLVED); build and tests pass; test file unchanged; 0 approvals |
| 4 | same | ReadyForReview in 461 s; 13 calls; 2 verifications; 0 repair attempts; build and tests pass; test file unchanged; 0 approvals |
| 5 | same | ReadyForReview in 843 s; 18 calls; 2 verifications; 1 repair attempt (RESOLVED); build and tests pass; test file unchanged; 0 approvals |

Three of three runs on a working endpoint reached a verified result with nothing asked. That is n = 3 on one model and one small application, a measurement of the path with a real model, not of how often a model succeeds on real repositories; the second COMPLETION stage in each run is the gate repeating the checks.

**The approval deck with the same real model (follow-up, same day, same credential and bounds).** `apps/desktop/e2e/live-approvals.spec.ts`: the default Supervised mode, the same seeded application, and an instruction whose natural steps include a command no rule covers; the test plays the person at the deck. Retained under `evidence/m10/PX-144/`:

| Run | Instruction adds | Result |
|---|---|---|
| 1 | commit after setting a git identity with `git config` | ReadyForReview in 444 s; 11 calls; 2 real cards (`git config user.name`, `git config user.email`, "protected write", the Core's reason shown), each approved once; each approval is named in its receipt; build and tests pass; test file unchanged; 1 repair attempt RESOLVED. `git config` is a protected path, which never offers Always, so no rule was made |
| 2 | two `git remote add` commands | the test chose the bare prefix `git`; the Core refused the rule (`PATTERN_TOO_BROAD`), the second command asked again and was approved once; ReadyForReview in 342 s. A flaw in the test's prefix choice, kept as a result: the refusal is the Core's, through the real UI |
| 3 | same, prefix `git remote` | ReadyForReview in 213 s; 9 calls; ONE card (`git remote add backup`), Always made one task rule, the second `git remote add mirror` ran with no card and its receipt names the rule (`allowlist:<rule id>`); build and tests pass; test file unchanged |

So a real model reached a protected effect, the deck showed it with its exact command and reason, a person's Always made a rule that took effect (the defect finding 2 described), and the run continued. n = 1 for the reuse leg, one model, one command family. Not covered by any live run: a larger repository, other models, Windows and Linux, a cost-capped run (the binding is unmetered), repair-attempt display against a live run (the Electron test covers it with a scripted provider).

## 10. Rollback

Revert the commit; no data is migrated. `MODBIT_AUTO_VERIFY=0`, `MODBIT_AUTO_RESUME=0` and `MODBIT_DEFAULT_MAX_*` turn the three runtime behaviours down without a revert, and choosing `ASK` per task restores the old default.

## 11. Risks and open items (not decided here)

- A repository's `package.json` scripts run through the derived plan (as `pnpm test` always did); a malicious script is repository code, not an effect the kernel can see. Real confinement is `review_isolated` / `cloud_isolated`.
- Heuristic evidence: the build, type check, lint and Go checks are exit-code based and labelled so; the completion gate treats an unestablished outcome as INDETERMINATE.
- Windows and Linux are CI_COMPATIBLE only for these paths; the new Core tests are `cfg(unix)` because they drive `sh` shims and a POSIX `tsc` wrapper.
- A live-model measurement of how often the notice leads to a fix is not made here (PX-145's live leg).
