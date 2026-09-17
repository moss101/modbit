# Task Card — IMP-EV-0284 Prompt-injection isolation in web content

## Identity

- Task ID: IMP-EV-0284
- Milestone: M7 Live browser (P0)
- Requirement: REQ-EV-0284; disposition ADOPT
- Mandatory behavior: Web page text is untrusted data; cannot alter policy/tool authority.
- Qualification: `QUAL-EV-0284` — Seed hostile page instructions; forbidden tool remains unavailable.
- Evidence tier: real-system (the real Core with a fake host; the real app on real pages)

## Goal

Web page text is untrusted data; cannot alter policy/tool authority.

## Existing-code audit

- classification: IMPLEMENTED by M7.7.
- production entry points: `crates/browser/src/injection.rs`, `services/modbit-core/src/tools.rs` (marking, security events, credential-free broker, `SECRET_EXFILTRATION_BLOCKED`), `crates/prompt-compiler` rule 6.
- proof: `qual_m7_7_…`: hostile page and README are data; the forbidden tool stays unavailable; no secret leaves.

## Limitations

- See M7.7.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_7_hostile_page_and_readme_are_data_the_key_never_leaves_and_the_forbidden_tool_stays_unavailable`
- `apps/desktop/e2e/browser.spec.ts`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
