# Task Card — IMP-EV-0193 Workspace-scoped MCP transport pool (M9.4)

## Identity

- Task ID: IMP-EV-0193 (part of milestone task M9.4 "external MCP gateway")
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0193 (share healthy MCP transports by normalized config fingerprint while isolating sessions/tenants); QUAL-EV-0193 (two sessions reuse transport; config/tenant change creates separate pool entry); docs/16 "MCP / external tools".
- Qualification: two sessions of the real Core against real MCP server processes.
- Evidence tier: real-system.

## Goal

Starting a server process per session would be wasteful and would multiply the surface a hostile server is given; sharing one indiscriminately would cross a tenant boundary. The pool shares exactly as far as is safe: one transport per `(tenant, workspace root, configuration fingerprint)`.

## Existing-code audit

- classification: ABSENT before this task (see IMP-EV-0104: nothing in the product spoke MCP).
- production entry points: `crates/mcp/src/config.rs` (`fingerprint` over everything that decides what process is started and how it behaves — name, command, arguments, environment, credential *handle* and scopes; `PoolKey::new(tenant, workspace, cfg)`); `services/modbit-core/src/mcp.rs` (`McpHub::entry` creating or returning the pool entry, `connection` starting the server on first use and recording the sessions it serves, and `Health::Ready { sharers, pid }` reporting how far the sharing went).

## What the pool guarantees

- Two sessions of one tenant in one workspace under one configuration get the **same** server process: same pool key, same pid, `sharers` above one, and exactly one handshake in the server's own log.
- One changed byte of configuration is a different fingerprint and therefore a different process — proven with two configurations of the same binary differing only in their arguments.
- A different workspace root is a different pool entry and a different process.
- A different tenant is a different pool entry: the tenant is the first field of the key, `external.cancel` never looks outside its own tenant's entries, and the key's isolation is covered directly by `config::tests::a_pool_key_isolates_tenants_and_workspaces`.
- A transport that has died is never handed out again: the entry drops it and the next caller gets a fresh process, and the entry's cached discovery goes with it.

## Limitations

- The pool has no idle eviction: a started server lives until the Core stops (`McpHub::shutdown` closes every child's input, which is how an MCP server is told to exit). A per-entry idle timeout would be the natural next step.
- Health is "the transport answered its handshake and has not closed"; there is no periodic `ping` probe.
- Tenant isolation is proven at the key (unit) rather than end-to-end, because one Core process serves one tenant: two tenants are two Cores, which would not exercise the pool key at all. The workspace field — the same mechanism, same code path — is proven end-to-end.

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0104_0193_a_real_mcp_server_lists_calls_and_cancels_while_two_sessions_share_one_transport`
- `crates/mcp` `config::tests::the_fingerprint_separates_what_makes_a_different_server`, `config::tests::a_pool_key_isolates_tenants_and_workspaces`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
