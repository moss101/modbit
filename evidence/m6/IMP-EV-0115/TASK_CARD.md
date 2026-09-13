# Task Card — IMP-EV-0115 File-defined custom agents

## Identity

- Task ID: IMP-EV-0115
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0115; owner label: Agent Runtime; subsystem: domain (`agent_profile.rs`), core (`agent_profiles.rs`, `spawn.rs`), CLI
- Qualification: QUAL-EV-0115 — Profile requesting forbidden tool receives narrowed surface.
- Evidence tier: real-system (real profile files installed under the Core's data dir; a real child admitted through the real transaction against a scripted OpenAI-compatible server)

## Goal

Custom agents are declarative profiles; Core compiles effective tools/capabilities.

## Existing-code audit

- classification: MISSING before: a child's tools came only from the spawn's `required_tools`; no reusable, file-defined profile existed.
- production entry points: `crates/domain/src/agent_profile.rs` (`AgentProfile`, `parse_profile`, `install`, `list`), `services/modbit-core/src/agent_profiles.rs` (`roots`, `load`, `compile`), `spawn.rs` step 0 (the profile compiled into the request and the capsule; `PROFILE_UNKNOWN` / `PROFILE_INVALID` refusals), `runtime.rs` (the capsule json carries `profile`, `profile_context`, `narrowed_tools`), `apps/cli` `agent install` / `agent list`.
- proof: a `researcher` profile asking for `fs.read`, `search.exact`, `git.worktree.close`, `nonexistent.tool` and `agent.spawn` is compiled into a child whose projection holds exactly the first two: the destructive tool is dropped as above the child's ceiling, the unknown one as not served, the delegation tool as a child delegates nothing — each named in `SubagentAdmitted.narrowed_tools` and the spawn's result; the child runs on the profile's model (`gpt-5-mini`) while the parent keeps `gpt-5`, with the profile's write scope and its body in the capsule, and reports.

## Limitations

- Profiles carry no per-profile policy layer (permissions come from policy); a profile's model must be served on the parent's endpoint.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0115_0182_0241_agent_profiles_compile_into_capsules_and_only_narrow`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
