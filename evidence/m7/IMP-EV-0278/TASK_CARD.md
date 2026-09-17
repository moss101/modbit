# Task Card — IMP-EV-0278 Stable semantic element references

## Identity

- Task ID: IMP-EV-0278
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0278; disposition ADOPT
- Mandatory behavior: References are scoped to browser-state version and invalidate safely on relevant change.
- Qualification: `QUAL-EV-0278` — DOM mutation makes stale ref return TARGET_STALE, never click wrong element.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

References are scoped to browser-state version and invalidate safely on relevant change.

## Existing-code audit

- classification: IMPLEMENTED by M7.2.
- production entry points: `crates/browser/src/compiler.rs` (`reference_of`, `resolve` → `TARGET_STALE` with look-alikes).
- proof: `qual_m7_2_…`: a re-render keeps the reference; a renamed element's reference is `TARGET_STALE`, never another node; `a_removed_or_changed_element_resolves_stale_never_to_another_node`.

## Limitations

- Ordinals move on reorder.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_2_entities_carry_stable_references_and_a_changed_element_resolves_stale`
- `a_removed_or_changed_element_resolves_stale_never_to_another_node`
- `references_are_stable_across_versions_and_dom_node_ids`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
