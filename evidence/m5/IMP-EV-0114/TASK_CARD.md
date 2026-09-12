# Task Card — IMP-EV-0114 Filesystem-discovered skills

## Identity

- Task ID: IMP-EV-0114
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0114; owner label: Skill Registry; subsystem: skills
- Qualification: `QUAL-EV-0114` — Add/remove skill on disk; registry refreshes with hash/provenance and invalid metadata fails.
- Evidence tier: real-system (packages added, removed and changed on disk between runs of one Core)

## Goal

Governed personal and project skills are discovered from the filesystem in a portable `SKILL.md` format; the registry follows the disk with content hashes and provenance, and invalid metadata fails visibly.

## Existing-code audit

- classification: NOT-FOUND before M5.5; built with M5.5's `SkillRegistry::discover` over `<workspace>/.modbit/skills` and `<profile>/skills`.
- production entry points: `crates/skills::SkillRegistry::discover` (per run, roots in order, first name wins, `rejected` for packages that do not load), `services/modbit-core/src/skills.rs` (a package that fails to load is a `SkillRejected` on the task, named by its directory, with the parser's code and reason).
- proof: `qual_ev_0105_0114_…`: run 1 selects `reader@1.0.0` with its hash; the package is removed and a `broken` one (no description) added — run 2 selects nothing and records `SkillRejected` `MISSING_FIELD` for `…/broken`; the package returns changed and re-signed — run 3 records version 1.1.0 with a different content hash.

## Limitations

- Discovery is per run, not a live watcher; a change mid-run applies to the next run.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0105_0114_skill_instructions_survive_compaction_and_the_registry_follows_the_disk`
- `a_package_parses_its_manifest_and_has_a_content_identity`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
