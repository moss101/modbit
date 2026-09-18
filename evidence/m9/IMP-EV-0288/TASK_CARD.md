# Task Card — IMP-EV-0288 Dynamic credential handles / broker injection (M9.3)

## Identity

- Task ID: IMP-EV-0288 (the remaining half of milestone task M9.3 "full protected-path/secret redaction/broker hardening")
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0288 (guest receives a short-lived scoped secret handle/injection, not static secrets in image/config); QUAL-EV-0288 (inspect guest image/env/artifacts; long-lived provider secret absent); docs/21 "dynamic credential handles via broker injection"; docs/23 "Secrets".
- Qualification: the backend conformance contract against the real reference backend and the real MicroVM backend — every backend passes all five credential steps or fails the contract.
- Evidence tier: real-system.

## Goal

A guest reaches a credentialed service without ever holding the credential, and the window in which the host will use that credential on its behalf is short and renewed rather than standing.

## Existing-code audit

- classification: PARTIAL before this task. M8.6 built the injection itself: the guest addresses a virtual host over plain HTTP through its own proxy, the broker forwards over TLS with the secret it holds under the handle, and the backend contract already proved the guest never held it (`egress_credentialed`, `egress_secret_never_in_guest`) and that the sandbox's own view lists virtual hosts, never values. What REQ-EV-0288 asks for beyond that is **short-lived**: a grant had no lifetime, a secret handed to the gateway lived as long as the sandbox, and there was no way to hand a live sandbox a fresh one.
- production entry points: `crates/sandbox/src/policy.rs` (`CredentialGrant::expires_at_ms`, `live_at`, `remaining_ms`); `crates/sandbox/src/egress.rs` (the broker's policy and secrets behind a lock, `CredentialLookup`, the expiry that drops the secret, `renew`); `apps/sandbox-gateway/src/routes.rs` (every grant stamped and capped at `MODBIT_SANDBOX_CREDENTIAL_TTL_MS`, the `POST /v1/sandboxes/{id}/credentials` renewal route); `crates/sandbox/src/client.rs` (`renew_credential`); `services/modbit-core/src/sandboxes.rs` (`CREDENTIAL_TTL_MS` and the renewal task that runs while the sandbox is live and stops when it is released).

## What is enforced

- **Short-lived by construction.** A grant carrying a secret cannot exist without an expiry: the gateway stamps one when the provisioner does not and caps any the provisioner asks for. The Core asks for ten minutes against a fifteen-minute ceiling.
- **Expiry drops the secret.** At the moment a grant's lifetime passes, the broker removes the value from its own memory and refuses the injection with the reason on the audit. The secret stops existing, not merely stops being used.
- **Renewal, not standing secrets.** A live sandbox is handed a fresh value and lifetime for a handle it *already* grants — never a new handle, so renewal cannot widen what the sandbox may reach. The Core renews every half-lifetime from the token in its own custody and stops when the sandbox is released, so a leaked sandbox loses its credential rather than keeping one.
- **Nothing to find in the image or the artifacts.** The compiled policy the backend provisions from names the virtual host and the handle and holds no value; the guest's environment carries the proxy and nothing else. Both are contract steps, not assertions in one test.

## Limitations

- The renewal is the Core's: a gateway whose worker's Core has stopped will let the handle expire, which is the intended failure direction but means a paused task loses its credential until its Core returns.
- The expiry is wall-clock on the broker's host; a clock moved backwards lengthens a window (a clock moved forwards shortens it).
- Renewal re-sends the same value with a later expiry. Rotating the *value* (a fresh token from the forge each time) is the provider's business and is not built.
- The HTTP renewal route is exercised through the client and the broker; the route's own authorization path is covered by the gateway's tenancy suite, which needs Postgres and runs in the hosted cloud job.

## Verification

- `apps/sandbox-gateway/tests/gateway.rs::qual_m8_3_the_reference_backend_passes_the_backend_contract` and `::qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` — both assert the contract ran `credential_handle_is_short_lived_and_absent_from_the_policy`, `credential_handle_expires` and `credential_handle_renews` beside M8.6's two steps, and that every step passed.
- `crates/sandbox` `policy::tests` (the grant's lifetime arithmetic).

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
