# Task Card — IMP-EV-0182 Extension-provided subagents

## Identity

- Task ID: IMP-EV-0182
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0182; owner label: Agent Runtime; subsystem: domain (`agent_profile.rs`), core (`agent_profiles.rs`, `spawn.rs`), CLI
- Qualification: QUAL-EV-0182 — Invalid/unsafe tool declaration is narrowed or rejected.
- Evidence tier: real-system (real profile files installed under the Core's data dir; a real child admitted through the real transaction against a scripted OpenAI-compatible server)

## Goal

Declarative agent profiles import into canonical Modbit profile schema.

## Existing-code audit

- classification: MISSING before: no import path for a foreign agent declaration existed.
- production entry points: `crates/domain/src/agent_profile.rs::import_claude_agent` (`tools: Read, Grep, Bash, Task, …` → canonical names via `map_claude_tool`; unknown names kept for the compiler), `install(..., Some("claude"), ..)` writing the canonical form; the same `compile` at admission.
- proof: a Claude-style `code-reviewer.md` imports as a canonical profile with `source: claude` and tools `fs.read`, `search.regex`, `proc.exec`, `agent.spawn`, `Mystery`; a foreign file carrying `permissions` is refused at import; at admission the unknown and delegation tools are narrowed and named, never granted.

## Limitations

- One foreign format (Claude Code agent files) is mapped; others fall back to the canonical schema.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0115_0182_0241_agent_profiles_compile_into_capsules_and_only_narrow`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
