# Operations and Incident Runbook

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Desktop diagnostics package

User-initiated export includes redacted build/version, Core health, DB integrity result, session/task IDs, recent error codes, event sequence ranges, provider health metadata, sandbox/browser/terminal lease states and checksums. Source/prompts/secrets are excluded unless user explicitly opts in.

## Common incidents

### Core will not start
1. verify DB schema compatibility and disk space;
2. run read-only SQLite integrity check;
3. inspect last migration marker;
4. if corrupt, preserve DB/object store, restore latest internal backup/checkpoint to new DB, never destructive auto-reset;
5. expose export/recovery path.

### Task stuck Running
Check kernel lease heartbeat, current RunStep, provider/tool deadline and protocol state. If owner lease stale, fence old generation and resume. Do not manually flip DB state.

### Unknown external effect
Freeze automatic retry, inspect Effect Ledger + target system idempotency/status, reconcile to `Succeeded/Failed/Unresolved`, require user decision if unresolved.

### Terminal stream gap
Reattach by session/generation/cursor; if replay cursor expired, load OutputRef spill. Do not restart process unless confirmed dead.

### Browser disconnected
Preserve BrowserSessionId/control lease. Reconnect CDP/view stream; if local webContents/remote Chrome is dead, record terminal state/evidence before creating replacement session. Credential re-use follows policy.

### Sandbox lost
Revoke old guest capability, mark in-flight effectful calls unknown, provision new sandbox, restore checkpoint, reconcile, resume.

### Provider outage
Router health moves provider unavailable, queued tasks wait/failover according to model policy. Existing effectful sequence is not replayed from ambiguous partial model stream.

## Cloud operational alarms

- event append failures;
- worker lease split-brain/stale-write rejection spike;
- sandbox provisioning p95 breach;
- cross-tenant authorization denials anomaly;
- effect unknown-outcome rate;
- object-store checksum mismatch;
- provider failure/rate-limit surge;
- checkpoint restore failures.

## Backup / restore

Postgres point-in-time recovery + object storage versioning/replication according to environment. Quarterly restore drill reconstructs selected sessions and verifies receipt/checkpoint hashes. Local Core uses migration backups before destructive schema changes and user-exportable session archive.

## Incident evidence

Never repair production by deleting evidence. Operational remediation appends administrative audit events and preserves the prior corrupted/failed references for forensics when policy permits.

## Routing incidents and rollback

On harmful underrouting, gate false-accept, reviewer escape, budget overrun, partial apply or quality-floor regression, disable the affected workflow/slice via privileged policy activation and reconcile active effects. Select the previous-good signed policy only if current registry/model eligibility, data rules and schema compatibility still hold; otherwise use a validated conservative initial-leg conditional plan or explicit failure. Switch at safe plan/epoch boundaries, preserve spent/reserved budgets and pending receipts, and record actor/reason/evidence/versions. Rollback requires no desktop release and is tested by EPR-012.

Profiler failure uses conservative deterministic profile. Expired registry fails unless a signed last-known-good generation is within freshness and current policy. Missing mandatory gate/reviewer prevents acceptance; budget exhaustion never mints more budget. Separate absent provider usage from zero and reconcile late charges. Monitor the calibration/gate/reviewer/cache/quality/accounting metrics in docs 34/61 and follow ordinary unknown-effect recovery.

## v1.1 conditional incident rules

Within an active transaction use only a previously validated slot; otherwise reconcile and stop before a separately admitted topology with remaining budget. Never roll back into obsolete primary templates or weakened reviewer permissions. Record QUALITY_FLOOR_INFEASIBLE and conservative statistics state rather than hiding a missed target. Gate false-accept or critical_surface_miss_rate violations block promotion independently of cost/success gains. Reviewer incident cleanup kills real process trees, revokes handles and disposes scratch without applying it to canonical state; trusted bounded evidence export stays under existing owners.
