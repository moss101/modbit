---
id: DR-M0-005
title: Reschedule IMP-EV-0242 to M4 and PX-030 to M10 (their qualifications need runtime that does not exist in M0)
status: accepted
date: 2026-09-09
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks" on 2026-09-09; scheduling-only change under docs/74 MILESTONE_OVERRIDES precedent (DR-PX-2026-09-05-006)
---

# DR-M0-005 — Reschedule IMP-EV-0242 and PX-030

## Trigger / evidence

`python3 tools/graph.py ready` lists IMP-EV-0242 and PX-030 as the last M0
work, and M1 cannot start until M0 is COMPLETE. Both qualifications require
runtime that M0 does not contain:

- QUAL-EV-0242: "Restart loses no durable truth even though hook process
  resets" needs a durable store (M1.1/M1.2), the hook bus and a real restart
  (M1.5, M4.6). IMP-EV-0242 landed in M0 only because its owner label "Core
  Architecture" maps to M0 in `tools/build_graph.py` OWNER_MAP.
- QUAL-PX-030 / PX-E2E-030: the platform conformance suites cover PTY and
  process control (M2.3), language services (M3.4), Git (M2.2), path policy
  (M2.1), secrets (M9), packaging and updater (M10) and the browser host (M7).
  The CI matrix itself has existed since M0.1 and already labels results
  CI_COMPATIBLE only.

Completing either in M0 would require a simulated hook process or suites over
capabilities that do not exist, which `docs/82` forbids.

## Current behavior

Both tasks are scheduled in M0 and block M1.

## Proposed replacement

1. `IMP-EV-0242` moves to **M4** (durable recovery spine) through
   `MILESTONE_OVERRIDES` in `tools/build_graph.py`; docs 40/41/42 stay
   byte-identical, the node records `milestone_override`.
2. `PX-030` moves to **M10 / RELEASE_ZERO** by editing its ledger row and task
   card in `docs/62` (locked; this record is the authority). Its prerequisite
   M0.1 is unchanged; PX-031 (M10) already depends on it. The CI matrix and
   the CI_COMPATIBLE label keep running from M0.1 as the matrix "from M0";
   the task completes when every conformance suite it names exists.
3. `IMP-EV-0208` stays in M0: its qualification is an architecture dependency
   test that runs against the workspace today.

## Migration

None. No status changes; both tasks remain NOT_STARTED in their new milestones.

## Compatibility

Releases remain nested (ALPHA ⊂ BETA ⊂ RELEASE_ZERO); PX-030 leaves ALPHA and
BETA and stays in RELEASE_ZERO. Alpha ships macOS only (`docs/75`), so no Alpha
proof depended on PX-030.

## Security impact

None.

## Test impact

`tools/test_dossier.py` expectations for PX-030's milestone move from M0 to
M10; the override is asserted like IMP-EV-0107's.

## Rollback

Remove the override and restore the two doc 62 cells with a superseding record.

## Explicit user approval

Owner standing instruction on 2026-09-09: "complete all tasks", with the
dossier's own rule that scheduling exceptions are Decision Records.
