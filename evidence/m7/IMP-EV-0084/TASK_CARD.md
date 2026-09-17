# Task Card — IMP-EV-0084 Single controller lock

## Identity

- Task ID: IMP-EV-0084
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0084; disposition ADOPT
- Mandatory behavior: Only one automation controller owns a target/session at a time.
- Qualification: `QUAL-EV-0084` — Two workers contend; second receives lease conflict.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Only one automation controller owns a target/session at a time.

## Existing-code audit

- classification: PARTIAL before: the control lease (M7.6) held one controller between the agent and the person; a second host connection silently replaced the first.
- production entry points: `services/modbit-core/src/browser.rs` (`attach` refuses a live second host: `AttachRefusal::HostConflict`), `services/modbit-core/src/server.rs` (`HOST_CONFLICT`), `crates/browser::ControlLease` (one controller — agent or person — per session, generation-fenced).
- proof: `qual_ev_0110_0084_…`: the desktop host attaches; a second desktop connection is refused `HOST_CONFLICT` naming the holder; once the first connection is gone the session can be hosted again; two `BrowserHostAttached` records, none for the refused attempts. `qual_m7_6_…`: the agent's input is refused while the person holds the lease.

## Limitations

- Sessions are per task; two tasks never share one browser session, so the lock between workers is structural (one session, one task, one host).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0110_0084_a_forged_peer_cannot_attach_and_a_second_host_gets_the_conflict`
- `qual_m7_6_taking_control_blocks_agent_input_at_once_and_returning_it_moves_the_lease`
- `apps/desktop/src/main/lease.test.ts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
