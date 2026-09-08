---
id: DR-M0-004
title: Module registration metadata and the canonical single-owner system list
status: accepted
date: 2026-09-08
supersedes: none
approved_by: repository owner (moss101), instruction "go ahead with M0.4" on 2026-09-08
---

# DR-M0-004 — Module registration metadata and the canonical single-owner system list

## Trigger / evidence

`docs/45` requires CI to fail when a code module registers a production
capability with no requirement id, or when an architectural owner has two
active production implementations without an approved migration ADR.
`docs/81` lists the canonical single-owner systems in prose only. Without a
machine-readable registration neither rule can be checked.

## Current behavior

Crates and packages carry no owner or capability metadata; duplicate owners
are detectable only by reading code.

## Proposed replacement

1. Every workspace member declares `[package.metadata.modbit]` (Cargo.toml) or
   `"modbit"` (package.json) with `owner` (a subsystem node id from the project
   graph), `canonical` (doc 81 systems it implements), `capabilities`
   (`{id, requirements[]}`) and, only during a migration, `migration_adr`.
2. `tools/architecture-lint/rules.toml` carries the doc 81 list as
   `[[canonical]]` entries (18 ids). Adding, renaming or removing one is a
   locked change requiring a Decision Record.
3. `architecture-lint modules` fails CI on a missing registration, unknown
   owner, capability without an existing requirement id, unknown canonical
   system, or a canonical system with more than one claimant unless exactly
   one claimant names an accepted migration record.

## Migration

None; all 40 members were registered in commit 1e94980. That commit cited
DR-M0-002 as its trailer because the rules file is locked; this record is the
decision behind the content of that change.

## Compatibility

Registration metadata is ignored by cargo and pnpm. No runtime effect.

## Security impact

None on the runtime. The check makes silent parallel implementations of
policy, effect or sandbox boundaries a CI failure.

## Test impact

`tools/architecture-lint/tests/modules.rs` (real cargo metadata, disposable
workspaces, accepted and proposed records) and the real-workspace test.

## Rollback

Remove the CI step and the `[[canonical]]` entries with a superseding record;
the metadata blocks may remain inert.

## Explicit user approval

Owner instruction on 2026-09-08: "go ahead with M0.4".
