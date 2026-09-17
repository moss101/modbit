# Task Card — IMP-EV-0283 Same live browser session user observe/takeover

## Identity

- Task ID: IMP-EV-0283
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0283; disposition ADOPT
- Mandatory behavior: Dock/expand/pop-out/remote viewer share one session; human takeover revokes automation controller.
- Qualification: `QUAL-EV-0283` — User takes over mid-run with no browser restart and agent resumes after reacquisition.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Dock/expand/pop-out/remote viewer share one session; human takeover revokes automation controller.

## Existing-code audit

- classification: IMPLEMENTED by M7.6 (one session shared by the panel and the agent; takeover moves the lease); preemption by activity by IMP-EV-0087.
- production entry points: `apps/desktop/src/main/browser.ts` (one `WebContentsView` for the person and the agent; `show` / `hide` place the same view), `crates/browser::ControlLease`, `SetBrowserControl`.
- proof: `qual_m7_6_…` and the `browser.spec.ts` takeover test: the person takes over mid-run with no browser restart, types into the same view, and the agent resumes after control returns at the next generation.

## Limitations

- Dock/expand/pop-out/remote viewers are not built: the one view is placed over the panel's placeholder.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_6_taking_control_blocks_agent_input_at_once_and_returning_it_moves_the_lease`
- `apps/desktop/e2e/browser.spec.ts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
