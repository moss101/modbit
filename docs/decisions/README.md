# Decision Records (ADRs) and status ledger

> Created by milestone task M0.2 (`docs/43`). The **register of record** for
> product decisions (`MOD-*`, `ADR-R-*`, `DR-*`) is `docs/02_AUTHORITY_AND_DECISIONS.md`;
> this directory holds the full Decision Record documents that doc 02's change-control
> section requires and the machine-checked ledger CI uses. It is not a second register.

## When a Decision Record is required

Any change to a **LOCKED** item (`docs/02` "Change control", `docs/46`, `AGENTS.md`)
needs a Decision Record with: trigger/evidence, current behavior, proposed
replacement, migration, compatibility, security impact, test impact, rollback and
explicit user approval. The paths that count as locked are declared as data in
`tools/architecture-lint/rules.toml` (`[[locked]]` entries) and enforced by
`architecture-lint locked` in CI (`.github/workflows/ci.yml`, job `locked-files`).

## How a commit links its Decision Record

A commit that touches a locked path must carry a Git trailer:

```text
Decision-Record: docs/decisions/DR-M0-002-locked-architecture-file-guard.md
```

The referenced file must exist in the commit's tree and carry `status: accepted`
in its front matter. Anything else (no trailer, missing file, `proposed`,
`rejected`, `superseded`) fails CI. Reseal commits that only regenerate
`MANIFEST.md`, `manifest.json`, `graph/` or `docs/98` do not touch locked paths
and need no trailer.

## Record format

```markdown
---
id: DR-M0-002
title: <short title>
status: proposed | accepted | superseded | rejected
date: YYYY-MM-DD
supersedes: <id or none>
approved_by: <who approved, and where>
---
```

followed by the nine sections listed above. Accepted records are immutable; a
change of mind is a new record that names the old one in `supersedes`.

## Status ledger

| ID | Title | Status | Date | Supersedes |
|---|---|---|---|---|
| DR-M0-002 | Locked architecture file guard and Decision Record trailer | accepted | 2026-09-08 | none |
| DR-M0-004 | Module registration metadata and the canonical single-owner system list | accepted | 2026-09-08 | none |
| DR-M0-005 | Reschedule IMP-EV-0242 to M4 and PX-030 to M10 | accepted | 2026-09-09 | none |
| DR-M1-006 | Reschedule twelve M1 tasks whose qualifications need M2–M6 runtime | accepted | 2026-09-09 | none |
| DR-M2-001 | Provider Gateway seals on the wire-faithful conformance suite; the live production-endpoint proof waits for credentials | accepted | 2026-09-09 | none |
| DR-M2-002 | Reschedule forty-three M2-labelled tasks whose qualifications need M3–M10 substrate | accepted | 2026-09-10 | none |
| DR-M3-001 | Confine the search-stack dependencies to the retrieval crate | accepted | 2026-09-10 | none |
| DR-M3-002 | The EPR direct baseline seals on the wire-faithful path; the production-endpoint run of EPR-E2E-000 waits for credentials | accepted | 2026-09-11 | none |
| DR-M3-003 | Benchmark method and conformance tasks seal on their harness halves; the live-model halves wait for credentials | accepted | 2026-09-11 | none |
