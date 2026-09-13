# Task Card — IMP-EV-0241 Profiles/bundles

## Identity

- Task ID: IMP-EV-0241
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0241; owner label: Agent Runtime; subsystem: domain (`agent_profile.rs`), core (`agent_profiles.rs`, `spawn.rs`), CLI
- Qualification: QUAL-EV-0241 — Profile validation rejects unknown/unsafe capability expansion.
- Evidence tier: real-system (real profile files installed under the Core's data dir; a real child admitted through the real transaction against a scripted OpenAI-compatible server)

## Goal

Profiles are declarative compiled config, not arbitrary runtime replacement.

## Existing-code audit

- classification: MISSING before: no profile validation existed.
- production entry points: `crates/domain/src/agent_profile.rs` (`UNSAFE_KEYS`: `effect_ceiling`, `lease`, `resources`, `operations`, `execution_profile`, `permissions`, `allow`, `network`, `secrets`, `credentials`, `depth`, `nesting` — refused at parse, so at install, at list and at every load), `services/modbit-core/src/agent_profiles.rs::load` (re-validation on every load).
- proof: `effect_ceiling: DESTRUCTIVE` and `lease: [...]` refuse the install naming the key, nothing written; a profile edited in place to add `permissions` is rejected by `list` and refuses the admission `PROFILE_INVALID` at stage `PROFILE` with nothing taken; a valid profile can only narrow the child's surface.

## Limitations

- Bundles (several profiles shipped together) are a directory of profile files; no signing yet (skills have it).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0115_0182_0241_agent_profiles_compile_into_capsules_and_only_narrow`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
