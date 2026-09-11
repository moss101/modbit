---
id: DR-M3-004
title: Reschedule IMP-EV-0284 to M7 (web page text needs the web tool family the tool matrix defers to the browser milestone)
status: accepted
date: 2026-09-11
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks"; scheduling-only change under docs/74 MILESTONE_OVERRIDES precedent (DR-M0-005, DR-M2-002)
---

# DR-M3-004 — Reschedule IMP-EV-0284 to M7

## Trigger / evidence

`IMP-EV-0284` (REQ-EV-0284, "web page text is untrusted data; cannot alter
policy or tool authority"; QUAL-EV-0284, "seed hostile page instructions; the
forbidden tool remains unavailable") is the last M3 work item that is neither
sealed nor blocked on credentials. Its subject is web page text, and nothing in
the product fetches a web page:

- `crates/tools/tool-matrix.json` records the `web` family (WebFetch,
  WebSearch) as `DEFERRED (no browser/network consumer yet; REQ-EV-0088/0147)`.
- REQ-EV-0088 (semantic UI risk classification) and REQ-EV-0147 (browser and
  E2E evidence), the requirements that gate a network-facing tool on a risk
  classification and an evidence normalization, are scheduled in **M7**, the
  milestone whose scope in `docs/98` is "actual Chromium, same-session
  takeover, hostile-page test".
- `crates/browser` carries no behaviour yet by its own declaration; behaviour
  arrives through the M7 tasks that name it as owner.

IMP-EV-0284 landed in M3 because its owner label "Context/Policy" maps to the
context engine's milestone in `tools/build_graph.py`. Proving it in M3 would
mean either adding a web fetch outside the risk classification the matrix
requires first, or seeding "hostile page instructions" into something that is
not a page. The isolation property itself — untrusted text never alters tool
authority — already holds for attached documents (IMP-EV-0203, sealed), and
the web-specific proof belongs with the web tools.

## Current behavior

IMP-EV-0284 is scheduled in M3 and NOT_STARTED. M3's roll-up is held by PX-020
(DR-M3-003) regardless.

## Proposed replacement

1. `IMP-EV-0284` moves to **M7** through `MILESTONE_OVERRIDES` in
   `tools/build_graph.py`; docs 40/41/42 stay byte-identical, and the node
   records `milestone_override` naming this record.
2. Its qualification runs unchanged in M7 against the real web fetch of the
   browser milestone: a hostile page is fetched by the product's own tool, and
   the forbidden tool stays unavailable.

## Migration

None: a scheduling edge moves; no code, data or document changes meaning.

## Compatibility

None affected.

## Security impact

None. The isolation rule is not weakened: the product still has no web fetch,
so there is no web text to isolate until M7 adds one under REQ-EV-0088.

## Test impact

None now; the M7 hostile-page test covers QUAL-EV-0284 when the web tools
exist.

## Rollback

Remove the override entry; the node returns to M3.
