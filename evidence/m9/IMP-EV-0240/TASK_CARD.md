# Task Card — IMP-EV-0240 Composable lifecycle registrations

## Identity

- Task ID: IMP-EV-0240 (REQ-EV-0240, ADAPT; owner Hook Bus)
- Milestone: M9
- Qualification: QUAL-EV-0240 — unload extension removes handlers without stale mutation path.
- Evidence tier: release-critical (persistence, a security boundary)
- Built on the IMP-EV-0042 Hook Bus (same change). The extension is the minimal package the Extension System tasks (IMP-EV-0138, 0183, 0225) extend: a directory with `modbit-extension.json` (name, version, hooks).

## Existing-code audit

- classification: NOT-FOUND. No extension could register anything with the Core.
- first missing link: no registration surface outside the Core.
- production entry points: `crates/tools/src/hooks.rs` (`ExtensionManifest::parse`, the `live` check before a handler starts and again before its answer is used — `UNLOADED`); `services/modbit-core/src/hooks.rs` (`load`, `unload`, `HookBus::extensions_of` rebuilt from the session log after a restart); session events `ExtensionLoaded`/`ExtensionUnloaded`; `server.rs` (`LoadExtension`/`UnloadExtension` under `repository.trust`).

## Verification

- `qual_ev_0240_unloading_an_extension_removes_its_handlers_without_a_stale_mutation` (real Core, real handler processes): a loaded extension's `before_tool` handler rewrites a write; `ListHooks` shows it; unloading removes it; a second extension's slow handler is running when it is unloaded — its rewrite is discarded (`UNLOADED`), the call writes what was asked and the late target never appears; afterwards no handler of it runs; loading twice, a missing directory, a directory with no manifest, an invalid manifest and an unknown unload are refused by name; after a Core restart a loaded extension is in force and an unloaded one is not.

## Limitations

- No signature, publisher or marketplace trust yet (IMP-EV-0225); loading is gated on the `repository.trust` capability and recorded with the manifest's digest.
- An extension registers hooks only; commands, tools and providers are IMP-EV-0138.

## Evidence

- `evidence.json` in this directory
- docs/16 "Hook Bus", docs/30
