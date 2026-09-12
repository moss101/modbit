# Task Card — IMP-EV-0181 Extension-provided skills

## Identity

- Task ID: IMP-EV-0181
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0181; owner label: Skill Registry; subsystem: skills / cli
- Qualification: `QUAL-EV-0181` — Install extension skill, validate hash, activate without capability escalation.
- Evidence tier: real-system (the real CLI installing into a real profile; the real Core running the installed skill)

## Goal

Portable SKILL.md packages provided by extensions are imported with provenance and trust: the hash validated at install, the lifecycle decided by the registry, nothing escalated.

## Existing-code audit

- classification: NOT-FOUND before: no install path; packages had to be placed by hand.
- production entry points: `crates/skills::install` (load, `--expect-hash` check, copy, `PROVENANCE.json` outside the identity, re-validation of the copy), `uninstall`; `apps/cli` `skill install|remove|list`.
- proof: `qual_ev_0181_0210_…`: a wrong expectation is refused naming both hashes; the right one installs with provenance; a second install needs `--replace`; the listing shows ENABLED under the trusted signature; the task's `SkillSelected` carries the same content hash; the destructive tool the skill names never becomes a call; removal and reload keep the identity.

## Limitations

- Install reads a directory (an archive format is not defined); trust is the registry's signature check, not an installer-side allowlist.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0181_0210_an_extension_skill_installs_lists_runs_its_procedure_and_survives_removal_and_reload`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
