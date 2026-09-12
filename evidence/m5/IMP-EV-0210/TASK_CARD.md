# Task Card — IMP-EV-0210 Tool/skill creation with registration validation

## Identity

- Task ID: IMP-EV-0210
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0210; owner label: Skill/Tool Developer Kit; subsystem: skills / procedural-runtime
- Qualification: `QUAL-EV-0210` — Test plugin registers, lists, invokes real effector and passes removal/reload.
- Evidence tier: real-system (a skill package with a procedure as the plugin; the real registry, the real isolate, a real `fs.read`)

## Goal

A new skill or procedural tool is validated for schema, registry wiring and invocation path — a Markdown file is not a skill until the registry lists it and its procedure runs through the real effector.

## Existing-code audit

- classification: PARTIAL before: M5.5 validated manifests; nothing exercised register → list → invoke → remove → reload as one path.
- production entry points: `crates/skills::{install, uninstall, SkillRegistry::discover}`; the procedure runs through `proc.exec` (M5.4) and the registry (M5.3).
- proof: `qual_ev_0181_0210_…`: the plugin registers (install), lists (ENABLED with its hash), its `count.js` procedure is invoked by a task through `proc.exec` as an ordinary `fs.read` tool call under the exec id (`ProgramEnded` COMPLETED), is removed (gone from the list; a second removal refused), and reloads with the same identity.

## Limitations

- The plugin is a skill package; native tool plugins (new registry namespaces) go through the tool matrix and REQ-EV-0230's lint, not an install command.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0181_0210_an_extension_skill_installs_lists_runs_its_procedure_and_survives_removal_and_reload`
- `qual_ev_0230_every_tool_namespace_carries_a_build_or_buy_justification`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
