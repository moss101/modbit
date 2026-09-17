---
id: DR-M7-001
title: Reschedule IMP-EV-0281 to M9 (site-declared structured tools are MCP servers a site declares; the gateway that lists and calls them is M9.4)
status: accepted
date: 2026-09-17
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks"; scheduling-only change under the MILESTONE_OVERRIDES precedent (DR-M0-005, DR-M2-002, DR-M3-004, DR-M5-001)
---

# DR-M7-001 — Reschedule IMP-EV-0281 to M9

## Trigger / evidence

`IMP-EV-0281` (REQ-EV-0281, "prefer authenticated site-declared structured
tool when available, then derived semantic action, primitive, vision";
QUAL-EV-0281, "same task selects native tool when trust/policy allow;
fallback works otherwise"; owner label "MCP/Browser Gateway", disposition
ADAPT) is scheduled in M7 by its owner label. Its first rung — the
site-declared structured tool — is an external tool a site advertises and
the agent calls under trust and policy, and nothing in the product speaks
to external tool servers:

- `M9.4` ("external MCP gateway"), `IMP-EV-0104` (MCP list / call / cancel
  lifecycle), `IMP-EV-0128` (MCP management and scoped auth) and
  `IMP-EV-0193` (workspace-scoped MCP transport pool) are all scheduled in
  **M9**; the External Tool Hub that discovers, namespaces
  (`external.<server>.*`), size-bounds and distrusts a declared tool's
  schema (docs/16 "MCP / external tools") arrives with them, and DR-M5-001
  moved IMP-EV-0187 there for the same reason.
- The other three rungs of the hierarchy are built and proven in M7:
  derived semantic actions by reference (M7.4), primitive structural CDP
  actions (the host), targeted vision (M7.5), with the ladder's precedence
  proven in `qual_ev_0082_0234_0277_0282_…` (IMP-EV-0082).
- Proving the first rung in M7 would need a stand-in "site tool" protocol
  and a stand-in hub outside the M9 design, which `docs/82` forbids as
  proof.

## Current behavior

IMP-EV-0281 is scheduled in M7 and NOT_STARTED; the other 28 M7 work items
are sealed or in progress without it.

## Proposed replacement

1. `IMP-EV-0281` moves to **M9** through `MILESTONE_OVERRIDES` in
   `tools/build_graph.py`; docs 40/41/42 stay byte-identical, and the node
   records `milestone_override` naming this record.
2. Its qualification runs unchanged in M9 against the real gateway: a local
   site declares a structured tool through an MCP server the hub lists; the
   same task selects the declared tool when trust and policy allow it and
   falls back to the derived semantic action (M7.4) when they do not or
   when the server is absent — the precedence order of docs/22 "Action
   hierarchy" with all four rungs real.

## Migration

None: a scheduling edge moves; no code, data or document changes meaning.

## Compatibility

None affected.

## Security impact

None. A declared tool will enter through the External Tool Hub's untrusted,
size-bounded schema path (docs/16) and the kernel's policy like every other
external tool; nothing in M7 grants a site any authority.

## Test impact

None now; QUAL-EV-0281 runs in M9 with the gateway.

## Rollback

Remove the override entry; the node returns to M7.
