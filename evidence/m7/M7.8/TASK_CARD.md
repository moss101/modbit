# Task Card — M7.8 Credential handle fill path

## Identity

- Task ID: M7.8
- Milestone: M7 Live browser (P0)
- Requirements: docs/22 "Credentials" (login data only through an explicit user-approved credential broker; the model receives field handles/status, not password values; the Browser Bridge fills a credential handle directly into a bound origin/field under policy), docs/39 step 2 / REQ-PX-022 custody rules (a secret in Electron main's safeStorage custody, never returned to the renderer, never logged), REQ-EV-0284 (content cannot request hidden secrets).
- Qualification: the fill-by-handle path end to end — a handle bound to one origin, offered on the page's snapshot, filled by the host from its own custody, refused at any other origin, logged by handle, the value nowhere in the transcript.
- Evidence tier: real-system (the real Core with a fake host; the real app with a real password field and a real POST)

## Goal

Let the agent sign in without ever holding a password: the person binds a credential to an origin once; the agent fills it by handle; the value moves from the keychain to the page and nowhere else.

## Existing-code audit

- classification: MISSING before this task: no broker, no handles, no fill path; a login needed the model to type a value it should never have.
- production entry points: `apps/desktop/src/main/credentials.ts` (`CredentialStore`: safeStorage ciphertext on disk, `secretFor(handle, pageOrigin)` bound to the origin), `apps/desktop/src/main/main.ts` (`credential:add/list/remove` IPC; handles registered with every (re)started Core), `apps/desktop/src/main/browser.ts` (`fill_credential` through CDP from the store, for the page's origin only), `apps/desktop/src/preload/preload.ts` + `renderer/index.tsx` (the Settings "Login credentials" section: handles, never values), `crates/protocol/proto/modbit/v1/surface.proto` (`RegisterBrowserCredential`, `ForgetBrowserCredential`; `browser.host`), `services/modbit-core/src/{server.rs,browser.rs}` (the in-memory handle registry; exact-origin validation), `crates/browser/src/lib.rs` (`CredentialHandle`, `origin_of`, `HostRequest::Act.credential_handle`, the port's `credential` / `credentials_for`), `crates/tools/src/browser.rs` (`credentials` on the snapshot; `fill_credential` with `CREDENTIAL_UNKNOWN` / `CREDENTIAL_TARGET_NOT_FIELD` / `CREDENTIAL_ORIGIN_MISMATCH` before the host), `crates/domain/src/task.rs` + `services/modbit-core/src/tools.rs` (`BrowserCredentialFilled`), `crates/tools/tool-matrix.json` (the `browser.act` note).
- proof: `qual_m7_8_a_credential_is_filled_by_handle_into_its_bound_origin_only_and_the_value_never_crosses` (real Core, fake host): a headless client is refused `CLIENT_CAPABILITY`; a path is refused `BAD_ORIGIN`; the snapshot offers the handle with label, account name and origin; the unknown handle, the button and the other origin are refused before the host; the one `act` the host sees for the credential names the handle and carries no value; the answer is `filled: true` with the handle; the log has one `BrowserCredentialFilled` (handle, origin, the password field's reference and name) beside its `BrowserActionPerformed`; a forgotten handle answers `existed: false` the second time. `apps/desktop/e2e/browser.spec.ts` (real app): the credential added in Settings shows as a handle bound to the site's origin (the settings text never carries the secret); the agent fills the username and the credential by handle; at the approval gate the real password field holds the real secret; after the approval the site receives `{u: ada, p: <secret>}` in the POST; the model saw the handle on the snapshot, `filled: true`, the postcondition of the sign-in — and never the value; the host's delivery log never carries it. `credentials.test.ts` (the store's rules: exact origin, wrong origin → null, memory-only when unencrypted).

## Limitations

- One secret per handle (a password or token); TOTP / second factors are not modelled.
- The binding is by origin; a site that signs in on a different origin than it serves (an SSO redirect) needs the credential bound to the sign-in origin.
- The Core's handle registry is memory-only: a Core restart without a desktop reconnect offers no handles until the desktop registers again (it does on reconnect).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_8_a_credential_is_filled_by_handle_into_its_bound_origin_only_and_the_value_never_crosses` (services/modbit-core, real Core)
- `apps/desktop/e2e/browser.spec.ts` (the credential handle fill test)
- `apps/desktop/src/main/credentials.test.ts` (the store's rules)
- `origin_is_scheme_host_and_port_only` (crates/browser)
- Regression: `qual_m7_1_…` … `qual_m7_7_…`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
