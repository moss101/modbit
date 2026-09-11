# Task Card — EPR-015 Versioned Outcome Statistics materialization

## Identity

- Task ID: EPR-015
- Milestone: M3 (moved from M2 by DR-M2-002)
- Requirement: REQ-EPR-015; owner label: eval-bench; subsystem: eval-bench
- Qualification: `QUAL-EPR-015` / `EPR-E2E-015` — persist and reload a real statistics snapshot sourced from attributable baseline outcomes; the compiler read interface joins pinned registry and stats versions independently; repeated events do not duplicate samples and sparse priors remain low-confidence. Negative: `EPR-FI-015` — reject a wrong tenant, stale or incompatible stats, a missing gate or verification key and a fabricated sample count; a changed registry cannot silently rewrite observations or qualify cheap plans.
- Evidence tier: real-system or production-equivalent

## Goal

Materialize immutable `stats_version` snapshots of what was observed, in the stores that already exist, keyed exactly, and separate from the Model Registry so that changing one cannot rewrite the other.

## Existing-code audit

- classification: ABSENT before this task. EPR-000 publishes baseline bundles of attributable outcomes, and EPR-002 keeps a `stats_version` reference, but nothing aggregated anything: there was no keyed dataset, no snapshot, and no way for a compiler to join observations to a pinned version.
- production entry points:
  - `benchmarks/outcome-statistics` — the dataset. `StatKey` is the exact key set: solver (model, Skill, harness), the joint escalation key (from model, to model, gate, repository, verification), reviewer family and revision finding class. `materialize` deduplicates samples by the outcome they came from, derives every count from what is left, computes a Wilson score interval, and marks anything below the sample threshold low-confidence. `Aggregate::qualifies` refuses to let a prior qualify a cheaper plan.
  - `samples_from_baseline` — every task in a published bundle is one attributable observation of the solver that ran it, carrying the bundle digest it came from.
  - `services/modbit-core/src/statistics.rs`, `MaterializeOutcomeStatistics` and `GetOutcomeStatistics` — the Core path. The snapshot is an artifact in the object store and a `SessionEvent::OutcomeStatisticsMaterialized` on the log, so there is no second state owner. Reads join on the pinned version and on the tenant.
- proof: a real task runs through Core; the session's baseline is published; a snapshot is materialized from it and reports what it was derived from, under which build, repository revision, environment and registry generation. Materializing again over the same outcomes counts them once and produces the same digest. One observation is reported as a prior, with an interval wider than half the range and a low-confidence flag, and the cost nobody reported stays unknown rather than averaging as zero. A reader that pins a different version is refused as stale, and the whole snapshot comes back unchanged after the Core is killed and restarted.

## Limitations

- Only the solver key has a producer today. The joint escalation key, the reviewer family key and the revision finding key are defined, validated and refused when incomplete, but nothing escalates, reviews or revises yet: those producers are EPR-005, EPR-006 and EPR-007. A key with no observations is absent rather than invented.
- Samples derived from a baseline carry `skill = "none"` and `harness = <build digest>`, because the direct path runs no Skill and its harness identity is the build that ran it. That is what the key says rather than leaving a component blank.
- Cost is not yet observed: the baseline keeps unknown cost unknown, so every sample's cost is `None` and the aggregate reports how many observations had no known cost instead of averaging zeros.
- Snapshots are per session today. A cross-session or cross-revision rollup has no caller until the compiler needs one.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_015_statistics_are_materialized_from_attributable_outcomes_and_reload` — the end-to-end proof through Core, including the restart.
- `counts_are_derived_and_the_same_outcome_counts_once`
- `a_sparse_key_stays_low_confidence_and_qualifies_nothing`
- `a_snapshot_refuses_what_it_cannot_stand_behind` — fabricated counts, missing gate and verification components, unattributed samples (EPR-FI-015).
- `the_registry_and_the_statistics_are_joined_independently` — stale version, wrong tenant, and a registry generation that cannot move an observation (EPR-FI-015).
- `baseline_outcomes_become_attributable_samples`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
