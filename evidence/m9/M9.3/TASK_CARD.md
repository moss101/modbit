# Task Card — M9.3 Full protected-path / secret redaction / broker hardening

## Identity

- Task ID: M9.3 (milestone task, docs/43)
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Substance: IMP-EV-0215 (secrets outside tool arguments) and IMP-EV-0288 (dynamic credential handles / broker injection), on the protected-path, redaction and custody work already sealed in M2.5, M2.8, M7.7 and M8.6.
- Evidence tier: real-system (both halves are proven against real substrates).

## What the milestone task asserts

A secret in this product is reachable by the code that needs it and by nothing else, and the window in which it is usable is as small as the work requires.

- **No secret is ever a tool parameter.** No registered tool's schema declares a secret-shaped field, and a call whose arguments carry a value in the Core's custody is refused `SECRET_EXFILTRATION_BLOCKED` before policy and before any effect, with the refusal recorded by shape and never by value (IMP-EV-0215, `qual_ev_0215_no_tool_schema_declares_a_secret_and_secrets_never_enter_arguments`).
- **A credential reaches a program by injection, not by possession.** A guest addresses a virtual host and the host's broker adds the header; the guest's environment, its output and the compiled policy it was built from never carry the value (M8.6, and the contract steps `egress_credentialed` / `egress_secret_never_in_guest`).
- **And the handle is short-lived.** Every grant carrying a secret has an expiry the gateway stamps and caps; at expiry the broker drops the value from its own memory and refuses; a longer task is renewed from the Core's custody rather than given a standing secret; a released sandbox stops being renewed (IMP-EV-0288, contract steps `credential_handle_is_short_lived_and_absent_from_the_policy` / `credential_handle_expires` / `credential_handle_renews`).
- **A server is handed a credential to use, not to repeat.** An external tool result that carries a custody secret back is redacted before it leaves the host and the replacement is a `SecurityEventRecorded` (M9.4/IMP-EV-0128, `qual_ev_0128_…`).
- **Protected paths hold after symlink resolution**, and the protected-effect receipt chain records every high-risk effect (M2.5/M2.8, M9.2).

## Limitations

- Provider keys and the forge token live in the Core's memory for the process's life; they are custody-checked and never journaled, but they are not themselves short-lived. The short-lived handle is what the *sandbox* gets.
- Redaction of a secret a server returns is substring matching over text; the controls that do not depend on pattern matching are the custody rule (a secret never enters arguments) and the lease rule (an unleased task never reaches a credentialed server).
- Rotating a credential's *value* is the provider's business; renewal re-sends the value the Core holds with a later expiry.

## Verification

- `crates/tools/tests/pipeline_and_direct.rs::qual_ev_0215_no_tool_schema_declares_a_secret_and_secrets_never_enter_arguments`
- `apps/sandbox-gateway/tests/gateway.rs::qual_m8_3_the_reference_backend_passes_the_backend_contract` and `::qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` (the five credential steps)
- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0128_layered_mcp_configuration_resolves_deterministically_and_a_credential_never_reaches_the_model`

## Evidence

- `evidence.json` in this directory
