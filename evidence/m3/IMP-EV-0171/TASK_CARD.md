# Task Card — IMP-EV-0171 Context Engine as shared service/ports

## Identity

- Task ID: IMP-EV-0171
- Milestone: M3 (context batch)
- Requirement: REQ-EV-0171; owner label: Context Engine; subsystem: context-engine
- Qualification: `QUAL-EV-0171` — Architecture test prevents duplicate search stacks in production modules.
- Evidence tier: real-system or production-equivalent

## Goal

All agents/reviewers/browser/testing use same context ports.

## Existing-code audit

- classification: IMPLEMENTED by DR-M3-001 in this batch (was PARTIAL: ports existed, no guard)
- production entry point: tools/architecture-lint/rules.toml confine rules; modbit-tools SearchPort served by the Core's IndexPort
- proof: every consumer reaches search through the same port and tools; the architecture lint (CI job) fails on a direct tantivy/usearch/tree-sitter dependency outside modbit-retrieval

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `confine_rejects_direct_use_outside_allowed_members`
- `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
