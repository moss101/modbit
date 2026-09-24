# Task Card — IMP-EV-0137 Import config from other agents

## Identity

- Task ID: IMP-EV-0137 (REQ-EV-0137, ADAPT; owner Importers, `crates/skills (import)`)
- Milestone: M9
- Qualification: QUAL-EV-0137 — malicious executable config is quarantined until user trusts.
- Evidence tier: release-critical (a security boundary: executable configuration from elsewhere)
- Built on IMP-EV-0183's importer (same change) and IMP-EV-0225's quarantine: the trust gate is the extension's.

## Existing-code audit

- classification: NOT-FOUND for skills/rules/MCP import with a trust gate (only one Claude agent file could be converted).
- first missing link: nothing held imported executable configuration inert.
- production entry points: as IMP-EV-0183; the gate is `services/modbit-core/src/extensions.rs` `load` (an import is unsigned, so quarantined) and `trust`; the importer skips commands that run shell and profiles that ask for authority.

## Verification

- `qual_ev_0137_malicious_executable_config_is_quarantined_until_the_person_trusts_it` (real Core): a repository's `.mcp.json` declares a server whose command writes a marker file the moment it starts, a Claude command runs `rm -rf` when expanded and a profile asks for every permission. Imported, the command and the profile are `SKIPPED`, the server is `MAPPED` and shown with its command line; loaded, the extension is quarantined — neither `external.list` nor an `external.call` naming the server starts it, and the marker does not appear; trusted, the listing starts it and the marker appears. Mutation: loading without quarantine fails the test.

## Limitations

- As IMP-EV-0183.

## Evidence

- `evidence.json` in this directory
- docs/16 "Importers as built"
