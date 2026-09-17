# Task Card — M7.4 Semantic actions and postconditions

## Identity

- Task ID: M7.4
- Milestone: M7 Live browser (P0)
- Requirements: docs/22 "Action hierarchy" (derived semantic actions on entities over primitive CDP), "Verification" (actions declare postconditions — URL/state fingerprint change, DOM/AX value; protected external actions require evidence before the receipt closes), "Prompt-injection isolation", docs/17 (`browser.action`: protected "Yes by effect"), REQ-EV-0280 (observed transitions recorded as evidence), REQ-EV-0278 (never act on a stale reference).
- Qualification: docs/51 E2E-013's action half — the agent acts through semantic IDs, fills and submits a form, the browser view visibly changes in the same session, a postcondition verifies the DOM/URL result; QUAL-EV-0280's transition evidence.
- Evidence tier: real-system (the real Core with a fake host; the real app on a real form with the approval decided on the card)

## Goal

Let the agent act on the page by the references it holds — as a person would, never on a node whose identity moved — under the effect the action really carries, and make every action answer for itself: the state after, what changed, whether what it promised held.

## Existing-code audit

- classification: MISSING before this task: the agent could read pages (M7.1–M7.3) and nothing else; the pipeline judged every call by its tool's registered effect class.
- production entry points: `crates/tools/src/registry.rs` (`Tool::effect_of`, default the registered class); `crates/tools/src/pipeline.rs` (the call's class from `effect_of`, never below the registered one; `InvokeContext.effect_class`); `crates/browser/src/compiler.rs` (`ActionRisk`, `PROTECTED_WORDS`, `classify_action`); `crates/browser/src/lib.rs` (`HostRequest::Act`, `HostResponse::Acted`); `services/modbit-core/src/browser.rs` (actions fenced by the control lease like navigations); `crates/tools/src/browser.rs` (`browser.act`: `KNOWN` entities for the synchronous classification, resolution by identity, `TARGET_STALE` / `TARGET_UNLOCATED` / `TARGET_DISABLED` / `ACTION_UNSAFE`, the host request, the read after, the delta, the postcondition checks, `POSTCONDITION_FAILED`); `crates/domain/src/task.rs` (`BrowserActionPerformed`); `services/modbit-core/src/tools.rs` (the event journaled beside the call); `services/modbit-core/src/runtime.rs` (an action is progress); `apps/desktop/src/main/browser.ts` (`act`: resolve, click at the box, fill, select, check, press, settle); `crates/tools/tool-matrix.json` (the `browser.act` row with its per-call effect note).
- proof: `qual_m7_4_actions_run_by_reference_under_the_effect_they_carry_and_check_their_postconditions` (real Core, fake host): the fill on the Email field runs without approval (`ReversibleWrite`), its value postcondition holds and the delta names the new value; the click on a dead Help link runs, its `url_contains` / `changed` postcondition fails and the call fails `POSTCONDITION_FAILED` with the action recorded; the click on a never-named reference is `TARGET_STALE` and the host receives nothing; the click on the form's Sign in button opens an approval (`browser.act`, `ExternalSideEffect`) before anything reaches the host, and once the person approves the exact intent it runs once — the host's page turns into the welcome page, `navigated`, the URL and text postconditions hold; the host received fill (node 5), click (node 8), click (node 7) and nothing else, each under lease generation 1; the log holds three `BrowserActionPerformed` with `ReversibleWrite` / `ReversibleWrite` / `ExternalSideEffect`, verdicts true / false / true and different fingerprints around the submission. `apps/desktop/e2e/browser.spec.ts` (real app, real form): Email and Password filled on the live page (the value postcondition holds; the delta carries the new value); the Sign in click stops the task `awaiting approval` naming `browser.act asks for a ExternalSideEffect effect`, no request has reached the fixture, the person approves on the card, the form's GET lands on `/welcome?email=ada%40example.test&pw=hunter2` with the welcome heading, `navigated: true`, the postconditions held, the Core's record and the person's view both on the welcome page.

## Limitations

- The risk classification is the first cut (a word list and form membership; IMP-EV-0088 refines it with page classification and origin changes); an unknown reference is judged page-only and refused at run time if it turns out protected, which costs the model one re-read.
- `select` is by option text or value only; drag, hover, file upload and scroll are not actions yet; `press` knows Enter, Tab, Escape, Backspace, arrows and Space.
- The settle after an action is a navigation the host sees start within 400 ms or a 150 ms quiet period; slower in-page updates are found at the next read.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_4_actions_run_by_reference_under_the_effect_they_carry_and_check_their_postconditions` (services/modbit-core, real Core)
- `apps/desktop/e2e/browser.spec.ts` (the M7.4 steps)
- `submissions_and_consequential_actions_are_protected_fields_and_tabs_are_not` (crates/browser)
- Regression: `qual_m7_1_…`, `qual_m7_2_…`, `qual_m7_3_…`, `qual_ev_0217_…` / `qual_ev_0230_…`, the pipeline tests

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
