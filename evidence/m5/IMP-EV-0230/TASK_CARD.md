# Task Card — IMP-EV-0230 Procedural skills over existing tools

## Identity

- Task ID: IMP-EV-0230
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0230; owner label: Skill Compiler; subsystem: tools (registry lint)
- Qualification: `QUAL-EV-0230` — Build/buy lint requires justification for every new tool namespace.
- Evidence tier: production-equivalent (a test over the production registry and the canonical inventory document)

## Goal

Prefer skills and scripts over existing canonical tools; a native tool namespace exists only with a build/buy justification tied to the canonical inventory.

## Existing-code audit

- classification: PARTIAL before: the tool matrix (REQ-EV-0217) covered every tool's owner, effect and test but nothing asked why a namespace exists.
- production entry points: `crates/tools/tool-matrix.json` `namespaces` (namespace → docs/17 `inventory_ref` row → justification), `crates/tools/tests/pipeline_and_direct.rs::qual_ev_0230_…` (every registered namespace has a row with a justification of at least 20 characters and an `inventory_ref` that is a row of docs/17; stale rows fail).
- proof: the lint passes over the eleven registered namespaces (`artifact`, `change`, `context`, `evidence`, `fs`, `git`, `knowledge`, `lsp`, `search`, `shell`, `test`) and fails on a namespace without a row or with an `inventory_ref` absent from docs/17.

## Limitations

- The justification is prose reviewed by people; the lint enforces its presence and its anchor in docs/17, not its merit.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0230_every_tool_namespace_carries_a_build_or_buy_justification`
- `qual_ev_0217_compatibility_matrix_covers_every_registered_tool_with_owner_effect_and_test`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
