# Task Card — PX-009 CI-result ingestion as provenance-bearing evidence

## Identity

- Task ID: PX-009 (REQ-PX-009; related REQ-EV-0010)
- Milestone: M9; release: RELEASE_ZERO; prerequisites PX-007, M2.8 (both COMPLETE)
- Owner: verification (`crates/verification::ci`); Core orchestration in `services/modbit-core/src/ci_evidence.rs`; forge read `crates/tools` `forge.ci.status`
- Qualification: QUAL-PX-009 / PX-E2E-009 — real check-run results for the task branch are ingested as evidence artifacts with provider, run id, commit and OutputRef logs and appear in Review with provenance ci; a green CI run cannot mark a qualification PASS, a qualification naming a real test still runs in Modbit, a mismatched commit is rejected.
- Evidence tier: release-critical (evidence semantics, protocol and schema)

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. `forge.ci.status` (PX-006) read a commit's check runs as untrusted data (name, status, conclusion, URL, head sha) and nothing else used it; no path recorded CI results on a task, Review showed none, and nothing tied a run to the commit the pull request carries.
- first missing link: an ingestion that reads the runs for the pushed commit and files them as evidence — with the run id and the run's own log, which the tool did not return.
- production entry points: `crates/verification/src/ci.rs` (`ingest`: commit match, `MISMATCHED_COMMIT`/`MALFORMED` refusals, log assembly and bounding, `CI_PROVENANCE`, `CI_EVIDENCE_CLASS`); `crates/tools/src/forge.rs` (`forge.ci.status` adds run id, completion time and bounded output); `services/modbit-core/src/ci_evidence.rs` (`IngestCiResults`: the pull request from the log, the commit from the Core's own `modbit/pr-<task>` branch, the read through the kernel under the task's lease, logs as objects, `CiEvidenceRecorded`); `review.rs` (`ReviewBundle.ci_evidence`, `ci_log:` evidence links); `crates/domain` (`TaskEvent::CiEvidenceRecorded`, `CiCheckRecord`, `CiRejectedRecord`); surface protocol `IngestCiResults` / `CiResultsIngested` / `CiCheckView` / `CiRejectedView`.

## Verification

- `qual_px_009_ci_results_are_evidence_with_provenance_and_never_a_verification_result` (services/modbit-core, real Core, GitHub-compatible forge over HTTP, a real bare remote): refused `NO_PULL_REQUEST` before a pull request; after PX-007's approved open, ingestion reads `/commits/<pushed sha>/check-runs` under the Core's token, accepts the run for the pushed commit (name `ci`, run 101, success, provenance ci, its log readable by `ReadObjectRange`) and refuses the one for an older commit (`MISMATCHED_COMMIT`); no verification run, gate record or risk derivation is written; Review lists the run with its commit and provenance and links its log, and its verification runs are unchanged; returned to work, the task's own check (the configured command the project also calls `ci`) runs in Modbit on the new revision and fails, the gate rejects that revision, and the green evidence stands under its own commit.
- `crates/verification` unit: a run on another commit or without identity is refused; a long log is cut on a character boundary and says so.
- Regression: `qual_px_006_…` and `qual_px_007_…` with the forge fake listing a second, older-commit run.

## Limitations

- GitHub's check-runs API only (the forge adapter's one provider); workflow-run job logs beyond a check run's own output are not fetched.
- Ingestion is on request (`IngestCiResults`); the webhook path (PX-011) can drive it when check-suite events are mapped — not wired here.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- docs/29 and docs/30 carry the as-built paragraphs
