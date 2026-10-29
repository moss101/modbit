---
id: DR-GOV-2026-10-30-008
title: The trajectory metrics module is bound to the PX-189 producer's production wiring
status: accepted
date: 2026-10-30
supersedes: none
approved_by: owner, chat instruction of 2026-10-30 ("Bind via DOC-GOV-008")
---

# DR-GOV-2026-10-30-008 — bind `trajectory.rs` to PX-189

> **Accepted 2026-10-30** by the owner's chat instruction. Dossier work only (DOC-GOV-008). The change record and the full change-control table are in `../97_DOSSIER_MAINTENANCE_LOG.md`; this file exists so the one locked path the change touches, the PX-189 task card in doc 62, carries an accepted record.

## 1. Trigger and evidence

`crates/core-runtime/src/trajectory.rs` (commit 74f52c33) reads the task and tool-call projections and returns per-task failed, cancelled, unknown-outcome and exact-repeat call counts and recurring goals. AUDIT (code read): no Core, desktop or CLI code calls it. PX-189 already specifies one read-only producer over the log projection that feeds the Skill Evolution Lab, and does not mention the module.

## 2. Decision

PX-189's Production wiring names the module as an input of that one producer, and says the producer is its caller. Nothing else changes: no requirement row, disposition, owner, task, qualification, gate, release rule or product status; PX-189 stays NOT_STARTED; the owner stays `skills`; the module stays in `core-runtime`.

## 3. Options rejected

- A new requirement row or subsystem for trajectory metrics: a second owner beside PX-189 and `skills` (doc 81).
- Building the producer in this task: release-critical product work that PX-189 owns and PX-E2E-189 proves; it is not part of this approval.

## 4. What the approval does not cover

The producer, redaction, sealing, the lab read, any protocol or storage change, and any status move of PX-189.
