# Task Card — IMP-EV-0110 Browser/computer behind authenticated peer boundary

## Identity

- Task ID: IMP-EV-0110
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0110; disposition ADOPT
- Mandatory behavior: Browser worker attaches via authenticated session and cannot bypass Core policy.
- Qualification: `QUAL-EV-0110` — Forged peer cannot attach/take over session.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Browser worker attaches via authenticated session and cannot bypass Core policy.

## Existing-code audit

- classification: IMPLEMENTED by M7.1 (authenticated surface, `browser.host` capability, `HOST_NOT_ISOLATED`, `PARTITION_MISMATCH`); the take-over refusal is new.
- production entry points: `services/modbit-core/src/server.rs` (`browser.host` only on the desktop kind; `attach_browser_host`: isolation and partition checks; `HOST_CONFLICT`), `services/modbit-core/src/browser.rs` (`AttachRefusal`).
- proof: `qual_ev_0110_0084_…`: the CLI kind is refused `CLIENT_CAPABILITY` naming `browser.host`; a second desktop connection is refused `HOST_CONFLICT`; a wrong partition is refused; the session is hosted again only once the holder's connection is gone. `qual_m7_1_…`: an un-isolated view is refused `HOST_NOT_ISOLATED`.

## Limitations

- Peer identity is the authenticated connection's client kind and the boot secret; there is no per-host certificate.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0110_0084_a_forged_peer_cannot_attach_and_a_second_host_gets_the_conflict`
- `qual_m7_1_a_browser_session_is_hosted_by_the_desktop_and_driven_through_the_cdp_bridge`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
