# Task Card — M5.5 Skill manifest / selector / compiler, provenance and signing

## Identity

- Task ID: M5.5
- Milestone: M5 Procedural runtime and skills (P0)
- Requirements: docs/16 "Skills" (packs with manifest, compatibility range, instructions, procedure templates, eval metadata and provenance; explicit or selector-driven selection; compiled into minimal instructions + tool projection with large content by reference; lifecycle `incubator → evaluated → signed → enabled`; a skill cannot request capabilities beyond task/user policy), docs/26 (portable `SKILL.md`-style package, exact registry/discovery validation, `model-invocable: false`, compact loading; promotion only by qualification, never by the skill's own say-so), docs/23 (skills are signed/versioned), REQ-EV-0209 (SKILL.md-based packaging), REQ-EV-0061 (a skill's capability ceiling only narrows), REQ-EV-0105 (the instruction manifest records skill source/version/reason), docs/36 supply-chain discipline for signatures.
- Qualification: the skill half of docs/43 M5 ("skill provenance"): a signed project skill selected by trigger and injected with its identity on the log; an unsigned one not injected; an explicitly named unsigned one rejected with the reason; a tampered signed package refused.
- Evidence tier: real-system (real packages on disk, real Ed25519 signatures under trusted keys handed to the real Core, the real prompt compiler and log)

## Goal

Make skills first-class, governed inputs: a portable package with a content identity, a lifecycle the operator's trust and policy decide, a selector that is explicit or trigger-driven, a compiler that injects bounded instructions and intersects the skill's tools with the task's policy surface — never widening it — and a log record of every selection and rejection.

## Existing-code audit

- classification: NOT-FOUND before this task. `crates/skills` was the M0.1 stub; no package format, registry, selector, compiler or signature check existed; the prompt compiler had a workspace-rules segment the runtime left empty.
- production entry points:
  - `crates/skills/src/lib.rs` — `SkillManifest` (+ `Compatibility`, `Provenance`, `EvalMetadata`), `parse_skill_md` (front-matter `key: value`, `key: [a, b]`, `outer.inner: value`), `load_package` (content hash over every file but the attestations; `procedures/*.js`; `resources/*` by reference), `Lifecycle`, `Evaluation` (`EVALUATION.json`), `Attestation` / `SignedSkill` (`SIGNATURE.json`), `verify` (trusted key, Ed25519 strict verify, content/name/version match), `sign` (tooling/tests), `trusted_keys_from_env`, `SkillPolicy`, `SkillRegistry::discover` (roots in order, first name wins, each package's lifecycle decided with its note), `select` (explicit names then trigger phrases; rejections typed), `compile` → `CompiledSkill` (bounded instructions with the cut declared, `tool_projection` = required ∩ given, `tools_unavailable`, procedures and resources by reference, `instructions_hash`).
  - `services/modbit-core/src/skills.rs` — `roots` (`<workspace>/.modbit/skills`, `<profile>/skills`), `policy` (`MODBIT_SKILLS_ENABLE_INCUBATOR`), `select_for_run` (keys from `MODBIT_SKILL_KEYS`; compile against the task's policy surface; `SkillSelected` / `SkillRejected` on the task; the instructions returned for the prompt).
  - `services/modbit-core/src/runtime.rs` — skills selected once per run before the loop, injected as `PromptInput.skills`; `StartConfig.skills`.
  - `crates/prompt-compiler/src/lib.rs` — `PromptInput.skills` rendered inside the rules segment ("Skills selected for this task (…they grant nothing)"), so the segment hash and the cache key carry them.
  - `crates/protocol/proto/modbit/v1/surface.proto` — `StartTask.skills`; `apps/cli` — `task run --skill <name>...`; `crates/domain/src/task.rs` — `SkillSelected`, `SkillRejected`.
- proof: (real Core) three project skills under `.modbit/skills` — `house-style` signed by a key the Core trusts, `draft-style` unsigned, `tampered-style` signed then changed — and a task whose goal says "NOTES": every model request's system messages carry `# Skill: house-style v1.0.0`, its instructions, the note that `net.fetch` is not offered by the task's policy and the procedure template `retitle`, and nothing of the other two; `SkillSelected` records name, version, the package's content hash, `ENABLED`, `TRIGGER:notes`, the source path, `tool_projection` `[change.apply, fs.read, git.worktree.close, git.worktree.create]`, `tools_unavailable` `[net.fetch]`; `StartTask.skills = [draft-style, no-such-skill]` yields `SkillRejected` `NOT_ENABLED` (reason: Incubator, no signature) and `UNKNOWN`. (Package tests) manifest parsing and refusals; the content identity excludes attestations and changes with any file; lifecycle across incubator / evaluated / signed / tampered / foreign-key packages under the default and a development policy; `verify` refusals (unknown key, bad signature, other content); key parsing from the environment; the selector's explicit, trigger, not-enabled, unknown and `model_invocable: false` cases; the compiler's intersection, toolset expansion, budget cut and hash.

## Limitations

- The selector is explicit names plus trigger phrases; a context-driven selector (relevance from retrieval) is not built.
- `EVALUATION.json` is read, not produced: the qualification harness that writes it is the Skill Evolution Lab (M5.7 / EXPERIMENT).
- `capability_ceiling` is carried in the manifest and recorded; enforcement is the kernel's, through the projection the skill cannot widen.
- Skills are selected once per run against the policy surface; a skill discovered or signed mid-run applies to the next run.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m5_5_signed_skills_are_selected_compiled_and_recorded_and_unsigned_ones_are_not` (services/modbit-core, real Core)
- `crates/skills/tests/packages.rs`: `a_package_parses_its_manifest_and_has_a_content_identity`, `the_lifecycle_follows_attestations_and_policy_and_a_tampered_package_is_not_signed`, `the_selector_takes_explicit_names_and_trigger_phrases_and_refuses_with_a_reason`, `the_compiler_injects_bounded_instructions_and_never_widens_the_projection`
- Regression: the prompt-compiler unit tests (segment hashes), the protocol round trip (`StartTask.skills`)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
