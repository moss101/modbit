# Task Card — PX-215 Standing computer access per application: view only, click, full or never, held by the Core and only ever narrowing a grant

## Identity

- Task ID: PX-215 (REQ-PX-215; owner browser); qualification QUAL-PX-215 / PX-E2E-215; Decision Record DR-PX-2026-11-11-021 (MWS-19); AFW-A12, A13, A14 unchanged.
- Evidence tier: release-critical (a standing policy record that the computer runtime enforces: permissions and policy, canonical persistence, protocol).
- Prerequisites: PX-208, PX-069, DOC-PX-020 (all COMPLETE).
- Branch `claude/pensive-bardeen-38b766`, from `main` at c73af335.

## Existing-code audit (before any edit)

- Computer runtime (`crates/computer`): sessions, observation grants, the per-input approval, administrative `ComputerPolicy` (`act_denied`, `observe_denied`, `screen_denied`, `apps_allow`) and the versioned list of applications no approval lifts (`policy::NON_DRIVABLE`). Every check ends in a typed `Refusal` (25 codes, docs/66 CUC-D01). PRODUCTION-WORKING (PX-069/070). The checks run in `runtime/act.rs::plan` (inputs, used by `prepare_act` before an approval is asked and by `act`), `runtime/observe.rs::start_checks` (a start) and `session_for` (a look).
- A per-application level the person sets: NOT-FOUND (protocol, Core, runtime, desktop). The Settings page had one Computer control row (Review, the permission window) and nothing that said how far the agent may go in one application.
- The refusal reason on the card: the card joins the transcript's tool row (which carries a failure code, not the message) with `ListComputerActivity`. A refusal made in `prepare` leaves no activity entry and `InvokeContext.tool_call_id` was unset there, so a call's own refusal could not be joined to its card. NOT-FOUND.
- Durable Core-wide settings with an audit: the automation ledger (`AggregateType::Automation`, a ledger session of the Core's own, folded at boot) and provider configuration (a Core-wide command with no session lease) are the precedents.

## First missing link

No record of the person's standing choice, and no check against it. Everything it must sit beside (grant, approval, policy, the never-driven list, scope) exists and is owned by the Computer Runtime, so the work extends that runtime and adds no second policy engine or approval system.

## Invariants (REQ-PX-215, MWS-19)

1. A level per application is held by the Core; the runtime refuses a call above it with a typed refusal the card shows; `never` refuses the session.
2. Nothing is set by default; with no level the grant alone decides (behaviour unchanged).
3. A level only ever narrows: it is one more refusal after every existing check; `full` means "no standing restriction", not "approved"; a level above `never` cannot be set for an application no approval lifts.
4. A change is an audited event on the log and applies from the next action (the store is read at every call).

## What was built

- `crates/computer/src/access.rs` (new, the runtime's own): `Level` (never < view_only < click < full), `Need` (observe, click, keyboard), `StandingAccess` (the fold of the changes), `AccessEvent` (`ComputerAccessSet` / `ComputerAccessCleared`, with the level before), `normalize_key` (one identifier: no pattern, no whitespace), `refusal` (`ACTION_UNSAFE` with `standing_access {application, level, needed, changed_in}`; no 26th code). `Act::need()`: press, click, move, drag, scroll need `click`; set_value, type, key need `full`.
- Runtime enforcement beside the grant: `plan` (every input; before the approval in `prepare_act`), `start_checks` (a start, at `never`, before an approval is asked), `session_for` (a look at `never` set mid-session; release is never refused), `computer.apps`/`computer.resolve` report `standing_access` and `drivable:false` for `never`. Each refusal queues `TaskEvent::ComputerAccessRefused {control_id, tool_call_id, bundle_id, application, level, needed, reason}` on the task's log. `crates/tools/src/pipeline.rs` now gives `prepare` the call's own id so that event joins its card.
- Core (`services/modbit-core/src/computer.rs`): `SetComputerAccess` (capability `approval.resolve`; a Core-wide setting, no session or lease, like provider configuration) appends one event on the Core's ledger session (`AggregateType::ComputerAccess`, actor `user:<id>`) through `execute_command` (idempotent by command id; the same level again records nothing), then applies it to the runtime's store; refuses BAD_PAYLOAD, BAD_LEVEL, BAD_APPLICATION, NOT_DRIVABLE. `GetComputerAccess` (events.subscribe) returns the levels, the running drivable applications the actuator reports, and the audit trail read back from the log (who, level, previous, offset). Boot folds the whole log into the store. `ListComputerActivity` gains `ACCESS_REFUSED` entries (`access_level`, `access_needed`, `reason`).
- Protocol (additive, `computer.proto`): `GetComputerAccess`, `ComputerAccessView`, `ComputerAccessEntry`, `ComputerAccessApplication`, `ComputerAccessChange`, `SetComputerAccess`, `ComputerAccessChanged`, `ComputerActivityEntry.access_level/access_needed`. Fixtures on both sides (`computer_access_view`, `get_computer_access`, `set_computer_access`, `computer_access_changed`, `computer_activity_access_refused`; Rust encodes, TypeScript decodes and re-encodes byte for byte).
- Desktop: `getComputerAccess`/`setComputerAccess` in `ide-adapter-core`; `main/computer-ipc.ts` (`computer:access`, `computer:setAccess`, validated in `computer-args.ts`; cards joined with the Core's ACCESS_REFUSED); two preload methods (`computerAccess`, `computerSetAccess`) with `hardening-guard.test.ts`'s pin and `workers.spec.ts`'s bridge list updated and noted; Settings > Permissions "Standing access for applications" (rows with a level select, a form with the running applications offered, the Core's words back, the Core's refusals shown); the action card says the level, what the call needed, that nothing was done and where to change it.

## Verification

macOS 27.0.1 arm64, rustc 1.97.1, node 26.11.1, Electron 44.3.0. The model is the repository's scripted stand-in; the Core, its SQLite store and socket, the actuator fixture process (the same authenticated RPC as the macOS helper, over an in-memory desktop), Electron main, the preload and the renderer are real. The signed macOS actuator helper (PX-071) is not part of this row and was not used.

- Runtime unit tests (`crates/computer`): 7 for the level and 8 for the runtime (view only, click, never at start and mid-session, no level, change mid-session both ways, narrowing only).
- `cargo test -p modbit-core --test px_computer qual_px_215` (real Core binary, real fixture actuator process, a person who approves through `ResolveApproval`): 7 passed. View only: a press, a type and a set_value refused with their reason, no approval asked, no `ComputerActionPerformed`, looking works, the log and the activity carry each refusal with level and need. Click: a press goes through, typing refused. Never: the start refused before any approval, `computer.apps` says so, another application unaffected. No level: unchanged, nothing recorded. Mid-session: narrowed, refused, widened, allowed inside one session, the changes on the log in order between the calls with `user:` as the actor. Negative: full asks for no approval less, opens no session without the grant, gives no scope, lifts no never-driven application (NOT_DRIVABLE), bad level and pattern refused, rejected commands record nothing. Fold: the same level again records nothing, clear is its own event, a restart of the Core (new process, same data) holds the same levels and the trail, and a cleared level does not come back.
- Playwright against the real app, `px-215-standing-access.spec.ts`: Settings (nothing set by default, set through the form, changed with the keyboard, refusals shown, cleared, the Core's trail equals the changes, axe in light, dark and high contrast) and the three action cards (click and typing at view only, a start at never) with the Core's own record behind them.
- Mutation checks, `mutation-checks-2026-11-20.txt`: a level not enforced; a refusal not recorded (also against the real app: the card loses its words); a change not in the audit trail; a level that lifts a never-driven application. Each made its test fail.
- Neighbours and the Core suite, formatting, lint: see `evidence.json` and the logs.
- Real screenshots of the running app, light and dark, beside the neighbouring Settings sections (Login credentials uses the same card, row and form language) and the existing computer cards: `after/*.png`.

## Limitations

- A refusal is `ACTION_UNSAFE` (escalation abandon): the agent is told to stop trying, and a refused call is not progress, so the loop's no-progress rule ends a run after a few of them. The app spec therefore uses one refusal per task; the Core test starts its run with room for a script of refusals (`start_task_long`), as the existing computer tests do.
- `click` includes pressing an element (an accessibility action), scrolling and dragging; `full` adds text, values and keys. A press on a text field's "focus" is a click-class input.
- The list of running applications comes from the attached actuator; with none attached a level is set by identifier.
- Another window's change is read when the window regains focus; the ledger session is not on a session's event stream.
- If PX-216 lands first both rows add preload methods: the hardening pin conflicts and is re-pinned to the merged preload.
- macOS only so far; Linux and Windows are CI's. CI run 44755846351 on the PR's final head 0fa6cda0: all 19 checks green on Linux, macOS and Windows (`ci-run-44755846351.json`); merged as 54908396 (#238). The owner reviewed the screenshots and asked for the seal on 2026-11-20.

## Status

COMPLETE (sealed 2026-11-20).
