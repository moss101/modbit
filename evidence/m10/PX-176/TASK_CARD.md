# PX-176 task card — one structured observation per computer-use step

Decision Record: DR-PX-2026-10-26-018 (D1, D2). Evidence tier: release-critical (it changes what the model is told by an effect-bearing tool loop and when a run can complete).

## Audit (before)
Classification: DOCUMENTED-ONLY for the record, IMPLEMENTED-PARTIAL for its parts. A computer-use child held `shell.exec` and `computer.*` in one loop, but each result was its own differently shaped text: a command's streams and exit code in one shape, an interface action's `changes` and snapshot in another, the frame token only on screenshots. A timed-out `shell.exec` was handled as a check failure: it opened a failure signature that made `task.complete` refuse with OPEN_FAILURES, so the run could not complete. Command results did not count as progress for the "3 turns without progress" harness rule, so alternating commands and clicks tripped it. First missing link: nothing joined the pieces per step.

## Change
- `services/modbit-core/src/computer.rs`: `ComputerHost::step_observation(task, tool, status, out)` returns, for a computer-use child and a `computer.*` or `shell.exec` result, one record: `step` (1-based), `tool`, `kind` (`gui` | `command`), `status`, `frame` and `snapshot` (the ones the child currently holds, carried across command steps), `tree_delta` (for `computer.state` the added/removed accessibility lines against the previous tree, bounded to 40 lines each; for an input the runtime's own `changes` and `structure_changed`; null for a command), `stdout`, `stderr` (each bounded to 4000 characters and ending with `[…truncated by the Core]` when cut), `exit_code`, `timed_out` and `truncated`. Only these named fields are copied; the tree text is already masked by the PX-076 pipeline.
- `services/modbit-core/src/runtime.rs`: the record is appended to the step's tool result as `observation: {...}` (so it is part of the logged result object a restart replays); a child's `shell.exec` result counts as progress; a child's `shell.exec` is not a check, so a timeout or failure is an error observation and not an open failure that blocks completion.

## Proof
`services/modbit-core/tests/px_computer.rs::qual_px_176_every_step_of_a_computer_use_child_is_one_structured_observation`: real Core process, real fixture actuator process, real approvals, real `git` and `sleep` commands, scripted model alternating commands and interface actions. Asserts: the records come back in call order with rising step numbers and the right kind; a command's record holds its stdout and exit code and the frame/snapshot of the last interface read; an input's record holds the tree changes and empty streams; `git help -a` output past the bound is cut at the marker with `truncated: true`; a command that times out (500 ms) is a record with `timed_out: true` and a non-SUCCESS status and the child goes on to a COMPLETED envelope; the records are present in the objects the log refers to. The full `px_computer` suite (32 tests, including QUAL-PX-075) passes. Mutation: `evidence/m10/PX-176/mutation-check.txt` (the record disabled → the test fails).

## Known limits
- A command step does not re-read the interface, so its `frame`/`snapshot` are the last ones the child saw and its `tree_delta` is null; the record says so by carrying no new frame. A new frame needs an observing call.
- A `computer.state` delta is a line-set comparison; a reordered tree shows as added and removed lines.
- The timeout leg runs on Unix (`sleep`); the Windows run skips it.
- The restart replay is shown by the record being in the logged result; the test does not kill the Core mid-run (the other PX-069/075 tests cover Core restarts of this loop).
