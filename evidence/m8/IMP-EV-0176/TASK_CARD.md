# Task Card — IMP-EV-0176 Workspace Capsule

## Identity

- Task ID: IMP-EV-0176
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0176 (a portable, bounded task package of context, decisions, evidence and runtime requirements); QUAL-EV-0176 (a handoff verifies no authority or secret value is smuggled in the capsule); docs/21 "Handoff local → cloud" (never raw secret values; capability parity before the owner switches).
- Qualification: the M8.7 handoff scenario on real components, with capsules that smuggle a grant field and a secret-shaped value refused at admission.
- Evidence tier: real-system.

## Goal

The handoff bundle is the capsule: the task's log (its context, decisions and evidence as events and objects), its git delta and checkpoint, its runtime requirements as capability names and its secrets as handles — and the cloud's admission verifies that nothing in it is authority or a secret value.

## Existing-code audit

- classification: PARTIAL before this task: the M8.7 bundle carried the right content and the laptop scanned nothing into it, but the admission trusted the manifest's shape — a capsule with a grant field or a token pasted into a text field would have been admitted.
- production entry points: `apps/cloud-api/src/routes.rs` (`capsule_smuggles`: the manifest refused when any key is a grant or credential field — `lease`, `token`, `api_key`, `secret`, `credentials`, `private_key`, … — `409 CAPSULE_SMUGGLES_AUTHORITY`; the manifest and the bundled log refused when any value is secret-shaped — a provider key, a forge token, a worker token, a bearer, an AWS key id, a PEM private key — `409 CAPSULE_SMUGGLES_SECRET`; both on the command ledger), `services/modbit-core/src/handoff.rs` (the capsule's content: log, objects, git bundle, manifest with `capabilities`, `secret_handles`, `tools_used`).
- proof: `apps/cloud-worker/tests/cloud_worker.rs::qual_m8_7_…`: the laptop holds a provider key and a forge token; every file of the exported capsule is scanned for both values (none; the handle `forge-token` is named); a manifest with `token: <the forge token>` is `409 CAPSULE_SMUGGLES_AUTHORITY`; a manifest whose goal text quotes the provider key is `409 CAPSULE_SMUGGLES_SECRET`; a manifest naming an unserved capability is `409 CAPABILITY_PARITY`; the real capsule is admitted and continued in a sandbox.

## Limitations

- The secret scan is by shape (known token prefixes, bearers, PEM); an opaque high-entropy string with no recognizable shape passes — the laptop's export never writes a custody value into the bundle in the first place, which the scan of the exported files proves.
- Bounded by the handoff's bounds (the upload body limit; objects by hash).

## Verification

- `qual_m8_7_a_local_task_hands_off_to_the_cloud_and_continues_from_its_checkpoint_in_a_sandbox`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
