# Task Card — IMP-EV-0209 SKILL.md-based procedural packaging

## Identity

- Task ID: IMP-EV-0209
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0209; owner label: Skill Package; subsystem: skills
- Qualification: `QUAL-EV-0209` — Package parser validates metadata/resources and rejects malformed/oversized package.
- Evidence tier: production-equivalent (the production parser over real directories)

## Goal

Portable skill documentation and resources in a `SKILL.md` package, with Modbit's execution policy kept separate: the parser validates the manifest and the resources and refuses what does not fit.

## Existing-code audit

- classification: NOT-FOUND before M5.5; the parser, the file limits and the typed refusals are M5.5's `load_package` with this task's size ceilings.
- production entry points: `crates/skills::parse_skill_md` / `load_package` (front matter, required fields, name charset; `MAX_FILE_BYTES` 1 MiB, `MAX_PACKAGE_BYTES` 4 MiB → `SkillError::Oversized { path, bytes, limit }`); resources by reference with hash and size.
- proof: `qual_ev_0209_the_parser_rejects_malformed_and_oversized_packages`: no front matter, a bad name and a missing field are refused with their codes; a 1 MiB + 1 resource is `Oversized` naming the file; five near-limit resources make the package `Oversized` as a whole.

## Limitations

- Limits are constants; a per-deployment ceiling is not configurable yet.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0209_the_parser_rejects_malformed_and_oversized_packages`
- `a_package_parses_its_manifest_and_has_a_content_identity`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
