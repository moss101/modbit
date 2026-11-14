---
id: DR-GOV-2026-11-11-009
title: The Workspace Editor is withdrawn: DR-PX-2026-10-03-009 is rejected, PX-077..081 leave Release Zero, and MOD-SURF-002 and MOD-IDE-002 stand as written
status: accepted
date: 2026-11-11
supersedes: none
approved_by: owner, chat instruction of 2026-11-11 ("Drop the editor ... implement this", then "drop IDE")
---

# DR-GOV-2026-11-11-009 — withdraw the Workspace Editor

> **Accepted 2026-11-11** by the owner's chat instruction. Dossier work only (DOC-GOV-009). The change record and the full change-control table are in `../97_DOSSIER_MAINTENANCE_LOG.md`. No product code changes: nothing of the editor was ever built.

## 1. Trigger and evidence

- On 2026-11-08 the owner asked whether the product was creating an IDE, because it was meant to be built without one.
- **What the dossier held.** `DR-PX-2026-10-03-009` (status `proposed`, never ratified) admitted a scoped Workspace Editor as a secondary surface and proposed a scoped supersession of two locked rows: MOD-SURF-002 ("no built-in full IDE or Monaco architecture; editing is not the primary UI", LOCKED) and MOD-IDE-002 ("build a new general-purpose code editor", REJECTED). It added doc 67 (21 requirements), rows REQ-PX-077..081, task cards PX-077..081 and scenarios PX-E2E-077..081, all RELEASE_ZERO, and the provisional decision MOD-EDIT-001.
- **What the graph held.** AUDIT: PX-077..081 were NOT_STARTED, gated behind DOC-PX-009 (REAL_TESTING, awaiting ratification); no product code, protocol message or renderer component implemented any of them. The five tasks counted toward the 539 Release Zero work items.
- **The owner's decision.** Offered three options (drop, ratify as scoped, defer past Release Zero), the owner chose to drop it: "Drop the editor ... implement this", "drop IDE".

## 2. Current behavior

The ledger plans five editor tasks for Release Zero. The register shows MOD-EDIT-001 as PROVISIONAL, and docs 02 and 03 carry "proposed, no effect until accepted" supersession entries. Docs 20, 29, 32 and 65 carry notes that point at the proposal.

## 3. Decision

1. **DR-PX-2026-10-03-009 is rejected.** Its status becomes `rejected`; the file stays as the record of what was proposed and why it is not built. Its dossier task DOC-PX-009 keeps its identity and is resolved by this record.
2. **REQ-PX-077..081 are withdrawn.** The ledger requires `REQ-PX` numbers to be contiguous, so the five rows stay as DEFERRED rows with release POST_ZERO and no task, milestone or prerequisite, titled "Withdrawn". Their task cards PX-077..081 and scenarios PX-E2E-077..081 are removed, and their qualifications say that no proof is owed. DEFERRED here means "not owed", not "scheduled later": reactivation needs a new accepted Decision Record that names its own proof.
3. **MOD-SURF-002 and MOD-IDE-002 are restored as written.** They were never superseded (the proposal never took effect); the proposed supersession entries are marked not in force. MOD-EDIT-001 becomes REJECTED. MOD-SURF-001 and MOD-IDE-001 (no Code-OSS) were never touched.
4. **Doc 67 is marked withdrawn** at its head and its content is kept as provenance. It is not authoritative for any build.
5. **What stays.** Review remains the place where agent work is judged, with per-hunk decisions at a revision; the one-hunk inline patch (PX-005) stays; the Files app stays what PX-048 specifies; language services remain Core-side and serve agents and adapters (REQ-EV-0141, REQ-EV-0160). None of these was added by DR-009.

## 4. Options rejected

- **Ratify the editor as scoped (secondary surface, no Monaco).** The owner's goal is a product without an IDE; a text engine, drafts, find and replace, tabs and language intelligence are the parts of one, even when scoped.
- **Defer it past Release Zero.** A deferral reads as a plan to build it. The owner chose to drop it, so no activation text is kept.
- **Delete the five ledger rows.** `tools/dossier_px.py` requires contiguous `REQ-PX` numbers and the rows are part of the audit trail.

## 5. Migration and compatibility

- RELEASE_ZERO loses five work items (539 to 534); ALPHA and BETA are unchanged because the five were RELEASE_ZERO only. Nothing else depended on PX-077..081 (checked in the graph before the change).
- The 291 `REQ-EV` rows, the EPR ledger, every gate and every threshold are untouched. The `REQ-PX` ledger keeps 198 rows; ADOPT tasks go from 176 to 171 and DEFERRED rows from 22 to 27.
- The `tools/build_graph.py` parity table keeps the DR-009 entry (its dossier task and change record stay, the change record now REJECTED); `dr_status` learns the value `rejected`.

## 6. Security impact

None. The change removes planned attack surface (a draft overlay, a multi-file save path and an editor text engine) and adds none.

## 7. Test impact

`tools/test_dossier.py`: the parity records carry a three-valued status, the unratified-records test covers DOC-PX-011 and DOC-PX-012 only, the pinned counts become 171 ADOPT tasks and 27 DEFERRED rows, a new test pins the withdrawal (rows, absent cards and scenarios, the record's status, MOD-EDIT-001 REJECTED, doc 67 marked), and the governance chain test covers DOC-GOV-009.

## 8. Rollback

Revert the change commit, or restore the files to the hashes in `../../evidence/dossier-gov-009/baseline.json`, and rerun the reseal. To build an editor later, write a new Decision Record that names its scope and proof; this record does not forbid asking again, it only stops the current plan.

## 9. What the approval does not cover

Any product code, any change to Review, the inline patch or the Files app, and any attestation of a release gate.
