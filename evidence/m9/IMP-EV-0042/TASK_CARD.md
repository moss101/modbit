# Task Card — IMP-EV-0042 Typed lifecycle hooks

## Identity

- Task ID: IMP-EV-0042 (REQ-EV-0042, ADOPT; owner Hook Bus, subsystem `extensions-hooks`)
- Milestone: M9
- Qualification: QUAL-EV-0042 — slow/failing hook follows configured fail policy and cannot bypass monotonic guard.
- Evidence tier: release-critical (policy, execution, a security boundary — code outside the Core runs at every lifecycle point)
- Canonical owner: `crates/tools/src/hooks.rs` (types, handler runner, monotonic fold) under the existing tool pipeline and its REQ-EV-0239 verdict; the Core's `services/modbit-core/src/hooks.rs` resolves registrations and journals. No second policy engine: the Capability Kernel still decides last.

## Existing-code audit

- classification: DOCUMENTED-ONLY. The configuration layers carried a `hooks` list of strings, resolved in authority order with provenance, and nothing ever read it: no hook point, no handler, no timeout, no record.
- first missing link: there was no lifecycle point at which anything could run.
- production entry points: `crates/tools/src/hooks.rs` (`HookPoint` ×12, `HookSpec::parse`, `run_handler`, `fire`, `HookPort`); `crates/tools/src/pipeline.rs` (hook stage after validation and the custody check, before the kernel; after hooks past the effect); `services/modbit-core/src/hooks.rs` (`resolve`: config layers + extensions, project hooks only in a trusted repository; `HookScope::fire` journals `HookInvoked`; `halt`); `runtime.rs` (`before_run`, per-round re-resolution and halt, `before_model`, `after_model`, `after_run`, `run_verification` wrapper, compaction start and install); `tools.rs` (the scope in every tool call's context); `server.rs` (`ListHooks`); `crates/core-runtime/src/diagnostics.rs` (`HOOK_DENIED`/`HOOK_FAILED`, class `Policy`).

## Verification

- `qual_ev_0042_a_slow_or_failing_hook_follows_its_fail_policy_and_cannot_pass_the_monotonic_guard` (real Core, real handler processes): a fail-closed hook that sleeps past its 300 ms timeout is killed and `change.apply` is `POLICY_DENIED`/`HOOK_TIMEOUT` with the file untouched; a fail-open hook exiting 3 is recorded `FAILED` and the change happens; a hook answering `allow` under a policy that denies `fs.write` changes nothing — the kernel's deny stands and the answer is `MALFORMED` ("only the Capability Kernel allows"); an observing hook that denies is `IGNORED`; an `after_tool` observer receives the typed `hooks-1` request with the call's result; `before_run` is observed and a `before_model` interceptor stops the run (`Waiting`, `HOOK_DENIED`) before the provider is asked; `HooksResolved` lists the active hooks; a repository's hook is refused until `TrustRepository`, then listed.
- `crates/tools` unit test `a_declaration_is_typed_and_validated_before_anything_runs`.

## Limitations

- Handlers run as host processes with a scrubbed environment and the workspace as their directory; they are the configuration's or a loaded extension's, never the model's. They are not sandboxed beyond that (as MCP stdio servers are not).
- The payloads are summaries (a model request's message count and tools, not its transcript).

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- docs/16 "Hook Bus" and docs/30 carry the as-built paragraphs
