---
id: DR-M9-001
title: Add `crates/mcp` and `tools/mcp-testserver` to the module layout (the External Tool Hub of M9.4)
status: accepted
date: 2026-09-18
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks"; a module addition under the locked-path rule of DR-M0-002, recorded here because `docs/12_REPOSITORY_AND_MODULE_LAYOUT.md` is locked
---

# DR-M9-001 — `crates/mcp` and `tools/mcp-testserver` in the module layout

## Trigger / evidence

`M9.4` ("external MCP gateway") builds the External Tool Hub that
`docs/17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md` has listed as the
owner of `external.list` / `external.call` / `external.cancel` since the
dossier was written, and that `docs/16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md`
specifies under "MCP / external tools". Two facts force a module decision:

- The MCP protocol itself — framing, the handshake, the bounded parse of an
  untrusted server's declaration and results, the configuration fingerprint,
  the in-flight call table — is pure and belongs beside the other pure
  protocol crates, not inside `crates/tools` (which would make the tool
  registry the owner of a wire protocol) and not inside
  `services/modbit-core` (which would make it untestable without a Core).
- `docs/56_TOOL_CAPABILITY_CONFORMANCE.md` requires a **real MCP test
  server** for the MCP suite. A real counterparty is a program, so it is a
  binary in the tree; `docs/82` forbids proving the hub against a stand-in
  inside the hub.

`docs/12` is a locked path (`tools/architecture-lint/rules.toml`
`[[locked]]`), so naming these two modules in it needs this record.

## Current behavior

`docs/12` lists the crates of the workspace and the members of `tools/`.
Neither the MCP protocol crate nor the MCP test server appears, because
neither existed: nothing in the product spoke MCP.

## Proposed replacement

1. `crates/mcp/` is added to the crate list of `docs/12` as
   "external tool (MCP) protocol, bounded untrusted discovery, pool keys".
   It is a **pure** crate: `modbit-domain`, `serde`, `serde_json`, `sha2`,
   `hex`, `base64`, `jsonschema`. It performs no I/O and starts no process.
2. `tools/mcp-testserver/` is added to the `tools/` list as
   "a conformant MCP server over stdio: the real counterparty of the
   external-tool suites". It is test substrate beside
   `tools/protocol-fixtures` and `tools/guest-image`, not product.
3. Ownership: both register `owner = "external-tools"` (the "MCP Hub,
   Integrations & Web Gateway" subsystem of `graph/project-graph.json`) and
   `canonical = []` — the hub is an **adapter behind** the doc 81 tool
   registry boundary, not a new canonical single-owner system, so the
   canonical list of `docs/81` is unchanged.
4. The host half — processes, pipes, the pool, credential custody — stays
   in `services/modbit-core/src/mcp.rs`, and the three tools stay in
   `crates/tools/src/external.rs`, both inside modules `docs/12` already
   names.

## Migration

None. Two directories are added and named in the layout; no existing module
moves, changes owner or changes its dependency direction. No data, schema
or wire format is touched.

## Compatibility

None affected. `crates/mcp` is new and nothing outside `crates/tools` and
`services/modbit-core` depends on it; `tools/mcp-testserver` is built by
the workspace build and used only by tests.

## Security impact

Positive and bounded. Putting the protocol in a pure crate is what makes
the untrusted-content rules of `docs/16` unit-testable in isolation: the
name validation and namespacing, the byte and depth bounds on schemas, the
dropping of fields the host does not know, the base64 validation of media
payloads, and the rule that a server's claim may only raise what a call
costs. `tools/mcp-testserver` adds a hostile mode so those rules are also
exercised against a real hostile counterparty. Neither module holds a
secret: a server's credential is a handle in configuration, resolved from
the Core's in-memory custody at spawn.

Dependency direction is unchanged and still enforced: `modbit-mcp` depends
only on `modbit-domain` and leaf libraries, and `architecture-lint` passes
its `[[forbid]]` and `[[confine]]` rules with the new member present.

## Test impact

`crates/mcp` carries its own unit suites (framing and bounds, discovery
refusals, typed result parts, cancellation and unknown-outcome rules,
fingerprint and pool key, read declarations). The real-system proof is
`qual_ev_0104_0193_a_real_mcp_server_lists_calls_and_cancels_while_two_sessions_share_one_transport`
(QUAL-EV-0104, QUAL-EV-0193), which runs the real Core against real
`modbit-mcp-testserver` processes.

## Rollback

Remove the two directories from `docs/12`, the two members from
`Cargo.toml`, and with them the gateway: `external.list` / `external.call`
/ `external.cancel` would return to being an unbuilt inventory row and
IMP-EV-0104, IMP-EV-0193, IMP-EV-0128, IMP-EV-0224, IMP-EV-0187 and
IMP-EV-0281 would return to NOT_STARTED.
