# Task Card — IMP-EV-0088 Semantic UI risk classification

## Identity

- Task ID: IMP-EV-0088
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0088; disposition ADOPT
- Mandatory behavior: Risk = target × action × data × context, with credentials/security/unknown targets elevated.
- Qualification: `QUAL-EV-0088` — Destructive/credential UI action requests approval even if click tool normally allowed.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Risk = target × action × data × context, with credentials/security/unknown targets elevated.

## Existing-code audit

- classification: PARTIAL before: `classify_action` (M7.4) elevated submissions and consequential names; a credential fill was page-only.
- production entry points: `crates/browser/src/compiler.rs` (`classify_action`: target × action × data — `fill_credential` is protected whatever the field; a form button, a protected word, an unlabeled visual region are protected; an unknown target is judged page-only and refused `ACTION_UNSAFE` at run time), `crates/tools/src/browser.rs` (`Tool::effect_of` per call → the kernel's approval).
- proof: `qual_m7_8_…`: the credential fill opens an approval (`ExternalSideEffect`) before the host is asked; the refused fills open none. `qual_m7_4_…`: the submission asks approval; a field does not. `qual_ev_0089_…`: a protected action on a reference judged page-only is `ACTION_UNSAFE`.

## Limitations

- Risk words are English; the context dimension is the landmark path (a form) and the data dimension is the credential handle — no per-site policy yet.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_8_a_credential_is_filled_by_handle_into_its_bound_origin_only_and_the_value_never_crosses`
- `qual_m7_4_actions_run_by_reference_under_the_effect_they_carry_and_check_their_postconditions`
- `submissions_and_consequential_actions_are_protected_fields_and_tabs_are_not`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
