---
id: DR-PX-2026-10-03-010
title: Automations through the existing Scheduler, policy kernel, approval aggregate and effect ledger, with unattended runs failing closed (PX-082..086, doc 68)
status: accepted
date: 2026-10-03
supersedes: none
approved_by: owner instruction 2026-10-05 (goal: implement research/audit/01-TASK-LIST.md); the basis of 007 is the owner's decisions of 2026-10-03
---

# DR-PX-2026-10-03-010 — Automations

## Problem and goal

Every Modbit task starts because a person asked. Real engineering work also starts on a schedule or on an event: a nightly drift report, a pull request opened, a failed CI run. The owner has made automations a real goal. Two sealed constraints govern it: MOD-AUTO-001 (DEFERRED) and REQ-EV-0149 and REQ-EV-0264 (DEFERRED) defer recurring and triggered work, and doc 81 forbids a second scheduler, policy engine, approval system or effect ledger. The goal is to let a person define work that starts by itself, run it through the existing owners under an explicit principal, a policy ceiling and a budget, and have unattended runs fail closed.

## Trigger and evidence

- **Owner's statement of 2026-10-03** and its hard carry-overs: automations run through the existing scheduler, policy, approval and effect-ledger owners (no second scheduler), fail closed, and have crash and restart semantics.
- **Sealed requirement text.** REQ-EV-0149: host-managed triggers create tasks with explicit principal, budget and policy; REQ-EV-0264: a host scheduler may create software-engineering runs and the model never self-grants scheduling authority; REQ-EV-0275: no second scheduler; REQ-EV-0236 (REJECT): messaging and consumer breadth is out; REQ-EV-0046: detached agents have a permission ceiling.
- **Research (tags per doc 65 section 3).** STATIC: the reference product's automations are cloud agents started by triggers (schedule, forge events, chat, incident and issue-tracker events, a webhook) with run records (running, failed, succeeded, skipped, cancelled), principals (service account, creator, current user), configuration-as-code definitions shown read-only, a gate prompt that decides whether the rest runs, validation on enable, a test-run that simulates trigger payloads through the filters, and linear-time validation of user expressions (`F05` A1); local scheduled tasks run only while the app is open, require a local project and a trusted workspace, and ship a template that fetches and reports without mutating the tree (`F02` §3.8); a webhook uses a generated authorisation header (`F05` A1). DISK: the sampled profile had the automations UI gate on and no definitions. UNVERIFIED: no live observation of the surface was possible; Modbit's concurrency, idempotency, expiry and approval rules below are independent design.
- **Audit of `main` 5fdb47f** (doc 68 section 3): the Scheduler, capacity tickets, `CreateTask` origins and forge webhook intake (PX-011) are production-working; time-based triggers, a principal model for definition-created runs and parked-approval expiry are NOT-FOUND or partial; definitions, run history and UI are NOT-FOUND.

## Current behavior

No definition store, timer source, trigger evaluation, run record or UI. Cloud webhooks create tasks for a tenant (PX-011) but nothing matches them to a definition. The `automation` subsystem exists in the graph as DEFERRED with no crate or milestone.

## Proposed replacement

Specified in doc 68 (23 requirements) and rows PX-082 to PX-086, all release-critical:

1. **PX-082** definitions: versioned, principal-bound, validated, enabled by an approval bound to the exact definition hash; repository-supplied definitions inert until approved; no model authority.
2. **PX-083** trigger evaluation and dispatch inside the existing Scheduler: a durable time source, idempotency per event id, concurrency policies, missed-run policy, budgets.
3. **PX-084** unattended run policy: permission ceiling, parked approvals that expire and fail closed, untrusted payloads, kill switches.
4. **PX-085** cloud triggers: signed webhooks with replay protection, tenant mapping, the cloud time source.
5. **PX-086** the surface and CLI: list, editor with validation, exact enable approval, dry-run test, run history, pause.

## Independent Modbit design

An automation is a **definition**, a versioned event-sourced record in the `automation` owner, which runs nothing. When a trigger fires the Core creates an ordinary task through `CreateTask` with origin `automation` and provenance (definition, version, event id); the existing Scheduler admits it with a capacity ticket; the ordinary loop runs it in an isolated worktree or sandbox; the approval aggregate and effect ledger govern its protected effects. The Scheduler gains one input (a durable due-queue of time triggers) and one event source, not a second loop. Dispatch is idempotent per (definition, version, event id). A model can never create, edit, enable or schedule a definition. Unattended means detached: a permission ceiling equal to the definition's profile intersected with its principal's policy, no interactive privilege expansion, protected effects park in Needs Attention with an expiry (24 h default) and then fail closed. Capability profiles default to read-only; any write, network or external effect needs an enable approval that lists the exact capabilities, paths and hosts and binds the definition hash. The first release ships one real reference definition (report drift of the default branch, read-only, Kernel-enforced).

