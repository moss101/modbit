# Task Card — IMP-EV-0128 MCP management + scoped auth (M9.4)

## Identity

- Task ID: IMP-EV-0128 (part of milestone task M9.4 "external MCP gateway")
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0128 (scopes, credential broker, health, lazy discovery and audit); QUAL-EV-0128 (user/project MCP conflict resolves deterministically; credentials never enter the model prompt); docs/23 "External tool servers as built"; docs/16 "MCP / external tools".
- Qualification: the real Core with three real configuration layers on disk and real MCP server processes.
- Evidence tier: real-system.

## Goal

Decide which external servers a task may see, and what it may reach through them, from configuration the *host* owns — and make a credential usable by a server without ever letting it, or anything derived from it, reach the model.

## Existing-code audit

- classification: PARTIAL before this task. The typed `ConfigurationResolver` (IMP-EV-0039, COMPLETE) already implemented the merge laws for `mcp_servers` / `mcp_deny` with provenance and refused widenings — but **nothing in the product ever built a `ResolvedConfig`**: the Core's kernel port passed `config: None` and the hub took its servers from `MODBIT_MCP_SERVERS` at boot. Credential custody, lazy discovery and a health enum arrived with IMP-EV-0104. Scoped auth against the task's lease, the liveness check on a reused transport, result redaction and the declaration audit did not exist.
- production entry points added: `services/modbit-core/src/config.rs` (the three layers, read from `MODBIT_ADMIN_CONFIG` or `<data-dir>/admin-config.json`, `<workspace>/.modbit/config.json` and `<data-dir>/config.json`, resolved and **pinned per task**, released when the task ends); `services/modbit-core/src/mcp.rs` (`servers_from` / `refused_servers` over the resolved configuration, `TaskScope`, the lease check, the liveness ping, redaction); `crates/mcp/src/config.rs` (`requires`, `needed_capabilities`, `missing_capabilities`); `crates/mcp/src/port.rs` (`Health::Unleased`, `requires`, `provenance`, `refused_servers`); `crates/policy/src/config.rs` (a shadowed MCP definition is recorded in the provenance rather than dropped in silence); `services/modbit-core/src/tools.rs` (the security events for a refused declaration and for a redacted answer).

## What is enforced

- **Deterministic conflict.** A name two layers define is decided by the higher one; the lower layer's definition is in the provenance. A higher layer's deny wins over any lower layer's addition, and a lower layer cannot remove a higher layer's server — both refusals are reported by `external.list` under `refused_servers`.
- **Scoped auth.** A server declares `requires` (ordinary capability ids), and a named credential adds `secret.use` on its own. They are checked against the task's capability lease *before the server is started*: unleased is `UNLEASED` in the listing with what is missing, and `EXTERNAL_CAPABILITY_NOT_LEASED` on a call. A server is never a way around the task's own lease.
- **Credential broker.** The value lives only in the Core's memory (`MODBIT_MCP_CREDENTIAL_<HANDLE>` at boot), reaches the child through its environment, and is in the Core's custody set — so an argument carrying it is refused before policy (M9.3).
- **Never in the model's context.** Every answer is scanned for the custody secrets; one that comes back is replaced, counted on the result, and recorded as `SecurityEventRecorded` / `SECRET_IN_EXTERNAL_RESULT`.
- **Health.** A pooled transport is handed out only while it still answers: reuse pings it under a short deadline and a silent server is replaced rather than reported ready.
- **Audit.** Declarations the host refused are recorded on the task as `EXTERNAL_DECLARATION_REFUSED`, and the configuration's provenance is on every listing.

## Limitations

- Configuration is read from files; there is no command to write one. Proposing and trusting a server conversationally is IMP-EV-0224.
- The resolved configuration is not yet passed to the Capability Kernel's own `config` slot (per-capability `ALLOW`/`ASK`/`DENY` from a layer), which is the rest of REQ-EV-0039's consumer side; the MCP half is what REQ-EV-0128 asks for and is what is wired.
- Redaction matches secrets of 8 bytes or more as substrings of text and of embedded resource text; a server that encodes a credential (base64, split across parts) is not caught by it — the custody rule that a secret never enters arguments, and the lease rule that an unleased task never reaches a credentialed server, are the controls that do not depend on pattern matching.
- The liveness ping costs one round trip per server per `external.list`.

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0128_layered_mcp_configuration_resolves_deterministically_and_a_credential_never_reaches_the_model`
- `services/modbit-core` `config::tests::the_three_layers_are_read_from_their_places_and_a_task_pins_the_answer`, `config::tests::a_missing_or_broken_layer_is_no_opinion_and_never_an_error`
- `crates/policy` `config::tests::a_shadowed_mcp_definition_is_recorded_not_silently_dropped`
- `crates/mcp` `config::tests::a_server_needs_what_its_configuration_requires_and_a_credential_needs_secret_use`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
