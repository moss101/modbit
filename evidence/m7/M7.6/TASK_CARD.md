# Task Card — M7.6 Control lease / takeover

## Identity

- Task ID: M7.6
- Milestone: M7 Live browser (P0)
- Requirements: docs/22 "Live user takeover" (a control lease; user takeover immediately blocks agent input, not observation; returning control increments the lease generation to fence stale input events; the session does not restart), docs/44 row "Live same-session browser/takeover", REQ-EV-0283 (same live browser session user observe/takeover), REQ-EV-0087 (human activity preemption, the browser half).
- Qualification: docs/51 E2E-015 — the agent is navigating; the user clicks Take Control and types; agent input is blocked immediately; the same session remains; returning control increments the lease generation and stale agent inputs are fenced.
- Evidence tier: real-system (the real Core; the host's rule unit-tested; the real app on a real page)

## Goal

One session, two hands: the person can take the wheel at any moment and the agent's input stops at once — at the Core and at the host — while it keeps seeing the page; the hand back moves a generation so nothing the agent sent before lands after.

## Existing-code audit

- classification: PARTIAL before this task: M7.1 defined `ControlLease` with fencing and the Core refused agent input under the user's control, but nothing could hand control over (no command, no event, no UI) and the host did not fence generations.
- production entry points: `crates/protocol/proto/modbit/v1/surface.proto` (`SetBrowserControl` / `BrowserControlChanged`); `crates/domain/src/task.rs` (`BrowserControlChanged`); `services/modbit-core/src/browser.rs` (`hand_control`; the lease rebuilt from `BrowserControlChanged` after a restart; `Act` fenced like `Navigate`); `services/modbit-core/src/server.rs` (the `SetBrowserControl` handler under `session.control` and the session lease; a no-op hand-over answers `changed: false`, replayed); `packages/ide-adapter-core/src/client.ts` (`setBrowserControl`); `apps/desktop/src/main/lease.ts` (`admitsAgentInput`), `browser.ts` (`leaseGeneration` / `controller` per hosted session, learned at open, attach and every hand-over; agent `navigate` / `act` refused `USER_HAS_CONTROL` or `STALE_GENERATION`; `setControl`; `typeAsPerson`; the log carries each request's generation); `main.ts` (`browser:control`, `browser:typeAsPerson`); `preload.ts`; `apps/desktop/src/renderer/index.tsx` (the panel's Take / Return control, `data-controller`, `data-lease-generation`; `takeover active` from `browserState`).
- proof: `qual_m7_6_taking_control_blocks_agent_input_at_once_and_returning_it_moves_the_lease` (real Core, fake host): a navigation under the agent's lease runs (generation 1); the person takes control (generation 2, journaled) and the agent's next navigation is refused `USER_HAS_CONTROL` with nothing reaching the host while a snapshot still reaches it; taking control again is a no-op (generation 2, `changed: false`); returning control is generation 3 and the next navigation reaches the host stamped 3; the host saw exactly navigate@1, snapshot@2, navigate@3; the session view and the two `BrowserControlChanged` on the log agree; the `BrowserNavigated` records carry generations 1 and 3; after a Core restart the lease is rebuilt at generation 3 for the agent. `lease.test.ts`: the host's rule refuses a stale generation whoever holds control and refuses agent input under the person. `browser.spec.ts` (takeover): the agent navigates and reads the login page; the person takes control from the panel (generation 2, `takeover active`) and types into the same session; the agent's fill, released then, is refused `USER_HAS_CONTROL` naming generation 2 and no action reaches the host; the agent's read shows `"value":"typed by the person"`; the view is still the login page; control returns (generation 3, the panel populated again); the agent's fill lands under generation 3 with the person's text as `value_before` and its postcondition held; the Core records the agent at generation 3.

## Limitations

- Control is per session and hand-overs are explicit (the panel's buttons); the person's raw input into the view while the agent holds control is not intercepted (docs/22 puts the takeover on the button, not on keystrokes — IMP-EV-0087's preemption-on-activity is the computer-use half).
- A stale-generation input can only arise from a request already on the wire during a hand-over; the host's rule is proven in isolation and the Core-side refusal end to end.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_6_taking_control_blocks_agent_input_at_once_and_returning_it_moves_the_lease` (services/modbit-core, real Core)
- `apps/desktop/e2e/browser.spec.ts` — "browser takeover: …"
- `apps/desktop/src/main/lease.test.ts`; `control_lease_fences_stale_agent_input` (crates/browser)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
