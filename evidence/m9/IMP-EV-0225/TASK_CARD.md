# Task Card — IMP-EV-0225 Marketplace trust surfaced

## Identity

- Task ID: IMP-EV-0225 (REQ-EV-0225, ADOPT; owner Extension System)
- Milestone: M9
- Qualification: QUAL-EV-0225 — unsigned/untrusted extension is quarantined.
- Evidence tier: release-critical (a security boundary: third-party code and endpoints)

## Existing-code audit

- classification: NOT-FOUND. Extensions (IMP-EV-0240) loaded active with no publisher, source or signature and nothing shown before activation.
- first missing link: no publisher identity or signature on an extension.
- production entry points: `crates/tools/src/extensions.rs` (`verify` — ed25519 over the manifest's exact bytes against `MODBIT_EXTENSION_KEYS`; `SignatureStatus` and its quarantine reason; `capabilities`); `services/modbit-core/src/extensions.rs` (`inspect`; `load` quarantines anything not `VERIFIED`, refuses a manifest other than the inspected one; `trust` releases only `UNSIGNED`/`UNKNOWN_KEY` for the exact digest, never `INVALID`); `hooks.rs` (a quarantined extension registers no hook and serves no tool); session events `ExtensionLoaded` (signature, quarantine) and `ExtensionTrusted`.

## Verification

- `qual_ev_0225_an_unsigned_or_untrusted_extension_is_quarantined` (real Core): `InspectExtension` shows publisher, source, `UNSIGNED` and three capabilities (the hook's program, the command, the provider's address) before anything loads; the unsigned extension loads quarantined — its hook never runs, its command is `EXTENSION_QUARANTINED`, its provider is not in `ListModels`; trusting it with a wrong digest is `DIGEST_MISMATCH`, with the inspected digest it becomes active (provider registered, hook runs, command queues its text); one signed by an unknown key is quarantined naming the key; one changed after signing is `INVALID`, quarantined and `EXTENSION_TAMPERED` when trusted; a trusted publisher's is `VERIFIED` and active on load; a manifest changed after inspection is `EXTENSION_CHANGED`.
- `crates/tools` unit test `only_a_trusted_key_over_the_exact_bytes_verifies`.

## Limitations

- The surface is the protocol (any client: desktop, CLI, IDE); a dedicated desktop extensions panel is not part of this change.
- No revocation list: a publisher key is trusted until it is removed from `MODBIT_EXTENSION_KEYS`.

## Evidence

- `evidence.json` in this directory
- docs/16 "Extension System", docs/30
