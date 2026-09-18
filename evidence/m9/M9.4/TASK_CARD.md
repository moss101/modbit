# Task Card — M9.4 External MCP gateway

## Identity

- Task ID: M9.4 (milestone task, docs/43)
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Substance: IMP-EV-0104 (list / call / cancel lifecycle), IMP-EV-0193 (workspace-scoped transport pool), IMP-EV-0128 (management + scoped auth), IMP-EV-0224 (conversational configuration), IMP-EV-0187 (rich media results, moved here by DR-M5-001), IMP-EV-0281 (site-declared structured-action precedence, moved here by DR-M7-001).
- Evidence tier: real-system — every proof runs the real Core against real MCP server processes speaking the real protocol over real pipes.

## What the milestone task asserts

Modbit can use tools it did not write, from programs it does not control, without any of them gaining authority.

- **The lifecycle is real.** `external.list` / `external.call` / `external.cancel` over JSON-RPC 2.0 on a stdio transport, with the `initialize` handshake, lazy discovery, `notifications/cancelled` on the wire, and the call's `session / task / turn / call` identity carried to the server and back into the audit.
- **A server is data, not authority.** Names are validated and namespaced `external.<server>.<tool>`; descriptions are control-stripped and truncated; schemas are byte- and depth-bounded with no `$ref`; every field the host does not know — a capability claim, a system instruction, an effect class — is dropped before the host reads the tool. A hostile server's whole repertoire is refused item by item while its good tools stand.
- **What a call costs is the host's judgement.** Only the host's configuration can call a tool a read; an unknown tool, an unconfigured server, or a server claiming `readOnlyHint` is an `ExternalSideEffect` the kernel binds to an approval, and a server claiming `destructiveHint` takes the host's read declaration away.
- **Sharing goes exactly as far as is safe.** One transport per `(tenant, workspace root, configuration fingerprint)`; a changed configuration or a different workspace gets its own process; a dead transport is never handed out again.
- **Interruption is never assumed away.** A read that ends without an answer failed; an effectful call that ends without an answer is `EXTERNAL_OUTCOME_UNKNOWN`.
- **Which servers exist is configuration, and trusting one is a person's decision.** The admin/project/user layers resolve deterministically with provenance; a proposal is inert until a person trusts it and the credential gate is met; taking trust away stops the program.
- **A credential is used, never repeated.** It reaches the server through its environment from the Core's memory, and an answer that carries it back is redacted with the replacement recorded.
- **Media is media.** An image a server returns goes through the same Media Pipeline a workspace read uses and reaches a vision-capable model as the egress copy.
- **The action ladder's first rung exists.** A protected browser action prefers the site's own structured tool when the host trusts a server bound to the page's origin, and the interface is the fallback otherwise.

## Limitations

- Only the stdio transport is built. A streamable-HTTP transport — and with it a server a site could host itself — is not.
- `tools/list` pagination is bounded and `notifications/tools/list_changed` is recorded but not acted on: discovery is per transport.
- An agent proposes a server through a person's client rather than through a tool, because docs/17 defines the namespace as `external.list / call / cancel`.
- The pool has no idle eviction; a started server lives until the Core stops or trust is taken away.
- Audio results are normalized but no routed model takes audio input.

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0104_0193_a_real_mcp_server_lists_calls_and_cancels_while_two_sessions_share_one_transport`
- `::qual_ev_0128_layered_mcp_configuration_resolves_deterministically_and_a_credential_never_reaches_the_model`
- `::qual_ev_0224_a_proposed_external_server_is_inert_until_the_host_trusts_it_and_holds_its_credential`
- `::qual_ev_0187_an_mcp_image_result_reaches_a_vision_capable_model_through_the_media_pipeline`
- `::qual_ev_0281_a_site_tool_is_preferred_for_a_protected_action_when_trust_allows_and_the_page_is_the_fallback`
- the `crates/mcp` unit suites (framing, bounds, discovery refusals, typed parts, cancellation rules, fingerprint and pool key, read declarations)

## Evidence

- `evidence.json` in this directory
