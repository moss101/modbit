---
id: DR-M0-002
title: Locked architecture file guard and Decision Record trailer
status: accepted
date: 2026-09-08
supersedes: none
approved_by: repository owner (moss101), instruction "go ahead with M0.2" on 2026-09-08
---

# DR-M0-002 — Locked architecture file guard and Decision Record trailer

## Trigger / evidence

`docs/43` M0.2 requires: "CI rejects changed locked architecture file without
linked ADR metadata." `docs/02` change control says a PR that silently changes a
locked invariant fails architecture CI, and `docs/70` says architecture-impact
PRs include an ADR link. Until this record nothing enforced either sentence.

## Current behavior

Locked decisions, requirement rows, owners and integrity constants are declared
LOCKED in prose (`docs/02`, `docs/46`, `AGENTS.md`) and the dossier self-tests
pin counts, but any commit can edit those files and pass CI.

## Proposed replacement

1. `tools/architecture-lint/rules.toml` gains `[[locked]]` entries naming the
   locked paths (globs) with the authority reason for each.
2. `architecture-lint locked --base <sha> --head <sha>` walks every commit in
   the range with real Git. A commit that changes a locked path must carry a
   `Decision-Record:` trailer whose value is a path under `docs/decisions/`
   that exists in that commit's tree with `status: accepted` front matter.
3. CI job `locked-files` runs the check on every push and pull request with
   full history. Violations fail the job.
4. `docs/decisions/README.md` is the ledger of Decision Records; accepted
   records are immutable and are themselves locked paths.

## Migration

No existing file changes meaning. Historical commits are not re-checked; the
guard applies from the commit that introduces it (which carries this record as
its own trailer).

## Compatibility

Reseal-only commits (manifests, graph, `docs/98`) touch no locked path and are
unaffected. The dossier tools ignore `docs/decisions/` when counting numbered
docs; the manifest builder hashes the records so they are package payload.

## Security impact

None on the product runtime. The guard raises the bar for silent changes to
security-relevant authority (`docs/81`, `docs/23` is not locked here but may be
added by a later record).

## Test impact

`tools/architecture-lint/tests/locked_files.rs` creates real Git repositories
and proves: locked change without trailer fails; with an accepted record passes;
missing record fails; `proposed` record fails; unlocked change passes; CLI exit
codes 0/1/2.

## Rollback

Remove the `locked-files` CI job and the `[[locked]]` entries with a superseding
Decision Record. The subcommand can remain.

## Explicit user approval

Owner instruction on 2026-09-08: "go ahead with M0.2".
