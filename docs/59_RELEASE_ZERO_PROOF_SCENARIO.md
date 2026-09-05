# Release Zero — Single Proof Scenario

> **Authority date:** 2026-09-03  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Why this scenario exists

Before broad feature expansion, Modbit must prove the entire architecture is real. Release Zero is one end-to-end scenario that crosses UI, Core, model, context, tools, Git, terminal, verification, persistence, browser, effects and recovery without internal shortcuts.

## Environment

- signed packaged desktop candidate;
- `multi-package` real Git fixture checked out at frozen commit;
- local Core + `modbit-execd`;
- configured live production model provider test account;
- local embedded Chromium;
- SQLite/object/index stores created fresh;
- test account with policy profile allowing worktree writes and requiring approval for a protected external browser submission.

## User goal

> “In the sample web application, add server-side validation for invite email domains, add tests, run the application, verify the behavior in the browser, and prepare the changes for review. Do not submit the final invite until I approve it.”

## Required execution

1. Desktop creates durable task before model call.
2. Core allocates branch/worktree and indexes/retrieves relevant code.
3. Model receives provenance-bound Context Pack and task-scoped tools.
4. Agent edits through Workspace File Service.
5. Real unit/integration tests run through durable terminal.
6. Agent launches real app process; terminal session is visible.
7. Agent opens same live embedded browser shown to user.
8. Semantic browser runtime navigates/fills invite form structurally.
9. User restarts renderer during task; stream recovers.
10. Agent reaches protected final invite submit; Core creates Approval with bound intent and moves Needs Attention.
11. Kill Core while approval is pending; restart.
12. Same approval/protocol state restores.
13. User denies submission. Browser must not submit invite.
14. Agent verifies validation behavior without protected send, collects evidence and reaches Ready for Review.
15. User reviews revision-bound diff, test output, browser evidence, receipts and denies/accepts merge.

## Pass conditions

- no mock provider/browser/filesystem/database;
- no manual DB edit/internal state injection;
- Git diff and tests are real;
- task survives renderer + Core restart;
- command failure if encountered does not incorrectly fail task;
- approval intent survives restart and denial prevents effect;
- browser session remains user-observable and structurally controlled;
- all large outputs are bounded/OutputRef-backed;
- Effect Ledger chain verifies;
- final evidence bundle contains build digest, event checksum, context pack refs, checkpoint proof, Git diff, test outputs and browser/approval evidence;
- no critical/high security finding or raw secret leakage.

## Exit criterion

Do not claim “Modbit end-to-end works” until this exact scenario passes on a packaged candidate. After it passes, expand into full catalog and cloud counterpart.


## V2 superseding scenario

`60_RELEASE_ZERO_EXPANDED_PROOF.md` is the authoritative superset. It adds a real subagent, real multimodal artifact path, multi-client replay and the latest source-coverage evidence while preserving the original restart/approval/Git/test/browser proof.
