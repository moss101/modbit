# Task Card — PX-183 Ranked skill selection beyond exact trigger phrases, inside the existing budget and trust rules

## Identity

- Task ID: PX-183 (REQ-PX-183; owner skills); qualification QUAL-PX-183; prerequisite DOC-PX-017 (row registered on PR #148's branch)
- Evidence tier: release-critical (what instructions reach the model; trust rules unchanged)

## Existing-code audit (main 5b17f473)

- Classification: IMPLEMENTED-PARTIAL. `modbit_skills::select` injects a skill's body only for an explicit name or an exact trigger phrase in the goal; every other enabled, trusted skill is offered through the PX-105 index (name and description under the 2,000-token budget, read on demand with `skill.load`), ordered by scope and name only. Under a tight budget the index drops from the end of that order, so the skill a goal is about could be dropped while unrelated ones stayed.
- Owners kept: `crates/skills` (the one selection and index code), `services/modbit-core/src/skills.rs`. Trust (`model_loadable`), the budget and `skill.load` are unchanged; the ranking never selects, trusts or enables.

## Built

- `crates/skills/src/lib.rs`: `relevance(goal, manifest)` — distinct goal words (lower-case, three or more letters, function words removed, a plural or verb ending stripped) shared with the trigger phrases (×5), the name (×3) and the description (×1); deterministic; unit test.
- `services/modbit-core/src/skills.rs`: the index is ordered by relevance to the task's goal, ties in the old precedence order (a goal no skill matches gives the old index); `Plan.scores`.
- `crates/domain` `SkillIndexRecorded.scores` (`name:score` in index order, serde default).

## Proof (local, macOS arm64)

- `services/modbit-core/tests/surface_protocol.rs::qual_px_183_the_skill_index_is_ranked_by_the_goal_within_its_budget` (real Core, real skill files, owner trust by hash, a 260-token budget, 40 unrelated skills): a goal with no trigger phrase lists `zz-lintfix-px183` first while name order would have dropped it; skills were dropped for budget and the index stayed within it; a better-matching untrusted skill is never listed; the explicitly named skill's body is injected; two tasks with the same goal record the same index hash and scores.
- Existing skill tests re-run (surface_protocol `skill`, slash_inventory, the Core's skills unit tests, CLI skill tests); clippy.
- Mutation check: `mutation-check.txt`.

## Limitations

- Lexical ranking only; the optional semantic match of the row is not built (the embedder sits behind dependency admission, PX-112). Graph status cannot move until PR #148 (which registers PX-183) lands on main.
