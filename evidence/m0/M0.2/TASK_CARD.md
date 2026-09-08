# Task Card — M0.2 Add authoritative ADRs and status ledger

## Identity

- Task ID: M0.2
- Milestone: M0 — Repository and authority
- Canonical owner: Architecture governance (dossier authority, `docs/02`); tooling in `tools/architecture-lint`
- Requirement IDs: roadmap task `docs/43` M0.2; supports docs/02 change control, docs/46 freeze gate, docs/70 branch policy
- Risk class: medium (governance guard; no runtime behavior)
- Evidence tier: release-critical (touches evidence/governance semantics)
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

CI rejects any commit that changes a locked architecture file unless the commit carries a `Decision-Record:` trailer naming an accepted record under `docs/decisions/`.

## Non-goals

- Duplicating the decision register (`docs/02` remains the register of record).
- Re-checking historical commits.
- Protobuf generation (M0.3).

## Required reading

`AGENTS.md`, `docs/02` (change control), `docs/12` (docs/decisions layout), `docs/46`, `docs/70`, `docs/74`, `docs/93`.

## Existing-code audit

- classification: NOT-FOUND
- production entry point: none (no docs/decisions, no guard, no CI job)
- current owner: none
- real effector/storage boundary: Git history (real `git` binary)
- tests found: none
- first missing/broken link: no machine-checkable link between a locked-file change and its Decision Record
- duplicate/drift risks: a second decision register; avoided by making docs/decisions hold full records and a ledger only, pointing at docs/02

## Invariants

- Locked paths are data in `tools/architecture-lint/rules.toml` and are themselves locked.
- Accepted records are immutable; changes are new records with `supersedes`.
- Reseal-only commits need no trailer.

## Implementation slice

- domain/API: `architecture_lint::locked` (`check_locked`, `is_accepted_record`, `commits_in_range`)
- persistence/migrations: `docs/decisions/` (README ledger + DR-M0-002); manifest scope extended to hash them
- policy/capabilities: `[locked_policy]` and `[[locked]]` rules
- service/effector: `architecture-lint locked` subcommand over real Git; CI job `locked-files` with full history
- UI/model projection: none
- failure/recovery: exit 1 on violation, 2 on git/rules error; zero/absent base falls back to the head commit's parent
- evidence/observability: violations name commit, paths and reason

## Verification

- unit: front-matter status detection
- integration (real Git): no trailer → reject; accepted record → pass; record added in same commit → pass; missing / proposed / outside-dir record → reject; unlocked change → pass; range respected; CLI exit codes
- real repo: guard run over ceef604..HEAD (0 violations) and a live scratch branch with a silent docs/11 change (1 violation, `inject-failure-silent-locked-change.log`)
- E2E: hosted CI job `locked-files` on the introducing commit (which carries its own trailer)

## Completion evidence

See `evidence.json`.
