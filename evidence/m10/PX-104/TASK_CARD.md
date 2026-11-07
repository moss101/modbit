# Task Card — PX-104 Review reachable from any task state, with live per-hunk controls and the originating-call chip

## Identity

- Task ID: PX-104 (REQ-PX-104; owner desktop); qualification QUAL-PX-104 / PX-E2E-104; related REQ-EV-0151, REQ-EV-0016
- Evidence tier: release-critical (drives effect-bearing hunk decisions and reverts through the desktop)
- COMPLETE is not claimed: prerequisite PX-062 is NOT_STARTED and PX-048 is REAL_TESTING; the live-gateway leg and the failed / needs-attention task states are not run.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- Changes tab (`apps/desktop/src/renderer/apps/changes-app.tsx`, PX-048): PRODUCTION-WORKING and already reachable in every task state, showing the live revision-bound diff, but with only a whole-file/hunk "Revert", locked while the task runs, no attribution and no decision.
- Bridge: `changes:*` handlers in `apps/desktop/src/main/changes-ipc.ts`; no `DecideHunk` caller anywhere. IMPLEMENTED-PARTIAL.
- First missing link: the live panel over PX-103 (no preload/main/adapter call for `DecideHunk`, no attribution fields in the bundle view, no chip, no jump to the producing call).

## Implementation

- `packages/ide-adapter-core/src/changes.ts`: `decideHunk`.
- `apps/desktop/src/main/changes-ipc.ts`: `changes:decideHunk` with argument validation (hunk index, decision, file revision required, hunk digest, bounded reason, command id); `main.ts` carries the attribution fields in `review:bundle`; `preload.ts` exposes `decideHunk` (the bridge pin of `hardening-guard.test.ts` is moved for this reviewed bridge change).
- `changes-app.tsx` / `changes-model.ts`: per hunk, the Core's attribution chip (opens the producing call in the conversation via the shell's `openConversation(task, "tool:<id>")`), Accept and Reject buttons that send the file revision and hunk digest this view read (not disabled by a running task; disabled only for a binary file or an action of this view in flight), the Core's typed answer rendered (the decision banner with the agent notice, or the refusal with the Core's own code, e.g. STALE_REVISION / STALE_HUNK, and the reload that works), an "Accepted" badge from the Core's record, "never accepted automatically" for an unattributed hunk. The renderer decides nothing.
- Section headings of the tab are `h2` (they were `h3` under the page `h1`; axe heading-order).

## Verification

- `apps/desktop/e2e/px-104-live-review.spec.ts` (Playwright on the Electron app built from source, a real Core over its socket, a real Git checkout, the repository's scripted OpenAI-compatible model that holds the task Running at a gate): the Changes tab is open while the task is Running and every hunk's chip names `change.apply`; the chip opens the producing call (row highlighted); a hunk is rejected with a reason by keyboard while the run is live: the file equals the expected text by hash, the hunk count falls, the run continues (released from the gate) and the model receives the notice once, with the reason; with the task waiting for the person (`user.ask`) a decision on a file that moved is refused with the Core's code STALE_REVISION and writes nothing, and the reload works; a write by no tool call shows "Not attributed", the chip is inert, "never accepted automatically" is shown and the person accepts it; axe-core reports no violation (twice); the renderer's process is killed (`forcefullyCrashRenderer`) and the replacement view shows the same digests and decisions read from the Core.
- Node tests: `changes-ipc.test.ts` (validators, nine-channel guard, ack shape), `apps-models.test.ts` (chip words, canDecide, refusal words); `typecheck` of desktop and adapters.
- Regressions: `changes-app.spec.ts`, `apps-panel.spec.ts`, `review.spec.ts` pass with the same build.
- Mutation check: `mutation-check.txt` (the decision call drops the file revision: the stale test fails; restored passes).

## Limitations (unproven gaps)

- The Changes tab is exercised while the task is Running and while it waits for the person (`user.ask`); the failed and needs-attention states and the waiting-for-approval state are not driven in this spec (the tab's code does not branch on them; the completed state is covered by `changes-app.spec.ts` of PX-048).
- The app is the unpackaged build (`node build.mjs`, run by Electron from `dist/`), not a signed package.
- The live gateway leg is not run (scripted model).
- macOS only was run.
