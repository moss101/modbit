# PX-105 task card — Skills a user can use

Classification at adoption: DOCUMENTED-ONLY. Audit before this seal: PRODUCTION-WORKING. The first missing link named by BLD-07 (trust command and index segment) was closed by earlier waves; this card records the audit and the proof, and no new product code was needed.

## Acceptance clause → proof
| Clause | Where it is proven |
|---|---|
| Untrusted skill listed but absent from the request; `skill.load` refuses it | `qual_px_105_052_trust_by_hash_index_and_skill_load_reach_a_model_only_when_the_owner_says` (real Core, scripted provider capturing the request body) |
| `skill trust name@hash` adds the index line and `skill.load` returns the body | same test, and the CLI test `skill_trust_by_hash_from_the_cli_gates_what_reaches_the_model_and_one_byte_revokes_it` (real `modbit` CLI against a real Core process) |
| One byte edited makes it untrusted again | the CLI test above; mutation-check.txt shows removing the hash binding fails it |
| Thirty skills exceed the budget: ordered degradation with a count, under the byte cap | `qual_px_105_the_index_degrades_under_its_budget_and_a_path_gated_skill_waits_for_its_files` |
| Skill with `paths` appears only after a matching file is active | same test; `a_skill_with_paths_is_active_only_for_a_matching_path` in crates/skills |
| Imported skill quarantined until trusted | `qual_px_105_an_imported_skill_stays_quarantined_until_the_owner_trusts_its_hash` |
| System prohibition overrides user trust | `qual_px_105_a_system_skill_is_not_shadowed_and_a_system_prohibition_overrides_user_trust` |
| Hostile body with fake system tags is labelled and cannot widen tool access | `load_reads_a_bounded_labelled_slice_and_a_hostile_body_cannot_close_the_fence` (crates/skills) and the tool-name assertions in the CLI test |

Production wiring: crates/skills (registry, trust, index, load), services/modbit-core/src/skills.rs, apps/cli skill verbs, the skill.load tool in crates/tools.
