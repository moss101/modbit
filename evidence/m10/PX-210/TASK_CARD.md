# Task Card — PX-210 Next-prompt suggestion: one suggested next message after a turn, accepted with Tab and never sent by itself

## Identity

- Task ID: PX-210 (REQ-PX-210; owner core-runtime); qualification QUAL-PX-210 / PX-E2E-210; Decision Record DR-PX-2026-11-11-021 (MWS-16)
- Evidence tier: release-critical (a new protocol message, a model call, a charge to the task's budget and a new event on the canonical log)
- Prerequisites: PX-200, DOC-PX-020. Neither is on `main` yet (the record and the rows arrive with moss101/modbit#196), so this row cannot be moved by `graph.py` and was not.
- Branch `wip/desktop-2-core-header-suggest`; built together with PX-218.

## Existing-code audit (before any edit)

- The side question, traced from the production caller to the provider boundary: the composer's panel -> `window.modbit.composerSideQuestion` -> `composer:sideQuestion` (`apps/desktop/src/main/composer-ipc.ts`) -> `CoreClient.askSideQuestion` -> `AskSideQuestion` (`services/modbit-core/src/server.rs`) -> `side::ask` -> `ProviderGateway::stream`. PRODUCTION-WORKING. It appends nothing and it is not charged: `usage.rs` says its invoice rows are `NOT_IN_LOG` by design, and every cost reader (`usage::calls`, `economics::view`, `SpendScan`) counts `ModelUsageRecorded` on the task's log and nothing else.
- Next-prompt suggestion: NOT-FOUND in the protocol, the Core, the client, main, the preload and the renderer.
- First missing link: no protocol message asks for one; and behind it, no way to charge a call that writes nothing to the task's log, because the ledger is the log (request accounting calls itself "not a second authority").
- Also seen and left alone: `side.rs` replays the last twelve raw messages, which can begin with a tool result whose call is outside the window; a provider that checks the pairing refuses that. The suggestion sends the end of the conversation as text instead.

## What was built

- Protocol (`surface.proto`, additive): `SuggestNextPrompt { task_id }` and `NextPromptSuggestion { text, task_offset, input_tokens, output_tokens, cost_known, cost_minor, endpoint, model, snapshot_messages, usage_offset }`; `TaskEconomicsView.suggestion_calls = 450`, `suggestion_cost_minor = 451`. Fixtures on both sides.
- Core, `services/modbit-core/src/suggest.rs`, beside `side.rs`: one request with no tools, an output cap of 96 tokens and a 20 s deadline, over the task's goal, plan and last eight messages as bounded text. The answer is cut to one line of at most 200 characters and redacted as a stream's text is.
- How it is charged: the usage is one `SuggestionUsageRecorded` (`crates/domain/src/session.rs`) on the session's aggregate, with no task lineage, naming the task in its payload and carrying tokens and the route, never the text. `usage::calls` makes it a row of the task's ledger (so `GetTaskEconomics` counts its tokens, its call and its price, and `ReconcileInvoice` finds it); `SpendScan::observe_off_log` adds it to what the task's cost cap is judged against at the next run, in the parent's own spend and in a child's hold. It is recorded before anything is delivered, and it is inside no run, so `runs` does not list it.
- Refusals, each a typed code with no model call: `SUGGESTIONS_OFF`, `TASK_ENDED`, `CHILD_TASK`, `TURN_RUNNING`, `NO_FINISHED_TURN`, `INPUT_QUEUED`, `ALREADY_ANSWERED`, `SUGGESTION_IN_FLIGHT`, `BUDGET_EXHAUSTED`, `BUDGET_WOULD_EXCEED` (the worst case is one token per byte of input plus the output cap, at the registry's prices), `COST_UNMETERED`, `MODEL_NOT_ALLOWED`, `LEASE_REQUIRED` / `STALE_LEASE`. After the call, with its charge kept: `TASK_MOVED`, `EMPTY_SUGGESTION`.
- Two switches. The person's: "Suggest the next message" in the composer's menu, on by default, stored with the composer's preferences (`modbit.composer.v1`); off, the composer never asks. The Core's: `MODBIT_NEXT_PROMPT_SUGGESTIONS=off`; off, the Core refuses and calls no model.
- Desktop: `CoreClient.suggestNextPrompt`, `composer:suggest` (validated ids, the lease, the Core's refusal handed back as an answer), `window.modbit.composerSuggest`, and in the composer: asked once when a turn is seen to end with nothing queued and nothing typed; shown as the placeholder with a "Tab to use this suggestion" hint; Tab puts it in the box; any typing, a new turn, a queued message or the setting going off withdraws it. Existing test ids are unchanged.

## Verification

All local, macOS 27.0.1 arm64, rustc 1.97.1, node 26.11.1, Electron 44.3.0, Playwright 1.63.0. The model is the repository's scripted stand-in, which records every request; the Core, its SQLite store, its socket, Electron main, the preload and the renderer are real.

- `cargo test -p modbit-core --test px_210_next_prompt`: 3 passed (`px210-core.log`).
  - A suggestion arrives after a finished turn from one request with no tools. The task's log is byte-identical: a SHA-256 over every column of every row of the task, read from `core/core.db`, is the same before and after, across a Core restart and after two more suggestions. The session's log gains exactly one record, on the session's aggregate, with no task lineage and only usage and route keys; the suggested text is nowhere in the log. The header, the conversation rows and the task's state are unchanged; the session's cursor moves by one.
  - The cost is in the task's economics (`model_calls`, tokens, `cost_minor`, `priced_calls`, `suggestion_calls`, `suggestion_cost_minor`), is the same after a restart, and `runs` is unchanged.
  - Budget: at the cap `BUDGET_EXHAUSTED`; one unit under it `BUDGET_WOULD_EXCEED`; with room it is made and `cost_remaining_minor` shrinks by it; a cap that the turns alone would fit under stops the next run with `HarnessBudgetExhausted` (`max_cost_minor`, used = turns + suggestions) and no model request.
  - Refusals with the model's request count unchanged: a turn running (the model holding its answer), an input queued, no lease, a stale lease, a task that has not run, an unknown task, a capped task that cannot be priced, a cancelled task, a repeated command id.
  - With `MODBIT_NEXT_PROMPT_SUGGESTIONS` = `off`, `0`, `false`: `SUGGESTIONS_OFF`, no request, no record; with `on`, a suggestion.
- `cargo test -p modbit-core --bin modbit-core suggest`: 5 passed (the budget gate, the task gate, one line without its wrapping, the bounded snapshot, one call at a time per task).
- Desktop end to end, `pnpm --filter @modbit/desktop exec playwright test -c e2e/playwright.config.ts px-210-suggestion`: 3 passed (`px210-e2e.log`), and 12 of 12 with `--repeat-each 4`. The suggestion is the composer's placeholder after a turn; Tab fills the box and nothing is sent (no message, nothing queued, no request); typing dismisses it and it does not return; a turn that ends with a message queued gets none; with the person's setting off the model receives no extra request across three turns and a reload, and on again the next turn brings one; with the Core's switch off, none. axe clean on the composer with a suggestion.
- Desktop unit: `pnpm --filter @modbit/desktop test` 354 passed, 4 of them new (the setting's default and persistence, when a suggestion is due, what Tab takes, the handler's lease, mapping and refusals).
- `composer.spec.ts`, `conversation.spec.ts`, `workers.spec.ts`, `keyboard.spec.ts`, `approvals.spec.ts` on the tree merged with `main`: 27 passed, 1 skipped (`live-approvals`, which needs a real provider) (`e2e-composer-conversation.log`).
- Negative proofs kept: `mutation-check-2026-11-11.txt`. Ten mutations, each making its test fail: the charge put on the task's log (caught twice: by the Core's own `TASK_MOVED` guard, and with that guard off by the test), a queued input ignored, a running turn ignored, the cap ignored, the charge not counted by the cap, the Core's switch ignored, the person's setting ignored, Tab that also sends, typing that does not dismiss.
- The Core's full suite, `cargo test -p modbit-core --locked --no-fail-fast`, on the final Core code (28da0141; no Rust or proto file changed after it): 684 passed, 0 failed, 8 ignored (a release benchmark and seven macOS-helper tests that need Accessibility and Screen Recording for the test process), 46 test binaries, 14 minutes (`core-suite.log`). An earlier run at bcb6f06c, before the task gate became a pure function: 682 passed, 0 failed, 8 ignored.
- The workspace, `cargo test --workspace --locked --no-fail-fast` at c7dc463d: 1863 passed, 0 failed, 27 ignored, across 243 test binaries and 42 doc-test runs, 29 minutes. It ran before the task gate was reshaped in `suggest.rs` and before `origin/main` was merged; the Core's suite above was rerun after both, and no other crate changed.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked` (0 warnings), `tools/lint_policy.sh`, the four `architecture-lint` checks, `pnpm typecheck`, `pnpm test`: all exit 0.

- Real screenshots of the running app: `after/composer-suggestion-light.png`, `after/composer-after-tab-light.png`, `after/composer-long-suggestion-dark-900.png` (a long suggestion is clipped before the hint, never under it).

## Limitations

- "Writes nothing" (the record's section 9) holds for the task's log and for the suggested text. It does not hold for the session's log: the charge the same rule requires is one record there, so the session's cursor moves by one per suggestion and subscribed clients receive that record. This is the one place the two requirements meet; it needs the owner's eye.
- The side question is still not charged. That is how it was built (QUAL-EV-0261) and this change does not alter it.
- Request accounting (`GetRequestOutcome`) is derived from a task's runs and does not list suggestion calls; the economics, the ledger, the invoice reconciliation and the cost cap do.
- The suggestion uses the model the task last ran on. There is no cheaper model for it.
- A subagent's task gets no suggestion (`CHILD_TASK`): its budget is its parent's slice, which its own log does not state. That refusal is proven by a unit test of the gate, not on a real Core.
- The composer asks only when it sees a turn end by itself: not when a conversation is opened later, and not after the person stopped the turn or the model was refused.
- The person's setting belongs to the desktop. Another client asks or does not ask on its own; the Core's switch is the operator's.
- The rest of the desktop suite runs with the Core's switch off (`e2e/playwright.config.ts`), because its scripted models answer by request order. So only `px-210-suggestion.spec.ts` exercises the feature end to end.
- PX-E2E-210 asks for the budget at its cap from the desktop. A priced registry cannot be activated through the desktop's bridge, so that leg is the Core test's; the desktop shows nothing for any refusal, which the Core-switch test covers.
- The preload changed (one channel), so `hardening-guard.test.ts`'s pin was updated with a note. That pin exists to make such a change reviewed; the review is the pull request's.
- A repeated command id is refused, not replayed, because the first answer is not kept.
- macOS only so far. Linux and Windows are CI's.
- Not sealed: the row is not on `main`, and sealing is not this task's.
