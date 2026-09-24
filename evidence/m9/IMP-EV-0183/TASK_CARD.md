# Task Card — IMP-EV-0183 Extension context/commands/MCP resources

## Identity

- Task ID: IMP-EV-0183 (REQ-EV-0183, ADAPT; owner Extension System / Importers, `crates/skills (import)`)
- Milestone: M9
- Qualification: QUAL-EV-0183 — compatibility fixture imports and migration report labels mapped/skipped/conflicts.
- Evidence tier: release-critical (configuration that becomes instructions and executable tool servers)
- No external-reference runtime: everything is converted into a Modbit extension, read by the existing owners (rules, agent profiles, skills, External Tool Hub, extension commands).

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. `modbit_domain::agent_profile::import_claude_agent` converted one Claude agent file; nothing else of another agent's configuration could be brought in, and nothing reported what was or was not.
- first missing link: no importer for instruction manifests, commands, skills or MCP servers, and no report.
- production entry points: `crates/skills/src/import.rs` (`import`, `ImportReport`, `Existing`); `services/modbit-core/src/extensions.rs` (`import`, `existing_for`, `check_files`, `active_dirs`); `crates/tools/src/extensions.rs` (`files` in the manifest); `rules.rs`, `agent_profiles.rs`, `skills.rs` (an active extension's `rules/`, `agents/`, `skills/`); `server.rs` (`ImportAgentConfig`).

## Verification

- `the_compatibility_fixture_imports_and_every_item_is_labelled` (`crates/skills`): the committed fixture `tests/fixtures/agent-configs/mixed` (AGENTS.md, CLAUDE.md with an `@` import, a nested AGENTS.md, .cursorrules, a Cursor `.mdc` with globs, Copilot instructions, three Claude commands, two Claude agents, a Claude skill with a script, a Gemini TOML command, Cursor and VS Code MCP files, Claude settings) yields exactly 19 labelled items — each rule, command, agent, skill and server `MAPPED`/`SKIPPED`/`CONFLICT` as expected, with reasons; every written file is listed by its digest; the rule, skill and profile written parse as Modbit's own; against a destination that already has a `reviewer` profile and a `docs` server both are `CONFLICT`; an existing import is not overwritten unless asked.
- `qual_ev_0183_…` (real Core): the fixture plus a real stdio server imports with labels (credential not imported), loads quarantined (the server not listed), and once trusted its rules are active in a run's `RulesSelected`, its server is listed and its command queues its text; a rule file changed after the import refuses the load (`EXTENSION_INVALID`).

## Limitations

- TOML commands (Gemini) and non-stdio MCP servers are skipped by name, not converted.
- Instruction files' `@file` imports are not followed; each named file can be imported as a rule of its own.

## Evidence

- `evidence.json` in this directory
- docs/16 "Importers as built", docs/30
