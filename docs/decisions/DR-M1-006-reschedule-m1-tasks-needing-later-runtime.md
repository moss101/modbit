---
id: DR-M1-006
title: Reschedule twelve M1 implementation tasks whose qualifications need M2–M6 runtime
status: accepted
date: 2026-09-09
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks" on 2026-09-09; scheduling-only change under the docs/74 MILESTONE_OVERRIDES precedent (DR-PX-2026-09-05-006, DR-M0-005)
---

# DR-M1-006 — Reschedule twelve M1 tasks that need later runtime

## Trigger / evidence

All five M1 milestone tasks are COMPLETE. Of the 26 `IMP-EV-*` tasks that
`tools/build_graph.py` OWNER_MAP places in M1, twelve have qualifications that
require runtime M1 does not contain, so completing them in M1 would need a
simulated agent, verification engine, checkpoint engine or media pipeline
(forbidden by `docs/82`):

| Task | Needs | New milestone |
|---|---|---|
| IMP-EV-0035, IMP-EV-0175 (Context Inspector) | `PromptEnvelope` context ids (M2.7 prompt compiler, M3 Context Pack) | M3 |
| IMP-EV-0077, IMP-EV-0122, IMP-EV-0123 (session branching, fork, rewind) | checkpoints and worktrees (M4.3), Change Engine | M4 |
| IMP-EV-0098 (typed tool result re-entry) | provider gateway replay (M2.6/M2.7) | M2 |
| IMP-EV-0119 (goal mode) | verification engine acceptance (M2.8) | M2 |
| IMP-EV-0190 (channel image/file ingestion) | MediaEnvelope (M2.10) | M2 |
| IMP-EV-0191, IMP-EV-0261 (steer/collect/followup dispatch, side question) | a running agent turn to interrupt or coalesce (M2.7) | M2 |
| IMP-EV-0221 (background task list/output/stop) | terminal broker (M2.3) | M2 |
| IMP-EV-0180 (default background delegation policy) | subagent scheduler (M6) | M6 |

The remaining fourteen (0010, 0037, 0039, 0054, 0101, 0102, 0103, 0108, 0121,
0143, 0152, 0192, 0262, 0273) are real at M1 and are implemented on the M1.1–M1.5
substrate with their own qualification tests.

## Current behavior

The twelve tasks block M2 while being unimplementable without M2+ code.

## Proposed replacement

Add the twelve to `MILESTONE_OVERRIDES` in `tools/build_graph.py` naming this
record; docs 40/41/42 stay byte-identical; each node records
`milestone_override`. Statuses stay NOT_STARTED.

## Migration

None.

## Compatibility

Release membership follows the new milestones (all remain inside ALPHA except
IMP-EV-0180 which joins BETA with M6, and 0035/0175 which join BETA with M3).
Releases remain nested.

## Security impact

None.

## Test impact

`tools/test_dossier.py` asserts the override on IMP-EV-0119 and IMP-EV-0180.

## Rollback

Remove the overrides with a superseding record.

## Explicit user approval

Owner standing instruction on 2026-09-09: "complete all tasks", with the
dossier's own rule that scheduling exceptions are Decision Records.
