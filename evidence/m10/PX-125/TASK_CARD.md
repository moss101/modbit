# Task Card — PX-125 GitHub pull request read, diff and status comments through the forge adapter: the live leg

## Identity

- Task ID: PX-125 (REQ-PX-125; owner external-tools); qualification QUAL-PX-125 / PX-E2E-125; DR-PX-2026-09-05, DR-PX-2026-10-05-013; DR-M6-002 (the real-GitHub level deferred until the owner supplies a token and a test repository).
- Evidence tier: release-critical (an external effect through the Capability Kernel, approvals and the credential broker).
- Prerequisites: PX-006, PX-007, M9.4, DOC-PX-013 (all COMPLETE).
- Branch `wip/px-125-live`, from `main` at 93307f67. The row was WIRED on a wire-faithful GitHub fake since 2026-10-17; its closing proof, on real GitHub, was open.

## What the owner supplied (2026-11-20/21)

- A private scratch repository `moss101/modbit-forge-test` (README and a one-job workflow), created by `gh repo create` on the owner's instruction. It holds no project data.
- A fine-grained personal access token named `modbit-forge-test`, created in GitHub's settings in the in-app browser with the owner signed in, limited to that one repository: Contents, Pull requests, Issues and Actions read and write, Metadata read; expires 2026-12-20. It is stored at `~/.config/modbit/forge-test-token` (mode 0600, outside the repository) and reaches the test through `MODBIT_GITHUB_TOKEN` only. It was checked before use: the test repository answers 200 and another private repository of the same account answers 404.

## Existing-code audit

- `forge.pr.read`, `forge.pr.diff`, `forge.pr.comment`, `forge.issue.comment` (`crates/tools/src/forge.rs`), the broker-held token (`services/modbit-core/src/forge.rs`) and the approval path: PRODUCTION-WORKING on the wire-faithful fake (`px_125_forge.rs`), WIRED.
- A run against real GitHub: NOT-FOUND. First missing link: no test could be given a real repository and token, and a missing variable must fail such a test, never skip it.

## What was built

- `services/modbit-core/tests/px_125_live.rs`: one `#[ignore]` test, `qual_px_125_live_the_forge_adapter_reads_a_real_pull_request_and_its_diff_and_posts_approved_redacted_comments`. It fails (it does not skip) when `MODBIT_GITHUB_TOKEN` or `MODBIT_GITHUB_TEST_REPO` is missing. It builds its own fixture on the real repository (a branch with three files, one carrying a planted instruction and one large enough to page, and a pull request whose body carries a planted instruction), drives the real `modbit-core` binary over its real socket, and closes the pull request, deletes its branch and deletes every comment it made even when an assertion fails. GitHub is reached from the test through `gh api` with the token in `GH_TOKEN` only (never an argument, never printed) and a throwaway `GH_CONFIG_DIR`. Run with:

  ```text
  MODBIT_GITHUB_TOKEN=... MODBIT_GITHUB_TEST_REPO=owner/name cargo test -p modbit-core --test px_125_live -- --ignored
  ```

- No product code changed.

## Verification

macOS 27.0.1 arm64, rustc 1.97.1, the real `modbit-core` binary, real GitHub (api.github.com).

- `px125-live.log`: 1 passed in 24 s. On a fresh real pull request: `forge.pr.read` returns it labelled `forge_pr` and `UNTRUSTED_EXTERNAL_CONTENT`, number, state, author and head branch equal to what the test made, and the planted instruction in its body is marked (`OVERRIDE_INSTRUCTIONS`, `EXFILTRATE_SECRET`); `forge.pr.diff` returns the three files, a truncated bounded preview, a diff of more than 30 KB that pages by range (`artifact.range`, at least three pages of at most 16 KiB) to exactly its byte length with the last line present, the planted instruction in the diff marked before it is paged, and include/exclude filters; a comment is `APPROVAL_PENDING` with nothing on GitHub, and after approval exactly one comment with the exact approved body is on the real pull request with one receipt; the same idempotency key again is answered from the record and sends nothing; a body carrying a planted credential-shaped string reaches GitHub with `[redacted]` in its place (read back from GitHub); the Core's own token in a body is `POLICY_DENIED / SECRET_EXFILTRATION_BLOCKED` and sends nothing; a credential-named field is `INVALID_ARGUMENTS`; another host is `EGRESS_DENIED`; the log holds two `ForgeCommentPosted`, a `PROMPT_INJECTION_SUSPECTED` security event and neither the planted string nor the token; a profile without the capability (`review_isolated`) is `POLICY_DENIED` and sends nothing; with the token blanked the tools answer `NO_FORGE_TOKEN` and ask nothing of GitHub. No log or event contains `github_pat_` (`grep` over the retained logs: 0).
- Negative proof, `mutation-redaction-skipped.log`: `comment_text` in `crates/tools/src/forge.rs` changed to pass the body through unredacted; the live test fails at `redactions` (0, expected 1). Source restored, then touched, and the unmutated run above made after it.
- The repository after every run: all fixture pull requests closed, no comments, only `main`.
- `cargo fmt --all -- --check`, `cargo clippy -p modbit-core --tests --locked` (0 warnings), `tools/lint_policy.sh`: exit 0.

## A finding the live run made

- Checks: GitHub's fine-grained personal access tokens have no "Checks" permission (the picker offers none; "Commit statuses" exists, which covers the combined-status endpoint, not check runs). `GET .../check-runs` with such a token is `403 Resource not accessible by personal access token`. The adapter answers `checks: { available: false, reason: FORGE_FORBIDDEN }` with no invented summary, which is right; the test accepts exactly that or, for a token that can read check runs (a GitHub App's or a classic one), the summary that says it is never a verification result. So the real check-runs read is not proven by this token. PX-127's "a real workflow run's result appears in Review" and PX-126's real CI delivery need a different credential than this token.

## Limitations

- The 403 and the primary and secondary rate limits cannot be provoked on demand against real GitHub; they stay on the wire-faithful fake (`px_125_forge.rs`), as the row's acceptance says. The fake does not close the row; this live run does the rest.
- The live test runs on this macOS machine only. CI has no repository secrets, so no workflow runs it; `tools/integrations.json` still lists `forge.github` as deferred under DR-M6-002 and `integration_gate.py --release` still fails on it. Closing that needs the repository secrets `MODBIT_GITHUB_TOKEN` and `MODBIT_GITHUB_TEST_REPO` and a `live_`-named test that the live workflow selects (tools/integration_gate.py). That is the owner's call and a separate change; it was not made here.
- The token expires on 2026-12-20; a rerun after that needs a new one.
- Every run creates and removes a branch and a pull request on the scratch repository (it shows as closed pull requests there).
- Not sealed: sealing is the owner's.

## Status

E2E_PROVEN on macOS against real GitHub; COMPLETE is for the owner.
