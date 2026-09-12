---
id: DR-M5-001
title: Reschedule IMP-EV-0187 to M9 (rich MCP media results need the external MCP gateway of M9.4)
status: accepted
date: 2026-09-12
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks"; scheduling-only change under the MILESTONE_OVERRIDES precedent (DR-M0-005, DR-M2-002, DR-M3-004)
---

# DR-M5-001 — Reschedule IMP-EV-0187 to M9

## Trigger / evidence

`IMP-EV-0187` (REQ-EV-0187, "rich MCP media results: normalize text/image/
audio/file/resource outputs into typed ToolResult media parts";
QUAL-EV-0187, "MCP test server returns image+text; both reach vision-capable
model and evidence store"; docs/58 MEDIA-E2E-006) is scheduled in M5 by its
owner label. Its subject is the output of an MCP server, and nothing in the
product speaks MCP:

- `M9.4` ("external MCP gateway"), `IMP-EV-0104` (MCP list / call / cancel
  lifecycle), `IMP-EV-0128` (MCP management and scoped auth) and
  `IMP-EV-0193` (workspace-scoped MCP transport pool) are all scheduled in
  **M9**; the External Tool Hub that normalizes an external tool's result
  arrives with them.
- `crates/tools/tool-matrix.json` records no MCP family; docs/17 lists
  `external.list / call / cancel` as the hub's surface, unbuilt.
- The media half of the requirement — typed media parts, the pipeline's
  scan and budgets, the egress copy a vision-capable model receives — is
  built (M2.10, IMP-EV-0188) and is what the MCP hub will call.

Proving IMP-EV-0187 in M5 would need a stand-in "MCP test server" and a
stand-in hub outside the M9 design, which `docs/82` forbids as proof.

## Current behavior

IMP-EV-0187 is scheduled in M5 and NOT_STARTED; the other 39 M5 work items
are sealed or in progress without it.

## Proposed replacement

1. `IMP-EV-0187` moves to **M9** through `MILESTONE_OVERRIDES` in
   `tools/build_graph.py`; docs 40/41/42 stay byte-identical, and the node
   records `milestone_override` naming this record.
2. Its qualification runs unchanged in M9 against the real MCP gateway: a
   local MCP test server returns text and an image, the hub normalizes both
   into typed media parts, the media pipeline scans and budgets the image,
   and a vision-capable provider receives both with contiguous call ids and
   evidence.

## Migration

None: a scheduling edge moves; no code, data or document changes meaning.

## Compatibility

None affected.

## Security impact

None. The media pipeline's scan, budgets and untrusted labelling already
apply to every media read; an MCP result will enter through the same
pipeline when the hub exists.

## Test impact

None now; MEDIA-E2E-006 covers QUAL-EV-0187 in M9.

## Rollback

Remove the override entry; the node returns to M5.
