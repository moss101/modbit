# Task Card — PX-029 Explicit degradation path for Tier C and Unsupported languages

## Identity

- Task ID: PX-029
- Milestone: M3 (release BETA)
- Requirement: REQ-PX-029; owner label: context-engine; subsystem: context-engine
- Qualification: `QUAL-PX-029` — A fixture with an Unsupported language and one with a Tier C language: retrieval falls back to text, verification uses only configured commands, the plan states the limitation, every client shows the language state, edits to the Unsupported language require explicit per-task opt-in with provenance.
- Evidence tier: real-system or production-equivalent

## Goal

When a file's language is below the tier the task needs, the product degrades explicitly and visibly, with no silent fallback.

## Existing-code audit

- classification: PARTIAL before this task. PX-027 recorded which languages have a tier; nothing acted on the answer, so a file in a language with no record was retrieved, edited and reported exactly like one with a proven suite.
- production entry point: `crates/verification/src/tiers.rs` (`state_of`, `state_of_path`, `text_format`); `crates/verification/src/plan.rs` (`configured_commands` from `.modbit/verification.json`); `services/modbit-core/src/tools.rs` (`state_of`, the symbol filter, the language provenance on `FileChanged`); `crates/core-runtime/src/harness.rs` (`HarnessRefusal::UnsupportedLanguage`, `may_edit_language`); `AllowUnsupportedLanguage` → `UnsupportedLanguageOptInRecorded`; `ContextEntryView.language_state`; CLI `task allow-language` and the language lines of `context show`.
- proof: a language the product claims nothing about degrades in the open. Retrieval indexes it as text and makes no structural claim: the symbol query answers with nothing rather than with names from a grammar nobody qualified. The verification plan says a repository with no runner of ours has only the commands it configures, and `.modbit/verification.json` is how a Tier C repository gets evidence at all — the plan records that this evidence is heuristic. Every client sees the state per file: the language, the tier or none, whether structure is claimed, whether an edit needs an opt-in, and what the product does instead. An edit is refused with the language named until the user opts this task in, and the change that follows carries `language` and `unsupported_language` on the canonical `FileChanged` event. A Tier C file in the same repository is edited without ceremony and still carries no structural claim.

## Limitations

The opt-in is per task and per language, and `*` opts into all of them. A file whose language cannot be identified is treated as text rather than as unsupported: the product never had a claim to lose there, and the alternative would gate every LICENSE and Dockerfile behind a prompt. The symbol tool filters by the recorded tier, so a language cannot climb from Tier C to Tier B through the conformance suite alone; promoting a language is a deliberate change with its own evidence (PX-028). The degradation is shown by the Core, the CLI and the Context Inspector; the desktop Review screen shows the file list without the language line yet.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_px_029_an_unsupported_language_degrades_explicitly_and_edits_need_an_opt_in`
- `qual_px_027_language_tier_suites_run_on_real_fixtures_and_a_tier_is_only_a_recorded_pass`
- `every_shipped_record_stands_and_an_unrecorded_language_is_unsupported`
- `qual_px_000_headless_cli_task_lifecycle`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
