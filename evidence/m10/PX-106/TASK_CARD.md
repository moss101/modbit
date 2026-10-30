# Task Card — PX-106 Customize view over the existing skills, hooks and extensions (no catalog)

## Identity

- Task ID: PX-106 (REQ-PX-106; owner desktop); qualification QUAL-PX-106 / PX-E2E-106; related REQ-EV-0114, REQ-EV-0128, REQ-EV-0139
- Evidence tier: release-critical (drives trust, enable and revoke decisions that change what runs and what reaches the model)
- COMPLETE is not claimed: prerequisite PX-105 is NOT_STARTED in the graph (the Core's skill trust it needs is present and exercised here, but the CLI verbs and the rest of PX-105's acceptance are not mine to close) and PX-052 is REAL_TESTING.

## Existing-code audit

- adoption audit: DOCUMENTED-ONLY; no product caller present
- Skills: Core `ListSkills` / `TrustSkill` / `UntrustSkill` (`services/modbit-core/src/skills.rs`): PRODUCTION-WORKING (content-hash-bound trust file in the profile, System scope, index budget); the desktop used only its slash projection. No `modbit skill` verb exists in `apps/cli` yet (so "the CLI state too" is proven here as the shared on-disk trust record the Core reads, not through a CLI verb).
- Extensions: `LoadExtension` / `TrustExtension` / `UnloadExtension` / `ImportAgentConfig` (`services/modbit-core/src/extensions.rs`): PRODUCTION-WORKING; the list was a string per extension inside `ListHooks`.
- Hooks: `ListHooks` (`services/modbit-core/src/hooks.rs`): PRODUCTION-WORKING list of hooks in force; NOT-FOUND: any way to switch one off, scope/can-disable facts, the journal's last result as a field, a task-less extension list, tool servers per extension.
- Desktop: no Customize surface, no skill/hook/extension IPC. NOT-FOUND.
- First missing link: a view over the existing registries, and (in the Core) the one action the registries lacked: enable/disable of a hook at its scope.

## Implementation

- Core: `crates/protocol/proto/modbit/v1/customize.proto` (new: `SetHookEnabled`, `HookEnabledSet`, `ExtensionRow`, `ToolServerRow`); additive fields on `HookView`, `HookListView`, `ListHooks` in `surface.proto`; `hooks.rs`: `resolve` leaves a switched-off hook out of force (from the latest `HookEnabledSet` on the session's log; survives a restart), `set_enabled` (refuses a device/administrator-layer hook `HOOK_SCOPE_FORBIDS` and an unknown id `HOOK_UNKNOWN`), the list carries scope, enabled, can_disable, the trusted-workspace gate and the journal's last outcome, and structured extension rows with tool servers; `server.rs` dispatch (`ListHooks` with only a session; `SetHookEnabled`, lease-fenced, `repository.trust`).
- Desktop: `packages/ide-adapter-core/src/customize.ts`; `apps/desktop/src/main/customize-ipc.ts` (nine channels, validated; the folder for an import comes from the OS dialog and the path of "open source" from the Core's own listing, never from the renderer; no handler browses, installs from a network location or updates); preload bridge (pin of `hardening-guard.test.ts` moved for this reviewed bridge change); `renderer/customize/` (the view and pure helpers); top-bar "Customize" button and shell wiring.
- The three lists show: skills (scope, trust and why, hash, provenance, invocation, paths and whether active, the budget share of the index and the aggregate), hooks (event, command, scope, trusted-workspace gate, last result from the journal, locked reason), extensions (scope, state, signature, manifest digest, hooks, commands, tool servers with their state and whether a named credential is held). Actions: trust a skill by the hash shown / revoke; switch a hook on or off; trust the exact manifest shown / unload an extension; import another agent's configuration (arrives quarantined); open source.

## Verification

- `services/modbit-core/tests/px_106_customize.rs` (real Core process, real hook processes, SIGKILL): a user-layer hook switched off fires on no later event (also for a task made later and after a SIGKILL of the Core), the administrator's hook still fires and is refused `HOOK_SCOPE_FORBIDS`, an unknown id and an unfenced call are refused, re-enabling fires again, the list carries the journal's last result; an unsigned extension is a structured QUARANTINED row (tool server INERT, credential not held), its hook never runs, a wrong manifest digest is `DIGEST_MISMATCH`, the right one makes it ACTIVE (server CONFIGURED) and its hook runs, its hook can be switched off, unloading removes the row.
- `apps/desktop/e2e/px-106-customize.spec.ts` (Playwright on the Electron app built from source, real Core, real skill registry and trust file on disk, real hook processes, scripted model whose requests are recorded): each list shows scope, trust, provenance; trusting by the hash shown (by keyboard) writes the trust record the Core reads for that name and hash only, the next model request carries that skill's index line and not the other's; trust with no hash is refused in main (BAD_ARGUMENT) and a wrong hash by the Core (HASH_MISMATCH); a skill changed under the view is refused with the Core's code and trusting the refreshed hash works; revoke removes it from the next request; the administrator's hook is locked (HOOK_SCOPE_FORBIDS) and the person's, switched off, does not fire on the next task's event and fires again when switched on by keyboard; the fixture configuration of another agent imports quarantined (tool servers inert), its skill is absent until the exact manifest is trusted and is then listed as the extension's own, not enabled until trusted by hash; unloading removes both; open source asks the file manager for the folder the Core listed; no control, link or bridge function offers a catalog, a download, an install or an update; axe-core passes (twice).
- Node tests: `customize-ipc.test.ts`, `customize-model.test.ts`.
- Mutation checks: `mutation-check.txt` (the Trust button sends no hash: the hash test fails), `mutation-check-core.txt` (a switched-off hook stays in force: both Core tests fail); restored passes.

## Limitations (unproven gaps)

- "Trusting a skill changes the CLI state too": no `modbit skill` verb exists (PX-105); what is proven is that the shared trust record on disk, which the Core reads for every client, changes.
- A tool server's state is what the Core knows without starting it (CONFIGURED / INERT, credential held or not); there is no liveness or health probe of a running server.
- Disabling an extension is "unload"; there is no re-enable of an unloaded extension from this view except importing again (an extension loaded from a directory has no row once unloaded).
- Hooks are listed for a task (they are resolved from its pinned configuration); the view uses the open or most recent task.
- The live gateway leg is not run; macOS only; unpackaged build.
- The Core's `ListSkills` index share is displayed as the Core costs it; this task did not change the index or `skill.load` (PX-105's).

## Seal note (2026-10-28)

The caveat in the Identity section ("COMPLETE is not claimed: prerequisite PX-105 is NOT_STARTED") is superseded: PX-105 and PX-052 are COMPLETE in the graph, and the `apps/cli` skill verbs exist (`apps/cli/tests/px_105_skill_cli.rs`), so "trusting by hash changes the CLI state too" holds. CI run 40742315645 passes `px-106-customize.spec.ts` (six tests, including the axe pass) on the real Electron app against the real Core, on macOS, Ubuntu and Windows.