## Security model

- Principal bound on every run; credentials only through the broker for that principal; definitions hold no secret.
- Enable approval bound to the definition hash; edits create versions and disable until re-approved; repository-supplied definitions load disabled and enable only by hash approval at a repository revision.
- Trigger payloads are untrusted external content with provenance, scanned for injection, never authority.
- Webhooks: HMAC over the body, timestamp window, nonce against replay, tenant mapping, secret shown once and held by the broker.
- Unattended runs cannot self-approve and no agent in a run can approve; native computer control (DR-PX-2026-10-03-008) is unavailable to them; local trusted repository required.
- Kill switches: global and per-definition pause, session emergency stop, auto-disable after five consecutive failures.
- Crash and restart: next due slot persisted; missed slots follow the declared policy and never replay in bulk; a run killed mid-flight reconciles through the existing recovery path to one outcome.

## Canonical owner mapping (doc 81)

`automation` owns definitions only (PX-082); `core-runtime` owns trigger evaluation and dispatch inside the existing Scheduler (PX-083); `effects-security` owns the unattended policy (PX-084); `sandbox-cloud` owns cloud intake (PX-085); `desktop` owns the surface (PX-086). The Scheduler, policy kernel, approval aggregate, effect ledger and task runtime keep their single owners.

## Alternatives rejected

- **A separate automation runner or queue.** Rejected: a second scheduler (doc 81, REQ-EV-0275).
- **A daemon that fires local schedules while the app is closed.** Not in this record: it is shell integration and an execution-trust question; local schedules fire only while the Core runs, as in the reference. Left as an owner question.
- **Auto-approving effects in automation runs.** Rejected: defeats the point of unattended policy; parked approvals expire and fail closed instead.
- **Letting the model create automations from conversation.** Rejected: REQ-EV-0264.
- **Messaging, incident and issue-tracker triggers now.** Rejected: REQ-EV-0236 for messaging; other forges and trackers need a forge-family adapter and their own qualification.
- **Replaying every missed slot after downtime.** Rejected: a burst of unattended runs after a restart; skip or catch up once.
- **Chained prompts with independent models per step.** Rejected: one plan per run; only an optional read-only gate step is adopted.

## Supersessions

Recorded in docs 02 and 03 and in doc 68 section 6, effective only on acceptance:

- **MOD-AUTO-001 (DEFERRED, broad consumer automations and scheduling)** is superseded in scope: software-engineering automations are adopted; broad consumer automations stay out (REQ-EV-0236).
- **REQ-EV-0149 and REQ-EV-0264 (DEFERRED)**: the sealed base rows stay byte-identical (docs 40, 41 and 42 are locked and immutable); REQ-PX-082 to REQ-PX-086 are the additive adoption and carry the same constraints.
- The `automation` subsystem (DEFERRED) is activated as the owner of definitions only.
- Nothing EPR-pinned is touched. Pinned counts are unchanged.

## Migration

Additive: new PX rows, change record, DOC-PX-010, a new crate registered under the `automation` owner (doc 12 and doc 13 are locked paths; the implementing task cites this record for the additive notes).

## Compatibility

Rows are M10, RELEASE_ZERO, depending on PX-011, PX-040, M6.2, M6.7 and, for the surface, PX-044 to PX-046 and PX-058 of DR-PX-2026-10-03-007. Cloud automations depend on the cloud API of M8.

## Security impact

Introduces unattended execution. Each control above has a negative proof, including mutation checks that fail when the hash binding, the idempotency key, the ceiling intersection or the timestamp window is removed.

## Test impact

Five qualifications and scenarios in doc 62 on a real Core with a controllable time source, real worktrees, the live gateway and a staging Cloud API; kills across slots and mid-run; hostile pull-request fixtures.

## Rollback

Revert the commits adding the rows and spec and rerun the reseal; a new record is needed for the locked paths.

## Consequences and owner questions

- **Question 1.** Should local schedules fire while the app is closed (a login-item daemon)? This record says no; the answer belongs with DR-PX-2026-10-03-012 and an execution-trust decision.
- **Question 2.** Are the schedule floor (5 minutes) and the approval expiry (24 hours) the right defaults? They are policy data.
- RELEASE_ZERO grows by five work items.

## Explicit user approval

Accepted on 2026-10-05 by the owner's instruction recorded in `approved_by` (the goal to implement research/audit/01-TASK-LIST.md, whose BLD-25 is the automation layer). The record is ratified as drafted, which includes its defaults for the two owner questions above (local schedules fire only while the Core runs; a five-minute schedule floor and a 24-hour approval expiry as policy data); the owner may change them by a later record. The scoped supersession of MOD-AUTO-001 takes effect from this date. DOC-PX-010 is therefore COMPLETE.
