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
