# Task Card — IMP-EV-0139 Lifecycle hooks (governed interception)

## Identity

- Task ID: IMP-EV-0139 (REQ-EV-0139, ADOPT; owner Hook Bus)
- Milestone: M9
- Qualification: QUAL-EV-0139 — mutating hook cannot override final monotonic deny.
- Evidence tier: release-critical (policy and a security boundary)
- Built on the IMP-EV-0042 Hook Bus (same change); the interception is a stage of the existing pipeline, before the Capability Kernel.

## Existing-code audit

- classification: DOCUMENTED-ONLY (as IMP-EV-0042): no interception existed; the pipeline's monotonic verdict (REQ-EV-0239) did.
- first missing link: nothing could rewrite or stop a call ahead of the kernel.
- production entry points: `crates/tools/src/pipeline.rs` (a rewrite is re-validated against the tool's schema — `HOOK_REWRITE_INVALID` — re-checked for custody credentials, re-hashed, its effect class recomputed; the kernel decides on it); `crates/tools/src/hooks.rs` (`mutate` only from an intercepting hook before a tool or change; no `allow`); `services/modbit-core/src/hooks.rs` (`Rewrite`: the rewritten targets are read at the rewrite so the change record is the rewrite's; `HookInvoked` carries the rewrite's hash and object); `tools.rs` (final arguments drive the change and retrieval records).

## Verification

- `qual_ev_0139_a_rewriting_hook_cannot_override_the_final_deny` (real Core, real handler processes): a hook rewrites `change.apply` of `notes.txt` onto `rewritten.txt` — the rewrite is what runs, `notes.txt` is untouched, the `FileChanged` record names `rewritten.txt`, `HookInvoked` is `MUTATED` with the rewrite's hash and object, and the result's `arguments_hash` is the rewrite's; under a policy denying `fs.write` a rewritten call is denied by the kernel (not by a hook) and nothing is written; a rewrite onto `.env` is refused; a rewrite that breaks the schema is `HOOK_REWRITE_INVALID`.

## Limitations

- Only tool arguments can be rewritten (`before_tool`, `before_change`); a model request, a verification or a compaction can be stopped, not rewritten.

## Evidence

- `evidence.json` in this directory
- docs/16 "Hook Bus"
