# Competence baseline — gui-regression v1.0.0

- task list digest: `02bdb743e6da71c8df599409f0f4a09dda7f27743955deee5039f4ab743d9d0b`
- bundle digest: `f7a8c9196a01d19701b1ff8f5632cd019ed605d501a497634b12ddcb6d408cca`
- protocol: 1 trials/task, max_turns 24, openai `gpt-5-mini` at 127.0.0.1:64406 (direct)
- Modbit revision: `fc367f897f46ac5e3eab9c5a0fff703b86464bea`; build digest `2d5b5f19f197`; harness competence-baseline/1

| metric | value | 95% interval |
|---|---|---|
| verified success | 24/24 = 1.000 | 0.862–1.000 |
| first-pass success (zero RepairAttempts) | 24/24 = 1.000 | 0.862–1.000 |
| first-candidate success (at most one repair attempt, no escalation) | 24/24 = 1.000 | 0.862–1.000 |
| repair loops (attempts→trials) | {0: 24} | — |
| equivalent-hypothesis escalations | 0 (0.00/trial) | — |
| no-progress escalations | 0 (0.00/trial) | — |
| wrong-effect attempts blocked | 0 (0.00/trial) | — |
| evidence coverage | 24/24 = 1.000 | 0.862–1.000 |
| cost | total 0.0616 USD; per verified success Some(0.0025651250000000006) | — |
| edits without retrieval record | 0 | must be 0 |
| files outside original plan / expansions / questions per trial | 0.00 / 0.00 / 0.00 | — |
| regressions attributed | 0 | — |
| flaky quarantines | 0 (0.00/trial) | — |
| DI-3 violations | 0 | — |

| task | verified | first-pass | states |
|---|---|---|---|
| gui_element/files-archive-cancel | 1/1 | 1/1 | ReadyForReview |
| gui_element/files-archive-confirm | 1/1 | 1/1 | ReadyForReview |
| gui_element/ledger-copy-invoice-total | 1/1 | 1/1 | ReadyForReview |
| gui_element/notes-bold | 1/1 | 1/1 | ReadyForReview |
| gui_element/notes-set-text | 1/1 | 1/1 | ReadyForReview |
| gui_element/orders-clear | 1/1 | 1/1 | ReadyForReview |
| gui_element/orders-express | 1/1 | 1/1 | ReadyForReview |
| gui_element/orders-quantity-four | 1/1 | 1/1 | ReadyForReview |
| gui_element/orders-submit-for-ada | 1/1 | 1/1 | ReadyForReview |
| gui_element/settings-dark-mode | 1/1 | 1/1 | ReadyForReview |
| gui_element/settings-font-apply | 1/1 | 1/1 | ReadyForReview |
| gui_element/settings-notifications-off | 1/1 | 1/1 | ReadyForReview |
| gui_input/calculator-clear-then-seven | 1/1 | 1/1 | ReadyForReview |
| gui_input/calculator-two-plus-three | 1/1 | 1/1 | ReadyForReview |
| gui_input/ledger-tab-to-reference | 1/1 | 1/1 | ReadyForReview |
| gui_input/ledger-type-amount-shortcut | 1/1 | 1/1 | ReadyForReview |
| gui_input/notes-save-shortcut | 1/1 | 1/1 | ReadyForReview |
| gui_input/notes-type-and-save | 1/1 | 1/1 | ReadyForReview |
| gui_input/notes-type-text | 1/1 | 1/1 | ReadyForReview |
| gui_input/orders-customer-by-typing | 1/1 | 1/1 | ReadyForReview |
| gui_input/orders-stamp-by-click | 1/1 | 1/1 | ReadyForReview |
| gui_input/orders-submit-by-click | 1/1 | 1/1 | ReadyForReview |
| gui_input/search-click-then-type | 1/1 | 1/1 | ReadyForReview |
| gui_input/search-type-enter | 1/1 | 1/1 | ReadyForReview |

## Axes of suite gui-regression (PX-179)

- bundle digest: `f7a8c9196a01d19701b1ff8f5632cd019ed605d501a497634b12ddcb6d408cca`
- gate: an axis fails when its verified-success rate is more than 0.05 below the baseline's
- output tokens per thousand tasks: 373333 (8960 output tokens over 24 trials, 280 model calls); 10.0486 USD per thousand tasks at catalog list prices
- source: per trial: GetTaskEconomics of the task and every child it admitted, equal to the ModelUsageRecorded events of the trial's log
- per-task gate: a task that verified in the baseline and does not now fails the gate, named

| axis | tasks | verified | 95% interval | output tokens / 1000 tasks |
|---|---|---|---|---|
| gui_element | 12 | 12/12 = 1.000 | 0.758–1.000 | 370667 |
| gui_input | 12 | 12/12 = 1.000 | 0.758–1.000 | 376000 |

