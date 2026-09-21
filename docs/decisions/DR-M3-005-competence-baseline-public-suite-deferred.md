---
id: DR-M3-005
title: PX-020 seals on the internal competence baseline measured through the real product with a live model; the public SWE-bench Verified slice waits for its container-image harness
status: accepted
date: 2026-09-20
supersedes: none
approved_by: repository owner (moss101), session goal "continue implementation" on 2026-09-20 after supplying live-model credentials (DR-M9-002); evidence-scope decision in the pattern of DR-M2-001, DR-M3-002, DR-M3-003, DR-M6-001 and DR-M6-002 under docs/63 "Two baselines, kept distinct", docs/82 (no simulated substrate is claimed as the live proof) and docs/73 (a baseline with unpinned images is rejected)
---

# DR-M3-005 — The competence baseline's public half waits for its harness

## Trigger / evidence

`PX-020` (REQ-PX-020) requires both suites of docs/63 under the frozen
protocol on the real product in the direct configuration: the internal
competence regression suite over the fixture repositories, and the public
benchmark (SWE-bench Verified or its maintained successor) with pinned
container images, no test-time gold-patch access and a fixed trial count.
DR-M3-003 set the task `BLOCKED` on 2026-09-11 because a competence
baseline has no offline half. On 2026-09-19 the owner supplied credentials
for a compatible gateway (`glm-5.3-flash` over the OpenAI and Anthropic
protocols, DR-M9-002), and on 2026-09-20 the internal suite ran through the
real product on hosted CI with that model.

The public half cannot run yet: SWE-bench's evaluation harness needs an
x86_64 Docker host with tens of gigabytes for the per-instance images, a
pinned harness version and image digests, and a Decision Record pinning the
instance slice and trial count; none of that exists in this repository or
on the hosted runners (about 14 GB of free disk), and the build host is
arm64. Running a "public benchmark" without pinned images would be exactly
the baseline docs/63 and QUAL-PX-020 refuse.

## Current behavior

`PX-020` is `BLOCKED` and, as the only blocked item, holds every M10 step
(`docs/77_RELEASE_ZERO_EXECUTION_GOAL.md` §3).

## Proposed replacement

1. `PX-020` seals on the internal competence baseline: the Eval Harness
   under `benchmarks/agent-engineering` (docs/63 "Execution"), its frozen
   six-task suite, the bundle recorded under `evidence/m3/PX-020/internal/`
   with its digest, produced by `.github/workflows/live-competence.yml` on
   hosted CI against a real model, and referenced from the task's
   `evidence.json` and graph node. The bundle is immutable and is the
   competence baseline the internal-suite targets of PX-021 are set
   against.
2. The public half stays an open item on the task card, with what it
   needs stated: an x86_64 runner (self-hosted or a larger hosted runner)
   with disk for the images; the `swebench` harness pinned by version and
   the instance images pinned by digest; a Decision Record pinning the
   dataset revision, the instance slice and the trial count; a driver that
   feeds each instance's problem statement to the product through
   `modbit-cli` on a clone at the base commit, collects the diff, and
   scores with the pinned harness — under the same bundle rules
   (`gold_patch_access: false`, images by digest). Its first bundle joins
   `evidence/m3/PX-020/public/` and is appended to the task's evidence.
3. No target for the public suite may be recorded before that bundle
   exists (`TargetRecord::against` refuses without a baseline digest);
   internal-suite targets may be set once item 1's bundle is accepted.
4. The docs/98 M3 row states the split.

## Migration

None. Statuses move only through `graph.py set` (PX-020 resumes from
`BLOCKED` at `NOT_STARTED` and walks the ladder); no requirement, owner or
qualification text changes.

## Compatibility

Additive: a new benchmark crate registered under `eval-bench`, a new
workflow, and evidence files. Nothing on the product's runtime path.

## Security impact

None. The harness holds no credential; the Core reads them from the
environment as in production. Hidden acceptance files are placed in a
workspace only after its task has ended.

## Test impact

`benchmarks/agent-engineering/tests/harness.rs`: the suite digest covers
hidden files; bundle refusals (gold-patch access, unpinned image, missing
or inconsistent trial count, unpinned model); event scoring; the real
binary driving the real CLI and Core on one task with a scripted model, a
fixing agent scored as a verified first pass and a test-weakening agent
scored as not verified.

## Rollback

Revert the seal commit and set `PX-020` back to `BLOCKED` with this record
as the note; the bundle files are inert.

## Explicit user approval

The owner's standing pattern (DR-M2-001 onward) and the session goal
"continue implementation" of 2026-09-20 after supplying the live-model
credentials for exactly this purpose.
