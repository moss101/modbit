---
id: DR-M6-003
title: The screen-state matrix seals on the rows the Core can force today; the rows whose substrate is M7–M9 are mapped now and forced when it exists
status: accepted
date: 2026-09-13
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks"; evidence-scope decision in the line of DR-M2-002, DR-M3-004 and DR-M5-001 (a qualification that names substrate of a later milestone seals on what exists and records the rest), under docs/82 (nothing simulated is claimed as forced)
---

# DR-M6-003 — PX-023 seals on the forced rows; the M7–M9 rows follow their substrate

## Trigger / evidence

QUAL-PX-023 requires "each screen's empty, loading, populated, error,
degraded and recovery states … forced against a real Core (Core restart,
provider down, stale bundle, offline, unknown outcome, quality floor
infeasible, human continuation)" and the docs/39 matrix lists, per screen,
the degraded and recovery states that must exist. Five of the matrix's
rows name substrate that does not exist in M6:

| Row | Substrate | Milestone |
|---|---|---|
| Home / Fleet: stale policy bundle (`REGISTRY_STALE`, routing on the last-good bundle) | the published policy bundle and its registry freshness (docs/38 error category `REGISTRY_STALE`; no bundle publication exists — every Core routes on its built-in registry) | M9 |
| Home / Fleet: offline (local tasks continue, cloud features disabled with reason) | the cloud control plane and account plane (docs/24); no cloud feature exists to disable | M8 |
| New Task: budget exhausted for the day | a daily budget on the session or organization; the Core has per-task budgets (`HarnessBudgetExhausted`, capacity tickets) and no daily one | M9 (memory/effects/security hardening) with the organization policy bundle |
| Browser: takeover active, session lost with recovery choice, visual fallback in use | the Browser screen and the live browser (DR-M3-004) | M7 |
| Settings: organization policy overrides a user preference | organization bundles (docs/24, docs/38) | M8/M9 |

Two further rows are derivable from records the Core already writes but
have no honest end-to-end forcing path in M6:

- "quality floor infeasible under policy" is `RoutingPlanAdmitted.feasibility =
  QUALITY_FLOOR_INFEASIBLE` (REQ-EPR-016); the surface suite forces it
  (`qual_epr_016_…`) and the desktop maps it at the reducer level
  (`screens.test.ts`), but forcing it through the packaged app would mean a
  registry whose every plan misses the floor, which the Core's built-in
  registry does not offer;
- "policy forbids the requested execution mode" is a `CreateTask` /
  `StartTask` rejection (`PROFILE_UNSUPPORTED`, `PLAN_MODE_*`,
  `CAPABILITY_DENIED`) the composer maps; the desktop only submits the
  `local_trusted` profile, so no packaged path requests a forbidden mode.

## Current behavior

Before PX-023 the desktop had the Core-restart banner, the recovery
banner and the error banner (M0/M1) and nothing else of the matrix: no
per-screen state, no cause/next-action/evidence on a degraded card, no
notification model, run-aggregate events dropped by the renderer, wire
wait reasons (`USER_INPUT`, `APPROVAL`) never matched by the reducer, no
way to answer a question or decide an approval on the card, no pull
request from the Review.

## Proposed replacement

1. PX-023 seals on the rows forced against the real Core in
   `apps/desktop/e2e/states.spec.ts`, `onboarding.spec.ts` and
   `user-patch.spec.ts` (Core restarting, Core recovered with the recovery
   report, provider down, provider unavailable, repository missing,
   `NO_PROVIDER`, reconnecting by cursor, unknown tool outcome under
   reconciliation, awaiting your answer, awaiting approval with the exact
   intent, awaiting human continuation through the acceptance gate,
   verification incomplete / rejected as the gate says, stale candidate
   revision, pull request denied then opened, keychain state as reported),
   the notification model (`notifications.test.ts`, `states.spec.ts`) and
   the reducer-level mapping of every other row in `screens.ts`.
2. The five substrate rows are implemented in `screens.ts` as far as the
   Core names them today (the Settings overrides list is rendered empty; a
   `REGISTRY_STALE` rejection maps to a degraded Fleet state when a bundle
   exists) and are forced end to end by the milestone that builds the
   substrate: M7 (Browser rows in PX-E2E-023's Browser section, with the
   web content isolation of DR-M3-004), M8 (offline and organization
   overrides), M9 (stale bundle, daily budget). Each of those milestones'
   task cards names this record.
3. The two derivable-but-unforced rows stay proven at the reducer level and
   in the surface suite; a registry publication path (M9) will force the
   quality-floor row through the packaged app.

## Migration

None: renderer-only derivations, three additive IPC channels
(`task:status`, `question:respond`, `approval:resolve`, `review:pullRequest`,
`notify:*`) and one additive client method each; no event or command
changes shape.

## Compatibility

No interface changes meaning.

## Security impact

Notifications carry a title and one line, never a path, a payload or a
secret; OS delivery is off until opted in. Approvals from the card name
the intent hash the Core showed (INTENT_MISMATCH otherwise). The forge
token stays in the Core (DR-M6-002).

## Test impact

`screens.test.ts`, `notifications.test.ts`, `model.test.ts` (wire wait
reasons), `states.spec.ts` (three tests), the assertions added to
`onboarding.spec.ts`, `user-patch.spec.ts` and `fleet-agents.spec.ts`
(the evidence line now follows run events).

## Rollback

Revert the PX-023 commit; the M6.6 board remains.
