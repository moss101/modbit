# modbit-cli

Headless thin client for the local SurfaceProtocol (REQ-PX-000). It holds no
provider credential and runs no tool, policy, Git or filesystem code: every
effect goes through the Core over the authenticated socket.

## Exit codes (the documented contract)

| Code | Meaning |
|------|---------|
| 0 | The command completed; for `task run --wait`, `question answer --wait` and `task status`: the task is ReadyForReview or Completed |
| 1 | Client, transport or Core rejection (`modbit-cli: ...` on stderr) |
| 2 | NEEDS_INPUT: the task is Waiting (a typed question or a protected-effect approval is pending); never a hang |
| 3 | The task is Cancelled or Failed |
| 4 | The task is still Queued or Running (`task status` without waiting) |

## JSON lines

`events tail --session <id> [--after N] [--count N] --json` prints one JSON
object per event: `offset`, `sequence`, `aggregate_type`, `event_type`,
`task_id` (hex or null) and `payload` (the event payload as JSON). Resume from
the last `offset` after a disconnect or a Core restart; no event is duplicated.

## Questions

`question list --task <id>` shows the task's typed questions with their
options, flags and answers. `question answer --session <id> --task <id>
--question <id> [--option <id>] [--wait] [free text]` records the answer and
resumes the run.

## Attaching to a running Core

A Core publishes its ready line (endpoint, boot secret, protocol version) to
`<data-dir>/core.ready`, readable by the owner only, and removes it on
shutdown. The CLI attaches to that Core when it is alive and spawns one
otherwise; a spawn that loses the profile lock to a Core that is just
starting retries the attach. This is what lets a second shell answer a
question or approve an effect while `task run --wait` holds the Core.

## Language labels (PX-026)

`modbit --data-dir <dir> language list` prints what the product claims per language today (docs/76): the three Alpha candidates are labelled `ALPHA_BASELINE` (Tier C plus compile and test evidence) with the provisional context-engine capabilities and the not-yet-claimed Tier A listed explicitly; everything else is `UNSUPPORTED`.

## Diagnostics (IMP-EV-0142, docs/71)

- `doctor --session <id>` prints the build, uptime, SQLite's integrity check, how many of the session's hash chains verified, the receipt chain, each provider endpoint's host and health (whether a credential is configured — never its value), the latest failures by class and code, and lease counts. It exits 1 when the store or a chain does not verify.
- `trace --session <id> [--task <id>]` prints what happened as metadata only: offset, aggregate and sequence, event type, task, run and a failure's code. No payloads.
- `export diagnostics --session <id> [--task <id>] [--include-content] --out <file>` writes a `modbit.diagnostics/1` package: the above plus every aggregate's range and chain head and the objects the events reference. Payloads appear only with `--include-content`. The Core redacts the whole package (every value in its custody, every credential shape) and seals it with a digest.
- `diagnostics verify <file>` replays a package against this profile's log: its digest, every aggregate's chain, and the head it pinned. It exits 1 when anything differs.
- `export handoff --session <id> --task <id> --out <dir>` parks the task and writes M8.7's handoff bundle (no secret value in it).

## Usage and invoices (IMP-EV-0032)

`task economics --task <id>` adds a `ledger` line — the cost in the active registry's minor units, its currency and generation, and how many calls were priced, unpriced or never reported — and, per run and per step, the calls, tokens, cost, tool calls, tool time and verification time. `usage reconcile --task <id> --invoice <file> [--tolerance-bp N]` compares a provider invoice sample (`modbit.invoice-sample/1`, docs/34) with the task's ledger row by provider request id, prints every row that did not match, and exits 1 unless every row is within the tolerance.

## Dashboard (M10.1)

`dashboard --session <id>` prints the session's operations picture as the Core aggregates it from its log: tasks by state; the cost in registry minor units with priced/unpriced/unreported calls; tokens; one line per model and per task (calls, tool calls, cost, SLO starts); tool outcomes; the cold and warm SLO figures across cloud starts (-1 when never reached); recent failures; and provider health. The desktop's Operations pane shows the same view.
