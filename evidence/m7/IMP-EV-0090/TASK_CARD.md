# Task Card — IMP-EV-0090 Safe typing + clipboard guard

## Identity

- Task ID: IMP-EV-0090
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0090; disposition ADOPT
- Mandatory behavior: Prefer reversible entry; preserve/restore clipboard and verify destination before replacement.
- Qualification: `QUAL-EV-0090` — Clipboard secret is restored and never enters model/evidence body.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Prefer reversible entry; preserve/restore clipboard and verify destination before replacement.

## Existing-code audit

- classification: IMPLEMENTED by construction since M7.4 (text enters through `Input.insertText`, never the clipboard); the destination check is new.
- production entry points: `apps/desktop/src/main/browser.ts` (`fill` / `fill_credential`: the destination is verified editable before the value is replaced — `TARGET_NOT_EDITABLE` otherwise; text is inserted as typed input; the previous value is reported by the delta — reversible), no clipboard API is used anywhere in the host.
- proof: `browser.spec.ts` faults test (real app): a secret placed in the OS clipboard before the agent types is intact after the fills and appears in no model text; a div that takes no text is refused before replacement; a real field takes the typed value with its postcondition.

## Limitations

- The clipboard assertion runs only where the runner offers a clipboard.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `apps/desktop/e2e/browser.spec.ts`
- `qual_ev_0089_every_failure_code_of_the_taxonomy_is_emitted_with_its_recovery`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
