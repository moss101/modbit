# Task Card — IMP-EV-0138 Unified plugins with commands/tools/hooks/providers

## Identity

- Task ID: IMP-EV-0138 (REQ-EV-0138, ADAPT; owner Extension System, subsystem `extensions-hooks`)
- Milestone: M9
- Qualification: QUAL-EV-0138 — extension crash/timeout cannot bypass Core or corrupt run state.
- Evidence tier: release-critical (execution, a security boundary, provider routing)
- No new runtime: the package routes each part to its existing canonical owner — Hook Bus (IMP-EV-0042), External Tool Hub (M9.4), the task input queue (REQ-EV-0262), the Provider Gateway.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. The Hook Bus change (IMP-EV-0240) introduced the extension directory and manifest with hooks only; nothing let one package add tools, commands or providers, and a manifest could not be inspected before loading.
- first missing link: the manifest had no place for tools, commands or providers.
- production entry points: `crates/tools/src/extensions.rs` (`ExtensionManifest` with `hooks`/`tools`/`commands`/`providers`, `capabilities`, host-only declarations refused); `services/modbit-core/src/extensions.rs` (`load`, `unload`, `run_command`, `register_providers`, `ensure_session`, `servers_of`); `tools.rs` (extension servers join the task's hub scope, trusted by the Core, a configured server of the same name wins); `mcp.rs` (`credential` for a provider's handle); `server.rs` (`RunExtensionCommand`; `StartTask` registers the session's extension providers before routing).

## Verification

- `qual_ev_0138_an_extension_crash_or_timeout_cannot_bypass_the_core_or_corrupt_run_state` (real Core, signed extension): the extension's provider `ext.kit.local` points at a scripted OpenAI-compatible server and a run pinned to it reaches it (every request `kit-model`); its tool server (the real MCP test server) dies mid-call and the call is `UNKNOWN_OUTCOME`/`EXTERNAL_OUTCOME_UNKNOWN`, never a success; its `before_tool` hook times out and its `after_tool` hook exits 9 (both fail open, recorded `TIMEOUT`/`FAILED`, not applied) and the run still completes its verified change (`ReadyForReview`); its command's text is queued as the person's input with provenance `extension:kit/annotate`; after a kill and restart the recovery report has nothing to repair and the task is where it was; unloaded, its provider leaves the gateway and a run pinned to it is refused.
- `crates/tools` unit test `a_manifest_is_validated_whole_and_says_what_it_would_do` (including refused self-declared trust and reads).

## Limitations

- A provider is registered Core-wide while any session has its extension active (the gateway's endpoints are Core-wide); it is named for the extension.
- Commands are templates; they carry no tools or permissions of their own.

## Evidence

- `evidence.json` in this directory
- docs/16 "Extension System", docs/30
