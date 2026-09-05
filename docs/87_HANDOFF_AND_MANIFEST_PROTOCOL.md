# Handoff and Manifest Protocol

An AI agent may stop work only with a machine-actionable handoff.

## Required handoff fields

- task and requirement IDs;
- branch/worktree and exact commit/revision;
- status from canonical lifecycle;
- files added/changed/deleted;
- migrations/schema versions;
- interfaces/events changed;
- tests run and exact outcomes;
- real-system evidence refs;
- faults/security cases exercised;
- unresolved failures with reproduction commands;
- decisions made and ADR refs;
- remaining acceptance criteria;
- next safe action.

## Forbidden handoffs

“mostly done”, “should work”, “tests look good”, “backend implemented”, “UI wired”, “just needs cleanup” or similar summaries without explicit remaining acceptance criteria.

## Manifest update

A task becomes COMPLETE only when the manifest points to its qualification/E2E evidence. If evidence expires because architecture or dependency versions materially change, move the task back to REAL_TESTING.
