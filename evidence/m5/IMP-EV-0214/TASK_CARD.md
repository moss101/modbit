# Task Card — IMP-EV-0214 Disable model invocation metadata

## Identity

- Task ID: IMP-EV-0214
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0214; owner label: Skill Registry; subsystem: skills
- Qualification: `QUAL-EV-0214` — Model cannot invoke a skill marked non-model-invocable.
- Evidence tier: real-system (a signed `model_invocable: false` package with a matching trigger, on the real Core)

## Goal

A skill can be user/system-only: the model's selector never picks it up, however well its triggers match; an operator may still name it.

## Existing-code audit

- classification: NOT-FOUND before M5.5; built with M5.5's manifest (`model_invocable`, default true) and selector.
- production entry points: `crates/skills::select` skips `model_invocable: false` skills for trigger selection; explicit names still work; `services/modbit-core/src/skills.rs` passes `StartTask.skills` as the explicit list.
- proof: `qual_ev_0061_0214_…`: `system-only` (signed, trigger `notes`, `model_invocable: false`) never reaches a model request; the package test `the_selector_takes_explicit_names_and_trigger_phrases_and_refuses_with_a_reason` shows the same skill selected when named explicitly.

## Limitations

- Explicit selection is the operator's (`StartTask.skills` / CLI `--skill`); the desktop does not expose it yet.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0061_0214_a_skill_cannot_widen_task_authority_and_a_non_invocable_skill_is_not_selected`
- `the_selector_takes_explicit_names_and_trigger_phrases_and_refuses_with_a_reason`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
