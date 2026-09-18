# Task Card — IMP-EV-0104 MCP list / call / cancel lifecycle (M9.4)

## Identity

- Task ID: IMP-EV-0104 (part of milestone task M9.4 "external MCP gateway")
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0104 (external tools support discovery, call, cancellation with task/turn/call identity); QUAL-EV-0104 (real MCP test server supports list/call/cancel and audit correlation); docs/16 "MCP / external tools"; docs/17 `external.list` / `external.call` / `external.cancel`; docs/11 boundary 4; docs/52 "Malicious skill/plugin/MCP"; docs/54 fault 25; docs/56 "MCP | real MCP test server".
- Qualification: the real `modbit-core` binary against real MCP server processes speaking the real protocol over real pipes.
- Evidence tier: real-system.

## Goal

An agent can discover, invoke and cancel the tools of an external MCP server through the same registry, capability kernel and pipeline as every native tool — with the server treated as an untrusted program throughout, and with the call's `session_id / task_id / turn_id / call_id` identity carried to the server and back into the audit.

## Existing-code audit

- classification: ABSENT before this task. Nothing in the product spoke MCP: `crates/tools/tool-matrix.json` recorded no MCP family, `crates/policy/src/config.rs` carried `mcp_servers` / `mcp_deny` in the resolved-configuration layers with nothing reading them, and docs/17 listed `external.list / call / cancel` as the hub's unbuilt surface. DR-M5-001 and DR-M7-001 had already moved IMP-EV-0187 and IMP-EV-0281 into M9 for want of this gateway.
- production entry points added: `crates/mcp` (the protocol: JSON-RPC 2.0 framing over newline-delimited stdio, the `initialize` handshake and version negotiation, the bounded untrusted parse of `tools/list` and of a `tools/call` result's content blocks, the configuration fingerprint and pool key, the in-flight call table); `services/modbit-core/src/mcp.rs` (the External Tool Hub: child processes and their pipes, lazy discovery, the pool, credential custody, cancellation guards); `crates/tools/src/external.rs` (`external.list`, `external.call`, `external.cancel`); `crates/policy/src/kernel.rs` (the `external.list` / `external.call` lease operations and the `review_isolated` denial of `external.call`); `services/modbit-core/src/tools.rs` (the hub wired into `InvokeContext`, its credentials joined to the Core's custody set, and `ToolCallProposed` now recording the class the call is judged under rather than the tool's registered floor).
- test substrate added: `tools/mcp-testserver`, a conformant MCP server over stdio (initialize / initialized / tools/list / tools/call / notifications/cancelled / ping) whose behavior is set by environment: an audit log of every message it receives, a real external effect it appends to a file, a slow tool that honors cancellation, a tool that exits mid-call, and a hostile mode that declares everything a hostile server would.

## What the lifecycle is

- **list** — `external.list` starts a trusted server the first time anything needs it (lazy discovery) and returns every server with its tools, health, scopes, configuration layer and pool key. Every name is validated and namespaced `external.<server>.<tool>`; every description is control-stripped and truncated; every schema must be a JSON-Schema object inside a byte and depth bound with no `$ref`; every field the host does not know is dropped. A bad tool is refused by name with a reason and the rest of the server's list stands. A server that is only *proposed* is never started.
- **call** — `external.call` validates the arguments against the server's own declared schema, sends `tools/call` with the correlation `_meta`, and normalizes the result into typed text / image / audio / resource parts under the host's bounds. A server reporting `isError` is an application failure of the external tool, reported as such.
- **cancel** — every request is registered before it is sent and holds a guard; a cancellation, a timeout, a dropped task or a dead server all send `notifications/cancelled` naming the request id. A read that ends without an answer failed; an effectful call that ends without an answer is `EXTERNAL_OUTCOME_UNKNOWN`.
- **identity** — `session_id`, `task_id`, `turn_id` and `call_id` travel with every call under the namespaced `_meta` key `modbit.dev/correlation`, so the server's own record of what it was asked lines up with the Core's tool-call log.

## Limitations

- Only the stdio transport is built. A streamable-HTTP transport (and with it a site-declared server, IMP-EV-0281) is not.
- Scoped auth beyond a single broker-held credential per server, and the conversational configuration path, are IMP-EV-0128 and IMP-EV-0224; a server's configuration reaches the hub from `MODBIT_MCP_SERVERS` at boot for now, and the `mcp_servers` / `mcp_deny` configuration layers are not yet read by the hub.
- Rich media results reach the tool layer as typed parts with their byte lengths; routing them through the Media Pipeline to a vision-capable model is IMP-EV-0187.
- `tools/list` pagination is followed to a bounded number of pages; `notifications/tools/list_changed` is recorded in the handshake but not acted on (discovery is per transport).

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0104_0193_a_real_mcp_server_lists_calls_and_cancels_while_two_sessions_share_one_transport`
- `crates/mcp` unit suites: `protocol` (framing, bounds, handshake), `discovery` (namespacing, refusals, bounds, dropped fields, argument validation), `result` (typed parts, base64, bounds), `calls` (cancellation and unknown-outcome rules), `config` (names, secret-shaped environment, fingerprint, pool key, read declarations)
- `crates/tools/tests/pipeline_and_direct.rs::qual_ev_0217_...` and `::qual_ev_0230_...` (the family's matrix rows and the namespace justification), `::qual_ev_0215_...` (no `external.*` schema declares a secret)

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
